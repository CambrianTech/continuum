//! ORM Types - Database-agnostic data structures
//!
//! These types mirror the TypeScript DataStorageAdapter interface but in Rust.
//! Adapters work with these types; the ORM layer handles serialization.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;

/// UUID type (stored as string for cross-platform compatibility)
pub type UUID = String;

/// Generic record data - JSON object with string keys
pub type RecordData = serde_json::Map<String, Value>;

/// Field type for schema definition
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[ts(export, export_to = "../../../shared/generated/orm/FieldType.ts")]
#[serde(rename_all = "lowercase")]
pub enum FieldType {
    String,
    Number,
    Boolean,
    Date,
    Json,
    Uuid,
}

/// Schema field definition
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/SchemaField.ts")]
#[serde(rename_all = "camelCase")]
pub struct SchemaField {
    pub name: String,
    pub field_type: FieldType,
    #[serde(default)]
    pub indexed: bool,
    #[serde(default)]
    pub unique: bool,
    #[serde(default)]
    pub nullable: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<usize>,
}

/// Composite index definition
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/SchemaIndex.ts")]
#[serde(rename_all = "camelCase")]
pub struct SchemaIndex {
    pub name: String,
    pub fields: Vec<String>,
    #[serde(default)]
    pub unique: bool,
}

/// Collection schema - defines table structure
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/CollectionSchema.ts")]
#[serde(rename_all = "camelCase")]
pub struct CollectionSchema {
    pub collection: String,
    pub fields: Vec<SchemaField>,
    #[serde(default)]
    pub indexes: Vec<SchemaIndex>,
}

/// Record metadata - timestamps and versioning
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/RecordMetadata.ts")]
#[serde(rename_all = "camelCase")]
pub struct RecordMetadata {
    pub created_at: String,
    pub updated_at: String,
    pub version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
#[ts(export, export_to = "../../../shared/generated/orm/DataRecord.ts")]
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
#[ts(export, export_to = "../../../shared/generated/orm/StorageResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct StorageResult<T> {
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
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
#[ts(export, export_to = "../../../shared/generated/orm/ResultMetadata.ts")]
#[serde(rename_all = "camelCase")]
pub struct ResultMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub total_count: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub query_time_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_hit: Option<bool>,
}

/// Collection statistics
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/CollectionStats.ts")]
#[serde(rename_all = "camelCase")]
pub struct CollectionStats {
    pub name: String,
    pub record_count: usize,
    pub total_size: usize,
    pub last_modified: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub indices: Option<Vec<String>>,
}

/// Batch operation type
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/BatchOperationType.ts")]
#[serde(rename_all = "lowercase")]
pub enum BatchOperationType {
    Create,
    Read,
    Update,
    Delete,
}

/// Batch storage operation
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/orm/BatchOperation.ts")]
#[serde(rename_all = "camelCase")]
pub struct BatchOperation {
    pub operation_type: BatchOperationType,
    pub collection: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<UUID>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "Record<string, unknown> | undefined")]
    pub data: Option<Value>,
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
