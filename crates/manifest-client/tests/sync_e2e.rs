//! End-to-end sync tests using FileProvider (no network required).
//!
//! These tests verify the full pipeline:
//!   manifest JSON on disk → fetch_manifest → compute_sync_plan
//!
//! They run against real manifest JSON fixtures, so they exercise parsing,
//! path validation, and the sync-diff algorithm together.

use std::io::Write as _;
use launcher_manifest_client::{
    fetch_manifest, compute_sync_plan, FileProvider,
    LocalState, OptionalChoices, LoaderAction, InstalledLoader, InstalledMod,
};

// ── Fixture helpers ───────────────────────────────────────────────────────────

fn write_manifest(content: &str) -> (tempfile::NamedTempFile, std::path::PathBuf) {
    let mut f = tempfile::NamedTempFile::new().unwrap();
    f.write_all(content.as_bytes()).unwrap();
    let p = f.path().to_path_buf();
    (f, p)
}

const MANIFEST_TWO_MODS: &str = r#"{
  "schema_version": 1,
  "manifest_version": "2026.05.01-1",
  "released_at": "2026-05-01T00:00:00Z",
  "minecraft": { "version": "1.21.1", "java_version": 21 },
  "loader": { "type": "neoforge", "version": "21.1.95" },
  "required_mods": [
    {
      "id": "create",
      "name": "Create",
      "source": { "type": "self_hosted", "url": "http://cdn.example.com/create.jar" },
      "sha512": "aaaaaaaabbbbbbbb",
      "size": 1000,
      "filename": "create.jar"
    },
    {
      "id": "jei",
      "name": "Just Enough Items",
      "source": { "type": "self_hosted", "url": "http://cdn.example.com/jei.jar" },
      "sha512": "ccccccccdddddddd",
      "size": 500,
      "filename": "jei.jar"
    }
  ],
  "optional_mods": [],
  "config_overrides": [],
  "removed_files": [],
  "additional_jvm_args": []
}"#;

const MANIFEST_WITH_OPTIONAL: &str = r#"{
  "schema_version": 1,
  "manifest_version": "2026.05.01-1",
  "released_at": "2026-05-01T00:00:00Z",
  "minecraft": { "version": "1.21.1", "java_version": 21 },
  "required_mods": [],
  "optional_mods": [
    {
      "id": "sodium",
      "name": "Sodium",
      "source": { "type": "self_hosted", "url": "http://cdn.example.com/sodium.jar" },
      "sha512": "eeeeeeeefffffff0",
      "size": 800,
      "filename": "sodium.jar",
      "default_enabled": false
    }
  ],
  "config_overrides": [],
  "removed_files": [],
  "additional_jvm_args": []
}"#;

const MANIFEST_WITH_TRAVERSAL: &str = r#"{
  "schema_version": 1,
  "manifest_version": "2026.05.01-1",
  "released_at": "2026-05-01T00:00:00Z",
  "minecraft": { "version": "1.21.1", "java_version": 21 },
  "required_mods": [],
  "optional_mods": [],
  "config_overrides": [
    {
      "path": "../../etc/passwd",
      "url": "http://evil.com/passwd",
      "sha512": "x",
      "apply": "always"
    }
  ],
  "removed_files": [],
  "additional_jvm_args": []
}"#;

// ── Tests — fresh install ─────────────────────────────────────────────────────

#[tokio::test]
async fn fresh_install_needs_all_required_mods() {
    let (_f, path) = write_manifest(MANIFEST_TWO_MODS);
    let provider  = FileProvider::new(path);
    let manifest  = fetch_manifest(&provider, "").await.unwrap();
    let plan      = compute_sync_plan(&LocalState::default(), &manifest, &OptionalChoices::default());

    assert_eq!(plan.mods_to_download.len(), 2);
    let ids: Vec<_> = plan.mods_to_download.iter().map(|m| m.id.as_str()).collect();
    assert!(ids.contains(&"create"));
    assert!(ids.contains(&"jei"));
    assert!(plan.mods_to_remove.is_empty());
}

#[tokio::test]
async fn nothing_to_do_when_already_up_to_date() {
    let (_f, path) = write_manifest(MANIFEST_TWO_MODS);
    let provider  = FileProvider::new(path);
    let manifest  = fetch_manifest(&provider, "").await.unwrap();

    let mut local = LocalState::default();
    local.installed_mods.insert("create".into(), InstalledMod {
        sha512: "aaaaaaaabbbbbbbb".into(),
        filename: "create.jar".into(),
        is_optional: false,
    });
    local.installed_mods.insert("jei".into(), InstalledMod {
        sha512: "ccccccccdddddddd".into(),
        filename: "jei.jar".into(),
        is_optional: false,
    });
    local.loader_installed = Some(InstalledLoader {
        loader_type: "neoforge".into(),
        version: "21.1.95".into(),
    });

    let plan = compute_sync_plan(&local, &manifest, &OptionalChoices::default());

    assert!(plan.mods_to_download.is_empty());
    assert!(plan.mods_to_remove.is_empty());
    assert!(matches!(plan.loader_action, LoaderAction::None));
}

// ── Tests — loader ────────────────────────────────────────────────────────────

#[tokio::test]
async fn loader_install_triggered_on_fresh_install() {
    let (_f, path) = write_manifest(MANIFEST_TWO_MODS);
    let provider  = FileProvider::new(path);
    let manifest  = fetch_manifest(&provider, "").await.unwrap();
    let plan      = compute_sync_plan(&LocalState::default(), &manifest, &OptionalChoices::default());

    assert!(matches!(plan.loader_action, LoaderAction::Install(_)));
    if let LoaderAction::Install(spec) = &plan.loader_action {
        assert_eq!(spec.version, "21.1.95");
    }
}

#[tokio::test]
async fn loader_reinstall_triggered_on_version_change() {
    let (_f, path) = write_manifest(MANIFEST_TWO_MODS);
    let provider  = FileProvider::new(path);
    let manifest  = fetch_manifest(&provider, "").await.unwrap();

    let mut local = LocalState::default();
    local.loader_installed = Some(InstalledLoader {
        loader_type: "neoforge".into(),
        version: "21.1.80".into(), // old version
    });

    let plan = compute_sync_plan(&local, &manifest, &OptionalChoices::default());
    assert!(matches!(plan.loader_action, LoaderAction::Reinstall(_)));
}

// ── Tests — optional mods ─────────────────────────────────────────────────────

#[tokio::test]
async fn optional_not_downloaded_if_not_enabled() {
    let (_f, path) = write_manifest(MANIFEST_WITH_OPTIONAL);
    let provider  = FileProvider::new(path);
    let manifest  = fetch_manifest(&provider, "").await.unwrap();
    let plan      = compute_sync_plan(&LocalState::default(), &manifest, &OptionalChoices::default());

    assert!(plan.optional_mods_to_download.is_empty());
    assert!(plan.mods_to_download.is_empty());
}

#[tokio::test]
async fn optional_downloaded_when_enabled() {
    let (_f, path) = write_manifest(MANIFEST_WITH_OPTIONAL);
    let provider  = FileProvider::new(path);
    let manifest  = fetch_manifest(&provider, "").await.unwrap();

    let choices = OptionalChoices {
        enabled: vec!["sodium".into()],
        ..Default::default()
    };
    let plan = compute_sync_plan(&LocalState::default(), &manifest, &choices);

    assert_eq!(plan.optional_mods_to_download.len(), 1);
    assert_eq!(plan.optional_mods_to_download[0].id, "sodium");
}

// ── Tests — security: path traversal ─────────────────────────────────────────

#[tokio::test]
async fn fetch_manifest_rejects_path_traversal_in_config_overrides() {
    let (_f, path) = write_manifest(MANIFEST_WITH_TRAVERSAL);
    let provider  = FileProvider::new(path);
    let result    = fetch_manifest(&provider, "").await;

    assert!(result.is_err(), "should reject manifest with traversal paths");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("path") || msg.contains(".."),
        "error should mention the invalid path, got: {msg}"
    );
}

// ── Tests — manifest fields ───────────────────────────────────────────────────

#[tokio::test]
async fn manifest_fields_parsed_correctly() {
    let (_f, path) = write_manifest(MANIFEST_TWO_MODS);
    let provider  = FileProvider::new(path);
    let manifest  = fetch_manifest(&provider, "").await.unwrap();

    assert_eq!(manifest.schema_version, 1);
    assert_eq!(manifest.manifest_version, "2026.05.01-1");
    assert_eq!(manifest.minecraft.version, "1.21.1");
    assert_eq!(manifest.minecraft.java_version, 21);
    assert_eq!(manifest.required_mods.len(), 2);
    assert!(manifest.loader.is_some());
    assert_eq!(manifest.loader.unwrap().version, "21.1.95");
}

#[tokio::test]
async fn unsigned_manifest_accepted_when_no_pubkey() {
    let (_f, path) = write_manifest(MANIFEST_TWO_MODS);
    let provider  = FileProvider::new(path);
    // Empty public key → accept unsigned manifests
    assert!(fetch_manifest(&provider, "").await.is_ok());
}

#[tokio::test]
async fn unsigned_manifest_rejected_when_pubkey_configured() {
    let (_f, path) = write_manifest(MANIFEST_TWO_MODS);
    let provider  = FileProvider::new(path);
    // Any non-empty pubkey → must reject unsigned manifest
    let result = fetch_manifest(&provider, "deadbeefdeadbeef").await;
    assert!(result.is_err());
}
