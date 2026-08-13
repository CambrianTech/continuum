//! `embedding/similarity` — cosine similarity between two embedding vectors.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::modules::embedding::cosine_similarity;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/embedding/SimilarityParams.ts"
)]
pub struct SimilarityParams {
    /// First embedding vector.
    pub a: Vec<f32>,
    /// Second embedding vector — must match `a`'s dimension.
    pub b: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/embedding/SimilarityResult.ts"
)]
pub struct SimilarityResult {
    /// Cosine similarity in [-1, 1].
    pub similarity: f32,
    /// Dimension of the compared vectors.
    pub dimensions: usize,
}

/// Cosine similarity between two embedding vectors of equal dimension.
#[derive(Default)]
pub struct EmbeddingSimilarity;

#[async_trait]
impl ActionCommand for EmbeddingSimilarity {
    const NAME: &'static str = "embedding/similarity";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Cosine similarity between two embedding vectors of equal dimension (result in [-1, 1]).";
    type Params = SimilarityParams;
    type Output = SimilarityResult;

    async fn run(&self, _ctx: &Ctx, p: SimilarityParams) -> Result<SimilarityResult, CommandError> {
        if p.a.len() != p.b.len() {
            return Err(CommandError::Invalid(format!(
                "dimension mismatch: {} vs {}",
                p.a.len(),
                p.b.len()
            )));
        }
        Ok(SimilarityResult {
            similarity: cosine_similarity(&p.a, &p.b),
            dimensions: p.a.len(),
        })
    }
}
crate::register_stateless_command!(EmbeddingSimilarity);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the typed wrapper scores identical vectors as 1.0 and
    // fails loud (Invalid) on a dimension mismatch instead of returning garbage —
    // the contract the persona tool surface now depends on.
    #[tokio::test]
    async fn identical_is_one_and_mismatch_errors() {
        let out = EmbeddingSimilarity
            .run(
                &Ctx::default(),
                SimilarityParams {
                    a: vec![0.5, 0.25, 0.75],
                    b: vec![0.5, 0.25, 0.75],
                },
            )
            .await
            .expect("ok");
        assert!((out.similarity - 1.0).abs() < 1e-6);
        assert_eq!(out.dimensions, 3);

        let err = EmbeddingSimilarity
            .run(
                &Ctx::default(),
                SimilarityParams {
                    a: vec![1.0, 2.0],
                    b: vec![1.0],
                },
            )
            .await;
        assert!(matches!(err, Err(CommandError::Invalid(_))));
    }
}
