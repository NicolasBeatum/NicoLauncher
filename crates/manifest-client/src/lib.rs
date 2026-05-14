use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use launcher_core::{Error, Result};
use launcher_downloader::DownloadJob;
use launcher_mods::ModSource;
use serde::{Deserialize, Serialize};
use tracing::{debug, info, warn};

// ── Schema types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerManifest {
    pub schema_version: u32,
    pub manifest_version: String,
    pub released_at: chrono::DateTime<chrono::Utc>,
    pub minecraft: MinecraftSpec,
    #[serde(default)]
    pub loader: Option<LoaderSpec>,
    #[serde(default)]
    pub required_mods: Vec<ModEntry>,
    #[serde(default)]
    pub optional_mods: Vec<OptionalModEntry>,
    #[serde(default)]
    pub config_overrides: Vec<ConfigOverride>,
    #[serde(default)]
    pub removed_files: Vec<String>,
    #[serde(default)]
    pub additional_jvm_args: Vec<String>,
    pub announcement: Option<Announcement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinecraftSpec {
    pub version: String,
    pub java_version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoaderSpec {
    #[serde(rename = "type")]
    pub loader_type: launcher_core::LoaderType,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModEntry {
    pub id: String,
    pub name: String,
    pub source: ModSource,
    pub sha512: String,
    pub size: u64,
    pub filename: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OptionalModEntry {
    #[serde(flatten)]
    pub base: ModEntry,
    #[serde(default)]
    pub default_enabled: bool,
    pub category: Option<String>,
    pub description: Option<String>,
    pub icon_url: Option<String>,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub conflicts_with: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigOverride {
    pub path: String,
    pub url: String,
    pub sha512: String,
    /// "always" | "if_missing"
    pub apply: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Announcement {
    pub id: String,
    pub title: String,
    pub body_md: String,
    pub show_until: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Debug, Deserialize)]
struct SignedManifest {
    manifest: String,
    signature: String,
}

// ── Local state ───────────────────────────────────────────────────────────────

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct InstalledMod {
    pub sha512: String,
    pub filename: String,
    /// true if this mod is from the optional_mods list (it still lives in `mods/` alongside required mods)
    #[serde(default)]
    pub is_optional: bool,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct LocalState {
    pub applied_manifest_version: Option<String>,
    pub applied_at: Option<chrono::DateTime<chrono::Utc>>,
    /// mod_id → installed info (sha512 + filename)
    pub installed_mods: HashMap<String, InstalledMod>,
    /// config path → sha512
    pub applied_configs: HashMap<String, String>,
    pub loader_installed: Option<InstalledLoader>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InstalledLoader {
    pub loader_type: String,
    pub version: String,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct OptionalChoices {
    pub enabled: Vec<String>,
    pub last_seen_optional_ids: Vec<String>,
    pub dismissed_announcement_ids: Vec<String>,
}

impl LocalState {
    pub async fn load(path: &Path) -> Self {
        match tokio::fs::read(path).await {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub async fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let json = serde_json::to_vec_pretty(self)?;
        tokio::fs::write(path, json).await?;
        Ok(())
    }
}

impl OptionalChoices {
    pub async fn load(path: &Path) -> Self {
        match tokio::fs::read(path).await {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    pub async fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let json = serde_json::to_vec_pretty(self)?;
        tokio::fs::write(path, json).await?;
        Ok(())
    }
}

// ── Sync plan ─────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct SyncPlan {
    /// Required mods missing or hash-mismatched
    pub mods_to_download: Vec<ModEntry>,
    /// Enabled optional mods missing or hash-mismatched (not yet in CAS)
    pub optional_mods_to_download: Vec<ModEntry>,
    /// Filenames to delete from `mods/` (required mods removed from the manifest)
    pub mods_to_remove: Vec<String>,
    /// Filenames to delete from `mods/` (optional mods now disabled / removed from manifest)
    pub optional_mods_to_remove: Vec<String>,
    pub configs_to_apply: Vec<ConfigOverride>,
    pub files_to_delete: Vec<String>,
    pub loader_action: LoaderAction,
}

#[derive(Debug)]
pub enum LoaderAction {
    None,
    Install(LoaderSpec),
    Reinstall(LoaderSpec),
}

pub fn compute_sync_plan(
    current: &LocalState,
    remote: &ServerManifest,
    optional_choices: &OptionalChoices,
) -> SyncPlan {
    let desired_required: Vec<&ModEntry> = remote.required_mods.iter().collect();
    let desired_optional: Vec<&ModEntry> = remote
        .optional_mods
        .iter()
        .filter(|o| optional_choices.enabled.contains(&o.base.id))
        .map(|o| &o.base)
        .collect();

    let desired_required_ids: std::collections::HashSet<&str> =
        desired_required.iter().map(|m| m.id.as_str()).collect();
    let desired_optional_ids: std::collections::HashSet<&str> =
        desired_optional.iter().map(|m| m.id.as_str()).collect();

    // Required mods that are missing or hash-mismatched
    let mods_to_download: Vec<ModEntry> = desired_required
        .iter()
        .filter(|m| {
            current
                .installed_mods
                .get(&m.id)
                .map(|inst| inst.sha512 != m.sha512)
                .unwrap_or(true)
        })
        .map(|m| (*m).clone())
        .collect();

    // Enabled optional mods not yet installed / hash changed → need to download to CAS
    // NOTE: the downloader skips files already present in CAS, so "no re-download" is free.
    let optional_mods_to_download: Vec<ModEntry> = desired_optional
        .iter()
        .filter(|m| {
            current
                .installed_mods
                .get(&m.id)
                .map(|inst| inst.sha512 != m.sha512)
                .unwrap_or(true)
        })
        .map(|m| (*m).clone())
        .collect();

    // Required mods no longer in the manifest → remove from mods/
    let mods_to_remove: Vec<String> = current
        .installed_mods
        .iter()
        .filter(|(id, info)| !info.is_optional && !desired_required_ids.contains(id.as_str()))
        .map(|(_, info)| info.filename.clone())
        .collect();

    // Optional mods now disabled / removed → remove link from mods/
    let optional_mods_to_remove: Vec<String> = current
        .installed_mods
        .iter()
        .filter(|(id, info)| info.is_optional && !desired_optional_ids.contains(id.as_str()))
        .map(|(_, info)| info.filename.clone())
        .collect();

    // Config overrides: "always" or not yet applied
    let configs_to_apply: Vec<ConfigOverride> = remote
        .config_overrides
        .iter()
        .filter(|c| c.apply == "always" || !current.applied_configs.contains_key(&c.path))
        .cloned()
        .collect();

    let loader_action = match &remote.loader {
        None => LoaderAction::None,
        Some(spec) => match &current.loader_installed {
            None => LoaderAction::Install(spec.clone()),
            Some(installed)
                if installed.loader_type != spec.loader_type.to_string()
                    || installed.version != spec.version =>
            {
                LoaderAction::Reinstall(spec.clone())
            }
            _ => LoaderAction::None,
        },
    };

    SyncPlan {
        mods_to_download,
        optional_mods_to_download,
        mods_to_remove,
        optional_mods_to_remove,
        configs_to_apply,
        files_to_delete: remote.removed_files.clone(),
        loader_action,
    }
}

// ── Providers ─────────────────────────────────────────────────────────────────

#[async_trait::async_trait]
pub trait ManifestProvider: Send + Sync {
    async fn fetch_raw(&self) -> Result<String>;
    fn name(&self) -> &str;
}

pub struct HttpProvider {
    url: String,
    http: reqwest::Client,
}

impl HttpProvider {
    pub fn new(url: impl Into<String>) -> Result<Self> {
        let http = reqwest::Client::builder()
            .user_agent(concat!(
                "mc-launcher-template/",
                env!("CARGO_PKG_VERSION")
            ))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|e| Error::Other(e.to_string()))?;
        Ok(Self { url: url.into(), http })
    }
}

#[async_trait::async_trait]
impl ManifestProvider for HttpProvider {
    async fn fetch_raw(&self) -> Result<String> {
        let mut last_err = String::new();
        for attempt in 1u64..=3 {
            match self.http.get(&self.url).send().await {
                Ok(resp) => {
                    return resp
                        .error_for_status()
                        .map_err(|e| Error::Manifest(e.to_string()))?
                        .text()
                        .await
                        .map_err(|e| Error::Manifest(e.to_string()));
                }
                Err(e) => {
                    warn!("Manifest fetch attempt {attempt}/3: {e}");
                    last_err = e.to_string();
                    tokio::time::sleep(Duration::from_secs(attempt)).await;
                }
            }
        }
        Err(Error::Manifest(format!(
            "Failed to fetch manifest after 3 attempts: {last_err}"
        )))
    }

    fn name(&self) -> &str { "http" }
}

pub struct FileProvider {
    path: std::path::PathBuf,
}

impl FileProvider {
    pub fn new(path: impl Into<std::path::PathBuf>) -> Self {
        Self { path: path.into() }
    }
}

#[async_trait::async_trait]
impl ManifestProvider for FileProvider {
    async fn fetch_raw(&self) -> Result<String> {
        tokio::fs::read_to_string(&self.path)
            .await
            .map_err(|e| Error::Manifest(format!("Cannot read manifest {:?}: {e}", self.path)))
    }

    fn name(&self) -> &str { "file" }
}

/// Git-hosting provider — accepts regular GitHub/GitLab blob URLs and converts them
/// to raw file URLs automatically.  Falls back to plain HTTP for other URLs.
///
/// Examples:
///   `https://github.com/user/repo/blob/main/manifest.json`
///   `https://gitlab.com/user/repo/-/blob/main/manifest.json`
///   `https://raw.githubusercontent.com/user/repo/main/manifest.json`  (already raw)
pub struct GitProvider {
    inner: HttpProvider,
    raw_url: String,
}

impl GitProvider {
    pub fn new(url: impl Into<String>) -> Result<Self> {
        let raw_url = Self::to_raw_url(url.into());
        Ok(Self {
            inner: HttpProvider::new(&raw_url)?,
            raw_url,
        })
    }

    /// Convert a web (blob) URL from a known git host to its raw equivalent.
    /// Unknown URLs are returned unchanged (assumed already raw or custom CDN).
    fn to_raw_url(url: String) -> String {
        // GitHub: https://github.com/<owner>/<repo>/blob/<branch>/<path>
        //      → https://raw.githubusercontent.com/<owner>/<repo>/<branch>/<path>
        if let Some(rest) = url.strip_prefix("https://github.com/") {
            if rest.contains("/blob/") {
                return format!(
                    "https://raw.githubusercontent.com/{}",
                    rest.replacen("/blob/", "/", 1)
                );
            }
            // Already a raw.githubusercontent.com link passed as github.com → keep
        }

        // GitLab: https://gitlab.com/<owner>/<repo>/-/blob/<branch>/<path>
        //      → https://gitlab.com/<owner>/<repo>/-/raw/<branch>/<path>
        if url.contains("/-/blob/") {
            return url.replace("/-/blob/", "/-/raw/");
        }

        // Codeberg / Gitea / Forgejo: https://codeberg.org/<owner>/<repo>/raw/branch/<branch>/<path>
        // These already use /raw/ so they pass through unchanged.

        url // unknown format → use as-is
    }
}

#[async_trait::async_trait]
impl ManifestProvider for GitProvider {
    async fn fetch_raw(&self) -> Result<String> {
        debug!("Git manifest raw URL: {}", self.raw_url);
        self.inner.fetch_raw().await
    }
    fn name(&self) -> &str { "git" }
}

// ── Manifest loading (handles signed / unsigned) ──────────────────────────────

/// Valida que todos los paths de un manifest sean relativos y no escapen del directorio.
/// Debe llamarse justo después de parsear el manifest.
pub fn validate_manifest_paths(manifest: &ServerManifest) -> Result<()> {
    for cfg in &manifest.config_overrides {
        validate_relative_path(&cfg.path).map_err(|e| {
            Error::Manifest(format!("config_overrides path inválido {:?}: {e}", cfg.path))
        })?;
    }
    for path in &manifest.removed_files {
        validate_relative_path(path).map_err(|e| {
            Error::Manifest(format!("removed_files path inválido {:?}: {e}", path))
        })?;
    }
    Ok(())
}

fn validate_relative_path(path: &str) -> std::result::Result<(), &'static str> {
    use std::path::{Component, Path};
    // Rechazar paths absolutos (e.g. C:\... en Windows)
    if Path::new(path).is_absolute() {
        return Err("el path no puede ser absoluto");
    }
    // Rechazar paths root-relativos que empiezan con / o \ (en Windows is_absolute() devuelve
    // false para "/foo" porque no tiene letra de unidad)
    if path.starts_with('/') || path.starts_with('\\') {
        return Err("el path no puede comenzar con / o \\");
    }
    // Rechazar componentes ".."
    for component in Path::new(path).components() {
        if matches!(component, Component::ParentDir) {
            return Err("el path no puede contener '..'");
        }
    }
    Ok(())
}

/// Fetch and parse a manifest, verifying Ed25519 signature when a public key is configured.
/// Also validates that all file paths are safe (no traversal attacks).
pub async fn fetch_manifest(
    provider: &dyn ManifestProvider,
    public_key_hex: &str,
) -> Result<ServerManifest> {
    let raw = provider.fetch_raw().await?;
    debug!(
        "Fetched manifest from {} ({} bytes)",
        provider.name(),
        raw.len()
    );

    let manifest = if let Ok(signed) = serde_json::from_str::<SignedManifest>(&raw) {
        if !public_key_hex.is_empty() {
            verify_ed25519(&signed.manifest, &signed.signature, public_key_hex)?;
            info!("Manifest signature verified.");
        }
        serde_json::from_str::<ServerManifest>(&signed.manifest)
            .map_err(|e| Error::Manifest(format!("Inner manifest parse error: {e}")))?
    } else {
        if !public_key_hex.is_empty() {
            return Err(Error::Manifest(
                "Manifest is unsigned but manifest_public_key is configured. Refusing to load.".into(),
            ));
        }
        serde_json::from_str::<ServerManifest>(&raw)
            .map_err(|e| Error::Manifest(format!("Parse error: {e}")))?
    };

    // Security: reject manifests with path traversal attempts
    validate_manifest_paths(&manifest)?;

    Ok(manifest)
}

fn verify_ed25519(message: &str, sig_hex: &str, pubkey_hex: &str) -> Result<()> {
    use ed25519_dalek::{Signature, VerifyingKey};

    let key_bytes = hex::decode(pubkey_hex)
        .map_err(|e| Error::Manifest(format!("Invalid public key hex: {e}")))?;
    let key_array: [u8; 32] = key_bytes
        .try_into()
        .map_err(|_| Error::Manifest("Public key must be 32 bytes".into()))?;
    let verifying_key = VerifyingKey::from_bytes(&key_array)
        .map_err(|e| Error::Manifest(format!("Invalid public key: {e}")))?;

    let sig_bytes = hex::decode(sig_hex)
        .map_err(|e| Error::Manifest(format!("Invalid signature hex: {e}")))?;
    let sig_array: [u8; 64] = sig_bytes
        .try_into()
        .map_err(|_| Error::Manifest("Signature must be 64 bytes".into()))?;
    let signature = Signature::from_bytes(&sig_array);

    verifying_key
        .verify_strict(message.as_bytes(), &signature)
        .map_err(|e| Error::Manifest(format!("Signature verification failed: {e}")))
}

// ── Download job builder ──────────────────────────────────────────────────────

/// Build `DownloadJob`s targeting the CAS directory (`cache/mod-files/<sha[0..2]>/<sha>`).
/// After downloading, call `link_mods_from_cas` to hardlink/copy into the mods dir.
pub fn mod_cas_download_jobs(mods: &[ModEntry], cas_dir: &Path) -> Vec<DownloadJob> {
    mods.iter()
        .filter_map(|m| {
            if m.sha512.len() < 4 { return None; }
            let url = source_url(&m.source)?;
            let cas_path = cas_dir.join(&m.sha512[..2]).join(&m.sha512);
            let mut job = DownloadJob::new(&url, cas_path);
            if !m.sha512.is_empty() { job = job.with_sha512(&m.sha512); }
            if m.size > 0           { job = job.with_size(m.size); }
            Some(job)
        })
        .collect()
}

/// Hardlink (or copy as fallback) each mod from the CAS into `mods_dir/<filename>`.
/// Returns the list of filenames that were successfully linked.
pub async fn link_mods_from_cas(
    mods: &[ModEntry],
    cas_dir: &Path,
    mods_dir: &Path,
) -> Vec<String> {
    let mut linked = Vec::new();
    for m in mods {
        if m.sha512.len() < 4 { continue; }
        let src  = cas_dir.join(&m.sha512[..2]).join(&m.sha512);
        let dest = mods_dir.join(&m.filename);
        // Eliminar destino previo si existe (puede ser un hardlink viejo o archivo corrupto)
        let _ = tokio::fs::remove_file(&dest).await;
        // Intentar hardlink primero; si falla (dispositivos distintos, permisos, etc.) → copia
        let ok = tokio::fs::hard_link(&src, &dest).await.is_ok()
            || tokio::fs::copy(&src, &dest).await.is_ok();
        if ok { linked.push(m.filename.clone()); }
    }
    linked
}

/// Build `DownloadJob`s for the given mod entries (mods that need to be downloaded).
/// Legacy: descarga directamente al directorio de mods sin CAS.
pub fn mod_download_jobs(mods: &[ModEntry], mods_dir: &Path) -> Vec<DownloadJob> {
    mods.iter()
        .filter_map(|m| {
            let url = source_url(&m.source)?;
            let mut job = DownloadJob::new(&url, mods_dir.join(&m.filename));
            if !m.sha512.is_empty() { job = job.with_sha512(&m.sha512); }
            if m.size > 0           { job = job.with_size(m.size); }
            Some(job)
        })
        .collect()
}

fn source_url(source: &ModSource) -> Option<String> {
    match source {
        ModSource::SelfHosted { url } => Some(url.clone()),
        ModSource::Modrinth { download_url, .. } => download_url.clone(),
        ModSource::CurseForge { download_url, .. } => download_url.clone(),
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── validate_relative_path ────────────────────────────────────────────────

    #[test]
    fn valid_paths_pass() {
        assert!(validate_relative_path("config/mymod.toml").is_ok());
        assert!(validate_relative_path("options.txt").is_ok());
        assert!(validate_relative_path("config/subfolder/file.cfg").is_ok());
    }

    #[test]
    fn parent_dir_rejected() {
        assert!(validate_relative_path("../outside.txt").is_err());
        assert!(validate_relative_path("config/../../etc/passwd").is_err());
        assert!(validate_relative_path("a/b/../../../secret").is_err());
    }

    #[test]
    fn absolute_path_rejected() {
        assert!(validate_relative_path("/etc/passwd").is_err());
        #[cfg(windows)]
        assert!(validate_relative_path("C:\\Windows\\System32\\evil.dll").is_err());
    }

    // ── validate_manifest_paths ───────────────────────────────────────────────

    fn make_manifest(config_paths: &[&str], removed: &[&str]) -> ServerManifest {
        ServerManifest {
            schema_version: 1,
            manifest_version: "test".into(),
            released_at: chrono::Utc::now(),
            minecraft: MinecraftSpec { version: "1.21.1".into(), java_version: 21 },
            loader: None,
            required_mods: vec![],
            optional_mods: vec![],
            config_overrides: config_paths.iter().map(|p| ConfigOverride {
                path: p.to_string(),
                url: "http://example.com/f".into(),
                sha512: String::new(),
                apply: "always".into(),
            }).collect(),
            removed_files: removed.iter().map(|s| s.to_string()).collect(),
            additional_jvm_args: vec![],
            announcement: None,
        }
    }

    #[test]
    fn manifest_with_safe_paths_ok() {
        let m = make_manifest(&["config/server.toml", "options.txt"], &["mods/old.jar"]);
        assert!(validate_manifest_paths(&m).is_ok());
    }

    #[test]
    fn manifest_with_traversal_in_config_override_fails() {
        let m = make_manifest(&["../../etc/passwd"], &[]);
        assert!(validate_manifest_paths(&m).is_err());
    }

    #[test]
    fn manifest_with_traversal_in_removed_files_fails() {
        let m = make_manifest(&[], &["../launcher.exe"]);
        assert!(validate_manifest_paths(&m).is_err());
    }

    // ── compute_sync_plan ─────────────────────────────────────────────────────

    fn mod_entry(id: &str, sha: &str) -> ModEntry {
        ModEntry {
            id: id.into(),
            name: id.into(),
            source: launcher_mods::ModSource::SelfHosted { url: format!("http://x/{id}.jar") },
            sha512: sha.into(),
            size: 100,
            filename: format!("{id}.jar"),
        }
    }

    fn opt_entry(id: &str) -> OptionalModEntry {
        OptionalModEntry {
            base: mod_entry(id, &format!("sha-{id}")),
            default_enabled: false,
            category: None,
            description: None,
            icon_url: None,
            depends_on: vec![],
            conflicts_with: vec![],
        }
    }

    #[test]
    fn fresh_install_downloads_all_required() {
        let manifest = ServerManifest {
            required_mods: vec![mod_entry("create", "sha1"), mod_entry("jei", "sha2")],
            ..make_manifest(&[], &[])
        };
        let local = LocalState::default();
        let choices = OptionalChoices::default();
        let plan = compute_sync_plan(&local, &manifest, &choices);

        assert_eq!(plan.mods_to_download.len(), 2);
        assert!(plan.mods_to_remove.is_empty());
    }

    #[test]
    fn unchanged_mods_are_skipped() {
        let manifest = ServerManifest {
            required_mods: vec![mod_entry("create", "sha1")],
            ..make_manifest(&[], &[])
        };
        let mut local = LocalState::default();
        local.installed_mods.insert(
            "create".into(),
            InstalledMod { sha512: "sha1".into(), filename: "create.jar".into(), is_optional: false },
        );
        let choices = OptionalChoices::default();
        let plan = compute_sync_plan(&local, &manifest, &choices);

        assert!(plan.mods_to_download.is_empty());
        assert!(plan.mods_to_remove.is_empty());
    }

    #[test]
    fn updated_mod_triggers_redownload() {
        let manifest = ServerManifest {
            required_mods: vec![mod_entry("create", "sha-new")],
            ..make_manifest(&[], &[])
        };
        let mut local = LocalState::default();
        local.installed_mods.insert(
            "create".into(),
            InstalledMod { sha512: "sha-old".into(), filename: "create.jar".into(), is_optional: false },
        );
        let choices = OptionalChoices::default();
        let plan = compute_sync_plan(&local, &manifest, &choices);

        assert_eq!(plan.mods_to_download.len(), 1);
        assert_eq!(plan.mods_to_download[0].sha512, "sha-new");
    }

    #[test]
    fn removed_mod_is_queued_for_deletion() {
        let manifest = ServerManifest {
            required_mods: vec![mod_entry("jei", "sha-jei")],
            ..make_manifest(&[], &[])
        };
        let mut local = LocalState::default();
        local.installed_mods.insert(
            "old-mod".into(),
            InstalledMod { sha512: "sha-old".into(), filename: "old-mod.jar".into(), is_optional: false },
        );
        local.installed_mods.insert(
            "jei".into(),
            InstalledMod { sha512: "sha-jei".into(), filename: "jei.jar".into(), is_optional: false },
        );
        let choices = OptionalChoices::default();
        let plan = compute_sync_plan(&local, &manifest, &choices);

        assert!(plan.mods_to_download.is_empty()); // jei ya está
        assert_eq!(plan.mods_to_remove.len(), 1);
        assert_eq!(plan.mods_to_remove[0], "old-mod.jar");
    }

    #[test]
    fn enabled_optional_is_included() {
        let manifest = ServerManifest {
            optional_mods: vec![opt_entry("jei")],
            ..make_manifest(&[], &[])
        };
        let local = LocalState::default();
        let choices = OptionalChoices { enabled: vec!["jei".into()], ..Default::default() };
        let plan = compute_sync_plan(&local, &manifest, &choices);
        assert_eq!(plan.optional_mods_to_download.len(), 1);
        assert!(plan.mods_to_download.is_empty());
    }

    #[test]
    fn disabled_optional_is_not_downloaded() {
        let manifest = ServerManifest {
            optional_mods: vec![opt_entry("jei")],
            ..make_manifest(&[], &[])
        };
        let local = LocalState::default();
        let choices = OptionalChoices::default(); // jei NOT enabled
        let plan = compute_sync_plan(&local, &manifest, &choices);
        assert!(plan.optional_mods_to_download.is_empty());
    }

    #[test]
    fn disabled_optional_that_was_installed_is_queued_for_removal() {
        let manifest = ServerManifest {
            optional_mods: vec![opt_entry("jei")],
            ..make_manifest(&[], &[])
        };
        let mut local = LocalState::default();
        local.installed_mods.insert(
            "jei".into(),
            InstalledMod { sha512: "sha-jei".into(), filename: "jei.jar".into(), is_optional: true },
        );
        let choices = OptionalChoices::default(); // jei NOT enabled
        let plan = compute_sync_plan(&local, &manifest, &choices);
        assert_eq!(plan.optional_mods_to_remove.len(), 1);
        assert_eq!(plan.optional_mods_to_remove[0], "jei.jar");
    }
}
