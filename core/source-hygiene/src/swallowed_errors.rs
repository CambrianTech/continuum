//! **A failure is handled or propagated, never dropped on the floor.**
//!
//! Joel keeps saying it: no fallbacks that hide a failure, no silent defaults, and no comment
//! that excuses one ("the leak": a comment exemption is how every count stopped meaning anything).
//! A swallowed error is the quiet slower path CLAUDE.md forbids: the act carries on with a value
//! nobody measured, and the next symptom appears three layers away with no cause attached.
//!
//! This rule counts, in production code, the shapes that discard a failure:
//!
//! - `x.ok();` as a statement: a `Result` turned into an `Option` and thrown away;
//! - `let _ = <call>;`: a call's `Result` (or `#[must_use]` value) bound to nothing;
//! - `Err(_) => {}` or `Err(e) => ()`: a catch-all error arm that does nothing (an arm on a
//!   named variant, `Err(RecvError::Lagged(_)) => {}`, is a typed decision and is not counted);
//! - `if let Err(..) = x {}` with an empty body;
//! - a fallback on a `Result`: `.unwrap_or_default()` or `.unwrap_or(<literal>)` after a call
//!   that returns one (`parse`, `read_to_string`, `ok`, `try_*`, ...), and
//!   `.unwrap_or_else(|_| ..)`, whose `|_|` can only be a discarded error.
//!
//! There is NO comment exemption. A deliberate ignore is rare and gets a typed, named form: a
//! wrapper whose argument is a reason ENUM, so the reason is data a reader and a ratchet can
//! count, never prose. None exists yet (2026-11-07), so this rule holds the count and does not
//! ask a touched file to end clean (`HeldOnly::NoSanctionedForm`). The PR that added this rule
//! lists the legitimate shapes found and proposes the type
//! (`continuum_base::Ignored::because(IgnoreReason::..)`); when it lands, the rule spares that
//! form alone and moves to `Touched::EndsClean`. The total is judged; each crate's count is
//! recorded and printed, never judged alone, so a crate cut that moves sites still passes.
//!
//! The rule is lexical: it cannot see types, so a fallback on a call this list does not name is
//! not counted (under-counting, never a false site), and `let _ = <call>` is counted whatever the
//! call returns (a `()` call bound to `_` is noise worth deleting anyway).

use super::lexer::tokens::{matching_close, matching_open, tokenize, Kind, Token};
use super::{SourceFile, SourceRule, Violation};

pub struct SwallowedErrors;

/// Calls whose return is a `Result` (or, for `ok`, a `Result` already flattened to an `Option`),
/// so a fallback right after one hides a failure. A call that returns a plain value cannot be
/// followed by `.unwrap_or*` at all, so a name here that is also a plain-value method elsewhere
/// (`to_string`) is only ever the `Result` one at such a site.
const RESULT_CALLS: &[&str] = &[
    "ok", "parse", "read_to_string", "read_to_end", "read_dir", "read", "metadata", "symlink_metadata",
    "canonicalize", "from_str", "from_slice", "from_reader", "from_value", "from_utf8", "to_string",
    "to_string_pretty", "to_vec", "var", "duration_since", "elapsed", "modified", "output", "status",
    "current_dir", "join", "decode",
];

/// The shape a site has, which is also its message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Swallow {
    OkStatement,
    LetUnderscore,
    EmptyErrArm,
    EmptyIfLetErr,
    ResultFallback,
}

impl Swallow {
    fn what(self) -> &'static str {
        match self {
            Swallow::OkStatement => "a Result dropped by `.ok();`",
            Swallow::LetUnderscore => "a call's result bound to `_`",
            Swallow::EmptyErrArm => "an error arm that does nothing",
            Swallow::EmptyIfLetErr => "an `if let Err` with an empty body",
            Swallow::ResultFallback => "a fallback hiding a failed Result",
        }
    }
}

/// Every swallow in `src`, as (line, shape).
pub fn swallows(src: &str) -> Vec<(usize, Swallow)> {
    let tokens = tokenize(src);
    let code = &tokens.code;
    let mut out = Vec::new();
    for at in 0..code.len() {
        let found = if is_dot_call(code, at, "ok") && code.get(at + 4).is_some_and(|t| t.is(';')) {
            discarding_statement(code, at).then_some(Swallow::OkStatement)
        } else if code[at].is_ident("let") && code.get(at + 1).is_some_and(|t| t.is_ident("_")) && is_assign(code, at + 2) {
            let_underscore_call(code, at + 3).then_some(Swallow::LetUnderscore)
        } else if code[at].is_ident("Err") && catch_all_pattern(code, at + 1) && !code.get(at.wrapping_sub(1)).is_some_and(|t| t.is_ident("let")) {
            empty_arm(code, at + 4).then_some(Swallow::EmptyErrArm)
        } else if code[at].is_ident("if") && code.get(at + 1).is_some_and(|t| t.is_ident("let")) && code.get(at + 2).is_some_and(|t| t.is_ident("Err")) {
            empty_if_let_body(code, at + 3).then_some(Swallow::EmptyIfLetErr)
        } else if code[at].is('.') {
            result_fallback(code, at).then_some(Swallow::ResultFallback)
        } else {
            None
        };
        if let Some(shape) = found {
            out.push((code[at].line, shape));
        }
    }
    out
}

/// `.name()` starting at `at`: the dot, the name, and an empty argument list.
fn is_dot_call(code: &[Token], at: usize, name: &str) -> bool {
    code[at].is('.')
        && code.get(at + 1).is_some_and(|t| t.is_ident(name))
        && code.get(at + 2).is_some_and(|t| t.is('('))
        && code.get(at + 3).is_some_and(|t| t.is(')'))
}

/// A lone `=` at `at`: an assignment or binding, never `==`, `=>`, `<=`, `>=` or `!=`.
fn is_assign(code: &[Token], at: usize) -> bool {
    let Some(t) = code.get(at) else { return false };
    if !t.is('=') {
        return false;
    }
    let next_joins = code.get(at + 1).is_some_and(|n| (n.is('=') || n.is('>')) && n.line == t.line);
    let prev_joins = at > 0 && ["=", "!", "<", ">"].iter().any(|c| code[at - 1].kind == Kind::Punct && code[at - 1].text == *c);
    !next_joins && !prev_joins
}

/// Whether the statement holding the `.ok()` at `dot` discards its value: it is not bound by
/// `let`, assigned, returned or broken out with. Walks back over balanced groups to the
/// statement's first token.
fn discarding_statement(code: &[Token], dot: usize) -> bool {
    let mut at = dot;
    while at > 0 {
        let prev = &code[at - 1];
        if prev.is(';') || prev.is('{') || prev.is('}') {
            break;
        }
        if prev.is(')') || prev.is(']') {
            match matching_open(code, at - 1) {
                Some(open) => at = open,
                None => break,
            }
        } else {
            at -= 1;
        }
    }
    let start = &code[at];
    if start.is_ident("let") || start.is_ident("return") || start.is_ident("break") {
        return false;
    }
    !(at..dot).any(|i| is_assign(code, i))
}

/// Whether the expression after `let _ =` (starting at `from`) ends in a call: `)` or `.await`
/// right before its `;`.
fn let_underscore_call(code: &[Token], from: usize) -> bool {
    let mut at = from;
    while at < code.len() {
        let t = &code[at];
        if t.is(';') {
            return at > from && (code[at - 1].is(')') || code[at - 1].is_ident("await"));
        }
        at = if t.is('(') || t.is('[') || t.is('{') { matching_close(code, at) + 1 } else { at + 1 };
    }
    false
}

/// Whether `(` at `open` holds a catch-all pattern: `(_)` or one binding (`(e)`, `(_ignored)`).
/// A named variant (`Err(RecvError::Lagged(_)) => {}`) is a decision on a typed value, which is
/// exactly the handling this rule asks for, so it is not a site.
fn catch_all_pattern(code: &[Token], open: usize) -> bool {
    let binding = code.get(open + 1).is_some_and(|t| t.kind == Kind::Ident && t.text.starts_with(|c: char| c == '_' || c.is_ascii_lowercase()));
    code.get(open).is_some_and(|t| t.is('(')) && binding && code.get(open + 2).is_some_and(|t| t.is(')'))
}

/// Whether the tokens at `at` are `=> {}` or `=> ()`.
fn empty_arm(code: &[Token], at: usize) -> bool {
    let arrow = code.get(at).is_some_and(|t| t.is('=')) && code.get(at + 1).is_some_and(|t| t.is('>'));
    let empty = |open: char, close: char| code.get(at + 2).is_some_and(|t| t.is(open)) && code.get(at + 3).is_some_and(|t| t.is(close));
    arrow && (empty('{', '}') || empty('(', ')'))
}

/// Whether `if let Err(..) = <scrutinee> {}` has an empty body; `pattern` is the `(` after `Err`.
fn empty_if_let_body(code: &[Token], pattern: usize) -> bool {
    if !code.get(pattern).is_some_and(|t| t.is('(')) {
        return false;
    }
    let mut at = matching_close(code, pattern) + 1;
    if !is_assign(code, at) {
        return false;
    }
    at += 1;
    while at < code.len() {
        let t = &code[at];
        if t.is('{') {
            return code.get(at + 1).is_some_and(|n| n.is('}'));
        }
        at = if t.is('(') || t.is('[') { matching_close(code, at) + 1 } else { at + 1 };
    }
    false
}

/// Whether the `.` at `dot` starts a fallback that hides a failed `Result`.
fn result_fallback(code: &[Token], dot: usize) -> bool {
    let Some(method) = code.get(dot + 1) else { return false };
    if !code.get(dot + 2).is_some_and(|t| t.is('(')) {
        return false;
    }
    if method.is_ident("unwrap_or_else") {
        // `|_|`: Option's closure takes nothing, so a discarded argument is always an error.
        return code.get(dot + 3).is_some_and(|t| t.is('|'))
            && code.get(dot + 4).is_some_and(|t| t.is_ident("_"))
            && code.get(dot + 5).is_some_and(|t| t.is('|'));
    }
    let fallback = if method.is_ident("unwrap_or_default") {
        code.get(dot + 3).is_some_and(|t| t.is(')'))
    } else if method.is_ident("unwrap_or") {
        literal_argument(code, dot + 2)
    } else {
        false
    };
    fallback && dot > 0 && receiver_returns_result(code, dot - 1)
}

/// Whether the argument list opened at `open` is one literal: `0`, `""`, `-1`, `true`, `false`.
fn literal_argument(code: &[Token], open: usize) -> bool {
    let mut at = open + 1;
    if code.get(at).is_some_and(|t| t.is('-')) {
        at += 1;
    }
    let literal = code.get(at).is_some_and(|t| t.kind == Kind::Literal || t.is_ident("true") || t.is_ident("false"));
    literal && code.get(at + 1).is_some_and(|t| t.is(')'))
}

/// Whether the call that closes at `close` (`)`) is one of [`RESULT_CALLS`] or a `try_*`,
/// turbofish included (`parse::<u32>()`).
fn receiver_returns_result(code: &[Token], close: usize) -> bool {
    if !code[close].is(')') {
        return false;
    }
    let Some(open) = matching_open(code, close) else { return false };
    let mut name = open.wrapping_sub(1);
    if code.get(name).is_some_and(|t| t.is('>')) {
        let Some(generic) = matching_open(code, name) else { return false };
        name = generic.wrapping_sub(3); // `::` is two tokens before the `<`
    }
    code.get(name).is_some_and(|t| t.kind == Kind::Ident && (RESULT_CALLS.contains(&t.text) || t.text.starts_with("try_")))
}

impl SourceRule for SwallowedErrors {
    fn name(&self) -> &'static str {
        "swallowed_errors"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        let lines: Vec<&str> = file.production.lines().collect();
        swallows(&file.production)
            .into_iter()
            .map(|(line, shape)| Violation {
                rule: "swallowed_errors",
                file: file.repo.clone(),
                line,
                source: format!("{}: {}", shape.what(), lines.get(line - 1).map_or("", |l| l.trim())), // map_or: a token's line is always inside its file
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ratchet::{HeldOnly, Touched};
    use crate::scan;

    fn shapes(src: &str) -> Vec<Swallow> {
        swallows(src).into_iter().map(|(_, s)| s).collect()
    }

    // what this catches: a new swallowed error in any crate. The total may only fall (per-crate
    // counts are the receipt);
    // there is no comment exemption, and no touched-clean judgement until a typed ignore exists.
    #[test]
    fn swallowed_errors_only_fall() {
        let violations = scan(&[&SwallowedErrors]);
        let count = violations.len();
        crate::ratchet::hold_sites(
            "swallowed_errors",
            &violations,
            Touched::HeldOnly(HeldOnly::NoSanctionedForm),
            || {
                format!(
                    "Handle the failure (match its variant), propagate it (`?`), or report it where \
                     its owner reads it; a comment does not excuse a dropped error. First {} of \
                     {count} (not necessarily new):\n{}",
                    count.min(15),
                    violations.iter().take(15).map(|v| format!("  {}:{}  {}", v.file, v.line, v.source)).collect::<Vec<_>>().join("\n")
                )
            },
        );
    }

    // what this catches: the predicate missing a shape it exists for (negative control: an added
    // offender is a site), across lines and through comments, which are never an exemption.
    #[test]
    fn every_swallow_shape_is_a_site_and_a_comment_excuses_none() {
        assert_eq!(shapes("fn f() { tx.send(x).ok(); }"), [Swallow::OkStatement]);
        assert_eq!(shapes("fn f() { std::fs::remove_file(p)\n    .ok(); // best effort\n}"), [Swallow::OkStatement]);
        assert_eq!(shapes("fn f() { let _ = tx.send(x); }"), [Swallow::LetUnderscore]);
        assert_eq!(shapes("fn f() { let _ = writeln!(out, \"x\"); }"), [Swallow::LetUnderscore]);
        assert_eq!(shapes("async fn f() { let _ = h.await; }"), [Swallow::LetUnderscore]);
        assert_eq!(shapes("match r { Ok(v) => v, Err(_) => {} }"), [Swallow::EmptyErrArm]);
        assert_eq!(shapes("match r { Ok(v) => v, Err(e) => (), }"), [Swallow::EmptyErrArm]);
        assert_eq!(shapes("fn f() { if let Err(_) = save(x) {} }"), [Swallow::EmptyIfLetErr]);
        assert_eq!(shapes("let n = s.parse::<u32>().unwrap_or(0);"), [Swallow::ResultFallback]);
        assert_eq!(shapes("let t = fs::read_to_string(p).unwrap_or_default();"), [Swallow::ResultFallback]);
        assert_eq!(shapes("let v = r.ok().unwrap_or(false);"), [Swallow::ResultFallback]);
        assert_eq!(shapes("let v = r.try_into().unwrap_or(-1);"), [Swallow::ResultFallback]);
        assert_eq!(shapes("let v = load().unwrap_or_else(|_| Vec::new());"), [Swallow::ResultFallback]);
    }

    // what this catches: the predicate flagging what handles or keeps a value, which would make
    // the count noise and the rule something people route around.
    #[test]
    fn handled_bound_and_option_fallbacks_are_not_sites() {
        assert!(shapes("let x = r.ok();").is_empty(), "bound");
        assert!(shapes("fn f() { self.v = r.ok(); }").is_empty(), "assigned");
        assert!(shapes("fn f() -> Option<u8> { return r.ok(); }").is_empty(), "returned");
        assert!(shapes("fn f() -> Option<u8> { r.ok() }").is_empty(), "the block's value");
        assert!(shapes("let _guard = lock.lock();").is_empty(), "a named guard is kept alive");
        assert!(shapes("let _ = value;").is_empty(), "not a call");
        assert!(shapes("let _ = run()?;").is_empty(), "propagated");
        assert!(shapes("match r { Err(e) => return Err(e.into()), Ok(v) => v }").is_empty(), "handled");
        assert!(shapes("if let Err(e) = save(x) { report(e); }").is_empty(), "handled");
        assert!(shapes("match r { Err(RecvError::Lagged(_)) => {} Ok(v) => v }").is_empty(), "a typed variant decided on");
        assert!(shapes("let n = map.get(k).copied().unwrap_or(0);").is_empty(), "an Option's absence");
        assert!(shapes("let n = s.parse::<u32>().unwrap_or(limit);").is_empty(), "not a literal");
        assert!(shapes("let v = opt.unwrap_or_else(|| 3);").is_empty(), "Option's closure");
        assert!(shapes("let s = \"x.ok();\";").is_empty(), "inside a string");
    }
}
