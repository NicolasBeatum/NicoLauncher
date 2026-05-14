use serde::Serialize;
use tauri::State;

use launcher_auth::AuthClient;

use crate::state::AppState;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct AuthSessionDto {
    pub username: String,
    pub uuid: String,
    pub user_type: String,
}

#[tauri::command]
pub async fn auth_login_microsoft(
    state: State<'_, AppState>,
) -> Result<AuthSessionDto, String> {
    let client_id = state.config.auth.microsoft_client_id.trim();
    if client_id.is_empty() {
        return Err("microsoft_client_id not configured in launcher.config.toml".into());
    }

    let auth = AuthClient::new(client_id).map_err(|e| e.to_string())?;
    let account_json = state.paths.root.join("account.json");

    let session = auth
        .login(&account_json)
        .await
        .map_err(|e| e.to_string())?;

    let dto = AuthSessionDto {
        username: session.username.clone(),
        uuid: session.uuid.clone(),
        user_type: session.user_type.clone(),
    };

    *state.session.lock().await = Some(session);
    Ok(dto)
}

#[tauri::command]
pub async fn auth_login_offline(
    state: State<'_, AppState>,
    username: String,
) -> Result<AuthSessionDto, String> {
    let username = if username.trim().is_empty() {
        "Jugador".to_string()
    } else {
        username.trim().to_string()
    };

    let session = launcher_auth::AuthSession {
        username: username.clone(),
        uuid: "00000000-0000-0000-0000-000000000001".to_string(),
        access_token: "0".to_string(),
        user_type: "offline".to_string(),
    };

    let dto = AuthSessionDto {
        username: session.username.clone(),
        uuid: session.uuid.clone(),
        user_type: session.user_type.clone(),
    };
    *state.session.lock().await = Some(session);
    Ok(dto)
}

#[tauri::command]
pub async fn auth_logout(state: State<'_, AppState>) -> Result<(), String> {
    let client_id = state.config.auth.microsoft_client_id.trim();
    let auth = AuthClient::new(client_id).map_err(|e| e.to_string())?;
    let account_json = state.paths.root.join("account.json");
    auth.logout(&account_json).await.map_err(|e| e.to_string())?;
    *state.session.lock().await = None;
    Ok(())
}

#[tauri::command]
pub async fn auth_current_session(
    state: State<'_, AppState>,
) -> Result<Option<AuthSessionDto>, String> {
    Ok(state.session.lock().await.as_ref().map(|s| AuthSessionDto {
        username: s.username.clone(),
        uuid: s.uuid.clone(),
        user_type: s.user_type.clone(),
    }))
}

#[tauri::command]
pub async fn auth_refresh(state: State<'_, AppState>) -> Result<AuthSessionDto, String> {
    let client_id = state.config.auth.microsoft_client_id.trim();
    if client_id.is_empty() {
        return Err("microsoft_client_id not configured".into());
    }

    let auth = AuthClient::new(client_id).map_err(|e| e.to_string())?;
    let account_json = state.paths.root.join("account.json");

    match auth.resume(&account_json).await.map_err(|e| e.to_string())? {
        Some(session) => {
            let dto = AuthSessionDto {
                username: session.username.clone(),
                uuid: session.uuid.clone(),
                user_type: session.user_type.clone(),
            };
            *state.session.lock().await = Some(session);
            Ok(dto)
        }
        None => Err("No saved session. Please log in.".into()),
    }
}
