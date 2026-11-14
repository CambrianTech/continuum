//! Vector ranking and hydration of selected records.
use super::vector_index::{unit_f32, EmbedderId, Producer, QueryVector, RecordHit, VectorStoreError};
use super::{require_public_collection, DataState, VectorHit, VectorSearchResults};
use crate::log_info;
use std::num::NonZeroUsize;

/// A `vector/search` query and the producer it names: a served embedder, or the caller.
pub(crate) fn wire_query<'a>(values: &'a [f64], embedder: Option<&str>) -> QueryVector<'a> {
    let producer = match embedder {
        Some(id) => Producer::Embedder(EmbedderId::new(id)),
        None => Producer::WireQuery,
    };
    QueryVector { values, producer }
}

impl DataState {
    /// Nearest-neighbour search over a collection's resident unit-vector matrix.
    ///
    /// The query is the one vector normalized here: it arrives from the wire as numbers,
    /// so this is where its descriptor is built, naming its producer (the space the caller
    /// says it embedded in). Rows were normalized by theirs at write. Only the matrix whose
    /// descriptor is compatible with the query's is scored; each score is one dot product
    /// (cosine for unit vectors), returned best first above `threshold`.
    pub(crate) async fn vector_search(
        &self,
        handle: &str,
        collection: &str,
        query: QueryVector<'_>,
        k: usize,
        threshold: f64,
        include_data: bool,
    ) -> Result<VectorSearchResults, VectorStoreError> {
        require_public_collection(collection).map_err(|fault| VectorStoreError::Refused(fault.into()))?;
        let search_start = std::time::Instant::now();

        let corpus = self.vector_corpus(handle, collection).await?;
        let corpus_size = corpus.len();
        let Some(k) = NonZeroUsize::new(k).filter(|_| corpus_size > 0) else {
            return Ok(VectorSearchResults { results: Vec::new(), count: 0, corpus_size });
        };
        // The wire carries f64 numbers; the matrix is f32. Narrowing is the conversion the
        // wire forces, fused into the query's one normalization and its one byte copy.
        let query = unit_f32(query.values.iter().map(|value| *value as f32), &query.producer)?;
        let ranked = corpus.search(&query, k)?;
        // Best first, so the threshold ends the walk at the first score below it.
        let adapter = if include_data { Some(self.get_adapter(handle).await.map_err(|fault| VectorStoreError::Store(fault.into()))?) } else { None };
        let mut results = Vec::with_capacity(ranked.len());
        for RecordHit { id, score } in ranked {
            let score = f64::from(score);
            if score < threshold {
                break;
            }
            let data = match &adapter {
                Some(adapter) => Some(
                    adapter
                        .read(collection, &id.to_string())
                        .await
                        .into_result()
                        .map_err(|fault| VectorStoreError::Hydration { id, fault: fault.into() })?
                        .data,
                ),
                None => None,
            };
            results.push(VectorHit { id: id.to_string(), score, distance: 1.0 - score, data });
        }
        let count = results.len();

        log_info!(
            "data",
            "vector/search",
            "Complete: {} results from {} vectors in {:?}",
            count,
            corpus_size,
            search_start.elapsed()
        );

        Ok(VectorSearchResults { results, count, corpus_size })
    }
}
