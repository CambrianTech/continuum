//! A crate is tested and audited by construction, never by a reviewer's checklist.
//!
//! A crate cut out of `continuum-core` must be named in four lists, or part of it goes
//! unwatched while every check stays green:
//! 1. `tools/scripts/ci-change-scope.cjs` `TESTED_CRATES`, or CI never runs its tests;
//! 2. a job in `.github/workflows/continuum-rust-tests.yml` (its own `matrix.crate == '…'`
//!    steps, or the generic allowlist), or the selected crate runs nothing;
//! 3. [`crate::MODULE_CRATES`] / `SPLIT_CRATES`, or every per-file ratchet stops seeing its
//!    files and their counts fall for no change;
//! 4. `scripts/ratchets/check-rust-warnings.sh`, or its warnings go uncounted.
//!
//! Review caught a missing entry on 2026-10-26, twice: #5456 (continuum-dreaming, all four)
//! and #5450 (continuum-sentinel: 122 tests in no CI job, and two `loose_id_guard` entries
//! that vanished because their files left the scan, not because the code changed). Card
//! 85138cb9. This rule reads the four lists and the workspace, so the next cut fails in
//! seconds instead.

use std::collections::BTreeSet;
use std::path::Path;

/// Workspace crates with tests that the crate matrix does not select, each with the reason.
/// The list only shrinks: a crate leaves it when it joins `TESTED_CRATES`.
pub const UNSELECTED_WITH_TESTS: &[(&str, &str)] = &[
    ("airc-test-fixtures", "pending: never selected by any CI job (card 996e3537)"),
    ("continuum-airc-protocol", "pending: never selected by any CI job (card 996e3537)"),
    ("continuum-cli-lifecycle", "its own job: continuum-rust-tests.yml runs `cargo test -p continuum-cli-lifecycle`"),
    ("continuum-context", "pending: never selected by any CI job (card 996e3537)"),
    ("continuum-orm-derive", "pending: never selected by any CI job (card 996e3537)"),
    ("continuum-positron", "pending: never selected by any CI job (card 996e3537)"),
    ("continuum-text-analysis", "pending: never selected by any CI job (card 996e3537)"),
    ("continuum-tool-protocol", "pending: never selected by any CI job (card 996e3537)"),
    ("expert-pager-policy", "pending: never selected by any CI job (card 996e3537)"),
    ("inference-grpc", "pending: never selected by any CI job (card 996e3537)"),
    ("livekit-protocol", "pending: never selected by any CI job (card 996e3537)"),
    ("llama", "pending: never selected by any CI job (card 996e3537)"),
    ("serving-replay", "pending: never selected by any CI job (card 996e3537)"),
    ("source-hygiene", "its own workflow: source-hygiene.yml runs this crate's tests against the base tree"),
    ("tool-working-set", "pending: never selected by any CI job (card 996e3537)"),
];

/// Selected crates that keep their own audit outside core's ratchets, each with the reason.
pub const AUDITED_ELSEWHERE: &[(&str, &str)] = &[
    ("continuum-host", "pending: in no per-file ratchet yet; add it to SPLIT_CRATES (card 996e3537)"),
];

/// Audited crates whose warnings the warning ratchet does not count yet, each with the reason.
pub const WARNINGS_UNCOUNTED: &[(&str, &str)] = &[
    ("continuum-kernel", "pending: ~13 warnings outside the count; fix them, then count it (card 996e3537)"),
];

/// The quoted `continuum-…`/crate names inside `text[start..]` up to the first `end`.
fn quoted_names(text: &str, start: &str, end: &str) -> Option<BTreeSet<String>> {
    let from = text.find(start)? + start.len();
    let body = &text[from..from + text[from..].find(end)?];
    Some(
        body.split(['\'', '"'])
            .skip(1)
            .step_by(2)
            .filter(|name| !name.is_empty())
            .map(str::to_string)
            .collect(),
    )
}

/// `TESTED_CRATES` from the selector script.
pub fn tested_crates(selector: &str) -> Option<BTreeSet<String>> {
    quoted_names(selector, "const TESTED_CRATES = [", "];")
}

/// Every crate the test workflow has a job for: its own `matrix.crate == 'x'` steps, or the
/// generic `contains(fromJSON('[…]'), matrix.crate)` allowlist.
pub fn workflow_crates(workflow: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in workflow.lines() {
        if let Some((_, rest)) = line.split_once("matrix.crate == '") {
            if let Some((name, _)) = rest.split_once('\'') {
                out.insert(name.to_string());
            }
        }
        if line.contains("matrix.crate)") {
            if let Some(names) = quoted_names(line, "fromJSON('[", "]'") {
                out.extend(names);
            }
        }
    }
    out
}

/// The crates whose warnings the warning ratchet counts (hyphenated spelling).
pub fn warning_crates(script: &str) -> BTreeSet<String> {
    script
        .lines()
        .filter(|line| line.contains(".get(\"name\") not in ("))
        .flat_map(|line| line.split('"').skip(1).step_by(2).map(str::to_string).collect::<Vec<_>>())
        .filter(|name| name.contains('-'))
        .collect()
}

/// The workspace member directories under `core/`, read from the root manifest.
pub fn core_members(manifest: &str) -> BTreeSet<String> {
    manifest
        .lines()
        .map(|line| line.split('#').next().unwrap_or("").trim()) // unwrap_or: split always yields one piece
        .filter_map(|line| line.strip_prefix("\"core/"))
        .filter_map(|rest| rest.split_once('"').map(|(name, _)| name.to_string()))
        .collect()
}

/// Whether a crate's `src/` declares any test.
pub fn has_tests(src: &Path) -> bool {
    crate::collect_files(src).iter().any(|file| file.raw.contains("#[test]") || file.raw.contains("#[tokio::test"))
}

/// Every wiring gap, as one message per missing entry.
pub fn gaps(repo: &Path) -> Result<Vec<String>, String> {
    let read = |rel: &str| std::fs::read_to_string(repo.join(rel)).map_err(|e| format!("{rel}: {e}"));
    let tested = tested_crates(&read("tools/scripts/ci-change-scope.cjs")?).ok_or("no TESTED_CRATES in the selector")?;
    let jobs = workflow_crates(&read(".github/workflows/continuum-rust-tests.yml")?);
    let warned = warning_crates(&read("scripts/ratchets/check-rust-warnings.sh")?);
    let members = core_members(&read("Cargo.toml")?);
    if tested.is_empty() || jobs.is_empty() || warned.is_empty() || members.is_empty() {
        return Err(format!(
            "a list read empty (tested {}, jobs {}, warned {}, members {}): the parse no longer matches its file",
            tested.len(), jobs.len(), warned.len(), members.len()
        ));
    }
    let audited: BTreeSet<&str> = crate::MODULE_CRATES.iter().chain(crate::SPLIT_CRATES).copied().collect();
    let exempt = |list: &[(&str, &str)], name: &str| list.iter().any(|(crate_name, _)| *crate_name == name);
    let mut out = Vec::new();
    for name in &members {
        if !tested.contains(name) && !exempt(UNSELECTED_WITH_TESTS, name) && has_tests(&repo.join("core").join(name).join("src")) {
            out.push(format!("{name} has tests but is not in TESTED_CRATES (tools/scripts/ci-change-scope.cjs): CI never runs them"));
        }
    }
    for name in &tested {
        if !jobs.contains(name) {
            out.push(format!("{name} is selected but has no job in continuum-rust-tests.yml (own steps or the generic allowlist)"));
        }
        if name == crate::AUDITED_CRATE || exempt(AUDITED_ELSEWHERE, name) {
            continue;
        }
        if !audited.contains(name.as_str()) {
            out.push(format!("{name} is not in source_hygiene::MODULE_CRATES or SPLIT_CRATES: the per-file ratchets do not see its files"));
        }
    }
    for name in std::iter::once(crate::AUDITED_CRATE).chain(crate::MODULE_CRATES.iter().copied()) {
        if !warned.contains(name) && !exempt(WARNINGS_UNCOUNTED, name) {
            out.push(format!("{name} is audited but not counted by scripts/ratchets/check-rust-warnings.sh"));
        }
    }
    // an exemption whose gap is closed must go, or the list stops meaning anything
    for (name, _) in UNSELECTED_WITH_TESTS {
        if tested.contains(*name) {
            out.push(format!("{name} is selected now: delete its UNSELECTED_WITH_TESTS entry"));
        }
    }
    for (name, _) in AUDITED_ELSEWHERE {
        if audited.contains(name) {
            out.push(format!("{name} is audited now: delete its AUDITED_ELSEWHERE entry"));
        }
    }
    for (name, _) in WARNINGS_UNCOUNTED {
        if warned.contains(*name) {
            out.push(format!("{name} is counted now: delete its WARNINGS_UNCOUNTED entry"));
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a crate cut from core that is missing from any of the four lists
    // (#5456, #5450): its tests unrun, its files unaudited, or its warnings uncounted.
    #[test]
    fn every_crate_is_tested_audited_and_counted() {
        let Some(repo) = crate::repo_root() else {
            panic!("no repository root above the audited crate: the lists cannot be read");
        };
        let gaps = gaps(&repo).unwrap_or_else(|e| panic!("{e}"));
        assert!(gaps.is_empty(), "crate wiring gaps:\n  {}", gaps.join("\n  "));
    }

    // what this catches: a parser drifting from its file's shape, so a list reads empty or
    // partial and every gap goes silent.
    #[test]
    fn each_list_is_read_in_the_shape_its_file_writes_it() {
        let selector = "const TESTED_CRATES = ['continuum-core', 'continuum-mind',\n  'continuum-dreaming'];\nconst X = ['other'];";
        assert_eq!(tested_crates(selector).unwrap(), ["continuum-core", "continuum-dreaming", "continuum-mind"].map(String::from).into());
        let workflow = "        if: matrix.crate == 'continuum-core'\n        if: contains(fromJSON('[\"continuum-hands\",\"continuum-live\"]'), matrix.crate)\n";
        assert_eq!(workflow_crates(workflow), ["continuum-core", "continuum-hands", "continuum-live"].map(String::from).into());
        let script = "    if (rec.get(\"target\") or {}).get(\"name\") not in (\"continuum-core\", \"continuum_core\", \"continuum-mind\", \"continuum_mind\"):";
        assert_eq!(warning_crates(script), ["continuum-core", "continuum-mind"].map(String::from).into());
        let manifest = "members = [\n    # core/ — substrate\n    \"core/continuum-base\",\n    # \"core/archive\" removed\n    \"core/llama\",\n    \"apps/cli\",\n]";
        assert_eq!(core_members(manifest), ["continuum-base", "llama"].map(String::from).into());
    }
}
