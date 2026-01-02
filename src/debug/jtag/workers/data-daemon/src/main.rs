/// RustDataDaemon - Adapter-Aware Concurrent Data Layer
///
/// ARCHITECTURE:
/// - Single coordinator for all database adapters
/// - Adapter-specific concurrency strategies (SQLite queue, Postgres pool)
/// - Handle-based API (like textureId from graphics APIs)
/// - Prevents lock contention through proper coordination
///
/// FLOW:
/// 1. TypeScript DataDaemon → Unix socket → RustDataDaemon
/// 2. RustDataDaemon routes to correct adapter with correct strategy
/// 3. SQLite: Single writer queue (serialized writes, parallel reads)
/// 4. Postgres: Connection pool (full concurrency)
/// 5. Return results via Unix socket

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::{fs, thread};
use ts_rs::TS;
use uuid::Uuid;

// ============================================================================
// Core Types (ts-rs exported for TypeScript)
// ============================================================================

/// Opaque handle to a database adapter (like textureId)
/// Serialized as UUID string in JSON
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AdapterHandle(Uuid);

impl AdapterHandle {
    fn new() -> Self {
        Self(Uuid::new_v4())
    }
}

/// Adapter type (determines concurrency strategy)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/types/")]
#[serde(rename_all = "lowercase")]
pub enum AdapterType {
    Sqlite,
    Postgres,
    Json,
}

/// Adapter configuration
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/types/")]
pub struct AdapterConfig {
    adapter_type: AdapterType,
    connection_string: String,
    #[ts(skip)]
    options: Option<HashMap<String, Value>>,
}

// ============================================================================
// Request/Response Types
// ============================================================================

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "command")]
enum Request {
    #[serde(rename = "ping")]
    Ping,

    #[serde(rename = "adapter/open")]
    AdapterOpen { config: AdapterConfig },

    #[serde(rename = "adapter/close")]
    AdapterClose { handle: AdapterHandle },

    #[serde(rename = "data/list")]
    DataList {
        handle: AdapterHandle,
        collection: String,
        limit: Option<usize>,
        offset: Option<usize>,
        filter: Option<Value>,
        order_by: Option<Vec<OrderBy>>,
    },

    #[serde(rename = "data/create")]
    DataCreate {
        handle: AdapterHandle,
        collection: String,
        data: Value,
    },

    #[serde(rename = "data/delete")]
    DataDelete {
        handle: AdapterHandle,
        collection: String,
        id: String,
    },

    #[serde(rename = "data/update")]
    DataUpdate {
        handle: AdapterHandle,
        collection: String,
        id: String,
        data: Value,
    },
}

#[derive(Debug, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/types/")]
pub struct OrderBy {
    field: String,
    direction: String, // "asc" | "desc"
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "status")]
enum Response {
    #[serde(rename = "ok")]
    Ok { data: Value },

    #[serde(rename = "error")]
    Error { message: String },

    #[serde(rename = "pong")]
    Pong { uptime_seconds: u64 },
}

// ============================================================================
// Concurrency Strategy Trait
// ============================================================================

trait ConcurrencyStrategy: Send + Sync {
    /// Execute read operation (can be parallel)
    fn execute_read(&self, query: &str) -> Result<Value, String>;

    /// Execute write operation (adapter-specific queueing)
    fn execute_write(&self, query: &str, params: &Value) -> Result<Value, String>;

    /// Close adapter and cleanup resources
    fn close(&self) -> Result<(), String>;
}

// ============================================================================
// Storage Detection
// ============================================================================

#[derive(Debug, Clone, Copy)]
enum StorageType {
    InternalSSD,
    ExternalSSD,
    SDCard,
    HDD,
    Unknown,
}

/// Detect storage type by sampling system characteristics
fn detect_storage_type(path: &Path) -> StorageType {
    // Get absolute path
    let abs_path = match fs::canonicalize(path) {
        Ok(p) => p,
        Err(_) => return StorageType::Unknown,
    };

    let path_str = abs_path.to_string_lossy();

    // Check if on external volume (macOS specific)
    if path_str.starts_with("/Volumes/") {
        // Use diskutil to check if removable
        let volume_name = path_str
            .strip_prefix("/Volumes/")
            .and_then(|s| s.split('/').next())
            .unwrap_or("");

        if let Ok(output) = Command::new("diskutil")
            .args(&["info", &format!("/Volumes/{}", volume_name)])
            .output()
        {
            let info = String::from_utf8_lossy(&output.stdout);

            // Check for removable media
            if info.contains("Removable Media:") && info.contains("Removable") {
                return StorageType::SDCard;
            }

            // Check for SSD
            if info.contains("Solid State:") && info.contains("Yes") {
                return StorageType::ExternalSSD;
            }

            // Assume HDD if not SSD
            return StorageType::HDD;
        }

        // Default to SD card for /Volumes if detection fails (conservative)
        return StorageType::SDCard;
    }

    // Internal drive
    StorageType::InternalSSD
}

/// Get optimized SQLite pragmas based on storage type and workload
fn get_sqlite_pragmas(storage: StorageType, multi_writer: bool) -> String {
    match storage {
        StorageType::InternalSSD => {
            // Fast internal SSD - can use WAL safely
            if multi_writer {
                "PRAGMA journal_mode=WAL; \
                 PRAGMA synchronous=NORMAL; \
                 PRAGMA temp_store=MEMORY; \
                 PRAGMA busy_timeout=5000;".to_string()
            } else {
                "PRAGMA journal_mode=WAL; \
                 PRAGMA synchronous=NORMAL; \
                 PRAGMA temp_store=MEMORY; \
                 PRAGMA locking_mode=EXCLUSIVE; \
                 PRAGMA busy_timeout=5000;".to_string()
            }
        }
        StorageType::ExternalSSD => {
            // External SSD - WAL OK but more conservative
            "PRAGMA journal_mode=WAL; \
             PRAGMA synchronous=NORMAL; \
             PRAGMA wal_autocheckpoint=1000; \
             PRAGMA temp_store=MEMORY; \
             PRAGMA busy_timeout=5000;".to_string()
        }
        StorageType::SDCard | StorageType::HDD | StorageType::Unknown => {
            // SD card / HDD / Unknown - NO WAL (reliability over concurrency)
            "PRAGMA journal_mode=DELETE; \
             PRAGMA synchronous=NORMAL; \
             PRAGMA temp_store=MEMORY; \
             PRAGMA locking_mode=EXCLUSIVE; \
             PRAGMA busy_timeout=5000;".to_string()
        }
    }
}

// ============================================================================
// SQLite Strategy: Single Writer Queue + Storage-Aware Configuration
// ============================================================================

struct SqliteStrategy {
    connection_path: String,
    storage_type: StorageType,
    writer_queue: Arc<Mutex<VecDeque<WriteOperation>>>,
    connection: Arc<Mutex<rusqlite::Connection>>,
}

struct WriteOperation {
    query: String,
    params: Value,
}

impl SqliteStrategy {
    fn new(connection_path: String) -> Result<Self, String> {
        // Detect storage type by sampling system
        let storage_type = detect_storage_type(Path::new(&connection_path));

        println!("🔍 Detected storage type: {:?} for {}", storage_type, connection_path);

        // Check for WAL artifacts before opening
        let wal_path = format!("{}-wal", connection_path);
        let shm_path = format!("{}-shm", connection_path);
        let has_wal_artifacts = Path::new(&wal_path).exists() || Path::new(&shm_path).exists();

        // Open connection
        let conn = rusqlite::Connection::open(&connection_path)
            .map_err(|e| format!("Failed to open SQLite: {}", e))?;

        // If switching FROM WAL to DELETE mode, checkpoint first
        if has_wal_artifacts && matches!(storage_type, StorageType::SDCard | StorageType::HDD | StorageType::Unknown) {
            println!("⚠️  Found WAL artifacts, checkpointing before mode switch...");

            // Checkpoint and truncate WAL (force cleanup)
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                .map_err(|e| format!("Failed to checkpoint WAL: {}", e))?;
        }

        // Configure based on detected storage (assume single-writer for now)
        let pragmas = get_sqlite_pragmas(storage_type, false);
        conn.execute_batch(&pragmas)
            .map_err(|e| format!("Failed to configure SQLite: {}", e))?;

        // Verify WAL files are gone if we switched to DELETE mode
        if has_wal_artifacts && matches!(storage_type, StorageType::SDCard | StorageType::HDD | StorageType::Unknown) {
            if Path::new(&wal_path).exists() || Path::new(&shm_path).exists() {
                println!("⚠️  Warning: WAL artifacts still present after mode switch");
            } else {
                println!("✅ WAL artifacts cleaned up successfully");
            }
        }

        let mode_desc = match storage_type {
            StorageType::InternalSSD => "WAL mode - internal SSD optimized",
            StorageType::ExternalSSD => "WAL mode - external SSD optimized",
            _ => "DELETE mode - SD card/HDD reliable",
        };

        println!("✅ SQLite adapter opened: {} ({})", connection_path, mode_desc);

        Ok(Self {
            connection_path,
            storage_type,
            writer_queue: Arc::new(Mutex::new(VecDeque::new())),
            connection: Arc::new(Mutex::new(conn)),
        })
    }

    /// Process write queue serially (prevents lock contention)
    fn process_write_queue(&self, op: WriteOperation) -> Result<Value, String> {
        let mut queue = self.writer_queue.lock().unwrap();
        queue.push_back(op);

        // Process all queued writes serially
        let mut results = Vec::new();
        while let Some(write_op) = queue.pop_front() {
            let conn = self.connection.lock().unwrap();

            // Execute write (simplified - would need proper query building)
            let rows_affected = conn.execute(&write_op.query, [])
                .map_err(|e| format!("SQLite write failed: {}", e))?;

            results.push(json!({ "rows_affected": rows_affected }));
        }

        Ok(json!({ "results": results }))
    }
}

impl ConcurrencyStrategy for SqliteStrategy {
    fn execute_read(&self, query: &str) -> Result<Value, String> {
        // Reads can run in parallel (WAL mode allows this)
        let conn = self.connection.lock().unwrap();

        let mut stmt = conn.prepare(query)
            .map_err(|e| format!("Failed to prepare query: {}", e))?;

        let column_count = stmt.column_count();

        // Get column names before query_map (to avoid borrowing issues)
        let column_names: Vec<String> = (0..column_count)
            .map(|i| stmt.column_name(i).unwrap_or("unknown").to_string())
            .collect();

        let mut rows = Vec::new();

        let row_iter = stmt.query_map([], |row| {
            let mut row_data = serde_json::Map::new();
            for (i, column_name) in column_names.iter().enumerate() {
                let value: Result<String, _> = row.get(i);
                if let Ok(v) = value {
                    row_data.insert(column_name.clone(), json!(v));
                }
            }
            Ok(Value::Object(row_data))
        }).map_err(|e| format!("Query execution failed: {}", e))?;

        for row in row_iter {
            rows.push(row.map_err(|e| format!("Row parse error: {}", e))?);
        }

        Ok(json!({ "items": rows, "count": rows.len() }))
    }

    fn execute_write(&self, query: &str, params: &Value) -> Result<Value, String> {
        // Queue write for serial processing
        self.process_write_queue(WriteOperation {
            query: query.to_string(),
            params: params.clone(),
        })
    }

    fn close(&self) -> Result<(), String> {
        // Process any remaining writes before closing
        let queue_size = self.writer_queue.lock().unwrap().len();
        if queue_size > 0 {
            println!("⚠️  Closing SQLite adapter with {} pending writes", queue_size);
        }

        // Checkpoint WAL if using WAL mode (ensure data persistence)
        if matches!(self.storage_type, StorageType::InternalSSD | StorageType::ExternalSSD) {
            let conn = self.connection.lock().unwrap();
            println!("📝 Checkpointing WAL before close...");

            // TRUNCATE mode: checkpoint and delete WAL files
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                .map_err(|e| format!("Failed to checkpoint WAL on close: {}", e))?;

            println!("✅ WAL checkpointed successfully");
        }

        println!("✅ SQLite adapter closed: {}", self.connection_path);
        Ok(())
    }
}

// ============================================================================
// Postgres Strategy: Connection Pool (Full Concurrency)
// ============================================================================

struct PostgresStrategy {
    // TODO: Implement connection pool with deadpool-postgres
    // For now, placeholder
}

impl ConcurrencyStrategy for PostgresStrategy {
    fn execute_read(&self, _query: &str) -> Result<Value, String> {
        Err("Postgres strategy not yet implemented".to_string())
    }

    fn execute_write(&self, _query: &str, _params: &Value) -> Result<Value, String> {
        Err("Postgres strategy not yet implemented".to_string())
    }

    fn close(&self) -> Result<(), String> {
        Ok(())
    }
}

// ============================================================================
// JSON Strategy: File-Level Locking
// ============================================================================

struct JsonStrategy {
    base_path: PathBuf,
    file_locks: Arc<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>>,
}

impl JsonStrategy {
    fn new(base_path: String) -> Result<Self, String> {
        Ok(Self {
            base_path: PathBuf::from(base_path),
            file_locks: Arc::new(Mutex::new(HashMap::new())),
        })
    }
}

impl ConcurrencyStrategy for JsonStrategy {
    fn execute_read(&self, query: &str) -> Result<Value, String> {
        // Read JSON file with file-level lock
        let file_path = self.base_path.join(query);

        let locks = self.file_locks.lock().unwrap();
        let file_lock = locks.get(&file_path)
            .ok_or_else(|| "File not found".to_string())?;

        let _guard = file_lock.lock().unwrap();

        let content = fs::read_to_string(&file_path)
            .map_err(|e| format!("Failed to read file: {}", e))?;

        serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse JSON: {}", e))
    }

    fn execute_write(&self, query: &str, params: &Value) -> Result<Value, String> {
        // Write JSON file with file-level lock
        let file_path = self.base_path.join(query);

        let mut locks = self.file_locks.lock().unwrap();
        let file_lock = locks.entry(file_path.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())));

        let _guard = file_lock.lock().unwrap();

        let content = serde_json::to_string_pretty(params)
            .map_err(|e| format!("Failed to serialize JSON: {}", e))?;

        fs::write(&file_path, content)
            .map_err(|e| format!("Failed to write file: {}", e))?;

        Ok(json!({ "success": true }))
    }

    fn close(&self) -> Result<(), String> {
        println!("✅ JSON adapter closed");
        Ok(())
    }
}

// ============================================================================
// Adapter Registry
// ============================================================================

struct AdapterRegistry {
    adapters: Arc<Mutex<HashMap<AdapterHandle, (AdapterType, Box<dyn ConcurrencyStrategy>)>>>,
}

impl AdapterRegistry {
    fn new() -> Self {
        Self {
            adapters: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    fn register(&self, adapter_type: AdapterType, strategy: Box<dyn ConcurrencyStrategy>) -> AdapterHandle {
        let handle = AdapterHandle::new();
        let mut adapters = self.adapters.lock().unwrap();
        adapters.insert(handle, (adapter_type.clone(), strategy));

        println!("📝 Registered adapter: {:?} with handle {:?}", adapter_type, handle);
        handle
    }

    /// Execute a read operation on an adapter
    fn execute_read(&self, handle: AdapterHandle, query: &str) -> Result<Value, String> {
        let adapters = self.adapters.lock().unwrap();
        let (_, strategy) = adapters.get(&handle)
            .ok_or_else(|| format!("Adapter not found: {:?}", handle))?;
        strategy.execute_read(query)
    }

    /// Execute a write operation on an adapter
    fn execute_write(&self, handle: AdapterHandle, query: &str, params: &Value) -> Result<Value, String> {
        let adapters = self.adapters.lock().unwrap();
        let (_, strategy) = adapters.get(&handle)
            .ok_or_else(|| format!("Adapter not found: {:?}", handle))?;
        strategy.execute_write(query, params)
    }

    fn close(&self, handle: AdapterHandle) -> Result<(), String> {
        let mut adapters = self.adapters.lock().unwrap();
        if let Some((adapter_type, strategy)) = adapters.remove(&handle) {
            strategy.close()?;
            println!("🗑️  Closed adapter: {:?} with handle {:?}", adapter_type, handle);
            Ok(())
        } else {
            Err(format!("Adapter not found: {:?}", handle))
        }
    }
}

// ============================================================================
// RustDataDaemon - Main Coordinator
// ============================================================================

struct RustDataDaemon {
    registry: Arc<AdapterRegistry>,
}

impl RustDataDaemon {
    fn new() -> Self {
        Self {
            registry: Arc::new(AdapterRegistry::new()),
        }
    }

    fn handle_request(&self, request: Request) -> Response {
        match request {
            Request::Ping => Response::Pong { uptime_seconds: 0 },

            Request::AdapterOpen { config } => {
                match self.open_adapter(config) {
                    Ok(handle) => Response::Ok {
                        data: json!({ "handle": handle })
                    },
                    Err(e) => Response::Error { message: e },
                }
            }

            Request::AdapterClose { handle } => {
                match self.registry.close(handle) {
                    Ok(_) => Response::Ok {
                        data: json!({ "closed": true })
                    },
                    Err(e) => Response::Error { message: e },
                }
            }

            Request::DataList { handle, collection, limit, offset, filter, order_by } => {
                match self.data_list(handle, &collection, limit, offset, filter.as_ref(), order_by.as_ref()) {
                    Ok(data) => Response::Ok { data },
                    Err(e) => Response::Error { message: e },
                }
            }

            Request::DataCreate { handle, collection, data } => {
                match self.data_create(handle, &collection, &data) {
                    Ok(result) => Response::Ok { data: result },
                    Err(e) => Response::Error { message: e },
                }
            }

            Request::DataDelete { handle, collection, id } => {
                match self.data_delete(handle, &collection, &id) {
                    Ok(result) => Response::Ok { data: result },
                    Err(e) => Response::Error { message: e },
                }
            }

            Request::DataUpdate { handle, collection, id, data } => {
                match self.data_update(handle, &collection, &id, &data) {
                    Ok(result) => Response::Ok { data: result },
                    Err(e) => Response::Error { message: e },
                }
            }
        }
    }

    fn open_adapter(&self, config: AdapterConfig) -> Result<AdapterHandle, String> {
        let strategy: Box<dyn ConcurrencyStrategy> = match config.adapter_type {
            AdapterType::Sqlite => {
                Box::new(SqliteStrategy::new(config.connection_string)?)
            }
            AdapterType::Postgres => {
                Box::new(PostgresStrategy {})
            }
            AdapterType::Json => {
                Box::new(JsonStrategy::new(config.connection_string)?)
            }
        };

        let handle = self.registry.register(config.adapter_type, strategy);
        Ok(handle)
    }

    /// List entities from a collection with filtering and pagination
    fn data_list(
        &self,
        handle: AdapterHandle,
        collection: &str,
        limit: Option<usize>,
        offset: Option<usize>,
        filter: Option<&Value>,
        order_by: Option<&Vec<OrderBy>>,
    ) -> Result<Value, String> {
        // Build SELECT query
        let mut query = format!("SELECT * FROM {}", collection);

        // Add WHERE clause from filter
        if let Some(filter_obj) = filter {
            if let Some(obj) = filter_obj.as_object() {
                let conditions: Vec<String> = obj.iter()
                    .filter_map(|(key, value)| {
                        match value {
                            Value::String(s) => Some(format!("{} = '{}'", key, s.replace("'", "''"))),
                            Value::Number(n) => Some(format!("{} = {}", key, n)),
                            Value::Bool(b) => Some(format!("{} = {}", key, if *b { 1 } else { 0 })),
                            Value::Null => Some(format!("{} IS NULL", key)),
                            _ => None, // Skip complex nested objects for now
                        }
                    })
                    .collect();

                if !conditions.is_empty() {
                    query.push_str(" WHERE ");
                    query.push_str(&conditions.join(" AND "));
                }
            }
        }

        // Add ORDER BY
        if let Some(orders) = order_by {
            if !orders.is_empty() {
                let order_clauses: Vec<String> = orders.iter()
                    .map(|o| format!("{} {}", o.field, o.direction.to_uppercase()))
                    .collect();
                query.push_str(" ORDER BY ");
                query.push_str(&order_clauses.join(", "));
            }
        }

        // Add LIMIT and OFFSET
        if let Some(lim) = limit {
            query.push_str(&format!(" LIMIT {}", lim));
        }
        if let Some(off) = offset {
            query.push_str(&format!(" OFFSET {}", off));
        }

        println!("📋 DataList query: {}", query);
        self.registry.execute_read(handle, &query)
    }

    /// Create a new entity in a collection
    fn data_create(
        &self,
        handle: AdapterHandle,
        collection: &str,
        data: &Value,
    ) -> Result<Value, String> {
        let obj = data.as_object()
            .ok_or_else(|| "Data must be an object".to_string())?;

        // Build INSERT query
        let columns: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
        let values: Vec<String> = obj.values()
            .map(|v| match v {
                Value::String(s) => format!("'{}'", s.replace("'", "''")),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => if *b { "1" } else { "0" }.to_string(),
                Value::Null => "NULL".to_string(),
                Value::Array(_) | Value::Object(_) => {
                    // Serialize complex types as JSON strings
                    format!("'{}'", serde_json::to_string(v).unwrap_or_default().replace("'", "''"))
                }
            })
            .collect();

        let query = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            collection,
            columns.join(", "),
            values.join(", ")
        );

        println!("➕ DataCreate query: {}", query);
        self.registry.execute_write(handle, &query, data)
    }

    /// Delete an entity from a collection by ID
    fn data_delete(
        &self,
        handle: AdapterHandle,
        collection: &str,
        id: &str,
    ) -> Result<Value, String> {
        let query = format!("DELETE FROM {} WHERE id = '{}'", collection, id.replace("'", "''"));

        println!("🗑️  DataDelete query: {}", query);
        self.registry.execute_write(handle, &query, &json!({}))
    }

    /// Update an entity in a collection by ID
    fn data_update(
        &self,
        handle: AdapterHandle,
        collection: &str,
        id: &str,
        data: &Value,
    ) -> Result<Value, String> {
        let obj = data.as_object()
            .ok_or_else(|| "Data must be an object".to_string())?;

        // Build SET clauses
        let set_clauses: Vec<String> = obj.iter()
            .filter(|(key, _)| *key != "id") // Don't update id
            .map(|(key, value)| {
                let val_str = match value {
                    Value::String(s) => format!("'{}'", s.replace("'", "''")),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => if *b { "1" } else { "0" }.to_string(),
                    Value::Null => "NULL".to_string(),
                    Value::Array(_) | Value::Object(_) => {
                        // Serialize complex types as JSON strings
                        format!("'{}'", serde_json::to_string(value).unwrap_or_default().replace("'", "''"))
                    }
                };
                format!("{} = {}", key, val_str)
            })
            .collect();

        if set_clauses.is_empty() {
            return Err("No fields to update".to_string());
        }

        let query = format!(
            "UPDATE {} SET {} WHERE id = '{}'",
            collection,
            set_clauses.join(", "),
            id.replace("'", "''")
        );

        println!("✏️  DataUpdate query: {}", query);
        self.registry.execute_write(handle, &query, data)
    }
}

// ============================================================================
// Connection Handler (Same pattern as ArchiveWorker)
// ============================================================================

fn handle_connection(stream: UnixStream, daemon: Arc<RustDataDaemon>) -> std::io::Result<()> {
    let mut reader = BufReader::new(&stream);
    let mut writer = stream.try_clone()?;

    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 { break; }

        let request: Request = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                eprintln!("Parse error: {}", e);
                continue;
            }
        };

        let response = daemon.handle_request(request);

        let response_json = serde_json::to_string(&response)?;
        writeln!(writer, "{}", response_json)?;
        writer.flush()?;
    }

    Ok(())
}

// ============================================================================
// Main Entry Point
// ============================================================================

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <worker-socket>", args[0]);
        eprintln!("Example: {} /tmp/jtag-data-daemon.sock", args[0]);
        std::process::exit(1);
    }

    let worker_socket = &args[1];

    // Remove socket if exists
    if fs::metadata(worker_socket).is_ok() {
        fs::remove_file(worker_socket)?;
    }

    println!("🦀 RustDataDaemon starting...");
    println!("📡 Worker socket: {}", worker_socket);

    let daemon = Arc::new(RustDataDaemon::new());
    println!("✅ RustDataDaemon ready\n");

    // Bind socket
    let listener = UnixListener::bind(worker_socket)?;
    println!("✅ Listening for connections\n");

    // Accept connections
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                let daemon_clone = daemon.clone();
                thread::spawn(move || {
                    if let Err(e) = handle_connection(stream, daemon_clone) {
                        eprintln!("Connection error: {}", e);
                    }
                });
            }
            Err(e) => eprintln!("Accept error: {}", e),
        }
    }

    Ok(())
}
