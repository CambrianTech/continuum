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
    BatchOperation, BatchOperationType, CollectionSchema, CollectionStats, DataRecord,
    RecordMetadata, StorageResult, UUID, METADATA_KEYS,
};

// No artificial cap on reader pool — AdapterConfig.max_connections controls it.
// WAL mode supports unlimited concurrent readers.

/// Open a SQLite connection with the given flags and apply performance PRAGMAs.
///
/// For `:memory:` databases, uses `file::memory:?cache=shared` with SQLITE_OPEN_URI
/// so that multiple connections (writer + reader pool) share the same in-memory DB.
/// Without this, each connection to `:memory:` gets its own separate database.
fn open_connection(path: &str, flags: OpenFlags) -> Result<Connection, String> {
    let (effective_path, effective_flags) = if path == ":memory:" {
        // Shared-cache URI: all connections see the same in-memory database
        ("file::memory:?cache=shared".to_string(), flags | OpenFlags::SQLITE_OPEN_URI)
    } else {
        (path.to_string(), flags)
    };

    let conn = Connection::open_with_flags(&effective_path, effective_flags)
        .map_err(|e| format!("SQLite open failed: {}", e))?;

    // Performance PRAGMAs — applied to every connection.
    // mmap_size=0: disabled. On macOS, mmap'd pages count toward RSS.
    // With 20+ databases × 256MB mmap = 5GB+ RSS inflation under bursty load.
    // SQLite page cache (cache_size) provides the same benefit without RSS bloat.
    conn.execute_batch(
        "PRAGMA journal_mode=WAL;\
         PRAGMA synchronous=NORMAL;\
         PRAGMA busy_timeout=5000;\
         PRAGMA cache_size=-8192;\
         PRAGMA mmap_size=0;\
         PRAGMA temp_store=MEMORY;"
    ).map_err(|e| format!("SQLite PRAGMA error: {}", e))?;

    Ok(conn)
}

/// Adaptive memory management: adjust cache_size based on system memory pressure.
/// Called before operations, rate-limited to once per 10 seconds via atomic timestamp.
fn apply_memory_pressure(conn: &Connection, last_check: &AtomicU64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let last = last_check.load(Ordering::Relaxed);
    if now.saturating_sub(last) < 10 {
        return;
    }
    // CAS: only one thread applies the pressure adjustment
    if last_check.compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed).is_err() {
        return;
    }
    let level = crate::system_resources::MemoryPressureMonitor::current_level();
    let _ = match level {
        crate::system_resources::PressureLevel::Critical
        | crate::system_resources::PressureLevel::High => {
            conn.execute_batch("PRAGMA cache_size=-2048; PRAGMA shrink_memory;")
        }
        crate::system_resources::PressureLevel::Warning => {
            conn.execute_batch("PRAGMA cache_size=-4096;")
        }
        crate::system_resources::PressureLevel::Normal => {
            conn.execute_batch("PRAGMA cache_size=-8192;")
        }
    };
}

/// SQLite storage adapter with reader pool + dedicated writer.
///
/// Reads are dispatched round-robin across N reader connections.
/// Writes go to a single dedicated writer connection (Mutex serializes).
/// WAL mode ensures readers and writers never block each other.
/// All synchronous rusqlite calls run via tokio::task::spawn_blocking.
pub struct SqliteAdapter {
    /// Single writer connection for mutations (Mutex serializes writes)
    writer: Option<Arc<Mutex<Connection>>>,
    /// Pool of reader connections for concurrent reads
    readers: Vec<Arc<Mutex<Connection>>>,
    /// Round-robin counter for reader selection
    reader_index: AtomicUsize,
    /// Timestamp of last memory pressure check (unix seconds)
    last_pressure_check: Arc<AtomicU64>,
}

impl SqliteAdapter {
    pub fn new() -> Self {
        Self {
            writer: None,
            readers: Vec::new(),
            reader_index: AtomicUsize::new(0),
            last_pressure_check: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Get writer connection for mutations
    fn get_writer(&self) -> Result<Arc<Mutex<Connection>>, String> {
        self.writer
            .as_ref()
            .cloned()
            .ok_or_else(|| "SQLite adapter not initialized".to_string())
    }

    /// Get next reader connection via round-robin
    fn get_reader(&self) -> Result<Arc<Mutex<Connection>>, String> {
        if self.readers.is_empty() {
            return Err("SQLite adapter not initialized".to_string());
        }
        let idx = self.reader_index.fetch_add(1, Ordering::Relaxed) % self.readers.len();
        Ok(self.readers[idx].clone())
    }
}

impl Default for SqliteAdapter {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Synchronous Database Operations ─────────────────────────────────────────

/// Infer SQLite column type from a JSON value.
fn infer_sqlite_type(value: &Value) -> &'static str {
    match value {
        Value::Bool(_) => "INTEGER",
        Value::Number(n) => {
            if n.is_i64() { "INTEGER" } else { "REAL" }
        }
        Value::String(_) => "TEXT",
        Value::Array(_) | Value::Object(_) => "TEXT", // JSON stored as text
        Value::Null => "TEXT",                        // Default to TEXT for null
    }
}

/// Ensure table exists by creating it dynamically from record data.
/// This mimics TypeScript's auto-table-creation behavior.
fn ensure_table_exists(conn: &Connection, table: &str, data: &Value) -> Result<(), String> {
    // Build columns from the data object, inferring types
    let mut columns = vec![
        "id TEXT PRIMARY KEY".to_string(),
        "created_at TEXT NOT NULL".to_string(),
        "updated_at TEXT NOT NULL".to_string(),
        "version INTEGER NOT NULL DEFAULT 1".to_string(),
    ];

    if let Value::Object(obj) = data {
        for (key, value) in obj {
            if METADATA_KEYS.contains(&key.as_str()) {
                continue;
            }
            let col_name = naming::to_snake_case(key);
            let col_type = infer_sqlite_type(value);
            columns.push(format!("{} {}", col_name, col_type));
        }
    }

    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {} ({})",
        table,
        columns.join(", ")
    );

    conn.execute(&sql, []).map_err(|e| {
        clog_error!("SQLite table creation error for '{}': {}", table, e);
        format!("Create table failed for '{}': {}", table, e)
    })?;
    Ok(())
}

/// Schema evolution: add missing columns to an existing table.
/// Called when INSERT/UPDATE fails with "has no column named X".
/// Returns true if columns were added (caller should retry).
fn evolve_table_schema(conn: &Connection, table: &str, data: &Value) -> bool {
    // Get existing columns
    let existing: Vec<String> = match conn.prepare(&format!("PRAGMA table_info({})", table)) {
        Ok(mut stmt) => {
            stmt.query_map([], |row| row.get::<_, String>(1))
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
                .unwrap_or_default()
        }
        Err(_) => return false,
    };

    let mut added = 0;
    if let Value::Object(obj) = data {
        for (key, value) in obj {
            if METADATA_KEYS.contains(&key.as_str()) {
                continue;
            }
            let col_name = naming::to_snake_case(key);
            if !existing.iter().any(|c| c == &col_name) {
                let col_type = infer_sqlite_type(value);
                let alter = format!("ALTER TABLE {} ADD COLUMN {} {}", table, col_name, col_type);
                match conn.execute(&alter, []) {
                    Ok(_) => {
                        clog_info!("Schema evolution: added column {}.{} ({})", table, col_name, col_type);
                        added += 1;
                    }
                    Err(e) => {
                        clog_error!("Schema evolution failed for {}.{}: {}", table, col_name, e);
                    }
                }
            }
        }
    }
    added > 0
}

fn do_create(conn: &Connection, record: DataRecord) -> StorageResult<DataRecord> {
    let table = naming::to_table_name(&record.collection);
    let now = chrono::Utc::now().to_rfc3339();

    // Auto-create table if it doesn't exist (like TypeScript does)
    if let Err(e) = ensure_table_exists(conn, &table, &record.data) {
        return StorageResult::err(e);
    }

    // Build column list and values from data
    let mut columns = vec![
        "id".to_string(),
        "created_at".to_string(),
        "updated_at".to_string(),
        "version".to_string(),
    ];
    let mut placeholders = vec!["?", "?", "?", "?"];
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![
        Box::new(record.id.clone()),
        Box::new(now.clone()),
        Box::new(now.clone()),
        Box::new(1i64),
    ];

    if let Value::Object(data) = &record.data {
        for (key, value) in data {
            if METADATA_KEYS.contains(&key.as_str()) {
                continue;
            }
            columns.push(naming::to_snake_case(key));
            placeholders.push("?");
            values.push(value_to_sql_boxed(value));
        }
    }

    let sql = format!(
        "INSERT INTO {} ({}) VALUES ({})",
        table,
        columns.join(", "),
        placeholders.join(", ")
    );

    let params: Vec<&dyn rusqlite::ToSql> = values.iter().map(|b| b.as_ref()).collect();

    match conn.execute(&sql, params.as_slice()) {
        Ok(_) => StorageResult::ok(DataRecord {
            metadata: RecordMetadata {
                created_at: now.clone(),
                updated_at: now,
                version: 1,
                ..record.metadata
            },
            ..record
        }),
        Err(e) => {
            let err_msg = e.to_string();
            if err_msg.contains("has no column named") || err_msg.contains("no such column") {
                // Schema evolution: add missing columns and retry
                if evolve_table_schema(conn, &table, &record.data) {
                    match conn.execute(&sql, params.as_slice()) {
                        Ok(_) => return StorageResult::ok(DataRecord {
                            metadata: RecordMetadata {
                                created_at: now.clone(),
                                updated_at: now,
                                version: 1,
                                ..record.metadata
                            },
                            ..record
                        }),
                        Err(e2) => return StorageResult::err(
                            format!("Insert failed after schema evolution: {}", e2)
                        ),
                    }
                }
            }
            StorageResult::err(format!("Insert failed: {}", e))
        }
    }
}

fn do_read(conn: &Connection, collection: &str, id: &UUID) -> StorageResult<DataRecord> {
    let table = naming::to_table_name(collection);
    let sql = format!("SELECT * FROM {} WHERE id = ? LIMIT 1", table);

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            // If table doesn't exist, return "not found" instead of error
            if e.to_string().contains("no such table") {
                return StorageResult::err(format!("Record not found: {}", id));
            }
            return StorageResult::err(format!("Prepare failed: {}", e));
        }
    };

    let columns: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

    match stmt.query_row(params![id], |row| row_to_record(row, collection, &columns)) {
        Ok(record) => StorageResult::ok(record),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            StorageResult::err(format!("Record not found: {}", id))
        }
        Err(e) => StorageResult::err(format!("Query failed: {}", e)),
    }
}

fn do_query(conn: &Connection, query: StorageQuery) -> StorageResult<Vec<DataRecord>> {
    let table = naming::to_table_name(&query.collection);
    let (where_clause, where_params) = build_where_clause(&query.filter);
    let order_clause = build_order_clause(&query.sort);

    let select_clause = build_select_clause(&query.select);
    let mut sql = format!("SELECT {} FROM {}", select_clause, table);
    if !where_clause.is_empty() {
        sql.push(' ');
        sql.push_str(&where_clause);
    }
    if !order_clause.is_empty() {
        sql.push(' ');
        sql.push_str(&order_clause);
    }
    if let Some(limit) = query.limit {
        sql.push_str(&format!(" LIMIT {}", limit));
    }
    if let Some(offset) = query.offset {
        sql.push_str(&format!(" OFFSET {}", offset));
    }

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => {
            // Table doesn't exist → empty results (not an error)
            if e.to_string().contains("no such table") {
                return StorageResult::ok(Vec::new());
            }
            return StorageResult::err(format!("Prepare failed: {}", e));
        }
    };

    let columns: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let params: Vec<Box<dyn rusqlite::ToSql>> =
        where_params.iter().map(value_to_sql_boxed).collect();
    let params_ref: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();

    let rows = match stmt.query_map(params_ref.as_slice(), |row| {
        row_to_record(row, &query.collection, &columns)
    }) {
        Ok(r) => r,
        Err(e) => return StorageResult::err(format!("Query failed: {}", e)),
    };

    let records: Result<Vec<_>, _> = rows.collect();
    match records {
        Ok(r) => StorageResult::ok(r),
        Err(e) => StorageResult::err(format!("Row conversion failed: {}", e)),
    }
}

fn do_count(conn: &Connection, query: StorageQuery) -> StorageResult<usize> {
    let table = naming::to_table_name(&query.collection);
    let (where_clause, where_params) = build_where_clause(&query.filter);

    let mut sql = format!("SELECT COUNT(*) FROM {}", table);
    if !where_clause.is_empty() {
        sql.push(' ');
        sql.push_str(&where_clause);
    }

    let params: Vec<Box<dyn rusqlite::ToSql>> =
        where_params.iter().map(value_to_sql_boxed).collect();
    let params_ref: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();

    match conn.query_row(&sql, params_ref.as_slice(), |row| row.get::<_, i64>(0)) {
        Ok(count) => StorageResult::ok(count as usize),
        Err(e) => {
            // Table doesn't exist → count is 0 (not an error)
            if e.to_string().contains("no such table") {
                return StorageResult::ok(0);
            }
            StorageResult::err(format!("Count failed: {}", e))
        }
    }
}

fn do_update(
    conn: &Connection,
    collection: &str,
    id: &UUID,
    data: Value,
    increment_version: bool,
) -> StorageResult<DataRecord> {
    let table = naming::to_table_name(collection);
    let now = chrono::Utc::now().to_rfc3339();

    let mut sets = vec!["updated_at = ?".to_string()];
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(now.clone())];

    if increment_version {
        sets.push("version = version + 1".to_string());
    }

    if let Value::Object(obj) = &data {
        for (key, value) in obj {
            if METADATA_KEYS.contains(&key.as_str()) {
                continue;
            }
            sets.push(format!("{} = ?", naming::to_snake_case(key)));
            values.push(value_to_sql_boxed(value));
        }
    }

    values.push(Box::new(id.clone()));

    let sql = format!("UPDATE {} SET {} WHERE id = ?", table, sets.join(", "));
    let params_ref: Vec<&dyn rusqlite::ToSql> = values.iter().map(|b| b.as_ref()).collect();

    match conn.execute(&sql, params_ref.as_slice()) {
        Ok(rows) if rows > 0 => do_read(conn, collection, id),
        Ok(_) => StorageResult::err(format!("Record not found: {}", id)),
        Err(e) => {
            let err_msg = e.to_string();
            if err_msg.contains("has no column named") || err_msg.contains("no such column") {
                let table = naming::to_table_name(collection);
                if evolve_table_schema(conn, &table, &data) {
                    match conn.execute(&sql, params_ref.as_slice()) {
                        Ok(rows) if rows > 0 => return do_read(conn, collection, id),
                        Ok(_) => return StorageResult::err(format!("Record not found: {}", id)),
                        Err(e2) => return StorageResult::err(
                            format!("Update failed after schema evolution: {}", e2)
                        ),
                    }
                }
            }
            StorageResult::err(format!("Update failed: {}", e))
        }
    }
}

fn do_delete(conn: &Connection, collection: &str, id: &UUID) -> StorageResult<bool> {
    let table = naming::to_table_name(collection);
    let sql = format!("DELETE FROM {} WHERE id = ?", table);

    match conn.execute(&sql, params![id]) {
        Ok(rows) => StorageResult::ok(rows > 0),
        Err(e) => StorageResult::err(format!("Delete failed: {}", e)),
    }
}

fn do_ensure_schema(conn: &Connection, schema: CollectionSchema) -> StorageResult<bool> {
    let table = naming::to_table_name(&schema.collection);

    let mut columns = vec![
        "id TEXT PRIMARY KEY".to_string(),
        "created_at TEXT NOT NULL".to_string(),
        "updated_at TEXT NOT NULL".to_string(),
        "version INTEGER NOT NULL DEFAULT 1".to_string(),
    ];

    for field in &schema.fields {
        let col_name = naming::to_snake_case(&field.name);
        let col_type = match field.field_type {
            super::types::FieldType::String => "TEXT",
            super::types::FieldType::Number => "REAL",
            super::types::FieldType::Boolean => "INTEGER",
            super::types::FieldType::Date => "TEXT",
            super::types::FieldType::Json => "TEXT",
            super::types::FieldType::Uuid => "TEXT",
        };

        let mut col_def = format!("{} {}", col_name, col_type);
        if !field.nullable {
            col_def.push_str(" NOT NULL");
        }
        if field.unique {
            col_def.push_str(" UNIQUE");
        }
        columns.push(col_def);
    }

    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {} ({})",
        table,
        columns.join(", ")
    );

    if let Err(e) = conn.execute(&sql, []) {
        return StorageResult::err(format!("Create table failed: {}", e));
    }

    // Create single-field indexes from schema
    for field in &schema.fields {
        if field.indexed {
            let col_name = naming::to_snake_case(&field.name);
            let idx_name = format!("idx_{}_{}", table, col_name);
            let idx_sql = format!(
                "CREATE INDEX IF NOT EXISTS {} ON {} ({})",
                idx_name, table, col_name
            );
            if let Err(e) = conn.execute(&idx_sql, []) {
                return StorageResult::err(format!("Create index failed: {}", e));
            }
        }
    }

    // Create composite indexes from schema
    for index in &schema.indexes {
        let cols: Vec<String> = index
            .fields
            .iter()
            .map(|f| naming::to_snake_case(f))
            .collect();
        let unique = if index.unique { "UNIQUE " } else { "" };
        let idx_sql = format!(
            "CREATE {}INDEX IF NOT EXISTS {} ON {} ({})",
            unique,
            naming::to_snake_case(&index.name),
            table,
            cols.join(", ")
        );
        if let Err(e) = conn.execute(&idx_sql, []) {
            return StorageResult::err(format!("Create composite index failed: {}", e));
        }
    }

    StorageResult::ok(true)
}

fn do_list_collections(conn: &Connection) -> StorageResult<Vec<String>> {
    let mut stmt = match conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'")
    {
        Ok(s) => s,
        Err(e) => return StorageResult::err(format!("Prepare failed: {}", e)),
    };

    let rows = match stmt.query_map([], |row| row.get::<_, String>(0)) {
        Ok(r) => r,
        Err(e) => return StorageResult::err(format!("Query failed: {}", e)),
    };

    let tables: Result<Vec<_>, _> = rows.collect();
    match tables {
        Ok(t) => StorageResult::ok(t),
        Err(e) => StorageResult::err(format!("Row conversion failed: {}", e)),
    }
}

fn do_truncate(conn: &Connection, collection: &str) -> StorageResult<bool> {
    let table = naming::to_table_name(collection);
    let sql = format!("DELETE FROM {}", table);

    match conn.execute(&sql, []) {
        Ok(_) => StorageResult::ok(true),
        Err(e) => StorageResult::err(format!("Truncate failed: {}", e)),
    }
}

fn do_clear_all(conn: &Connection) -> StorageResult<ClearAllResult> {
    let tables_result = do_list_collections(conn);
    let tables = match tables_result.data {
        Some(t) => t,
        None => return StorageResult::err(tables_result.error.unwrap_or_default()),
    };

    let mut cleared = Vec::new();
    for table in &tables {
        if do_truncate(conn, table).success {
            cleared.push(table.clone());
        }
    }

    StorageResult::ok(ClearAllResult {
        tables_cleared: cleared,
        records_deleted: 0,
    })
}

fn do_cleanup(conn: &Connection) -> Result<(), String> {
    conn.execute_batch("VACUUM; ANALYZE;")
        .map_err(|e| format!("Cleanup failed: {}", e))
}

// ─── Helper Functions ────────────────────────────────────────────────────────

fn value_to_sql_boxed(value: &Value) -> Box<dyn rusqlite::ToSql> {
    match value {
        Value::Null => Box::new(Option::<String>::None),
        Value::Bool(b) => Box::new(if *b { 1i64 } else { 0i64 }),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Box::new(i)
            } else if let Some(f) = n.as_f64() {
                Box::new(f)
            } else {
                Box::new(n.to_string())
            }
        }
        Value::String(s) => Box::new(s.clone()),
        Value::Array(_) | Value::Object(_) => Box::new(value.to_string()),
    }
}

fn row_to_record(
    row: &rusqlite::Row,
    collection: &str,
    columns: &[String],
) -> Result<DataRecord, rusqlite::Error> {
    let mut data = serde_json::Map::new();
    let mut id: Option<String> = None;
    let mut created_at: Option<String> = None;
    let mut updated_at: Option<String> = None;
    let mut version: Option<u32> = None;

    for (i, col) in columns.iter().enumerate() {
        // Check if this column is likely a boolean (is_*, has_*, *_active, etc.)
        let is_boolean_col = col.starts_with("is_")
            || col.starts_with("has_")
            || col.ends_with("_active")
            || col.ends_with("_enabled")
            || col.ends_with("_visible")
            || col.ends_with("_deleted");

        let value: Value = match row.get_ref(i)? {
            rusqlite::types::ValueRef::Null => Value::Null,
            rusqlite::types::ValueRef::Integer(n) => {
                // Convert 0/1 to false/true for boolean columns
                if is_boolean_col && (n == 0 || n == 1) {
                    json!(n == 1)
                } else {
                    json!(n)
                }
            }
            rusqlite::types::ValueRef::Real(n) => json!(n),
            rusqlite::types::ValueRef::Text(s) => {
                let s = std::str::from_utf8(s).unwrap_or("");
                if (s.starts_with('{') && s.ends_with('}'))
                    || (s.starts_with('[') && s.ends_with(']'))
                {
                    serde_json::from_str(s).unwrap_or_else(|_| json!(s))
                } else {
                    json!(s)
                }
            }
            rusqlite::types::ValueRef::Blob(b) => {
                json!(base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    b
                ))
            }
        };

        let camel_col = naming::to_camel_case(col);
        match col.as_str() {
            "id" => id = value.as_str().map(|s| s.to_string()),
            "created_at" => created_at = value.as_str().map(|s| s.to_string()),
            "updated_at" => updated_at = value.as_str().map(|s| s.to_string()),
            "version" => version = value.as_u64().map(|n| n as u32),
            _ => {
                data.insert(camel_col, value);
            }
        }
    }

    // Include base fields in data for TypeScript compatibility
    if let Some(ref id_str) = id {
        data.insert("id".to_string(), json!(id_str));
    }
    if let Some(ref ts) = created_at {
        data.insert("createdAt".to_string(), json!(ts));
    }
    if let Some(ref ts) = updated_at {
        data.insert("updatedAt".to_string(), json!(ts));
    }
    if let Some(v) = version {
        data.insert("version".to_string(), json!(v));
    }

    Ok(DataRecord {
        id: id.unwrap_or_default(),
        collection: collection.to_string(),
        data: Value::Object(data),
        metadata: RecordMetadata {
            created_at: created_at.unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
            updated_at: updated_at.unwrap_or_else(|| chrono::Utc::now().to_rfc3339()),
            version: version.unwrap_or(1),
            tags: None,
            schema: None,
            ttl: None,
        },
    })
}

fn build_where_clause(filter: &Option<HashMap<String, FieldFilter>>) -> (String, Vec<Value>) {
    let mut conditions = Vec::new();
    let mut params = Vec::new();

    if let Some(filters) = filter {
        for (field, filter) in filters {
            let column = naming::to_snake_case(field);
            match filter {
                FieldFilter::Value(v) => {
                    if v.is_null() {
                        conditions.push(format!("{} IS NULL", column));
                    } else {
                        conditions.push(format!("{} = ?", column));
                        params.push(v.clone());
                    }
                }
                FieldFilter::Operator(op) => match op {
                    QueryOperator::Eq(v) => {
                        conditions.push(format!("{} = ?", column));
                        params.push(v.clone());
                    }
                    QueryOperator::Ne(v) => {
                        conditions.push(format!("{} != ?", column));
                        params.push(v.clone());
                    }
                    QueryOperator::Gt(v) => {
                        conditions.push(format!("{} > ?", column));
                        params.push(v.clone());
                    }
                    QueryOperator::Gte(v) => {
                        conditions.push(format!("{} >= ?", column));
                        params.push(v.clone());
                    }
                    QueryOperator::Lt(v) => {
                        conditions.push(format!("{} < ?", column));
                        params.push(v.clone());
                    }
                    QueryOperator::Lte(v) => {
                        conditions.push(format!("{} <= ?", column));
                        params.push(v.clone());
                    }
                    QueryOperator::In(values) => {
                        let placeholders: Vec<_> = values.iter().map(|_| "?").collect();
                        conditions.push(format!("{} IN ({})", column, placeholders.join(", ")));
                        params.extend(values.iter().cloned());
                    }
                    QueryOperator::NotIn(values) => {
                        let placeholders: Vec<_> = values.iter().map(|_| "?").collect();
                        conditions.push(format!("{} NOT IN ({})", column, placeholders.join(", ")));
                        params.extend(values.iter().cloned());
                    }
                    QueryOperator::Exists(exists) => {
                        if *exists {
                            conditions.push(format!("{} IS NOT NULL", column));
                        } else {
                            conditions.push(format!("{} IS NULL", column));
                        }
                    }
                    QueryOperator::Regex(pattern) => {
                        conditions.push(format!("{} LIKE ?", column));
                        params.push(json!(format!("%{}%", pattern)));
                    }
                    QueryOperator::Contains(substr) => {
                        conditions.push(format!("{} LIKE ?", column));
                        params.push(json!(format!("%{}%", substr)));
                    }
                    QueryOperator::IsNull => {
                        conditions.push(format!("{} IS NULL", column));
                    }
                    QueryOperator::IsNotNull => {
                        conditions.push(format!("{} IS NOT NULL", column));
                    }
                },
            }
        }
    }

    if conditions.is_empty() {
        (String::new(), params)
    } else {
        (format!("WHERE {}", conditions.join(" AND ")), params)
    }
}

/// Build SELECT clause from optional column projection.
/// Converts camelCase field names to snake_case for SQL.
/// Always includes id, created_at, updated_at, version (metadata columns).
fn build_select_clause(select: &Option<Vec<String>>) -> String {
    match select {
        Some(cols) if !cols.is_empty() => {
            let mut selected: Vec<String> = vec![
                "id".to_string(),
                "created_at".to_string(),
                "updated_at".to_string(),
                "version".to_string(),
            ];
            for col in cols {
                let snake = naming::to_snake_case(col);
                if snake != "id" && snake != "created_at" && snake != "updated_at" && snake != "version" {
                    selected.push(snake);
                }
            }
            selected.join(", ")
        }
        _ => "*".to_string(),
    }
}

fn build_order_clause(sort: &Option<Vec<super::query::SortSpec>>) -> String {
    if let Some(sorts) = sort {
        if !sorts.is_empty() {
            let parts: Vec<_> = sorts
                .iter()
                .map(|s| {
                    let dir = match s.direction {
                        SortDirection::Asc => "ASC",
                        SortDirection::Desc => "DESC",
                    };
                    format!("{} {}", naming::to_snake_case(&s.field), dir)
                })
                .collect();
            return format!("ORDER BY {}", parts.join(", "));
        }
    }
    String::new()
}

// ─── Async Trait Implementation ──────────────────────────────────────────────

#[async_trait]
impl StorageAdapter for SqliteAdapter {
    fn name(&self) -> &'static str {
        "sqlite"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            supports_transactions: true,
            supports_indexing: true,
            supports_full_text_search: false,
            supports_vector_search: false,
            supports_joins: true,
            supports_batch: true,
            max_record_size: 1_000_000_000,
        }
    }

    async fn initialize(&mut self, config: AdapterConfig) -> Result<(), String> {
        let path = config.connection_string.clone();
        let reader_count = config.max_connections.saturating_sub(1).max(1);

        // Open writer connection (read-write + create)
        let writer_flags = OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        let writer_conn = open_connection(&path, writer_flags)?;
        self.writer = Some(Arc::new(Mutex::new(writer_conn)));

        // Open reader pool (read-only connections)
        let reader_flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
        for _ in 0..reader_count {
            let reader_conn = open_connection(&path, reader_flags)?;
            self.readers.push(Arc::new(Mutex::new(reader_conn)));
        }

        clog_info!(
            "SQLite adapter initialized: 1 writer + {} readers for {}",
            reader_count,
            path
        );
        Ok(())
    }

    async fn close(&mut self) -> Result<(), String> {
        // Drop all connection Arcs — connections close when last reference drops
        self.writer.take();
        self.readers.clear();
        Ok(())
    }

    // ─── READ operations → reader pool (concurrent via spawn_blocking) ──────

    async fn read(&self, collection: &str, id: &UUID) -> StorageResult<DataRecord> {
        let conn = match self.get_reader() {
            Ok(c) => c,
            Err(e) => return StorageResult::err(e),
        };
        let collection = collection.to_string();
        let id = id.clone();
        let pressure = self.last_pressure_check.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            apply_memory_pressure(&conn, &pressure);
            do_read(&conn, &collection, &id)
        })
        .await
        .unwrap_or_else(|e| StorageResult::err(format!("spawn_blocking failed: {}", e)))
    }

    async fn query(&self, query: StorageQuery) -> StorageResult<Vec<DataRecord>> {
        let conn = match self.get_reader() {
            Ok(c) => c,
            Err(e) => return StorageResult::err(e),
        };
        let pressure = self.last_pressure_check.clone();
        let collection_name = query.collection.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            apply_memory_pressure(&conn, &pressure);
            let start = std::time::Instant::now();
            let result = do_query(&conn, query);
            if start.elapsed().as_millis() > 100 {
                clog_warn!(
                    "SLOW query on {}: {}ms",
                    collection_name,
                    start.elapsed().as_millis()
                );
            }
            result
        })
        .await
        .unwrap_or_else(|e| StorageResult::err(format!("spawn_blocking failed: {}", e)))
    }

    async fn query_with_join(&self, query: StorageQuery) -> StorageResult<Vec<DataRecord>> {
        self.query(query).await
    }

    async fn count(&self, query: StorageQuery) -> StorageResult<usize> {
        let conn = match self.get_reader() {
            Ok(c) => c,
            Err(e) => return StorageResult::err(e),
        };
        let pressure = self.last_pressure_check.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            apply_memory_pressure(&conn, &pressure);
            do_count(&conn, query)
        })
        .await
        .unwrap_or_else(|e| StorageResult::err(format!("spawn_blocking failed: {}", e)))
    }

    async fn list_collections(&self) -> StorageResult<Vec<String>> {
        let conn = match self.get_reader() {
            Ok(c) => c,
            Err(e) => return StorageResult::err(e),
        };
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            do_list_collections(&conn)
        })
        .await
        .unwrap_or_else(|e| StorageResult::err(format!("spawn_blocking failed: {}", e)))
    }

    async fn collection_stats(&self, collection: &str) -> StorageResult<CollectionStats> {
        let count_result = self
            .count(StorageQuery {
                collection: collection.to_string(),
                ..Default::default()
            })
            .await;

        let record_count = count_result.data.unwrap_or(0);

        StorageResult::ok(CollectionStats {
            name: collection.to_string(),
            record_count,
            total_size: 0,
            last_modified: chrono::Utc::now().to_rfc3339(),
            schema: None,
            indices: None,
        })
    }

    // ─── WRITE operations → dedicated writer (serialized via Mutex) ─────────

    async fn create(&self, record: DataRecord) -> StorageResult<DataRecord> {
        let conn = match self.get_writer() {
            Ok(c) => c,
            Err(e) => return StorageResult::err(e),
        };
        let pressure = self.last_pressure_check.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            apply_memory_pressure(&conn, &pressure);
            do_create(&conn, record)
        })
        .await
        .unwrap_or_else(|e| StorageResult::err(format!("spawn_blocking failed: {}", e)))
    }

    async fn update(
        &self,
        collection: &str,
        id: &UUID,
        data: Value,
        increment_version: bool,
    ) -> StorageResult<DataRecord> {
        let conn = match self.get_writer() {
            Ok(c) => c,
            Err(e) => return StorageResult::err(e),
        };
        let collection = collection.to_string();
        let id = id.clone();
        let pressure = self.last_pressure_check.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            apply_memory_pressure(&conn, &pressure);
            do_update(&conn, &collection, &id, data, increment_version)
        })
        .await
        .unwrap_or_else(|e| StorageResult::err(format!("spawn_blocking failed: {}", e)))
    }

    async fn delete(&self, collection: &str, id: &UUID) -> StorageResult<bool> {
        let conn = match self.get_writer() {
            Ok(c) => c,
            Err(e) => return StorageResult::err(e),
        };
        let collection = collection.to_string();
        let id = id.clone();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            do_delete(&conn, &collection, &id)
        })
        .await
        .unwrap_or_else(|e| StorageResult::err(format!("spawn_blocking failed: {}", e)))
    }

    async fn batch(&self, operations: Vec<BatchOperation>) -> StorageResult<Vec<Value>> {
        let mut results = Vec::with_capacity(operations.len());
        for op in operations {
            let result = match op.operation_type {
                BatchOperationType::Create => {
                    if let (Some(id), Some(data)) = (op.id, op.data) {
                        let record = DataRecord {
                            id,
                            collection: op.collection,
                            data,
                            metadata: RecordMetadata::default(),
                        };
                        let r = self.create(record).await;
                        json!({"success": r.success, "error": r.error})
                    } else {
                        json!({"success": false, "error": "Missing id or data"})
                    }
                }
                BatchOperationType::Read => {
                    if let Some(id) = op.id {
                        let r = self.read(&op.collection, &id).await;
                        json!({"success": r.success, "data": r.data, "error": r.error})
                    } else {
                        json!({"success": false, "error": "Missing id"})
                    }
                }
                BatchOperationType::Update => {
                    if let (Some(id), Some(data)) = (op.id, op.data) {
                        let r = self.update(&op.collection, &id, data, true).await;
                        json!({"success": r.success, "error": r.error})
                    } else {
                        json!({"success": false, "error": "Missing id or data"})
                    }
                }
                BatchOperationType::Delete => {
                    if let Some(id) = op.id {
                        let r = self.delete(&op.collection, &id).await;
                        json!({"success": r.success, "error": r.error})
                    } else {
                        json!({"success": false, "error": "Missing id"})
                    }
                }
            };
            results.push(result);
        }
        StorageResult::ok(results)
    }

    async fn ensure_schema(&self, schema: CollectionSchema) -> StorageResult<bool> {
        let conn = match self.get_writer() {
            Ok(c) => c,
            Err(e) => return StorageResult::err(e),
        };
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            do_ensure_schema(&conn, schema)
        })
        .await
        .unwrap_or_else(|e| StorageResult::err(format!("spawn_blocking failed: {}", e)))
    }

    async fn truncate(&self, collection: &str) -> StorageResult<bool> {
        let conn = match self.get_writer() {
            Ok(c) => c,
            Err(e) => return StorageResult::err(e),
        };
        let collection = collection.to_string();
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            do_truncate(&conn, &collection)
        })
        .await
        .unwrap_or_else(|e| StorageResult::err(format!("spawn_blocking failed: {}", e)))
    }

    async fn clear_all(&self) -> StorageResult<ClearAllResult> {
        let conn = match self.get_writer() {
            Ok(c) => c,
            Err(e) => return StorageResult::err(e),
        };
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            do_clear_all(&conn)
        })
        .await
        .unwrap_or_else(|e| StorageResult::err(format!("spawn_blocking failed: {}", e)))
    }

    async fn cleanup(&self) -> Result<(), String> {
        let conn = self.get_writer()?;
        tokio::task::spawn_blocking(move || {
            let conn = conn.lock().unwrap();
            do_cleanup(&conn)
        })
        .await
        .map_err(|e| format!("spawn_blocking failed: {}", e))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    async fn setup_adapter() -> (SqliteAdapter, tempfile::TempDir) {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");

        let mut adapter = SqliteAdapter::new();
        adapter
            .initialize(AdapterConfig {
                connection_string: db_path.to_str().unwrap().to_string(),
                ..Default::default()
            })
            .await
            .unwrap();

        (adapter, dir)
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_create_and_read() {
        let (adapter, _dir) = setup_adapter().await;

        adapter
            .ensure_schema(CollectionSchema {
                collection: "users".to_string(),
                fields: vec![super::super::types::SchemaField {
                    name: "name".to_string(),
                    field_type: super::super::types::FieldType::String,
                    indexed: false,
                    unique: false,
                    nullable: false,
                    max_length: None,
                }],
                indexes: vec![],
            })
            .await;

        let record = DataRecord {
            id: "test-123".to_string(),
            collection: "users".to_string(),
            data: json!({"name": "Joel"}),
            metadata: RecordMetadata::default(),
        };

        let create_result = adapter.create(record).await;
        assert!(create_result.success, "Create failed: {:?}", create_result.error);

        let read_result = adapter.read("users", &"test-123".to_string()).await;
        assert!(read_result.success, "Read failed: {:?}", read_result.error);
        let data = read_result.data.unwrap();
        assert_eq!(data.data["name"], "Joel");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_concurrent_reads() {
        let (adapter, _dir) = setup_adapter().await;

        // Pre-create table via schema
        adapter
            .ensure_schema(CollectionSchema {
                collection: "items".to_string(),
                fields: vec![super::super::types::SchemaField {
                    name: "value".to_string(),
                    field_type: super::super::types::FieldType::Number,
                    indexed: false,
                    unique: false,
                    nullable: false,
                    max_length: None,
                }],
                indexes: vec![],
            })
            .await;

        // Create test data
        for i in 0..20 {
            let record = DataRecord {
                id: format!("item-{}", i),
                collection: "items".to_string(),
                data: json!({"value": i}),
                metadata: RecordMetadata::default(),
            };
            let result = adapter.create(record).await;
            assert!(result.success, "Create item-{} failed: {:?}", i, result.error);
        }

        // Truly concurrent reads via spawn_blocking — each goes to a different reader
        let ids: Vec<String> = (0..20).map(|i| format!("item-{}", i)).collect();
        let futures: Vec<_> = ids.iter().map(|id| adapter.read("items", id)).collect();
        let results = futures::future::join_all(futures).await;

        let success_count = results.iter().filter(|r| r.success).count();
        let errors: Vec<_> = results
            .iter()
            .enumerate()
            .filter(|(_, r)| !r.success)
            .map(|(i, r)| format!("item-{}: {:?}", i, r.error))
            .collect();
        assert_eq!(
            success_count, 20,
            "All reads should succeed (got {}/20). Errors: {:?}",
            success_count, errors
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn test_concurrent_writes() {
        let (adapter, _dir) = setup_adapter().await;

        // Concurrent writes — all serialized through the writer Mutex
        let mut futures = Vec::new();
        for i in 0..10 {
            let record = DataRecord {
                id: format!("write-{}", i),
                collection: "concurrent_writes".to_string(),
                data: json!({"value": i, "name": format!("item-{}", i)}),
                metadata: RecordMetadata::default(),
            };
            futures.push(adapter.create(record));
        }
        let results = futures::future::join_all(futures).await;

        let success_count = results.iter().filter(|r| r.success).count();
        assert_eq!(success_count, 10, "All concurrent writes should succeed");

        // Verify all data is readable
        let query_result = adapter
            .query(StorageQuery {
                collection: "concurrent_writes".to_string(),
                ..Default::default()
            })
            .await;
        assert!(query_result.success);
        assert_eq!(query_result.data.unwrap().len(), 10);
    }
}
