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
                "SELECT * FROM {} ORDER BY {} ASC LIMIT ?",
                collection, order_by
            ))
            .map_err(|e| e.to_string())?;

        let column_count = stmt.column_count();
        let column_names: Vec<String> = (0..column_count)
            .map(|i| stmt.column_name(i).unwrap().to_string())
            .collect();

        let rows = stmt
            .query_map([limit], |row| {
                let mut obj = serde_json::Map::new();

                for (i, col_name) in column_names.iter().enumerate() {
                    // Try to get value as different types
                    let value: Value = if let Ok(val) = row.get::<_, Option<String>>(i) {
                        val.map(Value::String).unwrap_or(Value::Null)
                    } else if let Ok(val) = row.get::<_, Option<i64>>(i) {
                        val.map(|v| Value::Number(v.into())).unwrap_or(Value::Null)
                    } else if let Ok(val) = row.get::<_, Option<f64>>(i) {
                        val.and_then(|v| serde_json::Number::from_f64(v))
                            .map(Value::Number)
                            .unwrap_or(Value::Null)
                    } else {
                        Value::Null
                    };

                    obj.insert(col_name.clone(), value);
                }

                Ok(Value::Object(obj))
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

        let obj = row.as_object().ok_or_else(|| "Row must be an object".to_string())?;

        // Build column names and placeholders
        let columns: Vec<String> = obj.keys().cloned().collect();
        let placeholders: Vec<String> = columns.iter().map(|_| "?".to_string()).collect();

        let sql = format!(
            "INSERT OR REPLACE INTO {} ({}) VALUES ({})",
            collection,
            columns.join(", "),
            placeholders.join(", ")
        );

        // Convert values to rusqlite params
        let params: Vec<Box<dyn rusqlite::ToSql>> = columns
            .iter()
            .map(|col| {
                let val = &obj[col];
                let param: Box<dyn rusqlite::ToSql> = match val {
                    Value::String(s) => Box::new(s.clone()),
                    Value::Number(n) => {
                        if let Some(i) = n.as_i64() {
                            Box::new(i)
                        } else if let Some(f) = n.as_f64() {
                            Box::new(f)
                        } else {
                            Box::new(None::<String>)
                        }
                    }
                    Value::Bool(b) => Box::new(*b as i64),
                    Value::Null => Box::new(None::<String>),
                    _ => Box::new(val.to_string()),
                };
                param
            })
            .collect();

        let param_refs: Vec<&dyn rusqlite::ToSql> = params.iter().map(|p| p.as_ref()).collect();

        conn.execute(&sql, &param_refs[..])
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
