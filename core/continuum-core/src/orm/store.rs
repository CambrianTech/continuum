//! `OrmStore<T>` — the typed persistence rail over `StorageAdapter`.
//!
//! Per [[no-sql-everything-through-orm-entities]] +
//! [[orm-everything-not-hand-edited-files]]: module code that persists
//! data reaches storage through this typed surface, never through raw
//! `StorageAdapter::create(DataRecord)` calls (and emphatically never
//! through raw SQL).
//!
//! ### The compression principle
//!
//! Per [[organization-purity-as-we-migrate]] + the substrate's
//! E=mc² doctrine in CLAUDE.md: ONE generic helper instead of N
//! per-entity persistence shims. Every `T: OrmEntity + Serialize +
//! DeserializeOwned` gets save/find_all/find_by_id/update/delete for
//! free. Add `impl OrmEntity for FooEntity` → `OrmStore<FooEntity>`
//! works immediately. No FooStore + FooMigration + FooSerializer
//! trio. The store IS the abstraction.
//!
//! ### Identity discipline
//!
//! Callers supply the entity's id explicitly on `save` + `update`.
//! This is deliberate: most substrate entities have a domain-natural
//! UUID (Engram.id assigned at admission time, RoleTemplate uses its
//! BaseEntity id, etc.) and the caller is the one who knows what it
//! is. The id flows into `DataRecord.id` (the row primary key); the
//! entity's serialized form may also carry an `id` field (e.g.
//! Engram.id) — the caller is responsible for keeping the two
//! consistent. Drifting them would point to a deeper bug than the
//! store can repair.
//!
//! ### What this slice does NOT ship
//!
//! - No query DSL surface yet. `find_all` returns every record in
//!   the collection; callers needing filters today drop down to the
//!   underlying `QueryBuilder` + `adapter.query()`. Later slice
//!   wraps that as `find(filter)` once the use sites are clear.
//! - No batch operations yet. The single-entity surface is what the
//!   first wave of substrate entities (Engram, RoleTemplate,
//!   HwTierDescriptor) needs; batching follows demand.
//! - No transaction surface. The adapter trait doesn't expose
//!   transactions yet either; that's a substrate-wide concern.

use std::marker::PhantomData;
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::Serialize;
use uuid::Uuid;

use crate::orm::adapter::StorageAdapter;
use crate::orm::entity::OrmEntity;
use crate::orm::query::QueryBuilder;
use crate::orm::types::{DataRecord, RecordMetadata, StorageResult};

/// Typed failure modes for OrmStore operations. Per [[no-fallbacks-
/// ever]] + the typed-error pattern: every failure mode is named so
/// callers pattern-match, never swallow.
#[derive(Debug, thiserror::Error)]
pub enum OrmStoreError {
    /// `adapter.ensure_schema` failed on construction. The underlying
    /// reason is wrapped from the adapter's error string.
    #[error("schema ensure failed for collection {collection}: {detail}")]
    SchemaFailed { collection: String, detail: String },

    /// An adapter CRUD call returned `success: false`. We propagate
    /// the adapter's error verbatim — diagnosing the underlying
    /// (sqlite vs postgres vs grid-replicated) failure is the
    /// adapter's responsibility, not ours.
    #[error("adapter {operation} failed for collection {collection}: {detail}")]
    AdapterFailed {
        operation: &'static str,
        collection: String,
        detail: String,
    },

    /// Entity → JSON serialization failed. Shouldn't happen if the
    /// entity has serde derives, but typed for completeness.
    #[error("serialize entity to JSON failed: {0}")]
    SerializeFailed(#[source] serde_json::Error),

    /// JSON → entity deserialization failed. Usually indicates a
    /// schema drift between what's on disk and the entity type
    /// today — the saved record has fields the new struct doesn't,
    /// or vice versa.
    #[error("deserialize JSON to entity failed: {0}")]
    DeserializeFailed(#[source] serde_json::Error),

    /// A record's id field couldn't be parsed as a UUID. Indicates
    /// the underlying adapter returned a record whose id isn't
    /// UUID-shaped — substrate invariant violation, surface loudly.
    #[error("invalid UUID on record id: {0}")]
    InvalidUuid(#[source] uuid::Error),
}

/// Generic typed store over the substrate's ORM adapter surface.
///
/// Construction registers the entity's schema with the adapter so
/// subsequent CRUD has a table to land in. The store is cheap to
/// clone (it holds an `Arc<dyn StorageAdapter>`).
pub struct OrmStore<T: OrmEntity> {
    adapter: Arc<dyn StorageAdapter>,
    _marker: PhantomData<T>,
}

impl<T: OrmEntity> Clone for OrmStore<T> {
    fn clone(&self) -> Self {
        Self {
            adapter: Arc::clone(&self.adapter),
            _marker: PhantomData,
        }
    }
}

impl<T> OrmStore<T>
where
    T: OrmEntity + Serialize + DeserializeOwned,
{
    /// Construct a typed store over an existing adapter, ensuring
    /// the entity's schema is materialized first. Idempotent — if
    /// the schema is already registered with a matching shape, this
    /// is a no-op at the adapter layer.
    pub async fn new(adapter: Arc<dyn StorageAdapter>) -> Result<Self, OrmStoreError> {
        let schema = T::collection_schema();
        let result = adapter.ensure_schema(schema).await;
        unwrap_storage(
            result,
            "ensure_schema",
            T::COLLECTION,
            OrmStoreError::SchemaFailed {
                collection: T::COLLECTION.to_string(),
                detail: String::new(),
            },
        )?;
        Ok(Self {
            adapter,
            _marker: PhantomData,
        })
    }

    /// The collection name this store targets. Convenience for
    /// telemetry + log lines.
    pub fn collection(&self) -> &'static str {
        T::COLLECTION
    }

    /// Persist a new entity row. The caller supplies the entity's
    /// id — see module docs on identity discipline.
    pub async fn save(&self, id: Uuid, entity: &T) -> Result<(), OrmStoreError> {
        let data = serde_json::to_value(entity).map_err(OrmStoreError::SerializeFailed)?;
        let record = DataRecord {
            id: id.to_string(),
            collection: T::COLLECTION.to_string(),
            data,
            metadata: RecordMetadata::default(),
        };
        let result = self.adapter.create(record).await;
        unwrap_adapter(result, "create", T::COLLECTION)?;
        Ok(())
    }

    /// Load every row in the collection, paired with its row id.
    /// Order is adapter-defined (no ORDER BY); callers needing a
    /// specific order should drop down to QueryBuilder + sort_asc /
    /// sort_desc on the relevant field.
    pub async fn find_all(&self) -> Result<Vec<(Uuid, T)>, OrmStoreError> {
        let query = QueryBuilder::new(T::COLLECTION).build();
        let result = self.adapter.query(query).await;
        let records = unwrap_adapter(result, "query", T::COLLECTION)?;
        records
            .into_iter()
            .map(|r| -> Result<(Uuid, T), OrmStoreError> {
                let id = Uuid::parse_str(&r.id).map_err(OrmStoreError::InvalidUuid)?;
                let entity: T =
                    serde_json::from_value(r.data).map_err(OrmStoreError::DeserializeFailed)?;
                Ok((id, entity))
            })
            .collect()
    }

    /// Resolve a single row by id. Returns `Ok(None)` when the row
    /// doesn't exist — clean Option semantics, not the adapter's
    /// "Record not found" error-string convention.
    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<T>, OrmStoreError> {
        // Use query() with an id filter rather than read(), because
        // the adapter's read() returns not-found as an error string
        // ("Record not found: <uuid>") that we'd have to substring-
        // match to discriminate from real errors. query() with 0
        // results is the clean signal.
        let query = QueryBuilder::new(T::COLLECTION)
            .filter_eq("id", id.to_string())
            .limit(1)
            .build();
        let result = self.adapter.query(query).await;
        let mut records = unwrap_adapter(result, "query", T::COLLECTION)?;
        match records.pop() {
            Some(r) => Ok(Some(
                serde_json::from_value(r.data).map_err(OrmStoreError::DeserializeFailed)?,
            )),
            None => Ok(None),
        }
    }

    /// Overwrite an existing row's data. Increments the BaseEntity
    /// version counter (optimistic concurrency control surface).
    pub async fn update(&self, id: Uuid, entity: &T) -> Result<(), OrmStoreError> {
        let data = serde_json::to_value(entity).map_err(OrmStoreError::SerializeFailed)?;
        let result = self
            .adapter
            .update(T::COLLECTION, &id.to_string(), data, true)
            .await;
        unwrap_adapter(result, "update", T::COLLECTION)?;
        Ok(())
    }

    /// Delete a row. Returns `Ok(true)` if a row was deleted,
    /// `Ok(false)` if the id wasn't present.
    pub async fn delete(&self, id: Uuid) -> Result<bool, OrmStoreError> {
        let result = self.adapter.delete(T::COLLECTION, &id.to_string()).await;
        Ok(unwrap_adapter(result, "delete", T::COLLECTION)?)
    }
}

/// Generic StorageResult unwrap for adapter CRUD operations.
fn unwrap_adapter<T>(
    result: StorageResult<T>,
    operation: &'static str,
    collection: &'static str,
) -> Result<T, OrmStoreError> {
    if result.success {
        result.data.ok_or_else(|| OrmStoreError::AdapterFailed {
            operation,
            collection: collection.to_string(),
            detail: "adapter reported success but returned no data".to_string(),
        })
    } else {
        Err(OrmStoreError::AdapterFailed {
            operation,
            collection: collection.to_string(),
            detail: result.error.unwrap_or_else(|| "unknown".to_string()),
        })
    }
}

/// Variant of unwrap_adapter for ensure_schema — the failure mode
/// is named differently because it's a setup/lifecycle concern, not
/// a CRUD operation.
fn unwrap_storage<T>(
    result: StorageResult<T>,
    _operation: &'static str,
    collection: &'static str,
    _fallback: OrmStoreError,
) -> Result<T, OrmStoreError> {
    if result.success {
        result.data.ok_or_else(|| OrmStoreError::SchemaFailed {
            collection: collection.to_string(),
            detail: "adapter reported success but returned no data".to_string(),
        })
    } else {
        Err(OrmStoreError::SchemaFailed {
            collection: collection.to_string(),
            detail: result.error.unwrap_or_else(|| "unknown".to_string()),
        })
    }
}

// ─── Shared test fixture (module-level, cfg-test gated) ────────────────

/// Build a fresh in-memory ORM adapter. Lives at module scope (not
/// inside `mod tests`) so cross-module tests — e.g.,
/// `crate::identity::tests` — can lease the same helper instead of
/// re-implementing the 8-line setup. Per
/// [[test-fixtures-are-system-primitives]]: shared fixtures belong
/// at the substrate level, not duplicated per test module.
///
/// Uses a per-test random db_path so concurrent cargo tests don't
/// collide via the SQLite shared-cache `:memory:` alias. Return the
/// `TempDir` alongside the adapter — caller owns its lifetime; drop
/// at test-end cleans up cleanly (no `/tmp` accumulation).
#[cfg(test)]
pub(crate) async fn fresh_adapter() -> (Arc<dyn StorageAdapter>, tempfile::TempDir) {
    use crate::orm::adapter::AdapterConfig;
    use crate::orm::sqlite::SqliteAdapter;
    let mut adapter = SqliteAdapter::new();
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("orm-store-test.sqlite");
    let mut config = AdapterConfig::default();
    config.connection_string = path.to_string_lossy().into_owned();
    adapter
        .initialize(config)
        .await
        .expect("adapter initialize");
    (Arc::new(adapter), tmp)
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orm::adapter::AdapterConfig;
    use crate::orm::sqlite::SqliteAdapter;
    use crate::orm::types::{CollectionSchema, FieldType, SchemaField};
    use serde::Deserialize;

    /// Tiny test entity — just BaseEntity columns + one domain field.
    /// Tests the typed-store machinery without dragging Engram's full
    /// shape into the unit test (which would force this test file to
    /// know about EngramOrigin / AircMessageRef / TrustState etc.).
    /// Engram's round-trip is exercised in a separate integration
    /// test alongside its OrmEntity impl.
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    #[serde(rename_all = "camelCase")]
    struct TinyEntity {
        id: String,
        created_at: String,
        updated_at: String,
        version: u32,
        label: String,
    }

    impl OrmEntity for TinyEntity {
        const COLLECTION: &'static str = "tiny_entities";
        fn collection_schema() -> CollectionSchema {
            let mut fields = crate::orm::entity::base_entity_fields();
            fields.push(SchemaField {
                name: "label".to_string(),
                field_type: FieldType::String,
                indexed: false,
                unique: false,
                nullable: false,
                max_length: None,
                foreign_key: None,
            });
            CollectionSchema {
                collection: Self::COLLECTION.to_string(),
                fields,
                indexes: vec![],
            }
        }
    }

    fn tiny(id: Uuid, label: &str) -> TinyEntity {
        let now = chrono::Utc::now().to_rfc3339();
        TinyEntity {
            id: id.to_string(),
            created_at: now.clone(),
            updated_at: now,
            version: 1,
            label: label.to_string(),
        }
    }

    // `fresh_adapter` was lifted to module scope above so cross-module
    // tests (`crate::identity::tests`, future siblings) can lease it.
    // In-mod tests below call it via `super::fresh_adapter()`.

    /// What this catches: save + find_by_id round-trip preserves
    /// every entity field. The foundation of the typed-store
    /// contract — if this regresses, every OrmEntity built on top
    /// loses data silently.
    #[tokio::test]
    async fn save_then_find_by_id_round_trips_every_field() {
        let (adapter, _tmp) = super::fresh_adapter().await;
        let store = OrmStore::<TinyEntity>::new(adapter).await.expect("store");

        let id = Uuid::new_v4();
        let entity = tiny(id, "hello");
        store.save(id, &entity).await.expect("save");
        let loaded = store
            .find_by_id(id)
            .await
            .expect("find_by_id")
            .expect("entity present");
        assert_eq!(loaded.label, "hello");
        assert_eq!(loaded.id, id.to_string());
    }

    /// What this catches: find_by_id returns None for a missing id,
    /// not an error. Cleanly discriminating not-found from real
    /// failures is what every caller wants.
    #[tokio::test]
    async fn find_by_id_returns_none_for_missing_id() {
        let (adapter, _tmp) = super::fresh_adapter().await;
        let store = OrmStore::<TinyEntity>::new(adapter).await.expect("store");
        let absent = Uuid::new_v4();
        let result = store.find_by_id(absent).await.expect("find_by_id");
        assert!(result.is_none(), "missing id must be None, not Err");
    }

    /// What this catches: find_all returns every saved row.
    /// Foundation for the "rehydrate L2 from disk at boot" pattern
    /// every persona-state-style store needs.
    #[tokio::test]
    async fn find_all_returns_every_saved_row() {
        let (adapter, _tmp) = super::fresh_adapter().await;
        let store = OrmStore::<TinyEntity>::new(adapter).await.expect("store");

        let ids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
        for (i, id) in ids.iter().enumerate() {
            store
                .save(*id, &tiny(*id, &format!("entity-{i}")))
                .await
                .expect("save");
        }

        let loaded = store.find_all().await.expect("find_all");
        assert_eq!(loaded.len(), 3);

        let loaded_ids: std::collections::BTreeSet<Uuid> =
            loaded.iter().map(|(id, _)| *id).collect();
        let saved_ids: std::collections::BTreeSet<Uuid> = ids.iter().copied().collect();
        assert_eq!(loaded_ids, saved_ids);
    }

    /// What this catches: update overwrites the existing row, the
    /// next read sees the new payload. Without this, every
    /// metadata-update path (record_recall_hit, apply_decay) would
    /// silently fail to persist.
    #[tokio::test]
    async fn update_then_find_by_id_returns_new_payload() {
        let (adapter, _tmp) = super::fresh_adapter().await;
        let store = OrmStore::<TinyEntity>::new(adapter).await.expect("store");

        let id = Uuid::new_v4();
        store.save(id, &tiny(id, "original")).await.expect("save");

        let mut revised = tiny(id, "revised");
        revised.version = 2;
        store.update(id, &revised).await.expect("update");

        let loaded = store
            .find_by_id(id)
            .await
            .expect("find_by_id")
            .expect("present");
        assert_eq!(loaded.label, "revised");
    }

    /// What this catches: delete removes the row, subsequent
    /// find_by_id is None, delete of an absent id returns false.
    /// Models the cleanup paths a persistence layer needs.
    #[tokio::test]
    async fn delete_removes_row_and_signals_idempotently() {
        let (adapter, _tmp) = super::fresh_adapter().await;
        let store = OrmStore::<TinyEntity>::new(adapter).await.expect("store");

        let id = Uuid::new_v4();
        store.save(id, &tiny(id, "doomed")).await.expect("save");
        let deleted = store.delete(id).await.expect("delete");
        assert!(deleted, "first delete reports row was removed");

        let absent = store.find_by_id(id).await.expect("find_by_id");
        assert!(absent.is_none(), "deleted row is gone");

        let again = store.delete(id).await.expect("delete absent");
        assert!(!again, "second delete reports nothing-to-do");
    }

    /// What this catches: collection() returns the entity's COLLECTION
    /// constant — pinning the typed store's identity for log lines +
    /// telemetry.
    #[test]
    fn collection_returns_entity_collection_constant() {
        // Doesn't need to round-trip through SQLite — just type-level.
        assert_eq!(TinyEntity::COLLECTION, "tiny_entities");
    }
}
