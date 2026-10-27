//! Vector ranking and hydration of selected records.
use super::{require_public_collection, DataState, VectorHit, VectorSearchResults};
use crate::log_info;
use rayon::prelude::*;

impl DataState {
    /// Vector similarity search with in-memory caching
    ///
    /// OPTIMIZATION: Vectors are cached in memory per (dbPath, collection).
    /// First search loads from the adapter; subsequent searches reuse the corpus.
    ///
    /// Flow:
    /// 1. Clone the collection cell under a short map guard.
    /// 2. Coalesce cold loads; errors are retryable and invalidation detaches old loads.
    /// 3. Parallel rayon search against cached vectors
    ///
    /// Cosine-similarity nearest-neighbour search over a collection's embeddings.
    /// Loads (and caches) the collection's vectors, scores them against the query
    /// vector in parallel, and returns the top-`k` hits above `threshold`.
    pub(crate) async fn vector_search(
        &self,
        handle: &str,
        collection: &str,
        query_vector: Vec<f64>,
        k: usize,
        threshold: f64,
        include_data: bool,
    ) -> Result<VectorSearchResults, String> {
        require_public_collection(collection)?;
        use std::time::Instant;
        let search_start = Instant::now();

        let corpus = self.vector_corpus(handle, collection).await?;

        if corpus.is_empty() || k == 0 {
            return Ok(VectorSearchResults {
                results: Vec::new(),
                count: 0,
                corpus_size: corpus.len(),
            });
        }

        let corpus_size = corpus.len();

        // Step 2: Parallel cosine similarity with rayon
        let query_vec = &query_vector;

        let mut scored: Vec<(usize, f64)> = corpus
            .par_iter()
            .enumerate()
            .filter_map(|(index, cv)| {
                let score = Self::cosine_similarity(query_vec, &cv.embedding);
                if score >= threshold {
                    Some((index, score))
                } else {
                    None
                }
            })
            .collect();

        // Preserve stable corpus order for equal scores, including signed zero.
        // NaNs never enter this list: they fail the threshold comparison above.
        let rank = |a: &(usize, f64), b: &(usize, f64)| {
            if a.1 == b.1 {
                a.0.cmp(&b.0)
            } else {
                b.1.total_cmp(&a.1)
            }
        };
        if k < scored.len() {
            scored.select_nth_unstable_by(k, rank);
            scored.truncate(k);
        }
        scored.sort_unstable_by(rank);
        // Only selected hits acquire owned IDs; rejected candidates borrow the corpus.
        let top_k: Vec<(String, f64)> = scored
            .into_iter()
            .map(|(index, score)| (corpus[index].id.clone(), score))
            .collect();
        let count = top_k.len();

        // Build results
        let results: Vec<VectorHit> = if include_data {
            // Fetch full records for top-k (need another query)
            let adapter = self.get_adapter(handle).await?;
            let mut full_results = Vec::new();

            for (id, score) in &top_k {
                let result = adapter.read(collection, id).await;
                if !result.success {
                    return Err(match result.error {
                        Some(error) => format!("vector result {collection}/{id}: {error}"),
                        None => format!(
                            "vector result {collection}/{id} read failed without an error receipt"
                        ),
                    });
                }
                let record = result.data.ok_or_else(|| {
                    format!("vector result {collection}/{id} read succeeded without data")
                })?;
                full_results.push(VectorHit {
                    id: id.clone(),
                    score: *score,
                    distance: 1.0 - score,
                    data: Some(record.data),
                });
            }
            full_results
        } else {
            top_k
                .into_iter()
                .map(|(id, score)| VectorHit {
                    id,
                    score,
                    distance: 1.0 - score,
                    data: None,
                })
                .collect()
        };

        log_info!(
            "data",
            "vector/search",
            "Complete: {} results from {} vectors in {:?}",
            count,
            corpus_size,
            search_start.elapsed()
        );

        Ok(VectorSearchResults {
            results,
            count,
            corpus_size,
        })
    }
}
