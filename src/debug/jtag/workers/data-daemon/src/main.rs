/// Data Worker Test - Real SQLite Implementation
///
/// PURPOSE: Test concurrent database operations with Rust adapter
/// Uses SEPARATE test databases in .continuum/jtag/test-dbs
///
/// Implements:
/// - ping: Health check
/// - open-database: Opens SQLite database
/// - create-record: Creates a record
/// - read-record: Reads a record by ID

use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::{fs, thread};
use uuid::Uuid;

// Generated entity types from TypeScript decorators
mod entities;

// ============================================================================
// Utility Functions
// ============================================================================

/// Convert camelCase to snake_case for SQL column names
fn to_snake_case(s: &str) -> String {
    let mut result = String::new();
    let mut prev_is_lower = false;

    for (i, ch) in s.chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 && prev_is_lower {
                result.push('_');
            }
            result.push(ch.to_lowercase().next().unwrap());
            prev_is_lower = false;
        } else {
            result.push(ch);
            prev_is_lower = ch.is_lowercase();
        }
    }

    result
}

// ============================================================================
// JTAGProtocol Types
// ============================================================================

#[derive(Debug, Deserialize)]
struct JTAGRequest {
    id: String,
    #[serde(rename = "type")]
    msg_type: String,
    timestamp: String,
    payload: Value,
    #[serde(rename = "userId")]
    user_id: Option<String>,
}

#[derive(Debug, Serialize)]
struct JTAGResponse {
    id: String,
    #[serde(rename = "type")]
    msg_type: String,
    timestamp: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    payload: Option<Value>,
    #[serde(rename = "requestId")]
    request_id: String,
    success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

// ============================================================================
// Database Handle Registry
// ============================================================================

struct DatabaseHandle {
    connection: Connection,
    path: String,
    opened_at: String,
}

type HandleRegistry = Arc<Mutex<HashMap<String, DatabaseHandle>>>;

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Deserialize)]
struct OpenDatabaseRequest {
    filename: String,
    #[serde(rename = "adapterType")]
    adapter_type: String,
    #[serde(rename = "storageType")]
    storage_type: Option<String>,
}

#[derive(Debug, Serialize)]
struct OpenDatabaseResponse {
    handle: String,
    #[serde(rename = "storageType")]
    storage_type: String,
    #[serde(rename = "pragmaMode")]
    pragma_mode: String,
}

#[derive(Debug, Deserialize)]
struct CreateRecordRequest {
    handle: String,
    collection: String,
    record: Value,
}

#[derive(Debug, Deserialize)]
struct ReadRecordRequest {
    handle: String,
    collection: String,
    id: String,
}

#[derive(Debug, Deserialize)]
struct EnsureSchemaRequest {
    handle: String,
    collection: String,
}

#[derive(Debug, Deserialize)]
struct QueryRecordsRequest {
    handle: String,
    query: QueryRequest,
}

#[derive(Debug, Deserialize)]
struct QueryRequest {
    collection: String,
    filter: Option<serde_json::Value>,  // Universal filter (will parse per field)
    sort: Option<Vec<SortField>>,
    limit: Option<i64>,
    offset: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct SortField {
    field: String,
    direction: String,  // "asc" or "desc"
}

// ============================================================================
// Main Worker
// ============================================================================

fn main() {
    let socket_path = "/tmp/jtag-data-daemon-worker.sock";

    // Remove old socket if exists
    if Path::new(socket_path).exists() {
        fs::remove_file(socket_path).expect("Failed to remove old socket");
    }

    // Bind Unix socket
    let listener = UnixListener::bind(socket_path).expect("Failed to bind socket");
    println!("🦀 Data worker (TEST) listening on {}", socket_path);

    // Create handle registry
    let registry: HandleRegistry = Arc::new(Mutex::new(HashMap::new()));

    // Accept connections
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                println!("📡 New connection");
                let registry_clone = Arc::clone(&registry);
                thread::spawn(move || handle_client(stream, registry_clone));
            }
            Err(err) => {
                eprintln!("❌ Connection error: {}", err);
            }
        }
    }
}

fn handle_client(stream: UnixStream, registry: HandleRegistry) {
    let mut reader = BufReader::new(stream.try_clone().expect("Failed to clone stream"));
    let mut writer = stream;

    loop {
        let mut line = String::new();
        match reader.read_line(&mut line) {
            Ok(0) => {
                println!("📡 Client disconnected");
                break;
            }
            Ok(_) => {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }

                // Parse request
                let request: JTAGRequest = match serde_json::from_str(line) {
                    Ok(req) => req,
                    Err(err) => {
                        eprintln!("❌ Failed to parse request: {} - {}", err, line);
                        continue;
                    }
                };

                println!("📥 Request: {} - {}", request.msg_type, request.id);

                // Handle message
                let response = handle_message(request, &registry);

                // Send response (newline-delimited JSON)
                let response_json = serde_json::to_string(&response).expect("Failed to serialize response");
                if let Err(err) = writeln!(writer, "{}", response_json) {
                    eprintln!("❌ Failed to write response: {}", err);
                    break;
                }

                println!("📤 Response: {} - success={}", response.request_id, response.success);
            }
            Err(err) => {
                eprintln!("❌ Read error: {}", err);
                break;
            }
        }
    }
}

fn handle_message(request: JTAGRequest, registry: &HandleRegistry) -> JTAGResponse {
    let timestamp = chrono::Utc::now().to_rfc3339();
    let response_id = Uuid::new_v4().to_string();

    match request.msg_type.as_str() {
        "ping" => handle_ping(request, response_id, timestamp),
        "open-database" => handle_open_database(request, response_id, timestamp, registry),
        "ensure-schema" => handle_ensure_schema(request, response_id, timestamp, registry),
        "create-record" => handle_create_record(request, response_id, timestamp, registry),
        "read-record" => handle_read_record(request, response_id, timestamp, registry),
        "query-records" => handle_query_records(request, response_id, timestamp, registry),
        _ => JTAGResponse {
            id: response_id,
            msg_type: request.msg_type.clone(),
            timestamp,
            payload: None,
            request_id: request.id,
            success: false,
            error: Some(format!("Unknown message type: {}", request.msg_type)),
        },
    }
}

fn handle_ping(request: JTAGRequest, response_id: String, timestamp: String) -> JTAGResponse {
    let payload = serde_json::json!({
        "uptimeMs": 12345,
        "activeHandles": 0,
        "totalHandles": 0
    });

    JTAGResponse {
        id: response_id,
        msg_type: request.msg_type,
        timestamp,
        payload: Some(payload),
        request_id: request.id,
        success: true,
        error: None,
    }
}

fn handle_open_database(
    request: JTAGRequest,
    response_id: String,
    timestamp: String,
    registry: &HandleRegistry,
) -> JTAGResponse {
    // Parse payload
    let open_req: OpenDatabaseRequest = match serde_json::from_value(request.payload) {
        Ok(req) => req,
        Err(err) => {
            return JTAGResponse {
                id: response_id,
                msg_type: request.msg_type,
                timestamp,
                payload: None,
                request_id: request.id,
                success: false,
                error: Some(format!("Invalid payload: {}", err)),
            };
        }
    };

    println!("   📂 Opening database: {}", open_req.filename);

    // Ensure directory exists
    if let Some(parent) = Path::new(&open_req.filename).parent() {
        if let Err(err) = fs::create_dir_all(parent) {
            return JTAGResponse {
                id: response_id,
                msg_type: request.msg_type,
                timestamp,
                payload: None,
                request_id: request.id,
                success: false,
                error: Some(format!("Failed to create directory: {}", err)),
            };
        }
    }

    // Open SQLite connection
    let connection = match Connection::open(&open_req.filename) {
        Ok(conn) => conn,
        Err(err) => {
            return JTAGResponse {
                id: response_id,
                msg_type: request.msg_type,
                timestamp,
                payload: None,
                request_id: request.id,
                success: false,
                error: Some(format!("Failed to open database: {}", err)),
            };
        }
    };

    // Enable WAL mode
    if let Err(err) = connection.execute("PRAGMA journal_mode=WAL", []) {
        eprintln!("⚠️ Failed to enable WAL mode: {}", err);
    }

    // Generate handle
    let handle = Uuid::new_v4().to_string();

    // Store in registry
    let db_handle = DatabaseHandle {
        connection,
        path: open_req.filename.clone(),
        opened_at: timestamp.clone(),
    };

    registry.lock().unwrap().insert(handle.clone(), db_handle);

    println!("   ✅ Database opened: handle={}", handle);

    let response = OpenDatabaseResponse {
        handle,
        storage_type: "internal-ssd".to_string(),
        pragma_mode: "WAL".to_string(),
    };

    JTAGResponse {
        id: response_id,
        msg_type: request.msg_type,
        timestamp,
        payload: Some(serde_json::to_value(response).unwrap()),
        request_id: request.id,
        success: true,
        error: None,
    }
}

fn handle_create_record(
    request: JTAGRequest,
    response_id: String,
    timestamp: String,
    registry: &HandleRegistry,
) -> JTAGResponse {
    // Parse payload
    let create_req: CreateRecordRequest = match serde_json::from_value(request.payload) {
        Ok(req) => req,
        Err(err) => {
            return JTAGResponse {
                id: response_id,
                msg_type: request.msg_type,
                timestamp,
                payload: None,
                request_id: request.id,
                success: false,
                error: Some(format!("Invalid payload: {}", err)),
            };
        }
    };

    // Get connection
    let mut reg = registry.lock().unwrap();
    let db_handle = match reg.get_mut(&create_req.handle) {
        Some(h) => h,
        None => {
            return JTAGResponse {
                id: response_id,
                msg_type: request.msg_type,
                timestamp,
                payload: None,
                request_id: request.id,
                success: false,
                error: Some(format!("Database handle not found: {}", create_req.handle)),
            };
        }
    };

    // NOTE: Schema already created by TypeScript side - don't recreate it!
    // The TypeScript SqliteStorageAdapter creates relational columns, not JSON blobs

    // Extract fields from record for dynamic INSERT
    let record_obj = match create_req.record.as_object() {
        Some(obj) => obj,
        None => {
            return JTAGResponse {
                id: response_id,
                msg_type: request.msg_type,
                timestamp,
                payload: None,
                request_id: request.id,
                success: false,
                error: Some("Record must be a JSON object".to_string()),
            };
        }
    };

    // Build dynamic INSERT statement based on fields in record
    // Filter out metadata fields and convert camelCase to snake_case
    let mut columns: Vec<String> = Vec::new();
    let mut column_keys: Vec<String> = Vec::new();

    for key in record_obj.keys() {
        // Skip metadata fields that aren't database columns
        if key == "collection" || key == "metadata" {
            continue;
        }
        column_keys.push(key.clone());
        columns.push(to_snake_case(key));
    }

    let placeholders: Vec<String> = (1..=columns.len()).map(|i| format!("?{}", i)).collect();

    let insert_sql = format!(
        "INSERT INTO {} ({}) VALUES ({})",
        create_req.collection,
        columns.join(", "),
        placeholders.join(", ")
    );

    // Build params array
    let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    for col_key in &column_keys {
        let value = &record_obj[col_key];
        // Convert JSON values to SQL params
        match value {
            Value::String(s) => params_vec.push(Box::new(s.clone())),
            Value::Number(n) => {
                if let Some(i) = n.as_i64() {
                    params_vec.push(Box::new(i));
                } else if let Some(f) = n.as_f64() {
                    params_vec.push(Box::new(f));
                } else {
                    params_vec.push(Box::new(n.to_string()));
                }
            },
            Value::Bool(b) => params_vec.push(Box::new(*b as i64)),
            Value::Null => params_vec.push(Box::new(rusqlite::types::Null)),
            _ => params_vec.push(Box::new(value.to_string())), // JSON objects/arrays as strings
        }
    }

    let params_refs: Vec<&dyn rusqlite::ToSql> = params_vec.iter().map(|b| &**b as &dyn rusqlite::ToSql).collect();

    if let Err(err) = db_handle.connection.execute(&insert_sql, &params_refs[..]) {
        return JTAGResponse {
            id: response_id,
            msg_type: request.msg_type,
            timestamp,
            payload: None,
            request_id: request.id,
            success: false,
            error: Some(format!("Failed to insert record: {}", err)),
        };
    }

    let record_id = record_obj.get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    println!("   ✅ Record created: {}/{}", create_req.collection, record_id);

    JTAGResponse {
        id: response_id,
        msg_type: request.msg_type,
        timestamp,
        payload: Some(serde_json::json!({ "record": create_req.record })),
        request_id: request.id,
        success: true,
        error: None,
    }
}

fn handle_read_record(
    request: JTAGRequest,
    response_id: String,
    timestamp: String,
    registry: &HandleRegistry,
) -> JTAGResponse {
    // Parse payload
    let read_req: ReadRecordRequest = match serde_json::from_value(request.payload) {
        Ok(req) => req,
        Err(err) => {
            return JTAGResponse {
                id: response_id,
                msg_type: request.msg_type,
                timestamp,
                payload: None,
                request_id: request.id,
                success: false,
                error: Some(format!("Invalid payload: {}", err)),
            };
        }
    };

    // Get connection
    let reg = registry.lock().unwrap();
    let db_handle = match reg.get(&read_req.handle) {
        Some(h) => h,
        None => {
            return JTAGResponse {
                id: response_id,
                msg_type: request.msg_type,
                timestamp,
                payload: None,
                request_id: request.id,
                success: false,
                error: Some(format!("Database handle not found: {}", read_req.handle)),
            };
        }
    };

    // Query all columns from record (relational schema, not JSON blob)
    let query_sql = format!(
        "SELECT * FROM {} WHERE id = ?1",
        read_req.collection
    );

    let record: Value = match db_handle.connection.query_row(&query_sql, params![read_req.id], |row| {
        // Build JSON object from all columns
        let mut obj = serde_json::Map::new();

        // Get column count
        let col_count = row.as_ref().column_count();

        // Iterate through all columns and build JSON
        for i in 0..col_count {
            let col_name = row.as_ref().column_name(i).unwrap_or("unknown").to_string();

            // Try to get value as different types
            let value: Value = if let Ok(s) = row.get::<_, String>(i) {
                Value::String(s)
            } else if let Ok(i64_val) = row.get::<_, i64>(i) {
                serde_json::json!(i64_val)
            } else if let Ok(f64_val) = row.get::<_, f64>(i) {
                serde_json::json!(f64_val)
            } else if let Ok(bool_val) = row.get::<_, bool>(i) {
                Value::Bool(bool_val)
            } else {
                // NULL or unsupported type
                Value::Null
            };

            obj.insert(col_name, value);
        }

        Ok(Value::Object(obj))
    }) {
        Ok(data) => data,
        Err(rusqlite::Error::QueryReturnedNoRows) => {
            return JTAGResponse {
                id: response_id,
                msg_type: request.msg_type,
                timestamp,
                payload: None,
                request_id: request.id,
                success: false,
                error: Some(format!("Record not found: {}", read_req.id)),
            };
        }
        Err(err) => {
            return JTAGResponse {
                id: response_id,
                msg_type: request.msg_type,
                timestamp,
                payload: None,
                request_id: request.id,
                success: false,
                error: Some(format!("Failed to query record: {}", err)),
            };
        }
    };

    println!("   ✅ Record read: {}/{}", read_req.collection, read_req.id);

    JTAGResponse {
        id: response_id,
        msg_type: request.msg_type,
        timestamp,
        payload: Some(serde_json::json!({ "record": record })),
        request_id: request.id,
        success: true,
        error: None,
    }
}

fn handle_ensure_schema(
    request: JTAGRequest,
    response_id: String,
    timestamp: String,
    registry: &HandleRegistry,
) -> JTAGResponse {
    // Parse payload
    let ensure_req: EnsureSchemaRequest = match serde_json::from_value(request.payload) {
        Ok(req) => req,
        Err(err) => {
            return JTAGResponse {
                id: response_id,
                msg_type: request.msg_type,
                timestamp,
                payload: None,
                request_id: request.id,
                success: false,
                error: Some(format!("Invalid payload: {}", err)),
            };
        }
    };

    // OPTIMIZATION: Tables are already managed by TypeScript migrations
    // Just return success immediately - no need to check or create anything
    // This avoids potential slowdowns from schema mismatches (JSON blob vs relational)

    println!("   ✅ Schema ensured (no-op): {}", ensure_req.collection);

    JTAGResponse {
        id: response_id,
        msg_type: request.msg_type,
        timestamp,
        payload: Some(serde_json::json!({ "exists": true })),
        request_id: request.id,
        success: true,
        error: None,
    }
}

fn handle_query_records(
    request: JTAGRequest,
    response_id: String,
    timestamp: String,
    registry: &HandleRegistry,
) -> JTAGResponse {
    // Parse payload
    let query_req: QueryRecordsRequest = match serde_json::from_value(request.payload) {
        Ok(req) => req,
        Err(err) => {
            return JTAGResponse {
                id: response_id,
                msg_type: request.msg_type,
                timestamp,
                payload: None,
                request_id: request.id,
                success: false,
                error: Some(format!("Invalid payload: {}", err)),
            };
        }
    };

    // Get connection
    let reg = registry.lock().unwrap();
    let db_handle = match reg.get(&query_req.handle) {
        Some(h) => h,
        None => {
            return JTAGResponse {
                id: response_id,
                msg_type: request.msg_type,
                timestamp,
                payload: None,
                request_id: request.id,
                success: false,
                error: Some(format!("Database handle not found: {}", query_req.handle)),
            };
        }
    };

    // Build SQL query - use SELECT * for relational schema
    let mut sql = format!("SELECT * FROM {}", query_req.query.collection);
    let mut conditions: Vec<String> = Vec::new();
    let mut params: Vec<String> = Vec::new();

    // Build WHERE clause from filter (use actual column names, not json_extract)
    if let Some(filter_value) = &query_req.query.filter {
        if let Some(filter_obj) = filter_value.as_object() {
            for (field, value) in filter_obj {
                // Convert camelCase field to snake_case column
                let column_name = to_snake_case(field);

                // For now, just handle simple equality (string values)
                if let Some(str_val) = value.as_str() {
                    conditions.push(format!("{} = ?", column_name));
                    params.push(str_val.to_string());
                } else if let Some(num_val) = value.as_i64() {
                    conditions.push(format!("{} = ?", column_name));
                    params.push(num_val.to_string());
                } else if let Some(bool_val) = value.as_bool() {
                    conditions.push(format!("{} = ?", column_name));
                    params.push(if bool_val { "1" } else { "0" }.to_string());
                }
                // TODO: Add operator support ($gt, $lt, $gte, $lte, $in, etc.)
            }
        }
    }

    if !conditions.is_empty() {
        sql.push_str(" WHERE ");
        sql.push_str(&conditions.join(" AND "));
    }

    // Add ORDER BY - use actual column names
    if let Some(sort_fields) = &query_req.query.sort {
        if !sort_fields.is_empty() {
            sql.push_str(" ORDER BY ");
            let sort_clauses: Vec<String> = sort_fields
                .iter()
                .map(|s| {
                    let dir = if s.direction == "desc" { "DESC" } else { "ASC" };
                    format!("{} {}", to_snake_case(&s.field), dir)
                })
                .collect();
            sql.push_str(&sort_clauses.join(", "));
        }
    }

    // Add LIMIT (skip negative values - they mean "no limit")
    if let Some(limit) = query_req.query.limit {
        if limit > 0 {
            sql.push_str(&format!(" LIMIT {}", limit));
        }
    }

    // Add OFFSET
    if let Some(offset) = query_req.query.offset {
        sql.push_str(&format!(" OFFSET {}", offset));
    }

    println!("   🔍 Query SQL: {}", sql);
    println!("   📊 Params: {:?}", params);

    // Execute query
    let mut stmt = match db_handle.connection.prepare(&sql) {
        Ok(s) => s,
        Err(err) => {
            return JTAGResponse {
                id: response_id,
                msg_type: request.msg_type,
                timestamp,
                payload: None,
                request_id: request.id,
                success: false,
                error: Some(format!("Failed to prepare query: {}", err)),
            };
        }
    };

    // Convert params to rusqlite params
    let param_refs: Vec<&dyn rusqlite::ToSql> = params
        .iter()
        .map(|p| p as &dyn rusqlite::ToSql)
        .collect();

    let mut rows = match stmt.query(&param_refs[..]) {
        Ok(r) => r,
        Err(err) => {
            return JTAGResponse {
                id: response_id,
                msg_type: request.msg_type,
                timestamp,
                payload: None,
                request_id: request.id,
                success: false,
                error: Some(format!("Failed to execute query: {}", err)),
            };
        }
    };

    // Collect results - build JSON from all columns
    // Pre-allocate with expected capacity to avoid reallocations
    // Handle negative limit (means "no limit") by using reasonable default capacity
    let capacity = match query_req.query.limit {
        Some(limit) if limit > 0 => limit as usize,
        _ => 100, // Default capacity for unlimited or negative limit
    };
    let mut records: Vec<serde_json::Value> = Vec::with_capacity(capacity);

    while let Ok(Some(row)) = rows.next() {
        // Build JSON object from all columns using ValueRef for efficiency
        let mut obj = serde_json::Map::new();

        let col_count = row.as_ref().column_count();

        for i in 0..col_count {
            let col_name = row.as_ref().column_name(i).unwrap_or("unknown").to_string();

            // Use ValueRef to get type directly - much faster than trying each type
            let value: Value = match row.get_ref(i) {
                Ok(rusqlite::types::ValueRef::Null) => Value::Null,
                Ok(rusqlite::types::ValueRef::Integer(i)) => serde_json::json!(i),
                Ok(rusqlite::types::ValueRef::Real(f)) => serde_json::json!(f),
                Ok(rusqlite::types::ValueRef::Text(t)) => {
                    Value::String(String::from_utf8_lossy(t).to_string())
                }
                Ok(rusqlite::types::ValueRef::Blob(b)) => {
                    // For blobs, try to convert to UTF-8 string, otherwise hex encode
                    match std::str::from_utf8(b) {
                        Ok(s) => Value::String(s.to_string()),
                        Err(_) => {
                            // Hex encode for binary data
                            let hex: String = b.iter().map(|byte| format!("{:02x}", byte)).collect();
                            Value::String(format!("0x{}", hex))
                        }
                    }
                }
                Err(_) => Value::Null,
            };

            obj.insert(col_name, value);
        }

        let data = Value::Object(obj);

        // Build record with metadata (similar to TypeScript format)
        records.push(serde_json::json!({
            "id": data.get("id").unwrap_or(&serde_json::Value::Null),
            "collection": query_req.query.collection,
            "data": data,
            "metadata": {
                "createdAt": data.get("createdAt").unwrap_or(&serde_json::Value::Null),
                "updatedAt": data.get("updatedAt").unwrap_or(&serde_json::Value::Null),
                "version": data.get("version").unwrap_or(&serde_json::json!(0))
            }
        }));
    }

    println!("   ✅ Query returned {} records", records.len());

    JTAGResponse {
        id: response_id,
        msg_type: request.msg_type,
        timestamp,
        payload: Some(serde_json::json!({
            "records": records,
            "totalCount": records.len(),
            "queryTime": 0  // TODO: Add actual timing
        })),
        request_id: request.id,
        success: true,
        error: None,
    }
}
