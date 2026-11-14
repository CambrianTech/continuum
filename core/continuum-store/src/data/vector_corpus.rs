//! The resident vector matrix per collection; cache cell detachment owns invalidation.
use super::vector_index::{CpuMatrix, MatrixBuilder, RecallTarget, VectorStoreError};
use super::DataState;
use crate::orm::binary::{BinaryPageOutput, BinaryPageRequest};
use crate::runtime::shared_compute::ComputeError;
use std::num::NonZeroUsize;
use std::sync::Arc;

/// The binary field every vector collection declares for its embeddings: the one name the
/// writers, the filters, the stats and the matrix all use.
pub(super) const EMBEDDING_FIELD: &str = "embedding";

impl DataState {
    /// Builds the collection's resident matrix from native pages: each page's raw producer
    /// bytes, read from one pinned snapshot, land in one reused buffer and are appended to the
    /// matrix in one copy, never decoded into numbers on the way. Rows without a vector are not
    /// in the pages.
    pub(super) async fn vector_corpus(
        &self,
        handle: &str,
        collection: &str,
    ) -> Result<Arc<CpuMatrix>, VectorStoreError> {
        let scope = self.resolve_handle(handle).map_err(VectorStoreError::Store)?;
        self.vector_cache
            .get_or_try_compute(&scope, collection, async {
                let adapter = self.adapter_for_backend(handle, scope.clone()).await.map_err(VectorStoreError::Store)?;
                // One pinned snapshot for every page, so the matrix is one consistent revision.
                let snapshot = adapter.binary_snapshot(collection).await.map_err(VectorStoreError::Store)?;
                // The store's page size bounds one transfer, not the searchable corpus:
                // every page is visited before the matrix is sealed.
                let limit = NonZeroUsize::new(super::default_page_size()).ok_or(VectorStoreError::PageSize)?;
                let mut matrix = MatrixBuilder::default();
                let mut bytes = Vec::new();
                let mut ids = Vec::new();
                let mut after = None;
                loop {
                    bytes.clear();
                    ids.clear();
                    let request = BinaryPageRequest { field: EMBEDDING_FIELD, after, limit };
                    let descriptor = snapshot
                        .page_into(&request, BinaryPageOutput { bytes: &mut bytes, ids: &mut ids })
                        .await
                        .map_err(VectorStoreError::Store)?;
                    let Some(descriptor) = descriptor else { break };
                    matrix.append_page(&descriptor, &bytes, &ids)?;
                    match ids.last() {
                        Some(last) if ids.len() == limit.get() => after = Some(*last),
                        _ => break,
                    }
                }
                Ok::<_, VectorStoreError>(matrix.seal(RecallTarget::STORE))
            })
            .await
            .map_err(|error| match error {
                ComputeError::Initialize(error) => error,
                ComputeError::TypeMismatch { scope, key } => VectorStoreError::CacheType { scope, key },
            })
    }
}
