use std::path::Path;

use serde::Deserialize;
use tracing::{debug, info};

use launcher_core::{Error, Result, maven_to_path};
use launcher_downloader::DownloadJob;
use launcher_meta::types::{Arguments, Library, LibraryDownloads, Artifact};

use crate::merge::LoaderProfile;

const FABRIC_META: &str = "https://meta.fabricmc.net/v2";

pub struct FabricProvider {
    client: reqwest::Client,
}

impl FabricProvider {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(concat!(
                "mc-launcher-template/",
                env!("CARGO_PKG_VERSION")
            ))
            .build()
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(Self { client })
    }

    /// List available Fabric loader versions for a given MC version.
    pub async fn list_versions(&self, mc_version: &str) -> Result<Vec<String>> {
        let url = format!("{FABRIC_META}/versions/loader/{mc_version}");
        let entries: Vec<FabricLoaderEntry> = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| Error::Other(e.to_string()))?
            .json()
            .await
            .map_err(|e| Error::Other(e.to_string()))?;

        Ok(entries.into_iter().map(|e| e.loader.version).collect())
    }

    /// Return the most recent stable Fabric loader version for a given MC version.
    pub async fn recommended_version(&self, mc_version: &str) -> Result<String> {
        self.list_versions(mc_version)
            .await?
            .into_iter()
            .find(|v| !v.contains("beta") && !v.contains("alpha"))
            .ok_or_else(|| Error::Other(format!("No stable Fabric loader for MC {mc_version}")))
    }

    /// Fetch the Fabric profile JSON and convert it to a `LoaderProfile` ready for merging.
    pub async fn resolve_profile(
        &self,
        mc_version: &str,
        loader_version: &str,
        cache_path: Option<&Path>,
    ) -> Result<LoaderProfile> {
        let url = format!(
            "{FABRIC_META}/versions/loader/{mc_version}/{loader_version}/profile/json"
        );

        let bytes = if let Some(path) = cache_path {
            if path.exists() {
                debug!("Fabric profile: reading from cache {:?}", path);
                tokio::fs::read(path).await?
            } else {
                self.download_bytes(&url, path).await?
            }
        } else {
            self.client
                .get(&url)
                .send()
                .await
                .map_err(|e| Error::Other(e.to_string()))?
                .bytes()
                .await
                .map_err(|e| Error::Other(e.to_string()))?
                .to_vec()
        };

        let profile: FabricProfileJson = serde_json::from_slice(&bytes)
            .map_err(|e| Error::Other(format!("Fabric profile parse error: {e}")))?;

        info!(
            "Fabric loader {loader_version} for MC {mc_version}: {} libraries",
            profile.libraries.len()
        );

        Ok(LoaderProfile {
            main_class: profile.main_class,
            libraries: profile
                .libraries
                .into_iter()
                .map(fabric_lib_to_meta_lib)
                .collect(),
            arguments: profile.arguments.map(|a| Arguments {
                game: a.game.into_iter().map(launcher_meta::types::Argument::Plain).collect(),
                jvm:  a.jvm .into_iter().map(launcher_meta::types::Argument::Plain).collect(),
            }),
        })
    }

    /// Build `DownloadJob`s for all Fabric libraries that need downloading.
    pub fn library_download_jobs(
        profile: &LoaderProfile,
        libraries_dir: &Path,
    ) -> Vec<DownloadJob> {
        profile
            .libraries
            .iter()
            .filter_map(|lib| {
                if let Some(dl) = &lib.downloads {
                    if let Some(artifact) = &dl.artifact {
                        if artifact.url.is_empty() { return None; }
                        let dest = if let Some(path) = &artifact.path {
                            libraries_dir.join(path)
                        } else if let Some(rel) = maven_to_path(&lib.name) {
                            libraries_dir.join(rel)
                        } else {
                            return None;
                        };
                        return Some(
                            DownloadJob::new(&artifact.url, dest)
                                .with_sha1(&artifact.sha1)
                        );
                    }
                }
                // Library only has a `url` base (Maven repo) — skip; resolved via maven_url
                None
            })
            .collect()
    }

    async fn download_bytes(&self, url: &str, cache_path: &Path) -> Result<Vec<u8>> {
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
            .map_err(|e| Error::Other(e.to_string()))?
            .to_vec();

        if let Some(parent) = cache_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        tokio::fs::write(cache_path, &bytes).await?;

        Ok(bytes)
    }
}

impl Default for FabricProvider {
    fn default() -> Self {
        Self::new().expect("Failed to build Fabric HTTP client")
    }
}

// ── Fabric JSON types (from meta.fabricmc.net) ────────────────────────────────

#[derive(Deserialize)]
struct FabricLoaderEntry {
    loader: FabricLoaderVersion,
}

#[derive(Deserialize)]
struct FabricLoaderVersion {
    version: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct FabricProfileJson {
    main_class: String,
    libraries: Vec<FabricLibrary>,
    #[serde(default)]
    arguments: Option<FabricArguments>,
}

#[derive(Deserialize)]
struct FabricArguments {
    #[serde(default)]
    game: Vec<String>,
    #[serde(default)]
    jvm: Vec<String>,
}

#[derive(Deserialize)]
struct FabricLibrary {
    name: String,
    url: String,
    /// SHA-1 hash if provided
    sha1: Option<String>,
    size: Option<u64>,
}

/// Convert a Fabric library entry (name + Maven repo URL) into the meta Library type.
fn fabric_lib_to_meta_lib(lib: FabricLibrary) -> Library {
    let rel = maven_to_path(&lib.name);
    let download_url = rel.as_ref().map(|path| {
        format!("{}{}", lib.url.trim_end_matches('/'), format!("/{}", path.display()).replace('\\', "/"))
    });

    let artifact = download_url.map(|url| Artifact {
        path: rel.map(|p| p.to_string_lossy().replace('\\', "/")),
        sha1: lib.sha1.unwrap_or_default(),
        size: lib.size.unwrap_or(0),
        url,
    });

    Library {
        name: lib.name,
        downloads: Some(LibraryDownloads { artifact, classifiers: None }),
        rules: None,
        natives: None,
        extract: None,
        url: Some(lib.url),
    }
}
