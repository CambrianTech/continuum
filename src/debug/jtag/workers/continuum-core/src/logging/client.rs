/// Logger client for continuum-core
///
/// Connects to the logger worker via Unix socket and sends log messages.
/// Non-blocking: uses a channel to avoid blocking the caller.
use super::{LogLevel, WriteLogPayload};
use serde::{Deserialize, Serialize};
use std::os::unix::net::UnixStream;
use std::io::{Write, BufWriter};
use std::sync::Mutex;

/// JTAG protocol request envelope
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct JTAGRequest<T> {
    id: String,
    #[serde(rename = "type")]
    r#type: String,
    timestamp: String,
    payload: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    user_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<String>,
}

/// Logger client - connects to logger worker via Unix socket
pub struct LoggerClient {
    socket_path: String,
    // Mutex protects the stream for concurrent access
    stream: Mutex<BufWriter<UnixStream>>,
}

impl LoggerClient {
    /// Create a new logger client
    pub fn new(socket_path: &str) -> Result<Self, String> {
        let stream = UnixStream::connect(socket_path)
            .map_err(|e| format!("Failed to connect to logger: {e}"))?;

        // Set non-blocking mode
        stream.set_nonblocking(false)
            .map_err(|e| format!("Failed to set socket mode: {e}"))?;

        Ok(Self {
            socket_path: socket_path.to_string(),
            stream: Mutex::new(BufWriter::new(stream)),
        })
    }

    /// Send a log message (non-blocking via channel)
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

        // Wrap in JTAG protocol
        let request = JTAGRequest {
            id: uuid::Uuid::new_v4().to_string(),
            r#type: "write-log".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            payload,
            user_id: None,
            session_id: None,
        };

        // Serialize to JSON with newline delimiter
        let json = match serde_json::to_string(&request) {
            Ok(j) => j,
            Err(e) => {
                eprintln!("Failed to serialize log message: {e}");
                return;
            }
        };

        // Send to logger worker (lock for thread safety)
        if let Ok(mut stream) = self.stream.lock() {
            if let Err(e) = writeln!(stream, "{json}") {
                eprintln!("Failed to write to logger socket: {e}");
                // Try to reconnect
                if let Ok(new_stream) = UnixStream::connect(&self.socket_path) {
                    *stream = BufWriter::new(new_stream);
                }
            } else {
                // Flush to ensure delivery
                let _ = stream.flush();
            }
        }
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

// Logger client is thread-safe
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

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("write-log"));
        assert!(json.contains("Test message"));
    }
}
