//! Shared AIRC wire contract for Continuum realtime envelopes.
//!
//! Publish, replay, and live attach all use these helpers so the
//! `forge.body_hint` contract has one definition.

use airc_core::{Body, Headers, TranscriptEvent};
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

    serde_json::from_value(value.clone())
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
