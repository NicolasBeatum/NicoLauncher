use launcher_meta::types::{Rule, RuleAction};

/// Evaluate a list of rules and return true if the current environment passes.
/// The last matching rule wins; if no rules match, allow by default.
pub fn eval_rules(rules: &[Rule]) -> bool {
    if rules.is_empty() {
        return true;
    }

    let mut allowed = false;
    for rule in rules {
        if rule_matches(rule) {
            allowed = rule.action == RuleAction::Allow;
        }
    }
    allowed
}

fn rule_matches(rule: &Rule) -> bool {
    // Feature rules (e.g., demo mode, custom resolution) — we don't support features in Phase 1
    if rule.features.is_some() {
        return false;
    }

    match &rule.os {
        None => true,
        Some(os) => {
            let name_ok = os.name.as_deref().map_or(true, |name| {
                match name {
                    "windows" => cfg!(target_os = "windows"),
                    "osx"     => cfg!(target_os = "macos"),
                    "linux"   => cfg!(target_os = "linux"),
                    _         => false,
                }
            });

            let arch_ok = os.arch.as_deref().map_or(true, |arch| {
                match arch {
                    "x86"   => cfg!(target_arch = "x86"),
                    "x86_64"| "amd64" => cfg!(target_arch = "x86_64"),
                    "aarch64"| "arm64" => cfg!(target_arch = "aarch64"),
                    _ => false,
                }
            });

            // Version regex matching — skip for now (rarely used)
            name_ok && arch_ok
        }
    }
}
