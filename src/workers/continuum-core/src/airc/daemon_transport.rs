//! Daemon-backed realtime transport for Continuum AIRC envelopes.
//!
//! Continuum publishes structured events through the running AIRC daemon
//! using typed IPC requests. No shell command, no stdout parsing, no JSON
//! command adapter in the hot path.

use std::path::PathBuf;
use std::sync::Arc;

use airc_core::{Body, Headers, MentionTarget, RoomId};
use airc_ipc::{
    DaemonClient, InboxRequest, PublishRequest, PublishResponse, ResolveWireRequest,
    ResolveWireResponse,
};
use airc_protocol::{FrameKind, HEADER_FORGE_BODY_HINT};
use async_trait::async_trait;

use crate::airc::event_transport::AircEventTransport;
use crate::airc::realtime::AircRealtimeDelivery;
use crate::airc::realtime_store::{
    AircRealtimePublishParams, AircRealtimePublishResult, AircRealtimeReplayParams,
    AircRealtimeReplayResult, AircRealtimeStore, InMemoryAircRealtimeStore, MAX_ROOM_REPLAY_LIMIT,
};

const CONTINUUM_BODY_HINT: &str = "continuum.airc.realtime.envelope.v1";
const HEADER_CONTINUUM_EVENT_ID: &str = "continuum.event_id";
const HEADER_CONTINUUM_SOURCE_ID: &str = "continuum.source_id";
const HEADER_CONTINUUM_DELIVERY: &str = "continuum.delivery";
const HEADER_CONTINUUM_TRACE_ID: &str = "continuum.trace_id";

#[async_trait]
pub trait AircDaemonClient: Send + Sync {
    async fn resolve_wire(
        &self,
        request: ResolveWireRequest,
    ) -> Result<ResolveWireResponse, String>;

    async fn publish(&self, request: PublishRequest) -> Result<PublishResponse, String>;

    async fn inbox(&self, request: InboxRequest) -> Result<airc_ipc::InboxResponse, String>;
}

#[async_trait]
impl AircDaemonClient for DaemonClient {
    async fn resolve_wire(
        &self,
        request: ResolveWireRequest,
    ) -> Result<ResolveWireResponse, String> {
        DaemonClient::resolve_wire(self, request)
            .await
            .map_err(|error| error.to_string())
    }

    async fn publish(&self, request: PublishRequest) -> Result<PublishResponse, String> {
        DaemonClient::publish(self, request)
            .await
            .map_err(|error| error.to_string())
    }

    async fn inbox(&self, request: InboxRequest) -> Result<airc_ipc::InboxResponse, String> {
        DaemonClient::inbox(self, request)
            .await
            .map_err(|error| error.to_string())
    }
}

#[derive(Clone)]
pub struct DaemonAircEventTransport {
    client: Arc<dyn AircDaemonClient>,
}

impl DaemonAircEventTransport {
    pub fn new(socket_path: PathBuf) -> Self {
        Self::with_client(Arc::new(DaemonClient::new(socket_path)))
    }

    pub fn with_client(client: Arc<dyn AircDaemonClient>) -> Self {
        Self { client }
    }
}

#[async_trait]
impl AircEventTransport for DaemonAircEventTransport {
    async fn publish(
        &self,
        params: AircRealtimePublishParams,
    ) -> Result<AircRealtimePublishResult, String> {
        let envelope = params.envelope;
        envelope.validate_delivery()?;

        let wire = self.resolve_wire(envelope.room_id).await?;
        let publish = self
            .client
            .publish(PublishRequest {
                wire,
                channel: envelope.room_id,
                kind: frame_kind_for_delivery(envelope.delivery),
                target: MentionTarget::All,
                body: Body::Json(serde_json::to_value(&envelope).map_err(|error| {
                    format!("failed to encode continuum airc envelope: {error}")
                })?),
                headers: headers_for_envelope(&envelope),
            })
            .await?;

        Ok(AircRealtimePublishResult {
            ok: true,
            event_id: publish.event_id.to_string(),
            room_id: publish.channel_id.as_uuid(),
            delivery: envelope.delivery,
            stored_for_replay: matches!(
                envelope.delivery,
                AircRealtimeDelivery::Durable | AircRealtimeDelivery::Control
            ),
            coalesced_presence_key: None,
            replay_depth: 0,
            active_presence_count: 0,
            active_subscription_count: 0,
            active_peer_manifest_count: 0,
        })
    }

    async fn replay(
        &self,
        params: AircRealtimeReplayParams,
    ) -> Result<AircRealtimeReplayResult, String> {
        let response = self
            .client
            .inbox(InboxRequest {
                since: None,
                channel: Some(RoomId::from_uuid(params.room_id)),
                limit: Some(params.limit.unwrap_or(MAX_ROOM_REPLAY_LIMIT)),
            })
            .await?;

        let projection = InMemoryAircRealtimeStore::new(MAX_ROOM_REPLAY_LIMIT);
        for event in response.events {
            let Some(body) = event.body else {
                continue;
            };
            if event
                .headers
                .get(HEADER_FORGE_BODY_HINT)
                .map(String::as_str)
                != Some(CONTINUUM_BODY_HINT)
            {
                continue;
            }
            let Body::Json(value) = body else {
                continue;
            };
            let envelope = serde_json::from_value(value)
                .map_err(|error| format!("failed to decode continuum airc envelope: {error}"))?;
            projection.publish(AircRealtimePublishParams { envelope })?;
        }

        projection.replay(params)
    }
}

impl DaemonAircEventTransport {
    async fn resolve_wire(&self, room_id: uuid::Uuid) -> Result<PathBuf, String> {
        let response = self
            .client
            .resolve_wire(ResolveWireRequest { channel: room_id })
            .await?;
        response.wire.ok_or_else(|| {
            format!(
                "airc channel {room_id} is not joined in the daemon scope; run airc join before publishing"
            )
        })
    }
}

fn frame_kind_for_delivery(delivery: AircRealtimeDelivery) -> FrameKind {
    match delivery {
        AircRealtimeDelivery::Durable => FrameKind::Message,
        AircRealtimeDelivery::EphemeralCoalesced => FrameKind::Event,
        AircRealtimeDelivery::Control | AircRealtimeDelivery::ReceiptOnly => FrameKind::Control,
    }
}

fn headers_for_envelope(envelope: &crate::airc::realtime::AircRealtimeEnvelope) -> Headers {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airc::realtime::{
        AircRealtimeEnvelope, AircRealtimePayload, AircRealtimePayloadRef, AircRealtimeSchema,
    };
    use airc_core::{ClientId, EventId, PeerId, TranscriptEvent, TranscriptKind};
    use parking_lot::Mutex;
    use serde_json::json;
    use uuid::Uuid;

    #[derive(Default)]
    struct FakeDaemonClient {
        wire: Mutex<Option<PathBuf>>,
        publishes: Mutex<Vec<PublishRequest>>,
        inbox_events: Mutex<Vec<TranscriptEvent>>,
    }

    #[async_trait]
    impl AircDaemonClient for FakeDaemonClient {
        async fn resolve_wire(
            &self,
            _request: ResolveWireRequest,
        ) -> Result<ResolveWireResponse, String> {
            Ok(ResolveWireResponse {
                wire: self.wire.lock().clone(),
            })
        }

        async fn publish(&self, request: PublishRequest) -> Result<PublishResponse, String> {
            self.publishes.lock().push(request);
            Ok(PublishResponse {
                event_id: EventId::from_u128(0xfeed),
                lamport: 7,
                occurred_at_ms: 1000,
                channel_id: RoomId::from_u128(0xA1),
            })
        }

        async fn inbox(&self, _request: InboxRequest) -> Result<airc_ipc::InboxResponse, String> {
            Ok(airc_ipc::InboxResponse {
                events: self.inbox_events.lock().clone(),
                newest: None,
            })
        }
    }

    fn envelope(event_id: &str) -> AircRealtimeEnvelope {
        AircRealtimeEnvelope::new(
            event_id.to_string(),
            Uuid::from_u128(0xA1),
            "continuum".to_string(),
            100,
            AircRealtimePayload::ExistingSchema {
                payload: AircRealtimePayloadRef::inline(
                    AircRealtimeSchema::EventBridgePayload,
                    json!({"event": "persona.ready"}),
                ),
            },
        )
    }

    #[tokio::test]
    async fn publish_resolves_wire_then_sends_structured_body() {
        let fake = Arc::new(FakeDaemonClient::default());
        *fake.wire.lock() = Some(PathBuf::from("/tmp/airc-wire"));
        let transport = DaemonAircEventTransport::with_client(fake.clone());

        let result = transport
            .publish(AircRealtimePublishParams {
                envelope: envelope("evt-1"),
            })
            .await
            .unwrap();

        assert!(result.ok);
        let publishes = fake.publishes.lock();
        assert_eq!(publishes.len(), 1);
        assert_eq!(publishes[0].wire, PathBuf::from("/tmp/airc-wire"));
        assert_eq!(publishes[0].kind, FrameKind::Message);
        assert_eq!(
            publishes[0]
                .headers
                .get(HEADER_FORGE_BODY_HINT)
                .map(String::as_str),
            Some(CONTINUUM_BODY_HINT)
        );
    }

    #[tokio::test]
    async fn publish_fails_loud_when_room_is_not_joined() {
        let fake = Arc::new(FakeDaemonClient::default());
        let transport = DaemonAircEventTransport::with_client(fake);

        let error = transport
            .publish(AircRealtimePublishParams {
                envelope: envelope("evt-1"),
            })
            .await
            .unwrap_err();

        assert!(error.contains("not joined"));
    }

    #[tokio::test]
    async fn replay_decodes_only_continuum_body_hint_events() {
        let fake = Arc::new(FakeDaemonClient::default());
        let env = envelope("evt-1");
        let event = TranscriptEvent {
            event_id: EventId::from_u128(1),
            room_id: RoomId::from_uuid(env.room_id),
            peer_id: PeerId::from_u128(2),
            client_id: ClientId::from_u128(3),
            kind: TranscriptKind::Message,
            occurred_at_ms: 100,
            lamport: 1,
            target: MentionTarget::All,
            headers: headers_for_envelope(&env),
            body: Some(Body::Json(serde_json::to_value(&env).unwrap())),
            attachment: None,
            receipt: None,
            metadata: serde_json::Value::Null,
        };
        fake.inbox_events.lock().push(event);
        let transport = DaemonAircEventTransport::with_client(fake);

        let replay = transport
            .replay(AircRealtimeReplayParams {
                room_id: env.room_id,
                after_event_id: None,
                limit: Some(10),
                include_presence: None,
                include_subscriptions: None,
                include_peer_manifests: None,
                include_capability_index: None,
                now_ms: None,
            })
            .await
            .unwrap();

        assert_eq!(replay.events.len(), 1);
        assert_eq!(replay.events[0].event_id, "evt-1");
    }
}
