/// Mod management commands.
///
/// Two separate systems coexist:
///
/// 1. **Server optional mods** — defined in the manifest's `optional_mods` field.
///    Downloaded via CAS and linked into `mods/` when enabled.
///    Managed by sync (play button), with a "rebuild" shortcut to re-link from cache.
///
/// 2. **User mods** — arbitrary `.jar` files placed by the user in `mods-optional/`.
///    Nothing to do with the server; enabled/disabled by hardlinking into `mods/`.
use serde::Serialize;
use tauri::State;

use launcher_manifest_client::{link_mods_from_cas, OptionalChoices};

use crate::state::AppState;

// ── Server optional mods (manifest-defined) ────────────────────────────────────

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct OptionalModDto {
    pub id: String,
    pub name: String,
    pub description: Option<String>,
    pub category: Option<String>,
    pub icon_url: Option<String>,
    pub default_enabled: bool,
    pub depends_on: Vec<String>,
    pub conflicts_with: Vec<String>,
    /// User has enabled this mod
    pub enabled: bool,
    /// .jar is already in the local CAS cache (enabling is instant, no download needed)
    pub in_cache: bool,
    /// Hardlink currently present in mods/ (mod is actually active in the game right now)
    pub linked: bool,
}

/// List server-defined optional mods with their current enabled/cached/linked status.
#[tauri::command]
pub async fn manifest_optional_mods_list(
    state: State<'_, AppState>,
) -> Result<Vec<OptionalModDto>, String> {
    let manifest_guard = state.manifest.lock().await;
    let manifest = manifest_guard.as_ref().ok_or("No manifest loaded.")?;

    let instance_id = state.active_instance.lock().await.clone();
    let ipaths = state.paths.instance(&instance_id);
    let choices = OptionalChoices::load(&ipaths.choices_file).await;
    let cas_dir = &state.paths.mod_files;

    let dtos = manifest
        .optional_mods
        .iter()
        .map(|m| {
            let sha = &m.base.sha512;
            let in_cache = sha.len() >= 2
                && cas_dir.join(&sha[..2]).join(sha).exists();
            let linked = ipaths.mods.join(&m.base.filename).exists();
            OptionalModDto {
                id: m.base.id.clone(),
                name: m.base.name.clone(),
                description: m.description.clone(),
                category: m.category.clone(),
                icon_url: m.icon_url.clone(),
                default_enabled: m.default_enabled,
                depends_on: m.depends_on.clone(),
                conflicts_with: m.conflicts_with.clone(),
                enabled: choices.enabled.contains(&m.base.id),
                in_cache,
                linked,
            }
        })
        .collect();

    Ok(dtos)
}

/// Enable or disable a server optional mod.
///
/// If enabling and the mod is already in CAS → links it to mods/ immediately (returns `true`).
/// If enabling and NOT in CAS → saves the choice; next sync will download + link (returns `false`).
/// If disabling → removes the link from mods/ and updates choices.
#[tauri::command]
pub async fn manifest_optional_mod_set_enabled(
    id: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<bool, String> {
    let manifest_guard = state.manifest.lock().await;
    let manifest = manifest_guard.as_ref().ok_or("No manifest loaded.")?;

    let instance_id = state.active_instance.lock().await.clone();
    let ipaths = state.paths.instance(&instance_id);
    let cas_dir = state.paths.mod_files.clone();
    let mut choices = OptionalChoices::load(&ipaths.choices_file).await;

    let opt = manifest
        .optional_mods
        .iter()
        .find(|m| m.base.id == id)
        .ok_or_else(|| format!("Mod '{id}' not found in manifest"))?;

    let mut linked_now = false;

    if enabled {
        if !choices.enabled.contains(&id) {
            choices.enabled.push(id.clone());
        }
        // Instant link if already cached
        let sha = &opt.base.sha512;
        let cas_path = if sha.len() >= 2 { cas_dir.join(&sha[..2]).join(sha) } else { Default::default() };
        if cas_path.exists() {
            tokio::fs::create_dir_all(&ipaths.mods).await.map_err(|e| e.to_string())?;
            link_mods_from_cas(&[opt.base.clone()], &cas_dir, &ipaths.mods).await;
            linked_now = true;
        }
    } else {
        choices.enabled.retain(|e| e != &id);
        // Remove link from mods/ if present
        let dst = ipaths.mods.join(&opt.base.filename);
        let _ = tokio::fs::remove_file(&dst).await;
    }

    choices
        .save(&ipaths.choices_file)
        .await
        .map_err(|e| e.to_string())?;

    Ok(linked_now)
}

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct UserModDto {
    /// .jar filename
    pub filename: String,
    /// true → hardlinked into mods/ and will be loaded by the game
    pub enabled: bool,
    /// file size in bytes (for display)
    pub size_bytes: u64,
}

// ── Helpers ────────────────────────────────────────────────────────────────────

async fn load_enabled(path: &std::path::Path) -> Vec<String> {
    tokio::fs::read(path)
        .await
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .unwrap_or_default()
}

async fn save_enabled(path: &std::path::Path, list: &[String]) -> Result<(), String> {
    if let Some(p) = path.parent() {
        tokio::fs::create_dir_all(p).await.map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_vec_pretty(list).map_err(|e| e.to_string())?;
    tokio::fs::write(path, json).await.map_err(|e| e.to_string())
}

// ── Commands ───────────────────────────────────────────────────────────────────

/// Return all .jar files found in `mods-optional/` with their enabled state.
#[tauri::command]
pub async fn user_mods_list(state: State<'_, AppState>) -> Result<Vec<UserModDto>, String> {
    let instance_id = state.active_instance.lock().await.clone();
    let ipaths = state.paths.instance(&instance_id);

    // Ensure the folder exists so the user can see it in the file manager
    tokio::fs::create_dir_all(&ipaths.optional_mods)
        .await
        .map_err(|e| e.to_string())?;

    let enabled = load_enabled(&ipaths.user_mods_state).await;

    let mut mods: Vec<UserModDto> = vec![];
    let mut dir = match tokio::fs::read_dir(&ipaths.optional_mods).await {
        Ok(d) => d,
        Err(_) => return Ok(vec![]),
    };
    while let Ok(Some(entry)) = dir.next_entry().await {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("jar") {
            let filename = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            let size_bytes = entry.metadata().await.map(|m| m.len()).unwrap_or(0);
            mods.push(UserModDto {
                enabled: enabled.contains(&filename),
                filename,
                size_bytes,
            });
        }
    }
    mods.sort_by(|a, b| a.filename.cmp(&b.filename));
    Ok(mods)
}

/// Enable or disable a user mod.
///
/// Enabling  → hardlink (or copy) `mods-optional/<filename>` → `mods/<filename>`
/// Disabling → remove `mods/<filename>` (only the copy we placed there)
#[tauri::command]
pub async fn user_mod_set_enabled(
    filename: String,
    enabled: bool,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let instance_id = state.active_instance.lock().await.clone();
    let ipaths = state.paths.instance(&instance_id);

    let src = ipaths.optional_mods.join(&filename);
    let dst = ipaths.mods.join(&filename);

    let mut list = load_enabled(&ipaths.user_mods_state).await;

    if enabled {
        if !src.exists() {
            return Err(format!(
                "Archivo no encontrado en mods-optional: {filename}"
            ));
        }
        tokio::fs::create_dir_all(&ipaths.mods)
            .await
            .map_err(|e| e.to_string())?;
        // Remove stale link / old version
        let _ = tokio::fs::remove_file(&dst).await;
        // Hardlink first (same filesystem → instant, zero extra disk space)
        // Fall back to copy if hardlink fails (different drive, etc.)
        let ok = tokio::fs::hard_link(&src, &dst).await.is_ok()
            || tokio::fs::copy(&src, &dst).await.map(|_| ()).is_ok();
        if !ok {
            return Err(format!("No se pudo activar {filename}"));
        }
        if !list.contains(&filename) {
            list.push(filename.clone());
        }
    } else {
        // Only remove from mods/ what we put there
        if list.contains(&filename) {
            let _ = tokio::fs::remove_file(&dst).await;
            list.retain(|f| f != &filename);
        }
    }

    save_enabled(&ipaths.user_mods_state, &list).await?;
    Ok(())
}

/// Open `mods-optional/` in the system file manager so the user can add/remove .jar files.
#[tauri::command]
pub async fn user_mods_open_folder(state: State<'_, AppState>) -> Result<(), String> {
    let instance_id = state.active_instance.lock().await.clone();
    let ipaths = state.paths.instance(&instance_id);
    tokio::fs::create_dir_all(&ipaths.optional_mods)
        .await
        .map_err(|e| e.to_string())?;

    open_in_file_manager(&ipaths.optional_mods)
}

fn open_in_file_manager(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    #[cfg(target_os = "linux")]
    {
        std::process::Command::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
