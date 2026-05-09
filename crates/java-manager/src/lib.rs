use std::path::{Path, PathBuf};
use std::process::Stdio;
use tracing::{debug, info};

use launcher_core::{Error, Result};

/// A detected Java installation.
#[derive(Debug, Clone)]
pub struct JavaInstallation {
    /// Path to the `java` / `java.exe` binary.
    pub binary: PathBuf,
    pub major_version: u32,
    pub full_version: String,
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
