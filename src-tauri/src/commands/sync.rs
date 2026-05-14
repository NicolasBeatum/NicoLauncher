use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use launcher_core::progress::{ProgressEvent, ProgressReporter};
use launcher_downloader::Downloader;
use launcher_manifest_client::{
    InstalledMod, LocalState, OptionalChoices, compute_sync_plan,
    mod_cas_download_jobs, link_mods_from_cas, LoaderAction,
};


use crate::state::AppState;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct SyncPlanDto {
    pub mods_to_download: usize,
    /// Enabled optional mods not yet in cache — will be downloaded on sync
    pub optional_mods_to_download: usize,
    pub mods_to_remove: usize,
    pub configs_to_apply: usize,
    pub files_to_delete: usize,
    pub loader_action: String,
}

/// Mod en el estado instalado que ya no existe en disco (fue borrado manualmente).
#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct MissingModDto {
    pub id: String,
    pub name: String,
    pub filename: String,
}

#[derive(Debug, Serialize, Clone)]
struct ProgressDto {
    stage: Option<String>,
    current: Option<u64>,
    total: Option<u64>,
    message: Option<String>,
}

#[tauri::command]
pub async fn sync_compute_plan(state: State<'_, AppState>) -> Result<SyncPlanDto, String> {
    let manifest_guard = state.manifest.lock().await;
    let manifest = match manifest_guard.as_ref() {
        Some(m) => m,
        None => return Ok(SyncPlanDto {
            mods_to_download: 0,
            optional_mods_to_download: 0,
            mods_to_remove: 0,
            configs_to_apply: 0,
            files_to_delete: 0,
            loader_action: "none".into(),
        }),
    };

    let instance_id = state.active_instance.lock().await.clone();
    let ipaths = state.paths.instance(&instance_id);

    let local_state = LocalState::load(&ipaths.state_file).await;
    let choices = OptionalChoices::load(&ipaths.choices_file).await;
    let plan = compute_sync_plan(&local_state, manifest, &choices);

    Ok(SyncPlanDto {
        mods_to_download: plan.mods_to_download.len(),
        optional_mods_to_download: plan.optional_mods_to_download.len(),
        mods_to_remove: plan.mods_to_remove.len(),
        configs_to_apply: plan.configs_to_apply.len(),
        files_to_delete: plan.files_to_delete.len(),
        loader_action: match &plan.loader_action {
            LoaderAction::None => "none".into(),
            LoaderAction::Install(s) => format!("install:{}", s.loader_type),
            LoaderAction::Reinstall(s) => format!("reinstall:{}", s.loader_type),
        },
    })
}

/// Devuelve la lista de mods que el estado registra como instalados pero no están en disco.
/// Se llama antes de sync_apply para mostrar al usuario un aviso.
#[tauri::command]
pub async fn sync_check_missing(state: State<'_, AppState>) -> Result<Vec<MissingModDto>, String> {
    let instance_id = state.active_instance.lock().await.clone();
    let ipaths = state.paths.instance(&instance_id);
    let local_state = LocalState::load(&ipaths.state_file).await;
    let manifest_guard = state.manifest.lock().await;

    let cas_dir = &state.paths.mod_files;
    let missing: Vec<MissingModDto> = local_state
        .installed_mods
        .iter()
        .filter(|(_, info)| {
            // All mods (required and optional) live in mods/
            let in_mods = ipaths.mods.join(&info.filename).exists();
            let in_cas  = info.sha512.len() >= 4
                && cas_dir.join(&info.sha512[..2]).join(&info.sha512).exists();
            !in_mods && !in_cas
        })
        .map(|(id, info)| {
            // Intentar obtener el nombre legible desde el manifest cargado
            let name = manifest_guard
                .as_ref()
                .and_then(|m| {
                    m.required_mods
                        .iter()
                        .find(|md| &md.id == id)
                        .or_else(|| {
                            m.optional_mods
                                .iter()
                                .find(|o| &o.base.id == id)
                                .map(|o| &o.base)
                        })
                        .map(|md| md.name.clone())
                })
                .unwrap_or_else(|| id.clone());

            MissingModDto {
                id: id.clone(),
                name,
                filename: info.filename.clone(),
            }
        })
        .collect();

    Ok(missing)
}

/// Aplica el sync. `restore_mods` contiene los IDs de mods faltantes que el usuario
/// eligió volver a descargar; los no incluidos se mantienen como "instalados" en el estado
/// aunque no estén en disco (el usuario eligió continuar sin ellos).
#[tauri::command]
pub async fn sync_apply(
    restore_mods: Vec<String>,
    app: AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    let manifest = {
        let guard = state.manifest.lock().await;
        guard.clone().ok_or("No manifest loaded.")?
    };

    let instance_id = state.active_instance.lock().await.clone();
    let ipaths = state.paths.instance(&instance_id);

    let mut local_state = LocalState::load(&ipaths.state_file).await;
    let choices = OptionalChoices::load(&ipaths.choices_file).await;

    // Gestión de mods faltantes según elección del usuario:
    // - Si el mod está en disco → no se toca.
    // - Si está ausente Y el usuario eligió restaurarlo (está en restore_mods)
    //   → se elimina del estado para que compute_sync_plan lo incluya en la descarga.
    // - Si está ausente Y el usuario eligió continuar sin él
    //   → se mantiene en el estado; compute_sync_plan no lo descargará.
    local_state.installed_mods.retain(|id, info| {
        // All mods (required and optional) live in mods/
        if ipaths.mods.join(&info.filename).exists() {
            return true; // en disco, sin cambios
        }
        // Ausente: conservar en estado solo si el usuario NO lo marcó para restaurar
        !restore_mods.contains(id)
    });

    let plan = compute_sync_plan(&local_state, &manifest, &choices);

    // Asegurar que existen los directorios de la instancia
    ipaths.ensure_all().await.map_err(|e| e.to_string())?;
    state.paths.ensure_all().await.map_err(|e| e.to_string())?;

    // Emitir el plan inmediatamente para que el usuario vea qué va a pasar
    let total_mods = plan.mods_to_download.len() + plan.optional_mods_to_download.len();
    let plan_msg = format!(
        "Plan: {} req · {} opt · {} eliminar · {} configs · loader: {:?}",
        plan.mods_to_download.len(),
        plan.optional_mods_to_download.len(),
        plan.mods_to_remove.len() + plan.optional_mods_to_remove.len(),
        plan.configs_to_apply.len(),
        matches!(plan.loader_action, LoaderAction::None),
    );
    let _ = app.emit("progress", ProgressDto {
        stage: Some("Verificando…".into()),
        current: Some(0),
        total: Some((total_mods + plan.configs_to_apply.len()) as u64),
        message: Some(plan_msg),
    });

    // Eliminar mods requeridos obsoletos de mods/
    for filename in &plan.mods_to_remove {
        let path = ipaths.mods.join(filename);
        if path.exists() { tokio::fs::remove_file(&path).await.map_err(|e| e.to_string())?; }
        local_state.installed_mods.retain(|_, v| &v.filename != filename);
    }

    // Eliminar mods opcionales ahora desactivados de mods/
    for filename in &plan.optional_mods_to_remove {
        let path = ipaths.mods.join(filename);
        if path.exists() { tokio::fs::remove_file(&path).await.map_err(|e| e.to_string())?; }
        local_state.installed_mods.retain(|_, v| &v.filename != filename);
    }

    // Descargar mods → CAS (cache/mod-files/<sha[0..2]>/<sha>) → hardlink a mods/
    // Incluye requeridos + opcionales habilitados. El downloader omite archivos ya en CAS.
    let all_to_download: Vec<_> = plan.mods_to_download.iter()
        .chain(plan.optional_mods_to_download.iter())
        .cloned()
        .collect();

    if !all_to_download.is_empty() {
        let cas_dir = state.paths.mod_files.clone();
        tokio::fs::create_dir_all(&cas_dir).await.map_err(|e| e.to_string())?;

        let jobs = mod_cas_download_jobs(&all_to_download, &cas_dir);
        let total = jobs.len() as u64;

        let app_clone = app.clone();
        let (reporter, mut rx) = ProgressReporter::channel(64);

        let dl_task = tokio::spawn(async move {
            let downloader = Downloader::new(8, 120, reporter)?;
            downloader.download_many(jobs).await
        });

        tokio::spawn(async move {
            let mut current = 0u64;
            while let Some(event) = rx.recv().await {
                match event {
                    ProgressEvent::Stage { name, .. } => {
                        let _ = app_clone.emit("progress", ProgressDto {
                            stage: Some(name),
                            current: Some(0),
                            total: Some(total),
                            message: None,
                        });
                    }
                    ProgressEvent::Advance { delta } => {
                        current += delta;
                        let _ = app_clone.emit("progress", ProgressDto {
                            stage: None,
                            current: Some(current),
                            total: Some(total),
                            message: None,
                        });
                    }
                    ProgressEvent::Log { message, .. } => {
                        let _ = app_clone.emit("progress", ProgressDto {
                            stage: None,
                            current: Some(current),
                            total: Some(total),
                            message: Some(message),
                        });
                    }
                    ProgressEvent::Done => break,
                    _ => {}
                }
            }
        });

        dl_task.await.map_err(|e| e.to_string())?.map_err(|e| e.to_string())?;

        // ── Required mods ──────────────────────────────────────────────────────
        let all_entries: Vec<_> = plan.mods_to_download.iter()
            .chain(plan.optional_mods_to_download.iter())
            .cloned()
            .collect();
        link_mods_from_cas(&all_entries, &cas_dir, &ipaths.mods).await;

        for m in &plan.mods_to_download {
            local_state.installed_mods.insert(
                m.id.clone(),
                InstalledMod { sha512: m.sha512.clone(), filename: m.filename.clone(), is_optional: false },
            );
        }
        for m in &plan.optional_mods_to_download {
            local_state.installed_mods.insert(
                m.id.clone(),
                InstalledMod { sha512: m.sha512.clone(), filename: m.filename.clone(), is_optional: true },
            );
        }
    }

    // Aplicar config overrides
    // "always" → siempre sobreescribe
    // cualquier otro valor → solo si el archivo no existe en disco (no sobreescribir cambios del usuario)
    let configs_to_apply: Vec<_> = plan.configs_to_apply
        .iter()
        .filter(|c| c.apply == "always" || !ipaths.minecraft.join(&c.path).exists())
        .cloned()
        .collect();

    if !configs_to_apply.is_empty() {
        let total_cfg = configs_to_apply.len() as u64;
        let _ = app.emit("progress", ProgressDto {
            stage: Some(format!("Descargando configuración ({total_cfg} archivo{})…",
                if total_cfg == 1 { "" } else { "s" })),
            current: Some(0),
            total: Some(total_cfg),
            message: None,
        });

        // Cliente con timeout razonable; errores de config no abortan el sync
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(60))
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());

        for (i, cfg) in configs_to_apply.iter().enumerate() {
            let filename = cfg.path.split('/').last()
                .unwrap_or(&cfg.path)
                .to_string();

            // Anunciar qué archivo está descargándose
            let _ = app.emit("progress", ProgressDto {
                stage: None,
                current: Some(i as u64),
                total: Some(total_cfg),
                message: Some(format!("⬇ {filename}")),
            });

            let dest = ipaths.minecraft.join(&cfg.path);
            if let Some(parent) = dest.parent() {
                if let Err(e) = tokio::fs::create_dir_all(parent).await {
                    let _ = app.emit("progress", ProgressDto {
                        stage: None, current: None, total: None,
                        message: Some(format!("⚠ No se pudo crear directorio para {filename}: {e}")),
                    });
                    continue;
                }
            }

            match http.get(&cfg.url).send().await {
                Err(e) => {
                    let _ = app.emit("progress", ProgressDto {
                        stage: None, current: None, total: None,
                        message: Some(format!("⚠ Error descargando {filename}: {e}")),
                    });
                }
                Ok(resp) => match resp.bytes().await {
                    Err(e) => {
                        let _ = app.emit("progress", ProgressDto {
                            stage: None, current: None, total: None,
                            message: Some(format!("⚠ Error leyendo {filename}: {e}")),
                        });
                    }
                    Ok(bytes) => {
                        if let Err(e) = tokio::fs::write(&dest, &bytes).await {
                            let _ = app.emit("progress", ProgressDto {
                                stage: None, current: None, total: None,
                                message: Some(format!("⚠ Error guardando {filename}: {e}")),
                            });
                        } else {
                            local_state.applied_configs.insert(cfg.path.clone(), cfg.sha512.clone());
                        }
                    }
                },
            }

            // Avanzar barra al terminar este archivo (con éxito o error)
            let _ = app.emit("progress", ProgressDto {
                stage: None,
                current: Some((i + 1) as u64),
                total: Some(total_cfg),
                message: None,
            });
        }
    }

    // Eliminar archivos removidos del manifest
    for rel in &plan.files_to_delete {
        let abs = ipaths.minecraft.join(rel);
        if abs.exists() {
            tokio::fs::remove_file(&abs).await.map_err(|e| e.to_string())?;
        }
    }

    // Guardar estado
    local_state.applied_manifest_version = Some(manifest.manifest_version.clone());
    local_state.applied_at = Some(chrono::Utc::now());
    local_state.save(&ipaths.state_file).await.map_err(|e| e.to_string())?;

    let _ = app.emit("toast", serde_json::json!({
        "kind": "success",
        "message": format!("Sync completado — {}", manifest.manifest_version)
    }));

    Ok(())
}

/// Re-linkea desde CAS todos los mods opcionales habilitados sin re-descargarlos.
/// Útil cuando los archivos de mods/ se borraron pero la caché sigue intacta.
/// Devuelve los nombres de mods que NO estaban en caché (necesitan sync completo).
#[tauri::command]
pub async fn sync_rebuild_optional(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    let manifest = {
        let g = state.manifest.lock().await;
        g.clone().ok_or("No manifest loaded.")?
    };

    let instance_id = state.active_instance.lock().await.clone();
    let ipaths = state.paths.instance(&instance_id);
    let cas_dir = &state.paths.mod_files;
    let choices = OptionalChoices::load(&ipaths.choices_file).await;

    let mut to_link: Vec<launcher_manifest_client::ModEntry> = vec![];
    let mut needs_download: Vec<String> = vec![];

    for opt in &manifest.optional_mods {
        if !choices.enabled.contains(&opt.base.id) { continue; }
        let sha = &opt.base.sha512;
        let cas_path = if sha.len() >= 2 {
            cas_dir.join(&sha[..2]).join(sha)
        } else {
            continue;
        };
        if cas_path.exists() {
            to_link.push(opt.base.clone());
        } else {
            needs_download.push(opt.base.name.clone());
        }
    }

    if !to_link.is_empty() {
        tokio::fs::create_dir_all(&ipaths.mods).await.map_err(|e| e.to_string())?;
        link_mods_from_cas(&to_link, cas_dir, &ipaths.mods).await;
    }

    Ok(needs_download)
}
