//! Inbound daemon attach stream for Continuum's event bus.
//!
//! This is the runtime half of AIRC realtime integration: the daemon owns
//! transport, trust, replay, and live delivery; Continuum subscribes through
//! typed IPC and republishes valid EventBridge envelopes into MessageBus.

use std::path::PathBuf;
use std::sync::Arc;

use airc_ipc::{codec::read_frame, AttachRequest, DaemonClient, Response};
use tracing::warn;

use crate::airc::realtime_wire::{bus_event_from_envelope, envelope_from_event};
use crate::runtime::MessageBus;

pub fn spawn_daemon_attach(
    socket_path: PathBuf,
    bus: Arc<MessageBus>,
    runtime: &tokio::runtime::Handle,
) {
    runtime.spawn(async move {
        if let Err(error) = run_daemon_attach(socket_path, bus).await {
            warn!("AIRC daemon attach stream stopped: {error}");
        }
    });
}

pub async fn run_daemon_attach(socket_path: PathBuf, bus: Arc<MessageBus>) -> Result<(), String> {
    let client = DaemonClient::new(socket_path);
    let mut stream = client
        .attach(AttachRequest::default())
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
        Response::Event { event } => publish_transcript_event(event.as_ref(), bus).await,
        Response::Error { message } => Err(message),
        Response::Pong
        | Response::Status(_)
        | Response::Inbox(_)
        | Response::Publish(_)
        | Response::ResolveWire(_)
        | Response::Peers(_) => Ok(()),
    }
}

pub async fn publish_transcript_event(
    event: &airc_core::TranscriptEvent,
    bus: &MessageBus,
) -> Result<(), String> {
    let envelope = match envelope_from_event(event) {
        Ok(Some(envelope)) => envelope,
        Ok(None) => return Ok(()),
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
