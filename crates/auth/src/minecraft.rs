use tracing::debug;

use launcher_core::{Error, Result};
use crate::xbox::XstsTokens;

const MC_LOGIN_URL:   &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MC_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

#[derive(Debug, Clone)]
pub struct McSession {
    pub access_token: String,
    pub username: String,
    pub uuid: String,
}

pub async fn login(xsts: &XstsTokens, http: &reqwest::Client) -> Result<McSession> {
    let mc_token = mc_login(xsts, http).await?;
    debug!("MC access token obtained, fetching profile…");
    let (uuid, username) = mc_profile(&mc_token, http).await?;
    Ok(McSession { access_token: mc_token, username, uuid })
}

async fn mc_login(xsts: &XstsTokens, http: &reqwest::Client) -> Result<String> {
    let identity_token = format!("XBL3.0 x={};{}", xsts.user_hash, xsts.xsts_token);

    let body = serde_json::json!({ "identityToken": identity_token });

    let resp: serde_json::Value = http
        .post(MC_LOGIN_URL)
        .json(&body)
        .send()
        .await
        .map_err(|e| Error::Auth(format!("MC login request failed: {e}")))?
        .json()
        .await
        .map_err(|e| Error::Auth(format!("MC login response parse error: {e}")))?;

    resp.get("access_token")
        .and_then(|t| t.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| Error::Auth(format!("No access_token in MC login response: {resp}")))
}

async fn mc_profile(access_token: &str, http: &reqwest::Client) -> Result<(String, String)> {
    let resp: serde_json::Value = http
        .get(MC_PROFILE_URL)
        .bearer_auth(access_token)
        .send()
        .await
        .map_err(|e| Error::Auth(format!("MC profile request failed: {e}")))?
        .json()
        .await
        .map_err(|e| Error::Auth(format!("MC profile response parse error: {e}")))?;

    // Error case: account doesn't own Minecraft
    if let Some(err) = resp.get("error") {
        if err.as_str() == Some("NOT_FOUND") {
            return Err(Error::Auth(
                "This Microsoft account does not own Minecraft Java Edition.".into(),
            ));
        }
        return Err(Error::Auth(format!("MC profile error: {err}")));
    }

    let uuid = resp
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Auth("Missing 'id' in MC profile".into()))?
        .to_string();

    let username = resp
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or_else(|| Error::Auth("Missing 'name' in MC profile".into()))?
        .to_string();

    // Format UUID with dashes: xxxxxxxx-xxxx-xxxx-xxxx-xxxxxxxxxxxx
    let uuid_formatted = if uuid.len() == 32 && !uuid.contains('-') {
        format!(
            "{}-{}-{}-{}-{}",
            &uuid[0..8],
            &uuid[8..12],
            &uuid[12..16],
            &uuid[16..20],
            &uuid[20..32]
        )
    } else {
        uuid
    };

    Ok((uuid_formatted, username))
}
