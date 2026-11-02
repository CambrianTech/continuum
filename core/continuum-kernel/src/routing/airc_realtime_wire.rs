//! Shared AIRC wire contract for Continuum realtime envelopes: the body hint and headers,
//! encode and decode, the bus bridge, and the header predicates a receiver checks before
//! decoding. Publish, replay, live attach and a mind's perception all use these, so the
//! `forge.body_hint` contract has one definition, below every reader.

use airc_core::{Body, Headers, TranscriptEvent};
use serde::Deserialize;
use airc_protocol::{FrameKind, HEADER_FORGE_BODY_HINT};

use super::airc_realtime::{
    AircRealtimeDelivery, AircRealtimeEnvelope, AircRealtimePayload, AircRealtimeSchema,
};
use crate::runtime::message_bus::BusEvent;

pub const CONTINUUM_BODY_HINT: &str = "continuum.airc.realtime.envelope.v1";
pub const HEADER_CONTINUUM_EVENT_ID: &str = "continuum.event_id";
pub const HEADER_CONTINUUM_SOURCE_ID: &str = "continuum.source_id";
pub const HEADER_CONTINUUM_DELIVERY: &str = "continuum.delivery";
pub const HEADER_CONTINUUM_TRACE_ID: &str = "continuum.trace_id";

/// The line's logical sender, from the `continuum.source_id` header the publisher stamped (a relay
/// publishes a human's line for them); the transport peer when the header is absent or is not an id
/// (a presence subject, a free-form source). Read from the header alone: the body is never opened to
/// attribute a line (Joel, 2026-11-01: routing on headers, never parsed bodies).
///
/// Trust is the relay rule's, not this decoder's: whether a transport peer may speak for the sender it
/// names is a grid-trust decision (raised with Codex), exactly as it was for the body field it replaces.
pub fn logical_sender(event: &TranscriptEvent) -> uuid::Uuid {
    event
        .headers
        .get(HEADER_CONTINUUM_SOURCE_ID)
        .and_then(|value| uuid::Uuid::parse_str(value).ok()) // wire boundary: a header value is text; a non-id source is a non-person publisher
        .unwrap_or_else(|| event.peer_id.as_uuid()) // the transport peer is the real author when no relay named one
}

/// The sender a line's header claims it speaks for, when it names one by id. `None` when no claim
/// (or a non-id source) is made; the transport peer is then the author.
pub fn claimed_sender(event: &TranscriptEvent) -> Option<airc_core::PeerId> {
    event
        .headers
        .get(HEADER_CONTINUUM_SOURCE_ID)
        .and_then(|value| uuid::Uuid::parse_str(value).ok()) // wire boundary: a header value is text
        .map(airc_core::PeerId::from_uuid)
}

/// The message id a continuum envelope is named by (`continuum.event_id`), when it is an id.
pub fn claimed_message(event: &TranscriptEvent) -> Option<uuid::Uuid> {
    event.headers.get(HEADER_CONTINUUM_EVENT_ID).and_then(|value| uuid::Uuid::parse_str(value).ok()) // wire boundary: a header value is text
}

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
        payload: std::sync::Arc::new(inline.clone()),
    })
}

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

/// Correlated command traffic belongs to the request/reply dispatcher, not room
/// attention. Inspect the routing header before decoding or queueing its body.
pub fn is_command_frame(event: &TranscriptEvent) -> bool {
    event.headers.get(airc_protocol::HEADER_AIRC_CORRELATION_ID).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_core::{ClientId, EventId, MentionTarget, PeerId, RoomId, TranscriptKind};

    fn event(headers: Headers) -> TranscriptEvent {
        TranscriptEvent {
            event_id: EventId::from_u128(1),
            room_id: RoomId::from_u128(2),
            peer_id: PeerId::from_u128(0x4e),
            client_id: ClientId::from_u128(4),
            kind: TranscriptKind::Message,
            occurred_at_ms: 100,
            lamport: 1,
            target: MentionTarget::All,
            headers,
            body: None,
            attachment: None,
            receipt: None,
            metadata: serde_json::Value::Null,
        }
    }

    // what this catches: a relayed human line attributed to the relay peer (she answers the wrong
    // person), or a non-id source fabricating a sender: the header names the logical sender; absent
    // or not an id, the transport peer is the author.
    #[test]
    fn the_logical_sender_is_read_from_the_header() {
        let human = uuid::Uuid::from_u128(0x5e);
        let relayed = event(Headers::from([(HEADER_CONTINUUM_SOURCE_ID.to_string(), human.to_string())]));
        assert_eq!(logical_sender(&relayed), human);
        assert_eq!(logical_sender(&event(Headers::new())), uuid::Uuid::from_u128(0x4e));
        let named = event(Headers::from([(HEADER_CONTINUUM_SOURCE_ID.to_string(), "persona-1".to_string())]));
        assert_eq!(logical_sender(&named), uuid::Uuid::from_u128(0x4e), "a non-id source is not a person");
    }
}
