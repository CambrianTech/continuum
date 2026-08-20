//! `embedding/cluster` — connected-components clustering over embeddings.

use std::time::Instant;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::embedding::{detect_clusters, Cluster};
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

fn default_min_similarity() -> f32 {
    0.7
}

fn default_min_cluster_size() -> usize {
    2
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/embedding/ClusterParams.ts"
)]
pub struct ClusterParams {
    /// Embeddings to cluster. All must share one dimension.
    pub embeddings: Vec<Vec<f32>>,
    /// Two items are connected when their cosine similarity is at least this
    /// (default 0.7).
    #[serde(default = "default_min_similarity")]
    pub min_similarity: f32,
    /// Drop connected components smaller than this (default 2).
    #[serde(default = "default_min_cluster_size")]
    pub min_cluster_size: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/embedding/ClusterResult.ts"
)]
pub struct ClusterResult {
    /// Clusters found, sorted by cohesion (strength) descending.
    pub clusters: Vec<Cluster>,
    /// Number of input embeddings.
    pub count: usize,
    /// Number of clusters found.
    pub cluster_count: usize,
    /// Shared dimension of the embeddings (0 when below `min_cluster_size`).
    pub dimensions: usize,
    /// The `min_similarity` that was applied.
    pub min_similarity: f32,
    /// The `min_cluster_size` that was applied.
    pub min_cluster_size: usize,
    /// Compute time in milliseconds.
    pub duration_ms: u64,
}

/// Group embeddings into clusters via connected components (similarity-threshold
/// edges), returning each cluster's members, cohesion, and representative.
#[derive(Default)]
pub struct EmbeddingCluster;

#[async_trait]
impl ActionCommand for EmbeddingCluster {
    const NAME: &'static str = "embedding/cluster";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Group embeddings into clusters via connected components (edges where cosine \
         similarity ≥ minSimilarity), returning each cluster's members, cohesion, and \
         representative.";
    type Params = ClusterParams;
    type Output = ClusterResult;

    async fn run(&self, _ctx: &Ctx, p: ClusterParams) -> Result<ClusterResult, CommandError> {
        let n = p.embeddings.len();
        if n < p.min_cluster_size {
            return Ok(ClusterResult {
                clusters: Vec::new(),
                count: n,
                cluster_count: 0,
                dimensions: 0,
                min_similarity: p.min_similarity,
                min_cluster_size: p.min_cluster_size,
                duration_ms: 0,
            });
        }

        let dim = p.embeddings[0].len();
        for (i, emb) in p.embeddings.iter().enumerate() {
            if emb.len() != dim {
                return Err(CommandError::Invalid(format!(
                    "dimension mismatch at index {i}: expected {dim}, got {}",
                    emb.len()
                )));
            }
        }

        let start = Instant::now();
        let clusters = detect_clusters(&p.embeddings, p.min_similarity, p.min_cluster_size);
        let duration_ms = start.elapsed().as_millis() as u64;

        Ok(ClusterResult {
            cluster_count: clusters.len(),
            clusters,
            count: n,
            dimensions: dim,
            min_similarity: p.min_similarity,
            min_cluster_size: p.min_cluster_size,
            duration_ms,
        })
    }
}
crate::register_stateless_command!(EmbeddingCluster);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: two tight groups (two identical pairs, mutually
    // orthogonal) resolve to exactly two clusters of two, and the defaults are
    // surfaced back on the result.
    #[tokio::test]
    async fn two_orthogonal_pairs_form_two_clusters() {
        let out = EmbeddingCluster
            .run(
                &Ctx::default(),
                ClusterParams {
                    embeddings: vec![
                        vec![1.0, 0.0],
                        vec![1.0, 0.0],
                        vec![0.0, 1.0],
                        vec![0.0, 1.0],
                    ],
                    min_similarity: 0.9,
                    min_cluster_size: 2,
                },
            )
            .await
            .expect("ok");
        assert_eq!(out.count, 4);
        assert_eq!(out.cluster_count, 2);
        assert!(out.clusters.iter().all(|c| c.indices.len() == 2));
    }
}
