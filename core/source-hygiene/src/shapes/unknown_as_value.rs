//! **"Not measured" is a type, never a number** (CONTINUUM-ENGINE.md, RTOS rule 11).
//!
//! A `0` that means "no reading" reads as calm to every consumer: the host deficit that returned 0
//! for both "the OS reads normal" and "no verdict", the store pressure an unknown volume mapped to
//! `0.0` (review of 2026-11-14, `~/.continuum/evidence/js-shaped-rust-2026-11-14/review.md`). The
//! sites:
//! - `.map_or(0, ..)` and `.map_or(0.0, ..)`: an absence mapped to zero;
//! - `.unwrap_or(0)` / `.unwrap_or(0.0)` / `.unwrap_or_default()` on a reading whose name says it is
//!   a measurement (`free_bytes`, `latency_ms`, `pressure`, see [`MEASUREMENT_WORDS`]);
//! - `.max(1)` guarding a division: an unknown or empty denominator turned into one;
//! - `usize::MAX` / `u32::MAX` / `u64::MAX` returned as a sentinel (`return`, a match arm, a
//!   block's value, the fallback of `unwrap_or`/`map_or`).
//!
//! Absence is `Option`, `Probed` or an enum the caller must match.

use super::items::read_name;
use super::{Shape, Site, Unit};
use crate::lexer::tokens::{matching_close, matching_open, Kind, Token};

pub struct UnknownAsValue;

const MAP_OR_ZERO: &str = "an absence mapped to zero";
const MEASUREMENT_ZERO: &str = "an unread measurement read as zero";
const DIVISOR_GUARD: &str = "`.max(1)` hiding an empty or unknown divisor";
const MAX_SENTINEL: &str = "a MAX sentinel standing for absence";

/// Name segments that mark a value as a measurement: a quantity some probe reads, whose absence
/// is "not measured", never zero.
pub const MEASUREMENT_WORDS: &[&str] = &[
    "ms", "millis", "secs", "seconds", "us", "ns", "bytes", "kib", "mib", "gib", "kb", "mb", "gb", "tokens", "rate", "rss",
    "pressure", "latency", "duration", "elapsed", "ratio", "tps", "available", "free", "used", "median", "p50", "p95",
    "p99", "mean", "avg", "capacity", "residents", "reuse", "temperature", "load", "percent", "pct", "fraction",
    "throughput", "vram", "ram", "footprint", "measured", "speed",
];

/// The unsigned integer types whose `MAX` is used as "nothing measured".
const SENTINEL_TYPES: &[&str] = &["usize", "u32", "u64"];

impl Shape for UnknownAsValue {
    fn name(&self) -> &'static str {
        "unknown_as_value"
    }

    fn advice(&self) -> &'static str {
        "Keep the absence: return `Option<T>`, `Probed<T>` or an enum (`NoVerdict`, `Calm`, \
         `Owed(NonZeroU64)`) and let the caller match it; a divisor that may be zero is a \
         `NonZero*` or a typed empty case, never `.max(1)`."
    }

    fn sites(&self, unit: &Unit) -> Vec<Site> {
        let code = &unit.code;
        let mut out = Vec::new();
        for at in 0..code.len() {
            let what = if code[at].is('.') { method_site(code, at) } else { max_sentinel(code, at).then_some(MAX_SENTINEL) };
            if let Some(what) = what {
                out.push(Site { line: code[at].line, what });
            }
        }
        out
    }
}

/// The site a `.method(` at `dot` is, if any.
fn method_site(code: &[Token], dot: usize) -> Option<&'static str> {
    let method = code.get(dot + 1)?;
    let open = dot + 2;
    if !code.get(open)?.is('(') {
        return None;
    }
    match method.text {
        "map_or" => (literal_arg(code, open, 0).is_some_and(|after| code.get(after).is_some_and(|t| t.is(',')))).then_some(MAP_OR_ZERO),
        "unwrap_or" => {
            let zero = literal_arg(code, open, 0).is_some_and(|after| code.get(after).is_some_and(|t| t.is(')')));
            (zero && reads_measurement(code, dot)).then_some(MEASUREMENT_ZERO)
        }
        "unwrap_or_default" => (code.get(open + 1)?.is(')') && reads_measurement(code, dot)).then_some(MEASUREMENT_ZERO),
        "max" => {
            let one = literal_arg(code, open, 1).is_some_and(|after| code.get(after).is_some_and(|t| t.is(')')));
            (one && guards_a_division(code, dot)).then_some(DIVISOR_GUARD)
        }
        _ => None,
    }
}

/// When the argument list opened at `open` starts with the literal `value` (`0`, `0.0`, `0u64`,
/// `1_f64`), the index just past it.
fn literal_arg(code: &[Token], open: usize, value: u8) -> Option<usize> {
    let whole = code.get(open + 1)?;
    if whole.kind != Kind::Literal || numeric_value(whole.text)? != value {
        return None;
    }
    // `0.0` lexes as `0`, `.`, `0`: a fraction of zeros keeps the value.
    let fraction = code.get(open + 2).is_some_and(|t| t.is('.')) && code.get(open + 3).is_some_and(|t| t.kind == Kind::Literal && numeric_value(t.text) == Some(0));
    Some(if fraction { open + 4 } else { open + 2 })
}

/// The value of an integer literal of one digit, past `_` separators and a type suffix; `None`
/// for anything else.
fn numeric_value(text: &str) -> Option<u8> {
    let digits: String = text.chars().take_while(|c| c.is_ascii_digit() || *c == '_').filter(|c| *c != '_').collect();
    let suffix = &text[text.find(|c: char| !(c.is_ascii_digit() || c == '_')).unwrap_or(text.len())..]; // unwrap_or: an all-digit literal has no suffix
    let typed = suffix.is_empty() || ["u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize", "f32", "f64"].contains(&suffix);
    let value: u32 = digits.parse().ok()?;
    (typed && value <= 1).then_some(value as u8)
}

/// Whether the receiver of the `.` at `dot` reads a measurement, by its name.
fn reads_measurement(code: &[Token], dot: usize) -> bool {
    dot.checked_sub(1).and_then(|end| read_name(code, end)).is_some_and(is_measurement)
}

/// Whether a snake_case name has a measurement word among its segments.
pub fn is_measurement(name: &str) -> bool {
    name.split('_').any(|segment| MEASUREMENT_WORDS.contains(&segment.to_ascii_lowercase().as_str()))
}

/// Whether the `.max(1)` at `dot` is a denominator: the operand it ends is right of `/` or `%`
/// (`/=` too), alone in parentheses right of one (`x / (n.max(1) as f64)`), or the argument of a
/// `div` method.
fn guards_a_division(code: &[Token], dot: usize) -> bool {
    let start = operand_start(code, dot);
    let Some(before) = start.checked_sub(1) else { return false };
    if divides(code, before) {
        return true;
    }
    if !code[before].is('(') {
        return false;
    }
    // The parenthesized operand: `( n.max(1) [as T] )`.
    let mut after = matching_close(code, dot + 2) + 1;
    if code.get(after).is_some_and(|t| t.is_ident("as")) {
        after += 2;
    }
    if !(code.get(after).is_some_and(|t| t.is(')')) && matching_close(code, before) == after) {
        return false;
    }
    let Some(outer) = before.checked_sub(1) else { return false };
    let div_method = code[outer].kind == Kind::Ident && code[outer].text.contains("div") && outer > 0 && code[outer - 1].is('.');
    divides(code, outer) || div_method
}

/// Whether the token at `at` is a division or remainder operator (`/`, `%`, `/=`, `%=`).
fn divides(code: &[Token], at: usize) -> bool {
    let op = |i: usize| code[i].is('/') || code[i].is('%');
    op(at) || (code[at].is('=') && at > 0 && op(at - 1))
}

/// The first token of the operand that a method call at `dot` continues: back over fields,
/// paths, calls and indexes.
fn operand_start(code: &[Token], dot: usize) -> usize {
    let mut at = dot;
    while at > 0 {
        let prev = &code[at - 1];
        if prev.is(')') || prev.is(']') {
            match matching_open(code, at - 1) {
                Some(open) => at = open,
                None => break,
            }
        } else if prev.kind == Kind::Ident || prev.is('.') || prev.is(':') {
            at -= 1;
        } else {
            break;
        }
    }
    at
}

/// Whether a `<type>::MAX` starts at `at` and stands where a value is returned or defaulted.
fn max_sentinel(code: &[Token], at: usize) -> bool {
    let named = code[at].kind == Kind::Ident
        && SENTINEL_TYPES.contains(&code[at].text)
        && code.get(at + 1).is_some_and(|t| t.is(':'))
        && code.get(at + 2).is_some_and(|t| t.is(':'))
        && code.get(at + 3).is_some_and(|t| t.is_ident("MAX"));
    if !named || at == 0 {
        return false;
    }
    let before = &code[at - 1];
    let returned = before.is_ident("return");
    let arm = before.is('>') && at >= 2 && code[at - 2].is('=');
    let fallback = before.is('(') && at >= 2 && matches!(code[at - 2].text, "unwrap_or" | "map_or");
    let block_value = (before.is('{') || before.is(';')) && code.get(at + 4).is_some_and(|t| t.is('}'));
    returned || arm || fallback || block_value
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokens::tokenize;
    use crate::shapes::{CrateDir, Half, Target, Text};

    fn found(src: &str) -> Vec<&'static str> {
        let text = Text { repo: "core/x/src/a.rs".into(), krate: CrateDir("x".into()), half: Half::Production, target: Target::Library, text: src.into() };
        let unit = Unit { text: &text, code: tokenize(&text.text).code, lines: text.text.lines().collect() };
        UnknownAsValue.sites(&unit).into_iter().map(|s| s.what).collect()
    }

    // what this catches: a new unknown stored as a value anywhere in the core.
    #[test]
    fn unknown_as_value_only_falls() {
        crate::shapes::hold(UnknownAsValue);
    }

    // what this catches: the predicate missing a shape the 2026-11-14 review found.
    #[test]
    fn every_unknown_as_value_shape_is_a_site() {
        assert_eq!(found("let r = volume.map_or(0.0, |v| v.ratio());"), [MAP_OR_ZERO]);
        assert_eq!(found("let n = pool.map_or(0, |p| p.residents);"), [MAP_OR_ZERO]);
        assert_eq!(found("let a = snap.available_bytes.unwrap_or(0);"), [MEASUREMENT_ZERO]);
        assert_eq!(found("let a = probe.free_mib().copied().unwrap_or_default();"), [MEASUREMENT_ZERO]);
        assert_eq!(found("let p = own / volume.max(1);"), [DIVISOR_GUARD]);
        assert_eq!(found("let p = used as f64 / (total.max(1) as f64);"), [DIVISOR_GUARD]);
        assert_eq!(found("let q = a.div_ceil(b.len().max(1));"), [DIVISOR_GUARD]);
        assert_eq!(found("fn f() -> usize { if none { return usize::MAX; } 3 }"), [MAX_SENTINEL]);
        assert_eq!(found("let g = growth.map_or(u32::MAX, |g| g);"), [MAX_SENTINEL]);
        assert_eq!(found("match m { None => u64::MAX, Some(v) => v }"), [MAX_SENTINEL]);
    }

    // what this catches: a count of things where zero IS the value, a clamp that divides nothing,
    // or a MAX that is a bound, flagged as an unknown.
    #[test]
    fn real_zeros_bounds_and_clamps_are_not_sites() {
        assert!(found("let n = map.get(k).copied().unwrap_or(0);").is_empty(), "a tally with no entry is zero");
        assert!(found("let lanes = fit.max(1);").is_empty(), "a clamp, not a divisor");
        assert!(found("let x = a.map_or(1, f);").is_empty());
        assert!(found("if n == u64::MAX { overflow() }").is_empty(), "a comparison");
        assert!(found("let cap = u32::MAX as usize;").is_empty(), "a bound");
        assert!(found("let s = \"x.map_or(0, f)\";").is_empty(), "text");
    }
}
