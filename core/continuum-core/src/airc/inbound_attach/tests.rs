#![cfg(test)]
use super::*;
use crate::routing::airc_realtime::{AircRealtimeEnvelope, AircRealtimePayload, AircRealtimePayloadRef, AircRealtimeSchema};
use crate::routing::airc_realtime_wire::headers_for_envelope;
use airc_core::{Body, ClientId, EventId, MentionTarget, PeerId, RoomId, TranscriptEvent, TranscriptKind};
use serde_json::json;
use tokio::time::{timeout, Duration};
use uuid::Uuid;

// what this catches: #295 — the first-ever attach (no persisted watermark)
// used bare FromTranscriptStart, streaming a long-lived room's ENTIRE
// history into live perception (BigMama's fresh Windows node pegged 24
// cores ~45min servicing days of backlog as fresh events). First attach
// must coalesce backlog into one summary frame; a resumed attach must
// stream its gap uncoalesced (those events are owed to the transcript
// writer + perception).
#[test]
fn first_attach_coalesces_backlog_resume_streams_gap() {
    let channel = RoomId::from_u128(7);

    let first = attach_request_for_cursor(channel, None);
    assert_eq!(first.start(), AttachStart::FromTranscriptStart);
    assert!(
        first.coalesces_backlog(),
        "first-ever attach must not replay full room history into live perception"
    );
    assert_eq!(
        first.backlog_tail(),
        Some(FIRST_ATTACH_PAGE),
        "join shows one page back, not a void (airc PR #1312)"
    );

    let cursor = IpcCursor {
        epoch: 3,
        counter: 42,
        event_id: EventId::from_u128(9),
    };
    let resumed = attach_request_for_cursor(channel, Some(cursor));
    assert_eq!(resumed.start(), AttachStart::After(cursor));
    assert!(
        !resumed.coalesces_backlog(),
        "gap replay after a watermark feeds the transcript writer — never coalesced"
    );
}

// what this catches (card ac4bca50): the persisted watermark advances ONLY on the
// daemon's `AttachCursorAdvanced` heartbeat, and airc #1417 made that frame opt-in
// per attach. A request that does not ask never advances its cursor, so every boot
// resumes from the last cursor ever written — a week of every room replayed into
// perception and the capacity ledger on the IntelMac, 2026-09-21. Both arms must ask.
#[test]
fn every_attach_asks_for_the_cursor_heartbeat_it_persists() {
    let channel = RoomId::from_u128(7);
    assert!(
        attach_request_for_cursor(channel, None).wants_cursor_heartbeat(),
        "first attach: the seam summary advances once; the heartbeat keeps it moving"
    );
    let cursor = IpcCursor {
        epoch: 3,
        counter: 42,
        event_id: EventId::from_u128(9),
    };
    assert!(
        attach_request_for_cursor(channel, Some(cursor)).wants_cursor_heartbeat(),
        "resume: no coalesced seam, so the heartbeat is the ONLY advance source"
    );
}

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

    publish_transcript_event(&Arc::new(event), &bus).await.unwrap();

    let delivered = timeout(Duration::from_millis(200), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(delivered.name, "persona:ready");
    assert_eq!(delivered.payload["data"]["personaId"], "helper-ai");
}

/// Build the `chat_transcript` envelope exactly as `chat/send` →
/// `airc/realtime-publish` does: a `Body::Json` continuum envelope
/// whose inline carries the human message + its logical `senderId`.
fn chat_transcript_envelope(sender: Uuid, text: &str) -> AircRealtimeEnvelope {
    AircRealtimeEnvelope::new(
        "evt-1".to_string(),
        Uuid::from_u128(2),
        sender.to_string(),
        100,
        AircRealtimePayload::ExistingSchema {
            payload: AircRealtimePayloadRef::inline(
                AircRealtimeSchema::ChatTranscript,
                json!({
                    "messageId": Uuid::from_u128(0x3).to_string(),
                    "text": text,
                    "senderId": sender.to_string(),
                    "replyToId": serde_json::Value::Null,
                }),
            ),
        },
    )
}

#[tokio::test]
async fn chat_transcript_envelope_projects_chat_posted() {
    // what this catches: the Case-A completeness fix (task #84). A human
    // `chat/send` publishes a `chat_transcript` envelope (Body::Json,
    // as_text()==None) — NOT the plain-text shape a persona `say` uses.
    // It must project to the SAME thin `chat:posted` the plain-text
    // sibling emits so the positron read surface carries human turns,
    // not just persona/peer turns. A regression that reverts the
    // `else if chat_posted_from_envelope` arm makes human questions
    // structurally invisible in `ChatViewState` (the room shows only
    // the persona's replies). senderId must be the envelope's LOGICAL
    // sender, never the relaying transport peer (PeerId 3).
    let bus = MessageBus::new();
    let mut receiver = bus.receiver();
    let sender = Uuid::from_u128(0x5e);
    let envelope = chat_transcript_envelope(sender, "is anyone there?");
    let event = transcript_event(
        Some(Body::Json(serde_json::to_value(&envelope).unwrap())),
        headers_for_envelope(&envelope),
    );

    publish_transcript_event(&Arc::new(event), &bus).await.unwrap();

    let delivered = timeout(Duration::from_millis(200), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(delivered.name, "chat:posted");
    assert_eq!(delivered.payload["content"], "is anyone there?");
    // messageId prefers the envelope's inline `messageId` — the id
    // chat/send persisted the row under — so the projection, the durable
    // store, and the sender agree on ONE identity per human message and
    // the store-side projector (#140) dedups instead of double-writing.
    // (event_id remains the fallback for envelopes without an inline id.)
    assert_eq!(
        delivered.payload["messageId"],
        Uuid::from_u128(0x3).to_string()
    );
    assert_eq!(delivered.payload["roomId"], Uuid::from_u128(2).to_string());
    assert_eq!(delivered.payload["timestamp"], 100);
    // the LOGICAL sender the envelope carries, not transport PeerId 3.
    assert_eq!(delivered.payload["senderId"], sender.to_string());
    // identity-free like its sibling: no fabricated name/kind.
    assert!(delivered.payload.get("senderName").is_none());
    assert!(delivered.payload.get("senderKind").is_none());
}

#[tokio::test]
async fn non_continuum_body_is_ignored() {
    let bus = MessageBus::new();
    let mut receiver = bus.receiver();
    let event = transcript_event(
        Some(Body::Json(json!({"eventName": "ignored"}))),
        Default::default(),
    );

    publish_transcript_event(&Arc::new(event), &bus).await.unwrap();

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

    publish_transcript_event(&Arc::new(event), &bus).await.unwrap();

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

    publish_transcript_event(&Arc::new(event), &bus).await.unwrap();

    assert!(timeout(Duration::from_millis(20), receiver.recv())
        .await
        .is_err());
}

#[tokio::test]
async fn wall_post_event_projects_thin_wall_changed_signal() {
    // what this catches: the wall classification arm (task #89 Unit 2).
    // A `WallPostPublished` transcript event (no continuum envelope)
    // must project a `wall:changed` bus signal carrying ONLY the
    // room_id — the re-read cue the positron wall projector folds. The
    // signal must NOT carry post content (the supersede-projected board
    // is re-read from airc, never reconstructed from one delta). A
    // regression that drops this arm silently stops every pinned board
    // from ever updating on the renderer.
    let bus = MessageBus::new();
    let mut receiver = bus.receiver();
    let mut event = transcript_event(None, Default::default());
    event.kind = TranscriptKind::WallPostPublished;

    publish_transcript_event(&Arc::new(event), &bus).await.unwrap();

    let delivered = timeout(Duration::from_millis(200), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(delivered.name, "wall:changed");
    assert_eq!(delivered.payload["roomId"], Uuid::from_u128(2).to_string());
    // A cue, not the content: the board is re-read authoritatively.
    assert!(delivered.payload.get("body").is_none());
    assert!(delivered.payload.get("category").is_none());
}

#[tokio::test]
async fn work_board_event_projects_thin_kanban_changed_signal() {
    // what this catches: the kanban classification arm (task #117 Unit
    // 3b-ii). A work-board change rides the transcript as a
    // `TranscriptKind::System` event carrying airc-work's body-hint
    // header — NOT a dedicated TranscriptKind — so the classifier
    // discriminates via airc-work's own `transcript_is_work_event`, not
    // a kind match. This must project a `kanban:changed` bus signal
    // carrying ONLY the room_id: the re-read cue the positron kanban
    // projector folds by re-reading the authoritative board
    // (`work_board_complete`), never reconstructing the fold from one
    // delta. A regression that drops this arm silently freezes every
    // rendered kanban board. Built via the real producer
    // (`encode_work_event`) so a header-contract drift fails HERE, and
    // asserts the chat/wall arms yield first for the System kind + JSON
    // body (no chat/wall misclassification of a work event).
    let work_event = airc_work::WorkEvent::CardStateChanged(airc_work::CardStateChanged {
        card_id: airc_work::WorkCardId::new(),
        state: airc_work::CardState::Open,
        changed_by: PeerId::from_u128(3),
        changed_at_ms: 100,
    });
    let (headers, body) =
        airc_work::encode_work_event(&work_event).expect("work event encodes for the fixture");

    let bus = MessageBus::new();
    let mut receiver = bus.receiver();
    let mut event = transcript_event(Some(body), headers);
    event.kind = TranscriptKind::System;

    publish_transcript_event(&Arc::new(event), &bus).await.unwrap();

    let delivered = timeout(Duration::from_millis(200), receiver.recv())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(delivered.name, "kanban:changed");
    assert_eq!(delivered.payload["roomId"], Uuid::from_u128(2).to_string());
    // A cue, not the content: the board is re-read authoritatively.
    assert!(delivered.payload.get("cards").is_none());
    assert!(delivered.payload.get("state").is_none());
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

    publish_transcript_event(&Arc::new(event), &bus).await.unwrap();

    assert!(timeout(Duration::from_millis(20), receiver.recv())
        .await
        .is_err());
}

// what this catches: the 2026-10-30 incident. An untrusted peer published
// `persona/placement/reserve` requests (a resident's whole prompt in params) into
// #general, and this seam fed them to two residents as room speech
// (`mind.feed.event from=e9093c03 residents=2`). A frame whose transport kind says
// command is routed away before perception; a plain line in the same room, to the
// same resident, still wakes her.
#[tokio::test]
async fn a_peer_command_in_a_room_never_reaches_a_mind_and_room_speech_still_does() {
    const ROOM: Uuid = Uuid::from_u128(0x2026_1030_0028);
    let persona = Uuid::from_u128(0x2026_1030_0029);
    let dir = tempfile::tempdir().expect("her mind dir");
    let (mut region, _awareness) = crate::persona::perception_region::PerceptionRegion::boot(
        PeerId::from_uuid(persona),
        dir.path(),
        1_000,
        0,
    );
    region.join(ROOM, "general");
    region.set_dial(crate::cognition::attention::AttentionDial::default(), 0);
    let _ = region.wake_for(0); // a fresh region owes no resume wake
    let (wake_tx, mut wakes) = tokio::sync::mpsc::channel(4);
    crate::persona::perception_feed::register(
        persona,
        PeerId::from_uuid(persona),
        "Aiko",
        Arc::new(std::sync::Mutex::new(region)),
        wake_tx,
        None,
    );
    let bus = MessageBus::new();

    // airc's typed Command frame (airc#1620), as if it surfaced on the room stream:
    // the room path has no command handling, and perception never takes it.
    let mut command = transcript_event(
        Some(Body::Json(json!({
            "kind": "peer",
            "path": "persona/placement/reserve",
            "params": { "prompt": "@Aiko a resident's whole prompt" }
        }))),
        airc_core::Headers::new(),
    );
    command.kind = TranscriptKind::Command;
    command.room_id = RoomId::from_uuid(ROOM);
    command.target = MentionTarget::Peer(PeerId::from_uuid(persona));
    publish_transcript_event(&Arc::new(command), &bus)
        .await
        .expect("a protocol frame is routed, not an error");
    assert!(wakes.try_recv().is_err(), "a peer command is not something she perceives");

    let mut speech = transcript_event(Some(Body::text("@Aiko how is it going?")), airc_core::Headers::new());
    speech.room_id = RoomId::from_uuid(ROOM);
    publish_transcript_event(&Arc::new(speech), &bus)
        .await
        .expect("room speech is fed");
    let woke = matches!(
        wakes.try_recv(),
        Ok(crate::cognition::conversation::Wake::Perceive { .. })
    );
    crate::persona::perception_feed::unregister(persona);
    assert!(woke, "a line addressed to her in her room still wakes her");
}
