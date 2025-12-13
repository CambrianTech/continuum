/// Data Worker - Message Types using JTAGProtocol
///
/// This uses the universal JTAGProtocol from workers/shared/jtag_protocol.rs
/// which mirrors shared/ipc/JTAGProtocol.ts on the TypeScript side.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

// Import shared JTAGProtocol types
#[path = "../../shared/jtag_protocol.rs"]
mod jtag_protocol;

// Re-export JTAG protocol types for library users
pub use jtag_protocol::{JTAGErrorType, JTAGRequest, JTAGResponse};

// ============================================================================
// Core Data Types - Match TypeScript DataStorageAdapter.ts
// ============================================================================

/// Database handle type - 'default' or UUID for multi-database operations
pub type DbHandle = String;

/// Data record metadata - versioning and timestamps
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DataRecordMetadata {
    pub created_at: String,
    pub updated_at: String,
    pub version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ttl: Option<u64>,  // Time to live in seconds
}

/// Universal data record structure - wraps all data with metadata
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DataRecord {
    pub id: String,
    pub collection: String,
    #[ts(type = "any")]
    pub data: serde_json::Value,
    pub metadata: DataRecordMetadata,
}

// ============================================================================
// Data Command Types (owned by data worker)
// ============================================================================

/// Order direction for sorting
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "lowercase")]
pub enum OrderDirection {
    Asc,
    Desc,
}

/// Order by clause for queries
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct OrderBy {
    pub field: String,
    pub direction: OrderDirection,
}

// ============================================================================
// data/list - Query with filters and ordering
// ============================================================================

/// Payload for data/list command
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DataListPayload {
    pub collection: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_handle: Option<DbHandle>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(type = "Record<string, any>", optional)]
    pub filter: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_by: Option<Vec<OrderBy>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
}

/// Result for data/list command - returns raw entities (matches TypeScript)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DataListResult {
    #[ts(type = "Array<any>")]
    pub items: Vec<serde_json::Value>,
    pub total: usize,
    pub limit: u32,
    pub offset: u32,
}

// ============================================================================
// data/read - Single document by ID
// ============================================================================

/// Payload for data/read command
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DataReadPayload {
    pub collection: String,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_handle: Option<DbHandle>,
}

/// Result for data/read command - returns raw entity (matches TypeScript)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DataReadResult {
    #[ts(type = "any | null")]
    pub data: Option<serde_json::Value>,
}

// ============================================================================
// data/create - Insert new document
// ============================================================================

/// Payload for data/create command
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DataCreatePayload {
    pub collection: String,
    #[ts(type = "any")]
    pub document: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_handle: Option<DbHandle>,
}

/// Result for data/create command - returns raw entity (matches TypeScript)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DataCreateResult {
    #[ts(type = "any")]
    pub data: serde_json::Value,
}

// ============================================================================
// data/update - Modify existing document
// ============================================================================

/// Payload for data/update command
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DataUpdatePayload {
    pub collection: String,
    pub id: String,
    #[ts(type = "Record<string, any>")]
    pub updates: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub db_handle: Option<DbHandle>,
}

/// Result for data/update command - returns raw entity (matches TypeScript)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export)]
#[serde(rename_all = "camelCase")]
pub struct DataUpdateResult {
    #[ts(type = "any")]
    pub data: serde_json::Value,
}

// ============================================================================
// TypeScript Export Test
// ============================================================================

#[cfg(test)]
mod export_typescript {
    use super::*;

    #[test]
    fn export_bindings() {
        DataRecordMetadata::export().expect("Failed to export DataRecordMetadata");
        DataRecord::export().expect("Failed to export DataRecord");
        OrderDirection::export().expect("Failed to export OrderDirection");
        OrderBy::export().expect("Failed to export OrderBy");
        DataListPayload::export().expect("Failed to export DataListPayload");
        DataListResult::export().expect("Failed to export DataListResult");
        DataReadPayload::export().expect("Failed to export DataReadPayload");
        DataReadResult::export().expect("Failed to export DataReadResult");
        DataCreatePayload::export().expect("Failed to export DataCreatePayload");
        DataCreateResult::export().expect("Failed to export DataCreateResult");
        DataUpdatePayload::export().expect("Failed to export DataUpdatePayload");
        DataUpdateResult::export().expect("Failed to export DataUpdateResult");
        println!("✅ TypeScript bindings exported to bindings/");
    }
}
