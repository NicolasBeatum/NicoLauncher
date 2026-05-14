//! Forge loader support (1.17+ modern format, install_profile processors).
//! Legacy Forge (1.5–1.16) is not supported — too different and very old.

use std::collections::HashMap;
use std::io::Read as _;
use std::path::{Path, PathBuf};

use serde::Deserialize;
use tracing::{debug, info, warn};

use launcher_core::{Error, LauncherPaths, Result, maven_to_path, progress::ProgressReporter};
use launcher_downloader::{DownloadJob, Downloader};
use launcher_meta::types::{Arguments, Argument, Library};

use crate::merge::LoaderProfile;

const FORGE_PROMOTIONS: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
const FORGE_MAVEN_META: &str =
    "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml";
const FORGE_MAVEN: &str = "https://maven.minecraftforge.net";

// ── Provider ─────────────────────────────────────────────────────────────────

pub struct ForgeProvider {
    client: reqwest::Client,
}

impl ForgeProvider {
    pub fn new() -> Result<Self> {
        let client = reqwest::Client::builder()
            .user_agent(concat!("mc-launcher-template/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(Self { client })
    }

    /// List Forge versions available for a given Minecraft version.
    /// Only returns 1.17+ versions (modern installer format).
    pub async fn list_versions(&self, mc_version: &str) -> Result<Vec<String>> {
        // Fetch Maven metadata XML and parse out version strings matching mc_version
        let xml = self
            .client
            .get(FORGE_MAVEN_META)
            .send()
            .await
            .map_err(|e| Error::Other(e.to_string()))?
            .text()
            .await
            .map_err(|e| Error::Other(e.to_string()))?;

        let prefix = format!("{mc_version}-");
        let versions: Vec<String> = xml
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                // Lines look like: <version>1.21.1-47.3.0</version>
                trimmed
                    .strip_prefix("<version>")
                    .and_then(|s| s.strip_suffix("</version>"))
                    .filter(|v| v.starts_with(&prefix))
                    .map(|v| v.to_string())
            })
            .collect();

        // Return newest first (versions are oldest-first in Maven metadata)
        let mut rev = versions;
        rev.reverse();
        Ok(rev)
    }

    /// Return the "recommended" Forge version for a given MC version (from promotions API).
    /// Falls back to newest available if no promoted version found.
    pub async fn recommended_version(&self, mc_version: &str) -> Result<String> {
        // Try promotions first
        if let Ok(promos) = self.fetch_promotions().await {
            let rec_key = format!("{mc_version}-recommended");
            let lat_key = format!("{mc_version}-latest");
            if let Some(build) = promos.promos.get(&rec_key).or(promos.promos.get(&lat_key)) {
                // Promotions give just the Forge build number; full version is "{mc}-{build}"
                return Ok(format!("{mc_version}-{build}"));
            }
        }
        // Fall back to list
        self.list_versions(mc_version)
            .await?
            .into_iter()
            .next()
            .ok_or_else(|| Error::Other(format!("No Forge version found for MC {mc_version}")))
    }

    /// Download and install Forge, running install_profile processors.
    /// Returns a `LoaderProfile` ready to merge into the Mojang version JSON.
    pub async fn install(
        &self,
        mc_version: &str,
        forge_version: &str,  // full version string like "1.21.1-47.3.0"
        mc_client_jar: &Path,
        java_binary: &Path,
        paths: &LauncherPaths,
        log: &(dyn Fn(&str) + Send + Sync),
    ) -> Result<LoaderProfile> {
        // ── Step 1: Download installer JAR ────────────────────────────────────
        tokio::fs::create_dir_all(&paths.loader_installers).await?;
        let installer_jar = paths
            .loader_installers
            .join(format!("forge-{forge_version}-installer.jar"));

        if !installer_jar.exists() {
            let url = format!(
                "{FORGE_MAVEN}/net/minecraftforge/forge/{forge_version}/forge-{forge_version}-installer.jar"
            );
            log(&format!("Descargando Forge installer {forge_version}…"));
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
            debug!("Forge installer already cached: {:?}", installer_jar);
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

        let forge_version_json: ForgeVersionJson =
            serde_json::from_slice(&version_json_bytes).map_err(|e| {
                Error::Other(format!("Forge version.json parse error: {e}"))
            })?;

        info!(
            "Forge {forge_version} / MC {mc_version}: {} libs, {} processors",
            forge_version_json.libraries.len(),
            install_profile.processors.len()
        );

        // ── Step 3: Build LoaderProfile from version.json ─────────────────────
        // Forge 52.x (new ForgeBootstrap) needs two things that the version.json omits:
        //   a) The shim JAR on the classpath — it contains bootstrap-shim.list which
        //      ForgeBootstrap reads to discover the module path.
        //   b) -DlibraryDirectory passed as a JVM arg so ForgeBootstrap can resolve
        //      the relative paths in bootstrap-shim.list.
        let mut runtime_libs = forge_version_json.libraries.clone();

        // Inject the shim into the runtime classpath if it's not already there.
        let shim_coord = format!("net.minecraftforge:forge:{forge_version}:shim");
        let shim_path  = format!(
            "net/minecraftforge/forge/{forge_version}/forge-{forge_version}-shim.jar"
        );
        let shim_already_present = runtime_libs.iter().any(|l| l.name == shim_coord);
        if !shim_already_present {
            // Add as a no-download library entry (it's already on disk from install_profile).
            use launcher_meta::types::{LibraryDownloads, Artifact};
            runtime_libs.push(Library {
                name: shim_coord,
                downloads: Some(LibraryDownloads {
                    artifact: Some(Artifact {
                        path: Some(shim_path),
                        url: String::new(), // already downloaded — skip
                        sha1: String::new(),
                        size: 0,
                    }),
                    classifiers: None,
                }),
                natives: None,
                rules: None,
                extract: None,
                url: None,
            });
        }

        // Inject -DlibraryDirectory=${library_directory} into JVM args so
        // ForgeBootstrap can resolve bootstrap-shim.list paths at launch time.
        let mut merged_args = forge_version_json.arguments.clone().unwrap_or(Arguments {
            jvm: Vec::new(),
            game: Vec::new(),
        });
        let lib_dir_arg = "-DlibraryDirectory=${library_directory}".to_string();
        if !merged_args.jvm.iter().any(|a| matches!(a, Argument::Plain(s) if s.contains("libraryDirectory"))) {
            merged_args.jvm.push(Argument::Plain(lib_dir_arg));
        }
        // Also inject preferIPv6 (Forge 52.x expects this from the shim launch)
        let ipv6_arg = "-Djava.net.preferIPv6Addresses=system".to_string();
        if !merged_args.jvm.iter().any(|a| matches!(a, Argument::Plain(s) if s.contains("preferIPv6"))) {
            merged_args.jvm.push(Argument::Plain(ipv6_arg));
        }

        let loader_profile = LoaderProfile {
            main_class: forge_version_json.main_class.clone(),
            libraries: runtime_libs,
            arguments: Some(merged_args),
        };

        // ── Step 4: Download all libraries ────────────────────────────────────
        log("Descargando librerías de Forge…");
        let mut all_jobs: Vec<DownloadJob> = Vec::new();

        for lib in install_profile
            .libraries
            .iter()
            .chain(forge_version_json.libraries.iter())
        {
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
            .join(format!("forge-{forge_version}-work"));
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
                "Forge processor {}/{total}: {}…",
                i + 1,
                short_coord(&proc.jar)
            ));
            run_processor(proc, &ctx, java_binary, &paths.libraries)
                .await
                .map_err(|e| Error::Other(format!("Processor {} failed: {e}", proc.jar)))?;
        }

        log(&format!("Forge {forge_version} instalado ✓"));
        Ok(loader_profile)
    }

    /// Build `DownloadJob`s for all Forge libraries.
    pub fn library_download_jobs(profile: &LoaderProfile, libraries_dir: &Path) -> Vec<DownloadJob> {
        profile
            .libraries
            .iter()
            .filter_map(|lib| library_to_download_job(lib, libraries_dir))
            .collect()
    }

    async fn fetch_promotions(&self) -> Result<ForgePromotions> {
        self.client
            .get(FORGE_PROMOTIONS)
            .send()
            .await
            .map_err(|e| Error::Other(e.to_string()))?
            .json()
            .await
            .map_err(|e| Error::Other(e.to_string()))
    }
}

impl Default for ForgeProvider {
    fn default() -> Self {
        Self::new().expect("Failed to build Forge HTTP client")
    }
}

// ── Data context for variable substitution ───────────────────────────────────
// (Identical logic to NeoForge — Forge uses the same install_profile format)

struct DataContext {
    data: HashMap<String, DataValue>,
    libraries_dir: PathBuf,
    installer_jar: PathBuf,
    work_dir: PathBuf,
    mc_client_jar: PathBuf,
}

impl DataContext {
    fn resolve(&self, arg: &str) -> Result<String> {
        if let Some(inner) = arg.strip_prefix('{').and_then(|s| s.strip_suffix('}')) {
            return match inner {
                "SIDE"          => Ok("client".to_string()),
                "MINECRAFT_JAR" => Ok(self.mc_client_jar.to_string_lossy().into_owned()),
                "LIBRARIES_DIR" => Ok(self.libraries_dir.to_string_lossy().into_owned()),
                "INSTALLER"     => Ok(self.installer_jar.to_string_lossy().into_owned()),
                key => {
                    let val = self.data.get(key)
                        .ok_or_else(|| Error::Other(format!("Unknown data var {{{key}}}")))?;
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
        // ZIP entries never have a leading '/', but data map values often do.
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
    // Check if outputs already valid → skip
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
            debug!("Skipping Forge processor {} (outputs valid)", proc.jar);
            return Ok(());
        }
    }

    let jar_rel = maven_to_path(&proc.jar)
        .ok_or_else(|| Error::Other(format!("Cannot resolve processor jar: {}", proc.jar)))?;
    let jar_path = libraries_dir.join(&jar_rel);

    let main_class = {
        let p = jar_path.clone();
        tokio::task::spawn_blocking(move || read_main_class_from_jar(&p))
            .await
            .map_err(|e| Error::Other(e.to_string()))??
    };

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

    let mut resolved_args: Vec<String> = Vec::with_capacity(proc.args.len());
    for arg in &proc.args {
        resolved_args.push(ctx.resolve(arg)?);
    }

    debug!("[forge] Running: {} {}", main_class, resolved_args.join(" "));

    let output = tokio::process::Command::new(java_binary)
        .arg("-cp")
        .arg(&classpath)
        .arg(&main_class)
        .args(&resolved_args)
        .output()
        .await
        .map_err(|e| Error::Other(format!("Failed to spawn Forge processor: {e}")))?;

    if !output.stdout.is_empty() {
        debug!("[forge/stdout] {}", String::from_utf8_lossy(&output.stdout));
    }
    if !output.stderr.is_empty() {
        debug!("[forge/stderr] {}", String::from_utf8_lossy(&output.stderr));
    }

    if !output.status.success() {
        return Err(Error::Other(format!(
            "Forge processor {main_class} exited with code {:?}",
            output.status.code()
        )));
    }
    Ok(())
}

// ── ZIP helpers ───────────────────────────────────────────────────────────────

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
            if entry_name == name_a { a = Some(buf); } else { b = Some(buf); }
        }
        if a.is_some() && b.is_some() { break; }
    }

    let a = a.ok_or_else(|| Error::Other(format!("{name_a} not found in installer JAR")))?;
    let b = b.ok_or_else(|| Error::Other(format!("{name_b} not found in installer JAR")))?;
    Ok((a, b))
}

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

// ── Maven helpers ─────────────────────────────────────────────────────────────

fn maven_to_path_ext(coord: &str) -> Option<PathBuf> {
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

fn library_to_download_job(lib: &Library, libraries_dir: &Path) -> Option<DownloadJob> {
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
            let mut job = DownloadJob::new(&artifact.url, dest);
            if !artifact.sha1.is_empty() { job = job.with_sha1(&artifact.sha1); }
            if artifact.size > 0 { job = job.with_size(artifact.size); }

            // Forge lists some standard artifacts with its own Maven URL,
            // but they're actually only on Maven Central. Add MC as fallback.
            if artifact.url.contains("maven.minecraftforge.net") {
                if let Some(rel) = maven_to_path(&lib.name) {
                    let central_url = format!(
                        "https://repo1.maven.org/maven2/{}",
                        rel.to_string_lossy().replace('\\', "/")
                    );
                    job = job.with_fallback_url(central_url);
                }
            }

            return Some(job);
        }
    }
    None
}

fn short_coord(coord: &str) -> &str {
    let mut parts = coord.splitn(4, ':');
    let _ = parts.next();
    parts.next().unwrap_or(coord)
}

// ── JSON types ────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
struct ForgePromotions {
    promos: HashMap<String, String>,
}

#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct ForgeVersionJson {
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
    client: String,
    #[allow(dead_code)]
    server: String,
}

#[derive(Deserialize)]
struct Processor {
    #[serde(default)]
    sides: Vec<String>,
    jar: String,
    #[serde(default)]
    classpath: Vec<String>,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    outputs: HashMap<String, String>,
}
