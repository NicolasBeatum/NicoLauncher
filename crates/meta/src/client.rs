use std::path::Path;
use tokio::io::AsyncWriteExt;
use tracing::{debug, info};

use launcher_core::{Error, Result};
use crate::types::{AssetObjects, VersionJson, VersionManifestV2};

const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

const USER_AGENT: &str = concat!(
    "mc-launcher-template/",
    env!("CARGO_PKG_VERSION"),
    " (github.com/YOUR_ORG/mc-launcher-template)"
);

pub struct MojangMetaClient {
    client: reqwest::Client,
}

impl MojangMetaClient {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(Self { client })
    }

    /// Fetch the global version manifest (list of all MC versions).
    /// Pass `cache_path` to read a cached copy; re-fetch if missing or if `force` is true.
    pub async fn fetch_version_manifest(
        &self,
        cache_path: Option<&Path>,
        force: bool,
    ) -> Result<VersionManifestV2> {
        if let Some(path) = cache_path {
            if path.exists() && !force {
                debug!("Version manifest: reading from cache {:?}", path);
                let data = tokio::fs::read(path).await?;
                return Ok(serde_json::from_slice(&data)?);
            }
        }

        info!("Fetching version manifest from Mojang…");
        let bytes = self
            .client
            .get(VERSION_MANIFEST_URL)
            .send()
            .await
            .map_err(|e| Error::Other(e.to_string()))?
            .error_for_status()
            .map_err(|e| Error::Other(e.to_string()))?
            .bytes()
            .await
            .map_err(|e| Error::Other(e.to_string()))?;

        if let Some(path) = cache_path {
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            let mut f = tokio::fs::File::create(path).await?;
            f.write_all(&bytes).await?;
        }

        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Fetch the JSON for a specific MC version by its URL (from the version manifest).
    pub async fn fetch_version_json(
        &self,
        url: &str,
        cache_path: Option<&Path>,
    ) -> Result<VersionJson> {
        if let Some(path) = cache_path {
            if path.exists() {
                debug!("Version JSON: reading from cache {:?}", path);
                let data = tokio::fs::read(path).await?;
                return Ok(serde_json::from_slice(&data)?);
            }
        }

        info!("Fetching version JSON from {url}");
        let bytes = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| Error::Other(e.to_string()))?
            .error_for_status()
            .map_err(|e| Error::Other(e.to_string()))?
            .bytes()
            .await
            .map_err(|e| Error::Other(e.to_string()))?;

        if let Some(path) = cache_path {
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            let mut f = tokio::fs::File::create(path).await?;
            f.write_all(&bytes).await?;
        }

        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Fetch the asset objects index (the full map of virtual path → hash/size).
    pub async fn fetch_asset_index(
        &self,
        url: &str,
        cache_path: Option<&Path>,
    ) -> Result<AssetObjects> {
        if let Some(path) = cache_path {
            if path.exists() {
                debug!("Asset index: reading from cache {:?}", path);
                let data = tokio::fs::read(path).await?;
                return Ok(serde_json::from_slice(&data)?);
            }
        }

        info!("Fetching asset index from {url}");
        let bytes = self
            .client
            .get(url)
            .send()
            .await
            .map_err(|e| Error::Other(e.to_string()))?
            .error_for_status()
            .map_err(|e| Error::Other(e.to_string()))?
            .bytes()
            .await
            .map_err(|e| Error::Other(e.to_string()))?;

        if let Some(path) = cache_path {
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            let mut f = tokio::fs::File::create(path).await?;
            f.write_all(&bytes).await?;
        }

        Ok(serde_json::from_slice(&bytes)?)
    }

    /// Helper: look up a version entry by ID and return its download URL.
    pub async fn version_url(
        &self,
        version_id: &str,
        cache_path: Option<&Path>,
    ) -> Result<String> {
        let manifest = self.fetch_version_manifest(cache_path, false).await?;
        manifest
            .versions
            .iter()
            .find(|v| v.id == version_id)
            .map(|v| v.url.clone())
            .ok_or_else(|| launcher_core::Error::VersionNotFound(version_id.to_string()))
    }
}

impl Default for MojangMetaClient {
    fn default() -> Self {
        Self::new().expect("Failed to build HTTP client")
    }
}
