/// Logging module for continuum-core
///
/// Integrates with the existing logger worker via Unix socket.
/// Provides macros for performance timing and structured logging.
///
/// # Easy Logging (Zero Setup)
///
/// Auto-routing macros that derive category from module_path!():
///
/// ```rust
/// use crate::clog_info;
///
/// // Auto-routes to modules/voice.log based on module path
/// clog_info!("Session started for user {}", user_id);
/// clog_warn!("Rate limit approaching");
/// clog_error!("Connection failed: {}", err);
/// ```
///
/// # Explicit Logging (Full Control)
///
/// ```rust
/// use crate::log_info;
///
/// log_info!("personas/helper/cognition", "DecisionEngine", "Made decision: {}", decision);
/// ```
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

// ============================================================================
// Easy Auto-Routing Macros — Zero Setup Required
// ============================================================================
//
// These macros derive the category and component from module_path!().
// No need to specify category/component - just log!
//
// Category routing examples:
// - continuum_core::voice::orchestrator → modules/voice
// - continuum_core::modules::data → modules/data
// - continuum_core::orm::sqlite → modules/orm
// - continuum_core::ipc::* → system/ipc

/// Convert module_path!() to a log category.
///
/// Maps Rust module paths to concern-based log categories:
/// - `continuum_core::voice::*` → `modules/voice`
/// - `continuum_core::modules::data` → `modules/data`
/// - `continuum_core::orm::*` → `modules/orm`
/// - `continuum_core::ipc::*` → `system/ipc`
pub fn module_path_to_category(module_path: &str) -> &'static str {
    // Strip the crate prefix
    let path = module_path
        .strip_prefix("continuum_core::")
        .unwrap_or(module_path);

    // Match first segment to determine category
    if path.starts_with("modules::data") {
        "modules/data"
    } else if path.starts_with("modules::embedding") {
        "modules/embedding"
    } else if path.starts_with("modules::search") {
        "modules/search"
    } else if path.starts_with("modules::logger") {
        "modules/logger"
    } else if path.starts_with("modules::voice") {
        "modules/voice"
    } else if path.starts_with("modules::memory") {
        "modules/memory"
    } else if path.starts_with("modules::code") {
        "modules/code"
    } else if path.starts_with("modules::rag") {
        "modules/rag"
    } else if path.starts_with("modules::cognition") {
        "modules/cognition"
    } else if path.starts_with("modules::channel") {
        "modules/channel"
    } else if path.starts_with("modules::health") {
        "modules/health"
    } else if path.starts_with("modules::models") {
        "modules/models"
    } else if path.starts_with("voice::") {
        "modules/voice"
    } else if path.starts_with("orm::") {
        "modules/orm"
    } else if path.starts_with("ai::") || path.starts_with("inference::") {
        "modules/inference"
    } else if path.starts_with("memory::") {
        "modules/memory"
    } else if path.starts_with("rag::") {
        "modules/rag"
    } else if path.starts_with("code::") {
        "modules/code"
    } else if path.starts_with("ipc::") {
        "system/ipc"
    } else if path.starts_with("concurrent::") {
        "system/concurrent"
    } else if path.starts_with("ffi::") {
        "system/ffi"
    } else if path.starts_with("runtime::") {
        "system/runtime"
    } else if path.starts_with("persona::") {
        "modules/persona"
    } else {
        "system/core"
    }
}

/// Extract component name from module path (last segment).
pub fn extract_component(module_path: &str) -> &str {
    module_path.rsplit("::").next().unwrap_or(module_path)
}

// ============================================================================
// Non-Blocking Logging — Uses LoggerModule's Optimized Writer
// ============================================================================
//
// clog_* macros route to LoggerModule's queue_log() which:
// - Uses bounded sync_channel with try_send (GUARANTEED non-blocking)
// - Background writer with batching (250ms or 200 messages)
// - Per-category rate limiting (100 msg/sec)
// - File handle caching
// - Proper directory routing
//
// If LoggerModule not initialized, messages silently dropped.
// If channel full, messages silently dropped (NEVER blocks).

use crate::modules::logger::{queue_log, LogLevel as ModuleLogLevel};

/// Queue log entry via LoggerModule (NON-BLOCKING)
#[inline]
pub fn write_log_direct(category: &str, level: &str, component: &str, message: &str) {
    let log_level = match level {
        "DEBUG" => ModuleLogLevel::Debug,
        "INFO" => ModuleLogLevel::Info,
        "WARN" => ModuleLogLevel::Warn,
        "ERROR" => ModuleLogLevel::Error,
        _ => ModuleLogLevel::Info,
    };
    queue_log(category, log_level, component, message);
}

/// Easy info log — auto-routes by module_path!(), writes directly to files
#[macro_export]
macro_rules! clog_info {
    ($($arg:tt)*) => {{
        let category = $crate::logging::module_path_to_category(module_path!());
        let component = $crate::logging::extract_component(module_path!());
        let message = format!($($arg)*);
        $crate::logging::write_log_direct(category, "INFO", component, &message);
    }};
}

/// Easy warn log — auto-routes by module_path!(), writes directly to files
#[macro_export]
macro_rules! clog_warn {
    ($($arg:tt)*) => {{
        let category = $crate::logging::module_path_to_category(module_path!());
        let component = $crate::logging::extract_component(module_path!());
        let message = format!($($arg)*);
        $crate::logging::write_log_direct(category, "WARN", component, &message);
    }};
}

/// Easy error log — auto-routes by module_path!(), writes directly to files
#[macro_export]
macro_rules! clog_error {
    ($($arg:tt)*) => {{
        let category = $crate::logging::module_path_to_category(module_path!());
        let component = $crate::logging::extract_component(module_path!());
        let message = format!($($arg)*);
        $crate::logging::write_log_direct(category, "ERROR", component, &message);
    }};
}

/// Easy debug log — auto-routes by module_path!(), writes directly to files
#[macro_export]
macro_rules! clog_debug {
    ($($arg:tt)*) => {{
        let category = $crate::logging::module_path_to_category(module_path!());
        let component = $crate::logging::extract_component(module_path!());
        let message = format!($($arg)*);
        $crate::logging::write_log_direct(category, "DEBUG", component, &message);
    }};
}

/// Log to explicit category (for cross-cutting concerns like personas, sentinels)
#[macro_export]
macro_rules! clog_to {
    ($category:expr, info, $($arg:tt)*) => {{
        let component = $crate::logging::extract_component(module_path!());
        let message = format!($($arg)*);
        $crate::logging::write_log_direct($category, "INFO", component, &message);
    }};
    ($category:expr, warn, $($arg:tt)*) => {{
        let component = $crate::logging::extract_component(module_path!());
        let message = format!($($arg)*);
        $crate::logging::write_log_direct($category, "WARN", component, &message);
    }};
    ($category:expr, error, $($arg:tt)*) => {{
        let component = $crate::logging::extract_component(module_path!());
        let message = format!($($arg)*);
        $crate::logging::write_log_direct($category, "ERROR", component, &message);
    }};
    ($category:expr, debug, $($arg:tt)*) => {{
        let component = $crate::logging::extract_component(module_path!());
        let message = format!($($arg)*);
        $crate::logging::write_log_direct($category, "DEBUG", component, &message);
    }};
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_module_path_conversion() {
        assert_eq!(
            module_path_to_category("continuum_core::voice::orchestrator"),
            "modules/voice"
        );
        assert_eq!(
            module_path_to_category("continuum_core::modules::data"),
            "modules/data"
        );
        assert_eq!(
            module_path_to_category("continuum_core::orm::sqlite"),
            "modules/orm"
        );
        assert_eq!(
            module_path_to_category("continuum_core::ipc::handler"),
            "system/ipc"
        );
    }

    #[test]
    fn test_extract_component() {
        assert_eq!(extract_component("continuum_core::voice::orchestrator"), "orchestrator");
        assert_eq!(extract_component("my_module"), "my_module");
    }
}
