//! Immutable vector snapshots; cache cell detachment owns invalidation.
use super::{DataState, StorageQuery};
use crate::orm::query::{Cursor, CursorDirection, SortDirection, SortSpec};
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
        let scope = self.resolve_handle(handle)?;
        self.vector_cache
            .get_or_try_compute(&scope, collection, async {
                let adapter = self.adapter_for_backend(handle, scope.clone()).await?;
                // Reuse the store's page size for transient decoding, not a cap on
                // the searchable corpus. Every page is visited before publication.
                let page_size = super::default_page_size();
                let mut cursor = None;
                let mut vectors = Vec::new();
                loop {
                    let records = adapter.query(StorageQuery {
                        collection: collection.to_owned(),
                        select: Some(vec!["embedding".into()]),
                        sort: Some(vec![SortSpec { field: "id".into(), direction: SortDirection::Asc }]),
                        cursor,
                        limit: Some(page_size),
                        ..Default::default()
                    }).await.into_result()?;
                    let page_len = records.len();
                    let last_id = records.last().map(|record| record.id.clone());
                    for record in records {
                        if let Some(embedding) = record.data.get("embedding").filter(|value| !value.is_null()) {
                            let embedding = Self::parse_embedding(embedding)
                                .map_err(|e| format!("vector corpus record {}: {e}", record.id))?;
                            if !embedding.is_empty() {
                                vectors.push(CachedVector { id: record.id, embedding });
                            }
                        }
                    }
                    if page_len < page_size { break; }
                    cursor = Some(Cursor {
                        field: "id".into(),
                        value: last_id.ok_or("full vector page has no final identity")?.into(),
                        direction: CursorDirection::After,
                    });
                }
                Ok::<_, String>(vectors)
            })
            .await
            .map_err(|error| error.to_string())
    }
}
