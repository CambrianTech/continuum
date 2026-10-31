//! **An OS difference lives in its capability's adapter, never as a branch in the logic.**
//!
//! Joel, 2026-10-24: "The operating systems should only be mostly the diff of that nuance in an
//! adapter not a complete copy"; "The adapters implement only the diff, same with subclasses";
//! "don't just dump shit into utils". `continuum-host` holds one module per capability whose
//! behaviour differs by OS (durable placement, positioned reads, …): the contract and logic once
//! in its `mod.rs`, the differing calls in its `unix.rs`/`windows.rs`. Everywhere else, a
//! `cfg(windows)`, `cfg(unix)` or `cfg(target_os = …)` is a platform branch interleaved with
//! logic, the copy this rule counts. Before `continuum-host` existed, `rg 'cfg\((target_os|windows|unix)'`
//! over core/ on canary c60cf8913 found 419 such lines in 137 files (the core binary's service,
//! install and reboot code alone held 73); this scan's crates are a subset, and its first CI run
//! records the count it holds.
//!
//! The count may only fall. Fix one by naming the capability in `continuum-host` and moving each
//! OS's lines into its adapter, never by hiding the branch behind a macro.
//! `continuum-host` itself is not in this scan's crates, so its adapters are never counted.

use super::{SourceFile, SourceRule, Violation};

pub struct OsBranches;

/// Whether a code line selects code by OS: an attribute (`#[cfg(...)]`, `#[cfg_attr(...)]`) or a
/// `cfg!(...)` expression naming an OS predicate.
fn branches_on_os(code: &str) -> bool {
    let selects = code.contains("cfg(") || code.contains("cfg_attr(") || code.contains("cfg!(");
    selects
        && ["windows", "unix", "target_os", "target_family"]
            .iter()
            .any(|os| code.contains(os))
}

impl SourceRule for OsBranches {
    fn name(&self) -> &'static str {
        "os_branches_in_logic"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        file.production_lines()
            .filter(|(_, l)| {
                let code = crate::lexer::split_code_and_comment(l).0;
                branches_on_os(code)
            })
            .map(|(n, l)| Violation { rule: self.name(), file: file.rel.clone(), line: n, source: l.trim().to_string() })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;

    // what this catches: a new OS branch interleaved with logic anywhere the gates audit. Move
    // the differing calls into the capability's adapter in continuum-host instead.
    #[test]
    fn os_branches_in_logic_never_increase() {
        let violations = scan(&[&OsBranches]);
        let count = violations.len();
        crate::ratchet::hold("os_branches_in_logic", count, || {
            format!(
                "OS branches in logic rose to {count}. An OS difference belongs in its capability's \
                 adapter in continuum-host (mod.rs holds the logic once, unix.rs/windows.rs only \
                 the calls that differ), never interleaved here.\nFirst {} of {count}:\n{}",
                count.min(20),
                violations.iter().take(20).map(|v| format!("  {}:{}  {}", v.file, v.line, v.source)).collect::<Vec<_>>().join("\n")
            )
        });
    }

    // what this catches: the predicate drifting. Every OS selector form is counted; a feature
    // gate, a test gate and a comment are not.
    #[test]
    fn every_os_selector_is_counted_and_nothing_else() {
        let check = |src: &str| OsBranches.check(&SourceFile::for_test("x.rs", src)).len();
        assert_eq!(check("#[cfg(windows)]"), 1);
        assert_eq!(check("#[cfg(not(unix))]"), 1);
        assert_eq!(check("#[cfg(target_os = \"macos\")]"), 1);
        assert_eq!(check("#[cfg_attr(unix, path = \"unix.rs\")]"), 1);
        assert_eq!(check("if cfg!(windows) { a() }"), 1);
        assert_eq!(check("#[cfg(feature = \"metal\")]"), 0);
        assert_eq!(check("#[cfg(test)]"), 0);
        assert_eq!(check("// on windows a cfg(windows) arm would differ"), 0);
    }
}
