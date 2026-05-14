//! Comandos de generación y actualización del manifest.
//!
//! Dos flujos:
//! - `manifest generate` (legacy): un único comando con todos los argumentos.
//! - `manifest init` + `manifest update` (recomendado): workflow con lockfile.toml.

use std::{
    collections::HashMap,
    io::{self, Write as _},
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use chrono::Utc;
use launcher_mods::ModSource;
use serde::{Deserialize, Serialize};
use sha2::Digest as _;
use tracing::{info, warn};

use crate::lockfile::{LockfileConfig, OptionalModOverride, LOCKFILE_NAME};

// ── Tipos de salida (compatibles con ServerManifest del launcher) ─────────────

#[derive(Debug, Serialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub manifest_version: String,
    pub released_at: String,
    pub minecraft: MinecraftSpec,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub loader: Option<LoaderSpec>,
    pub required_mods: Vec<ModEntryOut>,
    pub optional_mods: Vec<OptionalModEntryOut>,
    pub config_overrides: Vec<ConfigOverrideOut>,
    pub removed_files: Vec<String>,
    pub additional_jvm_args: Vec<String>,
    pub announcement: Option<serde_json::Value>,
}

#[derive(Debug, Serialize)]
pub struct MinecraftSpec {
    pub version: String,
    pub java_version: u32,
}

#[derive(Debug, Serialize)]
pub struct LoaderSpec {
    #[serde(rename = "type")]
    pub loader_type: String,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModEntryOut {
    pub id: String,
    pub name: String,
    pub source: ModSource,
    pub sha512: String,
    pub size: u64,
    pub filename: String,
}

/// Entrada de mod opcional — extiende ModEntryOut con metadatos de visualización.
#[derive(Debug, Serialize)]
pub struct OptionalModEntryOut {
    // ModEntry fields (flattened)
    pub id: String,
    pub name: String,
    pub source: ModSource,
    pub sha512: String,
    pub size: u64,
    pub filename: String,
    // Optional-mod extras
    pub default_enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub conflicts_with: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigOverrideOut {
    pub path: String,
    pub url: String,
    pub sha512: String,
    pub apply: String,
}

// ── Tipos de la API de Modrinth ───────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct ModrinthVersionFile {
    url: String,
    filename: String,
    primary: bool,
}

#[derive(Debug, Deserialize)]
struct ModrinthVersion {
    project_id: String,
    id: String,
    version_number: String,
    files: Vec<ModrinthVersionFile>,
}

#[derive(Debug, Deserialize)]
struct ModrinthProject {
    id: String,
    slug: String,
    title: String,
    #[serde(default)]
    icon_url: Option<String>,
    #[serde(default)]
    description: Option<String>,
}

// ── Info de un archivo escaneado ──────────────────────────────────────────────

struct FileInfo {
    path: PathBuf,
    sha1: String,
    sha512: String,
    size: u64,
}

// ── Helpers de I/O interactiva ────────────────────────────────────────────────

/// Imprime una pregunta y lee la respuesta del usuario.
/// Si el usuario pulsa Enter sin escribir nada, devuelve `default`.
fn prompt(question: &str, default: Option<&str>) -> String {
    match default {
        Some(d) if !d.is_empty() => print!("  {} [{}]: ", question, d),
        _ => print!("  {}: ", question),
    }
    io::stdout().flush().unwrap();
    let mut buf = String::new();
    io::stdin().read_line(&mut buf).unwrap();
    let trimmed = buf.trim().to_string();
    if trimmed.is_empty() {
        default.unwrap_or("").to_string()
    } else {
        trimmed
    }
}

/// Pregunta sí/no. Devuelve `true` si el usuario responde s/S/y/Y o pulsa Enter
/// cuando `default_yes` es true.
fn prompt_yn(question: &str, default_yes: bool) -> bool {
    let hint = if default_yes { "S/n" } else { "s/N" };
    let answer = prompt(question, Some(hint));
    if answer == hint {
        return default_yes; // solo Enter
    }
    let lower = answer.to_lowercase();
    if lower.is_empty() {
        default_yes
    } else {
        lower.starts_with('s') || lower.starts_with('y')
    }
}

/// Lee un path opcional: si el usuario deja vacío devuelve None, si escribe algo devuelve Some.
fn prompt_optional_path(question: &str, default: Option<&str>) -> Option<String> {
    let val = prompt(question, default);
    if val.is_empty() { None } else { Some(val) }
}

// ── Cálculo de versión ────────────────────────────────────────────────────────

/// Genera la siguiente versión de manifest basada en la fecha de hoy.
/// Formato: `YYYY.MM.DD-N` donde N se incrementa si ya existe una versión de hoy.
fn next_manifest_version(existing: Option<&str>) -> String {
    let today = Utc::now().format("%Y.%m.%d").to_string();
    let counter = match existing {
        Some(v) => {
            if let Some((date, n)) = v.rsplit_once('-') {
                if date == today {
                    n.parse::<u32>().unwrap_or(0) + 1
                } else {
                    1
                }
            } else {
                1
            }
        }
        None => 1,
    };
    format!("{}-{}", today, counter)
}

// ═══════════════════════════════════════════════════════════════════════════════
// COMANDO: manifest init
// ═══════════════════════════════════════════════════════════════════════════════

/// Asistente interactivo que crea `lockfile.toml` en el directorio actual.
pub async fn run_init(lockfile_path: &Path, force: bool) -> Result<()> {
    if lockfile_path.exists() && !force {
        println!();
        println!("  ⚠️  {} ya existe.", LOCKFILE_NAME);
        if !prompt_yn("¿Sobreescribir?", false) {
            println!("  Cancelado.");
            return Ok(());
        }
    }

    println!();
    println!("🚀  Asistente de configuración — {}", LOCKFILE_NAME);
    println!("────────────────────────────────────────────────────────────");
    println!("  Presiona Enter para aceptar el valor entre corchetes.");
    println!();

    // ── Proyecto ───────────────────────────────────────────────────────────────
    println!("── Proyecto ──────────────────────────────────────────────────");
    let mc_version = prompt("Versión de Minecraft (ej: 1.21.1)", Some("1.21.1"));
    let java_version: u32 = {
        let raw = prompt("Versión de Java requerida", Some("21"));
        raw.parse().unwrap_or(21)
    };
    let loader_raw = prompt("Mod loader (neoforge / fabric / forge / vanilla)", Some("neoforge"));
    let loader: Option<String> = if loader_raw.is_empty() || loader_raw == "vanilla" {
        None
    } else {
        Some(loader_raw)
    };
    let loader_version: Option<String> = if loader.is_some() {
        prompt_optional_path("Versión del loader (vacío = latest)", None)
    } else {
        None
    };

    // ── Carpetas ───────────────────────────────────────────────────────────────
    println!();
    println!("── Carpetas ──────────────────────────────────────────────────");
    let mods = prompt_optional_path("Carpeta de mods requeridos (vacío = ninguna)", Some("mods"));
    let optional_mods = if prompt_yn("¿Incluir mods opcionales del servidor?", false) {
        prompt_optional_path("  Carpeta de mods opcionales", Some("optional-mods"))
    } else {
        None
    };
    let shaderpacks = if prompt_yn("¿Incluir shaderpacks?", false) {
        prompt_optional_path("  Carpeta de shaderpacks", Some("shaderpacks"))
    } else {
        None
    };
    let resourcepacks = if prompt_yn("¿Incluir resourcepacks?", false) {
        prompt_optional_path("  Carpeta de resourcepacks", Some("resourcepacks"))
    } else {
        None
    };
    let configs = if prompt_yn("¿Incluir configs (.minecraft/)?", false) {
        prompt_optional_path("  Carpeta de configs", Some("configs"))
    } else {
        None
    };
    let output = prompt("Archivo de salida", Some("manifest.json"));

    // ── Distribución ───────────────────────────────────────────────────────────
    println!();
    println!("── Distribución ──────────────────────────────────────────────");
    let self_hosted_url =
        prompt_optional_path("URL base del servidor (ej: https://cdn.example.com)", None);
    let apply_mode = prompt(
        "Modo de aplicación para configs (if_missing / always)",
        Some("if_missing"),
    );

    // ── Construir y guardar ────────────────────────────────────────────────────
    use crate::lockfile::{HostingSection, PathsSection, ProjectSection};

    let config = LockfileConfig {
        project: ProjectSection {
            mc_version,
            java_version,
            loader,
            loader_version,
        },
        paths: PathsSection {
            mods,
            optional_mods,
            shaderpacks,
            resourcepacks,
            configs,
            output,
        },
        hosting: HostingSection {
            self_hosted_url,
            apply_mode,
        },
        optional_mod_overrides: vec![],
    };

    config.save(lockfile_path)?;

    println!();
    println!("✅  {} creado.", LOCKFILE_NAME);
    println!();
    println!("   Próximos pasos:");
    println!("     1. Coloca tus mods en la carpeta configurada");
    if config.optional_mods_dir().is_some() {
        println!(
            "     2. Añade metadatos opcionales editando [[optional_mod]] en {}",
            LOCKFILE_NAME
        );
        println!("     3. Ejecuta: mc-launcher manifest update");
    } else {
        println!("     2. Ejecuta: mc-launcher manifest update");
    }
    println!(
        "     {} Firma el manifest: mc-launcher sign sign {}",
        if config.optional_mods_dir().is_some() { "4." } else { "3." },
        config.output_path().display()
    );

    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════════════
// COMANDO: manifest update
// ═══════════════════════════════════════════════════════════════════════════════

pub async fn run_update(lockfile_path: &Path, yes: bool) -> Result<()> {
    // ── 1. Leer lockfile ───────────────────────────────────────────────────────
    if !lockfile_path.exists() {
        bail!(
            "No se encontró {}.\n  Ejecuta primero: mc-launcher manifest init",
            LOCKFILE_NAME
        );
    }

    let cfg = LockfileConfig::load(lockfile_path)?;

    println!();
    println!("📋  {}", LOCKFILE_NAME);
    {
        let loader_str = match (&cfg.project.loader, &cfg.project.loader_version) {
            (Some(l), Some(v)) => format!(" · {} {}", l, v),
            (Some(l), None) => format!(" · {} (latest)", l),
            _ => String::new(),
        };
        println!(
            "   MC {}{}  →  {}",
            cfg.project.mc_version,
            loader_str,
            cfg.paths.output
        );
    }

    // ── 2. Leer manifest anterior (si existe) ──────────────────────────────────
    let output_path = cfg.output_path();
    let existing_manifest: Option<ExistingManifest> = read_existing_manifest(&output_path);

    if let Some(ref em) = existing_manifest {
        println!(
            "   Manifest anterior: versión {} ({} mods requeridos, {} opcionales)",
            em.manifest_version,
            em.required_mods.len(),
            em.optional_mods.len(),
        );
    } else {
        println!("   Sin manifest anterior — se generará uno nuevo.");
    }

    // ── 3. Escanear archivos ───────────────────────────────────────────────────
    println!();
    println!("🔍  Escaneando archivos...");

    let mod_files = match cfg.mods_dir() {
        Some(d) => scan_dir(&d, &["jar"], false)
            .with_context(|| format!("Escaneando mods en {:?}", d))?,
        None => vec![],
    };

    let optional_files = match cfg.optional_mods_dir() {
        Some(d) => scan_dir(&d, &["jar"], false)
            .with_context(|| format!("Escaneando mods opcionales en {:?}", d))?,
        None => vec![],
    };

    let shaderpack_files = match cfg.shaderpacks_dir() {
        Some(d) => scan_dir(&d, &["zip"], false)
            .with_context(|| format!("Escaneando shaderpacks en {:?}", d))?,
        None => vec![],
    };

    let resourcepack_files = match cfg.resourcepacks_dir() {
        Some(d) => scan_dir(&d, &["zip", "jar"], false)
            .with_context(|| format!("Escaneando resourcepacks en {:?}", d))?,
        None => vec![],
    };

    let config_files: Vec<(FileInfo, String)> = match cfg.configs_dir() {
        Some(d) => {
            let raw = scan_dir(&d, &[], true)
                .with_context(|| format!("Escaneando configs en {:?}", d))?;
            raw.into_iter()
                .map(|fi| {
                    let rel = fi
                        .path
                        .strip_prefix(&d)
                        .unwrap_or(&fi.path)
                        .to_string_lossy()
                        .replace('\\', "/");
                    (fi, rel)
                })
                .collect()
        }
        None => vec![],
    };

    if !mod_files.is_empty() {
        println!("   Mods requeridos:  {} archivos .jar", mod_files.len());
    }
    if !optional_files.is_empty() {
        println!("   Mods opcionales:  {} archivos .jar", optional_files.len());
    }
    if !shaderpack_files.is_empty() {
        println!("   Shaderpacks:      {} archivos", shaderpack_files.len());
    }
    if !resourcepack_files.is_empty() {
        println!("   Resourcepacks:    {} archivos", resourcepack_files.len());
    }
    if !config_files.is_empty() {
        println!("   Configs:          {} archivos", config_files.len());
    }

    let total = mod_files.len()
        + optional_files.len()
        + shaderpack_files.len()
        + resourcepack_files.len()
        + config_files.len();
    if total == 0 {
        bail!("No se encontraron archivos. Comprueba las rutas en lockfile.toml.");
    }

    // ── 4. Calcular diffs y decidir qué consultar a Modrinth ─────────────────
    let http = reqwest::Client::builder()
        .user_agent("mc-launcher-template/admin-cli")
        .build()?;

    // Mods requeridos
    let (req_diff, req_unchanged) = diff_mods(
        &mod_files,
        existing_manifest
            .as_ref()
            .map(|em| em.required_mods.as_slice())
            .unwrap_or(&[]),
    );
    // Mods opcionales
    let (opt_diff, opt_unchanged) = diff_mods(
        &optional_files,
        existing_manifest
            .as_ref()
            .map(|em| em.optional_mods.as_slice())
            .unwrap_or(&[]),
    );

    // Archivos nuevos/modificados que necesitan consulta a Modrinth
    let files_needing_lookup: Vec<&FileInfo> = req_diff
        .new_or_changed
        .iter()
        .copied()
        .chain(opt_diff.new_or_changed.iter().copied())
        .chain(shaderpack_files.iter())
        .chain(resourcepack_files.iter())
        .collect();

    let (version_map, project_map) = if !files_needing_lookup.is_empty() {
        let hashes: Vec<&str> = files_needing_lookup
            .iter()
            .map(|f| f.sha1.as_str())
            .collect();
        println!();
        println!(
            "🌐  Consultando Modrinth ({} archivos nuevos/modificados)...",
            hashes.len()
        );
        let vm = modrinth_version_files(&http, &hashes).await?;
        let pids: Vec<&str> = vm
            .values()
            .map(|v| v.project_id.as_str())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        let pm = modrinth_projects(&http, &pids).await?;
        (vm, pm)
    } else {
        println!();
        println!("✨  Todos los archivos están en caché (sin consulta a Modrinth).");
        (HashMap::new(), HashMap::new())
    };

    // ── 5. Construir entradas finales ─────────────────────────────────────────
    println!();
    let required_mods = build_required_diff_display(
        &req_diff,
        &req_unchanged,
        &version_map,
        &project_map,
        cfg.hosting.self_hosted_url.as_deref(),
    );

    let optional_mods = build_optional_diff_display(
        &opt_diff,
        &opt_unchanged,
        &version_map,
        &project_map,
        cfg.hosting.self_hosted_url.as_deref(),
        &cfg.optional_mod_overrides,
    );

    let mut config_overrides: Vec<ConfigOverrideOut> = Vec::new();

    if !shaderpack_files.is_empty() {
        println!();
        println!("── Shaderpacks ────────────────────────────────────────────────");
        config_overrides.extend(build_config_entries(
            &shaderpack_files,
            "shaderpacks",
            &version_map,
            &project_map,
            cfg.hosting.self_hosted_url.as_deref(),
            &cfg.hosting.apply_mode,
        ));
    }

    if !resourcepack_files.is_empty() {
        println!();
        println!("── Resourcepacks ──────────────────────────────────────────────");
        config_overrides.extend(build_config_entries(
            &resourcepack_files,
            "resourcepacks",
            &version_map,
            &project_map,
            cfg.hosting.self_hosted_url.as_deref(),
            &cfg.hosting.apply_mode,
        ));
    }

    if !config_files.is_empty() {
        println!();
        println!("── Configs ────────────────────────────────────────────────────");
        for (fi, rel_path) in &config_files {
            let url = build_self_hosted_url(cfg.hosting.self_hosted_url.as_deref(), rel_path);
            println!("  📄  {}", rel_path);
            config_overrides.push(ConfigOverrideOut {
                path: rel_path.clone(),
                url,
                sha512: fi.sha512.clone(),
                apply: cfg.hosting.apply_mode.clone(),
            });
        }
    }

    // ── 6. Resumen del diff ────────────────────────────────────────────────────
    println!();
    let has_changes = !req_diff.removed.is_empty()
        || !req_diff.new_or_changed.is_empty()
        || !opt_diff.removed.is_empty()
        || !opt_diff.new_or_changed.is_empty()
        || existing_manifest.is_none();

    if !has_changes {
        println!("  ✨  Sin cambios respecto al manifest anterior.");
        if !yes && !prompt_yn("¿Regenerar manifest igualmente?", false) {
            println!("  Cancelado.");
            return Ok(());
        }
    } else if !yes {
        let q = format!("¿Escribir {}?", output_path.display());
        if !prompt_yn(&q, true) {
            println!("  Cancelado.");
            return Ok(());
        }
    }

    // ── 7. Escribir manifest ───────────────────────────────────────────────────
    let existing_version = existing_manifest
        .as_ref()
        .map(|em| em.manifest_version.as_str());
    let manifest_version = next_manifest_version(existing_version);

    let manifest = Manifest {
        schema_version: 1,
        manifest_version: manifest_version.clone(),
        released_at: Utc::now().to_rfc3339(),
        minecraft: MinecraftSpec {
            version: cfg.project.mc_version.clone(),
            java_version: cfg.project.java_version,
        },
        loader: cfg.project.loader.as_ref().map(|l| LoaderSpec {
            loader_type: l.clone(),
            version: cfg
                .project
                .loader_version
                .clone()
                .unwrap_or_else(|| "latest".into()),
        }),
        required_mods: required_mods.clone(),
        optional_mods,
        config_overrides,
        removed_files: vec![],
        additional_jvm_args: vec![],
        announcement: None,
    };

    let json = serde_json::to_string_pretty(&manifest)?;
    std::fs::write(&output_path, &json)
        .with_context(|| format!("Escribiendo {:?}", output_path))?;

    // ── 8. Resumen final ───────────────────────────────────────────────────────
    println!();
    println!(
        "✅  {} generado  (versión {}, {} mods requeridos{})",
        output_path.display(),
        manifest_version,
        required_mods.len(),
        if manifest.optional_mods.is_empty() {
            String::new()
        } else {
            format!(", {} opcionales", manifest.optional_mods.len())
        }
    );

    let modrinth_count = required_mods
        .iter()
        .filter(|m| matches!(&m.source, ModSource::Modrinth { .. }))
        .count();
    let self_count = required_mods.len() - modrinth_count;
    if self_count > 0 {
        println!(
            "   Modrinth: {}  ·  self-hosted: {}",
            modrinth_count, self_count
        );
    }

    let has_placeholders = required_mods
        .iter()
        .any(|m| matches!(&m.source, ModSource::SelfHosted { url } if url.contains("TU_SERVIDOR")))
        || manifest
            .config_overrides
            .iter()
            .any(|c| c.url.contains("TU_SERVIDOR"));

    if has_placeholders {
        println!();
        println!("  ⚠️  Algunos archivos no están en Modrinth.");
        println!(
            "   Edita las URLs en {} o añade `self_hosted_url` en [hosting].",
            output_path.display()
        );
    }

    println!();
    println!(
        "   Siguiente paso: mc-launcher sign sign {}",
        output_path.display()
    );

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Diff helpers
// ─────────────────────────────────────────────────────────────────────────────

struct ModDiff<'a> {
    /// Archivos en el scan actual que no están en el manifest anterior (por sha512)
    new_or_changed: Vec<&'a FileInfo>,
    /// Entradas del manifest anterior que ya no están en el scan
    removed: Vec<ModEntryOut>,
}

/// Separa los archivos escaneados en "sin cambios" y "nuevos/modificados"
/// comparando sha512 con el manifest anterior.
fn diff_mods<'a>(
    scanned: &'a [FileInfo],
    existing: &[ModEntryOut],
) -> (ModDiff<'a>, Vec<ModEntryOut>) {
    let existing_by_sha: HashMap<&str, &ModEntryOut> = existing
        .iter()
        .map(|m| (m.sha512.as_str(), m))
        .collect();

    let scanned_shas: std::collections::HashSet<&str> =
        scanned.iter().map(|f| f.sha512.as_str()).collect();

    let mut new_or_changed = Vec::new();
    let mut unchanged = Vec::new();

    for fi in scanned {
        if let Some(existing) = existing_by_sha.get(fi.sha512.as_str()) {
            unchanged.push((*existing).clone());
        } else {
            new_or_changed.push(fi);
        }
    }

    let removed: Vec<ModEntryOut> = existing
        .iter()
        .filter(|m| !scanned_shas.contains(m.sha512.as_str()))
        .cloned()
        .collect();

    (ModDiff { new_or_changed, removed }, unchanged)
}

/// Construye la lista final de mods requeridos y muestra el diff en consola.
fn build_required_diff_display(
    diff: &ModDiff,
    unchanged: &[ModEntryOut],
    version_map: &HashMap<String, ModrinthVersion>,
    project_map: &HashMap<String, ModrinthProject>,
    self_hosted: Option<&str>,
) -> Vec<ModEntryOut> {
    if unchanged.is_empty() && diff.new_or_changed.is_empty() && diff.removed.is_empty() {
        return vec![];
    }

    println!("── Mods requeridos ────────────────────────────────────────────");

    // Construir mapa project_id → removed (para detectar actualizaciones)
    let removed_by_project: HashMap<String, &ModEntryOut> = diff
        .removed
        .iter()
        .filter_map(|m| {
            if let ModSource::Modrinth { project_id, .. } = &m.source {
                Some((project_id.clone(), m))
            } else {
                None
            }
        })
        .collect();

    let mut entries: Vec<ModEntryOut> = Vec::new();

    // Sin cambios
    for m in unchanged {
        println!("  ✅  {}  (sin cambios)", m.name);
        entries.push(m.clone());
    }

    // Nuevos / actualizados
    for fi in &diff.new_or_changed {
        let filename = fi.path.file_name().unwrap_or_default().to_string_lossy();
        let entry = resolve_mod_entry(fi, &filename, version_map, project_map, self_hosted);

        // ¿Es una actualización de un mod existente?
        let update_label = if let ModSource::Modrinth { project_id, .. } = &entry.source {
            removed_by_project.get(project_id.as_str()).map(|old| {
                // Extraer versión del nombre del archivo anterior (best-effort)
                old.name.clone()
            })
        } else {
            None
        };

        if let Some(old_name) = update_label {
            // Determinar versión anterior y nueva para el display
            let old_entry = diff.removed.iter().find(|m| m.name == old_name);
            let old_ver = version_from_modrinth_source(&old_entry);
            let new_ver = if let ModSource::Modrinth { version_id, .. } = &entry.source {
                version_map
                    .get(fi.sha1.as_str())
                    .map(|v| v.version_number.as_str())
                    .unwrap_or(version_id.as_str())
                    .to_string()
            } else {
                String::new()
            };
            if !new_ver.is_empty() && !old_ver.is_empty() {
                println!("  🔄  {}  {} → {}  (actualizado)", entry.name, old_ver, new_ver);
            } else {
                println!("  🔄  {}  (actualizado)", entry.name);
            }
        } else if let ModSource::Modrinth { .. } = &entry.source {
            let ver = version_map
                .get(fi.sha1.as_str())
                .map(|v| v.version_number.as_str())
                .unwrap_or("");
            println!("  ➕  {}{}  [Modrinth]", entry.name, if ver.is_empty() { String::new() } else { format!(" {}", ver) });
        } else {
            println!("  ➕  {}  [self-hosted]", entry.name);
        }

        entries.push(entry);
    }

    // Eliminados (que no son actualizaciones)
    let entries_project_ids: std::collections::HashSet<String> = entries
        .iter()
        .filter_map(|e| {
            if let ModSource::Modrinth { project_id, .. } = &e.source {
                Some(project_id.clone())
            } else {
                None
            }
        })
        .collect();

    for m in &diff.removed {
        let is_updated = if let ModSource::Modrinth { project_id, .. } = &m.source {
            entries_project_ids.contains(project_id)
        } else {
            false
        };
        if !is_updated {
            println!("  ❌  {}  (eliminado)", m.name);
        }
    }

    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

/// Construye la lista final de mods opcionales y muestra el diff en consola.
fn build_optional_diff_display(
    diff: &ModDiff,
    unchanged: &[ModEntryOut],
    version_map: &HashMap<String, ModrinthVersion>,
    project_map: &HashMap<String, ModrinthProject>,
    self_hosted: Option<&str>,
    overrides: &[OptionalModOverride],
) -> Vec<OptionalModEntryOut> {
    if unchanged.is_empty() && diff.new_or_changed.is_empty() && diff.removed.is_empty() {
        return vec![];
    }

    println!();
    println!("── Mods opcionales ────────────────────────────────────────────");

    let mut entries: Vec<OptionalModEntryOut> = Vec::new();

    // Sin cambios: reconstruir como OptionalModEntryOut desde el override
    for m in unchanged {
        let ov = overrides.iter().find(|o| o.id == m.id);
        let entry = apply_optional_override(
            OptionalModEntryOut {
                id: m.id.clone(),
                name: m.name.clone(),
                source: m.source.clone(),
                sha512: m.sha512.clone(),
                size: m.size,
                filename: m.filename.clone(),
                default_enabled: false,
                category: None,
                description: None,
                icon_url: None,
                depends_on: vec![],
                conflicts_with: vec![],
            },
            ov,
        );
        println!("  ✅  {}  (sin cambios)", entry.name);
        entries.push(entry);
    }

    // Nuevos / modificados
    for fi in &diff.new_or_changed {
        let filename = fi.path.file_name().unwrap_or_default().to_string_lossy();
        let base = resolve_mod_entry(fi, &filename, version_map, project_map, self_hosted);
        let ov = overrides.iter().find(|o| o.id == base.id);

        // Enriquecer con datos de Modrinth si disponibles
        let modrinth_proj = if let ModSource::Modrinth { project_id, .. } = &base.source {
            project_map.get(project_id.as_str())
        } else {
            None
        };

        let ver = version_map
            .get(fi.sha1.as_str())
            .map(|v| v.version_number.as_str())
            .unwrap_or("");

        let entry = apply_optional_override(
            OptionalModEntryOut {
                id: base.id.clone(),
                name: base.name.clone(),
                source: base.source.clone(),
                sha512: base.sha512.clone(),
                size: base.size,
                filename: base.filename.clone(),
                default_enabled: false,
                category: None,
                description: modrinth_proj.and_then(|p| p.description.clone()),
                icon_url: modrinth_proj.and_then(|p| p.icon_url.clone()),
                depends_on: vec![],
                conflicts_with: vec![],
            },
            ov,
        );

        let source_label = if matches!(&entry.source, ModSource::Modrinth { .. }) {
            "[Modrinth]"
        } else {
            "[self-hosted]"
        };
        let enabled_label = if entry.default_enabled {
            " 🟢 habilitado por defecto"
        } else {
            ""
        };
        println!(
            "  ➕  {}{}  {}{}",
            entry.name,
            if ver.is_empty() { String::new() } else { format!(" {}", ver) },
            source_label,
            enabled_label
        );
        entries.push(entry);
    }

    for m in &diff.removed {
        println!("  ❌  {}  (eliminado)", m.name);
    }

    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

fn apply_optional_override(
    mut entry: OptionalModEntryOut,
    ov: Option<&OptionalModOverride>,
) -> OptionalModEntryOut {
    if let Some(o) = ov {
        entry.default_enabled = o.default_enabled;
        if o.category.is_some() {
            entry.category = o.category.clone();
        }
        if o.description.is_some() {
            entry.description = o.description.clone();
        }
        if o.icon_url.is_some() {
            entry.icon_url = o.icon_url.clone();
        }
        if !o.depends_on.is_empty() {
            entry.depends_on = o.depends_on.clone();
        }
        if !o.conflicts_with.is_empty() {
            entry.conflicts_with = o.conflicts_with.clone();
        }
    }
    entry
}

fn version_from_modrinth_source(m: &Option<&ModEntryOut>) -> String {
    // Best-effort: intentamos extraer versión del filename
    m.map(|m| {
        m.filename
            .trim_end_matches(".jar")
            .rsplit('-')
            .next()
            .unwrap_or("")
            .to_string()
    })
    .unwrap_or_default()
}

// ─────────────────────────────────────────────────────────────────────────────
// Lectura del manifest anterior
// ─────────────────────────────────────────────────────────────────────────────

struct ExistingManifest {
    manifest_version: String,
    required_mods: Vec<ModEntryOut>,
    optional_mods: Vec<ModEntryOut>,
}

fn read_existing_manifest(path: &Path) -> Option<ExistingManifest> {
    let content = std::fs::read_to_string(path).ok()?;
    let v: serde_json::Value = serde_json::from_str(&content).ok()?;

    let manifest_version = v
        .get("manifest_version")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let required_mods: Vec<ModEntryOut> = v
        .get("required_mods")
        .and_then(|m| serde_json::from_value(m.clone()).ok())
        .unwrap_or_default();

    // optional_mods are stored with extra fields; we only need base ModEntryOut fields
    let optional_mods: Vec<ModEntryOut> = v
        .get("optional_mods")
        .and_then(|m| m.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|e| serde_json::from_value(e.clone()).ok())
                .collect()
        })
        .unwrap_or_default();

    Some(ExistingManifest { manifest_version, required_mods, optional_mods })
}

// ═══════════════════════════════════════════════════════════════════════════════
// COMANDO: manifest generate (legacy, sin lockfile)
// ═══════════════════════════════════════════════════════════════════════════════

#[allow(clippy::too_many_arguments)]
pub async fn run(
    mods_dir: Option<&Path>,
    shaderpacks_dir: Option<&Path>,
    resourcepacks_dir: Option<&Path>,
    configs_dir: Option<&Path>,
    mc_version: &str,
    java_version: u32,
    loader: Option<&str>,
    loader_version: Option<&str>,
    output: &Path,
    self_hosted_url: Option<&str>,
    apply_mode: &str,
) -> Result<()> {
    let mut mod_files: Vec<FileInfo> = Vec::new();
    let mut shaderpack_files: Vec<FileInfo> = Vec::new();
    let mut resourcepack_files: Vec<FileInfo> = Vec::new();
    let mut config_files: Vec<(FileInfo, String)> = Vec::new();

    if let Some(dir) = mods_dir {
        mod_files = scan_dir(dir, &["jar"], false)?;
        info!("Mods:          {} archivos .jar", mod_files.len());
    }
    if let Some(dir) = shaderpacks_dir {
        shaderpack_files = scan_dir(dir, &["zip"], false)?;
        info!("Shaderpacks:   {} archivos .zip", shaderpack_files.len());
    }
    if let Some(dir) = resourcepacks_dir {
        resourcepack_files = scan_dir(dir, &["zip", "jar"], false)?;
        info!("Resourcepacks: {} archivos", resourcepack_files.len());
    }
    if let Some(dir) = configs_dir {
        let raw = scan_dir(dir, &[], true)?;
        info!("Configs:       {} archivos", raw.len());
        config_files = raw
            .into_iter()
            .map(|fi| {
                let rel = fi
                    .path
                    .strip_prefix(dir)
                    .unwrap_or(&fi.path)
                    .to_string_lossy()
                    .replace('\\', "/");
                (fi, rel)
            })
            .collect();
    }

    let total = mod_files.len()
        + shaderpack_files.len()
        + resourcepack_files.len()
        + config_files.len();
    if total == 0 {
        bail!("No se encontraron archivos en las carpetas especificadas.");
    }

    let http = reqwest::Client::builder()
        .user_agent("mc-launcher-template/admin-cli")
        .build()?;

    let modrinth_files: Vec<&FileInfo> = mod_files
        .iter()
        .chain(shaderpack_files.iter())
        .chain(resourcepack_files.iter())
        .collect();

    let all_sha1s: Vec<&str> = modrinth_files.iter().map(|f| f.sha1.as_str()).collect();

    let (version_map, project_map) = if !all_sha1s.is_empty() {
        info!("Consultando Modrinth para {} hashes...", all_sha1s.len());
        let vm = modrinth_version_files(&http, &all_sha1s).await?;
        let pids: Vec<&str> = vm
            .values()
            .map(|v| v.project_id.as_str())
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();
        let pm = modrinth_projects(&http, &pids).await?;
        (vm, pm)
    } else {
        (HashMap::new(), HashMap::new())
    };

    println!();
    if !mod_files.is_empty() {
        println!("── Mods ───────────────────────────────────────────────────────");
    }
    let required_mods = build_mod_entries(&mod_files, &version_map, &project_map, self_hosted_url);

    let mut config_overrides: Vec<ConfigOverrideOut> = Vec::new();

    if !shaderpack_files.is_empty() {
        println!();
        println!("── Shaderpacks ────────────────────────────────────────────────");
        config_overrides.extend(build_config_entries(
            &shaderpack_files,
            "shaderpacks",
            &version_map,
            &project_map,
            self_hosted_url,
            apply_mode,
        ));
    }
    if !resourcepack_files.is_empty() {
        println!();
        println!("── Resourcepacks ──────────────────────────────────────────────");
        config_overrides.extend(build_config_entries(
            &resourcepack_files,
            "resourcepacks",
            &version_map,
            &project_map,
            self_hosted_url,
            apply_mode,
        ));
    }
    if !config_files.is_empty() {
        println!();
        println!("── Configs ────────────────────────────────────────────────────");
        for (fi, rel_path) in &config_files {
            let url = build_self_hosted_url(self_hosted_url, rel_path);
            println!("  📄  {} → {}", rel_path, url);
            config_overrides.push(ConfigOverrideOut {
                path: rel_path.clone(),
                url,
                sha512: fi.sha512.clone(),
                apply: apply_mode.to_string(),
            });
        }
    }

    let manifest_version = next_manifest_version(None);
    let manifest = Manifest {
        schema_version: 1,
        manifest_version: manifest_version.clone(),
        released_at: Utc::now().to_rfc3339(),
        minecraft: MinecraftSpec {
            version: mc_version.to_string(),
            java_version,
        },
        loader: loader.map(|l| LoaderSpec {
            loader_type: l.to_string(),
            version: loader_version.unwrap_or("latest").to_string(),
        }),
        required_mods: required_mods.clone(),
        optional_mods: vec![],
        config_overrides,
        removed_files: vec![],
        additional_jvm_args: vec![],
        announcement: None,
    };

    let json = serde_json::to_string_pretty(&manifest)?;
    std::fs::write(output, &json)
        .with_context(|| format!("Escribiendo manifest en {:?}", output))?;

    let modrinth_mods = manifest
        .required_mods
        .iter()
        .filter(|m| matches!(&m.source, ModSource::Modrinth { .. }))
        .count();
    let self_mods = manifest.required_mods.len() - modrinth_mods;

    println!();
    println!("📄 Manifest generado en {:?}", output);
    if !mod_files.is_empty() {
        println!(
            "   Mods:    {} (Modrinth: {}, self-hosted: {})",
            manifest.required_mods.len(),
            modrinth_mods,
            self_mods
        );
    }
    let cfg_count = manifest.config_overrides.len();
    if cfg_count > 0 {
        println!("   Configs: {}", cfg_count);
    }

    let has_placeholders = manifest.required_mods.iter().any(|m| {
        matches!(&m.source, ModSource::SelfHosted { url } if url.contains("TU_SERVIDOR"))
    }) || manifest
        .config_overrides
        .iter()
        .any(|c| c.url.contains("TU_SERVIDOR"));

    if has_placeholders {
        println!();
        println!("⚠️  Algunos archivos no se encontraron en Modrinth.");
        println!("   Edita las URLs 'TU_SERVIDOR' en el manifest o usa --self-hosted-url.");
    }

    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers compartidos de construcción de entradas
// ─────────────────────────────────────────────────────────────────────────────

/// Resuelve un único archivo a una entrada de mod (con Modrinth o self-hosted).
fn resolve_mod_entry(
    fi: &FileInfo,
    filename: &str,
    version_map: &HashMap<String, ModrinthVersion>,
    project_map: &HashMap<String, ModrinthProject>,
    self_hosted: Option<&str>,
) -> ModEntryOut {
    if let Some(ver) = version_map.get(fi.sha1.as_str()) {
        let primary = ver
            .files
            .iter()
            .find(|f| f.primary)
            .or_else(|| ver.files.first());
        if let Some(f) = primary {
            let project = project_map.get(ver.project_id.as_str());
            let (id, name) = match project {
                Some(p) => (p.slug.clone(), p.title.clone()),
                None => (ver.project_id.clone(), filename.to_string()),
            };
            return ModEntryOut {
                id,
                name,
                source: ModSource::Modrinth {
                    project_id: ver.project_id.clone(),
                    version_id: ver.id.clone(),
                    download_url: Some(f.url.clone()),
                },
                sha512: fi.sha512.clone(),
                size: fi.size,
                filename: f.filename.clone(),
            };
        }
    }

    // Self-hosted fallback
    let url = build_self_hosted_url(self_hosted, filename);
    let id = slug_from_filename(filename);
    ModEntryOut {
        id: id.clone(),
        name: filename.to_string(),
        source: ModSource::SelfHosted { url },
        sha512: fi.sha512.clone(),
        size: fi.size,
        filename: filename.to_string(),
    }
}

fn build_mod_entries(
    files: &[FileInfo],
    version_map: &HashMap<String, ModrinthVersion>,
    project_map: &HashMap<String, ModrinthProject>,
    self_hosted: Option<&str>,
) -> Vec<ModEntryOut> {
    let mut entries: Vec<ModEntryOut> = Vec::new();

    for fi in files {
        let filename = fi.path.file_name().unwrap_or_default().to_string_lossy();
        let entry = resolve_mod_entry(fi, &filename, version_map, project_map, self_hosted);

        if let ModSource::Modrinth { .. } = &entry.source {
            let ver = version_map
                .get(fi.sha1.as_str())
                .map(|v| v.version_number.as_str())
                .unwrap_or("");
            println!("  ✅  {} — {}", entry.name, ver);
        } else {
            warn!(
                "  ⚠️   {} — no encontrado en Modrinth",
                filename
            );
        }
        entries.push(entry);
    }

    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

fn build_config_entries(
    files: &[FileInfo],
    subfolder: &str,
    version_map: &HashMap<String, ModrinthVersion>,
    project_map: &HashMap<String, ModrinthProject>,
    self_hosted: Option<&str>,
    apply_mode: &str,
) -> Vec<ConfigOverrideOut> {
    let mut entries: Vec<ConfigOverrideOut> = Vec::new();

    for fi in files {
        let filename = fi.path.file_name().unwrap_or_default().to_string_lossy();
        let mc_path = format!("{subfolder}/{filename}");

        let url = if let Some(ver) = version_map.get(fi.sha1.as_str()) {
            let primary = ver
                .files
                .iter()
                .find(|f| f.primary)
                .or_else(|| ver.files.first());
            if let Some(f) = primary {
                let project = project_map.get(ver.project_id.as_str());
                let name = project.map(|p| p.title.as_str()).unwrap_or(&filename);
                println!("  ✅  {} — {}", name, ver.version_number);
                f.url.clone()
            } else {
                build_self_hosted_url(self_hosted, &filename)
            }
        } else {
            let url = build_self_hosted_url(self_hosted, &filename);
            println!("  ⚠️   {} — no encontrado en Modrinth → {}", filename, url);
            url
        };

        entries.push(ConfigOverrideOut {
            path: mc_path,
            url,
            sha512: fi.sha512.clone(),
            apply: apply_mode.to_string(),
        });
    }

    entries
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers: filesystem
// ─────────────────────────────────────────────────────────────────────────────

fn scan_dir(dir: &Path, extensions: &[&str], recursive: bool) -> Result<Vec<FileInfo>> {
    if !dir.is_dir() {
        bail!("{:?} no es una carpeta válida", dir);
    }
    let mut files: Vec<FileInfo> = Vec::new();
    scan_dir_inner(dir, extensions, recursive, &mut files)?;
    files.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(files)
}

fn scan_dir_inner(
    dir: &Path,
    extensions: &[&str],
    recursive: bool,
    out: &mut Vec<FileInfo>,
) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            if recursive {
                scan_dir_inner(&path, extensions, true, out)?;
            }
            continue;
        }

        if !extensions.is_empty() {
            let ext = path
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_lowercase();
            if !extensions.contains(&ext.as_str()) {
                continue;
            }
        }

        let bytes =
            std::fs::read(&path).with_context(|| format!("Leyendo {:?}", path))?;
        let size = bytes.len() as u64;

        let mut h1 = sha1::Sha1::new();
        h1.update(&bytes);
        let sha1 = hex::encode(h1.finalize());

        let mut h512 = sha2::Sha512::new();
        h512.update(&bytes);
        let sha512 = hex::encode(h512.finalize());

        out.push(FileInfo { path, sha1, sha512, size });
    }
    Ok(())
}

fn build_self_hosted_url(base: Option<&str>, file_or_rel: &str) -> String {
    match base {
        Some(b) => format!("{}/{}", b.trim_end_matches('/'), file_or_rel),
        None => format!("https://TU_SERVIDOR/files/{file_or_rel}"),
    }
}

fn slug_from_filename(filename: &str) -> String {
    filename
        .trim_end_matches(".jar")
        .trim_end_matches(".zip")
        .to_lowercase()
        .replace(' ', "-")
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers: Modrinth API
// ─────────────────────────────────────────────────────────────────────────────

async fn modrinth_version_files<'a>(
    http: &reqwest::Client,
    hashes: &[&'a str],
) -> Result<HashMap<String, ModrinthVersion>> {
    if hashes.is_empty() {
        return Ok(HashMap::new());
    }

    #[derive(Serialize)]
    struct Body<'a> {
        hashes: &'a [&'a str],
        algorithm: &'static str,
    }

    let resp = http
        .post("https://api.modrinth.com/v2/version_files")
        .json(&Body { hashes, algorithm: "sha1" })
        .send()
        .await?;

    if !resp.status().is_success() {
        bail!("Modrinth /version_files respondió {}", resp.status());
    }

    Ok(resp.json::<HashMap<String, ModrinthVersion>>().await?)
}

async fn modrinth_projects<'a>(
    http: &reqwest::Client,
    ids: &[&'a str],
) -> Result<HashMap<String, ModrinthProject>> {
    if ids.is_empty() {
        return Ok(HashMap::new());
    }

    let ids_json = serde_json::to_string(ids)?;
    let resp = http
        .get("https://api.modrinth.com/v2/projects")
        .query(&[("ids", ids_json.as_str())])
        .send()
        .await?;

    if !resp.status().is_success() {
        bail!("Modrinth /projects respondió {}", resp.status());
    }

    let list: Vec<ModrinthProject> = resp.json().await?;
    Ok(list.into_iter().map(|p| (p.id.clone(), p)).collect())
}
