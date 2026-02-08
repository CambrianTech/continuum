/// DataModule — Storage and ORM operations via the StorageAdapter trait.
///
/// Handles: data/* commands (create, read, update, delete, query, batch)
/// Uses the ORM module's StorageAdapter trait for database-agnostic operations.
///
/// CRITICAL: Database paths are ALWAYS passed by the caller (TypeScript handle layer).
/// NO defaults, NO environment variables, NO fallbacks. The caller owns the paths.

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
use serde::Deserialize;
use serde_json::{json, Value};
use std::any::Any;
use std::sync::Arc;
use tokio::sync::Mutex;

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
}

impl DataModule {
    pub fn new() -> Self {
        Self {
            adapters: DashMap::new(),
            init_lock: Mutex::new(()),
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
            command_prefixes: &["data/", "adapter/"],
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

impl DataModule {
    async fn handle_create(&self, params: Value) -> Result<CommandResult, String> {
        log_info!("data", "create", "[1] ENTER handle_create");
        let params: CreateParams =
            serde_json::from_value(params.clone()).map_err(|e| {
                log_error!("data", "create", "Parse error: {}, params: {}", e, params);
                format!("Invalid params: {e}")
            })?;

        let id = params.id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
        log_info!("data", "create", "[2] id={}, collection={}", id, params.collection);

        let record = DataRecord {
            id: id.clone(),
            collection: params.collection,
            data: params.data,
            metadata: RecordMetadata::default(),
        };

        log_info!("data", "create", "[3] calling get_adapter");
        let adapter = self.get_adapter(&params.db_path).await?;
        log_info!("data", "create", "[4] got adapter, calling create");
        let result = adapter.create(record).await;
        log_info!("data", "create", "[5] create done, success={}", result.success);

        Ok(CommandResult::Json(serde_json::to_value(result).unwrap()))
    }

    async fn handle_read(&self, params: Value) -> Result<CommandResult, String> {
        let params: ReadParams =
            serde_json::from_value(params).map_err(|e| format!("Invalid params: {e}"))?;

        let adapter = self.get_adapter(&params.db_path).await?;
                let result = adapter.read(&params.collection, &params.id).await;

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
        log_info!("data", "query", "Starting query handler");
        let params: QueryParams =
            serde_json::from_value(params.clone()).map_err(|e| {
                log_error!("data", "query", "Parse error: {}, params: {}", e, params);
                format!("Invalid params: {e}")
            })?;

        log_info!("data", "query", "Parsed params: collection={}, db_path={}", params.collection, params.db_path);

        let query = StorageQuery {
            collection: params.collection.clone(),
            filter: params.filter,
            sort: params.sort,
            limit: params.limit,
            offset: params.offset,
            ..Default::default()
        };

        log_info!("data", "query", "[3] Getting adapter for: {}", params.db_path);
        let adapter = self.get_adapter(&params.db_path).await?;
        log_info!("data", "query", "[4] Got adapter, executing query");
        let result = adapter.query(query).await;
        log_info!("data", "query", "[5] Query complete: success={}", result.success);

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
        let params: CountParams =
            serde_json::from_value(params).map_err(|e| format!("Invalid params: {e}"))?;

        let query = StorageQuery {
            collection: params.collection,
            filter: params.filter.map(|m| {
                m.into_iter()
                    .map(|(k, v)| (k, FieldFilter::Value(v)))
                    .collect()
            }),
            ..Default::default()
        };

        let adapter = self.get_adapter(&params.db_path).await?;
                let result = adapter.count(query).await;

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
