use super::*;

/// Open a SQLite connection with the given flags and apply performance PRAGMAs.
///
/// For `:memory:` databases, uses `file::memory:?cache=shared` with SQLITE_OPEN_URI
/// so that multiple connections (writer + reader pool) share the same in-memory DB.
/// Without this, each connection to `:memory:` gets its own separate database.
pub(super) fn open_connection(path: &str, flags: OpenFlags) -> Result<Connection, String> {
    let (effective_path, effective_flags) = if path == ":memory:" {
        // Shared-cache URI: all connections see the same in-memory database
        (
            "file::memory:?cache=shared".to_string(),
            flags | OpenFlags::SQLITE_OPEN_URI,
        )
    } else {
        (path.to_string(), flags)
    };

    // Ensure parent directory exists (persona data dirs may not exist on fresh installs)
    if effective_path != "file::memory:?cache=shared" {
        if let Some(parent) = std::path::Path::new(&effective_path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
    }

    let conn = Connection::open_with_flags(&effective_path, effective_flags)
        .map_err(|e| format!("SQLite open failed: unable to open database file: {}", e))?;

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
         PRAGMA temp_store=MEMORY;\
         PRAGMA foreign_keys=ON;",
    )
    .map_err(|e| format!("SQLite PRAGMA error: {}", e))?;

    Ok(conn)
}

/// Adaptive memory management: adjust cache_size based on system memory pressure.
/// Called before operations, rate-limited to once per 10 seconds via atomic timestamp.
pub(super) fn apply_memory_pressure(conn: &Connection, last_check: &AtomicU64) {
    let now = continuum_base::time::EpochMs::now().secs();
    let last = last_check.load(Ordering::Relaxed);
    if now.saturating_sub(last) < 10 {
        return;
    }
    // CAS: only one thread applies the pressure adjustment
    if last_check
        .compare_exchange(last, now, Ordering::Relaxed, Ordering::Relaxed)
        .is_err()
    {
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
    pub(super) writer: Option<Arc<Mutex<Connection>>>,
    /// Pool of reader connections for concurrent reads
    pub(super) readers: Vec<Arc<Mutex<Connection>>>,
    /// Round-robin counter for reader selection
    pub(super) reader_index: AtomicUsize,
    /// Timestamp of last memory pressure check (unix seconds)
    pub(super) last_pressure_check: Arc<AtomicU64>,
    /// Column → declared field type, per collection, as `ensure_schema` was told
    /// (every `OrmStore` registers its schema before its first read). A TEXT column is
    /// decoded BY THIS, never by sniffing its bytes: a String field whose text happens
    /// to look like JSON stays the string it is. Collections nobody declared (the
    /// TS-style dynamic tables) keep the legacy sniff.
    pub(super) declared: Arc<std::sync::RwLock<HashMap<String, Arc<HashMap<String, FieldType>>>>>,
    /// Each sequenced collection's committed high-water, published by the committing write.
    pub(super) sequences: Arc<super::sequenced::SequenceRevisions>,
}

impl SqliteAdapter {
    pub fn new() -> Self {
        Self {
            writer: None,
            readers: Vec::new(),
            reader_index: AtomicUsize::new(0),
            last_pressure_check: Arc::new(AtomicU64::new(0)),
            declared: Arc::new(std::sync::RwLock::new(HashMap::new())),
            sequences: Arc::default(),
        }
    }

    /// The declared column types of `collection`, when a store registered them.
    pub(super) fn declared_types(&self, collection: &str) -> Option<Arc<HashMap<String, FieldType>>> {
        self.declared.read().ok()?.get(collection).cloned()
    }

    /// Get writer connection for mutations
    pub(super) fn get_writer(&self) -> Result<Arc<Mutex<Connection>>, String> {
        self.writer
            .as_ref()
            .cloned()
            .ok_or_else(|| "SQLite adapter not initialized".to_string())
    }

    /// Get next reader connection via round-robin
    pub(super) fn get_reader(&self) -> Result<Arc<Mutex<Connection>>, String> {
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
/// The SQL column type a declared field gets: THE mapping, for table creation and for
/// column evolution alike. NUMERIC affinity preserves the storage class (an inserted
/// integer reads back as integer, a float as float); REAL coerced every value to REAL and
/// broke integer deserialization.
pub(super) fn sql_column_type(field_type: &FieldType) -> &'static str {
    match field_type {
        FieldType::String | FieldType::Date | FieldType::Json | FieldType::Uuid => "TEXT",
        FieldType::Number => "NUMERIC",
        FieldType::Boolean => "INTEGER",
    }
}

/// Infer SQLite column type from a JSON value: only for a collection that declared no types.
pub(super) fn infer_sqlite_type(value: &Value) -> &'static str {
    match value {
        Value::Bool(_) => "INTEGER",
        Value::Number(n) => {
            if n.is_i64() {
                "INTEGER"
            } else {
                "REAL"
            }
        }
        Value::String(_) => "TEXT",
        Value::Array(_) | Value::Object(_) => "TEXT", // JSON stored as text
        Value::Null => "TEXT",                        // Default to TEXT for null
    }
}

/// Ensure table exists by creating it dynamically from record data.
/// This mimics TypeScript's auto-table-creation behavior.
pub(super) fn ensure_table_exists(conn: &Connection, table: &str, data: &Value) -> Result<(), String> {
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
/// `types` are the collection's declared fields (camelCase, like `data`'s keys). A column is
/// typed by its declared field; the written value's shape is consulted only for an
/// undeclared field. Typing by the value made an optional `u64` whose first write was null a
/// TEXT column, and SQLite's TEXT affinity then stored every later integer as text
/// (`staged_credit.rejected_at_ms` on the 5090: 58 rows of text, a codec error at read,
/// Codex root on build 6199, 2026-10-13).
pub(super) fn evolve_table_schema(conn: &Connection, table: &str, data: &Value, types: Option<&HashMap<String, FieldType>>) -> bool {
    // Get existing columns
    let existing: Vec<String> = match conn.prepare(&format!("PRAGMA table_info({})", table)) {
        Ok(mut stmt) => stmt
            .query_map([], |row| row.get::<_, String>(1))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default(),
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
                let col_type = match types.and_then(|t| t.get(key.as_str())) {
                    Some(declared) => sql_column_type(declared),
                    None => infer_sqlite_type(value),
                };
                let alter = format!("ALTER TABLE {} ADD COLUMN {} {}", table, col_name, col_type);
                match conn.execute(&alter, []) {
                    Ok(_) => {
                        clog_info!(
                            "Schema evolution: added column {}.{} ({})",
                            table,
                            col_name,
                            col_type
                        );
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

pub(super) fn projection_dummy(select: &Option<Vec<String>>) -> Option<Value> {
    let cols = select.as_ref()?;
    if cols.is_empty() {
        return None;
    }
    let mut dummy = serde_json::Map::new();
    for col in cols {
        dummy.insert(col.clone(), Value::Null);
    }
    Some(Value::Object(dummy))
}

pub(super) fn do_create(conn: &Connection, record: DataRecord, types: Option<&HashMap<String, FieldType>>) -> StorageResult<DataRecord> {
    let table = naming::to_table_name(&record.collection);
    let now = chrono::Utc::now().to_rfc3339();

    // Auto-create table if it doesn't exist (like TypeScript does)
    if let Err(e) = ensure_table_exists(conn, &table, &record.data) {
        return StorageResult::err(e);
    }

    let (columns, values) = super::conditional::insert_columns(&record, &now);
    let placeholders = vec!["?"; columns.len()];

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
                if evolve_table_schema(conn, &table, &record.data, types) {
                    match conn.execute(&sql, params.as_slice()) {
                        Ok(_) => {
                            return StorageResult::ok(DataRecord {
                                metadata: RecordMetadata {
                                    created_at: now.clone(),
                                    updated_at: now,
                                    version: 1,
                                    ..record.metadata
                                },
                                ..record
                            })
                        }
                        Err(e2) => {
                            return StorageResult::err(format!(
                                "Insert failed after schema evolution: {}",
                                e2
                            ))
                        }
                    }
                }
            }
            StorageResult::err(format!("Insert failed: {}", e))
        }
    }
}

pub(super) fn do_read(
    conn: &Connection,
    collection: &str,
    id: &UUID,
    types: Option<&HashMap<String, FieldType>>,
) -> StorageResult<DataRecord> {
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

    match stmt.query_row(params![id], |row| row_to_record(row, collection, &columns, types)) {
        Ok(record) => StorageResult::ok(record),
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            StorageResult::err(format!("Record not found: {}", id))
        }
        Err(e) => StorageResult::err(format!("Query failed: {}", e)),
    }
}

pub(super) fn do_query(
    conn: &Connection,
    query: StorageQuery,
    types: Option<&HashMap<String, FieldType>>,
) -> StorageResult<Vec<DataRecord>> {
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
        row_to_record(row, &query.collection, &columns, types)
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

pub(super) fn do_count(conn: &Connection, query: StorageQuery) -> StorageResult<usize> {
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

pub(super) fn do_update(
    conn: &Connection,
    collection: &str,
    id: &UUID,
    data: Value,
    increment_version: bool,
    types: Option<&HashMap<String, FieldType>>,
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
        Ok(rows) if rows > 0 => do_read(conn, collection, id, types),
        Ok(_) => StorageResult::err(format!("Record not found: {}", id)),
        Err(e) => {
            let err_msg = e.to_string();
            if err_msg.contains("has no column named") || err_msg.contains("no such column") {
                let table = naming::to_table_name(collection);
                if evolve_table_schema(conn, &table, &data, types) {
                    match conn.execute(&sql, params_ref.as_slice()) {
                        Ok(rows) if rows > 0 => return do_read(conn, collection, id, types),
                        Ok(_) => return StorageResult::err(format!("Record not found: {}", id)),
                        Err(e2) => {
                            return StorageResult::err(format!(
                                "Update failed after schema evolution: {}",
                                e2
                            ))
                        }
                    }
                }
            }
            StorageResult::err(format!("Update failed: {}", e))
        }
    }
}

pub(super) fn do_delete(conn: &Connection, collection: &str, id: &UUID) -> StorageResult<bool> {
    let table = naming::to_table_name(collection);
    let sql = format!("DELETE FROM {} WHERE id = ?", table);

    match conn.execute(&sql, params![id]) {
        Ok(rows) => StorageResult::ok(rows > 0),
        Err(e) => StorageResult::err(format!("Delete failed: {}", e)),
    }
}

/// Apply ONE batch operation on an already-open transaction.
///
/// Returns `Err` on any failure so [`do_batch`] can roll the whole batch back.
/// The pre-transaction version of this logic folded failures into a
/// `{"success": false}` JSON element and kept going, which is what let a batch
/// half-apply while reporting success.
pub(super) fn do_batch_one(
    conn: &Connection,
    op: BatchOperation,
    declared: &HashMap<String, Arc<HashMap<String, FieldType>>>,
) -> Result<Value, String> {
    match op.operation_type {
        BatchOperationType::Create => {
            let (Some(id), Some(data)) = (op.id, op.data) else {
                return Err("create requires both id and data".to_string());
            };
            let record = DataRecord {
                id,
                collection: op.collection,
                data,
                metadata: RecordMetadata::default(),
            };
            let types = declared.get(&record.collection).map(|t| t.as_ref());
            let r = do_create(conn, record, types);
            if r.success {
                Ok(json!({"success": true}))
            } else {
                Err(r.error.unwrap_or_else(|| "create failed".to_string())) // the adapter reported failure without detail; the operation still failed
            }
        }
        BatchOperationType::Read => {
            let Some(id) = op.id else {
                return Err("read requires an id".to_string());
            };
            let r = do_read(conn, &op.collection, &id, declared.get(&op.collection).map(|t| t.as_ref()));
            if r.success {
                Ok(json!({"success": true, "data": r.data}))
            } else {
                Err(r.error.unwrap_or_else(|| "read failed".to_string())) // the adapter reported failure without detail; the operation still failed
            }
        }
        BatchOperationType::Update => {
            let (Some(id), Some(data)) = (op.id, op.data) else {
                return Err("update requires both id and data".to_string());
            };
            let r = do_update(conn, &op.collection, &id, data, true, declared.get(&op.collection).map(|t| t.as_ref()));
            if r.success {
                Ok(json!({"success": true}))
            } else {
                Err(r.error.unwrap_or_else(|| "update failed".to_string())) // the adapter reported failure without detail; the operation still failed
            }
        }
        BatchOperationType::Ensure => {
            let (Some(id), Some(data)) = (op.id, op.data) else {
                return Err("ensure requires both id and data".to_string());
            };
            let record = DataRecord {
                id,
                collection: op.collection,
                data,
                metadata: RecordMetadata::default(),
            };
            let created = super::conditional::ensure_row(conn, &record)?;
            Ok(json!({"success": true, "created": created}))
        }
        BatchOperationType::Append(spec) => {
            let (Some(id), Some(data)) = (op.id, op.data) else {
                return Err("append requires both id and data".to_string());
            };
            let record = DataRecord {
                id,
                collection: op.collection,
                data,
                metadata: RecordMetadata::default(),
            };
            let sequenced = super::sequenced::append_sequenced(conn, record, &spec)?;
            Ok(json!({"success": true, "seq": sequenced.seq, "fresh": sequenced.fresh}))
        }
        BatchOperationType::Delete => {
            let Some(id) = op.id else {
                return Err("delete requires an id".to_string());
            };
            let r = do_delete(conn, &op.collection, &id);
            if r.success {
                Ok(json!({"success": true}))
            } else {
                Err(r.error.unwrap_or_else(|| "delete failed".to_string())) // the adapter reported failure without detail; the operation still failed
            }
        }
    }
}

/// Run a batch ATOMICALLY: all operations commit, or none do.
///
/// `BEGIN IMMEDIATE` takes the write lock up front rather than upgrading a read
/// lock mid-batch, so a concurrent writer cannot wedge us halfway through. The
/// caller holds the writer mutex for the whole call, so this connection is never
/// already inside a transaction.
///
/// **Why this is all-or-nothing rather than best-effort:** a partially applied
/// batch is indistinguishable, to the caller, from a complete one — the previous
/// implementation returned `StorageResult::ok` with the failures buried in the
/// per-operation JSON, and `supports_transactions: true` was declared by this
/// adapter while nothing here ever opened a transaction. A parent row that
/// survives its failed children is a record claiming more than actually happened.
pub(super) fn do_batch(
    conn: &Connection,
    operations: Vec<BatchOperation>,
    declared: &HashMap<String, Arc<HashMap<String, FieldType>>>,
) -> StorageResult<Vec<Value>> {
    if let Err(e) = conn.execute_batch("BEGIN IMMEDIATE") {
        return StorageResult::err(format!("Failed to open batch transaction: {}", e));
    }

    let mut results = Vec::with_capacity(operations.len());
    for (index, op) in operations.into_iter().enumerate() {
        match do_batch_one(conn, op, declared) {
            Ok(value) => results.push(value),
            Err(e) => {
                // Roll back FIRST, then report. The rollback failing does not
                // change the verdict — the batch did not succeed either way.
                if let Err(rollback_err) = conn.execute_batch("ROLLBACK") {
                    return StorageResult::err(format!(
                        "Batch operation {} failed ({}), and rollback also failed: {}",
                        index, e, rollback_err
                    ));
                }
                return StorageResult::err(format!(
                    "Batch operation {} failed, entire batch rolled back: {}",
                    index, e
                ));
            }
        }
    }

    if let Err(e) = conn.execute_batch("COMMIT") {
        let _ = conn.execute_batch("ROLLBACK");
        return StorageResult::err(format!("Failed to commit batch: {}", e));
    }
    StorageResult::ok(results)
}

/// SQL keyword for a CascadeRule. Same mapping for sqlite
/// (both speak SQL-standard cascade keywords).
pub(super) fn cascade_rule_sql(rule: super::super::types::CascadeRule) -> &'static str {
    use super::super::types::CascadeRule;
    match rule {
        CascadeRule::Restrict => "RESTRICT",
        CascadeRule::Cascade => "CASCADE",
        CascadeRule::SetNull => "SET NULL",
        CascadeRule::NoAction => "NO ACTION",
    }
}
