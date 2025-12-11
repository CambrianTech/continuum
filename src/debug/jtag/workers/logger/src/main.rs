/// Simple Logger Worker - Rust IPC Demo
///
/// This is a minimal proof-of-concept that:
/// 1. Listens on a Unix domain socket
/// 2. Receives JSON messages from TypeScript
/// 3. Writes log entries to files
/// 4. Sends JSON responses back to TypeScript
///
/// Run: cargo run -- /tmp/logger-worker.sock

mod messages;

use messages::*;
use std::collections::{HashMap, HashSet};
use std::fs::{self, File, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread;

// DEBUG LOGGING TO FILE
fn debug_log(msg: &str) {
    use std::io::Write;
    let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let log_msg = format!("[{}] {}\n", timestamp, msg);
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/rust-worker-debug.log")
    {
        let _ = file.write_all(log_msg.as_bytes());
        let _ = file.flush();
    }
}

// Global file handle cache (shared across connections)
type FileCache = Arc<Mutex<HashMap<String, File>>>;
// Track which categories have headers written (shared across connections)
type HeaderTracker = Arc<Mutex<HashSet<String>>>;

fn main() -> std::io::Result<()> {
    // Log startup
    debug_log("========================================");
    debug_log(&format!("RUST WORKER STARTING - PID: {}", std::process::id()));
    debug_log(&format!("Start time: {}", chrono::Utc::now().to_rfc3339()));
    debug_log("========================================");

    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        debug_log("ERROR: Missing socket path argument");
        eprintln!("Usage: {} <socket-path>", args[0]);
        eprintln!("Example: {} /tmp/logger-worker.sock", args[0]);
        std::process::exit(1);
    }

    let socket_path = &args[1];
    debug_log(&format!("Socket path: {}", socket_path));

    // Remove socket file if it exists
    if Path::new(socket_path).exists() {
        debug_log("Removing existing socket file");
        std::fs::remove_file(socket_path)?;
    }

    // Get log directory from environment or use default
    let log_dir = std::env::var("JTAG_LOG_DIR")
        .unwrap_or_else(|_| ".continuum/jtag/logs/system".to_string());
    debug_log(&format!("Log directory: {}", log_dir));

    println!("🦀 Rust Logger Worker starting...");
    println!("📡 Listening on: {}", socket_path);
    println!("📁 Log directory: {}", log_dir);

    // Create shared file cache and header tracker
    let file_cache: FileCache = Arc::new(Mutex::new(HashMap::new()));
    let headers_written: HeaderTracker = Arc::new(Mutex::new(HashSet::new()));

    debug_log("Binding to socket...");
    let listener = UnixListener::bind(socket_path)?;
    debug_log("Socket bound successfully");

    println!("✅ Ready to accept connections");
    debug_log("Entering accept loop (multi-threaded)");

    // Accept connections and spawn threads for concurrent handling
    let mut conn_count = 0;
    for stream in listener.incoming() {
        conn_count += 1;
        debug_log(&format!(">>> INCOMING CONNECTION #{}", conn_count));

        match stream {
            Ok(stream) => {
                println!("\n🔗 New connection from TypeScript (spawning thread)");
                debug_log(&format!("Connection #{} accepted, spawning thread", conn_count));

                // Clone for thread
                let log_dir_clone = log_dir.clone();
                let file_cache_clone = Arc::clone(&file_cache);
                let headers_clone = Arc::clone(&headers_written);
                let conn_id = conn_count;

                // Spawn thread to handle connection concurrently
                thread::spawn(move || {
                    debug_log(&format!("[Thread-{}] handle_client starting", conn_id));

                    if let Err(e) = handle_client(stream, &log_dir_clone, file_cache_clone, headers_clone) {
                        eprintln!("❌ Error handling client #{}: {}", conn_id, e);
                        debug_log(&format!("[Thread-{}] ERROR: {}", conn_id, e));
                    } else {
                        debug_log(&format!("[Thread-{}] COMPLETE", conn_id));
                    }

                    println!("✅ Connection #{} complete", conn_id);
                });

                debug_log(&format!("Thread spawned for connection #{}", conn_count));
            }
            Err(e) => {
                eprintln!("❌ Connection error: {}", e);
                debug_log(&format!("Connection #{} accept failed: {}", conn_count, e));
            }
        }
    }

    debug_log("Accept loop ended (should never happen)");
    Ok(())
}

fn handle_client(stream: UnixStream, log_dir: &str, file_cache: FileCache, headers_written: HeaderTracker) -> std::io::Result<()> {
    debug_log("handle_client: START");
    debug_log("Creating BufReader and cloning stream for writer");
    let mut reader = BufReader::new(&stream);
    let mut writer = stream.try_clone()?;
    debug_log("Reader/writer created successfully");

    // Read JSON messages line by line
    loop {
        debug_log("Loop iteration: Calling read_line()...");
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line)?;
        debug_log(&format!("read_line() returned {} bytes", bytes_read));

        if bytes_read == 0 {
            debug_log("bytes_read == 0, client disconnected (EOF)");
            println!("📪 Client disconnected (EOF)");
            break;
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        println!("📨 Received: {} bytes", line.len());

        // Parse request
        let request: Result<WorkerRequest<WriteLogPayload>, _> =
            serde_json::from_str(line);

        match request {
            Ok(req) => {
                println!("✅ Parsed request: type={}, id={}", req.r#type, req.id);

                // Process the log message
                let bytes_written = process_log_message(&req.payload, log_dir, &file_cache, &headers_written)?;

                // Build response
                let response = WorkerResponse::success(
                    req.id.clone(),
                    req.r#type.clone(),
                    WriteLogResult { bytes_written }
                );

                // Send response back to TypeScript
                let response_json = serde_json::to_string(&response)
                    .expect("Failed to serialize response");

                writeln!(writer, "{}", response_json)?;
                writer.flush()?;

                println!("✅ Sent response: {} bytes written", bytes_written);
            }
            Err(e) => {
                eprintln!("❌ Failed to parse request: {}", e);
                eprintln!("   Raw message: {}", line);

                // Try to extract request ID for error response
                if let Ok(base_msg) = serde_json::from_str::<serde_json::Value>(line) {
                    if let Some(id) = base_msg.get("id").and_then(|v| v.as_str()) {
                        let error_response = WorkerResponse::<WriteLogResult>::error(
                            id.to_string(),
                            "write-log".to_string(),
                            WriteLogResult { bytes_written: 0 },
                            format!("Parse error: {}", e),
                            ErrorType::Validation
                        );

                        let error_json = serde_json::to_string(&error_response)
                            .expect("Failed to serialize error response");

                        writeln!(writer, "{}", error_json)?;
                        writer.flush()?;
                    }
                }
            }
        }
    }

    Ok(())
}

fn process_log_message(
    payload: &WriteLogPayload,
    log_dir: &str,
    file_cache: &FileCache,
    headers_written: &HeaderTracker
) -> std::io::Result<usize> {
    // Build log file path from category
    let log_file_path = PathBuf::from(log_dir).join(format!("{}.log", payload.category));

    // Check if we need to write header for this category
    let mut headers = headers_written.lock().unwrap();
    let needs_header = !headers.contains(&payload.category);

    // Format timestamp
    let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    // Get or create file handle from cache
    let mut cache = file_cache.lock().unwrap();
    let file = cache.entry(payload.category.clone()).or_insert_with(|| {
        // Ensure directory exists
        if let Some(parent) = log_file_path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        // Open file in append mode, create if doesn't exist
        OpenOptions::new()
            .create(true)
            .write(true)
            .append(true)
            .open(&log_file_path)
            .expect(&format!("Failed to open log file: {:?}", log_file_path))
    });

    let mut total_bytes = 0;

    // Write header if this is the first log for this category
    if needs_header {
        let header = generate_header(&payload.component, &payload.category, &timestamp);
        file.write_all(header.as_bytes())?;
        file.flush()?;
        total_bytes += header.len();
        headers.insert(payload.category.clone());
        println!("📋 Wrote header for: {} ({})", payload.component, payload.category);
    }

    // Drop locks before writing log entry (avoid deadlock)
    drop(headers);
    drop(cache);

    // Format log entry (with [RUST] marker)
    let log_entry = format!(
        "[RUST] [{}] [{}] {}: {}",
        timestamp,
        payload.level.to_string().to_uppercase(),
        payload.component,
        payload.message
    );

    // Append args if present
    let full_log_entry = if let Some(args) = &payload.args {
        format!("{} {}\n", log_entry, args)
    } else {
        format!("{}\n", log_entry)
    };

    // Re-acquire cache lock to write log entry
    let mut cache = file_cache.lock().unwrap();
    let file = cache.get_mut(&payload.category).unwrap();
    file.write_all(full_log_entry.as_bytes())?;
    file.flush()?;
    total_bytes += full_log_entry.len();

    println!("📝 LOG: {} → {:?}", log_entry.trim(), log_file_path);

    Ok(total_bytes)
}

fn generate_header(component: &str, category: &str, timestamp: &str) -> String {
    format!(
        "================================================================================\n\
         COMPONENT: {}\n\
         CATEGORY: {}\n\
         SESSION: session-{}\n\
         STARTED: {}\n\
         PID: {}\n\
         ================================================================================\n\
         \n\
         LOG FORMAT:\n\
           [RUST] [timestamp] [LEVEL] Component: message [args]\n\
         \n\
         LOG LEVELS:\n\
           DEBUG - Detailed diagnostic information\n\
           INFO  - General informational messages\n\
           WARN  - Warning messages\n\
           ERROR - Error messages\n\
         \n\
         LOG ENTRIES BEGIN BELOW:\n\
         ================================================================================\n\
         \n",
        component,
        category,
        chrono::Utc::now().timestamp_millis(),
        timestamp,
        std::process::id()
    )
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Debug => write!(f, "debug"),
            LogLevel::Info => write!(f, "info"),
            LogLevel::Warn => write!(f, "warn"),
            LogLevel::Error => write!(f, "error"),
        }
    }
}
