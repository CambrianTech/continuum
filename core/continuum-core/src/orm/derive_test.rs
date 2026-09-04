//! End-to-end test of `#[derive(Entity)]` from continuum-orm-derive.
//!
//! Exercises the macro's behavior on representative struct shapes:
//! - BaseEntity composition via `#[serde(flatten)] pub base: BaseEntity`
//! - Type inference for String / Uuid / numbers / bool / Vec / Option
//! - Field-level `#[entity(indexed)]`, `#[entity(unique)]`,
//!   `#[entity(json)]`, `#[entity(skip)]`
//! - Schema-field name camelCase translation from snake_case fields
//! - Full round-trip through OrmStore over real SQLite (proof that the
//!   derived schema accepts saves + loads)
//!
//! Per [[no-sql-everything-through-orm-entities]] + Joel 2026-06-03
//! ("Entities need to be defined, and in one place, rust"): if the
//! derived schema can save + load via the typed store, drift between
//! the Rust struct and any consumer of the schema is structurally
//! prevented going forward.

#![cfg(test)]

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::orm::adapter::{AdapterConfig, StorageAdapter};
use crate::orm::entity::{BaseEntity, OrmEntity};
use crate::orm::sqlite::SqliteAdapter;
use crate::orm::types::{CascadeRule, FieldType};
use crate::orm::Entity;
use crate::orm::OrmStore;

/// Representative entity covering every type-inference branch the
/// derive supports + struct-level composite indexes. Lives in tests,
/// not production — production entities (Engram + RecallMetadata)
/// migrate to the derive in slice #168 once the foundation +
/// relational features are both proven.
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "derive_test_widgets")]
#[entity(index(name = "idx_label_score", fields = ["label", "score"]))]
#[entity(index(name = "idx_unique_owner_slug", fields = ["ownerId", "slug"], unique = true))]
struct DeriveTestWidget {
    #[serde(flatten)]
    base: BaseEntity,

    /// String, no attrs → FieldType::String, not indexed.
    name: String,

    /// String, indexed.
    #[entity(indexed)]
    label: String,

    /// String, unique.
    #[entity(unique)]
    slug: String,

    /// Number from u64, indexed.
    #[entity(indexed)]
    score: u64,

    /// Number from f32.
    weight: f32,

    /// Number from i32.
    delta: i32,

    /// Boolean.
    is_active: bool,

    /// Uuid by type-name match.
    owner_id: Uuid,

    /// Vec → JSON column.
    tags: Vec<String>,

    /// Option<String> → nullable String.
    description: Option<String>,

    /// Option<u64> → nullable Number.
    expires_at_ms: Option<u64>,

    /// Forced JSON column via attribute — the natural Rust type for
    /// a JSON column is `serde_json::Value`. Stays JSON round-trip
    /// through the adapter.
    #[entity(json)]
    forced_json_blob: serde_json::Value,

    /// Skipped from the schema AND the serde payload — truly in-memory.
    /// `#[entity(skip)]` only declares "not in schema"; pair with
    /// `#[serde(skip)]` to keep it out of the JSON the adapter sees.
    /// Without `serde(skip)` the adapter's schema-evolution path would
    /// re-add the column on save (defeating the point).
    #[entity(skip)]
    #[serde(skip)]
    in_memory_only: bool,
}

fn fresh_widget(name: &str) -> DeriveTestWidget {
    DeriveTestWidget {
        base: BaseEntity::for_new_record(),
        name: name.to_string(),
        label: format!("label-{name}"),
        slug: format!("slug-{name}-{}", Uuid::new_v4()),
        score: 42,
        weight: 1.25,
        delta: -7,
        is_active: true,
        owner_id: Uuid::new_v4(),
        tags: vec!["alpha".to_string(), "beta".to_string()],
        description: Some("optional present".to_string()),
        expires_at_ms: None,
        forced_json_blob: serde_json::json!({"raw": true}),
        in_memory_only: true,
    }
}

/// Type-inference fixture covering branches the round-trip test
/// can't easily exercise. Tests assert `collection_schema()`'s output
/// directly with no disk I/O — pure macro-output verification.
///
/// Schema-only (no serde derives) because the workspace's `chrono`
/// crate doesn't enable the `serde` feature, and SystemTime/PathBuf
/// don't ship serde adapters out of the box either. We're verifying
/// the macro's TYPE INFERENCE, not round-trip serde behavior.
///
/// Added 2026-06-03 per reviewer-1 BLOCK finding on PR #1519: the
/// prior fixture only exercised types DeriveTestWidget happened to
/// use; common types like `SystemTime`, `PathBuf`, `Box<T>`, `Arc<T>`
/// were silently bucketed to `Json` without test coverage.
#[allow(dead_code)]
#[derive(Entity)]
#[entity(collection = "type_inference_probe")]
struct TypeInferenceProbe {
    /// BaseEntity id via the primary_key pattern (avoids needing
    /// serde::flatten which depends on Serialize/Deserialize).
    #[entity(primary_key)]
    id: Uuid,

    /// SystemTime → FieldType::Date. Same branch as chrono::DateTime,
    /// chrono::NaiveDateTime, chrono::Date.
    when: std::time::SystemTime,

    /// PathBuf → FieldType::String (serdes as string in any
    /// serializer that handles it; the macro recognizes the type
    /// name regardless of whether serde has wired it up).
    config_path: std::path::PathBuf,

    /// Box<String> — transparent wrapper, peels to String.
    boxed_label: Box<String>,

    /// Arc<u64> — transparent wrapper, peels to Number.
    shared_counter: std::sync::Arc<u64>,

    /// Option<Option<String>> — flattens to nullable String.
    double_optional: Option<Option<String>>,

    /// u128 — Number variant we missed in the original list.
    big_number: u128,
}

/// Child entity exercising `#[entity(foreign_key(...))]` — references
/// DeriveTestWidget by widget_id, cascade-deletes on parent removal.
/// The widget_id column becomes a FOREIGN KEY in CREATE TABLE; the
/// adapter enforces referential integrity.
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
#[serde(rename_all = "camelCase")]
#[entity(collection = "derive_test_widget_notes")]
struct DeriveTestWidgetNote {
    #[serde(flatten)]
    base: BaseEntity,

    /// FK to derive_test_widgets.id. ON DELETE CASCADE so cleanup is
    /// the DB's responsibility, not application code.
    #[entity(foreign_key("derive_test_widgets.id", on_delete = "cascade"))]
    widget_id: Uuid,

    note: String,
}

async fn fresh_adapter() -> (Arc<dyn StorageAdapter>, tempfile::TempDir) {
    let mut adapter = SqliteAdapter::new();
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = tmp.path().join("derive-test.sqlite");
    let mut config = AdapterConfig::default();
    config.connection_string = path.to_string_lossy().into_owned();
    adapter
        .initialize(config)
        .await
        .expect("adapter initialize");
    (Arc::new(adapter), tmp)
}

/// What this catches: derive emits the collection name from the
/// struct-level `#[entity(collection = "...")]` attribute.
#[test]
fn collection_constant_matches_struct_attribute() {
    assert_eq!(DeriveTestWidget::COLLECTION, "derive_test_widgets");
}

/// What this catches: the derived schema carries BaseEntity columns
/// (because of `#[serde(flatten)] pub base: BaseEntity`) AND every
/// non-skipped domain field. Names are camelCase to match serde
/// serialization. Skipped fields don't appear.
#[test]
fn schema_has_base_columns_plus_domain_fields_minus_skipped() {
    let schema = DeriveTestWidget::collection_schema();
    let names: std::collections::BTreeSet<&str> =
        schema.fields.iter().map(|f| f.name.as_str()).collect();

    // BaseEntity columns (via flatten + base recognition).
    for base in ["id", "createdAt", "updatedAt", "version"] {
        assert!(
            names.contains(base),
            "expected BaseEntity column {base:?} in derived schema; have {names:?}"
        );
    }

    // Domain columns (camelCase).
    for domain in [
        "name",
        "label",
        "slug",
        "score",
        "weight",
        "delta",
        "isActive",
        "ownerId",
        "tags",
        "description",
        "expiresAtMs",
        "forcedJsonBlob",
    ] {
        assert!(
            names.contains(domain),
            "expected domain column {domain:?} in derived schema; have {names:?}"
        );
    }

    // Skipped field absent.
    assert!(
        !names.contains("inMemoryOnly"),
        "skipped field leaked into schema: {names:?}"
    );
}

/// What this catches: field types are inferred from the Rust struct
/// definition correctly across every supported shape.
#[test]
fn field_types_inferred_correctly() {
    let schema = DeriveTestWidget::collection_schema();
    let by_name: std::collections::HashMap<&str, &crate::orm::types::SchemaField> =
        schema.fields.iter().map(|f| (f.name.as_str(), f)).collect();

    // Strings.
    for s in ["name", "label", "slug", "description"] {
        assert_eq!(
            by_name[s].field_type,
            FieldType::String,
            "{s} should infer String"
        );
    }

    // Numbers (u64, f32, i32, u64 via Option).
    for n in ["score", "weight", "delta", "expiresAtMs"] {
        assert_eq!(
            by_name[n].field_type,
            FieldType::Number,
            "{n} should infer Number"
        );
    }

    // Boolean.
    assert_eq!(by_name["isActive"].field_type, FieldType::Boolean);

    // Uuid (by type-name match).
    assert_eq!(by_name["ownerId"].field_type, FieldType::Uuid);

    // Vec → Json.
    assert_eq!(by_name["tags"].field_type, FieldType::Json);

    // Forced JSON column.
    assert_eq!(by_name["forcedJsonBlob"].field_type, FieldType::Json);
}

/// What this catches: `#[entity(indexed)]` and `#[entity(unique)]`
/// land on the schema fields. Other fields default to non-indexed
/// + non-unique.
#[test]
fn indexed_and_unique_attributes_propagate() {
    let schema = DeriveTestWidget::collection_schema();
    let by_name: std::collections::HashMap<&str, &crate::orm::types::SchemaField> =
        schema.fields.iter().map(|f| (f.name.as_str(), f)).collect();

    assert!(by_name["label"].indexed, "label declared indexed");
    assert!(by_name["score"].indexed, "score declared indexed");
    assert!(by_name["slug"].unique, "slug declared unique");

    assert!(!by_name["name"].indexed, "name has no indexed attr");
    assert!(!by_name["name"].unique, "name has no unique attr");
}

/// What this catches: `Option<T>` translates to `nullable = true`
/// on the field; non-Option fields stay non-nullable.
#[test]
fn option_translates_to_nullable() {
    let schema = DeriveTestWidget::collection_schema();
    let by_name: std::collections::HashMap<&str, &crate::orm::types::SchemaField> =
        schema.fields.iter().map(|f| (f.name.as_str(), f)).collect();

    assert!(by_name["description"].nullable, "Option<String> → nullable");
    assert!(by_name["expiresAtMs"].nullable, "Option<u64> → nullable");
    assert!(!by_name["name"].nullable, "String → not nullable");
    assert!(!by_name["score"].nullable, "u64 → not nullable");
}

/// What this catches: end-to-end save + find_by_id + find_all round
/// trip through OrmStore over real SQLite, using the DERIVED
/// schema. Proof that the macro's output is a working OrmEntity
/// impl — not just type-shape correctness in isolation.
#[tokio::test]
async fn round_trip_through_orm_store() {
    let (adapter, _tmp) = fresh_adapter().await;
    let store = OrmStore::<DeriveTestWidget>::new(adapter)
        .await
        .expect("store construction with derived schema");

    let widget = fresh_widget("apex");
    let id = Uuid::parse_str(&widget.base.id).expect("base id is a UUID");

    store.save(id, &widget).await.expect("save derived widget");
    let loaded = store
        .find_by_id(id)
        .await
        .expect("find_by_id")
        .expect("widget should be present");

    assert_eq!(loaded.name, "apex");
    assert_eq!(loaded.label, "label-apex");
    assert_eq!(loaded.score, 42);
    assert_eq!(loaded.delta, -7);
    assert!(loaded.is_active);
    assert_eq!(loaded.tags, vec!["alpha", "beta"]);
    assert_eq!(loaded.description.as_deref(), Some("optional present"));
    assert!(loaded.expires_at_ms.is_none());
    // The `in_memory_only` field is part of the struct shape, but
    // serde round-trips it through whatever the adapter stored. The
    // adapter dropped it from the schema (correctly), so its value
    // is whatever serde defaults to on deserialization — bool's
    // default is false. The point of the assertion is to confirm
    // the field wasn't persisted, not to assert any particular
    // value.
    assert!(
        !loaded.in_memory_only,
        "skipped field should not round-trip a true value"
    );

    let all = store.find_all().await.expect("find_all");
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].0, id);
}

// ── Composite index + foreign-key tests (#167) ────────────────────────

/// What this catches: struct-level `#[entity(index(...))]` declarations
/// land on the schema's `indexes` vec with correct name, fields, and
/// unique flag.
#[test]
fn composite_index_attributes_propagate() {
    let schema = DeriveTestWidget::collection_schema();
    assert_eq!(schema.indexes.len(), 2);

    let by_name: std::collections::HashMap<&str, &crate::orm::types::SchemaIndex> = schema
        .indexes
        .iter()
        .map(|i| (i.name.as_str(), i))
        .collect();

    let label_score = by_name["idx_label_score"];
    assert_eq!(label_score.fields, vec!["label", "score"]);
    assert!(!label_score.unique, "label_score is not unique");

    let owner_slug = by_name["idx_unique_owner_slug"];
    assert_eq!(owner_slug.fields, vec!["ownerId", "slug"]);
    assert!(owner_slug.unique, "owner_slug declared unique");
}

/// What this catches: field-level `#[entity(foreign_key(...))]`
/// populates `SchemaField.foreign_key` with the target collection,
/// target field, and cascade rules.
#[test]
fn foreign_key_attribute_populates_schema_field() {
    let schema = DeriveTestWidgetNote::collection_schema();
    let widget_id = schema
        .fields
        .iter()
        .find(|f| f.name == "widgetId")
        .expect("widgetId field present");

    let fk = widget_id
        .foreign_key
        .as_ref()
        .expect("widgetId must carry a foreign_key");
    assert_eq!(fk.collection, "derive_test_widgets");
    assert_eq!(fk.field, "id");
    assert_eq!(fk.on_delete, CascadeRule::Cascade);
    assert_eq!(fk.on_update, CascadeRule::Restrict); // default
}

/// What this catches: end-to-end relational round-trip. Parent +
/// child entities both register their schemas (with FK constraint
/// declared in CREATE TABLE), child inserts referencing existing
/// parent succeed, ON DELETE CASCADE wipes child rows when parent
/// goes away. SQLite enforces this because we set
/// `PRAGMA foreign_keys=ON` at connection open.
#[tokio::test]
async fn foreign_key_cascade_deletes_children_via_db_enforcement() {
    let (adapter, _tmp) = fresh_adapter().await;
    let widgets = OrmStore::<DeriveTestWidget>::new(Arc::clone(&adapter))
        .await
        .expect("widget store");
    let notes = OrmStore::<DeriveTestWidgetNote>::new(Arc::clone(&adapter))
        .await
        .expect("note store");

    let widget = fresh_widget("parent");
    let widget_id = Uuid::parse_str(&widget.base.id).expect("base id parses");
    widgets.save(widget_id, &widget).await.expect("save widget");

    let note = DeriveTestWidgetNote {
        base: BaseEntity::for_new_record(),
        widget_id,
        note: "observation A".to_string(),
    };
    let note_id = Uuid::parse_str(&note.base.id).expect("note base id parses");
    notes.save(note_id, &note).await.expect("save note");

    // Sanity: child is findable.
    assert!(notes
        .find_by_id(note_id)
        .await
        .expect("find_by_id pre-delete")
        .is_some());

    // Delete the parent widget. SQLite's CASCADE rule must propagate
    // and wipe the child note row.
    let deleted = widgets.delete(widget_id).await.expect("delete widget");
    assert!(deleted);

    let after = notes
        .find_by_id(note_id)
        .await
        .expect("find_by_id post-delete");
    assert!(
        after.is_none(),
        "ON DELETE CASCADE must remove the child row at the DB layer"
    );
}

// ── Slice A: proc-macro hardening (review fixes for #1519) ────────────

/// What this catches: SystemTime / chrono::DateTime → FieldType::Date,
/// not Json. Reviewer-1 finding: the original infer_field_type
/// bucketed any non-listed named type to Json, silently turning
/// timestamps into JSON columns and never producing the Date variant
/// the enum declares. Same code path for chrono::DateTime,
/// NaiveDateTime, Date, NaiveDate — the macro matches by type
/// last-segment name.
#[test]
fn systemtime_infers_as_date() {
    let schema = TypeInferenceProbe::collection_schema();
    let when = schema
        .fields
        .iter()
        .find(|f| f.name == "when")
        .expect("when field present");
    assert_eq!(when.field_type, FieldType::Date);
}

/// What this catches: PathBuf → FieldType::String, not Json. Reviewer-1
/// finding: PathBuf serdes as a string but the macro bucketed it as
/// Json. Without this branch, any persistence path with a PathBuf
/// field (logging paths, model paths, persona home derivations) would
/// land in a JSON column instead of a String column.
#[test]
fn pathbuf_infers_as_string() {
    let schema = TypeInferenceProbe::collection_schema();
    let config_path = schema
        .fields
        .iter()
        .find(|f| f.name == "configPath")
        .expect("configPath field present");
    assert_eq!(config_path.field_type, FieldType::String);
}

/// What this catches: `Box<String>` / `Arc<u64>` peel through the
/// transparent wrapper. Reviewer-1 finding: the macro inferred from
/// the last segment name (`Box` / `Arc`) and bucketed both as Json.
/// Now the inference walks through and picks String / Number.
#[test]
fn box_and_arc_wrappers_peel_to_inner_type() {
    let schema = TypeInferenceProbe::collection_schema();
    let boxed = schema
        .fields
        .iter()
        .find(|f| f.name == "boxedLabel")
        .expect("boxedLabel present");
    let shared = schema
        .fields
        .iter()
        .find(|f| f.name == "sharedCounter")
        .expect("sharedCounter present");
    assert_eq!(boxed.field_type, FieldType::String);
    assert_eq!(shared.field_type, FieldType::Number);
}

/// What this catches: `Option<Option<T>>` collapses to nullable inner
/// type, not a Json column. Reviewer-1 finding: unwrap_option only
/// peeled one layer; the inner Option fell through to Json. Now
/// unwrap_option is recursive — double-Option becomes nullable inner.
#[test]
fn double_option_collapses_to_nullable_inner_type() {
    let schema = TypeInferenceProbe::collection_schema();
    let double = schema
        .fields
        .iter()
        .find(|f| f.name == "doubleOptional")
        .expect("doubleOptional present");
    assert_eq!(double.field_type, FieldType::String);
    assert!(
        double.nullable,
        "double-Option must propagate the nullable flag"
    );
}

/// What this catches: u128 — Reviewer-1 noted the original number
/// list was missing u128/i128. They serdes as numeric (within range)
/// or stringified-numeric (out of range); the schema treats them as
/// Number columns consistently with the other integer types.
#[test]
fn u128_infers_as_number() {
    let schema = TypeInferenceProbe::collection_schema();
    let big = schema
        .fields
        .iter()
        .find(|f| f.name == "bigNumber")
        .expect("bigNumber present");
    assert_eq!(big.field_type, FieldType::Number);
}

/// What this catches: the TypeInferenceProbe schema register +
/// resolve roundtrip cleanly through OrmEntityRegistry. Smoke test
/// that the new fixture is registry-safe alongside the production
/// entities.
#[test]
fn type_inference_probe_registers_cleanly() {
    use crate::orm::entity::OrmEntityRegistry;
    let registry = OrmEntityRegistry::new();
    registry
        .register::<TypeInferenceProbe>()
        .expect("TypeInferenceProbe registers cleanly");
    let resolved = registry
        .resolve("type_inference_probe")
        .expect("type_inference_probe resolves");
    assert_eq!(resolved.collection, "type_inference_probe");
}
