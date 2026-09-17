//! `embedding/similar` — rank TEXT candidates against a TEXT query, in the ONE
//! process-wide embedding space, with significance against that space's measured
//! unrelated-null.
//!
//! The vector-math siblings (`embedding/top-k`, `embedding/cluster`) take vectors
//! the caller already has. This is the text-level composition every "find me the
//! ones like this" surface needs — duplicate cards on the board (`work/similar`),
//! the citizen or human whose bio matches a need, a document among many — so the
//! embed-then-rank pair is written once, here, and never re-derived per domain.
//!
//! ## Why significance, not a bare threshold
//!
//! Neural spaces are anisotropic: unrelated texts do NOT score ~0 (Qwen3-Embedding
//! baselines near 0.25–0.30). A cosine threshold calibrated for one embedder is
//! silently wrong under another. Every hit therefore carries `z` — how many
//! standard deviations above the embedder's MEASURED unrelated-pair mean it sits —
//! and `significant` is `z >= min_z`. When the provider has not calibrated
//! (`unrelated_null() == None`) `z` is absent and `significant` falls back to the
//! raw `threshold`, and the result SAYS which rule decided (`gate`), so a reader
//! never mistakes a raw-threshold verdict for a calibrated one.
use std::sync::Arc;
use std::time::Instant;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::cognition::embedding::EmbeddingProvider;
use crate::modules::embedding::top_k_similar;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

fn default_k() -> usize {
    10
}

/// A measured null is usable only if unrelated pairs actually SPREAD: a space whose
/// unrelated cosines all land on one value has no scale to judge significance
/// against, and `mean + k·std` collapses onto the mean — every pair sharing one
/// token clears it. The lexical fallback (`lexical-fnv-tf`) measures ≈ (0, 0) by
/// construction (disjoint vocabularies share no buckets), so on a node whose
/// embedder resolved to it (the Intel tier: 8,768 `recall.embedder.resolved
/// kind=lexical` in one day — Cormac, #4140 review) a z-gate would group almost
/// everything. Below this spread the embedder is UNCALIBRATED for this purpose.
pub const MIN_NULL_STD: f32 = 0.01;

/// The gate an embedder supports, named on every result: a SEMANTIC space with a
/// usable null carries its `(mean, std)`; anything else is `uncalibrated` and
/// carries none. Non-semantic (token-overlap) spaces are uncalibrated whatever
/// their null: measured, `lexical-fnv-tf` has real spread AND scores two distinct
/// cards at 0.70 on shared stopwords — the null is honest and the space still
/// cannot say "the same".
pub fn gate_for(embedder: &dyn EmbeddingProvider) -> (&'static str, Option<(f32, f32)>) {
    match embedder.unrelated_null() {
        Some((mean, std)) if embedder.is_semantic() && std >= MIN_NULL_STD => {
            ("z", Some((mean, std)))
        }
        _ => ("uncalibrated", None),
    }
}
fn default_min_z() -> f32 {
    3.0
}

/// One thing to score: an id the caller recognises and the text that stands for it.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/embedding/SimilarCandidate.ts"
)]
pub struct SimilarCandidate {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/embedding/SimilarParams.ts"
)]
pub struct SimilarParams {
    /// The text to compare against.
    pub query: String,
    /// What to rank. Each carries its own id so the caller reads the answer back
    /// without an index dance.
    pub candidates: Vec<SimilarCandidate>,
    /// How many hits to return (default 10). Sorted by similarity, descending.
    #[serde(default = "default_k")]
    pub k: usize,
    /// Raw cosine floor, used ONLY when the embedder has no measured null.
    #[serde(default)]
    pub threshold: f32,
    /// Significance floor in standard deviations above the unrelated-pair mean
    /// (default 3). Used when the embedder is calibrated.
    #[serde(default = "default_min_z")]
    pub min_z: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/embedding/SimilarHit.ts"
)]
pub struct SimilarHit {
    pub id: String,
    pub similarity: f32,
    /// Standard deviations above the embedder's measured unrelated-pair mean.
    /// Absent when the embedder has not calibrated.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub z: Option<f32>,
    /// Past the gate the result names (`gate`).
    pub significant: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/embedding/SimilarResult.ts"
)]
pub struct SimilarResult {
    pub results: Vec<SimilarHit>,
    pub count: usize,
    pub total_candidates: usize,
    /// The embedding space every score lives in.
    pub space: String,
    /// `"z"` when significance came from a measured null with real spread;
    /// `"threshold"` when the caller passed a raw cosine floor and the null is
    /// unusable; `"uncalibrated"` when the null is unusable and no floor was given —
    /// then NOTHING is marked significant (ranking still holds; significance does not).
    pub gate: String,
    /// The measured unrelated-pair `(mean, std)` the `z` values are against.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub unrelated_null: Option<(f32, f32)>,
    pub duration_ms: u64,
}

/// Score `candidates` against `query` in `embedder`'s space. Pure over the
/// provider: the command and `work/similar` both ride this.
pub async fn rank_texts(
    embedder: &Arc<dyn EmbeddingProvider>,
    query: &str,
    candidates: &[SimilarCandidate],
    k: usize,
    threshold: f32,
    min_z: f32,
) -> Result<SimilarResult, CommandError> {
    let start = Instant::now();
    if query.trim().is_empty() {
        return Err(CommandError::Invalid("query is empty".into()));
    }
    let q = embedder.embed(query).await;
    if q.iter().all(|v| *v == 0.0) {
        return Err(CommandError::Internal(format!(
            "embedder '{}' produced no signal for the query",
            embedder.id()
        )));
    }
    let mut targets: Vec<Vec<f32>> = Vec::with_capacity(candidates.len());
    for c in candidates {
        targets.push(embedder.embed(&c.text).await);
    }
    let (gate, null) = gate_for(embedder.as_ref());
    // A raw floor is honoured only when the caller gave one AND the null is unusable;
    // with a real null the z decides; with neither, nothing is significant.
    let gate = if gate == "uncalibrated" && threshold > 0.0 {
        "threshold"
    } else {
        gate
    };
    // Rank everything so the gate sees the full ordering, then cut.
    let ranked = top_k_similar(&q, &targets, k, f32::MIN);
    let results: Vec<SimilarHit> = ranked
        .into_iter()
        .map(|(index, similarity)| {
            let z = null.map(|(mean, std)| (similarity - mean) / std);
            let significant = match (gate, z) {
                ("z", Some(z)) => z >= min_z,
                ("threshold", _) => similarity >= threshold,
                _ => false,
            };
            SimilarHit {
                id: candidates[index].id.clone(),
                similarity,
                z,
                significant,
            }
        })
        .collect();
    Ok(SimilarResult {
        count: results.len(),
        results,
        total_candidates: candidates.len(),
        space: embedder.id().to_string(),
        gate: gate.to_string(),
        unrelated_null: null,
        duration_ms: start.elapsed().as_millis() as u64,
    })
}

pub struct EmbeddingSimilar {
    pub embedder: Arc<dyn EmbeddingProvider>,
}

#[async_trait]
impl ActionCommand for EmbeddingSimilar {
    const NAME: &'static str = "embedding/similar";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Rank text candidates by similarity to a text query — cards, people, documents, anything \
         with an id and a text. Each hit carries the cosine, `z` (standard deviations above the \
         embedder's measured unrelated-pair mean) and `significant` (z >= minZ, default 3). Use it \
         to find duplicates, the citizen whose bio fits a need, or the nearest prior work. The \
         result names the embedding space and which gate decided significance.";
    type Params = SimilarParams;
    type Output = SimilarResult;

    async fn run(&self, _ctx: &Ctx, p: SimilarParams) -> Result<SimilarResult, CommandError> {
        rank_texts(
            &self.embedder,
            &p.query,
            &p.candidates,
            p.k,
            p.threshold,
            p.min_z,
        )
        .await
    }
}

crate::register_command!(EmbeddingSimilar);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::DeterministicEmbeddingProvider;

    fn cands() -> Vec<SimilarCandidate> {
        [
            ("a", "sympy expand of TensorProduct stops incomplete"),
            (
                "b",
                "astropy separability_matrix computes wrong for nested models",
            ),
            ("c", "Bug in expand of TensorProduct + Workaround + Fix"),
        ]
        .into_iter()
        .map(|(id, text)| SimilarCandidate {
            id: id.into(),
            text: text.into(),
        })
        .collect()
    }

    // what this catches: the hit carries the CALLER's id (not an index), ranks the
    // near-duplicate first, and names the gate that decided significance — an
    // uncalibrated embedder must say `threshold`, never pretend to a z.
    #[tokio::test]
    async fn ranks_by_caller_id_and_names_its_gate() {
        let e: Arc<dyn EmbeddingProvider> = Arc::new(DeterministicEmbeddingProvider);
        let out = rank_texts(
            &e,
            "expand of TensorProduct is incomplete",
            &cands(),
            2,
            0.0,
            3.0,
        )
        .await
        .expect("ranks");
        assert_eq!(out.count, 2);
        assert_eq!(out.total_candidates, 3);
        assert_eq!(
            out.gate, "uncalibrated",
            "a non-semantic fixture with no floor marks nothing significant, and says so"
        );
        assert!(out.results.iter().all(|h| h.z.is_none() && !h.significant));
        assert!(
            out.results[0].similarity >= out.results[1].similarity,
            "descending"
        );
        assert!(
            out.results.iter().any(|h| h.id == "a" || h.id == "c"),
            "a TensorProduct card outranks the astropy one: {:?}",
            out.results
        );
    }

    // what this catches (Cormac, #4140): the lexical fallback's measured null is ≈ (0, 0);
    // a z over it calls every one-token overlap significant. Its gate must read
    // `uncalibrated`, `z` must be absent, and nothing may be marked significant —
    // ranking still holds, significance does not.
    #[tokio::test]
    async fn a_degenerate_null_marks_nothing_significant_and_says_so() {
        let e: Arc<dyn EmbeddingProvider> =
            Arc::new(crate::cognition::embedding::LexicalEmbedder::new());
        let out = rank_texts(&e, "expand of TensorProduct is incomplete", &cands(), 3, 0.0, 3.0)
            .await
            .expect("ranks");
        assert_eq!(out.gate, "uncalibrated");
        assert!(out.results.iter().all(|h| h.z.is_none() && !h.significant));
        assert_eq!(out.count, 3, "ranking is still delivered");
        // With an explicit raw floor the caller takes responsibility, and the gate says so.
        let out = rank_texts(&e, "expand of TensorProduct is incomplete", &cands(), 3, 0.5, 3.0)
            .await
            .expect("ranks");
        assert_eq!(out.gate, "threshold");
    }

    #[tokio::test]
    async fn an_empty_query_is_refused() {
        let e: Arc<dyn EmbeddingProvider> = Arc::new(DeterministicEmbeddingProvider);
        assert!(rank_texts(&e, "  ", &cands(), 3, 0.0, 3.0).await.is_err());
    }
}
