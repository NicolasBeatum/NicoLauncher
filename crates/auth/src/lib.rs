pub mod minecraft;
pub mod oauth;
pub mod storage;
pub mod xbox;

use std::path::Path;

use tracing::info;

use launcher_core::Result;
pub use storage::AccountMetadata;

/// An active Minecraft session ready to be passed to the game launcher.
#[derive(Debug, Clone)]
pub struct AuthSession {
    pub username: String,
    pub uuid: String,
    /// MC access token — kept in memory only, never persisted to disk.
    pub access_token: String,
    pub user_type: String,
}

impl AuthSession {
    pub fn offline(username: &str) -> Self {
        Self {
            username: username.to_string(),
            uuid: "00000000-0000-0000-0000-000000000001".to_string(),
            access_token: "0".to_string(),
            user_type: "offline".to_string(),
        }
    }
}

/// High-level auth client. Wraps the full OAuth → XBL → XSTS → MC chain.
pub struct AuthClient {
    client_id: String,
    http: reqwest::Client,
}

impl AuthClient {
    pub fn new(client_id: impl Into<String>) -> launcher_core::Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!(
                "mc-launcher-template/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .map_err(|e| launcher_core::Error::Auth(e.to_string()))?;
        Ok(Self { client_id: client_id.into(), http })
    }

    /// Full login flow: opens browser, waits for OAuth callback, chains to MC.
    /// Stores refresh token in keychain and saves metadata to `account_json`.
    pub async fn login(&self, account_json: &Path) -> Result<AuthSession> {
        info!("Starting Microsoft OAuth login…");
        let ms_tokens = oauth::login(&self.client_id, &self.http).await?;

        storage::store_refresh_token(&ms_tokens.refresh_token)?;

        let xsts = xbox::authenticate(&ms_tokens.access_token, &self.http).await?;
        let mc   = minecraft::login(&xsts, &self.http).await?;

        info!("Logged in as {} ({})", mc.username, mc.uuid);

        let meta = AccountMetadata::new(
            mc.username.clone(),
            mc.uuid.clone(),
            ms_tokens.expires_in,
        );
        storage::save_account(account_json, &meta).await?;

        Ok(AuthSession {
            username: mc.username,
            uuid: mc.uuid,
            access_token: mc.access_token,
            user_type: "msa".to_string(),
        })
    }

    /// Resume an existing session: refresh MS token if needed, re-obtain MC token.
    /// Returns `None` if no saved session exists (user needs to login).
    pub async fn resume(&self, account_json: &Path) -> Result<Option<AuthSession>> {
        let meta = match storage::load_account(account_json).await {
            Some(m) => m,
            None    => return Ok(None),
        };

        let refresh_token = match storage::load_refresh_token() {
            Ok(t)  => t,
            Err(_) => return Ok(None),
        };

        info!("Refreshing session for {}…", meta.username);
        let ms_tokens = oauth::refresh(&self.client_id, &refresh_token, &self.http).await?;

        // Store updated refresh token (MS may rotate it)
        storage::store_refresh_token(&ms_tokens.refresh_token)?;

        let xsts = xbox::authenticate(&ms_tokens.access_token, &self.http).await?;
        let mc   = minecraft::login(&xsts, &self.http).await?;

        // Update metadata with new expiry
        let new_meta = AccountMetadata::new(
            mc.username.clone(),
            mc.uuid.clone(),
            ms_tokens.expires_in,
        );
        storage::save_account(account_json, &new_meta).await?;

        Ok(Some(AuthSession {
            username: mc.username,
            uuid: mc.uuid,
            access_token: mc.access_token,
            user_type: "msa".to_string(),
        }))
    }

    /// Log out: delete keychain token and account.json.
    pub async fn logout(&self, account_json: &Path) -> Result<()> {
        storage::delete_refresh_token()?;
        storage::delete_account(account_json).await?;
        info!("Logged out");
        Ok(())
    }
}
