//! **Every serialization site names the boundary it crosses, and the count can
//! only go down.**
//!
//! Joel's standing law, 2026-08-23, after the pixel + serialization audits:
//! *"The way we did it was preventing your insane use of
//! memcpy/serialization/rasterizing… It's like a cancer everywhere."* /
//! *"Base64 is just fucking insane."* / *"You write js level code all the time.
//! Never cpp/rust performant rtos. It's like a sickness."*
//!
//! # The disease this guards against
//!
//! Managed-language reflexes transplanted into Rust: typed data serialized into
//! `serde_json::Value`/strings to move BETWEEN IN-PROCESS LAYERS, then parsed
//! back out on the other side — paying encode + alloc + decode where a `&T`,
//! an `Arc<T>`, or a handle should have traveled. Base64 is the worst form:
//! pixels inflated 4/3× into JSON envelopes cloned per consumer, on the same
//! machine that produced them. Each site compiles, passes review, and taxes the
//! hot path forever; the taxes COMPOUND (per tick × per consumer × per frame).
//! The 2026-08-23 audits found the compounding total was most of the substrate's
//! self-inflicted CPU waste — see [[i-write-js-level-code-in-rust-the-sickness-and-the-rtos-cure]].
//!
//! # What counts as justified
//!
//! Serialization is CORRECT at a true system boundary: a socket to another
//! process, a file format on disk, a wire protocol, a foreign API's contract.
//! So the rule is the unwrap rule's shape — a same-line `//` comment with real
//! content (≥ [`super::unwrap_justification`]'s bar) naming WHICH boundary this
//! encoding crosses. "// IPC wire to the web client" is a reason;
//! "// serialize" is the shape of a reason, which is worse than nothing.
//!
//! # Why a ratchet, not a wall
//!
//! Same argument as the unwrap guard verbatim: hundreds are in tree today, a
//! wall would fail forever and get ignored, and the only rule with teeth on day
//! one is **the unjustified count may never rise** — every new site names its
//! boundary, every old one touched is a chance to ratchet down.

use super::{split_code_and_comment, SourceFile, SourceRule, Violation};

/// Same bar as the unwrap rule — a justification has to say something.
// context-budget-exempt: bounds the length of a COMMENT a human types, never a
// window, prompt, or token budget.
const MIN_JUSTIFICATION_CHARS: usize = 12;

/// The call shapes this rule watches. Serde's owned-encode family (each one an
/// allocation + full traversal of the value) and the base64 engine (the
/// pixels-in-JSON signature). `to_writer`/`from_reader`/`from_str`/`from_slice`
/// are deliberately NOT counted: they read/write an existing byte stream, which
/// is almost always an actual boundary already — counting them would bury the
/// signal under legitimate wire code.
const SHAPES: &[&str] = &[
    "serde_json::to_string_pretty(",
    "serde_json::to_string(",
    "serde_json::to_vec(",
    "serde_json::to_value(",
    "serde_json::from_value(",
    "STANDARD.encode(",
    "STANDARD.decode(",
    "STANDARD_NO_PAD.encode(",
    "STANDARD_NO_PAD.decode(",
];

/// Unjustified production occurrences at the time this guard landed
/// (2026-08-23, the day the audits ran — measured by running the rule with a
/// zero baseline and reading the count, so there is no slack for new sites to
/// hide in).
///
/// **This number may only ever go DOWN.** A single total, not a per-file map,
/// for the same reason as the unwrap ratchet: pressure on the whole surface.
const BASELINE_UNJUSTIFIED: usize = 242;

pub struct BoundarySerialization;

impl SourceRule for BoundarySerialization {
    fn name(&self) -> &'static str {
        "boundary-serialization"
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
                .unwrap_or(false); // JUSTIFIED: no same-line comment IS the unjustified case
            if !justified {
                out.push(Violation {
                    rule: "boundary-serialization",
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

    /// What this catches: a NEW in-process serialization site landing without
    /// naming the boundary it crosses. The assertion is a RATCHET, not a wall —
    /// see the module header. If this fails because you added one: either the
    /// encode crosses a true boundary (say which, same line) or it shouldn't
    /// exist (pass `&T`/`Arc<T>`/a handle instead). If it fails because you
    /// removed some, lower `BASELINE_UNJUSTIFIED` and take the win.
    #[test]
    fn the_unjustified_serialization_count_never_rises() {
        let violations = scan(&[&BoundarySerialization]);
        let count = violations.len();

        assert!(
            count <= BASELINE_UNJUSTIFIED,
            "unjustified serialization sites rose to {count} (baseline {BASELINE_UNJUSTIFIED}).\n\
             Serialize ONLY at a true system boundary (socket, disk format, wire protocol, \n\
             foreign API) and say WHICH boundary on the same line. Between in-process layers, \n\
             data travels as &T / Arc<T> / a handle — typed → Value → typed is the sickness \n\
             the 2026-08-23 audits measured as most of the substrate's self-inflicted CPU \n\
             waste. Base64 pixels inside JSON are never justified on-machine.\n\
             First few new ones:\n{}",
            violations
                .iter()
                .take(10)
                .map(|v| format!("  {}:{}  {}", v.file, v.line, v.source))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// What this catches: the justification predicate accepting a non-reason —
    /// a rule `// json` satisfies reports zero violations forever while the
    /// codebase rots.
    #[test]
    fn a_token_comment_does_not_count_as_a_boundary_name() {
        let rule = BoundarySerialization;
        let file = SourceFile::for_test(
            "fake.rs",
            "let s = serde_json::to_string(&x); // json\n\
             let t = serde_json::to_string(&y); // IPC wire to the web client\n",
        );
        let v = rule.check(&file);
        assert_eq!(v.len(), 1, "short token comment must not justify; real boundary name must");
        assert_eq!(v[0].line, 1);
    }
}
