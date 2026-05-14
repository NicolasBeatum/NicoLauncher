use std::fmt::Write as FmtWrite;

use serde::{Deserialize, Serialize};
use tauri::State;

use launcher_manifest_client::LocalState;
use crate::state::{AppState, UserSettings};

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SettingsDto {
    pub ram_mb: u32,
    pub ram_min_mb: u32,
    pub ram_max_mb: u32,
    pub java_path_override: Option<String>,
    pub extra_jvm_args: Vec<String>,
    pub theme: String,
    pub language: String,
    pub allow_ram_config: bool,
    pub allow_jvm_args_edit: bool,
    pub allow_java_path_override: bool,
}

#[tauri::command]
pub async fn settings_get(state: State<'_, AppState>) -> Result<SettingsDto, String> {
    let s = state.settings.lock().await.clone();
    Ok(SettingsDto {
        ram_mb: s.ram_mb,
        ram_min_mb: state.config.runtime.ram_min_mb,
        ram_max_mb: state.config.runtime.ram_max_mb,
        java_path_override: s.java_path_override,
        extra_jvm_args: s.extra_jvm_args,
        theme: s.theme,
        language: s.language,
        allow_ram_config: state.config.features.allow_ram_config,
        allow_jvm_args_edit: state.config.features.allow_jvm_args_edit,
        allow_java_path_override: state.config.features.allow_java_path_override,
    })
}

#[tauri::command]
pub async fn settings_set(
    state: State<'_, AppState>,
    settings: SettingsDto,
) -> Result<(), String> {
    let new_settings = UserSettings {
        ram_mb: settings.ram_mb.clamp(
            state.config.runtime.ram_min_mb,
            state.config.runtime.ram_max_mb,
        ),
        java_path_override: settings.java_path_override,
        extra_jvm_args: settings.extra_jvm_args,
        theme: settings.theme,
        language: settings.language,
    };

    let settings_path = state.paths.root.join("settings.json");
    new_settings.save(&settings_path).await.map_err(|e| e.to_string())?;
    *state.settings.lock().await = new_settings;
    Ok(())
}

#[tauri::command]
pub async fn java_detect(state: State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
    let java_dir = Some(state.paths.java.as_path());
    let result = launcher_java_manager::find_java(8, java_dir)
        .await
        .map(|j| vec![serde_json::json!({
            "binary": j.binary.display().to_string(),
            "majorVersion": j.major_version,
        })])
        .unwrap_or_default();
    Ok(result)
}

/// Abre la carpeta de mods de la instancia activa en el explorador de archivos.
#[tauri::command]
pub async fn mods_open_folder(state: State<'_, AppState>) -> Result<(), String> {
    let instance_id = state.active_instance.lock().await.clone();
    let ipaths = state.paths.instance(&instance_id);
    // Crear el directorio si aún no existe
    if !ipaths.mods.exists() {
        ipaths.ensure_all().await.map_err(|e| e.to_string())?;
    }
    open::that(&ipaths.mods).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub async fn logs_open_folder(state: State<'_, AppState>) -> Result<(), String> {
    let logs_dir = &state.paths.logs;
    if !logs_dir.exists() {
        tokio::fs::create_dir_all(logs_dir)
            .await
            .map_err(|e| e.to_string())?;
    }
    open::that(logs_dir).map_err(|e| e.to_string())?;
    Ok(())
}

/// Elimina un archivo de config override del disco y lo quita del estado aplicado,
/// de modo que el próximo sync lo vuelva a descargar.
/// Uso típico: restablecer options.txt a los valores del servidor.
#[tauri::command]
pub async fn reset_config_override(path: String, state: State<'_, AppState>) -> Result<(), String> {
    let instance_id = state.active_instance.lock().await.clone();
    let ipaths = state.paths.instance(&instance_id);

    // Eliminar el archivo del disco si existe
    let file_path = ipaths.minecraft.join(&path);
    if file_path.exists() {
        tokio::fs::remove_file(&file_path)
            .await
            .map_err(|e| e.to_string())?;
    }

    // Quitar del registro de configs aplicadas para que el sync lo vuelva a aplicar
    let mut local_state = LocalState::load(&ipaths.state_file).await;
    local_state.applied_configs.remove(&path);
    local_state.save(&ipaths.state_file).await.map_err(|e| e.to_string())?;

    Ok(())
}

/// Generate a diagnostics report file and open it with the default app.
/// Returns the absolute path of the created file.
#[tauri::command]
pub async fn create_diagnostics_report(state: State<'_, AppState>) -> Result<String, String> {
    let mut report = String::new();

    // ── Header ────────────────────────────────────────────────────────────────
    let now = chrono::Local::now();
    writeln!(report, "=== {} — Diagnostics Report ===", state.config.branding.display_name).ok();
    writeln!(report, "Generated : {}", now.format("%Y-%m-%d %H:%M:%S %Z")).ok();
    writeln!(report, "Version   : {}", env!("CARGO_PKG_VERSION")).ok();
    writeln!(report).ok();

    // ── System ────────────────────────────────────────────────────────────────
    writeln!(report, "=== System ===").ok();
    writeln!(report, "OS   : {} {}", std::env::consts::OS, std::env::consts::ARCH).ok();
    writeln!(report, "Family: {}", std::env::consts::FAMILY).ok();
    writeln!(report).ok();

    // ── Launcher config (no secrets) ──────────────────────────────────────────
    writeln!(report, "=== Config ===").ok();
    writeln!(report, "Manifest provider : {}", state.config.server.manifest_provider).ok();
    writeln!(report, "Fallback MC ver   : {}", state.config.runtime.fallback_mc_version).ok();
    writeln!(report, "RAM default       : {} MB", state.config.runtime.ram_default_mb).ok();
    writeln!(report, "Download threads  : {}", state.config.runtime.download_concurrency).ok();
    writeln!(report, "Updater enabled   : {}", state.config.updater.enabled).ok();
    writeln!(report).ok();

    // ── Paths ─────────────────────────────────────────────────────────────────
    writeln!(report, "=== Paths ===").ok();
    writeln!(report, "Root   : {}", state.paths.root.display()).ok();
    writeln!(report, "Cache  : {}", state.paths.cache.display()).ok();
    writeln!(report, "Mods   : {}", state.paths.mods.display()).ok();
    writeln!(report, "Java   : {}", state.paths.java.display()).ok();
    writeln!(report, "Logs   : {}", state.paths.logs.display()).ok();
    writeln!(report).ok();

    // ── Java detection ────────────────────────────────────────────────────────
    writeln!(report, "=== Java ===").ok();
    let java_dir = &state.paths.java;
    for major in [8u32, 11, 17, 21] {
        match launcher_java_manager::find_java(major, Some(java_dir)).await {
            Ok(j) => {
                writeln!(report, "  Java {major}+  found: {} ({})", j.full_version, j.binary.display()).ok();
            }
            Err(_) => {
                writeln!(report, "  Java {major}+  NOT found").ok();
            }
        }
    }
    writeln!(report).ok();

    // ── Recent launch logs ───────────────────────────────────────────────────
    let recent_logs: Vec<String> = state.launch_logs
        .lock()
        .map(|v| v.clone())
        .unwrap_or_default();

    if !recent_logs.is_empty() {
        writeln!(report, "=== Recent Launch Logs (last {}) ===", recent_logs.len()).ok();
        for line in &recent_logs {
            writeln!(report, "  {line}").ok();
        }
        writeln!(report).ok();
    }

    // ── Write file ────────────────────────────────────────────────────────────
    let logs_dir = &state.paths.logs;
    tokio::fs::create_dir_all(logs_dir)
        .await
        .map_err(|e| e.to_string())?;

    let filename = format!("diagnostics-{}.txt", now.format("%Y%m%d-%H%M%S"));
    let path = logs_dir.join(&filename);
    tokio::fs::write(&path, &report)
        .await
        .map_err(|e| e.to_string())?;

    // Open with the default text editor
    open::that(&path).map_err(|e| e.to_string())?;

    Ok(path.display().to_string())
}
