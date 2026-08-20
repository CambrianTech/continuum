//! **Every unwrap carries an inline justification, and the count can only go down.**
//!
//! Joel's standing law, stated repeatedly and repeatedly re-broken because it lived
//! only in CLAUDE.md: *"Consider all unwraps suspect and if legit mark in comments
//! why… Comment on same line. We will eventually force CI to require justification
//! for these."*
//!
//! # What the two forms actually cost
//!
//! They are not the same hazard and the guard counts them separately:
//!
//! - `unwrap()` / `expect()` — **crash.** Loud, immediate, debuggable. Bad, but the
//!   failure tells you where it was.
//! - `unwrap_or*` — **silent laundering.** An `Option` means UNKNOWN; the default
//!   turns unknown into a *quantity*, and a governor then budgets against it. This
//!   is the one that ruined memory allocation: `read_system_free_bytes().unwrap_or(total)`
//!   told the ResourceGovernor the whole pool was free every time a Mach syscall
//!   failed, and nothing anywhere crashed. See [`crate::gpu::device_probe`].
//!
//! # Why a ratchet, not a wall
//!
//! There are thousands of these in tree. A guard that demands they all be justified
//! today fails forever, gets `#[ignore]`d within a week, and enforces nothing — the
//! exact fate of a rule that lives in prose. So the guard enforces the only thing
//! that matters going forward: **the unjustified count may never rise.** Every new
//! `unwrap` must carry its reason, and every old one anybody touches is a chance to
//! ratchet the baseline down. That is a rule with teeth on day one.
//!
//! # What counts as justified
//!
//! A same-line `//` comment with real content (see [`MIN_JUSTIFICATION_CHARS`]) —
//! not `// ok`, not `// fixme`. String-literal-aware, so a `//` inside a URL is not
//! mistaken for a comment (see [`super::split_code_and_comment`]).

use super::{split_code_and_comment, SourceRule, SourceFile, Violation};

/// A justification has to say something. `// ok` and `// safe` are not reasons; they
/// are the shape of a reason, which is worse than nothing because it silences the
/// guard. Twelve characters is roughly "unwrap: seeded" — short, but forced to name
/// a fact.
const MIN_JUSTIFICATION_CHARS: usize = 12;

/// The call shapes this rule watches, longest-first so `unwrap_or_else` is not
/// matched as `unwrap_or`.
const SHAPES: &[&str] = &[
    ".unwrap_or_else(",
    ".unwrap_or_default(",
    ".unwrap_or(",
    ".unwrap()",
    ".expect(",
];

/// Unjustified production occurrences at the time this guard landed (2026-08-20).
///
/// **This number may only ever go DOWN.** Lowering it when you clean a file is the
/// point; raising it to make a red build green is defeating the guard and should be
/// refused in review. The count is deliberately a single total rather than a
/// per-file map: a per-file baseline invites "just add my file", and the pressure
/// should be on the whole surface.
///
/// Top offenders when it was taken, so the ratchet has obvious first targets:
/// `ai/openai_adapter.rs` 60, `modules/grid/handlers.rs` 48, `tool_parsing/parsers.rs` 46,
/// `orm/sqlite.rs` 33, `commands/agent/solve.rs` 32. (First measured at 2253; the
/// correct code-vs-prose split then revealed 14 more that a doc-comment mention of
/// `#[cfg(test)]` had been hiding — see `scan`.)
const BASELINE_UNJUSTIFIED: usize = 2267;

pub struct UnwrapJustification;

impl SourceRule for UnwrapJustification {
    fn name(&self) -> &'static str {
        "unwrap-justification"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        let mut out = Vec::new();
        for (line_no, line) in file.production_lines() {
            let (code, comment) = split_code_and_comment(line);
            if !SHAPES.iter().any(|s| code.contains(s)) {
                continue;
            }
            let justified = comment
                .map(|c| c.trim().chars().count() >= MIN_JUSTIFICATION_CHARS)
                .unwrap_or(false); // JUSTIFIED: no same-line comment IS the unjustified case — the default is the answer, not a stand-in
            if !justified {
                out.push(Violation {
                    rule: "unwrap-justification",
                    file: file.rel.clone(),
                    line: line_no,
                    source: code.trim().to_string(),
                });
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source_hygiene::scan;

    /// What this catches: a NEW unjustified unwrap landing anywhere in the crate.
    ///
    /// The assertion is a RATCHET, not a wall — see the module header for why a wall
    /// would have been enforcement theatre. If this fails because you added one:
    /// write the reason on the same line. If it fails because you REMOVED some,
    /// lower `BASELINE_UNJUSTIFIED` and take the win.
    #[test]
    fn the_unjustified_unwrap_count_never_rises() {
        let violations = scan(&[&UnwrapJustification]);
        let count = violations.len();

        assert!(
            count <= BASELINE_UNJUSTIFIED,
            "unjustified production unwraps rose to {count} (baseline {BASELINE_UNJUSTIFIED}).\n\
             Every unwrap/expect/unwrap_or needs a same-line `//` comment saying WHY it is safe.\n\
             `unwrap_or` especially: an Option means UNKNOWN, and a default turns unknown into a \
             quantity the governor then budgets against — that is how the Metal free-VRAM lie \
             happened (gpu::device_probe).\n\
             First few new ones:\n{}",
            violations
                .iter()
                .take(10)
                .map(|v| format!("  {}:{}  {}", v.file, v.line, v.source))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// What this catches: the justification predicate accepting a non-reason. A rule
    /// that `// ok` satisfies is a rule that reports zero violations forever while
    /// the codebase rots — silence that looks like health.
    #[test]
    fn a_token_comment_does_not_count_as_a_justification() {
        let rule = UnwrapJustification;
        let mk = |src: &str| SourceFile {
            rel: "fake.rs".into(),
            production: src.into(),
            raw: src.into(),
        };

        assert_eq!(rule.check(&mk("let x = o.unwrap(); // ok")).len(), 1, "`// ok` is not a reason");
        assert_eq!(rule.check(&mk("let x = o.unwrap();")).len(), 1, "no comment at all");
        assert_eq!(
            rule.check(&mk("let x = o.unwrap(); // JUSTIFIED: seeded at construction")).len(),
            0,
            "a real reason passes"
        );
        assert_eq!(
            rule.check(&mk("let n = o.unwrap_or(0); // measured elsewhere, 0 is a real count here")).len(),
            0
        );
        assert_eq!(rule.check(&mk("let plain = compute();")).len(), 0, "no unwrap, no violation");
    }
}
