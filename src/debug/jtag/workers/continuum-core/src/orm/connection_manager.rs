//! Connection Manager - Pool-per-database connection management
//!
//! Manages SQLite connections across 40+ databases (13 personas × 2-3 DBs each + shared DBs).
//! Key design principles:
//! - Pool per database file (SQLite locks are per-file)
//! - Lazy pool creation (don't open all DBs at startup)
//! - LRU eviction when at capacity
//! - Small pools (2-3 connections) - SQLite WAL constraints
//!
//! Architecture:
//! ```text
//! TypeScript
//!     ↓ IPC (dbPath required in every request)
//! ConnectionManager
//!     ├── pools: DashMap<PathBuf, ManagedPool>
//!     │   ├── persona1/memory.db -> Pool(2 conns)
//!     │   ├── persona2/memory.db -> Pool(2 conns)
//!     │   ├── main.db -> Pool(3 conns)
//!     │   └── ...
//!     ↓
//! SqliteAdapter (per pool)
//! ```

use dashmap::DashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

use super::adapter::{AdapterConfig, StorageAdapter};
use super::query::StorageQuery;
use super::sqlite::SqliteAdapter;
use super::types::{BatchOperation, CollectionSchema, DataRecord, StorageResult, UUID};
use serde_json::Value;

/// Configuration for the connection manager
#[derive(Debug, Clone)]
pub struct ConnectionManagerConfig {
    /// Maximum number of pools before LRU eviction (default: 50)
    pub max_pools: usize,
    /// Connections per pool (default: 2 for SQLite WAL)
    pub connections_per_pool: usize,
    /// Idle timeout before pool closure (default: 5 minutes)
    pub idle_timeout: Duration,
    /// Connection timeout (default: 30 seconds)
    pub connection_timeout: Duration,
}

impl Default for ConnectionManagerConfig {
    fn default() -> Self {
        Self {
            max_pools: 50,
            connections_per_pool: 2,
            idle_timeout: Duration::from_secs(300), // 5 minutes
            connection_timeout: Duration::from_secs(30),
        }
    }
}

/// Managed pool with metadata for LRU eviction
struct ManagedPool {
    /// The underlying adapter
    adapter: Arc<RwLock<SqliteAdapter>>,
    /// Last access time for LRU tracking
    last_access: AtomicU64,
    /// Database path (stored for debugging/logging)
    #[allow(dead_code)]
    path: PathBuf,
}

impl ManagedPool {
    fn new(adapter: SqliteAdapter, path: PathBuf) -> Self {
        Self {
            adapter: Arc::new(RwLock::new(adapter)),
            last_access: AtomicU64::new(Self::now_millis()),
            path,
        }
    }

    fn touch(&self) {
        self.last_access.store(Self::now_millis(), Ordering::Relaxed);
    }

    fn last_access_millis(&self) -> u64 {
        self.last_access.load(Ordering::Relaxed)
    }

    fn now_millis() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }
}

/// Connection manager - single entry point for all database operations
///
/// Provides pool-per-database connection management with:
/// - Lazy pool creation
/// - LRU eviction when at capacity
/// - Per-request database path (NO fallbacks)
pub struct ConnectionManager {
    /// Pool per database file
    pools: DashMap<PathBuf, Arc<ManagedPool>>,
    /// Configuration
    config: ConnectionManagerConfig,
}

impl ConnectionManager {
    /// Create a new connection manager with default config
    pub fn new() -> Self {
        Self::with_config(ConnectionManagerConfig::default())
    }

    /// Create a connection manager with custom config
    pub fn with_config(config: ConnectionManagerConfig) -> Self {
        Self {
            pools: DashMap::new(),
            config,
        }
    }

    /// Get or create a pool for the given database path
    ///
    /// CRITICAL: db_path MUST be provided - no fallbacks allowed
    async fn get_or_create_pool(&self, db_path: &Path) -> Result<Arc<ManagedPool>, String> {
        // Fast path: pool exists
        if let Some(pool) = self.pools.get(db_path) {
            pool.touch();
            return Ok(pool.clone());
        }

        // Slow path: need to create pool
        // First, check if we need to evict
        if self.pools.len() >= self.config.max_pools {
            self.evict_lru().await?;
        }

        // Create new adapter
        let mut adapter = SqliteAdapter::new();
        adapter
            .initialize(AdapterConfig {
                connection_string: db_path.to_string_lossy().to_string(),
                namespace: None,
                timeout_ms: self.config.connection_timeout.as_millis() as u64,
                max_connections: self.config.connections_per_pool,
            })
            .await?;

        let managed = Arc::new(ManagedPool::new(adapter, db_path.to_path_buf()));
        self.pools.insert(db_path.to_path_buf(), managed.clone());

        Ok(managed)
    }

    /// Evict the least recently used pool
    async fn evict_lru(&self) -> Result<(), String> {
        // Find the LRU pool
        let mut oldest: Option<(PathBuf, u64)> = None;

        for entry in self.pools.iter() {
            let last_access = entry.value().last_access_millis();
            match &oldest {
                None => oldest = Some((entry.key().clone(), last_access)),
                Some((_, oldest_time)) if last_access < *oldest_time => {
                    oldest = Some((entry.key().clone(), last_access));
                }
                _ => {}
            }
        }

        // Evict the oldest
        if let Some((path, _)) = oldest {
            if let Some((_, pool)) = self.pools.remove(&path) {
                let mut adapter = pool.adapter.write().await;
                adapter.close().await?;
            }
        }

        Ok(())
    }

    /// Evict pools that have been idle too long
    pub async fn evict_idle(&self) -> Result<usize, String> {
        let cutoff =
            ManagedPool::now_millis() - self.config.idle_timeout.as_millis() as u64;
        let mut evicted = 0;

        let idle_paths: Vec<PathBuf> = self
            .pools
            .iter()
            .filter(|entry| entry.value().last_access_millis() < cutoff)
            .map(|entry| entry.key().clone())
            .collect();

        for path in idle_paths {
            if let Some((_, pool)) = self.pools.remove(&path) {
                let mut adapter = pool.adapter.write().await;
                if adapter.close().await.is_ok() {
                    evicted += 1;
                }
            }
        }

        Ok(evicted)
    }

    /// Get current pool count
    pub fn pool_count(&self) -> usize {
        self.pools.len()
    }

    /// Get pool paths (for debugging)
    pub fn pool_paths(&self) -> Vec<PathBuf> {
        self.pools.iter().map(|e| e.key().clone()).collect()
    }

    /// Close all pools
    pub async fn close_all(&self) -> Result<(), String> {
        let paths: Vec<PathBuf> = self.pools.iter().map(|e| e.key().clone()).collect();
        for path in paths {
            if let Some((_, pool)) = self.pools.remove(&path) {
                let mut adapter = pool.adapter.write().await;
                let _ = adapter.close().await;
            }
        }
        Ok(())
    }

    // ─── CRUD Operations (delegate to pool) ──────────────────────────────────────

    /// Create a record in the specified database
    pub async fn create(&self, db_path: &Path, record: DataRecord) -> StorageResult<DataRecord> {
        let pool = match self.get_or_create_pool(db_path).await {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let adapter = pool.adapter.read().await;
        adapter.create(record).await
    }

    /// Read a record by ID
    pub async fn read(
        &self,
        db_path: &Path,
        collection: &str,
        id: &UUID,
    ) -> StorageResult<DataRecord> {
        let pool = match self.get_or_create_pool(db_path).await {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let adapter = pool.adapter.read().await;
        adapter.read(collection, id).await
    }

    /// Query records
    pub async fn query(
        &self,
        db_path: &Path,
        query: StorageQuery,
    ) -> StorageResult<Vec<DataRecord>> {
        let pool = match self.get_or_create_pool(db_path).await {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let adapter = pool.adapter.read().await;
        adapter.query(query).await
    }

    /// Count records matching query
    pub async fn count(&self, db_path: &Path, query: StorageQuery) -> StorageResult<usize> {
        let pool = match self.get_or_create_pool(db_path).await {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let adapter = pool.adapter.read().await;
        adapter.count(query).await
    }

    /// Update a record
    pub async fn update(
        &self,
        db_path: &Path,
        collection: &str,
        id: &UUID,
        data: Value,
        increment_version: bool,
    ) -> StorageResult<DataRecord> {
        let pool = match self.get_or_create_pool(db_path).await {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let adapter = pool.adapter.read().await;
        adapter.update(collection, id, data, increment_version).await
    }

    /// Delete a record
    pub async fn delete(
        &self,
        db_path: &Path,
        collection: &str,
        id: &UUID,
    ) -> StorageResult<bool> {
        let pool = match self.get_or_create_pool(db_path).await {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let adapter = pool.adapter.read().await;
        adapter.delete(collection, id).await
    }

    /// Execute batch operations
    pub async fn batch(
        &self,
        db_path: &Path,
        operations: Vec<BatchOperation>,
    ) -> StorageResult<Vec<Value>> {
        let pool = match self.get_or_create_pool(db_path).await {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let adapter = pool.adapter.read().await;
        adapter.batch(operations).await
    }

    // ─── Schema Operations ────────────────────────────────────────────────────────

    /// Ensure schema exists
    pub async fn ensure_schema(
        &self,
        db_path: &Path,
        schema: CollectionSchema,
    ) -> StorageResult<bool> {
        let pool = match self.get_or_create_pool(db_path).await {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let adapter = pool.adapter.read().await;
        adapter.ensure_schema(schema).await
    }

    /// List collections in database
    pub async fn list_collections(&self, db_path: &Path) -> StorageResult<Vec<String>> {
        let pool = match self.get_or_create_pool(db_path).await {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let adapter = pool.adapter.read().await;
        adapter.list_collections().await
    }

    // ─── Maintenance Operations ───────────────────────────────────────────────────

    /// Truncate a collection
    pub async fn truncate(&self, db_path: &Path, collection: &str) -> StorageResult<bool> {
        let pool = match self.get_or_create_pool(db_path).await {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let adapter = pool.adapter.read().await;
        adapter.truncate(collection).await
    }

    /// Run cleanup/optimization on a database
    pub async fn cleanup(&self, db_path: &Path) -> Result<(), String> {
        let pool = self.get_or_create_pool(db_path).await?;
        let adapter = pool.adapter.read().await;
        adapter.cleanup().await
    }
}

impl Default for ConnectionManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_create_pool_on_demand() {
        let manager = ConnectionManager::new();
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Initially no pools
        assert_eq!(manager.pool_count(), 0);

        // Query creates pool on demand
        let result = manager
            .query(&db_path, StorageQuery {
                collection: "users".to_string(),
                ..Default::default()
            })
            .await;

        // Pool was created (even if query fails because table doesn't exist)
        assert_eq!(manager.pool_count(), 1);

        manager.close_all().await.unwrap();
    }

    #[tokio::test]
    async fn test_pool_reuse() {
        let manager = ConnectionManager::new();
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // First access creates pool
        let _ = manager
            .list_collections(&db_path)
            .await;
        assert_eq!(manager.pool_count(), 1);

        // Second access reuses pool
        let _ = manager
            .list_collections(&db_path)
            .await;
        assert_eq!(manager.pool_count(), 1);

        manager.close_all().await.unwrap();
    }

    #[tokio::test]
    async fn test_multiple_dbs() {
        let manager = ConnectionManager::new();
        let dir = tempdir().unwrap();

        let db1 = dir.path().join("db1.db");
        let db2 = dir.path().join("db2.db");
        let db3 = dir.path().join("db3.db");

        let _ = manager.list_collections(&db1).await;
        let _ = manager.list_collections(&db2).await;
        let _ = manager.list_collections(&db3).await;

        assert_eq!(manager.pool_count(), 3);

        // Each path is tracked
        let paths = manager.pool_paths();
        assert!(paths.contains(&db1));
        assert!(paths.contains(&db2));
        assert!(paths.contains(&db3));

        manager.close_all().await.unwrap();
    }

    #[tokio::test]
    async fn test_lru_eviction() {
        let config = ConnectionManagerConfig {
            max_pools: 2, // Only allow 2 pools
            ..Default::default()
        };
        let manager = ConnectionManager::with_config(config);
        let dir = tempdir().unwrap();

        let db1 = dir.path().join("db1.db");
        let db2 = dir.path().join("db2.db");
        let db3 = dir.path().join("db3.db");

        // Create 2 pools (at capacity)
        let _ = manager.list_collections(&db1).await;
        let _ = manager.list_collections(&db2).await;
        assert_eq!(manager.pool_count(), 2);

        // Access db1 again to make db2 the LRU
        let _ = manager.list_collections(&db1).await;

        // Creating db3 should evict db2 (LRU)
        let _ = manager.list_collections(&db3).await;
        assert_eq!(manager.pool_count(), 2);

        let paths = manager.pool_paths();
        assert!(paths.contains(&db1));
        assert!(paths.contains(&db3));
        assert!(!paths.contains(&db2)); // db2 was evicted

        manager.close_all().await.unwrap();
    }

    #[tokio::test]
    async fn test_create_and_read() {
        let manager = ConnectionManager::new();
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        // Create schema
        manager
            .ensure_schema(
                &db_path,
                CollectionSchema {
                    collection: "users".to_string(),
                    fields: vec![super::super::types::SchemaField {
                        name: "name".to_string(),
                        field_type: super::super::types::FieldType::String,
                        indexed: false,
                        unique: false,
                        nullable: false,
                        max_length: None,
                    }],
                    indexes: vec![],
                },
            )
            .await;

        // Create record
        let record = DataRecord {
            id: "user-123".to_string(),
            collection: "users".to_string(),
            data: serde_json::json!({"name": "Joel"}),
            metadata: super::super::types::RecordMetadata::default(),
        };

        let create_result = manager.create(&db_path, record).await;
        assert!(create_result.success);

        // Read it back
        let read_result = manager
            .read(&db_path, "users", &"user-123".to_string())
            .await;
        assert!(read_result.success);
        let data = read_result.data.unwrap();
        assert_eq!(data.data["name"], "Joel");

        manager.close_all().await.unwrap();
    }
}
