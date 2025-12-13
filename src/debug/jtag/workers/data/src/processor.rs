/// Data Processor Module - Background Processing
///
/// This module runs in a dedicated background thread and processes
/// data operations asynchronously. All database operations happen here:
/// - data/list (query with filters/ordering)
/// - data/read (single document by ID)
/// - data/create (insert new document)
/// - data/update (modify existing document)
///
/// The main thread queues operations here and returns immediately,
/// freeing the main thread from blocking database I/O.

use crate::database::{self, DbPool};
use crate::health::StatsHandle;
use crate::messages::*;
use crate::ShutdownSignal;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::fs::OpenOptions;
use std::io::Write;

// ============================================================================
// Queued Data Operation (internal type)
// ============================================================================

/// Data operation queued for processing
pub enum QueuedDataOp {
    List {
        request_id: String,
        payload: DataListPayload,
        response_tx: mpsc::Sender<(String, Result<DataListResult, String>)>,
    },
    Read {
        request_id: String,
        payload: DataReadPayload,
        response_tx: mpsc::Sender<(String, Result<DataReadResult, String>)>,
    },
    Create {
        request_id: String,
        payload: DataCreatePayload,
        response_tx: mpsc::Sender<(String, Result<DataCreateResult, String>)>,
    },
    Update {
        request_id: String,
        payload: DataUpdatePayload,
        response_tx: mpsc::Sender<(String, Result<DataUpdateResult, String>)>,
    },
}

/// Debug logging to file (temporary)
fn debug_log(msg: &str) {
    let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let log_msg = format!("[{}] {}\n", timestamp, msg);
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
            QueuedDataOp::List {
                request_id,
                payload,
                response_tx,
            } => {
                debug_log(&format!("[Processor] Processing data/list for collection: {}", payload.collection));
                let result = database::with_retry(|| database::execute_list(&db_pool, payload.clone()));
                let _ = response_tx.send((request_id, result.clone()));
                result.map(|_| ()).map_err(|e| e)
            }
            QueuedDataOp::Read {
                request_id,
                payload,
                response_tx,
            } => {
                debug_log(&format!("[Processor] Processing data/read for collection: {}, id: {}", payload.collection, payload.id));
                let result = database::with_retry(|| database::execute_read(&db_pool, payload.clone()));
                let _ = response_tx.send((request_id, result.clone()));
                result.map(|_| ()).map_err(|e| e)
            }
            QueuedDataOp::Create {
                request_id,
                payload,
                response_tx,
            } => {
                debug_log(&format!("[Processor] Processing data/create for collection: {}", payload.collection));
                let result = database::with_retry(|| database::execute_create(&db_pool, payload.clone()));
                let _ = response_tx.send((request_id, result.clone()));
                result.map(|_| ()).map_err(|e| e)
            }
            QueuedDataOp::Update {
                request_id,
                payload,
                response_tx,
            } => {
                debug_log(&format!("[Processor] Processing data/update for collection: {}, id: {}", payload.collection, payload.id));
                let result = database::with_retry(|| database::execute_update(&db_pool, payload.clone()));
                let _ = response_tx.send((request_id, result.clone()));
                result.map(|_| ()).map_err(|e| e)
            }
        };

        // Record success/error in stats
        match result {
            Ok(_) => {
                // Success already recorded when queued
            }
            Err(e) => {
                eprintln!("❌ Processor error: {}", e);
                debug_log(&format!("[Processor] Error: {}", e));

                let mut s = stats.lock().unwrap();
                s.record_error();
            }
        }

        // Log throughput every 100 operations
        if processed % 100 == 0 {
            debug_log(&format!("[Processor] Processed {} data operations", processed));
        }
    }

    debug_log(&format!(
        "[Processor] Queue drained, processed {} total operations",
        processed
    ));
}
