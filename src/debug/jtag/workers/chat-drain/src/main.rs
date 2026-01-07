/// Chat Drain Worker - Production Rust IPC Service
///
/// This worker provides high-performance chat message processing for the JTAG system.
/// It handles:
/// - Multi-threaded concurrent connections
/// - Chat message queuing for async processing
/// - RAG context building
/// - AI API call orchestration
/// - Health monitoring and graceful shutdown
///
/// Architecture:
/// - main.rs: Orchestration and connection acceptance
/// - connection_handler: Message parsing and routing
/// - processor: Chat-specific processing (RAG, AI calls, tools)
/// - health: Universal protocol implementation (ping, status, shutdown)
/// - messages: Protocol types (shared with TypeScript)
///
/// Usage: cargo run --release -- /tmp/chat-drain-worker.sock
mod connection_handler;
mod health;
mod messages;
mod processor;

use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::net::UnixListener;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

// ============================================================================
// Queue Message Type
// ============================================================================

use messages::ChatMessagePayload;

/// Message sent through the chat processing queue
#[derive(Clone)]
pub struct QueuedChat {
    pub payload: ChatMessagePayload,
}

// ============================================================================
// Shared State
// ============================================================================

/// Shared shutdown signal across all threads
pub type ShutdownSignal = Arc<AtomicBool>;

// ============================================================================
// Debug Logging (Temporary)
// ============================================================================

fn debug_log(msg: &str) {
    let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let log_msg = format!("[{timestamp}] {msg}\n");
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/chat-drain-worker-debug.log")
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
        "CHAT DRAIN WORKER STARTING - PID: {}",
        std::process::id()
    ));
    debug_log(&format!("Start time: {}", chrono::Utc::now().to_rfc3339()));
    debug_log("========================================");

    // Parse command line arguments
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        debug_log("ERROR: Missing socket path argument");
        eprintln!("Usage: {} <socket-path>", args[0]);
        eprintln!("Example: {} /tmp/chat-drain-worker.sock", args[0]);
        std::process::exit(1);
    }

    let socket_path = &args[1];
    debug_log(&format!("Socket path: {socket_path}"));

    // Remove socket file if it exists
    if Path::new(socket_path).exists() {
        debug_log("Removing existing socket file");
        std::fs::remove_file(socket_path)?;
    }

    println!("🦀 Rust Chat Drain Worker starting...");
    println!("📡 Listening on: {socket_path}");

    // Create shared state
    let stats = health::create_stats();
    let shutdown_signal: ShutdownSignal = Arc::new(AtomicBool::new(false));

    // Bind socket
    debug_log("Binding to socket...");
    let listener = UnixListener::bind(socket_path)?;
    debug_log("Socket bound successfully");

    println!("✅ Ready to accept connections");
    debug_log("Entering accept loop (multi-threaded)");

    // Create chat message queue channel (unbounded for max throughput)
    let (chat_tx, chat_rx) = mpsc::channel::<QueuedChat>();
    debug_log("Created chat queue channel");

    // Spawn dedicated processor thread (drains queue and processes chat)
    let processor_stats = stats.clone();
    let processor_shutdown = shutdown_signal.clone();
    thread::spawn(move || {
        debug_log("[Processor Thread] Started - draining chat queue");
        println!("🔥 Background chat processor thread started");

        processor::process_chat_queue(chat_rx, processor_stats, processor_shutdown);

        debug_log("[Processor Thread] Shutdown complete");
    });

    println!("⚡ Queue-based architecture active (non-blocking chat processing)");

    // Accept connections and spawn threads for concurrent handling
    let mut conn_count = 0;
    for stream in listener.incoming() {
        // Check shutdown signal
        if shutdown_signal.load(Ordering::Relaxed) {
            debug_log("Shutdown signal received, stopping accept loop");
            break;
        }

        conn_count += 1;
        debug_log(&format!(">>> INCOMING CONNECTION #{conn_count}"));

        match stream {
            Ok(stream) => {
                println!("\\n🔗 New connection from TypeScript (spawning thread)");
                debug_log(&format!(
                    "Connection #{conn_count} accepted, spawning thread"
                ));

                // Increment connection counter
                {
                    let mut s = stats.lock().unwrap();
                    s.record_connection();
                }

                // Clone shared state for thread
                let chat_tx_clone = chat_tx.clone();
                let stats_clone = stats.clone();
                let shutdown_clone = shutdown_signal.clone();
                let conn_id = conn_count;

                // Spawn thread to handle connection concurrently
                thread::spawn(move || {
                    debug_log(&format!("[Thread-{conn_id}] Starting connection handler"));

                    if let Err(e) = connection_handler::handle_client(
                        stream,
                        chat_tx_clone,
                        stats_clone,
                        shutdown_clone,
                    ) {
                        eprintln!("❌ Error handling client #{conn_id}: {e}");
                        debug_log(&format!("[Thread-{conn_id}] ERROR: {e}"));
                    } else {
                        debug_log(&format!("[Thread-{conn_id}] COMPLETE"));
                    }

                    println!("✅ Connection #{conn_id} complete");
                });

                debug_log(&format!("Thread spawned for connection #{conn_count}"));
            }
            Err(e) => {
                eprintln!("❌ Connection error: {e}");
                debug_log(&format!("Connection #{conn_count} accept failed: {e}"));
            }
        }
    }

    debug_log("Accept loop ended - beginning graceful shutdown");
    println!("🛑 Shutting down gracefully...");

    // Drop the sender to close the channel
    drop(chat_tx);

    // Wait a bit for processor thread to drain queue
    std::thread::sleep(std::time::Duration::from_secs(2));

    debug_log("Shutdown complete");
    println!("✅ Chat Drain Worker shut down cleanly");
    Ok(())
}
