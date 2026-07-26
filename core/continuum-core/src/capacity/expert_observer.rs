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
use std::sync::{Arc, Mutex};

use dashmap::DashMap;

use super::expert_predictor::{fold_transition, per_token_experts};
use super::expert_residency::ExpertId;

/// Lock-cheap live tally of expert firings, fed by the serving path's ggml eval-callback.
/// `Arc`-shared: the SAME instance is set as `ContextParams.expert_observer` (incremented
/// from the backend thread) AND read by the residency planner (governor tick). DashMap
/// keeps the hot-path increment shard-local — never a global lock across a decode
/// ([[rtos-brain-no-region-on-hot-path]] — the observe path stays cheap).
#[derive(Debug, Default)]
pub struct LiveExpertObserver {
    hits: DashMap<ExpertId, u64>,
    /// Cross-layer co-occurrence: `(predecessor, successor)` → count, lock-cheap like
    /// `hits`. The PREDICT signal (what's about to fire), vs `hits` = residency (what fired).
    cooccur: DashMap<(ExpertId, ExpertId), u64>,
    /// Predecessor occurrence count — the denominator of `P(successor | predecessor)`.
    seen: DashMap<ExpertId, u64>,
    /// The previous layer's per-token expert rows THIS forward pass — the only mutable
    /// batch state. Touched once per layer-observe during synchronous graph compute (not
    /// per token, never across await), so a single uncontended `Mutex` is correct + cheap.
    /// A DECREASE in layer index means a new forward pass began (reset), so trajectories
    /// never bleed across passes — no pass-id needed (continuous batching runs layers in
    /// order over the whole batch, so batch-position IS the token's cross-layer trajectory).
    prev: Mutex<Option<(u32, Vec<Vec<ExpertId>>)>>,
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

    /// Snapshot the cross-layer co-occurrence tally for the cold-tick predictor build:
    /// `(seen, cooccur)` as plain maps, fed to
    /// [`CrossLayerExpertPredictor::from_cooccurrence`](super::expert_predictor::CrossLayerExpertPredictor::from_cooccurrence).
    /// A SNAPSHOT, not a drain — the tally keeps accumulating; the governor tick rebuilds
    /// the predictor from the current snapshot each pass. Same pattern as `snapshot_hits`.
    pub fn snapshot_cooccurrence(
        &self,
    ) -> (HashMap<ExpertId, u64>, HashMap<(ExpertId, ExpertId), u64>) {
        let seen = self.seen.iter().map(|e| (*e.key(), *e.value())).collect();
        let cooccur = self.cooccur.iter().map(|e| (*e.key(), *e.value())).collect();
        (seen, cooccur)
    }

    /// The per-tick PREDICTED prefetch signal for `ExpertActivationProfile.predicted` — the
    /// producer's output into BigMama's ranking authority. Build a predictor from the live
    /// co-occurrence snapshot and predict from EVERY currently-hot expert as the fired
    /// context: the result is the cold experts likely to fire next given what's already hot,
    /// with noisy-OR confidence in `[0,1]`. Parameter-free (no magic top-K) — the whole hot
    /// set is the context, and `predict` excludes already-hot experts (they're resident;
    /// `hits` covers them). The driver sets `profile.predicted = observer.predicted()`; her
    /// `priority()` then folds it as the prefetch tier below any proven hit.
    pub fn predicted(&self) -> HashMap<ExpertId, f32> {
        let (seen, cooccur) = self.snapshot_cooccurrence();
        if cooccur.is_empty() {
            return HashMap::new(); // no cross-layer signal yet — nothing to prefetch
        }
        let predictor = super::expert_predictor::CrossLayerExpertPredictor::from_cooccurrence(seen, cooccur);
        let hot: Vec<ExpertId> = self.hits.iter().map(|e| *e.key()).collect();
        predictor.predict(&hot)
    }
}

impl llama::ExpertObserver for LiveExpertObserver {
    fn observe(&self, layer: u32, experts: &[i32], n_expert_used: usize) {
        // RESIDENCY — tally every selection (flat), unchanged. Lock-cheap DashMap.
        for &e in experts {
            // Router indices are non-negative; guard defensively so a stray -1 (padding /
            // a not-selected slot) never keys the map at u32::MAX.
            if e < 0 {
                continue;
            }
            let id = ExpertId { layer, expert: e as u32 };
            *self.hits.entry(id).or_insert(0) += 1;
        }

        // PREDICTION — cross-layer co-occurrence. Unpack per-token rows, and against the
        // previous layer this pass, fold each batch-position's transition into the
        // (predecessor, successor) tally. A layer DECREASE = a new forward pass → reset.
        if n_expert_used == 0 {
            return;
        }
        let rows = per_token_experts(layer, experts, n_expert_used);
        let mut prev = self.prev.lock().unwrap();
        if let Some((prev_layer, prev_rows)) = prev.as_ref() {
            if layer > *prev_layer {
                for (prev_row, curr_row) in prev_rows.iter().zip(rows.iter()) {
                    fold_transition(
                        prev_row,
                        curr_row,
                        |p, n| *self.cooccur.entry((p, n)).or_insert(0) += 1,
                        |p| *self.seen.entry(p).or_insert(0) += 1,
                    );
                }
            }
            // layer <= prev_layer → new pass; fall through and replace prev below.
        }
        *prev = Some((layer, rows));
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
        obs.observe(5, &[3, 7, 3, 1], 2);
        // layer 6: a -1 padding slot must be skipped, never keyed.
        obs.observe(6, &[0, -1, 2], 3);
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

    // what this catches: the hot-path cross-layer CAPTURE end to end — per-token rows
    // unpacked, transitions folded into the co-occurrence tally across increasing layers,
    // and the snapshot rebuilds a predictor that predicts the learned successor. A layer
    // DECREASE resets the pass so nothing bleeds. This is the observer half of PREDICT:
    // observe → snapshot_cooccurrence → CrossLayerExpertPredictor → predicted.
    #[test]
    fn observe_captures_cross_layer_cooccurrence_and_snapshot_predicts() {
        use super::super::expert_predictor::CrossLayerExpertPredictor;
        let obs = LiveExpertObserver::default();
        // One token (n_expert_used = full width). Pass 1: layer0 expert 3 → layer1 expert 7.
        obs.observe(0, &[3], 1);
        obs.observe(1, &[7], 1); // 1>0 → learn (0,3)→(1,7)
        // Pass 2 begins (layer resets to 0): must NOT learn (1,7)→(0,3) across the boundary.
        obs.observe(0, &[3], 1);
        obs.observe(1, &[7], 1); // learn (0,3)→(1,7) again

        let (seen, cooccur) = obs.snapshot_cooccurrence();
        let predictor = CrossLayerExpertPredictor::from_cooccurrence(seen, cooccur);
        let pred = predictor.predict(&[ExpertId { layer: 0, expert: 3 }]);
        assert_eq!(
            pred.get(&ExpertId { layer: 1, expert: 7 }).copied(),
            Some(1.0),
            "(0,3) preceded (1,7) in both passes → prefetch it with full confidence"
        );
        // The cross-pass bleed (1,7)→(0,3) must not exist.
        assert!(
            predictor.predict(&[ExpertId { layer: 1, expert: 7 }]).is_empty(),
            "layer-decrease reset prevents trajectories bleeding across forward passes"
        );
    }

    // what this catches: the per-tick PRODUCER seam — observer.predicted() builds the
    // predictor from its own live co-occurrence and predicts from the whole hot set with no
    // caller having to thread the predictor. The learned successor comes back as a prefetch
    // candidate; a cold observer (no cross-layer signal yet) yields empty, never a spurious
    // prefetch. This is the one call BigMama's driver makes: profile.predicted = obs.predicted().
    #[test]
    fn predicted_builds_from_live_cooccurrence_and_is_empty_when_cold() {
        let obs = LiveExpertObserver::default();
        // Cold: no transitions observed → nothing to prefetch, not a panic or a phantom entry.
        assert!(obs.predicted().is_empty(), "no cross-layer signal → no prefetch");

        // Learn (0,3)→(1,7) over two passes; expert 3 is now hot (it fired).
        obs.observe(0, &[3], 1);
        obs.observe(1, &[7], 1);
        obs.observe(0, &[3], 1);
        obs.observe(1, &[7], 1);

        let predicted = obs.predicted();
        // The hot set {3@l0, 7@l1} is the fired context; 7@l1 is the learned successor of
        // 3@l0. It already fired (it's hot), so predict excludes it — the value here is that
        // the producer runs end-to-end and returns a bounded [0,1] map, never panics, and
        // never re-lists an already-resident expert as a prefetch.
        for (_id, p) in &predicted {
            assert!((0.0..=1.0).contains(p), "confidence stays in [0,1]");
        }
        assert!(
            !predicted.contains_key(&ExpertId { layer: 1, expert: 7 }),
            "an already-hot expert is resident, never a prefetch candidate"
        );
    }
}
