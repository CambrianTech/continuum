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
//! the two writers live on opposite sides of the persona structs: the service
//! loop (turn start) and the airc runtime's heartbeat task (renewal gate).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use uuid::Uuid;

fn pulse() -> &'static Mutex<HashMap<Uuid, u64>> {
    static PULSE: OnceLock<Mutex<HashMap<Uuid, u64>>> = OnceLock::new();
    PULSE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Stamp "she is thinking NOW". Called at spawn (grace window covers the
/// post-boot deaf period, #412) and at every service-loop turn start.
pub fn touch(persona_id: Uuid, now_ms: u64) {
    if let Ok(mut map) = pulse().lock() {
        map.insert(persona_id, now_ms);
    }
}

/// Milliseconds since her last stamped cognition. `None` = never stamped in
/// this process — callers decide the posture; the renewal gate treats it as
/// NOT earned (an unstamped persona renewing forever is the exact lie this
/// module exists to end; spawn stamps immediately, so live citizens are
/// always stamped).
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

    // what this catches: THE SPAWN STAMP GOING MISSING AGAIN. This module's
    // doc promises two writers ("called at spawn … and at every service-loop
    // turn start") and rests its `None`-is-not-earned posture on the first one
    // ("spawn stamps immediately, so live citizens are always stamped"). For
    // as long as that sentence existed the spawn writer did NOT, so a citizen
    // who had not yet taken her first turn was indistinguishable from one
    // silent for hours: renewals denied from birth, holds lapsing un-renewed,
    // bench cards claimed → lapsed → re-claimed in a spin that produced
    // nothing (found live 2026-08-21; `renewal_resumed` had ZERO rows ever).
    //
    // A semantic test cannot catch this — the contract above is satisfied by
    // an unwired module. The only thing that catches it is asserting the call
    // SITE exists, with comments stripped so this file's own prose can never
    // stand in for the wiring (the #344 audit predicate, same reasoning).
    #[test]
    fn spawn_stamps_the_pulse_so_a_prefirst_turn_citizen_is_not_read_as_idle() {
        let host_rs = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("src/persona/host.rs");
        let src = std::fs::read_to_string(&host_rs)
            .unwrap_or_else(|e| panic!("cannot read {}: {e}", host_rs.display()));
        // Drop `//`-prefixed content: a doc comment describing the stamp is
        // exactly what masked its absence, so prose must not count as wiring.
        let code_only: String = src
            .lines()
            .map(|line| match line.find("//") {
                Some(idx) => &line[..idx],
                None => line,
            })
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            code_only.contains("cognition_pulse::touch"),
            "persona/host.rs no longer stamps the cognition pulse at spawn. \
             Without it `idle_ms` returns None for every citizen who has not \
             yet taken a turn, `renewal_earned(None, _)` is false, and her \
             claims lapse un-renewed from birth — restore the stamp after \
             `prime()` rather than relaxing the renewal gate."
        );
    }
}
