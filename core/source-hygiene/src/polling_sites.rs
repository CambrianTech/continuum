//! **A long process is a handle and an event, never a poll.**
//!
//! Joel, 2026-11-07: "long processes use handles and events, never polling". A loop that sleeps
//! (or ticks an `interval`) and then re-reads a store, a file, a directory or a peer to find out
//! whether something changed spends a wakeup per period whether or not anything did, answers a
//! period late when it does, and multiplies by every mind on the node. The thing that changes
//! knows when it changed: it publishes (`watch`, `broadcast`, the completion of a handle), and
//! the waiter wakes on that (docs/architecture/CONCURRENCY-STYLE-GUIDE.md, "work wakes work").
//!
//! A **polling site** is a production `sleep(..)` / `sleep_until(..)` (`tokio::time::`,
//! `std::thread::`, or an awaited bare `sleep`) or an `interval`'s `.tick()`, whose innermost
//! enclosing loop (`loop`, `while`, `for .. in`) also calls one of [`REREADS`]: the calls that
//! look again at something outside the loop to detect change (the filesystem, a child process,
//! a socket or HTTP peer, a store query). A sleep in a loop that re-reads nothing (a frame pacer,
//! a select arm's timeout) is not counted; a re-read with no sleep is not a poll.
//!
//! **Owners, allowed by module path ([`CADENCE_OWNERS`]), never by a comment.** The style guide
//! names who owns a cadence: the `SubstrateGovernor` with its `CadenceTable`/`CadenceHint`
//! (regions declare a preference, the governor decides), the `ServiceModule` tick driver, the
//! pressure monitors in `system_resources/` with the `PressureBroker` tick, and one reconnect
//! owner per scope with backoff (`airc/daemon_liveness`, the daemon's liveness owner), plus the
//! host-told retry owner (`provisioning/rate_limit`, which waits exactly as long as a server's
//! `Retry-After` says). Their ticks ARE the substrate's cadence; everyone else rides them.
//!
//! The count, per crate, may only fall, and a file a change touches leaves with none: replace
//! the poll with the owner's event (a `watch::Receiver::changed()`, a handle's completion, a
//! bus subscription) or move the cadence into the owner.

use super::lexer::tokens::{matching_close, tokenize, Kind, Token};
use super::{SourceFile, SourceRule, Violation};

/// Calls that look again at something outside the loop: the filesystem, a child process, a
/// socket or HTTP peer, a store, a shared value. One of these inside a sleeping loop is how a
/// loop polls.
pub const REREADS: &[&str] = &[
    // filesystem
    "read_dir", "read_to_string", "metadata", "symlink_metadata", "exists", "try_exists", "is_file", "is_dir", "modified",
    // a child process or a peer process
    "try_wait", "Command", "pid_alive", "kill",
    // a socket, IPC or HTTP peer
    "connect", "reqwest", "fetch", "execute", "send_request", "request", "probe", "ping", "health",
    // a store query, or a shared value read again
    "query", "list", "find_many", "find_all", "select", "status", "snapshot", "read", "load", "refresh", "poll", "try_recv", "try_lock",
];

/// Prefixes of calls that look again (`read_board`, `list_cards`, `fetch_status`, `check_ready`).
pub const REREAD_PREFIXES: &[&str] = &["read_", "list_", "fetch_", "query_", "load_", "check_", "poll_", "refresh_", "scan_", "stat_"];

/// Atomic read-modify-writes share the `fetch_` prefix but write a counter; they look at nothing.
const ATOMIC_WRITES: &[&str] = &["fetch_add", "fetch_sub", "fetch_and", "fetch_or", "fetch_xor", "fetch_max", "fetch_min", "fetch_update", "fetch_nand"];

/// A wait that names itself a poll or a retry (`sleep(POLL)`, `poll.tick()`, `KILL_POLL`,
/// `sleep(retry_delay(attempt))`, `sleep(backoff)`) is one: its author said what the loop does,
/// trying again to see whether something changed. Retry and backoff have owners
/// ([`CADENCE_OWNERS`]); everywhere else they are counted.
const SELF_NAMED: &[&str] = &["poll", "retry", "backoff"];

/// Is the identifier a call that looks again?
fn rereads(ident: &str) -> bool {
    !ATOMIC_WRITES.contains(&ident) && (REREADS.contains(&ident) || REREAD_PREFIXES.iter().any(|p| ident.starts_with(p)))
}

/// The modules that own a cadence (see the module doc). Repository-path prefixes.
pub const CADENCE_OWNERS: &[&str] = &[
    "core/continuum-mind/src/persona/substrate_governor",
    "core/continuum-kernel/src/runtime/cadence_table.rs",
    "core/continuum-kernel/src/runtime/brain_region.rs",
    "core/continuum-kernel/src/runtime/module_harness.rs",
    "core/continuum-kernel/src/runtime/governor_bus.rs",
    "core/continuum-kernel/src/system_resources/memory_pressure",
    "core/continuum-kernel/src/system_resources/disk_pressure",
    "core/continuum-kernel/src/system_resources/fd_pressure",
    "core/continuum-kernel/src/paging/broker.rs",
    "core/continuum-core/src/modules/pressure_broker_module.rs",
    "core/continuum-core/src/airc/daemon_liveness.rs",
    "core/continuum-core/src/provisioning/rate_limit.rs",
];

pub struct PollingSites;

/// Is the token at `at` the start of a loop keyword in statement position (not a field or path)?
fn keyword(code: &[Token], at: usize, name: &str) -> bool {
    code[at].is_ident(name) && (at == 0 || !(code[at - 1].is('.') || code[at - 1].is(':')))
}

/// The `{` that opens the body of the loop whose keyword is at `at`, if it is a loop. A `for`
/// is a loop only with an `in` before its body (`impl X for Y {` and `for<'a>` have none).
fn loop_body_open(code: &[Token], at: usize) -> Option<usize> {
    let is_for = keyword(code, at, "for");
    if !(keyword(code, at, "loop") || keyword(code, at, "while") || is_for) {
        return None;
    }
    let (mut depth, mut saw_in) = (0usize, false);
    for (k, token) in code.iter().enumerate().skip(at + 1) {
        match token.text {
            "(" | "[" if token.kind == Kind::Punct => depth += 1,
            ")" | "]" if token.kind == Kind::Punct => depth = depth.saturating_sub(1),
            ";" | "}" if token.kind == Kind::Punct && depth == 0 => return None,
            "{" if token.kind == Kind::Punct && depth == 0 => return (!is_for || saw_in).then_some(k),
            "in" if token.kind == Kind::Ident && depth == 0 => saw_in = true,
            _ => {}
        }
    }
    None
}

/// Every loop body in `code`, as `(open, close)` token indices.
pub(crate) fn loop_bodies(code: &[Token]) -> Vec<(usize, usize)> {
    (0..code.len()).filter_map(|at| loop_body_open(code, at).map(|open| (open, matching_close(code, open)))).collect()
}

/// Does the call opened at `open` (its `(`) end in `.await` or a select arm's `=>`?
fn awaited(code: &[Token], open: usize) -> bool {
    let close = matching_close(code, open);
    let next = |k: usize| code.get(close + k);
    let dot_await = next(1).is_some_and(|t| t.is('.')) && next(2).is_some_and(|t| t.is_ident("await"));
    let arm = next(1).is_some_and(|t| t.is('=')) && next(2).is_some_and(|t| t.is('>'));
    dot_await || arm
}

/// Is the token at `at` a wait: a sleep, or an interval's tick?
pub(crate) fn waits(code: &[Token], at: usize) -> bool {
    let token = &code[at];
    let opens = code.get(at + 1).is_some_and(|t| t.is('('));
    if !opens {
        return false;
    }
    if token.is_ident("sleep") || token.is_ident("sleep_until") {
        let qualified = at >= 3 && code[at - 1].is(':') && (code[at - 3].is_ident("time") || code[at - 3].is_ident("thread"));
        return qualified || awaited(code, at + 1);
    }
    token.is_ident("tick") && at >= 1 && code[at - 1].is('.') && awaited(code, at + 1)
}

/// What the wait at `at` names itself, in its receiver (`poll.tick()`) or its argument
/// (`sleep(POLL_INTERVAL)`), if it is a poll or a retry.
fn self_named(code: &[Token], at: usize) -> Option<&'static str> {
    let receiver = at.checked_sub(2).map(|r| &code[r]);
    let argument = &code[at + 1..=matching_close(code, at + 1)];
    let names: Vec<String> = receiver.into_iter().chain(argument).filter(|t| t.kind == Kind::Ident).map(|t| t.text.to_ascii_lowercase()).collect();
    SELF_NAMED.iter().copied().find(|word| names.iter().any(|name| name.contains(word)))
}

/// The polling sites of `production`: each wait's line and the re-read its loop makes.
pub fn polling_sites(production: &str) -> Vec<(usize, &str)> {
    let code = tokenize(production).code;
    let bodies = loop_bodies(&code);
    let mut out = Vec::new();
    for at in (0..code.len()).filter(|&at| waits(&code, at)) {
        let innermost = bodies.iter().filter(|(open, close)| *open < at && at < *close).min_by_key(|(open, close)| close - open);
        let Some(&(open, close)) = innermost else {
            continue;
        };
        // A macro (`select!`, `probe!`) shares a name with a call; only a call re-reads.
        let reread = (open..close).find(|&k| code[k].kind == Kind::Ident && rereads(code[k].text) && !code.get(k + 1).is_some_and(|t| t.is('!')));
        if let Some(k) = reread {
            out.push((code[at].line, code[k].text));
        } else if let Some(word) = self_named(&code, at) {
            out.push((code[at].line, word));
        }
    }
    out
}

impl SourceRule for PollingSites {
    fn name(&self) -> &'static str {
        "polling_sites"
    }

    fn check(&self, file: &SourceFile) -> Vec<Violation> {
        if CADENCE_OWNERS.iter().any(|owner| file.repo.starts_with(owner)) {
            return Vec::new();
        }
        let lines: Vec<&str> = file.production.lines().collect();
        polling_sites(&file.production)
            .into_iter()
            .map(|(line, reread)| Violation {
                rule: self.name(),
                file: file.repo.clone(),
                line,
                source: format!("re-reads `{reread}`: {}", lines.get(line - 1).map_or("", |l| l.trim())), // map_or: a token's line is always in the text it came from
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scan;

    fn advice(violations: &[Violation]) -> impl Fn() -> String + '_ {
        move || {
            format!(
                "A sleeping loop that re-reads to detect change is a poll. Wake on the owner's event \
                 instead (a watch::Receiver::changed(), a handle's completion, a bus subscription), \
                 or move the cadence into its owner (polling_sites.rs CADENCE_OWNERS).\nFirst {} of {}:\n{}",
                violations.len().min(20),
                violations.len(),
                violations.iter().take(20).map(|v| format!("  {}:{}  {}", v.file, v.line, v.source)).collect::<Vec<_>>().join("\n")
            )
        }
    }

    // what this catches: a new poll anywhere the gates audit, or one left in a touched file.
    #[test]
    fn polling_sites_fall_and_touched_files_have_none() {
        let violations = scan(&[&PollingSites]);
        crate::ratchet::hold_sites("polling_sites", &violations, advice(&violations));
        crate::ratchet::touched_clean("polling_sites", &violations, advice(&violations));
    }

    // what this catches: the negative control. An added poll is counted, so the ratchet rises
    // and fails; a pacer, a select timeout and the cadence owner are not.
    #[test]
    fn an_added_poll_is_counted_and_a_pacer_is_not() {
        let count = |rel: &str, src: &str| PollingSites.check(&SourceFile { repo: rel.to_owned(), ..SourceFile::for_test(rel, src) }).len();
        let poll = "async fn wait() {\n    loop {\n        if path.exists() { break; }\n        tokio::time::sleep(PERIOD).await;\n    }\n}";
        assert_eq!(count("core/x/src/a.rs", poll), 1, "a sleep beside a file re-read");
        let ticked = "let mut t = interval(P);\nwhile running {\n    t.tick().await;\n    let rows = store.query(q).await;\n}";
        assert_eq!(count("core/x/src/a.rs", ticked), 1, "an interval tick beside a store query");
        let selected = "loop {\n    tokio::select! {\n        _ = t.tick() => { let _ = std::fs::read_dir(d); }\n    }\n}";
        assert_eq!(count("core/x/src/a.rs", selected), 1, "a tick as a select arm");
        let retry = "loop {\n    match attach().await { Ok(s) => return s, Err(_) => tokio::time::sleep(retry_delay(n)).await }\n}";
        assert_eq!(count("core/x/src/a.rs", retry), 1, "a retry loop names itself");
        let counter = "loop {\n    n.fetch_add(1, Relaxed);\n    std::thread::sleep(FRAME);\n}";
        assert_eq!(count("core/x/src/a.rs", counter), 0, "an atomic write looks at nothing");
        let pacer = "loop {\n    render(frame);\n    std::thread::sleep(FRAME);\n}";
        assert_eq!(count("core/x/src/a.rs", pacer), 0, "a frame pacer re-reads nothing");
        let unlooped = "async fn once() { if p.exists() { tokio::time::sleep(D).await; } }";
        assert_eq!(count("core/x/src/a.rs", unlooped), 0, "no loop, no poll");
        let impl_for = "impl Drop for Poller {\n    fn drop(&mut self) { std::thread::sleep(D); p.exists(); }\n}";
        assert_eq!(count("core/x/src/a.rs", impl_for), 0, "`impl X for Y {{` is not a loop");
        let named = "loop {\n    return sleep();\n    p.exists();\n}";
        assert_eq!(count("core/x/src/a.rs", named), 0, "a function that happens to be named sleep");
        assert_eq!(count("core/continuum-mind/src/persona/substrate_governor.rs", poll), 0, "the cadence owner");
        let commented = "loop {\n    // p.exists() then tokio::time::sleep(D).await\n    work();\n}";
        assert_eq!(count("core/x/src/a.rs", commented), 0, "a comment is not code");
    }
}
