//! The core's own chat store, read as rooms' rows: the scroll-back the chat history command pages.
//!
//! A resident's prompt no longer tops her conversation up from here: her thread of the room is
//! seeded from the airc room, the one record, and a shallow airc page after a reboot (#249) is
//! airc's to answer, not a second copy's.

use std::sync::Arc;

use serde_json::{json, Value};
use uuid::Uuid;

use crate::runtime::command_executor::CommandExecutor;
use crate::runtime::late_bound::LateBound;

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
    pub media: Vec<crate::cognition::conversation::channel_items::MediaItemRequest>,
}

/// Which page of a room's conversation to read. A cursor names a message the
/// reader holds; the page is the `limit` rows adjacent to it, keyed by
/// `(timestamp, id)` so pages neither overlap nor skip (chat/poll).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoomPage {
    /// The newest rows.
    Newest,
    /// The rows just older than this message: scroll-back.
    Before(Uuid),
    /// The rows just newer than this message: paging back down toward the present.
    After(Uuid),
}

/// The rows of a room's conversation on `page`, from the core's OWN chat
/// store — the durable truth the digest's history top-up and the humans read.
/// Live 2026-09-04: the daemon's page for the busiest room came back EMPTY
/// for every citizen (its ring was flooded by board System events, none of
/// which pass the conversation-kind filter), so the store-backed catch-up
/// forwarded nothing for the room they were working in.
pub async fn room_rows(room: Uuid, page: RoomPage, limit: usize) -> Result<Vec<RoomRow>, String> {
    let Some(executor) = EXECUTOR.cloned() else {
        return Err("durable_history: executor not yet installed (early boot)".to_string());
    };
    let result = executor
        .execute_json(
            "chat/poll",
            match page {
                RoomPage::Newest => json!({ "roomId": room, "limit": limit }),
                RoomPage::Before(anchor) => json!({ "roomId": room, "beforeMessageId": anchor, "limit": limit }),
                RoomPage::After(anchor) => json!({ "roomId": room, "afterMessageId": anchor, "limit": limit }),
            },
        )
        .await
        .map_err(|e| format!("durable_history: chat/poll failed: {e}"))?;
    /// What `chat/poll` answers: its rows, decoded once at this boundary.
    #[derive(serde::Deserialize)]
    struct Polled {
        messages: Vec<Value>,
    }
    let Polled { messages } = serde_json::from_value(result).map_err(|e| format!("durable_history: chat/poll result: {e}"))?; // boundary: chat/poll's JSON answer, typed here once
    let messages = &messages;
    // Corrupt media is a read error, not permission to silently omit a whole
    // message or invent a text-only version of an image-bearing turn.
    for message in messages {
        if let Some(media) = message.get("content").and_then(|c| c.get("media")) {
            serde_json::from_value::<Vec<crate::cognition::conversation::channel_items::MediaItemRequest>>(media.clone()) // boundary: validate media from the durable chat storage JSON format.
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

/// A chat row as a transcript event: kind Message, a text body (media as the airc
/// media headers), wall time, the row id as the event id, lamport 0. The inbound
/// seam's tests build their room events with it; no production path re-admits rows.
#[cfg(test)]
pub(crate) fn event_from_row(room: Uuid, row: crate::persona::durable_history::RoomRow) -> airc_lib::TranscriptEvent {
    use airc_core::{
        Body, ClientId, EventId, Headers, MentionTarget, PeerId, RoomId, TranscriptKind,
    };
    let room_id = RoomId::from_uuid(room);
    let (body, headers) = if row.media.is_empty() {
        (Body::text(&row.text), Headers::default())
    } else {

        let mut headers = Headers::default();
        headers.insert(
            airc_protocol::HEADER_FORGE_BODY_HINT.to_string(),
            crate::routing::airc_realtime_wire::CONTINUUM_BODY_HINT.to_string(),
        );
        (Body::Json(crate::cognition::conversation::room_event::chat_transcript_envelope(
            row.id,
            &crate::cognition::conversation::room_event::ChatTranscriptLine {
                room_id: room,
                sender_id: row.sender,
                text: &row.text,
                media: &row.media,
                reply_to_id: None,
            },
            row.occurred_at_ms,
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
