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

/// Extract `group:artifact` from a Maven coordinate `group:artifact:version[:classifier]`.
fn group_artifact(name: &str) -> &str {
    let second_colon = name.match_indices(':').nth(1).map(|(i, _)| i);
    match second_colon {
        Some(i) => &name[..i],
        None    => name,
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
