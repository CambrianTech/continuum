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
//! Design doc: docs/infrastructure/RUST-ORM-ARCHITECTURE.md
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

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{BufWriter, Write};
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
    pub on_delete: Option<crate::orm::CascadeRule>,
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

/// The file's top level: its version and hash are required by this type, so a file without
/// them fails to parse rather than being checked field by field. `entities` stays a `Value`
/// so it is hashed exactly as TS emitted it (no Serialize derives on the typed structs just
/// for hashing), then re-deserialized into typed structs. `$generatedAt` is not read.
#[derive(Deserialize)]
struct EntitySchemasFile {
    #[serde(rename = "$schemaVersion")]
    schema_version: u64,
    #[serde(rename = "$sha256")]
    sha256: String,
    entities: Value,
}

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
    include_str!("../../../protocol/typescript/entity_schemas.json");

/// The embedded `entity_schemas.json` did not load: malformed JSON, an unsupported
/// schema version, or a SHA mismatch. A build-time invariant broken at runtime; the
/// caller refuses its operation with this error rather than serving an empty map.
#[derive(Debug, Clone, thiserror::Error)]
#[error("entity_schemas.json failed to load: {0}. This is a build-time invariant. Try: npm run build:ts")]
pub struct EntitySchemasUnavailable(String);

/// Lazy-load the entity schemas. First caller triggers parse + SHA check;
/// subsequent callers get the cached map, or the cached load error.
///
/// Design intentionally rejects silent degradation: an empty or wrong map
/// would cause `ensure_schema` to succeed for bogus collections. A failed load
/// is an error every caller returns, so the user rebuilds TS.
pub fn get_entity_schemas() -> Result<&'static HashMap<String, EntitySchema>, &'static EntitySchemasUnavailable> {
    static CACHE: OnceLock<Result<HashMap<String, EntitySchema>, EntitySchemasUnavailable>> = OnceLock::new();
    CACHE.get_or_init(|| parse_and_verify().map_err(EntitySchemasUnavailable)).as_ref()
}

/// Resolve one collection to its schema (`None`: not a TS-declared entity). Separate
/// helper so callers don't have to touch the top-level HashMap API.
pub fn resolve(collection: &str) -> Result<Option<&'static EntitySchema>, &'static EntitySchemasUnavailable> {
    Ok(get_entity_schemas()?.get(collection))
}

/// Convert an EntitySchema (the decorator-sourced shape) to the
/// adapter-facing CollectionSchema (what ensure_schema expects). This is
/// the typed-data side of Phase 2: the language level never mentions SQL,
/// but the adapter still needs a concrete schema struct to build CREATE
/// TABLE from. The mapping collapses TS decorator field types
/// ('primary', 'foreign_key', 'text', 'enum', 'date', 'number', 'boolean',
/// 'json', 'blob') to the Rust FieldType enum that adapters understand.
pub fn to_collection_schema(es: &EntitySchema) -> Result<crate::orm::types::CollectionSchema, String> {
    use crate::orm::types::{CascadeRule, CollectionSchema, FieldType, ForeignKeyRef, SchemaField, SchemaIndex};
    let fields = es.fields.iter().map(|f| {
        let field_type = match f.field_type.as_str() {
            "date" => FieldType::Date,
            "number" => FieldType::Number,
            "boolean" => FieldType::Boolean,
            "json" => FieldType::Json,
            "binary" => FieldType::Binary,
            "primary" | "foreign_key" | "text" | "enum" => FieldType::String,
            other => return Err(format!("{}.{}: undeclared field type {other}", es.collection, f.field_name)),
        };
        let opts = f.options.as_ref();
        let foreign_key = opts.and_then(|o| o.references.as_deref()).map(|reference| {
            let (collection, field) = reference.split_once('.').ok_or_else(||
                format!("{}.{}: reference must declare collection.field: {reference}", es.collection, f.field_name))?;
            if collection.is_empty() || field.is_empty() || field.contains('.') {
                return Err(format!("invalid reference {reference}"));
            }
            Ok(ForeignKeyRef { collection: collection.into(), field: field.into(),
                on_delete: opts.and_then(|o| o.on_delete).unwrap_or(CascadeRule::Restrict), on_update: CascadeRule::Restrict })
        }).transpose()?;
        Ok(SchemaField {
            default_value: opts.and_then(|o| o.default.clone()),
            name: f.field_name.clone(), field_type,
            indexed: opts.and_then(|o| o.index).unwrap_or(false),
            unique: opts.and_then(|o| o.unique).unwrap_or(false),
            nullable: opts.and_then(|o| o.nullable).unwrap_or(false),
            max_length: opts.and_then(|o| o.max_length).map(|n| n as usize),
            foreign_key,
        })
    }).collect::<Result<Vec<_>, String>>()?;
    let indexes = es.composite_indexes.iter().map(|c| {
        let descending = match c.direction.as_deref() {
            None | Some("ASC") => false,
            Some("DESC") => true,
            Some(other) => return Err(format!("{}: invalid index direction {other}", c.name)),
        };
        Ok(SchemaIndex { descending, name: c.name.clone(), fields: c.fields.clone(), unique: c.unique.unwrap_or(false) })
    }).collect::<Result<Vec<_>, String>>()?;
    Ok(CollectionSchema { collection: es.collection.clone(), fields, indexes })
}

fn parse_and_verify() -> Result<HashMap<String, EntitySchema>, String> {
    let EntitySchemasFile { schema_version, sha256: embedded_sha, entities: mut entities_value } =
        serde_json::from_str(ENTITY_SCHEMAS_JSON).map_err(|e| format!("parse error: {}", e))?;
    if schema_version != 1 {
        return Err(format!(
            "unsupported $schemaVersion: {} (this build expects 1)",
            schema_version
        ));
    }
    // Recompute SHA-256 over canonical `{ "entities": <entities> }`.
    // Canonicalization: sort object keys, preserve array order. Must match
    // the TS codegen's `canonicalize()` helper for the hash to agree.
    let computed = canonical_digest(&mut entities_value)?;

    if computed != embedded_sha {
        return Err(format!(
            "SHA-256 drift: embedded={}, computed={}. \
             Someone hand-edited entity_schemas.json; re-run npm run build:ts",
            embedded_sha,
            computed,
        ));
    }

    // Second pass: now that the hash checks out, deserialize into typed
    // structs for consumers.
    let entities: HashMap<String, EntitySchema> = serde_json::from_value(entities_value)
        .map_err(|e| format!("typed parse of entities failed: {}", e))?;

    validate_relationships(&entities)?;
    Ok(entities)
}

fn validate_relationships(entities: &HashMap<String, EntitySchema>) -> Result<(), String> {
    let schemas = entities.values().map(to_collection_schema).collect::<Result<Vec<_>, _>>()?;
    for schema in &schemas {
        for source in &schema.fields {
            let Some(reference) = &source.foreign_key else { continue; };
            let target = schemas.iter().find(|target| target.collection == reference.collection)
                .ok_or_else(|| format!("{}.{}: undeclared relationship target {}", schema.collection, source.name, reference.collection))?;
            let field = target.fields.iter().find(|field| field.name == reference.field)
                .ok_or_else(|| format!("{}.{}: missing target field {}.{}", schema.collection, source.name, target.collection, reference.field))?;
            let unique = field.name == "id" || field.unique || target.indexes.iter().any(|index|
                index.unique && index.fields == [field.name.clone()]);
            if !unique {
                return Err(format!("{}.{}: relationship target {}.{} must be unique", schema.collection, source.name, target.collection, field.name));
            }
            if reference.on_delete == crate::orm::CascadeRule::SetNull && !source.nullable {
                return Err(format!("{}.{}: SET NULL requires nullable declaration", schema.collection, source.name));
            }
        }
    }
    Ok(())
}

fn canonical_digest(entities: &mut Value) -> Result<String, String> {
    // Sort in place (a no-op with serde_json's default sorted maps). The same
    // owned tree is subsequently consumed by typed schema decoding.
    entities.sort_all_objects();
    #[derive(Serialize)]
    struct Envelope<'a> {
        entities: &'a Value,
    }
    let mut hasher = Sha256::new();
    {
        // This is the generated schema's canonical JSON digest boundary, not
        // runtime payload conversion. Buffer small serializer writes without
        // allocating a second tree or a complete serialized document.
        let mut writer = BufWriter::new(&mut hasher);
        serde_json::to_writer(&mut writer, &Envelope { entities })
            .map_err(|e| format!("canonical serialize failed: {e}"))?;
        writer.flush().map_err(|e| format!("canonical digest flush failed: {e}"))?;
    }
    Ok(hex_encode(&hasher.finalize()))
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

    #[test]
    fn canonical_digest_preserves_values_and_array_order() -> Result<(), Box<dyn std::error::Error>> {
        let mut entities: Value = serde_json::from_str(
            r#"{"z":[{"z":null,"a":"quote\"\n雪"},18446744073709551615,-3.25,true],"a":{}}"#,
        )?;
        let original = entities.clone();
        let expected = r#"{"entities":{"a":{},"z":[{"a":"quote\"\n雪","z":null},18446744073709551615,-3.25,true]}}"#;
        assert_eq!(canonical_digest(&mut entities)?, hex_encode(&Sha256::digest(expected.as_bytes())));
        assert_eq!(entities, original);
        entities["z"][0]["z"] = Value::Bool(false);
        assert_ne!(canonical_digest(&mut entities)?, hex_encode(&Sha256::digest(expected.as_bytes())));
        Ok(())
    }

    #[test]
    #[ignore = "isolated generated-schema hash allocation/timing comparison"]
    fn canonical_digest_replay() -> Result<(), Box<dyn std::error::Error>> {
        // Reference is the removed algorithm, retained only to compare exact
        // bytes and cost on the actual generated schema workload.
        fn old_canonical(v: &Value) -> Value {
            match v {
                Value::Object(map) => {
                    let sorted: std::collections::BTreeMap<_, _> = map.iter()
                        .map(|(key, value)| (key, old_canonical(value))).collect();
                    Value::Object(sorted.into_iter().map(|(key, value)| (key.clone(), value)).collect())
                }
                Value::Array(items) => Value::Array(items.iter().map(old_canonical).collect()),
                other => other.clone(),
            }
        }
        let file: EntitySchemasFile = serde_json::from_str(ENTITY_SCHEMAS_JSON)?;
        for native_first in [false, true] {
            let mut native_us = 0;
            let mut old_us = 0;
            for _ in 0..20 {
                for native in [native_first, !native_first] {
                    // Each path receives an owned parsed tree; parsing is excluded.
                    let mut entities = file.entities.clone();
                    let start = std::time::Instant::now();
                    let computed = if native {
                        canonical_digest(&mut entities)?
                    } else {
                        let wrapped = Value::Object(serde_json::Map::from_iter([
                            ("entities".to_string(), entities.clone()),
                        ]));
                        let text = serde_json::to_string(&old_canonical(&wrapped))?;
                        hex_encode(&Sha256::digest(text.as_bytes()))
                    };
                    let elapsed = start.elapsed().as_micros();
                    assert_eq!(computed, file.sha256);
                    if native { native_us += elapsed; } else { old_us += elapsed; }
                }
            }
            eprintln!("schema digest 20 runs native_first={native_first}: old_us={old_us} streamed_us={native_us}; input_bytes={}", ENTITY_SCHEMAS_JSON.len());
        }
        Ok(())
    }

    #[test]
    fn declarations_preserve_constraints_and_refuse_unknown_types() -> Result<(), String> {
        let mut entity: EntitySchema = serde_json::from_value(serde_json::json!({
            "collection": "child", "entityClass": "Child", "fields": [{
                "fieldName": "parentId", "fieldType": "foreign_key", "options": {
                    "references": "parent.id", "nullable": true, "maxLength": 36,
                    "default": "root", "index": true
                }
            }], "compositeIndexes": [{"name": "parent_order", "fields": ["parentId"], "direction": "DESC"}]
        })).map_err(|e| e.to_string())?;
        let schema = to_collection_schema(&entity)?;
        let field = &schema.fields[0];
        assert_eq!(field.default_value, Some(Value::String("root".into())));
        assert_eq!(field.max_length, Some(36));
        assert!(field.nullable && field.indexed && schema.indexes[0].descending);
        let reference = field.foreign_key.as_ref().ok_or("missing relationship")?;
        assert_eq!((&*reference.collection, &*reference.field), ("parent", "id"));
        entity.fields[0].field_type = "undeclared".into();
        assert!(to_collection_schema(&entity).is_err());
        entity.fields[0].field_type = "foreign_key".into();
        entity.composite_indexes[0].direction = Some("sideways".into());
        assert!(to_collection_schema(&entity).is_err());
        Ok(())
    }

    /// Smoke: the file parses, SHA checks, and we get a non-empty map.
    /// If this fails, either the JSON is malformed or the SHA canonicalization
    /// disagrees between TS and Rust.
    #[test]
    fn entity_schemas_load() {
        let schemas = get_entity_schemas().expect("entity_schemas.json loads");
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
        let users = resolve("users").expect("entity_schemas.json loads").expect("users entity");
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
