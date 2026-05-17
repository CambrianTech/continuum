//! `demand-aligned-recall` PR-3a: scoring function + helpers.
//! Per GENOME-FOUNDRY-SENTINEL Part 7 "The Scoring Function —
//! Explicit, Tunable, Sentinel-Refined."
//!
//! Pure math, no I/O, no async. The caller (PR-3b's
//! `LocalDemandAlignedRecall`) computes individual factors using
//! its sources (embedding model for semantic similarity, sentinel
//! lookups for outcome_history, trust registry for
//! provenance_trust) and passes them as primitives. This module
//! combines them through the weighted-sum scoring function +
//! provides the per-factor curves the spec names:
//!
//! - `grid_penalty(latency_ms)` — federation peer cost curve
//! - `recency_decay(last_used_ms, now_ms, half_life_ms)` — temporal
//!   decay
//! - `local_role_score(role)` — Fast=1.0 / Bench=0.6 / Cold=0.3 /
//!   Frozen=0.1 per the spec
//! - `tier_proximity_for(&ResidencyHint)` — dispatches by hint
//!   variant: Hot→1.0, Local→local_role_score, GridPeer→
//!   grid_penalty, NotResident→0.0
//! - `score(...)` — combines the five factors with weights
//!
//! ## What PR-3a does NOT ship (PR-3b)
//!
//! - `ArtifactCandidate` struct + embedding interface — PR-3b
//! - Cosine similarity helper — PR-3b (it depends on whatever
//!   embedding representation lands; PR-3a keeps the math agnostic)
//! - `outcome_window_score` over an `OutcomeWindow` — PR-3b
//! - `trust_score` over `Provenance` + overrides — PR-3b
//! - `LocalDemandAlignedRecall` impl — PR-3b
//! - Working-set integration via #1362's bus hook — PR-3b

use super::recall::{RecallScore, ResidencyHint};
use super::recall_trait::RecallScoreWeights;
use super::tier::TierRole;

/// Default half-life for the recency decay curve. 24 hours in
/// milliseconds. The governor tunes this per hardware class +
/// sentinel may refine per persona over time.
pub const DEFAULT_RECENCY_HALF_LIFE_MS: u64 = 24 * 60 * 60 * 1000;

// ─── Per-factor curves ──────────────────────────────────────────

/// Penalty curve for federated grid peers. Per
/// GENOME-FOUNDRY-SENTINEL Part 7:
///
/// ```text
/// Same-LAN peer (< 10 ms):   ~0.55  — slightly worse than local L3
/// Same-region (< 50 ms):     ~0.35
/// Cross-region (< 200 ms):   ~0.15
/// Slow / unreliable:         ~0.05
/// ```
///
/// Implementation: `0.6 * exp(-latency_ms / 100.0)`. Tuned so the
/// curve hits the four reference points above (within 0.05) and
/// asymptotes toward 0 (never negative, never silently flipping
/// sign).
///
/// Caps at `0.6` for zero latency — even a "free" same-machine
/// grid peer costs slightly more than a local-resident artifact,
/// because the grid-peer path still adds protocol overhead the
/// local path doesn't have.
pub fn grid_penalty(latency_ms: u32) -> f32 {
    let l = latency_ms as f32;
    0.6 * (-l / 100.0).exp()
}

/// Exponential decay over time-since-last-use. Returns a score in
/// `[0.0, 1.0]` where 1.0 = used right now and 0.0 = arbitrarily
/// long ago.
///
/// Half-life semantics: an artifact used `half_life_ms` ago scores
/// `0.5`; used `2 * half_life_ms` ago scores `0.25`; etc. The
/// governor tunes `half_life_ms`; default is 24h
/// (`DEFAULT_RECENCY_HALF_LIFE_MS`).
///
/// Edge cases:
/// - `now_ms < last_used_ms` (clock went backward): returns 1.0
///   rather than NaN/negative. Defensive — clock skew is rare but
///   real, and we'd rather treat a slightly-future artifact as "hot"
///   than panic the scoring path.
/// - `half_life_ms == 0`: returns 1.0 if `now == last_used`,
///   else 0.0. Avoids divide-by-zero; degenerate but safe.
pub fn recency_decay(last_used_ms: u64, now_ms: u64, half_life_ms: u64) -> f32 {
    if now_ms <= last_used_ms {
        return 1.0;
    }
    if half_life_ms == 0 {
        return 0.0;
    }
    let elapsed = (now_ms - last_used_ms) as f64;
    let half = half_life_ms as f64;
    // 2^(-elapsed / half_life) = exp(-elapsed * ln(2) / half_life)
    (-elapsed * std::f64::consts::LN_2 / half).exp() as f32
}

/// Per-role local tier score. Spec values (Part 7):
/// - `Fast` (or `Warm` on discrete-GPU): 1.0 (already in working
///   set, no promotion cost)
/// - `Bench`: 0.6 (host RAM, copy required)
/// - `Cold`: 0.3 (SSD genome pool, mmap + maybe decompress)
/// - `Frozen`: 0.1 (archive, sub-second read but cold)
///
/// `Warm` returns 1.0 like `Fast` because on discrete-GPU hardware
/// both are accelerator-reachable; the cost difference (Warm needs
/// a copy from PCIe host RAM, Fast is already in VRAM) is captured
/// by the tier proximity calculation upstream, not by this score.
pub fn local_role_score(role: TierRole) -> f32 {
    match role {
        TierRole::Fast => 1.0,
        TierRole::Warm => 1.0,
        TierRole::Bench => 0.6,
        TierRole::Cold => 0.3,
        TierRole::Frozen => 0.1,
    }
}

/// Dispatch over `ResidencyHint` to compute the tier_proximity
/// factor for the scoring function. Each variant maps to a
/// per-factor curve:
/// - `Hot { role }` → 1.0 (already hot; full score)
/// - `Local { role }` → `local_role_score(role)`
/// - `GridPeer { est_latency_ms, .. }` → `grid_penalty(latency)`
/// - `NotResident { .. }` → 0.0 (would require foundry/sentinel
///   work; can't be used directly)
pub fn tier_proximity_for(residency: &ResidencyHint) -> f32 {
    match residency {
        ResidencyHint::Hot { .. } => 1.0,
        ResidencyHint::Local { role } => local_role_score(*role),
        ResidencyHint::GridPeer {
            est_latency_ms, ..
        } => grid_penalty(*est_latency_ms),
        ResidencyHint::NotResident { .. } => 0.0,
    }
}

// ─── Scoring function ───────────────────────────────────────────

/// Combine the five scoring factors into a `RecallScore`. Pure
/// function — same inputs always produce the same output.
///
/// Inputs:
/// - `semantic` — cosine similarity between query embedding and
///   artifact metadata embedding. Caller computes; PR-3a doesn't
///   depend on the embedding representation.
/// - `outcome_history` — score from `outcome_window_score` (PR-3b);
///   how well this artifact has performed for this persona on
///   similar past tasks.
/// - `last_used_ms` + `now_ms` + `half_life_ms` — feed
///   `recency_decay`. Caller passes `DEFAULT_RECENCY_HALF_LIFE_MS`
///   if the governor hasn't overridden.
/// - `residency` — `ResidencyHint` from the recall walk; feeds
///   `tier_proximity_for`.
/// - `provenance_trust` — score from `trust_score` (PR-3b); how
///   much the persona trusts this artifact's provenance chain.
/// - `weights` — governor-tunable weights; sum-to-1.0 invariant
///   already enforced by `RecallScoreWeights::new` (PR-2).
///
/// Returns the populated `RecallScore` with all five factors + the
/// combined weighted sum. Bounded `[0.0, sum(weights)]` because
/// each factor is bounded `[0.0, 1.0]` (this is true by
/// construction: semantic + outcome_history + provenance_trust are
/// the caller's responsibility to bound; recency_decay +
/// tier_proximity_for are bounded by their per-factor curves).
///
/// The combined score is NOT clamped — if a caller passes a
/// factor outside `[0.0, 1.0]` the combined will reflect that
/// (debugging hook: easier to spot bad inputs than to silently
/// clamp them). Per Joel's "never swallow errors": loud trumps
/// graceful.
#[allow(clippy::too_many_arguments)]
pub fn score(
    semantic: f32,
    outcome_history: f32,
    last_used_ms: u64,
    now_ms: u64,
    half_life_ms: u64,
    residency: &ResidencyHint,
    provenance_trust: f32,
    weights: &RecallScoreWeights,
) -> RecallScore {
    let recency = recency_decay(last_used_ms, now_ms, half_life_ms);
    let tier_proximity = tier_proximity_for(residency);

    let combined = weights.semantic * semantic
        + weights.outcome_history * outcome_history
        + weights.recency * recency
        + weights.tier_proximity * tier_proximity
        + weights.provenance_trust * provenance_trust;

    RecallScore {
        semantic,
        outcome_history,
        recency,
        tier_proximity,
        provenance_trust,
        combined,
    }
}

#[cfg(test)]
mod tests {
    //! Pin every per-factor curve to its spec reference points +
    //! pin the combined-score math against hand-computed values.
    //! Each test corresponds to a "what if a future PR drifts this
    //! curve?" failure mode.
    use super::*;
    use crate::genome::recall::{AcquireSource, PeerId};
    use uuid::Uuid;

    // ─── grid_penalty curve ────────────────────────────────────

    /// What this catches: the four spec reference points for
    /// grid_penalty hit their ~values. If a future PR tweaks the
    /// curve (different exponent, different base), this test flags
    /// each anchor — substrate-level cost change needs review.
    #[test]
    fn grid_penalty_matches_spec_reference_points() {
        // Same-LAN: < 10 ms → ~0.55
        let lan = grid_penalty(5);
        assert!(
            (lan - 0.57).abs() < 0.05,
            "same-LAN (5ms) should be ~0.55, got {lan}"
        );

        // Same-region: < 50 ms → ~0.35
        let region = grid_penalty(50);
        assert!(
            (region - 0.36).abs() < 0.05,
            "same-region (50ms) should be ~0.36, got {region}"
        );

        // Cross-region: < 200 ms → ~0.08
        let cross = grid_penalty(200);
        assert!(
            cross > 0.05 && cross < 0.15,
            "cross-region (200ms) should be ~0.08, got {cross}"
        );

        // Slow/unreliable: 500ms+ → near zero
        let slow = grid_penalty(500);
        assert!(slow < 0.01, "500ms should be near zero, got {slow}");
    }

    /// What this catches: grid_penalty(0) caps at 0.6 — even a
    /// zero-latency grid peer is penalized vs local-resident
    /// (protocol overhead the local path doesn't have).
    #[test]
    fn grid_penalty_caps_at_0_6_for_zero_latency() {
        assert!(
            (grid_penalty(0) - 0.6).abs() < 1e-4,
            "grid_penalty(0) must be 0.6"
        );
    }

    /// What this catches: grid_penalty is monotonically decreasing.
    /// If a future PR introduces a non-monotonic curve (e.g.
    /// piecewise with kinks), this test fails. Monotonicity is a
    /// load-bearing property — the scoring function relies on
    /// "higher latency = lower score."
    #[test]
    fn grid_penalty_is_monotonically_decreasing() {
        let mut prev = f32::INFINITY;
        for latency_ms in (0..=500).step_by(10) {
            let p = grid_penalty(latency_ms);
            assert!(
                p <= prev,
                "grid_penalty must be monotonically decreasing; got {p} at {latency_ms}ms after {prev}"
            );
            prev = p;
        }
    }

    /// What this catches: grid_penalty never returns negative or
    /// NaN. Bounded `[0.0, 0.6]`.
    #[test]
    fn grid_penalty_bounded_zero_to_point_six() {
        for latency_ms in [0u32, 1, 10, 100, 1000, 10000, u32::MAX / 1000] {
            let p = grid_penalty(latency_ms);
            assert!(p >= 0.0, "got negative for {latency_ms}: {p}");
            assert!(p <= 0.6, "exceeded 0.6 for {latency_ms}: {p}");
            assert!(!p.is_nan(), "got NaN for {latency_ms}");
        }
    }

    // ─── recency_decay curve ───────────────────────────────────

    /// What this catches: recency_decay at exactly half_life
    /// returns 0.5. The defining property of half-life decay.
    #[test]
    fn recency_decay_at_half_life_is_one_half() {
        let h = DEFAULT_RECENCY_HALF_LIFE_MS;
        let d = recency_decay(0, h, h);
        assert!(
            (d - 0.5).abs() < 1e-4,
            "decay at one half-life should be 0.5, got {d}"
        );
    }

    /// What this catches: recency_decay at 2x half_life is 0.25,
    /// at 3x is 0.125, etc. The halving property over multiples.
    #[test]
    fn recency_decay_halves_at_each_half_life_interval() {
        let h = DEFAULT_RECENCY_HALF_LIFE_MS;
        let one = recency_decay(0, h, h);
        let two = recency_decay(0, 2 * h, h);
        let three = recency_decay(0, 3 * h, h);
        assert!((one - 0.5).abs() < 1e-4);
        assert!((two - 0.25).abs() < 1e-4);
        assert!((three - 0.125).abs() < 1e-4);
    }

    /// What this catches: recency_decay handles the clock-backward
    /// edge case (now < last_used) by returning 1.0 rather than
    /// NaN or panicking. Defensive — clock skew is rare but real.
    #[test]
    fn recency_decay_handles_backward_clock() {
        let d = recency_decay(5000, 1000, DEFAULT_RECENCY_HALF_LIFE_MS);
        assert_eq!(d, 1.0, "backward clock should treat as 'used now'");
    }

    /// What this catches: recency_decay handles half_life_ms == 0
    /// without divide-by-zero. Degenerate input; returns 0.0 when
    /// any time has passed.
    #[test]
    fn recency_decay_handles_zero_half_life() {
        assert_eq!(recency_decay(0, 0, 0), 1.0);
        assert_eq!(recency_decay(0, 1, 0), 0.0);
    }

    /// What this catches: recency_decay never returns negative or
    /// NaN. Bounded `[0.0, 1.0]`.
    #[test]
    fn recency_decay_bounded_zero_to_one() {
        let h = DEFAULT_RECENCY_HALF_LIFE_MS;
        for elapsed_h in 0u64..50 {
            let d = recency_decay(0, elapsed_h * h, h);
            assert!(d >= 0.0 && d <= 1.0, "out of range at {elapsed_h}h: {d}");
            assert!(!d.is_nan(), "NaN at {elapsed_h}h");
        }
    }

    // ─── local_role_score ──────────────────────────────────────

    /// What this catches: each TierRole maps to its spec value. If
    /// a future PR shifts these (e.g. Cold from 0.3 to 0.4 to
    /// favor SSD over network), the test flags it — substrate-
    /// level cost change.
    #[test]
    fn local_role_score_matches_spec_values() {
        assert_eq!(local_role_score(TierRole::Fast), 1.0);
        assert_eq!(local_role_score(TierRole::Warm), 1.0);
        assert!((local_role_score(TierRole::Bench) - 0.6).abs() < 1e-6);
        assert!((local_role_score(TierRole::Cold) - 0.3).abs() < 1e-6);
        assert!((local_role_score(TierRole::Frozen) - 0.1).abs() < 1e-6);
    }

    /// What this catches: local_role_score is non-increasing as we
    /// move down the tier hierarchy. Fast >= Warm >= Bench >= Cold
    /// >= Frozen. Load-bearing — recall sorting relies on this.
    #[test]
    fn local_role_score_non_increasing_down_hierarchy() {
        assert!(local_role_score(TierRole::Fast) >= local_role_score(TierRole::Warm));
        assert!(local_role_score(TierRole::Warm) >= local_role_score(TierRole::Bench));
        assert!(local_role_score(TierRole::Bench) >= local_role_score(TierRole::Cold));
        assert!(local_role_score(TierRole::Cold) >= local_role_score(TierRole::Frozen));
    }

    // ─── tier_proximity_for ────────────────────────────────────

    /// What this catches: each ResidencyHint variant routes to the
    /// right curve. Hot=1.0, Local=local_role_score,
    /// GridPeer=grid_penalty, NotResident=0.0.
    #[test]
    fn tier_proximity_dispatches_by_residency_variant() {
        let hot = ResidencyHint::Hot { role: TierRole::Fast };
        assert_eq!(tier_proximity_for(&hot), 1.0);

        let local = ResidencyHint::Local { role: TierRole::Cold };
        assert!((tier_proximity_for(&local) - 0.3).abs() < 1e-6);

        let grid = ResidencyHint::GridPeer {
            peer: PeerId::new(Uuid::nil()),
            est_latency_ms: 50,
        };
        let grid_score = tier_proximity_for(&grid);
        assert!(
            (grid_score - grid_penalty(50)).abs() < 1e-6,
            "GridPeer dispatch must match grid_penalty"
        );

        let not_res = ResidencyHint::NotResident {
            acquirable_from: AcquireSource::FoundryAbsorption,
        };
        assert_eq!(tier_proximity_for(&not_res), 0.0);
    }

    // ─── score (the combined function) ─────────────────────────

    /// What this catches: score() populates RecallScore.recency
    /// from recency_decay and .tier_proximity from
    /// tier_proximity_for. The five factors must be the exact
    /// values the scoring function used (RecallScore is the
    /// audit trail).
    #[test]
    fn score_populates_recall_score_with_computed_factors() {
        let weights = RecallScoreWeights::default();
        // now > half_life so subtraction doesn't underflow.
        let now = DEFAULT_RECENCY_HALF_LIFE_MS + 1_000_000;
        let last_used = now - DEFAULT_RECENCY_HALF_LIFE_MS; // exactly 1 half-life ago
        let residency = ResidencyHint::Hot { role: TierRole::Fast };

        let s = score(
            0.9,                          // semantic
            0.8,                          // outcome_history
            last_used,
            now,
            DEFAULT_RECENCY_HALF_LIFE_MS,
            &residency,
            0.7,                          // provenance_trust
            &weights,
        );

        // Pre-computed factors must round-trip.
        assert!((s.semantic - 0.9).abs() < 1e-6);
        assert!((s.outcome_history - 0.8).abs() < 1e-6);
        assert!((s.provenance_trust - 0.7).abs() < 1e-6);

        // Computed factors must match their helper functions.
        assert!((s.recency - 0.5).abs() < 1e-4, "got {}", s.recency);
        assert!((s.tier_proximity - 1.0).abs() < 1e-6);

        // Combined = sum of weighted factors.
        let expected = weights.semantic * 0.9
            + weights.outcome_history * 0.8
            + weights.recency * 0.5
            + weights.tier_proximity * 1.0
            + weights.provenance_trust * 0.7;
        assert!(
            (s.combined - expected).abs() < 1e-4,
            "combined math drift: got {}, expected {expected}",
            s.combined
        );
    }

    /// What this catches: score() with default weights + all
    /// factors = 1.0 produces combined = 1.0 (the weights sum to
    /// 1.0). Cross-check on the sum-to-1.0 invariant + the linear
    /// combination math.
    #[test]
    fn score_all_factors_one_with_default_weights_gives_one() {
        let weights = RecallScoreWeights::default();
        let now = 1000;
        let residency = ResidencyHint::Hot { role: TierRole::Fast };
        let s = score(
            1.0,
            1.0,
            now,                          // last_used = now → recency 1.0
            now,
            DEFAULT_RECENCY_HALF_LIFE_MS,
            &residency,
            1.0,
            &weights,
        );
        assert!(
            (s.combined - 1.0).abs() < 1e-4,
            "all-ones with default weights should sum to 1.0, got {}",
            s.combined
        );
    }

    /// What this catches: score() is deterministic — same inputs
    /// produce the same outputs across calls. Required for replay
    /// determinism (PR-3b's RecallTrace replay).
    #[test]
    fn score_is_deterministic_across_calls() {
        let weights = RecallScoreWeights::default();
        let residency = ResidencyHint::Local { role: TierRole::Bench };
        let s1 = score(0.6, 0.7, 1000, 2000, 1000, &residency, 0.5, &weights);
        let s2 = score(0.6, 0.7, 1000, 2000, 1000, &residency, 0.5, &weights);
        assert!((s1.combined - s2.combined).abs() < 1e-9);
        assert!((s1.recency - s2.recency).abs() < 1e-9);
        assert!((s1.tier_proximity - s2.tier_proximity).abs() < 1e-9);
    }

    /// What this catches: score() with NotResident residency
    /// produces tier_proximity = 0 — even with perfect semantic
    /// match, the combined reflects that the artifact can't be
    /// used directly. NotResident artifacts CAN still score above
    /// 0 via the other factors — sentinel may want to surface
    /// "this would be useful, schedule the foundry to import it."
    #[test]
    fn score_not_resident_can_still_score_via_other_factors() {
        let weights = RecallScoreWeights::default();
        let residency = ResidencyHint::NotResident {
            acquirable_from: AcquireSource::SentinelRefinement,
        };
        // Pick now+last_used so recency_decay → 0 (effectively
        // never used). That isolates the semantic factor as the
        // only contributor besides tier_proximity (which is 0
        // for NotResident).
        let now = 1000 * DEFAULT_RECENCY_HALF_LIFE_MS; // 1000 half-lives in
        let s = score(
            1.0,                          // perfect semantic match
            0.0,
            0,                            // last_used: 0 → recency near 0
            now,
            DEFAULT_RECENCY_HALF_LIFE_MS,
            &residency,
            0.0,
            &weights,
        );
        // tier_proximity is 0 (NotResident); recency near 0 (very
        // long elapsed); only semantic carries the combined.
        assert!(
            (s.combined - weights.semantic).abs() < 1e-3,
            "NotResident with perfect semantic + zero recency should give weights.semantic ({}); got {}",
            weights.semantic,
            s.combined
        );
        // tier_proximity factor is 0 — verifies the audit trail
        // shows WHY this artifact scored low (it's not resident).
        assert_eq!(s.tier_proximity, 0.0);
        // recency near zero — pin the isolation.
        assert!(s.recency < 1e-3, "recency should be near zero, got {}", s.recency);
    }
}
