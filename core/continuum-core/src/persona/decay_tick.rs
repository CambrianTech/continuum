//! Hippocampus decay tick — completes the source/drain pair at the
//! engram-metadata layer.
//!
//! ### What this module is
//!
//! Pure-function `apply_decay_sweep` that iterates a
//! `RecallMetadataRegistry`'s engrams and applies Algorithm 4 decay
//! to each. Returns a `DecayTickStats` describing what happened.
//!
//! Per [[source-drain-is-the-universal-pattern]]: admission is the
//! source (slice 6 wired this), decay is the drain (this slice
//! completes the pair). The substrate stays alive because every
//! source has a drain — slice 6 + slice 8 together = the engram-
//! metadata layer's source/drain pair is now complete.
//!
//! ### What this module is NOT (yet)
//!
//! - NOT a `ServiceModule` — slice 8.5+ wraps this in the
//!   hippocampus sleep-region tick once the cognition aggregate has
//!   a multi-persona registry holder. The pure-function form here
//!   is what that ServiceModule's tick body will call.
//! - NOT multi-persona — operates on a single registry at a time.
//!   The aggregation across personas lives one tier up.
//!
//! ### Doctrine alignment
//!
//! - [[RTOS-brain-no-region-on-hot-path]]: this runs in the sleep-
//!   region's tick when wrapped as a ServiceModule, never on the
//!   cognition hot path. The pure-function form here is what that
//!   tick body calls.
//! - [[substrate-is-a-good-citizen-on-the-host]]: structurally
//!   incapable of double-decay (RecallMetadata's `last_decayed_ms`
//!   field enforces the invariant per slice 5's cleanup); cheap
//!   sweep — `engram_ids()` + per-engram `apply_decay` is O(N)
//!   over the working set, no allocations on the hot path beyond
//!   the engram_ids() Vec.

use std::sync::Arc;

use crate::persona::recall_metadata::RecallMetadataRegistry;

/// Outcome of one decay sweep across a registry. Per the
/// [[substrate-is-a-good-citizen-on-the-host]] "observability
/// honest" rule, the caller sees exactly what happened so telemetry
/// + future tuning can read the substrate's behavior at run time.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DecayTickStats {
    /// Number of engrams scanned (registry size at sweep start).
    pub engrams_scanned: u32,
    /// Number of engrams that had decay actually applied (delta>0,
    /// not protected, not already-up-to-date).
    pub engrams_decayed: u32,
    /// Number of engrams skipped because their novelty protection
    /// window was still active.
    pub engrams_protected: u32,
    /// Number of engrams skipped because `now_ms <=
    /// last_decayed_ms` (clock skew / racing tick / engram only
    /// just admitted and last_decayed already at now).
    pub engrams_no_op: u32,
    /// Number of engram_ids that were in the snapshot but had no
    /// entry by the time we tried to update them (eviction raced
    /// with sweep). Recorded for visibility — should normally be 0.
    pub engrams_disappeared: u32,
}

impl DecayTickStats {
    /// True when every scanned engram resolved to decayed +
    /// protected + no_op + disappeared. Useful as an internal
    /// consistency check.
    pub fn accounting_balances(&self) -> bool {
        self.engrams_scanned
            == self.engrams_decayed
                + self.engrams_protected
                + self.engrams_no_op
                + self.engrams_disappeared
    }
}

/// Apply Algorithm 4 decay to every engram currently tracked in
/// `registry`. Returns stats describing the sweep.
///
/// Per [[substrate-is-a-good-citizen-on-the-host]] async-everywhere
/// rule: this function itself doesn't do I/O, so it stays sync.
/// The caller (sleep-region tick) is the async one.
///
/// Per the doctrine that invariants live in the data structure:
/// double-decay is structurally impossible because
/// `RecallMetadataRegistry::apply_decay` uses `last_decayed_ms`
/// internally (see slice 5 cleanup, commit `d2f90d6b7`). This
/// sweep is safe to call any number of times with the same
/// `now_ms` — repeat calls all see delta=0 on the second pass and
/// are no-ops.
pub fn apply_decay_sweep(registry: &Arc<RecallMetadataRegistry>, now_ms: u64) -> DecayTickStats {
    let mut stats = DecayTickStats::default();
    let engram_ids = registry.engram_ids();
    stats.engrams_scanned = engram_ids.len() as u32;
    for engram_id in engram_ids {
        // Sample BEFORE the decay call so we can classify the outcome
        // without depending on the inner DashMap's atomicity details.
        let before = match registry.get(engram_id) {
            Some(m) => m,
            None => {
                stats.engrams_disappeared = stats.engrams_disappeared.saturating_add(1);
                continue;
            }
        };
        if before.is_protected(now_ms) {
            stats.engrams_protected = stats.engrams_protected.saturating_add(1);
            continue;
        }
        if now_ms <= before.last_decayed_ms {
            stats.engrams_no_op = stats.engrams_no_op.saturating_add(1);
            continue;
        }
        registry.apply_decay(engram_id, now_ms);
        stats.engrams_decayed = stats.engrams_decayed.saturating_add(1);
    }
    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::recall_metadata::RecallMetadata;
    use uuid::Uuid;

    #[test]
    fn empty_registry_no_ops() {
        let r = Arc::new(RecallMetadataRegistry::new());
        let stats = apply_decay_sweep(&r, 1_000_000);
        assert_eq!(stats, DecayTickStats::default());
        assert!(stats.accounting_balances());
    }

    #[test]
    fn single_engram_decayed() {
        let r = Arc::new(RecallMetadataRegistry::new());
        let id = Uuid::new_v4();
        r.admit(
            id,
            RecallMetadata {
                salience: 0.8,
                last_decayed_ms: 0,
                ..Default::default()
            },
        );
        let stats = apply_decay_sweep(&r, 7_200_000); // 2h
        assert_eq!(stats.engrams_scanned, 1);
        assert_eq!(stats.engrams_decayed, 1);
        assert_eq!(stats.engrams_protected, 0);
        assert_eq!(stats.engrams_no_op, 0);
        assert!(stats.accounting_balances());

        let after = r.get(id).unwrap();
        assert!(after.salience < 0.8, "salience should have decayed");
        assert_eq!(after.last_decayed_ms, 7_200_000);
    }

    #[test]
    fn protected_engram_skipped() {
        let r = Arc::new(RecallMetadataRegistry::new());
        let id = Uuid::new_v4();
        r.admit(
            id,
            RecallMetadata {
                salience: 0.8,
                protected_until_ms: 100_000_000_000,
                last_decayed_ms: 0,
                ..Default::default()
            },
        );
        let stats = apply_decay_sweep(&r, 7_200_000);
        assert_eq!(stats.engrams_scanned, 1);
        assert_eq!(stats.engrams_protected, 1);
        assert_eq!(stats.engrams_decayed, 0);
        assert!(stats.accounting_balances());

        let after = r.get(id).unwrap();
        assert_eq!(after.salience, 0.8, "protected salience must not decay");
    }

    #[test]
    fn now_at_or_before_last_decayed_is_no_op() {
        let r = Arc::new(RecallMetadataRegistry::new());
        let id = Uuid::new_v4();
        r.admit(
            id,
            RecallMetadata {
                salience: 0.8,
                last_decayed_ms: 5_000_000,
                ..Default::default()
            },
        );
        // Tick at now < last_decayed (clock skew).
        let stats = apply_decay_sweep(&r, 1_000_000);
        assert_eq!(stats.engrams_scanned, 1);
        assert_eq!(stats.engrams_no_op, 1);
        assert_eq!(stats.engrams_decayed, 0);
        assert!(stats.accounting_balances());

        // Tick at now == last_decayed (immediate refire).
        let stats2 = apply_decay_sweep(&r, 5_000_000);
        assert_eq!(stats2.engrams_no_op, 1);
        assert_eq!(stats2.engrams_decayed, 0);
    }

    #[test]
    fn multiple_engrams_classified_correctly() {
        let r = Arc::new(RecallMetadataRegistry::new());
        let decayable = Uuid::new_v4();
        let protected = Uuid::new_v4();
        let stale = Uuid::new_v4();
        r.admit(
            decayable,
            RecallMetadata {
                salience: 0.7,
                last_decayed_ms: 0,
                ..Default::default()
            },
        );
        r.admit(
            protected,
            RecallMetadata {
                salience: 0.9,
                protected_until_ms: 100_000_000_000,
                last_decayed_ms: 0,
                ..Default::default()
            },
        );
        r.admit(
            stale,
            RecallMetadata {
                salience: 0.5,
                last_decayed_ms: 10_000_000,
                ..Default::default()
            },
        );

        let stats = apply_decay_sweep(&r, 5_000_000);
        assert_eq!(stats.engrams_scanned, 3);
        assert_eq!(
            stats.engrams_decayed, 1,
            "only `decayable` should have decayed"
        );
        assert_eq!(stats.engrams_protected, 1);
        assert_eq!(stats.engrams_no_op, 1);
        assert_eq!(stats.engrams_disappeared, 0);
        assert!(stats.accounting_balances());

        // The `decayable` engram actually saw its salience drop.
        assert!(r.get(decayable).unwrap().salience < 0.7);
        // The other two unchanged.
        assert_eq!(r.get(protected).unwrap().salience, 0.9);
        assert_eq!(r.get(stale).unwrap().salience, 0.5);
    }

    #[test]
    fn repeated_sweeps_with_same_now_are_idempotent() {
        let r = Arc::new(RecallMetadataRegistry::new());
        let id = Uuid::new_v4();
        r.admit(
            id,
            RecallMetadata {
                salience: 0.8,
                last_decayed_ms: 0,
                ..Default::default()
            },
        );
        // First sweep decays.
        let first = apply_decay_sweep(&r, 7_200_000);
        assert_eq!(first.engrams_decayed, 1);
        let after_first = r.get(id).unwrap();

        // Second sweep at SAME now should be no-op (last_decayed_ms
        // now equals now_ms after the first sweep).
        let second = apply_decay_sweep(&r, 7_200_000);
        assert_eq!(second.engrams_decayed, 0);
        assert_eq!(second.engrams_no_op, 1);
        let after_second = r.get(id).unwrap();
        assert_eq!(
            after_first.salience, after_second.salience,
            "repeated sweep at same now must not double-decay"
        );
    }
}
