/// Connection Handler Module - IPC Message Processing
///
/// This module handles individual client connections:
/// - Newline-delimited JSON message parsing
/// - Message routing (write-log, ping, etc.)
/// - Response generation
/// - Error handling
///
/// Each connection runs in its own thread for concurrency.
use crate::file_manager::{self, FileCache, HeaderTracker};
use crate::health::StatsHandle;
use crate::messages::*;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;

/// Debug logging to file (temporary - will be removed).
fn debug_log(msg: &str) {
    use std::fs::OpenOptions;
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

/// Handle a single client connection.
///
/// This function runs in its own thread and processes messages
/// until the client disconnects (EOF on socket).
///
/// Message types:
/// - "write-log": Write log entry to file
/// - "ping": Health check (return stats)
/// - Unknown types: Return error response
pub fn handle_client(
    stream: UnixStream,
    log_dir: &str,
    file_cache: FileCache,
    headers_written: HeaderTracker,
    stats: StatsHandle,
) -> std::io::Result<()> {
    debug_log("handle_client: START");
    debug_log("Creating BufReader and cloning stream for writer");
    let mut reader = BufReader::new(&stream);
    let mut writer = stream.try_clone()?;
    debug_log("Reader/writer created successfully");

    // Process messages until client disconnects
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

        debug_log("About to trim line");
        let line = line.trim();
        debug_log(&format!("Trimmed line length: {}", line.len()));

        if line.is_empty() {
            debug_log("Line is empty, continuing loop");
            continue;
        }

        debug_log(&format!(
            "Line content (first 50 chars): {:?}",
            &line.chars().take(50).collect::<String>()
        ));
        println!("📨 Received: {} bytes", line.len());

        // Parse and route message
        match parse_message(line) {
            Ok((msg_type, msg_id)) => {
                println!("✅ Parsed request: type={}, id={}", msg_type, msg_id);
                handle_message(
                    line,
                    &msg_type,
                    &msg_id,
                    log_dir,
                    &file_cache,
                    &headers_written,
                    &stats,
                    &mut writer,
                )?;
            }
            Err(e) => {
                eprintln!("❌ Failed to parse request: {}", e);
                eprintln!("   Raw message: {}", line);
                send_parse_error(line, &mut writer, &e)?;
            }
        }
    }

    Ok(())
}

// ============================================================================
// Message Parsing
// ============================================================================

/// Parse base message to extract type and id fields.
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

/// Route message to appropriate handler based on type.
fn handle_message(
    line: &str,
    msg_type: &str,
    msg_id: &str,
    log_dir: &str,
    file_cache: &FileCache,
    headers_written: &HeaderTracker,
    stats: &StatsHandle,
    writer: &mut UnixStream,
) -> std::io::Result<()> {
    match msg_type {
        "write-log" => handle_write_log(line, log_dir, file_cache, headers_written, stats, writer),
        "ping" => handle_ping(line, file_cache, stats, writer),
        _ => handle_unknown(msg_type, msg_id, writer),
    }
}

// ============================================================================
// Message Handlers
// ============================================================================

/// Handle write-log request.
fn handle_write_log(
    line: &str,
    log_dir: &str,
    file_cache: &FileCache,
    headers_written: &HeaderTracker,
    stats: &StatsHandle,
    writer: &mut UnixStream,
) -> std::io::Result<()> {
    // Parse request
    let request: JTAGRequest<WriteLogPayload> =
        serde_json::from_str(line).expect("Failed to parse write-log payload");

    // Write log message
    let bytes_written =
        file_manager::write_log_message(&request.payload, log_dir, file_cache, headers_written)?;

    // Update stats
    {
        let mut s = stats.lock().unwrap();
        s.record_request();
    }

    // Build and send response
    let response = JTAGResponse::success(
        request.id.clone(),
        request.r#type.clone(),
        WriteLogResult { bytes_written },
    );
    send_response(&response, writer)?;

    println!("✅ Sent response: {} bytes written", bytes_written);
    Ok(())
}

/// Handle ping request (health check).
fn handle_ping(
    line: &str,
    file_cache: &FileCache,
    stats: &StatsHandle,
    writer: &mut UnixStream,
) -> std::io::Result<()> {
    // Parse request
    let request: JTAGRequest<PingPayload> =
        serde_json::from_str(line).expect("Failed to parse ping payload");

    // Gather stats
    let (uptime_ms, connections_total, requests_processed) = {
        let s = stats.lock().unwrap();
        (s.uptime_ms(), s.connections_total(), s.requests_processed())
    };

    let active_categories = file_manager::active_category_count(file_cache);

    // Build and send response
    let ping_result = PingResult {
        uptime_ms,
        connections_total,
        requests_processed,
        active_categories,
    };
    let response = JTAGResponse::success(request.id.clone(), request.r#type.clone(), ping_result);
    send_response(&response, writer)?;

    println!(
        "✅ Sent ping response: uptime={}ms, connections={}, requests={}, categories={}",
        uptime_ms, connections_total, requests_processed, active_categories
    );
    Ok(())
}

/// Handle unknown message type.
fn handle_unknown(msg_type: &str, msg_id: &str, writer: &mut UnixStream) -> std::io::Result<()> {
    eprintln!("❌ Unknown message type: {}", msg_type);
    let error_response = JTAGResponse::<WriteLogResult>::error(
        msg_id.to_string(),
        msg_type.to_string(),
        WriteLogResult { bytes_written: 0 },
        format!("Unknown message type: {}", msg_type),
        JTAGErrorType::Validation,
    );
    send_response(&error_response, writer)
}

// ============================================================================
// Response Sending
// ============================================================================

/// Send a response message (generic).
fn send_response<T: serde::Serialize>(
    response: &JTAGResponse<T>,
    writer: &mut UnixStream,
) -> std::io::Result<()> {
    let json = serde_json::to_string(response).expect("Failed to serialize response");
    writeln!(writer, "{}", json)?;
    writer.flush()
}

/// Send parse error response.
fn send_parse_error(
    line: &str,
    writer: &mut UnixStream,
    error: &serde_json::Error,
) -> std::io::Result<()> {
    // Try to extract request ID for error response
    if let Ok(base_msg) = serde_json::from_str::<serde_json::Value>(line) {
        if let Some(id) = base_msg.get("id").and_then(|v| v.as_str()) {
            let error_response = JTAGResponse::<WriteLogResult>::error(
                id.to_string(),
                "write-log".to_string(),
                WriteLogResult { bytes_written: 0 },
                format!("Parse error: {}", error),
                JTAGErrorType::Validation,
            );
            send_response(&error_response, writer)?;
        }
    }
    Ok(())
}
