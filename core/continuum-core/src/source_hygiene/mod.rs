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

pub mod boundary_serialization;
pub mod identity_discipline;
pub mod production_reachability;
pub mod tenant_neutrality;
pub mod test_mod_singularity;
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
    /// The WHOLE file, test mods included.
    ///
    /// Carried alongside `production` rather than re-read on demand because a rule
    /// that compares one text shape against another must be able to get both from the
    /// same read. (The first cut of [`production_reachability`] re-read the file and
    /// fell back to `production` when the read failed — a fallback that silently
    /// swapped the shape and reintroduced the exact miscount it was written to fix.)
    pub raw: String,
}

impl SourceFile {
    /// Production lines paired with their 1-indexed line number in the whole file,
    /// so a violation can point at a real editor line.
    pub fn production_lines(&self) -> impl Iterator<Item = (usize, &str)> {
        self.production.lines().enumerate().map(|(i, l)| (i + 1, l))
    }

    /// Build a `SourceFile` from an inline snippet — fixture for a rule's own
    /// unit tests, so predicate regressions are pinned without touching disk.
    #[cfg(test)]
    pub fn for_test(rel: &str, production: &str) -> Self {
        Self {
            rel: rel.to_string(),
            production: production.to_string(),
            raw: production.to_string(),
        }
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

/// Every `src/` directory in the workspace — this crate's, its sibling crates', and
/// the apps'.
///
/// **Why a rule ever needs more than `crate_src_root`:** some hygiene questions are
/// not answerable from one crate. "Is this `pub` type referenced anywhere?" is the
/// motivating case — `continuum-core` is a LIBRARY, so a type with no in-crate caller
/// may be perfectly well wired from `apps/cli` or `continuum-mcp`. A guard that
/// reported those as dead would be wrong on its first run and deleted by its second.
///
/// So the asymmetry is deliberate and load-bearing: **violations are only ever raised
/// against `crate_src_root()`, while REFERENCES are searched across every root here.**
/// Widening the reference corpus can only ever REMOVE findings, never add one.
///
/// Resolved from the manifest (`core/continuum-core` → repo root two levels up) so the
/// scan does not depend on the process's working directory. Roots that do not exist
/// are skipped, so this stays correct if the layout changes under it.
pub fn workspace_src_roots() -> Vec<PathBuf> {
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut roots = vec![manifest.join("src")];
    // `core/continuum-core` → `continuum/`. `ancestors()` rather than two `parent()`
    // unwraps: a path shallower than expected yields no root instead of a panic.
    let Some(repo) = manifest.ancestors().nth(2) else {
        return roots;
    };
    for group in ["core", "apps"] {
        let Ok(entries) = std::fs::read_dir(repo.join(group)) else {
            continue;
        };
        for entry in entries.flatten() {
            let src = entry.path().join("src");
            // The crate's own root is already first; skip the duplicate rather than
            // paying to read every file in it twice.
            if src.is_dir() && src != roots[0] {
                roots.push(src);
            }
        }
    }
    roots
}

/// Read every `.rs` file under `root`, pre-split into production and test halves.
///
/// Unreadable files are skipped rather than failing the scan: a hygiene guard must
/// not turn a transient IO error into a red build that says nothing about hygiene.
pub fn collect_files(root: &Path) -> Vec<SourceFile> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
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
                .strip_prefix(root)
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
            out.push(SourceFile {
                rel,
                production,
                raw: src,
            });
        }
    }
    out
}

/// Walk this crate's `src/` once and apply every per-file rule to every `.rs` file.
pub fn scan(rules: &[&dyn SourceRule]) -> Vec<Violation> {
    let files = collect_files(&crate_src_root());
    let mut out = Vec::new();
    for file in &files {
        for rule in rules {
            out.extend(rule.check(file));
        }
    }
    sort_violations(&mut out);
    out
}

/// A rule that needs to see the WHOLE crate at once, not one file at a time.
///
/// [`SourceRule`] covers the common case — a predicate over a single file — and every
/// rule that CAN be expressed that way should be, because per-file rules are cheap and
/// trivially testable. But some hygiene questions are irreducibly cross-file: "does
/// anything reference this?" cannot be answered from the file that declares it. That
/// is this trait, and the reason it is a separate one rather than a widened
/// `SourceRule`: a per-file rule that received the whole crate would invite every
/// future rule to reach for global state it does not need.
pub trait CrateRule {
    fn name(&self) -> &'static str;
    /// `files` are this crate's, the only source of violations. `corpus` is every
    /// production line in the workspace — the reference haystack, never a source of
    /// violations. See [`workspace_src_roots`] for why the two differ.
    fn check_crate(&self, files: &[SourceFile], corpus: &str) -> Vec<Violation>;
}

/// Apply crate-wide rules. Reads this crate once for violations and the workspace once
/// for references.
pub fn scan_crate(rules: &[&dyn CrateRule]) -> Vec<Violation> {
    let files = collect_files(&crate_src_root());
    // The reference corpus is RAW file text — production AND test halves.
    //
    // This asymmetry against `files` (production-only, the violation source) is
    // deliberate. A type referenced only from another file's `#[cfg(test)]` mod is
    // WIRED — to a test harness, on purpose; `GridSimulator` and `RecordingGridCapture`
    // are exactly that. Counting only production halves reported them as dead, and a
    // guard that cries wolf on legitimate harnesses is one that gets deleted. Asking
    // "referenced NOWHERE, not even by a test" is the sharper question and still
    // catches the motivating defect, whose only mentions are inside its own file.
    let mut corpus = String::new();
    for root in workspace_src_roots() {
        for text in raw_sources(&root) {
            corpus.push_str(&text);
            corpus.push('\n');
        }
    }
    let mut out = Vec::new();
    for rule in rules {
        out.extend(rule.check_crate(&files, &corpus));
    }
    sort_violations(&mut out);
    out
}

/// Every `.rs` file under `root`, unsplit — the whole text including test mods.
fn raw_sources(root: &Path) -> Vec<String> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|e| e == "rs") {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    out.push(text);
                }
            }
        }
    }
    out
}

fn sort_violations(v: &mut [Violation]) {
    v.sort_by(|a, b| (a.file.as_str(), a.line).cmp(&(b.file.as_str(), b.line)));
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
