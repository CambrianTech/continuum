//! Immutable vector snapshots; cache cell detachment owns invalidation.
use super::{DataState, StorageQuery};
use std::sync::Arc;

pub(super) struct CachedVector {
    pub(super) id: String,
    pub(super) embedding: Vec<f64>,
}

impl DataState {
    pub(super) async fn vector_corpus(
        &self,
        handle: &str,
        collection: &str,
    ) -> Result<Arc<Vec<CachedVector>>, String> {
        self.vector_cache
            .get_or_try_compute(handle, collection, async {
                let adapter = self.get_adapter(handle).await?;
                let result = adapter
                    .query(StorageQuery {
                        collection: collection.to_owned(),
                        ..Default::default()
                    })
                    .await;
                if !result.success {
                    return Err(match result.error {
                        Some(error) => error,
                        None => format!(
                            "vector corpus query for {collection} failed without an error receipt"
                        ),
                    });
                }
                let records = result.data.ok_or_else(|| {
                    format!("vector corpus query for {collection} succeeded without data")
                })?;
                let mut vectors = Vec::new();
                for record in records {
                    if let Some(embedding) = record.data.get("embedding") {
                        let embedding = Self::parse_embedding(embedding);
                        if !embedding.is_empty() {
                            vectors.push(CachedVector {
                                id: record.id,
                                embedding,
                            });
                        }
                    }
                }
                Ok(vectors)
            })
            .await
            .map_err(|error| error.to_string())
    }
}
