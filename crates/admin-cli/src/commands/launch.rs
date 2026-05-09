use anyhow::Context;
use tracing::info;

use launcher_auth::AuthClient;
use launcher_core::{LauncherPaths, LoaderType, ProgressEvent, ProgressReporter};
use launcher_downloader::{DownloadJob, Downloader};
use launcher_java_manager::find_java;
use launcher_launcher::launch::{AuthSession, LaunchSpec};
use launcher_loaders::{FabricProvider, merge};
use launcher_meta::MojangMetaClient;

use crate::config::LauncherConfig;

pub async fn run(
    mc_version: String,
    loader: LoaderType,
    loader_version: Option<String>,
    offline_username: Option<String>,
    ram_mb: Option<u32>,
    config: &LauncherConfig,
) -> anyhow::Result<()> {
    // ── Paths ─────────────────────────────────────────────────────────────────
    let paths = LauncherPaths::new(&config.branding.internal_id)
        .context("Cannot determine launcher data directory")?;
    paths.ensure_all().await.context("Creating launcher directories")?;

    // ── Progress reporter ─────────────────────────────────────────────────────
    let (reporter, mut rx) = ProgressReporter::channel(64);
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                ProgressEvent::Stage { name, total } => {
                    if let Some(t) = total {
                        println!("  ▶ {name} ({t} items)");
                    } else {
                        println!("  ▶ {name}");
                    }
                }
                ProgressEvent::Log { message, .. } => println!("    {message}"),
                ProgressEvent::Error { message }   => eprintln!("  ✗ {message}"),
                ProgressEvent::Done                => println!("  ✓ done"),
                _ => {}
            }
        }
    });

    // ── Auth ──────────────────────────────────────────────────────────────────
    let auth_session: AuthSession = if let Some(username) = offline_username {
        println!("\n[auth] Offline mode as '{username}'");
        AuthSession::offline(&username)
    } else {
        println!("\n[auth] Resuming session…");
        let client_id = config.auth.microsoft_client_id.trim();
        if client_id.is_empty() {
            anyhow::bail!(
                "Not logged in and no offline username provided.\n\
                 Options:\n\
                   mc-launcher auth login             (Microsoft account)\n\
                   mc-launcher launch --offline Player (offline/testing)"
            );
        }
        let auth = AuthClient::new(client_id).context("Creating auth client")?;
        let account_json = paths.root.join("account.json");
        match auth.resume(&account_json).await.context("Resuming auth session")? {
            Some(s) => {
                println!("      Logged in as: {}", s.username);
                AuthSession {
                    username: s.username,
                    uuid: s.uuid,
                    access_token: s.access_token,
                    user_type: s.user_type,
                }
            }
            None => anyhow::bail!(
                "No saved session found. Run `mc-launcher auth login` first."
            ),
        }
    };

    // ── Mojang metadata ───────────────────────────────────────────────────────
    println!("\n[1/5] Fetching Minecraft metadata…");
    let meta_client = MojangMetaClient::new().context("Creating HTTP client")?;
    let manifest_cache = paths.manifest_cache.join("version_manifest_v2.json");
    let version_url = meta_client
        .version_url(&mc_version, Some(&manifest_cache))
        .await
        .context(format!("Looking up version '{mc_version}'"))?;

    let vj_cache = paths.manifest_cache.join(format!("{mc_version}.json"));
    let mut version_json = meta_client
        .fetch_version_json(&version_url, Some(&vj_cache))
        .await
        .context("Fetching version JSON")?;

    let required_java = version_json.java_version.as_ref().map(|j| j.major_version).unwrap_or(17);

    // ── Loader ────────────────────────────────────────────────────────────────
    if loader != LoaderType::Vanilla {
        println!("[*]   Setting up {loader} loader…");
    }

    let extra_lib_jobs: Vec<DownloadJob> = match loader {
        LoaderType::Fabric => {
            let fabric = FabricProvider::new().context("Creating Fabric provider")?;
            let lv = match loader_version {
                Some(v) => v,
                None => {
                    info!("Resolving latest stable Fabric version for MC {mc_version}…");
                    fabric.recommended_version(&mc_version).await
                        .context("Resolving Fabric version")?
                }
            };
            println!("      Fabric loader {lv}");

            let cache = paths.manifest_cache.join(format!("fabric-{mc_version}-{lv}.json"));
            let profile = fabric.resolve_profile(&mc_version, &lv, Some(&cache)).await
                .context("Fetching Fabric profile")?;

            let jobs = FabricProvider::library_download_jobs(&profile, &paths.libraries);
            merge(&mut version_json, &profile);
            jobs
        }
        LoaderType::Vanilla => vec![],
        other => anyhow::bail!("{other} loader is not yet supported (planned for Fase 5)"),
    };

    // ── Java ──────────────────────────────────────────────────────────────────
    println!("[2/5] Locating Java {required_java}+…");
    let java = find_java(required_java, Some(&paths.java))
        .await
        .context(format!("Finding Java {required_java}"))?;
    println!("      Java {} at {:?}", java.major_version, java.binary);

    // ── Download ──────────────────────────────────────────────────────────────
    println!("[3/5] Downloading client JAR…");
    let downloader = Downloader::new(
        config.runtime.download_concurrency,
        config.runtime.download_timeout_secs,
        reporter.clone(),
    ).context("Creating downloader")?;

    let client_jar_dir = paths.cache.join("client");
    tokio::fs::create_dir_all(&client_jar_dir).await?;
    let client_jar = client_jar_dir.join(format!("{mc_version}.jar"));
    downloader.download_one(
        DownloadJob::new(&version_json.downloads.client.url, &client_jar)
            .with_sha1(&version_json.downloads.client.sha1),
    ).await.context("Downloading client JAR")?;

    println!("[4/5] Downloading libraries and assets…");
    let lib_jobs = build_library_jobs(&version_json.libraries, &paths.libraries);

    let asset_index_cache = paths.asset_indexes.join(format!("{}.json", version_json.assets));
    let asset_objects = meta_client
        .fetch_asset_index(&version_json.asset_index.url, Some(&asset_index_cache))
        .await.context("Fetching asset index")?;

    let asset_jobs: Vec<DownloadJob> = asset_objects.objects.values().map(|obj| {
        let prefix = &obj.hash[..2];
        let dest   = paths.asset_objects.join(prefix).join(&obj.hash);
        let url    = format!("https://resources.download.minecraft.net/{prefix}/{}", obj.hash);
        DownloadJob::new(url, dest).with_sha1(&obj.hash)
    }).collect();

    let mut all_jobs = lib_jobs;
    all_jobs.extend(asset_jobs);
    all_jobs.extend(extra_lib_jobs);
    println!("      {} files to check/download", all_jobs.len());
    downloader.download_many(all_jobs).await.context("Downloading libraries and assets")?;

    // ── Launch ────────────────────────────────────────────────────────────────
    println!("[5/5] Launching Minecraft {mc_version} ({loader})…");

    let quick_connect = if config.features.quick_connect {
        Some((config.server.address.clone(), config.server.port))
    } else {
        None
    };

    let spec = LaunchSpec {
        version_json,
        java_binary: java.binary,
        paths,
        auth: auth_session,
        ram_mb: ram_mb.unwrap_or(config.runtime.ram_default_mb),
        extra_jvm_args: config.runtime.default_jvm_args.clone(),
        quick_connect,
    };

    let mut proc = launcher_launcher::launch(spec, reporter)
        .await.context("Spawning Minecraft process")?;

    println!("Minecraft running (PID {}). Waiting for exit…", proc.pid);
    let code = proc.wait().await.context("Waiting for game")?;
    println!("Minecraft exited with code {code}");

    Ok(())
}

fn build_library_jobs(
    libraries: &[launcher_meta::types::Library],
    libraries_dir: &std::path::Path,
) -> Vec<DownloadJob> {
    use launcher_core::maven_to_path;
    use launcher_meta::types::RuleAction;

    let mut jobs = Vec::new();
    for lib in libraries {
        let rules = lib.rules.as_deref().unwrap_or(&[]);
        let allowed = if rules.is_empty() {
            true
        } else {
            let mut result = false;
            for rule in rules {
                let matches = rule.os.as_ref().map_or(true, |os| {
                    os.name.as_deref().map_or(true, |name| match name {
                        "windows" => cfg!(target_os = "windows"),
                        "osx"     => cfg!(target_os = "macos"),
                        "linux"   => cfg!(target_os = "linux"),
                        _         => false,
                    })
                });
                if matches { result = rule.action == RuleAction::Allow; }
            }
            result
        };
        if !allowed { continue; }

        let is_native = lib.natives.as_ref().map_or(false, |n| {
            let key = if cfg!(target_os = "windows") { "windows" }
                      else if cfg!(target_os = "macos") { "osx" }
                      else { "linux" };
            n.contains_key(key)
        });
        if is_native { continue; }

        if let Some(dl) = &lib.downloads {
            if let Some(artifact) = &dl.artifact {
                if artifact.url.is_empty() { continue; }
                let dest = if let Some(path) = &artifact.path {
                    libraries_dir.join(path)
                } else if let Some(rel) = maven_to_path(&lib.name) {
                    libraries_dir.join(rel)
                } else { continue };
                jobs.push(DownloadJob::new(&artifact.url, dest).with_sha1(&artifact.sha1));
            }
        }
    }
    jobs
}
