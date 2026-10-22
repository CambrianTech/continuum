use super::*;

/// Format a tokio-postgres error with full detail chain.
/// The default Display for these errors often just says "db error" — useless.
pub(super) fn format_pg_error(e: &tokio_postgres::Error) -> String {
    if let Some(db_err) = e.as_db_error() {
        format!(
            "{}: {} (column: {}, detail: {})",
            db_err.severity(),
            db_err.message(),
            db_err.column().unwrap_or("?"),
            db_err.detail().unwrap_or("none")
        )
    } else {
        format!("{:?}", e)
    }
}

/// PostgreSQL storage adapter — async-native with connection pooling
pub struct PostgresAdapter {
    pub(super) pool: Option<Pool>,
    /// Database schema (namespace) for multi-tenant isolation
    pub(super) schema: String,
    /// Cached column types per table — avoids repeated information_schema queries.
    /// Invalidated per-table when schema evolution adds new columns.
    pub(super) col_type_cache: Arc<RwLock<HashMap<String, HashMap<String, String>>>>,
    /// Per-table set of column names known to exist after a successful ensure.
    /// Lets us short-circuit ensure_table_exists_pg on the hot write path
    /// when the incoming row introduces no new columns. Process-local; if a
    /// concurrent writer ALTERs the table, the next miss-then-success rebuilds
    /// the entry.
    pub(super) ensured_columns_cache: Arc<RwLock<HashMap<String, HashSet<String>>>>,
}

impl PostgresAdapter {
    pub fn new() -> Self {
        Self {
            pool: None,
            schema: "public".to_string(),
            col_type_cache: Arc::new(RwLock::new(HashMap::new())),
            ensured_columns_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub(super) fn pool(&self) -> Result<&Pool, String> {
        self.pool
            .as_ref()
            .ok_or_else(|| "PostgreSQL adapter not initialized".to_string())
    }

    /// Get column types for a table, using cache when available.
    /// Cache is invalidated per-table when schema evolution adds columns.
    pub(super) async fn cached_column_types(
        &self,
        client: &impl deadpool_postgres::GenericClient,
        bare_table: &str,
    ) -> HashMap<String, String> {
        // Check cache first (read lock — concurrent reads OK)
        {
            let cache = self.col_type_cache.read().await;
            if let Some(types) = cache.get(bare_table) {
                return types.clone();
            }
        }
        // Cache miss — fetch from information_schema and populate
        let types = get_column_types(client, bare_table, &self.schema).await;
        if !types.is_empty() {
            let mut cache = self.col_type_cache.write().await;
            cache.insert(bare_table.to_string(), types.clone());
        }
        types
    }

    /// Invalidate cached column types for a table (after schema evolution).
    /// The ensured-columns cache is managed exclusively by
    /// `ensure_table_exists_cached`, so we do NOT touch it here — the wrapper
    /// only inserts after a real ALTER has succeeded, so the entry remains
    /// authoritative for the columns we just added.
    pub(super) async fn invalidate_column_cache(&self, bare_table: &str) {
        let mut cache = self.col_type_cache.write().await;
        cache.remove(bare_table);
    }

    /// Hot-path wrapper around `ensure_table_exists_pg`.
    ///
    /// Most writes don't introduce new columns — running CREATE IF NOT EXISTS +
    /// information_schema lookup + per-column ALTER existence check on every
    /// row burns 2 round-trips for nothing. We track the set of column names
    /// proven to exist for a table; if every column the row needs is already
    /// in that set, we skip straight to the INSERT.
    ///
    /// Returns `true` when we actually hit Postgres (caller should invalidate
    /// the column-type cache because an ALTER may have happened); `false`
    /// means the cache absorbed the call.
    pub(super) async fn ensure_table_exists_cached(
        &self,
        client: &impl deadpool_postgres::GenericClient,
        qualified_table: &str,
        bare_table: &str,
        data: &Value,
    ) -> Result<bool, String> {
        let needed: Vec<String> = if let Value::Object(obj) = data {
            obj.keys()
                .filter(|k| !METADATA_KEYS.contains(&k.as_str()))
                .map(|k| naming::to_snake_case(k))
                .collect()
        } else {
            Vec::new()
        };

        {
            let cache = self.ensured_columns_cache.read().await;
            if let Some(known) = cache.get(bare_table) {
                if needed.iter().all(|c| known.contains(c)) {
                    return Ok(false);
                }
            }
        }

        ensure_table_exists_pg(client, qualified_table, bare_table, &self.schema, data).await?;

        {
            let mut cache = self.ensured_columns_cache.write().await;
            let entry = cache.entry(bare_table.to_string()).or_insert_with(|| {
                let mut s = HashSet::new();
                s.insert("id".to_string());
                s.insert("created_at".to_string());
                s.insert("updated_at".to_string());
                s.insert("version".to_string());
                s
            });
            for c in &needed {
                entry.insert(c.clone());
            }
        }
        Ok(true)
    }

    /// Return a schema-qualified table name for SQL statements
    pub(super) fn table_ref(&self, collection: &str) -> String {
        let table = naming::to_table_name(collection);
        if self.schema == "public" {
            table
        } else {
            format!("{}.{}", self.schema, table)
        }
    }

    /// `create` with the connection INJECTED, so a caller holding an open
    /// transaction can run it on that same client. The trait method below is a
    /// thin wrapper that checks out its own connection.
    pub(super) async fn create_on(
        &self,
        client: &impl deadpool_postgres::GenericClient,
        record: DataRecord,
    ) -> StorageResult<DataRecord> {
        let bare_table = naming::to_table_name(&record.collection);
        let qualified_table = self.table_ref(&record.collection);
        let now: DateTime<Utc> = Utc::now();
        let now_rfc3339 = now.to_rfc3339();

        // Ensure table exists (auto-create from data shape).
        // Invalidate column type cache only when the cached helper actually
        // hit Postgres — if the row introduced no new columns, both caches
        // stay warm and we save 2 round-trips on the steady-state hot path.
        let schema_changed = match self
            .ensure_table_exists_cached(client, &qualified_table, &bare_table, &record.data)
            .await
        {
            Ok(c) => c,
            Err(e) => return StorageResult::err(e),
        };
        if schema_changed {
            self.invalidate_column_cache(&bare_table).await;
        }

        // Get column types for type-aware parameter coercion
        let col_types = self.cached_column_types(client, &bare_table).await;

        let (columns, params) = super::conditional::insert_columns(&record, now, &col_types);

        let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("${}", i)).collect();
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT (id) DO NOTHING",
            qualified_table,
            columns.join(", "),
            placeholders.join(", ")
        );

        let params_ref: Vec<&(dyn ToSql + Sync)> =
            params.iter().map(|b| &**b as &(dyn ToSql + Sync)).collect();

        match client.execute(&sql, &params_ref).await {
            Ok(rows) => {
                if rows == 0 {
                    // ON CONFLICT DO NOTHING — record already exists
                    return StorageResult::err(format!("Record already exists: {}", record.id));
                }
                StorageResult::ok(DataRecord {
                    metadata: RecordMetadata {
                        created_at: now_rfc3339.clone(),
                        updated_at: now_rfc3339,
                        version: 1,
                        ..record.metadata
                    },
                    ..record
                })
            }
            Err(e) => {
                // Diagnostic: include column names + types for debugging serialization errors
                let col_info: Vec<String> = columns
                    .iter()
                    .enumerate()
                    .map(|(i, c)| {
                        let pg_type = col_types.get(c).map(|t| t.as_str()).unwrap_or("?");
                        format!("${}: {}({})", i + 1, c, pg_type)
                    })
                    .collect();
                StorageResult::err(format!(
                    "Insert failed [{}]: {:?} | columns: [{}]",
                    qualified_table,
                    e,
                    col_info.join(", ")
                ))
            }
        }
    }

    /// `read` with the connection INJECTED, so a caller holding an open
    /// transaction can run it on that same client. The trait method below is a
    /// thin wrapper that checks out its own connection.
    pub(super) async fn read_on(
        &self,
        client: &impl deadpool_postgres::GenericClient,
        collection: &str,
        id: &UUID,
    ) -> StorageResult<DataRecord> {
        let table = self.table_ref(collection);

        let sql = format!("SELECT * FROM {} WHERE id = $1 LIMIT 1", table);

        let rows = match client.query(&sql, &[&id]).await {
            Ok(r) => r,
            Err(e) => {
                let msg = format_pg_error(&e);
                if msg.contains("does not exist") {
                    return StorageResult::err(format!("Record not found: {}", id));
                }
                return StorageResult::err(format!("Query failed: {}", msg));
            }
        };

        if rows.is_empty() {
            return StorageResult::err(format!("Record not found: {}", id));
        }

        match row_to_record(&rows[0], collection, rows[0].columns()) {
            Ok(record) => StorageResult::ok(record),
            Err(e) => StorageResult::err(format!("Row conversion failed: {}", e)),
        }
    }

    /// `delete` with the connection INJECTED, so a caller holding an open
    /// transaction can run it on that same client. The trait method below is a
    /// thin wrapper that checks out its own connection.
    pub(super) async fn delete_on(
        &self,
        client: &impl deadpool_postgres::GenericClient,
        collection: &str,
        id: &UUID,
    ) -> StorageResult<bool> {
        let table = self.table_ref(collection);
        let sql = format!("DELETE FROM {} WHERE id = $1", table);

        match client.execute(&sql, &[&id]).await {
            Ok(rows) => StorageResult::ok(rows > 0),
            Err(e) => StorageResult::err(format!("Delete failed: {}", e)),
        }
    }

    /// `update` with the connection INJECTED, so a caller holding an open
    /// transaction can run it on that same client. The trait method below is a
    /// thin wrapper that checks out its own connection.
    pub(super) async fn update_on(
        &self,
        client: &impl deadpool_postgres::GenericClient,
        collection: &str,
        id: &UUID,
        data: Value,
        increment_version: bool,
    ) -> StorageResult<DataRecord> {
        let bare_table = naming::to_table_name(collection);
        let table = self.table_ref(collection);
        let now: DateTime<Utc> = Utc::now();

        // Get column types for type-aware parameter coercion
        let col_types = self.cached_column_types(client, &bare_table).await;

        let mut sets = vec!["updated_at = $1".to_string()];
        let mut params: Vec<Box<dyn ToSql + Sync + Send>> = vec![Box::new(now)];
        let mut idx = 1_usize;

        if increment_version {
            sets.push("version = version + 1".to_string());
        }

        if let Value::Object(obj) = &data {
            for (key, value) in obj {
                if METADATA_KEYS.contains(&key.as_str()) {
                    continue;
                }
                idx += 1;
                let col_name = naming::to_snake_case(key);
                let pg_type = col_types.get(&col_name).map(|s| s.as_str());
                sets.push(format!("{} = ${}", col_name, idx));
                params.push(value_to_pg_typed(value, pg_type));
            }
        }

        idx += 1;
        params.push(Box::new(id.clone()));

        let sql = format!(
            "UPDATE {} SET {} WHERE id = ${}",
            table,
            sets.join(", "),
            idx
        );
        let params_ref: Vec<&(dyn ToSql + Sync)> =
            params.iter().map(|b| &**b as &(dyn ToSql + Sync)).collect();

        match client.execute(&sql, &params_ref).await {
            // read_on, NOT self.read: self.read checks out its OWN pooled
            // connection, which inside a batch transaction is a DIFFERENT session
            // that cannot see this statement's uncommitted row — a Create+Update of
            // the same new row in one batch would read back "not found" — and on a
            // one-connection pool it blocks for the 10s pool timeout first.
            Ok(rows) if rows > 0 => self.read_on(client, collection, id).await,
            Ok(_) => StorageResult::err(format!("Record not found: {}", id)),
            Err(e) => {
                // Schema evolution: auto-add missing columns and retry.
                // Evict the ensured-columns cache first — it lied (claimed a
                // column existed that Postgres just rejected), so we must hit
                // information_schema to rebuild ground truth.
                let err_msg = format_pg_error(&e);
                if err_msg.contains("does not exist") && err_msg.contains("column") {
                    self.ensured_columns_cache.write().await.remove(&bare_table);
                    if let Err(evolve_err) =
                        ensure_table_exists_pg(client, &table, &bare_table, &self.schema, &data)
                            .await
                    {
                        return StorageResult::err(format!(
                            "Update failed [{}]: {} (schema evolution also failed: {})",
                            bare_table, err_msg, evolve_err
                        ));
                    }
                    self.invalidate_column_cache(&bare_table).await;
                    // Retry the update after adding columns
                    match client.execute(&sql, &params_ref).await {
                        // Same reason as the first read-back: stay on this client.
                        Ok(rows) if rows > 0 => self.read_on(client, collection, id).await,
                        Ok(_) => StorageResult::err(format!("Record not found: {}", id)),
                        Err(e2) => StorageResult::err(format!(
                            "Update failed [{}] after schema evolution: {}",
                            bare_table,
                            format_pg_error(&e2)
                        )),
                    }
                } else {
                    StorageResult::err(format!("Update failed [{}]: {}", bare_table, err_msg))
                }
            }
        }
    }
}

impl Default for PostgresAdapter {
    fn default() -> Self {
        Self::new()
    }
}

// ─── SQL Helpers ──────────────────────────────────────────────────────────────
/// Convert a serde_json Value to a boxed ToSql, coercing types to match
/// the target PostgreSQL column type. This avoids tokio-postgres type mismatches.
///
/// Key coercions:
/// - NULL: typed to match column (Option::<f64>::None for DOUBLE PRECISION, etc.)
/// - Number → BOOLEAN: 0/1 integers from SQLite coerced to bool
/// - Number → DOUBLE PRECISION: i64 coerced to f64
/// - String → BOOLEAN: "true"/"false" coerced to bool
pub(super) fn value_to_pg_typed(value: &Value, pg_data_type: Option<&str>) -> Box<dyn ToSql + Sync + Send> {
    match value {
        Value::Null => {
            // NULL must be typed to match the target column — Postgres driver rejects
            // Option::<String>::None for a DOUBLE PRECISION column, etc.
            match pg_data_type {
                Some("double precision") | Some("real") | Some("numeric") => {
                    Box::new(Option::<f64>::None)
                }
                Some("bigint") | Some("integer") | Some("smallint") => {
                    Box::new(Option::<i64>::None)
                }
                Some("boolean") => Box::new(Option::<bool>::None),
                Some("jsonb") | Some("json") => Box::new(Option::<Json<Value>>::None),
                Some("timestamp with time zone") => Box::new(Option::<DateTime<Utc>>::None),
                Some("timestamp without time zone") => {
                    Box::new(Option::<chrono::NaiveDateTime>::None)
                }
                _ => Box::new(Option::<String>::None),
            }
        }
        Value::Bool(b) => {
            match pg_data_type {
                Some("text") | Some("character varying") => {
                    // Column is TEXT but value is boolean — serialize as string
                    Box::new(if *b {
                        "true".to_string()
                    } else {
                        "false".to_string()
                    })
                }
                Some("bigint") | Some("integer") | Some("smallint") => {
                    // Column is integer — store as 0/1
                    Box::new(if *b { 1_i64 } else { 0_i64 })
                }
                Some("jsonb") | Some("json") => Box::new(Json(value.clone())),
                _ => Box::new(*b),
            }
        }
        Value::Number(n) => {
            match pg_data_type {
                Some("boolean") => {
                    // SQLite stores booleans as 0/1 integers
                    Box::new(n.as_i64().map(|i| i != 0).unwrap_or(false))
                }
                Some("double precision") | Some("real") | Some("numeric") => {
                    Box::new(n.as_f64().unwrap_or(0.0))
                }
                Some("bigint") | Some("integer") | Some("smallint") => {
                    Box::new(n.as_i64().unwrap_or(0))
                }
                _ => {
                    // No column type info — serialize as string (TEXT is the safest default).
                    // Sending i64 to a TEXT column causes WrongType { postgres: Text, rust: "i64" }.
                    // If the column is actually numeric, Postgres implicitly casts '123'::text to int.
                    Box::new(n.to_string())
                }
            }
        }
        Value::String(s) => {
            match pg_data_type {
                Some("boolean") => {
                    // SQLite may also store booleans as "true"/"false" strings
                    let b = s.eq_ignore_ascii_case("true") || s == "1";
                    Box::new(b)
                }
                Some("bigint") | Some("integer") | Some("smallint") => {
                    // Numeric strings from SQLite
                    Box::new(s.parse::<i64>().unwrap_or(0))
                }
                Some("double precision") | Some("real") | Some("numeric") => {
                    Box::new(s.parse::<f64>().unwrap_or(0.0))
                }
                Some("jsonb") | Some("json") => {
                    // JSON stored as TEXT in SQLite
                    match serde_json::from_str::<Value>(s) {
                        Ok(v) => Box::new(Json(v)),
                        Err(_) => Box::new(s.clone()),
                    }
                }
                _ => Box::new(s.clone()),
            }
        }
        // Arrays and objects — normally JSONB, but coerce if target column differs
        Value::Array(_) | Value::Object(_) => {
            match pg_data_type {
                Some("text") | Some("character varying") => {
                    // Column is TEXT but value is JSON — serialize to string
                    Box::new(serde_json::to_string(value).unwrap_or_default())
                }
                Some("boolean") => {
                    // Shouldn't happen with fixed pg_type_from_value, but handle gracefully
                    Box::new(false)
                }
                Some("bigint") | Some("integer") | Some("smallint") => Box::new(0_i64),
                Some("double precision") | Some("real") | Some("numeric") => Box::new(0.0_f64),
                _ => Box::new(Json(value.clone())),
            }
        }
    }
}

/// Query column data types from information_schema for type-aware parameter coercion
pub(super) async fn get_column_types(
    client: &impl deadpool_postgres::GenericClient,
    table: &str,
    schema: &str,
) -> HashMap<String, String> {
    let sql = format!(
        "SELECT column_name, data_type FROM information_schema.columns \
         WHERE table_schema = '{}' AND table_name = '{}'",
        schema, table
    );
    match client.query(&sql, &[]).await {
        Ok(rows) => {
            let mut types = HashMap::new();
            for row in &rows {
                types.insert(row.get::<_, String>(0), row.get::<_, String>(1));
            }
            types
        }
        Err(_) => HashMap::new(),
    }
}

/// Infer PostgreSQL column type from a JSON value.
/// Value type takes priority over naming conventions — an Array named "sessions_active"
/// is JSONB, not BOOLEAN, regardless of the "_active" suffix.
pub(super) fn pg_type_from_value(value: &Value, col_name: &str) -> &'static str {
    // Concrete types from value — no guessing needed
    match value {
        Value::Bool(_) => return "BOOLEAN",
        Value::Array(_) | Value::Object(_) => return "JSONB",
        _ => {}
    }

    // For ambiguous types (null, number, string), use naming convention hints
    let is_boolean_col = col_name.starts_with("is_")
        || col_name.starts_with("has_")
        || col_name.ends_with("_active")
        || col_name.ends_with("_enabled")
        || col_name.ends_with("_visible")
        || col_name.ends_with("_deleted");

    if is_boolean_col {
        return "BOOLEAN";
    }

    match value {
        Value::Number(n) => {
            if n.is_i64() {
                "BIGINT"
            } else {
                "DOUBLE PRECISION"
            }
        }
        Value::String(_) | Value::Null => "TEXT",
        _ => "TEXT", // unreachable — concrete types handled above
    }
}

/// Map FieldType enum to PostgreSQL type string
pub(super) fn pg_type_from_field_type(ft: &super::super::types::FieldType) -> &'static str {
    match ft {
        super::super::types::FieldType::String => "TEXT",
        super::super::types::FieldType::Number => "DOUBLE PRECISION",
        super::super::types::FieldType::Boolean => "BOOLEAN",
        super::super::types::FieldType::Date => "TIMESTAMPTZ",
        super::super::types::FieldType::Json => "JSONB",
        super::super::types::FieldType::Uuid => "TEXT",
    }
}

/// SQL keyword for a CascadeRule under Postgres semantics. Same
/// mapping as sqlite — both speak SQL-standard cascade keywords.
pub(super) fn pg_cascade_rule_sql(rule: super::super::types::CascadeRule) -> &'static str {
    use super::super::types::CascadeRule;
    match rule {
        CascadeRule::Restrict => "RESTRICT",
        CascadeRule::Cascade => "CASCADE",
        CascadeRule::SetNull => "SET NULL",
        CascadeRule::NoAction => "NO ACTION",
    }
}
