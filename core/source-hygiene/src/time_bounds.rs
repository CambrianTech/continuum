//! **A duration or a rate is DERIVED from a measurement, or it is a NAMED FLOOR that says so.**
//!
//! # The class
//!
//! A static number standing in for a time or a rate the substrate already measures. The
//! defects that made this rule (2026-09-19/20):
//!
//! | The constant | What it cost |
//! |---|---|
//! | a flat 10 s KV-page wedge bound, sized from the M5's 0.1 s (#4287) | cut off legitimate page saves on the 5090 |
//! | `PREFILL_TARGET_SECONDS × CONSERVATIVE_PREFILL_TOKENS_PER_S` | a 500 t/s assumption no box in the fleet met, clipping every conversation to 15,000 tokens |
//!
//! Sizes (tokens, bytes, counts, shares) are `budget_discipline::SelfSized`'s class: they are
//! granted by the one allocation and allow no exemption. Durations have no owner to take
//! their value from yet (a cadence ladder and measured deadlines), so this rule still
//! accepts a `derived-or-floor: <why>` justification and only holds its count; when the time
//! owner exists, the marker goes the way of the size exemptions.
//!
//! # A ratchet
//!
//! The count may never rise; every touched duration is a chance to lower it.

use super::budget_discipline::is_duration_name;
use super::{lexer::split_code_and_comment, SourceFile, SourceRule, Violation};

/// The marker a constant spends to say it is deliberately a constant.
pub const JUSTIFICATION_MARKER: &str = "derived-or-floor:";

/// A justification has to say something. Same bar, and the same reasoning, as the unwrap
/// ratchet's [`super::unwrap_justification`]: a marker with nothing after it is the SHAPE
/// of a reason, which is worse than no marker at all because it silences the guard.
// derived-or-floor: a FLOOR on the length of a COMMENT a human types — it bounds prose, never a token, a second or a rate, and there is nothing here to measure
// context-budget-exempt: this bounds the length of a COMMENT, not a window, prompt or
// token budget — the de-hardcode guard matches it on the `CHARS` suffix alone, which is
// that guard erring toward noise on purpose (same exemption its sibling in
// `unwrap_justification` spends, for the same reason).
const MIN_REASON_CHARS: usize = 16;

/// The trees a live cognition / serving turn actually runs through. Violations are raised
/// only here: `audio_constants.rs` and the video renderer are full of constants that are
/// PHYSICS (a sample rate, a frame budget), and flagging them would bury the findings
/// that matter under noise — the way a guard earns being deleted.
const LIVE_PATHS: &[&str] = &[
    "cognition/",
    "inference/",
    "persona/",
    "governor/",
    "paging/",
    "runtime/",
    "capacity/",
    "system_resources/",
];


pub struct TimeBounds;

impl TimeBounds {
    /// Does the marker appear with a real reason after it — on this line, or in the
    /// contiguous comment block directly above it?
    ///
    /// The block-above form exists because a `const` carries a doc comment, not a
    /// trailing one; the same allowance the de-hardcode guard makes, for the same reason.
    fn justified(lines: &[&str], index: usize) -> bool {
        let reason_on = |text: &str| {
            text.split_once(JUSTIFICATION_MARKER)
                .is_some_and(|(_, why)| why.trim().chars().count() >= MIN_REASON_CHARS)
        };
        if let Some(comment) = split_code_and_comment(lines[index]).1 {
            if reason_on(comment) {
                return true;
            }
        }
        lines[..index]
            .iter()
            .rev()
            .take_while(|l| {
                let t = l.trim_start();
                t.starts_with("//") || t.is_empty()
            })
            .any(|l| reason_on(l))
    }
}

impl SourceRule for TimeBounds {
    fn name(&self) -> &'static str {
        "underived_duration"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        // Vendored upstream ports carry the upstream model's own numbers; they are not
        // ours to restyle.
        if file.rel.contains("vendored/") {
            return Vec::new();
        }
        let lines: Vec<&str> = file.raw.lines().collect();
        let live = LIVE_PATHS.iter().any(|p| file.rel.starts_with(p));
        let mut out = Vec::new();
        for (line_no, line) in file.production_lines() {
            let (code, _) = split_code_and_comment(line);
            if Self::justified(&lines, line_no - 1) {
                continue;
            }
            // A constant times a constant is the shape wherever it appears — it needs no
            // live-path test, because it IS the defect this guard was written for.
            if let Some((l, r)) = crate::lexer::constant_product(code).filter(|(l, r)| is_duration_name(l) || is_duration_name(r)) {
                out.push(Violation {
                    rule: "underived_duration",
                    file: file.rel.clone(),
                    line: line_no,
                    source: format!("{l} * {r} — a bound with no measurement in it"),
                });
                continue;
            }
            if !live {
                continue;
            }
            if let Some((name, ty, value)) = crate::lexer::numeric_const(code) {
                if is_duration_name(name) && crate::lexer::is_bare_literal(value) {
                    out.push(Violation {
                        rule: "underived_duration",
                        file: file.rel.clone(),
                        line: line_no,
                        source: format!("const {name}: {ty} = {value}"),
                    });
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;

    /// What this catches: a new duration or rate landing as a constant with nothing measured
    /// behind it and no line saying why a constant is right there.
    #[test]
    fn underived_durations_never_increase() {
        let violations = scan(&[&TimeBounds]);
        let count = violations.len();
        crate::ratchet::hold("underived_durations", count, || {
            format!(
                "underived, unjustified durations rose to {count}. Derive it from what the substrate \
                 measures (`inference::prefill_rate`, `inference::decode_knee`), or say on the line or \
                 in the block above why a constant is right there:\n    // {JUSTIFICATION_MARKER} <why>\n\
                 First {} of {count} (not necessarily new):\n{}",
                count.min(15),
                violations.iter().take(15).map(|v| format!("  {}:{}  {}", v.file, v.line, v.source)).collect::<Vec<_>>().join("\n")
            )
        });
    }

    /// What this catches: the guard going blind below a test-only helper gated inside an
    /// `impl` (a production split that cut at the first `#[cfg(test)]` reported nothing below).
    #[test]
    fn a_mid_file_cfg_test_helper_does_not_blind_the_scan() {
        let raw = "impl Faculty {\n    #[cfg(test)]\n    fn helper() {}\n}\n\
                   let cap = TARGET_SECONDS * CONSERVATIVE_TOKENS_PER_S;\n\
                   #[cfg(test)]\nmod tests {}\n";
        let file = SourceFile::fixture("cognition/llm_deliberation_faculty.rs", &crate::lexer::production_half(raw), raw);
        let found = TimeBounds.check(&file);
        assert_eq!(found.len(), 1, "the product below an indented #[cfg(test)] must still be seen");
        assert_eq!(found[0].line, 5);
    }

    /// What this catches: the predicates that make the rule useful rather than noise: a
    /// duration product and a bare duration are found, a derivation and a real reason pass,
    /// and a size is not this rule's class.
    #[test]
    fn a_duration_is_found_a_reason_passes_and_a_size_is_not_its_class() {
        let count = |rel: &str, raw: &str| TimeBounds.check(&SourceFile::fixture(rel, raw, raw)).len();
        assert_eq!(count("cognition/x.rs", "let c = A_SECONDS * B_TOKENS_PER_S;"), 1);
        assert_eq!(count("cognition/x.rs", "const PAGE_WEDGE_TIMEOUT_MS: u64 = 10_000;"), 1);
        for visibility in ["pub", "pub(crate)", "pub(super)", "pub(in crate::owner)"] {
            assert_eq!(count("cognition/x.rs", &format!("{visibility} const PAGE_WEDGE_TIMEOUT_MS: u64 = 10_000;")), 1, "{visibility}");
        }
        assert_eq!(count("cognition/x.rs", "const PAGE_WEDGE_TIMEOUT_MS: u64 = MEASURED_P99_MS * 4;"), 0, "a derivation");
        assert_eq!(count("cognition/x.rs", "const PAGE_WEDGE_TIMEOUT_MS: u64 = 10_000; // derived-or-floor: ok"), 1, "a marker with no reason");
        assert_eq!(count("cognition/x.rs", "// derived-or-floor: the protocol's own stated deadline, not ours\nconst ACK_DEADLINE_MS: u64 = 500;"), 0);
        assert_eq!(count("cognition/x.rs", "const FILL_CAP_TOKENS: usize = 15_000;"), 0, "a size is SelfSized's class");
        assert_eq!(count("audio_constants.rs", "const SILENCE_THRESHOLD_MS: u64 = 704;"), 0, "off a live path");
    }
}
