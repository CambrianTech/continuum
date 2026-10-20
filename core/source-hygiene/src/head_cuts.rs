//! **Text bound for her prompt never loses its newest end, and never shrinks by a literal.**
//!
//! # The class
//!
//! A cut that keeps the START of text drops what happened last: the latest message, the
//! result she just got, the outcome at the end of a log. She then acts on a world where
//! time and causality never moved forward, and loops or hangs (Joel, 2026-10-20). And one
//! source that shrinks itself by its own number breaks the whole budget, because the
//! budget no longer accounts for what the prompt holds. The sweep that day found ~30:
//! a per-message `.clamp(48, 256)` in room history, `first line at 140` and `800 chars`
//! on recalled acts, `chars().take(cap) + "…[truncated]"` on command results, a 12-card
//! board, `.min(1_500)` on her own work turns.
//!
//! # What it asks
//!
//! On the paths that render her prompt, text shrinks only through the one budget, oldest
//! first, collapsed to a handle she can expand (`Contribution::with_parts` +
//! `grounding_plan` + `expand_command`, or `tool_executor::spill`), keeping the newest end.
//! A cut that is NOT prompt text (a log label, a file name in a probe) says so with
//! `head-cut-ok: <why>` on its line or in the comment block above.
//!
//! # A ratchet
//!
//! The count may only fall; lower `BASELINE_HEAD_CUTS` when you remove one.

use super::{split_code_and_comment, SourceFile, SourceRule, Violation};

/// The marker a cut spends to say it never reaches a prompt.
pub const JUSTIFICATION_MARKER: &str = "head-cut-ok:";

/// A reason has to say something (the same bar as `derived-or-floor:`).
// derived-or-floor: bounds the length of a comment a human types, never a token, a second or a rate
// context-budget-exempt: bounds a COMMENT's length, not a prompt; matched on the CHARS suffix alone
const MIN_REASON_CHARS: usize = 16;

/// The trees that render what a mind reads.
const PROMPT_PATHS: &[&str] = &["cognition/", "persona/", "rag/", "modules/", "airc/", "positron/"];

/// Cuts that keep a head whatever bounds them.
const HEAD_KEEPERS: &[&str] = &[".chars().take(", "head_to_tokens(", "truncate_chars(", ".lines().next()"];

/// Shrinking calls that take a literal size: a ceiling no budget decided.
const LITERAL_CEILINGS: &[&str] = &[".take(", ".truncate(", "[.."];

/// A literal `.min(N)` / `.clamp(_, N)` is a size ceiling only beside a size word; without
/// one it is arithmetic (a backoff exponent, a percentage), which this rule must not flag.
const SIZE_WORDS: &[&str] = &["cap", "char", "token", "len", "budget", "share", "limit", "max"];

pub struct HeadCuts;

impl HeadCuts {
    /// `call` followed (after spaces) by a decimal digit: a size written into the code.
    fn literal_after(code: &str, call: &str) -> bool {
        code.match_indices(call).any(|(i, _)| {
            code[i + call.len()..].trim_start().starts_with(|c: char| c.is_ascii_digit())
        })
    }

    /// `.clamp(a, N)` with a literal upper bound.
    fn literal_clamp_ceiling(code: &str) -> bool {
        code.match_indices(".clamp(").any(|(i, _)| {
            let args = &code[i + ".clamp(".len()..];
            let Some(close) = args.find(')') else { return false };
            let Some((_, upper)) = args[..close].rsplit_once(',') else { return false };
            upper.trim_start().starts_with(|c: char| c.is_ascii_digit())
        })
    }

    fn cut(code: &str) -> Option<&'static str> {
        if let Some(k) = HEAD_KEEPERS.iter().find(|k| code.contains(**k)) {
            return Some(k);
        }
        if let Some(c) = LITERAL_CEILINGS.iter().find(|c| Self::literal_after(code, c)) {
            return Some(c);
        }
        let lower = code.to_ascii_lowercase();
        if !SIZE_WORDS.iter().any(|w| lower.contains(w)) {
            return None;
        }
        if Self::literal_after(code, ".min(") {
            return Some(".min(N)");
        }
        Self::literal_clamp_ceiling(code).then_some(".clamp(_, N)")
    }

    /// The marker on this line or in the contiguous comment block above it.
    fn justified(lines: &[&str], idx: usize) -> bool {
        let says = |l: &str| {
            l.find(JUSTIFICATION_MARKER)
                .is_some_and(|at| l[at + JUSTIFICATION_MARKER.len()..].trim().chars().count() >= MIN_REASON_CHARS)
        };
        if lines.get(idx).is_some_and(|l| says(l)) {
            return true;
        }
        lines[..idx]
            .iter()
            .rev()
            .take_while(|l| l.trim_start().starts_with("//"))
            .any(|l| says(l))
    }
}

impl SourceRule for HeadCuts {
    fn name(&self) -> &'static str {
        "head-cut"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        if !PROMPT_PATHS.iter().any(|p| file.rel.starts_with(p)) {
            return Vec::new();
        }
        let lines: Vec<&str> = file.raw.lines().collect();
        file.production_lines()
            .filter_map(|(line_no, line)| {
                let (code, _) = split_code_and_comment(line);
                let cut = Self::cut(code)?;
                (!Self::justified(&lines, line_no - 1)).then(|| Violation {
                    rule: "head-cut",
                    file: file.rel.clone(),
                    line: line_no,
                    source: format!("{cut}  {}", code.trim()),
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;

    /// Measured on this branch (2026-10-20). Lives in the test mod so the production build
    /// spends no `never used` warning on a number only a test reads.
    // 179 -> 188, not new debt: production_half now blanks only the #[cfg(test)]-gated item, so
    // production code BELOW a mid-file test mod or gated helper is scanned (measured on canary
    // a85454889 + this change, 2026-10-20).
    const BASELINE_HEAD_CUTS: usize = 188;

    /// What this catches: a new cut on a prompt path that keeps the head of text or
    /// shrinks it by a literal, the shape that hides her newest message or result.
    #[test]
    fn head_cuts_on_prompt_paths_never_increase() {
        let violations = scan(&[&HeadCuts]);
        let count = violations.len();
        eprintln!("head cuts: {count} (baseline {BASELINE_HEAD_CUTS})");
        assert!(
            count <= BASELINE_HEAD_CUTS,
            "head cuts on prompt paths rose to {count} (baseline {BASELINE_HEAD_CUTS}).\n\
             A cut that keeps the start drops the newest end, and she acts as if time never \
             moved. Shrink through the one budget, oldest first, to a handle she can expand \
             (Contribution::with_parts + grounding_plan, or tool_executor::spill). If this \
             text never reaches a prompt: `// {JUSTIFICATION_MARKER} <why>`.\n\
             First {} of {count} (not necessarily new):\n{}",
            count.min(20),
            violations.iter().take(20).map(|v| format!("  {}:{}  {}", v.file, v.line, v.source)).collect::<Vec<_>>().join("\n")
        );
        crate::assert_baseline_is_current("BASELINE_HEAD_CUTS", count, BASELINE_HEAD_CUTS);
    }

    /// What this catches: the predicates that make the rule useful rather than noise. A
    /// budget-derived bound with no head-keeping call is fine; a literal ceiling, a head
    /// keeper and a literal clamp are each caught; a real reason silences one; a bare
    /// marker does not.
    #[test]
    fn literals_and_head_keepers_are_caught_and_a_reason_silences_one() {
        let check = |rel: &str, src: &str| HeadCuts.check(&SourceFile::for_test(rel, src)).len();
        assert_eq!(check("persona/a.rs", "let n = items.len().min(share);"), 0);
        assert_eq!(check("persona/a.rs", "items.truncate(5);"), 1);
        assert_eq!(check("persona/a.rs", "let s: String = body.chars().take(cap).collect();"), 1);
        assert_eq!(check("persona/a.rs", "let cap = (budget / 8).clamp(48, 256);"), 1);
        assert_eq!(check("persona/a.rs", "let head = &s[..197];"), 1);
        assert_eq!(check("persona/a.rs", "let keep = self.budget().trail_head_chars().min(1_500);"), 1);
        assert_eq!(check("airc/a.rs", "let mult = 1u32 << failed_attempts.min(10);"), 0, "arithmetic, not a size");
        assert_eq!(check("audio/a.rs", "items.truncate(5);"), 0, "not a prompt path");
        assert_eq!(check("persona/a.rs", "// head-cut-ok: a probe label, never prompt text\nlabel.truncate(40);"), 0);
        assert_eq!(check("persona/a.rs", "label.truncate(40); // head-cut-ok:"), 1, "a bare marker is not a reason");
    }
}
