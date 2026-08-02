//! MoE trace tail — the daemon-side observation front end of the pin
//! actuator (#281). Tails the fork's `GGML_MOE_TRACE_FILE` (12-byte
//! binary records: tkey u64 + expert u32, appended per activated
//! expert), segments decode tokens, and folds them into the
//! [`BanditPlanController`] whose `pin_list` the serving daemon's
//! governed plan publish carries to her `ResidencyCache`.
//!
//! This is the DAEMON-integrated sibling of the standalone
//! `moe-pager-driver` bin (same primitives from `expert_pager_policy`,
//! same offset-resume + truncation-reset discipline) with one upgrade:
//! the tkey table is SYNTHESIZED from the model's `n_layers`
//! ([`TkeyTable::for_layers`]) instead of loaded from an operator JSON
//! — zero configuration, the system owns the seam.
//!
//! Read discipline: incremental (only bytes past the last offset),
//! bounded per drain ([`MAX_DRAIN_BYTES`]) so a long catch-up backlog
//! can never stall the daemon tick — on overflow the tail JUMPS to the
//! recent window and lets the segmenter re-sync on the next key-cycle
//! boundary (recency is the signal; stale backlog is not). A file
//! shorter than the stored offset = a fresh serve (the fork opens
//! `"wb"`) → full state reset, stale scores never leak across serves.

use std::collections::HashSet;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use expert_pager_policy::controller::BanditPlanController;
use expert_pager_policy::plan_file::PlanPin;
use expert_pager_policy::segment::{
    parse_records, PrefillBoundaryDetector, TkeyTable, TokenSegmenter, RECORD_BYTES,
};
use expert_pager_policy::ExpertId;

use super::expert_predictor::CrossLayerExpertPredictor;

/// Per-drain read ceiling. A decode token is ~17 KB of trace (~1472
/// experts × 12 B), so 4 MiB ≈ 240 tokens of catch-up per tick — far
/// more than accumulates between 5 s ticks at single-digit tok/s, while
/// bounding the worst-case tick cost when the daemon was down for hours.
const MAX_DRAIN_BYTES: u64 = 4 * 1024 * 1024;

/// Adaptive budget factor: the bandit predicts against 1.5× the first
/// observed token's working set (the prototypes' heuristic, carried
/// from the driver bin).
const BUDGET_FACTOR_NUM: usize = 3;
const BUDGET_FACTOR_DEN: usize = 2;

/// Streaming trace consumer + controller owner. One per serving lane;
/// lives behind the daemon's sync Mutex (never held across await).
pub struct MoeTraceTail {
    /// Geometry the table was synthesized for — a model change (new
    /// n_layers) rebuilds the table and resets the controller.
    n_layers: u32,
    table: TkeyTable,
    offset: u64,
    carry: Vec<u8>,
    segmenter: TokenSegmenter,
    boundary: PrefillBoundaryDetector,
    controller: Option<BanditPlanController>,
    /// Decode tokens folded in since the last reset (telemetry).
    pub tokens_observed: u64,
    /// True when the last drain crossed the prefill→decode boundary —
    /// the caller publishes the warm-start plan immediately (prefill's
    /// tail predicts ~47-66% of decode experts; acting AT the boundary
    /// is when the hint matters most).
    pub boundary_crossed: bool,
    /// The pin list as last written to the plan file — the write-churn
    /// gate's memory. The sticky budget band alone would suppress plan
    /// writes while pins roll; comparing against this lets the caller
    /// write exactly when the ACTUATOR state changed and stay silent
    /// (no mtime churn under her per-token poll) when it didn't.
    last_published_pins: Vec<PlanPin>,
    // ── Predictive-scheduling instrument (Joel 2026-08-02: "use our ml
    // advantage for predictive scheduling"). The go/no-go for the CUDA
    // copy-stream work is a MEASURED number, not an assumption: exposed
    // H2D per token = (1 − schedulable coverage) × the ~11 GB expert
    // working set. Coverage splits into what RECENCY covers (repeats of
    // the previous token's set — residency handles those for free) and
    // what PREDICTION covers (of the non-repeat delta, how many the
    // cross-token predictor called one token ahead). Both measured live
    // from the real trace, no mechanism required.
    /// Cross-token transition model: `observe_transition(prev, next)`
    /// per decode token; `predict(current)` scores next-token deltas.
    predictor: CrossLayerExpertPredictor,
    /// The previous decode token's expert set — the transition's LHS
    /// and the repeat baseline.
    prev_token_set: Option<Vec<ExpertId>>,
    /// The delta prediction made at token N for token N+1 (top-|set|
    /// candidates), scored against reality when N+1 folds.
    predicted_delta: Option<HashSet<ExpertId>>,
    /// Repeats: experts of token N+1 already in token N's set.
    repeat_hits: u64,
    /// Denominator for both recalls: total experts across scored tokens.
    repeat_total: u64,
    /// Of the NON-repeat experts, how many the predictor called.
    delta_hits: u64,
    delta_total: u64,
}

impl MoeTraceTail {
    pub fn new(n_layers: u32) -> Self {
        Self {
            n_layers,
            table: TkeyTable::for_layers(n_layers),
            offset: 0,
            carry: Vec::new(),
            segmenter: TokenSegmenter::new(),
            boundary: PrefillBoundaryDetector::new(),
            controller: None,
            tokens_observed: 0,
            boundary_crossed: false,
            last_published_pins: Vec::new(),
            predictor: CrossLayerExpertPredictor::new(),
            prev_token_set: None,
            predicted_delta: None,
            repeat_hits: 0,
            repeat_total: 0,
            delta_hits: 0,
            delta_total: 0,
        }
    }

    fn reset_stream_state(&mut self) {
        self.offset = 0;
        self.carry.clear();
        self.segmenter = TokenSegmenter::new();
        self.boundary = PrefillBoundaryDetector::new();
        self.controller = None;
        self.tokens_observed = 0;
        self.last_published_pins.clear();
        self.predictor = CrossLayerExpertPredictor::new();
        self.prev_token_set = None;
        self.predicted_delta = None;
        self.repeat_hits = 0;
        self.repeat_total = 0;
        self.delta_hits = 0;
        self.delta_total = 0;
    }

    /// Did the actuator state move since the last plan write? Compares
    /// (and on change, records) the candidate pin list. The caller
    /// writes the plan when this is true, when the budget moved, or at
    /// the prefill→decode boundary — and stays silent otherwise.
    pub fn pins_changed(&mut self, pins: &[PlanPin]) -> bool {
        if self.last_published_pins.as_slice() == pins {
            return false;
        }
        self.last_published_pins = pins.to_vec();
        true
    }

    /// Ensure the tail matches the active model's geometry; a change
    /// rebuilds the synthesized table AND resets everything (scores
    /// from another model's experts are meaningless).
    pub fn ensure_geometry(&mut self, n_layers: u32) {
        if self.n_layers != n_layers {
            self.n_layers = n_layers;
            self.table = TkeyTable::for_layers(n_layers);
            self.reset_stream_state();
        }
    }

    /// Drain new trace bytes and fold completed decode tokens into the
    /// bandit. Missing file = serve not started (no-op). Returns the
    /// number of tokens folded this drain.
    pub fn drain(&mut self, trace_path: &Path) -> u64 {
        self.boundary_crossed = false;
        let Ok(mut file) = std::fs::File::open(trace_path) else {
            return 0;
        };
        let len = file.metadata().map(|m| m.len()).unwrap_or(0);
        if len < self.offset {
            // Truncated: a NEW serve started — reset, stale scores
            // belong to the previous generation.
            self.reset_stream_state();
        }
        if len <= self.offset {
            return 0;
        }
        if len - self.offset > MAX_DRAIN_BYTES {
            // Backlog overflow: jump to the recent window (record-aligned
            // relative to the file start so parse framing holds — the
            // fork writes from byte 0 in whole records) and re-sync.
            let target = len - MAX_DRAIN_BYTES;
            self.offset = target - (target % RECORD_BYTES as u64);
            self.carry.clear();
            self.segmenter = TokenSegmenter::new();
        }
        if file.seek(SeekFrom::Start(self.offset)).is_err() {
            return 0;
        }
        let mut fresh = Vec::with_capacity((len - self.offset) as usize);
        let Ok(_) = (&mut file).take(len - self.offset).read_to_end(&mut fresh) else {
            return 0;
        };
        self.offset += fresh.len() as u64;
        self.carry.extend_from_slice(&fresh);
        let (records, consumed) = parse_records(&self.carry);
        self.carry.drain(..consumed);
        debug_assert!(self.carry.len() < RECORD_BYTES);

        let mut folded = 0u64;
        for rec in records {
            if let Some(token) = self.segmenter.push(rec, &self.table) {
                if token.is_empty() {
                    continue;
                }
                let experts: Vec<_> = token.into_iter().collect();
                if self.boundary.observe(experts.len()) {
                    self.boundary_crossed = true;
                }
                // Predictive-scheduling instrument: score last token's
                // prediction against reality, then learn the transition
                // and predict the NEXT delta. Skipped for the first token
                // (no baseline) — honest None, never a fake 100%.
                if let Some(prev) = self.prev_token_set.take() {
                    let prev_set: HashSet<ExpertId> = prev.iter().copied().collect();
                    let repeats =
                        experts.iter().filter(|e| prev_set.contains(e)).count() as u64;
                    self.repeat_hits += repeats;
                    self.repeat_total += experts.len() as u64;
                    if let Some(pred) = self.predicted_delta.take() {
                        self.delta_total += experts.len() as u64 - repeats;
                        self.delta_hits += experts
                            .iter()
                            .filter(|e| !prev_set.contains(e) && pred.contains(e))
                            .count() as u64;
                    }
                    self.predictor.observe_transition(&prev, &experts);
                }
                let mut scored: Vec<(ExpertId, f32)> =
                    self.predictor.predict(&experts).into_iter().collect();
                scored.sort_by(|a, b| b.1.total_cmp(&a.1));
                self.predicted_delta = Some(
                    scored.into_iter().take(experts.len()).map(|(e, _)| e).collect(),
                );
                self.prev_token_set = Some(experts.clone());

                let ctl = self.controller.get_or_insert_with(|| {
                    BanditPlanController::new(
                        experts.len() * BUDGET_FACTOR_NUM / BUDGET_FACTOR_DEN,
                    )
                });
                ctl.observe_token(&experts);
                self.tokens_observed += 1;
                folded += 1;
            }
        }
        folded
    }

    /// The current hot-routed pin list, or empty before the first
    /// observed token (a budget-only plan — exactly the v1 wire shape,
    /// so the actuator degrades to the validated behavior when the
    /// trace is dark). Pins roll with the bandit's decay window every
    /// call — the anti-fossil property: yesterday's hot experts fade
    /// out of the list as their scores decay.
    pub fn pin_list(&self, top_n: usize) -> Vec<PlanPin> {
        match (&self.controller, top_n) {
            (Some(ctl), n) if n > 0 => ctl.pin_list(n),
            _ => Vec::new(),
        }
    }

    /// Fraction (×100) of each token's experts already present in the
    /// PREVIOUS token's set — what pure recency residency covers with no
    /// prediction at all. `None` before two decode tokens (honest void).
    pub fn repeat_recall_x100(&self) -> Option<u32> {
        (self.repeat_total > 0).then(|| (self.repeat_hits * 100 / self.repeat_total) as u32)
    }

    /// Of the NON-repeat experts (the delta recency can't cover), the
    /// fraction (×100) the cross-token predictor called one token ahead
    /// — what predictive prefetch adds on top of recency.
    pub fn predicted_delta_recall_x100(&self) -> Option<u32> {
        (self.delta_total > 0).then(|| (self.delta_hits * 100 / self.delta_total) as u32)
    }

    /// Total schedulable coverage (×100): repeats + predicted deltas over
    /// all experts. THE go/no-go number — exposed H2D per token scales
    /// with (100 − this) × per-token working-set bytes.
    pub fn schedulable_coverage_x100(&self) -> Option<u32> {
        (self.repeat_total > 0)
            .then(|| ((self.repeat_hits + self.delta_hits) * 100 / self.repeat_total) as u32)
    }

    /// The current next-token delta prediction, strongest-first — the
    /// future plan-file `prefetch` list (#273's third axis), published
    /// once the wire extension is coordinated with the consumer.
    pub fn predicted_next(&self, top_n: usize) -> Vec<ExpertId> {
        let Some(prev) = &self.prev_token_set else {
            return Vec::new();
        };
        let mut scored: Vec<(ExpertId, f32)> =
            self.predictor.predict(prev).into_iter().collect();
        scored.sort_by(|a, b| b.1.total_cmp(&a.1));
        scored.into_iter().take(top_n).map(|(e, _)| e).collect()
    }
}

/// How many experts the governed lease can afford to PIN: half the
/// lease (the other half stays free for the recency working set —
/// pinning the whole budget would evict the very tokens-in-flight the
/// cache exists to retain), divided by one expert's bytes. Zero when
/// geometry is unknown/degenerate — a budget-only plan, never a
/// division panic.
pub fn pin_ceiling(
    budget_bytes: u64,
    expert_bytes_total: u64,
    n_layers: u32,
    n_experts_per_layer: u32,
) -> usize {
    let expert_count = u64::from(n_layers) * u64::from(n_experts_per_layer);
    if expert_count == 0 || expert_bytes_total == 0 {
        return 0;
    }
    let per_expert = expert_bytes_total / expert_count;
    if per_expert == 0 {
        return 0;
    }
    ((budget_bytes / 2) / per_expert) as usize
}

#[cfg(test)]
mod tests {
    use super::*;
    use expert_pager_policy::segment::fnv1a_name_key;

    /// Append `experts` as one decode token's worth of records for the
    /// gate matrix of `layer` (one tensor-key group). Cycling back to
    /// an earlier layer's key is what closes the token.
    fn append_records(buf: &mut Vec<u8>, layer: u32, experts: &[u32]) {
        let tkey = fnv1a_name_key(&format!("blk.{layer}.ffn_gate_exps.weight"));
        for &e in experts {
            buf.extend_from_slice(&tkey.to_le_bytes());
            buf.extend_from_slice(&e.to_le_bytes());
        }
    }

    /// Two-layer token: records for layer 0 then layer 1. Repeating
    /// layer 0 in the NEXT call wraps the router cycle and completes it.
    fn token_bytes(experts_l0: &[u32], experts_l1: &[u32]) -> Vec<u8> {
        let mut buf = Vec::new();
        append_records(&mut buf, 0, experts_l0);
        append_records(&mut buf, 1, experts_l1);
        buf
    }

    // what this catches: the whole daemon-side observation loop against
    // a real file — offset-resumed incremental reads (drain twice, no
    // re-count), token completion on cycle wrap, and pins emerging in
    // REAL (layer, expert) coordinates from the synthesized table (no
    // JSON table anywhere).
    #[test]
    fn drains_incrementally_and_pins_real_coordinates() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let trace = tmp.path().join("moe-trace.bin");
        let mut tail = MoeTraceTail::new(2);

        // Token A (open) — nothing completes yet.
        std::fs::write(&trace, token_bytes(&[3, 7], &[9])).expect("write");
        assert_eq!(tail.drain(&trace), 0, "token A still open");

        // Token B's records arrive: A completes on cycle wrap.
        let mut more = std::fs::read(&trace).expect("read");
        more.extend_from_slice(&token_bytes(&[3, 8], &[9]));
        std::fs::write(&trace, &more).expect("write");
        assert_eq!(tail.drain(&trace), 1, "token A folds");
        assert_eq!(tail.tokens_observed, 1);

        // Pins are real coordinates from token A's set.
        let pins = tail.pin_list(8);
        assert!(!pins.is_empty());
        assert!(
            pins.iter().all(|p| (p.layer == 0 && (p.expert == 3 || p.expert == 7))
                || (p.layer == 1 && p.expert == 9)),
            "pins must be token A's (layer, expert) set, got {pins:?}"
        );

        // No new bytes → no re-count (offset held).
        assert_eq!(tail.drain(&trace), 0);
    }

    // what this catches: truncation-reset — a shorter file is a NEW
    // serve (fork reopens "wb"); stale bandit scores and the old offset
    // must not survive into the new generation.
    #[test]
    fn truncation_resets_state_for_the_new_serve() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let trace = tmp.path().join("moe-trace.bin");
        let mut tail = MoeTraceTail::new(2);

        let mut buf = token_bytes(&[1, 2], &[3]);
        buf.extend_from_slice(&token_bytes(&[1, 2], &[3]));
        std::fs::write(&trace, &buf).expect("write");
        tail.drain(&trace);
        assert!(tail.tokens_observed >= 1);

        // New serve: file restarts smaller.
        std::fs::write(&trace, token_bytes(&[5], &[6])).expect("write");
        tail.drain(&trace);
        assert_eq!(
            tail.tokens_observed, 0,
            "reset: no tokens counted from the fresh open stream yet"
        );
        assert!(tail.pin_list(8).is_empty(), "stale pins do not survive a new serve");
    }

    // what this catches: a geometry change (different model) rebuilds
    // the synthesized table and resets — layer coordinates from one
    // model must never label another model's trace.
    #[test]
    fn geometry_change_resets_table_and_state() {
        let mut tail = MoeTraceTail::new(4);
        tail.tokens_observed = 7;
        tail.ensure_geometry(4);
        assert_eq!(tail.tokens_observed, 7, "same geometry: state kept");
        tail.ensure_geometry(8);
        assert_eq!(tail.tokens_observed, 0, "new geometry: full reset");
    }

    // what this catches: the predictive-scheduling instrument's two
    // recalls measure what they claim. A perfectly REPEATING stream is
    // 100% covered by recency alone (repeat_recall=100, no deltas to
    // predict); an ALTERNATING stream (disjoint sets A,B,A,B...) is 0%
    // recency but the cross-token predictor learns A->B/B->A after one
    // full cycle, so delta recall climbs — prediction covering exactly
    // what recency cannot. First token scores nothing (honest None).
    #[test]
    fn coverage_instrument_separates_recency_from_prediction() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let trace = tmp.path().join("moe-trace.bin");
        let mut tail = MoeTraceTail::new(2);
        assert_eq!(tail.repeat_recall_x100(), None, "no tokens yet");

        // Repeating stream: same experts every token. Completed tokens
        // need cycle wraps, so append the pattern repeatedly.
        let mut buf = Vec::new();
        for _ in 0..5 {
            buf.extend_from_slice(&token_bytes(&[3, 7], &[9]));
        }
        std::fs::write(&trace, &buf).expect("write");
        tail.drain(&trace);
        assert!(tail.tokens_observed >= 3);
        assert_eq!(tail.repeat_recall_x100(), Some(100), "identical tokens = pure recency");
        assert_eq!(
            tail.schedulable_coverage_x100(),
            Some(100),
            "full coverage with no prediction needed"
        );

        // Alternating stream on a fresh tail: token A = {(0,1),(1,2)},
        // token B = {(0,5),(1,6)} — fully disjoint.
        let trace2 = tmp.path().join("moe-trace-2.bin");
        let mut tail2 = MoeTraceTail::new(2);
        let mut buf2 = Vec::new();
        for _ in 0..6 {
            buf2.extend_from_slice(&token_bytes(&[1], &[2]));
            buf2.extend_from_slice(&token_bytes(&[5], &[6]));
        }
        std::fs::write(&trace2, &buf2).expect("write");
        tail2.drain(&trace2);
        assert_eq!(tail2.repeat_recall_x100(), Some(0), "disjoint tokens = zero recency");
        let delta = tail2
            .predicted_delta_recall_x100()
            .expect("delta scored after warmup");
        assert!(
            delta >= 50,
            "predictor must learn the alternation (measured {delta}), covering what recency can't"
        );
        assert!(tail2.predicted_next(4).len() > 0, "a live next-delta prediction exists");
    }

    // what this catches: the pin ceiling is HALF the lease over real
    // per-expert bytes (pins must never consume the whole budget — the
    // recency window is the cache's reason to exist), and degenerate
    // geometry yields 0 (budget-only plan), never a division panic.
    #[test]
    fn pin_ceiling_is_half_lease_and_degenerates_to_zero() {
        // 1000 experts of 10 MB each; lease 4 GB → half = 2 GB → 200 pins.
        let mb = 1024 * 1024;
        assert_eq!(pin_ceiling(4000 * mb, 10_000 * mb, 10, 100), 200);
        assert_eq!(pin_ceiling(4000 * mb, 0, 10, 100), 0);
        assert_eq!(pin_ceiling(4000 * mb, 10_000 * mb, 0, 0), 0);
    }
}
