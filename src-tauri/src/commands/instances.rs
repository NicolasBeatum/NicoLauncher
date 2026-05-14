use serde::{Deserialize, Serialize};
use tauri::State;

use crate::config::InstanceConfig;
use crate::state::AppState;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct InstanceDto {
    pub id:           String,
    pub display_name: String,
    pub description:  String,
    pub color:        String,
    pub icon:         String,
    pub is_active:    bool,
}

/// Formato del archivo instances-registry.json remoto.
#[derive(Debug, Deserialize)]
struct InstancesRegistry {
    instances: Vec<InstanceConfig>,
}

/// Descarga el instances-registry remoto (si está configurado) y lo guarda en estado.
/// Llámalo una vez al arrancar y cada vez que quieras refrescar.
#[tauri::command]
pub async fn refresh_instances_registry(
    state: State<'_, AppState>,
) -> Result<(), String> {
    let url = state.config.server.instances_url.clone();
    if url.is_empty() {
        // No hay URL configurada — limpiar cualquier resultado anterior y salir
        *state.remote_instances.lock().await = None;
        return Ok(());
    }

    let response = state.http
        .get(&url)
        .send()
        .await
        .map_err(|e| format!("Error al descargar instances-registry: {e}"))?;

    if !response.status().is_success() {
        return Err(format!(
            "El servidor devolvió {} al descargar instances-registry",
            response.status()
        ));
    }

    let registry: InstancesRegistry = response
        .json()
        .await
        .map_err(|e| format!("Error al parsear instances-registry: {e}"))?;

    tracing::info!(
        "instances-registry cargado: {} instancias desde {}",
        registry.instances.len(),
        url
    );

    *state.remote_instances.lock().await = Some(registry.instances);
    Ok(())
}

/// Devuelve todas las instancias (remotas si están disponibles, si no las del config),
/// marcando cuál está activa.
#[tauri::command]
pub async fn get_instances(state: State<'_, AppState>) -> Result<Vec<InstanceDto>, String> {
    let active = state.active_instance.lock().await.clone();

    let source: Vec<InstanceConfig> = {
        let remote = state.remote_instances.lock().await;
        match remote.as_ref() {
            Some(list) if !list.is_empty() => list.clone(),
            _ => state.config.effective_instances(),
        }
    };

    let dtos = source
        .into_iter()
        .map(|i| InstanceDto {
            is_active:    i.id == active,
            id:           i.id,
            display_name: i.display_name,
            description:  i.description,
            color:        i.color,
            icon:         i.icon,
        })
        .collect();

    Ok(dtos)
}

/// Cambia la instancia activa y limpia el manifest cacheado.
#[tauri::command]
pub async fn set_active_instance(
    id:    String,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Verificar que la instancia existe en remote o en config
    let exists = {
        let remote = state.remote_instances.lock().await;
        match remote.as_ref() {
            Some(list) if !list.is_empty() => list.iter().any(|i| i.id == id),
            _ => state.config.find_instance(&id).is_some(),
        }
    };

    if !exists {
        return Err(format!("Instancia '{id}' no encontrada"));
    }

    *state.active_instance.lock().await = id;
    // Limpiar manifest — el frontend lo re-descargará para la nueva instancia
    *state.manifest.lock().await = None;
    Ok(())
}

/// Devuelve el ID de la instancia activa.
#[tauri::command]
pub async fn get_active_instance(state: State<'_, AppState>) -> Result<String, String> {
    Ok(state.active_instance.lock().await.clone())
}
