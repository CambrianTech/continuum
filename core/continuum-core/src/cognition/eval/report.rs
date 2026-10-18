//! Turning graded rows into the run's scoreboard: the self-verify rate, the speed and latency
//! aggregates, the pass rate, the truthful lane placement report, and the one assembly of
//! `CognitionEvalResult` every arm uses.

use super::*;

/// Fraction of tasks where she ACTED at least once before settling — the
/// verify-don't-assert reflex as a number. `acts=0` everywhere = she answered
/// from the model's head and never ran her own code (the #1 trainable lever per
/// [[asha-coder-baseline-and-spawn-race]]).
pub(super) fn self_verify_rate(results: &[EvalTaskResult]) -> f64 {
    if results.is_empty() {
        return 0.0;
    }
    let acted = results.iter().filter(|r| r.acts > 0).count();
    acted as f64 / results.len() as f64
}

/// Speed + latency aggregates over the per-task results — the SPEED and LATENCY
/// axes of the scoreboard. P95 is the honest tail (a mean hides an occasional
/// stall); wall-clock throughput is averaged per-task (not total_tokens/total_ms)
/// so one slow task can't dominate. The PREFILL-vs-DECODE breakdown (real decode
/// tok/s, cache-hit-rate, the prefill/decode ms split) is the lever data that
/// makes speed iterable instead of one conflated number — measured 77% of eval
/// time was prefill, not decode. Empty set → all zero.
#[derive(Debug, PartialEq)]
pub(super) struct SpeedAggregates {
    pub(super) mean_latency_ms: f64,
    pub(super) p95_latency_ms: u64,
    /// WALL-CLOCK mean tok/s — diluted by prefill + cognition overhead.
    pub(super) mean_tokens_per_second: f64,
    /// REAL mean decode tok/s off the lane clock — the honest generation rate.
    pub(super) mean_decode_tokens_per_second: f64,
    /// Mean KV-cache hit-rate (cached / total prompt tokens).
    pub(super) mean_cache_hit_rate: f64,
    pub(super) total_prefill_ms: u64,
    pub(super) total_decode_ms: u64,
    pub(super) total_output_tokens: u32,
}

pub(super) fn speed_latency_aggregates(results: &[EvalTaskResult]) -> SpeedAggregates {
    if results.is_empty() {
        return SpeedAggregates {
            mean_latency_ms: 0.0,
            p95_latency_ms: 0,
            mean_tokens_per_second: 0.0,
            mean_decode_tokens_per_second: 0.0,
            mean_cache_hit_rate: 0.0,
            total_prefill_ms: 0,
            total_decode_ms: 0,
            total_output_tokens: 0,
        };
    }
    let n = results.len() as f64;
    let mean_latency = results.iter().map(|r| r.latency_ms as f64).sum::<f64>() / n;
    let mean_tps = results.iter().map(|r| r.tokens_per_second).sum::<f64>() / n;
    let mean_decode_tps = results
        .iter()
        .map(|r| r.decode_tokens_per_second)
        .sum::<f64>()
        / n;
    let mean_cache_hit = results.iter().map(|r| r.cache_hit_rate).sum::<f64>() / n;
    let total_out = results
        .iter()
        .fold(0u32, |acc, r| acc.saturating_add(r.output_tokens));
    let total_prefill_ms = results
        .iter()
        .fold(0u64, |acc, r| acc.saturating_add(r.prefill_ms));
    let total_decode_ms = results
        .iter()
        .fold(0u64, |acc, r| acc.saturating_add(r.decode_ms));
    // P95: sort latencies, take the ceil(0.95 * n)-th (1-indexed) — the value at or
    // below which 95% of tasks settled.
    let mut lat: Vec<u64> = results.iter().map(|r| r.latency_ms).collect();
    lat.sort_unstable();
    let idx = (((lat.len() as f64) * 0.95).ceil() as usize)
        .saturating_sub(1)
        .min(lat.len() - 1);
    SpeedAggregates {
        mean_latency_ms: mean_latency,
        p95_latency_ms: lat[idx],
        mean_tokens_per_second: mean_tps,
        mean_decode_tokens_per_second: mean_decode_tps,
        mean_cache_hit_rate: mean_cache_hit,
        total_prefill_ms,
        total_decode_ms,
        total_output_tokens: total_out,
    }
}

/// Pass rate over the run's task count; an empty run scores 0, never NaN.
pub(super) fn pass_rate(score: u32, total: u32) -> f64 {
    if total > 0 {
        score as f64 / total as f64
    } else {
        0.0
    }
}

/// One graded pass: whose run, how many tasks, how many passed, and every task's row.
pub(super) struct GradedRun {
    pub(super) persona_id: Uuid,
    pub(super) total: u32,
    pub(super) score: u32,
    pub(super) results: Vec<EvalTaskResult>,
}

/// The TRUTHFUL placement report — the actual decision, never a hardcoded lie. A run with
/// placement evidence ran on a DEDICATED throwaway lane (isolated window + slot → reliable,
/// reproducible); a run without any co-tenanted the live persona lane (unreliable — starves
/// behind live turns). A hardcoded "live persona lane" string once made the two
/// indistinguishable, so a wedge-prone shared-lane score looked identical to a trusted
/// isolated one and glass-boxing was impossible (2026-07-21). Returns (lane, reason).
/// [[dedicated-eval-lane-must-keep-its-own-window]]
fn lane_report(placement: Option<&PlacementEvidence>) -> (String, String) {
    match placement {
        Some(ev) => (format!("{} (dedicated eval lane)", ev.device), ev.reason.clone()),
        None => (
            "gpu (SHARED live persona lane — co-tenant, unreliable)".to_string(),
            "no dedicated lane could be placed — measured as a co-tenant on the living persona's \
             GPU lane (may starve behind live turns)"
                .to_string(),
        ),
    }
}

impl CognitionEvalResult {
    /// The scoreboard for one graded pass: the score and its rate, the speed and latency
    /// axes, and which lane measured it. A/B fields (`gene_id`, `base_pass_rate`, `lift`)
    /// and `run_id` are the caller's to set; everything derived from the pass is set here,
    /// once, for every arm.
    pub(super) fn graded(
        run: GradedRun,
        placement: Option<&PlacementEvidence>,
        infra_unavailable: Option<InfraUnavailable>,
    ) -> Self {
        let agg = speed_latency_aggregates(&run.results);
        let (lane_placement, lane_placement_reason) = lane_report(placement);
        CognitionEvalResult {
            run_id: None,
            detached: false,
            persona_id: run.persona_id.to_string(),
            score: run.score,
            total: run.total,
            pass_rate: pass_rate(run.score, run.total),
            self_verify_rate: self_verify_rate(&run.results),
            mean_latency_ms: agg.mean_latency_ms,
            p95_latency_ms: agg.p95_latency_ms,
            mean_tokens_per_second: agg.mean_tokens_per_second,
            mean_decode_tokens_per_second: agg.mean_decode_tokens_per_second,
            mean_cache_hit_rate: agg.mean_cache_hit_rate,
            total_prefill_ms: agg.total_prefill_ms,
            total_decode_ms: agg.total_decode_ms,
            total_output_tokens: agg.total_output_tokens,
            results: run.results,
            gene_id: None,
            base_pass_rate: None,
            lift: None,
            lane_placement,
            lane_placement_reason,
            lane_free_vram_bytes: placement.and_then(|e| e.free_vram_bytes),
            lane_estimated_footprint_bytes: placement.and_then(|e| e.footprint_bytes),
            infra_unavailable,
        }
    }
}
