mod commands;
mod config;
mod state;

use std::sync::Arc;
use tokio::sync::Mutex;
use tauri::Manager;

use state::{AppState, UserSettings};

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .setup(|app| {
            // Load config from repo root (dev) or next to binary (release)
            let config_path = if cfg!(debug_assertions) {
                // Walk up from the exe (target/debug/) to find the workspace root
                let exe = std::env::current_exe().unwrap_or_default();
                let root = exe
                    .ancestors()
                    .find(|p| p.join("launcher.config.toml").exists())
                    .map(|p| p.join("launcher.config.toml"))
                    .unwrap_or_else(|| std::path::PathBuf::from("launcher.config.toml"));
                root
            } else {
                app.path().resource_dir()
                    .expect("resource dir")
                    .join("launcher.config.toml")
            };

            let config = config::LauncherConfig::load(&config_path)
                .expect("Cannot load launcher.config.toml");

            let paths = launcher_core::LauncherPaths::new(&config.branding.internal_id)
                .expect("Cannot determine launcher paths");

            let http = reqwest::Client::builder()
                .user_agent(concat!("mc-launcher-template/", env!("CARGO_PKG_VERSION")))
                .build()
                .expect("Cannot build HTTP client");

            let settings_path = paths.root.join("settings.json");
            // Load settings synchronously during setup (before async runtime starts)
            let settings = std::fs::read(settings_path)
                .ok()
                .and_then(|b| serde_json::from_slice(&b).ok())
                .unwrap_or_else(|| UserSettings::from_config(&config));

            // La instancia activa por defecto es la primera configurada
            let default_instance = config.effective_instances()
                .into_iter()
                .next()
                .map(|i| i.id)
                .unwrap_or_else(|| "default".into());

            let app_state = AppState {
                config,
                paths,
                http,
                session: Arc::new(Mutex::new(None)),
                manifest: Arc::new(Mutex::new(None)),
                game: Arc::new(Mutex::new(None)),
                settings: Arc::new(Mutex::new(settings)),
                active_instance: Arc::new(Mutex::new(default_instance)),
                remote_instances: Arc::new(Mutex::new(None)),
                launch_logs: Arc::new(std::sync::Mutex::new(Vec::new())),
                launch_error: Arc::new(std::sync::Mutex::new(None)),
                game_started: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                game_exit_code: Arc::new(std::sync::Mutex::new(None)),
                update_logs: Arc::new(std::sync::Mutex::new(Vec::new())),
                update_done: Arc::new(std::sync::atomic::AtomicBool::new(false)),
                update_error: Arc::new(std::sync::Mutex::new(None)),
            };

            app.manage(app_state);
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            // Branding
            commands::branding::get_branding,
            // Instances
            commands::instances::get_instances,
            commands::instances::set_active_instance,
            commands::instances::get_active_instance,
            commands::instances::refresh_instances_registry,
            // Auth
            commands::auth::auth_login_microsoft,
            commands::auth::auth_login_offline,
            commands::auth::auth_logout,
            commands::auth::auth_current_session,
            commands::auth::auth_refresh,
            // Manifest
            commands::manifest::manifest_fetch,
            commands::manifest::manifest_get_cached,
            commands::manifest::dismiss_announcement,
            // Server status
            commands::status::server_status,
            // Sync
            commands::sync::sync_compute_plan,
            commands::sync::sync_check_missing,
            commands::sync::sync_apply,
            commands::sync::sync_rebuild_optional,
            // Launch
            commands::launch::launch_game,
            commands::launch::get_launch_status,
            commands::launch::game_is_running,
            commands::launch::game_kill,
            // Server optional mods (defined in manifest)
            commands::mods::manifest_optional_mods_list,
            commands::mods::manifest_optional_mod_set_enabled,
            // User-managed mods (local .jar files in mods-optional/)
            commands::mods::user_mods_list,
            commands::mods::user_mod_set_enabled,
            commands::mods::user_mods_open_folder,
            // Settings
            commands::settings::settings_get,
            commands::settings::settings_set,
            commands::settings::java_detect,
            commands::settings::logs_open_folder,
            commands::settings::mods_open_folder,
            commands::settings::reset_config_override,
            commands::settings::create_diagnostics_report,
            // Updater
            commands::updater::check_update,
            commands::updater::install_update,
            commands::updater::get_update_status,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
