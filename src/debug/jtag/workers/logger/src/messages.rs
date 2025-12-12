/// Logger Worker - Message Types using JTAGProtocol
///
/// This uses the universal JTAGProtocol from workers/shared/jtag_protocol.rs
/// which mirrors shared/ipc/JTAGProtocol.ts on the TypeScript side.
use serde::{Deserialize, Serialize};

// Import shared JTAGProtocol types
#[path = "../../shared/jtag_protocol.rs"]
mod jtag_protocol;

// Re-export JTAG protocol types for library users
pub use jtag_protocol::{JTAGErrorType, JTAGRequest, JTAGResponse};

// ============================================================================
// Logger-Specific Types (owned by logger worker)
// ============================================================================

/// Log levels matching TypeScript LogLevel type.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warn,
    Error,
}

/// Payload for write-log requests.
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

/// Payload for write-log responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteLogResult {
    pub bytes_written: usize,
}

// ============================================================================
// Health Check Types (for detecting frozen worker)
// ============================================================================

/// Ping request payload (empty - just proves worker is alive)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingPayload {}

/// Ping result - includes uptime and connection stats
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingResult {
    pub uptime_ms: u64,
    pub connections_total: u64,
    pub requests_processed: u64,
    pub active_categories: usize,
}

// Helper functions (success/error) are now in the shared jtag_protocol module
