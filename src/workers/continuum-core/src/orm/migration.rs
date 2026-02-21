//! Migration Engine — Streaming data transfer between ANY two StorageAdapters
//!
//! Works between any adapter pairing: SQLite→Postgres, Postgres→SQLite,
//! SQLite→SQLite, future→future. Only uses the StorageAdapter trait,
//! so any adapter works for free.
//!
//! Features:
//! - Cursor-based streaming (no full dataset in memory)
//! - Per-collection progress tracking with atomic counters
//! - Pause/resume support
//! - Non-destructive (source data is never deleted)
//! - Configurable batch size and throttle

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use ts_rs::TS;

use super::adapter::StorageAdapter;
use super::query::StorageQuery;
use super::types::{DataRecord, RecordMetadata};

/// Migration configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MigrationConfig {
    /// Records per batch (default: 500)
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Milliseconds to pause between batches (default: 10)
    #[serde(default = "default_throttle_ms")]
    pub throttle_ms: u64,
    /// Optional list of collections to migrate (None = all)
    pub collections: Option<Vec<String>>,
}

fn default_batch_size() -> usize { 500 }
fn default_throttle_ms() -> u64 { 10 }

impl Default for MigrationConfig {
    fn default() -> Self {
        Self {
            batch_size: 500,
            throttle_ms: 10,
            collections: None,
        }
    }
}

/// Per-collection migration status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/MigrationStatus.ts")]
#[serde(rename_all = "lowercase")]
pub enum MigrationStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
    Paused,
}

/// Per-collection migration progress (thread-safe atomics)
pub struct CollectionMigrationState {
    pub collection: String,
    pub status: std::sync::RwLock<MigrationStatus>,
    pub total: AtomicUsize,
    pub migrated: AtomicUsize,
    pub failed: AtomicUsize,
    pub error: std::sync::RwLock<Option<String>>,
}

impl CollectionMigrationState {
    fn new(collection: String, total: usize) -> Self {
        Self {
            collection,
            status: std::sync::RwLock::new(MigrationStatus::Pending),
            total: AtomicUsize::new(total),
            migrated: AtomicUsize::new(0),
            failed: AtomicUsize::new(0),
            error: std::sync::RwLock::new(None),
        }
    }

    fn to_json(&self) -> Value {
        json!({
            "collection": self.collection,
            "status": format!("{:?}", *self.status.read().unwrap()).to_lowercase(),
            "total": self.total.load(Ordering::Relaxed),
            "migrated": self.migrated.load(Ordering::Relaxed),
            "failed": self.failed.load(Ordering::Relaxed),
            "error": *self.error.read().unwrap(),
        })
    }
}

/// Shared migration state — safe to read/pause while engine is running.
/// All fields use atomic or RwLock for lock-free concurrent access.
#[derive(Clone)]
pub struct MigrationHandle {
    states: Arc<std::sync::RwLock<HashMap<String, Arc<CollectionMigrationState>>>>,
    paused: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    source: Arc<dyn StorageAdapter>,
    target: Arc<dyn StorageAdapter>,
}

impl MigrationHandle {
    /// Get current migration status as JSON (lock-free atomics)
    pub fn status_json(&self) -> Value {
        let states = self.states.read().unwrap();
        let collections: Vec<Value> = states.values().map(|s| s.to_json()).collect();
        let total: usize = states.values().map(|s| s.total.load(Ordering::Relaxed)).sum();
        let migrated: usize = states.values().map(|s| s.migrated.load(Ordering::Relaxed)).sum();
        let failed: usize = states.values().map(|s| s.failed.load(Ordering::Relaxed)).sum();

        json!({
            "total": total,
            "migrated": migrated,
            "failed": failed,
            "paused": self.paused.load(Ordering::Relaxed),
            "running": self.running.load(Ordering::Relaxed),
            "collections": collections,
        })
    }

    /// Pause the migration (atomic flag — no lock needed)
    pub fn pause(&self) {
        self.paused.store(true, Ordering::Relaxed);
    }

    /// Resume the migration (atomic flag — caller must call engine.run() again)
    pub fn resume(&self) {
        self.paused.store(false, Ordering::Relaxed);
    }

    /// Whether the migration is still running
    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    /// Verify migration by comparing record counts
    pub async fn verify(&self) -> Result<Value, String> {
        let mut results = Vec::new();
        let mut all_match = true;

        let states = self.states.read().unwrap().clone();
        for (collection, state) in &states {
            let source_count = self
                .source
                .count(StorageQuery {
                    collection: collection.clone(),
                    ..Default::default()
                })
                .await;
            let target_count = self
                .target
                .count(StorageQuery {
                    collection: collection.clone(),
                    ..Default::default()
                })
                .await;

            let src = source_count.data.unwrap_or(0);
            let tgt = target_count.data.unwrap_or(0);
            let matches = src == tgt;
            if !matches {
                all_match = false;
            }

            results.push(json!({
                "collection": collection,
                "sourceCount": src,
                "targetCount": tgt,
                "migrated": state.migrated.load(Ordering::Relaxed),
                "failed": state.failed.load(Ordering::Relaxed),
                "matches": matches,
            }));
        }

        Ok(json!({
            "verified": all_match,
            "collections": results,
        }))
    }
}

/// Migration engine — streams data between any two adapters
pub struct MigrationEngine {
    source: Arc<dyn StorageAdapter>,
    target: Arc<dyn StorageAdapter>,
    config: MigrationConfig,
    states: Arc<std::sync::RwLock<HashMap<String, Arc<CollectionMigrationState>>>>,
    paused: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
}

impl MigrationEngine {
    /// Create a new migration engine
    pub fn new(
        source: Arc<dyn StorageAdapter>,
        target: Arc<dyn StorageAdapter>,
        config: MigrationConfig,
    ) -> Self {
        Self {
            source,
            target,
            config,
            states: Arc::new(std::sync::RwLock::new(HashMap::new())),
            paused: Arc::new(AtomicBool::new(false)),
            running: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Get a lightweight handle for status/pause/verify (safe to use while engine runs)
    pub fn handle(&self) -> MigrationHandle {
        MigrationHandle {
            states: self.states.clone(),
            paused: self.paused.clone(),
            running: self.running.clone(),
            source: self.source.clone(),
            target: self.target.clone(),
        }
    }

    /// Run the full migration
    pub async fn run(&mut self) -> Result<Value, String> {
        self.running.store(true, Ordering::Relaxed);

        // Step 1: Discover collections
        let collections = match &self.config.collections {
            Some(explicit) => explicit.clone(),
            None => {
                let result = self.source.list_collections().await;
                result.data.ok_or_else(|| {
                    self.running.store(false, Ordering::Relaxed);
                    result.error.unwrap_or_else(|| "Failed to list source collections".into())
                })?
            }
        };

        // Step 2: Count records per collection, initialize state
        // Collect counts first (async), then insert under write lock (sync)
        let mut counted: Vec<(String, usize)> = Vec::new();
        for collection in &collections {
            let count = self
                .source
                .count(StorageQuery {
                    collection: collection.clone(),
                    ..Default::default()
                })
                .await;
            counted.push((collection.clone(), count.data.unwrap_or(0)));
        }
        {
            let mut states = self.states.write().unwrap();
            for (collection, total) in counted {
                let state = Arc::new(CollectionMigrationState::new(collection.clone(), total));
                states.insert(collection, state);
            }
        }

        // Step 3: Migrate each collection
        for collection in &collections {
            if self.paused.load(Ordering::Relaxed) {
                break;
            }
            if let Err(e) = self.migrate_collection(collection).await {
                self.running.store(false, Ordering::Relaxed);
                return Err(e);
            }
        }

        self.running.store(false, Ordering::Relaxed);
        Ok(self.handle().status_json())
    }

    /// Migrate a single collection with streaming pagination
    async fn migrate_collection(&self, collection: &str) -> Result<(), String> {
        let state = {
            let states = self.states.read().unwrap();
            states.get(collection)
                .ok_or_else(|| format!("No state for collection: {}", collection))?
                .clone()
        };

        *state.status.write().unwrap() = MigrationStatus::InProgress;

        // Ensure target schema exists by reading first record
        let total = state.total.load(Ordering::Relaxed);
        if total == 0 {
            *state.status.write().unwrap() = MigrationStatus::Completed;
            return Ok(());
        }

        let batch_size = self.config.batch_size;
        let mut offset = state.migrated.load(Ordering::Relaxed); // Resume from last position

        loop {
            if self.paused.load(Ordering::Relaxed) {
                *state.status.write().unwrap() = MigrationStatus::Paused;
                return Ok(());
            }

            // Fetch a batch from source
            let query = StorageQuery {
                collection: collection.to_string(),
                limit: Some(batch_size),
                offset: Some(offset),
                ..Default::default()
            };

            let result = self.source.query(query).await;
            let records = match result.data {
                Some(r) if !r.is_empty() => r,
                Some(_) => break, // Empty page = done
                None => {
                    let err = result.error.unwrap_or_else(|| "Source query failed".into());
                    *state.error.write().unwrap() = Some(err.clone());
                    *state.status.write().unwrap() = MigrationStatus::Failed;
                    return Err(err);
                }
            };

            let batch_count = records.len();

            // Insert batch into target
            for record in records {
                let target_record = DataRecord {
                    id: record.id.clone(),
                    collection: collection.to_string(),
                    data: record.data.clone(),
                    metadata: RecordMetadata {
                        created_at: record.metadata.created_at.clone(),
                        updated_at: record.metadata.updated_at.clone(),
                        version: record.metadata.version,
                        tags: record.metadata.tags.clone(),
                        schema: record.metadata.schema.clone(),
                        ttl: record.metadata.ttl,
                    },
                };

                let insert = self.target.create(target_record).await;
                if insert.success {
                    state.migrated.fetch_add(1, Ordering::Relaxed);
                } else {
                    let fail_count = state.failed.fetch_add(1, Ordering::Relaxed);
                    // Log first failures per collection (cap at 10 to avoid noise)
                    if fail_count < 10 {
                        if let Some(ref err) = insert.error {
                            eprintln!("[MIGRATION] Failed {}/{}: {}", collection, record.id, err);
                        }
                    }
                }
            }

            offset += batch_count;

            // Throttle between batches
            if self.config.throttle_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(self.config.throttle_ms)).await;
            }

            // If we got fewer than batch_size, we're done
            if batch_count < batch_size {
                break;
            }
        }

        *state.status.write().unwrap() = MigrationStatus::Completed;
        Ok(())
    }

    /// Convenience: get current status
    pub fn status_json(&self) -> Value {
        self.handle().status_json()
    }

    /// Convenience: pause
    pub fn pause(&self) {
        self.paused.store(true, Ordering::Relaxed);
    }

    /// Convenience: resume
    pub fn resume(&self) {
        self.paused.store(false, Ordering::Relaxed);
    }

    /// Convenience: verify
    pub async fn verify(&self) -> Result<Value, String> {
        self.handle().verify().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orm::sqlite::SqliteAdapter;
    use crate::orm::adapter::AdapterConfig;
    use crate::orm::types::{CollectionSchema, SchemaField, FieldType};

    async fn setup_sqlite(path: &str) -> Arc<dyn StorageAdapter> {
        let mut adapter = SqliteAdapter::new();
        adapter
            .initialize(AdapterConfig {
                connection_string: path.to_string(),
                ..Default::default()
            })
            .await
            .unwrap();
        Arc::new(adapter) as Arc<dyn StorageAdapter>
    }

    #[tokio::test]
    async fn test_migration_sqlite_to_sqlite() {
        let dir = tempfile::tempdir().unwrap();
        let source_path = dir.path().join("source.db");
        let target_path = dir.path().join("target.db");

        let source = setup_sqlite(source_path.to_str().unwrap()).await;
        let target = setup_sqlite(target_path.to_str().unwrap()).await;

        // Create schema + data in source
        source
            .ensure_schema(CollectionSchema {
                collection: "items".to_string(),
                fields: vec![SchemaField {
                    name: "name".to_string(),
                    field_type: FieldType::String,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                }],
                indexes: vec![],
            })
            .await;

        for i in 0..25 {
            source
                .create(DataRecord {
                    id: format!("item-{}", i),
                    collection: "items".to_string(),
                    data: json!({"name": format!("Item {}", i)}),
                    metadata: RecordMetadata::default(),
                })
                .await;
        }

        // Run migration
        let mut engine = MigrationEngine::new(
            source.clone(),
            target.clone(),
            MigrationConfig {
                batch_size: 10,
                throttle_ms: 0,
                collections: None,
            },
        );

        let status = engine.run().await.unwrap();
        assert_eq!(status["total"], 25);
        assert_eq!(status["migrated"], 25);
        assert_eq!(status["failed"], 0);

        // Verify
        let verify = engine.verify().await.unwrap();
        assert!(verify["verified"].as_bool().unwrap());

        // Check target has data
        let target_count = target
            .count(StorageQuery {
                collection: "items".to_string(),
                ..Default::default()
            })
            .await;
        assert_eq!(target_count.data.unwrap(), 25);
    }

    #[tokio::test]
    async fn test_migration_pause_and_resume() {
        let dir = tempfile::tempdir().unwrap();
        let source_path = dir.path().join("source_pause.db");
        let target_path = dir.path().join("target_pause.db");

        let source = setup_sqlite(source_path.to_str().unwrap()).await;
        let target = setup_sqlite(target_path.to_str().unwrap()).await;

        source
            .ensure_schema(CollectionSchema {
                collection: "items".to_string(),
                fields: vec![SchemaField {
                    name: "val".to_string(),
                    field_type: FieldType::String,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                }],
                indexes: vec![],
            })
            .await;

        for i in 0..10 {
            source
                .create(DataRecord {
                    id: format!("p-{}", i),
                    collection: "items".to_string(),
                    data: json!({"val": format!("V{}", i)}),
                    metadata: RecordMetadata::default(),
                })
                .await;
        }

        let mut engine = MigrationEngine::new(
            source.clone(),
            target.clone(),
            MigrationConfig {
                batch_size: 3,
                throttle_ms: 0,
                collections: None,
            },
        );

        // Pre-pause before running
        engine.pause();
        let status = engine.run().await.unwrap();
        // Should have paused before migrating (or after first batch)
        assert!(status["paused"].as_bool().unwrap());

        // Resume and complete
        engine.resume();
        let status = engine.run().await.unwrap();
        assert_eq!(status["migrated"], 10);
    }

    #[tokio::test]
    async fn test_migration_empty_source() {
        let dir = tempfile::tempdir().unwrap();
        let source_path = dir.path().join("empty_src.db");
        let target_path = dir.path().join("empty_tgt.db");

        let source = setup_sqlite(source_path.to_str().unwrap()).await;
        let target = setup_sqlite(target_path.to_str().unwrap()).await;

        source
            .ensure_schema(CollectionSchema {
                collection: "empty".to_string(),
                fields: vec![],
                indexes: vec![],
            })
            .await;

        let mut engine = MigrationEngine::new(
            source.clone(),
            target.clone(),
            MigrationConfig::default(),
        );

        let status = engine.run().await.unwrap();
        assert_eq!(status["total"], 0);
        assert_eq!(status["migrated"], 0);
    }

    #[tokio::test]
    async fn test_migration_verify() {
        let dir = tempfile::tempdir().unwrap();
        let source_path = dir.path().join("verify_src.db");
        let target_path = dir.path().join("verify_tgt.db");

        let source = setup_sqlite(source_path.to_str().unwrap()).await;
        let target = setup_sqlite(target_path.to_str().unwrap()).await;

        source
            .ensure_schema(CollectionSchema {
                collection: "verifyable".to_string(),
                fields: vec![SchemaField {
                    name: "x".to_string(),
                    field_type: FieldType::Number,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                }],
                indexes: vec![],
            })
            .await;

        for i in 0..5 {
            source
                .create(DataRecord {
                    id: format!("v-{}", i),
                    collection: "verifyable".to_string(),
                    data: json!({"x": i}),
                    metadata: RecordMetadata::default(),
                })
                .await;
        }

        let mut engine = MigrationEngine::new(
            source.clone(),
            target.clone(),
            MigrationConfig {
                batch_size: 100,
                throttle_ms: 0,
                collections: None,
            },
        );

        engine.run().await.unwrap();

        let verify = engine.verify().await.unwrap();
        assert!(verify["verified"].as_bool().unwrap());
        let cols = verify["collections"].as_array().unwrap();
        assert_eq!(cols[0]["sourceCount"], 5);
        assert_eq!(cols[0]["targetCount"], 5);
        assert!(cols[0]["matches"].as_bool().unwrap());
    }
}
