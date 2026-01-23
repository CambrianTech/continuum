/// Logging module for continuum-core
///
/// Integrates with the existing logger worker via Unix socket.
/// Provides macros for performance timing and structured logging.
pub mod timing;
pub mod client;

pub use timing::TimingGuard;
pub use client::LoggerClient;

// Re-export macros (they're already at crate root via #[macro_export])
pub use crate::{time_section, time_async};

use serde::{Deserialize, Serialize};

/// Log levels matching the logger worker
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// Payload for write-log requests (matches logger worker)
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

/// Global logger instance (lazy static)
use std::sync::OnceLock;
pub static LOGGER: OnceLock<LoggerClient> = OnceLock::new();

/// Initialize the global logger (idempotent - safe to call multiple times)
pub fn init_logger(socket_path: &str) -> Result<(), String> {
    // If already initialized, just return success
    if LOGGER.get().is_some() {
        return Ok(());
    }

    let client = LoggerClient::new(socket_path)?;
    LOGGER.set(client).map_err(|_| "Logger already initialized".to_string())
}

/// Get the global logger instance
pub fn logger() -> &'static LoggerClient {
    LOGGER.get().expect("Logger not initialized - call init_logger() first")
}

/// Log macros for convenience
#[macro_export]
macro_rules! log_debug {
    ($category:expr, $component:expr, $($arg:tt)*) => {
        if let Some(logger) = $crate::logging::LOGGER.get() {
            logger.log(
                $category,
                $crate::logging::LogLevel::Debug,
                $component,
                &format!($($arg)*),
                None
            );
        }
    };
}

#[macro_export]
macro_rules! log_info {
    ($category:expr, $component:expr, $($arg:tt)*) => {
        if let Some(logger) = $crate::logging::LOGGER.get() {
            logger.log(
                $category,
                $crate::logging::LogLevel::Info,
                $component,
                &format!($($arg)*),
                None
            );
        }
    };
}

#[macro_export]
macro_rules! log_warn {
    ($category:expr, $component:expr, $($arg:tt)*) => {
        if let Some(logger) = $crate::logging::LOGGER.get() {
            logger.log(
                $category,
                $crate::logging::LogLevel::Warn,
                $component,
                &format!($($arg)*),
                None
            );
        }
    };
}

#[macro_export]
macro_rules! log_error {
    ($category:expr, $component:expr, $($arg:tt)*) => {
        if let Some(logger) = $crate::logging::LOGGER.get() {
            logger.log(
                $category,
                $crate::logging::LogLevel::Error,
                $component,
                &format!($($arg)*),
                None
            );
        }
    };
}
