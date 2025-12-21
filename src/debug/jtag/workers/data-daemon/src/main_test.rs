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

// ============================================================================
// Main Worker
// ============================================================================

fn main() {
    let socket_path = "/tmp/jtag-data-worker.sock";

    // Remove old socket if exists
    if Path::new(socket_path).exists() {
        fs::remove_file(socket_path).expect("Failed to remove old socket");
    }

    // Bind Unix socket
    let listener = UnixListener::bind(socket_path).expect("Failed to bind socket");
    println!("🦀 Data worker (TEST) listening on {socket_path}");

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
                eprintln!("❌ Connection error: {err}");
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
                        eprintln!("❌ Failed to parse request: {err} - {line}");
                        continue;
                    }
                };

                println!("📥 Request: {} - {}", request.msg_type, request.id);

                // Handle message
                let response = handle_message(request, &registry);

                // Send response (newline-delimited JSON)
                let response_json = serde_json::to_string(&response).expect("Failed to serialize response");
                if let Err(err) = writeln!(writer, "{response_json}") {
                    eprintln!("❌ Failed to write response: {err}");
                    break;
                }

                println!("📤 Response: {} - success={}", response.request_id, response.success);
            }
            Err(err) => {
                eprintln!("❌ Read error: {err}");
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
        "create-record" => handle_create_record(request, response_id, timestamp, registry),
        "read-record" => handle_read_record(request, response_id, timestamp, registry),
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
                error: Some(format!("Invalid payload: {err}")),
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
                error: Some(format!("Failed to create directory: {err}")),
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
                error: Some(format!("Failed to open database: {err}")),
            };
        }
    };

    // Enable WAL mode
    if let Err(err) = connection.execute("PRAGMA journal_mode=WAL", []) {
        eprintln!("⚠️ Failed to enable WAL mode: {err}");
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

    println!("   ✅ Database opened: handle={handle}");

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
                error: Some(format!("Invalid payload: {err}")),
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

    // Create table if not exists
    let create_table_sql = format!(
        "CREATE TABLE IF NOT EXISTS {} (id TEXT PRIMARY KEY, data TEXT)",
        create_req.collection
    );

    if let Err(err) = db_handle.connection.execute(&create_table_sql, []) {
        return JTAGResponse {
            id: response_id,
            msg_type: request.msg_type,
            timestamp,
            payload: None,
            request_id: request.id,
            success: false,
            error: Some(format!("Failed to create table: {err}")),
        };
    }

    // Extract ID from data
    let record_id = match create_req.record.get("id") {
        Some(Value::String(id)) => id.clone(),
        _ => {
            return JTAGResponse {
                id: response_id,
                msg_type: request.msg_type,
                timestamp,
                payload: None,
                request_id: request.id,
                success: false,
                error: Some("Missing or invalid 'id' field in data".to_string()),
            };
        }
    };

    // Serialize data
    let data_json = serde_json::to_string(&create_req.record).unwrap();

    // Insert record
    let insert_sql = format!(
        "INSERT INTO {} (id, data) VALUES (?1, ?2)",
        create_req.collection
    );

    if let Err(err) = db_handle
        .connection
        .execute(&insert_sql, params![record_id, data_json])
    {
        return JTAGResponse {
            id: response_id,
            msg_type: request.msg_type,
            timestamp,
            payload: None,
            request_id: request.id,
            success: false,
            error: Some(format!("Failed to insert record: {err}")),
        };
    }

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
                error: Some(format!("Invalid payload: {err}")),
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

    // Query record
    let query_sql = format!(
        "SELECT data FROM {} WHERE id = ?1",
        read_req.collection
    );

    let data_json: String = match db_handle
        .connection
        .query_row(&query_sql, params![read_req.id], |row| row.get(0))
    {
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
                error: Some(format!("Failed to query record: {err}")),
            };
        }
    };

    // Parse data
    let data: Value = match serde_json::from_str(&data_json) {
        Ok(d) => d,
        Err(err) => {
            return JTAGResponse {
                id: response_id,
                msg_type: request.msg_type,
                timestamp,
                payload: None,
                request_id: request.id,
                success: false,
                error: Some(format!("Failed to parse record data: {err}")),
            };
        }
    };

    println!("   ✅ Record read: {}/{}", read_req.collection, read_req.id);

    JTAGResponse {
        id: response_id,
        msg_type: request.msg_type,
        timestamp,
        payload: Some(serde_json::json!({ "record": data })),
        request_id: request.id,
        success: true,
        error: None,
    }
}
