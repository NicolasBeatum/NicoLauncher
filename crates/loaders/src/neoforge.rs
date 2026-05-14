use std::collections::HashMap;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use tracing::{debug, info, warn};

use launcher_core::{Error, LauncherPaths, Result, maven_to_path, progress::ProgressReporter};
use launcher_downloader::{DownloadJob, Downloader};
use launcher_meta::types::{Arguments, Library};

use crate::merge::LoaderProfile;

const NEOFORGE_VERSIONS_API: &str =
    "https://maven.neoforged.net/api/maven/versions/releases/net%2Fneoforged%2Fneoforge";
const NEOFORGE_MAVEN: &str = "https://maven.neoforged.net/releases";

// ── Provider ─────────────────────────────────────────────────────────────────

pub struct NeoForgeProvider {
    client: reqwest::Client,
}

impl NeoForgeProvider {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("mc-launcher-template/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(Self { client })
    }

    /// List NeoForge versions compatible with a given Minecraft version.
    /// NeoForge uses `<major-1>.<minor>.<patch>` prefixing:
    /// MC 1.21.1 → prefix "21.1.", MC 1.20.4 → prefix "20.4."
    pub async fn list_versions(&self, mc_version: &str) -> Result<Vec<String>> {
        let prefix = mc_version_to_nf_prefix(mc_version)?;

        let resp: NeoMavenVersionsResponse = self
            .client
            .get(NEOFORGE_VERSIONS_API)
            .send()
            .await
            .map_err(|e| Error::Other(e.to_string()))?
            .json()
            .await
            .map_err(|e| Error::Other(e.to_string()))?;

        // Versions come newest-first from the API; filter by prefix
        let filtered: Vec<String> = resp
            .versions
            .into_iter()
            .filter(|v| v.starts_with(&prefix))
            .collect();

        Ok(filtered)
    }

    /// Return the newest stable NeoForge loader version for a given MC version.
    pub async fn recommended_version(&self, mc_version: &str) -> Result<String> {
        self.list_versions(mc_version)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| Error::Other(format!("No NeoForge version found for MC {mc_version}")))
    }

    /// Download and install NeoForge:
    ///   1. Download the installer JAR.
    ///   2. Extract `version.json` (loader profile) and `install_profile.json` (processors).
    ///   3. Download all required libraries.
    ///   4. Run client-side processors (patch the Minecraft client).
    ///
    /// Returns a `LoaderProfile` ready to merge into the Mojang version JSON.
    pub async fn install(
        &self,
        mc_version: &str,
        loader_version: &str,
        mc_client_jar: &Path,
        java_binary: &Path,
        paths: &LauncherPaths,
        log: &(dyn Fn(&str) + Send + Sync),
    ) -> Result<LoaderProfile> {
        // ── Step 1: Download installer JAR ────────────────────────────────────
        tokio::fs::create_dir_all(&paths.loader_installers).await?;
        let installer_jar = paths
            .loader_installers
            .join(format!("neoforge-{loader_version}-installer.jar"));

        if !installer_jar.exists() {
            let url = format!(
                "{NEOFORGE_MAVEN}/net/neoforged/neoforge/{loader_version}/neoforge-{loader_version}-installer.jar"
            );
            log(&format!("Descargando NeoForge installer {loader_version}…"));
            let bytes = self
                .client
                .get(&url)
                .send()
                .await
                .map_err(|e| Error::Other(e.to_string()))?
                .error_for_status()
                .map_err(|e| Error::Other(e.to_string()))?
                .bytes()
                .await
                .map_err(|e| Error::Other(e.to_string()))?;
            tokio::fs::write(&installer_jar, &bytes).await?;
        } else {
            debug!("NeoForge installer already cached: {:?}", installer_jar);
        }

        // ── Step 2: Extract JSON files from installer ZIP ─────────────────────
        log("Extrayendo install_profile.json y version.json…");
        let jar_path = installer_jar.clone();
        let (install_profile_bytes, version_json_bytes) =
            tokio::task::spawn_blocking(move || {
                extract_two_files(&jar_path, "install_profile.json", "version.json")
            })
            .await
            .map_err(|e| Error::Other(e.to_string()))??;

        let install_profile: InstallProfile =
            serde_json::from_slice(&install_profile_bytes).map_err(|e| {
                Error::Other(format!("install_profile.json parse error: {e}"))
            })?;

        let nf_version: NeoForgeVersionJson =
            serde_json::from_slice(&version_json_bytes).map_err(|e| {
                Error::Other(format!("NeoForge version.json parse error: {e}"))
            })?;

        info!(
            "NeoForge {loader_version} / MC {mc_version}: {} libs, {} processors",
            nf_version.libraries.len(),
            install_profile.processors.len()
        );

        // ── Step 3: Build LoaderProfile from version.json ─────────────────────
        let loader_profile = LoaderProfile {
            main_class: nf_version.main_class.clone(),
            libraries: nf_version.libraries.clone(),
            arguments: nf_version.arguments.clone(),
        };

        // ── Step 3b: Extract embedded maven jars from installer JAR ──────────
        // The installer bundles bootstrap dependencies (gson, asm, etc.) in maven/.
        // These must be extracted to libraries_dir before running processors.
        log("Extrayendo dependencias embebidas del installer…");
        let jar_for_maven = installer_jar.clone();
        let libs_for_maven = paths.libraries.clone();
        tokio::task::spawn_blocking(move || {
            extract_embedded_maven(&jar_for_maven, &libs_for_maven)
        })
        .await
        .map_err(|e| Error::Other(e.to_string()))??;

        // ── Step 4: Download all libraries ────────────────────────────────────
        log("Descargando librerías de NeoForge…");
        let mut all_jobs: Vec<DownloadJob> = Vec::new();

        for lib in install_profile.libraries.iter().chain(nf_version.libraries.iter()) {
            if let Some(job) = library_to_download_job(lib, &paths.libraries) {
                all_jobs.push(job);
            }
        }

        if !all_jobs.is_empty() {
            let dl = Downloader::new(8, 60, ProgressReporter::noop())?;
            dl.download_many(all_jobs).await?;
        }

        // ── Step 5: Run client-side processors ───────────────────────────────
        let work_dir = paths
            .loader_installers
            .join(format!("neoforge-{loader_version}-work"));
        tokio::fs::create_dir_all(&work_dir).await?;

        let ctx = DataContext {
            data: install_profile.data.clone(),
            libraries_dir: paths.libraries.clone(),
            installer_jar: installer_jar.clone(),
            work_dir: work_dir.clone(),
            mc_client_jar: mc_client_jar.to_path_buf(),
        };

        let client_processors: Vec<&Processor> = install_profile
            .processors
            .iter()
            .filter(|p| {
                p.sides.is_empty() || p.sides.iter().any(|s| s == "client")
            })
            .collect();

        let total = client_processors.len();
        for (i, proc) in client_processors.iter().enumerate() {
            log(&format!(
                "NeoForge processor {}/{total}: {}…",
                i + 1,
                short_coord(&proc.jar)
            ));
            run_processor(proc, &ctx, java_binary, &paths.libraries)
                .await
                .map_err(|e| Error::Other(format!("Processor {} failed: {e}", proc.jar)))?;
        }

        log(&format!("NeoForge {loader_version} instalado ✓"));
        Ok(loader_profile)
    }

    /// Build `DownloadJob`s for all NeoForge libraries that need downloading.
    pub fn library_download_jobs(profile: &LoaderProfile, libraries_dir: &Path) -> Vec<DownloadJob> {
        profile
            .libraries
            .iter()
            .filter_map(|lib| library_to_download_job(lib, libraries_dir))
            .collect()
    }
}

impl Default for NeoForgeProvider {
    fn default() -> Self {
        Self::new().expect("Failed to build NeoForge HTTP client")
    }
}

// ── Data context for variable substitution ───────────────────────────────────

struct DataContext {
    data: HashMap<String, DataValue>,
    libraries_dir: PathBuf,
    installer_jar: PathBuf,
    work_dir: PathBuf,
    mc_client_jar: PathBuf,
}

impl DataContext {
    /// Resolve a processor argument:
    ///   - `{KEY}` → data variable
    ///   - `[maven:coord]` → absolute library path
    ///   - special: `{MINECRAFT_JAR}`, `{SIDE}`, `{LIBRARIES_DIR}`, `{INSTALLER}`
    ///   - everything else → literal
    fn resolve(&self, arg: &str) -> Result<String> {
        if let Some(inner) = arg.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
            return match inner {
                "SIDE"          => Ok("client".to_string()),
                "MINECRAFT_JAR" => Ok(self.mc_client_jar.to_string_lossy().into_owned()),
                "LIBRARIES_DIR" => Ok(self.libraries_dir.to_string_lossy().into_owned()),
                "INSTALLER"     => Ok(self.installer_jar.to_string_lossy().into_owned()),
                key => {
                    let val = self.data.get(key)
                        .ok_or_else(|| Error::Other(format!("Unknown data variable {{{key}}}")))?;
                    self.resolve_data_value(&val.client)
                }
            };
        }
        if arg.starts_with('[') && arg.ends_with(']') {
            return self.resolve_maven_coord(&arg[1..arg.len() - 1]);
        }
        Ok(arg.to_string())
    }

    fn resolve_data_value(&self, val: &str) -> Result<String> {
        if val.starts_with('[') && val.ends_with(']') {
            return self.resolve_maven_coord(&val[1..val.len() - 1]);
        }
        if val.starts_with('"') && val.ends_with('"') {
            return Ok(val[1..val.len() - 1].to_string());
        }
        // Path inside installer JAR — extract to work_dir.
        // ZIP entries never have a leading '/', but the data map values often do.
        let stripped = val.trim_start_matches('/');
        let safe = stripped.replace('/', std::path::MAIN_SEPARATOR_STR);
        let dest = self.work_dir.join(&safe);
        if !dest.exists() {
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| Error::Other(format!("Cannot create dir {parent:?}: {e}")))?;
            }
            extract_file_from_zip_sync(&self.installer_jar, stripped, &dest)?;
        }
        Ok(dest.to_string_lossy().into_owned())
    }

    fn resolve_maven_coord(&self, coord: &str) -> Result<String> {
        let rel = maven_to_path_ext(coord)
            .ok_or_else(|| Error::Other(format!("Cannot resolve maven coord: {coord}")))?;
        Ok(self.libraries_dir.join(rel).to_string_lossy().into_owned())
    }
}

// ── Processor execution ───────────────────────────────────────────────────────

async fn run_processor(
    proc: &Processor,
    ctx: &DataContext,
    java_binary: &Path,
    libraries_dir: &Path,
) -> Result<()> {
    // Check if all outputs are already valid → skip
    if !proc.outputs.is_empty() {
        let mut all_valid = true;
        for (out_key, out_sha_key) in &proc.outputs {
            let path_str = match ctx.resolve(out_key) {
                Ok(s) => s,
                Err(_) => { all_valid = false; break; }
            };
            let path = Path::new(&path_str);
            if !path.exists() {
                all_valid = false;
                break;
            }
            if !out_sha_key.is_empty() {
                if let Ok(expected_sha) = ctx.resolve(out_sha_key) {
                    if !expected_sha.is_empty() {
                        let actual = launcher_core::hash::sha1_file(path)
                            .await
                            .unwrap_or_default();
                        if actual != expected_sha {
                            all_valid = false;
                            break;
                        }
                    }
                }
            }
        }
        if all_valid {
            debug!("Skipping processor {} (outputs valid)", proc.jar);
            return Ok(());
        }
    }

    // Resolve processor JAR path and find its Main-Class
    let jar_coord = &proc.jar;
    let jar_rel = maven_to_path(jar_coord)
        .ok_or_else(|| Error::Other(format!("Cannot resolve processor jar: {jar_coord}")))?;
    let jar_path = libraries_dir.join(&jar_rel);

    let main_class = {
        let p = jar_path.clone();
        tokio::task::spawn_blocking(move || read_main_class_from_jar(&p))
            .await
            .map_err(|e| Error::Other(e.to_string()))??
    };

    // Build classpath — use maven_to_path_ext to handle @jar / @txt / etc. suffixes
    let sep = if cfg!(windows) { ';' } else { ':' };
    let mut cp_parts = vec![jar_path.to_string_lossy().into_owned()];
    for dep in &proc.classpath {
        if let Some(rel) = maven_to_path_ext(dep) {
            let full = libraries_dir.join(&rel);
            if !full.exists() {
                warn!("Classpath dep missing on disk: {:?}", full);
            }
            cp_parts.push(full.to_string_lossy().into_owned());
        } else {
            warn!("Cannot resolve classpath dep: {dep}");
        }
    }
    let classpath: String = cp_parts.join(&sep.to_string());

    // Resolve args
    let mut resolved_args: Vec<String> = Vec::with_capacity(proc.args.len());
    for arg in &proc.args {
        resolved_args.push(ctx.resolve(arg)?);
    }

    debug!("[neoforge] Running: {} {}", main_class, resolved_args.join(" "));

    let output = tokio::process::Command::new(java_binary)
        .arg("-cp")
        .arg(&classpath)
        .arg(&main_class)
        .args(&resolved_args)
        .output()
        .await
        .map_err(|e| Error::Other(format!("Failed to spawn processor: {e}")))?;

    if !output.stdout.is_empty() {
        debug!("[neoforge/stdout] {}", String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        debug!("[neoforge/stderr] {}", String::from_utf8_lossy(&output.stderr));
    }

    if !output.status.success() {
        return Err(Error::Other(format!(
            "Processor {main_class} exited with code {:?}",
            output.status.code()
        )));
    }

    Ok(())
}

// ── ZIP helpers ───────────────────────────────────────────────────────────────

/// Extract exactly two named entries from a ZIP (JAR) file.
fn extract_two_files(
    zip_path: &Path,
    name_a: &str,
    name_b: &str,
) -> Result<(Vec<u8>, Vec<u8>)> {
    let file = std::fs::File::open(zip_path)
        .map_err(|e| Error::Other(format!("Cannot open installer JAR: {e}")))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| Error::Other(format!("ZIP read error: {e}")))?;

    let mut a = None;
    let mut b = None;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| Error::Other(e.to_string()))?;
        let entry_name = entry.name().to_string();
        if entry_name == name_a || entry_name == name_b {
            let mut buf = Vec::new();
            entry
                .read_to_end(&mut buf)
                .map_err(|e| Error::Other(e.to_string()))?;
            if entry_name == name_a {
                a = Some(buf);
            } else {
                b = Some(buf);
            }
        }
        if a.is_some() && b.is_some() {
            break;
        }
    }

    let a = a.ok_or_else(|| Error::Other(format!("{name_a} not found in installer JAR")))?;
    let b = b.ok_or_else(|| Error::Other(format!("{name_b} not found in installer JAR")))?;
    Ok((a, b))
}

/// Extract a single file from a ZIP at `entry_name` to `dest`.
fn extract_file_from_zip_sync(zip_path: &Path, entry_name: &str, dest: &Path) -> Result<()> {
    let file = std::fs::File::open(zip_path)
        .map_err(|e| Error::Other(format!("Cannot open {zip_path:?}: {e}")))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| Error::Other(format!("ZIP error: {e}")))?;

    let mut entry = archive
        .by_name(entry_name)
        .map_err(|_| Error::Other(format!("{entry_name} not found in {zip_path:?}")))?;

    let mut buf = Vec::new();
    entry
        .read_to_end(&mut buf)
        .map_err(|e| Error::Other(e.to_string()))?;

    std::fs::write(dest, &buf)
        .map_err(|e| Error::Other(format!("Cannot write {dest:?}: {e}")))?;
    Ok(())
}

/// Read `Main-Class` from `META-INF/MANIFEST.MF` inside a JAR file.
fn read_main_class_from_jar(jar_path: &Path) -> Result<String> {
    let file = std::fs::File::open(jar_path)
        .map_err(|e| Error::Other(format!("Cannot open JAR {jar_path:?}: {e}")))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| Error::Other(format!("ZIP error for {jar_path:?}: {e}")))?;

    let mut manifest = archive
        .by_name("META-INF/MANIFEST.MF")
        .map_err(|_| Error::Other(format!("No MANIFEST.MF in {jar_path:?}")))?;

    let mut content = String::new();
    manifest
        .read_to_string(&mut content)
        .map_err(|e| Error::Other(e.to_string()))?;

    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("Main-Class:") {
            return Ok(rest.trim().to_string());
        }
    }

    Err(Error::Other(format!("No Main-Class in {jar_path:?}")))
}

// ── Maven coordinate helpers ──────────────────────────────────────────────────

/// Extract all entries in `maven/` inside the installer JAR to `libraries_dir`.
/// NeoForge bundles bootstrap JARs (gson, asm, installertools itself, etc.) this way.
fn extract_embedded_maven(installer_path: &Path, libraries_dir: &Path) -> Result<()> {
    let file = std::fs::File::open(installer_path)
        .map_err(|e| Error::Other(format!("Cannot open installer for maven extraction: {e}")))?;
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| Error::Other(format!("ZIP error: {e}")))?;

    let prefix = "maven/";
    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| Error::Other(e.to_string()))?;
        let name = entry.name().to_string();

        if !name.starts_with(prefix) || name.ends_with('/') {
            continue;
        }

        let rel = &name[prefix.len()..];
        // Use forward-slash replacement for cross-platform paths
        let dest = libraries_dir.join(rel.replace('/', std::path::MAIN_SEPARATOR_STR));

        if dest.exists() {
            continue; // already extracted
        }

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| Error::Other(format!("Cannot create dir: {e}")))?;
        }

        let mut buf = Vec::new();
        entry
            .read_to_end(&mut buf)
            .map_err(|e| Error::Other(e.to_string()))?;
        std::fs::write(&dest, &buf)
            .map_err(|e| Error::Other(format!("Cannot write {dest:?}: {e}")))?;

        debug!("[neoforge] Extracted embedded: {rel}");
    }

    Ok(())
}

/// Like `maven_to_path` but also handles the `@ext` suffix for non-JAR artifacts.
/// e.g. `net.neoforged:neoforge:21.1.95:client-mappings@txt`
///      → `net/neoforged/neoforge/21.1.95/neoforge-21.1.95-client-mappings.txt`
fn maven_to_path_ext(coord: &str) -> Option<PathBuf> {
    // Split off @ext if present
    let (coord_no_ext, ext) = if let Some(pos) = coord.rfind('@') {
        (&coord[..pos], &coord[pos + 1..])
    } else {
        (coord, "jar")
    };

    let parts: Vec<&str> = coord_no_ext.splitn(4, ':').collect();
    let (group, artifact, version) = match parts.as_slice() {
        [g, a, v] | [g, a, v, _] => (*g, *a, *v),
        _ => return None,
    };
    let classifier = if parts.len() == 4 { Some(parts[3]) } else { None };

    let group_path = group.replace('.', "/");
    let filename = match classifier {
        Some(c) => format!("{artifact}-{version}-{c}.{ext}"),
        None    => format!("{artifact}-{version}.{ext}"),
    };
    Some(Path::new(&group_path).join(artifact).join(version).join(filename))
}

/// Convert a library entry to a DownloadJob, if it has a valid download URL.
fn library_to_download_job(lib: &Library, libraries_dir: &Path) -> Option<DownloadJob> {
    if let Some(dl) = &lib.downloads {
        if let Some(artifact) = &dl.artifact {
            if artifact.url.is_empty() {
                return None;
            }
            let dest = if let Some(path) = &artifact.path {
                libraries_dir.join(path)
            } else if let Some(rel) = maven_to_path(&lib.name) {
                libraries_dir.join(rel)
            } else {
                return None;
            };
            let mut job = DownloadJob::new(&artifact.url, dest);
            if !artifact.sha1.is_empty() {
                job = job.with_sha1(&artifact.sha1);
            }
            if artifact.size > 0 {
                job = job.with_size(artifact.size);
            }
            return Some(job);
        }
    }
    None
}

/// Convert MC version string to NeoForge prefix.
/// `1.21.1` → `21.1.`  |  `1.20.4` → `20.4.`  |  `1.20.2` → `20.2.`
fn mc_version_to_nf_prefix(mc_version: &str) -> Result<String> {
    let parts: Vec<&str> = mc_version.split('.').collect();
    match parts.as_slice() {
        [_major, minor, patch] => Ok(format!("{minor}.{patch}.")),
        [_major, minor] => Ok(format!("{minor}.0.")),
        _ => Err(Error::Other(format!(
            "Cannot derive NeoForge prefix from MC version: {mc_version}"
        ))),
    }
}

fn short_coord(coord: &str) -> &str {
    // Return just artifact:version part for logging
    let mut parts = coord.splitn(4, ':');
    let _ = parts.next(); // group
    parts.next().unwrap_or(coord)
}

// ── JSON types ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct NeoMavenVersionsResponse {
    versions: Vec<String>,
}

/// Subset of the NeoForge `version.json` that we need (inheritsFrom Mojang version).
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct NeoForgeVersionJson {
    #[serde(rename = "mainClass")]
    main_class: String,
    #[serde(default)]
    libraries: Vec<Library>,
    #[serde(default)]
    arguments: Option<Arguments>,
}

#[derive(Deserialize)]
struct InstallProfile {
    #[serde(default)]
    data: HashMap<String, DataValue>,
    #[serde(default)]
    processors: Vec<Processor>,
    #[serde(default)]
    libraries: Vec<Library>,
}

#[derive(Deserialize, Clone)]
struct DataValue {
    /// Value for the "client" side (which is what we care about).
    client: String,
    #[allow(dead_code)]
    server: String,
}

#[derive(Deserialize)]
struct Processor {
    /// Which sides this processor applies to. Empty = all sides.
    #[serde(default)]
    sides: Vec<String>,
    /// Maven coordinate of the processor's JAR.
    jar: String,
    /// Additional classpath entries (maven coordinates).
    #[serde(default)]
    classpath: Vec<String>,
    /// Arguments to pass to the processor's main class.
    #[serde(default)]
    args: Vec<String>,
    /// Expected output files and their SHA-1 hashes (for skip-if-valid check).
    #[serde(default)]
    outputs: HashMap<String, String>,
}
