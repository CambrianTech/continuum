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
        // Best first, so the threshold ends the hits at the first score below it.
        let hits: Vec<RecordHit> = ranked.into_iter().take_while(|hit| f64::from(hit.score) >= threshold).collect();
        // Every hit's record in ONE store query, never one read per hit.
        let mut records: std::collections::HashMap<String, serde_json::Value> = if include_data && !hits.is_empty() {
            let adapter = self.get_adapter(handle).await.map_err(|fault| VectorStoreError::Open(fault.into()))?;
            let ids = hits.iter().map(|hit| serde_json::Value::String(hit.id.to_string())).collect();
            let query = crate::orm::QueryBuilder::new(collection)
                .filter("id", crate::orm::query::QueryOperator::In(ids))
                .limit(hits.len())
                .build();
            adapter.query(&query).await?.into_iter().map(|record| (record.id, record.data)).collect()
        } else {
            std::collections::HashMap::new()
        };
        let mut results = Vec::with_capacity(hits.len());
        for RecordHit { id, score } in hits {
            let data = if include_data {
                // A ranked row the query did not return was deleted after ranking.
                Some(records.remove(&id.to_string()).ok_or_else(|| VectorStoreError::Hydration {
                    id,
                    fault: crate::orm::StorageError::NotFound { collection: collection.to_owned().into(), id: id.to_string() },
                })?)
            } else {
                None
            };
            let score = f64::from(score);
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
