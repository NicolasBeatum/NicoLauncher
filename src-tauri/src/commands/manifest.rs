use serde::Serialize;
use tauri::State;

use launcher_manifest_client::{FileProvider, GitProvider, HttpProvider, ManifestProvider, OptionalChoices, ServerManifest, fetch_manifest};

use crate::config::InstanceConfig;
use crate::state::AppState;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct ServerManifestDto {
    pub manifest_version: String,
    pub mc_version: String,
    pub loader_type: Option<String>,
    pub loader_version: Option<String>,
    pub required_mods_count: usize,
    pub optional_mods_count: usize,
    /// ID del anuncio activo (necesario para llamar dismiss_announcement).
    pub announcement_id: Option<String>,
    pub announcement_title: Option<String>,
    pub announcement_body: Option<String>,
    /// true si el usuario ya descartó este anuncio.
    pub announcement_dismissed: bool,
}

fn manifest_to_dto(manifest: &ServerManifest, dismissed_ids: &[String]) -> ServerManifestDto {
    let (ann_id, ann_title, ann_body, dismissed) = match &manifest.announcement {
        None => (None, None, None, false),
        Some(ann) => {
            // Respetar show_until si está definido
            let expired = ann.show_until
                .map(|t| chrono::Utc::now() > t)
                .unwrap_or(false);
            let dismissed = dismissed_ids.contains(&ann.id) || expired;
            (
                Some(ann.id.clone()),
                Some(ann.title.clone()),
                Some(ann.body_md.clone()),
                dismissed,
            )
        }
    };

    ServerManifestDto {
        manifest_version: manifest.manifest_version.clone(),
        mc_version: manifest.minecraft.version.clone(),
        loader_type: manifest.loader.as_ref().map(|l| l.loader_type.to_string()),
        loader_version: manifest.loader.as_ref().map(|l| l.version.clone()),
        required_mods_count: manifest.required_mods.len(),
        optional_mods_count: manifest.optional_mods.len(),
        announcement_id: ann_id,
        announcement_title: ann_title,
        announcement_body: ann_body,
        announcement_dismissed: dismissed,
    }
}

fn build_provider(
    manifest_url:      &str,
    manifest_provider: &str,
) -> Result<Box<dyn ManifestProvider>, String> {
    match manifest_provider {
        "http" | "https" => Ok(Box::new(
            HttpProvider::new(manifest_url).map_err(|e| e.to_string())?,
        )),
        // Git hosting: GitHub, GitLab, Gitea, etc.
        // Accepts standard blob URLs and converts to raw automatically.
        // Example: https://github.com/user/repo/blob/main/manifest.json
        "git" | "github" | "gitlab" => Ok(Box::new(
            GitProvider::new(manifest_url).map_err(|e| e.to_string())?,
        )),
        "file" => {
            let path_str = manifest_url.trim_start_matches("file://");
            let path = if std::path::Path::new(path_str).is_absolute() {
                std::path::PathBuf::from(path_str)
            } else {
                let exe = std::env::current_exe().unwrap_or_default();
                let root = exe
                    .ancestors()
                    .find(|p| p.join("launcher.config.toml").exists())
                    .unwrap_or_else(|| exe.parent().unwrap_or(std::path::Path::new(".")));
                root.join(path_str)
            };
            Ok(Box::new(FileProvider::new(path)))
        }
        other => Err(format!("Unknown manifest_provider: '{other}'. Valid: http, git, file")),
    }
}

/// Busca la instancia activa tanto en remote_instances como en config (en ese orden).
async fn resolve_active_instance(state: &AppState, instance_id: &str) -> Option<InstanceConfig> {
    let remote = state.remote_instances.lock().await;
    if let Some(list) = remote.as_ref() {
        if let Some(inst) = list.iter().find(|i| i.id == instance_id) {
            return Some(inst.clone());
        }
    }
    state.config.find_instance(instance_id)
}

#[tauri::command]
pub async fn manifest_fetch(state: State<'_, AppState>) -> Result<ServerManifestDto, String> {
    let instance_id = state.active_instance.lock().await.clone();
    let instance = resolve_active_instance(&state, &instance_id)
        .await
        .ok_or_else(|| format!("Instancia '{instance_id}' no encontrada"))?;

    let ipaths = state.paths.instance(&instance_id);
    let cache_path = state.paths.manifest_cache.join(format!("{instance_id}-manifest.json"));
    let choices = OptionalChoices::load(&ipaths.choices_file).await;

    let provider = build_provider(&instance.manifest_url, &instance.manifest_provider)?;

    match fetch_manifest(&*provider, &instance.manifest_public_key).await {
        Ok(manifest) => {
            // Guardar en caché de disco para uso offline
            if let Ok(json) = serde_json::to_vec_pretty(&manifest) {
                let _ = tokio::fs::create_dir_all(&state.paths.manifest_cache).await;
                let _ = tokio::fs::write(&cache_path, &json).await;
            }
            let dto = manifest_to_dto(&manifest, &choices.dismissed_announcement_ids);
            *state.manifest.lock().await = Some(manifest);
            Ok(dto)
        }
        Err(live_err) => {
            // Intentar cargar desde caché de disco (modo offline)
            match tokio::fs::read(&cache_path).await {
                Ok(bytes) => match serde_json::from_slice::<launcher_manifest_client::ServerManifest>(&bytes) {
                    Ok(manifest) => {
                        tracing::warn!("Manifest fetch failed ({live_err}); using cached version");
                        let dto = manifest_to_dto(&manifest, &choices.dismissed_announcement_ids);
                        *state.manifest.lock().await = Some(manifest);
                        // Indicar al frontend que está en modo offline
                        Err(format!("__offline__:{}", dto.manifest_version))
                    }
                    Err(_) => Err(live_err.to_string()),
                },
                Err(_) => Err(live_err.to_string()),
            }
        }
    }
}

#[tauri::command]
pub async fn manifest_get_cached(
    state: State<'_, AppState>,
) -> Result<Option<ServerManifestDto>, String> {
    let guard = state.manifest.lock().await;
    let Some(manifest) = guard.as_ref() else { return Ok(None) };

    let instance_id = state.active_instance.lock().await.clone();
    let ipaths = state.paths.instance(&instance_id);
    let choices = OptionalChoices::load(&ipaths.choices_file).await;

    Ok(Some(manifest_to_dto(manifest, &choices.dismissed_announcement_ids)))
}

/// Descarta un anuncio: no volverá a mostrarse para esta instancia.
#[tauri::command]
pub async fn dismiss_announcement(
    id:    String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let instance_id = state.active_instance.lock().await.clone();
    let ipaths = state.paths.instance(&instance_id);
    let mut choices = OptionalChoices::load(&ipaths.choices_file).await;

    if !choices.dismissed_announcement_ids.contains(&id) {
        choices.dismissed_announcement_ids.push(id);
        choices.save(&ipaths.choices_file).await.map_err(|e| e.to_string())?;
    }
    Ok(())
}
