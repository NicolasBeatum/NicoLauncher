use std::sync::atomic::Ordering;
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_updater::UpdaterExt;
use tracing::{info, warn};

use crate::state::AppState;

/// Returned by `check_update` — serialized to the frontend.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub available: bool,
    pub version: String,
    pub current_version: String,
    pub notes: Option<String>,
}

/// Polled by the frontend while an update is being downloaded/installed.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatusDto {
    pub logs: Vec<String>,
    pub done: bool,
    pub error: Option<String>,
}

/// Check whether a newer version is available.
///
/// Returns `None` when:
///   - the updater is disabled in config
///   - `release_url` or `release_public_key` are not set
///   - the endpoint can't be reached (logged as a warning)
#[tauri::command]
pub async fn check_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<Option<UpdateInfo>, String> {
    let cfg = &state.config.updater;

    if !cfg.enabled {
        return Ok(None);
    }
    if cfg.release_url.is_empty() || cfg.release_public_key.is_empty() {
        warn!("Updater enabled but release_url or release_public_key not set — skipping check");
        return Ok(None);
    }

    let endpoint: url::Url = cfg.release_url.parse()
        .map_err(|e| format!("Invalid updater endpoint URL: {e}"))?;

    let updater = app
        .updater_builder()
        .pubkey(&cfg.release_public_key)
        .endpoints(vec![endpoint])
        .map_err(|e| format!("Updater builder error: {e}"))?
        .build()
        .map_err(|e| format!("Updater build error: {e}"))?;

    match updater.check().await {
        Ok(Some(update)) => {
            info!("Update available: {} → {}", update.current_version, update.version);
            Ok(Some(UpdateInfo {
                available: true,
                version: update.version.clone(),
                current_version: update.current_version.clone(),
                notes: update.body.clone(),
            }))
        }
        Ok(None) => {
            info!("No update available");
            Ok(Some(UpdateInfo {
                available: false,
                version: String::new(),
                current_version: app.package_info().version.to_string(),
                notes: None,
            }))
        }
        Err(e) => {
            warn!("Update check failed: {e}");
            // Don't surface network errors as hard errors — just report no update
            Ok(None)
        }
    }
}

/// Download and install the available update.
///
/// Runs in a background task. Poll `get_update_status` for progress.
/// The app must be restarted after `done: true` is returned.
#[tauri::command]
pub async fn install_update(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let cfg = state.config.updater.clone();

    if !cfg.enabled || cfg.release_url.is_empty() || cfg.release_public_key.is_empty() {
        return Err("Updater not configured".into());
    }

    // Reset state
    state.update_done.store(false, Ordering::Relaxed);
    if let Ok(mut e) = state.update_error.lock() { *e = None; }
    if let Ok(mut l) = state.update_logs.lock() { l.clear(); }

    let update_logs  = state.update_logs.clone();
    let update_done  = state.update_done.clone();
    let update_error = state.update_error.clone();

    tokio::spawn(async move {
        let result = run_install(app, cfg, update_logs.clone()).await;
        match result {
            Ok(()) => {
                update_done.store(true, Ordering::Relaxed);
            }
            Err(e) => {
                if let Ok(mut err) = update_error.lock() { *err = Some(e.clone()); }
                if let Ok(mut logs) = update_logs.lock() {
                    logs.push(format!("ERROR: {e}"));
                }
            }
        }
    });

    Ok(())
}

/// Poll the status of a running update download/install.
#[tauri::command]
pub fn get_update_status(state: State<'_, AppState>) -> UpdateStatusDto {
    let logs = state.update_logs.lock()
        .map(|mut v| { let out = v.clone(); v.clear(); out })
        .unwrap_or_default();
    let done  = state.update_done.load(Ordering::Relaxed);
    let error = state.update_error.lock().ok().and_then(|mut e| e.take());
    UpdateStatusDto { logs, done, error }
}

async fn run_install(
    app: AppHandle,
    cfg: crate::config::UpdaterConfig,
    logs: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
) -> Result<(), String> {
    let push = |msg: &str| {
        if let Ok(mut v) = logs.lock() { v.push(msg.to_string()); }
    };

    let endpoint: url::Url = cfg.release_url.parse()
        .map_err(|e| format!("Invalid URL: {e}"))?;

    let updater = app
        .updater_builder()
        .pubkey(&cfg.release_public_key)
        .endpoints(vec![endpoint])
        .map_err(|e| format!("Updater builder: {e}"))?
        .build()
        .map_err(|e| format!("Updater build: {e}"))?;

    let update = updater.check().await
        .map_err(|e| format!("Update check: {e}"))?
        .ok_or("No hay actualización disponible")?;

    push(&format!("Descargando actualización {}…", update.version));

    let mut downloaded: u64 = 0;
    let mut last_reported: u64 = 0;

    update.download_and_install(
        |chunk_size, total| {
            downloaded += chunk_size as u64;
            let report_every = 5 * 1024 * 1024; // 5 MB
            if downloaded.saturating_sub(last_reported) >= report_every {
                last_reported = downloaded;
                let mb = downloaded as f64 / 1_048_576.0;
                match total {
                    Some(t) => {
                        if let Ok(mut v) = logs.lock() {
                            v.push(format!("  {:.0} / {:.0} MB", mb, t as f64 / 1_048_576.0));
                        }
                    }
                    None => {
                        if let Ok(mut v) = logs.lock() {
                            v.push(format!("  {:.0} MB descargados", mb));
                        }
                    }
                }
            }
        },
        || {
            if let Ok(mut v) = logs.lock() {
                v.push("Instalando actualización… (el launcher se reiniciará)".into());
            }
        },
    )
    .await
    .map_err(|e| format!("Error instalando: {e}"))?;

    push("Actualización instalada. Reiniciando…");
    Ok(())
}
