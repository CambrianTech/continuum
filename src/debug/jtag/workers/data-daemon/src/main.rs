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
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{HashMap, VecDeque};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::time::Instant;
use std::{fs, thread};
use ts_rs::TS;
use uuid::Uuid;

mod timing;
use timing::{RequestTimer, METRICS};

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

    /// Vector similarity search - reads vectors from SQLite, computes cosine similarity
    /// Query vector comes from TypeScript (small: 384 floats), corpus stays in Rust
    /// Returns full records with scores (not just IDs) to avoid k IPC round trips
    #[serde(rename = "vector/search")]
    VectorSearch {
        handle: AdapterHandle,
        collection: String,
        query_vector: Vec<f64>,
        k: Option<usize>,
        threshold: Option<f64>,
        /// If true, return full record data (not just IDs) - eliminates k IPC round trips
        include_data: Option<bool>,
    },

    /// Store JSON data in content-addressable blob storage
    /// Returns sha256 hash for retrieval
    #[serde(rename = "blob/store")]
    BlobStore {
        /// JSON data to store (will be compressed)
        data: Value,
        /// Base path for blob storage (default: ~/.continuum/blobs)
        base_path: Option<String>,
    },

    /// Retrieve JSON data from blob storage by hash
    #[serde(rename = "blob/retrieve")]
    BlobRetrieve {
        /// SHA256 hash (format: "sha256:abc123...")
        hash: String,
        /// Base path for blob storage
        base_path: Option<String>,
    },

    /// Check if blob exists
    #[serde(rename = "blob/exists")]
    BlobExists {
        hash: String,
        base_path: Option<String>,
    },

    /// Delete blob by hash
    #[serde(rename = "blob/delete")]
    BlobDelete {
        hash: String,
        base_path: Option<String>,
    },

    /// Get blob storage statistics
    #[serde(rename = "blob/stats")]
    BlobStats { base_path: Option<String> },

    /// Execute a raw SQL query with optional JOIN support
    /// Returns raw query results - caller does any transformation
    /// Use for complex queries that would otherwise require multiple IPC round trips
    #[serde(rename = "data/query")]
    DataQuery {
        handle: AdapterHandle,
        /// Raw SQL query string (SELECT only, no modifications)
        sql: String,
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

    /// Vector similarity search - reads vectors from storage, computes cosine similarity
    /// Returns top-k results with record IDs/scores, optionally with full record data
    fn vector_search(
        &self,
        collection: &str,
        query_vector: &[f64],
        k: usize,
        threshold: f64,
        include_data: bool,
    ) -> Result<Value, String>;

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
            .args(["info", &format!("/Volumes/{volume_name}")])
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
///
/// IMPORTANT: In multi_writer mode, we NEVER set journal_mode or locking_mode.
/// TypeScript (better-sqlite3) already has the database open, and changing these
/// pragmas requires exclusive access which would fail with "database is locked".
fn get_sqlite_pragmas(storage: StorageType, multi_writer: bool) -> String {
    if multi_writer {
        // Multi-writer mode: Only set pragmas that don't require exclusive access
        // Skip journal_mode (TypeScript already set it)
        // Skip locking_mode (would conflict with TypeScript)
        "PRAGMA synchronous=NORMAL; \
         PRAGMA temp_store=MEMORY; \
         PRAGMA busy_timeout=5000;"
            .to_string()
    } else {
        // Single-writer mode: Can set everything
        match storage {
            StorageType::InternalSSD => "PRAGMA journal_mode=WAL; \
                 PRAGMA synchronous=NORMAL; \
                 PRAGMA temp_store=MEMORY; \
                 PRAGMA locking_mode=EXCLUSIVE; \
                 PRAGMA busy_timeout=5000;"
                .to_string(),
            StorageType::ExternalSSD => "PRAGMA journal_mode=WAL; \
                 PRAGMA synchronous=NORMAL; \
                 PRAGMA wal_autocheckpoint=1000; \
                 PRAGMA temp_store=MEMORY; \
                 PRAGMA busy_timeout=5000;"
                .to_string(),
            StorageType::SDCard | StorageType::HDD | StorageType::Unknown => {
                "PRAGMA journal_mode=DELETE; \
                 PRAGMA synchronous=NORMAL; \
                 PRAGMA temp_store=MEMORY; \
                 PRAGMA locking_mode=EXCLUSIVE; \
                 PRAGMA busy_timeout=5000;"
                    .to_string()
            }
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

#[allow(dead_code)]
struct WriteOperation {
    query: String,
    params: Value, // Reserved for parameterized queries
}

/// Compute cosine similarity between two vectors
/// Uses SIMD-friendly 8-way loop unrolling for auto-vectorization
#[inline]
fn cosine_similarity(a: &[f64], b: &[f64]) -> f64 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    // 8-way loop unrolling for SIMD auto-vectorization
    let len = a.len();
    let chunks = len / 8;
    let remainder = len % 8;

    let mut dot0 = 0.0;
    let mut dot1 = 0.0;
    let mut dot2 = 0.0;
    let mut dot3 = 0.0;
    let mut dot4 = 0.0;
    let mut dot5 = 0.0;
    let mut dot6 = 0.0;
    let mut dot7 = 0.0;

    let mut norm_a0 = 0.0;
    let mut norm_a1 = 0.0;
    let mut norm_a2 = 0.0;
    let mut norm_a3 = 0.0;
    let mut norm_a4 = 0.0;
    let mut norm_a5 = 0.0;
    let mut norm_a6 = 0.0;
    let mut norm_a7 = 0.0;

    let mut norm_b0 = 0.0;
    let mut norm_b1 = 0.0;
    let mut norm_b2 = 0.0;
    let mut norm_b3 = 0.0;
    let mut norm_b4 = 0.0;
    let mut norm_b5 = 0.0;
    let mut norm_b6 = 0.0;
    let mut norm_b7 = 0.0;

    // Process 8 elements at a time
    for i in 0..chunks {
        let base = i * 8;
        let a0 = a[base];
        let a1 = a[base + 1];
        let a2 = a[base + 2];
        let a3 = a[base + 3];
        let a4 = a[base + 4];
        let a5 = a[base + 5];
        let a6 = a[base + 6];
        let a7 = a[base + 7];

        let b0 = b[base];
        let b1 = b[base + 1];
        let b2 = b[base + 2];
        let b3 = b[base + 3];
        let b4 = b[base + 4];
        let b5 = b[base + 5];
        let b6 = b[base + 6];
        let b7 = b[base + 7];

        dot0 += a0 * b0;
        dot1 += a1 * b1;
        dot2 += a2 * b2;
        dot3 += a3 * b3;
        dot4 += a4 * b4;
        dot5 += a5 * b5;
        dot6 += a6 * b6;
        dot7 += a7 * b7;

        norm_a0 += a0 * a0;
        norm_a1 += a1 * a1;
        norm_a2 += a2 * a2;
        norm_a3 += a3 * a3;
        norm_a4 += a4 * a4;
        norm_a5 += a5 * a5;
        norm_a6 += a6 * a6;
        norm_a7 += a7 * a7;

        norm_b0 += b0 * b0;
        norm_b1 += b1 * b1;
        norm_b2 += b2 * b2;
        norm_b3 += b3 * b3;
        norm_b4 += b4 * b4;
        norm_b5 += b5 * b5;
        norm_b6 += b6 * b6;
        norm_b7 += b7 * b7;
    }

    // Combine accumulators
    let mut dot = dot0 + dot1 + dot2 + dot3 + dot4 + dot5 + dot6 + dot7;
    let mut norm_a = norm_a0 + norm_a1 + norm_a2 + norm_a3 + norm_a4 + norm_a5 + norm_a6 + norm_a7;
    let mut norm_b = norm_b0 + norm_b1 + norm_b2 + norm_b3 + norm_b4 + norm_b5 + norm_b6 + norm_b7;

    // Handle remainder
    let base = chunks * 8;
    for i in 0..remainder {
        let av = a[base + i];
        let bv = b[base + i];
        dot += av * bv;
        norm_a += av * av;
        norm_b += bv * bv;
    }

    let denominator = (norm_a * norm_b).sqrt();
    if denominator == 0.0 {
        0.0
    } else {
        dot / denominator
    }
}

/// Deserialize BLOB to f64 vector
/// Format: raw little-endian f64 bytes (8 bytes per float)
fn blob_to_f64_vec(blob: &[u8]) -> Vec<f64> {
    let num_floats = blob.len() / 8;
    let mut result = Vec::with_capacity(num_floats);

    for i in 0..num_floats {
        let start = i * 8;
        let bytes: [u8; 8] = blob[start..start + 8].try_into().unwrap_or([0u8; 8]);
        result.push(f64::from_le_bytes(bytes));
    }

    result
}

impl SqliteStrategy {
    fn new(connection_path: String) -> Result<Self, String> {
        // Detect storage type by sampling system
        let storage_type = detect_storage_type(Path::new(&connection_path));

        println!("🔍 Detected storage type: {storage_type:?} for {connection_path}");

        // Check for WAL artifacts before opening (indicates prior WAL mode usage)
        let wal_path = format!("{connection_path}-wal");
        let shm_path = format!("{connection_path}-shm");
        if Path::new(&wal_path).exists() || Path::new(&shm_path).exists() {
            println!(
                "⚠️  WAL artifacts exist for {connection_path} - prior connection may have crashed"
            );
        }

        // Open connection
        let conn = rusqlite::Connection::open(&connection_path)
            .map_err(|e| format!("Failed to open SQLite: {e}"))?;

        // Configure with multi_writer=true since TypeScript (better-sqlite3) may have the database open
        // SKIP journal_mode and locking_mode changes - they require exclusive access
        // SKIP checkpoint - also requires exclusive access when other connections exist
        let pragmas = get_sqlite_pragmas(storage_type, true);
        conn.execute_batch(&pragmas)
            .map_err(|e| format!("Failed to configure SQLite: {e}"))?;

        let mode_desc = match storage_type {
            StorageType::InternalSSD => "WAL mode - internal SSD optimized",
            StorageType::ExternalSSD => "WAL mode - external SSD optimized",
            _ => "DELETE mode - SD card/HDD reliable",
        };

        println!("✅ SQLite adapter opened: {connection_path} ({mode_desc})");

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
            let rows_affected = conn
                .execute(&write_op.query, [])
                .map_err(|e| format!("SQLite write failed: {e}"))?;

            results.push(json!({ "rows_affected": rows_affected }));
        }

        Ok(json!({ "results": results }))
    }
}

impl ConcurrencyStrategy for SqliteStrategy {
    fn execute_read(&self, query: &str) -> Result<Value, String> {
        // Reads can run in parallel (WAL mode allows this)
        let conn = self.connection.lock().unwrap();

        let mut stmt = conn
            .prepare(query)
            .map_err(|e| format!("Failed to prepare query: {e}"))?;

        let column_count = stmt.column_count();

        // Get column names before query_map (to avoid borrowing issues)
        let column_names: Vec<String> = (0..column_count)
            .map(|i| stmt.column_name(i).unwrap_or("unknown").to_string())
            .collect();

        let mut rows = Vec::new();

        let row_iter = stmt
            .query_map([], |row| {
                let mut row_data = serde_json::Map::new();
                for (i, column_name) in column_names.iter().enumerate() {
                    let value: Result<String, _> = row.get(i);
                    if let Ok(v) = value {
                        row_data.insert(column_name.clone(), json!(v));
                    }
                }
                Ok(Value::Object(row_data))
            })
            .map_err(|e| format!("Query execution failed: {e}"))?;

        for row in row_iter {
            rows.push(row.map_err(|e| format!("Row parse error: {e}"))?);
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

    /// Vector search: read embeddings from SQLite, compute cosine similarity with rayon
    /// Vectors stay in Rust - only query vector comes over IPC (small: 3KB for 384 dims)
    /// When include_data=true, returns full record data with scores (eliminates k IPC round trips)
    fn vector_search(
        &self,
        collection: &str,
        query_vector: &[f64],
        k: usize,
        threshold: f64,
        include_data: bool,
    ) -> Result<Value, String> {
        let conn = self.connection.lock().unwrap();

        // Query embeddings from the collection
        // Embeddings are stored as BLOB in the 'embedding' column
        let query = format!("SELECT id, embedding FROM {collection} WHERE embedding IS NOT NULL");

        let mut stmt = conn
            .prepare(&query)
            .map_err(|e| format!("Failed to prepare vector query: {e}"))?;

        // Collect all vectors first (need to release connection lock before parallel work)
        let mut corpus: Vec<(String, Vec<f64>)> = Vec::new();

        let rows = stmt
            .query_map([], |row| {
                let id: String = row.get(0)?;
                // Try BLOB first, then TEXT (JSON array)
                let embedding: Vec<f64> = if let Ok(blob) = row.get::<_, Vec<u8>>(1) {
                    blob_to_f64_vec(&blob)
                } else if let Ok(text) = row.get::<_, String>(1) {
                    // Parse JSON array: "[0.1, 0.2, ...]"
                    serde_json::from_str(&text).unwrap_or_default()
                } else {
                    Vec::new()
                };
                Ok((id, embedding))
            })
            .map_err(|e| format!("Vector query failed: {e}"))?;

        for row in rows {
            let (id, embedding) = row.map_err(|e| format!("Row error: {e}"))?;
            if !embedding.is_empty() {
                corpus.push((id, embedding));
            }
        }

        // Release statement before parallel computation
        drop(stmt);

        if corpus.is_empty() {
            return Ok(json!({
                "results": [],
                "count": 0,
                "corpus_size": 0
            }));
        }

        let corpus_size = corpus.len();

        // Parallel cosine similarity computation with rayon
        let mut scored: Vec<(String, f64)> = corpus
            .par_iter()
            .filter_map(|(id, embedding)| {
                let score = cosine_similarity(query_vector, embedding);
                if score >= threshold {
                    Some((id.clone(), score))
                } else {
                    None
                }
            })
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top-k IDs
        let top_k: Vec<(String, f64)> = scored.into_iter().take(k).collect();
        let count = top_k.len();

        if !include_data || top_k.is_empty() {
            // Fast path: just return IDs and scores
            let results: Vec<Value> = top_k
                .into_iter()
                .map(|(id, score)| {
                    json!({
                        "id": id,
                        "score": score,
                        "distance": 1.0 - score
                    })
                })
                .collect();

            return Ok(json!({
                "results": results,
                "count": count,
                "corpus_size": corpus_size
            }));
        }

        // Optimized path: fetch full records for top-k IDs in a single query
        // Build IN clause with top-k IDs
        let id_list: Vec<String> = top_k
            .iter()
            .map(|(id, _)| format!("'{}'", id.replace("'", "''")))
            .collect();
        let full_query = format!(
            "SELECT * FROM {} WHERE id IN ({})",
            collection,
            id_list.join(", ")
        );

        let mut full_stmt = conn
            .prepare(&full_query)
            .map_err(|e| format!("Failed to prepare full record query: {e}"))?;

        // Get column names
        let column_count = full_stmt.column_count();
        let column_names: Vec<String> = (0..column_count)
            .map(|i| full_stmt.column_name(i).unwrap_or("unknown").to_string())
            .collect();

        // Fetch all records into a map by ID
        let mut records_by_id: HashMap<String, Value> = HashMap::new();

        let record_rows = full_stmt
            .query_map([], |row| {
                let mut row_data = serde_json::Map::new();
                for (i, column_name) in column_names.iter().enumerate() {
                    // Skip embedding column entirely (large, not needed in results)
                    if column_name == "embedding" {
                        continue;
                    }
                    // Try to get as different types
                    if let Ok(v) = row.get::<_, String>(i) {
                        row_data.insert(column_name.clone(), json!(v));
                    } else if let Ok(v) = row.get::<_, i64>(i) {
                        row_data.insert(column_name.clone(), json!(v));
                    } else if let Ok(v) = row.get::<_, f64>(i) {
                        row_data.insert(column_name.clone(), json!(v));
                    } else if let Ok(v) = row.get::<_, Vec<u8>>(i) {
                        row_data.insert(
                            column_name.clone(),
                            json!(format!("[BLOB {} bytes]", v.len())),
                        );
                    } else {
                        row_data.insert(column_name.clone(), Value::Null);
                    }
                }
                Ok(Value::Object(row_data))
            })
            .map_err(|e| format!("Full record query failed: {e}"))?;

        for row_result in record_rows {
            let row = row_result.map_err(|e| format!("Row error: {e}"))?;
            if let Some(id) = row.get("id").and_then(|v| v.as_str()) {
                records_by_id.insert(id.to_string(), row);
            }
        }

        // Build results in score order, merging data with scores
        let results: Vec<Value> = top_k
            .into_iter()
            .filter_map(|(id, score)| {
                records_by_id.get(&id).map(|data| {
                    json!({
                        "id": id,
                        "score": score,
                        "distance": 1.0 - score,
                        "data": data
                    })
                })
            })
            .collect();

        let final_count = results.len();
        Ok(json!({
            "results": results,
            "count": final_count,
            "corpus_size": corpus_size
        }))
    }

    fn close(&self) -> Result<(), String> {
        // Process any remaining writes before closing
        let queue_size = self.writer_queue.lock().unwrap().len();
        if queue_size > 0 {
            println!("⚠️  Closing SQLite adapter with {queue_size} pending writes");
        }

        // Checkpoint WAL if using WAL mode (ensure data persistence)
        if matches!(
            self.storage_type,
            StorageType::InternalSSD | StorageType::ExternalSSD
        ) {
            let conn = self.connection.lock().unwrap();
            println!("📝 Checkpointing WAL before close...");

            // TRUNCATE mode: checkpoint and delete WAL files
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
                .map_err(|e| format!("Failed to checkpoint WAL on close: {e}"))?;

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

    fn vector_search(
        &self,
        _collection: &str,
        _query_vector: &[f64],
        _k: usize,
        _threshold: f64,
        _include_data: bool,
    ) -> Result<Value, String> {
        Err("Postgres vector search not yet implemented".to_string())
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
        let file_lock = locks
            .get(&file_path)
            .ok_or_else(|| "File not found".to_string())?;

        let _guard = file_lock.lock().unwrap();

        let content =
            fs::read_to_string(&file_path).map_err(|e| format!("Failed to read file: {e}"))?;

        serde_json::from_str(&content).map_err(|e| format!("Failed to parse JSON: {e}"))
    }

    fn execute_write(&self, query: &str, params: &Value) -> Result<Value, String> {
        // Write JSON file with file-level lock
        let file_path = self.base_path.join(query);

        let mut locks = self.file_locks.lock().unwrap();
        let file_lock = locks
            .entry(file_path.clone())
            .or_insert_with(|| Arc::new(Mutex::new(())));

        let _guard = file_lock.lock().unwrap();

        let content = serde_json::to_string_pretty(params)
            .map_err(|e| format!("Failed to serialize JSON: {e}"))?;

        fs::write(&file_path, content).map_err(|e| format!("Failed to write file: {e}"))?;

        Ok(json!({ "success": true }))
    }

    fn vector_search(
        &self,
        _collection: &str,
        _query_vector: &[f64],
        _k: usize,
        _threshold: f64,
        _include_data: bool,
    ) -> Result<Value, String> {
        Err("JSON vector search not yet implemented".to_string())
    }

    fn close(&self) -> Result<(), String> {
        println!("✅ JSON adapter closed");
        Ok(())
    }
}

// ============================================================================
// Adapter Registry - with path-based caching for concurrent access
// ============================================================================

struct AdapterRegistry {
    adapters: Arc<Mutex<HashMap<AdapterHandle, (AdapterType, Arc<dyn ConcurrencyStrategy>)>>>,
    /// Cache: database path → shared adapter (prevents concurrent opens of same DB)
    path_cache: Arc<Mutex<HashMap<String, Arc<dyn ConcurrencyStrategy>>>>,
    /// Serializes adapter opening to prevent concurrent SQLite pragma configuration
    open_lock: Arc<Mutex<()>>,
}

impl AdapterRegistry {
    fn new() -> Self {
        Self {
            adapters: Arc::new(Mutex::new(HashMap::new())),
            path_cache: Arc::new(Mutex::new(HashMap::new())),
            open_lock: Arc::new(Mutex::new(())),
        }
    }

    /// Register an adapter, reusing cached connection if available
    fn register_with_cache(
        &self,
        adapter_type: AdapterType,
        path: &str,
    ) -> Result<AdapterHandle, String> {
        // Serialize all opens to prevent concurrent pragma configuration
        let _open_guard = self.open_lock.lock().unwrap();

        // Check cache first
        {
            let cache = self.path_cache.lock().unwrap();
            if let Some(existing) = cache.get(path) {
                // Reuse existing adapter
                let handle = AdapterHandle::new();
                let mut adapters = self.adapters.lock().unwrap();
                adapters.insert(handle, (adapter_type.clone(), existing.clone()));
                println!("♻️  Reusing cached adapter for: {path} → {handle:?}");
                return Ok(handle);
            }
        }

        // Create new adapter (still under open_lock)
        let strategy: Arc<dyn ConcurrencyStrategy> = match adapter_type {
            AdapterType::Sqlite => Arc::new(SqliteStrategy::new(path.to_string())?),
            AdapterType::Postgres => Arc::new(PostgresStrategy {}),
            AdapterType::Json => Arc::new(JsonStrategy::new(path.to_string())?),
        };

        // Cache the new adapter
        {
            let mut cache = self.path_cache.lock().unwrap();
            cache.insert(path.to_string(), strategy.clone());
        }

        // Register with new handle
        let handle = AdapterHandle::new();
        {
            let mut adapters = self.adapters.lock().unwrap();
            adapters.insert(handle, (adapter_type.clone(), strategy));
        }

        println!("📝 Registered new adapter: {path} → {handle:?}");
        Ok(handle)
    }

    /// Execute a read operation on an adapter
    fn execute_read(&self, handle: AdapterHandle, query: &str) -> Result<Value, String> {
        let adapters = self.adapters.lock().unwrap();
        let (_, strategy) = adapters
            .get(&handle)
            .ok_or_else(|| format!("Adapter not found: {handle:?}"))?;
        strategy.execute_read(query)
    }

    /// Execute a write operation on an adapter
    fn execute_write(
        &self,
        handle: AdapterHandle,
        query: &str,
        params: &Value,
    ) -> Result<Value, String> {
        let adapters = self.adapters.lock().unwrap();
        let (_, strategy) = adapters
            .get(&handle)
            .ok_or_else(|| format!("Adapter not found: {handle:?}"))?;
        strategy.execute_write(query, params)
    }

    /// Execute vector similarity search on an adapter
    fn vector_search(
        &self,
        handle: AdapterHandle,
        collection: &str,
        query_vector: &[f64],
        k: usize,
        threshold: f64,
        include_data: bool,
    ) -> Result<Value, String> {
        let adapters = self.adapters.lock().unwrap();
        let (_, strategy) = adapters
            .get(&handle)
            .ok_or_else(|| format!("Adapter not found: {handle:?}"))?;
        strategy.vector_search(collection, query_vector, k, threshold, include_data)
    }

    fn close(&self, handle: AdapterHandle) -> Result<(), String> {
        let mut adapters = self.adapters.lock().unwrap();
        if let Some((adapter_type, strategy)) = adapters.remove(&handle) {
            strategy.close()?;
            println!("🗑️  Closed adapter: {adapter_type:?} with handle {handle:?}");
            Ok(())
        } else {
            Err(format!("Adapter not found: {handle:?}"))
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

    #[allow(dead_code)]
    fn handle_request(&self, request: Request) -> Response {
        match request {
            Request::Ping => Response::Pong { uptime_seconds: 0 },

            Request::AdapterOpen { config } => match self.open_adapter(config) {
                Ok(handle) => Response::Ok {
                    data: json!({ "handle": handle }),
                },
                Err(e) => Response::Error { message: e },
            },

            Request::AdapterClose { handle } => match self.registry.close(handle) {
                Ok(_) => Response::Ok {
                    data: json!({ "closed": true }),
                },
                Err(e) => Response::Error { message: e },
            },

            Request::DataList {
                handle,
                collection,
                limit,
                offset,
                filter,
                order_by,
            } => {
                match self.data_list(
                    handle,
                    &collection,
                    limit,
                    offset,
                    filter.as_ref(),
                    order_by.as_ref(),
                ) {
                    Ok(data) => Response::Ok { data },
                    Err(e) => Response::Error { message: e },
                }
            }

            Request::DataCreate {
                handle,
                collection,
                data,
            } => match self.data_create(handle, &collection, &data) {
                Ok(result) => Response::Ok { data: result },
                Err(e) => Response::Error { message: e },
            },

            Request::DataDelete {
                handle,
                collection,
                id,
            } => match self.data_delete(handle, &collection, &id) {
                Ok(result) => Response::Ok { data: result },
                Err(e) => Response::Error { message: e },
            },

            Request::DataUpdate {
                handle,
                collection,
                id,
                data,
            } => match self.data_update(handle, &collection, &id, &data) {
                Ok(result) => Response::Ok { data: result },
                Err(e) => Response::Error { message: e },
            },

            Request::VectorSearch {
                handle,
                collection,
                query_vector,
                k,
                threshold,
                include_data,
            } => {
                match self.vector_search(
                    handle,
                    &collection,
                    &query_vector,
                    k,
                    threshold,
                    include_data,
                ) {
                    Ok(data) => Response::Ok { data },
                    Err(e) => Response::Error { message: e },
                }
            }

            Request::BlobStore { data, base_path } => {
                match self.blob_store(&data, base_path.as_deref()) {
                    Ok(result) => Response::Ok { data: result },
                    Err(e) => Response::Error { message: e },
                }
            }

            Request::BlobRetrieve { hash, base_path } => {
                match self.blob_retrieve(&hash, base_path.as_deref()) {
                    Ok(data) => Response::Ok { data },
                    Err(e) => Response::Error { message: e },
                }
            }

            Request::BlobExists { hash, base_path } => {
                match self.blob_exists(&hash, base_path.as_deref()) {
                    Ok(exists) => Response::Ok {
                        data: json!({ "exists": exists }),
                    },
                    Err(e) => Response::Error { message: e },
                }
            }

            Request::BlobDelete { hash, base_path } => {
                match self.blob_delete(&hash, base_path.as_deref()) {
                    Ok(deleted) => Response::Ok {
                        data: json!({ "deleted": deleted }),
                    },
                    Err(e) => Response::Error { message: e },
                }
            }

            Request::BlobStats { base_path } => match self.blob_stats(base_path.as_deref()) {
                Ok(stats) => Response::Ok { data: stats },
                Err(e) => Response::Error { message: e },
            },

            Request::DataQuery { handle, sql } => match self.data_query(handle, &sql) {
                Ok(data) => Response::Ok { data },
                Err(e) => Response::Error { message: e },
            },
        }
    }

    /// Timed version of handle_request that fills in timing phases
    /// Returns (response, result_count) for metrics
    fn handle_request_timed(
        &self,
        timer: &mut RequestTimer,
        request: Request,
    ) -> (Response, Option<usize>) {
        let route_start = Instant::now();

        match request {
            Request::Ping => {
                timer.record.route_ns = route_start.elapsed().as_nanos() as u64;
                (Response::Pong { uptime_seconds: 0 }, None)
            }

            Request::AdapterOpen { config } => {
                timer.record.route_ns = route_start.elapsed().as_nanos() as u64;
                let execute_start = Instant::now();
                let result = self.open_adapter(config);
                timer.record.execute_ns = execute_start.elapsed().as_nanos() as u64;

                match result {
                    Ok(handle) => {
                        timer.set_adapter_handle(&format!("{handle:?}"));
                        (
                            Response::Ok {
                                data: json!({ "handle": handle }),
                            },
                            None,
                        )
                    }
                    Err(e) => {
                        timer.set_error(&e);
                        (Response::Error { message: e }, None)
                    }
                }
            }

            Request::AdapterClose { handle } => {
                timer.set_adapter_handle(&format!("{handle:?}"));
                timer.record.route_ns = route_start.elapsed().as_nanos() as u64;
                let execute_start = Instant::now();
                let result = self.registry.close(handle);
                timer.record.execute_ns = execute_start.elapsed().as_nanos() as u64;

                match result {
                    Ok(_) => (
                        Response::Ok {
                            data: json!({ "closed": true }),
                        },
                        None,
                    ),
                    Err(e) => {
                        timer.set_error(&e);
                        (Response::Error { message: e }, None)
                    }
                }
            }

            Request::DataList {
                handle,
                collection,
                limit,
                offset,
                filter,
                order_by,
            } => {
                timer.set_adapter_handle(&format!("{handle:?}"));
                timer.set_collection(&collection);
                timer.record.route_ns = route_start.elapsed().as_nanos() as u64;

                let result = self.data_list_timed(
                    timer,
                    handle,
                    &collection,
                    limit,
                    offset,
                    filter.as_ref(),
                    order_by.as_ref(),
                );

                match result {
                    Ok(data) => {
                        let count = data
                            .get("count")
                            .and_then(|c| c.as_u64())
                            .map(|c| c as usize);
                        (Response::Ok { data }, count)
                    }
                    Err(e) => {
                        timer.set_error(&e);
                        (Response::Error { message: e }, None)
                    }
                }
            }

            Request::DataCreate {
                handle,
                collection,
                data,
            } => {
                timer.set_adapter_handle(&format!("{handle:?}"));
                timer.set_collection(&collection);
                timer.record.route_ns = route_start.elapsed().as_nanos() as u64;

                let result = self.data_create_timed(timer, handle, &collection, &data);

                match result {
                    Ok(data) => (Response::Ok { data }, Some(1)),
                    Err(e) => {
                        timer.set_error(&e);
                        (Response::Error { message: e }, None)
                    }
                }
            }

            Request::DataDelete {
                handle,
                collection,
                id,
            } => {
                timer.set_adapter_handle(&format!("{handle:?}"));
                timer.set_collection(&collection);
                timer.record.route_ns = route_start.elapsed().as_nanos() as u64;

                let result = self.data_delete_timed(timer, handle, &collection, &id);

                match result {
                    Ok(data) => (Response::Ok { data }, Some(1)),
                    Err(e) => {
                        timer.set_error(&e);
                        (Response::Error { message: e }, None)
                    }
                }
            }

            Request::DataUpdate {
                handle,
                collection,
                id,
                data,
            } => {
                timer.set_adapter_handle(&format!("{handle:?}"));
                timer.set_collection(&collection);
                timer.record.route_ns = route_start.elapsed().as_nanos() as u64;

                let result = self.data_update_timed(timer, handle, &collection, &id, &data);

                match result {
                    Ok(data) => (Response::Ok { data }, Some(1)),
                    Err(e) => {
                        timer.set_error(&e);
                        (Response::Error { message: e }, None)
                    }
                }
            }

            Request::VectorSearch {
                handle,
                collection,
                query_vector,
                k,
                threshold,
                include_data,
            } => {
                timer.set_adapter_handle(&format!("{handle:?}"));
                timer.set_collection(&collection);
                timer.record.route_ns = route_start.elapsed().as_nanos() as u64;

                let execute_start = Instant::now();
                let result = self.vector_search(
                    handle,
                    &collection,
                    &query_vector,
                    k,
                    threshold,
                    include_data,
                );
                timer.record.execute_ns = execute_start.elapsed().as_nanos() as u64;

                match result {
                    Ok(data) => {
                        let count = data
                            .get("count")
                            .and_then(|c| c.as_u64())
                            .map(|c| c as usize);
                        (Response::Ok { data }, count)
                    }
                    Err(e) => {
                        timer.set_error(&e);
                        (Response::Error { message: e }, None)
                    }
                }
            }

            // Blob operations (no adapter handle needed, file-based)
            Request::BlobStore { data, base_path } => {
                timer.record.route_ns = route_start.elapsed().as_nanos() as u64;
                let execute_start = Instant::now();
                let result = self.blob_store(&data, base_path.as_deref());
                timer.record.execute_ns = execute_start.elapsed().as_nanos() as u64;

                match result {
                    Ok(data) => (Response::Ok { data }, Some(1)),
                    Err(e) => {
                        timer.set_error(&e);
                        (Response::Error { message: e }, None)
                    }
                }
            }

            Request::BlobRetrieve { hash, base_path } => {
                timer.record.route_ns = route_start.elapsed().as_nanos() as u64;
                let execute_start = Instant::now();
                let result = self.blob_retrieve(&hash, base_path.as_deref());
                timer.record.execute_ns = execute_start.elapsed().as_nanos() as u64;

                match result {
                    Ok(data) => (Response::Ok { data }, Some(1)),
                    Err(e) => {
                        timer.set_error(&e);
                        (Response::Error { message: e }, None)
                    }
                }
            }

            Request::BlobExists { hash, base_path } => {
                timer.record.route_ns = route_start.elapsed().as_nanos() as u64;
                let execute_start = Instant::now();
                let result = self.blob_exists(&hash, base_path.as_deref());
                timer.record.execute_ns = execute_start.elapsed().as_nanos() as u64;

                match result {
                    Ok(exists) => (
                        Response::Ok {
                            data: json!({ "exists": exists }),
                        },
                        None,
                    ),
                    Err(e) => {
                        timer.set_error(&e);
                        (Response::Error { message: e }, None)
                    }
                }
            }

            Request::BlobDelete { hash, base_path } => {
                timer.record.route_ns = route_start.elapsed().as_nanos() as u64;
                let execute_start = Instant::now();
                let result = self.blob_delete(&hash, base_path.as_deref());
                timer.record.execute_ns = execute_start.elapsed().as_nanos() as u64;

                match result {
                    Ok(deleted) => (
                        Response::Ok {
                            data: json!({ "deleted": deleted }),
                        },
                        Some(if deleted { 1 } else { 0 }),
                    ),
                    Err(e) => {
                        timer.set_error(&e);
                        (Response::Error { message: e }, None)
                    }
                }
            }

            Request::BlobStats { base_path } => {
                timer.record.route_ns = route_start.elapsed().as_nanos() as u64;
                let execute_start = Instant::now();
                let result = self.blob_stats(base_path.as_deref());
                timer.record.execute_ns = execute_start.elapsed().as_nanos() as u64;

                match result {
                    Ok(stats) => (Response::Ok { data: stats }, None),
                    Err(e) => {
                        timer.set_error(&e);
                        (Response::Error { message: e }, None)
                    }
                }
            }

            Request::DataQuery { handle, sql } => {
                timer.set_adapter_handle(&format!("{handle:?}"));
                timer.record.route_ns = route_start.elapsed().as_nanos() as u64;

                let execute_start = Instant::now();
                let result = self.data_query(handle, &sql);
                timer.record.execute_ns = execute_start.elapsed().as_nanos() as u64;

                match result {
                    Ok(data) => {
                        let count = data
                            .get("count")
                            .and_then(|c| c.as_u64())
                            .map(|c| c as usize);
                        (Response::Ok { data }, count)
                    }
                    Err(e) => {
                        timer.set_error(&e);
                        (Response::Error { message: e }, None)
                    }
                }
            }
        }
    }

    fn open_adapter(&self, config: AdapterConfig) -> Result<AdapterHandle, String> {
        // Use register_with_cache to:
        // 1. Serialize all opens (prevents concurrent pragma configuration)
        // 2. Reuse existing adapters for same database path
        self.registry
            .register_with_cache(config.adapter_type, &config.connection_string)
    }

    /// List entities from a collection with filtering and pagination
    #[allow(dead_code)]
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
        let mut query = format!("SELECT * FROM {collection}");

        // Add WHERE clause from filter
        if let Some(filter_obj) = filter {
            if let Some(obj) = filter_obj.as_object() {
                let conditions: Vec<String> = obj
                    .iter()
                    .filter_map(|(key, value)| {
                        match value {
                            Value::String(s) => {
                                Some(format!("{} = '{}'", key, s.replace("'", "''")))
                            }
                            Value::Number(n) => Some(format!("{key} = {n}")),
                            Value::Bool(b) => Some(format!("{} = {}", key, if *b { 1 } else { 0 })),
                            Value::Null => Some(format!("{key} IS NULL")),
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
                let order_clauses: Vec<String> = orders
                    .iter()
                    .map(|o| format!("{} {}", o.field, o.direction.to_uppercase()))
                    .collect();
                query.push_str(" ORDER BY ");
                query.push_str(&order_clauses.join(", "));
            }
        }

        // Add LIMIT and OFFSET
        if let Some(lim) = limit {
            query.push_str(&format!(" LIMIT {lim}"));
        }
        if let Some(off) = offset {
            query.push_str(&format!(" OFFSET {off}"));
        }

        println!("📋 DataList query: {query}");
        self.registry.execute_read(handle, &query)
    }

    /// Create a new entity in a collection
    #[allow(dead_code)]
    fn data_create(
        &self,
        handle: AdapterHandle,
        collection: &str,
        data: &Value,
    ) -> Result<Value, String> {
        let obj = data
            .as_object()
            .ok_or_else(|| "Data must be an object".to_string())?;

        // Build INSERT query
        let columns: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
        let values: Vec<String> = obj
            .values()
            .map(|v| match v {
                Value::String(s) => format!("'{}'", s.replace("'", "''")),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => if *b { "1" } else { "0" }.to_string(),
                Value::Null => "NULL".to_string(),
                Value::Array(_) | Value::Object(_) => {
                    // Serialize complex types as JSON strings
                    format!(
                        "'{}'",
                        serde_json::to_string(v)
                            .unwrap_or_default()
                            .replace("'", "''")
                    )
                }
            })
            .collect();

        let query = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            collection,
            columns.join(", "),
            values.join(", ")
        );

        println!("➕ DataCreate query: {query}");
        self.registry.execute_write(handle, &query, data)
    }

    /// Delete an entity from a collection by ID
    #[allow(dead_code)]
    fn data_delete(
        &self,
        handle: AdapterHandle,
        collection: &str,
        id: &str,
    ) -> Result<Value, String> {
        let query = format!(
            "DELETE FROM {} WHERE id = '{}'",
            collection,
            id.replace("'", "''")
        );

        println!("🗑️  DataDelete query: {query}");
        self.registry.execute_write(handle, &query, &json!({}))
    }

    /// Update an entity in a collection by ID
    #[allow(dead_code)]
    fn data_update(
        &self,
        handle: AdapterHandle,
        collection: &str,
        id: &str,
        data: &Value,
    ) -> Result<Value, String> {
        let obj = data
            .as_object()
            .ok_or_else(|| "Data must be an object".to_string())?;

        // Build SET clauses
        let set_clauses: Vec<String> = obj
            .iter()
            .filter(|(key, _)| *key != "id") // Don't update id
            .map(|(key, value)| {
                let val_str = match value {
                    Value::String(s) => format!("'{}'", s.replace("'", "''")),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => if *b { "1" } else { "0" }.to_string(),
                    Value::Null => "NULL".to_string(),
                    Value::Array(_) | Value::Object(_) => {
                        // Serialize complex types as JSON strings
                        format!(
                            "'{}'",
                            serde_json::to_string(value)
                                .unwrap_or_default()
                                .replace("'", "''")
                        )
                    }
                };
                format!("{key} = {val_str}")
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

        println!("✏️  DataUpdate query: {query}");
        self.registry.execute_write(handle, &query, data)
    }

    /// Vector similarity search - delegates to adapter strategy
    /// Query vector comes over IPC (small: 3KB for 384 dims), corpus stays in Rust
    /// When include_data=true, returns full record data with scores (eliminates k IPC round trips)
    fn vector_search(
        &self,
        handle: AdapterHandle,
        collection: &str,
        query_vector: &[f64],
        k: Option<usize>,
        threshold: Option<f64>,
        include_data: Option<bool>,
    ) -> Result<Value, String> {
        let k = k.unwrap_or(10);
        let threshold = threshold.unwrap_or(0.0);
        let include_data = include_data.unwrap_or(true); // Default to include_data for optimization

        println!(
            "🔍 VectorSearch: collection={}, k={}, threshold={:.3}, query_dim={}, include_data={}",
            collection,
            k,
            threshold,
            query_vector.len(),
            include_data
        );

        self.registry
            .vector_search(handle, collection, query_vector, k, threshold, include_data)
    }

    // ========================================================================
    // Timed versions of data operations (captures query_build, lock_wait, execute)
    // ========================================================================

    fn data_list_timed(
        &self,
        timer: &mut RequestTimer,
        handle: AdapterHandle,
        collection: &str,
        limit: Option<usize>,
        offset: Option<usize>,
        filter: Option<&Value>,
        order_by: Option<&Vec<OrderBy>>,
    ) -> Result<Value, String> {
        // Query build phase
        let query_build_start = Instant::now();

        let mut query = format!("SELECT * FROM {collection}");

        if let Some(filter_obj) = filter {
            if let Some(obj) = filter_obj.as_object() {
                let conditions: Vec<String> = obj
                    .iter()
                    .filter_map(|(key, value)| match value {
                        Value::String(s) => Some(format!("{} = '{}'", key, s.replace("'", "''"))),
                        Value::Number(n) => Some(format!("{key} = {n}")),
                        Value::Bool(b) => Some(format!("{} = {}", key, if *b { 1 } else { 0 })),
                        Value::Null => Some(format!("{key} IS NULL")),
                        _ => None,
                    })
                    .collect();

                if !conditions.is_empty() {
                    query.push_str(" WHERE ");
                    query.push_str(&conditions.join(" AND "));
                }
            }
        }

        if let Some(orders) = order_by {
            if !orders.is_empty() {
                let order_clauses: Vec<String> = orders
                    .iter()
                    .map(|o| format!("{} {}", o.field, o.direction.to_uppercase()))
                    .collect();
                query.push_str(" ORDER BY ");
                query.push_str(&order_clauses.join(", "));
            }
        }

        if let Some(lim) = limit {
            query.push_str(&format!(" LIMIT {lim}"));
        }
        if let Some(off) = offset {
            query.push_str(&format!(" OFFSET {off}"));
        }

        timer.record.query_build_ns = query_build_start.elapsed().as_nanos() as u64;

        // Lock wait + execute phase (combined in registry.execute_read)
        let execute_start = Instant::now();
        let result = self.registry.execute_read(handle, &query);
        timer.record.execute_ns = execute_start.elapsed().as_nanos() as u64;

        result
    }

    fn data_create_timed(
        &self,
        timer: &mut RequestTimer,
        handle: AdapterHandle,
        collection: &str,
        data: &Value,
    ) -> Result<Value, String> {
        // Query build phase
        let query_build_start = Instant::now();

        let obj = data
            .as_object()
            .ok_or_else(|| "Data must be an object".to_string())?;

        let columns: Vec<&str> = obj.keys().map(|k| k.as_str()).collect();
        let values: Vec<String> = obj
            .values()
            .map(|v| match v {
                Value::String(s) => format!("'{}'", s.replace("'", "''")),
                Value::Number(n) => n.to_string(),
                Value::Bool(b) => if *b { "1" } else { "0" }.to_string(),
                Value::Null => "NULL".to_string(),
                Value::Array(_) | Value::Object(_) => {
                    format!(
                        "'{}'",
                        serde_json::to_string(v)
                            .unwrap_or_default()
                            .replace("'", "''")
                    )
                }
            })
            .collect();

        let query = format!(
            "INSERT INTO {} ({}) VALUES ({})",
            collection,
            columns.join(", "),
            values.join(", ")
        );

        timer.record.query_build_ns = query_build_start.elapsed().as_nanos() as u64;

        // Execute phase
        let execute_start = Instant::now();
        let result = self.registry.execute_write(handle, &query, data);
        timer.record.execute_ns = execute_start.elapsed().as_nanos() as u64;

        result
    }

    fn data_delete_timed(
        &self,
        timer: &mut RequestTimer,
        handle: AdapterHandle,
        collection: &str,
        id: &str,
    ) -> Result<Value, String> {
        // Query build phase
        let query_build_start = Instant::now();
        let query = format!(
            "DELETE FROM {} WHERE id = '{}'",
            collection,
            id.replace("'", "''")
        );
        timer.record.query_build_ns = query_build_start.elapsed().as_nanos() as u64;

        // Execute phase
        let execute_start = Instant::now();
        let result = self.registry.execute_write(handle, &query, &json!({}));
        timer.record.execute_ns = execute_start.elapsed().as_nanos() as u64;

        result
    }

    fn data_update_timed(
        &self,
        timer: &mut RequestTimer,
        handle: AdapterHandle,
        collection: &str,
        id: &str,
        data: &Value,
    ) -> Result<Value, String> {
        // Query build phase
        let query_build_start = Instant::now();

        let obj = data
            .as_object()
            .ok_or_else(|| "Data must be an object".to_string())?;

        let set_clauses: Vec<String> = obj
            .iter()
            .filter(|(key, _)| *key != "id")
            .map(|(key, value)| {
                let val_str = match value {
                    Value::String(s) => format!("'{}'", s.replace("'", "''")),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => if *b { "1" } else { "0" }.to_string(),
                    Value::Null => "NULL".to_string(),
                    Value::Array(_) | Value::Object(_) => {
                        format!(
                            "'{}'",
                            serde_json::to_string(value)
                                .unwrap_or_default()
                                .replace("'", "''")
                        )
                    }
                };
                format!("{key} = {val_str}")
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

        timer.record.query_build_ns = query_build_start.elapsed().as_nanos() as u64;

        // Execute phase
        let execute_start = Instant::now();
        let result = self.registry.execute_write(handle, &query, data);
        timer.record.execute_ns = execute_start.elapsed().as_nanos() as u64;

        result
    }

    // ========================================================================
    // Blob Storage Methods (Content-addressable file storage)
    // ========================================================================

    /// Get default blob base path relative to home directory
    fn get_blob_base_path(&self, custom_path: Option<&str>) -> PathBuf {
        if let Some(path) = custom_path {
            PathBuf::from(path)
        } else {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".continuum/blobs")
        }
    }

    /// Get blob file path from hash (sharded by first 2 chars)
    fn get_blob_path(&self, base: &Path, hash: &str) -> PathBuf {
        // Remove "sha256:" prefix if present
        let hex = hash.strip_prefix("sha256:").unwrap_or(hash);
        let shard = &hex[..2.min(hex.len())];
        let filename = &hex[2.min(hex.len())..];
        base.join(shard).join(format!("{filename}.blob"))
    }

    /// Store JSON data as compressed blob, return content hash
    fn blob_store(&self, data: &Value, base_path: Option<&str>) -> Result<Value, String> {
        use flate2::write::GzEncoder;
        use flate2::Compression;
        use sha2::{Digest, Sha256};
        use std::io::Write as IoWrite;

        let base = self.get_blob_base_path(base_path);

        // Serialize to JSON
        let json =
            serde_json::to_string(data).map_err(|e| format!("JSON serialize failed: {e}"))?;
        let original_size = json.len();

        // Compute SHA256 hash
        let mut hasher = Sha256::new();
        hasher.update(json.as_bytes());
        let hash_bytes = hasher.finalize();
        let hash = format!("sha256:{hash_bytes:x}");

        // Get file path
        let file_path = self.get_blob_path(&base, &hash);

        // Check if already exists (deduplication)
        if file_path.exists() {
            let metadata =
                fs::metadata(&file_path).map_err(|e| format!("Failed to stat blob: {e}"))?;
            return Ok(json!({
                "hash": hash,
                "size": original_size,
                "compressedSize": metadata.len(),
                "deduplicated": true,
                "storedAt": format!("{:?}", metadata.modified().ok())
            }));
        }

        // Ensure directory exists
        if let Some(parent) = file_path.parent() {
            fs::create_dir_all(parent).map_err(|e| format!("Failed to create blob dir: {e}"))?;
        }

        // Compress with gzip
        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(json.as_bytes())
            .map_err(|e| format!("Compression failed: {e}"))?;
        let compressed = encoder
            .finish()
            .map_err(|e| format!("Compression finish failed: {e}"))?;
        let compressed_size = compressed.len();

        // Write atomically (write to temp, then rename)
        let temp_path = file_path.with_extension("tmp");
        fs::write(&temp_path, &compressed)
            .map_err(|e| format!("Failed to write temp blob: {e}"))?;
        fs::rename(&temp_path, &file_path).map_err(|e| format!("Failed to rename blob: {e}"))?;

        Ok(json!({
            "hash": hash,
            "size": original_size,
            "compressedSize": compressed_size,
            "deduplicated": false,
            "storedAt": chrono::Utc::now().to_rfc3339()
        }))
    }

    /// Retrieve JSON data from blob by hash
    fn blob_retrieve(&self, hash: &str, base_path: Option<&str>) -> Result<Value, String> {
        use flate2::read::GzDecoder;
        use std::io::Read as IoRead;

        let base = self.get_blob_base_path(base_path);
        let file_path = self.get_blob_path(&base, hash);

        if !file_path.exists() {
            return Err(format!("Blob not found: {hash}"));
        }

        // Read compressed data
        let compressed = fs::read(&file_path).map_err(|e| format!("Failed to read blob: {e}"))?;

        // Decompress
        let mut decoder = GzDecoder::new(&compressed[..]);
        let mut json_str = String::new();
        decoder
            .read_to_string(&mut json_str)
            .map_err(|e| format!("Decompression failed: {e}"))?;

        // Parse JSON
        let data: Value =
            serde_json::from_str(&json_str).map_err(|e| format!("JSON parse failed: {e}"))?;

        Ok(data)
    }

    /// Check if blob exists
    fn blob_exists(&self, hash: &str, base_path: Option<&str>) -> Result<bool, String> {
        let base = self.get_blob_base_path(base_path);
        let file_path = self.get_blob_path(&base, hash);
        Ok(file_path.exists())
    }

    /// Delete blob by hash
    fn blob_delete(&self, hash: &str, base_path: Option<&str>) -> Result<bool, String> {
        let base = self.get_blob_base_path(base_path);
        let file_path = self.get_blob_path(&base, hash);

        if !file_path.exists() {
            return Ok(false);
        }

        fs::remove_file(&file_path).map_err(|e| format!("Failed to delete blob: {e}"))?;
        Ok(true)
    }

    /// Get blob storage statistics
    fn blob_stats(&self, base_path: Option<&str>) -> Result<Value, String> {
        let base = self.get_blob_base_path(base_path);

        if !base.exists() {
            return Ok(json!({
                "totalBlobs": 0,
                "totalCompressedBytes": 0,
                "shardCount": 0
            }));
        }

        let mut total_blobs = 0u64;
        let mut total_bytes = 0u64;
        let mut shard_count = 0u64;

        // Walk shard directories
        let entries = fs::read_dir(&base).map_err(|e| format!("Failed to read blob dir: {e}"))?;

        for entry in entries {
            let entry = entry.map_err(|e| format!("Dir entry error: {e}"))?;
            let path = entry.path();

            if path.is_dir() {
                shard_count += 1;

                let files =
                    fs::read_dir(&path).map_err(|e| format!("Failed to read shard dir: {e}"))?;

                for file in files {
                    let file = file.map_err(|e| format!("File entry error: {e}"))?;
                    let file_path = file.path();

                    if file_path.extension().is_some_and(|e| e == "blob") {
                        total_blobs += 1;
                        if let Ok(metadata) = fs::metadata(&file_path) {
                            total_bytes += metadata.len();
                        }
                    }
                }
            }
        }

        Ok(json!({
            "totalBlobs": total_blobs,
            "totalCompressedBytes": total_bytes,
            "shardCount": shard_count,
            "basePath": base.to_string_lossy()
        }))
    }

    // ========================================================================
    // Generic SQL Query (For complex queries with JOINs, etc.)
    // ========================================================================

    /// Execute a raw SQL SELECT query
    /// Returns raw results - caller handles any transformation
    /// Security: Only SELECT queries allowed (checked before execution)
    fn data_query(&self, handle: AdapterHandle, sql: &str) -> Result<Value, String> {
        // Security check: only allow SELECT queries
        let sql_upper = sql.trim().to_uppercase();
        if !sql_upper.starts_with("SELECT") {
            return Err("Only SELECT queries are allowed via data/query".to_string());
        }

        // Reject dangerous patterns
        if sql_upper.contains("DROP ")
            || sql_upper.contains("DELETE ")
            || sql_upper.contains("UPDATE ")
            || sql_upper.contains("INSERT ")
            || sql_upper.contains("ALTER ")
            || sql_upper.contains("CREATE ")
            || sql_upper.contains("; ")
        {
            return Err("Query contains disallowed SQL keywords".to_string());
        }

        println!("📊 DataQuery: {sql}");
        self.registry.execute_read(handle, sql)
    }
}

// ============================================================================
// Connection Handler (Same pattern as ArchiveWorker)
// ============================================================================

fn handle_connection(stream: UnixStream, daemon: Arc<RustDataDaemon>) -> std::io::Result<()> {
    let mut reader = BufReader::new(&stream);
    let mut writer = stream.try_clone()?;

    loop {
        // Start timing before socket read
        METRICS.request_start();
        let read_start = Instant::now();

        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            METRICS.request_end();
            break;
        }

        let socket_read_ns = read_start.elapsed().as_nanos() as u64;

        // Parse phase
        let parse_start = Instant::now();
        let request: Request = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                eprintln!("Parse error: {e}");
                METRICS.request_end();
                continue;
            }
        };
        let parse_ns = parse_start.elapsed().as_nanos() as u64;

        // Get request type for timing
        let request_type = match &request {
            Request::Ping => "ping",
            Request::AdapterOpen { .. } => "adapter/open",
            Request::AdapterClose { .. } => "adapter/close",
            Request::DataList { .. } => "data/list",
            Request::DataCreate { .. } => "data/create",
            Request::DataDelete { .. } => "data/delete",
            Request::DataUpdate { .. } => "data/update",
            Request::VectorSearch { .. } => "vector/search",
            Request::BlobStore { .. } => "blob/store",
            Request::BlobRetrieve { .. } => "blob/retrieve",
            Request::BlobExists { .. } => "blob/exists",
            Request::BlobDelete { .. } => "blob/delete",
            Request::BlobStats { .. } => "blob/stats",
            Request::DataQuery { .. } => "data/query",
        };

        // Start request timer
        let mut timer = RequestTimer::start(request_type);
        timer.record.socket_read_ns = socket_read_ns;
        timer.record.parse_ns = parse_ns;

        // Handle request (includes route, query_build, lock_wait, execute phases)
        let (response, result_count) = daemon.handle_request_timed(&mut timer, request);

        // Serialize phase
        let serialize_start = Instant::now();
        let response_json = serde_json::to_string(&response)?;
        timer.record.serialize_ns = serialize_start.elapsed().as_nanos() as u64;

        // Socket write phase
        let write_start = Instant::now();
        writeln!(writer, "{response_json}")?;
        writer.flush()?;
        timer.record.socket_write_ns = write_start.elapsed().as_nanos() as u64;

        // Set result metadata
        if let Some(count) = result_count {
            timer.set_result_count(count);
        }
        timer.set_concurrent(METRICS.get_active_count());

        // Record timing
        let record = timer.finish();
        METRICS.record(record);
        METRICS.request_end();
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
    println!("📡 Worker socket: {worker_socket}");
    println!("📊 Timing log: /tmp/jtag-data-daemon-timing.jsonl");

    let daemon = Arc::new(RustDataDaemon::new());
    println!("✅ RustDataDaemon ready (with precision timing)\n");

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
                        eprintln!("Connection error: {e}");
                    }
                });
            }
            Err(e) => eprintln!("Accept error: {e}"),
        }
    }

    Ok(())
}
