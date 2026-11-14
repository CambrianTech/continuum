//! **A wait never decides flow** (CONTINUUM-ENGINE.md, RTOS rule 10).
//!
//! A stage emits what it delivers and whatever needs it subscribes. A sleep, a poll interval, a
//! periodic tick or a stopwatch moves work forward only as fast as the machine it was tuned on:
//! `continuum stop` waited 100 s on a timer for a core that exits in 2.2 s (2026-11-13/14), and
//! two CI tests fail on any machine slower than the one their duration was picked on.
//!
//! The sites, each a token shape (never text in a string or a comment):
//! - a call to `sleep`, `sleep_until` (`thread::sleep`, `tokio::time::sleep`, a clock's `.sleep`);
//! - a call to `interval` or `interval_at`, and a module's `tick_interval: Some(..)`;
//! - a `const` or `static` typed `Duration`: a wait's length written down instead of the event;
//! - a call to `timeout`, `timeout_at` or `.recv_timeout` whose `Err` side does not fail: the
//!   elapsed case is matched, `if let`-ed or bound and then carries on (`continue`, a default, a
//!   retry). A timeout that propagates (`?`), panics (`expect`), or whose `Err` arm returns an
//!   error is the last-resort error path rule 10 allows, and is not a site.
//!
//! The one exemption is a remote that may never answer, named on the line or in the comment block
//! above: `// event-exempt: <the remote, and the error event watched first>`. A reason under
//! [`crate::lexer::MIN_REASON_CHARS`] is no reason.
//!
//! [`crate::time_bounds`] (unjustified duration constants on the live paths) and
//! [`crate::wall_clock`] (epoch reads) count neither calls nor `Duration` constants: this does.

use std::ops::Range;

use super::{Shape, Site, Unit};
use crate::lexer::marker_with_reason;
use crate::lexer::tokens::{matching_close, matching_open, Kind, Token};

pub struct WaitsThatDecideFlow;

/// The exemption marker.
pub const EVENT_EXEMPT: &str = "event-exempt:";

const SLEEP: &str = "a sleep moving work forward";
const INTERVAL: &str = "an interval polling instead of subscribing";
const TICK: &str = "a periodic module tick";
const DURATION_CONST: &str = "a Duration constant: a wait's length in place of its event";
const TIMEOUT: &str = "a timeout whose Err side carries on: a stopwatch deciding flow";

impl Shape for WaitsThatDecideFlow {
    fn name(&self) -> &'static str {
        "waits_that_decide_flow"
    }

    fn advice(&self) -> &'static str {
        "Wait on the event itself: the process's exit, the port bound, the checkpoint written, a \
         `watch`/`Notify`/channel the producing stage signals. A timeout belongs only on the error \
         path to a remote that may never answer, and fails there (`?`, an `Err` arm that returns an \
         error); say so with `// event-exempt: <the remote, and the error event watched first>`. In a \
         test, assert on the completion event, never on elapsed time."
    }

    fn sites(&self, unit: &Unit) -> Vec<Site> {
        let code = &unit.code;
        let mut out = Vec::new();
        for at in 0..code.len() {
            let Some(what) = wait_at(code, at) else { continue };
            let line = code[at].line;
            if !marker_with_reason(&unit.lines, line - 1, EVENT_EXEMPT) {
                out.push(Site { line, what });
            }
        }
        out
    }
}

/// The wait shape that starts at token `at`, if one does.
fn wait_at(code: &[Token], at: usize) -> Option<&'static str> {
    let t = &code[at];
    if t.kind != Kind::Ident {
        return None;
    }
    let prev = at.checked_sub(1).map(|p| &code[p]);
    let called = code.get(at + 1).is_some_and(|n| n.is('(')) && !prev.is_some_and(|p| p.is_ident("fn"));
    let method = prev.is_some_and(|p| p.is('.'));
    match t.text {
        "sleep" | "sleep_until" if called => Some(SLEEP),
        "interval" | "interval_at" if called && !method => Some(INTERVAL),
        // A builder's `.timeout(d)` configures a request's own deadline; the free `timeout(d, f)`
        // and the channel's `.recv_timeout(d)` are the waits.
        "timeout" | "timeout_at" if called && !method => (!fails_on_elapse(code, at)).then_some(TIMEOUT),
        "recv_timeout" if called => (!fails_on_elapse(code, at)).then_some(TIMEOUT),
        "tick_interval" => (code.get(at + 1).is_some_and(|n| n.is(':')) && code.get(at + 2).is_some_and(|n| n.is_ident("Some"))).then_some(TICK),
        "const" | "static" => duration_typed(code, at).then_some(DURATION_CONST),
        _ => None,
    }
}

/// Whether the `const`/`static` at `at` is typed `Duration` (any path to it).
fn duration_typed(code: &[Token], at: usize) -> bool {
    let mut i = at + 1;
    if code.get(i).is_some_and(|t| t.is_ident("mut")) {
        i += 1;
    }
    if !code.get(i).is_some_and(|t| t.kind == Kind::Ident) || !code.get(i + 1).is_some_and(|t| t.is(':')) {
        return false;
    }
    let ty_start = i + 2;
    let Some(eq) = code[ty_start..].iter().position(|t| t.is('=') || t.is(';')).map(|n| ty_start + n) else { return false };
    let ty = &code[ty_start..eq];
    ty.last().is_some_and(|t| t.is_ident("Duration")) && ty.iter().all(|t| t.kind == Kind::Ident || t.is(':'))
}

/// Whether the timeout called at `at` fails when it elapses: propagated or panicked on in its own
/// chain, or matched, `if let`-ed or `let ... else`-d into a side that returns an error.
fn fails_on_elapse(code: &[Token], at: usize) -> bool {
    let close = matching_close(code, at + 1);
    let (end, terminator) = expression_end(code, close + 1);
    if is_error_path(code, close + 1..end) {
        return true;
    }
    let statement = statement_keyword(code, at);
    let Some(open) = terminator else { return false };
    match statement {
        Some(Keyword::Match) if code[open].is('{') => match_err_arm(code, open).is_some_and(|arm| is_error_path(code, arm)),
        Some(Keyword::If) if code[open].is('{') => {
            let then_close = matching_close(code, open);
            // `if let Err(_) = timeout(..)` or `if timeout(..).is_err()`: the then-block is the
            // elapsed side; otherwise the else-block is.
            // unwrap_or_default: a reversed range (an unclosed group) reads as empty
            let tests_err = code.get(statement_start(code, at)..end).unwrap_or_default().iter().any(|t| t.is_ident("Err") || t.is_ident("is_err"));
            if tests_err {
                is_error_path(code, open..then_close)
            } else {
                else_block(code, then_close + 1).is_some_and(|block| is_error_path(code, block))
            }
        }
        Some(Keyword::Let) => else_block(code, open).is_some_and(|block| is_error_path(code, block)),
        _ => false,
    }
}

/// The keyword that decides how a timeout's result is consumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Keyword {
    Match,
    If,
    Let,
    While,
}

/// The nearest `match`/`if`/`let`/`while` before `at` in its statement (`if let` reads as `if`).
fn statement_keyword(code: &[Token], at: usize) -> Option<Keyword> {
    let start = statement_start(code, at);
    let mut i = at;
    while i > start {
        i -= 1;
        let t = &code[i];
        if t.is(')') || t.is(']') {
            i = matching_open(code, i).unwrap_or(i); // unwrap_or: an unmatched close is its own group
            continue;
        }
        let keyword = match t.text {
            "match" if t.kind == Kind::Ident => Keyword::Match,
            "if" if t.kind == Kind::Ident => Keyword::If,
            "while" if t.kind == Kind::Ident => Keyword::While,
            "let" if t.kind == Kind::Ident => match i.checked_sub(1).map(|p| code[p].text) {
                Some("if") => Keyword::If,
                Some("while") => Keyword::While,
                _ => Keyword::Let,
            },
            _ => continue,
        };
        return Some(keyword);
    }
    None
}

/// The first token of the statement holding `at`: after the nearest `;`, `{` or `}` before it,
/// balanced groups skipped.
fn statement_start(code: &[Token], at: usize) -> usize {
    let mut i = at;
    while i > 0 {
        let prev = &code[i - 1];
        if prev.is(';') || prev.is('{') || prev.is('}') {
            break;
        }
        i = if prev.is(')') || prev.is(']') { matching_open(code, i - 1).unwrap_or(i - 1) } else { i - 1 }; // unwrap_or: an unmatched close is its own group
    }
    i
}

/// Where the expression continuing at `from` ends: the index of its terminator, and that index
/// when it opens a block or an `else` (what consumes the value) rather than ending a statement.
fn expression_end(code: &[Token], from: usize) -> (usize, Option<usize>) {
    let mut at = from;
    while at < code.len() {
        let t = &code[at];
        if t.is('{') || t.is_ident("else") {
            return (at, Some(at));
        }
        if t.is(';') || t.is(',') || t.is(')') || t.is(']') || t.is('}') {
            return (at, None);
        }
        at = if t.is('(') || t.is('[') { matching_close(code, at) + 1 } else { at + 1 };
    }
    (at, None)
}

/// The body of the `Err(..)` arm of the match block opened at `open`, or of its `_` arm when it
/// has no `Err` arm.
fn match_err_arm(code: &[Token], open: usize) -> Option<Range<usize>> {
    let close = matching_close(code, open);
    let mut wildcard = None;
    let mut at = open + 1;
    while at < close {
        let t = &code[at];
        if t.is('(') || t.is('[') || t.is('{') {
            at = matching_close(code, at) + 1;
            continue;
        }
        let pattern_end = if t.is_ident("Err") && code.get(at + 1).is_some_and(|n| n.is('(')) {
            Some(matching_close(code, at + 1) + 1)
        } else if t.is_ident("_") {
            Some(at + 1)
        } else {
            None
        };
        if let Some(arrow) = pattern_end.filter(|&p| code.get(p).is_some_and(|n| n.is('=')) && code.get(p + 1).is_some_and(|n| n.is('>'))) {
            let body = arm_body(code, arrow + 2, close);
            if t.is_ident("Err") {
                return Some(body);
            }
            wildcard = Some(body);
        }
        at += 1;
    }
    wildcard
}

/// An arm's body starting at `from`: its block, or its expression up to the next top-level comma.
fn arm_body(code: &[Token], from: usize, block_close: usize) -> Range<usize> {
    if code.get(from).is_some_and(|t| t.is('{')) {
        return from..matching_close(code, from);
    }
    let mut at = from;
    while at < block_close && !code[at].is(',') {
        at = if code[at].is('(') || code[at].is('[') || code[at].is('{') { matching_close(code, at) + 1 } else { at + 1 };
    }
    from..at
}

/// The block of the `else` at `at` (`else { .. }`), when there is one.
fn else_block(code: &[Token], at: usize) -> Option<Range<usize>> {
    if !code.get(at)?.is_ident("else") || !code.get(at + 1)?.is('{') {
        return None;
    }
    Some(at + 1..matching_close(code, at + 1))
}

/// Whether `range` fails: propagates (`?`), builds or returns an `Err`, or panics.
fn is_error_path(code: &[Token], range: Range<usize>) -> bool {
    // unwrap_or_default: a reversed range (an unclosed group) reads as empty
    code.get(range).unwrap_or_default().iter().any(|t| {
        t.is('?') || (t.kind == Kind::Ident && matches!(t.text, "Err" | "panic" | "bail" | "unreachable" | "anyhow" | "expect" | "unwrap" | "todo"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokens::tokenize;
    use crate::shapes::{CrateDir, Half, Target, Text};

    fn found(src: &str) -> Vec<&'static str> {
        let text = Text { repo: "core/x/src/a.rs".into(), krate: CrateDir("x".into()), half: Half::Production, target: Target::Library, text: src.into() };
        let unit = Unit { text: &text, code: tokenize(&text.text).code, lines: text.text.lines().collect() };
        WaitsThatDecideFlow.sites(&unit).into_iter().map(|s| s.what).collect()
    }

    // what this catches: a new wait anywhere in the core, production or test. Fix it by waiting on
    // the event; never by raising the count.
    #[test]
    fn waits_that_decide_flow_only_fall() {
        crate::shapes::hold(WaitsThatDecideFlow);
    }

    // what this catches: the predicate missing a wait it exists for, across lines.
    #[test]
    fn every_wait_shape_is_a_site() {
        assert_eq!(found("async fn f() { tokio::time::sleep(Duration::from_millis(50)).await; }"), [SLEEP]);
        assert_eq!(found("fn f() { std::thread::sleep(KILL_POLL); }"), [SLEEP]);
        assert_eq!(found("let mut tick = tokio::time::interval(CATCH_UP_EVERY);"), [INTERVAL]);
        assert_eq!(found("ModuleConfig { tick_interval: Some(Duration::from_secs(5)), ..d }"), [TICK]);
        assert_eq!(found("const READY_POLL: std::time::Duration = Duration::from_millis(100);"), [DURATION_CONST]);
        assert_eq!(found("pub(crate) static SETTLE: Duration = Duration::ZERO;"), [DURATION_CONST]);
        assert_eq!(found("match timeout(d, rx.recv()).await { Ok(v) => v, Err(_) => continue }"), [TIMEOUT]);
        assert_eq!(found("if let Ok(v) = timeout(d, f).await { use_it(v) }"), [TIMEOUT], "no else: the elapsed case carries on");
        assert_eq!(found("let r = timeout(d, f).await;"), [TIMEOUT], "bound: its Err side is out of sight");
        assert_eq!(found("let v = timeout(d, f).await.unwrap_or_else(|_| fallback());"), [TIMEOUT]);
        assert_eq!(found("while rx.recv_timeout(d).is_err() { retry(); }"), [TIMEOUT]);
    }

    // what this catches: the rule flagging the error path rule 10 allows, or a reasoned exemption,
    // or a word that only looks like a wait, which would make the count noise.
    #[test]
    fn failing_timeouts_exemptions_and_lookalikes_are_not_sites() {
        assert!(found("let v = timeout(d, f).await?;").is_empty(), "propagated");
        assert!(found("let v = timeout(d, f).await.map_err(|_| E::Hung)?;").is_empty(), "propagated typed");
        assert!(found("let v = timeout(d, f).await.expect(\"the event\");").is_empty(), "a test's last-resort guard");
        assert!(found("match timeout(d, f).await { Ok(v) => v, Err(_) => return Err(E::Hung) }").is_empty());
        assert!(found("let Ok(v) = timeout(d, f).await else { return Err(E::Hung) };").is_empty());
        assert!(found("if timeout(d, f).await.is_err() { bail!(\"hung\") }").is_empty());
        assert!(found("// event-exempt: the GitHub API may never answer; the reset is watched first\nlet v = timeout(d, f).await;").is_empty());
        assert_eq!(found("let v = timeout(d, f).await; // event-exempt: short"), [TIMEOUT], "a reason under the bar is none");
        assert!(found("let c = Client::builder().timeout(d).build();").is_empty(), "a request's own deadline setting");
        assert!(found("fn sleep(&self) {}").is_empty(), "a definition");
        assert!(found("let s = \"sleep(1)\"; // sleep(2)").is_empty(), "text");
        assert!(found("const LIMIT: usize = 3;").is_empty());
    }
}
