use tracing::debug;

use launcher_core::{Error, Result};
use crate::oauth::json_str;

const XBL_URL:  &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";

#[derive(Debug)]
pub struct XstsTokens {
    pub xsts_token: String,
    pub user_hash: String,
}

pub async fn authenticate(
    ms_access_token: &str,
    http: &reqwest::Client,
) -> Result<XstsTokens> {
    let xbl = xbl_auth(ms_access_token, http).await?;
    debug!("XBL token obtained");
    let xsts = xsts_auth(&xbl.xbl_token, http).await?;
    debug!("XSTS token obtained");
    Ok(xsts)
}

// ── Internal ──────────────────────────────────────────────────────────────────

struct XblResult {
    xbl_token: String,
    #[allow(dead_code)]
    user_hash: String,
}

async fn xbl_auth(ms_access_token: &str, http: &reqwest::Client) -> Result<XblResult> {
    let body = serde_json::json!({
        "Properties": {
            "AuthMethod": "RPS",
            "SiteName": "user.auth.xboxlive.com",
            "RpsTicket": format!("d={ms_access_token}")
        },
        "RelyingParty": "http://auth.xboxlive.com",
        "TokenType": "JWT"
    });

    let resp: serde_json::Value = http
        .post(XBL_URL)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| Error::Auth(format!("XBL request failed: {e}")))?
        .json()
        .await
        .map_err(|e| Error::Auth(format!("XBL response parse error: {e}")))?;

    let token    = json_str(&resp, "Token")?;
    let user_hash = resp["DisplayClaims"]["xui"][0]["uhs"]
        .as_str()
        .ok_or_else(|| Error::Auth("Missing uhs in XBL response".into()))?
        .to_string();

    Ok(XblResult { xbl_token: token, user_hash })
}

async fn xsts_auth(xbl_token: &str, http: &reqwest::Client) -> Result<XstsTokens> {
    let body = serde_json::json!({
        "Properties": {
            "SandboxId": "RETAIL",
            "UserTokens": [xbl_token]
        },
        "RelyingParty": "rp://api.minecraftservices.com/",
        "TokenType": "JWT"
    });

    let resp: serde_json::Value = http
        .post(XSTS_URL)
        .header("Content-Type", "application/json")
        .header("Accept", "application/json")
        .json(&body)
        .send()
        .await
        .map_err(|e| Error::Auth(format!("XSTS request failed: {e}")))?
        .json()
        .await
        .map_err(|e| Error::Auth(format!("XSTS response parse error: {e}")))?;

    // XSTS-specific error codes
    if let Some(xerr) = resp.get("XErr").and_then(|v| v.as_u64()) {
        let msg = match xerr {
            2148916233 => "This Microsoft account has no Xbox account. \
                           Create one at xbox.com and try again.",
            2148916238 => "This account belongs to a minor. A parent or guardian \
                           must add it to a Family group first.",
            _ => "XSTS authentication failed. Make sure your account has access to Minecraft.",
        };
        return Err(Error::Auth(msg.to_string()));
    }

    let token     = json_str(&resp, "Token")?;
    let user_hash = resp["DisplayClaims"]["xui"][0]["uhs"]
        .as_str()
        .ok_or_else(|| Error::Auth("Missing uhs in XSTS response".into()))?
        .to_string();

    Ok(XstsTokens { xsts_token: token, user_hash })
}
