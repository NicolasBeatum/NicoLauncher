use std::path::{Path, PathBuf};
use crate::Result;

/// All filesystem paths used by one launcher installation.
/// Root: `%APPDATA%/<internal_id>` on Windows,
///       `~/Library/Application Support/<internal_id>` on macOS,
///       `~/.local/share/<internal_id>` on Linux.
#[derive(Debug, Clone)]
pub struct LauncherPaths {
    pub root: PathBuf,
    pub minecraft: PathBuf,
    pub mods: PathBuf,
    pub cache: PathBuf,
    pub libraries: PathBuf,
    pub assets: PathBuf,
    pub asset_indexes: PathBuf,
    pub asset_objects: PathBuf,
    pub mod_files: PathBuf,
    pub loader_installers: PathBuf,
    pub manifest_cache: PathBuf,
    pub java: PathBuf,
    pub optional_mods: PathBuf,
    pub logs: PathBuf,
    pub natives: PathBuf,
}

impl LauncherPaths {
    pub fn new(internal_id: &str) -> crate::Result<Self> {
        let data_dir = dirs::data_dir()
            .ok_or_else(|| crate::Error::Other("Cannot determine user data directory".into()))?;
        let root = data_dir.join(internal_id);
        Ok(Self::from_root(root))
    }

    pub fn from_root(root: PathBuf) -> Self {
        let minecraft      = root.join("minecraft");
        let mods           = minecraft.join("mods");
        let cache          = root.join("cache");
        let libraries      = cache.join("libraries");
        let assets         = cache.join("assets");
        let asset_indexes  = assets.join("indexes");
        let asset_objects  = assets.join("objects");
        let mod_files      = cache.join("mod-files");
        let loader_installers = cache.join("loader-installers");
        let manifest_cache = cache.join("manifest-cache");
        let java           = root.join("java");
        let optional_mods  = root.join("optional-mods");
        let logs           = root.join("logs");
        let natives        = cache.join("natives");

        Self {
            root,
            minecraft,
            mods,
            cache,
            libraries,
            assets,
            asset_indexes,
            asset_objects,
            mod_files,
            loader_installers,
            manifest_cache,
            java,
            optional_mods,
            logs,
            natives,
        }
    }

    /// Create all directories that must exist before the launcher operates.
    pub async fn ensure_all(&self) -> Result<()> {
        let dirs = [
            &self.root,
            &self.minecraft,
            &self.mods,
            &self.cache,
            &self.libraries,
            &self.assets,
            &self.asset_indexes,
            &self.asset_objects,
            &self.mod_files,
            &self.loader_installers,
            &self.manifest_cache,
            &self.java,
            &self.optional_mods,
            &self.logs,
            &self.natives,
        ];
        for dir in &dirs {
            tokio::fs::create_dir_all(dir).await?;
        }
        Ok(())
    }

    /// Maven-style library path: `group/artifact/version/artifact-version.jar`
    pub fn library_path(&self, maven_name: &str) -> Option<PathBuf> {
        maven_to_path(maven_name).map(|rel| self.libraries.join(rel))
    }

    /// CAS path for a mod file: `{mod_files}/{sha512[0..2]}/{sha512}.jar`
    pub fn mod_cas_path(&self, sha512: &str) -> PathBuf {
        self.mod_files.join(&sha512[..2]).join(sha512)
    }
}

/// Convert a Maven coordinate (`group:artifact:version[:classifier]`) to a
/// relative path (`group/artifact/version/artifact-version[-classifier].jar`).
pub fn maven_to_path(name: &str) -> Option<PathBuf> {
    let parts: Vec<&str> = name.splitn(4, ':').collect();
    let (group, artifact, version) = match parts.as_slice() {
        [g, a, v] | [g, a, v, _] => (*g, *a, *v),
        _ => return None,
    };
    let classifier = if parts.len() == 4 { Some(parts[3]) } else { None };

    let group_path = group.replace('.', "/");
    let filename = match classifier {
        Some(c) => format!("{artifact}-{version}-{c}.jar"),
        None    => format!("{artifact}-{version}.jar"),
    };
    Some(Path::new(&group_path).join(artifact).join(version).join(filename))
}
