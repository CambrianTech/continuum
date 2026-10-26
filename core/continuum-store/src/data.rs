//! DataModule — Storage and ORM operations via the StorageAdapter trait.
//!
//! Handles: data/* commands (create, read, update, delete, query, batch)
//! Also handles: vector/* commands (vector similarity search with in-memory caching)
//! Uses the ORM module's StorageAdapter trait for database-agnostic operations.
//!
//! Callers pass opaque database handles. This module resolves handles against
//! the host's storage configuration; callers do not construct backend paths.

use crate::orm::{
    adapter::{AdapterConfig, StorageAdapter},
    migration::{
        MigrationConfig, MigrationEngine, MigrationHandle, MigrationProgress, MigrationVerification,
    },
    query::{FieldFilter, SortDirection, SortSpec, StorageQuery},
    sqlite::SqliteAdapter,
    types::{DataRecord, StorageResult, UUID},
};
use crate::runtime::{CommandRequest, CommandResponse, CommandResult, ModuleContext};
use crate::{log_error, log_info};
use chrono;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, RwLock};
use tokio::sync::{Mutex, Semaphore};

// ============================================================================
// Vector Search Types and Cache
// ============================================================================
mod adapters;

mod mutations;

mod vector_corpus;

mod vector_search;

use vector_corpus::CachedVector;

/// Phase 1 typed-IPC: deserialize raw IPC params into the typed struct
/// the handler expects, uniformly logging the parse error with
/// command-name context. Used in `handle_command` dispatch arms;
/// CONSUMES `params` (no `.clone()`) — that was one of the per-call hot-path
/// wins flagged in `docs/architecture/ORM-IDEALISM-PLAN.md`.
///
/// Usage:
///     "data/read" => self.handle_read(deserialize_params!(command, params)?).await,
///
/// `command` is the outer variable being matched on. Returns
/// `Result<TypedParams, String>`; the `?` propagates the formatted error
/// string back to the IPC dispatch layer.
macro_rules! deserialize_params {
    ($command:expr, $params:expr) => {
        serde_json::from_value($params).map_err(|e| {
            log_error!("data", "handle_command", "{} parse error: {}", $command, e);
            format!("Invalid params for {}: {}", $command, e)
        })
    };
}

mod max_concurrent_queries;
pub use max_concurrent_queries::*;
mod data_state;
mod data_state_2;
pub use data_state_2::*;
use crate::orm::list_result::DataListResult;

mod data_list_params;
pub use data_list_params::*;

#[cfg(test)]
mod tests;
