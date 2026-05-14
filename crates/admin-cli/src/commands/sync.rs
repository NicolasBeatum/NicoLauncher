use anyhow::Context;
use tracing::info;

use launcher_core::{progress::ProgressReporter, LauncherPaths};
use launcher_downloader::Downloader;
use launcher_manifest_client::{
    compute_sync_plan, fetch_manifest, mod_download_jobs, FileProvider, HttpProvider,
    InstalledMod, LoaderAction, LocalState, ManifestProvider, OptionalChoices,
};

use crate::config::LauncherConfig;

pub async fn run(config: &LauncherConfig) -> anyhow::Result<()> {
    // 1. Build provider from config
    let provider: Box<dyn ManifestProvider> = match config.server.manifest_provider.as_str() {
        "http" | "https" => Box::new(
            HttpProvider::new(&config.server.manifest_url)
                .context("Building HTTP manifest provider")?,
        ),
        "file" => {
            let path = config.server.manifest_url.trim_start_matches("file://");
            Box::new(FileProvider::new(path))
        }
        other => anyhow::bail!("Unknown manifest_provider '{other}'. Use 'http' or 'file'."),
    };

    // 2. Fetch + parse manifest (verifies Ed25519 if key is configured)
    println!("Fetching manifest ({})...", config.server.manifest_provider);
    let manifest = fetch_manifest(&*provider, &config.server.manifest_public_key)
        .await
        .context("Fetching server manifest")?;

    println!(
        "Manifest: {} | MC {} | {} required mod(s)",
        manifest.manifest_version,
        manifest.minecraft.version,
        manifest.required_mods.len(),
    );

    // 3. Load persisted state
    let paths = LauncherPaths::new(&config.branding.internal_id)
        .context("Building launcher paths")?;
    let state_path = paths.root.join("current-state.json");
    let choices_path = paths.root.join("optional-choices.json");

    let mut state = LocalState::load(&state_path).await;
    let choices = OptionalChoices::load(&choices_path).await;

    // 4. Diff
    let plan = compute_sync_plan(&state, &manifest, &choices);

    if plan.mods_to_download.is_empty()
        && plan.mods_to_remove.is_empty()
        && plan.configs_to_apply.is_empty()
        && plan.files_to_delete.is_empty()
        && matches!(plan.loader_action, LoaderAction::None)
    {
        println!("Already up to date.");
        return Ok(());
    }

    if !plan.mods_to_download.is_empty() {
        println!("  + {} mod(s) to download", plan.mods_to_download.len());
    }
    if !plan.mods_to_remove.is_empty() {
        println!("  - {} mod(s) to remove", plan.mods_to_remove.len());
    }
    if !plan.configs_to_apply.is_empty() {
        println!("  ~ {} config file(s) to apply", plan.configs_to_apply.len());
    }
    if !plan.files_to_delete.is_empty() {
        println!("  x {} file(s) to delete", plan.files_to_delete.len());
    }
    match &plan.loader_action {
        LoaderAction::Install(s) => {
            println!("  * Install loader: {} {}", s.loader_type, s.version)
        }
        LoaderAction::Reinstall(s) => {
            println!("  * Reinstall loader: {} {}", s.loader_type, s.version)
        }
        LoaderAction::None => {}
    }
    println!();

    paths.ensure_all().await.context("Creating launcher directories")?;

    // 5. Remove obsolete mods
    for filename in &plan.mods_to_remove {
        let path = paths.mods.join(filename);
        if path.exists() {
            tokio::fs::remove_file(&path)
                .await
                .with_context(|| format!("Removing mod '{filename}'"))?;
            info!("Removed: {filename}");
        }
        state
            .installed_mods
            .retain(|_, v| &v.filename != filename);
    }

    // 6. Download mods
    if !plan.mods_to_download.is_empty() {
        let jobs = mod_download_jobs(&plan.mods_to_download, &paths.mods);
        let downloader = Downloader::new(
            config.runtime.download_concurrency,
            config.runtime.download_timeout_secs,
            ProgressReporter::noop(),
        )
        .context("Creating downloader")?;

        println!(
            "Downloading {} mod(s)...",
            plan.mods_to_download.len()
        );
        downloader
            .download_many(jobs)
            .await
            .context("Downloading mods")?;

        for m in &plan.mods_to_download {
            state.installed_mods.insert(
                m.id.clone(),
                InstalledMod {
                    sha512: m.sha512.clone(),
                    filename: m.filename.clone(),
                    is_optional: false,
                },
            );
        }
        println!("Done.");
    }

    // 7. Apply config overrides
    if !plan.configs_to_apply.is_empty() {
        let http = reqwest::Client::builder()
            .user_agent(concat!(
                "mc-launcher-template/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .context("Building HTTP client for configs")?;

        for cfg in &plan.configs_to_apply {
            let dest = paths.minecraft.join(&cfg.path);
            if let Some(parent) = dest.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            let bytes = http
                .get(&cfg.url)
                .send()
                .await
                .with_context(|| format!("Downloading config '{}'", cfg.path))?
                .error_for_status()
                .with_context(|| format!("HTTP error for config '{}'", cfg.path))?
                .bytes()
                .await?;
            tokio::fs::write(&dest, &bytes).await?;
            state
                .applied_configs
                .insert(cfg.path.clone(), cfg.sha512.clone());
            info!("Applied config: {}", cfg.path);
        }
    }

    // 8. Delete removed files
    for rel in &plan.files_to_delete {
        let abs = paths.minecraft.join(rel);
        if abs.exists() {
            tokio::fs::remove_file(&abs)
                .await
                .with_context(|| format!("Deleting '{rel}'"))?;
            info!("Deleted: {rel}");
        }
    }

    // 9. Save updated state
    state.applied_manifest_version = Some(manifest.manifest_version.clone());
    state.applied_at = Some(chrono::Utc::now());
    state.save(&state_path).await.context("Saving current-state.json")?;

    println!(
        "Sync complete. Applied manifest version: {}",
        manifest.manifest_version
    );

    if let Some(ann) = &manifest.announcement {
        let still_active = ann
            .show_until
            .map(|t| t > chrono::Utc::now())
            .unwrap_or(true);
        if still_active
            && !choices
                .dismissed_announcement_ids
                .contains(&ann.id)
        {
            println!("\n[ {} ]\n{}", ann.title, ann.body_md);
        }
    }

    Ok(())
}
