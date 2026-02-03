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
mod rate_limiter;

use std::os::unix::net::UnixListener;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

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

    // Spawn dedicated writer thread with BATCHED flushing + rate limiting
    //
    // Instead of flushing to disk after every message (which was causing 55%+ of
    // main-thread time in IPC latency), we now:
    //   1. Rate-limit per category (100 msg/sec default — drops excess, logs warning)
    //   2. Write messages to OS buffers (fast, no disk I/O)
    //   3. Drain the channel in batches (non-blocking try_recv after first message)
    //   4. Flush all dirty files every 250ms OR after 200 messages (whichever first)
    //
    // This reduces disk flushes from ~700/sec (peak) to ~4/sec
    // and prevents any single category from flooding disk I/O.
    let writer_file_cache = file_cache.clone();
    let writer_headers = headers_written.clone();
    let writer_log_dir = log_dir.clone();
    thread::spawn(move || {
        const FLUSH_INTERVAL: Duration = Duration::from_millis(250);
        const MAX_BATCH_BEFORE_FLUSH: usize = 200;

        let mut pending_writes: usize = 0;

        // Rate limiter: 100 messages/sec per category (prevents spam flooding)
        let mut limiter = rate_limiter::RateLimiter::new(100);

        // Process a single payload with rate limiting
        let process_payload = |payload: &messages::WriteLogPayload,
                                    limiter: &mut rate_limiter::RateLimiter,
                                    pending: &mut usize| {
            match limiter.check(&payload.category) {
                rate_limiter::RateDecision::Allow => {
                    if let Err(e) = file_manager::write_log_message(
                        payload,
                        &writer_log_dir,
                        &writer_file_cache,
                        &writer_headers,
                    ) {
                        eprintln!("❌ Logger write error: {e}");
                    }
                    *pending += 1;
                }
                rate_limiter::RateDecision::Drop => {
                    // Silently dropped — warning logged when burst ends
                }
                rate_limiter::RateDecision::BurstEnded(dropped) => {
                    // Log that we dropped messages from previous burst
                    let warning = messages::WriteLogPayload {
                        category: payload.category.clone(),
                        level: messages::LogLevel::Warn,
                        component: "RateLimiter".to_string(),
                        message: format!(
                            "Rate limit: dropped {} messages from '{}' (>100/sec)",
                            dropped, payload.category
                        ),
                        args: None,
                    };
                    let _ = file_manager::write_log_message(
                        &warning,
                        &writer_log_dir,
                        &writer_file_cache,
                        &writer_headers,
                    );
                    // Also write the current message
                    if let Err(e) = file_manager::write_log_message(
                        payload,
                        &writer_log_dir,
                        &writer_file_cache,
                        &writer_headers,
                    ) {
                        eprintln!("❌ Logger write error: {e}");
                    }
                    *pending += 2;
                }
            }
        };

        // Simple loop: block up to FLUSH_INTERVAL, process batch, flush.
        // CRITICAL: Always use FLUSH_INTERVAL as timeout to avoid busy-spin.
        // (Previous version used Duration::ZERO which caused 100% CPU)
        loop {
            match log_rx.recv_timeout(FLUSH_INTERVAL) {
                Ok(payload) => {
                    process_payload(&payload, &mut limiter, &mut pending_writes);

                    // Drain remaining messages non-blocking (batch)
                    while pending_writes < MAX_BATCH_BEFORE_FLUSH {
                        match log_rx.try_recv() {
                            Ok(payload) => {
                                process_payload(&payload, &mut limiter, &mut pending_writes);
                            }
                            Err(_) => break,
                        }
                    }

                    // Flush if batch limit reached
                    if pending_writes >= MAX_BATCH_BEFORE_FLUSH {
                        file_manager::flush_all(&writer_file_cache);
                        pending_writes = 0;
                    }
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    // Periodic flush — fires every FLUSH_INTERVAL when idle
                    if pending_writes > 0 {
                        file_manager::flush_all(&writer_file_cache);
                        pending_writes = 0;
                    }
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    if pending_writes > 0 {
                        file_manager::flush_all(&writer_file_cache);
                    }
                    break;
                }
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
