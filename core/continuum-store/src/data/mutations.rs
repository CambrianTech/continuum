//! Public storage mutations and their vector-corpus invalidation boundary.
//! Acknowledged writes detach the backend corpus. Detached writes after caller
//! cancellation and direct adapter writes still require their own invalidation.
use super::{DataState, require_public_collection, require_record_fields};
use crate::orm::seam::{Deleted, RowRef, Update, Versioning};
use crate::orm::types::{BatchOperation, BatchOutcome, DataRecord, RecordMetadata, UUID};
use crate::sdk_codegen::CommandError;
use serde_json::{json, Value};

impl DataState {
    /// Create a record (the one source behind `data/create`). `handle` names the storage; a
    /// missing `id` is minted (UUIDv7, time-ordered). Publishes `<collection>:created`.
    pub(crate) async fn create_record(
        &self,
        handle: &str,
        collection: String,
        id: Option<UUID>,
        data: Value,
    ) -> Result<DataRecord, CommandError> {
        require_public_collection(&collection).map_err(CommandError::Invalid)?;
        let start = std::time::Instant::now();
        require_record_fields(&data).map_err(CommandError::Invalid)?;
        let id = id.unwrap_or_else(|| uuid::Uuid::now_v7().to_string()); // unwrap_or_else: an id the caller did not choose is minted here
        let record = DataRecord { id: id.clone(), collection: collection.clone(), data, metadata: RecordMetadata::default() };
        let (scope, adapter) = self.adapter_with_identity(handle).await?;
        let created = adapter.create(record).await;
        self.log_slow_query("create", &collection, start.elapsed().as_millis());
        let created = created?;
        self.vector_cache.invalidate(&scope, &collection);
        self.publish_event(&collection, "created", json!({ "id": id, "collection": collection }));
        Ok(created)
    }

    /// Update a record by id (the one source behind `data/update`). Publishes `<collection>:updated`.
    pub(crate) async fn update_record(
        &self,
        handle: &str,
        collection: String,
        id: UUID,
        data: Value,
        versioning: Versioning,
    ) -> Result<DataRecord, CommandError> {
        require_public_collection(&collection).map_err(CommandError::Invalid)?;
        require_record_fields(&data).map_err(CommandError::Invalid)?;
        let (scope, adapter) = self.adapter_with_identity(handle).await?;
        let updated = adapter.update(Update { row: RowRef { collection: &collection, id: &id }, data, versioning }).await?;
        self.vector_cache.invalidate(&scope, &collection);
        self.publish_event(&collection, "updated", json!({ "id": id, "collection": collection }));
        Ok(updated)
    }

    /// Delete a record by id (the one source behind `data/delete`). Publishes `<collection>:deleted`
    /// when a row was removed.
    pub(crate) async fn delete_record(&self, handle: &str, collection: &str, id: UUID) -> Result<Deleted, CommandError> {
        require_public_collection(collection).map_err(CommandError::Invalid)?;
        let (scope, adapter) = self.adapter_with_identity(handle).await?;
        let deleted = adapter.delete(RowRef { collection, id: &id }).await?;
        if deleted == Deleted::Removed {
            self.vector_cache.invalidate(&scope, collection);
            self.publish_event(collection, "deleted", json!({ "id": id, "collection": collection }));
        }
        Ok(deleted)
    }

    /// Apply a batch atomically (the one source behind `data/batch`).
    pub(crate) async fn batch_operations(
        &self,
        handle: &str,
        operations: Vec<BatchOperation>,
    ) -> Result<Vec<BatchOutcome>, CommandError> {
        for operation in &operations {
            let (collection, data) = match operation {
                BatchOperation::Read(row) | BatchOperation::Delete(row) => (&row.collection, None),
                BatchOperation::Create(row) | BatchOperation::Update(row) | BatchOperation::Ensure(row) => {
                    (&row.collection, Some(&row.data))
                }
                BatchOperation::Append { row, .. } => (&row.collection, Some(&row.data)),
            };
            require_public_collection(collection).map_err(CommandError::Invalid)?;
            if let Some(data) = data {
                require_record_fields(data).map_err(CommandError::Invalid)?;
            }
        }
        let (scope, adapter) = self.adapter_with_identity(handle).await?;
        let outcomes = adapter.batch(operations).await?;
        // A batch can touch several collections in this store.
        self.vector_cache.invalidate_scope(&scope);
        self.publish_event("batch", "completed", json!({ "operationCount": outcomes.len() }));
        Ok(outcomes)
    }

    /// Delete every record in one collection, keeping its schema (`data/truncate`); returns how many.
    pub(crate) async fn truncate_collection(&self, handle: &str, collection: &str) -> Result<usize, CommandError> {
        require_public_collection(collection).map_err(CommandError::Invalid)?;
        let (scope, adapter) = self.adapter_with_identity(handle).await?;
        let removed = adapter.truncate(collection).await?;
        self.vector_cache.invalidate(&scope, collection);
        Ok(removed)
    }

    /// Wipe every collection in a store (`data/clear-all`).
    pub(crate) async fn clear_all_collections(
        &self,
        handle: &str,
    ) -> Result<crate::orm::adapter::ClearAllResult, CommandError> {
        let (scope, adapter) = self.adapter_with_identity(handle).await?;
        let cleared = adapter.clear_all().await?;
        self.vector_cache.invalidate_scope(&scope);
        Ok(cleared)
    }
}
