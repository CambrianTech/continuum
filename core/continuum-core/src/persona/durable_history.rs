//! DurableRoomHistory — the mind-side read of the durable room transcript (#249).
//!
//! A core reboot restarts every persona's embedded airc runtime, whose transcript
//! log then holds only events since ITS boot — and #242's cursor semantics
//! deliberately never replay old log entries as fresh perception. Net effect
//! (glass-boxed 2026-07-30): each mind's visible conversation collapsed to ONE
//! message post-boot, and the room degenerated into a greeting chorus — every
//! persona mirroring the only utterance it could see.
//!
//! The durable chat store (#140) already holds the room's history. This trait is
//! the narrow read the wake path uses to TOP UP a shallow live window with that
//! durable tail — presented as prior conversation context (grounding), never as
//! fresh wake triggers, so #242's no-replay contract holds. It is the mind-side
//! sibling of the web's post-cursor hydration and of #265's speech-ring seeding:
//! one durable transcript, every consumer hydrates from it.

use std::sync::Arc;

use async_trait::async_trait;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::runtime::command_executor::CommandExecutor;
use crate::runtime::late_bound::LateBound;

/// One durable transcript line, minimally shaped for hydration: identity for
/// dedup against live events, sender for role attribution, text for content.
#[derive(Debug, Clone)]
pub struct HydratedLine {
    pub message_id: String,
    pub sender_id: String,
    pub text: String,
    pub media: Vec<super::channel_items::MediaItemRequest>,
}

/// Read the latest lines of a room's durable transcript, chronological
/// (oldest first). Implementations are read-only; hydration must never write.
#[async_trait]
pub trait DurableRoomHistory: Send + Sync {
    async fn room_tail(&self, room: Uuid, limit: usize) -> Result<Vec<HydratedLine>, String>;
}

/// Substrate-wide executor slot for the production reader — same late-bound
/// pattern as `training_producer::EXECUTOR`, installed from the ipc bootstrap
/// once the command executor exists. Before installation `room_tail` fails
/// loud-but-recoverable (the caller logs and serves the shallow window; a
/// missing executor at early boot is a not-yet, not a bug).
static EXECUTOR: LateBound<CommandExecutor> = LateBound::new("durable_history::executor");

pub fn install_executor(executor: Arc<CommandExecutor>) {
    EXECUTOR.install(executor);
}

/// One row of a room's durable conversation, in the shape the inbound seam
/// needs to re-admit it as a turn: the row id IS the airc event id (the chat
/// store is the projection of the wire), the sender is the peer, and the
/// timestamp is wall time.
#[derive(Debug, Clone)]
pub struct RoomRow {
    pub id: Uuid,
    pub sender: Uuid,
    pub occurred_at_ms: u64,
    pub text: String,
    pub media: Vec<super::channel_items::MediaItemRequest>,
}

/// The newest `limit` rows of a room's conversation from the core's OWN chat
/// store — the durable truth the digest's history top-up and the humans read.
/// Live 2026-09-04: the daemon's page for the busiest room came back EMPTY
/// for every citizen (its ring was flooded by board System events, none of
/// which pass the conversation-kind filter), so the store-backed catch-up
/// forwarded nothing for the room they were working in.
pub async fn room_rows(room: Uuid, limit: usize) -> Result<Vec<RoomRow>, String> {
    let Some(executor) = EXECUTOR.cloned() else {
        return Err("durable_history: executor not yet installed (early boot)".to_string());
    };
    let result = executor
        .execute_json("chat/poll", json!({ "roomId": room.to_string(), "limit": limit }))
        .await
        .map_err(|e| format!("durable_history: chat/poll failed: {e}"))?;
    let messages = result
        .get("messages")
        .and_then(Value::as_array)
        .ok_or_else(|| "durable_history: chat/poll result missing `messages`".to_string())?;
    // Corrupt media is a read error, not permission to silently omit a whole
    // message or invent a text-only version of an image-bearing turn.
    for message in messages {
        if let Some(media) = message.get("content").and_then(|c| c.get("media")) {
            serde_json::from_value::<Vec<super::channel_items::MediaItemRequest>>(media.clone()) // boundary: validate media from the durable chat storage JSON format.
                .map_err(|error| format!("durable_history: invalid media references: {error}"))?;
        }
    }
    Ok(messages
        .iter()
        .filter_map(|m| {
            let id = Uuid::parse_str(m.get("id")?.as_str()?).ok()?;
            let sender = Uuid::parse_str(m.get("senderId")?.as_str()?).ok()?;
            let stamp = m.get("timestamp").and_then(Value::as_str)?;
            let occurred_at_ms = chrono::DateTime::parse_from_rfc3339(stamp)
                .ok()?
                .timestamp_millis()
                .max(0) as u64;
            let text = m.get("content")?.get("text")?.as_str()?.to_string();
            let media = m.get("content")?.get("media")
                .map(|value| serde_json::from_value(value.clone())) // boundary: decode durable chat storage JSON into typed room history.
                .transpose().ok()?.unwrap_or_default(); // Absent media denotes a legacy text-only stored row; malformed present media was rejected above.
            Some(RoomRow { id, sender, occurred_at_ms, text, media })
        })
        .collect())
}

/// Production reader over the durable chat store, via the SAME `chat/poll`
/// command every other consumer uses — one read path, no parallel query stack.
pub struct ChatStoreHistory;

#[async_trait]
impl DurableRoomHistory for ChatStoreHistory {
    async fn room_tail(&self, room: Uuid, limit: usize) -> Result<Vec<HydratedLine>, String> {
        Ok(room_rows(room, limit).await?.into_iter().map(|row| HydratedLine {
            message_id: row.id.to_string(), sender_id: row.sender.to_string(),
            text: row.text, media: row.media,
        }).collect())
    }
}

/// A durable chat row as the transcript event the inbound seam admits: kind
/// Message, a text body, wall time, the row id as the event id. Lamport is 0 —
/// the loop head judges staleness by event id, never by this clock.
pub(crate) fn event_from_row(room: Uuid, row: crate::persona::durable_history::RoomRow) -> airc_lib::TranscriptEvent {
    use airc_core::{
        Body, ClientId, EventId, Headers, MentionTarget, PeerId, RoomId, TranscriptKind,
    };
    let room_id = RoomId::from_uuid(room);
    let (body, headers) = if row.media.is_empty() {
        (Body::text(&row.text), Headers::default())
    } else {
        let params = crate::modules::chat::types::ChatSendParams {
            room_id: room, sender_id: row.sender, text: row.text.clone(),
            media: row.media, reply_to_id: None,
        };
        let mut headers = Headers::default();
        headers.insert(
            airc_protocol::HEADER_FORGE_BODY_HINT.to_string(),
            crate::airc::realtime_wire::CONTINUUM_BODY_HINT.to_string(),
        );
        (Body::Json(crate::modules::chat::ChatModule::transcript_envelope(
            row.id, &params, row.occurred_at_ms,
        )), headers)
    };
    airc_lib::TranscriptEvent {
        event_id: EventId::from_uuid(row.id),
        room_id,
        peer_id: PeerId::from_uuid(row.sender),
        client_id: ClientId::new(),
        kind: TranscriptKind::Message,
        occurred_at_ms: row.occurred_at_ms,
        lamport: 0,
        target: MentionTarget::Room(room_id),
        headers,
        body: Some(body),
        attachment: None,
        receipt: None,
        metadata: serde_json::Value::Null,
    }
}
