use async_trait::async_trait;
use serde_json::Value;
use std::error::Error;

/// Universal Storage Adapter Trait
/// Mirrors TypeScript DataStorageAdapter abstract class
#[async_trait]
pub trait StorageAdapter: Send + Sync {
    /// Initialize storage backend with configuration
    async fn initialize(&mut self, config: Value) -> Result<(), Box<dyn Error>>;

    /// Create a record
    async fn create(&self, collection: &str, record: Value) -> Result<Value, Box<dyn Error>>;

    /// Read a record by ID
    async fn read(&self, collection: &str, id: &str) -> Result<Option<Value>, Box<dyn Error>>;

    /// Query records with filters
    async fn query(&self, query: Value) -> Result<Vec<Value>, Box<dyn Error>>;

    /// Update a record
    async fn update(&self, collection: &str, id: &str, data: Value) -> Result<Value, Box<dyn Error>>;

    /// Delete a record
    async fn delete(&self, collection: &str, id: &str) -> Result<bool, Box<dyn Error>>;

    /// Ensure schema exists
    async fn ensure_schema(&self, collection: &str, schema: Option<Value>) -> Result<bool, Box<dyn Error>>;

    /// List collections
    async fn list_collections(&self) -> Result<Vec<String>, Box<dyn Error>>;

    /// Get collection stats
    async fn get_collection_stats(&self, collection: &str) -> Result<Value, Box<dyn Error>>;

    /// Cleanup and close
    async fn close(&mut self) -> Result<(), Box<dyn Error>>;
}
