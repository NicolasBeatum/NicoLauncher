//! `lockfile.toml` — proyecto de manifest para el admin CLI.
//!
//! Guarda toda la configuración de generación para que no haya que repetir
//! los argumentos cada vez. El flujo normal es:
//!
//!   mc-launcher manifest init    # crea lockfile.toml de forma interactiva
//!   mc-launcher manifest update  # lee lockfile.toml y genera/actualiza manifest.json

use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

pub const LOCKFILE_NAME: &str = "lockfile.toml";

// ── Secciones ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectSection {
    /// Versión de Minecraft (ej: "1.21.1")
    pub mc_version: String,
    /// Major de Java requerido (ej: 21)
    #[serde(default = "default_java_version")]
    pub java_version: u32,
    /// Mod loader: neoforge, fabric, forge, vanilla
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loader: Option<String>,
    /// Versión del loader (None = "latest")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loader_version: Option<String>,
}

fn default_java_version() -> u32 {
    21
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PathsSection {
    /// Carpeta con los .jar de mods requeridos
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mods: Option<String>,
    /// Carpeta con los .jar de mods opcionales del servidor
    #[serde(skip_serializing_if = "Option::is_none")]
    pub optional_mods: Option<String>,
    /// Carpeta con los .zip de shaderpacks
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shaderpacks: Option<String>,
    /// Carpeta con los .zip/.jar de resourcepacks
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resourcepacks: Option<String>,
    /// Carpeta de configs a copiar a .minecraft/ (estructura espejada)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub configs: Option<String>,
    /// Archivo de salida
    #[serde(default = "default_output")]
    pub output: String,
}

fn default_output() -> String {
    "manifest.json".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HostingSection {
    /// URL base del servidor propio (ej: "https://cdn.example.com")
    /// Los mods no encontrados en Modrinth usarán `<self_hosted_url>/<filename>`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub self_hosted_url: Option<String>,
    /// Modo de aplicación para configs/shaderpacks/resourcepacks
    /// "if_missing" = solo si el jugador no lo tiene | "always" = sobreescribir
    #[serde(default = "default_apply_mode")]
    pub apply_mode: String,
}

fn default_apply_mode() -> String {
    "if_missing".into()
}

/// Metadatos adicionales para un mod opcional del servidor.
/// Se define como `[[optional_mod]]` en lockfile.toml.
/// El ID debe coincidir con el slug de Modrinth o con el ID generado desde el filename.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OptionalModOverride {
    /// ID del mod (slug de Modrinth o slug generado desde el filename)
    pub id: String,
    /// Si debe estar habilitado por defecto para los jugadores
    #[serde(default)]
    pub default_enabled: bool,
    /// Categoría de visualización (ej: "performance", "visuals")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    /// Descripción corta que verá el jugador
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// URL del icono (se usa el de Modrinth si no se especifica)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    /// IDs de mods que deben estar habilitados para que este funcione
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub depends_on: Vec<String>,
    /// IDs de mods incompatibles con este
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub conflicts_with: Vec<String>,
}

// ── Config principal ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockfileConfig {
    pub project: ProjectSection,
    #[serde(default)]
    pub paths: PathsSection,
    #[serde(default)]
    pub hosting: HostingSection,
    /// Metadatos extra para cada mod opcional (tabla TOML: `[[optional_mod]]`)
    #[serde(default, rename = "optional_mod", skip_serializing_if = "Vec::is_empty")]
    pub optional_mod_overrides: Vec<OptionalModOverride>,
}

impl LockfileConfig {
    /// Carga el lockfile desde disco.
    pub fn load(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("No se pudo leer {:?}", path))?;
        toml::from_str(&content)
            .with_context(|| format!("Error al parsear {:?}", path))
    }

    /// Guarda el lockfile en disco.
    pub fn save(&self, path: &Path) -> Result<()> {
        let content =
            toml::to_string_pretty(self).context("Error al serializar lockfile.toml")?;
        std::fs::write(path, content)
            .with_context(|| format!("Error al escribir {:?}", path))
    }

    // ── Helpers de rutas ───────────────────────────────────────────────────────

    pub fn mods_dir(&self) -> Option<PathBuf> {
        self.paths.mods.as_deref().map(PathBuf::from)
    }
    pub fn optional_mods_dir(&self) -> Option<PathBuf> {
        self.paths.optional_mods.as_deref().map(PathBuf::from)
    }
    pub fn shaderpacks_dir(&self) -> Option<PathBuf> {
        self.paths.shaderpacks.as_deref().map(PathBuf::from)
    }
    pub fn resourcepacks_dir(&self) -> Option<PathBuf> {
        self.paths.resourcepacks.as_deref().map(PathBuf::from)
    }
    pub fn configs_dir(&self) -> Option<PathBuf> {
        self.paths.configs.as_deref().map(PathBuf::from)
    }
    pub fn output_path(&self) -> PathBuf {
        PathBuf::from(&self.paths.output)
    }

    /// Devuelve el override para un mod opcional dado su ID, si existe.
    #[allow(dead_code)]
    pub fn optional_override(&self, id: &str) -> Option<&OptionalModOverride> {
        self.optional_mod_overrides.iter().find(|o| o.id == id)
    }
}
