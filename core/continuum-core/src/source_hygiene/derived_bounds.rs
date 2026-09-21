//! **A clip is DERIVED from a measurement, or it is a NAMED FLOOR that says so.**
//!
//! # The class
//!
//! Four defects of one shape surfaced in a single night (2026-09-19/20), each a static
//! number standing in for a measurement the substrate already takes:
//!
//! | The constant | What it cost |
//! |---|---|
//! | a 768-token output-allowance floor (#4194) | turns ENDED INSIDE the reasoning channel — the mind never reached its answer |
//! | the KV cache type defaulting to `f16` | halved the usable context on two of three boxes |
//! | a flat 10 s KV-page wedge bound, sized from the M5's 0.1 s (#4287) | cut off legitimate page saves on the 5090 |
//! | `PREFILL_TARGET_SECONDS × CONSERVATIVE_PREFILL_TOKENS_PER_S` | a flat 15,000-token conversation fill on every machine; the 500 t/s half was 5–20× what any box in the fleet prefills, so the clip failed its own 30 s purpose everywhere WHILE taking 27% of a 67,072-token lane away from its holder |
//!
//! Joel's standing instruction, the design constraint for this whole module: *static
//! clips and fallbacks silently destroy the system and then we do forensics.* Every one
//! of the four compiled, had passing tests, and was invisible until someone measured.
//!
//! # Why this guard and not the one that already exists
//!
//! `cognition::context_budget`'s de-hardcode guard is a WALL over constants whose NAME
//! says window/context/token/prompt/chars, exempted line-by-line with
//! `context-budget-exempt:`. It is load-bearing and stays. It did not catch this class
//! for two reasons, and this rule is exactly those two reasons:
//!
//! 1. **It cannot see a PRODUCT.** Both halves of `30 × 500` were individually exempted,
//!    in good faith, each with a true sentence about itself — and their product was a
//!    bound nothing had ever measured. [`ConstantProduct`] below is that shape: a
//!    constant multiplied by a constant is a bound with no measurement in it.
//! 2. **Its vocabulary stops at size.** Time and RATE are the units this class actually
//!    hides in — `_SECONDS`, `_SECS`, `_MS`, `_PER_S`, `_TPS` — and `_MS` is explicitly
//!    excluded there as "categorically not a size bound". True, and beside the point: a
//!    seconds constant times a measured rate is fine, a seconds constant standing in FOR
//!    a rate is the defect.
//!
//! # What it asks
//!
//! A bound on a live path is either derived from something the substrate measures, or it
//! carries a `derived-or-floor:` justification naming why a constant is the right answer
//! THERE — a floor that only ever raises a budget, a protocol's own stated requirement, a
//! physical unit, a product latency intent. The marker may sit on the same line or in the
//! contiguous comment block directly above the declaration, because a reason worth
//! writing rarely fits after a semicolon.
//!
//! # A ratchet, not a wall
//!
//! There are 160-odd of these in tree and most are fine. A guard that demanded all of
//! them today would fail forever and be `#[ignore]`d inside a week — the documented fate
//! of every rule that lives in prose. So: **the count may never rise.** Every new bound
//! arrives derived or justified, and every old one anybody touches is a chance to lower
//! `BASELINE_UNDERIVED_BOUNDS` (in the test mod).

use super::{split_code_and_comment, SourceFile, SourceRule, Violation};

/// The marker a constant spends to say it is deliberately a constant.
pub const JUSTIFICATION_MARKER: &str = "derived-or-floor:";

/// A justification has to say something. Same bar, and the same reasoning, as the unwrap
/// ratchet's [`super::unwrap_justification`]: a marker with nothing after it is the SHAPE
/// of a reason, which is worse than no marker at all because it silences the guard.
// derived-or-floor: a FLOOR on the length of a COMMENT a human types — it bounds prose, never a token, a second or a rate, and there is nothing here to measure
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

/// Name fragments that say a constant IS a bound, whatever its units.
const BOUND_WORDS: &[&str] = &[
    "CAP",
    "BOUND",
    "LIMIT",
    "MAX",
    "MIN",
    "FLOOR",
    "CEILING",
    "BUDGET",
    "TIMEOUT",
    "DEADLINE",
    "THRESHOLD",
    "TARGET",
    "QUOTA",
    "RESERVE",
    "ALLOWANCE",
    "INTERVAL",
    "CADENCE",
];

/// Units the substrate MEASURES — tokens, seconds, milliseconds, tokens-per-second. A
/// constant in one of these units is standing where a measurement could stand.
const MEASURED_UNITS: &[&str] = &["_PER_S", "_TPS", "_SECONDS", "_SECS", "_MS", "_TOKENS", "TOKENS_"];

/// Numeric types a bound can wear. A `&str`, a `bool` or a `Duration` named `..._MAX` is
/// not a quantity this rule can reason about.
const NUMERIC_TYPES: &[&str] = &[
    "u8", "u16", "u32", "usize", "u64", "i32", "i64", "f32", "f64",
];

/// Const declaration prefixes, longest first so `pub const` is not read as `const`.
const CONST_PREFIXES: &[&str] = &[
    "pub(crate) const ",
    "pub(super) const ",
    "pub const ",
    "const ",
];

pub struct DerivedBounds;

impl DerivedBounds {
    /// The file's PRODUCTION half, cut at the first COLUMN-ZERO `#[cfg(test)]`.
    ///
    /// This rule cannot use [`SourceFile::production`], and the reason is the motivating
    /// defect itself. The shared split cuts at the first `#[cfg(test)]` anywhere,
    /// including an indented test-only helper inside an `impl` — and
    /// `cognition/llm_deliberation_faculty.rs` has one at line 2275, three hundred lines
    /// ABOVE the `30 × 500` clip at 2496. Scanned through the shared split, this guard
    /// reports zero on the very line it exists for: silence that looks like health.
    ///
    /// A test MOD sits at file scope by convention, so column zero is the honest cut. The
    /// cost is that a `#[cfg(test)]` helper function mid-file is scanned as production —
    /// erring toward noise here rather than toward silence, deliberately, because this
    /// rule has already been shown a case where silence was wrong.
    fn production_lines<'a>(file: &'a SourceFile) -> impl Iterator<Item = (usize, &'a str)> + 'a {
        let end = file
            .raw
            .lines()
            .position(|l| l.starts_with("#[cfg(test)]"))
            .unwrap_or(usize::MAX); // no test mod in this file = every line is production
        file.raw
            .lines()
            .enumerate()
            .take_while(move |(i, _)| *i < end)
            .map(|(i, l)| (i + 1, l))
    }

    /// Is `token` a SCREAMING_SNAKE constant name? At least one underscore, so a type
    /// (`Duration`) or a single-letter generic is never mistaken for one.
    fn is_constant(token: &str) -> bool {
        token.len() >= 3
            && token.contains('_')
            && token.starts_with(|c: char| c.is_ascii_uppercase())
            && token
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_')
    }

    /// `CONST_A * CONST_B` on this line — a bound assembled from constants alone, with
    /// no measurement anywhere in it. Returns the two names.
    ///
    /// A constant times a LITERAL (`MIN_SERVE_CTX * 8`) is deliberately NOT this shape:
    /// it is a derivation from a named bound, which the de-hardcode guard already blesses
    /// and which keeps one owner for the number.
    fn constant_product<'a>(code: &'a str) -> Option<(&'a str, &'a str)> {
        let bytes = code.as_bytes();
        for (i, c) in code.char_indices() {
            if c != '*' {
                continue;
            }
            // `**`, `*/`, `/*` are not multiplication; nor is `*mut` / `*const`, whose
            // keywords are lowercase and so can never be constant names.
            if bytes.get(i + 1).is_some_and(|b| *b == b'*' || *b == b'/') {
                continue;
            }
            if i > 0 && (bytes[i - 1] == b'*' || bytes[i - 1] == b'/') {
                continue;
            }
            let left = Self::trailing_identifier(&code[..i]);
            let right = Self::leading_identifier(&code[i + 1..]);
            if let (Some(l), Some(r)) = (left, right) {
                if Self::is_constant(l) && Self::is_constant(r) {
                    return Some((l, r));
                }
            }
        }
        None
    }

    fn trailing_identifier<'a>(before: &'a str) -> Option<&'a str> {
        let t = before.trim_end();
        let start = t
            .char_indices()
            .rev()
            .take_while(|(_, c)| c.is_alphanumeric() || *c == '_')
            .last()
            .map(|(i, _)| i)?;
        Some(&t[start..])
    }

    fn leading_identifier<'a>(after: &'a str) -> Option<&'a str> {
        let t = after.trim_start();
        let end = t
            .char_indices()
            .take_while(|(_, c)| c.is_alphanumeric() || *c == '_')
            .last()
            .map(|(i, c)| i + c.len_utf8())?;
        Some(&t[..end])
    }

    /// `(name, type, value)` if this line declares a numeric constant.
    fn const_declaration<'a>(code: &'a str) -> Option<(&'a str, &'a str, &'a str)> {
        let trimmed = code.trim_start();
        let rest = CONST_PREFIXES
            .iter()
            .find_map(|p| trimmed.strip_prefix(*p))?;
        let (name, tail) = rest.split_once(':')?;
        let (ty, value) = tail.split_once('=')?;
        let ty = ty.trim();
        if !NUMERIC_TYPES.contains(&ty) {
            return None;
        }
        Some((name.trim(), ty, value.trim().trim_end_matches(';').trim()))
    }

    /// Does this constant's NAME say it is a bound, or that it carries a unit the
    /// substrate measures?
    fn names_a_bound(name: &str) -> bool {
        BOUND_WORDS.iter().any(|w| name.contains(w))
            || MEASURED_UNITS.iter().any(|u| name.contains(u))
    }

    /// A value the compiler reads as a fresh guess: a bare decimal or float literal. An
    /// expression over another named bound is a derivation and passes.
    fn is_bare_literal(value: &str) -> bool {
        !value.is_empty()
            && value.starts_with(|c: char| c.is_ascii_digit())
            && value
                .chars()
                .all(|c| c.is_ascii_digit() || c == '_' || c == '.')
    }

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

impl SourceRule for DerivedBounds {
    fn name(&self) -> &'static str {
        "derived-or-justified-bound"
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
        for (line_no, line) in Self::production_lines(file) {
            let (code, _) = split_code_and_comment(line);
            if Self::justified(&lines, line_no - 1) {
                continue;
            }
            // A constant times a constant is the shape wherever it appears — it needs no
            // live-path test, because it IS the defect this guard was written for.
            if let Some((l, r)) = Self::constant_product(code) {
                out.push(Violation {
                    rule: "derived-or-justified-bound",
                    file: file.rel.clone(),
                    line: line_no,
                    source: format!("{l} * {r} — a bound with no measurement in it"),
                });
                continue;
            }
            if !live {
                continue;
            }
            if let Some((name, ty, value)) = Self::const_declaration(code) {
                if Self::names_a_bound(name) && Self::is_bare_literal(value) {
                    out.push(Violation {
                        rule: "derived-or-justified-bound",
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
    use crate::source_hygiene::scan;

    /// Underived, unjustified bounds at the time this guard landed (2026-09-20).
    ///
    /// **This number may only ever go DOWN.** Raising it to make a red build green is
    /// defeating the guard, and should be refused in review for the same reason the
    /// unwrap and reachability baselines are: every one of the four defects in the module
    /// header was individually "not worth blocking on".
    ///
    /// Lives inside the test mod, unlike its siblings, so it costs the production build no
    /// `never used` warning — the warning ratchet is monotonic and a new guard should not
    /// spend a slot in it just to hold a number only a test reads.
    const BASELINE_UNDERIVED_BOUNDS: usize = 159;

    /// What this catches: a new bound landing on a live path as a constant, with nothing
    /// measured behind it and no line saying why a constant is right there.
    ///
    /// A RATCHET, not a wall — see the module header. If this fails because you added
    /// one: derive it from what the substrate already measures
    /// (`inference::prefill_rate`, `inference::decode_knee`, `cognition::working_set`,
    /// `ContextBudget`), or write `derived-or-floor: <why>` and say what makes a constant
    /// correct THERE. If it fails because you derived one that used to be a constant,
    /// LOWER the baseline and take the win.
    #[test]
    fn underived_unjustified_bounds_never_increase() {
        let violations = scan(&[&DerivedBounds]);
        let count = violations.len();
        // Visible on a PASS too, so lowering the baseline after a clean-up is one build.
        eprintln!("underived bounds: {count} (baseline {BASELINE_UNDERIVED_BOUNDS})");
        assert!(
            count <= BASELINE_UNDERIVED_BOUNDS,
            "underived, unjustified bounds rose to {count} (baseline \
             {BASELINE_UNDERIVED_BOUNDS}).\n\
             \n\
             THE CLASS: a constant standing in for a measurement the substrate already \
             takes. A 768-token output floor ended turns INSIDE the reasoning channel \
             (#4194); a KV cache type defaulting to f16 halved the context on two of \
             three boxes; a 10 s page-wedge bound sized from the M5's 0.1 s cut off \
             legitimate saves on the 5090 (#4287); and `PREFILL_TARGET_SECONDS × \
             CONSERVATIVE_PREFILL_TOKENS_PER_S` clipped every machine's conversation to \
             a flat 15,000 tokens with a 500 t/s assumption no box in the fleet has ever \
             met. Each compiled, each passed its tests, each was found by forensics.\n\
             \n\
             THE FIX: derive it. The measurements exist — `inference::prefill_rate` \
             (prefill t/s, with the fresh → stale → conservative ladder), \
             `inference::decode_knee` (decode t/s), `cognition::working_set` (her \
             measured need), `ContextBudget` (fractions of the SERVED window). If a \
             constant genuinely is right there — a floor that only ever raises a budget, \
             a protocol's stated requirement, a physical unit, a product latency intent \
             — say so on the line or in the comment block above it:\n\
             \x20   // {JUSTIFICATION_MARKER} a FLOOR — <why a constant is correct here>\n\
             \n\
             This scan cannot tell new from old. To find yours: diff against your base \
             and look for added `const` lines on cognition/inference/persona/governor/\
             paging/runtime/capacity/system_resources paths, or a `CONST * CONST` \
             product anywhere.\n\
             First {} of {count} in tree order (NOT necessarily new):\n{}",
            violations.len().min(15),
            violations
                .iter()
                .take(15)
                .map(|v| format!("  {}:{}  {}", v.file, v.line, v.source))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    /// What this catches: the guard going blind to the exact line it was written for.
    ///
    /// `cognition/llm_deliberation_faculty.rs` gates a test-only helper `#[cfg(test)]`
    /// INSIDE an `impl`, three hundred lines above the clip. Scanned through the shared
    /// production split — which cuts at the first `#[cfg(test)]` anywhere — this rule
    /// reports nothing at all on that file. Column zero is the honest cut.
    #[test]
    fn a_mid_file_cfg_test_helper_does_not_blind_the_scan() {
        let raw = "impl Faculty {\n    #[cfg(test)]\n    fn helper() {}\n}\n\
                   let cap = TARGET_SECONDS * CONSERVATIVE_TOKENS_PER_S;\n\
                   #[cfg(test)]\nmod tests {}\n";
        let file = SourceFile {
            rel: "cognition/llm_deliberation_faculty.rs".into(),
            // The shared split stops at the INDENTED attribute, losing the clip entirely.
            production: "impl Faculty {".into(),
            raw: raw.into(),
        };
        let found = DerivedBounds.check(&file);
        assert_eq!(found.len(), 1, "the clip below an indented #[cfg(test)] must still be seen");
        assert!(found[0].source.contains("TARGET_SECONDS * CONSERVATIVE_TOKENS_PER_S"));
        assert_eq!(found[0].line, 5);
    }

    /// What this catches: the two predicates that decide whether this rule is useful or
    /// noise. A rule that flagged `MIN_SERVE_CTX * 8` would punish the one derivation
    /// shape the codebase wants; a rule that a bare `// derived-or-floor:` satisfied
    /// would report zero forever while the substrate filled up with guesses.
    #[test]
    fn a_named_derivation_passes_and_a_token_marker_does_not() {
        let mk = |raw: &str| SourceFile {
            rel: "cognition/x.rs".into(),
            production: raw.into(),
            raw: raw.into(),
        };
        // A constant times a LITERAL is a derivation from a named bound.
        assert!(DerivedBounds.check(&mk("const A: u32 = MIN_SERVE_CTX * 8;")).is_empty());
        // A constant times a CONSTANT is not.
        assert_eq!(DerivedBounds.check(&mk("let c = A_SECONDS * B_TOKENS_PER_S;")).len(), 1);
        // A live-path bound with a bare literal and nothing said about it.
        assert_eq!(DerivedBounds.check(&mk("const FILL_CAP_TOKENS: usize = 15_000;")).len(), 1);
        // The same, derived from a measurement or another named bound: fine.
        assert!(DerivedBounds
            .check(&mk("const FILL_CAP_TOKENS: usize = BOOTSTRAP_WORKING_SET as usize;"))
            .is_empty());
        // A marker with nothing after it is the shape of a reason, not a reason.
        assert_eq!(
            DerivedBounds
                .check(&mk("const FILL_CAP_TOKENS: usize = 15_000; // derived-or-floor: ok"))
                .len(),
            1
        );
        // A real reason, same line or in the block above.
        assert!(DerivedBounds
            .check(&mk(
                "const FILL_CAP_TOKENS: usize = 15_000; // derived-or-floor: a FLOOR, it only ever raises a budget"
            ))
            .is_empty());
        assert!(DerivedBounds
            .check(&mk(
                "// derived-or-floor: the protocol's own stated maximum, not ours to choose\nconst FRAME_MAX_TOKENS: usize = 15_000;"
            ))
            .is_empty());
        // Off a live path, a bare bound constant is not this rule's business.
        let elsewhere = SourceFile {
            rel: "audio_constants.rs".into(),
            production: "const SILENCE_THRESHOLD_MS: u64 = 704;".into(),
            raw: "const SILENCE_THRESHOLD_MS: u64 = 704;".into(),
        };
        assert!(DerivedBounds.check(&elsewhere).is_empty());
    }
}
