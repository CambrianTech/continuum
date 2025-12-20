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

// Storage adapters (trait-based architecture)
mod storage;
use storage::{StorageAdapter, SqliteAdapter};

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
    adapter: Box<dyn StorageAdapter>,
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

#[derive(Debug, Deserialize, Serialize)]
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

    // Create SqliteAdapter and initialize it
    let mut adapter = SqliteAdapter::new();
    let config = serde_json::json!({
        "filename": open_req.filename
    });

    // Initialize adapter (async but we're in sync context - use tokio block)
    let init_result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(adapter.initialize(config));

    if let Err(err) = init_result {
        return JTAGResponse {
            id: response_id,
            msg_type: request.msg_type,
            timestamp,
            payload: None,
            request_id: request.id,
            success: false,
            error: Some(format!("Failed to initialize adapter: {}", err)),
        };
    }

    // Generate handle
    let handle = Uuid::new_v4().to_string();

    // Store in registry
    let db_handle = DatabaseHandle {
        adapter: Box::new(adapter),
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

    // Get adapter from registry
    let reg = registry.lock().unwrap();
    let db_handle = match reg.get(&create_req.handle) {
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

    println!("   📝 Creating record via adapter: {}", create_req.collection);

    // Call adapter.create() (async but we're in sync context - use tokio block)
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(db_handle.adapter.create(&create_req.collection, create_req.record.clone()));

    let record = match result {
        Ok(r) => r,
        Err(err) => {
            return JTAGResponse {
                id: response_id,
                msg_type: request.msg_type,
                timestamp,
                payload: None,
                request_id: request.id,
                success: false,
                error: Some(format!("Failed to create record: {}", err)),
            };
        }
    };

    let record_id = record.get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    println!("   ✅ Record created: {}/{}", create_req.collection, record_id);

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

    // Get adapter from registry
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

    println!("   📖 Reading record via adapter: {}/{}", read_req.collection, read_req.id);

    // Call adapter.read() (async but we're in sync context - use tokio block)
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(db_handle.adapter.read(&read_req.collection, &read_req.id));

    let record = match result {
        Ok(Some(r)) => r,
        Ok(None) => {
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
                error: Some(format!("Failed to read record: {}", err)),
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

    // Get adapter from registry
    let reg = registry.lock().unwrap();
    let db_handle = match reg.get(&ensure_req.handle) {
        Some(h) => h,
        None => {
            return JTAGResponse {
                id: response_id,
                msg_type: request.msg_type,
                timestamp,
                payload: None,
                request_id: request.id,
                success: false,
                error: Some(format!("Database handle not found: {}", ensure_req.handle)),
            };
        }
    };

    println!("   🔧 Ensuring schema via adapter: {}", ensure_req.collection);

    // Call adapter.ensure_schema() (async but we're in sync context - use tokio block)
    let result = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(db_handle.adapter.ensure_schema(&ensure_req.collection, None));

    match result {
        Ok(_) => {
            println!("   ✅ Schema ensured: {}", ensure_req.collection);
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
        Err(err) => {
            JTAGResponse {
                id: response_id,
                msg_type: request.msg_type,
                timestamp,
                payload: None,
                request_id: request.id,
                success: false,
                error: Some(format!("Failed to ensure schema: {}", err)),
            }
        }
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

    // Build query JSON for adapter
    let query_json = serde_json::json!({
        "collection": query_req.query.collection,
        "filter": query_req.query.filter,
        "sort": query_req.query.sort,
        "limit": query_req.query.limit,
        "offset": query_req.query.offset
    });

    println!("   🔍 Query via adapter: {:?}", query_json);

    // Get adapter from registry
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

    // Call adapter.query() (async but we're in sync context - use tokio block)
    let records = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(db_handle.adapter.query(query_json));

    let records = match records {
        Ok(r) => r,
        Err(err) => {
            return JTAGResponse {
                id: response_id,
                msg_type: request.msg_type,
                timestamp,
                payload: None,
                request_id: request.id,
                success: false,
                error: Some(format!("Query failed: {}", err)),
            };
        }
    };

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
