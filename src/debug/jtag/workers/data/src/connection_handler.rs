/// Connection Handler Module - Universal Protocol + SQL Operations
///
/// Routes messages to appropriate handlers:
/// - ping → health check (universal protocol)
/// - shutdown → graceful shutdown (universal protocol)
/// - status → diagnostics (universal protocol)
/// - sql/query → queue for background SQL execution (SELECT)
/// - sql/execute → queue for background SQL execution (INSERT/UPDATE/DELETE)

use crate::health::{self, StatsHandle};
use crate::messages::*;
use crate::processor::QueuedDataOp;
use crate::ShutdownSignal;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::sync::atomic::Ordering;
use std::sync::mpsc;

/// Debug logging to file (temporary)
fn debug_log(msg: &str) {
    use std::fs::OpenOptions;
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

/// Handle a single client connection
///
/// Routes messages based on type:
/// - Universal protocol messages (ping, shutdown, status) - handled immediately
/// - Data messages (data/list, data/read, data/create, data/update) - queued for async processing
pub fn handle_client(
    stream: UnixStream,
    data_tx: mpsc::Sender<QueuedDataOp>,
    stats: StatsHandle,
    shutdown_signal: ShutdownSignal,
) -> std::io::Result<()> {
    debug_log("handle_client: START");
    let mut reader = BufReader::new(&stream);
    let mut writer = stream.try_clone()?;

    // Process messages until client disconnects
    loop {
        // Check shutdown signal
        if shutdown_signal.load(Ordering::Relaxed) {
            debug_log("Shutdown signal detected, closing connection");
            break;
        }

        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line)?;

        if bytes_read == 0 {
            debug_log("Client disconnected (EOF)");
            println!("📪 Client disconnected (EOF)");
            break;
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        debug_log(&format!("Received message: {} bytes", line.len()));
        println!("📨 Received: {} bytes", line.len());

        // Parse and route message
        match parse_message(line) {
            Ok((msg_type, msg_id)) => {
                println!("✅ Parsed request: type={}, id={}", msg_type, msg_id);
                handle_message(
                    line,
                    &msg_type,
                    &msg_id,
                    &data_tx,
                    &stats,
                    &shutdown_signal,
                    &mut writer,
                )?;
            }
            Err(e) => {
                eprintln!("❌ Failed to parse request: {}", e);
                send_parse_error(line, &mut writer, &e)?;
            }
        }
    }

    Ok(())
}

// ============================================================================
// Message Parsing
// ============================================================================

/// Parse base message to extract type and id fields
fn parse_message(line: &str) -> Result<(String, String), serde_json::Error> {
    let msg: serde_json::Value = serde_json::from_str(line)?;
    let msg_type = msg
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let msg_id = msg
        .get("id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok((msg_type, msg_id))
}

// ============================================================================
// Message Routing
// ============================================================================

/// Route message to appropriate handler
fn handle_message(
    line: &str,
    msg_type: &str,
    msg_id: &str,
    data_tx: &mpsc::Sender<QueuedDataOp>,
    stats: &StatsHandle,
    shutdown_signal: &ShutdownSignal,
    writer: &mut UnixStream,
) -> std::io::Result<()> {
    match msg_type {
        "ping" => handle_ping(line, stats, writer),
        "shutdown" => handle_shutdown(line, shutdown_signal, writer),
        "status" => handle_status(line, stats, writer),
        "sql/query" => handle_sql_query(line, data_tx, stats, writer),
        "sql/execute" => handle_sql_execute(line, data_tx, stats, writer),
        _ => handle_unknown(msg_type, msg_id, writer),
    }
}

// ============================================================================
// Universal Protocol Handlers
// ============================================================================

/// Handle ping request (health check)
fn handle_ping(
    line: &str,
    stats: &StatsHandle,
    writer: &mut UnixStream,
) -> std::io::Result<()> {
    let request: JTAGRequest<serde_json::Value> =
        serde_json::from_str(line).expect("Failed to parse ping");

    let ping_result = {
        let s = stats.lock().unwrap();
        health::generate_ping_result(&s)
    };

    let response = JTAGResponse::success(
        request.id.clone(),
        request.r#type.clone(),
        ping_result,
    );
    send_response(&response, writer)?;

    println!("✅ Sent ping response");
    Ok(())
}

/// Handle shutdown request (graceful shutdown)
fn handle_shutdown(
    line: &str,
    shutdown_signal: &ShutdownSignal,
    writer: &mut UnixStream,
) -> std::io::Result<()> {
    let request: JTAGRequest<health::ShutdownPayload> =
        serde_json::from_str(line).expect("Failed to parse shutdown");

    // Set shutdown signal
    shutdown_signal.store(true, Ordering::Relaxed);

    let shutdown_result = health::ShutdownResult {
        queue_drained: 0, // TODO: Track actual queue size
        shutdown_time_ms: 0, // Will be calculated by main thread
    };

    let response = JTAGResponse::success(
        request.id.clone(),
        request.r#type.clone(),
        shutdown_result,
    );
    send_response(&response, writer)?;

    println!("✅ Shutdown initiated");
    debug_log("Shutdown signal set");
    Ok(())
}

/// Handle status request (detailed diagnostics)
fn handle_status(
    line: &str,
    stats: &StatsHandle,
    writer: &mut UnixStream,
) -> std::io::Result<()> {
    let request: JTAGRequest<health::StatusPayload> =
        serde_json::from_str(line).expect("Failed to parse status");

    let status_result = {
        let s = stats.lock().unwrap();
        health::generate_status_result(&s, request.payload.verbose)
    };

    let response = JTAGResponse::success(
        request.id.clone(),
        request.r#type.clone(),
        status_result,
    );
    send_response(&response, writer)?;

    println!("✅ Sent status response");
    Ok(())
}

// ============================================================================
// SQL Operation Handlers
// ============================================================================

/// Handle sql/query request (SELECT)
fn handle_sql_query(
    line: &str,
    data_tx: &mpsc::Sender<QueuedDataOp>,
    stats: &StatsHandle,
    writer: &mut UnixStream,
) -> std::io::Result<()> {
    let request: JTAGRequest<SqlQueryPayload> =
        serde_json::from_str(line).expect("Failed to parse sql/query");

    // Create response channel
    let (response_tx, response_rx) = mpsc::channel();

    // Queue operation for background processing (non-blocking fast path)
    if let Err(e) = data_tx.send(QueuedDataOp::Query {
        request_id: request.id.clone(),
        payload: request.payload.clone(),
        response_tx,
    }) {
        eprintln!("❌ Failed to queue sql/query operation: {}", e);
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Queue send failed: {}", e),
        ));
    }

    // Update stats
    {
        let mut s = stats.lock().unwrap();
        s.record_request();
    }

    // Wait for result from processor (blocking on this connection's thread only)
    match response_rx.recv() {
        Ok((_req_id, Ok(result))) => {
            let response = JTAGResponse::success(
                request.id.clone(),
                request.r#type.clone(),
                result,
            );
            send_response(&response, writer)?;
            println!("✅ sql/query operation completed");
        }
        Ok((_req_id, Err(e))) => {
            let response = JTAGResponse::<SqlQueryResult>::error(
                request.id.clone(),
                request.r#type.clone(),
                SqlQueryResult { rows: vec![] },
                e,
                JTAGErrorType::Internal,
            );
            send_response(&response, writer)?;
        }
        Err(e) => {
            eprintln!("❌ Failed to receive result: {}", e);
        }
    }

    Ok(())
}

/// Handle sql/execute request (INSERT/UPDATE/DELETE)
fn handle_sql_execute(
    line: &str,
    data_tx: &mpsc::Sender<QueuedDataOp>,
    stats: &StatsHandle,
    writer: &mut UnixStream,
) -> std::io::Result<()> {
    let request: JTAGRequest<SqlExecutePayload> =
        serde_json::from_str(line).expect("Failed to parse sql/execute");

    let (response_tx, response_rx) = mpsc::channel();

    if let Err(e) = data_tx.send(QueuedDataOp::Execute {
        request_id: request.id.clone(),
        payload: request.payload.clone(),
        response_tx,
    }) {
        eprintln!("❌ Failed to queue sql/execute operation: {}", e);
        return Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            format!("Queue send failed: {}", e),
        ));
    }

    {
        let mut s = stats.lock().unwrap();
        s.record_request();
    }

    match response_rx.recv() {
        Ok((_req_id, Ok(result))) => {
            let response = JTAGResponse::success(
                request.id.clone(),
                request.r#type.clone(),
                result,
            );
            send_response(&response, writer)?;
            println!("✅ sql/execute operation completed");
        }
        Ok((_req_id, Err(e))) => {
            let response = JTAGResponse::<SqlExecuteResult>::error(
                request.id.clone(),
                request.r#type.clone(),
                SqlExecuteResult {
                    changes: 0,
                    last_insert_id: None,
                },
                e,
                JTAGErrorType::Internal,
            );
            send_response(&response, writer)?;
        }
        Err(e) => {
            eprintln!("❌ Failed to receive result: {}", e);
        }
    }

    Ok(())
}

/// Handle unknown message type
fn handle_unknown(msg_type: &str, msg_id: &str, writer: &mut UnixStream) -> std::io::Result<()> {
    eprintln!("❌ Unknown message type: {}", msg_type);
    let error_response = JTAGResponse::<serde_json::Value>::error(
        msg_id.to_string(),
        msg_type.to_string(),
        serde_json::Value::Null,
        format!("Unknown message type: {}", msg_type),
        JTAGErrorType::Validation,
    );
    send_response(&error_response, writer)
}

// ============================================================================
// Response Sending
// ============================================================================

/// Send a response message (generic)
fn send_response<T: serde::Serialize>(
    response: &JTAGResponse<T>,
    writer: &mut UnixStream,
) -> std::io::Result<()> {
    let json = serde_json::to_string(response).expect("Failed to serialize response");
    writeln!(writer, "{}", json)?;
    writer.flush()
}

/// Send parse error response
fn send_parse_error(
    line: &str,
    writer: &mut UnixStream,
    error: &serde_json::Error,
) -> std::io::Result<()> {
    if let Ok(base_msg) = serde_json::from_str::<serde_json::Value>(line) {
        if let Some(id) = base_msg.get("id").and_then(|v| v.as_str()) {
            let error_response = JTAGResponse::<serde_json::Value>::error(
                id.to_string(),
                "unknown".to_string(),
                serde_json::Value::Null,
                format!("Parse error: {}", error),
                JTAGErrorType::Validation,
            );
            send_response(&error_response, writer)?;
        }
    }
    Ok(())
}
