#![allow(dead_code)]
use serde::Deserialize;

/// Subset of launcher.config.toml needed by the CLI.
#[derive(Debug, Deserialize)]
pub struct LauncherConfig {
    pub branding: BrandingConfig,
    pub server: ServerConfig,
    pub runtime: RuntimeConfig,
    pub java: JavaConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub features: FeaturesConfig,
}

#[derive(Debug, Deserialize)]
pub struct BrandingConfig {
    pub internal_id: String,
    pub display_name: String,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub address: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

fn default_port() -> u16 { 25565 }

#[derive(Debug, Deserialize)]
pub struct RuntimeConfig {
    #[serde(default = "default_ram")]
    pub ram_default_mb: u32,
    #[serde(default = "default_concurrency")]
    pub download_concurrency: usize,
    #[serde(default = "default_timeout")]
    pub download_timeout_secs: u64,
    #[serde(default)]
    pub default_jvm_args: Vec<String>,
}

fn default_ram()         -> u32   { 4096 }
fn default_concurrency() -> usize { 8 }
fn default_timeout()     -> u64   { 120 }

#[derive(Debug, Deserialize)]
pub struct JavaConfig {
    #[serde(default = "default_java_strategy")]
    pub strategy: String,
}

fn default_java_strategy() -> String { "detect_or_download".to_string() }

#[derive(Debug, Deserialize, Default)]
pub struct AuthConfig {
    #[serde(default)]
    pub microsoft_client_id: String,
}

#[derive(Debug, Deserialize, Default)]
pub struct FeaturesConfig {
    #[serde(default = "bool_true")]
    pub quick_connect: bool,
}

fn bool_true() -> bool { true }

impl LauncherConfig {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }
}
