use std::collections::HashMap;
use serde::{Deserialize, Serialize};

// ── Version manifest ─────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct VersionManifestV2 {
    pub latest: LatestVersions,
    pub versions: Vec<VersionEntry>,
}

#[derive(Debug, Deserialize)]
pub struct LatestVersions {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct VersionEntry {
    pub id: String,
    #[serde(rename = "type")]
    pub version_type: VersionType,
    pub url: String,
    pub sha1: String,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VersionType {
    Release,
    Snapshot,
    OldBeta,
    OldAlpha,
}

// ── Version JSON ─────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct VersionJson {
    pub id: String,
    #[serde(rename = "mainClass")]
    pub main_class: String,
    /// Modern format (1.13+)
    pub arguments: Option<Arguments>,
    /// Legacy format (pre-1.13)
    #[serde(rename = "minecraftArguments")]
    pub minecraft_arguments: Option<String>,
    #[serde(rename = "assetIndex")]
    pub asset_index: AssetIndexInfo,
    pub assets: String,
    pub downloads: VersionDownloads,
    pub libraries: Vec<Library>,
    #[serde(rename = "javaVersion")]
    pub java_version: Option<JavaVersionReq>,
    #[serde(rename = "type")]
    pub version_type: VersionType,
    #[serde(rename = "minimumLauncherVersion")]
    pub minimum_launcher_version: Option<u32>,
}

#[derive(Debug, Deserialize)]
pub struct Arguments {
    #[serde(default)]
    pub game: Vec<Argument>,
    #[serde(default)]
    pub jvm: Vec<Argument>,
}

/// An argument element can be a plain string or a conditional object.
#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Conditional {
        rules: Vec<Rule>,
        value: ArgumentValue,
    },
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
pub enum ArgumentValue {
    Single(String),
    Multiple(Vec<String>),
}

impl ArgumentValue {
    pub fn as_vec(&self) -> Vec<String> {
        match self {
            ArgumentValue::Single(s)   => vec![s.clone()],
            ArgumentValue::Multiple(v) => v.clone(),
        }
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct AssetIndexInfo {
    pub id: String,
    pub sha1: String,
    pub size: u64,
    #[serde(rename = "totalSize")]
    pub total_size: Option<u64>,
    pub url: String,
}

#[derive(Debug, Deserialize)]
pub struct VersionDownloads {
    pub client: Artifact,
    #[serde(rename = "client_mappings")]
    pub client_mappings: Option<Artifact>,
    pub server: Option<Artifact>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Artifact {
    pub sha1: String,
    pub size: u64,
    pub url: String,
    /// Relative path within the libraries dir (only present on library artifacts)
    pub path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct JavaVersionReq {
    pub component: String,
    #[serde(rename = "majorVersion")]
    pub major_version: u32,
}

// ── Libraries ────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
pub struct Library {
    pub name: String,
    pub downloads: Option<LibraryDownloads>,
    pub rules: Option<Vec<Rule>>,
    pub natives: Option<HashMap<String, String>>,
    pub extract: Option<ExtractConfig>,
    /// Old-format base URL (pre-1.13 some libs lacked `downloads`)
    pub url: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct LibraryDownloads {
    pub artifact: Option<Artifact>,
    pub classifiers: Option<HashMap<String, Artifact>>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ExtractConfig {
    #[serde(default)]
    pub exclude: Vec<String>,
}

// ── Rules ────────────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize, Clone)]
pub struct Rule {
    pub action: RuleAction,
    pub os: Option<OsRule>,
    pub features: Option<HashMap<String, bool>>,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Deserialize, Clone)]
pub struct OsRule {
    pub name: Option<String>,
    pub version: Option<String>,
    pub arch: Option<String>,
}

// ── Asset index ───────────────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct AssetObjects {
    pub objects: HashMap<String, AssetObject>,
}

#[derive(Debug, Deserialize)]
pub struct AssetObject {
    pub hash: String,
    pub size: u64,
}
