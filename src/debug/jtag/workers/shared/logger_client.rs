/// LoggerClient - Rust Client for LoggerWorker (Rust-to-Rust Logging)
///
/// This allows Rust workers to log to the central LoggerWorker over Unix socket.
/// Uses the same JTAG protocol that TypeScript uses.
///
/// USAGE:
/// ```rust
/// let mut logger = LoggerClient::connect(
///     "/tmp/jtag-logger-worker.sock",
///     "TrainingWorker"
/// )?;
///
/// logger.info("Worker starting...")?;
/// logger.error(&format!("Failed to process: {}", err))?;
/// ```
///
/// This replaces temporary debug_log() functions with production logging.
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::os::unix::net::UnixStream;

// Import shared JTAG protocol
#[path = "jtag_protocol.rs"]
mod jtag_protocol;
use jtag_protocol::{JTAGRequest, JTAGResponse};

// ============================================================================
// Logger-Specific Types (minimal subset for logging)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteLogPayload {
    pub category: String,
    pub level: LogLevel,
    pub component: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub args: Option<serde_json::Value>,
}

// ============================================================================
// LoggerClient
// ============================================================================

/// Client for sending logs to LoggerWorker over Unix socket.
///
/// This is a lightweight, fire-and-forget logger. Errors are silently ignored
/// to avoid crashing workers if LoggerWorker is unavailable.
pub struct LoggerClient {
    stream: Option<UnixStream>,
    component: String,
    category: String,
}

impl LoggerClient {
    /// Connect to LoggerWorker.
    ///
    /// If connection fails, logs will be silently dropped (fail-safe).
    ///
    /// # Arguments
    /// * `socket_path` - Path to LoggerWorker socket (e.g., "/tmp/jtag-logger-worker.sock")
    /// * `component` - Component name (e.g., "TrainingWorker", "DataWorker")
    pub fn connect(socket_path: &str, component: &str) -> Self {
        let stream = UnixStream::connect(socket_path).ok();

        if stream.is_none() {
            eprintln!("⚠️  LoggerClient: Failed to connect to {}", socket_path);
            eprintln!("   Logs will be written to stderr instead");
        }

        Self {
            stream,
            component: component.to_string(),
            category: "rust-workers".to_string(),
        }
    }

    /// Set custom log category.
    pub fn with_category(mut self, category: String) -> Self {
        self.category = category;
        self
    }

    /// Log a debug message.
    pub fn debug(&mut self, message: &str) {
        self.log_internal(LogLevel::Debug, message);
    }

    /// Log an info message.
    pub fn info(&mut self, message: &str) {
        self.log_internal(LogLevel::Info, message);
    }

    /// Log a warning message.
    pub fn warn(&mut self, message: &str) {
        self.log_internal(LogLevel::Warn, message);
    }

    /// Log an error message.
    pub fn error(&mut self, message: &str) {
        self.log_internal(LogLevel::Error, message);
    }

    /// Internal logging implementation.
    ///
    /// If LoggerWorker is unavailable, falls back to stderr.
    fn log_internal(&mut self, level: LogLevel, message: &str) {
        // Fallback to stderr if no connection
        if self.stream.is_none() {
            let level_str = match level {
                LogLevel::Debug => "DEBUG",
                LogLevel::Info => "INFO",
                LogLevel::Warn => "WARN",
                LogLevel::Error => "ERROR",
            };
            eprintln!("[{}] {}: {}", level_str, self.component, message);
            return;
        }

        // Build JTAG request
        let request: JTAGRequest<WriteLogPayload> = JTAGRequest {
            id: uuid::Uuid::new_v4().to_string(),
            r#type: "write-log".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
            payload: WriteLogPayload {
                category: self.category.clone(),
                level,
                component: self.component.clone(),
                message: message.to_string(),
                args: None,
            },
            user_id: Some("rust-worker".to_string()),
            session_id: None,
        };

        // Send to LoggerWorker (fire-and-forget)
        if let Some(ref mut stream) = self.stream {
            if let Ok(json) = serde_json::to_string(&request) {
                let _ = writeln!(stream, "{}", json);
                let _ = stream.flush();
            }
        }
    }
}

// ============================================================================
// Global Logger (Optional Convenience)
// ============================================================================

use std::sync::Mutex;

static GLOBAL_LOGGER: Mutex<Option<LoggerClient>> = Mutex::new(None);

/// Initialize global logger (call once at worker startup).
pub fn init_global_logger(socket_path: &str, component: &str) {
    let logger = LoggerClient::connect(socket_path, component);
    *GLOBAL_LOGGER.lock().unwrap() = Some(logger);
}

/// Log to global logger (convenience functions).
pub fn log_info(message: &str) {
    if let Some(ref mut logger) = *GLOBAL_LOGGER.lock().unwrap() {
        logger.info(message);
    }
}

pub fn log_warn(message: &str) {
    if let Some(ref mut logger) = *GLOBAL_LOGGER.lock().unwrap() {
        logger.warn(message);
    }
}

pub fn log_error(message: &str) {
    if let Some(ref mut logger) = *GLOBAL_LOGGER.lock().unwrap() {
        logger.error(message);
    }
}
