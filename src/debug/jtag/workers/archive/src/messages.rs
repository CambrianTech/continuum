/// Message Protocol for ArchiveWorker
///
/// TypeScript → Rust: Archive tasks
/// Rust → TypeScript: Command execution requests
use serde::{Deserialize, Serialize};

// ============================================================================
// TypeScript → Rust Messages (Archive Tasks)
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "command")]
pub enum ArchiveRequest {
    /// Queue an archive task
    #[serde(rename = "archive")]
    Archive {
        task_id: String,
        collection: String,
        source_handle: String,
        dest_handle: String,
        max_rows: usize,
        batch_size: usize,
    },

    /// Ping for health check
    #[serde(rename = "ping")]
    Ping,

    /// Get queue status
    #[serde(rename = "status")]
    Status,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum ArchiveResponse {
    /// Task queued successfully
    #[serde(rename = "queued")]
    Queued {
        task_id: String,
        queue_position: usize,
    },

    /// Task completed
    #[serde(rename = "complete")]
    Complete {
        task_id: String,
        rows_archived: usize,
        duration_ms: u64,
    },

    /// Task failed
    #[serde(rename = "error")]
    Error {
        task_id: String,
        error: String,
    },

    /// Ping response
    #[serde(rename = "pong")]
    Pong {
        uptime_seconds: u64,
        queue_size: usize,
        tasks_completed: usize,
    },

    /// Status response
    #[serde(rename = "status")]
    Status {
        queue_size: usize,
        active_tasks: usize,
        completed_tasks: usize,
    },
}

// ============================================================================
// Rust → TypeScript Messages (Command Execution)
// ============================================================================

/// Request from Rust to execute a command via TypeScript Commands.execute()
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandExecutionRequest {
    pub request_id: String,
    pub command: String,
    pub params: serde_json::Value,
}

/// Response from TypeScript with command result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandExecutionResponse {
    pub request_id: String,
    pub success: bool,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
}

// ============================================================================
// Archive Task (Internal)
// ============================================================================

#[derive(Debug, Clone)]
pub struct ArchiveTask {
    pub task_id: String,
    pub collection: String,
    pub source_handle: String,
    pub dest_handle: String,
    pub max_rows: usize,
    pub batch_size: usize,
}
