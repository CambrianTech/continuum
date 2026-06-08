//! IPC protocol types — request/response surface shared by every command.
//!
//! Split out of `ipc/mod.rs` (was 1288 LOC single-file dir, parallel-dir
//! smell flagged in claude-tab-1's audit broadcast 2026-05-18 19:40Z).
//! Per Joel's zero-users no-migration-ceremony directive, no separate
//! re-export ceremony — `ipc/mod.rs` `pub use`s these types so existing
//! call sites resolve unchanged.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Inbox message for IPC (mirrors InboxMessage but with string UUIDs for
/// JSON transport).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ipc/InboxMessageRequest.ts"
)]
pub struct InboxMessageRequest {
    pub id: String,
    pub room_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub sender_type: String, // "human", "persona", "agent", "system"
    pub content: String,
    /// Timestamp in milliseconds (fits in JS number, max safe ~9 quadrillion)
    #[ts(type = "number")]
    pub timestamp: u64,
    pub priority: f32,
    #[ts(optional)]
    pub source_modality: Option<String>, // "chat", "voice"
    #[ts(optional)]
    pub voice_session_id: Option<String>,
}

// NOTE: InboxMessageRequest is used for ts-rs TypeScript generation.
// The to_inbox_message() method was removed when migrating to CognitionModule.
// See modules/cognition.rs for the parsing logic.

// All commands route through ServiceModule implementations in src/modules/.

/// Wire response for every command. `request_id` round-trips to let
/// the TS client correlate concurrent requests.
#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct Response {
    pub(crate) success: bool,
    pub(crate) result: Option<serde_json::Value>,
    pub(crate) error: Option<String>,
    #[serde(rename = "requestId")]
    pub(crate) request_id: Option<u64>,
}

impl Response {
    pub(crate) fn success(result: serde_json::Value) -> Self {
        Self {
            success: true,
            result: Some(result),
            error: None,
            request_id: None,
        }
    }

    pub(crate) fn error(msg: String) -> Self {
        Self {
            success: false,
            result: None,
            error: Some(msg),
            request_id: None,
        }
    }

    pub(crate) fn with_request_id(mut self, request_id: Option<u64>) -> Self {
        self.request_id = request_id;
        self
    }
}
