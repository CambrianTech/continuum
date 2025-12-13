/// Logger Worker - Production Rust IPC Service
///
/// This worker provides high-performance log file management for the JTAG system.
/// It handles:
/// - Multi-threaded concurrent connections
/// - File handle caching for performance
/// - Auto-recovery if log files deleted
/// - Health monitoring via ping messages
///
/// Architecture:
/// - main.rs: Orchestration and connection acceptance
/// - connection_handler: Message parsing and routing
/// - file_manager: File operations and caching
/// - health: Statistics tracking
/// - messages: Protocol types (shared with TypeScript)
///
/// Usage: cargo run --release -- /tmp/logger-worker.sock
mod connection_handler;
mod file_manager;
mod health;
mod messages;

use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::sync::mpsc;
use std::thread;

// ============================================================================
// Queue Message Type
// ============================================================================

// ============================================================================
// Debug Logging (Temporary)
// ============================================================================

fn debug_log(msg: &str) {
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

// ============================================================================
// Main Entry Point
// ============================================================================

fn main() -> std::io::Result<()> {
    // Log startup
    debug_log("========================================");
    debug_log(&format!(
        "RUST WORKER STARTING - PID: {}",
        std::process::id()
    ));
    debug_log(&format!("Start time: {}", chrono::Utc::now().to_rfc3339()));
    debug_log("========================================");

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        debug_log("ERROR: Missing socket path argument");
        eprintln!("Usage: {} <socket-path>", args[0]);
        eprintln!("Example: {} /tmp/logger-worker.sock", args[0]);
        std::process::exit(1);
    }

    let socket_path = &args[1];
    debug_log(&format!("Socket path: {}", socket_path));

    // Get log directory from environment or use default
    let log_dir =
        std::env::var("JTAG_LOG_DIR").unwrap_or_else(|_| ".continuum/jtag/logs/system".to_string());
    debug_log(&format!("Log directory: {}", log_dir));

    // Remove socket file if it exists
    if Path::new(socket_path).exists() {
        debug_log("Removing existing socket file");
        std::fs::remove_file(socket_path)?;
    }

    println!("🦀 Rust Logger Worker starting...");
    println!("📡 Listening on: {}", socket_path);
    println!("📁 Log directory: {}", log_dir);

    // Create shared state (file cache, headers, stats)
    let file_cache = file_manager::create_file_cache();
    let headers_written = file_manager::create_header_tracker();
    let stats = health::create_stats();

    // Bind socket
    debug_log("Binding to socket...");
    let listener = UnixListener::bind(socket_path)?;
    debug_log("Socket bound successfully");

    println!("✅ Ready to accept connections");
    debug_log("Entering accept loop (multi-threaded)");

    // Create log queue channel (unbounded for max throughput)
    let (log_tx, log_rx) = mpsc::channel::<messages::WriteLogPayload>();
    debug_log("Created log queue channel");

    // Spawn dedicated writer thread (drains queue and writes to files)
    let writer_file_cache = file_cache.clone();
    let writer_headers = headers_written.clone();
    let writer_log_dir = log_dir.clone();
    thread::spawn(move || {
        debug_log("[Writer Thread] Started - draining log queue");
        println!("🔥 Background log writer thread started");

        let mut processed = 0;
        for payload in log_rx.iter() {
            processed += 1;
            if let Err(e) = file_manager::write_log_message(
                &payload,
                &writer_log_dir,
                &writer_file_cache,
                &writer_headers,
            ) {
                eprintln!("❌ Writer thread error: {}", e);
                debug_log(&format!("[Writer Thread] Error writing log: {}", e));
            }

            // Log throughput every 100 messages
            if processed % 100 == 0 {
                debug_log(&format!("[Writer Thread] Processed {} logs", processed));
            }
        }

        debug_log("[Writer Thread] Channel closed, exiting");
    });

    println!("⚡ Queue-based architecture active (non-blocking log writes)");

    // Accept connections and spawn threads for concurrent handling
    let mut conn_count = 0;
    for stream in listener.incoming() {
        conn_count += 1;
        debug_log(&format!(">>> INCOMING CONNECTION #{}", conn_count));

        match stream {
            Ok(stream) => {
                println!("\n🔗 New connection from TypeScript (spawning thread)");
                debug_log(&format!(
                    "Connection #{} accepted, spawning thread",
                    conn_count
                ));

                // Increment connection counter
                {
                    let mut s = stats.lock().unwrap();
                    s.record_connection();
                }

                // Clone shared state for thread
                let log_dir_clone = log_dir.clone();
                let file_cache_clone = file_cache.clone();
                let headers_clone = headers_written.clone();
                let stats_clone = stats.clone();
                let log_tx_clone = log_tx.clone();
                let conn_id = conn_count;

                // Spawn thread to handle connection concurrently
                thread::spawn(move || {
                    debug_log(&format!("[Thread-{}] Starting connection handler", conn_id));

                    if let Err(e) = connection_handler::handle_client(
                        stream,
                        &log_dir_clone,
                        file_cache_clone,
                        headers_clone,
                        stats_clone,
                        log_tx_clone,
                    ) {
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
