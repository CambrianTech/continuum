//! `LiveExpertObserver` — the continuum-side sink for MoE expert selections observed
//! during serving (the second half of the OBSERVE seam).
//!
//! core/llama's ggml eval-callback calls [`observe`](llama::ExpertObserver::observe) on
//! every `ffn_moe_topk` node; this tallies per-expert hits into a concurrent map (the
//! callback runs on a backend compute thread), which [`snapshot_hits`] folds into an
//! [`ExpertActivationProfile`] for `plan_expert_residency`. "measured beats predicted" —
//! this IS the measurement the whole residency brain was built to consume but had no
//! source for (`hits` was empty).
//!
//! Lane seam: core/llama → `observe(layer, experts)` → THIS tally → `snapshot_hits` →
//! `ExpertActivationProfile::hits` → `plan_expert_residency` → BigMama's pager. This file
//! is the continuum end of the callback.

use std::collections::HashMap;
use std::sync::Arc;

use dashmap::DashMap;

use super::expert_residency::ExpertId;

/// Lock-cheap live tally of expert firings, fed by the serving path's ggml eval-callback.
/// `Arc`-shared: the SAME instance is set as `ContextParams.expert_observer` (incremented
/// from the backend thread) AND read by the residency planner (governor tick). DashMap
/// keeps the hot-path increment shard-local — never a global lock across a decode
/// ([[rtos-brain-no-region-on-hot-path]] — the observe path stays cheap).
#[derive(Debug, Default)]
pub struct LiveExpertObserver {
    hits: DashMap<ExpertId, u64>,
}

impl LiveExpertObserver {
    /// A shared observer to hand to `ContextParams.expert_observer` and keep a clone of
    /// for the planner to read.
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Fold the live tally into a plain map for [`ExpertActivationProfile::hits`]. A
    /// SNAPSHOT, not a drain — the tally keeps accumulating; the caller applies decay so
    /// the profile tracks the current task, not all history.
    pub fn snapshot_hits(&self) -> HashMap<ExpertId, u64> {
        self.hits.iter().map(|e| (*e.key(), *e.value())).collect()
    }

    /// Total selections observed — a cheap liveness signal ("are we seeing routing at
    /// all?"), useful to assert the callback is actually firing before trusting the plan.
    pub fn total_hits(&self) -> u64 {
        self.hits.iter().map(|e| *e.value()).sum()
    }
}

impl llama::ExpertObserver for LiveExpertObserver {
    fn observe(&self, layer: u32, experts: &[i32]) {
        for &e in experts {
            // Router indices are non-negative; guard defensively so a stray -1 (padding /
            // a not-selected slot) never keys the map at u32::MAX.
            if e < 0 {
                continue;
            }
            let id = ExpertId { layer, expert: e as u32 };
            *self.hits.entry(id).or_insert(0) += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use llama::ExpertObserver;

    // what this catches: the OBSERVE tally — repeated (layer, expert) selections accumulate
    // per ExpertId, negatives are skipped, and the snapshot matches. This is the continuum
    // end of the eval-callback seam: a truthful live PGO signal into plan_expert_residency.
    #[test]
    fn observe_tallies_per_expert_skips_negatives_and_snapshots() {
        let obs = LiveExpertObserver::default();
        // layer 5: two tokens pick experts [3,7] then [3,1] → expert 3 fires twice.
        obs.observe(5, &[3, 7, 3, 1]);
        // layer 6: a -1 padding slot must be skipped, never keyed.
        obs.observe(6, &[0, -1, 2]);
        let snap = obs.snapshot_hits();
        assert_eq!(snap.get(&ExpertId { layer: 5, expert: 3 }), Some(&2));
        assert_eq!(snap.get(&ExpertId { layer: 5, expert: 7 }), Some(&1));
        assert_eq!(snap.get(&ExpertId { layer: 5, expert: 1 }), Some(&1));
        assert_eq!(snap.get(&ExpertId { layer: 6, expert: 0 }), Some(&1));
        assert_eq!(snap.get(&ExpertId { layer: 6, expert: 2 }), Some(&1));
        // 4 valid + 2 valid; the -1 contributed nothing.
        assert_eq!(obs.total_hits(), 6);
        assert_eq!(snap.len(), 5);
    }
}
