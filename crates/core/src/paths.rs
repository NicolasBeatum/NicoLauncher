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

    /// Devuelve las rutas específicas de una instancia.
    /// Cada instancia tiene su propio directorio .minecraft y archivos de estado.
    /// Las rutas compartidas (cache, java, logs) siguen siendo las del launcher raíz.
    pub fn instance(&self, instance_id: &str) -> InstancePaths {
        let base = self.root.join("instances").join(instance_id);
        let minecraft = base.join("minecraft");
        InstancePaths {
            base: base.clone(),
            minecraft: minecraft.clone(),
            mods: minecraft.join("mods"),
            optional_mods: minecraft.join("mods-optional"),
            state_file:        base.join("current-state.json"),
            choices_file:      base.join("optional-choices.json"),
            user_mods_state:   base.join("user-mods-enabled.json"),
        }
    }
}

/// Rutas específicas de una instancia/servidor.
#[derive(Debug, Clone)]
pub struct InstancePaths {
    /// Raíz de la instancia: `{launcher_root}/instances/{id}/`
    pub base: PathBuf,
    /// Directorio .minecraft de la instancia
    pub minecraft: PathBuf,
    /// Carpeta mods/ dentro de .minecraft — mods del servidor (gestionados por sync)
    pub mods: PathBuf,
    /// Carpeta mods-optional/ — mods locales del usuario (gestionados por el usuario)
    pub optional_mods: PathBuf,
    /// Archivo de estado de sincronización
    pub state_file: PathBuf,
    /// Elecciones de anuncios descartados, etc.
    pub choices_file: PathBuf,
    /// Lista de mods de usuario actualmente activados (hardlinkeados a mods/)
    pub user_mods_state: PathBuf,
}

impl InstancePaths {
    pub async fn ensure_all(&self) -> crate::Result<()> {
        for dir in [&self.base, &self.minecraft, &self.mods, &self.optional_mods] {
            tokio::fs::create_dir_all(dir).await?;
        }
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── maven_to_path ─────────────────────────────────────────────────────────

    #[test]
    fn maven_simple_coord() {
        let p = maven_to_path("com.google.guava:guava:30.1").unwrap();
        assert_eq!(
            p,
            PathBuf::from("com/google/guava/guava/30.1/guava-30.1.jar")
        );
    }

    #[test]
    fn maven_with_classifier() {
        let p = maven_to_path("net.minecraftforge:forge:1.21.1-52.1.0:universal").unwrap();
        assert_eq!(
            p,
            PathBuf::from("net/minecraftforge/forge/1.21.1-52.1.0/forge-1.21.1-52.1.0-universal.jar")
        );
    }

    #[test]
    fn maven_dotted_group() {
        let p = maven_to_path("org.ow2.asm:asm:9.6").unwrap();
        assert_eq!(
            p,
            PathBuf::from("org/ow2/asm/asm/9.6/asm-9.6.jar")
        );
    }

    #[test]
    fn maven_invalid_too_short_returns_none() {
        assert!(maven_to_path("invalid").is_none());
        assert!(maven_to_path("only:two").is_none());
    }

    #[test]
    fn maven_classifier_not_included_in_group_artifact_path() {
        let with    = maven_to_path("net.sf.jopt-simple:jopt-simple:5.0.4:shaded").unwrap();
        let without = maven_to_path("net.sf.jopt-simple:jopt-simple:5.0.4").unwrap();
        // They share the same dir but different filenames
        assert_ne!(with, without);
        assert!(with.to_str().unwrap().ends_with("-shaded.jar"));
        assert!(!without.to_str().unwrap().contains("shaded"));
    }

    // ── LauncherPaths::from_root ──────────────────────────────────────────────

    #[test]
    fn paths_are_children_of_root() {
        let root  = PathBuf::from("/test/root");
        let paths = LauncherPaths::from_root(root.clone());

        assert_eq!(paths.root, root);
        assert!(paths.minecraft.starts_with(&root));
        assert!(paths.cache.starts_with(&root));
        assert!(paths.mods.starts_with(&paths.minecraft));
        assert!(paths.libraries.starts_with(&paths.cache));
        assert!(paths.mod_files.starts_with(&paths.cache));
        assert!(paths.assets.starts_with(&paths.cache));
    }

    #[test]
    fn mod_cas_path_uses_two_char_prefix() {
        let paths = LauncherPaths::from_root(PathBuf::from("/root"));
        let sha   = "abcdef1234567890";
        let cas   = paths.mod_cas_path(sha);
        // Should be: /root/cache/mod-files/ab/abcdef1234567890
        assert!(cas.starts_with(paths.mod_files));
        assert!(cas.to_str().unwrap().contains("ab"));
        assert!(cas.file_name().unwrap() == sha);
    }

    #[test]
    fn library_path_uses_maven_layout() {
        let paths = LauncherPaths::from_root(PathBuf::from("/root"));
        let p = paths.library_path("com.google.guava:guava:30.1").unwrap();
        assert!(p.starts_with(&paths.libraries));
        assert!(p.to_str().unwrap().contains("guava-30.1.jar"));
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
