//! DataModule — Storage and ORM operations via the StorageAdapter trait.
//!
//! Handles: data/* commands (create, read, update, delete, query, batch)
//! Also handles: vector/* commands (vector similarity search with in-memory caching)
//! Uses the ORM module's StorageAdapter trait for database-agnostic operations.
//!
//! CRITICAL: Database paths are ALWAYS passed by the caller (TypeScript handle layer).
//! NO defaults, NO environment variables, NO fallbacks. The caller owns the paths.

use chrono;
use crate::{log_error, log_info};
use crate::modules::embedding::generate_embeddings_batch;
use crate::orm::{
    adapter::{AdapterConfig, StorageAdapter},
    query::{FieldFilter, StorageQuery},
    sqlite::SqliteAdapter,
    types::{BatchOperation, CollectionSchema, DataRecord, RecordMetadata, UUID},
};
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use async_trait::async_trait;
use dashmap::DashMap;
use rayon::prelude::*;
use serde::Deserialize;
use serde_json::{json, Value};
use std::any::Any;
use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use tokio::sync::Mutex;

// ============================================================================
// Vector Search Types and Cache
// ============================================================================

/// Cached vector for in-memory similarity search
struct CachedVector {
    id: String,
    embedding: Vec<f64>,
}

/// Collection vector cache with Arc for zero-copy sharing during concurrent searches
struct VectorCache {
    vectors: Arc<Vec<CachedVector>>,
}

/// Cache key: (db_path, collection)
type VectorCacheKey = (String, String);

// ============================================================================
// Paginated Query State
// ============================================================================

/// Paginated query state - server-side cursor management
/// Advantage over TypeScript: no IPC per page, just in-memory state
#[derive(Debug)]
struct PaginatedQueryState {
    // NOTE: query_id is NOT stored here - it's the DashMap key
    db_path: String,
    collection: String,
    filter: Option<std::collections::HashMap<String, FieldFilter>>,
    sort: Option<Vec<crate::orm::query::SortSpec>>,
    page_size: usize,
    total_count: u64,
    current_page: usize,
    /// Cursor: last ID from previous page for efficient keyset pagination
    cursor_id: Option<String>,
    has_more: bool,
    /// Creation time for future TTL-based cleanup of stale queries
    #[allow(dead_code)]
    created_at: std::time::Instant,
}

/// DataModule manages storage operations. Database path comes from each request.
///
/// NOTE: SqliteAdapter uses an internal worker thread with mpsc channels.
/// All methods take &self and the sender is Clone+Send, so we don't need
/// a Mutex around the adapter - concurrent sends are safe.
pub struct DataModule {
    /// Adapter cache: path -> initialized adapter
    /// Lazy initialization per unique path
    /// Uses Arc<SqliteAdapter> without Mutex - SqliteAdapter is internally thread-safe
    adapters: DashMap<String, Arc<SqliteAdapter>>,
    /// Mutex only used during adapter initialization (one-time setup)
    init_lock: Mutex<()>,
    /// Vector cache: (db_path, collection) -> vectors
    /// Uses RwLock for concurrent reads (no mutex contention during searches)
    vector_cache: RwLock<HashMap<VectorCacheKey, VectorCache>>,
    /// Paginated query state: queryId -> state
    /// Server-side cursor management for efficient pagination
    paginated_queries: DashMap<String, PaginatedQueryState>,
    /// Module context for inter-module communication (event bus, shared compute)
    /// Set during initialize(), used to publish data change events
    context: RwLock<Option<Arc<ModuleContext>>>,
}

impl DataModule {
    pub fn new() -> Self {
        Self {
            adapters: DashMap::new(),
            init_lock: Mutex::new(()),
            vector_cache: RwLock::new(HashMap::new()),
            paginated_queries: DashMap::new(),
            context: RwLock::new(None),
        }
    }

    /// Publish a data change event to the message bus.
    /// Events follow pattern: data:{collection}:{action}
    /// Actions: created, updated, deleted, batch
    fn publish_event(&self, collection: &str, action: &str, payload: serde_json::Value) {
        let ctx_guard = self.context.read().unwrap();
        if let Some(ctx) = ctx_guard.as_ref() {
            let event_name = format!("data:{}:{}", collection, action);
            ctx.bus.publish_async_only(&event_name, payload);
        }
    }

    /// Log a slow query to the module's dedicated log file.
    /// Only logs if duration exceeds threshold (50ms).
    fn log_slow_query(&self, operation: &str, collection: &str, duration_ms: u128) {
        if duration_ms < 50 {
            return;
        }
        let ctx_guard = self.context.read().unwrap();
        if let Some(ctx) = ctx_guard.as_ref() {
            let logger = ctx.logger("data");
            logger.timing_with_meta(
                operation,
                duration_ms as u64,
                &format!("collection={}", collection),
            );
        }
    }

    /// Get or create adapter for the given path. Path is REQUIRED.
    /// NOTE: No Mutex around adapter - SqliteAdapter is internally thread-safe via mpsc channels.
    async fn get_adapter(&self, db_path: &str) -> Result<Arc<SqliteAdapter>, String> {
        // Check cache first (fast path - no lock needed)
        if let Some(adapter) = self.adapters.get(db_path) {
            return Ok(adapter.clone());
        }

        // Slow path: need to initialize. Use lock to prevent double-init.
        let _guard = self.init_lock.lock().await;

        // Double-check after acquiring lock
        if let Some(adapter) = self.adapters.get(db_path) {
            return Ok(adapter.clone());
        }

        // Create and initialize new adapter
        let mut adapter = SqliteAdapter::new();
        let config = AdapterConfig {
            connection_string: db_path.to_string(),
            namespace: None,
            timeout_ms: 30_000,
            max_connections: 20,
        };
        adapter.initialize(config).await?;

        let adapter = Arc::new(adapter);
        self.adapters.insert(db_path.to_string(), adapter.clone());

        Ok(adapter)
    }
}

impl Default for DataModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for DataModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "data",
            priority: ModulePriority::Normal,
            command_prefixes: &["data/", "adapter/", "vector/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // Store context for event publishing
        let ctx_arc = Arc::new(ModuleContext::new(
            ctx.registry.clone(),
            ctx.bus.clone(),
            ctx.compute.clone(),
            ctx.runtime.clone(),
        ));
        *self.context.write().unwrap() = Some(ctx_arc);
        log_info!("data", "init", "DataModule initialized with event bus");
        Ok(())
    }

    async fn handle_command(
        &self,
        command: &str,
        params: Value,
    ) -> Result<CommandResult, String> {
        log_info!("data", "handle_command", "Received: {} params: {}", command, params);
        match command {
            "data/create" => self.handle_create(params).await,
            "data/read" => self.handle_read(params).await,
            "data/update" => self.handle_update(params).await,
            "data/delete" => self.handle_delete(params).await,
            "data/query" | "data/list" => self.handle_query(params).await,
            "data/queryWithJoin" => self.handle_query_with_join(params).await,
            "data/count" => self.handle_count(params).await,
            "data/batch" => self.handle_batch(params).await,
            "data/ensure-schema" => self.handle_ensure_schema(params).await,
            "data/list-collections" => self.handle_list_collections(params).await,
            "data/collection-stats" => self.handle_collection_stats(params).await,
            "data/truncate" => self.handle_truncate(params).await,
            "data/clear-all" => self.handle_clear_all(params).await,

            // Paginated queries - server-side cursor management
            "data/query-open" => self.handle_query_open(params).await,
            "data/query-next" => self.handle_query_next(params).await,
            "data/query-close" => self.handle_query_close(params).await,

            "adapter/capabilities" => self.handle_capabilities(params).await,
            "adapter/info" => self.handle_info(params).await,

            // Vector search (migrated from data-daemon-worker)
            "vector/search" => self.handle_vector_search(params).await,
            "vector/index" => self.handle_index_vector(params).await,
            "vector/stats" => self.handle_vector_stats(params).await,
            "vector/invalidate-cache" => self.handle_invalidate_vector_cache(params).await,
            "vector/backfill" => self.handle_backfill_vectors(params).await,

            _ => Err(format!("Unknown data command: {command}")),
        }
    }

    async fn shutdown(&self) -> Result<(), String> {
        // Close all adapters - take ownership to get mutable access
        let paths: Vec<String> = self.adapters.iter().map(|e| e.key().clone()).collect();
        for path in paths {
            if let Some((_, adapter)) = self.adapters.remove(&path) {
                // Try to get exclusive access for proper close
                // If other refs exist, drop will clean up eventually
                if let Ok(mut adapter) = Arc::try_unwrap(adapter) {
                    let _ = adapter.close().await;
                }
            }
        }
        Ok(())
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// Command param structs - ALL require dbPath

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CreateParams {
    db_path: String,
    collection: String,
    id: Option<UUID>,
    data: Value,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReadParams {
    db_path: String,
    collection: String,
    id: UUID,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateParams {
    db_path: String,
    collection: String,
    id: UUID,
    data: Value,
    #[serde(default)]
    increment_version: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DeleteParams {
    db_path: String,
    collection: String,
    id: UUID,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryParams {
    db_path: String,
    collection: String,
    #[serde(default)]
    filter: Option<std::collections::HashMap<String, FieldFilter>>,
    #[serde(default)]
    sort: Option<Vec<crate::orm::query::SortSpec>>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryWithJoinParams {
    db_path: String,
    collection: String,
    #[serde(default)]
    filter: Option<std::collections::HashMap<String, FieldFilter>>,
    #[serde(default)]
    sort: Option<Vec<crate::orm::query::SortSpec>>,
    #[serde(default)]
    limit: Option<usize>,
    #[serde(default)]
    offset: Option<usize>,
    #[serde(default)]
    joins: Option<Vec<crate::orm::query::JoinSpec>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CountParams {
    db_path: String,
    collection: String,
    #[serde(default)]
    filter: Option<serde_json::Map<String, Value>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BatchParams {
    db_path: String,
    operations: Vec<BatchOperation>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SchemaParams {
    db_path: String,
    schema: CollectionSchema,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CollectionParams {
    db_path: String,
    collection: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct DbPathOnly {
    db_path: String,
}

/// Vector search params (matches data-daemon API)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VectorSearchParams {
    db_path: String,
    collection: String,
    query_vector: Vec<f64>,
    #[serde(default = "default_k")]
    k: usize,
    #[serde(default)]
    threshold: f64,
    #[serde(default = "default_true")]
    include_data: bool,
}

fn default_k() -> usize { 10 }
fn default_true() -> bool { true }
fn default_batch_size() -> usize { 100 }

/// Index vector params - store embedding for a record
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IndexVectorParams {
    db_path: String,
    collection: String,
    id: String,
    embedding: Vec<f64>,
}

/// Backfill vectors params - generate embeddings for existing records
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BackfillVectorsParams {
    db_path: String,
    collection: String,
    text_field: String,
    #[serde(default = "default_batch_size")]
    batch_size: usize,
    #[serde(default)]
    model: Option<String>,
    #[serde(default)]
    filter: Option<std::collections::HashMap<String, FieldFilter>>,
}

/// Vector stats params
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct VectorStatsParams {
    db_path: String,
    collection: String,
}

// ============================================================================
// Paginated Query Params
// ============================================================================

fn default_page_size() -> usize { 100 }

/// Open paginated query params
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryOpenParams {
    db_path: String,
    collection: String,
    #[serde(default)]
    filter: Option<std::collections::HashMap<String, FieldFilter>>,
    #[serde(default)]
    sort: Option<Vec<crate::orm::query::SortSpec>>,
    #[serde(default = "default_page_size")]
    page_size: usize,
}

/// Get next page params
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryNextParams {
    query_id: String,
}

/// Close query params
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueryCloseParams {
    query_id: String,
}

impl DataModule {
    async fn handle_create(&self, params: Value) -> Result<CommandResult, String> {
        use std::time::Instant;
        let start = Instant::now();

        let params: CreateParams =
            serde_json::from_value(params.clone()).map_err(|e| {
                log_error!("data", "create", "Parse error: {}, params: {}", e, params);
                format!("Invalid params: {e}")
            })?;

        let id = params.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        let collection = params.collection.clone();

        let record = DataRecord {
            id: id.clone(),
            collection: params.collection,
            data: params.data,
            metadata: RecordMetadata::default(),
        };

        let adapter = self.get_adapter(&params.db_path).await?;
        let result = adapter.create(record).await;
        let total_ms = start.elapsed().as_millis();

        // Log slow creates to module log file
        self.log_slow_query("create", &collection, total_ms);

        // Publish event on success
        if result.success {
            self.publish_event(&collection, "created", json!({
                "id": id,
                "collection": collection
            }));
        }

        Ok(CommandResult::Json(serde_json::to_value(result).unwrap()))
    }

    async fn handle_read(&self, params: Value) -> Result<CommandResult, String> {
        use std::time::Instant;
        let start = Instant::now();

        let params: ReadParams =
            serde_json::from_value(params).map_err(|e| format!("Invalid params: {e}"))?;

        let adapter = self.get_adapter(&params.db_path).await?;
        let result = adapter.read(&params.collection, &params.id).await;
        let total_ms = start.elapsed().as_millis();

        // Log slow reads to module log file
        self.log_slow_query("read", &params.collection, total_ms);

        Ok(CommandResult::Json(serde_json::to_value(result).unwrap()))
    }

    async fn handle_update(&self, params: Value) -> Result<CommandResult, String> {
        let params: UpdateParams =
            serde_json::from_value(params.clone()).map_err(|e| {
                log_error!("data", "update", "Parse error: {}, params: {}", e, params);
                format!("Invalid params: {e}")
            })?;

        let collection = params.collection.clone();
        let id = params.id.clone();

        let adapter = self.get_adapter(&params.db_path).await?;
        let result = adapter
            .update(
                &params.collection,
                &params.id,
                params.data,
                params.increment_version,
            )
            .await;

        // Publish event on success
        if result.success {
            self.publish_event(&collection, "updated", json!({
                "id": id,
                "collection": collection
            }));
        }

        Ok(CommandResult::Json(serde_json::to_value(result).unwrap()))
    }

    async fn handle_delete(&self, params: Value) -> Result<CommandResult, String> {
        let params: DeleteParams =
            serde_json::from_value(params).map_err(|e| format!("Invalid params: {e}"))?;

        let collection = params.collection.clone();
        let id = params.id.clone();

        let adapter = self.get_adapter(&params.db_path).await?;
        let result = adapter.delete(&params.collection, &params.id).await;

        // Publish event on success
        if result.success {
            self.publish_event(&collection, "deleted", json!({
                "id": id,
                "collection": collection
            }));
        }

        Ok(CommandResult::Json(serde_json::to_value(result).unwrap()))
    }

    async fn handle_query(&self, params: Value) -> Result<CommandResult, String> {
        use std::time::Instant;
        let start = Instant::now();

        let params: QueryParams =
            serde_json::from_value(params.clone()).map_err(|e| {
                log_error!("data", "query", "Parse error: {}, params: {}", e, params);
                format!("Invalid params: {e}")
            })?;

        let query = StorageQuery {
            collection: params.collection.clone(),
            filter: params.filter,
            sort: params.sort,
            limit: params.limit,
            offset: params.offset,
            ..Default::default()
        };

        let adapter = self.get_adapter(&params.db_path).await?;
        let result = adapter.query(query).await;
        let total_ms = start.elapsed().as_millis();

        // Log slow queries to module log file
        self.log_slow_query("query", &params.collection, total_ms);

        Ok(CommandResult::Json(serde_json::to_value(result).unwrap()))
    }

    async fn handle_query_with_join(&self, params: Value) -> Result<CommandResult, String> {
        let params: QueryWithJoinParams =
            serde_json::from_value(params).map_err(|e| format!("Invalid params: {e}"))?;

        let query = StorageQuery {
            collection: params.collection,
            filter: params.filter,
            sort: params.sort,
            limit: params.limit,
            offset: params.offset,
            joins: params.joins,
            ..Default::default()
        };

        let adapter = self.get_adapter(&params.db_path).await?;
                let result = adapter.query_with_join(query).await;

        Ok(CommandResult::Json(serde_json::to_value(result).unwrap()))
    }

    async fn handle_count(&self, params: Value) -> Result<CommandResult, String> {
        use std::time::Instant;
        let start = Instant::now();

        let params: CountParams =
            serde_json::from_value(params).map_err(|e| format!("Invalid params: {e}"))?;

        let query = StorageQuery {
            collection: params.collection.clone(),
            filter: params.filter.map(|m| {
                m.into_iter()
                    .map(|(k, v)| (k, FieldFilter::Value(v)))
                    .collect()
            }),
            ..Default::default()
        };

        let adapter_start = Instant::now();
        let adapter = self.get_adapter(&params.db_path).await?;
        let adapter_ms = adapter_start.elapsed().as_millis();

        let count_start = Instant::now();
        let result = adapter.count(query).await;
        let count_ms = count_start.elapsed().as_millis();

        let total_ms = start.elapsed().as_millis();
        if total_ms > 50 {
            log_info!("data", "count", "TIMING: collection={}, total={}ms (adapter={}ms, count={}ms), success={}",
                params.collection, total_ms, adapter_ms, count_ms, result.success);
        }

        Ok(CommandResult::Json(serde_json::to_value(result).unwrap()))
    }

    async fn handle_batch(&self, params: Value) -> Result<CommandResult, String> {
        let params: BatchParams =
            serde_json::from_value(params).map_err(|e| format!("Invalid params: {e}"))?;

        let op_count = params.operations.len();

        let adapter = self.get_adapter(&params.db_path).await?;
        let result = adapter.batch(params.operations).await;

        // Publish batch event on success
        if result.success {
            self.publish_event("batch", "completed", json!({
                "operationCount": op_count,
                "successCount": result.data.as_ref().map(|d| d.len()).unwrap_or(0)
            }));
        }

        Ok(CommandResult::Json(serde_json::to_value(result).unwrap()))
    }

    async fn handle_ensure_schema(&self, params: Value) -> Result<CommandResult, String> {
        let params: SchemaParams =
            serde_json::from_value(params).map_err(|e| format!("Invalid params: {e}"))?;

        let adapter = self.get_adapter(&params.db_path).await?;
                let result = adapter.ensure_schema(params.schema).await;

        Ok(CommandResult::Json(serde_json::to_value(result).unwrap()))
    }

    async fn handle_list_collections(&self, params: Value) -> Result<CommandResult, String> {
        let params: DbPathOnly =
            serde_json::from_value(params).map_err(|e| format!("Invalid params: {e}"))?;

        let adapter = self.get_adapter(&params.db_path).await?;
                let result = adapter.list_collections().await;

        Ok(CommandResult::Json(serde_json::to_value(result).unwrap()))
    }

    async fn handle_collection_stats(&self, params: Value) -> Result<CommandResult, String> {
        let params: CollectionParams =
            serde_json::from_value(params).map_err(|e| format!("Invalid params: {e}"))?;

        let adapter = self.get_adapter(&params.db_path).await?;
                let result = adapter.collection_stats(&params.collection).await;

        Ok(CommandResult::Json(serde_json::to_value(result).unwrap()))
    }

    async fn handle_truncate(&self, params: Value) -> Result<CommandResult, String> {
        let params: CollectionParams =
            serde_json::from_value(params).map_err(|e| format!("Invalid params: {e}"))?;

        let adapter = self.get_adapter(&params.db_path).await?;
                let result = adapter.truncate(&params.collection).await;

        Ok(CommandResult::Json(serde_json::to_value(result).unwrap()))
    }

    async fn handle_clear_all(&self, params: Value) -> Result<CommandResult, String> {
        let params: DbPathOnly =
            serde_json::from_value(params).map_err(|e| format!("Invalid params: {e}"))?;

        let adapter = self.get_adapter(&params.db_path).await?;
                let result = adapter.clear_all().await;

        Ok(CommandResult::Json(serde_json::to_value(result).unwrap()))
    }

    async fn handle_capabilities(&self, params: Value) -> Result<CommandResult, String> {
        let params: DbPathOnly =
            serde_json::from_value(params).map_err(|e| format!("Invalid params: {e}"))?;

        let adapter = self.get_adapter(&params.db_path).await?;
                let caps = adapter.capabilities();

        Ok(CommandResult::Json(json!({
            "supportsTransactions": caps.supports_transactions,
            "supportsJoins": caps.supports_joins,
            "supportsIndexing": caps.supports_indexing,
            "supportsFullTextSearch": caps.supports_full_text_search,
            "supportsVectorSearch": caps.supports_vector_search,
            "supportsBatch": caps.supports_batch,
            "maxRecordSize": caps.max_record_size,
        })))
    }

    async fn handle_info(&self, params: Value) -> Result<CommandResult, String> {
        let params: DbPathOnly =
            serde_json::from_value(params).map_err(|e| format!("Invalid params: {e}"))?;

        let adapter = self.get_adapter(&params.db_path).await?;
                let caps = adapter.capabilities();

        Ok(CommandResult::Json(json!({
            "adapter": adapter.name(),
            "path": params.db_path,
            "capabilities": {
                "supportsTransactions": caps.supports_transactions,
                "supportsJoins": caps.supports_joins,
            }
        })))
    }

    // =========================================================================
    // Vector Search (migrated from data-daemon-worker)
    // =========================================================================

    /// Vector similarity search with in-memory caching
    ///
    /// OPTIMIZATION: Vectors are cached in memory per (dbPath, collection).
    /// First search loads from SQLite, subsequent searches are instant.
    ///
    /// Flow:
    /// 1. Check cache (RwLock read - concurrent, no blocking)
    /// 2. If miss, load from SQLite (serialized, but only once per collection)
    /// 3. Parallel rayon search against cached vectors
    async fn handle_vector_search(&self, params: Value) -> Result<CommandResult, String> {
        use std::time::Instant;
        let search_start = Instant::now();

        let params: VectorSearchParams =
            serde_json::from_value(params.clone()).map_err(|e| {
                log_error!("data", "vector/search", "Parse error: {}, params: {}", e, params);
                format!("Invalid params: {e}")
            })?;

        let cache_key = (params.db_path.clone(), params.collection.clone());

        // Step 1: Try to get vectors from cache (RwLock read - concurrent)
        let cached_vectors: Option<Arc<Vec<CachedVector>>> = {
            let cache = self.vector_cache.read().unwrap();
            cache.get(&cache_key).map(|c| c.vectors.clone())
        };

        let corpus: Arc<Vec<CachedVector>> = if let Some(vectors) = cached_vectors {
            log_info!("data", "vector/search", "Cache HIT for {} ({} vectors)",
                params.collection, vectors.len());
            vectors
        } else {
            // Cache MISS - load from SQLite
            log_info!("data", "vector/search", "Cache MISS for {} - loading from SQLite",
                params.collection);
            let load_start = Instant::now();

            // Get adapter and load vectors
            let adapter = self.get_adapter(&params.db_path).await?;

            // Query all records with embeddings
            let query = StorageQuery {
                collection: params.collection.clone(),
                filter: None,
                sort: None,
                limit: None,
                offset: None,
                cursor: None,
                tags: None,
                time_range: None,
                joins: None,
            };

            let result = adapter.query(query).await;
            if !result.success {
                return Err(result.error.unwrap_or_else(|| "Query failed".to_string()));
            }

            // Extract vectors from records
            let mut vectors: Vec<CachedVector> = Vec::new();
            for record in result.data.unwrap_or_default() {
                if let Some(embedding) = record.data.get("embedding") {
                    let vec = Self::parse_embedding(embedding);
                    if !vec.is_empty() {
                        vectors.push(CachedVector {
                            id: record.id,
                            embedding: vec,
                        });
                    }
                }
            }

            let vectors_arc = Arc::new(vectors);
            let count = vectors_arc.len();

            // Store in cache
            {
                let mut cache = self.vector_cache.write().unwrap();
                cache.insert(cache_key, VectorCache { vectors: vectors_arc.clone() });
            }

            log_info!("data", "vector/search", "Cached {} vectors for {} in {:?}",
                count, params.collection, load_start.elapsed());
            vectors_arc
        };

        if corpus.is_empty() {
            return Ok(CommandResult::Json(json!({
                "results": [],
                "count": 0,
                "corpusSize": 0
            })));
        }

        let corpus_size = corpus.len();

        // Step 2: Parallel cosine similarity with rayon
        let query_vec = &params.query_vector;
        let threshold = params.threshold;

        let mut scored: Vec<(String, f64)> = corpus
            .par_iter()
            .filter_map(|cv| {
                let score = Self::cosine_similarity(query_vec, &cv.embedding);
                if score >= threshold {
                    Some((cv.id.clone(), score))
                } else {
                    None
                }
            })
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        let top_k: Vec<(String, f64)> = scored.into_iter().take(params.k).collect();
        let count = top_k.len();

        // Build results
        let results: Vec<Value> = if params.include_data {
            // Fetch full records for top-k (need another query)
            let adapter = self.get_adapter(&params.db_path).await?;
            let mut full_results = Vec::new();

            for (id, score) in &top_k {
                let result = adapter.read(&params.collection, id).await;
                if result.success {
                    if let Some(record) = result.data {
                        full_results.push(json!({
                            "id": id,
                            "score": score,
                            "distance": 1.0 - score,
                            "data": record.data
                        }));
                    }
                }
            }
            full_results
        } else {
            top_k.into_iter().map(|(id, score)| json!({
                "id": id,
                "score": score,
                "distance": 1.0 - score
            })).collect()
        };

        log_info!("data", "vector/search", "Complete: {} results from {} vectors in {:?}",
            count, corpus_size, search_start.elapsed());

        Ok(CommandResult::Json(json!({
            "results": results,
            "count": count,
            "corpusSize": corpus_size
        })))
    }

    /// Parse embedding from record data (supports BLOB and JSON array)
    fn parse_embedding(value: &Value) -> Vec<f64> {
        match value {
            Value::Array(arr) => arr.iter()
                .filter_map(|v| v.as_f64())
                .collect(),
            Value::String(s) => {
                // Try parsing as JSON array
                serde_json::from_str(s).unwrap_or_default()
            }
            _ => Vec::new(),
        }
    }

    /// Cosine similarity between two vectors
    /// Uses 4-way loop unrolling for SIMD-like performance
    fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
        if a.len() != b.len() || a.is_empty() {
            return 0.0;
        }

        let len = a.len();
        let limit = len - (len % 4);

        let mut dot = 0.0;
        let mut norm_a = 0.0;
        let mut norm_b = 0.0;

        // 4-way unrolled loop
        let mut i = 0;
        while i < limit {
            let a0 = a[i];
            let a1 = a[i + 1];
            let a2 = a[i + 2];
            let a3 = a[i + 3];
            let b0 = b[i];
            let b1 = b[i + 1];
            let b2 = b[i + 2];
            let b3 = b[i + 3];

            dot += a0 * b0 + a1 * b1 + a2 * b2 + a3 * b3;
            norm_a += a0 * a0 + a1 * a1 + a2 * a2 + a3 * a3;
            norm_b += b0 * b0 + b1 * b1 + b2 * b2 + b3 * b3;
            i += 4;
        }

        // Handle remainder
        while i < len {
            dot += a[i] * b[i];
            norm_a += a[i] * a[i];
            norm_b += b[i] * b[i];
            i += 1;
        }

        let denominator = (norm_a * norm_b).sqrt();
        if denominator == 0.0 { 0.0 } else { dot / denominator }
    }

    /// Index a vector - store embedding for a record
    /// Updates the record's 'embedding' field with the provided vector
    async fn handle_index_vector(&self, params: Value) -> Result<CommandResult, String> {
        use std::time::Instant;
        let start = Instant::now();

        let params: IndexVectorParams =
            serde_json::from_value(params.clone()).map_err(|e| {
                log_error!("data", "vector/index", "Parse error: {}, params: {}", e, params);
                format!("Invalid params: {e}")
            })?;

        let adapter = self.get_adapter(&params.db_path).await?;

        // Update the record's embedding field
        let update_data = json!({
            "embedding": params.embedding
        });

        let result = adapter
            .update(&params.collection, &params.id, update_data, false)
            .await;

        // Invalidate vector cache for this collection since we modified an embedding
        {
            let cache_key = (params.db_path.clone(), params.collection.clone());
            let mut cache = self.vector_cache.write().unwrap();
            cache.remove(&cache_key);
        }

        let total_ms = start.elapsed().as_millis();
        log_info!("data", "vector/index", "Indexed vector for {} in {}ms, success={}",
            params.id, total_ms, result.success);

        Ok(CommandResult::Json(serde_json::to_value(result).unwrap()))
    }

    /// Get vector index statistics for a collection
    async fn handle_vector_stats(&self, params: Value) -> Result<CommandResult, String> {
        use std::time::Instant;
        let start = Instant::now();

        let params: VectorStatsParams =
            serde_json::from_value(params.clone()).map_err(|e| {
                log_error!("data", "vector/stats", "Parse error: {}, params: {}", e, params);
                format!("Invalid params: {e}")
            })?;

        let adapter = self.get_adapter(&params.db_path).await?;

        // Get total record count
        let total_query = StorageQuery {
            collection: params.collection.clone(),
            ..Default::default()
        };
        let total_result = adapter.count(total_query).await;
        let total_records = total_result.data.unwrap_or(0);

        // Query to count records WITH embeddings
        // We need to query and check which have embedding field
        let query = StorageQuery {
            collection: params.collection.clone(),
            limit: Some(10000), // Reasonable limit
            ..Default::default()
        };
        let result = adapter.query(query).await;

        let mut records_with_vectors = 0;
        let mut vector_dimensions = 0;

        if let Some(records) = result.data {
            for record in &records {
                if let Some(embedding) = record.data.get("embedding") {
                    let vec = Self::parse_embedding(embedding);
                    if !vec.is_empty() {
                        records_with_vectors += 1;
                        if vector_dimensions == 0 {
                            vector_dimensions = vec.len();
                        }
                    }
                }
            }
        }

        // Check cache status
        let cache_key = (params.db_path.clone(), params.collection.clone());
        let cached_count = {
            let cache = self.vector_cache.read().unwrap();
            cache.get(&cache_key).map(|c| c.vectors.len()).unwrap_or(0)
        };

        let total_ms = start.elapsed().as_millis();
        log_info!("data", "vector/stats", "Stats for {} in {}ms: total={}, with_vectors={}, dims={}",
            params.collection, total_ms, total_records, records_with_vectors, vector_dimensions);

        // Wrap in StorageResult-style response for TypeScript compatibility
        Ok(CommandResult::Json(json!({
            "success": true,
            "data": {
                "collection": params.collection,
                "totalRecords": total_records,
                "recordsWithVectors": records_with_vectors,
                "vectorDimensions": vector_dimensions,
                "cachedVectors": cached_count,
                "lastUpdated": chrono::Utc::now().to_rfc3339()
            }
        })))
    }

    /// Invalidate vector cache for a collection
    /// Called when records are modified outside of vector/index
    async fn handle_invalidate_vector_cache(&self, params: Value) -> Result<CommandResult, String> {
        let params: CollectionParams =
            serde_json::from_value(params.clone()).map_err(|e| {
                log_error!("data", "vector/invalidate-cache", "Parse error: {}, params: {}", e, params);
                format!("Invalid params: {e}")
            })?;

        let cache_key = (params.db_path.clone(), params.collection.clone());
        let removed = {
            let mut cache = self.vector_cache.write().unwrap();
            cache.remove(&cache_key).is_some()
        };

        log_info!("data", "vector/invalidate-cache", "Invalidated cache for {}: removed={}",
            params.collection, removed);

        Ok(CommandResult::Json(json!({
            "success": true,
            "collection": params.collection,
            "cacheInvalidated": removed
        })))
    }

    /// Backfill vectors - generate embeddings for records missing them
    ///
    /// Uses batch embedding generation for efficiency (10x faster than single).
    /// Processes in configurable batch sizes to manage memory.
    async fn handle_backfill_vectors(&self, params: Value) -> Result<CommandResult, String> {
        use std::time::Instant;
        let start = Instant::now();

        let params: BackfillVectorsParams =
            serde_json::from_value(params.clone()).map_err(|e| {
                log_error!("data", "vector/backfill", "Parse error: {}, params: {}", e, params);
                format!("Invalid params: {e}")
            })?;

        let model_name = params.model.as_deref().unwrap_or("AllMiniLML6V2");
        let batch_size = params.batch_size;

        let adapter = self.get_adapter(&params.db_path).await?;

        // Query all records from collection
        let query = StorageQuery {
            collection: params.collection.clone(),
            filter: params.filter.clone(),
            ..Default::default()
        };
        let query_result = adapter.query(query).await;
        if !query_result.success {
            return Err(query_result.error.unwrap_or_else(|| "Query failed".to_string()));
        }

        let records = query_result.data.unwrap_or_default();
        let total = records.len();
        let mut processed = 0usize;
        let mut failed = 0usize;
        let mut skipped = 0usize;

        log_info!("data", "vector/backfill", "Starting backfill for {} records in {}",
            total, params.collection);

        // Process in batches for memory efficiency
        for chunk in records.chunks(batch_size) {
            // Collect texts that need embeddings
            let mut texts_to_embed: Vec<(usize, &str)> = Vec::new();

            for (i, record) in chunk.iter().enumerate() {
                // Check if already has embedding
                if let Some(embedding) = record.data.get("embedding") {
                    if !embedding.is_null() {
                        skipped += 1;
                        continue;
                    }
                }

                // Extract text from specified field
                if let Some(text) = record.data.get(&params.text_field) {
                    if let Some(text_str) = text.as_str() {
                        if !text_str.is_empty() {
                            texts_to_embed.push((i, text_str));
                        }
                    }
                }
            }

            if texts_to_embed.is_empty() {
                continue;
            }

            // Batch generate embeddings
            let text_refs: Vec<&str> = texts_to_embed.iter().map(|(_, t)| *t).collect();
            match generate_embeddings_batch(&text_refs, model_name) {
                Ok(embeddings) => {
                    // Update each record with its embedding
                    for ((idx, _), embedding) in texts_to_embed.iter().zip(embeddings.iter()) {
                        let record = &chunk[*idx];

                        // Convert f32 to f64 for JSON
                        let embedding_f64: Vec<f64> = embedding.iter().map(|&v| v as f64).collect();

                        let update_data = json!({
                            "embedding": embedding_f64
                        });

                        let update_result = adapter
                            .update(&params.collection, &record.id, update_data, false)
                            .await;

                        if update_result.success {
                            processed += 1;
                        } else {
                            failed += 1;
                        }
                    }
                }
                Err(e) => {
                    log_error!("data", "vector/backfill", "Batch embedding failed: {}", e);
                    failed += texts_to_embed.len();
                }
            }
        }

        // Invalidate vector cache since we modified embeddings
        {
            let cache_key = (params.db_path.clone(), params.collection.clone());
            let mut cache = self.vector_cache.write().unwrap();
            cache.remove(&cache_key);
        }

        let total_ms = start.elapsed().as_millis();
        log_info!("data", "vector/backfill",
            "Backfill complete for {}: total={}, processed={}, skipped={}, failed={} in {}ms",
            params.collection, total, processed, skipped, failed, total_ms);

        Ok(CommandResult::Json(json!({
            "success": true,
            "data": {
                "collection": params.collection,
                "total": total,
                "processed": processed,
                "skipped": skipped,
                "failed": failed,
                "elapsedMs": total_ms
            }
        })))
    }

    // =========================================================================
    // Paginated Query Handlers
    // =========================================================================

    /// Open a paginated query - returns handle with queryId
    ///
    /// Advantages over TypeScript:
    /// - No IPC overhead per page (state is Rust-side)
    /// - Cursor-based pagination using last ID (faster than OFFSET for large datasets)
    /// - DashMap for concurrent query state (lock-free reads)
    async fn handle_query_open(&self, params: Value) -> Result<CommandResult, String> {
        use std::time::Instant;
        let start = Instant::now();

        let params: QueryOpenParams =
            serde_json::from_value(params.clone()).map_err(|e| {
                log_error!("data", "query-open", "Parse error: {}, params: {}", e, params);
                format!("Invalid params: {e}")
            })?;

        let adapter = self.get_adapter(&params.db_path).await?;

        // Get total count first
        let count_query = StorageQuery {
            collection: params.collection.clone(),
            filter: params.filter.clone(),
            ..Default::default()
        };
        let count_result = adapter.count(count_query).await;
        let total_count = count_result.data.unwrap_or(0) as u64;

        // Generate unique query ID
        let query_id = uuid::Uuid::new_v4().to_string();

        // Create query state (query_id is the DashMap key, not stored in struct)
        let state = PaginatedQueryState {
            db_path: params.db_path.clone(),
            collection: params.collection.clone(),
            filter: params.filter,
            sort: params.sort,
            page_size: params.page_size,
            total_count,
            current_page: 0,
            cursor_id: None,
            has_more: total_count > 0,
            created_at: Instant::now(),
        };

        self.paginated_queries.insert(query_id.clone(), state);

        let total_ms = start.elapsed().as_millis();
        log_info!("data", "query-open", "Opened query {} for {} (total={}, pageSize={}) in {}ms",
            query_id, params.collection, total_count, params.page_size, total_ms);

        // Wrap in StorageResult-style response for TypeScript compatibility
        Ok(CommandResult::Json(json!({
            "success": true,
            "data": {
                "queryId": query_id,
                "collection": params.collection,
                "totalCount": total_count,
                "pageSize": params.page_size,
                "hasMore": total_count > 0
            }
        })))
    }

    /// Get next page from paginated query
    ///
    /// Uses keyset pagination (WHERE id > cursor) instead of OFFSET for performance.
    /// For sorted queries, combines sort column(s) with id for deterministic ordering.
    async fn handle_query_next(&self, params: Value) -> Result<CommandResult, String> {
        use std::time::Instant;
        let start = Instant::now();

        let params: QueryNextParams =
            serde_json::from_value(params.clone()).map_err(|e| {
                log_error!("data", "query-next", "Parse error: {}, params: {}", e, params);
                format!("Invalid params: {e}")
            })?;

        // Get query state (immutable borrow for read)
        let state_info = self.paginated_queries.get(&params.query_id)
            .map(|s| (
                s.db_path.clone(),
                s.collection.clone(),
                s.filter.clone(),
                s.sort.clone(),
                s.page_size,
                s.total_count,
                s.current_page,
                s.cursor_id.clone(),
                s.has_more,
            ));

        let (db_path, collection, filter, sort, page_size, total_count, current_page, _cursor_id, has_more) =
            state_info.ok_or_else(|| format!("Query {} not found", params.query_id))?;

        if !has_more {
            return Ok(CommandResult::Json(json!({
                "success": true,
                "data": {
                    "items": [],
                    "pageNumber": current_page,
                    "hasMore": false,
                    "totalCount": total_count as u64
                }
            })));
        }

        let adapter = self.get_adapter(&db_path).await?;

        // Build query with cursor-based pagination
        // For simplicity, using OFFSET initially. TODO: implement true keyset pagination
        let offset = current_page * page_size;
        let query = StorageQuery {
            collection: collection.clone(),
            filter: filter.clone(),
            sort: sort.clone(),
            limit: Some(page_size),
            offset: Some(offset),
            ..Default::default()
        };

        let result = adapter.query(query).await;
        if !result.success {
            return Err(result.error.unwrap_or_else(|| "Query failed".to_string()));
        }

        let records = result.data.unwrap_or_default();
        let items_count = records.len();
        let new_has_more = items_count == page_size && offset + items_count < total_count as usize;

        // Get last ID for cursor
        let new_cursor_id = records.last().map(|r| r.id.clone());

        // Update query state
        if let Some(mut state) = self.paginated_queries.get_mut(&params.query_id) {
            state.current_page += 1;
            state.cursor_id = new_cursor_id;
            state.has_more = new_has_more;
        }

        // Convert records to JSON
        let items: Vec<Value> = records.into_iter().map(|r| {
            json!({
                "id": r.id,
                "data": r.data,
                "metadata": {
                    "createdAt": r.metadata.created_at,
                    "updatedAt": r.metadata.updated_at,
                    "version": r.metadata.version
                }
            })
        }).collect();

        let total_ms = start.elapsed().as_millis();
        log_info!("data", "query-next", "Page {} for query {} ({} items, hasMore={}) in {}ms",
            current_page + 1, params.query_id, items_count, new_has_more, total_ms);

        // Wrap in StorageResult-style response for TypeScript compatibility
        Ok(CommandResult::Json(json!({
            "success": true,
            "data": {
                "items": items,
                "pageNumber": current_page + 1,
                "hasMore": new_has_more,
                "totalCount": total_count
            }
        })))
    }

    /// Close paginated query and free resources
    async fn handle_query_close(&self, params: Value) -> Result<CommandResult, String> {
        let params: QueryCloseParams =
            serde_json::from_value(params.clone()).map_err(|e| {
                log_error!("data", "query-close", "Parse error: {}, params: {}", e, params);
                format!("Invalid params: {e}")
            })?;

        let removed = self.paginated_queries.remove(&params.query_id).is_some();

        log_info!("data", "query-close", "Closed query {}: removed={}", params.query_id, removed);

        Ok(CommandResult::Json(json!({
            "success": removed,
            "queryId": params.query_id
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_data_module_requires_db_path() {
        let module = DataModule::new();

        // Should fail without dbPath
        let result = module
            .handle_command(
                "data/create",
                json!({
                    "collection": "test_users",
                    "data": { "name": "Alice" }
                }),
            )
            .await;

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("dbPath"));
    }

    #[tokio::test]
    async fn test_data_module_create_and_read() {
        let module = DataModule::new();

        // Create table first
        let schema = CollectionSchema {
            collection: "test_users".to_string(),
            fields: vec![
                crate::orm::types::SchemaField {
                    name: "name".to_string(),
                    field_type: crate::orm::types::FieldType::String,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                },
            ],
            indexes: vec![],
        };

        let _ = module
            .handle_command(
                "data/ensure-schema",
                json!({
                    "dbPath": ":memory:",
                    "schema": schema
                }),
            )
            .await;

        // Create with dbPath
        let create_result = module
            .handle_command(
                "data/create",
                json!({
                    "dbPath": ":memory:",
                    "collection": "test_users",
                    "data": { "name": "Alice" }
                }),
            )
            .await;

        assert!(create_result.is_ok());

        if let Ok(CommandResult::Json(result)) = create_result {
            assert!(result["success"].as_bool().unwrap_or(false));
            let id = result["data"]["id"].as_str().unwrap();

            // Read with dbPath
            let read_result = module
                .handle_command(
                    "data/read",
                    json!({
                        "dbPath": ":memory:",
                        "collection": "test_users",
                        "id": id
                    }),
                )
                .await;

            assert!(read_result.is_ok());
            if let Ok(CommandResult::Json(read)) = read_result {
                assert!(read["success"].as_bool().unwrap_or(false));
                assert_eq!(read["data"]["data"]["name"], "Alice");
            }
        }
    }

    #[tokio::test]
    async fn test_vector_index_and_stats() {
        let module = DataModule::new();

        // Create schema with embedding field
        let schema = CollectionSchema {
            collection: "test_vectors".to_string(),
            fields: vec![
                crate::orm::types::SchemaField {
                    name: "content".to_string(),
                    field_type: crate::orm::types::FieldType::String,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                },
                crate::orm::types::SchemaField {
                    name: "embedding".to_string(),
                    field_type: crate::orm::types::FieldType::Json,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                },
            ],
            indexes: vec![],
        };

        let _ = module
            .handle_command(
                "data/ensure-schema",
                json!({
                    "dbPath": ":memory:",
                    "schema": schema
                }),
            )
            .await;

        // Create a record
        let create_result = module
            .handle_command(
                "data/create",
                json!({
                    "dbPath": ":memory:",
                    "collection": "test_vectors",
                    "data": { "content": "Hello world" }
                }),
            )
            .await;

        assert!(create_result.is_ok());
        let record_id = if let Ok(CommandResult::Json(result)) = &create_result {
            result["data"]["id"].as_str().unwrap().to_string()
        } else {
            panic!("Create failed");
        };

        // Index a vector for this record
        let test_embedding: Vec<f64> = (0..384).map(|i| (i as f64) * 0.001).collect();
        let index_result = module
            .handle_command(
                "vector/index",
                json!({
                    "dbPath": ":memory:",
                    "collection": "test_vectors",
                    "id": record_id,
                    "embedding": test_embedding
                }),
            )
            .await;

        assert!(index_result.is_ok());
        if let Ok(CommandResult::Json(result)) = &index_result {
            assert!(result["success"].as_bool().unwrap_or(false));
        }

        // Get vector stats
        let stats_result = module
            .handle_command(
                "vector/stats",
                json!({
                    "dbPath": ":memory:",
                    "collection": "test_vectors"
                }),
            )
            .await;

        assert!(stats_result.is_ok());
        if let Ok(CommandResult::Json(result)) = stats_result {
            let stats = &result["data"];
            assert_eq!(stats["collection"], "test_vectors");
            assert_eq!(stats["totalRecords"], 1);
            assert_eq!(stats["recordsWithVectors"], 1);
            assert_eq!(stats["vectorDimensions"], 384);
        }
    }

    #[tokio::test]
    async fn test_vector_search_basic() {
        let module = DataModule::new();

        // Create schema
        let schema = CollectionSchema {
            collection: "test_search".to_string(),
            fields: vec![
                crate::orm::types::SchemaField {
                    name: "content".to_string(),
                    field_type: crate::orm::types::FieldType::String,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                },
                crate::orm::types::SchemaField {
                    name: "embedding".to_string(),
                    field_type: crate::orm::types::FieldType::Json,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                },
            ],
            indexes: vec![],
        };

        let _ = module
            .handle_command(
                "data/ensure-schema",
                json!({
                    "dbPath": ":memory:",
                    "schema": schema
                }),
            )
            .await;

        // Create records with embeddings
        let embeddings: Vec<Vec<f64>> = vec![
            (0..384).map(|i| (i as f64) * 0.001).collect(),
            (0..384).map(|i| (i as f64) * 0.002).collect(),
            (0..384).map(|i| (i as f64) * 0.003).collect(),
        ];

        for (idx, emb) in embeddings.iter().enumerate() {
            let _ = module
                .handle_command(
                    "data/create",
                    json!({
                        "dbPath": ":memory:",
                        "collection": "test_search",
                        "data": {
                            "content": format!("Document {}", idx),
                            "embedding": emb
                        }
                    }),
                )
                .await;
        }

        // Search for similar vectors
        let query_vector: Vec<f64> = (0..384).map(|i| (i as f64) * 0.001).collect();
        let search_result = module
            .handle_command(
                "vector/search",
                json!({
                    "dbPath": ":memory:",
                    "collection": "test_search",
                    "queryVector": query_vector,
                    "k": 3,
                    "threshold": 0.0,
                    "includeData": true
                }),
            )
            .await;

        assert!(search_result.is_ok());
        if let Ok(CommandResult::Json(result)) = search_result {
            let results = result["results"].as_array().unwrap();
            assert_eq!(results.len(), 3);
            // First result should be most similar (score close to 1.0)
            let first_score = results[0]["score"].as_f64().unwrap();
            assert!(first_score > 0.9, "Expected high similarity, got {}", first_score);
        }
    }

    #[tokio::test]
    async fn test_vector_cache_invalidation() {
        let module = DataModule::new();

        // Create schema
        let schema = CollectionSchema {
            collection: "test_cache".to_string(),
            fields: vec![
                crate::orm::types::SchemaField {
                    name: "embedding".to_string(),
                    field_type: crate::orm::types::FieldType::Json,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                },
            ],
            indexes: vec![],
        };

        let _ = module
            .handle_command(
                "data/ensure-schema",
                json!({
                    "dbPath": ":memory:",
                    "schema": schema
                }),
            )
            .await;

        // Create a record with embedding
        let _ = module
            .handle_command(
                "data/create",
                json!({
                    "dbPath": ":memory:",
                    "collection": "test_cache",
                    "data": {
                        "embedding": vec![1.0; 384]
                    }
                }),
            )
            .await;

        // First search populates cache
        let query: Vec<f64> = vec![1.0; 384];
        let _ = module
            .handle_command(
                "vector/search",
                json!({
                    "dbPath": ":memory:",
                    "collection": "test_cache",
                    "queryVector": query,
                    "k": 1
                }),
            )
            .await;

        // Verify cache has vectors via stats
        let stats_result = module
            .handle_command(
                "vector/stats",
                json!({
                    "dbPath": ":memory:",
                    "collection": "test_cache"
                }),
            )
            .await;

        if let Ok(CommandResult::Json(result)) = &stats_result {
            let stats = &result["data"];
            assert!(stats["cachedVectors"].as_u64().unwrap() > 0);
        }

        // Invalidate cache
        let invalidate_result = module
            .handle_command(
                "vector/invalidate-cache",
                json!({
                    "dbPath": ":memory:",
                    "collection": "test_cache"
                }),
            )
            .await;

        assert!(invalidate_result.is_ok());
        if let Ok(CommandResult::Json(result)) = invalidate_result {
            assert!(result["success"].as_bool().unwrap_or(false));
            assert!(result["cacheInvalidated"].as_bool().unwrap_or(false));
        }

        // Verify cache is empty
        let stats_after = module
            .handle_command(
                "vector/stats",
                json!({
                    "dbPath": ":memory:",
                    "collection": "test_cache"
                }),
            )
            .await;

        if let Ok(CommandResult::Json(result)) = stats_after {
            let stats = &result["data"];
            assert_eq!(stats["cachedVectors"].as_u64().unwrap(), 0);
        }
    }

    #[tokio::test]
    async fn test_paginated_query() {
        let module = DataModule::new();

        // Create schema
        let schema = CollectionSchema {
            collection: "test_paginated".to_string(),
            fields: vec![
                crate::orm::types::SchemaField {
                    name: "name".to_string(),
                    field_type: crate::orm::types::FieldType::String,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                },
            ],
            indexes: vec![],
        };

        let _ = module
            .handle_command(
                "data/ensure-schema",
                json!({
                    "dbPath": ":memory:",
                    "schema": schema
                }),
            )
            .await;

        // Create 25 records
        for i in 0..25 {
            let _ = module
                .handle_command(
                    "data/create",
                    json!({
                        "dbPath": ":memory:",
                        "collection": "test_paginated",
                        "data": { "name": format!("Item {}", i) }
                    }),
                )
                .await;
        }

        // Open paginated query with page size 10
        let open_result = module
            .handle_command(
                "data/query-open",
                json!({
                    "dbPath": ":memory:",
                    "collection": "test_paginated",
                    "pageSize": 10
                }),
            )
            .await;

        assert!(open_result.is_ok());
        let query_id = if let Ok(CommandResult::Json(result)) = &open_result {
            let data = &result["data"];
            assert_eq!(data["totalCount"], 25);
            assert_eq!(data["pageSize"], 10);
            assert!(data["hasMore"].as_bool().unwrap());
            data["queryId"].as_str().unwrap().to_string()
        } else {
            panic!("Expected JSON result");
        };

        // Get first page
        let page1 = module
            .handle_command(
                "data/query-next",
                json!({ "queryId": query_id }),
            )
            .await;

        assert!(page1.is_ok());
        if let Ok(CommandResult::Json(result)) = &page1 {
            let data = &result["data"];
            assert_eq!(data["items"].as_array().unwrap().len(), 10);
            assert_eq!(data["pageNumber"], 1);
            assert!(data["hasMore"].as_bool().unwrap());
        }

        // Get second page
        let page2 = module
            .handle_command(
                "data/query-next",
                json!({ "queryId": query_id }),
            )
            .await;

        assert!(page2.is_ok());
        if let Ok(CommandResult::Json(result)) = &page2 {
            let data = &result["data"];
            assert_eq!(data["items"].as_array().unwrap().len(), 10);
            assert_eq!(data["pageNumber"], 2);
            assert!(data["hasMore"].as_bool().unwrap());
        }

        // Get third page (should have 5 items)
        let page3 = module
            .handle_command(
                "data/query-next",
                json!({ "queryId": query_id }),
            )
            .await;

        assert!(page3.is_ok());
        if let Ok(CommandResult::Json(result)) = &page3 {
            let data = &result["data"];
            assert_eq!(data["items"].as_array().unwrap().len(), 5);
            assert_eq!(data["pageNumber"], 3);
            assert!(!data["hasMore"].as_bool().unwrap()); // No more pages
        }

        // Close query
        let close_result = module
            .handle_command(
                "data/query-close",
                json!({ "queryId": query_id }),
            )
            .await;

        assert!(close_result.is_ok());
        if let Ok(CommandResult::Json(result)) = close_result {
            assert!(result["success"].as_bool().unwrap());
        }
    }

    #[tokio::test]
    async fn test_backfill_vectors() {
        let module = DataModule::new();

        // Create schema with content and embedding fields
        let schema = CollectionSchema {
            collection: "test_backfill".to_string(),
            fields: vec![
                crate::orm::types::SchemaField {
                    name: "content".to_string(),
                    field_type: crate::orm::types::FieldType::String,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                },
                crate::orm::types::SchemaField {
                    name: "embedding".to_string(),
                    field_type: crate::orm::types::FieldType::Json,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                },
            ],
            indexes: vec![],
        };

        let _ = module
            .handle_command(
                "data/ensure-schema",
                json!({
                    "dbPath": ":memory:",
                    "schema": schema
                }),
            )
            .await;

        // Create records without embeddings
        for i in 0..5 {
            let _ = module
                .handle_command(
                    "data/create",
                    json!({
                        "dbPath": ":memory:",
                        "collection": "test_backfill",
                        "data": { "content": format!("Test content number {}", i) }
                    }),
                )
                .await;
        }

        // Run backfill
        let backfill_result = module
            .handle_command(
                "vector/backfill",
                json!({
                    "dbPath": ":memory:",
                    "collection": "test_backfill",
                    "textField": "content",
                    "batchSize": 10
                }),
            )
            .await;

        assert!(backfill_result.is_ok(), "Backfill should succeed");

        if let Ok(CommandResult::Json(result)) = backfill_result {
            assert!(result["success"].as_bool().unwrap_or(false));
            let data = &result["data"];
            assert_eq!(data["total"].as_u64().unwrap(), 5);
            assert_eq!(data["processed"].as_u64().unwrap(), 5);
            assert_eq!(data["failed"].as_u64().unwrap(), 0);
        }

        // Verify embeddings were added
        let stats_result = module
            .handle_command(
                "vector/stats",
                json!({
                    "dbPath": ":memory:",
                    "collection": "test_backfill"
                }),
            )
            .await;

        assert!(stats_result.is_ok());
        if let Ok(CommandResult::Json(result)) = stats_result {
            let stats = &result["data"];
            assert_eq!(stats["recordsWithVectors"].as_u64().unwrap(), 5);
            assert!(stats["vectorDimensions"].as_u64().unwrap() > 0);
        }
    }

    #[test]
    fn test_cosine_similarity() {
        // Test identical vectors
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![1.0, 0.0, 0.0];
        let sim = DataModule::cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.001, "Identical vectors should have similarity 1.0");

        // Test orthogonal vectors
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0];
        let sim = DataModule::cosine_similarity(&a, &b);
        assert!(sim.abs() < 0.001, "Orthogonal vectors should have similarity 0.0");

        // Test opposite vectors
        let a = vec![1.0, 0.0, 0.0];
        let b = vec![-1.0, 0.0, 0.0];
        let sim = DataModule::cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 0.001, "Opposite vectors should have similarity -1.0");

        // Test with 384-dimension vectors (typical embedding size)
        let a: Vec<f64> = (0..384).map(|i| (i as f64) * 0.01).collect();
        let b: Vec<f64> = (0..384).map(|i| (i as f64) * 0.01).collect();
        let sim = DataModule::cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 0.001, "Identical 384-dim vectors should have similarity 1.0");
    }
}
