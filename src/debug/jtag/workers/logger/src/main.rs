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

use std::os::unix::net::UnixListener;
use std::path::Path;
use std::sync::mpsc;
use std::thread;

// ============================================================================
// Main Entry Point
// ============================================================================

fn main() -> std::io::Result<()> {
    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <socket-path>", args[0]);
        eprintln!("Example: {} /tmp/logger-worker.sock", args[0]);
        std::process::exit(1);
    }

    let socket_path = &args[1];

    // Get log directory from environment or use default
    let log_dir =
        std::env::var("JTAG_LOG_DIR").unwrap_or_else(|_| ".continuum/jtag/logs/system".to_string());

    // Remove socket file if it exists
    if Path::new(socket_path).exists() {
        std::fs::remove_file(socket_path)?;
    }

    println!("🦀 Logger Worker starting on {socket_path}");

    // Create shared state (file cache, headers, stats)
    let file_cache = file_manager::create_file_cache();
    let headers_written = file_manager::create_header_tracker();
    let stats = health::create_stats();

    // Bind socket
    let listener = UnixListener::bind(socket_path)?;

    // Create log queue channel (unbounded for max throughput)
    let (log_tx, log_rx) = mpsc::channel::<messages::WriteLogPayload>();

    // Spawn dedicated writer thread (drains queue and writes to files)
    let writer_file_cache = file_cache.clone();
    let writer_headers = headers_written.clone();
    let writer_log_dir = log_dir.clone();
    thread::spawn(move || {
        for payload in log_rx.iter() {
            if let Err(e) = file_manager::write_log_message(
                &payload,
                &writer_log_dir,
                &writer_file_cache,
                &writer_headers,
            ) {
                eprintln!("❌ Logger write error: {e}");
            }
        }
    });

    println!("✅ Logger ready");

    // Accept connections and spawn threads for concurrent handling
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
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

                // Spawn thread to handle connection concurrently
                thread::spawn(move || {
                    if let Err(e) = connection_handler::handle_client(
                        stream,
                        &log_dir_clone,
                        file_cache_clone,
                        headers_clone,
                        stats_clone,
                        log_tx_clone,
                    ) {
                        eprintln!("❌ Logger client error: {e}");
                    }
                });
            }
            Err(e) => {
                eprintln!("❌ Logger connection error: {e}");
            }
        }
    }

    Ok(())
}
