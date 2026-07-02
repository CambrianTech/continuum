//! Inbound daemon attach stream for Continuum's event bus.
//!
//! This is the runtime half of AIRC realtime integration: the daemon owns
//! transport, trust, replay, and live delivery; Continuum subscribes through
//! typed IPC and republishes valid EventBridge envelopes into MessageBus.

use std::path::PathBuf;
use std::sync::Arc;

use airc_core::{RoomId, TranscriptKind};
use airc_ipc::{codec::read_frame, AttachRequest, AttachStart, DaemonClient, Response};
use airc_lib::decode_wire_event;
use tracing::warn;

use crate::airc::realtime_wire::{bus_event_from_envelope, envelope_from_event};
use crate::ipc::positron_source::CHAT_POSTED;
use crate::runtime::MessageBus;

pub fn spawn_daemon_attach(
    socket_path: PathBuf,
    channel: RoomId,
    bus: Arc<MessageBus>,
    runtime: &tokio::runtime::Handle,
) {
    runtime.spawn(async move {
        if let Err(error) = run_daemon_attach(socket_path, channel, bus).await {
            warn!("AIRC daemon attach stream stopped: {error}");
        }
    });
}

pub async fn run_daemon_attach(
    socket_path: PathBuf,
    channel: RoomId,
    bus: Arc<MessageBus>,
) -> Result<(), String> {
    let client = DaemonClient::new(socket_path);
    // Owner-core model (airc-daemon/src/server.rs:274): the router
    // subscribes per channel — no global fan-out table. AttachRequest
    // MUST carry `channel: Some(_)` or the daemon responds
    // `attach requires a channel in the owner-core model`. continuum
    // discovers the scope's default channel at boot via
    // `crate::airc::discover_default_channel` (parses `airc room`).
    // Multi-room scopes will spawn one daemon_attach task per channel
    // they care about — single-attach today, per-room fan-out as a
    // follow-up when continuum rooms become first-class.
    // airc#1222 bump: AttachRequest dropped Default for explicit
    // builders. The prior `..default()` was `from_now: false` —
    // FromTranscriptStart per airc-ipc's own "legacy meaning" doc — so
    // preserve full-backlog semantics rather than silently switching to
    // live-only (which would skip events on attach).
    let mut stream = client
        .attach(AttachRequest::new(channel, AttachStart::FromTranscriptStart))
        .await
        .map_err(|error| format!("failed to attach to airc daemon: {error}"))?;

    loop {
        let response = read_frame::<_, Response>(&mut stream)
            .await
            .map_err(|error| format!("failed to read airc daemon event: {error}"))?;
        let Some(response) = response else {
            return Ok(());
        };
        handle_attach_response(response, &bus).await?;
    }
}

pub async fn handle_attach_response(response: Response, bus: &MessageBus) -> Result<(), String> {
    match response {
        Response::Ok => Ok(()),
        // v5 owner-core schema (task #82): the daemon now streams raw
        // airc-wire envelope bytes; `airc_lib::decode_wire_event` is
        // the canonical helper that decodes + projects to a
        // TranscriptEvent. A malformed buffer is logged + skipped (the
        // live stream shouldn't die because one event failed to parse).
        Response::Event { envelope } => match decode_wire_event(envelope) {
            Ok(event) => publish_transcript_event(&event, bus).await,
            Err(error) => {
                warn!("Skipping malformed airc daemon event: {error}");
                Ok(())
            }
        },
        Response::Error { message } => Err(message),
        // Wildcard for non-event responses the daemon may emit on the
        // attach stream (Pong, Status, Inbox, Publish, Peers, cursor
        // advances, future variants). v5 dropped ResolveWire; future
        // variants come/go on the airc side without breaking continuum
        // — same `non_exhaustive`-style posture the airc-cli monitor
        // uses against the same enum.
        _ => Ok(()),
    }
}

pub async fn publish_transcript_event(
    event: &airc_core::TranscriptEvent,
    bus: &MessageBus,
) -> Result<(), String> {
    let envelope = match envelope_from_event(event) {
        Ok(Some(envelope)) => envelope,
        // Not a Continuum EventBridge envelope. Before dropping it, try
        // the thin chat projection: a plain airc chat message projects
        // to `chat:posted` for the positron renderer (task #84). Any
        // other kind (receipts, presence transitions, lifecycle) is not
        // ours here → skip. This is classification, not a fallback —
        // a non-message never fabricates a chat event.
        Ok(None) => {
            if let Some((name, payload)) = chat_posted_from_message(event) {
                bus.publish_async_only(name, payload);
            }
            return Ok(());
        }
        Err(error) => {
            warn!("Ignoring malformed Continuum AIRC realtime event: {error}");
            return Ok(());
        }
    };
    let Some(bus_event) = bus_event_from_envelope(&envelope) else {
        return Ok(());
    };
    bus.publish_async_only(&bus_event.name, bus_event.payload);
    Ok(())
}

/// Project a plain airc chat message into the THIN `chat:posted` bus
/// payload the positron chat projection consumes (`AircChatPosted` in
/// `ipc/positron_source.rs`). Returns `None` for any non-message event
/// or a message with no text body — those are not chat rows.
///
/// The payload is deliberately identity-free: `sender_id` is airc's
/// authoritative `peer_id`, and the projection resolves name / kind /
/// integrations downstream from the roster (presence cards). Identity
/// is a presence fact, never a message fact — see
/// `airc-to-positron-chat-projection`. airc's `event_id` IS the stable
/// `message_id` (same event → same id across replay + peers).
fn chat_posted_from_message(
    event: &airc_core::TranscriptEvent,
) -> Option<(&'static str, serde_json::Value)> {
    if event.kind != TranscriptKind::Message {
        return None;
    }
    let content = event.body.as_ref()?.as_text()?;
    let payload = serde_json::json!({
        "messageId": event.event_id.as_uuid(),
        "roomId": event.room_id.as_uuid(),
        "senderId": event.peer_id.as_uuid(),
        "content": content,
        "timestamp": event.occurred_at_ms,
    });
    Some((CHAT_POSTED, payload))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airc::realtime::{
        AircRealtimeEnvelope, AircRealtimePayload, AircRealtimePayloadRef, AircRealtimeSchema,
    };
    use crate::airc::realtime_wire::headers_for_envelope;
    use airc_core::{
        Body, ClientId, EventId, MentionTarget, PeerId, RoomId, TranscriptEvent, TranscriptKind,
    };
    use serde_json::json;
    use tokio::time::{timeout, Duration};
    use uuid::Uuid;

    fn transcript_event(body: Option<Body>, headers: airc_core::Headers) -> TranscriptEvent {
        TranscriptEvent {
            event_id: EventId::from_u128(1),
            room_id: RoomId::from_u128(2),
            peer_id: PeerId::from_u128(3),
            client_id: ClientId::from_u128(4),
            kind: TranscriptKind::Message,
            occurred_at_ms: 100,
            lamport: 1,
            target: MentionTarget::All,
            headers,
            body,
            attachment: None,
            receipt: None,
            metadata: serde_json::Value::Null,
        }
    }

    fn event_bridge_envelope() -> AircRealtimeEnvelope {
        AircRealtimeEnvelope::new(
            "evt-1".to_string(),
            Uuid::from_u128(2),
            "continuum-peer".to_string(),
            100,
            AircRealtimePayload::ExistingSchema {
                payload: AircRealtimePayloadRef::inline(
                    AircRealtimeSchema::EventBridgePayload,
                    json!({
                        "type": "event-bridge",
                        "eventName": "persona:ready",
                        "data": { "personaId": "helper-ai" }
                    }),
                ),
            },
        )
    }

    #[tokio::test]
    async fn valid_continuum_event_reaches_message_bus() {
        let bus = MessageBus::new();
        let mut receiver = bus.receiver();
        let envelope = event_bridge_envelope();
        let event = transcript_event(
            Some(Body::Json(serde_json::to_value(&envelope).unwrap())),
            headers_for_envelope(&envelope),
        );

        publish_transcript_event(&event, &bus).await.unwrap();

        let delivered = timeout(Duration::from_millis(200), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(delivered.name, "persona:ready");
        assert_eq!(delivered.payload["data"]["personaId"], "helper-ai");
    }

    #[tokio::test]
    async fn non_continuum_body_is_ignored() {
        let bus = MessageBus::new();
        let mut receiver = bus.receiver();
        let event = transcript_event(
            Some(Body::Json(json!({"eventName": "ignored"}))),
            Default::default(),
        );

        publish_transcript_event(&event, &bus).await.unwrap();

        assert!(timeout(Duration::from_millis(20), receiver.recv())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn plain_chat_message_projects_thin_chat_posted() {
        // what this catches: the thin emitter arm (task #84). A plain
        // airc chat message (Message kind + text body, NO continuum
        // envelope header) must project to a `chat:posted` bus event
        // carrying ONLY the identity-free message facts — airc's
        // event_id as messageId, peer_id as senderId, and the text.
        // Identity (name/kind/integrations) is resolved downstream from
        // the roster, never carried here. A regression that reverts the
        // Ok(None) arm to a bare `return Ok(())` drops every human/peer
        // chat line from the positron renderer.
        let bus = MessageBus::new();
        let mut receiver = bus.receiver();
        let event = transcript_event(Some(Body::text("hello room")), Default::default());

        publish_transcript_event(&event, &bus).await.unwrap();

        let delivered = timeout(Duration::from_millis(200), receiver.recv())
            .await
            .unwrap()
            .unwrap();
        assert_eq!(delivered.name, "chat:posted");
        assert_eq!(delivered.payload["content"], "hello room");
        // event_id (1) is the stable messageId; peer_id (3) the sender.
        assert_eq!(
            delivered.payload["messageId"],
            Uuid::from_u128(1).to_string()
        );
        assert_eq!(delivered.payload["roomId"], Uuid::from_u128(2).to_string());
        assert_eq!(
            delivered.payload["senderId"],
            Uuid::from_u128(3).to_string()
        );
        assert_eq!(delivered.payload["timestamp"], 100);
        // identity-free: the thin payload must NOT invent these.
        assert!(delivered.payload.get("senderName").is_none());
        assert!(delivered.payload.get("senderKind").is_none());
    }

    #[tokio::test]
    async fn non_message_kind_does_not_project_chat() {
        // what this catches: classification, not fallback. A non-Message
        // transcript event (here a Receipt) with no continuum envelope
        // must be dropped silently — it must NEVER fabricate a
        // `chat:posted` with an empty body ([[fallbacks-are-illegal-fail-loud]]).
        let bus = MessageBus::new();
        let mut receiver = bus.receiver();
        let mut event = transcript_event(None, Default::default());
        event.kind = TranscriptKind::Receipt;

        publish_transcript_event(&event, &bus).await.unwrap();

        assert!(timeout(Duration::from_millis(20), receiver.recv())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn malformed_continuum_body_is_ignored() {
        let envelope = event_bridge_envelope();
        let bus = MessageBus::new();
        let mut receiver = bus.receiver();
        let event = transcript_event(
            Some(Body::Json(json!({"not": "an envelope"}))),
            headers_for_envelope(&envelope),
        );

        publish_transcript_event(&event, &bus).await.unwrap();

        assert!(timeout(Duration::from_millis(20), receiver.recv())
            .await
            .is_err());
    }
}
