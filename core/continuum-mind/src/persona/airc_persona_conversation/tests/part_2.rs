#![cfg(test)]
use super::*;

    // what this catches: the store-backed catch-up's synthesized event must
    // decode as a room turn with the row's id, sender and text (the daemon page
    // was empty for the run room on 2026-09-04; this path is the hearing).
    #[test]
    pub(super) fn a_durable_chat_row_becomes_an_admissible_turn() {
        let room = Uuid::new_v4();
        let row = crate::persona::durable_history::RoomRow {
            id: Uuid::new_v4(),
            sender: Uuid::new_v4(),
            occurred_at_ms: 1_788_513_127_000,
            media: Vec::new(),
            text: "Joel here — which card do you hold?".to_string(),
        };
        let mut with_media = row.clone();
        with_media.media.push(crate::cognition::conversation::channel_items::MediaItemRequest {
            kind: "image".into(), mime_type: Some("image/png".into()),
            blob_hash: Some("sha256:1234".into()), url: None, description: None,
        });
        let media_event = event_from_row(room, with_media);
        let envelope = crate::routing::airc_realtime_wire::envelope_from_event(&media_event)
            .expect("valid hydrated envelope").expect("media envelope present");
        let encoded = serde_json::to_value(envelope).expect("encode");
        assert_eq!(encoded["payload"]["payload"]["inline"]["media"][0]["blobHash"], "sha256:1234");
        assert_eq!(media_event.event_id.as_uuid(), row.id);
        let incoming = perceptual_from_event(&media_event).expect("caption turn");
        assert_eq!(incoming.text, row.text);
        assert_eq!(incoming.media.len(), 1);
        let (sender, rendered) = crate::cognition::conversation::room_event::room_turn_from_event(&media_event).unwrap();
        let attention = AttentionEvent::classify(Arc::new(media_event.clone())).unwrap();
        let classified = attention.perception.unwrap();
        assert_eq!(SeenRooms::fingerprint(sender, &rendered),
            SeenRooms::fingerprint(classified.peer_id, &classified.render_content()));
        assert!(incoming.render_room_update().contains("contents not loaded"));
        let restored: IncomingMessage = serde_json::from_value(serde_json::to_value(&incoming).unwrap()).unwrap();
        assert_eq!(restored.media, incoming.media);
        let (id, sender) = (row.id, row.sender);
        let msg = perceptual_from_event(&event_from_row(room, row)).expect("a text row is a turn");
        assert_eq!((msg.event_id, msg.peer_id, msg.room_id), (id, sender, room));
        assert_eq!(msg.text, "Joel here — which card do you hold?");

        // Native AIRC attachment-only turns must reach the same typed projection,
        // without turning another machine's filesystem path into a local read.
        let mut native = media_event.clone();
        native.kind = airc_core::TranscriptKind::Attachment;
        native.body = None;
        native.headers = airc_core::Headers::default();
        native.attachment = Some(airc_core::AttachmentManifest {
            file_id: airc_core::FileId::new(), name: "site.png".into(),
            media_type: Some("image/png".into()), size_bytes: 42,
            content_hash: airc_core::ContentHash(format!("sha256:{}", "a".repeat(64))),
            local_path: Some("C:/private/site.png".into()),
            remote_ref: Some("https://example.invalid/site.png".into()),
        });
        let perceived = perceptual_from_event(&native).expect("native attachment is a room turn");
        assert_eq!(perceived.media.len(), 1);
        assert_eq!(perceived.media[0].kind, "image");
        assert!(perceived.text.is_empty());
        assert!(perceived.media[0].url.is_none());
        assert!(!perceived.render_room_update().contains("private"));
        native.headers.insert(airc_lib::HEADER_STREAM_ID.into(), "stream".into());
        assert!(perceptual_from_event(&native).is_err(), "attachments must not bypass stream filtering");
    }

    // what this catches: the same line under two ids (the sender's message id
    // and the say's event id) admitting twice.
    #[test]
    pub(super) fn the_same_line_under_a_second_id_is_seen_by_fingerprint() {
        let mut seen = SeenRooms::default();
        let room = Uuid::new_v4();
        let peer = Uuid::new_v4();
        let fp = SeenRooms::fingerprint(peer, "one line");
        assert!(!seen.was_seen_text(room, fp));
        seen.note_text(room, fp);
        assert!(seen.was_seen_text(room, fp));
        assert!(!seen.was_seen_text(room, SeenRooms::fingerprint(peer, "another line")));
    }

    /// Regression test for the slice-13.6 reviewer fix to PR #1514:
    /// `next_message` MUST refuse if `prime` wasn't called first.
    /// Per [[no-fallbacks-ever]] the lazy-subscribe fallback that
    /// used to live in next_message was a soft-language degradation
    /// path; this test locks the new typed-error contract.
    ///
    /// Construction is free; primed state stays false; the first
    /// `next_message` returns a typed `Err` naming the missing call.
    #[tokio::test]
    pub(super) async fn next_message_without_prime_errors_visibly() {
        let citizen: Arc<dyn AircCitizen> = Arc::new(StubAircCitizen::new(uuid::Uuid::new_v4()));
        let mut conversation = AircPersonaConversation::new(citizen);
        let err = conversation
            .next_message()
            .await
            .expect_err("next_message must error when stream is unprimed");
        assert!(
            err.contains("prime"),
            "error must name the missing call: {err}"
        );
    }

    /// Perception of the two on-wire room-turn shapes. These lock the
    /// human→persona WAKE path fixed after the glass-box diagnosis showed
    /// a `chat/send` reaching the subscribe stream but being dropped as
    /// `as_text() == None`.
    pub(super) mod perceptual {
        use super::super::super::perceptual_from_event;
        use crate::routing::airc_realtime::AircRealtimeEnvelope;
        use crate::routing::airc_realtime_wire::{body_for_envelope, headers_for_envelope};
        use airc_core::{Body, ClientId, EventId, Headers, MentionTarget, PeerId, RoomId, TranscriptEvent, TranscriptKind};
        use serde_json::json;
        use uuid::Uuid;

        fn event(peer: PeerId, body: Option<Body>, headers: Headers) -> TranscriptEvent {
            TranscriptEvent {
                event_id: EventId::from_u128(1),
                room_id: RoomId::from_u128(2),
                peer_id: peer,
                client_id: ClientId::from_u128(4),
                kind: TranscriptKind::Message,
                occurred_at_ms: 100,
                lamport: 7,
                target: MentionTarget::All,
                headers,
                body,
                attachment: None,
                receipt: None,
                metadata: serde_json::Value::Null,
            }
        }

        /// Encode an envelope exactly as `chat/send` → `airc/realtime-publish`
        /// would put it on the wire (Body::Json + continuum body-hint header),
        /// so the test exercises the real production decode.
        fn wire_event(peer: PeerId, envelope_json: serde_json::Value) -> TranscriptEvent {
            let envelope: AircRealtimeEnvelope =
                serde_json::from_value(envelope_json).expect("valid realtime envelope");
            let body = body_for_envelope(&envelope).expect("encode body");
            let headers = headers_for_envelope(&envelope);
            event(peer, Some(body), headers)
        }

        // what this catches: a peer's plain-text say() is perceived and
        // attributed to its transport peer_id — the direct path must keep
        // working (persona↔persona was never broken).
        #[test]
        fn plain_text_say_is_perceived() {
            let peer = PeerId::from_u128(42);
            let ev = event(peer, Some(Body::text("hello from a peer")), Headers::new());
            let msg = perceptual_from_event(&ev).expect("text body is a room turn");
            assert_eq!(msg.text, "hello from a peer");
            assert_eq!(msg.peer_id, peer.as_uuid());
            assert_eq!(msg.lamport, 7);
            // regression for the 2026-07-23 nil-room turn: the transport's
            // room_id MUST ride into the perceptual message (A.6) — dropping
            // it bound operator/CLI turns to a nil room where every
            // room-scoped RAG source abstained (no board, no kanban, no
            // roster) and the reply had no room to land in.
            assert_eq!(msg.room_id, RoomId::from_u128(2).as_uuid());
        }

        // what this catches: a presence heartbeat (`airc.heartbeat.kind` header) is
        // dropped at the door by the RECEIVE-side guard, and a plain say is not. The
        // subscribe-time `Not(Has(heartbeat))` filter arrived at the daemon inverted
        // (CambrianTech/airc#1368, 2026-09-04): citizens received ONLY heartbeats and
        // never a message, so every room was heard through the store page alone.
        #[test]
        fn heartbeat_is_dropped_at_the_door_and_a_say_is_not() {
            let peer = PeerId::from_u128(42);
            let mut headers = Headers::new();
            headers.insert(
                airc_lib::HEADER_HEARTBEAT_KIND.to_string(),
                "alive".to_string(),
            );
            let beat = event(peer, Some(Body::text("{\"kind\":\"alive\"}")), headers);
            assert!(crate::persona::airc_citizen::is_heartbeat(&beat));
            let say = event(peer, Some(Body::text("hello from a peer")), Headers::new());
            assert!(!crate::persona::airc_citizen::is_heartbeat(&say));
        }

        // what this catches: a live streaming token chunk (airc.stream.* headers,
        // text body) must NOT be perceived as a spoken room turn — card 65fca48d,
        // live 2026-07-24: chunks were stamped Durable by the publish path, came
        // back from page_recent as transcript, and flooded every persona's window
        // with per-250ms fragments ("Anwen: I", "Anwen: see that you're…"),
        // waking peers per fragment and leaking a streamed "PASS" sentinel as
        // room content. The settled utterance arrives separately via say(); the
        // chunk is typing-indicator-class traffic, skipped with a NAMED reason.
        #[test]
        fn stream_chunk_is_not_a_room_turn() {
            let peer = PeerId::from_u128(42);
            let mut headers = Headers::new();
            headers.insert(
                airc_lib::HEADER_STREAM_ID.into(),
                "b1946ac9-2a75-4a6f-9182-6b1c6e0e7a11".to_string(),
            );
            headers.insert(airc_lib::HEADER_STREAM_SEQ.into(), "3".to_string());
            headers.insert(
                airc_lib::HEADER_STREAM_KIND.into(),
                "text.token".to_string(),
            );
            let ev = event(peer, Some(Body::text(" see that you're")), headers);
            assert_eq!(
                perceptual_from_event(&ev),
                Err("stream_chunk"),
                "a streaming fragment must be skipped with its named reason — \
                 never surfaced as a spoken room line"
            );
        }

        // what this catches: THE bug. chat/send arrives as a Body::Json
        // chat_transcript envelope (as_text()==None). It MUST be perceived,
        // and attributed to the true logical sender (inline.senderId), not
        // the core transport peer that relayed the publish.
        #[test]
        fn chat_transcript_envelope_is_perceived_with_true_sender() {
            let relay_peer = PeerId::from_u128(7711); // the core's publish peer
            let human_sender = Uuid::from_u128(0xB0B);
            let ev = wire_event(
                relay_peer,
                json!({
                    "eventId": Uuid::from_u128(9).to_string(),
                    "roomId": Uuid::from_u128(2).to_string(),
                    "sourceId": human_sender.to_string(),
                    "createdAtMs": 100u64,
                    "delivery": "durable",
                    "payload": {
                        "kind": "existing_schema",
                        "payload": {
                            "schema": "chat_transcript",
                            "inline": {
                                "messageId": Uuid::from_u128(3).to_string(),
                                "text": "Asha, are you there?",
                                "senderId": human_sender.to_string(),
                                "replyToId": null,
                            }
                        }
                    }
                }),
            );
            let msg = perceptual_from_event(&ev).expect("chat_transcript is a room turn");
            assert_eq!(msg.text, "Asha, are you there?");
            assert_eq!(
                msg.peer_id, human_sender,
                "attribution must be the logical sender, not the relay peer"
            );
        }

        // what this catches: a non-message envelope (event-bridge, also
        // as_text()==None) must NOT be surfaced as a room turn — only actual
        // chat is perceptual, not every Json body on the stream.
        #[test]
        fn event_bridge_envelope_is_not_a_room_turn() {
            let ev = wire_event(
                PeerId::from_u128(5),
                json!({
                    "eventId": Uuid::from_u128(9).to_string(),
                    "roomId": Uuid::from_u128(2).to_string(),
                    "sourceId": PeerId::from_u128(5).as_uuid().to_string(),
                    "createdAtMs": 100u64,
                    "delivery": "durable",
                    "payload": {
                        "kind": "existing_schema",
                        "payload": {
                            "schema": "event_bridge_payload",
                            "inline": { "eventName": "data:users:created", "id": "x" }
                        }
                    }
                }),
            );
            assert_eq!(
                perceptual_from_event(&ev),
                Err("non_chat_schema"),
                "an event-bridge envelope is a LEGIT skip — and the reason must say so, \
                 distinguishable from a decode error or a lost body-hint header (#177)"
            );
        }
    }
