//! `embedding/similarity-matrix` — pairwise cosine similarities over a set of
//! embeddings, returned as the flat lower-triangular matrix (n·(n-1)/2 values).

use std::time::Instant;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::embedding::pairwise_similarity_matrix;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/embedding/SimilarityMatrixParams.ts"
)]
pub struct SimilarityMatrixParams {
    /// Embeddings to compare pairwise. All must share one dimension.
    pub embeddings: Vec<Vec<f32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/embedding/SimilarityMatrixResult.ts"
)]
pub struct SimilarityMatrixResult {
    /// Flat lower-triangular matrix: similarity of pair (i, j) for every i < j,
    /// row-major. Length = `pairs`. Empty when fewer than two embeddings.
    pub similarities: Vec<f32>,
    /// Number of input embeddings.
    pub count: usize,
    /// Number of pairs scored = count·(count-1)/2.
    pub pairs: usize,
    /// Shared dimension of the embeddings (0 when fewer than two).
    pub dimensions: usize,
    /// Compute time in milliseconds.
    pub duration_ms: u64,
}

/// Pairwise cosine similarities over a set of embeddings (flat lower triangle).
#[derive(Default)]
pub struct EmbeddingSimilarityMatrix;

#[async_trait]
impl ActionCommand for EmbeddingSimilarityMatrix {
    const NAME: &'static str = "embedding/similarity-matrix";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Pairwise cosine similarities over a set of embeddings, returned as the flat \
         lower-triangular matrix (n·(n-1)/2 values, row-major).";
    type Params = SimilarityMatrixParams;
    type Output = SimilarityMatrixResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: SimilarityMatrixParams,
    ) -> Result<SimilarityMatrixResult, CommandError> {
        let n = p.embeddings.len();
        if n < 2 {
            return Ok(SimilarityMatrixResult {
                similarities: Vec::new(),
                count: n,
                pairs: 0,
                dimensions: 0,
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
        let similarities = pairwise_similarity_matrix(&p.embeddings);
        let duration_ms = start.elapsed().as_millis() as u64;
        let pairs = similarities.len();

        Ok(SimilarityMatrixResult {
            similarities,
            count: n,
            pairs,
            dimensions: dim,
            duration_ms,
        })
    }
}
crate::register_stateless_command!(EmbeddingSimilarityMatrix);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: three vectors → exactly 3 pairs in (0,1),(0,2),(1,2)
    // order, the diagonal is never emitted, and a ragged input fails loud rather
    // than indexing OOB in the kernel.
    #[tokio::test]
    async fn three_vectors_emit_three_ordered_pairs() {
        let out = EmbeddingSimilarityMatrix
            .run(
                &Ctx::default(),
                SimilarityMatrixParams {
                    embeddings: vec![vec![1.0, 0.0], vec![1.0, 0.0], vec![0.0, 1.0]],
                },
            )
            .await
            .expect("ok");
        assert_eq!(out.count, 3);
        assert_eq!(out.pairs, 3);
        assert_eq!(out.similarities.len(), 3);
        // pair (0,1) identical → 1, pairs (0,2) and (1,2) orthogonal → 0
        assert!((out.similarities[0] - 1.0).abs() < 1e-6);
        assert!(out.similarities[1].abs() < 1e-6);
        assert!(out.similarities[2].abs() < 1e-6);

        let err = EmbeddingSimilarityMatrix
            .run(
                &Ctx::default(),
                SimilarityMatrixParams {
                    embeddings: vec![vec![1.0, 2.0], vec![1.0]],
                },
            )
            .await;
        assert!(matches!(err, Err(CommandError::Invalid(_))));
    }
}
