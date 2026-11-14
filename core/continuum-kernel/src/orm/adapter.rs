//! Storage Adapter Trait - The database abstraction interface
//!
//! All storage backends implement this trait. The ORM layer works with
//! this trait, never with concrete implementations directly.
//!
//! Supported backends:
//! - SQLite (implemented)
//! - MySQL (future)
//! - Oracle (future)
//! - REST (future)
//! - GraphQL (future)
//! - JSON file (future)

use async_trait::async_trait;
use serde_json::Value;

use super::query::StorageQuery;
use super::seam::{BinaryWrite, BinaryWritten, Deleted, FieldRef, RowRef, Update, Written};
use super::storage_error::StorageError;
use super::types::{
    BatchOperation, BatchOutcome, CollectionSchema, CollectionStats, DataRecord, UUID,
};

/// Database-side summary of a declared binary vector field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NumericArrayStats {
    pub total_records: usize,
    pub non_empty_records: usize,
    pub min_length: usize,
    pub max_length: usize,
}

/// Storage adapter configuration
#[derive(Debug, Clone)]
pub struct AdapterConfig {
    /// Connection string (database URL, file path, etc.)
    pub connection_string: String,
    /// Optional namespace for multi-tenant isolation
    pub namespace: Option<String>,
    /// Connection timeout in milliseconds
    pub timeout_ms: u64,
    /// Maximum connections in pool
    pub max_connections: usize,
}

impl Default for AdapterConfig {
    fn default() -> Self {
        Self {
            connection_string: String::new(),
            namespace: None,
            timeout_ms: 30_000,
            max_connections: 10,
        }
    }
}

/// Storage adapter capabilities
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/AdapterCapabilities.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct AdapterCapabilities {
    pub supports_transactions: bool,
    pub supports_indexing: bool,
    pub supports_full_text_search: bool,
    pub supports_vector_search: bool,
    pub supports_joins: bool,
    pub supports_batch: bool,
    pub max_record_size: usize,
}

/// The universal storage adapter trait
///
/// All database backends implement this trait. The ORM module calls
/// these methods; adapters translate to native database operations.
#[async_trait]
pub trait StorageAdapter: Send + Sync {
    /// The backend's name (e.g. "sqlite").
    fn name(&self) -> &'static str;
    /// What this backend provides.
    fn capabilities(&self) -> AdapterCapabilities;
    /// Close the connection: the one call that changes the adapter itself.
    async fn close(&mut self) -> Result<(), StorageError>;

    // ─── Rows ────────────────────────────────────────────────────────────────
    /// Create `record`; returns the stored row.
    async fn create(&self, record: DataRecord) -> Result<DataRecord, StorageError>;
    /// The row `row` names; [`StorageError::NotFound`] when it does not exist.
    async fn read(&self, row: RowRef<'_>) -> Result<DataRecord, StorageError>;
    /// The rows `query` selects.
    async fn query(&self, query: &StorageQuery) -> Result<Vec<DataRecord>, StorageError>;
    /// Stream the rows `query` selects without materializing them. Dropping the stream
    /// releases its database read and connection.
    async fn scan(
        &self,
        query: &StorageQuery,
    ) -> Result<futures::stream::BoxStream<'_, Result<DataRecord, StorageError>>, StorageError>;
    /// The rows `query` selects with their joins nested under alias keys.
    async fn query_with_join(&self, query: &StorageQuery) -> Result<Vec<DataRecord>, StorageError>;
    /// How many rows `query` selects (SQL COUNT, nothing fetched).
    async fn count(&self, query: &StorageQuery) -> Result<usize, StorageError>;
    /// Apply `update` to its row; returns the stored row.
    async fn update(&self, update: Update<'_>) -> Result<DataRecord, StorageError>;
    /// Delete one row.
    async fn delete(&self, row: RowRef<'_>) -> Result<Deleted, StorageError>;
    /// Write `record` unless its row already holds a GREATER `field`, as ONE atomic operation:
    /// create the row if absent, overwrite it when the stored `field` <= the record's, keep it
    /// otherwise. The compare runs inside the write, so no other writer can slip between a read
    /// and the write it justified. The primitive for a monotonic high-water mark.
    async fn upsert_monotonic(&self, record: DataRecord, field: &str) -> Result<Written, StorageError>;
    /// Delete every row `query`'s filter selects, as ONE statement; returns the count. The filter
    /// is judged when the delete runs, never by a stale selection.
    async fn delete_matching(&self, query: StorageQuery) -> Result<usize, StorageError>;

    // ─── Batches ─────────────────────────────────────────────────────────────
    /// Execute `operations` ATOMICALLY: all commit, or none do. The first failing operation
    /// aborts the batch, rolls back every earlier one, and is the error; the returned values
    /// describe operations that all committed.
    /// The operations are owned: the committing task outlives this call and moves each record in.
    async fn batch(&self, operations: Vec<BatchOperation>) -> Result<Vec<BatchOutcome>, StorageError>;
    /// The committed high-water of a collection written by
    /// [`super::types::BatchOperation::Append`], published by the committing write itself.
    /// A hint to drain from a cursor, never the data.
    fn sequence_revision(&self, collection: &str) -> tokio::sync::watch::Receiver<u64>;
    /// Native commits, in their own domain of the shared revision owner.
    fn native_revision(&self, collection: &str) -> tokio::sync::watch::Receiver<u64>;

    // ─── Schemas ─────────────────────────────────────────────────────────────
    /// Admit `schema`: create or evolve its table to the declaration.
    async fn ensure_schema(&self, schema: &CollectionSchema) -> Result<(), StorageError>;
    /// The admitted declaration, shared; never inferred from row values.
    fn collection_schema(&self, collection: &str) -> Result<std::sync::Arc<CollectionSchema>, StorageError>;
    /// Every admitted collection.
    async fn list_collections(&self) -> Result<Vec<String>, StorageError>;

    // ─── Native payloads ─────────────────────────────────────────────────────
    /// Bounded native payload reads; scalar projections use the normal query owner.
    async fn binary_snapshot(&self, collection: &str) -> Result<super::BinarySnapshot, StorageError>;
    /// Create `record` with its native payloads; returns the new row's id.
    async fn create_binary(
        &self,
        record: DataRecord,
        payloads: std::collections::BTreeMap<String, super::binary::BinaryValue>,
    ) -> Result<UUID, StorageError>;
    /// Write native payloads to one row under its condition.
    async fn update_binary(&self, write: BinaryWrite<'_>) -> Result<BinaryWritten, StorageError>;
    /// Counts and lengths of one binary field, from descriptors only (no payload is read).
    async fn numeric_array_stats(&self, field: FieldRef<'_>) -> Result<NumericArrayStats, StorageError>;
    async fn collection_stats(&self, collection: &str) -> Result<CollectionStats, StorageError>;

    // ─── Maintenance ─────────────────────────────────────────────────────────
    /// Delete every row of a collection; returns how many.
    async fn truncate(&self, collection: &str) -> Result<usize, StorageError>;
    /// Clear every collection.
    async fn clear_all(&self) -> Result<ClearAllResult, StorageError>;
    /// Reclaim space (e.g. VACUUM for SQLite).
    async fn cleanup(&self) -> Result<(), StorageError>;
}

/// Result of clear_all operation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/ClearAllResult.ts"
)]
pub struct ClearAllResult {
    pub tables_cleared: Vec<String>,
    #[ts(type = "number")]
    pub records_deleted: usize,
}

/// Naming converter utilities for adapters
pub mod naming {
    /// Convert camelCase to snake_case
    pub fn to_snake_case(s: &str) -> String {
        let mut result = String::with_capacity(s.len() + 4);
        for (i, c) in s.chars().enumerate() {
            if c.is_uppercase() {
                if i > 0 {
                    result.push('_');
                }
                result.push(c.to_ascii_lowercase());
            } else {
                result.push(c);
            }
        }
        result
    }

    /// Convert snake_case to camelCase
    pub fn to_camel_case(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        let mut capitalize_next = false;
        for c in s.chars() {
            if c == '_' {
                capitalize_next = true;
            } else if capitalize_next {
                result.push(c.to_ascii_uppercase());
                capitalize_next = false;
            } else {
                result.push(c);
            }
        }
        result
    }

    /// Convert collection name to table name (camelCase to snake_case)
    pub fn to_table_name(collection: &str) -> String {
        to_snake_case(collection)
    }

    /// Convert table name to collection name (snake_case to camelCase)
    pub fn to_collection_name(table: &str) -> String {
        to_camel_case(table)
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_to_snake_case() {
            assert_eq!(to_snake_case("chatMessages"), "chat_messages");
            assert_eq!(to_snake_case("userId"), "user_id");
            assert_eq!(to_snake_case("ID"), "i_d"); // Edge case
            assert_eq!(to_snake_case("already_snake"), "already_snake");
        }

        #[test]
        fn test_to_camel_case() {
            assert_eq!(to_camel_case("chat_messages"), "chatMessages");
            assert_eq!(to_camel_case("user_id"), "userId");
            assert_eq!(to_camel_case("alreadyCamel"), "alreadyCamel");
        }
    }
}
