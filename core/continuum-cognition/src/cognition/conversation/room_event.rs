//! What a mind PERCEIVES in a room event: a spoken turn (text, attached media) or a typed
//! board fact, each decoded once and rendered for her prompt. Perception, so it lives with
//! the persona; the wire it reads (the realtime envelope, its headers) is the kernel's
//! `routing::airc_realtime_wire`.

use airc_core::TranscriptEvent;

use crate::routing::airc_realtime::{AircRealtimeEnvelope, AircRealtimePayload, AircRealtimeSchema};
use crate::routing::airc_realtime_wire::{envelope_from_event, is_command_frame, is_stream_chunk};

/// Typed room content; attachment bytes remain outside transcript and prompt.
#[derive(Debug, Clone)]
pub struct RoomTurn {
    pub sender: uuid::Uuid,
    pub text: String,
    pub media: Vec<crate::cognition::conversation::channel_items::MediaItemRequest>,
}

/// A typed work fact in the room, as perception (EVENT-MIND.md §1b, phase 1
/// wiring). Measured live 2026-10-04 on the 5090: a typed `work_submission_reviewed`
/// (a PASS on her own card) replicated to her node and her inbound dropped it as
/// `non_chat_schema`, 519 of 520 events; only chat was perception. A citizen who
/// cannot perceive her own board cannot be woken by it. The decoder now yields the
/// board's typed events beside speech; `cognition::conversation::salience` judges them.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RoomWork {
    Reviewed { card_id: uuid::Uuid, outcome: crate::cognition::conversation::salience::ObservedVerdict, reviewer: uuid::Uuid, review: uuid::Uuid },
    StateChanged { card_id: uuid::Uuid, state: airc_work::CardState, by: uuid::Uuid },
    Submitted { card_id: uuid::Uuid, publisher: uuid::Uuid, #[serde(default)] submission: Option<uuid::Uuid> },
    Claimed { card_id: uuid::Uuid, owner: uuid::Uuid, #[serde(default)] claim: Option<uuid::Uuid> },
    /// A work event the mind does not yet render; counted, never dropped silently.
    Other { kind: String },
}

/// Decode a typed work event from the room, if the event is one. `Ok(None)` = not a
/// work event (the chat path applies). Pure lease renewals remain board state,
/// not new attention input. Headers decide; the body is decoded once.
pub fn room_work_from_event(event: &TranscriptEvent) -> Result<Option<RoomWork>, &'static str> {
    // The body hint is authoritative in the shared work codec. The kind header
    // is optional routing metadata; requiring it would hide valid bridge events.
    // Retain kind-only malformed frames for the existing decode diagnostic.
    if !airc_work::transcript_is_work_event(event)
        && !event.headers.contains_key(airc_work::HEADER_FORGE_WORK_EVENT_KIND)
    {
        return Ok(None);
    }
    let work = airc_work::decode_work_event(&event.headers, event.body.as_ref()).map_err(|_| "work_event_decode_error")?;
    use airc_work::WorkEvent as W;
    Ok(Some(match &work {
        // 2dcaacc1: a lease extension changes the durable board's expiry, not
        // the citizen's task. Both the one-feed classifier and catch-up pump
        // use this decoder, so renewal traffic cannot bypass source perception
        // and accumulate as required mid-action messages. The original event
        // remains in the board projection and its work/get expansion surface.
        W::ClaimHeartbeat(_) => return Err("work_lease_renewal"),
        W::WorkSubmissionReviewed(r) => RoomWork::Reviewed {
            card_id: r.card_id.as_uuid(),
            review: r.review_id.as_uuid(),
            outcome: match r.outcome {
                airc_work::WorkReviewOutcome::Passed => crate::cognition::conversation::salience::ObservedVerdict::Passed,
                airc_work::WorkReviewOutcome::Failed => crate::cognition::conversation::salience::ObservedVerdict::Failed,
                airc_work::WorkReviewOutcome::Unknown => crate::cognition::conversation::salience::ObservedVerdict::Unknown,
            },
            reviewer: r.reviewer.as_uuid(),
        },
        W::CardStateChanged(c) => RoomWork::StateChanged { card_id: c.card_id.as_uuid(), state: c.state, by: c.changed_by.as_uuid() },
        W::WorkSubmitted(s) => RoomWork::Submitted { card_id: s.card_id.as_uuid(), publisher: s.publisher.as_uuid(), submission: Some(s.submission_id.as_uuid()) },
        W::CardClaimed(c) => RoomWork::Claimed { card_id: c.card_id.as_uuid(), owner: c.owner.as_uuid(), claim: Some(c.claim_id.as_uuid()) },
        _ => RoomWork::Other {
            // Project the decoded variant through the SDK owner, never trust an
            // optional (or contradictory) routing hint as the board fact.
            kind: airc_work::work_event_headers(&work)
                .remove(airc_work::HEADER_FORGE_WORK_EVENT_KIND)
                .ok_or("work_event_decode_error")?,
        },
    }))
}

pub fn room_turn_from_event(event: &TranscriptEvent) -> Result<(uuid::Uuid, String), &'static str> {
    let turn = room_content_from_event(event)?;
    Ok((turn.sender, render_room_content(&turn.text, &turn.media)))
}

/// A bounded notice, not a claim that the model has inspected the attachment.
/// The event retains the full references for explicit inspection/replay.
pub(crate) fn render_room_content(text: &str, media: &[crate::cognition::conversation::channel_items::MediaItemRequest]) -> String {
    if media.is_empty() { return text.to_owned(); }
    let mut rendered = format!("{text}\n[{} media attachment(s); contents not loaded", media.len());
    for item in media.iter().take(8) {
        let kind: String = item.kind.chars().filter(|c| !c.is_control()).take(24).collect();
        rendered.push_str(&format!("; {kind}"));
        if let Some(hash) = item.blob_hash.as_deref().filter(|h| h.starts_with("sha256:") && h.len() == 71 && h[7..].bytes().all(|b| b.is_ascii_hexdigit())) {
            rendered.push_str(&format!(" {hash}"));
        }
    }
    rendered.push(']');
    rendered
}

/// Render a typed work fact for her prompt: a labelled BOARD FACT, never the actor's
/// words. Short ids because that is how the board renders them; the typed event
/// retains the full ids for her hands.
pub fn render_room_work(work: &RoomWork, actor: uuid::Uuid) -> String {
    let short = |id: uuid::Uuid| id.to_string()[..8].to_string();
    match work {
        RoomWork::Reviewed { card_id, outcome, reviewer, .. } => format!(
            "[board fact] review on card {}: {:?} (reviewer {})",
            short(*card_id), outcome, short(*reviewer)
        ),
        RoomWork::StateChanged { card_id, state, by } => format!(
            "[board fact] card {} moved to {:?} (by {})",
            short(*card_id), state, short(*by)
        ),
        RoomWork::Submitted { card_id, publisher, .. } => format!(
            "[board fact] submission published on card {} (by {})",
            short(*card_id), short(*publisher)
        ),
        RoomWork::Claimed { card_id, owner, .. } => format!(
            "[board fact] card {} claimed (by {})",
            short(*card_id), short(*owner)
        ),
        RoomWork::Other { kind } => format!("[board fact] {kind} (by {})", short(actor)),
    }
}

/// Recover `(logical sender, text)` for a ROOM TURN from EITHER on-wire shape —
/// the ONE place that decides what counts as a spoken room line:
///
/// 1. a peer's plain-text `say()` (`Body::Text`) → the transport peer said it;
/// 2. a human/web `chat/send` (`Body::Json` chat_transcript envelope) → the
///    envelope's logical sender said it.
///
/// Everything else returns a NAMED skip reason (never a bare drop — #177 was
/// diagnosed blind because a silent `None` collapsed "decode error", "not our
/// envelope", and "legit non-chat schema" into one indistinguishable void):
/// - `"no_continuum_body_hint"` — not a continuum envelope (or the header was lost);
/// - `"envelope_decode_error"` — hint present, serde decode FAILED (wire drift);
/// - `"non_chat_schema"` — a continuum envelope that is not a chat line
///   (presence, event-bridge, media-control) — a legit skip.
/// - `"stream_chunk"` — a live streaming token fragment (`airc.stream.*`
///   headers): typing-indicator-class traffic whose settled utterance arrives
///   separately via `say()` — never a spoken line (card 65fca48d).
///
/// Consumers: persona perception (`perceptual_from_event`), the digest element
/// (`ChannelElement::new` — was the third text-only blind surface), and any
/// future read surface. One decoder, every reader.
pub fn room_content_from_event(event: &TranscriptEvent) -> Result<RoomTurn, &'static str> {
    if is_command_frame(event) {
        return Err("command_frame");
    }
    // - `"stream_chunk"` — a live streaming token chunk (`airc.stream.*` headers,
    //   published by `publish_stream_chunk` as typing-indicator-class traffic).
    //   By the stream-chunk contract the settled utterance arrives separately via
    //   `say()`; the chunk is NEVER a spoken room line. Live 2026-07-24 (card
    //   65fca48d): the daemon publish path classified chunks Durable, so
    //   `page_recent` returned hours of per-250ms fragments ("Anwen: I",
    //   "Anwen: see that you're…") as transcript — they flooded every persona's
    //   digest window (a 21-31 "message" window that was mostly shards of ONE
    //   utterance), woke peers per fragment, leaked a streamed "PASS" sentinel
    //   as room content, and starved the repetition detectors of judgeable
    //   turns. The header check is the receive-side guard that holds regardless
    //   of how the sender's delivery class was stamped.
    if is_stream_chunk(event) {
        return Err("stream_chunk");
    }
    let mut turn = if let Some(text) = event.body.as_ref().and_then(|b| b.as_text()) {
        RoomTurn { sender: event.peer_id.as_uuid(), text: text.to_string(), media: Vec::new() }
    } else if event.body.is_none() && event.kind == airc_core::TranscriptKind::Attachment && event.attachment.is_some() {
        RoomTurn { sender: event.peer_id.as_uuid(), text: String::new(), media: Vec::new() }
    } else {
        match envelope_from_event(event) {
            Err(_) => return Err("envelope_decode_error"),
            Ok(None) => return Err("no_continuum_body_hint"),
            Ok(Some(envelope)) => chat_transcript_content(&envelope, event.peer_id.as_uuid()).ok_or("non_chat_schema")?,
        }
    };
    // Native AIRC attachments are first-class room input too. Preserve the
    // envelope's logical sender/caption; never treat a peer's path as a local read.
    if matches!(event.kind, airc_core::TranscriptKind::Message | airc_core::TranscriptKind::Attachment) {
        if let Some(attachment) = &event.attachment {
            let kind = attachment.media_type.as_deref()
                .and_then(|mime| mime.split_once('/').map(|(kind, _)| kind))
                .unwrap_or("file"); // Unknown MIME is displayed as a generic file, never a supported visual capability.
            let media = crate::cognition::conversation::channel_items::MediaItemRequest {
                kind: kind.to_owned(), mime_type: attachment.media_type.clone(),
                blob_hash: Some(attachment.content_hash.0.clone()),
                url: None, description: None,
            };
            if !turn.media.contains(&media) {
                turn.media.push(media);
            }
        }
    }
    Ok(turn)
}

/// Recover the `(logical sender, text)` from a decoded `chat_transcript`
/// envelope — the `Body::Json` shape `chat/send` (a human, the web client,
/// any non-`say` caller) publishes. Returns `None` for any other envelope
/// (EventBridge, presence, media-control) — a non-`chat_transcript`
/// fabricates nothing.
///
/// The sender falls back to `fallback_peer` (the transport peer that
/// relayed the publish) when `inline.senderId` is absent. Both are real
/// identities — this is attribution recovery of a present-but-relayed
/// sender, NOT a fabricated default for a missing one
/// ([[fallbacks-are-illegal-fail-loud]]).
///
/// This is the ONE decoder for the `chat_transcript` wire shape. Both the
/// persona perception path (`perceptual_from_event`) and the positron
/// projection path (`chat_posted_from_envelope`) read a human chat line
/// through it — the receive-side symmetry of the plain-text/`say` sibling.
/// A prior fix taught only the persona to read this shape; the positron
/// read surface had the identical structural blindness (human chat lines
/// reached the transcript but never `ChatViewState`) until it too routed
/// through this decoder.
pub fn chat_transcript_message(
    envelope: &AircRealtimeEnvelope,
    fallback_peer: uuid::Uuid,
) -> Option<(uuid::Uuid, String)> {
    let turn = chat_transcript_content(envelope, fallback_peer)?;
    Some((turn.sender, render_room_content(&turn.text, &turn.media)))
}

fn chat_transcript_content(envelope: &AircRealtimeEnvelope, fallback_peer: uuid::Uuid) -> Option<RoomTurn> {
    let AircRealtimePayload::ExistingSchema { payload } = &envelope.payload else {
        return None;
    };
    if payload.schema != AircRealtimeSchema::ChatTranscript {
        return None;
    }
    let inline = payload.inline.as_ref()?;
    let text = inline.get("text").and_then(serde_json::Value::as_str)?;
    let sender = inline
        .get("senderId")
        .and_then(serde_json::Value::as_str)
        .and_then(|s| uuid::Uuid::parse_str(s).ok())
        .unwrap_or(fallback_peer);
    let media = inline.get("media").map(|value| serde_json::from_value(value.clone())).transpose().ok()?.unwrap_or_default(); // boundary: AIRC transcript JSON decodes typed references; omitted media is the legacy text-only wire contract.
    Some(RoomTurn { sender, text: text.to_string(), media })
}

/// One chat-transcript line as its envelope carries it: the encode side of
/// [`chat_transcript_message`]'s decoder, so the shape is written and read in one place.
pub struct ChatTranscriptLine<'a> {
    pub room_id: uuid::Uuid,
    pub sender_id: uuid::Uuid,
    pub text: &'a str,
    pub media: &'a [crate::cognition::conversation::channel_items::MediaItemRequest],
    pub reply_to_id: Option<uuid::Uuid>,
}

/// The realtime envelope (as JSON) for one chat line: the shape operator publication,
/// caller-owned publication and a hydrated durable row all send.
pub fn chat_transcript_envelope(message_id: uuid::Uuid, line: &ChatTranscriptLine<'_>, now_ms: u64) -> serde_json::Value {
    serde_json::json!({
        "eventId": uuid::Uuid::new_v4().to_string(),
        "roomId": line.room_id.to_string(),
        "sourceId": line.sender_id.to_string(),
        "createdAtMs": now_ms,
        // Delivery must match the payload's semantics — see
        // `AircRealtimePayload::delivery()`. ExistingSchema/
        // ChatTranscript → Durable.
        "delivery": "durable",
        "payload": {
            "kind": "existing_schema",
            "payload": {
                "schema": "chat_transcript",
                "inline": {
                    "messageId": message_id.to_string(),
                    "text": line.text,
                    "media": line.media,
                    "senderId": line.sender_id.to_string(),
                    "replyToId": line.reply_to_id.map(|u| u.to_string()),
                }
            }
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the ONE chat_transcript decoder recovers the
    // logical sender + text from the Body::Json shape chat/send publishes.
    // This is the seam that keeps human chat lines from being structurally
    // invisible to BOTH persona perception and the positron read surface.
    // A regression that renamed the `chat_transcript` schema tag or the
    // inline `text`/`senderId` fields silently drops every human message
    // from the room.
    #[test]
    fn chat_transcript_message_recovers_sender_and_text() {
        let sender = uuid::Uuid::from_u128(0x5e).to_string();
        let relay = uuid::Uuid::from_u128(0x4e);
        // Encode exactly as chat/send → airc/realtime-publish builds it.
        let envelope: AircRealtimeEnvelope = serde_json::from_value(serde_json::json!({
            "eventId": uuid::Uuid::from_u128(0x1).to_string(),
            "roomId": uuid::Uuid::from_u128(0x2).to_string(),
            "sourceId": sender,
            "createdAtMs": 100,
            "delivery": "durable",
            "payload": {
                "kind": "existing_schema",
                "payload": {
                    "schema": "chat_transcript",
                    "inline": {
                        "messageId": uuid::Uuid::from_u128(0x3).to_string(),
                        "text": "is anyone there?",
                        "senderId": sender,
                        "replyToId": serde_json::Value::Null,
                    }
                }
            },
        }))
        .expect("chat/send envelope shape must decode into AircRealtimeEnvelope");

        let (recovered, text) =
            chat_transcript_message(&envelope, relay).expect("chat_transcript must decode");
        assert_eq!(
            recovered.to_string(),
            sender,
            "logical sender, not the relay"
        );
        assert_eq!(text, "is anyone there?");
    }

    // what this catches: media awareness must not embed URL/base64 or generated descriptions in every turn.
    #[test]
    fn media_notice_is_bounded_and_does_not_load_content() {
        let item = crate::cognition::conversation::channel_items::MediaItemRequest {
            kind: "image".into(), mime_type: Some("image/png".into()),
            blob_hash: Some(format!("sha256:{}", "a".repeat(64))),
            url: Some(format!("data:image/png;base64,{}", "x".repeat(100_000))),
            description: Some("unverified visual claim".into()),
        };
        let rendered = render_room_content("caption", &vec![item; 100]);
        assert!(rendered.contains("100 media attachment(s)"));
        assert!(rendered.contains("contents not loaded"));
        assert!(!rendered.contains("base64"));
        assert!(!rendered.contains("unverified visual claim"));
        assert!(rendered.len() < 1000);
    }

    // what this catches: attribution recovery, not fabrication. When the
    // inline omits senderId, the sender falls back to the transport peer
    // that relayed the publish — a real identity, never a nil/default.
    #[test]
    fn chat_transcript_message_falls_back_to_transport_peer() {
        let relay = uuid::Uuid::from_u128(0x4e);
        let envelope: AircRealtimeEnvelope = serde_json::from_value(serde_json::json!({
            "eventId": uuid::Uuid::from_u128(0x1).to_string(),
            "roomId": uuid::Uuid::from_u128(0x2).to_string(),
            "sourceId": uuid::Uuid::from_u128(0x5e).to_string(),
            "createdAtMs": 100,
            "delivery": "durable",
            "payload": {
                "kind": "existing_schema",
                "payload": {
                    "schema": "chat_transcript",
                    "inline": { "text": "hello" }
                }
            },
        }))
        .expect("envelope must decode");

        let (recovered, text) =
            chat_transcript_message(&envelope, relay).expect("chat_transcript must decode");
        assert_eq!(
            recovered, relay,
            "omitted senderId recovers to the relay peer"
        );
        assert_eq!(text, "hello");
    }

    // what this catches: classification, not fallback. A non-chat_transcript
    // envelope (here EventBridge) must yield None from this decoder — it is
    // NOT a chat line and must never be projected as one.
    #[test]
    fn non_chat_transcript_envelope_is_none() {
        let envelope: AircRealtimeEnvelope = serde_json::from_value(serde_json::json!({
            "eventId": uuid::Uuid::from_u128(0x1).to_string(),
            "roomId": uuid::Uuid::from_u128(0x2).to_string(),
            "sourceId": uuid::Uuid::from_u128(0x5e).to_string(),
            "createdAtMs": 100,
            "delivery": "durable",
            "payload": {
                "kind": "existing_schema",
                "payload": {
                    "schema": "event_bridge_payload",
                    "inline": { "eventName": "chat:posted" }
                }
            },
        }))
        .expect("envelope must decode");

        assert!(chat_transcript_message(&envelope, uuid::Uuid::from_u128(0x4e)).is_none());
    }
}
