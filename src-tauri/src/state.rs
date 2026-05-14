use std::sync::Arc;
use tokio::sync::Mutex;
use serde::{Deserialize, Serialize};

use launcher_auth::AuthSession;
use launcher_launcher::launch::GameProcess;
use launcher_manifest_client::ServerManifest;

use crate::config::{InstanceConfig, LauncherConfig};

/// Persisted user settings (separate from launcher.config.toml which is admin-set).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSettings {
    pub ram_mb: u32,
    pub java_path_override: Option<String>,
    pub extra_jvm_args: Vec<String>,
    pub theme: String,
    pub language: String,
}

impl UserSettings {
    pub fn from_config(config: &LauncherConfig) -> Self {
        Self {
            ram_mb: config.runtime.ram_default_mb,
            java_path_override: None,
            extra_jvm_args: vec![],
            theme: "dark".into(),
            language: "es".into(),
        }
    }

    pub async fn load(path: &std::path::Path, config: &LauncherConfig) -> Self {
        match tokio::fs::read(path).await {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|_| Self::from_config(config)),
            Err(_) => Self::from_config(config),
        }
    }

    pub async fn save(&self, path: &std::path::Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let json = serde_json::to_vec_pretty(self)?;
        tokio::fs::write(path, json).await?;
        Ok(())
    }
}

pub struct AppState {
    pub config: LauncherConfig,
    pub paths: launcher_core::LauncherPaths,
    pub http: reqwest::Client,
    pub session: Arc<Mutex<Option<AuthSession>>>,
    pub manifest: Arc<Mutex<Option<ServerManifest>>>,
    pub game: Arc<Mutex<Option<GameProcess>>>,
    pub settings: Arc<Mutex<UserSettings>>,
    /// ID de la instancia activa (se inicializa con la primera instancia de la config)
    pub active_instance: Arc<Mutex<String>>,
    /// Instancias descargadas desde el instances-registry remoto.
    /// None = no se ha intentado cargar todavía (o no hay URL configurada).
    /// Some(vec) = lista descargada; reemplaza las instancias estáticas del config.
    pub remote_instances: Arc<Mutex<Option<Vec<InstanceConfig>>>>,
    /// Ring buffer of recent launch log lines, polled by frontend
    pub launch_logs: Arc<std::sync::Mutex<Vec<String>>>,
    /// Set to Some(error) if launch failed, None if running/idle
    pub launch_error: Arc<std::sync::Mutex<Option<String>>>,
    /// True once the game process has started
    pub game_started: Arc<std::sync::atomic::AtomicBool>,
    /// Set to Some(code) when the game process exits
    pub game_exit_code: Arc<std::sync::Mutex<Option<i32>>>,

    // ── Updater state (polled by frontend) ──────────────────────────────────
    /// Progress lines from the running update download/install
    pub update_logs: Arc<std::sync::Mutex<Vec<String>>>,
    /// True once the update has been fully applied (restart required)
    pub update_done: Arc<std::sync::atomic::AtomicBool>,
    /// Error from the last update attempt, if any
    pub update_error: Arc<std::sync::Mutex<Option<String>>>,
}
