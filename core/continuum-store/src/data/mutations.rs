//! Public storage mutations and their vector-corpus invalidation boundary.
//! Acknowledged writes detach the backend corpus. Detached writes after caller
//! cancellation and direct adapter writes still require their own invalidation.
use super::{DataState, require_public_collection, require_record_fields};
use crate::orm::types::{BatchOperation, DataRecord, RecordMetadata, StorageResult, UUID};
use serde_json::{json, Value};

impl DataState {
    /// Phase 1 typed-IPC scaffold: takes already-deserialized `CreateParams`.
    /// Dispatch (handle_command) does the parse; handler body is pure logic.
    /// Follow this shape for QW#3's other hot-handler conversions.
    /// Create a record (single source for the typed `data/create` command).
    /// `handle` names the storage; the persona-facing command defaults it to
    /// "main". A missing `id` is minted. Publishes `<collection>:created` on
    /// success. The body is unchanged from the legacy `handle_create` arm.
    pub(crate) async fn create_record(
        &self,
        handle: &str,
        collection: String,
        id: Option<UUID>,
        data: Value,
    ) -> Result<StorageResult<DataRecord>, String> {
        require_public_collection(&collection)?;
        let start = std::time::Instant::now();

        require_record_fields(&data)?;
        let id = id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let record = DataRecord {
            id: id.clone(),
            collection: collection.clone(),
            data,
            metadata: RecordMetadata::default(),
        };

        let (scope, adapter) = self.adapter_with_identity(handle).await?;
        let result = adapter.create(record).await;
        self.log_slow_query("create", &collection, start.elapsed().as_millis());

        if result.success {
            self.vector_cache.invalidate(&scope, &collection);
            self.publish_event(
                &collection,
                "created",
                json!({ "id": id, "collection": collection }),
            );
        }

        Ok(result)
    }

    /// Update a record by id (single source for the typed `data/update` command).
    /// Publishes `<collection>:updated` on success.
    pub(crate) async fn update_record(
        &self,
        handle: &str,
        collection: String,
        id: UUID,
        data: Value,
        increment_version: bool,
    ) -> Result<StorageResult<DataRecord>, String> {
        require_public_collection(&collection)?;
        require_record_fields(&data)?;
        let (scope, adapter) = self.adapter_with_identity(handle).await?;
        let result = adapter
            .update(&collection, &id, data, increment_version)
            .await;

        if result.success {
            self.vector_cache.invalidate(&scope, &collection);
            self.publish_event(
                &collection,
                "updated",
                json!({ "id": id, "collection": collection }),
            );
        }

        Ok(result)
    }

    /// Delete a record by id (single source for the typed `data/delete` command).
    /// Publishes `<collection>:deleted` on success.
    pub(crate) async fn delete_record(
        &self,
        handle: &str,
        collection: String,
        id: UUID,
    ) -> Result<StorageResult<bool>, String> {
        require_public_collection(&collection)?;
        let (scope, adapter) = self.adapter_with_identity(handle).await?;
        let result = adapter.delete(&collection, &id).await;

        if result.success {
            self.vector_cache.invalidate(&scope, &collection);
            self.publish_event(
                &collection,
                "deleted",
                json!({ "id": id, "collection": collection }),
            );
        }

        Ok(result)
    }

    /// Apply a batch of create/update/delete operations atomically. Drives
    /// `data/batch`.
    pub(crate) async fn batch_operations(
        &self,
        handle: &str,
        operations: Vec<BatchOperation>,
    ) -> Result<StorageResult<Vec<Value>>, String> {
        for operation in &operations {
            require_public_collection(&operation.collection)?;
            if let Some(data) = &operation.data {
                require_record_fields(data)?;
            }
        }
        let op_count = operations.len();

        let (scope, adapter) = self.adapter_with_identity(handle).await?;
        let result = adapter.batch(operations).await;

        // Batch operations can touch several collections in this store.
        self.vector_cache.invalidate_scope(&scope);
        // Publish batch event on success
        if result.success {
            self.publish_event(
                "batch",
                "completed",
                json!({
                    "operationCount": op_count,
                    "successCount": result.data.as_ref().map(|d| d.len()).unwrap_or(0)
                }),
            );
        }

        Ok(result)
    }

    /// Delete every record in one collection (keeps the schema). Drives
    /// `data/truncate`.
    pub(crate) async fn truncate_collection(
        &self,
        handle: &str,
        collection: &str,
    ) -> Result<StorageResult<bool>, String> {
        require_public_collection(collection)?;
        let (scope, adapter) = self.adapter_with_identity(handle).await?;
        let result = adapter.truncate(collection).await;
        self.vector_cache.invalidate(&scope, collection);
        Ok(result)
    }

    /// Wipe every collection in a store. Drives `data/clear-all`.
    pub(crate) async fn clear_all_collections(
        &self,
        handle: &str,
    ) -> Result<StorageResult<crate::orm::adapter::ClearAllResult>, String> {
        let (scope, adapter) = self.adapter_with_identity(handle).await?;
        let result = adapter.clear_all().await;
        self.vector_cache.invalidate_scope(&scope);
        Ok(result)
    }

}
