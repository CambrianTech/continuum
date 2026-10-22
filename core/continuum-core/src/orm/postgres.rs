//! PostgreSQL Storage Adapter
//!
//! Implements the StorageAdapter trait for PostgreSQL databases.
//! Uses deadpool-postgres for async connection pooling with MVCC concurrency.
//!
//! Key differences from SQLite:
//! - Natively async (no worker thread needed)
//! - $1, $2, $3 parameter placeholders (not ?)
//! - JSONB for JSON fields (binary, indexed)
//! - Native BOOLEAN, BIGINT, TIMESTAMPTZ types
//! - information_schema for introspection (not sqlite_master)
//! - ~ for regex, ILIKE for case-insensitive contains
//! - MVCC: concurrent reads AND writes without lock contention

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use deadpool_postgres::{Config, ManagerConfig, Pool, RecyclingMethod, Runtime};
use serde_json::{json, Value};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_postgres::types::{Json, ToSql};
use tokio_postgres::NoTls;

use super::adapter::{naming, AdapterCapabilities, AdapterConfig, ClearAllResult, StorageAdapter};
use super::query::{FieldFilter, QueryOperator, SortDirection, StorageQuery};
use super::types::{
    BatchOperation, BatchOperationType, CollectionSchema, CollectionStats, DataRecord,
    RecordMetadata, StorageResult, METADATA_KEYS, UUID,
};

mod format_pg_error;
pub use format_pg_error::*;
mod build_where_clause;
use build_where_clause::*;
mod postgres_adapter;
mod ensure_table_exists_pg;
use ensure_table_exists_pg::*;

#[cfg(test)]
mod tests;
