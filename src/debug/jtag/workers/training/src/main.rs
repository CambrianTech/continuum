/// Training Worker - Production Rust IPC Service
///
/// This worker provides high-performance training data processing for the JTAG system.
/// It handles:
/// - Training data export to JSONL (for fine-tuning)
/// - Multi-threaded concurrent connections
/// - Health monitoring via ping messages
///
/// Architecture:
/// - main.rs: Orchestration and connection acceptance
/// - connection_handler: Message parsing and routing
/// - export: JSONL export operations
/// - health: Statistics tracking
/// - messages: Protocol types (shared with TypeScript)
///
/// Usage: cargo run --release -- /tmp/jtag-training-worker.sock
mod connection_handler;
mod export;
mod health;
mod messages;

// Import shared LoggerClient for Rust-to-Rust logging
#[path = "../../shared/logger_client.rs"]
mod logger_client;
use logger_client::LoggerClient;

use std::os::unix::net::UnixListener;
use std::path::Path;
use std::thread;

// ============================================================================
// Main Entry Point
// ============================================================================

fn main() -> std::io::Result<()> {
    // Initialize logger (connect to LoggerWorker)
    let mut logger = LoggerClient::connect(
        "/tmp/jtag-logger-worker.sock",
        "TrainingWorker"
    ).with_category("rust-workers/training".to_string());

    // Log startup
    logger.info("========================================");
    logger.info(&format!("Training Worker starting - PID: {}", std::process::id()));
    logger.info(&format!("Start time: {}", chrono::Utc::now().to_rfc3339()));
    logger.info("========================================");

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        logger.error("Missing socket path argument");
        eprintln!("Usage: {} <socket-path>", args[0]);
        eprintln!("Example: {} /tmp/jtag-training-worker.sock", args[0]);
        std::process::exit(1);
    }

    let socket_path = &args[1];
    logger.info(&format!("Socket path: {}", socket_path));

    // Remove socket file if it exists
    if Path::new(socket_path).exists() {
        logger.info("Removing existing socket file");
        std::fs::remove_file(socket_path)?;
    }

    println!("🦀 Rust Training Worker starting...");
    println!("📡 Listening on: {}", socket_path);

    // Create shared state (stats)
    let stats = health::create_stats();

    // Bind socket
    logger.info("Binding to socket...");
    let listener = UnixListener::bind(socket_path)?;
    logger.info("Socket bound successfully");

    println!("✅ Ready to accept connections");
    logger.info("Entering accept loop (multi-threaded)");

    // Accept connections and spawn threads for concurrent handling
    let mut conn_count = 0;
    for stream in listener.incoming() {
        conn_count += 1;
        logger.info(&format!("Incoming connection #{}", conn_count));

        match stream {
            Ok(stream) => {
                println!("\n🔗 New connection from TypeScript (spawning thread)");
                logger.info(&format!("Connection #{} accepted, spawning thread", conn_count));

                // Increment connection counter
                {
                    let mut s = stats.lock().unwrap();
                    s.record_connection();
                }

                // Clone shared state for thread
                let stats_clone = stats.clone();
                let conn_id = conn_count;

                // Spawn thread to handle connection concurrently
                thread::spawn(move || {
                    // Note: Spawned threads don't have access to logger
                    // They use connection_handler's internal logging
                    if let Err(e) = connection_handler::handle_client(stream, stats_clone) {
                        eprintln!("❌ Error handling client #{}: {}", conn_id, e);
                    }
                    println!("✅ Connection #{} complete", conn_id);
                });

                logger.info(&format!("Thread spawned for connection #{}", conn_count));
            }
            Err(e) => {
                logger.error(&format!("Connection #{} accept failed: {}", conn_count, e));
                eprintln!("❌ Connection error: {}", e);
            }
        }
    }

    logger.warn("Accept loop ended (should never happen)");
    Ok(())
}
