use std::path::Path;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use launcher_core::{Error, Result};

const KEYRING_SERVICE: &str = "mc-launcher-template";
const KEYRING_ENTRY:   &str = "ms_refresh_token";

/// Metadata persisted to disk (account.json). Does NOT include tokens.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountMetadata {
    pub username: String,
    pub uuid: String,
    pub expires_at: DateTime<Utc>,
}

impl AccountMetadata {
    pub fn is_expired(&self) -> bool {
        Utc::now() >= self.expires_at
    }

    pub fn new(username: String, uuid: String, expires_in_secs: u64) -> Self {
        Self {
            username,
            uuid,
            expires_at: Utc::now() + Duration::seconds(expires_in_secs as i64),
        }
    }
}

/// Store the Microsoft refresh token securely in the OS keychain.
pub fn store_refresh_token(token: &str) -> Result<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ENTRY)
        .map_err(|e| Error::Auth(format!("Keyring entry error: {e}")))?;
    entry
        .set_password(token)
        .map_err(|e| Error::Auth(format!("Cannot store refresh token in keychain: {e}")))?;
    debug!("Refresh token stored in keychain");
    Ok(())
}

/// Retrieve the Microsoft refresh token from the OS keychain.
pub fn load_refresh_token() -> Result<String> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ENTRY)
        .map_err(|e| Error::Auth(format!("Keyring entry error: {e}")))?;
    entry.get_password().map_err(|e| match e {
        keyring::Error::NoEntry => Error::Auth(
            "No saved login found. Run `mc-launcher auth login` first.".into(),
        ),
        other => Error::Auth(format!("Cannot read refresh token from keychain: {other}")),
    })
}

/// Delete the stored refresh token (logout).
pub fn delete_refresh_token() -> Result<()> {
    let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_ENTRY)
        .map_err(|e| Error::Auth(format!("Keyring entry error: {e}")))?;
    match entry.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()), // already gone
        Err(e) => Err(Error::Auth(format!("Cannot delete token from keychain: {e}"))),
    }
}

/// Load account metadata from account.json.
pub async fn load_account(account_json: &Path) -> Option<AccountMetadata> {
    let data = tokio::fs::read(account_json).await.ok()?;
    match serde_json::from_slice(&data) {
        Ok(meta) => Some(meta),
        Err(e) => {
            warn!("Corrupted account.json, ignoring: {e}");
            None
        }
    }
}

/// Save account metadata to account.json.
pub async fn save_account(account_json: &Path, meta: &AccountMetadata) -> Result<()> {
    if let Some(parent) = account_json.parent() {
        tokio::fs::create_dir_all(parent).await?;
    }
    let json = serde_json::to_vec_pretty(meta)?;
    tokio::fs::write(account_json, json).await?;
    Ok(())
}

/// Delete account.json (logout).
pub async fn delete_account(account_json: &Path) -> Result<()> {
    if account_json.exists() {
        tokio::fs::remove_file(account_json).await?;
    }
    Ok(())
}
