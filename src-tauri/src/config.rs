use serde::Deserialize;
use std::path::Path;

#[derive(Debug, Clone, Deserialize)]
pub struct LauncherConfig {
    pub branding: BrandingConfig,
    pub server: ServerConfig,
    pub runtime: RuntimeConfig,
    pub java: JavaConfig,
    #[serde(default)]
    pub auth: AuthConfig,
    #[serde(default)]
    pub features: FeaturesConfig,
    #[serde(default)]
    pub updater: UpdaterConfig,
    #[serde(default)]
    pub telemetry: TelemetryConfig,
    /// Lista de instancias/servidores del launcher.
    /// Si está vacía se sintetiza una instancia desde [server] (backward compat).
    #[serde(default)]
    pub instances: Vec<InstanceConfig>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct TelemetryConfig {
    /// Si false (default), no se envía ningún dato.
    #[serde(default)]
    pub enabled: bool,
    /// URL del endpoint que recibe eventos (POST JSON).
    #[serde(default)]
    pub endpoint: String,
    /// Enviar evento cuando el juego se lanza.
    #[serde(default)]
    pub report_launches: bool,
    /// Enviar evento cuando el launcher detecta un crash.
    #[serde(default)]
    pub report_crashes: bool,
}

/// Una instancia = un servidor/modpack con sus propios mods y manifest.
#[derive(Debug, Clone, Deserialize)]
pub struct InstanceConfig {
    /// Identificador único (se usa para las subcarpetas en disco). No cambiar tras publicar.
    pub id: String,
    /// Nombre visible en la UI.
    pub display_name: String,
    /// Descripción corta (opcional).
    #[serde(default)]
    pub description: String,
    /// URL del manifest.json de esta instancia.
    #[serde(default)]
    pub manifest_url: String,
    /// "http" | "file" — igual que server.manifest_provider.
    #[serde(default = "default_manifest_provider")]
    pub manifest_provider: String,
    /// Clave pública Ed25519 para verificar el manifest (vacío = sin verificación).
    #[serde(default)]
    pub manifest_public_key: String,
    /// Color de acento para esta instancia (hex). Si vacío usa el color primario del branding.
    #[serde(default)]
    pub color: String,
    /// Icono en assets/ (opcional).
    #[serde(default)]
    pub icon: String,
    /// Dirección IP/dominio del servidor (para el botón Conectar).
    #[serde(default)]
    pub server_address: String,
    #[serde(default = "default_port")]
    pub server_port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct BrandingConfig {
    pub internal_id: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub address: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_manifest_provider")]
    pub manifest_provider: String,
    #[serde(default)]
    pub manifest_url: String,
    #[serde(default)]
    pub manifest_public_key: String,
    /// URL de un instances-registry.json remoto.
    /// Si se define, el launcher descarga la lista de instancias desde ahí en lugar
    /// de (o además de) las definidas en [[instances]] del config.
    #[serde(default)]
    pub instances_url: String,
}

fn default_port() -> u16 { 25565 }
fn default_manifest_provider() -> String { "http".into() }

#[derive(Debug, Clone, Deserialize)]
pub struct RuntimeConfig {
    #[serde(default = "default_ram")]
    pub ram_default_mb: u32,
    #[serde(default = "default_ram_min")]
    pub ram_min_mb: u32,
    #[serde(default = "default_ram_max")]
    pub ram_max_mb: u32,
    #[serde(default = "default_concurrency")]
    pub download_concurrency: usize,
    #[serde(default = "default_timeout")]
    pub download_timeout_secs: u64,
    #[serde(default)]
    pub default_jvm_args: Vec<String>,
    /// Fallback MC version used when no server manifest is loaded (dev/testing).
    #[serde(default = "default_mc_version")]
    pub fallback_mc_version: String,
}

fn default_mc_version() -> String { "1.21.1".into() }

fn default_ram()         -> u32   { 4096 }
fn default_ram_min()     -> u32   { 2048 }
fn default_ram_max()     -> u32   { 16384 }
fn default_concurrency() -> usize { 8 }
fn default_timeout()     -> u64   { 120 }

#[derive(Debug, Clone, Deserialize)]
pub struct JavaConfig {
    #[serde(default = "default_java_strategy")]
    pub strategy: String,
}

fn default_java_strategy() -> String { "detect_or_download".into() }

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AuthConfig {
    #[serde(default)]
    pub microsoft_client_id: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct UpdaterConfig {
    /// Set to true and fill endpoint + pubkey to enable auto-updates.
    #[serde(default)]
    pub enabled: bool,
    /// URL to the update manifest endpoint.
    /// Supports `{{target}}`, `{{arch}}`, `{{current_version}}` placeholders.
    #[serde(default)]
    pub release_url: String,
    /// Content of updater.key.pub (the full file, newlines included).
    #[serde(default)]
    pub release_public_key: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct FeaturesConfig {
    #[serde(default = "bool_true")]
    pub quick_connect: bool,
    #[serde(default = "bool_true")]
    pub allow_ram_config: bool,
    #[serde(default)]
    pub allow_jvm_args_edit: bool,
    #[serde(default = "bool_true")]
    pub allow_java_path_override: bool,
    #[serde(default = "bool_true")]
    pub show_news: bool,
}

fn bool_true() -> bool { true }

impl LauncherConfig {
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        Ok(toml::from_str(&content)?)
    }

    /// Devuelve las instancias efectivas.
    /// Si no hay [[instances]] definidas, sintetiza una desde [server] (backward compat).
    pub fn effective_instances(&self) -> Vec<InstanceConfig> {
        if !self.instances.is_empty() {
            return self.instances.clone();
        }
        // Backward compat: una sola instancia desde [server]
        vec![InstanceConfig {
            id: "default".into(),
            display_name: self.branding.display_name.clone(),
            description: String::new(),
            manifest_url: self.server.manifest_url.clone(),
            manifest_provider: self.server.manifest_provider.clone(),
            manifest_public_key: self.server.manifest_public_key.clone(),
            color: String::new(),
            icon: String::new(),
            server_address: self.server.address.clone(),
            server_port: self.server.port,
        }]
    }

    /// Busca una instancia por id.
    pub fn find_instance(&self, id: &str) -> Option<InstanceConfig> {
        self.effective_instances().into_iter().find(|i| i.id == id)
    }
}
