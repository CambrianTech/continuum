//! `CrossLayerExpertPredictor` — the PREDICT half of the K3 expert-affinity loop.
//!
//! OBSERVE ([`super::expert_observer`]) measures which experts fired (residency:
//! keep what's proven hot). This predicts which experts are ABOUT to fire, from the
//! cross-layer structure of a forward pass: when expert `(layer L, e_i)` fires, some
//! `(layer L+k, e_j)` tend to follow. If we learn those transitions, then the moment
//! early-layer experts fire we can PREFETCH the likely late-layer ones RAM→VRAM before
//! the forward pass reaches them — the depth of the pass is the prefetch window. This
//! is the frontier-lite theory win: a 594 GB MoE served from a 32 GB GPU stops paging
//! reactively (miss → stall → load) and starts paging *ahead* of the miss.
//!
//! ## The seam (locked with BigMama, 2026-07-26)
//!
//! This produces `predicted: HashMap<ExpertId, f32>`, confidence in `[0, 1]`, written
//! onto [`ExpertActivationProfile::predicted`]. Her one ranking authority folds it as
//! `hits + predicted.clamp(0,1)*0.9 + mag.tanh()*0.09` — so a proven hit (integer ≥1)
//! ALWAYS outranks any prediction (≤0.99): a wrong prediction only wastes prefetch
//! slack, never evicts a proven-hot expert. **We guarantee the clamp invariant** (never
//! emit >1) so that ordering holds — noisy-OR gives us `[0,1]` by construction.
//!
//! ## Why noisy-OR
//!
//! Multiple already-fired experts may each predict the same upcoming expert `n`. Treat
//! each as independent evidence: `P(n fires) = 1 - Π_p (1 - P(n | p))`. This is ≤1 by
//! construction (the clamp invariant is free), rises with corroborating predecessors,
//! and needs no tuning constant — the honest combiner, not a hand-weighted sum.
//!
//! ## Purity
//!
//! This module is the pure learn+predict algorithm: transitions in, confidences out.
//! It does NOT capture the transitions from the live callback — that hot-path capture
//! (per-forward-pass trajectory grouping under `--parallel`) threads a pass-id through
//! the ggml eval-callback and is the next slice, coupled with the llama fork. Keeping
//! the algorithm pure makes it deterministically testable without a served model.

use std::collections::HashMap;

use super::expert_residency::ExpertId;

/// A learned cross-layer co-occurrence model + its prediction. Feed it observed
/// `(predecessor experts) → (successor experts)` transitions from forward passes; ask
/// it, given the experts fired so far this pass, which are likely to fire next.
#[derive(Debug, Default)]
pub struct CrossLayerExpertPredictor {
    /// For each predecessor expert `p`: how many transitions were observed FROM it
    /// (the denominator of `P(successor | p)`).
    seen: HashMap<ExpertId, u64>,
    /// `cooccur[p][n]` = times successor `n` followed predecessor `p` within a pass
    /// (the numerator of `P(n | p)`).
    cooccur: HashMap<ExpertId, HashMap<ExpertId, u64>>,
}

impl CrossLayerExpertPredictor {
    pub fn new() -> Self {
        Self::default()
    }

    /// Learn from one observed transition within a forward pass: every `next` expert
    /// followed every `prev` expert. Called by the capture layer once per adjacent
    /// (or windowed) layer step. Cheap integer tallies; no floats until `predict`.
    ///
    /// Self-transitions (`p == n`) are skipped — an expert never "predicts itself"; the
    /// point is to prefetch experts NOT yet resident, and a fired expert is already hot
    /// (its residency is `hits`, not a prediction).
    pub fn observe_transition(&mut self, prev: &[ExpertId], next: &[ExpertId]) {
        for &p in prev {
            let mut counted_p = false;
            for &n in next {
                if p == n {
                    continue;
                }
                *self.cooccur.entry(p).or_default().entry(n).or_insert(0) += 1;
                counted_p = true;
            }
            // Count `p` as a predecessor once per transition that yielded ≥1 distinct
            // successor, so `P(n|p) = cooccur[p][n] / seen[p]` stays a proper fraction.
            if counted_p {
                *self.seen.entry(p).or_insert(0) += 1;
            }
        }
    }

    /// `P(successor n | predecessor p)` — the learned conditional, or `0.0` if `p` was
    /// never observed as a predecessor or `n` never followed it.
    fn conditional(&self, p: &ExpertId, n: &ExpertId) -> f32 {
        let seen = match self.seen.get(p) {
            Some(&s) if s > 0 => s as f32,
            _ => return 0.0,
        };
        let co = self.cooccur.get(p).and_then(|m| m.get(n)).copied().unwrap_or(0) as f32;
        co / seen
    }

    /// Predict the experts likely to fire next, given the experts fired so far this
    /// pass. Confidence per candidate is noisy-OR over every fired predecessor that
    /// has ever preceded it: `1 - Π_p (1 - P(n | p))`, always in `[0, 1]`.
    ///
    /// Candidates already in `fired_so_far` are excluded (they're resident — `hits`
    /// covers them, not a prediction). Zero-confidence entries are omitted so the map
    /// is exactly "what to prefetch, and how strongly".
    pub fn predict(&self, fired_so_far: &[ExpertId]) -> HashMap<ExpertId, f32> {
        use std::collections::HashSet;
        let fired: HashSet<ExpertId> = fired_so_far.iter().copied().collect();

        // Candidate set: every successor any fired expert has ever preceded.
        let mut miss_prob: HashMap<ExpertId, f32> = HashMap::new();
        for p in &fired {
            let Some(succs) = self.cooccur.get(p) else { continue };
            for n in succs.keys() {
                if fired.contains(n) {
                    continue; // already resident this pass
                }
                let cond = self.conditional(p, n);
                // Accumulate Π (1 - P(n|p)) as we see each predecessor.
                let entry = miss_prob.entry(*n).or_insert(1.0);
                *entry *= 1.0 - cond;
            }
        }

        miss_prob
            .into_iter()
            .map(|(n, miss)| (n, (1.0 - miss).clamp(0.0, 1.0)))
            .filter(|(_, conf)| *conf > 0.0)
            .collect()
    }
}

/// Reshape one `ffn_moe_topk` observation — the flat row-major `[n_expert_used, n_tokens]`
/// I32 slice the ggml callback hands us — into per-token expert rows for `layer`. Row `t`
/// is the `n_expert_used` experts selected for batch-position `t`. Negative indices
/// (padding / not-selected slots) are dropped. Pure so the reshape (the load-bearing,
/// non-obvious row-major unpack) is testable without a served model.
pub fn per_token_experts(layer: u32, flat: &[i32], n_expert_used: usize) -> Vec<Vec<ExpertId>> {
    if n_expert_used == 0 {
        return Vec::new();
    }
    flat.chunks(n_expert_used)
        .map(|row| {
            row.iter()
                .filter(|&&e| e >= 0)
                .map(|&e| ExpertId { layer, expert: e as u32 })
                .collect()
        })
        .collect()
}

/// Accumulates one forward pass's layer-by-layer expert observations into cross-layer
/// transitions, feeding a [`CrossLayerExpertPredictor`]. Under continuous batching a single
/// graph runs all layers in increasing `il` order over the whole batch, so batch-position
/// `t` IS the same token's trajectory across layers — no pass-id needed; a DECREASE in
/// layer index marks the next batch (reset). This is the pure capture brain; the hot-path
/// wiring (holding one of these behind the observer, lock-cheap) is the integration slice.
#[derive(Debug, Default)]
pub struct BatchTrajectoryAccumulator {
    /// The previous layer observed this batch: `(layer, per-token expert rows)`.
    prev: Option<(u32, Vec<Vec<ExpertId>>)>,
}

impl BatchTrajectoryAccumulator {
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed one layer's observation (already reshaped to per-token rows). If it continues
    /// the current batch (layer strictly greater than the previous), learn the transition
    /// for every batch-position; if the layer index dropped (or is the first), start a new
    /// batch. Learns into `predictor` directly. Rows are matched by position; a ragged
    /// count (rare — shouldn't happen within one batch) matches the shorter, never panics.
    pub fn observe_layer(
        &mut self,
        layer: u32,
        per_token: Vec<Vec<ExpertId>>,
        predictor: &mut CrossLayerExpertPredictor,
    ) {
        if let Some((prev_layer, prev_rows)) = &self.prev {
            if layer > *prev_layer {
                for (prev_row, curr_row) in prev_rows.iter().zip(per_token.iter()) {
                    predictor.observe_transition(prev_row, curr_row);
                }
            }
            // layer <= prev_layer → a new forward pass began; fall through and replace prev.
        }
        self.prev = Some((layer, per_token));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn eid(layer: u32, expert: u32) -> ExpertId {
        ExpertId { layer, expert }
    }

    // what this catches: the learned conditional. After observing A→X twice and A→Y once
    // (two transitions FROM A), predict([A]) must give P(X|A)=1.0 and P(Y|A)=0.5 — the
    // measured cross-layer affinity, so the pager prefetches X with full confidence and Y
    // with half. This is the core signal the whole prefetch win rides on.
    #[test]
    fn learns_conditional_frequency_of_cross_layer_successors() {
        let a = eid(0, 3);
        let x = eid(1, 7);
        let y = eid(1, 2);
        let mut p = CrossLayerExpertPredictor::new();
        p.observe_transition(&[a], &[x, y]); // pass 1: A preceded X and Y
        p.observe_transition(&[a], &[x]); //    pass 2: A preceded X only

        let pred = p.predict(&[a]);
        assert_eq!(pred.get(&x).copied(), Some(1.0), "X followed A in both passes → P=1.0");
        assert_eq!(pred.get(&y).copied(), Some(0.5), "Y followed A in one of two → P=0.5");
    }

    // what this catches: noisy-OR combination — two independent predecessors each weakly
    // predicting the same expert corroborate to a HIGHER confidence, and it stays ≤1
    // (the clamp invariant BigMama's ranking depends on). A(→X @0.5) and B(→X @0.5)
    // firing together → 1-(0.5*0.5)=0.75, not 1.0 (naive sum would overflow the invariant).
    #[test]
    fn noisy_or_corroborates_and_never_exceeds_one() {
        let a = eid(0, 1);
        let b = eid(0, 2);
        let x = eid(2, 9);
        let mut p = CrossLayerExpertPredictor::new();
        // A→X once out of two A-transitions → P(X|A)=0.5
        p.observe_transition(&[a], &[x]);
        p.observe_transition(&[a], &[eid(2, 0)]);
        // B→X once out of two B-transitions → P(X|B)=0.5
        p.observe_transition(&[b], &[x]);
        p.observe_transition(&[b], &[eid(2, 0)]);

        let both = p.predict(&[a, b]);
        assert_eq!(both.get(&x).copied(), Some(0.75), "noisy-OR: 1-(1-0.5)(1-0.5)=0.75");
        assert!(both.values().all(|&c| (0.0..=1.0).contains(&c)), "clamp invariant holds");

        // One predecessor alone gives the weaker signal.
        let one = p.predict(&[a]);
        assert_eq!(one.get(&x).copied(), Some(0.5));
    }

    // what this catches: the honest empties + self-exclusion — an unknown predecessor
    // predicts nothing, an already-fired expert is never predicted (it's resident; hits
    // covers it, not a prediction), and self-transitions are dropped at learn time.
    #[test]
    fn unknown_predecessor_and_already_fired_are_excluded() {
        let a = eid(0, 1);
        let x = eid(1, 1);
        let mut p = CrossLayerExpertPredictor::new();
        p.observe_transition(&[a], &[x, a]); // self-transition A→A must be ignored

        assert!(p.predict(&[eid(5, 5)]).is_empty(), "an unseen predecessor predicts nothing");
        let pred = p.predict(&[a]);
        assert_eq!(pred.get(&x).copied(), Some(1.0), "X predicted from A");
        assert!(!pred.contains_key(&a), "A is fired/resident — never predict it");
    }

    // what this catches: the row-major [n_expert_used, n_tokens] unpack — token t's experts
    // are the t-th chunk of n_expert_used, and negative padding slots are dropped. Getting
    // this wrong scrambles which experts belong to which token → garbage co-occurrence.
    #[test]
    fn per_token_reshape_unpacks_row_major_and_drops_padding() {
        // n_expert_used=2, n_tokens=3: [t0: 5,7][t1: 5,-1(pad)][t2: 2,7]
        let flat = vec![5, 7, 5, -1, 2, 7];
        let rows = per_token_experts(4, &flat, 2);
        assert_eq!(rows.len(), 3, "three tokens");
        assert_eq!(rows[0], vec![eid(4, 5), eid(4, 7)]);
        assert_eq!(rows[1], vec![eid(4, 5)], "the -1 padding slot is dropped");
        assert_eq!(rows[2], vec![eid(4, 2), eid(4, 7)]);
        assert!(per_token_experts(4, &[], 0).is_empty(), "n_expert_used=0 → empty, no panic");
    }

    // what this catches: the batch trajectory capture — within one forward pass (layers in
    // increasing order) each batch-position's experts across layers become cross-layer
    // transitions; a DECREASE in layer index starts a NEW batch so trajectories never
    // bleed across forward passes. This is what turns raw per-layer observations into the
    // predictor's learning signal, with no pass-id.
    #[test]
    fn accumulator_learns_within_a_pass_and_resets_across_passes() {
        let mut acc = BatchTrajectoryAccumulator::new();
        let mut pred = CrossLayerExpertPredictor::new();

        // Pass 1: one token, layer 0 fires expert 3, layer 1 fires expert 7.
        acc.observe_layer(0, vec![vec![eid(0, 3)]], &mut pred); // first layer: no prev, just stores
        acc.observe_layer(1, vec![vec![eid(1, 7)]], &mut pred); // 1>0: learns (0,3)→(1,7)
        // Pass 2 begins: layer index DROPS to 0 → reset, must NOT learn (1,7)→(0,3) across passes.
        acc.observe_layer(0, vec![vec![eid(0, 3)]], &mut pred);
        acc.observe_layer(1, vec![vec![eid(1, 7)]], &mut pred); // learns (0,3)→(1,7) again

        // (0,3) precedes (1,7) in both passes → P=1.0; nothing learned backwards or across.
        let out = pred.predict(&[eid(0, 3)]);
        assert_eq!(out.get(&eid(1, 7)).copied(), Some(1.0), "forward transition learned each pass");
        // The cross-pass bleed (1,7)→(0,3) must NOT exist: predicting from (1,7) yields nothing.
        assert!(
            pred.predict(&[eid(1, 7)]).is_empty(),
            "layer-decrease reset prevents trajectories bleeding across forward passes"
        );
    }
}
