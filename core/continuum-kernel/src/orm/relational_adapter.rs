//! StorageAdapter boundary for the schema-driven SeaORM provider.
use super::{
    adapter::{AdapterCapabilities, AdapterConfig, ClearAllResult, StorageAdapter},
    query::{QueryBuilder, StorageQuery},
    relational::RelationalStore,
    types::{BatchOperation, CollectionSchema, CollectionStats, DataRecord, StorageResult, UUID},
};
use async_trait::async_trait;
use serde_json::Value;

pub(super) struct SeaOrmAdapter {
    store: RelationalStore,
}

fn receipt<T>(result: Result<T, String>) -> StorageResult<T> {
    match result {
        Ok(value) => StorageResult::ok(value),
        Err(error) => StorageResult::err(error),
    }
}

impl SeaOrmAdapter {
    pub async fn open(config: AdapterConfig) -> Result<Self, String> {
        Ok(Self {
            store: RelationalStore::open(config).await?,
        })
    }
}

#[async_trait]
impl StorageAdapter for SeaOrmAdapter {
    fn name(&self) -> &'static str {
        "sqlite"
    }
    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            supports_transactions: true,
            supports_indexing: true,
            supports_full_text_search: false,
            supports_vector_search: false,
            supports_joins: true,
            supports_batch: true,
            max_record_size: 1_000_000_000,
        }
    }
    async fn initialize(&mut self, _config: AdapterConfig) -> Result<(), String> {
        Err("SeaORM adapters are initialized by open; close and open a new owner to change databases".into())
    }
    async fn close(&mut self) -> Result<(), String> {
        let _guard = self.store.writer_gate.lock().await;
        self.store
            .db
            .clone()
            .close()
            .await
            .map_err(|e| e.to_string())
    }
    async fn create(&self, record: DataRecord) -> StorageResult<DataRecord> {
        receipt(self.store.create(record).await)
    }
    async fn read(&self, collection: &str, id: &UUID) -> StorageResult<DataRecord> {
        receipt(
            self.store
                .read_on(&self.store.db, collection, id)
                .await
                .and_then(|row| row.ok_or_else(|| format!("Record not found: {id}"))),
        )
    }
    async fn query(&self, query: StorageQuery) -> StorageResult<Vec<DataRecord>> {
        receipt(self.store.query(query).await)
    }
    async fn query_with_join(&self, query: StorageQuery) -> StorageResult<Vec<DataRecord>> {
        receipt(self.store.query_joined(query).await)
    }
    async fn count(&self, query: StorageQuery) -> StorageResult<usize> {
        receipt(self.store.count(query).await)
    }
    async fn update(
        &self,
        collection: &str,
        id: &UUID,
        data: Value,
        increment: bool,
    ) -> StorageResult<DataRecord> {
        receipt(
            self.store
                .update(collection.into(), id.clone(), data, increment)
                .await,
        )
    }
    async fn delete(&self, collection: &str, id: &UUID) -> StorageResult<bool> {
        receipt(
            self.store
                .delete_matching(
                    QueryBuilder::new(collection)
                        .filter_eq("id", id.clone())
                        .build(),
                )
                .await
                .map(|n| n != 0),
        )
    }
    async fn upsert_monotonic(&self, record: DataRecord, field: &str) -> StorageResult<bool> {
        receipt(self.store.upsert_monotonic(record, field.into()).await)
    }
    async fn delete_matching(&self, query: StorageQuery) -> StorageResult<usize> {
        receipt(self.store.delete_matching(query).await)
    }
    async fn batch(&self, operations: Vec<BatchOperation>) -> StorageResult<Vec<Value>> {
        receipt(self.store.batch(operations).await)
    }
    fn sequence_revision(&self, collection: &str) -> tokio::sync::watch::Receiver<u64> {
        self.store.revision(collection)
    }
    async fn ensure_schema(&self, schema: CollectionSchema) -> StorageResult<bool> {
        receipt(self.store.ensure(schema).await.map(|()| true))
    }
    async fn list_collections(&self) -> StorageResult<Vec<String>> {
        StorageResult::ok(self.store.collections())
    }
    async fn collection_stats(&self, collection: &str) -> StorageResult<CollectionStats> {
        receipt(self.store.stats(collection).await)
    }
    async fn truncate(&self, collection: &str) -> StorageResult<bool> {
        receipt(
            self.store
                .delete_matching(QueryBuilder::new(collection).build())
                .await
                .map(|_| true),
        )
    }
    async fn clear_all(&self) -> StorageResult<ClearAllResult> {
        receipt(self.store.clear().await)
    }
    async fn cleanup(&self) -> Result<(), String> {
        self.store.optimize().await
    }
}
