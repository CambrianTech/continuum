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

use crate::crate_policy::{checkout_policy, has_lib, modules, read_members, split, tested, warned, Member, Policy};

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

/// The most crates `pending` may name: debt only shrinks. Lower it when an entry leaves.
pub const PENDING_MAX: usize = 12;

/// Gaps in the exceptions themselves: a bin-only crate in the default tested set (no lib target,
/// so `cargo test -p <crate> --lib` has nothing to build), a pending crate with no tests (that is
/// by nature, not debt), and a pending list that grew.
pub fn policy_gaps(repo: &Path, members: &[Member], policy: &Policy) -> Vec<String> {
    let mut out = Vec::new();
    for m in tested(members, policy) {
        if !has_lib(repo, &m.dir) {
            out.push(format!("{} is bin-only (no lib target) but in the default tested set: `cargo test -p {0} --lib` cannot run. Name it in [workspace.metadata.ci] by_nature with the reason, or give it a lib", m.name));
        }
    }
    for m in members.iter().filter(|m| policy.pending.contains(&m.name)) {
        if !has_tests(repo, &m.dir) {
            out.push(format!("{} is in pending but has no tests: it is by_nature, not debt", m.name));
        }
    }
    if policy.pending.len() > PENDING_MAX {
        out.push(format!("pending names {} crates, over the ceiling {PENDING_MAX}: pending only shrinks. Select the crate in CI (delete it from the lists) or put it in by_nature with a reason", policy.pending.len()));
    }
    out
}

/// Whether the crate at `core/<dir>` declares any test in its `src/`.
pub fn has_tests(repo: &Path, dir: &str) -> bool {
    crate::collect_files(repo, &format!("core/{dir}/src")).iter().any(|file| file.raw.contains("#[test]") || file.raw.contains("#[tokio::test"))
}

/// The quoted strings of `"key":[...]` in the selector's `crate-sets` JSON line.
pub fn json_names(json: &str, key: &str) -> BTreeSet<String> {
    let Some((_, rest)) = json.split_once(&format!("\"{key}\":[")) else {
        return BTreeSet::new();
    };
    rest.split(']').next().unwrap_or("").split('"').skip(1).step_by(2).map(str::to_string).collect() // unwrap_or: split always yields one piece
}

/// The selector's derivation, run through node: the CI side of the same rule.
fn selector_sets(repo: &Path) -> Result<String, String> {
    let out = std::process::Command::new("node")
        .arg("tools/scripts/ci-change-scope.cjs")
        .arg("crate-sets")
        .current_dir(repo)
        .output()
        .map_err(|e| format!("node is required to check the selector against the Rust derivation (CI installs it): {e}"))?;
    if !out.status.success() {
        return Err(format!("ci-change-scope.cjs crate-sets failed: {}", String::from_utf8_lossy(&out.stderr)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Every wiring gap, as one message per problem.
pub fn gaps(repo: &Path) -> Result<Vec<String>, String> {
    let read = |rel: &str| std::fs::read_to_string(repo.join(rel)).map_err(|e| format!("{rel}: {e}"));
    let policy = checkout_policy().map_err(|e| e.to_string())?;
    let members = read_members(repo).map_err(|e| e.to_string())?;
    let workflow = read(".github/workflows/continuum-rust-tests.yml")?;
    let script = read("scripts/ratchets/check-rust-warnings.sh")?;
    let selector = read("tools/scripts/ci-change-scope.cjs")?;
    let known: BTreeSet<&str> = members.iter().map(|m| m.name.as_str()).collect();
    let tested_names: BTreeSet<&str> = tested(&members, &policy).iter().map(|m| m.name.as_str()).collect();
    let mut out = Vec::new();

    let named = [
        ("by_nature", &policy.by_nature),
        ("pending", &policy.pending),
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
    out.extend(policy_gaps(repo, &members, &policy));
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
    // The two derivers (this crate's and the selector's) must agree, or the rule lives twice.
    let json = selector_sets(repo)?;
    let rust_underscored = |names: Vec<&crate::crate_policy::Member>| -> BTreeSet<String> {
        names.iter().flat_map(|m| [m.name.clone(), m.name.replace('-', "_")]).collect()
    };
    let rust_dirs = |names: Vec<&crate::crate_policy::Member>| -> BTreeSet<String> { names.iter().map(|m| m.dir.clone()).collect() };
    let checks = [
        ("tested", tested(&members, &policy).iter().map(|m| m.name.clone()).collect::<BTreeSet<_>>()),
        ("warned", rust_underscored(warned(&members, &policy))),
        ("split", rust_dirs(split(&members, &policy))),
        ("modules", rust_dirs(modules(&members, &policy))),
    ];
    for (key, rust) in checks {
        let node = json_names(&json, key);
        if node != rust {
            out.push(format!("ci-change-scope.cjs and source-hygiene derive different `{key}` crates: selector {node:?}, rust {rust:?}"));
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

    // what this catches: a bin-only crate dropped into the workspace, which the derivation would
    // select for a lib-test job that cannot run, and a pending list that grows or hides a no-test crate.
    #[test]
    fn a_bin_only_crate_and_a_growing_pending_list_are_named_gaps() {
        let repo = std::env::temp_dir().join(format!("wiring-bin-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&repo);
        for (dir, lib) in [("has-lib", true), ("only-bin", false)] {
            let src = repo.join("core").join(dir).join("src");
            std::fs::create_dir_all(&src).unwrap_or_else(|e| panic!("{e}"));
            std::fs::write(repo.join("core").join(dir).join("Cargo.toml"), format!("[package]\nname = \"{dir}\"\n")).unwrap_or_else(|e| panic!("{e}"));
            std::fs::write(src.join(if lib { "lib.rs" } else { "main.rs" }), "fn f() {}\n").unwrap_or_else(|e| panic!("{e}"));
        }
        let members: Vec<Member> = ["has-lib", "only-bin"].map(|d| Member { name: d.into(), dir: d.into() }).into();
        let mut policy = Policy::default();
        let gaps = policy_gaps(&repo, &members, &policy);
        assert_eq!(gaps.len(), 1, "{gaps:?}");
        assert!(gaps[0].contains("only-bin") && gaps[0].contains("by_nature"), "{gaps:?}");
        policy.by_nature.insert("only-bin".into());
        policy.untested = policy.by_nature.clone();
        assert!(policy_gaps(&repo, &members, &policy).is_empty());
        policy.pending.insert("has-lib".into()); // no #[test] in it: misfiled
        assert!(policy_gaps(&repo, &members, &policy).iter().any(|g| g.contains("has no tests")));
        policy.pending.extend((0..PENDING_MAX).map(|i| format!("p{i}")));
        assert!(policy_gaps(&repo, &members, &policy).iter().any(|g| g.contains("ceiling")));
        let _ = std::fs::remove_dir_all(&repo);
    }

    // what this catches: the two readers drifting from the shapes their files use, so a list
    // reads empty and every gap goes silent.
    #[test]
    fn the_readers_match_the_shapes_their_files_use() {
        let workflow = "        if: matrix.crate == 'continuum-core'\n        if: matrix.crate == 'continuum-mind'\n";
        assert_eq!(own_step_crates(workflow), ["continuum-core", "continuum-mind"].map(String::from).into());
        assert_eq!(hand_listed_crates("if: contains(fromJSON('[\"continuum-hands\",\"continuum-live\"]'), matrix.crate)").len(), 1);
        assert_eq!(json_names("{\"tested\":[\"a\",\"b\"],\"split\":[]}", "tested"), ["a", "b"].map(String::from).into());
        assert!(json_names("{\"split\":[]}", "split").is_empty());
        assert!(hand_listed_crates("    # \"continuum-a\" \"continuum-b\" in a comment\n  const x = 'continuum-core';").is_empty());
    }
}
