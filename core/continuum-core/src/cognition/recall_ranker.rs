//! RecallRanker — the ADAPTER seam for "which candidate memories surface, and
//! in what order" (the hippocampus's ranking decision).
//!
//! ## Why an adapter (Joel, 2026-07-10: "I'd make an adapter either way")
//!
//! The ranking decision has two known implementations on a maturity ladder, and
//! more coming — exactly the shape that must live behind ONE trait
//! ([[joel-boundary-design-values]], the outlier-validation pattern):
//!
//! - **A (statistical, shipped):** [`SignificanceRanker`] — hypothesis testing
//!   against the embedder's MEASURED unrelated-null (H₀ = "this memory is
//!   unrelated"; surface on rejection at 3σ). Zero training data required,
//!   deterministic, debuggable — the bootstrap AND the permanent null model.
//! - **B (learned, next):** a trained head — logistic over
//!   `[cosine, salience, recency, novelty, hit_count]` first, bilinear over the
//!   embeddings after — trained on the label source only this system has:
//!   rustc-GRADED turns ("recall that helped" vs "didn't" comes from the gym,
//!   not human annotation). Must beat A head-to-head on the replay bench
//!   before it ships (same lift>0 rule as genome adoption). A per-persona head
//!   can ride the genome.
//!
//! Everything here is geometry/statistics over embeddings — never strings,
//! never keyword lists ([[no-hardcoded-heuristics-to-steer-cognition]]; the
//! German-room acceptance test). The faculty keeps what is NOT ranking: embeds
//! + cache, budget (count/token ceilings), working-memory dedup, Hebbian hit
//! recording, probes.

use async_trait::async_trait;

/// Per-candidate features the ranker scores. Embedding-space + usage signals
/// only — content never crosses this seam, so no implementation CAN regress to
/// string matching. Grows as learned rankers need more features (recency,
/// novelty-vs-store, hit_count) without changing the trait.
pub struct RecallCandidate<'a> {
    /// The memory's embedding (same space as the query — the faculty guarantees
    /// one provider per cycle).
    pub embedding: &'a [f32],
    /// Salience-decay score from the recall metadata (0..1).
    pub salience: f32,
}

/// One candidate's verdict: its ordering score and whether it clears the
/// relevance gate at all. `score` orders survivors; `passes` decides survival —
/// kept separate because a high-salience-low-relevance memory can outscore a
/// relevant one under some blends yet must still be droppable.
#[derive(Debug, Clone, Copy)]
pub struct RankVerdict {
    pub score: f32,
    pub passes: bool,
}

/// What the ranker knows about the embedding SPACE this turn (measured, never
/// assumed — see `EmbeddingProvider::unrelated_null`).
#[derive(Debug, Clone, Copy)]
pub struct SpaceCalibration {
    /// The space's measured unrelated-pair cosine null `(mean, std)`, when the
    /// provider has calibrated. `None` → the ranker falls back to its
    /// uncalibrated behavior.
    pub unrelated_null: Option<(f32, f32)>,
}

/// The ranking decision, behind the adapter seam. Implementations score + gate
/// every candidate against the query embedding; the faculty applies budget /
/// dedup / recording to the survivors.
#[async_trait]
pub trait RecallRanker: Send + Sync {
    /// Stable id — stamped into probes/captures so an A/B is attributable.
    fn id(&self) -> &'static str;

    /// Score + gate `candidates` against `query` (embeddings in the same
    /// space). Returns one verdict per candidate, in input order.
    async fn rank(
        &self,
        query: &[f32],
        candidates: &[RecallCandidate<'_>],
        space: SpaceCalibration,
    ) -> Vec<RankVerdict>;
}

/// Implementation A — the statistical baseline (see module docs). Blends
/// cosine-relevance with salience for ORDERING, and gates SURVIVAL by
/// significance against the space's measured unrelated-null.
pub struct SignificanceRanker {
    /// Blend weight for relevance vs salience in the ordering score:
    /// `weight·rel + (1−weight)·salience`. The A/B bench sweeps it; a learned
    /// ranker replaces it with a trained combination.
    pub relevance_weight: f32,
    /// Significance bar in σ vs the measured null (conventional 3σ — a
    /// statistical constant, not an embedder constant).
    pub sigma: f32,
    /// Fallback absolute cosine floor for an UNCALIBRATED space (legacy
    /// behavior — meaningful only where the space's null genuinely sits near 0).
    pub uncalibrated_floor: f32,
    /// Numerical guard for a degenerate measured σ ≈ 0 (e.g. the lexical space,
    /// where disjoint vocabularies score exactly 0). Division safety, not a
    /// tunable: with σ this small any nonzero cosine is significant — correct
    /// for such a space.
    pub std_epsilon: f32,
}

impl SignificanceRanker {
    pub fn new(relevance_weight: f32, uncalibrated_floor: f32) -> Self {
        Self {
            relevance_weight,
            sigma: 3.0,
            uncalibrated_floor,
            std_epsilon: 1e-4,
        }
    }
}

/// Blend a relevance score with a salience score at `weight` (the ordering
/// combination implementation A uses; a learned ranker supersedes it).
fn blend(salience: f32, relevance: f32, weight: f32) -> f32 {
    weight * relevance + (1.0 - weight) * salience
}

#[async_trait]
impl RecallRanker for SignificanceRanker {
    fn id(&self) -> &'static str {
        "significance-3sigma"
    }

    async fn rank(
        &self,
        query: &[f32],
        candidates: &[RecallCandidate<'_>],
        space: SpaceCalibration,
    ) -> Vec<RankVerdict> {
        let gated = self.relevance_weight > 0.0;
        candidates
            .iter()
            .map(|c| {
                let rel = crate::cognition::embedding::cosine_similarity(query, c.embedding);
                let passes = if !gated {
                    true
                } else if let Some((mu, sd)) = space.unrelated_null {
                    // H₀: "unrelated". Reject (→ surface) at `sigma` vs the
                    // MEASURED null — pure geometry, embedder- and
                    // language-agnostic.
                    (rel - mu) / sd.max(self.std_epsilon) >= self.sigma
                } else {
                    rel >= self.uncalibrated_floor
                };
                RankVerdict {
                    score: blend(c.salience, rel, self.relevance_weight),
                    passes,
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the adapter contract itself — implementation A gates by
    // significance vs a calibrated null (junk AT the null fails even at max
    // salience; a true match passes from low salience), and falls back to the
    // absolute floor when the space is uncalibrated. Any future implementation B
    // (learned) is A/B'd against these same semantics on the replay bench.
    #[tokio::test]
    async fn significance_ranker_gates_by_measured_null_and_falls_back_uncalibrated() {
        let ranker = SignificanceRanker::new(0.5, 0.15);
        let query = [1.0f32, 0.0];
        let junk = [0.28f32, 0.96]; // cos = 0.28 ≈ the null
        let matching = [0.8f32, 0.6]; // cos = 0.8 → z ≈ 13
        let cands = [
            RecallCandidate { embedding: &junk, salience: 0.99 },
            RecallCandidate { embedding: &matching, salience: 0.4 },
        ];

        // Calibrated space (μ=0.27, σ=0.04): junk fails, match passes.
        let v = ranker
            .rank(&query, &cands, SpaceCalibration { unrelated_null: Some((0.27, 0.04)) })
            .await;
        assert!(!v[0].passes, "null-scoring junk must fail even at salience 0.99");
        assert!(v[1].passes, "a significant match must pass from low salience");

        // Uncalibrated space: absolute-floor fallback (both clear 0.15 here —
        // legacy behavior preserved for spaces whose null genuinely sits near 0).
        let v = ranker
            .rank(&query, &cands, SpaceCalibration { unrelated_null: None })
            .await;
        assert!(v[0].passes && v[1].passes, "uncalibrated space keeps the legacy floor");
    }
}
