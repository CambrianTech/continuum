/// SQL Processor Module - Background Processing
///
/// This module runs in a dedicated background thread and processes
/// SQL operations asynchronously. All database operations happen here:
/// - sql/query (SELECT queries - returns rows)
/// - sql/execute (INSERT/UPDATE/DELETE - returns changes)
///
/// The main thread queues operations here and returns immediately,
/// freeing the main thread from blocking database I/O.
use crate::database::{self, DbPool};
use crate::health::StatsHandle;
use crate::messages::*;
use crate::ShutdownSignal;
use std::fs::OpenOptions;
use std::io::Write;
use std::sync::atomic::Ordering;
use std::sync::mpsc;

// ============================================================================
// Queued Data Operation (internal type)
// ============================================================================

/// SQL operation queued for processing
pub enum QueuedDataOp {
    Query {
        request_id: String,
        payload: SqlQueryPayload,
        response_tx: mpsc::Sender<(String, Result<SqlQueryResult, String>)>,
    },
    Execute {
        request_id: String,
        payload: SqlExecutePayload,
        response_tx: mpsc::Sender<(String, Result<SqlExecuteResult, String>)>,
    },
}

/// Debug logging to file (temporary)
fn debug_log(msg: &str) {
    let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let log_msg = format!("[{timestamp}] {msg}\n");
    if let Ok(mut file) = OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/data-worker-debug.log")
    {
        let _ = file.write_all(log_msg.as_bytes());
        let _ = file.flush();
    }
}

// ============================================================================
// Main Processing Loop
// ============================================================================

/// Main data processing loop - runs in background thread
///
/// This function drains the data operation queue and processes each operation:
/// 1. Acquire connection from pool
/// 2. Execute query (list/read/create/update)
/// 3. Send response back to connection handler
/// 4. Update stats (processed/errors)
pub fn process_data_queue(
    rx: mpsc::Receiver<QueuedDataOp>,
    db_pool: DbPool,
    stats: StatsHandle,
    shutdown_signal: ShutdownSignal,
) {
    debug_log("[Processor] Data processor thread started");
    let mut processed = 0;

    for queued_op in rx.iter() {
        // Check shutdown signal
        if shutdown_signal.load(Ordering::Relaxed) {
            debug_log("[Processor] Shutdown signal detected, draining remaining queue");
            // Continue processing to drain queue
        }

        processed += 1;

        // Update queue depth (decrements as we process)
        {
            let mut s = stats.lock().unwrap();
            // TODO: Track actual queue size
            s.set_queue_depth(0);
        }

        // Process the operation
        let result = match queued_op {
            QueuedDataOp::Query {
                request_id,
                payload,
                response_tx,
            } => {
                debug_log(&format!(
                    "[Processor] Processing SQL query: {}",
                    payload.sql
                ));
                let result =
                    database::with_retry(|| database::execute_query(&db_pool, payload.clone()));
                let _ = response_tx.send((request_id, result.clone()));
                result.map(|_| ())
            }
            QueuedDataOp::Execute {
                request_id,
                payload,
                response_tx,
            } => {
                debug_log(&format!(
                    "[Processor] Processing SQL statement: {}",
                    payload.sql
                ));
                let result =
                    database::with_retry(|| database::execute_statement(&db_pool, payload.clone()));
                let _ = response_tx.send((request_id, result.clone()));
                result.map(|_| ())
            }
        };

        // Record success/error in stats
        match result {
            Ok(_) => {
                // Success already recorded when queued
            }
            Err(e) => {
                eprintln!("❌ Processor error: {e}");
                debug_log(&format!("[Processor] Error: {e}"));

                let mut s = stats.lock().unwrap();
                s.record_error();
            }
        }

        // Log throughput every 100 operations
        if processed % 100 == 0 {
            debug_log(&format!(
                "[Processor] Processed {processed} data operations"
            ));
        }
    }

    debug_log(&format!(
        "[Processor] Queue drained, processed {processed} total operations"
    ));
}
