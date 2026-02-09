/// Logger client for continuum-core
///
/// Connects to the logger worker via Unix socket and sends log messages.
/// Non-blocking: uses a bounded channel — callers never block on I/O.
use super::{LogLevel, WriteLogPayload};
use serde::{Deserialize, Serialize};
use std::io::{BufWriter, Write};
use std::os::unix::net::UnixStream;
use std::sync::mpsc;

/// Channel capacity — if this many messages are queued, new ones are silently dropped.
/// At ~200 bytes per message, 1024 messages = ~200KB buffer.
const LOG_CHANNEL_CAPACITY: usize = 1024;

/// JTAG protocol request envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JTAGRequest<T> {
    id: String,
    #[serde(rename = "type")]
    r#type: String,
    timestamp: String,
    payload: T,
        user_id: Option<String>,
        session_id: Option<String>,
}

/// Logger client — fire-and-forget via bounded channel.
///
/// Callers enqueue log messages into a bounded channel (never blocks).
/// A background writer thread drains the channel and writes to the Unix socket.
/// If the channel is full, messages are silently dropped.
pub struct LoggerClient {
    sender: mpsc::SyncSender<String>,
}

impl LoggerClient {
    /// Connect to the logger worker and spawn a background writer thread.
    pub fn new(socket_path: &str) -> Result<Self, String> {
        let stream = UnixStream::connect(socket_path)
            .map_err(|e| format!("Failed to connect to logger: {e}"))?;

        let socket_path_owned = socket_path.to_string();
        let (sender, receiver) = mpsc::sync_channel::<String>(LOG_CHANNEL_CAPACITY);

        // Background writer thread — owns the socket, reads from channel
        std::thread::Builder::new()
            .name("logger-writer".into())
            .spawn(move || {
                let mut writer = BufWriter::new(stream);
                while let Ok(json) = receiver.recv() {
                    if writeln!(writer, "{json}").is_err() {
                        // Reconnect on write failure
                        if let Ok(new_stream) = UnixStream::connect(&socket_path_owned) {
                            writer = BufWriter::new(new_stream);
                            let _ = writeln!(writer, "{json}");
                        }
                    }
                    let _ = writer.flush();
                }
                // Channel closed — writer thread exits cleanly
            })
            .map_err(|e| format!("Failed to spawn logger thread: {e}"))?;

        Ok(Self { sender })
    }

    /// Send a log message. Never blocks — if the channel is full, the message is dropped.
    pub fn log(
        &self,
        category: &str,
        level: LogLevel,
        component: &str,
        message: &str,
        args: Option<serde_json::Value>,
    ) {
        let payload = WriteLogPayload {
            category: category.to_string(),
            level,
            component: component.to_string(),
            message: message.to_string(),
            args,
        };

        let request = JTAGRequest {
            id: uuid::Uuid::new_v4().to_string(),
            r#type: "write-log".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            payload,
            user_id: None,
            session_id: None,
        };

        let json = match serde_json::to_string(&request) {
            Ok(j) => j,
            Err(_) => return,
        };

        // Fire-and-forget: try_send returns immediately, drops if full
        let _ = self.sender.try_send(json);
    }

    /// Log with explicit level
    pub fn log_level(
        &self,
        category: &str,
        level: LogLevel,
        component: &str,
        message: &str,
    ) {
        self.log(category, level, component, message, None);
    }

    /// Debug log
    pub fn debug(&self, category: &str, component: &str, message: &str) {
        self.log(category, LogLevel::Debug, component, message, None);
    }

    /// Info log
    pub fn info(&self, category: &str, component: &str, message: &str) {
        self.log(category, LogLevel::Info, component, message, None);
    }

    /// Warning log
    pub fn warn(&self, category: &str, component: &str, message: &str) {
        self.log(category, LogLevel::Warn, component, message, None);
    }

    /// Error log
    pub fn error(&self, category: &str, component: &str, message: &str) {
        self.log(category, LogLevel::Error, component, message, None);
    }
}

// LoggerClient is thread-safe: SyncSender is Send+Sync
unsafe impl Send for LoggerClient {}
unsafe impl Sync for LoggerClient {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_logger_serialization() {
        let payload = WriteLogPayload {
            category: "test".to_string(),
            level: LogLevel::Info,
            component: "unittest".to_string(),
            message: "Test message".to_string(),
            args: None,
        };

        let request = JTAGRequest {
            id: "test-id".to_string(),
            r#type: "write-log".to_string(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
            payload,
            user_id: None,
            session_id: None,
        };

        let json = serde_json::to_string(&request).expect("Should serialize");
        assert!(json.contains("write-log"));
        assert!(json.contains("Test message"));
    }
}
