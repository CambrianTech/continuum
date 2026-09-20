//! The cognition pulse: per-persona "when did she last THINK" stamps.
//!
//! Exists to make the claim heartbeat HONEST. The renewal loop in
//! `airc_runtime.rs` was bound to the presence pump — "her work stays hers
//! while she breathes" — which fixed the #331 lapse-while-working incident by
//! overcorrecting into lease-immortality-while-idle: a citizen whose cognition
//! had been silent for HOURS still renewed every minute, so a stalled round
//! read as "actively held" forever and the lapsed-claim sweeper
//! (`benchmark_grade`) could never recover her finished artifact (glass-boxed
//! 2026-08-16: 600/600 inbound events filtered non-turn, zero `turn.start`,
//! three leases renewing on the minute, three written artifacts ungraded).
//!
//! The honest predicate is the renewal comment's own words — "the substrate
//! observes that she is WORKING" — and working means a recent TURN, not a live
//! process. A turn that starts and then defers on serving pressure still
//! counts: she is trying to think; starving her of a lease for the governor's
//! failure would be #384-class unfairness. Only true cognition silence lapses.
//!
//! One module owns the stamp (compression law). Keyed by persona uuid because
//! the participants live on opposite sides of the persona structs and never
//! share a struct to hang it off: two WRITERS — the service loop (turn start)
//! and a live `agent/solve` drive tick — and one READER, the airc runtime's
//! heartbeat task (the renewal gate).
//!
//! Both writers stamp on WORK ACTUALLY HAPPENING. That is the whole contract,
//! and the reason there is no spawn/boot/presence writer — see `touch`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use uuid::Uuid;

fn pulse() -> &'static Mutex<HashMap<Uuid, u64>> {
    static PULSE: OnceLock<Mutex<HashMap<Uuid, u64>>> = OnceLock::new();
    PULSE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Stamp "she is thinking NOW". Called at every service-loop turn start, and
/// on each tick of a live `agent/solve` drive (a running solve IS cognition,
/// and produces no airc turn — #425).
///
/// THERE IS DELIBERATELY NO SPAWN STAMP, and this doc used to claim one.
/// A birth stamp lived in `airc_runtime.rs` and was REMOVED on purpose: it
/// re-armed a full lease-length of "earned" renewals on every core restart, so
/// the renewal loop resurrected already-lapsed claims faster than the 180s
/// sweeper could observe them, and an entire overnight round (2026-08-16) sat
/// "actively held" with zero turns. The removal site carries the full account
/// — see the `NO birth stamp` comment above the heartbeat task. Read it before
/// adding any caller that stamps on a lifecycle event rather than on work.
///
/// (Corrected 2026-08-21, after this stale sentence talked me into re-adding
/// exactly the stamp that had been deleted. A doc describing a writer that no
/// longer exists reads identically to missing wiring.)
pub fn touch(persona_id: Uuid, now_ms: u64) {
    if let Ok(mut map) = pulse().lock() {
        map.insert(persona_id, now_ms);
    }
}

/// Milliseconds since her last stamped cognition. `None` = never stamped in
/// this process — callers decide the posture; the renewal gate treats it as
/// NOT earned (an unstamped persona renewing forever is the exact lie this
/// module exists to end).
///
/// `None` is therefore the EXPECTED reading for a freshly-booted citizen who
/// has not yet thought, and her holds lapsing within one TTL of boot is the
/// designed outcome, not a bug: she can re-claim, and the sweeper grades any
/// artifact she left behind. Do not "fix" it with a lifecycle stamp.
pub fn idle_ms(persona_id: Uuid, now_ms: u64) -> Option<u64> {
    pulse()
        .lock()
        .ok()
        .and_then(|map| map.get(&persona_id).copied())
        .map(|last| now_ms.saturating_sub(last))
}

/// THE renewal contract, pure for the test: a claim renewal is earned only by
/// cognition within one lease-length. Silence longer than the lease means the
/// hold lapses naturally — which is recoverable (she can re-claim, #2286) and
/// productive (the lapsed-claim sweeper grades any artifact she left behind).
pub fn renewal_earned(idle: Option<u64>, ttl_ms: u64) -> bool {
    matches!(idle, Some(ms) if ms <= ttl_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the renewal contract itself. Renewal-on-presence is
    // the regression this module ends — if an idle-beyond-one-lease or
    // never-stamped persona earns renewal again, leases go immortal, stalled
    // rounds read as "actively held" forever, and the lapsed-claim sweeper is
    // structurally unreachable (the 2026-08-16 stalemate).
    #[test]
    fn renewal_is_earned_only_by_recent_cognition() {
        let ttl = crate::modules::work::DEFAULT_CLAIM_TTL_MS;
        let persona = Uuid::from_u128(7);
        // never stamped → not earned
        assert!(!renewal_earned(idle_ms(persona, 1_000), ttl));
        // fresh cognition → earned, right up to one lease-length
        touch(persona, 1_000);
        assert!(renewal_earned(idle_ms(persona, 1_000), ttl));
        assert!(renewal_earned(idle_ms(persona, 1_000 + ttl), ttl));
        // silent past one lease-length → lapses naturally
        assert!(!renewal_earned(idle_ms(persona, 1_000 + ttl + 1), ttl));
        // she thinks again → earned again
        touch(persona, 2_000 + ttl);
        assert!(renewal_earned(idle_ms(persona, 2_000 + ttl), ttl));
    }
}
