//! A crate is tested and audited by construction, never by a reviewer's checklist.
//!
//! Every `core/` workspace member is tested (CI), audited (per-file ratchets) and counted
//! (warnings ratchet) unless the root `Cargo.toml` `[workspace.metadata.ci]` names it as an
//! exception, so cutting a crate is one `members` line ([`crate::crate_policy`]). Before this,
//! a cut had to edit four hand-kept lists and review caught a missed entry twice on
//! 2026-10-26: #5456 (continuum-dreaming, all four) and #5450 (continuum-sentinel: 122 tests
//! in no CI job, and two `loose_id_guard` entries that vanished because their files left the
//! scan). Card 85138cb9.
//!
//! This rule is the guard that the derivation stays the only source: no consumer may grow a
//! hand-kept crate list again, every exception names a real crate, and every crate the
//! workflow gives its own steps is one the policy says has them.

use std::collections::BTreeSet;
use std::path::Path;

use crate::crate_policy::{read_members, tested, parse_policy};

/// The crates named by `matrix.crate == 'x'` in the test workflow: its own per-crate steps.
pub fn own_step_crates(workflow: &str) -> BTreeSet<String> {
    workflow
        .lines()
        .filter_map(|line| line.split_once("matrix.crate == '"))
        .filter_map(|(_, rest)| rest.split_once('\'').map(|(name, _)| name.to_string()))
        .collect()
}

/// Whether `text` hand-lists crates: a quoted `continuum-…` name inside a bracketed or
/// parenthesised list of two or more quoted names on one line.
pub fn hand_listed_crates(text: &str) -> Vec<String> {
    text.lines()
        .filter(|line| !line.trim_start().starts_with(['#', '/']) || line.contains("fromJSON('[") )
        .filter(|line| line.matches("\"continuum-").count() + line.matches("'continuum-").count() >= 2)
        .map(|line| line.trim().chars().take(100).collect())
        .collect()
}

/// Every wiring gap, as one message per problem.
pub fn gaps(repo: &Path) -> Result<Vec<String>, String> {
    let read = |rel: &str| std::fs::read_to_string(repo.join(rel)).map_err(|e| format!("{rel}: {e}"));
    let root = read("Cargo.toml")?;
    let policy = parse_policy(&root)?;
    let members = read_members(repo)?;
    let workflow = read(".github/workflows/continuum-rust-tests.yml")?;
    let script = read("scripts/ratchets/check-rust-warnings.sh")?;
    let selector = read("tools/scripts/ci-change-scope.cjs")?;
    let known: BTreeSet<&str> = members.iter().map(|m| m.name.as_str()).collect();
    let tested_names: BTreeSet<&str> = tested(&members, &policy).iter().map(|m| m.name.as_str()).collect();
    let mut out = Vec::new();

    let named = [
        ("untested", &policy.untested),
        ("own_lib_test_step", &policy.own_lib_test_step),
        ("split_prefixed", &policy.split_prefixed),
        ("unaudited", &policy.unaudited),
        ("warnings_uncounted", &policy.warnings_uncounted),
    ];
    for (key, names) in named {
        for name in names.iter().filter(|n| !known.contains(n.as_str())) {
            out.push(format!("[workspace.metadata.ci] {key} names {name}, which is not a core/ workspace crate: delete the stale entry"));
        }
    }
    for name in policy.own_lib_test_step.iter().filter(|n| !tested_names.contains(n.as_str())) {
        out.push(format!("{name} has own lib-test steps but is not a tested crate"));
    }
    let own = own_step_crates(&workflow);
    for name in &policy.own_lib_test_step {
        if !own.contains(name) {
            out.push(format!("{name} is in own_lib_test_step but continuum-rust-tests.yml has no `matrix.crate == '{name}'` step: it runs no lib tests"));
        }
    }
    for name in own.iter().filter(|n| !tested_names.contains(n.as_str())) {
        out.push(format!("continuum-rust-tests.yml has steps for {name}, which is not a tested crate"));
    }
    for (file, text) in [
        ("continuum-rust-tests.yml", &workflow),
        ("check-rust-warnings.sh", &script),
        ("ci-change-scope.cjs", &selector),
    ] {
        for line in hand_listed_crates(text) {
            out.push(format!("{file} lists crates by hand again (derive from the workspace instead): {line}"));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a hand-kept crate list creeping back into a consumer, or a policy
    // exception that names a crate that is gone (#5456, #5450).
    #[test]
    fn crate_lists_are_derived_and_the_policy_names_real_crates() {
        let Some(repo) = crate::repo_root() else {
            panic!("no repository root above the audited crate: the lists cannot be read");
        };
        let gaps = gaps(&repo).unwrap_or_else(|e| panic!("{e}"));
        assert!(gaps.is_empty(), "crate wiring gaps:\n  {}", gaps.join("\n  "));
    }

    // what this catches: the two readers drifting from the shapes their files use, so a list
    // reads empty and every gap goes silent.
    #[test]
    fn the_readers_match_the_shapes_their_files_use() {
        let workflow = "        if: matrix.crate == 'continuum-core'\n        if: matrix.crate == 'continuum-mind'\n";
        assert_eq!(own_step_crates(workflow), ["continuum-core", "continuum-mind"].map(String::from).into());
        assert_eq!(hand_listed_crates("if: contains(fromJSON('[\"continuum-hands\",\"continuum-live\"]'), matrix.crate)").len(), 1);
        assert!(hand_listed_crates("    # \"continuum-a\" \"continuum-b\" in a comment\n  const x = 'continuum-core';").is_empty());
    }
}
