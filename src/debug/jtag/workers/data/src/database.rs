/// Database Module - Pure SQL Executor
///
/// This module provides a simple SQL execution layer:
/// - Connection pooling (r2d2 with 10 connections)
/// - SQL query execution (SELECT - returns rows)
/// - SQL statement execution (INSERT/UPDATE/DELETE - returns changes)
/// - Parameter binding (JSON values to SQL types)
/// - Error handling with retries
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::ToSql;
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

use crate::messages::{SqlExecutePayload, SqlExecuteResult, SqlQueryPayload, SqlQueryResult};

// ============================================================================
// Connection Pool
// ============================================================================

pub type DbPool = Pool<SqliteConnectionManager>;

/// Create a new database connection pool
pub fn create_pool<P: AsRef<Path>>(db_path: P) -> Result<DbPool, r2d2::Error> {
    let manager = SqliteConnectionManager::file(db_path).with_init(|conn| {
        // Use DELETE mode (matches TypeScript implementation)
        // Do NOT convert to WAL - keep database in original journal_mode
        conn.execute_batch(
            "PRAGMA busy_timeout=30000;
                 PRAGMA synchronous=NORMAL;",
        )
    });

    Pool::builder()
        .max_size(10) // 10 concurrent connections
        .connection_timeout(Duration::from_secs(30))
        .build(manager)
}

// ============================================================================
// SQL Parameter Conversion
// ============================================================================

/// Convert serde_json::Value to rusqlite-compatible parameter
fn json_value_to_sql(value: &Value) -> Box<dyn ToSql> {
    match value {
        Value::Null => Box::new(None::<String>),
        Value::Bool(b) => Box::new(*b),
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
        Value::Array(_) | Value::Object(_) => {
            // Serialize complex types as JSON strings
            Box::new(serde_json::to_string(value).unwrap_or_default())
        }
    }
}

// ============================================================================
// SQL Execution Operations
// ============================================================================

/// Execute SQL query (SELECT) - returns rows as JSON
pub fn execute_query(pool: &DbPool, payload: SqlQueryPayload) -> Result<SqlQueryResult, String> {
    let conn = pool
        .get()
        .map_err(|e| format!("Failed to get connection from pool: {e}"))?;

    // Prepare statement
    let mut stmt = conn
        .prepare(&payload.sql)
        .map_err(|e| format!("Prepare query failed: {e}"))?;

    // Convert JSON params to SQLite params
    let sql_params: Vec<Box<dyn ToSql>> = payload
        .params
        .iter()
        .map(|v| json_value_to_sql(v))
        .collect();

    // Create slice of references for rusqlite
    let params_refs: Vec<&dyn ToSql> = sql_params
        .iter()
        .map(|b| b.as_ref() as &dyn ToSql)
        .collect();

    // Get column names
    let column_count = stmt.column_count();
    let column_names: Vec<String> = (0..column_count)
        .map(|i| stmt.column_name(i).unwrap_or("").to_string())
        .collect();

    // Execute query and collect rows
    let rows = stmt
        .query_map(params_refs.as_slice(), |row| {
            let mut obj = serde_json::Map::new();

            for (i, col_name) in column_names.iter().enumerate() {
                // Try to get value as different types
                let value = if let Ok(s) = row.get::<usize, String>(i) {
                    Value::String(s)
                } else if let Ok(i_val) = row.get::<usize, i64>(i) {
                    Value::Number(i_val.into())
                } else if let Ok(f) = row.get::<usize, f64>(i) {
                    serde_json::Number::from_f64(f)
                        .map(Value::Number)
                        .unwrap_or(Value::Null)
                } else if let Ok(b) = row.get::<usize, bool>(i) {
                    Value::Bool(b)
                } else {
                    Value::Null
                };

                obj.insert(col_name.clone(), value);
            }

            Ok(Value::Object(obj))
        })
        .map_err(|e| format!("Query execution failed: {e}"))?;

    // Collect all rows
    let mut result_rows = Vec::new();
    for row_result in rows {
        match row_result {
            Ok(row_obj) => result_rows.push(row_obj),
            Err(e) => eprintln!("⚠️  Row fetch error: {e}"),
        }
    }

    Ok(SqlQueryResult { rows: result_rows })
}

/// Execute SQL statement (INSERT/UPDATE/DELETE) - returns changes count
pub fn execute_statement(
    pool: &DbPool,
    payload: SqlExecutePayload,
) -> Result<SqlExecuteResult, String> {
    let conn = pool
        .get()
        .map_err(|e| format!("Failed to get connection from pool: {e}"))?;

    // Convert JSON params to SQLite params
    let sql_params: Vec<Box<dyn ToSql>> = payload
        .params
        .iter()
        .map(|v| json_value_to_sql(v))
        .collect();

    // Create slice of references for rusqlite
    let params_refs: Vec<&dyn ToSql> = sql_params
        .iter()
        .map(|b| b.as_ref() as &dyn ToSql)
        .collect();

    // Execute statement
    let changes = conn
        .execute(&payload.sql, params_refs.as_slice())
        .map_err(|e| format!("Execute failed: {e}"))?;

    // Get last insert ID if applicable
    let last_insert_id = if payload.sql.trim().to_uppercase().starts_with("INSERT") {
        Some(conn.last_insert_rowid())
    } else {
        None
    };

    Ok(SqlExecuteResult {
        changes,
        last_insert_id,
    })
}

// ============================================================================
// Error Handling
// ============================================================================

/// Execute with retry on SQLITE_BUSY
pub fn with_retry<F, T>(mut f: F) -> Result<T, String>
where
    F: FnMut() -> Result<T, String>,
{
    let max_retries = 3;
    let mut attempts = 0;

    loop {
        match f() {
            Ok(result) => return Ok(result),
            Err(e) => {
                if e.contains("database is locked") && attempts < max_retries {
                    attempts += 1;
                    let backoff = Duration::from_millis(100 * (1 << attempts));
                    std::thread::sleep(backoff);
                    continue;
                }
                return Err(e);
            }
        }
    }
}
