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

/// Build the complete list of command-line arguments to pass to the Java binary.
///
/// Separated from [`launch`] so this logic can be tested without spawning a process.
/// The returned `Vec` contains everything after the Java binary path itself:
/// memory flags → JVM args → main class → game args → optional `--server`/`--port`.
pub fn build_java_args(spec: &LaunchSpec) -> Vec<String> {
    let vj    = &spec.version_json;
    let paths = &spec.paths;
    let sep   = if cfg!(target_os = "windows") { ";" } else { ":" };

    // ── Build classpath ───────────────────────────────────────────────────────
    let mut classpath_entries: Vec<PathBuf> = Vec::new();

    for lib in &vj.libraries {
        if !eval_rules(lib.rules.as_deref().unwrap_or(&[])) {
            continue;
        }

        // Native classifiers — skip (they get extracted separately)
        let is_native = lib.natives
            .as_ref()
            .map(|n| n.contains_key(current_os_key()))
            .unwrap_or(false);
        if is_native {
            continue;
        }

        if let Some(dl) = &lib.downloads {
            if let Some(artifact) = &dl.artifact {
                if let Some(path) = &artifact.path {
                    classpath_entries.push(paths.libraries.join(path));
                } else if let Some(rel) = launcher_core::maven_to_path(&lib.name) {
                    classpath_entries.push(paths.libraries.join(rel));
                }
            }
        } else if let Some(rel) = launcher_core::maven_to_path(&lib.name) {
            // Old-format library without explicit downloads block
            classpath_entries.push(paths.libraries.join(rel));
        }
    }

    // Client JAR always last in classpath
    let client_jar = paths.cache.join("client").join(format!("{}.jar", vj.id));
    classpath_entries.push(client_jar);

    let classpath = classpath_entries
        .iter()
        .map(|p| p.to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join(sep);

    // ── Variable substitution map ─────────────────────────────────────────────
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

    // ── Assemble argument list ────────────────────────────────────────────────
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
        // Legacy format (pre-1.13)
        cmd_args.push(format!("-Djava.library.path={}", paths.natives.display()));
        cmd_args.push(format!("-Dminecraft.launcher.brand={launcher_name}"));
        cmd_args.push(format!("-Dminecraft.launcher.version={launcher_version}"));
        cmd_args.push("-cp".to_string());
        cmd_args.push(classpath);
    }

    // Extra JVM args from user / config
    cmd_args.extend(spec.extra_jvm_args.iter().cloned());

    // Main class
    cmd_args.push(vj.main_class.clone());

    // Game arguments
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

    cmd_args
}

pub async fn launch(spec: LaunchSpec, reporter: ProgressReporter) -> Result<GameProcess> {
    reporter.stage("Preparing launch", None).await;

    let cmd_args = build_java_args(&spec);

    info!("Launching: {:?} {:?}", spec.java_binary, &cmd_args[..3.min(cmd_args.len())]);
    debug!("Full command: {:?} {}", spec.java_binary, cmd_args.join(" "));

    let mut child = tokio::process::Command::new(&spec.java_binary)
        .args(&cmd_args)
        .current_dir(&spec.paths.minecraft)
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

pub(crate) fn substitute(s: &str, vars: &HashMap<&str, String>) -> String {
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

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use launcher_core::LauncherPaths;
    use launcher_meta::types::VersionJson;

    /// Minimal VersionJson fixture — no libraries, simple game args.
    const MINIMAL_VJ: &str = r#"{
        "id": "1.21.1",
        "mainClass": "net.minecraft.client.main.Main",
        "assetIndex": { "id": "17", "sha1": "abc", "size": 0, "url": "" },
        "assets": "17",
        "downloads": { "client": { "sha1": "abc", "size": 0, "url": "" } },
        "libraries": [],
        "type": "release",
        "arguments": {
            "jvm": ["-Djava.library.path=${natives_directory}", "-cp", "${classpath}"],
            "game": ["--username", "${auth_player_name}", "--version", "${version_name}",
                     "--gameDir", "${game_directory}", "--assetsDir", "${assets_root}",
                     "--assetIndex", "${assets_index_name}", "--uuid", "${auth_uuid}",
                     "--accessToken", "${auth_access_token}", "--userType", "${user_type}"]
        }
    }"#;

    fn test_spec(ram_mb: u32) -> LaunchSpec {
        let vj: VersionJson = serde_json::from_str(MINIMAL_VJ).unwrap();
        let paths = LauncherPaths::from_root(std::env::temp_dir().join("mc_test_launcher"));
        LaunchSpec {
            version_json: vj,
            java_binary: PathBuf::from("java"),
            paths,
            auth: AuthSession {
                username: "TestPlayer".into(),
                uuid: "test-uuid".into(),
                access_token: "test-token".into(),
                user_type: "msa".into(),
            },
            ram_mb,
            extra_jvm_args: vec![],
            quick_connect: None,
        }
    }

    #[test]
    fn memory_flags_present_and_correct() {
        let args = build_java_args(&test_spec(8192));
        assert!(args.contains(&"-Xmx8192M".to_string()), "missing -Xmx");
        assert!(args.contains(&"-Xms4096M".to_string()), "missing -Xms (ram/2)");
    }

    #[test]
    fn main_class_appears_in_args() {
        let args = build_java_args(&test_spec(4096));
        assert!(
            args.contains(&"net.minecraft.client.main.Main".to_string()),
            "main class missing from args"
        );
    }

    #[test]
    fn auth_player_name_substituted() {
        let args = build_java_args(&test_spec(4096));
        // --username TestPlayer should appear as consecutive entries
        let pos = args.iter().position(|a| a == "--username").expect("--username missing");
        assert_eq!(args[pos + 1], "TestPlayer");
    }

    #[test]
    fn auth_uuid_substituted() {
        let args = build_java_args(&test_spec(4096));
        let pos = args.iter().position(|a| a == "--uuid").expect("--uuid missing");
        assert_eq!(args[pos + 1], "test-uuid");
    }

    #[test]
    fn no_unreplaced_placeholders() {
        let args = build_java_args(&test_spec(4096));
        for arg in &args {
            assert!(
                !arg.contains("${"),
                "unreplaced placeholder found: {arg}"
            );
        }
    }

    #[test]
    fn quick_connect_appended() {
        let mut spec = test_spec(4096);
        spec.quick_connect = Some(("play.miservidor.com".into(), 25565));
        let args = build_java_args(&spec);

        let pos = args.iter().position(|a| a == "--server").expect("--server missing");
        assert_eq!(args[pos + 1], "play.miservidor.com");
        let port_pos = args.iter().position(|a| a == "--port").expect("--port missing");
        assert_eq!(args[port_pos + 1], "25565");
    }

    #[test]
    fn no_quick_connect_when_none() {
        let args = build_java_args(&test_spec(4096));
        assert!(!args.contains(&"--server".to_string()));
        assert!(!args.contains(&"--port".to_string()));
    }

    #[test]
    fn extra_jvm_args_included() {
        let mut spec = test_spec(4096);
        spec.extra_jvm_args = vec!["-XX:+UseG1GC".into(), "-XX:G1HeapRegionSize=32M".into()];
        let args = build_java_args(&spec);
        assert!(args.contains(&"-XX:+UseG1GC".to_string()));
        assert!(args.contains(&"-XX:G1HeapRegionSize=32M".to_string()));
    }

    #[test]
    fn extra_jvm_args_appear_before_main_class() {
        let mut spec = test_spec(4096);
        spec.extra_jvm_args = vec!["-XX:+UseG1GC".into()];
        let args = build_java_args(&spec);
        let jvm_pos   = args.iter().position(|a| a == "-XX:+UseG1GC").unwrap();
        let class_pos = args.iter().position(|a| a == "net.minecraft.client.main.Main").unwrap();
        assert!(jvm_pos < class_pos, "extra JVM args must precede main class");
    }

    #[test]
    fn substitute_replaces_placeholder() {
        let mut vars = HashMap::new();
        vars.insert("foo", "bar".to_string());
        vars.insert("baz", "qux".to_string());
        assert_eq!(substitute("${foo}", &vars), "bar");
        assert_eq!(substitute("prefix_${baz}_suffix", &vars), "prefix_qux_suffix");
        assert_eq!(substitute("no_placeholder", &vars), "no_placeholder");
        // Unknown placeholder stays as-is
        assert_eq!(substitute("${unknown}", &vars), "${unknown}");
    }
}
