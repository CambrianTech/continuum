//! SQLite Storage Adapter
//!
//! Implements the StorageAdapter trait for SQLite databases.
//! Uses a dedicated thread for SQLite operations since rusqlite::Connection
//! is not Send+Sync.

use async_trait::async_trait;
use rusqlite::{params, Connection, OpenFlags};
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::sync::{mpsc, oneshot};

use super::adapter::{
    AdapterCapabilities, AdapterConfig, ClearAllResult, StorageAdapter, naming,
};
use super::query::{FieldFilter, QueryOperator, SortDirection, StorageQuery};
use super::types::{
    BatchOperation, BatchOperationType, CollectionSchema, CollectionStats, DataRecord,
    RecordMetadata, StorageResult, UUID,
};

/// Commands sent to the SQLite worker thread
enum SqliteCommand {
    Create {
        record: DataRecord,
        reply: oneshot::Sender<StorageResult<DataRecord>>,
    },
    Read {
        collection: String,
        id: UUID,
        reply: oneshot::Sender<StorageResult<DataRecord>>,
    },
    Query {
        query: StorageQuery,
        reply: oneshot::Sender<StorageResult<Vec<DataRecord>>>,
    },
    Count {
        query: StorageQuery,
        reply: oneshot::Sender<StorageResult<usize>>,
    },
    Update {
        collection: String,
        id: UUID,
        data: Value,
        increment_version: bool,
        reply: oneshot::Sender<StorageResult<DataRecord>>,
    },
    Delete {
        collection: String,
        id: UUID,
        reply: oneshot::Sender<StorageResult<bool>>,
    },
    EnsureSchema {
        schema: CollectionSchema,
        reply: oneshot::Sender<StorageResult<bool>>,
    },
    ListCollections {
        reply: oneshot::Sender<StorageResult<Vec<String>>>,
    },
    Truncate {
        collection: String,
        reply: oneshot::Sender<StorageResult<bool>>,
    },
    ClearAll {
        reply: oneshot::Sender<StorageResult<ClearAllResult>>,
    },
    Cleanup {
        reply: oneshot::Sender<Result<(), String>>,
    },
    Close,
}

/// SQLite storage adapter - uses a dedicated worker thread
pub struct SqliteAdapter {
    /// Command sender to worker thread
    sender: Option<mpsc::Sender<SqliteCommand>>,
    /// Worker thread handle
    _handle: Option<std::thread::JoinHandle<()>>,
}

impl SqliteAdapter {
    /// Create a new SQLite adapter
    pub fn new() -> Self {
        Self {
            sender: None,
            _handle: None,
        }
    }

    /// Get sender, returning error if not initialized
    fn get_sender(&self) -> Result<&mpsc::Sender<SqliteCommand>, String> {
        self.sender
            .as_ref()
            .ok_or_else(|| "SQLite adapter not initialized".to_string())
    }
}

impl Default for SqliteAdapter {
    fn default() -> Self {
        Self::new()
    }
}

/// Worker thread that owns the SQLite connection
fn sqlite_worker(path: String, mut receiver: mpsc::Receiver<SqliteCommand>) {
    // Open connection
    let conn = match Connection::open_with_flags(
        &path,
        OpenFlags::SQLITE_OPEN_READ_WRITE
            | OpenFlags::SQLITE_OPEN_CREATE
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    ) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("SQLite open error: {}", e);
            return;
        }
    };

    // Enable WAL mode
    if let Err(e) = conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;") {
        eprintln!("PRAGMA error: {}", e);
    }

    // Process commands until channel closes
    while let Some(cmd) = receiver.blocking_recv() {
        match cmd {
            SqliteCommand::Create { record, reply } => {
                let result = do_create(&conn, record);
                let _ = reply.send(result);
            }
            SqliteCommand::Read { collection, id, reply } => {
                let result = do_read(&conn, &collection, &id);
                let _ = reply.send(result);
            }
            SqliteCommand::Query { query, reply } => {
                let result = do_query(&conn, query);
                let _ = reply.send(result);
            }
            SqliteCommand::Count { query, reply } => {
                let result = do_count(&conn, query);
                let _ = reply.send(result);
            }
            SqliteCommand::Update { collection, id, data, increment_version, reply } => {
                let result = do_update(&conn, &collection, &id, data, increment_version);
                let _ = reply.send(result);
            }
            SqliteCommand::Delete { collection, id, reply } => {
                let result = do_delete(&conn, &collection, &id);
                let _ = reply.send(result);
            }
            SqliteCommand::EnsureSchema { schema, reply } => {
                let result = do_ensure_schema(&conn, schema);
                let _ = reply.send(result);
            }
            SqliteCommand::ListCollections { reply } => {
                let result = do_list_collections(&conn);
                let _ = reply.send(result);
            }
            SqliteCommand::Truncate { collection, reply } => {
                let result = do_truncate(&conn, &collection);
                let _ = reply.send(result);
            }
            SqliteCommand::ClearAll { reply } => {
                let result = do_clear_all(&conn);
                let _ = reply.send(result);
            }
            SqliteCommand::Cleanup { reply } => {
                let result = do_cleanup(&conn);
                let _ = reply.send(result);
            }
            SqliteCommand::Close => {
                break;
            }
        }
    }
}

// ─── Synchronous Database Operations ─────────────────────────────────────────

fn do_create(conn: &Connection, record: DataRecord) -> StorageResult<DataRecord> {
    let table = naming::to_table_name(&record.collection);
    let now = chrono::Utc::now().to_rfc3339();

    // Build column list and values from data
    let mut columns = vec!["id".to_string(), "created_at".to_string(), "updated_at".to_string(), "version".to_string()];
    let mut placeholders = vec!["?", "?", "?", "?"];
    let mut values: Vec<Box<dyn rusqlite::ToSql>> = vec![
        Box::new(record.id.clone()),
        Box::new(now.clone()),
        Box::new(now.clone()),
        Box::new(1i64),
    ];

    if let Value::Object(data) = &record.data {
        for (key, value) in data {
            if key == "id" {
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
        Err(e) => StorageResult::err(format!("Insert failed: {}", e)),
    }
}

fn do_read(conn: &Connection, collection: &str, id: &UUID) -> StorageResult<DataRecord> {
    let table = naming::to_table_name(collection);
    let sql = format!("SELECT * FROM {} WHERE id = ? LIMIT 1", table);

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(e) => return StorageResult::err(format!("Prepare failed: {}", e)),
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

    let mut sql = format!("SELECT * FROM {}", table);
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
        Err(e) => return StorageResult::err(format!("Prepare failed: {}", e)),
    };

    let columns: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
    let params: Vec<Box<dyn rusqlite::ToSql>> = where_params.iter().map(value_to_sql_boxed).collect();
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

    let params: Vec<Box<dyn rusqlite::ToSql>> = where_params.iter().map(value_to_sql_boxed).collect();
    let params_ref: Vec<&dyn rusqlite::ToSql> = params.iter().map(|b| b.as_ref()).collect();

    match conn.query_row(&sql, params_ref.as_slice(), |row| row.get::<_, i64>(0)) {
        Ok(count) => StorageResult::ok(count as usize),
        Err(e) => StorageResult::err(format!("Count failed: {}", e)),
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
            if key == "id" || key == "createdAt" || key == "created_at" {
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
        Err(e) => StorageResult::err(format!("Update failed: {}", e)),
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

    // Create indexes
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

    StorageResult::ok(true)
}

fn do_list_collections(conn: &Connection) -> StorageResult<Vec<String>> {
    let mut stmt = match conn.prepare(
        "SELECT name FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'",
    ) {
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
        let value: Value = match row.get_ref(i)? {
            rusqlite::types::ValueRef::Null => Value::Null,
            rusqlite::types::ValueRef::Integer(n) => json!(n),
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

    if let Some(ref id_str) = id {
        data.insert("id".to_string(), json!(id_str));
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

fn build_where_clause(
    filter: &Option<HashMap<String, FieldFilter>>,
) -> (String, Vec<Value>) {
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
        let (tx, rx) = mpsc::channel(100);

        // Spawn worker thread
        let handle = std::thread::spawn(move || {
            sqlite_worker(path, rx);
        });

        self.sender = Some(tx);
        self._handle = Some(handle);
        Ok(())
    }

    async fn close(&mut self) -> Result<(), String> {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(SqliteCommand::Close).await;
        }
        Ok(())
    }

    async fn create(&self, record: DataRecord) -> StorageResult<DataRecord> {
        let sender = match self.get_sender() {
            Ok(s) => s,
            Err(e) => return StorageResult::err(e),
        };
        let (reply_tx, reply_rx) = oneshot::channel();
        if sender.send(SqliteCommand::Create { record, reply: reply_tx }).await.is_err() {
            return StorageResult::err("Channel closed");
        }
        reply_rx.await.unwrap_or_else(|_| StorageResult::err("Channel closed"))
    }

    async fn read(&self, collection: &str, id: &UUID) -> StorageResult<DataRecord> {
        let sender = match self.get_sender() {
            Ok(s) => s,
            Err(e) => return StorageResult::err(e),
        };
        let (reply_tx, reply_rx) = oneshot::channel();
        if sender.send(SqliteCommand::Read {
            collection: collection.to_string(),
            id: id.clone(),
            reply: reply_tx,
        }).await.is_err() {
            return StorageResult::err("Channel closed");
        }
        reply_rx.await.unwrap_or_else(|_| StorageResult::err("Channel closed"))
    }

    async fn query(&self, query: StorageQuery) -> StorageResult<Vec<DataRecord>> {
        let sender = match self.get_sender() {
            Ok(s) => s,
            Err(e) => return StorageResult::err(e),
        };
        let (reply_tx, reply_rx) = oneshot::channel();
        if sender.send(SqliteCommand::Query { query, reply: reply_tx }).await.is_err() {
            return StorageResult::err("Channel closed");
        }
        reply_rx.await.unwrap_or_else(|_| StorageResult::err("Channel closed"))
    }

    async fn query_with_join(&self, query: StorageQuery) -> StorageResult<Vec<DataRecord>> {
        // TODO: Implement proper JOIN handling in Rust
        // For now, reuse the basic query (joins are ignored)
        // TypeScript SqliteStorageAdapter handles joins properly when TS backend is enabled
        self.query(query).await
    }

    async fn count(&self, query: StorageQuery) -> StorageResult<usize> {
        let sender = match self.get_sender() {
            Ok(s) => s,
            Err(e) => return StorageResult::err(e),
        };
        let (reply_tx, reply_rx) = oneshot::channel();
        if sender.send(SqliteCommand::Count { query, reply: reply_tx }).await.is_err() {
            return StorageResult::err("Channel closed");
        }
        reply_rx.await.unwrap_or_else(|_| StorageResult::err("Channel closed"))
    }

    async fn update(
        &self,
        collection: &str,
        id: &UUID,
        data: Value,
        increment_version: bool,
    ) -> StorageResult<DataRecord> {
        let sender = match self.get_sender() {
            Ok(s) => s,
            Err(e) => return StorageResult::err(e),
        };
        let (reply_tx, reply_rx) = oneshot::channel();
        if sender.send(SqliteCommand::Update {
            collection: collection.to_string(),
            id: id.clone(),
            data,
            increment_version,
            reply: reply_tx,
        }).await.is_err() {
            return StorageResult::err("Channel closed");
        }
        reply_rx.await.unwrap_or_else(|_| StorageResult::err("Channel closed"))
    }

    async fn delete(&self, collection: &str, id: &UUID) -> StorageResult<bool> {
        let sender = match self.get_sender() {
            Ok(s) => s,
            Err(e) => return StorageResult::err(e),
        };
        let (reply_tx, reply_rx) = oneshot::channel();
        if sender.send(SqliteCommand::Delete {
            collection: collection.to_string(),
            id: id.clone(),
            reply: reply_tx,
        }).await.is_err() {
            return StorageResult::err("Channel closed");
        }
        reply_rx.await.unwrap_or_else(|_| StorageResult::err("Channel closed"))
    }

    async fn batch(&self, operations: Vec<BatchOperation>) -> StorageResult<Vec<Value>> {
        // Execute sequentially through the worker
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
        let sender = match self.get_sender() {
            Ok(s) => s,
            Err(e) => return StorageResult::err(e),
        };
        let (reply_tx, reply_rx) = oneshot::channel();
        if sender.send(SqliteCommand::EnsureSchema { schema, reply: reply_tx }).await.is_err() {
            return StorageResult::err("Channel closed");
        }
        reply_rx.await.unwrap_or_else(|_| StorageResult::err("Channel closed"))
    }

    async fn list_collections(&self) -> StorageResult<Vec<String>> {
        let sender = match self.get_sender() {
            Ok(s) => s,
            Err(e) => return StorageResult::err(e),
        };
        let (reply_tx, reply_rx) = oneshot::channel();
        if sender.send(SqliteCommand::ListCollections { reply: reply_tx }).await.is_err() {
            return StorageResult::err("Channel closed");
        }
        reply_rx.await.unwrap_or_else(|_| StorageResult::err("Channel closed"))
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

    async fn truncate(&self, collection: &str) -> StorageResult<bool> {
        let sender = match self.get_sender() {
            Ok(s) => s,
            Err(e) => return StorageResult::err(e),
        };
        let (reply_tx, reply_rx) = oneshot::channel();
        if sender.send(SqliteCommand::Truncate {
            collection: collection.to_string(),
            reply: reply_tx,
        }).await.is_err() {
            return StorageResult::err("Channel closed");
        }
        reply_rx.await.unwrap_or_else(|_| StorageResult::err("Channel closed"))
    }

    async fn clear_all(&self) -> StorageResult<ClearAllResult> {
        let sender = match self.get_sender() {
            Ok(s) => s,
            Err(e) => return StorageResult::err(e),
        };
        let (reply_tx, reply_rx) = oneshot::channel();
        if sender.send(SqliteCommand::ClearAll { reply: reply_tx }).await.is_err() {
            return StorageResult::err("Channel closed");
        }
        reply_rx.await.unwrap_or_else(|_| StorageResult::err("Channel closed"))
    }

    async fn cleanup(&self) -> Result<(), String> {
        let sender = self.get_sender()?;
        let (reply_tx, reply_rx) = oneshot::channel();
        sender.send(SqliteCommand::Cleanup { reply: reply_tx }).await
            .map_err(|_| "Channel closed".to_string())?;
        reply_rx.await.map_err(|_| "Channel closed".to_string())?
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

    #[tokio::test]
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
        assert!(create_result.success);

        let read_result = adapter.read("users", &"test-123".to_string()).await;
        assert!(read_result.success);
        let data = read_result.data.unwrap();
        assert_eq!(data.data["name"], "Joel");
    }
}
