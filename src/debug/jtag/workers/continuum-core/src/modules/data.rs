//! DataModule — Storage and ORM operations via the StorageAdapter trait.
//!
//! Handles: data/* commands (create, read, update, delete, query, batch)
//! Also handles: vector/* commands (vector similarity search with in-memory caching)
//! Uses the ORM module's StorageAdapter trait for database-agnostic operations.
//!
//! CRITICAL: Database paths are ALWAYS passed by the caller (TypeScript handle layer).
//! NO defaults, NO environment variables, NO fallbacks. The caller owns the paths.

use crate::{log_error, log_info};
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
}

impl DataModule {
    pub fn new() -> Self {
        Self {
            adapters: DashMap::new(),
            init_lock: Mutex::new(()),
            vector_cache: RwLock::new(HashMap::new()),
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

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
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

            "adapter/capabilities" => self.handle_capabilities(params).await,
            "adapter/info" => self.handle_info(params).await,

            // Vector search (migrated from data-daemon-worker)
            "vector/search" => self.handle_vector_search(params).await,

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

        let adapter_start = Instant::now();
        let adapter = self.get_adapter(&params.db_path).await?;
        let adapter_ms = adapter_start.elapsed().as_millis();

        let create_start = Instant::now();
        let result = adapter.create(record).await;
        let create_ms = create_start.elapsed().as_millis();

        let total_ms = start.elapsed().as_millis();
        if total_ms > 50 {
            log_info!("data", "create", "TIMING: collection={}, total={}ms (adapter={}ms, create={}ms), success={}",
                collection, total_ms, adapter_ms, create_ms, result.success);
        }

        Ok(CommandResult::Json(serde_json::to_value(result).unwrap()))
    }

    async fn handle_read(&self, params: Value) -> Result<CommandResult, String> {
        use std::time::Instant;
        let start = Instant::now();

        let params: ReadParams =
            serde_json::from_value(params).map_err(|e| format!("Invalid params: {e}"))?;

        let adapter_start = Instant::now();
        let adapter = self.get_adapter(&params.db_path).await?;
        let adapter_ms = adapter_start.elapsed().as_millis();

        let read_start = Instant::now();
        let result = adapter.read(&params.collection, &params.id).await;
        let read_ms = read_start.elapsed().as_millis();

        let total_ms = start.elapsed().as_millis();
        if total_ms > 50 {
            log_info!("data", "read", "TIMING: collection={}, total={}ms (adapter={}ms, read={}ms), success={}",
                params.collection, total_ms, adapter_ms, read_ms, result.success);
        }

        Ok(CommandResult::Json(serde_json::to_value(result).unwrap()))
    }

    async fn handle_update(&self, params: Value) -> Result<CommandResult, String> {
        let params: UpdateParams =
            serde_json::from_value(params.clone()).map_err(|e| {
                log_error!("data", "update", "Parse error: {}, params: {}", e, params);
                format!("Invalid params: {e}")
            })?;

        let adapter = self.get_adapter(&params.db_path).await?;
                let result = adapter
            .update(
                &params.collection,
                &params.id,
                params.data,
                params.increment_version,
            )
            .await;

        Ok(CommandResult::Json(serde_json::to_value(result).unwrap()))
    }

    async fn handle_delete(&self, params: Value) -> Result<CommandResult, String> {
        let params: DeleteParams =
            serde_json::from_value(params).map_err(|e| format!("Invalid params: {e}"))?;

        let adapter = self.get_adapter(&params.db_path).await?;
                let result = adapter.delete(&params.collection, &params.id).await;

        Ok(CommandResult::Json(serde_json::to_value(result).unwrap()))
    }

    async fn handle_query(&self, params: Value) -> Result<CommandResult, String> {
        use std::time::Instant;
        let start = Instant::now();

        log_info!("data", "query", "Starting query handler");
        let params: QueryParams =
            serde_json::from_value(params.clone()).map_err(|e| {
                log_error!("data", "query", "Parse error: {}, params: {}", e, params);
                format!("Invalid params: {e}")
            })?;
        let parse_ms = start.elapsed().as_millis();

        log_info!("data", "query", "Parsed params: collection={}, db_path={} (parse: {}ms)",
            params.collection, params.db_path, parse_ms);

        let query = StorageQuery {
            collection: params.collection.clone(),
            filter: params.filter,
            sort: params.sort,
            limit: params.limit,
            offset: params.offset,
            ..Default::default()
        };

        let adapter_start = Instant::now();
        let adapter = self.get_adapter(&params.db_path).await?;
        let adapter_ms = adapter_start.elapsed().as_millis();

        let query_start = Instant::now();
        let result = adapter.query(query).await;
        let query_ms = query_start.elapsed().as_millis();

        let total_ms = start.elapsed().as_millis();

        // Log timing breakdown for slow queries (>50ms)
        if total_ms > 50 {
            log_info!("data", "query", "TIMING: collection={}, total={}ms (parse={}ms, adapter={}ms, query={}ms), success={}",
                params.collection, total_ms, parse_ms, adapter_ms, query_ms, result.success);
        }

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

        let adapter = self.get_adapter(&params.db_path).await?;
                let result = adapter.batch(params.operations).await;

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
}
