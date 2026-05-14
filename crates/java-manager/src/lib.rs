use std::path::{Path, PathBuf};
use std::process::Stdio;
use tracing::{debug, info};

use futures::StreamExt;
use tokio::io::AsyncWriteExt;

use launcher_core::{Error, Result};

/// A detected Java installation.
#[derive(Debug, Clone)]
pub struct JavaInstallation {
    /// Path to the `java` / `java.exe` binary.
    pub binary: PathBuf,
    pub major_version: u32,
    pub full_version: String,
}

/// Find OR auto-download the best Java installation for the requested major version.
///
/// If no suitable Java is found in the system or the launcher-managed dir,
/// downloads a JRE from Adoptium Temurin and installs it under
/// `managed_java_dir/<major>/`.
///
/// `log` is called with human-readable progress strings so the caller
/// (e.g., the Tauri launch command) can surface them to the user.
pub async fn ensure_java(
    required_major: u32,
    managed_java_dir: &Path,
    log: &(dyn Fn(&str) + Send + Sync),
) -> Result<JavaInstallation> {
    // 1. Try existing first (fast path)
    match find_java(required_major, Some(managed_java_dir)).await {
        Ok(java) => return Ok(java),
        Err(e) => {
            info!("Java {required_major} not found ({e}), downloading from Adoptium");
        }
    }

    // 2. Auto-download
    download_and_install_java(required_major, managed_java_dir, log).await?;

    // 3. Find again — should now succeed
    find_java(required_major, Some(managed_java_dir)).await
}

/// Find the best Java installation for the requested major version.
///
/// Search order:
///   1. `JAVA_HOME` env var
///   2. `PATH` (via `which`)
///   3. Common installation directories for the current OS
///   4. Managed java dir (`<launcher_root>/java/<major>`)
pub async fn find_java(
    required_major: u32,
    managed_java_dir: Option<&Path>,
) -> Result<JavaInstallation> {
    let mut candidates: Vec<PathBuf> = Vec::new();

    // 1. JAVA_HOME
    if let Ok(java_home) = std::env::var("JAVA_HOME") {
        let bin = PathBuf::from(&java_home).join("bin").join(java_binary_name());
        candidates.push(bin);
    }

    // 2. PATH
    if let Ok(path) = which::which(java_binary_name()) {
        candidates.push(path);
    }

    // 3. Common OS locations
    candidates.extend(system_java_locations());

    // 4. Launcher-managed JDK
    if let Some(dir) = managed_java_dir {
        let bin = dir
            .join(required_major.to_string())
            .join("bin")
            .join(java_binary_name());
        candidates.push(bin);
    }

    // Probe each candidate
    for candidate in candidates {
        if !candidate.exists() {
            continue;
        }
        match probe_java(&candidate).await {
            Ok(install) if install.major_version >= required_major => {
                info!(
                    "Using Java {} at {:?} (required >={})",
                    install.major_version, install.binary, required_major
                );
                return Ok(install);
            }
            Ok(install) => {
                debug!(
                    "Java {} at {:?} too old (required >={}), skipping",
                    install.major_version, install.binary, required_major
                );
            }
            Err(e) => {
                debug!("Could not probe {:?}: {e}", candidate);
            }
        }
    }

    Err(Error::JavaNotFound(format!(
        "No Java >= {required_major} found. Install Java {required_major} or set JAVA_HOME."
    )))
}

/// Run `java -version` and parse the output to get version info.
pub async fn probe_java(binary: &Path) -> Result<JavaInstallation> {
    let output = tokio::process::Command::new(binary)
        .arg("-version")
        .stderr(Stdio::piped())
        .stdout(Stdio::piped())
        .output()
        .await?;

    // `java -version` writes to stderr
    let text = String::from_utf8_lossy(&output.stderr).to_string()
        + &String::from_utf8_lossy(&output.stdout);

    parse_java_version(binary, &text)
}

fn parse_java_version(binary: &Path, output: &str) -> Result<JavaInstallation> {
    // Modern: `openjdk version "21.0.3" 2024-04-16`
    // Old:    `java version "1.8.0_412"`
    let version_str = output
        .lines()
        .next()
        .and_then(|line| {
            let start = line.find('"')?;
            let end   = line.rfind('"')?;
            if end > start { Some(&line[start + 1..end]) } else { None }
        })
        .ok_or_else(|| Error::JavaNotFound(format!("Could not parse java version from: {output}")))?;

    let major = parse_major_version(version_str)?;

    Ok(JavaInstallation {
        binary: binary.to_path_buf(),
        major_version: major,
        full_version: version_str.to_string(),
    })
}

fn parse_major_version(version: &str) -> Result<u32> {
    // "1.8.0_412" → 8
    // "17.0.11"   → 17
    // "21.0.3"    → 21
    let first = version.split('.').next().unwrap_or("0");
    let major: u32 = first.parse().map_err(|_| {
        Error::JavaNotFound(format!("Cannot parse major version from: {version}"))
    })?;
    Ok(if major == 1 {
        // Old-style "1.x.y" → extract x
        version
            .split('.')
            .nth(1)
            .and_then(|s| s.parse().ok())
            .unwrap_or(8)
    } else {
        major
    })
}

fn java_binary_name() -> &'static str {
    if cfg!(target_os = "windows") { "java.exe" } else { "java" }
}

fn system_java_locations() -> Vec<PathBuf> {
    let mut locs = Vec::new();

    #[cfg(target_os = "windows")]
    {
        let roots = [
            r"C:\Program Files\Java",
            r"C:\Program Files\Eclipse Adoptium",
            r"C:\Program Files\Microsoft",
            r"C:\Program Files\BellSoft",
            r"C:\Program Files\Zulu",
        ];
        for root in &roots {
            if let Ok(entries) = std::fs::read_dir(root) {
                for entry in entries.flatten() {
                    let bin = entry.path().join("bin").join("java.exe");
                    locs.push(bin);
                }
            }
        }
    }

    #[cfg(target_os = "macos")]
    {
        if let Ok(entries) = std::fs::read_dir("/Library/Java/JavaVirtualMachines") {
            for entry in entries.flatten() {
                let bin = entry.path().join("Contents/Home/bin/java");
                locs.push(bin);
            }
        }
    }

    #[cfg(target_os = "linux")]
    {
        if let Ok(entries) = std::fs::read_dir("/usr/lib/jvm") {
            for entry in entries.flatten() {
                let bin = entry.path().join("bin/java");
                locs.push(bin);
            }
        }
    }

    locs
}

// ─── Adoptium auto-download ──────────────────────────────────────────────────

#[derive(serde::Deserialize)]
struct AdoptiumRelease {
    binary: AdoptiumBinary,
    version: AdoptiumVersion,
}

#[derive(serde::Deserialize)]
struct AdoptiumBinary {
    package: AdoptiumPackage,
}

#[derive(serde::Deserialize)]
struct AdoptiumPackage {
    link: String,
    size: u64,
}

#[derive(serde::Deserialize)]
struct AdoptiumVersion {
    semver: String,
}

fn adoptium_os() -> &'static str {
    #[cfg(target_os = "windows")] { return "windows"; }
    #[cfg(target_os = "macos")]   { return "mac"; }
    #[cfg(target_os = "linux")]   { return "linux"; }
    #[allow(unreachable_code)]
    "linux"
}

fn adoptium_arch() -> &'static str {
    #[cfg(target_arch = "x86_64")]  { return "x64"; }
    #[cfg(target_arch = "aarch64")] { return "aarch64"; }
    #[allow(unreachable_code)]
    "x64"
}

async fn download_and_install_java(
    major: u32,
    managed_java_dir: &Path,
    log: &(dyn Fn(&str) + Send + Sync),
) -> Result<()> {
    let os   = adoptium_os();
    let arch = adoptium_arch();

    log(&format!("Consultando Adoptium para Java {major} ({os}/{arch})…"));

    let client = reqwest::Client::builder()
        .user_agent(concat!("mc-launcher/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| Error::Other(e.to_string()))?;

    let api_url = format!(
        "https://api.adoptium.net/v3/assets/latest/{major}/hotspot?os={os}&arch={arch}&image_type=jre"
    );

    let releases: Vec<AdoptiumRelease> = client
        .get(&api_url)
        .send()
        .await
        .map_err(|e| Error::Other(format!("Adoptium API: {e}")))?
        .error_for_status()
        .map_err(|e| Error::Other(format!("Adoptium API error: {e}")))?
        .json()
        .await
        .map_err(|e| Error::Other(format!("Adoptium API parse: {e}")))?;

    let release = releases.into_iter().next().ok_or_else(|| {
        Error::Other(format!("No Java {major} JRE encontrado en Adoptium"))
    })?;

    let pkg = &release.binary.package;
    let size_mb = pkg.size as f64 / 1_048_576.0;
    log(&format!(
        "Descargando Java {} ({:.0} MB)…",
        release.version.semver, size_mb
    ));

    // Prepare paths
    let dest_dir = managed_java_dir.join(major.to_string());
    tokio::fs::create_dir_all(&dest_dir)
        .await
        .map_err(|e| Error::Other(format!("No se puede crear directorio java: {e}")))?;

    let ext = if cfg!(target_os = "windows") { "zip" } else { "tar.gz" };
    let archive_path = managed_java_dir.join(format!("java-{major}-download.{ext}"));

    // Stream download with 10 MB progress updates
    let resp = client
        .get(&pkg.link)
        .send()
        .await
        .map_err(|e| Error::Other(format!("Descarga Java: {e}")))?
        .error_for_status()
        .map_err(|e| Error::Other(format!("Descarga Java HTTP: {e}")))?;

    let mut file = tokio::fs::File::create(&archive_path)
        .await
        .map_err(|e| Error::Other(format!("No se puede crear archivo temporal: {e}")))?;

    let total = pkg.size;
    let mut downloaded: u64 = 0;
    let mut last_reported: u64 = 0;

    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| Error::Other(e.to_string()))?;
        file.write_all(&chunk).await.map_err(|e| Error::Other(e.to_string()))?;
        downloaded += chunk.len() as u64;

        // Report every 10 MB
        if downloaded.saturating_sub(last_reported) >= 10 * 1024 * 1024 {
            log(&format!(
                "  {:.0} / {:.0} MB",
                downloaded as f64 / 1_048_576.0,
                total as f64 / 1_048_576.0
            ));
            last_reported = downloaded;
        }
    }
    file.flush().await.map_err(|e| Error::Other(e.to_string()))?;
    drop(file);

    log("Extrayendo Java…");

    let archive_clone = archive_path.clone();
    let dest_clone    = dest_dir.clone();
    tokio::task::spawn_blocking(move || extract_archive(&archive_clone, &dest_clone))
        .await
        .map_err(|e| Error::Other(format!("Extracción (tarea): {e}")))?
        .map_err(|e| Error::Other(format!("Extracción: {e}")))?;

    // Remove downloaded archive
    let _ = tokio::fs::remove_file(&archive_path).await;

    log(&format!("Java {major} instalado en {}", dest_dir.display()));
    Ok(())
}

/// Extract the Adoptium archive, stripping the top-level directory.
///
/// Adoptium ZIPs/tarballs always contain a single root folder
/// (e.g. `jdk-21.0.3+9-jre/`). We strip it so the layout becomes:
///   `dest/bin/java[.exe]`
///   `dest/lib/…`
fn extract_archive(archive: &Path, dest: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    {
        extract_zip(archive, dest)
    }
    #[cfg(not(target_os = "windows"))]
    {
        extract_tar_gz(archive, dest)
    }
}

#[cfg(target_os = "windows")]
fn extract_zip(archive: &Path, dest: &Path) -> std::io::Result<()> {
    let file = std::fs::File::open(archive)?;
    let mut zip = zip::ZipArchive::new(file)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

    for i in 0..zip.len() {
        let mut entry = zip
            .by_index(i)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

        let raw_name = entry.name().to_string();
        let stripped = strip_first_component(&raw_name);
        if stripped.is_empty() {
            continue; // skip the root dir entry itself
        }

        // Normalise separators (ZIP uses '/')
        let out_path = dest.join(stripped.replace('/', std::path::MAIN_SEPARATOR_STR));

        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out_file = std::fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out_file)?;
        }
    }
    Ok(())
}

#[cfg(not(target_os = "windows"))]
fn extract_tar_gz(archive: &Path, dest: &Path) -> std::io::Result<()> {
    // Delegate to the system `tar` binary — avoids adding flate2/tar deps.
    std::fs::create_dir_all(dest)?;
    let status = std::process::Command::new("tar")
        .args([
            "xzf",
            &archive.to_string_lossy(),
            "--strip-components=1",
            "-C",
            &dest.to_string_lossy(),
        ])
        .status()?;

    if !status.success() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "tar extraction failed",
        ));
    }
    Ok(())
}

/// Strip the first path component from a forward-slash separated path.
///
/// `"jdk-21.0.3+9-jre/bin/java.exe"` → `"bin/java.exe"`
/// `"jdk-21.0.3+9-jre/"` → `""`
fn strip_first_component(path: &str) -> &str {
    match path.find('/') {
        Some(idx) => &path[idx + 1..],
        None      => "",
    }
}
