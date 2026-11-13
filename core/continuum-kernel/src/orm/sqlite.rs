//! SQLite Storage Adapter
//!
//! Implements the StorageAdapter trait for SQLite databases.
//! Uses reader pool + dedicated writer for concurrent reads (WAL mode).
//!
//! Architecture:
//! - 1 writer connection behind Mutex: serializes mutations naturally
//! - N reader connections: handle concurrent reads via spawn_blocking
//! - WAL mode: readers never block writers, writers never block readers
//! - Round-robin dispatch across reader pool
//! - All synchronous rusqlite calls run via tokio::task::spawn_blocking

use crate::{clog_error, clog_info, clog_warn};
use async_trait::async_trait;
use rusqlite::{params, Connection, OpenFlags};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use super::adapter::{naming, AdapterCapabilities, AdapterConfig, ClearAllResult, StorageAdapter};
use super::query::{FieldFilter, QueryOperator, SortDirection, StorageQuery};
use super::types::{
    BatchOperation, BatchOperationType, CollectionSchema, CollectionStats, DataRecord, FieldType,
    RecordMetadata, StorageResult, METADATA_KEYS, UUID,
};

// No artificial cap on reader pool — AdapterConfig.max_connections controls it.
// WAL mode supports unlimited concurrent readers.

mod open_connection;
pub use open_connection::*;
mod do_ensure_schema;
use do_ensure_schema::*;
mod ensure_indexes;
pub use ensure_indexes::{IndexFailure, IndexPass};
mod sqlite_adapter;
mod conditional;
mod sequenced;
mod file;

#[cfg(test)]
mod tests;
