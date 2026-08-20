//! Shared AIRC wire contract for Continuum realtime envelopes.
//!
//! Publish, replay, and live attach all use these helpers so the
//! `forge.body_hint` contract has one definition.

use airc_core::{Body, Headers, TranscriptEvent};
use serde::Deserialize;
use airc_protocol::{FrameKind, HEADER_FORGE_BODY_HINT};

use crate::airc::realtime::{
    AircRealtimeDelivery, AircRealtimeEnvelope, AircRealtimePayload, AircRealtimeSchema,
};
use crate::runtime::message_bus::BusEvent;

pub const CONTINUUM_BODY_HINT: &str = "continuum.airc.realtime.envelope.v1";
pub const HEADER_CONTINUUM_EVENT_ID: &str = "continuum.event_id";
pub const HEADER_CONTINUUM_SOURCE_ID: &str = "continuum.source_id";
pub const HEADER_CONTINUUM_DELIVERY: &str = "continuum.delivery";
pub const HEADER_CONTINUUM_TRACE_ID: &str = "continuum.trace_id";

pub fn frame_kind_for_delivery(delivery: AircRealtimeDelivery) -> FrameKind {
    match delivery {
        AircRealtimeDelivery::Durable => FrameKind::Message,
        AircRealtimeDelivery::EphemeralCoalesced => FrameKind::Event,
        AircRealtimeDelivery::Control | AircRealtimeDelivery::ReceiptOnly => FrameKind::Control,
    }
}

pub fn headers_for_envelope(envelope: &AircRealtimeEnvelope) -> Headers {
    let mut headers = Headers::new();
    headers.insert(
        HEADER_FORGE_BODY_HINT.to_string(),
        CONTINUUM_BODY_HINT.to_string(),
    );
    headers.insert(
        HEADER_CONTINUUM_EVENT_ID.to_string(),
        envelope.event_id.clone(),
    );
    headers.insert(
        HEADER_CONTINUUM_SOURCE_ID.to_string(),
        envelope.source_id.clone(),
    );
    headers.insert(
        HEADER_CONTINUUM_DELIVERY.to_string(),
        format!("{:?}", envelope.delivery),
    );
    if let Some(trace_id) = &envelope.trace_id {
        headers.insert(HEADER_CONTINUUM_TRACE_ID.to_string(), trace_id.clone());
    }
    headers
}

pub fn body_for_envelope(envelope: &AircRealtimeEnvelope) -> Result<Body, String> {
    serde_json::to_value(envelope)
        .map(Body::Json)
        .map_err(|error| format!("failed to encode continuum airc envelope: {error}"))
}

pub fn envelope_from_event(
    event: &TranscriptEvent,
) -> Result<Option<AircRealtimeEnvelope>, String> {
    if event
        .headers
        .get(HEADER_FORGE_BODY_HINT)
        .map(String::as_str)
        != Some(CONTINUUM_BODY_HINT)
    {
        return Ok(None);
    }

    let Some(body) = event.body.as_ref() else {
        return Ok(None);
    };
    let Body::Json(value) = body else {
        return Ok(None);
    };

    // Borrow-decode: `&Value` is itself a Deserializer, so the envelope is read
    // in place. The previous `from_value(value.clone())` deep-copied the WHOLE
    // body first (`from_value` consumes by value, so the clone was reflexive) —
    // paid per event PER PERSONA, since every citizen's subscribe stream sees
    // every event. That is the same O(personas x payload) waste the stream-chunk
    // header guard above exists to avoid; the header decides, the body is never
    // copied to find out.
    AircRealtimeEnvelope::deserialize(value)
        .map(Some)
        .map_err(|error| format!("failed to decode continuum airc envelope: {error}"))
}

pub fn bus_event_from_envelope(envelope: &AircRealtimeEnvelope) -> Option<BusEvent> {
    let AircRealtimePayload::ExistingSchema { payload } = &envelope.payload else {
        return None;
    };
    if payload.schema != AircRealtimeSchema::EventBridgePayload {
        return None;
    }
    let inline = payload.inline.as_ref()?;
    let event_name = inline
        .get("eventName")
        .or_else(|| inline.get("event"))
        .or_else(|| inline.get("name"))
        .and_then(serde_json::Value::as_str)?;

    Some(BusEvent {
        name: event_name.to_string(),
        payload: inline.clone(),
    })
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
/// Is this event a live streaming token fragment rather than a settled room line?
///
/// THE predicate for the stream-chunk contract, in one place, so a caller that needs to
/// skip chunks BEFORE paying for a decode uses the same rule the decoder does. A chunk is
/// never a spoken line: the settled utterance arrives separately via `say()`.
///
/// Why a caller would want it early: every persona's subscribe stream receives every OTHER
/// persona's chunks. Measured live 2026-08-04 during a SWE solve — 2644 of 4776 filtered
/// inbound events (55%) were stream chunks, fanned out identically to all four personas
/// (1195 each) and decoded-then-discarded by each one independently. That is
/// O(personas x tokens) of decode work in the attention path, and it made 65% of the probe
/// stream noise. The earlier response to the same flood was to lower the LOG LEVEL, which
/// hid it without stopping it ([[persona-cognition-symptoms-are-not-noise-to-suppress-fix-the-mind-or-ask]]).
pub fn is_stream_chunk(event: &TranscriptEvent) -> bool {
    event.headers.get(airc_lib::HEADER_STREAM_ID).is_some()
}

pub fn room_turn_from_event(event: &TranscriptEvent) -> Result<(uuid::Uuid, String), &'static str> {
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
    if let Some(text) = event.body.as_ref().and_then(|b| b.as_text()) {
        return Ok((event.peer_id.as_uuid(), text.to_string()));
    }
    match envelope_from_event(event) {
        Err(_) => Err("envelope_decode_error"),
        Ok(None) => Err("no_continuum_body_hint"),
        Ok(Some(envelope)) => {
            chat_transcript_message(&envelope, event.peer_id.as_uuid()).ok_or("non_chat_schema")
        }
    }
}

pub fn chat_transcript_message(
    envelope: &AircRealtimeEnvelope,
    fallback_peer: uuid::Uuid,
) -> Option<(uuid::Uuid, String)> {
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
    Some((sender, text.to_string()))
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
