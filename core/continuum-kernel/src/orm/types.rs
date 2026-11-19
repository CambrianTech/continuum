//! ORM Types - Database-agnostic data structures
//!
//! These types mirror the TypeScript DataStorageAdapter interface but in Rust.
//! Adapters work with these types; the ORM layer handles serialization.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// UUID type (stored as string for cross-platform compatibility)
pub type UUID = String;

/// Metadata keys managed by the ORM — excluded from user data in create/update.
/// Single source of truth: adding a metadata column? Add it here.
/// Both camelCase (TypeScript) and snake_case (SQL) variants included.
pub const METADATA_KEYS: &[&str] = &[
    "id",
    "createdAt",
    "created_at",
    "updatedAt",
    "updated_at",
    "version",
];

/// Generic record data - JSON object with string keys
pub type RecordData = serde_json::Map<String, Value>;

/// Field type for schema definition
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../../../protocol/typescript/orm/FieldType.ts")]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    String,
    Number,
    Boolean,
    Date,
    Json,
    /// Producer-native bytes with a typed descriptor, stored as paired BLOBs.
    Binary,
    Uuid,
}

/// What to do to a referencing row when its target row changes.
///
/// SQL-standard semantics. The substrate names these explicitly so
/// every backend (sqlite, future grid-replicated) emits
/// the same intent rather than inheriting whatever default the
/// backend has.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(export, export_to = "../../../protocol/typescript/orm/CascadeRule.ts")]
#[serde(rename_all = "snake_case")]
pub enum CascadeRule {
    /// Default. If a referenced row goes away, the FK column on the
    /// referencing side becomes orphaned and the DB rejects the
    /// delete (`Restrict`/`NoAction` flavor).
    Restrict,
    /// Delete this row when the referenced row goes away. Use for
    /// owned-lifetime relationships (e.g. RecallMetadata when its
    /// Engram is deleted).
    Cascade,
    /// Null this row's FK column when the referenced row goes away.
    /// Useful for nullable references where loss-of-target is
    /// recoverable.
    SetNull,
    /// No-op at the constraint layer — the application owns the
    /// cleanup. Generally avoid; named so explicit-choice is
    /// possible.
    NoAction,
}

impl Default for CascadeRule {
    fn default() -> Self {
        CascadeRule::Restrict
    }
}

/// Foreign-key reference. A typed pointer from `this.field` to
/// `<collection>.<target_field>`, with cascade semantics for the
/// referenced row's lifecycle events.
///
/// Per Joel 2026-06-03 ("provide a relational db this time"): FK is
/// a first-class schema concept, not application-level convention.
/// The sqlite adapter emits the constraint at CREATE
/// TABLE time; queries can JOIN through it; the row-deletion
/// semantics are enforced by the DB, not by hand-coded cleanup.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/ForeignKeyRef.ts"
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ForeignKeyRef {
    /// Target collection name (e.g. "engrams").
    pub collection: String,
    /// Target field on the referenced row. Almost always "id" but
    /// named so non-id natural-key references are expressible.
    pub field: String,
    pub on_delete: CascadeRule,
    pub on_update: CascadeRule,
}

/// Schema field definition
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../../../protocol/typescript/orm/SchemaField.ts")]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SchemaField {
    /// Explicit declared insertion value; absence never invents a value.
    #[serde(default)]
    #[ts(optional, type = "unknown")]
    pub default_value: Option<Value>,
    pub name: String,
    pub field_type: FieldType,
    pub indexed: bool,
    pub unique: bool,
    pub nullable: bool,
    #[ts(optional)]
    pub max_length: Option<usize>,
    /// Optional FK reference to another collection's field. Adapters
    /// emit `FOREIGN KEY (this.name) REFERENCES <collection>(<field>)
    /// ON DELETE <on_delete> ON UPDATE <on_update>` at CREATE TABLE.
    #[ts(optional)]
    #[serde(default)]
    pub foreign_key: Option<ForeignKeyRef>,
}

/// Composite index definition
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../../../protocol/typescript/orm/SchemaIndex.ts")]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SchemaIndex {
    /// Direction of the final indexed column, matching decorator metadata.
    pub descending: bool,
    pub name: String,
    pub fields: Vec<String>,
    pub unique: bool,
}

/// Collection schema - defines table structure
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/CollectionSchema.ts"
)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CollectionSchema {
    pub collection: String,
    pub fields: Vec<SchemaField>,
    pub indexes: Vec<SchemaIndex>,
}

/// Record metadata - timestamps and versioning
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/RecordMetadata.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct RecordMetadata {
    pub created_at: String,
    pub updated_at: String,
    pub version: u32,
    #[ts(optional)]
    pub tags: Option<Vec<String>>,
    #[ts(optional)]
    pub schema: Option<String>,
    #[ts(optional)]
    pub ttl: Option<u64>,
}

impl Default for RecordMetadata {
    fn default() -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            created_at: now.clone(),
            updated_at: now,
            version: 1,
            tags: None,
            schema: None,
            ttl: None,
        }
    }
}

/// Universal data record
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/orm/DataRecord.ts")]
#[serde(rename_all = "camelCase")]
pub struct DataRecord {
    pub id: UUID,
    pub collection: String,
    #[ts(type = "Record<string, unknown>")]
    pub data: Value,
    pub metadata: RecordMetadata,
}

/// Storage operation result
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/StorageResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct StorageResult<T> {
    pub success: bool,
    #[ts(optional)]
    pub data: Option<T>,
    #[ts(optional)]
    pub error: Option<String>,
    #[ts(optional)]
    pub metadata: Option<ResultMetadata>,
}

impl<T> StorageResult<T> {
    pub fn ok(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
            metadata: None,
        }
    }

    pub fn err(error: impl Into<String>) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error.into()),
            metadata: None,
        }
    }

    pub fn with_metadata(mut self, metadata: ResultMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }
}

/// Result metadata for queries
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/ResultMetadata.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ResultMetadata {
    #[ts(optional)]
    pub total_count: Option<usize>,
    #[ts(optional)]
    pub query_time_ms: Option<u64>,
    #[ts(optional)]
    pub cache_hit: Option<bool>,
}

/// Collection statistics
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/orm/CollectionStats.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct CollectionStats {
    pub name: String,
    pub record_count: usize,
    pub total_size: usize,
    pub last_modified: String,
    #[ts(optional)]
    pub schema: Option<String>,
    #[ts(optional)]
    pub indices: Option<Vec<String>>,
}


/// How a [`BatchOperation::Append`] numbers its row: `sequence` is assigned
/// `max(sequence) + 1` inside the batch's write transaction, so sequence order IS commit order
/// (a row can never be acknowledged past an earlier allocation that commits later); `unique`
/// names the field whose value identifies the row, so a duplicate reuses the first row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/orm/SequenceSpec.ts")]
pub struct SequenceSpec {
    pub sequence: String,
    pub unique: String,
}

/// What an append did: created its row at `seq`, or found the row its unique value already names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/orm/Appended.ts")]
#[serde(tag = "kind", content = "seq", rename_all = "camelCase")]
pub enum Appended {
    Fresh(#[ts(type = "number")] u64),
    Existing(#[ts(type = "number")] u64),
}

impl Appended {
    /// The row's sequence, whichever way it was reached.
    pub fn seq(self) -> u64 {
        match self {
            Self::Fresh(seq) | Self::Existing(seq) => seq,
        }
    }
}

/// One row, named: what a read or a delete carries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/orm/RowKey.ts")]
pub struct RowKey {
    pub collection: String,
    pub id: UUID,
}

/// One row's declared fields to write: what a create, update or ensure carries.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/orm/RowWrite.ts")]
pub struct RowWrite {
    pub collection: String,
    pub id: UUID,
    #[ts(type = "Record<string, unknown>")]
    #[schemars(with = "serde_json::Value")]
    pub data: Value,
}

/// One operation of an atomic batch, carrying exactly what it needs: no operation exists
/// without its row, and no write without its data.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/orm/BatchOperation.ts")]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum BatchOperation {
    Read(RowKey),
    Create(RowWrite),
    Update(RowWrite),
    Delete(RowKey),
    /// Create the row unless a row with its id already exists (an idempotent import).
    Ensure(RowWrite),
    /// Create the row at the next commit-ordered sequence value, unless a row already holds the
    /// same unique value (then nothing is written and that row's sequence is returned).
    Append { row: RowWrite, spec: SequenceSpec },
}

/// What one batch operation did; the batch committed all of them.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/orm/BatchOutcome.ts")]
#[serde(tag = "outcome", rename_all = "camelCase")]
pub enum BatchOutcome {
    Read { record: DataRecord },
    Created { record: DataRecord },
    Updated,
    /// The delete removed its row.
    Removed,
    /// The delete found no row.
    Absent,
    /// The ensure created its row.
    Ensured,
    /// The ensure found its row already there and left it untouched.
    AlreadyPresent,
    Appended { appended: Appended },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_storage_result_ok() {
        let result = StorageResult::ok("test data".to_string());
        assert!(result.success);
        assert_eq!(result.data, Some("test data".to_string()));
        assert!(result.error.is_none());
    }

    #[test]
    fn test_storage_result_err() {
        let result: StorageResult<String> = StorageResult::err("test error");
        assert!(!result.success);
        assert!(result.data.is_none());
        assert_eq!(result.error, Some("test error".to_string()));
    }

    #[test]
    fn test_record_metadata_default() {
        let meta = RecordMetadata::default();
        assert_eq!(meta.version, 1);
        assert!(!meta.created_at.is_empty());
    }
}
