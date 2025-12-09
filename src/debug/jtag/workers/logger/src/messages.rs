/// Worker IPC Protocol - Rust Message Types
///
/// This mirrors shared/ipc/WorkerMessages.ts from the TypeScript side.
/// Keep in sync with TypeScript types using serde for JSON serialization.

use serde::{Deserialize, Serialize};

// ============================================================================
// Generic Message Envelope
// ============================================================================

/// Base message structure for all worker communication.
/// Generic over payload type T for type safety.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerMessage<T> {
    pub id: String,           // UUID for correlation
    pub r#type: String,       // Message type (use r#type to avoid keyword)
    pub timestamp: String,    // ISO 8601
    pub payload: T,           // Generic payload
}

/// Request message from TypeScript daemon to Rust worker.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerRequest<T> {
    pub id: String,
    pub r#type: String,
    pub timestamp: String,
    pub payload: T,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,
}

/// Response message from Rust worker to TypeScript daemon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkerResponse<T> {
    pub id: String,
    pub r#type: String,
    pub timestamp: String,
    pub payload: T,
    pub request_id: String,
    pub success: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type: Option<ErrorType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
}

/// Standard error types for worker operations.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ErrorType {
    Validation,
    Timeout,
    Internal,
    NotFound,
}

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
// Helper Functions
// ============================================================================

impl<T> WorkerResponse<T> {
    /// Create a success response.
    pub fn success(request_id: String, r#type: String, payload: T) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            r#type,
            timestamp: chrono::Utc::now().to_rfc3339(),
            payload,
            request_id,
            success: true,
            error: None,
            error_type: None,
            stack: None,
        }
    }

    /// Create an error response.
    pub fn error(request_id: String, r#type: String, payload: T, error: String, error_type: ErrorType) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            r#type,
            timestamp: chrono::Utc::now().to_rfc3339(),
            payload,
            request_id,
            success: false,
            error: Some(error),
            error_type: Some(error_type),
            stack: None,
        }
    }
}
