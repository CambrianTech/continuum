//! IPC Type Definitions for data-daemon worker
//!
//! **Single source of truth** — TypeScript types generated via `ts-rs`.
//! These are the wire types for communication between TypeScript and Rust
//! across the Unix socket boundary.
//!
//! Re-generate TypeScript bindings:
//!   cargo test --package data-daemon-worker export_bindings
//!
//! Output: shared/generated/data-daemon/*.ts

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use ts_rs::TS;

// ============================================================================
// Adapter Configuration Types
// ============================================================================

/// Database adapter type (determines concurrency strategy)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/data-daemon/AdapterType.ts")]
#[serde(rename_all = "lowercase")]
pub enum AdapterType {
    Sqlite,
    Postgres,
    Json,
}

/// Adapter configuration for opening a database connection
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/data-daemon/AdapterConfig.ts")]
pub struct AdapterConfig {
    pub adapter_type: AdapterType,
    pub connection_string: String,
    #[ts(skip)]
    pub options: Option<HashMap<String, Value>>,
}

/// Sort order specification for queries
#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/data-daemon/OrderBy.ts")]
pub struct OrderBy {
    pub field: String,
    /// "asc" or "desc"
    pub direction: String,
}

// ============================================================================
// Response Data Types — contents of Response::Ok { data }
//
// Each command returns a specific data shape. These types document and enforce
// the wire format so TypeScript can safely destructure responses.
// ============================================================================

/// Response data from `data/list` command
///
/// Contains query results as an array of row objects plus total count.
/// Each item is a raw SQLite row (snake_case keys, TEXT values for JSON columns).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/data-daemon/DataListResult.ts")]
pub struct DataListResult {
    /// Array of row objects from the query. Each row's shape depends on the table schema.
    /// JSON columns come back as TEXT strings — TypeScript must hydrate them.
    #[ts(type = "Array<Record<string, any>>")]
    pub items: Vec<Value>,
    /// Total number of rows matching the filter (before limit/offset)
    #[ts(type = "number")]
    pub count: usize,
}

/// Response data from `data/query` command (raw SQL)
///
/// Same shape as DataListResult but for arbitrary SQL queries.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/data-daemon/DataQueryResult.ts")]
pub struct DataQueryResult {
    #[ts(type = "Array<Record<string, any>>")]
    pub items: Vec<Value>,
    #[ts(type = "number")]
    pub count: usize,
}

/// Response data from `data/list_tables` command
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/data-daemon/ListTablesResult.ts")]
pub struct ListTablesResult {
    /// Table names in the database
    pub tables: Vec<String>,
    #[ts(type = "number")]
    pub count: usize,
}

/// A single hit from vector similarity search
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/data-daemon/VectorSearchHit.ts")]
pub struct VectorSearchHit {
    /// Record ID
    pub id: String,
    /// Cosine similarity score (0.0 to 1.0)
    pub score: f64,
    /// Distance (1.0 - score)
    pub distance: f64,
    /// Full record data when include_data=true
    #[ts(optional)]
    #[ts(type = "Record<string, any>")]
    pub data: Option<Value>,
}

/// Response data from `vector/search` command
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/data-daemon/VectorSearchResult.ts")]
pub struct VectorSearchResult {
    pub results: Vec<VectorSearchHit>,
    #[ts(type = "number")]
    pub count: usize,
    #[ts(type = "number")]
    pub corpus_size: usize,
}

/// Response data from `adapter/open` command
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/data-daemon/AdapterOpenResult.ts")]
pub struct AdapterOpenResult {
    /// Opaque handle UUID for subsequent operations
    pub handle: String,
}

/// Response data from `blob/store` command
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/data-daemon/BlobStoreResult.ts")]
pub struct BlobStoreResult {
    /// Content-addressable hash (format: "sha256:...")
    pub hash: String,
    /// Original uncompressed size in bytes
    #[ts(type = "number")]
    pub size: usize,
    /// Compressed size in bytes
    #[ts(type = "number")]
    pub compressed_size: usize,
    /// Whether the blob was deduplicated (already existed)
    pub deduplicated: bool,
    /// Timestamp when stored
    pub stored_at: String,
}

/// Response data from `blob/stats` command
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/data-daemon/BlobStatsResult.ts")]
pub struct BlobStatsResult {
    #[ts(type = "number")]
    pub total_blobs: usize,
    #[ts(type = "number")]
    pub total_compressed_bytes: usize,
    #[ts(type = "number")]
    pub shard_count: usize,
    pub base_path: String,
}

/// Response data from `blob/exists` command
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/data-daemon/BlobExistsResult.ts")]
pub struct BlobExistsResult {
    pub exists: bool,
}

/// Response data from `blob/delete` command
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/data-daemon/BlobDeleteResult.ts")]
pub struct BlobDeleteResult {
    pub deleted: bool,
}

/// Response data from write commands (data/create, data/update, data/delete, data/truncate).
///
/// The SQLite strategy serializes writes through a queue and returns results
/// for each executed statement.
///
/// Named `DataWriteResult` to avoid collision with continuum-core's file `WriteResult`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/data-daemon/DataWriteResult.ts")]
pub struct DataWriteResult {
    pub results: Vec<DataWriteRowResult>,
}

/// Result of a single write operation in the queue
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/data-daemon/DataWriteRowResult.ts")]
pub struct DataWriteRowResult {
    #[ts(type = "number")]
    pub rows_affected: usize,
}

// ============================================================================
// TypeScript Export Test
// ============================================================================

#[cfg(test)]
mod export_typescript {
    use super::*;

    #[test]
    fn export_bindings() {
        // Adapter types
        AdapterType::export().expect("Failed to export AdapterType");
        AdapterConfig::export().expect("Failed to export AdapterConfig");
        OrderBy::export().expect("Failed to export OrderBy");

        // Response data types
        DataListResult::export().expect("Failed to export DataListResult");
        DataQueryResult::export().expect("Failed to export DataQueryResult");
        ListTablesResult::export().expect("Failed to export ListTablesResult");
        VectorSearchHit::export().expect("Failed to export VectorSearchHit");
        VectorSearchResult::export().expect("Failed to export VectorSearchResult");
        AdapterOpenResult::export().expect("Failed to export AdapterOpenResult");
        DataWriteResult::export().expect("Failed to export DataWriteResult");
        DataWriteRowResult::export().expect("Failed to export DataWriteRowResult");
        BlobStoreResult::export().expect("Failed to export BlobStoreResult");
        BlobStatsResult::export().expect("Failed to export BlobStatsResult");
        BlobExistsResult::export().expect("Failed to export BlobExistsResult");
        BlobDeleteResult::export().expect("Failed to export BlobDeleteResult");

        println!("✅ data-daemon TypeScript bindings exported to shared/generated/data-daemon/");
    }
}
