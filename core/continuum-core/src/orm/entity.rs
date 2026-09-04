//! Rust-native entity registry — the Rust authoring path that runs
//! alongside the TS-decorator authoring path in
//! `crate::modules::entity_schemas`.
//!
//! Doctrine: ORM-everything ([[orm-everything-not-hand-edited-files]]).
//! Substrate-only entities (hw tiers, role templates, identity pools,
//! universes, future continuum config) are authored Rust-first — the
//! struct + serde derives are the source of truth; the
//! [`CollectionSchema`] falls out of an `OrmEntity` impl; ts-rs emits
//! the matching TS type. The TS-decorator path stays for user-facing
//! entities (chat, users, cognition).
//!
//! Resolution order in [`crate::modules::data::DataModule::handle_ensure_schema`]:
//!   1. Rust registry (this module) — substrate-authored
//!   2. `entity_schemas.json` (TS-derived) — user-app-authored
//!   3. Error: unknown collection
//!
//! Registration happens once at boot (typically from a module's `new()`)
//! and the registry is read-only thereafter — write-once-at-startup is
//! deliberate so we never get racy mid-lifetime schema swaps.

use crate::orm::types::CollectionSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{OnceLock, RwLock};
use ts_rs::TS;

/// **The canonical base shape every ORM record carries.** Source of
/// truth for both Rust runtime and TS wire types — ts-rs emits the
/// matching TS type in `protocol/typescript/orm/BaseEntity.ts`. The TS-
/// side hand-authored `BaseEntity.ts` is being migrated to this
/// generated version (single source of truth in Rust per Joel's
/// 2026-06-01 directive).
///
/// Two complementary layers in this module:
/// - `BaseEntity` (this struct) — the WIRE TYPE. What records look
///   like in memory + on JSON in/out. ts-rs makes it a TS type.
/// - `base_entity_fields()` (below) — the STORAGE COLUMNS. What the
///   schema declares to the adapter so the SQL table has the matching
///   id/createdAt/updatedAt/version columns.
///
/// The two are kept in lockstep by intent: changing one without the
/// other is a bug that the cross-test in `persona::mod.rs` catches
/// (every Rust-authored collection asserts the BaseEntity columns are
/// present).
///
/// Entity structs (e.g. `HwTierDescriptor`, `RoleTemplate`) carry
/// only their domain payload today; the base values are stamped by
/// the adapter at insert time and re-attached on read via the
/// `DataRecord` wrapper. A future slice may flatten `BaseEntity`
/// directly into entity structs via `#[serde(flatten)]` to match the
/// TS class-extension convention — kept on the slice-2 list rather
/// than churning struct shapes here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/orm/BaseEntity.ts")]
#[serde(rename_all = "camelCase")]
pub struct BaseEntity {
    /// UUID primary key. String-typed for cross-platform portability;
    /// adapters parse/format as needed.
    pub id: String,
    /// ISO 8601 timestamp. Stamped by the ORM on insert.
    pub created_at: String,
    /// ISO 8601 timestamp. Stamped by the ORM on every update.
    pub updated_at: String,
    /// Optimistic concurrency control — incremented on each update.
    /// New records start at 1.
    pub version: u32,
}

impl BaseEntity {
    /// Construct a fresh BaseEntity for a brand-new record. Generates a
    /// UUID v4, stamps `now()` for both timestamps, sets version=1.
    pub fn for_new_record() -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        BaseEntity {
            id: uuid::Uuid::new_v4().to_string(),
            created_at: now.clone(),
            updated_at: now,
            version: 1,
        }
    }
}

/// A Rust-native ORM entity. The `impl` block is hand-written; the
/// associated `CollectionSchema` carries the storage-side field shape
/// (flat fields + JSON columns for nested structs).
///
/// Nested structs in the serialized form (e.g. `RoleTemplate.identity:
/// IdentityDefaults`) are stored as JSON-typed columns; queries on
/// inner fields go through JSON-path operators at the adapter layer.
pub trait OrmEntity: Send + Sync + 'static {
    /// The collection name (table name in SQL backends). Must be unique
    /// across BOTH the Rust registry and `entity_schemas.json` — collision
    /// is a registration-time hard error.
    const COLLECTION: &'static str;

    /// Build the storage-side schema. Called once at registration.
    fn collection_schema() -> CollectionSchema;
}

/// Global write-once-at-boot registry of Rust-authored entities.
///
/// Concurrency: `RwLock` so the boot path can `write` once and every
/// `handle_ensure_schema` call thereafter takes a cheap `read`. The
/// intended lifecycle is single-writer-at-startup, many-readers-after;
/// no module should mutate the registry past initial boot.
pub struct OrmEntityRegistry {
    schemas: RwLock<HashMap<String, CollectionSchema>>,
}

impl OrmEntityRegistry {
    /// Fresh empty registry. Production uses `global()` for the
    /// process-wide singleton; tests construct fresh instances so they
    /// don't race on shared state when run in parallel.
    pub fn new() -> Self {
        OrmEntityRegistry {
            schemas: RwLock::new(HashMap::new()),
        }
    }

    /// Process-wide singleton. First call lazy-initializes; subsequent
    /// callers see the same instance.
    pub fn global() -> &'static OrmEntityRegistry {
        static INSTANCE: OnceLock<OrmEntityRegistry> = OnceLock::new();
        INSTANCE.get_or_init(OrmEntityRegistry::new)
    }

    /// Register an entity by type. Idempotent on identical schemas (same
    /// collection, same field set) — re-registration with the same shape
    /// is a no-op, so module boot order doesn't matter and multiple test
    /// inits don't clobber. Collision with a DIFFERENT shape is a hard
    /// error (returns `Err`) — that's a programming bug, surface it.
    ///
    /// Boot pattern:
    /// ```ignore
    /// OrmEntityRegistry::global().register::<HwTierDescriptor>()?;
    /// OrmEntityRegistry::global().register::<RoleTemplate>()?;
    /// ```
    pub fn register<E: OrmEntity>(&self) -> Result<(), RegistrationError> {
        let schema = E::collection_schema();
        let collection = schema.collection.clone();
        let mut map = self
            .schemas
            .write()
            .expect("OrmEntityRegistry lock poisoned");
        match map.get(&collection) {
            Some(existing) if schemas_equivalent(existing, &schema) => Ok(()),
            Some(_) => Err(RegistrationError::SchemaConflict {
                collection: collection.clone(),
            }),
            None => {
                map.insert(collection, schema);
                Ok(())
            }
        }
    }

    /// Resolve a collection to its Rust-authored schema, if any.
    /// Returns `None` when the collection isn't registered here; the
    /// caller falls back to `entity_schemas.json`.
    pub fn resolve(&self, collection: &str) -> Option<CollectionSchema> {
        let map = self
            .schemas
            .read()
            .expect("OrmEntityRegistry lock poisoned");
        map.get(collection).cloned()
    }

    /// All registered collection names. Useful for diagnostics and the
    /// `data/list-collections` path.
    pub fn collection_names(&self) -> Vec<String> {
        let map = self
            .schemas
            .read()
            .expect("OrmEntityRegistry lock poisoned");
        map.keys().cloned().collect()
    }

    /// Test-only reset of the global singleton. NOT for production use
    /// — registry is write-once at boot by design. Most tests should
    /// construct fresh `OrmEntityRegistry::new()` instances instead of
    /// resetting the global; this helper exists for the narrow case
    /// where production code under test reaches `OrmEntityRegistry::
    /// global()` directly and a clean global is needed.
    ///
    /// Caveat: cargo tests run in parallel by default, so resetting
    /// the global races with other tests doing the same. Prefer fresh
    /// `new()` instances; only reach for this when the SUT is
    /// hard-coded to the singleton.
    #[cfg(test)]
    pub fn reset_for_tests(&self) {
        self.schemas
            .write()
            .expect("OrmEntityRegistry lock poisoned")
            .clear();
    }
}

impl Default for OrmEntityRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Errors returned by `OrmEntityRegistry::register`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistrationError {
    /// Collection already registered with a DIFFERENT schema shape.
    /// Indicates two entities claiming the same collection name with
    /// incompatible fields — a programming bug.
    SchemaConflict { collection: String },
}

impl std::fmt::Display for RegistrationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistrationError::SchemaConflict { collection } => write!(
                f,
                "OrmEntityRegistry: collection '{}' registered twice with different schemas",
                collection
            ),
        }
    }
}

impl std::error::Error for RegistrationError {}

/// Base columns every ORM entity carries. Mirrors the TS-side
/// `BaseEntity` contract (id + createdAt + updatedAt + version) so
/// Rust-authored entities and TS-authored entities share one storage
/// shape — adapters, queries, vector index, and the round-trip-to-
/// JSON export all treat them uniformly.
///
/// Rust entities concatenate this with their own fields in
/// `OrmEntity::collection_schema()`:
///
/// ```ignore
/// fn collection_schema() -> CollectionSchema {
///     let mut fields = base_entity_fields();
///     fields.extend(vec![ /* entity-specific fields */ ]);
///     CollectionSchema { collection: Self::COLLECTION.into(), fields, indexes: vec![] }
/// }
/// ```
///
/// Field shapes (cross-checked against `entity_schemas.json` for the
/// canonical `users` and `memories` collections):
/// - `id` — Uuid, unique + indexed + not nullable. The primary key.
///   Distinct from any domain-natural key (e.g. `role_template.role`,
///   `hw_tier.tier_id`) which lives as its OWN unique-indexed field.
/// - `createdAt`, `updatedAt` — Date, indexed for "recent N" queries.
/// - `version` — Number, optimistic concurrency control.
///
/// camelCase field names because the existing adapters expect them
/// (the ORM auto-translates to snake_case at the SQL layer per
/// `crate::orm::mod.rs` preamble).
/// True if `snake_case_name` is one of the BaseEntity column names
/// adapters hardcode in their CREATE TABLE statements (`id`,
/// `created_at`, `updated_at`, `version`).
///
/// Adapters use this to dedupe schema.fields against their hardcoded
/// BaseEntity column block. Without dedup, a schema produced by
/// `base_entity_fields()` + domain-specific fields would arrive at
/// the adapter with `id` declared twice — once hardcoded in the
/// adapter, once in the iterated schema.fields — and CREATE TABLE
/// fails on the duplicate. This matches the documented contract
/// (`base_entity_fields()` declares BaseEntity columns explicitly)
/// without requiring entities to know how each backend lays out
/// CREATE TABLE.
pub fn is_base_entity_column(snake_case_name: &str) -> bool {
    matches!(
        snake_case_name,
        "id" | "created_at" | "updated_at" | "version"
    )
}

pub fn base_entity_fields() -> Vec<crate::orm::types::SchemaField> {
    use crate::orm::types::{FieldType, SchemaField};
    vec![
        SchemaField {
            name: "id".to_string(),
            field_type: FieldType::Uuid,
            indexed: true,
            unique: true,
            nullable: false,
            max_length: None,
            foreign_key: None,
        },
        SchemaField {
            name: "createdAt".to_string(),
            field_type: FieldType::Date,
            indexed: true,
            unique: false,
            nullable: false,
            max_length: None,
            foreign_key: None,
        },
        SchemaField {
            name: "updatedAt".to_string(),
            field_type: FieldType::Date,
            indexed: true,
            unique: false,
            nullable: false,
            max_length: None,
            foreign_key: None,
        },
        SchemaField {
            name: "version".to_string(),
            field_type: FieldType::Number,
            indexed: false,
            unique: false,
            nullable: false,
            max_length: None,
            foreign_key: None,
        },
    ]
}

/// Schema equivalence check for idempotent registration. Compares
/// collection name + field set (by name, type, index/unique/nullable
/// flags) + composite index set. Field ORDER doesn't matter for
/// equivalence — two registrations with the same fields in different
/// orders are equivalent.
fn schemas_equivalent(a: &CollectionSchema, b: &CollectionSchema) -> bool {
    if a.collection != b.collection {
        return false;
    }
    if a.fields.len() != b.fields.len() {
        return false;
    }
    // Order-independent compare. Build a name-keyed map of one side, walk
    // the other.
    let a_by_name: HashMap<&str, &crate::orm::types::SchemaField> =
        a.fields.iter().map(|f| (f.name.as_str(), f)).collect();
    for bf in &b.fields {
        let Some(af) = a_by_name.get(bf.name.as_str()) else {
            return false;
        };
        if af.field_type != bf.field_type
            || af.indexed != bf.indexed
            || af.unique != bf.unique
            || af.nullable != bf.nullable
            || af.max_length != bf.max_length
        {
            return false;
        }
    }
    // Indexes — compare by name + fields + unique.
    if a.indexes.len() != b.indexes.len() {
        return false;
    }
    let a_idx_by_name: HashMap<&str, &crate::orm::types::SchemaIndex> =
        a.indexes.iter().map(|i| (i.name.as_str(), i)).collect();
    for bi in &b.indexes {
        let Some(ai) = a_idx_by_name.get(bi.name.as_str()) else {
            return false;
        };
        if ai.fields != bi.fields || ai.unique != bi.unique {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::orm::types::{FieldType, SchemaField, SchemaIndex};

    // Two minimal test entities to exercise the registry without
    // dragging in HwTierDescriptor or RoleTemplate (which live in
    // crate::persona and would cause module-cycle pain in unit tests).

    struct Alpha;
    impl OrmEntity for Alpha {
        const COLLECTION: &'static str = "alpha_test_collection";
        fn collection_schema() -> CollectionSchema {
            CollectionSchema {
                collection: "alpha_test_collection".to_string(),
                fields: vec![SchemaField {
                    name: "id".to_string(),
                    field_type: FieldType::String,
                    indexed: true,
                    unique: true,
                    nullable: false,
                    max_length: None,
                    foreign_key: None,
                }],
                indexes: vec![],
            }
        }
    }

    struct Beta;
    impl OrmEntity for Beta {
        const COLLECTION: &'static str = "beta_test_collection";
        fn collection_schema() -> CollectionSchema {
            CollectionSchema {
                collection: "beta_test_collection".to_string(),
                fields: vec![
                    SchemaField {
                        name: "id".to_string(),
                        field_type: FieldType::String,
                        indexed: true,
                        unique: true,
                        nullable: false,
                        max_length: None,
                        foreign_key: None,
                    },
                    SchemaField {
                        name: "value".to_string(),
                        field_type: FieldType::Number,
                        indexed: false,
                        unique: false,
                        nullable: true,
                        max_length: None,
                        foreign_key: None,
                    },
                ],
                indexes: vec![SchemaIndex {
                    name: "idx_value".to_string(),
                    fields: vec!["value".to_string()],
                    unique: false,
                }],
            }
        }
    }

    // Conflicting Beta — same collection, different field set. Used to
    // verify SchemaConflict detection.
    struct BetaConflict;
    impl OrmEntity for BetaConflict {
        const COLLECTION: &'static str = "beta_test_collection";
        fn collection_schema() -> CollectionSchema {
            CollectionSchema {
                collection: "beta_test_collection".to_string(),
                fields: vec![SchemaField {
                    name: "different_field".to_string(),
                    field_type: FieldType::Boolean,
                    indexed: false,
                    unique: false,
                    nullable: false,
                    max_length: None,
                    foreign_key: None,
                }],
                indexes: vec![],
            }
        }
    }

    // All tests construct fresh `OrmEntityRegistry::new()` instances —
    // cargo runs unit tests in parallel and any test touching the
    // global singleton races with siblings. The `register<E>` API is
    // generic over registry instance, so this introduces zero
    // production-path divergence.

    /// Smoke: register + resolve roundtrip on a single entity.
    #[test]
    fn register_then_resolve_roundtrips() {
        let registry = OrmEntityRegistry::new();
        registry.register::<Alpha>().expect("register Alpha");
        let resolved = registry.resolve("alpha_test_collection").expect("resolve");
        assert_eq!(resolved.collection, "alpha_test_collection");
        assert_eq!(resolved.fields.len(), 1);
        assert_eq!(resolved.fields[0].name, "id");
    }

    /// Multiple entities coexist; both resolve independently.
    #[test]
    fn multiple_entities_resolve_independently() {
        let registry = OrmEntityRegistry::new();
        registry.register::<Alpha>().expect("register Alpha");
        registry.register::<Beta>().expect("register Beta");
        assert!(registry.resolve("alpha_test_collection").is_some());
        let beta = registry.resolve("beta_test_collection").expect("Beta");
        assert_eq!(beta.fields.len(), 2);
        assert_eq!(beta.indexes.len(), 1);
    }

    /// Unknown collection resolves to None (caller falls back to TS).
    #[test]
    fn unknown_collection_returns_none() {
        let registry = OrmEntityRegistry::new();
        assert!(registry.resolve("does_not_exist").is_none());
    }

    /// Idempotent re-registration of the SAME schema is a no-op.
    /// Load-bearing — module boot order and multiple test inits must
    /// not error.
    #[test]
    fn idempotent_reregister_same_schema_is_ok() {
        let registry = OrmEntityRegistry::new();
        registry.register::<Beta>().expect("first register");
        registry
            .register::<Beta>()
            .expect("re-register with same schema is no-op");
        registry
            .register::<Beta>()
            .expect("re-register again still no-op");
        let resolved = registry.resolve("beta_test_collection").expect("resolve");
        assert_eq!(resolved.fields.len(), 2);
    }

    /// Two entities claiming the same collection with DIFFERENT shapes
    /// is a SchemaConflict — surfaces the programming bug at boot
    /// instead of letting the second silently override.
    #[test]
    fn conflicting_schema_returns_error() {
        let registry = OrmEntityRegistry::new();
        registry.register::<Beta>().expect("first register");
        let err = registry
            .register::<BetaConflict>()
            .expect_err("conflict should error");
        assert!(matches!(
            err,
            RegistrationError::SchemaConflict { ref collection } if collection == "beta_test_collection"
        ));
    }

    /// collection_names returns every registered collection.
    #[test]
    fn collection_names_lists_all() {
        let registry = OrmEntityRegistry::new();
        registry.register::<Alpha>().expect("register Alpha");
        registry.register::<Beta>().expect("register Beta");
        let mut names = registry.collection_names();
        names.sort();
        assert_eq!(names, vec!["alpha_test_collection", "beta_test_collection"]);
    }

    /// BaseEntity wire type fields match `base_entity_fields()`
    /// storage columns. Load-bearing — these two layers must stay in
    /// lockstep; drift means TS consumers see a `BaseEntity` shape
    /// that doesn't actually live in the database (or vice versa).
    #[test]
    fn base_entity_wire_matches_storage_columns() {
        let base = BaseEntity::for_new_record();
        let json = serde_json::to_value(&base).expect("serialize");
        let obj = json.as_object().expect("base serializes as object");
        let wire_fields: std::collections::BTreeSet<&str> =
            obj.keys().map(|s| s.as_str()).collect();
        let storage = base_entity_fields();
        let storage_fields: std::collections::BTreeSet<&str> =
            storage.iter().map(|f| f.name.as_str()).collect();
        assert_eq!(
            wire_fields, storage_fields,
            "BaseEntity wire type and base_entity_fields() storage columns drifted"
        );
    }

    /// for_new_record produces sane defaults (id is a UUID, version=1,
    /// timestamps parse as RFC3339).
    #[test]
    fn for_new_record_defaults_are_sensible() {
        let base = BaseEntity::for_new_record();
        assert!(uuid::Uuid::parse_str(&base.id).is_ok(), "id must be UUID");
        assert_eq!(base.version, 1, "new record version = 1");
        chrono::DateTime::parse_from_rfc3339(&base.created_at)
            .expect("created_at must parse as RFC3339");
        chrono::DateTime::parse_from_rfc3339(&base.updated_at)
            .expect("updated_at must parse as RFC3339");
    }

    /// Field order doesn't affect schema equivalence (idempotent
    /// reregistration of structs whose fields happen to be declared in
    /// a different order must still no-op).
    #[test]
    fn equivalence_is_order_independent() {
        let registry = OrmEntityRegistry::new();

        struct OrderA;
        impl OrmEntity for OrderA {
            const COLLECTION: &'static str = "order_test";
            fn collection_schema() -> CollectionSchema {
                CollectionSchema {
                    collection: "order_test".to_string(),
                    fields: vec![
                        SchemaField {
                            name: "a".to_string(),
                            field_type: FieldType::String,
                            indexed: false,
                            unique: false,
                            nullable: false,
                            max_length: None,
                            foreign_key: None,
                        },
                        SchemaField {
                            name: "b".to_string(),
                            field_type: FieldType::Number,
                            indexed: false,
                            unique: false,
                            nullable: false,
                            max_length: None,
                            foreign_key: None,
                        },
                    ],
                    indexes: vec![],
                }
            }
        }
        struct OrderB;
        impl OrmEntity for OrderB {
            const COLLECTION: &'static str = "order_test";
            fn collection_schema() -> CollectionSchema {
                CollectionSchema {
                    collection: "order_test".to_string(),
                    fields: vec![
                        SchemaField {
                            name: "b".to_string(),
                            field_type: FieldType::Number,
                            indexed: false,
                            unique: false,
                            nullable: false,
                            max_length: None,
                            foreign_key: None,
                        },
                        SchemaField {
                            name: "a".to_string(),
                            field_type: FieldType::String,
                            indexed: false,
                            unique: false,
                            nullable: false,
                            max_length: None,
                            foreign_key: None,
                        },
                    ],
                    indexes: vec![],
                }
            }
        }

        registry.register::<OrderA>().expect("register A");
        registry
            .register::<OrderB>()
            .expect("re-register with reordered fields is equivalent");
    }
}
