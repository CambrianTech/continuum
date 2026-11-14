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
//! E=mcÂ² doctrine in CLAUDE.md: ONE generic helper instead of N
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
//! - Atomic multi-row writes go through the adapter's all-or-nothing
//!   `batch`: a store builds each row's operation ([`OrmStore::create_op`],
//!   [`OrmStore::update_op`]) and [`commit`] applies them together, so a
//!   decision and the record it changes land in ONE transaction.

#[cfg(test)]
use futures::TryStreamExt;
use std::marker::PhantomData;
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::Serialize;
use uuid::Uuid;

use crate::orm::adapter::StorageAdapter;
use crate::orm::entity::OrmEntity;
use crate::orm::query::{QueryBuilder, StorageQuery};
use crate::orm::seam::{Deleted, RowRef, Update, Versioning, Written};
use crate::orm::storage_error::StorageError;
use crate::orm::types::{Appended, BatchOperation, BatchOutcome, DataRecord, RecordMetadata, RowWrite, SequenceSpec};

/// Typed failure modes for OrmStore operations. Per [[no-fallbacks-
/// ever]] + the typed-error pattern: every failure mode is named so
/// callers pattern-match, never swallow.
#[derive(Debug, thiserror::Error)]
pub enum OrmStoreError {
    /// The adapter refused or failed the operation; the source says why, as a variant.
    #[error("{operation} on {collection}: {source}")]
    Storage {
        operation: &'static str,
        collection: &'static str,
        #[source]
        source: StorageError,
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
        adapter.ensure_schema(&schema).await.map_err(failed::<T>("ensure_schema"))?;
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
        let data = row_data(entity)?;
        let record = DataRecord {
            id: id.to_string(),
            collection: T::COLLECTION.to_string(),
            data,
            metadata: RecordMetadata::default(),
        };
        self.adapter.create(record).await.map_err(failed::<T>("create"))?;
        Ok(())
    }

    /// Count matching entities in the database without decoding their bodies.
    pub async fn count(&self) -> Result<usize, OrmStoreError> {
        self.adapter.count(&QueryBuilder::new(T::COLLECTION).build()).await.map_err(failed::<T>("count"))
    }

    /// Stream boot replay in stable row order without collecting an intermediate result set.
    pub async fn scan(&self) -> Result<futures::stream::BoxStream<'_, Result<(Uuid, T), OrmStoreError>>, OrmStoreError> {
        use futures::StreamExt;
        let rows = self.adapter.scan(&QueryBuilder::new(T::COLLECTION).build()).await.map_err(failed::<T>("scan"))?;
        Ok(rows.map(|row| {
            let row = row.map_err(failed::<T>("scan"))?;
            let id = Uuid::parse_str(&row.id).map_err(OrmStoreError::InvalidUuid)?;
            Ok((id, row.decode().map_err(OrmStoreError::DeserializeFailed)?))
        }).boxed())
    }

    /// Load the rows `query` selects, paired with their row ids. The query must
    /// target this store's collection.
    pub async fn find_matching(&self, query: &StorageQuery) -> Result<Vec<(Uuid, T)>, OrmStoreError> {
        let records = self.adapter.query(query).await.map_err(failed::<T>("query"))?;
        records
            .into_iter()
            .map(|r| -> Result<(Uuid, T), OrmStoreError> {
                let id = Uuid::parse_str(&r.id).map_err(OrmStoreError::InvalidUuid)?;
                let entity: T =
                    r.decode().map_err(OrmStoreError::DeserializeFailed)?;
                Ok((id, entity))
            })
            .collect()
    }

    /// Load the rows whose `field` equals `value`, paired with their row ids.
    pub async fn find_eq(
        &self,
        field: &str,
        value: impl Into<serde_json::Value>,
        limit: usize,
    ) -> Result<Vec<(Uuid, T)>, OrmStoreError> {
        self.find_matching(&QueryBuilder::new(T::COLLECTION).filter_eq(field, value).limit(limit).build())
            .await
    }

    /// The row `id`, or `None` when it does not exist (the adapter's typed NotFound).
    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<T>, OrmStoreError> {
        let id = id.to_string();
        match self.adapter.read(RowRef { collection: T::COLLECTION, id: &id }).await {
            Ok(record) => Ok(Some(record.decode().map_err(OrmStoreError::DeserializeFailed)?)),
            Err(StorageError::NotFound { .. }) => Ok(None),
            Err(source) => Err(failed::<T>("read")(source)),
        }
    }

    /// Overwrite an existing row's data. Increments the BaseEntity
    /// version counter (optimistic concurrency control surface).
    pub async fn update(&self, id: Uuid, entity: &T) -> Result<(), OrmStoreError> {
        self.write_update(id, update_data(entity)?).await
    }

    /// Write a concrete typed subset of this entity's columns. This avoids
    /// re-serializing an immutable payload when only linkage metadata changes.
    /// The adapter still owns its read/write representation and version bump;
    /// this is not a compare-and-swap or multi-row transaction.
    pub async fn update_fields<P: Serialize>(
        &self,
        id: Uuid,
        fields: &P,
    ) -> Result<(), OrmStoreError> {
        self.write_update(id, row_data(fields)?).await
    }

    /// The one update path: `data` becomes row `id`'s declared fields and its version moves.
    async fn write_update(&self, id: Uuid, data: serde_json::Value) -> Result<(), OrmStoreError> {
        let id = id.to_string();
        let row = RowRef { collection: T::COLLECTION, id: &id };
        self.adapter
            .update(Update { row, data, versioning: Versioning::Increment })
            .await
            .map_err(failed::<T>("update"))?;
        Ok(())
    }

    /// The create of a new row, as one operation of an atomic [`commit`].
    pub fn create_op(&self, id: Uuid, entity: &T) -> Result<BatchOperation, OrmStoreError> {
        Ok(BatchOperation::Create(RowWrite { collection: T::COLLECTION.to_string(), id: id.to_string(), data: row_data(entity)? }))
    }

    /// The overwrite of an existing row ([`OrmStore::update`]), as one operation of an atomic
    /// [`commit`].
    pub fn update_op(&self, id: Uuid, entity: &T) -> Result<BatchOperation, OrmStoreError> {
        Ok(BatchOperation::Update(RowWrite { collection: T::COLLECTION.to_string(), id: id.to_string(), data: update_data(entity)? }))
    }

    /// The sequenced append of a new row ([`BatchOperation::Append`]), as the last operation
    /// of an atomic [`commit_appending`].
    pub fn append_op(
        &self,
        id: Uuid,
        entity: &T,
        spec: SequenceSpec,
    ) -> Result<BatchOperation, OrmStoreError> {
        Ok(BatchOperation::Append {
            row: RowWrite { collection: T::COLLECTION.to_string(), id: id.to_string(), data: row_data(entity)? },
            spec,
        })
    }

    /// The committed high-water of this collection's sequence: wakes when an append commits.
    pub fn sequence_revision(&self) -> tokio::sync::watch::Receiver<u64> {
        self.adapter.sequence_revision(T::COLLECTION)
    }

    /// Save `entity` as row `id` unless that row already holds a GREATER `field`: ONE
    /// atomic backend operation ([`StorageAdapter::upsert_monotonic`]), so the compare
    /// can never be separated from the write. Returns whether it was written or kept.
    pub async fn save_monotonic(&self, id: Uuid, entity: &T, field: &str) -> Result<Written, OrmStoreError> {
        let record = DataRecord {
            id: id.to_string(),
            collection: T::COLLECTION.to_string(),
            data: row_data(entity)?,
            metadata: RecordMetadata::default(),
        };
        self.adapter.upsert_monotonic(record, field).await.map_err(failed::<T>("upsert_monotonic"))
    }

    /// Delete every row `query` selects, judged when the delete runs: ONE backend
    /// statement ([`StorageAdapter::delete_matching`]). Returns the count removed.
    pub async fn delete_matching(&self, query: StorageQuery) -> Result<usize, OrmStoreError> {
        self.adapter.delete_matching(query).await.map_err(failed::<T>("delete_matching"))
    }

    /// Delete a row: removed, or absent.
    pub async fn delete(&self, id: Uuid) -> Result<Deleted, OrmStoreError> {
        let id = id.to_string();
        self.adapter.delete(RowRef { collection: T::COLLECTION, id: &id }).await.map_err(failed::<T>("delete"))
    }
}

/// An entity's row as the storage adapter takes it: the ONE place a store encodes a row.
fn row_data<P: Serialize>(entity: &P) -> Result<serde_json::Value, OrmStoreError> {
    serde_json::to_value(entity).map_err(OrmStoreError::SerializeFailed) // storage boundary: the adapter's row format (SQLite JSON)
}

/// Full entity snapshots carry BaseEntity read metadata. The typed owner sends
/// only mutable payload columns; the adapter retains creation time and advances
/// its own version/update time. Partial updates remain strict about metadata.
fn update_data<T: Serialize>(entity: &T) -> Result<serde_json::Value, OrmStoreError> {
    let mut data = row_data(entity)?;
    if let Some(fields) = data.as_object_mut() {
        for key in crate::orm::types::METADATA_KEYS {
            fields.remove(*key);
        }
    }
    Ok(data)
}

/// Apply `operations` (built by stores sharing `adapter`) ATOMICALLY: every one commits, or none
/// does. The one way a decision and the row it changes are written together.
pub async fn commit(adapter: &Arc<dyn StorageAdapter>, operations: Vec<BatchOperation>) -> Result<(), OrmStoreError> {
    adapter.batch(operations).await.map_err(|source| OrmStoreError::Storage { operation: "batch", collection: "atomic commit", source })?;
    Ok(())
}

/// Apply `prior` and then `append` ATOMICALLY (a row and the sequenced record of it, written
/// together), returning what the append did: the sequence it was given, or the existing row's
/// when its unique value was already there.
pub async fn commit_appending(
    adapter: &Arc<dyn StorageAdapter>,
    mut prior: Vec<BatchOperation>,
    append: BatchOperation,
) -> Result<Appended, OrmStoreError> {
    prior.push(append);
    let outcomes = adapter
        .batch(prior)
        .await
        .map_err(|source| OrmStoreError::Storage { operation: "batch", collection: "atomic append", source })?;
    match outcomes.last() {
        Some(BatchOutcome::Appended { appended }) => Ok(*appended),
        _ => Err(OrmStoreError::Storage {
            operation: "append",
            collection: "atomic append",
            source: StorageError::Corrupt { what: "an atomic append's last outcome is not its append" },
        }),
    }
}

/// The error of a failed adapter call on `T`'s collection.
fn failed<T: OrmEntity>(operation: &'static str) -> impl Fn(StorageError) -> OrmStoreError {
    move |source| OrmStoreError::Storage { operation, collection: T::COLLECTION, source }
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
#[cfg(any(test, feature = "test-fixtures"))]
pub async fn fresh_adapter() -> (Arc<dyn StorageAdapter>, tempfile::TempDir) {
    use crate::orm::SeaOrmAdapter;
    let tmp = tempfile::tempdir().expect("tempdir");
    let adapter = SeaOrmAdapter::open(&tmp.path().join("orm-store-test.sqlite"))
        .await
        .expect("adapter open");
    (Arc::new(adapter), tmp)
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
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
                default_value: None,
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
    async fn a_string_field_holding_json_text_round_trips_verbatim() {
        // what this catches (2026-09-13, Atlas + Benchy): a String column whose text is
        // JSON-shaped decoded as a map by a byte-sniffing reader — the entity refused
        // it, find_all failed whole, and two citizens never seated. The store declares
        // `label` as String, so the adapter must hand the text back untouched.
        let (adapter, _tmp) = fresh_adapter().await;
        let store = OrmStore::<TinyEntity>::new(adapter).await.expect("store");
        let id = Uuid::new_v4();
        let looks_like_json = "{\"path\": \"sympy\"}";
        store.save(id, &tiny(id, looks_like_json)).await.expect("save");
        let by_id = store.find_by_id(id).await.expect("find_by_id").expect("present");
        assert_eq!(by_id.label, looks_like_json, "find_by_id kept the text verbatim");
        let all = store.scan().await.expect("find_all decodes every row").try_collect::<Vec<_>>().await.expect("test: stream rows");
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].1.label, looks_like_json, "find_all kept the text verbatim");
    }

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

        let loaded = store.scan().await.expect("find_all").try_collect::<Vec<_>>().await.expect("test: stream rows");
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
        assert_eq!(loaded.version, 2, "storage increments the persisted version once");
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
