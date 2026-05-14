use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::{debug, info};

use launcher_core::{Error, LauncherPaths, ProgressReporter, Result};
use launcher_meta::types::{Argument, VersionJson};

use crate::rules::eval_rules;

/// Minimal auth data needed to launch the game.
#[derive(Debug, Clone)]
pub struct AuthSession {
    pub username: String,
    pub uuid: String,
    pub access_token: String,
    /// "msa" for Microsoft accounts, "offline" for dev/testing
    pub user_type: String,
}

impl AuthSession {
    pub fn offline(username: &str) -> Self {
        Self {
            username: username.to_string(),
            uuid: "00000000-0000-0000-0000-000000000001".to_string(),
            access_token: "0".to_string(),
            user_type: "offline".to_string(),
        }
    }
}

pub struct LaunchSpec {
    pub version_json: VersionJson,
    pub java_binary: PathBuf,
    pub paths: LauncherPaths,
    pub auth: AuthSession,
    pub ram_mb: u32,
    pub extra_jvm_args: Vec<String>,
    /// If set, pass `--server <host> --port <port>` to auto-connect
    pub quick_connect: Option<(String, u16)>,
}

/// Handle to the running game process.
pub struct GameProcess {
    child: tokio::process::Child,
    pub pid: u32,
}

impl GameProcess {
    pub async fn wait(&mut self) -> Result<i32> {
        let status = self.child.wait().await?;
        Ok(status.code().unwrap_or(-1))
    }

    pub async fn kill(&mut self) -> Result<()> {
        self.child.kill().await?;
        Ok(())
    }
}

pub async fn launch(spec: LaunchSpec, reporter: ProgressReporter) -> Result<GameProcess> {
    let vj = &spec.version_json;
    let paths = &spec.paths;

    reporter.stage("Preparing launch", None).await;

    // ── Build classpath ───────────────────────────────────────────────────────
    let mut classpath_entries: Vec<PathBuf> = Vec::new();

    for lib in &vj.libraries {
        if !eval_rules(lib.rules.as_deref().unwrap_or(&[])) {
            continue;
        }

        // Native classifiers — skip adding to classpath (they get extracted)
        let is_native = if let Some(natives_map) = &lib.natives {
            let os_key = current_os_key();
            natives_map.contains_key(os_key)
        } else {
            false
        };
        if is_native {
            continue;
        }

        if let Some(dl) = &lib.downloads {
            if let Some(artifact) = &dl.artifact {
                if let Some(path) = &artifact.path {
                    classpath_entries.push(paths.libraries.join(path));
                } else {
                    if let Some(rel) = launcher_core::maven_to_path(&lib.name) {
                        classpath_entries.push(paths.libraries.join(rel));
                    }
                }
            }
        } else {
            // Old-format library without explicit downloads
            if let Some(rel) = launcher_core::maven_to_path(&lib.name) {
                classpath_entries.push(paths.libraries.join(rel));
            }
        }
    }

    // Client JAR always last in classpath
    let client_jar = paths.cache.join("client").join(format!("{}.jar", vj.id));
    classpath_entries.push(client_jar.clone());

    let sep = if cfg!(target_os = "windows") { ";" } else { ":" };
    let classpath = classpath_entries
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(sep);

    // ── Build variable substitution map ───────────────────────────────────────
    let launcher_name    = "mc-launcher-template";
    let launcher_version = env!("CARGO_PKG_VERSION");

    let vars: HashMap<&str, String> = [
        ("auth_player_name",    spec.auth.username.clone()),
        ("auth_uuid",           spec.auth.uuid.clone()),
        ("auth_access_token",   spec.auth.access_token.clone()),
        ("auth_xuid",           "0".to_string()),
        ("clientid",            "0".to_string()),
        ("user_type",           spec.auth.user_type.clone()),
        ("version_name",        vj.id.clone()),
        ("version_type",        format!("{:?}", vj.version_type).to_lowercase()),
        ("game_directory",      paths.minecraft.to_string_lossy().to_string()),
        ("assets_root",         paths.assets.to_string_lossy().to_string()),
        ("assets_index_name",   vj.assets.clone()),
        ("natives_directory",   paths.natives.to_string_lossy().to_string()),
        ("classpath",           classpath.clone()),
        ("launcher_name",       launcher_name.to_string()),
        ("launcher_version",    launcher_version.to_string()),
        // NeoForge / Forge bootstrap variables
        ("library_directory",   paths.libraries.to_string_lossy().to_string()),
        ("classpath_separator", sep.to_string()),
    ]
    .into_iter()
    .collect();

    // ── Collect JVM arguments ─────────────────────────────────────────────────
    let mut cmd_args: Vec<String> = Vec::new();

    // Memory
    cmd_args.push(format!("-Xmx{}M", spec.ram_mb));
    cmd_args.push(format!("-Xms{}M", spec.ram_mb / 2));

    // JVM args from version JSON
    if let Some(arguments) = &vj.arguments {
        for arg in &arguments.jvm {
            collect_arg(arg, &vars, &mut cmd_args);
        }
    } else {
        // Fallback defaults for old-format versions
        cmd_args.push(format!("-Djava.library.path={}", paths.natives.display()));
        cmd_args.push(format!("-Dminecraft.launcher.brand={launcher_name}"));
        cmd_args.push(format!("-Dminecraft.launcher.version={launcher_version}"));
        cmd_args.push("-cp".to_string());
        cmd_args.push(classpath);
    }

    // Extra JVM args from config / user settings
    cmd_args.extend(spec.extra_jvm_args.iter().cloned());

    // ── Main class ────────────────────────────────────────────────────────────
    cmd_args.push(vj.main_class.clone());

    // ── Game arguments ────────────────────────────────────────────────────────
    if let Some(arguments) = &vj.arguments {
        for arg in &arguments.game {
            collect_arg(arg, &vars, &mut cmd_args);
        }
    } else if let Some(legacy) = &vj.minecraft_arguments {
        for token in legacy.split(' ') {
            cmd_args.push(substitute(token, &vars));
        }
    }

    // Quick-connect
    if let Some((host, port)) = &spec.quick_connect {
        cmd_args.push("--server".to_string());
        cmd_args.push(host.clone());
        cmd_args.push("--port".to_string());
        cmd_args.push(port.to_string());
    }

    // ── Spawn ─────────────────────────────────────────────────────────────────
    info!("Launching: {:?} {:?}", spec.java_binary, &cmd_args[..3.min(cmd_args.len())]);
    debug!("Full command: {:?} {}", spec.java_binary, cmd_args.join(" "));

    let mut child = tokio::process::Command::new(&spec.java_binary)
        .args(&cmd_args)
        .current_dir(&paths.minecraft)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;

    let pid = child.id().ok_or_else(|| Error::Other("Could not get process PID".into()))?;
    info!("Minecraft started (PID {pid})");
    reporter.info(format!("Minecraft started (PID {pid})")).await;

    // Stream logs in background
    if let Some(stdout) = child.stdout.take() {
        let rep = reporter.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                rep.info(format!("[MC] {line}")).await;
            }
        });
    }
    if let Some(stderr) = child.stderr.take() {
        let rep = reporter.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                rep.info(format!("[MC/err] {line}")).await;
            }
        });
    }

    reporter.done().await;
    Ok(GameProcess { child, pid })
}

fn collect_arg(arg: &Argument, vars: &HashMap<&str, String>, out: &mut Vec<String>) {
    match arg {
        Argument::Plain(s) => out.push(substitute(s, vars)),
        Argument::Conditional { rules, value } => {
            if eval_rules(rules) {
                for s in value.as_vec() {
                    out.push(substitute(&s, vars));
                }
            }
        }
    }
}

fn substitute(s: &str, vars: &HashMap<&str, String>) -> String {
    let mut result = s.to_string();
    for (k, v) in vars {
        result = result.replace(&format!("${{{k}}}"), v);
    }
    result
}

fn current_os_key() -> &'static str {
    if cfg!(target_os = "windows")     { "windows" }
    else if cfg!(target_os = "macos")  { "osx" }
    else                               { "linux" }
}
