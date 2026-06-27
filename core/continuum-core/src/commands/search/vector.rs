//! `search/vector` — rank corpus vectors by cosine similarity to a query vector.

use super::engine::{CosineAlgorithm, SearchResult, VectorSearchInput};

crate::action_command! {
    /// Rank a list of corpus vectors by cosine similarity to a query vector.
    /// Returns per-vector similarity scores and the indices sorted best-first.
    /// Use for embedding / semantic similarity over precomputed vectors.
    pub struct SearchVector;
    name: "search/vector",
    access: AiSafe,
    params: VectorSearchInput,
    output: SearchResult,
    run(_this, _ctx, p) => {
        let algo = CosineAlgorithm {
            normalize: p.normalize,
            threshold: p.threshold,
        };
        let output = algo.vector_search(&p);
        Ok(SearchResult {
            algorithm: "cosine".to_string(),
            scores: output.scores,
            ranked_indices: output.ranked_indices,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;

    // what this catches: identical query and corpus vector ranks first with a
    // similarity at the top — the cosine contract the command exposes.
    #[tokio::test]
    async fn identical_vector_ranks_first() {
        let cmd = SearchVector;
        let out = cmd
            .run(
                &crate::sdk_codegen::Ctx::default(),
                VectorSearchInput {
                    query_vector: vec![1.0, 0.0, 0.0],
                    corpus_vectors: vec![vec![0.0, 1.0, 0.0], vec![1.0, 0.0, 0.0]],
                    normalize: true,
                    threshold: 0.0,
                },
            )
            .await
            .unwrap();
        assert_eq!(out.algorithm, "cosine");
        assert_eq!(out.ranked_indices[0], 1);
        assert!(out.scores[1] > out.scores[0]);
    }
}
