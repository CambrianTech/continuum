/// Database Module - SQLite Connection Pool and Query Operations
///
/// This module handles all SQLite database operations:
/// - Connection pooling (r2d2 with 10 connections)
/// - Query building (dynamic filters and ordering)
/// - CRUD operations (list, read, create, update)
/// - Error handling with retries

use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::ToSql;
use serde_json::Value;
use std::path::Path;
use std::time::Duration;

use crate::messages::{
    DataListPayload, DataListResult, DataReadPayload, DataReadResult, DataCreatePayload,
    DataCreateResult, DataUpdatePayload, DataUpdateResult, OrderDirection, DataRecord,
    DataRecordMetadata,
};

// ============================================================================
// Connection Pool
// ============================================================================

pub type DbPool = Pool<SqliteConnectionManager>;

/// Create a new database connection pool
pub fn create_pool<P: AsRef<Path>>(db_path: P) -> Result<DbPool, r2d2::Error> {
    let manager = SqliteConnectionManager::file(db_path)
        .with_init(|conn| {
            // Enable WAL mode for better concurrency
            conn.execute_batch(
                "PRAGMA journal_mode=WAL;
                 PRAGMA busy_timeout=5000;
                 PRAGMA synchronous=NORMAL;",
            )
        });

    Pool::builder()
        .max_size(10) // 10 concurrent connections
        .connection_timeout(Duration::from_secs(30))
        .build(manager)
}

// ============================================================================
// Query Builder Helpers
// ============================================================================

/// Build WHERE clause from JSON filter using json_extract
fn build_where_clause(filter: &Option<Value>) -> (String, Vec<String>) {
    let mut where_parts = Vec::new();
    let mut values = Vec::new();

    if let Some(Value::Object(obj)) = filter {
        for (key, value) in obj {
            // Use json_extract to query within JSON data column
            where_parts.push(format!("json_extract(data, '$.{}') = ?", key));
            // Convert value to string for SQL parameter
            let value_str = match value {
                Value::String(s) => s.clone(),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => b.to_string(),
                _ => serde_json::to_string(value).unwrap_or_default(),
            };
            values.push(value_str);
        }
    }

    let where_clause = if where_parts.is_empty() {
        String::new()
    } else {
        format!(" WHERE {}", where_parts.join(" AND "))
    };

    (where_clause, values)
}

/// Build ORDER BY clause
fn build_order_clause(order_by: &Option<Vec<crate::messages::OrderBy>>) -> String {
    if let Some(orders) = order_by {
        if orders.is_empty() {
            return String::new();
        }

        let order_parts: Vec<String> = orders
            .iter()
            .map(|o| {
                let direction = match o.direction {
                    OrderDirection::Asc => "ASC",
                    OrderDirection::Desc => "DESC",
                };
                format!("{} {}", o.field, direction)
            })
            .collect();

        format!(" ORDER BY {}", order_parts.join(", "))
    } else {
        String::new()
    }
}

// ============================================================================
// CRUD Operations
// ============================================================================

/// Execute data/list command
pub fn execute_list(pool: &DbPool, payload: DataListPayload) -> Result<DataListResult, String> {
    let conn = pool
        .get()
        .map_err(|e| format!("Failed to get connection from pool: {}", e))?;

    let limit = payload.limit.unwrap_or(50);
    let offset = payload.offset.unwrap_or(0);

    // Build query
    let (where_clause, filter_values) = build_where_clause(&payload.filter);
    let order_clause = build_order_clause(&payload.order_by);

    // Count query
    let count_query = format!("SELECT COUNT(*) FROM {}{}", payload.collection, where_clause);

    // Data query
    let data_query = format!(
        "SELECT data FROM {}{}{} LIMIT {} OFFSET {}",
        payload.collection, where_clause, order_clause, limit, offset
    );

    // Convert Vec<String> to Vec<&dyn ToSql> for SQLite parameters
    let params: Vec<&dyn rusqlite::ToSql> = filter_values
        .iter()
        .map(|s| s as &dyn rusqlite::ToSql)
        .collect();

    // Execute count
    let total: usize = conn
        .query_row(&count_query, params.as_slice(), |row| {
            row.get(0)
        })
        .map_err(|e| format!("Count query failed: {}", e))?;

    // Execute data query
    let mut stmt = conn
        .prepare(&data_query)
        .map_err(|e| format!("Prepare query failed: {}", e))?;

    let rows = stmt
        .query_map(params.as_slice(), |row| {
            let json_str: String = row.get(0)?;
            Ok(json_str)
        })
        .map_err(|e| format!("Query execution failed: {}", e))?;

    let mut items = Vec::new();
    for row_result in rows {
        match row_result {
            Ok(json_str) => {
                match serde_json::from_str::<Value>(&json_str) {
                    Ok(value) => items.push(value),
                    Err(e) => eprintln!("⚠️  Failed to parse JSON: {}", e),
                }
            }
            Err(e) => eprintln!("⚠️  Row fetch error: {}", e),
        }
    }

    // Return raw entity data (unwrap from any internal structure)
    Ok(DataListResult {
        items,
        total,
        limit,
        offset,
    })
}

/// Execute data/read command
pub fn execute_read(pool: &DbPool, payload: DataReadPayload) -> Result<DataReadResult, String> {
    let conn = pool
        .get()
        .map_err(|e| format!("Failed to get connection from pool: {}", e))?;

    let query = format!("SELECT data FROM {} WHERE id = ?", payload.collection);

    let result = conn.query_row(&query, [&payload.id], |row| {
        let json_str: String = row.get(0)?;
        Ok(json_str)
    });

    match result {
        Ok(json_str) => {
            let entity = serde_json::from_str::<Value>(&json_str)
                .map_err(|e| format!("Failed to parse JSON: {}", e))?;
            Ok(DataReadResult {
                data: Some(entity),
            })
        }
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(DataReadResult { data: None }),
        Err(e) => Err(format!("Query failed: {}", e)),
    }
}

/// Execute data/create command
pub fn execute_create(pool: &DbPool, payload: DataCreatePayload) -> Result<DataCreateResult, String> {
    let conn = pool
        .get()
        .map_err(|e| format!("Failed to get connection from pool: {}", e))?;

    // Extract ID from document
    let id = payload
        .document
        .get("id")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Document must have an 'id' field".to_string())?
        .to_string();

    // Serialize document to JSON string
    let json_str = serde_json::to_string(&payload.document)
        .map_err(|e| format!("Failed to serialize document: {}", e))?;

    // Insert query
    let query = format!(
        "INSERT INTO {} (id, data) VALUES (?, ?)",
        payload.collection
    );

    conn.execute(&query, [&id, &json_str])
        .map_err(|e| format!("Insert failed: {}", e))?;

    // Return the created entity (same as input document)
    Ok(DataCreateResult {
        data: payload.document.clone(),
    })
}

/// Execute data/update command
pub fn execute_update(pool: &DbPool, payload: DataUpdatePayload) -> Result<DataUpdateResult, String> {
    let conn = pool
        .get()
        .map_err(|e| format!("Failed to get connection from pool: {}", e))?;

    // Read existing document
    let query = format!("SELECT data FROM {} WHERE id = ?", payload.collection);

    let existing_json = conn
        .query_row(&query, [&payload.id], |row| {
            let json_str: String = row.get(0)?;
            Ok(json_str)
        })
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => {
                format!("Document not found with id: {}", payload.id)
            }
            _ => format!("Query failed: {}", e),
        })?;

    // Parse existing document
    let mut existing_doc = serde_json::from_str::<Value>(&existing_json)
        .map_err(|e| format!("Failed to parse existing document: {}", e))?;

    // Merge updates
    if let Value::Object(ref mut existing_obj) = existing_doc {
        if let Value::Object(updates_obj) = &payload.updates {
            for (key, value) in updates_obj {
                existing_obj.insert(key.clone(), value.clone());
            }

            // Serialize updated document
            let updated_json = serde_json::to_string(&existing_doc)
                .map_err(|e| format!("Failed to serialize updated document: {}", e))?;

            // Update query
            let update_query = format!("UPDATE {} SET data = ? WHERE id = ?", payload.collection);

            conn.execute(&update_query, [&updated_json, &payload.id])
                .map_err(|e| format!("Update failed: {}", e))?;

            // Return the updated entity
            return Ok(DataUpdateResult {
                data: existing_doc,
            });
        }
    }

    Err("Document or updates are not valid JSON objects".to_string())
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
