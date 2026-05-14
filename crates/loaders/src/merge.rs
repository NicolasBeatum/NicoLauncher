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
    use launcher_meta::types::{Arguments, Argument, Library};

    // ── group_artifact ────────────────────────────────────────────────────────

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

    // ── helpers ───────────────────────────────────────────────────────────────

    fn lib(name: &str) -> Library {
        Library {
            name: name.to_string(),
            downloads: None,
            rules: None,
            natives: None,
            extract: None,
            url: None,
        }
    }

    fn plain_args(jvm: &[&str], game: &[&str]) -> Arguments {
        Arguments {
            jvm:  jvm.iter().map(|s|  Argument::Plain(s.to_string())).collect(),
            game: game.iter().map(|s| Argument::Plain(s.to_string())).collect(),
        }
    }

    fn minimal_version_json(main_class: &str, libs: &[&str]) -> VersionJson {
        let json = format!(
            r#"{{
              "id": "1.21.1",
              "mainClass": "{main_class}",
              "assetIndex": {{ "id": "17", "sha1": "abc", "size": 0, "url": "" }},
              "assets": "17",
              "downloads": {{ "client": {{ "sha1": "abc", "size": 0, "url": "" }} }},
              "libraries": [{libs_json}],
              "type": "release"
            }}"#,
            libs_json = libs
                .iter()
                .map(|n| format!(r#"{{"name":"{n}"}}"#))
                .collect::<Vec<_>>()
                .join(",")
        );
        serde_json::from_str(&json).unwrap()
    }

    // ── merge() tests ─────────────────────────────────────────────────────────

    #[test]
    fn merge_replaces_main_class() {
        let mut base = minimal_version_json("net.minecraft.client.main.Main", &[]);
        let profile = LoaderProfile {
            main_class: "net.fabricmc.loader.impl.launch.knot.KnotClient".to_string(),
            libraries: vec![],
            arguments: None,
        };
        merge(&mut base, &profile);
        assert_eq!(base.main_class, "net.fabricmc.loader.impl.launch.knot.KnotClient");
    }

    #[test]
    fn merge_loader_lib_overrides_mojang_lib_same_ga() {
        // Mojang has guava 30.1, loader upgrades it to 32.1
        let mut base = minimal_version_json("Main", &["com.google.guava:guava:30.1"]);
        let profile = LoaderProfile {
            main_class: "Loader".into(),
            libraries: vec![lib("com.google.guava:guava:32.1.2-jre")],
            arguments: None,
        };
        merge(&mut base, &profile);
        assert_eq!(base.libraries.len(), 1, "should have exactly 1 guava entry");
        assert_eq!(base.libraries[0].name, "com.google.guava:guava:32.1.2-jre");
    }

    #[test]
    fn merge_loader_only_lib_is_added() {
        let mut base = minimal_version_json("Main", &["com.google.guava:guava:30.1"]);
        let profile = LoaderProfile {
            main_class: "Loader".into(),
            libraries: vec![lib("net.fabricmc:fabric-loader:0.16.0")],
            arguments: None,
        };
        merge(&mut base, &profile);
        assert_eq!(base.libraries.len(), 2);
        let names: Vec<_> = base.libraries.iter().map(|l| l.name.as_str()).collect();
        assert!(names.contains(&"com.google.guava:guava:30.1"));
        assert!(names.contains(&"net.fabricmc:fabric-loader:0.16.0"));
    }

    #[test]
    fn merge_mojang_only_lib_is_kept() {
        let mut base = minimal_version_json("Main", &[
            "com.google.guava:guava:30.1",
            "org.ow2.asm:asm:9.6",
        ]);
        let profile = LoaderProfile {
            main_class: "Loader".into(),
            // Loader only adds its own lib, doesn't touch asm
            libraries: vec![lib("net.fabricmc:fabric-loader:0.16.0")],
            arguments: None,
        };
        merge(&mut base, &profile);
        let names: Vec<_> = base.libraries.iter().map(|l| l.name.as_str()).collect();
        assert!(names.contains(&"org.ow2.asm:asm:9.6"), "Mojang-only lib should be kept");
    }

    #[test]
    fn merge_args_appended_after_mojang_args() {
        let mut base = minimal_version_json("Main", &[]);
        base.arguments = Some(plain_args(&["-Xss1M"], &["--username", "${auth_player_name}"]));

        let profile = LoaderProfile {
            main_class: "Loader".into(),
            libraries: vec![],
            arguments: Some(plain_args(&["-Dfabric.gameJarPath=${primary_jar}"], &[])),
        };
        merge(&mut base, &profile);

        let args = base.arguments.unwrap();
        assert_eq!(args.jvm.len(), 2, "should have Mojang + loader JVM arg");
        // Mojang's arg must come first
        assert!(matches!(&args.jvm[0], Argument::Plain(s) if s == "-Xss1M"));
        // Loader's arg appended
        assert!(matches!(&args.jvm[1], Argument::Plain(s) if s.contains("gameJarPath")));
        // Mojang's game arg preserved
        assert_eq!(args.game.len(), 2);
    }

    #[test]
    fn merge_with_no_base_args_uses_loader_args() {
        let mut base = minimal_version_json("Main", &[]);
        // base has no arguments block at all

        let profile = LoaderProfile {
            main_class: "Loader".into(),
            libraries: vec![],
            arguments: Some(plain_args(&["-Dfoo=bar"], &["--gameDir", "${game_directory}"])),
        };
        merge(&mut base, &profile);
        let args = base.arguments.unwrap();
        assert_eq!(args.jvm.len(), 1);
        assert_eq!(args.game.len(), 2);
    }

    #[test]
    fn merge_classifiers_both_kept() {
        // Two Forge libs with same GA but different classifiers must both be present
        let mut base = minimal_version_json("Main", &[
            "net.minecraftforge:forge:1.21.1-52.1.0:universal",
        ]);
        let profile = LoaderProfile {
            main_class: "Loader".into(),
            libraries: vec![lib("net.minecraftforge:forge:1.21.1-52.1.0:client")],
            arguments: None,
        };
        merge(&mut base, &profile);
        // Both classifiers must survive
        assert_eq!(base.libraries.len(), 2);
        let names: Vec<_> = base.libraries.iter().map(|l| l.name.as_str()).collect();
        assert!(names.iter().any(|n| n.contains("universal")));
        assert!(names.iter().any(|n| n.contains("client")));
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
