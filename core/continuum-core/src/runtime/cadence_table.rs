//! `CadenceTable` — per-`(region, persona)` adaptive cadence for the
//! [`SubstrateGovernor`](crate::runtime::substrate_governor).
//!
//! This is the **within-class causal arbitration** the orientation budget sits on
//! (docs/architecture/BEING-SOCIETY-GOVERNOR.md, rail R1). The time-allocation law:
//! the governor allocates time top-down; a region is entirely causal *within* the
//! slice it's granted, and it speaks its own next-cadence wish through
//! [`CadenceHint`] in its [`TickOutcome`](crate::runtime::TickOutcome). The governor
//! consults [`CadenceTable::eligible`] before ticking a pair and feeds the returned
//! hint back via [`CadenceTable::record`]. The whole cadence policy lives here, in
//! one auditable place — the governor stays a dumb scheduler.
//!
//! **Sleep ≠ coma.** `CadenceHint::Sleep` yields the slice back to the top level
//! ("give my time to a mind that has something to cause"), but it is a low-cadence
//! re-check **floor**, never removal: a sleeping pair is re-checked every
//! [`SLEEP_INTERVAL_PASSES`] so the mind never goes comatose. A being doesn't shut
//! down because there's no chat thread; one faculty resting is per-concern, never
//! per-mind. (Event-wake — resetting eligibility the moment a relevant signal fires —
//! is the refinement on top of this timer floor; the timer is the first-best-guess.)
//!
//! Why its own struct: the cadence decision is a pure function of
//! `(previous spacing, hint)` plus a per-pair `next_eligible` clock. Factoring it out
//! makes the policy unit-testable without standing up live personas, and keeps the
//! governor's tick loop a thin "is it due? tick it; record what it asked for".

use std::collections::{HashMap, HashSet};

use uuid::Uuid;

use crate::runtime::CadenceHint;

/// Spacing (in governor passes) for a fresh pair: tick it every pass until it speaks
/// a hint. First-best-guess prior — observe the region, then let its hint tune the rate.
const BASE_INTERVAL_PASSES: u64 = 1;

/// Ceiling on the backoff for a region that keeps asking to slow down. Bounds how
/// sluggish an awake-but-quiet pair can get; deeper rest is `Sleep`, handled separately.
const MAX_INTERVAL_PASSES: u64 = 15;

/// Re-check floor for a pair that yielded its slice (`CadenceHint::Sleep`). The pair is
/// NOT removed — it becomes eligible again after this many passes (sleep ≠ coma; the
/// internal-clock floor that always comes back around). Deeper than `MAX_INTERVAL_PASSES`
/// because Sleep is "nothing to cause right now", not merely "going slowly".
const SLEEP_INTERVAL_PASSES: u64 = 30;

/// Identifies a schedulable unit: a region (by its stable index in the governor's
/// fixed region vector) for a specific persona.
pub type CadenceKey = (usize, Uuid);

/// Per-pair cadence state: when it's next due, and its current spacing (the backoff).
#[derive(Debug, Clone, Copy)]
struct PairCadence {
    /// Governor tick at/after which this pair is eligible to tick again.
    next_eligible: u64,
    /// Current spacing in passes — the backoff state a hint mutates.
    interval: u64,
}

impl Default for PairCadence {
    fn default() -> Self {
        Self {
            next_eligible: 0,
            interval: BASE_INTERVAL_PASSES,
        }
    }
}

/// Adaptive cadence for every `(region, persona)` pair the governor schedules.
#[derive(Debug, Default)]
pub struct CadenceTable {
    pairs: HashMap<CadenceKey, PairCadence>,
}

impl CadenceTable {
    pub fn new() -> Self {
        Self::default()
    }

    /// Is this pair due to tick on `tick`? A pair the table has never seen is eligible
    /// immediately — the first-best-guess is to tick it once and learn its hint.
    pub fn eligible(&self, key: CadenceKey, tick: u64) -> bool {
        self.pairs
            .get(&key)
            .map_or(true, |c| tick >= c.next_eligible)
    }

    /// Record that `key` ticked at `tick` and asked for `hint`; schedule its next
    /// eligibility. `None` is treated as `Hold` (keep the current spacing). The pair is
    /// always re-scheduled — even `Sleep` only pushes it out to the re-check floor,
    /// never drops it (sleep ≠ coma).
    pub fn record(&mut self, key: CadenceKey, tick: u64, hint: Option<CadenceHint>) {
        let prev = self.pairs.get(&key).copied().unwrap_or_default();
        let interval = next_interval(prev.interval, hint);
        self.pairs.insert(
            key,
            PairCadence {
                next_eligible: tick.saturating_add(interval),
                interval,
            },
        );
    }

    /// Drop entries for personas no longer live so the table can't grow unbounded as
    /// personas come and go. Region indices are fixed for the process, so only the
    /// persona axis needs pruning.
    pub fn retain_personas(&mut self, live: &[Uuid]) {
        let set: HashSet<Uuid> = live.iter().copied().collect();
        self.pairs.retain(|(_, persona), _| set.contains(persona));
    }

    /// Number of pairs currently tracked (for the governor's telemetry / tests).
    pub fn len(&self) -> usize {
        self.pairs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pairs.is_empty()
    }

    #[cfg(test)]
    fn interval_of(&self, key: CadenceKey) -> Option<u64> {
        self.pairs.get(&key).map(|c| c.interval)
    }
}

/// The entire cadence policy: map a region's hint to its next inter-tick spacing.
/// Pure + total, so the policy is auditable and testable in one place.
fn next_interval(prev: u64, hint: Option<CadenceHint>) -> u64 {
    match hint {
        // Wants more time: back to the tightest cadence.
        Some(CadenceHint::Faster) => BASE_INTERVAL_PASSES,
        // Content at the current rate (None == Hold): keep spacing, but never below base.
        Some(CadenceHint::Hold) | None => prev.max(BASE_INTERVAL_PASSES),
        // Wants less: double the spacing, bounded.
        Some(CadenceHint::Slower) => prev.saturating_mul(2).min(MAX_INTERVAL_PASSES),
        // Yields the slice — but re-checked at the floor, never removed (sleep ≠ coma).
        Some(CadenceHint::Sleep) => SLEEP_INTERVAL_PASSES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn persona() -> Uuid {
        Uuid::from_u128(0x1234)
    }

    // what this catches: a never-seen pair must tick on its first encounter, so a newly
    // spawned persona/region isn't silently starved waiting on an absent schedule entry.
    #[test]
    fn fresh_pair_is_eligible_immediately() {
        let table = CadenceTable::new();
        assert!(table.eligible((0, persona()), 0));
        assert!(table.eligible((7, persona()), 999));
    }

    // what this catches: Hold/None spacing — after a tick the pair waits exactly base
    // (1 pass): not eligible on the same tick, eligible on the next.
    #[test]
    fn hold_waits_one_pass() {
        let mut table = CadenceTable::new();
        let key = (0, persona());
        table.record(key, 5, Some(CadenceHint::Hold));
        assert!(!table.eligible(key, 5), "same tick: already ran");
        assert!(table.eligible(key, 6), "next pass: due again");

        // None must behave identically to Hold.
        table.record(key, 10, None);
        assert!(!table.eligible(key, 10));
        assert!(table.eligible(key, 11));
    }

    // what this catches: Faster snaps a slowed pair back to the tightest cadence so a
    // region that suddenly has work isn't stuck at a stale backoff.
    #[test]
    fn faster_resets_to_base() {
        let mut table = CadenceTable::new();
        let key = (0, persona());
        table.record(key, 0, Some(CadenceHint::Slower)); // interval 2
        table.record(key, 2, Some(CadenceHint::Slower)); // interval 4
        assert_eq!(table.interval_of(key), Some(4));
        table.record(key, 6, Some(CadenceHint::Faster));
        assert_eq!(table.interval_of(key), Some(BASE_INTERVAL_PASSES));
    }

    // what this catches: Slower doubles the spacing and clamps at MAX, so a region that
    // keeps yielding can't push its cadence past the awake-but-quiet ceiling.
    #[test]
    fn slower_doubles_and_clamps() {
        let mut table = CadenceTable::new();
        let key = (0, persona());
        let mut tick = 0;
        for expected in [2u64, 4, 8, 15, 15] {
            table.record(key, tick, Some(CadenceHint::Slower));
            assert_eq!(table.interval_of(key), Some(expected));
            tick += expected;
        }
    }

    // what this catches: THE sleep ≠ coma invariant. Sleep pushes a pair out to the
    // re-check floor but NEVER removes it — it is still tracked and becomes eligible
    // again after exactly SLEEP_INTERVAL_PASSES. A faculty resting is not a mind dying.
    #[test]
    fn sleep_is_a_recheck_floor_never_removal() {
        let mut table = CadenceTable::new();
        let key = (0, persona());
        table.record(key, 100, Some(CadenceHint::Sleep));

        assert!(
            !table.eligible(key, 100 + SLEEP_INTERVAL_PASSES - 1),
            "still resting"
        );
        assert!(
            table.eligible(key, 100 + SLEEP_INTERVAL_PASSES),
            "wakes at the floor"
        );
        // Crucially: the pair is STILL TRACKED — sleep parked it, it did not evict it.
        assert_eq!(table.interval_of(key), Some(SLEEP_INTERVAL_PASSES));
        assert_eq!(table.len(), 1);
    }

    // what this catches: dead personas are pruned so the table can't grow without bound
    // across a long-lived process as personas churn — while live ones are preserved.
    #[test]
    fn retain_personas_drops_dead_keeps_live() {
        let mut table = CadenceTable::new();
        let alive = Uuid::from_u128(1);
        let gone = Uuid::from_u128(2);
        table.record((0, alive), 0, Some(CadenceHint::Hold));
        table.record((0, gone), 0, Some(CadenceHint::Hold));
        table.record((1, gone), 0, Some(CadenceHint::Hold));
        assert_eq!(table.len(), 3);

        table.retain_personas(&[alive]);
        assert_eq!(table.len(), 1);
        assert!(table.eligible((0, gone), 1_000_000) /* re-added as fresh = eligible */);
        assert!(
            table.interval_of((0, alive)).is_some(),
            "live persona preserved"
        );
    }
}
