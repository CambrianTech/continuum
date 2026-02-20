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
use deadpool_postgres::{Config, Pool, Runtime, ManagerConfig, RecyclingMethod};
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio_postgres::types::{Json, ToSql};
use tokio_postgres::NoTls;

use super::adapter::{
    AdapterCapabilities, AdapterConfig, ClearAllResult, StorageAdapter, naming,
};
use super::query::{FieldFilter, QueryOperator, SortDirection, StorageQuery};
use super::types::{
    BatchOperation, BatchOperationType, CollectionSchema, CollectionStats, DataRecord,
    RecordMetadata, StorageResult, UUID,
};

/// PostgreSQL storage adapter — async-native with connection pooling
pub struct PostgresAdapter {
    pool: Option<Pool>,
    /// Database schema (namespace) for multi-tenant isolation
    schema: String,
}

impl PostgresAdapter {
    pub fn new() -> Self {
        Self {
            pool: None,
            schema: "public".to_string(),
        }
    }

    fn pool(&self) -> Result<&Pool, String> {
        self.pool
            .as_ref()
            .ok_or_else(|| "PostgreSQL adapter not initialized".to_string())
    }

    /// Return a schema-qualified table name for SQL statements
    fn table_ref(&self, collection: &str) -> String {
        let table = naming::to_table_name(collection);
        if self.schema == "public" {
            table
        } else {
            format!("{}.{}", self.schema, table)
        }
    }
}

impl Default for PostgresAdapter {
    fn default() -> Self {
        Self::new()
    }
}

// ─── SQL Helpers ──────────────────────────────────────────────────────────────

/// Convert a serde_json Value to a boxed ToSql for tokio-postgres.
/// Uses default type inference (i64 for integers, f64 for floats).
fn value_to_pg(value: &Value) -> Box<dyn ToSql + Sync + Send> {
    value_to_pg_typed(value, None)
}

/// Convert a serde_json Value to a boxed ToSql, coercing types to match
/// the target PostgreSQL column type. This avoids tokio-postgres type mismatches.
///
/// Key coercions:
/// - NULL: typed to match column (Option::<f64>::None for DOUBLE PRECISION, etc.)
/// - Number → BOOLEAN: 0/1 integers from SQLite coerced to bool
/// - Number → DOUBLE PRECISION: i64 coerced to f64
/// - String → BOOLEAN: "true"/"false" coerced to bool
fn value_to_pg_typed(value: &Value, pg_data_type: Option<&str>) -> Box<dyn ToSql + Sync + Send> {
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
                Some("jsonb") | Some("json") => {
                    Box::new(Option::<Json<Value>>::None)
                }
                Some("timestamp with time zone") => {
                    Box::new(Option::<DateTime<Utc>>::None)
                }
                Some("timestamp without time zone") => {
                    Box::new(Option::<chrono::NaiveDateTime>::None)
                }
                _ => Box::new(Option::<String>::None),
            }
        }
        Value::Bool(b) => Box::new(*b),
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
                    // Default: prefer i64 for integers, f64 for floats
                    if let Some(i) = n.as_i64() {
                        Box::new(i)
                    } else if let Some(f) = n.as_f64() {
                        Box::new(f)
                    } else {
                        Box::new(n.to_string())
                    }
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
async fn get_column_types(
    client: &deadpool_postgres::Client,
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
fn pg_type_from_value(value: &Value, col_name: &str) -> &'static str {
    // Concrete types from value — no guessing needed
    match value {
        Value::Bool(_) => return "BOOLEAN",
        Value::Array(_) | Value::Object(_) => return "JSONB",
        _ => {}
    }

    // For ambiguous types (null, number, string), use naming convention hints
    let is_boolean_col = col_name.starts_with("is_") || col_name.starts_with("has_")
        || col_name.ends_with("_active") || col_name.ends_with("_enabled")
        || col_name.ends_with("_visible") || col_name.ends_with("_deleted");

    if is_boolean_col {
        return "BOOLEAN";
    }

    match value {
        Value::Number(n) => {
            if n.is_i64() { "BIGINT" } else { "DOUBLE PRECISION" }
        }
        Value::String(_) | Value::Null => "TEXT",
        _ => "TEXT", // unreachable — concrete types handled above
    }
}

/// Map FieldType enum to PostgreSQL type string
fn pg_type_from_field_type(ft: &super::types::FieldType) -> &'static str {
    match ft {
        super::types::FieldType::String => "TEXT",
        super::types::FieldType::Number => "DOUBLE PRECISION",
        super::types::FieldType::Boolean => "BOOLEAN",
        super::types::FieldType::Date => "TIMESTAMPTZ",
        super::types::FieldType::Json => "JSONB",
        super::types::FieldType::Uuid => "TEXT",
    }
}

/// Build WHERE clause with $N placeholders and collect parameter values
fn build_where_clause(
    filter: &Option<HashMap<String, FieldFilter>>,
    param_offset: usize,
) -> (String, Vec<Box<dyn ToSql + Sync + Send>>) {
    let mut conditions = Vec::new();
    let mut params: Vec<Box<dyn ToSql + Sync + Send>> = Vec::new();
    let mut idx = param_offset;

    if let Some(filters) = filter {
        for (field, filter) in filters {
            let column = naming::to_snake_case(field);
            match filter {
                FieldFilter::Value(v) => {
                    if v.is_null() {
                        conditions.push(format!("{} IS NULL", column));
                    } else {
                        idx += 1;
                        conditions.push(format!("{} = ${}", column, idx));
                        params.push(value_to_pg(v));
                    }
                }
                FieldFilter::Operator(op) => match op {
                    QueryOperator::Eq(v) => {
                        idx += 1;
                        conditions.push(format!("{} = ${}", column, idx));
                        params.push(value_to_pg(v));
                    }
                    QueryOperator::Ne(v) => {
                        idx += 1;
                        conditions.push(format!("{} != ${}", column, idx));
                        params.push(value_to_pg(v));
                    }
                    QueryOperator::Gt(v) => {
                        idx += 1;
                        conditions.push(format!("{} > ${}", column, idx));
                        params.push(value_to_pg(v));
                    }
                    QueryOperator::Gte(v) => {
                        idx += 1;
                        conditions.push(format!("{} >= ${}", column, idx));
                        params.push(value_to_pg(v));
                    }
                    QueryOperator::Lt(v) => {
                        idx += 1;
                        conditions.push(format!("{} < ${}", column, idx));
                        params.push(value_to_pg(v));
                    }
                    QueryOperator::Lte(v) => {
                        idx += 1;
                        conditions.push(format!("{} <= ${}", column, idx));
                        params.push(value_to_pg(v));
                    }
                    QueryOperator::In(values) => {
                        let placeholders: Vec<String> = values.iter().map(|v| {
                            idx += 1;
                            params.push(value_to_pg(v));
                            format!("${}", idx)
                        }).collect();
                        conditions.push(format!("{} IN ({})", column, placeholders.join(", ")));
                    }
                    QueryOperator::NotIn(values) => {
                        let placeholders: Vec<String> = values.iter().map(|v| {
                            idx += 1;
                            params.push(value_to_pg(v));
                            format!("${}", idx)
                        }).collect();
                        conditions.push(format!("{} NOT IN ({})", column, placeholders.join(", ")));
                    }
                    QueryOperator::Exists(exists) => {
                        if *exists {
                            conditions.push(format!("{} IS NOT NULL", column));
                        } else {
                            conditions.push(format!("{} IS NULL", column));
                        }
                    }
                    QueryOperator::Regex(pattern) => {
                        idx += 1;
                        conditions.push(format!("{} ~ ${}", column, idx));
                        params.push(Box::new(pattern.clone()));
                    }
                    QueryOperator::Contains(substr) => {
                        idx += 1;
                        conditions.push(format!("{} ILIKE ${}", column, idx));
                        params.push(Box::new(format!("%{}%", substr)));
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

/// Build ORDER BY clause
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

/// Convert a tokio_postgres Row to a DataRecord
fn row_to_record(
    row: &tokio_postgres::Row,
    collection: &str,
    columns: &[tokio_postgres::Column],
) -> Result<DataRecord, String> {
    let mut data = serde_json::Map::new();
    let mut id: Option<String> = None;
    let mut created_at: Option<String> = None;
    let mut updated_at: Option<String> = None;
    let mut version: Option<u32> = None;

    for (i, col) in columns.iter().enumerate() {
        let col_name = col.name();
        let pg_type = col.type_();

        let value: Value = match pg_type {
            &tokio_postgres::types::Type::BOOL => {
                match row.try_get::<_, Option<bool>>(i) {
                    Ok(Some(b)) => json!(b),
                    Ok(None) => Value::Null,
                    Err(_) => Value::Null,
                }
            }
            &tokio_postgres::types::Type::INT2 => {
                match row.try_get::<_, Option<i16>>(i) {
                    Ok(Some(n)) => json!(n),
                    Ok(None) => Value::Null,
                    Err(_) => Value::Null,
                }
            }
            &tokio_postgres::types::Type::INT4 => {
                match row.try_get::<_, Option<i32>>(i) {
                    Ok(Some(n)) => json!(n),
                    Ok(None) => Value::Null,
                    Err(_) => Value::Null,
                }
            }
            &tokio_postgres::types::Type::INT8 => {
                match row.try_get::<_, Option<i64>>(i) {
                    Ok(Some(n)) => json!(n),
                    Ok(None) => Value::Null,
                    Err(_) => Value::Null,
                }
            }
            &tokio_postgres::types::Type::FLOAT4 => {
                match row.try_get::<_, Option<f32>>(i) {
                    Ok(Some(n)) => json!(n),
                    Ok(None) => Value::Null,
                    Err(_) => Value::Null,
                }
            }
            &tokio_postgres::types::Type::FLOAT8 => {
                match row.try_get::<_, Option<f64>>(i) {
                    Ok(Some(n)) => json!(n),
                    Ok(None) => Value::Null,
                    Err(_) => Value::Null,
                }
            }
            &tokio_postgres::types::Type::JSONB | &tokio_postgres::types::Type::JSON => {
                match row.try_get::<_, Option<Json<Value>>>(i) {
                    Ok(Some(j)) => j.0,
                    Ok(None) => Value::Null,
                    Err(_) => Value::Null,
                }
            }
            &tokio_postgres::types::Type::TIMESTAMPTZ => {
                match row.try_get::<_, Option<DateTime<Utc>>>(i) {
                    Ok(Some(dt)) => json!(dt.to_rfc3339()),
                    Ok(None) => Value::Null,
                    Err(_) => Value::Null,
                }
            }
            &tokio_postgres::types::Type::TIMESTAMP => {
                match row.try_get::<_, Option<chrono::NaiveDateTime>>(i) {
                    Ok(Some(ndt)) => {
                        let dt: DateTime<Utc> = DateTime::from_naive_utc_and_offset(ndt, Utc);
                        json!(dt.to_rfc3339())
                    }
                    Ok(None) => Value::Null,
                    Err(_) => Value::Null,
                }
            }
            _ => {
                // Default: try as text
                match row.try_get::<_, Option<String>>(i) {
                    Ok(Some(s)) => {
                        // Auto-detect JSON strings
                        if (s.starts_with('{') && s.ends_with('}'))
                            || (s.starts_with('[') && s.ends_with(']'))
                        {
                            serde_json::from_str(&s).unwrap_or_else(|_| json!(s))
                        } else {
                            json!(s)
                        }
                    }
                    Ok(None) => Value::Null,
                    Err(_) => Value::Null,
                }
            }
        };

        let camel_col = naming::to_camel_case(col_name);
        match col_name {
            "id" => id = value.as_str().map(|s| s.to_string()),
            "created_at" => created_at = value.as_str().map(|s| s.to_string()),
            "updated_at" => updated_at = value.as_str().map(|s| s.to_string()),
            "version" => version = value.as_u64().map(|n| n as u32).or(value.as_i64().map(|n| n as u32)),
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

// ─── Async Trait Implementation ──────────────────────────────────────────────

#[async_trait]
impl StorageAdapter for PostgresAdapter {
    fn name(&self) -> &'static str {
        "postgres"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            supports_transactions: true,
            supports_indexing: true,
            supports_full_text_search: true,
            supports_vector_search: false, // pgvector would be a separate feature
            supports_joins: true,
            supports_batch: true,
            max_record_size: 1_073_741_824, // 1GB
        }
    }

    async fn initialize(&mut self, config: AdapterConfig) -> Result<(), String> {
        if let Some(ns) = &config.namespace {
            self.schema = ns.clone();
        }

        let mut pg_config = Config::new();
        pg_config.url = Some(config.connection_string.clone());
        pg_config.manager = Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        });

        let pool = pg_config
            .create_pool(Some(Runtime::Tokio1), NoTls)
            .map_err(|e| format!("Failed to create Postgres pool: {}", e))?;

        // Verify connectivity
        let client = pool
            .get()
            .await
            .map_err(|e| format!("Failed to connect to Postgres: {}", e))?;

        // Ensure schema exists
        client
            .execute(
                &format!("CREATE SCHEMA IF NOT EXISTS {}", self.schema),
                &[],
            )
            .await
            .map_err(|e| format!("Failed to create schema: {}", e))?;

        // Set search_path for this connection test
        client
            .execute(
                &format!("SET search_path TO {}", self.schema),
                &[],
            )
            .await
            .map_err(|e| format!("Failed to set search_path: {}", e))?;

        self.pool = Some(pool);
        Ok(())
    }

    async fn close(&mut self) -> Result<(), String> {
        // Drop the pool - all connections will be closed
        self.pool = None;
        Ok(())
    }

    async fn create(&self, record: DataRecord) -> StorageResult<DataRecord> {
        let pool = match self.pool() {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => return StorageResult::err(format!("Pool error: {}", e)),
        };

        let bare_table = naming::to_table_name(&record.collection);
        let qualified_table = self.table_ref(&record.collection);
        let now: DateTime<Utc> = Utc::now();
        let now_rfc3339 = now.to_rfc3339();

        // Ensure table exists (auto-create from data shape)
        if let Err(e) = ensure_table_exists_pg(&client, &qualified_table, &bare_table, &self.schema, &record.data).await {
            return StorageResult::err(e);
        }

        // Get column types for type-aware parameter coercion
        let col_types = get_column_types(&client, &bare_table, &self.schema).await;

        // Build column list and values
        let mut columns = vec![
            "id".to_string(),
            "created_at".to_string(),
            "updated_at".to_string(),
            "version".to_string(),
        ];
        let mut params: Vec<Box<dyn ToSql + Sync + Send>> = vec![
            Box::new(record.id.clone()),
            Box::new(now),
            Box::new(now),
            Box::new(1_i64),
        ];

        if let Value::Object(data) = &record.data {
            for (key, value) in data {
                if key == "id" || key == "createdAt" || key == "created_at"
                    || key == "updatedAt" || key == "updated_at"
                    || key == "version"
                {
                    continue;
                }
                let col_name = naming::to_snake_case(key);
                let pg_type = col_types.get(&col_name).map(|s| s.as_str());
                columns.push(col_name);
                params.push(value_to_pg_typed(value, pg_type));
            }
        }

        let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("${}", i)).collect();
        let sql = format!(
            "INSERT INTO {} ({}) VALUES ({}) ON CONFLICT (id) DO NOTHING",
            qualified_table,
            columns.join(", "),
            placeholders.join(", ")
        );

        let params_ref: Vec<&(dyn ToSql + Sync)> = params.iter().map(|b| &**b as &(dyn ToSql + Sync)).collect();

        match client.execute(&sql, &params_ref).await {
            Ok(rows) => {
                if rows == 0 {
                    // ON CONFLICT DO NOTHING — record already exists
                    return StorageResult::err(format!(
                        "Record already exists: {}", record.id
                    ));
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
                let col_info: Vec<String> = columns.iter().enumerate().map(|(i, c)| {
                    let pg_type = col_types.get(c).map(|t| t.as_str()).unwrap_or("?");
                    format!("${}: {}({})", i + 1, c, pg_type)
                }).collect();
                StorageResult::err(format!(
                    "Insert failed [{}]: {:?} | columns: [{}]",
                    qualified_table, e, col_info.join(", ")
                ))
            }
        }
    }

    async fn read(&self, collection: &str, id: &UUID) -> StorageResult<DataRecord> {
        let pool = match self.pool() {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => return StorageResult::err(format!("Pool error: {}", e)),
        };

        let table = self.table_ref(collection);

        let sql = format!(
            "SELECT * FROM {} WHERE id = $1 LIMIT 1",
            table
        );

        let rows = match client.query(&sql, &[&id]).await {
            Ok(r) => r,
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("does not exist") {
                    return StorageResult::err(format!("Record not found: {}", id));
                }
                return StorageResult::err(format!("Query failed: {}", e));
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

    async fn query(&self, query: StorageQuery) -> StorageResult<Vec<DataRecord>> {
        let pool = match self.pool() {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => return StorageResult::err(format!("Pool error: {}", e)),
        };

        let table = self.table_ref(&query.collection);
        let (where_clause, where_params) = build_where_clause(&query.filter, 0);
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

        let params_ref: Vec<&(dyn ToSql + Sync)> = where_params.iter().map(|b| &**b as &(dyn ToSql + Sync)).collect();

        let rows = match client.query(&sql, &params_ref).await {
            Ok(r) => r,
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("does not exist") {
                    return StorageResult::ok(Vec::new());
                }
                return StorageResult::err(format!("Query failed: {}", e));
            }
        };

        let mut records = Vec::with_capacity(rows.len());
        for row in &rows {
            match row_to_record(row, &query.collection, row.columns()) {
                Ok(record) => records.push(record),
                Err(e) => return StorageResult::err(format!("Row conversion failed: {}", e)),
            }
        }

        StorageResult::ok(records)
    }

    async fn query_with_join(&self, query: StorageQuery) -> StorageResult<Vec<DataRecord>> {
        let pool = match self.pool() {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => return StorageResult::err(format!("Pool error: {}", e)),
        };

        let table = self.table_ref(&query.collection);

        // If no joins specified, delegate to simple query
        let joins = match &query.joins {
            Some(j) if !j.is_empty() => j,
            _ => return self.query(query).await,
        };

        // Build JOIN SQL
        let mut select_parts = vec![format!("{}.*", table)];
        let mut join_clauses = Vec::new();

        for join in joins {
            let join_table = self.table_ref(&join.collection);
            let join_type_sql = match join.join_type {
                super::query::JoinType::Left => "LEFT JOIN",
                super::query::JoinType::Inner => "INNER JOIN",
            };

            let local_col = naming::to_snake_case(&join.local_field);
            let foreign_col = naming::to_snake_case(&join.foreign_field);

            // Select specific fields or all
            if let Some(fields) = &join.select {
                for field in fields {
                    let pg_col = naming::to_snake_case(field);
                    select_parts.push(format!(
                        "{}.{} AS {}__{}",
                        join_table, pg_col, join.alias, pg_col
                    ));
                }
            } else {
                select_parts.push(format!("{}.*", join_table));
            }

            join_clauses.push(format!(
                "{} {} ON {}.{} = {}.{}",
                join_type_sql, join_table, table, local_col, join_table, foreign_col
            ));
        }

        let (where_clause, where_params) = build_where_clause(&query.filter, 0);
        let order_clause = build_order_clause(&query.sort);

        let mut sql = format!(
            "SELECT {} FROM {} {}",
            select_parts.join(", "),
            table,
            join_clauses.join(" ")
        );

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

        let params_ref: Vec<&(dyn ToSql + Sync)> = where_params.iter().map(|b| &**b as &(dyn ToSql + Sync)).collect();

        let rows = match client.query(&sql, &params_ref).await {
            Ok(r) => r,
            Err(e) => return StorageResult::err(format!("Join query failed: {}", e)),
        };

        let mut records = Vec::with_capacity(rows.len());
        for row in &rows {
            match row_to_record(row, &query.collection, row.columns()) {
                Ok(record) => records.push(record),
                Err(e) => return StorageResult::err(format!("Row conversion failed: {}", e)),
            }
        }

        StorageResult::ok(records)
    }

    async fn count(&self, query: StorageQuery) -> StorageResult<usize> {
        let pool = match self.pool() {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => return StorageResult::err(format!("Pool error: {}", e)),
        };

        let table = self.table_ref(&query.collection);
        let (where_clause, where_params) = build_where_clause(&query.filter, 0);

        let mut sql = format!("SELECT COUNT(*) FROM {}", table);
        if !where_clause.is_empty() {
            sql.push(' ');
            sql.push_str(&where_clause);
        }

        let params_ref: Vec<&(dyn ToSql + Sync)> = where_params.iter().map(|b| &**b as &(dyn ToSql + Sync)).collect();

        match client.query_one(&sql, &params_ref).await {
            Ok(row) => {
                let count: i64 = row.get(0);
                StorageResult::ok(count as usize)
            }
            Err(e) => {
                let msg = e.to_string();
                if msg.contains("does not exist") {
                    return StorageResult::ok(0);
                }
                StorageResult::err(format!("Count failed: {}", e))
            }
        }
    }

    async fn update(
        &self,
        collection: &str,
        id: &UUID,
        data: Value,
        increment_version: bool,
    ) -> StorageResult<DataRecord> {
        let pool = match self.pool() {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => return StorageResult::err(format!("Pool error: {}", e)),
        };

        let bare_table = naming::to_table_name(collection);
        let table = self.table_ref(collection);
        let now: DateTime<Utc> = Utc::now();

        // Get column types for type-aware parameter coercion
        let col_types = get_column_types(&client, &bare_table, &self.schema).await;

        let mut sets = vec!["updated_at = $1".to_string()];
        let mut params: Vec<Box<dyn ToSql + Sync + Send>> = vec![Box::new(now)];
        let mut idx = 1_usize;

        if increment_version {
            sets.push("version = version + 1".to_string());
        }

        if let Value::Object(obj) = &data {
            for (key, value) in obj {
                if key == "id" || key == "createdAt" || key == "created_at" {
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

        let sql = format!("UPDATE {} SET {} WHERE id = ${}", table, sets.join(", "), idx);
        let params_ref: Vec<&(dyn ToSql + Sync)> = params.iter().map(|b| &**b as &(dyn ToSql + Sync)).collect();

        match client.execute(&sql, &params_ref).await {
            Ok(rows) if rows > 0 => self.read(collection, id).await,
            Ok(_) => StorageResult::err(format!("Record not found: {}", id)),
            Err(e) => StorageResult::err(format!("Update failed: {}", e)),
        }
    }

    async fn delete(&self, collection: &str, id: &UUID) -> StorageResult<bool> {
        let pool = match self.pool() {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => return StorageResult::err(format!("Pool error: {}", e)),
        };

        let table = self.table_ref(collection);
        let sql = format!("DELETE FROM {} WHERE id = $1", table);

        match client.execute(&sql, &[&id]).await {
            Ok(rows) => StorageResult::ok(rows > 0),
            Err(e) => StorageResult::err(format!("Delete failed: {}", e)),
        }
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
        let pool = match self.pool() {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => return StorageResult::err(format!("Pool error: {}", e)),
        };

        let table = self.table_ref(&schema.collection);

        let mut columns = vec![
            "id TEXT PRIMARY KEY".to_string(),
            "created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()".to_string(),
            "updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()".to_string(),
            "version BIGINT NOT NULL DEFAULT 1".to_string(),
        ];

        for field in &schema.fields {
            let col_name = naming::to_snake_case(&field.name);
            let col_type = pg_type_from_field_type(&field.field_type);

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

        if let Err(e) = client.execute(&sql, &[]).await {
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
                if let Err(e) = client.execute(&idx_sql, &[]).await {
                    return StorageResult::err(format!("Create index failed: {}", e));
                }
            }
        }

        // Create composite indexes
        for index in &schema.indexes {
            let cols: Vec<String> = index.fields.iter().map(|f| naming::to_snake_case(f)).collect();
            let unique = if index.unique { "UNIQUE " } else { "" };
            let idx_sql = format!(
                "CREATE {}INDEX IF NOT EXISTS {} ON {} ({})",
                unique,
                naming::to_snake_case(&index.name),
                table,
                cols.join(", ")
            );
            if let Err(e) = client.execute(&idx_sql, &[]).await {
                return StorageResult::err(format!("Create composite index failed: {}", e));
            }
        }

        StorageResult::ok(true)
    }

    async fn list_collections(&self) -> StorageResult<Vec<String>> {
        let pool = match self.pool() {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => return StorageResult::err(format!("Pool error: {}", e)),
        };

        let sql = format!(
            "SELECT table_name FROM information_schema.tables \
             WHERE table_schema = '{}' AND table_type = 'BASE TABLE'",
            self.schema
        );

        let rows = match client.query(&sql, &[]).await {
            Ok(r) => r,
            Err(e) => return StorageResult::err(format!("Query failed: {}", e)),
        };

        let tables: Vec<String> = rows.iter().map(|r| r.get::<_, String>(0)).collect();
        StorageResult::ok(tables)
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
        let pool = match self.pool() {
            Ok(p) => p,
            Err(e) => return StorageResult::err(e),
        };
        let client = match pool.get().await {
            Ok(c) => c,
            Err(e) => return StorageResult::err(format!("Pool error: {}", e)),
        };

        let table = self.table_ref(collection);
        let sql = format!("TRUNCATE TABLE {} CASCADE", table);

        match client.execute(&sql, &[]).await {
            Ok(_) => StorageResult::ok(true),
            Err(e) => StorageResult::err(format!("Truncate failed: {}", e)),
        }
    }

    async fn clear_all(&self) -> StorageResult<ClearAllResult> {
        let tables_result = self.list_collections().await;
        let tables = match tables_result.data {
            Some(t) => t,
            None => return StorageResult::err(tables_result.error.unwrap_or_default()),
        };

        let mut cleared = Vec::new();
        for table in &tables {
            if self.truncate(table).await.success {
                cleared.push(table.clone());
            }
        }

        StorageResult::ok(ClearAllResult {
            tables_cleared: cleared,
            records_deleted: 0,
        })
    }

    async fn cleanup(&self) -> Result<(), String> {
        let pool = self.pool()?;
        let client = pool
            .get()
            .await
            .map_err(|e| format!("Pool error: {}", e))?;

        // VACUUM FULL requires exclusive lock, use regular VACUUM + ANALYZE
        client
            .execute("VACUUM", &[])
            .await
            .map_err(|e| format!("Vacuum failed: {}", e))?;

        client
            .execute("ANALYZE", &[])
            .await
            .map_err(|e| format!("Analyze failed: {}", e))?;

        Ok(())
    }
}

/// Auto-create table from JSON data shape (matches SQLite behavior).
/// `qualified_table` is the schema-qualified name for DDL/DML (e.g. "test_orm.users").
/// `bare_table` is the unqualified name for information_schema queries.
/// `schema` is the schema name for information_schema filtering.
async fn ensure_table_exists_pg(
    client: &deadpool_postgres::Client,
    qualified_table: &str,
    bare_table: &str,
    schema: &str,
    data: &Value,
) -> Result<(), String> {
    // Check if table already exists
    let check_sql = format!(
        "SELECT EXISTS (SELECT 1 FROM information_schema.tables \
         WHERE table_schema = '{}' AND table_name = '{}')",
        schema, bare_table
    );
    let exists: bool = client
        .query_one(&check_sql, &[])
        .await
        .map_err(|e| format!("Table check failed: {}", e))?
        .get(0);

    if exists {
        // Table exists — check for new columns we need to add
        if let Value::Object(obj) = data {
            let cols_sql = format!(
                "SELECT column_name FROM information_schema.columns \
                 WHERE table_schema = '{}' AND table_name = '{}'",
                schema, bare_table
            );
            let rows = client
                .query(&cols_sql, &[])
                .await
                .map_err(|e| format!("Column check failed: {}", e))?;
            let existing_cols: Vec<String> = rows.iter().map(|r| r.get::<_, String>(0)).collect();

            for (key, value) in obj {
                if key == "id" || key == "createdAt" || key == "created_at"
                    || key == "updatedAt" || key == "updated_at"
                    || key == "version"
                {
                    continue;
                }
                let col_name = naming::to_snake_case(key);
                if !existing_cols.contains(&col_name) {
                    let col_type = pg_type_from_value(value, &col_name);
                    let alter_sql = format!(
                        "ALTER TABLE {} ADD COLUMN {} {}",
                        qualified_table, col_name, col_type
                    );
                    client
                        .execute(&alter_sql, &[])
                        .await
                        .map_err(|e| format!("Add column failed: {}", e))?;
                }
            }
        }
        return Ok(());
    }

    // Create table
    let mut columns = vec![
        "id TEXT PRIMARY KEY".to_string(),
        "created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()".to_string(),
        "updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()".to_string(),
        "version BIGINT NOT NULL DEFAULT 1".to_string(),
    ];

    if let Value::Object(obj) = data {
        for (key, value) in obj {
            if key == "id" || key == "createdAt" || key == "created_at"
                || key == "updatedAt" || key == "updated_at"
                || key == "version"
            {
                continue;
            }
            let col_name = naming::to_snake_case(key);
            let col_type = pg_type_from_value(value, &col_name);
            columns.push(format!("{} {}", col_name, col_type));
        }
    }

    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {} ({})",
        qualified_table,
        columns.join(", ")
    );

    client
        .execute(&sql, &[])
        .await
        .map_err(|e| format!("Create table failed: {}", e))?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// These tests require a live PostgreSQL instance.
    /// Run with: cargo test --lib -- --ignored postgres
    /// Or: DATABASE_URL=postgres://user:pass@localhost/testdb cargo test --lib -- --ignored

    fn get_test_url() -> String {
        std::env::var("DATABASE_URL")
            .unwrap_or_else(|_| "postgres://continuum:continuum@localhost:5432/continuum_test".to_string())
    }

    async fn setup_pg_adapter() -> PostgresAdapter {
        let mut adapter = PostgresAdapter::new();
        adapter
            .initialize(AdapterConfig {
                connection_string: get_test_url(),
                namespace: Some("test_orm".to_string()),
                timeout_ms: 30_000,
                max_connections: 5,
            })
            .await
            .expect("PostgreSQL connection failed - is Postgres running?");

        // Clean test schema
        let pool = adapter.pool().unwrap();
        let client = pool.get().await.unwrap();
        client
            .execute("SET search_path TO test_orm", &[])
            .await
            .unwrap();

        adapter
    }

    #[tokio::test]
    #[ignore] // Requires live Postgres
    async fn test_pg_create_and_read() {
        let adapter = setup_pg_adapter().await;

        // Clean up from previous runs
        let _ = adapter.truncate("users").await;

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
            id: "pg-test-123".to_string(),
            collection: "users".to_string(),
            data: json!({"name": "Joel"}),
            metadata: RecordMetadata::default(),
        };

        let create_result = adapter.create(record).await;
        assert!(create_result.success, "Create failed: {:?}", create_result.error);

        let read_result = adapter.read("users", &"pg-test-123".to_string()).await;
        assert!(read_result.success, "Read failed: {:?}", read_result.error);
        let data = read_result.data.unwrap();
        assert_eq!(data.data["name"], "Joel");
    }

    #[tokio::test]
    #[ignore] // Requires live Postgres
    async fn test_pg_query_with_filters() {
        let adapter = setup_pg_adapter().await;

        adapter
            .ensure_schema(CollectionSchema {
                collection: "query_test".to_string(),
                fields: vec![
                    super::super::types::SchemaField {
                        name: "name".to_string(),
                        field_type: super::super::types::FieldType::String,
                        indexed: true,
                        unique: false,
                        nullable: false,
                        max_length: None,
                    },
                    super::super::types::SchemaField {
                        name: "age".to_string(),
                        field_type: super::super::types::FieldType::Number,
                        indexed: false,
                        unique: false,
                        nullable: true,
                        max_length: None,
                    },
                ],
                indexes: vec![],
            })
            .await;

        let _ = adapter.truncate("query_test").await;

        // Create test records
        for (i, name) in ["Alice", "Bob", "Charlie"].iter().enumerate() {
            let record = DataRecord {
                id: format!("qt-{}", i),
                collection: "query_test".to_string(),
                data: json!({"name": name, "age": 20 + i as i64 * 5}),
                metadata: RecordMetadata::default(),
            };
            let r = adapter.create(record).await;
            assert!(r.success, "Create failed: {:?}", r.error);
        }

        // Query all
        let all = adapter
            .query(StorageQuery {
                collection: "query_test".to_string(),
                ..Default::default()
            })
            .await;
        assert!(all.success);
        assert_eq!(all.data.as_ref().unwrap().len(), 3);

        // Query with filter
        let mut filter = HashMap::new();
        filter.insert("name".to_string(), FieldFilter::Value(json!("Alice")));
        let filtered = adapter
            .query(StorageQuery {
                collection: "query_test".to_string(),
                filter: Some(filter),
                ..Default::default()
            })
            .await;
        assert!(filtered.success);
        assert_eq!(filtered.data.as_ref().unwrap().len(), 1);

        // Count
        let count = adapter
            .count(StorageQuery {
                collection: "query_test".to_string(),
                ..Default::default()
            })
            .await;
        assert!(count.success);
        assert_eq!(count.data.unwrap(), 3);
    }

    #[tokio::test]
    #[ignore] // Requires live Postgres
    async fn test_pg_update_and_delete() {
        let adapter = setup_pg_adapter().await;

        adapter
            .ensure_schema(CollectionSchema {
                collection: "update_test".to_string(),
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

        let _ = adapter.truncate("update_test").await;

        // Create
        let record = DataRecord {
            id: "upd-1".to_string(),
            collection: "update_test".to_string(),
            data: json!({"name": "Before"}),
            metadata: RecordMetadata::default(),
        };
        adapter.create(record).await;

        // Update
        let updated = adapter
            .update("update_test", &"upd-1".to_string(), json!({"name": "After"}), true)
            .await;
        assert!(updated.success);
        let data = updated.data.unwrap();
        assert_eq!(data.data["name"], "After");

        // Delete
        let deleted = adapter.delete("update_test", &"upd-1".to_string()).await;
        assert!(deleted.success);
        assert!(deleted.data.unwrap());

        // Verify gone
        let read = adapter.read("update_test", &"upd-1".to_string()).await;
        assert!(!read.success);
    }

    #[tokio::test]
    #[ignore] // Requires live Postgres
    async fn test_pg_list_collections_and_stats() {
        let adapter = setup_pg_adapter().await;

        // Ensure at least one table exists
        adapter
            .ensure_schema(CollectionSchema {
                collection: "stats_test".to_string(),
                fields: vec![super::super::types::SchemaField {
                    name: "data".to_string(),
                    field_type: super::super::types::FieldType::Json,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                }],
                indexes: vec![],
            })
            .await;

        let collections = adapter.list_collections().await;
        assert!(collections.success);
        assert!(collections.data.unwrap().contains(&"stats_test".to_string()));

        let stats = adapter.collection_stats("stats_test").await;
        assert!(stats.success);
    }

    #[tokio::test]
    #[ignore] // Requires live Postgres
    async fn test_pg_batch_operations() {
        let adapter = setup_pg_adapter().await;

        adapter
            .ensure_schema(CollectionSchema {
                collection: "batch_test".to_string(),
                fields: vec![super::super::types::SchemaField {
                    name: "value".to_string(),
                    field_type: super::super::types::FieldType::String,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                }],
                indexes: vec![],
            })
            .await;

        let _ = adapter.truncate("batch_test").await;

        let ops = vec![
            BatchOperation {
                operation_type: BatchOperationType::Create,
                collection: "batch_test".to_string(),
                id: Some("batch-1".to_string()),
                data: Some(json!({"value": "one"})),
            },
            BatchOperation {
                operation_type: BatchOperationType::Create,
                collection: "batch_test".to_string(),
                id: Some("batch-2".to_string()),
                data: Some(json!({"value": "two"})),
            },
            BatchOperation {
                operation_type: BatchOperationType::Read,
                collection: "batch_test".to_string(),
                id: Some("batch-1".to_string()),
                data: None,
            },
        ];

        let result = adapter.batch(ops).await;
        assert!(result.success);
        let results = result.data.unwrap();
        assert_eq!(results.len(), 3);
        assert!(results[0]["success"].as_bool().unwrap());
        assert!(results[1]["success"].as_bool().unwrap());
        assert!(results[2]["success"].as_bool().unwrap());
    }

    #[tokio::test]
    #[ignore] // Requires live Postgres
    async fn test_pg_jsonb_columns() {
        let adapter = setup_pg_adapter().await;

        adapter
            .ensure_schema(CollectionSchema {
                collection: "jsonb_test".to_string(),
                fields: vec![
                    super::super::types::SchemaField {
                        name: "config".to_string(),
                        field_type: super::super::types::FieldType::Json,
                        indexed: false,
                        unique: false,
                        nullable: true,
                        max_length: None,
                    },
                    super::super::types::SchemaField {
                        name: "tags".to_string(),
                        field_type: super::super::types::FieldType::Json,
                        indexed: false,
                        unique: false,
                        nullable: true,
                        max_length: None,
                    },
                ],
                indexes: vec![],
            })
            .await;

        let _ = adapter.truncate("jsonb_test").await;

        let record = DataRecord {
            id: "jsonb-1".to_string(),
            collection: "jsonb_test".to_string(),
            data: json!({
                "config": {"theme": "dark", "fontSize": 14},
                "tags": ["rust", "postgres", "orm"]
            }),
            metadata: RecordMetadata::default(),
        };

        let create = adapter.create(record).await;
        assert!(create.success, "JSONB create failed: {:?}", create.error);

        let read = adapter.read("jsonb_test", &"jsonb-1".to_string()).await;
        assert!(read.success, "JSONB read failed: {:?}", read.error);

        let data = read.data.unwrap();
        // JSONB should preserve the structure
        assert_eq!(data.data["config"]["theme"], "dark");
        assert_eq!(data.data["tags"][0], "rust");
    }

    #[tokio::test]
    #[ignore] // Requires live Postgres
    async fn test_pg_clear_all() {
        let adapter = setup_pg_adapter().await;

        // Create a table with data
        adapter
            .ensure_schema(CollectionSchema {
                collection: "clear_test".to_string(),
                fields: vec![super::super::types::SchemaField {
                    name: "x".to_string(),
                    field_type: super::super::types::FieldType::String,
                    indexed: false,
                    unique: false,
                    nullable: true,
                    max_length: None,
                }],
                indexes: vec![],
            })
            .await;

        let record = DataRecord {
            id: "clear-1".to_string(),
            collection: "clear_test".to_string(),
            data: json!({"x": "y"}),
            metadata: RecordMetadata::default(),
        };
        adapter.create(record).await;

        // Clear all
        let result = adapter.clear_all().await;
        assert!(result.success);

        // Verify empty
        let count = adapter
            .count(StorageQuery {
                collection: "clear_test".to_string(),
                ..Default::default()
            })
            .await;
        assert_eq!(count.data.unwrap(), 0);
    }
}
