/// Data Adapter - Abstraction layer for data operations
///
/// FUTURE-PROOF DESIGN:
/// - Phase 1: Uses rusqlite directly (temporary, fast)
/// - Phase 2: Calls TypeScript DataDaemon via IPC
/// - Phase 3: Calls Rust DataDaemon directly (shared crate, no IPC)
///
/// Interface stays the same, implementation evolves.

use serde_json::Value;
use std::sync::{Arc, Mutex};

// ============================================================================
// Trait: Data Operations
// ============================================================================

/// Abstract interface for data operations
/// Implementation can be swapped without changing worker code
pub trait DataAdapter: Send + Sync {
    /// List rows from collection
    fn list_rows(
        &self,
        collection: &str,
        handle: &str,
        limit: usize,
        order_by: &str,
    ) -> Result<Vec<Value>, String>;

    /// Insert row into collection
    fn insert_row(
        &self,
        collection: &str,
        handle: &str,
        row: &Value,
    ) -> Result<(), String>;

    /// Delete row from collection
    fn delete_row(
        &self,
        collection: &str,
        handle: &str,
        id: &str,
    ) -> Result<(), String>;

    /// Count rows in collection
    fn count_rows(&self, collection: &str, handle: &str) -> Result<usize, String>;
}

// ============================================================================
// Phase 1: Direct SQLite Implementation
// ============================================================================

use rusqlite::{Connection, Result as SqliteResult};

pub struct DirectSqliteAdapter {
    primary_conn: Arc<Mutex<Connection>>,
    archive_conn: Arc<Mutex<Connection>>,
}

impl DirectSqliteAdapter {
    pub fn new(primary_path: &str, archive_path: &str) -> SqliteResult<Self> {
        let primary_conn = Connection::open(primary_path)?;
        let archive_conn = Connection::open(archive_path)?;

        Ok(Self {
            primary_conn: Arc::new(Mutex::new(primary_conn)),
            archive_conn: Arc::new(Mutex::new(archive_conn)),
        })
    }

    fn get_connection(&self, handle: &str) -> Arc<Mutex<Connection>> {
        if handle == "primary" {
            self.primary_conn.clone()
        } else {
            self.archive_conn.clone()
        }
    }
}

impl DataAdapter for DirectSqliteAdapter {
    fn list_rows(
        &self,
        collection: &str,
        handle: &str,
        limit: usize,
        order_by: &str,
    ) -> Result<Vec<Value>, String> {
        let conn = self.get_connection(handle);
        let conn = conn.lock().unwrap();

        let mut stmt = conn
            .prepare(&format!(
                "SELECT data FROM {} ORDER BY {} ASC LIMIT ?",
                collection, order_by
            ))
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map([limit], |row| {
                let json_str: String = row.get(0)?;
                Ok(serde_json::from_str(&json_str).unwrap_or(Value::Null))
            })
            .map_err(|e| e.to_string())?
            .filter_map(|r| r.ok())
            .collect();

        Ok(rows)
    }

    fn insert_row(
        &self,
        collection: &str,
        handle: &str,
        row: &Value,
    ) -> Result<(), String> {
        let conn = self.get_connection(handle);
        let conn = conn.lock().unwrap();

        let id = row
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| "Missing id field".to_string())?;

        let json_str = serde_json::to_string(row).map_err(|e| e.to_string())?;

        conn.execute(
            &format!("INSERT OR REPLACE INTO {} (id, data) VALUES (?, ?)", collection),
            rusqlite::params![id, json_str],
        )
        .map_err(|e| e.to_string())?;

        Ok(())
    }

    fn delete_row(&self, collection: &str, handle: &str, id: &str) -> Result<(), String> {
        let conn = self.get_connection(handle);
        let conn = conn.lock().unwrap();

        conn.execute(
            &format!("DELETE FROM {} WHERE id = ?", collection),
            rusqlite::params![id],
        )
        .map_err(|e| e.to_string())?;

        Ok(())
    }

    fn count_rows(&self, collection: &str, handle: &str) -> Result<usize, String> {
        let conn = self.get_connection(handle);
        let conn = conn.lock().unwrap();

        let count: usize = conn
            .query_row(
                &format!("SELECT COUNT(*) FROM {}", collection),
                [],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;

        Ok(count)
    }
}

// ============================================================================
// Phase 3: Rust DataDaemon Adapter (Future)
// ============================================================================

/// Future implementation that calls Rust DataDaemon directly
///
/// ```rust
/// // When Rust DataDaemon exists as shared crate:
/// use jtag_data_daemon::{DataDaemon, DataDaemonConfig};
///
/// pub struct RustDataDaemonAdapter {
///     daemon: Arc<DataDaemon>,
/// }
///
/// impl DataAdapter for RustDataDaemonAdapter {
///     fn list_rows(&self, collection: &str, handle: &str, limit: usize, order_by: &str) -> Result<Vec<Value>, String> {
///         // Direct function call - no IPC!
///         self.daemon.list(collection, handle, limit, order_by)
///     }
/// }
/// ```
///
/// Benefits:
/// - No IPC overhead (shared memory)
/// - Type-safe interface
/// - Same code path as TypeScript
/// - Zero-copy data access
pub struct RustDataDaemonAdapter {
    // TODO: Add when Rust DataDaemon crate exists
    // daemon: Arc<DataDaemon>,
}

impl RustDataDaemonAdapter {
    #[allow(dead_code)]
    pub fn new(/* config: DataDaemonConfig */) -> Result<Self, String> {
        // TODO: Initialize Rust DataDaemon
        Err("Rust DataDaemon not yet implemented".to_string())
    }
}

// Uncomment when Rust DataDaemon exists
// impl DataAdapter for RustDataDaemonAdapter {
//     fn list_rows(&self, collection: &str, handle: &str, limit: usize, order_by: &str) -> Result<Vec<Value>, String> {
//         self.daemon.list(collection, handle, limit, order_by)
//     }
//
//     fn insert_row(&self, collection: &str, handle: &str, row: &Value) -> Result<(), String> {
//         self.daemon.create(collection, handle, row)
//     }
//
//     fn delete_row(&self, collection: &str, handle: &str, id: &str) -> Result<(), String> {
//         self.daemon.delete(collection, handle, id)
//     }
//
//     fn count_rows(&self, collection: &str, handle: &str) -> Result<usize, String> {
//         self.daemon.count(collection, handle)
//     }
// }
