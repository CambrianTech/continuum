//! Vector ranking and hydration of selected records.
use super::vector_index::{unit_f32, ResidentMatrix};
use super::{require_public_collection, DataState, VectorHit, VectorSearchResults};
use crate::log_info;
use std::num::NonZeroUsize;

/// The producer identity of a query vector that arrives on the `vector/search` wire.
const WIRE_QUERY: &str = "vector/search query";

impl DataState {
    /// Nearest-neighbour search over a collection's resident unit-vector matrix.
    ///
    /// The query is the one vector normalized here: it arrives from the wire as numbers,
    /// so this is its producer. Rows were normalized by theirs at write. Each score is one
    /// dot product (cosine for unit vectors), ranked by the matrix's prefilter and bounded
    /// top-k, and returned best first above `threshold`.
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
        let corpus_size = corpus.len();
        let Some(k) = NonZeroUsize::new(k).filter(|_| corpus_size > 0) else {
            return Ok(VectorSearchResults { results: Vec::new(), count: 0, corpus_size });
        };
        // The wire carries f64 numbers; the matrix is f32. Narrowing is the conversion the
        // wire forces, fused into the query's one normalization and its one byte copy.
        let query = unit_f32(query_vector.iter().map(|value| *value as f32), WIRE_QUERY)
            .map_err(|error| format!("vector/search query: {error}"))?;
        let ranked = corpus
            .search(&query, k)
            .map_err(|error| format!("vector/search {collection}: {error}"))?;
        // Best first, so the threshold ends the walk at the first score below it.
        let mut top_k: Vec<(String, f64)> = Vec::with_capacity(ranked.len());
        for (row, score) in ranked {
            let score = f64::from(score);
            if score < threshold {
                break;
            }
            let id = corpus
                .id(row)
                .ok_or_else(|| format!("vector/search {collection}: ranked row {row} has no identity"))?;
            top_k.push((id.to_owned(), score));
        }
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
