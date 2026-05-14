use launcher_meta::types::{Arguments, Library, VersionJson};

/// Merge a loader's profile JSON into the base Mojang VersionJson.
///
/// Rules (from spec §7.5):
///   1. Loader libraries override Mojang ones with the same group:artifact.
///   2. Loader's mainClass replaces Mojang's.
///   3. JVM + game args: Mojang first, then loader appended.
///   4. asset_index, java_version, client_jar: from Mojang (untouched).
pub fn merge(base: &mut VersionJson, loader_profile: &LoaderProfile) {
    // 1. Merge libraries
    merge_libraries(&mut base.libraries, &loader_profile.libraries);

    // 2. mainClass
    base.main_class = loader_profile.main_class.clone();

    // 3. Arguments
    match (&mut base.arguments, &loader_profile.arguments) {
        (Some(base_args), Some(loader_args)) => {
            base_args.jvm.extend(loader_args.jvm.iter().cloned());
            base_args.game.extend(loader_args.game.iter().cloned());
        }
        (None, Some(loader_args)) => {
            base.arguments = Some(loader_args.clone());
        }
        _ => {}
    }
}

fn merge_libraries(base: &mut Vec<Library>, loader_libs: &[Library]) {
    for loader_lib in loader_libs {
        let ga = group_artifact(&loader_lib.name);
        if let Some(pos) = base.iter().position(|b| group_artifact(&b.name) == ga) {
            base[pos] = loader_lib.clone();
        } else {
            base.push(loader_lib.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifiers_are_not_deduplicated() {
        assert_ne!(
            group_artifact("net.minecraftforge:forge:1.21.1-52.1.0:universal"),
            group_artifact("net.minecraftforge:forge:1.21.1-52.1.0:client"),
        );
    }

    #[test]
    fn same_ga_different_versions_deduplicates() {
        assert_eq!(
            group_artifact("com.google.guava:guava:30.1"),
            group_artifact("com.google.guava:guava:32.1.2-jre"),
        );
    }
}

/// Extract a deduplication key from a Maven coordinate.
///
/// Format: `group:artifact:version[:classifier]`
/// Key:    `group:artifact[:classifier]`   (version is ignored, classifier is kept)
///
/// Two libraries with the SAME group:artifact but DIFFERENT classifiers (e.g.
/// `forge:1.21.1:universal` vs `forge:1.21.1:client`) must NOT replace each
/// other — they are distinct JARs and must both be present on the classpath.
fn group_artifact(name: &str) -> String {
    let parts: Vec<&str> = name.splitn(4, ':').collect();
    match parts.as_slice() {
        // group:artifact:version:classifier  →  "group:artifact:classifier"
        [g, a, _v, c] => format!("{g}:{a}:{c}"),
        // group:artifact:version             →  "group:artifact"
        [g, a, _v]    => format!("{g}:{a}"),
        // group:artifact (no version)        →  "group:artifact"
        [g, a]        => format!("{g}:{a}"),
        // anything else                      →  the full name
        _             => name.to_string(),
    }
}

/// Loader-specific JSON that will be merged into the base VersionJson.
/// This is a subset of VersionJson fields that loaders can override.
#[derive(Debug, Clone)]
pub struct LoaderProfile {
    pub main_class: String,
    pub libraries: Vec<Library>,
    pub arguments: Option<Arguments>,
}
