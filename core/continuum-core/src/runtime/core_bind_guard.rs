//! core_bind_guard.rs — the pre-launch RECLAIM-OR-REFUSE decision for the core socket.
//!
//! ## The missing constraint this exists to supply
//!
//! A second `continuum-core-server` on one machine is never benign: both can hold
//! the same socket path (the later one unlinks and re-binds it), and whichever the
//! kernel hands a connection to is the one that answers. A shipped fix then looks
//! intermittently broken and an unshipped one intermittently fixed — measured
//! 2026-08-14, two cores alive at once (a debug build at 23:51 and the installed
//! release at 23:54).
//!
//! `stop` learned this first (it now reaps EVERY core and shouts SPLIT BRAIN on a
//! survivor). `start` did not: its only guard was "does a core answer `ping`?", so
//! a core that is running but NOT answering — wedged, mid-boot, or bound somewhere
//! this CLI cannot reach — was invisible to it, and `start` launched a second one
//! straight on top. Same missing constraint at a second site, which by
//! [[the-same-bug-at-two-sites-is-a-missing-constraint]] means the fix is ONE
//! primitive both paths call, not a second ad-hoc check.
//!
//! ## Why a pure decision, and why it lives in the lib
//!
//! The process-table read and the ping round-trip are the caller's (they are
//! platform- and transport-shaped, and the CLI already owns both: `running_core_pids`
//! and `core_is_up`). What is worth pinning is the DECISION over their two answers —
//! four rows, exhaustive — so it can be tested without spawning a core. It lives in
//! the lib rather than the `continuum` bin because CI runs
//! `cargo test -p continuum-core --lib`; a truth table tested inside a bin would not
//! be covered by the gate that guards it.
//!
//! ## Reclaim vs refuse
//!
//! [`BindDecision::Occupied`] does NOT auto-kill. `reboot` already guards destructive
//! restarts behind live-training and live-benchmark leases and makes the operator pass
//! `--force`; a `start` that silently reaped a wedged core would route around those
//! guards. So the default is to REFUSE, loudly, naming the pids and both ways out —
//! and `start --force` is the explicit reclaim ([[fallbacks-are-illegal-fail-loud]]).
//! An implicit autostart (`ensure_core_running`) never reclaims at all: a command that
//! merely wants a live core has no business killing one.

/// What the pre-launch guard decided, given a ping answer and the process table.
/// Exhaustive over both inputs so a new state cannot be silently dropped into an
/// existing arm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BindDecision {
    /// Nothing answers and no core process exists — the socket is ours to bind.
    Free,
    /// A core answers `ping`. `start` is a no-op here; this is the idempotent path.
    /// `pids` may be EMPTY and still be this arm: something is serving that this
    /// process cannot see in its own process table (a container, another user, a
    /// forwarded socket). Serving is serving — never launch a competitor for it.
    AlreadyServing { pids: Vec<i32> },
    /// Core process(es) are running but NOTHING answers `ping` — wedged, still
    /// booting, or bound elsewhere. Binding on top of this is what manufactures a
    /// split brain, so the caller must reclaim explicitly or refuse.
    Occupied { pids: Vec<i32> },
}

/// Decide whether this process may launch a core, from the two facts the caller can
/// observe: whether a `ping` round-trip succeeded, and which `continuum-core-server`
/// pids are in the process table.
///
/// Pure and total — the caller supplies the observations, so this is testable without
/// a live core, and both `start` and the implicit autostart share ONE truth table.
pub fn decide(ping_ok: bool, running: &[i32]) -> BindDecision {
    match (ping_ok, running.is_empty()) {
        // Serving wins over process-table visibility in BOTH directions: a core that
        // answers is a core, whether or not we can see its process.
        (true, _) => BindDecision::AlreadyServing {
            pids: running.to_vec(),
        },
        (false, true) => BindDecision::Free,
        (false, false) => BindDecision::Occupied {
            pids: running.to_vec(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the ONLY row that may launch a core is "nothing answers AND
    // no core process exists". If this ever widens, `start` can bind a second core.
    #[test]
    fn free_is_the_only_launchable_row() {
        assert_eq!(decide(false, &[]), BindDecision::Free);
        for (ping, pids) in [(true, &[][..]), (true, &[101][..]), (false, &[101][..])] {
            assert_ne!(
                decide(ping, pids),
                BindDecision::Free,
                "ping={ping} pids={pids:?} must not be launchable"
            );
        }
    }

    // what this catches: the regression that motivated this module — a core that is
    // RUNNING but not answering ping must never read as launchable. `start`'s old
    // guard was ping-only, so this row silently became a second core.
    #[test]
    fn a_running_but_silent_core_is_occupied_not_free() {
        assert_eq!(
            decide(false, &[70240, 68653]),
            BindDecision::Occupied {
                pids: vec![70240, 68653]
            }
        );
    }

    // what this catches: a serving core with no visible process (container, other
    // user, forwarded socket) must still suppress the launch. Treating an empty
    // process table as "free" here would spawn a competitor for a healthy core.
    #[test]
    fn serving_with_no_visible_process_is_still_serving() {
        assert_eq!(
            decide(true, &[]),
            BindDecision::AlreadyServing { pids: vec![] }
        );
    }

    // what this catches: a ping answer must dominate the process table, so the
    // ordinary idempotent `start` on a healthy box stays a no-op rather than
    // reporting contention against the very core that is answering.
    #[test]
    fn ping_dominates_the_process_table() {
        assert_eq!(
            decide(true, &[70240]),
            BindDecision::AlreadyServing { pids: vec![70240] }
        );
    }
}
