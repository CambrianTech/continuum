//! The JSON-transport inbox message (string UUIDs) the `cognition/enqueue-message` and
//! `cognition/admit-inbox-message` commands accept: moved down from core's `ipc/protocol.rs` with the
//! commands that parse it (plan phase 2); `ipc` re-exports it under the old path.

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
    /// domain [`InboxMessage`](crate::cognition::conversation::types::InboxMessage). The single
    /// wire→domain conversion for inbox messages — reused by every typed command
    /// that accepts an inbox message off the wire.
    ///
    /// Fails loud on a malformed UUID or an unknown `sender_type` (the legacy
    /// free-fn form silently dropped an unparseable `voice_session_id`; this
    /// surfaces it instead, per the no-silent-fallback rule).
    pub fn to_inbox_message(&self) -> Result<crate::cognition::conversation::types::InboxMessage, String> {
        use crate::cognition::conversation::types::{InboxMessage, Modality, SenderType};

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
