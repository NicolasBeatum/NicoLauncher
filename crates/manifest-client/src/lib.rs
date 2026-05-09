//! Server manifest schema, providers (HTTP/Git/File), signature verification, sync diff.
//! Implemented in Phase 3.

use async_trait::async_trait;
use launcher_core::Result;
use serde::{Deserialize, Serialize};

use launcher_mods::ModSource;

/// The server-side manifest describing the modpack.
#[derive(Debug, Serialize, Deserialize)]
pub struct ServerManifest {
    pub schema_version: u32,
    pub manifest_version: String,
    pub released_at: chrono::DateTime<chrono::Utc>,

    pub minecraft: MinecraftSpec,
    pub loader: LoaderSpec,

    pub required_mods: Vec<ModEntry>,
    pub optional_mods: Vec<OptionalModEntry>,
    pub config_overrides: Vec<ConfigOverride>,
    pub removed_files: Vec<String>,
    pub additional_jvm_args: Vec<String>,
    pub announcement: Option<Announcement>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MinecraftSpec {
    pub version: String,
    pub java_version: u32,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LoaderSpec {
    #[serde(rename = "type")]
    pub loader_type: launcher_core::LoaderType,
    pub version: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ModEntry {
    pub id: String,
    pub name: String,
    pub source: ModSource,
    pub sha512: String,
    pub size: u64,
    pub filename: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct OptionalModEntry {
    #[serde(flatten)]
    pub base: ModEntry,
    #[serde(default)]
    pub default_enabled: bool,
    pub category: Option<String>,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub conflicts_with: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigOverride {
    pub path: String,
    pub url: String,
    pub sha512: String,
    /// "always" | "if_missing"
    pub apply: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Announcement {
    pub id: String,
    pub title: String,
    pub body_md: String,
    pub show_until: Option<chrono::DateTime<chrono::Utc>>,
}

#[async_trait]
pub trait ManifestProvider: Send + Sync {
    async fn fetch(&self) -> Result<ServerManifest>;
    fn name(&self) -> &str;
}
