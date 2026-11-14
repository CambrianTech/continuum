//! Migration Engine — Streaming data transfer between ANY two StorageAdapters
//!
//! Works between any adapter pairing: SQLite→SQLite, future→future. Only uses the StorageAdapter trait,
//! so any adapter works for free.
//!
//! Features:
//! - Cursor-based streaming (no full dataset in memory)
//! - Per-collection progress tracking with atomic counters
//! - Pause/resume support
//! - Non-destructive (source data is never deleted)
//! - Configurable batch size and throttle

use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use ts_rs::TS;

use super::adapter::StorageAdapter;
use super::query::StorageQuery;
#[cfg(test)]
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

fn default_batch_size() -> usize {
    500
}
fn default_throttle_ms() -> u64 {
    10
}

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
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/MigrationStatus.ts"
)]
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
    pub status: parking_lot::RwLock<MigrationStatus>,
    pub total: AtomicUsize,
    pub migrated: AtomicUsize,
    pub failed: AtomicUsize,
    pub error: parking_lot::RwLock<Option<String>>,
}

impl CollectionMigrationState {
    fn new(collection: String, total: usize) -> Self {
        Self {
            collection,
            status: parking_lot::RwLock::new(MigrationStatus::Pending),
            total: AtomicUsize::new(total),
            migrated: AtomicUsize::new(0),
            failed: AtomicUsize::new(0),
            error: parking_lot::RwLock::new(None),
        }
    }

    fn to_progress(&self) -> CollectionProgress {
        CollectionProgress {
            collection: self.collection.clone(),
            status: format!("{:?}", *self.status.read()).to_lowercase(),
            total: self.total.load(Ordering::Relaxed),
            migrated: self.migrated.load(Ordering::Relaxed),
            failed: self.failed.load(Ordering::Relaxed),
            error: self.error.read().clone(),
        }
    }
}

/// Per-collection migration progress snapshot — the typed wire row behind
/// `migration/{start,status,pause,resume}`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/CollectionProgress.ts"
)]
pub struct CollectionProgress {
    pub collection: String,
    /// Lowercased status: `pending` | `inprogress` | `completed` | `failed` | `paused`.
    pub status: String,
    pub total: usize,
    pub migrated: usize,
    pub failed: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
}

/// Whole-migration progress snapshot across every collection — the typed result
/// of `migration/{start,status,pause,resume}` (replaces the old ad-hoc JSON blob).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/MigrationProgress.ts"
)]
pub struct MigrationProgress {
    pub total: usize,
    pub migrated: usize,
    pub failed: usize,
    pub paused: bool,
    pub running: bool,
    pub collections: Vec<CollectionProgress>,
}

/// Per-collection source/target count comparison from `migration/verify`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/CollectionVerification.ts"
)]
pub struct CollectionVerification {
    pub collection: String,
    pub source_count: usize,
    pub target_count: usize,
    pub migrated: usize,
    pub failed: usize,
    pub matches: bool,
}

/// Result of `migration/verify` — whether every migrated collection's target
/// record count matches its source.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/MigrationVerification.ts"
)]
pub struct MigrationVerification {
    pub verified: bool,
    pub collections: Vec<CollectionVerification>,
}

/// Shared migration state — safe to read/pause while engine is running.
/// All fields use atomic or RwLock for lock-free concurrent access.
#[derive(Clone)]
pub struct MigrationHandle {
    states: Arc<parking_lot::RwLock<HashMap<String, Arc<CollectionMigrationState>>>>,
    paused: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
    source: Arc<dyn StorageAdapter>,
    target: Arc<dyn StorageAdapter>,
}

impl MigrationHandle {
    /// Get current migration progress (lock-free atomics) as a typed snapshot.
    pub fn status(&self) -> MigrationProgress {
        let states = self.states.read();
        let collections: Vec<CollectionProgress> =
            states.values().map(|s| s.to_progress()).collect();
        let total: usize = states
            .values()
            .map(|s| s.total.load(Ordering::Relaxed))
            .sum();
        let migrated: usize = states
            .values()
            .map(|s| s.migrated.load(Ordering::Relaxed))
            .sum();
        let failed: usize = states
            .values()
            .map(|s| s.failed.load(Ordering::Relaxed))
            .sum();

        MigrationProgress {
            total,
            migrated,
            failed,
            paused: self.paused.load(Ordering::Relaxed),
            running: self.running.load(Ordering::Relaxed),
            collections,
        }
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

    /// Verify receipts, schemas and exact scalar/native values in bounded snapshots.
    /// Offline callers must keep writers quiescent across collections and adoption.
    pub async fn verify(&self) -> Result<MigrationVerification, String> {
        let mut results = Vec::new();
        let states = self.states.read().clone();
        // No collection receipts means no migration evidence, not successful
        // verification (including a failed discovery/count before initialization).
        let mut all_match = !states.is_empty();
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

            let src = source_count.into_result()?;
            let tgt = target_count.into_result()?;
            let receipts_match = src == tgt
                && *state.status.read() == MigrationStatus::Completed
                && state.failed.load(Ordering::Relaxed) == 0
                && state.migrated.load(Ordering::Relaxed) == src;
            let matches = if receipts_match {
                let source = self.source.binary_snapshot(collection).await?;
                let target = self.target.binary_snapshot(collection).await?;
                // A transfer-page bound, not a limit on verified rows.
                source.migration_matches(&target, std::num::NonZeroUsize::new(64)
                    .ok_or("invalid verification page size")?).await?
            } else { false };
            if !matches {
                all_match = false;
            }

            results.push(CollectionVerification {
                collection: collection.clone(),
                source_count: src,
                target_count: tgt,
                migrated: state.migrated.load(Ordering::Relaxed),
                failed: state.failed.load(Ordering::Relaxed),
                matches,
            });
        }

        Ok(MigrationVerification {
            verified: all_match,
            collections: results,
        })
    }
}

/// Migration engine — streams data between any two adapters
pub struct MigrationEngine {
    source: Arc<dyn StorageAdapter>,
    target: Arc<dyn StorageAdapter>,
    config: MigrationConfig,
    states: Arc<parking_lot::RwLock<HashMap<String, Arc<CollectionMigrationState>>>>,
    paused: Arc<AtomicBool>,
    running: Arc<AtomicBool>,
}

#[cfg(test)]
#[path = "migration_native_tests.rs"]
mod native_tests;

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
            states: Arc::new(parking_lot::RwLock::new(HashMap::new())),
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
    pub async fn run(&mut self) -> Result<MigrationProgress, String> {
        self.running.store(true, Ordering::Relaxed);

        // Step 1: Discover collections
        let collections = match &self.config.collections {
            Some(explicit) => explicit.clone(),
            None => {
                let result = self.source.list_collections().await;
                result.data.ok_or_else(|| {
                    self.running.store(false, Ordering::Relaxed);
                    result
                        .error
                        .unwrap_or_else(|| "Failed to list source collections".into())
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
            let total = match count.into_result() {
                Ok(total) => total,
                Err(error) => {
                    self.running.store(false, Ordering::Relaxed);
                    return Err(format!("count source {collection}: {error}"));
                }
            };
            counted.push((collection.clone(), total));
        }
        {
            let mut states = self.states.write();
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
        Ok(self.handle().status())
    }

    /// Migrate a single collection with streaming pagination
    async fn migrate_collection(&self, collection: &str) -> Result<(), String> {
        let state = {
            let states = self.states.read();
            states
                .get(collection)
                .ok_or_else(|| format!("No state for collection: {}", collection))?
                .clone()
        };

        *state.status.write() = MigrationStatus::InProgress;

        // Copy the admitted entity contract even when the collection is empty.
        // Constraints and codecs come from declarations, never the first row.
        let schema = self.source.collection_schema(collection).into_result()?;
        self.target.ensure_schema(schema).await.into_result()?;
        let total = state.total.load(Ordering::Relaxed);
        if total == 0 {
            *state.status.write() = MigrationStatus::Completed;
            return Ok(());
        }

        let batch_size = std::num::NonZeroUsize::new(self.config.batch_size)
            .ok_or("migration batch size must be positive")?;
        let snapshot = self.source.binary_snapshot(collection).await?;
        let mut after: Option<String> = None;

        loop {
            if self.paused.load(Ordering::Relaxed) {
                *state.status.write() = MigrationStatus::Paused;
                return Ok(());
            }

            let records = match snapshot.migration_page(&super::migration_snapshot::MigrationPage {
                after: after.as_deref(), limit: batch_size,
            }).await {
                Ok(rows) if rows.is_empty() => break,
                Ok(rows) => rows,
                Err(err) => {
                    *state.error.write() = Some(err.clone());
                    *state.status.write() = MigrationStatus::Failed;
                    return Err(err);
                }
            };

            let batch_count = records.len();

            // Insert batch into target
            for row in records {
                let next = row.record.id.clone();
                // Move the owned row into the destination; a rejected write is not
                // migrated work and must never be followed by Completed/verified.
                let inserted = if row.payloads.is_empty() {
                    self.target.create(row.record).await.into_result().map(|_| ())
                } else {
                    self.target.create_binary(row.record, row.payloads).await.into_result().map(|_| ())
                };
                if let Err(error) = inserted {
                    state.failed.fetch_add(1, Ordering::Relaxed);
                    *state.error.write() = Some(error.clone());
                    *state.status.write() = MigrationStatus::Failed;
                    return Err(error);
                }
                state.migrated.fetch_add(1, Ordering::Relaxed);
                after = Some(next);
            }

            // Throttle between batches
            if self.config.throttle_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(self.config.throttle_ms)).await;
            }

            // If we got fewer than batch_size, we're done
            if batch_count < batch_size.get() {
                break;
            }
        }

        *state.status.write() = MigrationStatus::Completed;
        Ok(())
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
    pub async fn verify(&self) -> Result<MigrationVerification, String> {
        self.handle().verify().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orm::adapter::AdapterConfig;
    use crate::orm::SeaOrmAdapter;
    use crate::orm::types::{CollectionSchema, FieldType, SchemaField};

    #[tokio::test]
    async fn equal_counts_cannot_verify_a_rejected_migration() -> Result<(), String> {
        let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
        let source = setup_sqlite(&dir.path().join("source.db").to_string_lossy()).await;
        let target = setup_sqlite(&dir.path().join("target.db").to_string_lossy()).await;
        let schema = CollectionSchema {
            collection: "items".into(),
            fields: crate::orm::base_entity_fields(),
            indexes: vec![],
        };
        for adapter in [&source, &target] {
            adapter.ensure_schema(schema.clone()).await.into_result()?;
            adapter.create(DataRecord {
                id: "collision".into(), collection: "items".into(),
                data: json!({}), metadata: RecordMetadata::default(),
            }).await.into_result()?;
        }
        let mut engine = MigrationEngine::new(source, target, MigrationConfig {
            batch_size: 1, throttle_ms: 0, collections: Some(vec!["items".into()]),
        });
        assert!(!engine.verify().await?.verified, "no run has no migration receipts");
        assert!(engine.run().await.is_err(), "a destination refusal stops migration");
        let status = engine.handle().status();
        assert_eq!(status.migrated, 0);
        assert_eq!(status.failed, 1);
        assert!(!engine.handle().is_running());
        let verification = engine.verify().await?;
        assert!(!verification.verified, "equal counts cannot erase the refusal");
        assert_eq!(verification.collections[0].source_count, 1);
        assert_eq!(verification.collections[0].target_count, 1);
        Ok(())
    }

    async fn setup_sqlite(path: &str) -> Arc<dyn StorageAdapter> {
        let adapter = SeaOrmAdapter::connect(AdapterConfig {
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
                fields: crate::orm::base_entity_fields().into_iter().chain([SchemaField {
                default_value: None,
                    name: "name".to_string(),
                    field_type: FieldType::String,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                    foreign_key: None,
                }]).collect(),
                indexes: vec![],
            })
            .await.into_result().expect("test: storage operation succeeds");

        for i in 0..25 {
            source
                .create(DataRecord {
                    id: format!("item-{}", i),
                    collection: "items".to_string(),
                    data: json!({"name": format!("Item {}", i)}),
                    metadata: RecordMetadata::default(),
                })
                .await.into_result().expect("test: storage operation succeeds");
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
        assert_eq!(status.total, 25);
        assert_eq!(status.migrated, 25);
        assert_eq!(status.failed, 0);

        // Verify
        let verify = engine.verify().await.unwrap();
        assert!(verify.verified);

        // Check target has data
        let target_count = target
            .count(StorageQuery {
                collection: "items".to_string(),
                ..Default::default()
            })
            .await.into_result().expect("test: storage operation succeeds");
        assert_eq!(target_count, 25);
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
                fields: crate::orm::base_entity_fields().into_iter().chain([SchemaField {
                default_value: None,
                    name: "val".to_string(),
                    field_type: FieldType::String,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                    foreign_key: None,
                }]).collect(),
                indexes: vec![],
            })
            .await.into_result().expect("test: storage operation succeeds");

        for i in 0..10 {
            source
                .create(DataRecord {
                    id: format!("p-{}", i),
                    collection: "items".to_string(),
                    data: json!({"val": format!("V{}", i)}),
                    metadata: RecordMetadata::default(),
                })
                .await.into_result().expect("test: storage operation succeeds");
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
        assert!(status.paused);

        // Resume and complete
        engine.resume();
        let status = engine.run().await.unwrap();
        assert_eq!(status.migrated, 10);
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
                fields: crate::orm::base_entity_fields(),
                indexes: vec![],
            })
            .await.into_result().expect("test: storage operation succeeds");

        let mut engine =
            MigrationEngine::new(source.clone(), target.clone(), MigrationConfig::default());

        let status = engine.run().await.unwrap();
        assert_eq!(status.total, 0);
        assert_eq!(status.migrated, 0);
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
                fields: crate::orm::base_entity_fields().into_iter().chain([SchemaField {
                default_value: None,
                    name: "x".to_string(),
                    field_type: FieldType::Number,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                    foreign_key: None,
                }]).collect(),
                indexes: vec![],
            })
            .await.into_result().expect("test: storage operation succeeds");

        for i in 0..5 {
            source
                .create(DataRecord {
                    id: format!("v-{}", i),
                    collection: "verifyable".to_string(),
                    data: json!({"x": i}),
                    metadata: RecordMetadata::default(),
                })
                .await.into_result().expect("test: storage operation succeeds");
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
        assert!(verify.verified);
        assert_eq!(verify.collections[0].source_count, 5);
        assert_eq!(verify.collections[0].target_count, 5);
        assert!(verify.collections[0].matches);
    }
}
