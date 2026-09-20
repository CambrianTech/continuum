//! **One `#[cfg(test)] mod` per file, and the excess count can only go down.**
//!
//! CLAUDE.md rule #1 of the test-infrastructure section, in prose since the
//! 3-mods-in-runtime.rs / 6-mods-in-grid/tests.rs audit — and re-broken anyway,
//! because prose stops nobody (the 2026-08 window added fresh multi-mod files,
//! including one by the operator running this audit). A second test mod in a file
//! is how test surface fragments: each new theme mints a sibling mod instead of a
//! nested `mod theme { use super::*; }` INSIDE the one mod, fixtures stop being
//! shared, and the file's test half stops reading as one suite.
//!
//! What counts: a literal `#[cfg(test)]` attribute whose gated item is a `mod`.
//! A `#[cfg(test)]`-gated helper fn/use/impl mid-file is NOT counted — gating a
//! helper is legitimate; fragmenting the suite is the defect. Same ratchet
//! doctrine as [`super::unwrap_justification`]: a wall would fail forever on the
//! 13 legacy offenders and get ignored; the ratchet has teeth on day one.

use super::{SourceFile, SourceRule, Violation};

/// Excess test mods (the 2nd, 3rd, … in a file) when this guard landed
/// (2026-08-22): 13 files, worst `modules/grid/tests.rs` (6 mods),
/// `runtime/runtime.rs` and `commands/benchmark.rs` (3 each).
///
/// **This number may only ever go DOWN.** Merging a file's mods into one (nested
/// themes inside it are fine) lowers it; raising it to green a build is defeating
/// the guard.
const BASELINE_EXCESS_TEST_MODS: usize = 19;

/// 1-indexed lines of `#[cfg(test)]` attributes that gate a `mod`.
fn test_mod_lines(file: &SourceFile) -> Vec<usize> {
    let lines: Vec<&str> = file.raw.lines().collect();
    let mut out = Vec::new();
    for (i, l) in lines.iter().enumerate() {
        if l.trim() != "#[cfg(test)]" {
            continue;
        }
        // Look ahead past further attributes / comments / blank lines to the gated
        // item; only a `mod` counts (a gated helper is legitimate mid-file).
        for t in lines.iter().skip(i + 1).map(|s| s.trim()) {
            if t.is_empty() || t.starts_with("#[") || t.starts_with("//") {
                continue;
            }
            if t.starts_with("mod ") || t.starts_with("pub mod ") || t.starts_with("pub(crate) mod ")
            {
                out.push(i + 1);
            }
            break;
        }
    }
    out
}

pub struct TestModSingularity;

impl SourceRule for TestModSingularity {
    fn name(&self) -> &'static str {
        "test-mod-singularity"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        test_mod_lines(file)
            .into_iter()
            .skip(1) // the FIRST test mod is the sanctioned one; every further mod is excess
            .map(|line| Violation {
                rule: "test-mod-singularity",
                file: file.rel.clone(),
                line,
                source: "#[cfg(test)] mod — a second test mod in this file".to_string(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_hygiene::scan;

    /// What this catches: a NEW second `#[cfg(test)] mod` landing in any file.
    /// If this fails because you added one: fold your tests into the file's
    /// existing mod (a nested `mod theme_name { use super::*; }` inside it keeps
    /// the grouping). If it fails because you MERGED some, lower the baseline and
    /// take the win.
    #[test]
    fn the_excess_test_mod_count_never_rises() {
        let violations = scan(&[&TestModSingularity]);
        let count = violations.len();
        assert!(
            count <= BASELINE_EXCESS_TEST_MODS,
            "excess test mods rose to {count} (baseline {BASELINE_EXCESS_TEST_MODS}).\n\
             One `#[cfg(test)] mod` per file — new themes nest INSIDE it as \
             `mod theme {{ use super::*; }}`.\nNew offenders:\n{}",
            violations
                .iter()
                .take(10)
                .map(|v| format!("  {}:{}", v.file, v.line))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// What this catches: the detector counting the wrong thing — a gated helper
    /// (legitimate) as a mod, or missing a mod behind stacked attributes. Either
    /// error direction makes the ratchet meaningless.
    #[test]
    fn only_gated_mods_count_and_the_first_is_free() {
        let mk = |src: &str| SourceFile {
            rel: "fake.rs".into(),
            production: String::new(),
            raw: src.into(),
        };
        let rule = TestModSingularity;
        assert_eq!(rule.check(&mk("#[cfg(test)]\nmod tests {}")).len(), 0, "one mod is sanctioned");
        assert_eq!(
            rule.check(&mk("#[cfg(test)]\nmod a {}\n#[cfg(test)]\nmod b {}")).len(),
            1,
            "the second mod is excess"
        );
        assert_eq!(
            rule.check(&mk("#[cfg(test)]\nfn helper() {}\n#[cfg(test)]\nmod tests {}")).len(),
            0,
            "a gated helper is not a mod"
        );
        assert_eq!(
            rule.check(&mk("#[cfg(test)]\n#[allow(dead_code)]\nmod t {}\n#[cfg(test)]\nmod u {}")).len(),
            1,
            "stacked attributes still reach the mod"
        );
    }
}
