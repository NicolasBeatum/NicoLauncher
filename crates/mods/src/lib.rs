use launcher_core::{Error, Result};
use serde::{Deserialize, Serialize};
use tracing::debug;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ModSource {
    Modrinth {
        project_id: String,
        version_id: String,
        download_url: Option<String>,
    },
    CurseForge {
        project_id: u64,
        file_id: u64,
        download_url: Option<String>,
    },
    SelfHosted {
        url: String,
    },
}

/// Resolve the direct download URL for a mod source.
///
/// For Modrinth entries without a `download_url`, hits the Modrinth v2 API.
/// For CurseForge entries without a `download_url`, returns an error (mirror it with SelfHosted).
pub async fn resolve_download_url(source: &ModSource, http: &reqwest::Client) -> Result<String> {
    match source {
        ModSource::SelfHosted { url } => Ok(url.clone()),

        ModSource::Modrinth {
            version_id,
            download_url,
            ..
        } => {
            if let Some(url) = download_url {
                return Ok(url.clone());
            }
            fetch_modrinth_url(version_id, http).await
        }

        ModSource::CurseForge {
            project_id,
            file_id,
            download_url,
        } => download_url.clone().ok_or_else(|| {
            Error::Other(format!(
                "CurseForge mod {project_id}/{file_id} has no direct download URL. \
                 Set allowModDistribution=true or mirror it as SelfHosted."
            ))
        }),
    }
}

async fn fetch_modrinth_url(version_id: &str, http: &reqwest::Client) -> Result<String> {
    #[derive(Deserialize)]
    struct Version {
        files: Vec<VersionFile>,
    }
    #[derive(Deserialize)]
    struct VersionFile {
        url: String,
        primary: bool,
    }

    debug!("Fetching Modrinth version {version_id} for download URL");

    let version: Version = http
        .get(format!("https://api.modrinth.com/v2/version/{version_id}"))
        .send()
        .await
        .map_err(|e| Error::Other(e.to_string()))?
        .error_for_status()
        .map_err(|e| Error::Other(e.to_string()))?
        .json()
        .await
        .map_err(|e| Error::Other(e.to_string()))?;

    version
        .files
        .iter()
        .find(|f| f.primary)
        .or_else(|| version.files.first())
        .map(|f| f.url.clone())
        .ok_or_else(|| Error::Other(format!("No files in Modrinth version {version_id}")))
}
