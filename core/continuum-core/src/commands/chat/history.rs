//! `chat/history` — a room's DURABLE transcript, read from the airc daemon's
//! own store (the `InboxRequest` bounded query), as a typed self-routing
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand).
//!
//! ## Why this verb exists (the missing read half, found 2026-08-31)
//!
//! The daemon has always been the durable record of a room: every chat line,
//! every radiated 💭 intent and ⚙ act receipt lands in its per-channel
//! transcript with replay cursors. But the ONLY durable read the interface
//! had was `chat/poll` over the operator `chat_messages` collection — which
//! citizens' airc-published speech never touches. Result, measured live: a
//! solve room with 67 radiated receipts answered `chat/poll` with ZERO rows,
//! so clicking into a run showed only the live tail and none of the story.
//! The store existed; the verb didn't ([[when-the-substrate-lacks-a-verb-build-the-verb]]).
//!
//! `chat/history` asks the daemon for the newest `limit` events on the room's
//! channel (`since: None` = most recent window — the hydrator's shape) and
//! decodes them through the SAME seams the live bridge uses
//! ([`decode_wire_event`] + the chat-transcript recovery in `realtime_wire`),
//! so history and live rows can never disagree about what a message is.

use crate::airc::realtime_wire::{chat_transcript_message, envelope_from_event, is_stream_chunk};
use crate::airc::discover_airc_socket;
use crate::sdk_codegen::CommandError;
use airc_ipc::{DaemonClient, InboxRequest};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

#[derive(Debug, Clone, Deserialize, Serialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/chat/ChatHistoryParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ChatHistoryParams {
    /// The room whose transcript to read.
    #[ts(type = "string")]
    pub room_id: uuid::Uuid,
    /// Max messages to return (newest window). Defaults to 50.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub limit: Option<u32>,
}

/// One durable transcript row — the wire twin of the live `chat:posted`
/// fold, deliberately shaped like `chat/poll`'s rows so the client's
/// history projection consumes either without a second parser.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/chat/ChatHistoryMessage.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ChatHistoryMessage {
    /// The event's durable id (the daemon's event id — stable across reads).
    pub id: String,
    pub sender_id: String,
    /// Message text (chat line, 💭 intent, or ⚙ receipt block — the
    /// transcript carries the WORK, not just the words).
    pub text: String,
    /// Unix ms.
    #[ts(type = "number")]
    pub timestamp: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/chat/ChatHistoryResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct ChatHistoryResult {
    pub count: u32,
    /// Oldest → newest within the returned window.
    pub messages: Vec<ChatHistoryMessage>,
}

crate::action_command! {
    /// Read a room's durable transcript from the airc daemon's store — chat lines AND
    /// radiated work receipts (💭/⚙), the full story of the activity. `chat/poll` reads
    /// only the operator message collection; THIS is how a solve room's history exists.
    pub struct ChatHistory;
    name: "chat/history",
    access: AiSafe,
    params: ChatHistoryParams,
    output: ChatHistoryResult,
    run(_this, _ctx, p) => {
        let socket = discover_airc_socket()
            .await
            .map_err(|e| CommandError::Internal(format!(
                "chat/history needs the airc daemon socket and discovery failed: {e}"
            )))?;
        let limit = p.limit.unwrap_or(50).min(500) as usize;
        let response = DaemonClient::new(socket)
            .inbox(InboxRequest {
                since: None,
                channel: Some(airc_core::RoomId::from_uuid(p.room_id)),
                limit: Some(limit),
                kinds: None,
            })
            .await
            .map_err(|e| CommandError::Internal(format!(
                "airc daemon inbox read failed for room {}: {e}",
                p.room_id
            )))?;
        let mut messages = Vec::with_capacity(response.envelopes.len());
        for envelope_bytes in response.envelopes {
            let Ok(event) = airc_lib::decode_wire_event(envelope_bytes) else {
                continue; // malformed rows must not poison the read
            };
            // Stream chunks are typing-indicator traffic — the settled utterance
            // arrives separately via say(). Same rule as the live decoder; without
            // it history renders one row per token fragment (seen live 2026-08-31).
            if is_stream_chunk(&event) {
                continue;
            }
            // Same recovery seams as the live bridge: prefer the plain text
            // body (receipts/radiations), else the chat_transcript schema
            // (chat/send lines) — one definition of "a message".
            let recovered = event
                .body
                .as_ref()
                .and_then(|b| b.as_text())
                .map(|t| (event.peer_id.as_uuid(), t.to_string()))
                .or_else(|| {
                    envelope_from_event(&event)
                        .ok()
                        .flatten()
                        .and_then(|env| chat_transcript_message(&env, event.peer_id.as_uuid()))
                });
            let Some((sender, text)) = recovered else {
                continue; // presence beats / cards / non-message events
            };
            messages.push(ChatHistoryMessage {
                id: event.event_id.as_uuid().to_string(),
                sender_id: sender.to_string(),
                text,
                timestamp: event.occurred_at_ms as i64,
            });
        }
        Ok(ChatHistoryResult { count: messages.len() as u32, messages })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::{AccessLevel, ActionCommand};

    // what this catches: the routing key + access tier of the ONE durable-transcript
    // read verb. Renaming the path orphans every hydrator; narrowing access re-creates
    // the everyone-offline class (a read-only room transcript is AiSafe by definition).
    #[test]
    fn history_is_aisafe_under_its_wire_name() {
        assert_eq!(ChatHistory::NAME, "chat/history");
        assert_eq!(ChatHistory::ACCESS, AccessLevel::AiSafe);
    }
}
