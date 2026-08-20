//! Entity Schema Loader — Phase 2 of ORM refactor
//!
//! Loads `protocol/typescript/entity_schemas.json` (emitted at build time
//! by `tools/generator/generate-entity-schemas.ts`) and provides typed access
//! to entity metadata. This is the Rust side of the decorator-metadata
//! pipeline:
//!
//!     TS @Field/@CompositeIndex/@Archive decorators
//!              │
//!              ▼
//!     ENTITY_REGISTRY walked at build time
//!              │
//!              ▼
//!     generate-entity-schemas.ts
//!              │
//!              ▼
//!     protocol/typescript/entity_schemas.json  (checked in)
//!              │
//!              ▼
//!     include_str! at Rust compile time  (this file)
//!              │
//!              ▼
//!     DataModule calls resolve(collection) when it needs entity shape
//!
//! Design doc: docs/architecture/ORM-PHASE-2-DESIGN.md
//!
//! SHA-256 drift detection: the JSON carries a `$sha256` field hashed over
//! the canonical `{ entities: ... }` subtree. On load we recompute and
//! compare — mismatch means someone hand-edited the generated file, which
//! is a hard fail with a clear message. The generator itself is idempotent;
//! re-run `npm run build:ts` to regenerate.
//!
//! Phase 2 step 2 — this module defines types + loader + SHA check.
//! Step 3 will wire `ensure_schema` through the resolver so the IPC wire
//! payload can shrink from inline CollectionSchema to just a collection
//! name. No behavior change in this module alone.

use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

// ─── JSON-facing types (camelCase serde) ──────────────────────────────────

/// Per-field options — everything except `fieldName` and `fieldType`.
/// Mirrors `FieldMetadata.options` in TS `FieldDecorators.ts`.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldOptions {
    #[serde(default)]
    pub references: Option<String>,
    #[serde(default)]
    pub index: Option<bool>,
    #[serde(default)]
    pub unique: Option<bool>,
    #[serde(default)]
    pub nullable: Option<bool>,
    /// TS typed as `any`. Use Value to preserve whatever literal shipped.
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default)]
    pub max_length: Option<u32>,
    #[serde(default)]
    pub description: Option<bool>,
    #[serde(default)]
    pub summary: Option<bool>,
    #[serde(default)]
    pub blob_threshold: Option<u32>,
    #[serde(default)]
    pub blob_ref_field: Option<String>,
}

/// A single field declaration from TS decorators.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldMetadata {
    pub field_name: String,
    /// Enum in TS — kept as String here for forward compat when new types
    /// land. Validation happens in the adapter that actually renders SQL.
    pub field_type: String,
    #[serde(default)]
    pub options: Option<FieldOptions>,
}

/// Composite (multi-column) index. Declared via `@CompositeIndex` on the
/// entity class in TS.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompositeIndexMetadata {
    pub name: String,
    pub fields: Vec<String>,
    #[serde(default)]
    pub unique: Option<bool>,
    /// "ASC" or "DESC", applies to the last field.
    #[serde(default)]
    pub direction: Option<String>,
}

/// Archival rule declared via `@Archive` on the entity class in TS.
/// Paired with a destination handle — the data module uses this to know
/// when/how to spill records to archive storage.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveConfig {
    pub source_handle: String,
    pub dest_handle: String,
    pub max_rows: u64,
    pub rows_per_archive: u64,
    pub max_archive_file_rows: u64,
    pub order_by_field: String,
}

/// Full schema for one registered entity (one row in ENTITY_REGISTRY on TS).
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EntitySchema {
    pub collection: String,
    pub entity_class: String,
    pub fields: Vec<FieldMetadata>,
    #[serde(default)]
    pub composite_indexes: Vec<CompositeIndexMetadata>,
    #[serde(default)]
    pub archive: Option<ArchiveConfig>,
}

// Top-level file metadata fields (`$schemaVersion`, `$generatedAt`,
// `$sha256`) are not bound to a struct — we parse the JSON as a Value once
// for SHA verification, then re-deserialize only the `entities` subtree
// into typed structs. See `parse_and_verify` below.

// ─── JSON shipping + loading ──────────────────────────────────────────────

/// Generated artifact baked into the binary at compile time. Any change to
/// the JSON file forces a Rust rebuild — intentional, so the binary and
/// the committed schema file can't drift.
///
/// Path is relative to THIS source file:
///   modules/entity_schemas.rs  (this file)
///   ../../../../protocol/typescript/entity_schemas.json
///     \_ modules -> \_ src -> \_ continuum-core -> \_ workers -> \_ src
const ENTITY_SCHEMAS_JSON: &str =
    include_str!("../../../../protocol/typescript/entity_schemas.json");

/// Lazy-load the entity schemas. First caller triggers parse + SHA check;
/// subsequent callers get the cached map. Panics (with a clear message) on
/// malformed JSON, unsupported schema version, or SHA mismatch — these are
/// build-time invariants that should never fail at runtime, so a panic is
/// the right signal.
///
/// Design intentionally rejects silent degradation: an empty or wrong map
/// would cause `ensure_schema` to succeed for bogus collections. Fail loud
/// so the user rebuilds TS.
pub fn get_entity_schemas() -> &'static HashMap<String, EntitySchema> {
    static CACHE: OnceLock<HashMap<String, EntitySchema>> = OnceLock::new();
    CACHE.get_or_init(|| {
        parse_and_verify().unwrap_or_else(|e| {
            panic!(
                "entity_schemas.json failed to load: {}\n\
                 This is a build-time invariant. Try: npm run build:ts",
                e
            )
        })
    })
}

/// Resolve one collection to its schema. Separate helper so callers don't
/// have to touch the top-level HashMap API.
pub fn resolve(collection: &str) -> Option<&'static EntitySchema> {
    get_entity_schemas().get(collection)
}

/// Convert an EntitySchema (the decorator-sourced shape) to the
/// adapter-facing CollectionSchema (what ensure_schema expects). This is
/// the typed-data side of Phase 2: the language level never mentions SQL,
/// but the adapter still needs a concrete schema struct to build CREATE
/// TABLE from. The mapping collapses TS decorator field types
/// ('primary', 'foreign_key', 'text', 'enum', 'date', 'number', 'boolean',
/// 'json', 'blob') to the Rust FieldType enum that adapters understand.
pub fn to_collection_schema(es: &EntitySchema) -> crate::orm::types::CollectionSchema {
    use crate::orm::types::{CollectionSchema, FieldType, SchemaField, SchemaIndex};

    let fields: Vec<SchemaField> = es
        .fields
        .iter()
        .map(|f| {
            // TS decorator types → Rust FieldType. Text-family stays String;
            // blob is large JSON in practice (opt-in blob store handles
            // the spilling behavior separately via blobThreshold).
            let field_type = match f.field_type.as_str() {
                "date" => FieldType::Date,
                "number" => FieldType::Number,
                "boolean" => FieldType::Boolean,
                "json" | "blob" => FieldType::Json,
                // primary | foreign_key | text | enum | anything else
                _ => FieldType::String,
            };
            let opts = f.options.as_ref();
            SchemaField {
                name: f.field_name.clone(),
                field_type,
                indexed: opts.and_then(|o| o.index).unwrap_or(false),
                unique: opts.and_then(|o| o.unique).unwrap_or(false),
                nullable: opts.and_then(|o| o.nullable).unwrap_or(false),
                max_length: opts.and_then(|o| o.max_length).map(|n| n as usize),
                foreign_key: None,
            }
        })
        .collect();

    let indexes: Vec<SchemaIndex> = es
        .composite_indexes
        .iter()
        .map(|c| SchemaIndex {
            name: c.name.clone(),
            fields: c.fields.clone(),
            unique: c.unique.unwrap_or(false),
        })
        .collect();

    CollectionSchema {
        collection: es.collection.clone(),
        fields,
        indexes,
    }
}

fn parse_and_verify() -> Result<HashMap<String, EntitySchema>, String> {
    // First pass: parse as untyped Value so we can canonicalize + hash the
    // `entities` subtree exactly as TS emitted it. Avoids needing Serialize
    // derives on our typed structs just for hashing.
    let raw: Value =
        serde_json::from_str(ENTITY_SCHEMAS_JSON).map_err(|e| format!("parse error: {}", e))?;

    // Validate schema version + extract sha256 from the top level.
    let schema_version = raw
        .get("$schemaVersion")
        .and_then(|v| v.as_u64())
        .ok_or_else(|| "$schemaVersion missing or not a number".to_string())?;
    if schema_version != 1 {
        return Err(format!(
            "unsupported $schemaVersion: {} (this build expects 1)",
            schema_version
        ));
    }
    let embedded_sha = raw
        .get("$sha256")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "$sha256 missing or not a string".to_string())?
        .to_string();
    let entities_value = raw
        .get("entities")
        .ok_or_else(|| "`entities` key missing".to_string())?;

    // Recompute SHA-256 over canonical `{ "entities": <entities> }`.
    // Canonicalization: sort object keys, preserve array order. Must match
    // the TS codegen's `canonicalize()` helper for the hash to agree.
    let wrapped = serde_json::json!({ "entities": entities_value });
    let canonical_value = canonicalize_value(&wrapped);
    let canonical_str = serde_json::to_string(&canonical_value)
        .map_err(|e| format!("canonical serialize failed: {}", e))?;
    let mut hasher = Sha256::new();
    hasher.update(canonical_str.as_bytes());
    let computed = hex_encode(&hasher.finalize());

    if computed != embedded_sha {
        return Err(format!(
            "SHA-256 drift: embedded={}, computed={}. \
             Someone hand-edited entity_schemas.json; re-run npm run build:ts",
            &embedded_sha[..16],
            &computed[..16],
        ));
    }

    // Second pass: now that the hash checks out, deserialize into typed
    // structs for consumers.
    let entities: HashMap<String, EntitySchema> = serde_json::from_value(entities_value.clone())
        .map_err(|e| format!("typed parse of entities failed: {}", e))?;

    Ok(entities)
}

fn canonicalize_value(v: &Value) -> Value {
    match v {
        Value::Object(map) => {
            let sorted: BTreeMap<&String, Value> = map
                .iter()
                .map(|(k, v)| (k, canonicalize_value(v)))
                .collect();
            let mut out = serde_json::Map::new();
            for (k, v) in sorted {
                out.insert(k.clone(), v);
            }
            Value::Object(out)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(canonicalize_value).collect()),
        other => other.clone(),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{:02x}", b));
    }
    s
}

// ─── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Smoke: the file parses, SHA checks, and we get a non-empty map.
    /// If this fails, either the JSON is malformed or the SHA canonicalization
    /// disagrees between TS and Rust.
    #[test]
    fn entity_schemas_load() {
        let schemas = get_entity_schemas();
        assert!(
            !schemas.is_empty(),
            "ENTITY_REGISTRY walk yielded no entities"
        );
        // Sanity: a well-known entity should resolve.
        assert!(
            schemas.contains_key("users"),
            "'users' collection missing from entity_schemas.json"
        );
    }

    /// The `users` entity should have at least the BaseEntity fields.
    #[test]
    fn users_has_base_fields() {
        let users = resolve("users").expect("users entity");
        let names: Vec<&str> = users.fields.iter().map(|f| f.field_name.as_str()).collect();
        for base in ["id", "createdAt", "updatedAt", "version"] {
            assert!(
                names.contains(&base),
                "users missing BaseEntity field '{}' — got {:?}",
                base,
                names
            );
        }
    }
}
