use super::*;

pub(super) fn do_ensure_schema(conn: &Connection, schema: CollectionSchema) -> StorageResult<bool> {
    let table = naming::to_table_name(&schema.collection);

    let mut columns = vec![
        "id TEXT PRIMARY KEY".to_string(),
        "created_at TEXT NOT NULL".to_string(),
        "updated_at TEXT NOT NULL".to_string(),
        "version INTEGER NOT NULL DEFAULT 1".to_string(),
    ];

    for field in &schema.fields {
        let col_name = naming::to_snake_case(&field.name);
        // Skip BaseEntity columns — adapter hardcodes them above to
        // pin the exact SQL shape (PRIMARY KEY on id, defaults on
        // created_at, etc.). Schemas authored via base_entity_fields()
        // re-declare these by design; dedup here so CREATE TABLE
        // doesn't crash on the duplicate column name.
        if super::super::entity::is_base_entity_column(&col_name) {
            continue;
        }
        let col_type = sql_column_type(&field.field_type);

        let mut col_def = format!("{} {}", col_name, col_type);
        if !field.nullable {
            col_def.push_str(" NOT NULL");
        }
        if field.unique {
            col_def.push_str(" UNIQUE");
        }
        columns.push(col_def);
    }

    // Foreign keys — emit per-column FOREIGN KEY clauses inside the
    // CREATE TABLE. Per the [[no-fallbacks-ever]] doctrine + Joel
    // 2026-06-03 ("provide a relational db this time"): FK is a
    // first-class schema concept enforced by the DB, not application
    // convention. Cascade rules are explicit so cross-backend
    // semantics match.
    for field in &schema.fields {
        if let Some(fk) = &field.foreign_key {
            let col_name = naming::to_snake_case(&field.name);
            let target_table = naming::to_table_name(&fk.collection);
            let target_col = naming::to_snake_case(&fk.field);
            columns.push(format!(
                "FOREIGN KEY ({}) REFERENCES {}({}) ON DELETE {} ON UPDATE {}",
                col_name,
                target_table,
                target_col,
                cascade_rule_sql(fk.on_delete),
                cascade_rule_sql(fk.on_update),
            ));
        }
    }

    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {} ({})",
        table,
        columns.join(", ")
    );

    if let Err(e) = conn.execute(&sql, []) {
        return StorageResult::err(format!("Create table failed: {}", e));
    }

    // The declared indexes, from their one owner (`ensure_indexes`). An explicit
    // ensure-schema still reports an index the database refused, as it always did.
    let pass = super::ensure_indexes::do_ensure_indexes(conn, &schema);
    if let Some(f) = pass.failed.first() {
        return StorageResult::err(format!("Create index {} on {} failed: {}", f.index, f.collection, f.error));
    }

    StorageResult::ok(true)
}

pub(super) fn do_list_collections(conn: &Connection) -> StorageResult<Vec<String>> {
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

pub(super) fn do_truncate(conn: &Connection, collection: &str) -> StorageResult<bool> {
    let table = naming::to_table_name(collection);
    let sql = format!("DELETE FROM {}", table);

    match conn.execute(&sql, []) {
        Ok(_) => StorageResult::ok(true),
        Err(e) => StorageResult::err(format!("Truncate failed: {}", e)),
    }
}

pub(super) fn do_clear_all(conn: &Connection) -> StorageResult<ClearAllResult> {
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

pub(super) fn do_cleanup(conn: &Connection) -> Result<(), String> {
    conn.execute_batch("VACUUM; ANALYZE;")
        .map_err(|e| format!("Cleanup failed: {}", e))
}

// ─── Helper Functions ────────────────────────────────────────────────────────
pub(super) fn value_to_sql_boxed(value: &Value) -> Box<dyn rusqlite::ToSql> {
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

/// A declared number stored as text (a column TEXT affinity made, see
/// `evolve_table_schema`). Integer-shaped text (an optional `-`, then digits) is an integer
/// EXACTLY as written (i64, else u64) or `None`: one past u64::MAX is not rounded to a float
/// (Codex root, review of this fix). Other text is a finite float, else `None`. No precision
/// is lost and no text is guessed into a number.
pub(super) fn declared_number(s: &str) -> Option<Value> {
    let digits = s.strip_prefix('-').unwrap_or(s);
    if !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit()) {
        return s
            .parse::<i64>()
            .map(|n| json!(n))
            .or_else(|_| s.parse::<u64>().map(|n| json!(n)))
            .ok();
    }
    s.parse::<f64>()
        .ok()
        .filter(|f| f.is_finite())
        .and_then(serde_json::Number::from_f64)
        .map(Value::Number)
}

pub(super) fn row_to_record(
    row: &rusqlite::Row,
    collection: &str,
    columns: &[String],
    // The collection's declared column types, when a store registered them.
    types: Option<&HashMap<String, FieldType>>,
) -> Result<DataRecord, rusqlite::Error> {
    let mut data = serde_json::Map::new();
    let mut id: Option<String> = None;
    let mut created_at: Option<String> = None;
    let mut updated_at: Option<String> = None;
    let mut version: Option<u32> = None;

    for (i, col) in columns.iter().enumerate() {
        // The declared types are keyed by the ENTITY's field names (camelCase, what
        // `ensure_schema` was given); the row's columns are snake_case. Looking a
        // multi-word column up by its snake name found nothing, so every such column
        // fell back to the shape sniff the declared types exist to replace
        // (`cancel_requested` read back as 0, 2026-10-13).
        let camel_col = naming::to_camel_case(col);
        let declared = types.and_then(|t| t.get(camel_col.as_str()));
        // A boolean is stored as INTEGER 0/1. DECODE IT BY THE DECLARED TYPE, the
        // same rule the TEXT branch below follows: a `bool` field whose name the old
        // prefix sniff did not know (`cancel_requested`, 2026-10-13) came back as `0`
        // and the entity refused the row. The name sniff survives only for
        // undeclared collections.
        let is_boolean_col = match declared {
            Some(declared) => matches!(declared, FieldType::Boolean),
            None => {
                col.starts_with("is_")
                    || col.starts_with("has_")
                    || col.ends_with("_active")
                    || col.ends_with("_enabled")
                    || col.ends_with("_visible")
                    || col.ends_with("_deleted")
            }
        };

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
                // DECODE BY THE DECLARED TYPE, NEVER BY THE BYTES. Nested fields are
                // stored as JSON text, so a schema-blind reader had to sniff `{…}` /
                // `[…]` — and a String field whose text happened to be JSON-shaped (a
                // chat line `{"path": "sympy"}` admitted as an engram's content,
                // 2026-08-09) came back as a map, the entity refused it, `find_all`
                // failed whole, and the citizen whose store held that ONE row never
                // seated (Atlas + Benchy, 2026-09-13: two of five seats in backoff
                // forever). The sniff survives only for undeclared collections.
                match declared {
                    Some(FieldType::String | FieldType::Uuid | FieldType::Date) => json!(s),
                    Some(FieldType::Json) => serde_json::from_str(s).unwrap_or_else(|_| json!(s)), // unwrap_or_else: a declared-JSON column holding non-JSON text is handed back as that text, never dropped
                    Some(FieldType::Number) => declared_number(s).ok_or_else(|| {
                        rusqlite::Error::FromSqlConversionFailure(
                            i,
                            rusqlite::types::Type::Text,
                            format!("column {col} is a declared number holding {s:?}").into(),
                        )
                    })?,
                    Some(FieldType::Boolean) => match s {
                        "1" | "true" => json!(true),
                        "0" | "false" => json!(false),
                        _ => {
                            return Err(rusqlite::Error::FromSqlConversionFailure(
                                i,
                                rusqlite::types::Type::Text,
                                format!("column {col} is a declared boolean holding {s:?}").into(),
                            ))
                        }
                    },
                    None => {
                        if (s.starts_with('{') && s.ends_with('}'))
                            || (s.starts_with('[') && s.ends_with(']'))
                        {
                            serde_json::from_str(s).unwrap_or_else(|_| json!(s)) // unwrap_or_else: legacy sniff — JSON-shaped text that does not parse stays text
                        } else {
                            json!(s)
                        }
                    }
                }
            }
            rusqlite::types::ValueRef::Blob(b) => {
                json!(base64::Engine::encode(
                    &base64::engine::general_purpose::STANDARD,
                    b
                ))
            }
        };

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

pub(super) fn build_where_clause(filter: &Option<HashMap<String, FieldFilter>>) -> (String, Vec<Value>) {
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
pub(super) fn build_select_clause(select: &Option<Vec<String>>) -> String {
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
                if snake != "id"
                    && snake != "created_at"
                    && snake != "updated_at"
                    && snake != "version"
                {
                    selected.push(snake);
                }
            }
            selected.join(", ")
        }
        _ => "*".to_string(),
    }
}

pub(super) fn build_order_clause(sort: &Option<Vec<super::super::query::SortSpec>>) -> String {
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
