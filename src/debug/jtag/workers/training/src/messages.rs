/// Training Worker - Message Types using JTAGProtocol
///
/// This uses the universal JTAGProtocol from workers/shared/jtag_protocol.rs
/// which mirrors shared/ipc/JTAGProtocol.ts on the TypeScript side.
use serde::{Deserialize, Serialize};

// Re-export JTAG protocol types from logger_client to avoid duplicate_mod warning
// logger_client already includes and re-exports jtag_protocol types
pub use super::logger_client::{JTAGErrorType, JTAGResponse, JtagRequest as JTAGRequest};

// ============================================================================
// Training-Specific Types (owned by training worker)
// ============================================================================

/// Payload for export-training requests.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportTrainingPayload {
    /// Output file path for JSONL export
    pub output_path: String,

    /// Maximum number of examples to export (0 = all)
    #[serde(default)]
    pub limit: usize,

    /// Minimum quality score threshold (0.0 - 1.0)
    #[serde(default)]
    pub min_quality: f64,

    /// Export format: 'openai', 'llama', 'alpaca'
    #[serde(default = "default_format")]
    pub format: String,
}

fn default_format() -> String {
    "openai".to_string()
}

/// Payload for export-training responses.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportTrainingResult {
    /// Number of examples exported
    pub examples_exported: usize,

    /// Total bytes written to file
    pub bytes_written: usize,

    /// Average quality score of exported examples
    pub average_quality: f64,

    /// Export duration in milliseconds
    pub duration_ms: u64,
}

// ============================================================================
// Health Check Types (for detecting frozen worker)
// ============================================================================

/// Ping request payload (empty - just proves worker is alive)
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingPayload {}

/// Ping result - includes uptime and stats
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PingResult {
    pub uptime_ms: u64,
    pub connections_total: u64,
    pub requests_processed: u64,
    pub examples_processed: u64,
}
