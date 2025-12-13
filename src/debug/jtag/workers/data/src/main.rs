/// Data Worker - Main Entry Point
///
/// This is the main entry point for the Data Worker, which handles all
/// database operations off the Node.js main thread using the Universal
/// Worker Protocol.
///
/// Architecture:
/// 1. Bind Unix socket (for IPC with Node.js)
/// 2. Create SQLite connection pool (10 connections)
/// 3. Spawn dedicated processor thread (drains queue, executes queries)
/// 4. Accept connections and spawn handler threads (concurrent clients)
/// 5. Handle shutdown gracefully (drain queue, close connections)
///
/// Usage:
///   cargo run --release -- /tmp/data-worker.sock

mod connection_handler;
mod database;
mod health;
mod messages;
mod processor;

use std::env;
use std::fs;
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

// ============================================================================
// Shared State Types
// ============================================================================

/// Shutdown signal (shared across threads via Arc)
pub type ShutdownSignal = Arc<AtomicBool>;

fn main() -> std::io::Result<()> {
    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <socket_path>", args[0]);
        eprintln!("Example: {} /tmp/data-worker.sock", args[0]);
        std::process::exit(1);
    }

    let socket_path = &args[1];
    let db_path = get_database_path();

    println!("🚀 Data Worker Starting...");
    println!("   Socket: {}", socket_path);
    println!("   Database: {}", db_path);

    // Remove socket file if it exists
    if Path::new(socket_path).exists() {
        fs::remove_file(socket_path)?;
    }

    // Create shared state
    let stats = health::create_stats();
    let shutdown_signal: ShutdownSignal = Arc::new(AtomicBool::new(false));

    // Create database connection pool
    println!("📊 Creating database connection pool (10 connections)...");
    let db_pool = database::create_pool(&db_path).expect("Failed to create database pool");
    println!("✅ Database pool ready");

    // Bind Unix socket
    println!("🔌 Binding Unix socket...");
    let listener = UnixListener::bind(socket_path)?;
    println!("✅ Socket bound successfully");

    // Create data operation queue channel (unbounded for max throughput)
    let (data_tx, data_rx) = mpsc::channel::<processor::QueuedDataOp>();

    // Spawn dedicated processor thread (drains queue and processes operations)
    let processor_pool = db_pool.clone();
    let processor_stats = stats.clone();
    let processor_shutdown = shutdown_signal.clone();
    let processor_thread = thread::spawn(move || {
        processor::process_data_queue(
            data_rx,
            processor_pool,
            processor_stats,
            processor_shutdown,
        );
    });
    println!("✅ Processor thread spawned");

    println!("🎧 Listening for connections on {}...", socket_path);
    println!("📡 Ready to process data operations");
    println!();

    // Accept connections and spawn threads for concurrent handling
    for stream in listener.incoming() {
        // Check shutdown signal
        if shutdown_signal.load(Ordering::Relaxed) {
            println!("🛑 Shutdown signal received, stopping new connections");
            break;
        }

        match stream {
            Ok(stream) => {
                // Record connection
                {
                    let mut s = stats.lock().unwrap();
                    s.record_connection();
                }

                let data_tx_clone = data_tx.clone();
                let stats_clone = stats.clone();
                let shutdown_clone = shutdown_signal.clone();

                // Spawn thread to handle this connection
                thread::spawn(move || {
                    if let Err(e) = connection_handler::handle_client(
                        stream,
                        data_tx_clone,
                        stats_clone,
                        shutdown_clone,
                    ) {
                        eprintln!("❌ Connection handler error: {}", e);
                    }
                });
            }
            Err(e) => {
                eprintln!("❌ Failed to accept connection: {}", e);
            }
        }
    }

    // Wait for processor thread to finish (drains queue)
    println!("⏳ Waiting for processor thread to drain queue...");
    drop(data_tx); // Close channel to signal processor to exit
    processor_thread
        .join()
        .expect("Failed to join processor thread");

    println!("✅ Data Worker shut down cleanly");
    Ok(())
}

// ============================================================================
// Database Path Discovery
// ============================================================================

/// Get database path from environment or use default
fn get_database_path() -> String {
    // Try environment variable first
    if let Ok(db_path) = env::var("CONTINUUM_DB_PATH") {
        return db_path;
    }

    // Try to find database in .continuum directory structure
    let home_dir = env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let continuum_dir = format!("{}/.continuum/sessions", home_dir);

    // Look for continuum.db in session directories
    if let Ok(entries) = fs::read_dir(&continuum_dir) {
        for entry in entries.flatten() {
            let session_path = entry.path();
            if session_path.is_dir() {
                // Check for shared/databases/continuum.db
                let db_path = session_path.join("shared/databases/continuum.db");
                if db_path.exists() {
                    return db_path.to_string_lossy().to_string();
                }
            }
        }
    }

    // Fallback to working directory
    "./continuum.db".to_string()
}
