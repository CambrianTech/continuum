/// Simple Logger Worker - Rust IPC Demo
///
/// This is a minimal proof-of-concept that:
/// 1. Listens on a Unix domain socket
/// 2. Receives JSON messages from TypeScript
/// 3. Writes log entries to stdout (or could write to files)
/// 4. Sends JSON responses back to TypeScript
///
/// Run: cargo run -- /tmp/logger-worker.sock

mod messages;

use messages::*;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <socket-path>", args[0]);
        eprintln!("Example: {} /tmp/logger-worker.sock", args[0]);
        std::process::exit(1);
    }

    let socket_path = &args[1];

    // Remove socket file if it exists
    if Path::new(socket_path).exists() {
        std::fs::remove_file(socket_path)?;
    }

    println!("🦀 Rust Logger Worker starting...");
    println!("📡 Listening on: {}", socket_path);

    let listener = UnixListener::bind(socket_path)?;

    println!("✅ Ready to accept connections");

    // Accept connections and handle them one at a time (single-threaded for simplicity)
    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                println!("\n🔗 New connection from TypeScript");
                if let Err(e) = handle_client(stream) {
                    eprintln!("❌ Error handling client: {}", e);
                }
            }
            Err(e) => {
                eprintln!("❌ Connection error: {}", e);
            }
        }
    }

    Ok(())
}

fn handle_client(stream: UnixStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(&stream);
    let mut writer = stream.try_clone()?;

    // Read JSON messages line by line
    loop {
        let mut line = String::new();
        let bytes_read = reader.read_line(&mut line)?;

        if bytes_read == 0 {
            println!("📪 Client disconnected");
            break;
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        println!("📨 Received: {} bytes", line.len());

        // Parse request
        let request: Result<WorkerRequest<WriteLogPayload>, _> =
            serde_json::from_str(line);

        match request {
            Ok(req) => {
                println!("✅ Parsed request: type={}, id={}", req.r#type, req.id);

                // Process the log message
                let bytes_written = process_log_message(&req.payload);

                // Build response
                let response = WorkerResponse::success(
                    req.id.clone(),
                    req.r#type.clone(),
                    WriteLogResult { bytes_written }
                );

                // Send response back to TypeScript
                let response_json = serde_json::to_string(&response)
                    .expect("Failed to serialize response");

                writeln!(writer, "{}", response_json)?;
                writer.flush()?;

                println!("✅ Sent response: {} bytes written", bytes_written);
            }
            Err(e) => {
                eprintln!("❌ Failed to parse request: {}", e);
                eprintln!("   Raw message: {}", line);

                // Try to extract request ID for error response
                if let Ok(base_msg) = serde_json::from_str::<serde_json::Value>(line) {
                    if let Some(id) = base_msg.get("id").and_then(|v| v.as_str()) {
                        let error_response = WorkerResponse::<WriteLogResult>::error(
                            id.to_string(),
                            "write-log".to_string(),
                            WriteLogResult { bytes_written: 0 },
                            format!("Parse error: {}", e),
                            ErrorType::Validation
                        );

                        let error_json = serde_json::to_string(&error_response)
                            .expect("Failed to serialize error response");

                        writeln!(writer, "{}", error_json)?;
                        writer.flush()?;
                    }
                }
            }
        }
    }

    Ok(())
}

fn process_log_message(payload: &WriteLogPayload) -> usize {
    // Format log entry
    let log_entry = format!(
        "[{}] [{}] {}: {}",
        payload.level.to_string().to_uppercase(),
        payload.category,
        payload.component,
        payload.message
    );

    // Print to stdout (in production, would write to file)
    println!("📝 LOG: {}", log_entry);

    // Return bytes written (length of formatted message)
    log_entry.len()
}

impl std::fmt::Display for LogLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LogLevel::Debug => write!(f, "debug"),
            LogLevel::Info => write!(f, "info"),
            LogLevel::Warn => write!(f, "warn"),
            LogLevel::Error => write!(f, "error"),
        }
    }
}
