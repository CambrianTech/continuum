/// Database Client - Direct SQL operations from Rust
///
/// SIMPLER ARCHITECTURE: Rust does SQL directly, no TypeScript callbacks
/// - Faster (no IPC overhead)
/// - Direct rusqlite access
/// - Works with raw JSON (no entity deserialization needed)
use rusqlite::{Connection, Result as SqliteResult};
use serde_json::Value;
use std::sync::{Arc, Mutex};

pub struct DatabaseClient {
    primary_conn: Arc<Mutex<Connection>>,
    archive_conn: Arc<Mutex<Connection>>,
}

impl DatabaseClient {
    /// Open database connections
    pub fn new(primary_path: &str, archive_path: &str) -> SqliteResult<Self> {
        let primary_conn = Connection::open(primary_path)?;
        let archive_conn = Connection::open(archive_path)?;

        Ok(Self {
            primary_conn: Arc::new(Mutex::new(primary_conn)),
            archive_conn: Arc::new(Mutex::new(archive_conn)),
        })
    }

    /// List rows from collection (source handle)
    pub fn list_rows(
        &self,
        collection: &str,
        source_handle: &str,
        limit: usize,
        order_by: &str,
    ) -> SqliteResult<Vec<Value>> {
        let conn = if source_handle == "primary" {
            &self.primary_conn
        } else {
            &self.archive_conn
        };

        let conn = conn.lock().unwrap();

        // Query rows as JSON
        let mut stmt = conn.prepare(&format!(
            "SELECT data FROM {} ORDER BY {} ASC LIMIT ?",
            collection, order_by
        ))?;

        let rows = stmt
            .query_map([limit], |row| {
                let json_str: String = row.get(0)?;
                Ok(serde_json::from_str(&json_str).unwrap_or(Value::Null))
            })?
            .filter_map(|r| r.ok())
            .collect();

        Ok(rows)
    }

    /// Insert row into collection (dest handle)
    pub fn insert_row(
        &self,
        collection: &str,
        dest_handle: &str,
        row: &Value,
    ) -> SqliteResult<()> {
        let conn = if dest_handle == "primary" {
            &self.primary_conn
        } else {
            &self.archive_conn
        };

        let conn = conn.lock().unwrap();

        // Extract ID from row
        let id = row
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| rusqlite::Error::InvalidQuery)?;

        // Insert row as JSON
        let json_str = serde_json::to_string(row).unwrap();

        conn.execute(
            &format!("INSERT OR REPLACE INTO {} (id, data) VALUES (?, ?)", collection),
            rusqlite::params![id, json_str],
        )?;

        Ok(())
    }

    /// Delete row from collection (source handle)
    pub fn delete_row(
        &self,
        collection: &str,
        source_handle: &str,
        id: &str,
    ) -> SqliteResult<()> {
        let conn = if source_handle == "primary" {
            &self.primary_conn
        } else {
            &self.archive_conn
        };

        let conn = conn.lock().unwrap();

        conn.execute(
            &format!("DELETE FROM {} WHERE id = ?", collection),
            rusqlite::params![id],
        )?;

        Ok(())
    }

    /// Count rows in collection
    pub fn count_rows(&self, collection: &str, source_handle: &str) -> SqliteResult<usize> {
        let conn = if source_handle == "primary" {
            &self.primary_conn
        } else {
            &self.archive_conn
        };

        let conn = conn.lock().unwrap();

        let count: usize = conn.query_row(
            &format!("SELECT COUNT(*) FROM {}", collection),
            [],
            |row| row.get(0),
        )?;

        Ok(count)
    }
}
