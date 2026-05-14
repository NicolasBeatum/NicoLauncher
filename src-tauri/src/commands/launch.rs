use serde::Serialize;
use std::sync::atomic::Ordering;
use std::time::Duration;
use tauri::State;
use tracing::{info, warn};

use launcher_auth::AuthSession;
use launcher_core::{LoaderType, progress::ProgressReporter};
use launcher_downloader::{DownloadJob, Downloader};
use launcher_java_manager::ensure_java;
use launcher_launcher::launch::LaunchSpec;
use launcher_loaders::{FabricProvider, QuiltProvider, NeoForgeProvider, ForgeProvider, merge};
use launcher_meta::MojangMetaClient;

use crate::state::AppState;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LaunchStatusDto {
    pub logs: Vec<String>,
    pub started: bool,
    pub error: Option<String>,
    pub exit_code: Option<i32>,
}

/// Polled by the frontend every 300ms while launching or running.
#[tauri::command]
pub fn get_launch_status(state: State<'_, AppState>) -> LaunchStatusDto {
    let logs = state.launch_logs.lock()
        .map(|mut v| { let out = v.clone(); v.clear(); out })
        .unwrap_or_default();
    let started   = state.game_started.load(Ordering::Relaxed);
    let error     = state.launch_error.lock().ok().and_then(|mut e| e.take());
    let exit_code = state.game_exit_code.lock().ok().and_then(|mut e| e.take());
    LaunchStatusDto { logs, started, error, exit_code }
}

#[tauri::command]
pub async fn launch_game(state: State<'_, AppState>) -> Result<(), String> {
    let session = state.session.lock().await.clone()
        .ok_or("Not logged in. Please authenticate first.")?;

    let manifest = state.manifest.lock().await.clone();
    let settings = state.settings.lock().await.clone();
    let paths = state.paths.clone();
    let config = state.config.clone();
    let instance_id = state.active_instance.lock().await.clone();

    let (mc_version, loader_spec) = match &manifest {
        Some(m) => (m.minecraft.version.clone(), m.loader.clone()),
        None    => (config.runtime.fallback_mc_version.clone(), None),
    };

    // Reset launch state
    state.game_started.store(false, Ordering::Relaxed);
    if let Ok(mut e) = state.launch_error.lock() { *e = None; }
    if let Ok(mut l) = state.launch_logs.lock() { l.clear(); }
    if let Ok(mut c) = state.game_exit_code.lock() { *c = None; }

    let launch_logs  = state.launch_logs.clone();
    let launch_error = state.launch_error.clone();
    let game_started = state.game_started.clone();
    let game_exit_code = state.game_exit_code.clone();

    tokio::spawn(async move {
        match run_launch(launch_logs.clone(), game_exit_code.clone(), session, paths, config, settings, mc_version, loader_spec, instance_id).await {
            Ok(_) => {
                game_started.store(true, Ordering::Relaxed);
            }
            Err(e) => {
                warn!("[launch] FAILED: {e}");
                if let Ok(mut logs) = launch_logs.lock() { logs.push(format!("ERROR: {e}")); }
                if let Ok(mut err) = launch_error.lock() { *err = Some(e.to_string()); }
            }
        }
    });

    Ok(())
}

async fn run_launch(
    launch_logs: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    game_exit_code: std::sync::Arc<std::sync::Mutex<Option<i32>>>,
    session: AuthSession,
    paths: launcher_core::LauncherPaths,
    config: crate::config::LauncherConfig,
    settings: crate::state::UserSettings,
    mc_version: String,
    loader_spec: Option<launcher_manifest_client::LoaderSpec>,
    instance_id: String,
) -> anyhow::Result<u32> {
    let log = |msg: &str| {
        info!("[launch] {msg}");
        if let Ok(mut v) = launch_logs.lock() { v.push(msg.to_string()); }
    };

    // Paths específicos de esta instancia (minecraft dir, mods dir)
    let ipaths = paths.instance(&instance_id);
    ipaths.ensure_all().await?;
    // Sobrescribir minecraft/mods en el LauncherPaths para que el launcher use el dir correcto
    let mut paths = paths;
    paths.minecraft = ipaths.minecraft;
    paths.mods      = ipaths.mods;

    let loader_debug = loader_spec.as_ref()
        .map(|l| format!("{} {}", l.loader_type, l.version))
        .unwrap_or_else(|| "vanilla (no manifest)".into());
    log(&format!("Iniciando lanzamiento — MC {mc_version} · loader: {loader_debug}"));
    log("Obteniendo metadata de Minecraft…");
    let meta = MojangMetaClient::new()?;
    let manifest_cache = paths.manifest_cache.join("version_manifest_v2.json");
    let version_url = meta.version_url(&mc_version, Some(&manifest_cache)).await?;
    let vj_cache = paths.manifest_cache.join(format!("{mc_version}.json"));
    let mut version_json = meta.fetch_version_json(&version_url, Some(&vj_cache)).await?;

    let required_java = version_json.java_version.as_ref().map(|j| j.major_version).unwrap_or(17);
    // noop reporter for downloads — channel would block since nobody drains it yet
    let dl_reporter = ProgressReporter::noop();
    // channel reporter only for the game process (stdout logs)
    let (reporter, mut rx) = ProgressReporter::channel(128);

    // Find (or auto-download) Java early — NeoForge/Forge processors also need it
    let java_override = settings.java_path_override.as_deref().map(std::path::Path::new);
    let java = if let Some(override_path) = java_override {
        log(&format!("Usando Java manual: {}", override_path.display()));
        launcher_java_manager::JavaInstallation {
            binary: override_path.to_path_buf(),
            major_version: required_java,
            full_version: String::new(),
        }
    } else {
        log(&format!("Buscando Java {required_java}+…"));
        ensure_java(required_java, &paths.java, &|msg| log(msg)).await?
    };
    log(&format!("Java {}: {}", java.full_version.trim(), java.binary.display()));

    // Download MC client JAR early — NeoForge processors need it to patch the client
    log("Descargando client JAR…");
    let downloader = Downloader::new(
        config.runtime.download_concurrency,
        config.runtime.download_timeout_secs,
        dl_reporter,
    )?;
    let client_jar_dir = paths.cache.join("client");
    tokio::fs::create_dir_all(&client_jar_dir).await?;
    let client_jar = client_jar_dir.join(format!("{mc_version}.jar"));
    downloader
        .download_one(
            DownloadJob::new(&version_json.downloads.client.url, &client_jar)
                .with_sha1(&version_json.downloads.client.sha1),
        )
        .await?;

    let loader_type = loader_spec.as_ref().map(|l| l.loader_type).unwrap_or(LoaderType::Vanilla);
    let extra_lib_jobs: Vec<DownloadJob> = match loader_type {
        LoaderType::Fabric => {
            let fabric = FabricProvider::new()?;
            let lv = loader_spec.as_ref().map(|s| s.version.clone())
                .unwrap_or_else(|| "latest".into());
            let lv = if lv == "latest" {
                fabric.recommended_version(&mc_version).await?
            } else { lv };
            log(&format!("Fabric loader {lv}…"));
            let cache = paths.manifest_cache.join(format!("fabric-{mc_version}-{lv}.json"));
            let profile = fabric.resolve_profile(&mc_version, &lv, Some(&cache)).await?;
            let jobs = FabricProvider::library_download_jobs(&profile, &paths.libraries);
            merge(&mut version_json, &profile);
            jobs
        }
        LoaderType::Quilt => {
            let quilt = QuiltProvider::new()?;
            let lv = loader_spec.as_ref().map(|s| s.version.clone())
                .unwrap_or_else(|| "latest".into());
            let lv = if lv == "latest" {
                quilt.recommended_version(&mc_version).await?
            } else { lv };
            log(&format!("Quilt loader {lv}…"));
            let cache = paths.manifest_cache.join(format!("quilt-{mc_version}-{lv}.json"));
            let profile = quilt.resolve_profile(&mc_version, &lv, Some(&cache)).await?;
            let jobs = QuiltProvider::library_download_jobs(&profile, &paths.libraries);
            merge(&mut version_json, &profile);
            jobs
        }
        LoaderType::NeoForge => {
            let nf = NeoForgeProvider::new()?;
            let lv = loader_spec.as_ref().map(|s| s.version.clone())
                .unwrap_or_else(|| "latest".into());
            let lv = if lv == "latest" {
                nf.recommended_version(&mc_version).await?
            } else { lv };
            log(&format!("NeoForge {lv}…"));
            // NeoForge install() handles its own downloads + processor execution
            let profile = nf.install(
                &mc_version,
                &lv,
                &client_jar,
                &java.binary,
                &paths,
                &log,
            ).await?;
            merge(&mut version_json, &profile);
            vec![]
        }
        LoaderType::Forge => {
            let forge = ForgeProvider::new()?;
            let lv = loader_spec.as_ref().map(|s| s.version.clone())
                .unwrap_or_else(|| "latest".into());
            let lv = if lv == "latest" {
                forge.recommended_version(&mc_version).await?
            } else { lv };
            log(&format!("Forge {lv}…"));
            // Forge install() handles its own downloads + processor execution
            let profile = forge.install(
                &mc_version,
                &lv,
                &client_jar,
                &java.binary,
                &paths,
                &log,
            ).await?;
            merge(&mut version_json, &profile);
            vec![]
        }
        _ => vec![],
    };

    log("Descargando librerías y assets…");
    let lib_jobs = build_library_jobs(&version_json.libraries, &paths.libraries);
    let asset_index_cache = paths.asset_indexes.join(format!("{}.json", version_json.assets));
    let asset_objects = meta
        .fetch_asset_index(&version_json.asset_index.url, Some(&asset_index_cache))
        .await?;
    let asset_jobs: Vec<DownloadJob> = asset_objects.objects.values().map(|obj| {
        let prefix = &obj.hash[..2];
        let dest = paths.asset_objects.join(prefix).join(&obj.hash);
        let url = format!("https://resources.download.minecraft.net/{prefix}/{}", obj.hash);
        // with_size enables fast size-check skip — no SHA1 read needed if size matches
        DownloadJob::new(url, dest).with_sha1(&obj.hash).with_size(obj.size)
    }).collect();

    let mut all_jobs = lib_jobs;
    all_jobs.extend(asset_jobs);
    all_jobs.extend(extra_lib_jobs);
    downloader.download_many(all_jobs).await?;

    log(&format!("Lanzando Minecraft {mc_version} con Java {}…", java.binary.display()));
    let quick_connect = if config.features.quick_connect {
        // Usar la dirección de la instancia activa, o la del [server] como fallback
        let instance = config.find_instance(&instance_id);
        let addr = instance.as_ref()
            .filter(|i| !i.server_address.is_empty())
            .map(|i| (i.server_address.clone(), i.server_port))
            .unwrap_or_else(|| (config.server.address.clone(), config.server.port));
        Some(addr)
    } else {
        None
    };

    let mut extra_jvm = config.runtime.default_jvm_args.clone();
    extra_jvm.extend(settings.extra_jvm_args.clone());

    let auth = launcher_launcher::launch::AuthSession {
        username: session.username.clone(),
        uuid: session.uuid.clone(),
        access_token: session.access_token.clone(),
        user_type: session.user_type.clone(),
    };

    let spec = LaunchSpec {
        version_json,
        java_binary: java.binary,
        paths: paths.clone(),
        auth,
        ram_mb: settings.ram_mb,
        extra_jvm_args: extra_jvm,
        quick_connect,
    };

    let mut proc = launcher_launcher::launch(spec, reporter).await?;
    let pid = proc.pid;

    // Telemetría de lanzamiento (fire-and-forget, no bloquea el hilo)
    if config.telemetry.enabled
        && config.telemetry.report_launches
        && !config.telemetry.endpoint.is_empty()
    {
        let endpoint  = config.telemetry.endpoint.clone();
        let mc_ver    = mc_version.clone();
        let loader_db = loader_debug.clone();
        tokio::spawn(async move {
            let client = match reqwest::Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
            {
                Ok(c) => c,
                Err(_) => return,
            };
            let _ = client
                .post(&endpoint)
                .json(&serde_json::json!({
                    "event":      "launch",
                    "mc_version": mc_ver,
                    "loader":     loader_db,
                    "timestamp":  chrono::Utc::now().to_rfc3339(),
                }))
                .send()
                .await;
        });
    }

    // Drain progress channel (game logs go to the buffer)
    let logs_for_rx = launch_logs.clone();
    tokio::spawn(async move {
        while let Some(event) = rx.recv().await {
            if let launcher_core::progress::ProgressEvent::Log { message, .. } = event {
                if let Ok(mut v) = logs_for_rx.lock() { v.push(format!("[MC] {message}")); }
            }
        }
    });

    // Wait for game exit — write code so the poller detects it
    let logs_for_exit = launch_logs.clone();
    tokio::spawn(async move {
        let code = proc.wait().await.unwrap_or(-1);
        if let Ok(mut v) = logs_for_exit.lock() {
            v.push(format!("Minecraft cerrado (código {code})"));
        }
        if let Ok(mut c) = game_exit_code.lock() { *c = Some(code); }
    });

    Ok(pid)
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
                        "osx" => cfg!(target_os = "macos"),
                        "linux" => cfg!(target_os = "linux"),
                        _ => false,
                    })
                });
                if matches { result = rule.action == RuleAction::Allow; }
            }
            result
        };
        if !allowed { continue; }

        if lib.natives.as_ref().map_or(false, |n| {
            let key = if cfg!(target_os = "windows") { "windows" }
                else if cfg!(target_os = "macos") { "osx" } else { "linux" };
            n.contains_key(key)
        }) { continue; }

        if let Some(dl) = &lib.downloads {
            if let Some(artifact) = &dl.artifact {
                if artifact.url.is_empty() { continue; }
                let dest = if let Some(path) = &artifact.path {
                    libraries_dir.join(path)
                } else if let Some(rel) = maven_to_path(&lib.name) {
                    libraries_dir.join(rel)
                } else { continue };
                let mut job = DownloadJob::new(&artifact.url, dest);
                if !artifact.sha1.is_empty() {
                    job = job.with_sha1(&artifact.sha1);
                }
                jobs.push(job);
            }
        }
    }
    jobs
}

#[tauri::command]
pub async fn game_is_running(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.game.lock().await.is_some())
}

#[tauri::command]
pub async fn game_kill(state: State<'_, AppState>) -> Result<(), String> {
    if let Some(proc) = state.game.lock().await.as_mut() {
        proc.kill().await.map_err(|e| e.to_string())?;
    }
    Ok(())
}
