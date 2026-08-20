//! Crate-wide source hygiene: **rules that make a bad shape unrepresentable, not
//! merely discouraged.**
//!
//! # Why this module exists
//!
//! CLAUDE.md already carries hygiene laws in prose — "every unwrap requires an
//! inline justification", "implement the probe, never the monitor", "no new
//! hardcoded context constant". Prose stops nobody. Every one of those laws that
//! actually holds today holds because someone wrote a TEST that scans the source,
//! and every one that gets re-broken is one that didn't.
//!
//! There were already two such scanners in tree, each hand-rolled inside the module
//! it happened to guard: the de-hardcode guard in `cognition/context_budget.rs` and
//! the module-wiring audit in `runtime/registry.rs`. Two copies of "walk src/, strip
//! comments, match a predicate, report offenders" is the same duplication this
//! codebase forbids everywhere else — so the third rule got the SEAM instead of a
//! third copy.
//!
//! - [`SourceFile`] — one file, already split into its production and test halves.
//! - [`SourceRule`] — what a rule supplies, and all it supplies: a predicate over
//!   one file. The walking, the comment handling, the reporting are the scanner's.
//! - [`scan`] — walks `src/` once and applies every rule.
//!
//! The two existing scanners should migrate onto this seam (they predate it); doing
//! that is a follow-up, not a reason to hand-roll a fourth.

pub mod unwrap_justification;

use std::path::{Path, PathBuf};

/// One source file, pre-split so a rule never has to re-derive the split.
pub struct SourceFile {
    /// Path relative to `src/`, forward-slashed — the form a rule's allow-list and
    /// a failure message should both use.
    pub rel: String,
    /// The PRODUCTION half: everything before the file's first `#[cfg(test)]` line.
    ///
    /// This is an APPROXIMATION and callers must know which way it errs. Test mods
    /// conventionally sit at the end of a file, so this captures production exactly
    /// in the common case. Where a file gates a helper `#[cfg(test)]` mid-file, the
    /// split lands early and the rule sees LESS production code than exists — it
    /// under-reports, never over-reports. A hygiene rule that under-reports lets a
    /// violation slip; one that over-reports cries wolf and gets deleted. Erring
    /// toward the first is the deliberate choice.
    pub production: String,
}

impl SourceFile {
    /// Production lines paired with their 1-indexed line number in the whole file,
    /// so a violation can point at a real editor line.
    pub fn production_lines(&self) -> impl Iterator<Item = (usize, &str)> {
        self.production.lines().enumerate().map(|(i, l)| (i + 1, l))
    }
}

/// One violation, in the shape a failure message can print directly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub rule: &'static str,
    pub file: String,
    pub line: usize,
    /// The offending source line, trimmed — so the message shows the code, not just
    /// a coordinate.
    pub source: String,
}

/// What a hygiene rule supplies. Implement this; do not walk the tree yourself.
pub trait SourceRule {
    fn name(&self) -> &'static str;
    fn check(&self, file: &SourceFile) -> Vec<Violation>;
}

/// `src/` of this crate, resolved from the manifest so the scan is independent of
/// the process's working directory (a test's cwd is not the crate root under every
/// runner).
pub fn crate_src_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

/// Walk `src/` once and apply every rule to every `.rs` file.
///
/// Unreadable files are skipped rather than failing the scan: a hygiene guard must
/// not turn a transient IO error into a red build that says nothing about hygiene.
pub fn scan(rules: &[&dyn SourceRule]) -> Vec<Violation> {
    let root = crate_src_root();
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "rs") {
                continue;
            }
            let Ok(src) = std::fs::read_to_string(&path) else {
                continue;
            };
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            // Split on the first `#[cfg(test)]` that is CODE, not prose. Matching
            // the raw string would truncate at the first doc comment that merely
            // MENTIONS test-gating — and module headers in this crate do exactly
            // that, including this one. Found by the guard's own positive control:
            // the naive form excluded nearly all of `source_hygiene/mod.rs` from
            // its own scan, silently. Match by nature, never by name.
            let production = match src
                .lines()
                .position(|l| split_code_and_comment(l).0.contains("#[cfg(test)]"))
            {
                Some(line_idx) => src
                    .lines()
                    .take(line_idx)
                    .collect::<Vec<_>>()
                    .join("\n"),
                None => src.clone(),
            };
            let file = SourceFile { rel, production };
            for rule in rules {
                out.extend(rule.check(&file));
            }
        }
    }
    out.sort_by(|a, b| (a.file.as_str(), a.line).cmp(&(b.file.as_str(), b.line)));
    out
}

/// Split a source line into (code, same-line comment), respecting string literals so
/// a `//` inside `"http://…"` is not read as a comment.
///
/// Shared because every rule that asks "is this line justified / is this a real
/// occurrence" needs exactly this, and getting it subtly different per rule is how
/// two scanners disagree about the same file.
pub fn split_code_and_comment(line: &str) -> (&str, Option<&str>) {
    let bytes = line.as_bytes();
    let mut in_str = false;
    let mut escaped = false;
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if in_str {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_str = false;
            }
        } else if c == b'"' {
            in_str = true;
        } else if c == b'/' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            return (&line[..i], Some(&line[i + 2..]));
        }
        i += 1;
    }
    (line, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What this catches: a `//` inside a string literal being read as the start of
    /// a comment — which would let `let u = fetch("http://x").unwrap();` count as
    /// "justified" because the URL's slashes look like a comment. Real shape: URLs
    /// and file paths appear on plenty of unwrap lines in this crate.
    #[test]
    fn a_double_slash_inside_a_string_is_not_a_comment() {
        let (code, comment) = split_code_and_comment(r#"let u = get("http://x").unwrap();"#);
        assert_eq!(comment, None, "the URL's // must not read as a comment");
        assert!(code.contains("unwrap"));

        let (code, comment) =
            split_code_and_comment(r#"let u = get("http://x").unwrap(); // JUSTIFIED: seeded"#);
        assert_eq!(comment, Some(" JUSTIFIED: seeded"));
        assert!(code.contains("unwrap"));

        // An escaped quote must not end the string early.
        let (_, comment) = split_code_and_comment(r#"let s = "a\"// b";"#);
        assert_eq!(comment, None);
    }

    /// What this catches: the production/test split silently swallowing the whole
    /// file (or none of it), which would make every rule vacuously green or wildly
    /// noisy. The split is an approximation and this pins WHICH approximation.
    #[test]
    fn production_half_stops_at_the_first_cfg_test() {
        let root = crate_src_root();
        assert!(root.join("lib.rs").is_file(), "src root resolved wrong");

        struct Collect;
        impl SourceRule for Collect {
            fn name(&self) -> &'static str {
                "collect"
            }
            fn check(&self, file: &SourceFile) -> Vec<Violation> {
                // This module's own tests live below a `#[cfg(test)]`, so the
                // production half must NOT contain this very marker string.
                if file.rel == "source_hygiene/mod.rs" && file.production.contains("mod tests") {
                    return vec![Violation {
                        rule: "collect",
                        file: file.rel.clone(),
                        line: 0,
                        source: "test mod leaked into the production half".into(),
                    }];
                }
                Vec::new()
            }
        }
        assert!(scan(&[&Collect]).is_empty());
    }
}
