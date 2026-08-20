//! `embedding/top-k` — the top-k most similar targets to a query embedding.

use std::time::Instant;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::embedding::top_k_similar;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

fn default_k() -> usize {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/embedding/TopKParams.ts"
)]
pub struct TopKParams {
    /// Query embedding.
    pub query: Vec<f32>,
    /// Candidate embeddings to rank against the query. All must match `query`'s
    /// dimension.
    pub targets: Vec<Vec<f32>>,
    /// How many top matches to return (default 10).
    #[serde(default = "default_k")]
    pub k: usize,
    /// Minimum similarity a target must clear to be included (default 0.0).
    #[serde(default)]
    pub threshold: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/embedding/TopKHit.ts"
)]
pub struct TopKHit {
    /// Index of the target in the input `targets` array.
    pub index: usize,
    /// Cosine similarity to the query.
    pub similarity: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/embedding/TopKResult.ts"
)]
pub struct TopKResult {
    /// Matches above `threshold`, sorted by similarity descending, capped at `k`.
    pub results: Vec<TopKHit>,
    /// Number of returned matches.
    pub count: usize,
    /// Number of candidate targets scored.
    pub total_targets: usize,
    /// The `k` that was applied.
    pub k: usize,
    /// The `threshold` that was applied.
    pub threshold: f32,
    /// Dimension of the query (0 when there were no targets).
    pub dimensions: usize,
    /// Compute time in milliseconds.
    pub duration_ms: u64,
}

/// Rank candidate embeddings by cosine similarity to a query and return the top k.
#[derive(Default)]
pub struct EmbeddingTopK;

#[async_trait]
impl ActionCommand for EmbeddingTopK {
    const NAME: &'static str = "embedding/top-k";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Rank candidate embeddings by cosine similarity to a query and return the top k \
         (above an optional threshold), sorted descending.";
    type Params = TopKParams;
    type Output = TopKResult;

    async fn run(&self, _ctx: &Ctx, p: TopKParams) -> Result<TopKResult, CommandError> {
        let dim = p.query.len();
        if p.targets.is_empty() {
            return Ok(TopKResult {
                results: Vec::new(),
                count: 0,
                total_targets: 0,
                k: p.k,
                threshold: p.threshold,
                dimensions: dim,
                duration_ms: 0,
            });
        }

        for (i, target) in p.targets.iter().enumerate() {
            if target.len() != dim {
                return Err(CommandError::Invalid(format!(
                    "dimension mismatch at target index {i}: expected {dim}, got {}",
                    target.len()
                )));
            }
        }

        let start = Instant::now();
        let ranked = top_k_similar(&p.query, &p.targets, p.k, p.threshold);
        let duration_ms = start.elapsed().as_millis() as u64;

        let total_targets = p.targets.len();
        let results: Vec<TopKHit> = ranked
            .into_iter()
            .map(|(index, similarity)| TopKHit { index, similarity })
            .collect();

        Ok(TopKResult {
            count: results.len(),
            results,
            total_targets,
            k: p.k,
            threshold: p.threshold,
            dimensions: dim,
            duration_ms,
        })
    }
}
crate::register_stateless_command!(EmbeddingTopK);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: ranking is similarity-descending and `k` truncates — the
    // nearest target ranks first and a k=1 request returns exactly that one.
    #[tokio::test]
    async fn ranks_descending_and_k_truncates() {
        let out = EmbeddingTopK
            .run(
                &Ctx::default(),
                TopKParams {
                    query: vec![1.0, 0.0],
                    targets: vec![
                        vec![0.0, 1.0], // orthogonal → 0
                        vec![1.0, 0.0], // identical → 1
                    ],
                    k: 1,
                    threshold: 0.0,
                },
            )
            .await
            .expect("ok");
        assert_eq!(out.count, 1);
        assert_eq!(out.total_targets, 2);
        assert_eq!(out.results[0].index, 1, "nearest (identical) ranks first");
        assert!((out.results[0].similarity - 1.0).abs() < 1e-6);
    }
}
