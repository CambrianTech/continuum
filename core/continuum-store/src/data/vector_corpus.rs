//! The resident vector matrix per collection; cache cell detachment owns invalidation.
use super::vector_index::{CpuMatrix, RecallTarget, ResidentMatrix};
use super::DataState;
use std::sync::Arc;

/// The binary field every vector collection declares for its embeddings.
pub(super) const EMBEDDING_FIELD: &str = "embedding";

impl DataState {
    /// Builds the collection's resident matrix from native pages: raw producer bytes are
    /// appended into one buffer, never decoded into JSON numbers. The storage owner's bulk
    /// `binary_page_into` replaces the per-record append below with one page append (and a
    /// revision watermark replaces whole rebuilds with deltas).
    pub(super) async fn vector_corpus(
        &self,
        handle: &str,
        collection: &str,
    ) -> Result<Arc<CpuMatrix>, String> {
        let scope = self.resolve_handle(handle)?;
        self.vector_cache
            .get_or_try_compute(&scope, collection, async {
                let adapter = self.adapter_for_backend(handle, scope.clone()).await?;
                // The store's page size bounds one transfer, not the searchable corpus:
                // every page is visited before the matrix is published.
                let page_size = super::default_page_size();
                let mut matrix = CpuMatrix::default();
                let mut after = None;
                loop {
                    let page = adapter
                        .binary_page(collection, EMBEDDING_FIELD, after.take(), page_size)
                        .await
                        .into_result()?;
                    let full = page.len() == page_size;
                    let last = page.last().map(|record| record.id.clone());
                    for record in page {
                        // A row without a vector is not indexed yet; it is not an error.
                        let Some(value) = record.value else { continue };
                        matrix
                            .append_rows(value.descriptor(), value.bytes(), vec![record.id.clone()])
                            .map_err(|error| format!("vector corpus record {}: {error}", record.id))?;
                    }
                    if !full {
                        break;
                    }
                    after = Some(last.ok_or("full vector page has no final identity")?);
                }
                matrix.calibrate(RecallTarget::STORE);
                Ok::<_, String>(matrix)
            })
            .await
            .map_err(|error| error.to_string())
    }
}
