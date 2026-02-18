//! Storage Adapter Trait - The database abstraction interface
//!
//! All storage backends implement this trait. The ORM layer works with
//! this trait, never with concrete implementations directly.
//!
//! Supported backends:
//! - SQLite (implemented)
//! - PostgreSQL (future)
//! - MySQL (future)
//! - Oracle (future)
//! - REST (future)
//! - GraphQL (future)
//! - JSON file (future)

use async_trait::async_trait;
use serde_json::Value;

use super::query::StorageQuery;
use super::types::{
    BatchOperation, CollectionSchema, CollectionStats, DataRecord, StorageResult, UUID,
};

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
#[derive(Debug, Clone, Default)]
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
    /// Get adapter name (e.g., "sqlite", "postgres")
    fn name(&self) -> &'static str;

    /// Get adapter capabilities
    fn capabilities(&self) -> AdapterCapabilities;

    /// Initialize the adapter with configuration
    async fn initialize(&mut self, config: AdapterConfig) -> Result<(), String>;

    /// Close the adapter connection
    async fn close(&mut self) -> Result<(), String>;

    // ─── CRUD Operations ─────────────────────────────────────────────────────

    /// Create a new record
    async fn create(&self, record: DataRecord) -> StorageResult<DataRecord>;

    /// Read a record by ID
    async fn read(&self, collection: &str, id: &UUID) -> StorageResult<DataRecord>;

    /// Query records with filters
    async fn query(&self, query: StorageQuery) -> StorageResult<Vec<DataRecord>>;

    /// Query with JOINs for optimal loading of related data
    /// Returns records with joined data nested under alias keys
    async fn query_with_join(&self, query: StorageQuery) -> StorageResult<Vec<DataRecord>>;

    /// Count records matching query (uses SQL COUNT, not fetch all)
    async fn count(&self, query: StorageQuery) -> StorageResult<usize>;

    /// Update a record
    async fn update(
        &self,
        collection: &str,
        id: &UUID,
        data: Value,
        increment_version: bool,
    ) -> StorageResult<DataRecord>;

    /// Delete a record
    async fn delete(&self, collection: &str, id: &UUID) -> StorageResult<bool>;

    // ─── Batch Operations ────────────────────────────────────────────────────

    /// Execute batch operations
    async fn batch(&self, operations: Vec<BatchOperation>) -> StorageResult<Vec<Value>>;

    // ─── Schema Operations ───────────────────────────────────────────────────

    /// Ensure collection schema exists
    async fn ensure_schema(&self, schema: CollectionSchema) -> StorageResult<bool>;

    /// List all collections
    async fn list_collections(&self) -> StorageResult<Vec<String>>;

    /// Get collection statistics
    async fn collection_stats(&self, collection: &str) -> StorageResult<CollectionStats>;

    // ─── Maintenance Operations ──────────────────────────────────────────────

    /// Truncate a collection (delete all records)
    async fn truncate(&self, collection: &str) -> StorageResult<bool>;

    /// Clear all collections
    async fn clear_all(&self) -> StorageResult<ClearAllResult>;

    /// Run cleanup/optimization (e.g., VACUUM for SQLite)
    async fn cleanup(&self) -> Result<(), String>;
}

/// Result of clear_all operation
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClearAllResult {
    pub tables_cleared: Vec<String>,
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
