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

        if corpus.is_empty() {
            return Ok(VectorSearchResults {
                results: Vec::new(),
                count: 0,
                corpus_size: 0,
            });
        }

        let corpus_size = corpus.len();

        // Step 2: Parallel cosine similarity with rayon
        let query_vec = &query_vector;

        let mut scored: Vec<(String, f64)> = corpus
            .par_iter()
            .filter_map(|cv| {
                let score = Self::cosine_similarity(query_vec, &cv.embedding);
                if score >= threshold {
                    Some((cv.id.clone(), score))
                } else {
                    None
                }
            })
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let top_k: Vec<(String, f64)> = scored.into_iter().take(k).collect();
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
                        None => format!("vector result {collection}/{id} read failed without an error receipt"),
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
