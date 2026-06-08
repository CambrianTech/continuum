/// JTAGProtocol - Universal Packet Format for JTAG System
///
/// This is THE universal packet format used everywhere in JTAG:
/// - Commands (Commands.execute)
/// - Events (Events.emit/subscribe)
/// - Worker IPC (Rust ↔ TypeScript)
/// - Daemon communication
///
/// DESIGN PRINCIPLE:
/// Rust workers should feel IDENTICAL to TypeScript workers.
/// Promises and events flow the same way regardless of implementation language.
///
/// MIRRORED IN TYPESCRIPT: shared/ipc/JTAGProtocol.ts
/// Keep these two files in sync!
use serde::{Deserialize, Serialize};

// ============================================================================
// Base Message Types (Generic Envelope)
// ============================================================================

/// Base request message - sent TO a worker/daemon/command handler.
/// Generic over payload type T for type safety.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JTAGRequest<T> {
    /// Unique identifier for this request (UUID v4)
    pub id: String,

    /// Message type identifier (e.g., 'write-log', 'ping', 'command:execute')
    #[serde(rename = "type")]
    pub r#type: String,

    /// ISO 8601 timestamp when request was created
    pub timestamp: String,

    /// The actual request data (type-safe via generics)
    pub payload: T,

    /// Optional user context (for auth/logging)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_id: Option<String>,

    /// Optional session context (for state management)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
}

/// Base response message - sent FROM a worker/daemon/command handler.
/// Generic over payload type T for type safety.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JTAGResponse<T> {
    /// Unique identifier for this response (UUID v4)
    pub id: String,

    /// Message type identifier (matches request type)
    #[serde(rename = "type")]
    pub r#type: String,

    /// ISO 8601 timestamp when response was created
    pub timestamp: String,

    /// The actual response data (type-safe via generics)
    pub payload: T,

    /// Links back to the request that triggered this response
    pub request_id: String,

    /// Whether the operation succeeded
    pub success: bool,

    /// Error message if success=false
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,

    /// Error category for programmatic handling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error_type: Option<JTAGErrorType>,

    /// Stack trace for debugging (only in development)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stack: Option<String>,
}

/// Standard error types for JTAG operations.
/// Consistent across all workers/commands/events.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum JTAGErrorType {
    /// Invalid input data
    Validation,
    /// Operation timed out
    Timeout,
    /// Internal error (bug or unexpected state)
    Internal,
    /// Resource not found
    NotFound,
    /// Permission denied
    Unauthorized,
    /// Service unavailable (worker down, etc.)
    Unavailable,
}

// ============================================================================
// Helper Functions
// ============================================================================

impl<T> JTAGResponse<T> {
    /// Create a success response from a request.
    #[allow(dead_code)]
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

    /// Create an error response from a request.
    #[allow(dead_code)]
    pub fn error(
        request_id: String,
        r#type: String,
        payload: T,
        error: String,
        error_type: JTAGErrorType,
    ) -> Self {
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
