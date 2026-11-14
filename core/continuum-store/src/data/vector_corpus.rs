//! The resident vector corpus per collection; cache cell detachment owns invalidation.
use super::vector_index::{EmbeddingDescriptor, MatrixBuilder, RecallTarget, VectorCorpus, VectorStoreError};
use super::DataState;
use crate::orm::binary::{BinaryPageOutput, BinaryPageRequest};
use crate::orm::BinarySnapshot;
use crate::runtime::shared_compute::ComputeError;
use std::num::NonZeroUsize;
use std::sync::Arc;

/// The binary field every vector collection declares for its embeddings: the one name the
/// writers, the filters, the stats and the matrix all use.
pub(super) const EMBEDDING_FIELD: &str = "embedding";

impl DataState {
    /// Builds the collection's corpus: one matrix per embedding descriptor. The pages are read
    /// from one pinned snapshot into the builders' own buffers, the snapshot is released, and
    /// only then are the matrices sealed (calibrated) off the async runtime.
    pub(super) async fn vector_corpus(
        &self,
        handle: &str,
        collection: &str,
    ) -> Result<Arc<VectorCorpus>, VectorStoreError> {
        let scope = self.resolve_handle(handle).map_err(VectorStoreError::Store)?;
        self.vector_cache
            .get_or_try_compute(&scope, collection, async {
                let adapter = self.adapter_for_backend(handle, scope.clone()).await.map_err(VectorStoreError::Store)?;
                let snapshot = adapter.binary_snapshot(collection).await.map_err(VectorStoreError::Store)?;
                // The store's page size bounds one transfer, not the searchable corpus.
                let limit = NonZeroUsize::new(super::default_page_size()).ok_or(VectorStoreError::PageSize)?;
                let groups = read_groups(snapshot, limit).await?;
                let matrices = tokio::task::spawn_blocking(move || {
                    groups
                        .into_iter()
                        .map(|(descriptor, builder)| (descriptor, builder.seal(RecallTarget::STORE)))
                        .collect::<Vec<_>>()
                })
                .await?;
                Ok::<_, VectorStoreError>(VectorCorpus::new(matrices))
            })
            .await
            .map_err(|error| match error {
                ComputeError::Initialize(error) => error,
                ComputeError::TypeMismatch { scope, key } => VectorStoreError::CacheType { scope, key },
            })
    }
}

/// Copies every page of every embedding descriptor out of `snapshot` into one builder per
/// descriptor, then releases the snapshot (and the database connection its read transaction
/// leases) before returning: no seal or calibration runs while a connection is held. The
/// raw producer bytes are appended in one copy per page, never decoded into numbers.
pub(super) async fn read_groups(
    snapshot: BinarySnapshot,
    limit: NonZeroUsize,
) -> Result<Vec<(EmbeddingDescriptor, MatrixBuilder)>, VectorStoreError> {
    let stored = snapshot.descriptors(EMBEDDING_FIELD, limit).await.map_err(VectorStoreError::Store)?;
    if stored.len() == limit.get() {
        return Err(VectorStoreError::DescriptorGroups { limit: limit.get() });
    }
    let mut groups = Vec::with_capacity(stored.len());
    let mut bytes = Vec::new();
    let mut ids = Vec::new();
    for binary in stored {
        let descriptor = EmbeddingDescriptor::from_binary(&binary)?;
        let mut builder = MatrixBuilder::default();
        let mut after = None;
        loop {
            bytes.clear();
            ids.clear();
            let request = BinaryPageRequest { field: EMBEDDING_FIELD, after, limit };
            let page = snapshot
                .page_matching_into(&request, &binary, BinaryPageOutput { bytes: &mut bytes, ids: &mut ids })
                .await
                .map_err(VectorStoreError::Store)?;
            let Some(page) = page else { break };
            builder.append_page(&page, &bytes, &ids)?;
            match ids.last() {
                Some(last) if ids.len() == limit.get() => after = Some(*last),
                _ => break,
            }
        }
        groups.push((descriptor, builder));
    }
    // The read transaction ends here, before any caller seals what it read.
    drop(snapshot);
    Ok(groups)
}
