//! IPC protocol types — request/response surface shared by every command.
//!
//! Split out of `ipc/mod.rs` (was 1288 LOC single-file dir, parallel-dir
//! smell flagged in claude-tab-1's audit broadcast 2026-05-18 19:40Z).
//! Per Joel's zero-users no-migration-ceremony directive, no separate
//! re-export ceremony — `ipc/mod.rs` `pub use`s these types so existing
//! call sites resolve unchanged.

use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

/// Inbox message for IPC (mirrors InboxMessage but with string UUIDs for
/// JSON transport).
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
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

impl InboxMessageRequest {
    /// Convert this JSON-transport request (string UUIDs, string enums) into the
    /// domain [`InboxMessage`](crate::persona::InboxMessage). The single
    /// wire→domain conversion for inbox messages — reused by every typed command
    /// that accepts an inbox message off the wire.
    ///
    /// Fails loud on a malformed UUID or an unknown `sender_type` (the legacy
    /// free-fn form silently dropped an unparseable `voice_session_id`; this
    /// surfaces it instead, per the no-silent-fallback rule).
    pub fn to_inbox_message(&self) -> Result<crate::persona::InboxMessage, String> {
        use crate::persona::{InboxMessage, Modality, SenderType};

        let sender_type = match self.sender_type.as_str() {
            "human" => SenderType::Human,
            "persona" => SenderType::Persona,
            "agent" => SenderType::Agent,
            "system" => SenderType::System,
            other => return Err(format!("Invalid sender_type: {other}")),
        };

        Ok(InboxMessage {
            id: Uuid::parse_str(&self.id).map_err(|e| format!("invalid id: {e}"))?,
            room_id: Uuid::parse_str(&self.room_id).map_err(|e| format!("invalid room_id: {e}"))?,
            sender_id: Uuid::parse_str(&self.sender_id)
                .map_err(|e| format!("invalid sender_id: {e}"))?,
            sender_name: self.sender_name.clone(),
            sender_type,
            content: self.content.clone(),
            timestamp: self.timestamp,
            priority: self.priority,
            source_modality: self.source_modality.as_deref().map(|m| match m {
                "voice" => Modality::Voice,
                _ => Modality::Chat,
            }),
            voice_session_id: self
                .voice_session_id
                .as_deref()
                .map(|s| Uuid::parse_str(s).map_err(|e| format!("invalid voice_session_id: {e}")))
                .transpose()?,
        })
    }
}

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
