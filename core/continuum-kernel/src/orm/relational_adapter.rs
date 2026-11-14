//! StorageAdapter boundary for the schema-driven SeaORM provider.
use super::storage_error::StorageError;
use super::{
    adapter::{AdapterCapabilities, AdapterConfig, ClearAllResult, StorageAdapter},
    query::{QueryBuilder, StorageQuery},
    relational::RelationalStore,
    seam::{BinaryWrite, BinaryWritten, Deleted, FieldRef, RowRef, Update, Written},
    types::{BatchOperation, BatchOutcome, CollectionSchema, CollectionStats, DataRecord, UUID},
};
use async_trait::async_trait;

pub struct SeaOrmAdapter {
    store: RelationalStore,
}

impl SeaOrmAdapter {
    /// Open the file owned by a typed ORM store.
    pub async fn open(path: &std::path::Path) -> Result<Self, StorageError> {
        let connection_string = path
            .to_str()
            .ok_or(StorageError::Config(super::storage_error::ConfigFault::PathText))?
            .to_owned();
        Self::connect(AdapterConfig {
            connection_string,
            ..AdapterConfig::default()
        })
        .await
    }

    pub async fn connect(config: AdapterConfig) -> Result<Self, StorageError> {
        Ok(Self {
            store: RelationalStore::open(&config).await?,
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
    async fn close(&mut self) -> Result<(), StorageError> {
        let _guard = self.store.writer_gate.lock().await;
        Ok(self.store.db.clone().close().await?)
    }
    async fn create(&self, record: DataRecord) -> Result<DataRecord, StorageError> {
        self.store.create(record).await
    }
    async fn read(&self, row: RowRef<'_>) -> Result<DataRecord, StorageError> {
        self.store
            .read_on(&self.store.db, row.collection, row.id)
            .await?
            .ok_or_else(|| StorageError::NotFound { collection: row.collection.to_owned().into(), id: row.id.clone() })
    }
    async fn query(&self, query: &StorageQuery) -> Result<Vec<DataRecord>, StorageError> {
        self.store.query(query).await
    }
    async fn scan(
        &self,
        query: &StorageQuery,
    ) -> Result<futures::stream::BoxStream<'_, Result<DataRecord, StorageError>>, StorageError> {
        use futures::StreamExt;
        use sea_orm::StreamTrait;
        let schema = self.store.schema(&query.collection)?;
        let decoder = super::relational_codec::RowDecoder::new(std::sync::Arc::clone(&schema), &[]);
        let statement =
            super::relational_query::select(&schema, query, super::relational_query::Selection::Stream)?;
        let rows = self.store.db.stream(sea_orm::DbBackend::Sqlite.build(&statement)).await?;
        Ok(rows.map(move |row| row.map_err(StorageError::from).and_then(|row| decoder.decode(row))).boxed())
    }
    async fn query_with_join(&self, query: &StorageQuery) -> Result<Vec<DataRecord>, StorageError> {
        self.store.query_joined(query).await
    }
    async fn count(&self, query: &StorageQuery) -> Result<usize, StorageError> {
        self.store.count(query).await
    }
    async fn update(&self, update: Update<'_>) -> Result<DataRecord, StorageError> {
        let row = super::types::RowKey { collection: update.row.collection.to_owned(), id: update.row.id.to_owned() };
        self.store.update(row, update.data, update.versioning).await
    }
    async fn delete(&self, row: RowRef<'_>) -> Result<Deleted, StorageError> {
        let removed = self
            .store
            .delete_matching(QueryBuilder::new(row.collection).filter_eq("id", row.id.as_str()).build())
            .await?;
        Ok(if removed == 0 { Deleted::Absent } else { Deleted::Removed })
    }
    async fn upsert_monotonic(&self, record: DataRecord, field: &str) -> Result<Written, StorageError> {
        let written = self.store.upsert_monotonic(record, field.to_owned()).await?;
        Ok(if written { Written::Written } else { Written::Kept })
    }
    async fn delete_matching(&self, query: StorageQuery) -> Result<usize, StorageError> {
        self.store.delete_matching(query).await
    }
    async fn batch(&self, operations: Vec<BatchOperation>) -> Result<Vec<BatchOutcome>, StorageError> {
        self.store.batch(operations).await
    }
    fn sequence_revision(&self, collection: &str) -> tokio::sync::watch::Receiver<u64> {
        self.store.revision(super::relational_sequence::Domain::Append, collection)
    }
    fn native_revision(&self, collection: &str) -> tokio::sync::watch::Receiver<u64> {
        self.store.revision(super::relational_sequence::Domain::Native, collection)
    }
    async fn ensure_schema(&self, schema: &CollectionSchema) -> Result<(), StorageError> {
        self.store.ensure(schema).await
    }
    fn collection_schema(&self, collection: &str) -> Result<std::sync::Arc<CollectionSchema>, StorageError> {
        self.store.schema(collection)
    }
    async fn list_collections(&self) -> Result<Vec<String>, StorageError> {
        Ok(self.store.collections())
    }
    async fn binary_snapshot(&self, collection: &str) -> Result<super::BinarySnapshot, StorageError> {
        self.store.binary_snapshot(collection).await
    }
    async fn create_binary(
        &self,
        record: DataRecord,
        payloads: std::collections::BTreeMap<String, super::binary::BinaryValue>,
    ) -> Result<UUID, StorageError> {
        self.store.create_binary(record, payloads).await
    }
    async fn update_binary(&self, write: BinaryWrite<'_>) -> Result<BinaryWritten, StorageError> {
        let written = self
            .store
            .update_binary(write)
            .await?;
        Ok(if written { BinaryWritten::Written } else { BinaryWritten::Unchanged })
    }
    async fn numeric_array_stats(&self, field: FieldRef<'_>) -> Result<super::adapter::NumericArrayStats, StorageError> {
        self.store.numeric_array_stats(field.collection, field.field).await
    }
    async fn collection_stats(&self, collection: &str) -> Result<CollectionStats, StorageError> {
        self.store.stats(collection).await
    }
    async fn truncate(&self, collection: &str) -> Result<usize, StorageError> {
        self.store.delete_matching(QueryBuilder::new(collection).build()).await
    }
    async fn clear_all(&self) -> Result<ClearAllResult, StorageError> {
        self.store.clear().await
    }
    async fn cleanup(&self) -> Result<(), StorageError> {
        self.store.optimize().await
    }
}
