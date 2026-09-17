//! PeerBreaker — the remote-lane breaker keyed by the PEER, not the persona.
//!
//! The remote-lane breaker counts deadline misses and, at [`COLD_AFTER_DEADLINES`]
//! in a row, refuses requests for [`COLD_WINDOW`] instead of burning turns. It used
//! to live as two atomics ON each [`AircRemoteInferenceAdapter`](super::adapter),
//! and every remote lane is built PER PERSONA (`remote_lane_factory` /
//! `PlacementSwitch::go_remote` each `Arc::new(adapter)`) — so a per-PEER fault (one
//! dead grid host) was counted once PER CITIZEN.
//!
//! Measured on the Intel tier 2026-09-17 (card ad96f5d1): eight personas exiled to
//! one dead peer, SIX 600 s deadline misses over fifty minutes, and only ONE citizen
//! rescued. The first three misses landed on three different adapters, each stuck at
//! 1; only when three misses happened to hit one persona in a row did her counter
//! reach 3, trip cold, and fall her home. A breaker keyed on the PEER would have
//! tripped at the THIRD miss — thirty minutes earlier — and fallen all eight home at
//! once, because the evidence is about the peer, not about who happened to ask.
//!
//! So the breaker keys on the fault's true cardinality: ONE breaker per peer, shared
//! by every adapter that targets it. An answer from ANY persona resets it (the peer
//! evaluated someone's mail, so it is alive for all of them); the cold window is
//! re-measured once for all; an adapter with no pinned peer (a dream, a test) keeps a
//! private breaker with no one to share it with. Pure: [`PeerBreaker`] takes `now_ms`
//! as an argument so the trip/reset/re-measure arithmetic is assertable with no clock.
//!
//! Same shape as the placement-switch registry
//! (`persona::placement_switch` `SWITCHES`): a process-global `DashMap` keyed by the
//! stable id, entries created on first need.

use dashmap::DashMap;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock};
use std::time::Duration;

/// Deadline misses in a row before a peer reads cold. Measured 2026-09-06: two
/// citizens bound to a peer whose command pump had died sent six requests in 100 s,
/// each waiting the full deadline, each burning a turn. Now summed ACROSS the
/// personas on that peer (card ad96f5d1), not counted once each.
pub const COLD_AFTER_DEADLINES: u32 = 3;
/// How long a cold peer stays cold before ONE request is let through to re-measure it.
pub const COLD_WINDOW: Duration = Duration::from_secs(300);

/// One peer's liveness breaker: deadline misses in a row and, once tripped, the
/// instant it may be re-measured. Shared across every adapter that targets the peer
/// via [`breaker_for`], so their evidence about one dead host SUMS.
#[derive(Debug, Default)]
pub struct PeerBreaker {
    /// Deadline misses in a row, across every caller. Reset by any answer.
    consecutive_deadlines: AtomicU32,
    /// While `now < cold_until_ms` the peer is COLD: requests are refused without a
    /// wire round trip. 0 = warm.
    cold_until_ms: AtomicU64,
}

impl PeerBreaker {
    /// Whether the peer currently refuses requests without a round trip.
    pub fn is_cold(&self, now_ms: u64) -> bool {
        let until = self.cold_until_ms.load(Ordering::Relaxed);
        until != 0 && now_ms < until
    }

    /// Fold ONE deadline miss. Returns `Some(deadlines)` when THIS miss tripped the
    /// peer cold — so the caller emits the `remote_lane.cold` probe carrying its own
    /// peer + persona labels — and `None` while still warming. On the trip the count
    /// resets so the next window starts fresh.
    pub fn observe_timeout(&self, now_ms: u64) -> Option<u32> {
        let n = self.consecutive_deadlines.fetch_add(1, Ordering::Relaxed) + 1;
        if n < COLD_AFTER_DEADLINES {
            return None;
        }
        let until = now_ms.saturating_add(COLD_WINDOW.as_millis() as u64);
        // Claim the trip with a CAS so concurrent misses at the threshold emit the
        // cold probe EXACTLY once (Cormac's #4173 review: a plain `fetch_add` + `>=`
        // lets two callers both cross — n=3 and n=4 — and both fire; harmless for the
        // refusal, since `cold_until` just gets written twice to nearly the same
        // instant, but it double-counts in the trip-frequency probe someone will
        // measure with). Only the caller that flips it from warm/expired to cold wins
        // and reports; the rest see a set `cold_until` and return None.
        let prev = self.cold_until_ms.load(Ordering::Relaxed);
        let warm = prev == 0 || now_ms >= prev;
        if warm
            && self
                .cold_until_ms
                .compare_exchange(prev, until, Ordering::Relaxed, Ordering::Relaxed)
                .is_ok()
        {
            self.consecutive_deadlines.store(0, Ordering::Relaxed);
            Some(n)
        } else {
            None
        }
    }

    /// Fold ANY answer from the peer (even a refusal): it evaluated our mail, so it is
    /// alive — reset the count for everyone routed to it.
    pub fn observe_answer(&self) {
        self.consecutive_deadlines.store(0, Ordering::Relaxed);
    }
}

/// Process-global: one breaker per peer id, created on first need. The shared state
/// that lets N personas' evidence about one dead peer SUM to the threshold instead of
/// each falling short of it alone (card ad96f5d1).
///
/// EVICTION DECISION (Cormac's #4173 review; CLAUDE.md's stated-bound house rule, the
/// 2026-07-13 incident): never evicted, and it does not need to be. Two atomics per
/// DISTINCT peer id ever seen — the grid's peers are a handful, not user-scaled data —
/// so the map is bounded by the fleet size and its footprint is negligible. A stated
/// bound, not an assumption.
static PEER_BREAKERS: LazyLock<DashMap<String, Arc<PeerBreaker>>> = LazyLock::new(DashMap::new);

/// The shared breaker for `peer`, created on first need. Every adapter that pins this
/// peer folds into and reads THIS breaker, so a peer death is learned once, for all.
pub fn breaker_for(peer: &str) -> Arc<PeerBreaker> {
    PEER_BREAKERS
        .entry(peer.to_string())
        .or_insert_with(|| Arc::new(PeerBreaker::default()))
        .clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card ad96f5d1, 2026-09-17): the breaker keys on the PEER, so
    // N adapters' evidence about ONE dead peer sums. Before this, each persona carried
    // her own counter and a per-peer fault needed COLD_AFTER_DEADLINES misses PER
    // citizen — measured 6 misses / 50 min rescued 1 of 8. Here three misses across
    // three DIFFERENT callers to one peer trip it once, cold for every caller.
    #[test]
    fn a_peer_keyed_breaker_sums_evidence_across_callers() {
        const NOW: u64 = 1_000_000;
        let peer = "peerkey-test-sum"; // unique to this test: the registry is process-global
        let a = breaker_for(peer);
        let b = breaker_for(peer);
        let c = breaker_for(peer);
        assert_eq!(a.observe_timeout(NOW), None, "miss 1 of 3, not cold yet");
        assert!(!b.is_cold(NOW), "one miss is not cold — for any caller");
        assert_eq!(b.observe_timeout(NOW), None, "miss 2 of 3");
        assert_eq!(
            c.observe_timeout(NOW),
            Some(COLD_AFTER_DEADLINES),
            "the third miss trips it, whoever carried it"
        );
        assert!(a.is_cold(NOW), "cold for EVERY caller on the peer, not just the one who tripped it");
        // A further miss in the same window does NOT re-trip (the trip reset the
        // count, and the CAS would refuse a second claim while cold anyway): the cold
        // probe fires once per trip, not once per miss after it.
        assert_eq!(a.observe_timeout(NOW), None, "no second trip in the same cold window");
        let after = NOW + COLD_WINDOW.as_millis() as u64;
        assert!(!a.is_cold(after), "warm again once the cold window passes");
    }

    // what this catches: an answer from ANY caller proves the peer is alive, so it
    // resets the shared count — a peer that answered someone is not one miss from cold.
    #[test]
    fn an_answer_from_any_caller_resets_the_shared_count() {
        const NOW: u64 = 2_000_000;
        let peer = "peerkey-test-reset";
        let a = breaker_for(peer);
        let b = breaker_for(peer);
        assert_eq!(a.observe_timeout(NOW), None);
        assert_eq!(a.observe_timeout(NOW), None); // two misses banked
        b.observe_answer(); // the peer answered SOMEONE → alive for all
        assert_eq!(a.observe_timeout(NOW), None, "reset: this is miss 1 again, not the trip");
        assert_eq!(a.observe_timeout(NOW), None, "miss 2");
        assert_eq!(a.observe_timeout(NOW), Some(COLD_AFTER_DEADLINES), "miss 3 trips");
    }
}
