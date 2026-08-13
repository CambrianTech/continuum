//! Daemon-backed realtime transport for Continuum AIRC envelopes.
//!
//! Continuum publishes structured events through the running AIRC daemon
//! using typed IPC requests. No shell command, no stdout parsing, no JSON
//! command adapter in the hot path.
//!
//! ### v5 owner-core schema (task #82)
//!
//! The previous v4 IPC carried `Response::Event { event:
//! Box<TranscriptEvent> }`, `PublishRequest { wire, body }`, and
//! `InboxResponse.events`. v5 split the IPC wire vocabulary from the
//! SDK projection:
//!
//!   - `PublishRequest.payload: Vec<u8>` — opaque bytes the daemon
//!     never parses; consumer owns the codec (continuum uses
//!     `Body::to_payload`, which is JSON bytes round-trippable by any
//!     other airc consumer via `Body::from_payload`).
//!   - `PublishRequest.kind: IpcKind` — converted from continuum's
//!     `FrameKind` via the SDK-side `impl From` landed in airc#1096.
//!   - `PublishRequest.{from_peer, from_client}: Uuid` — caller
//!     identity. continuum discovers `from_peer` from the daemon's
//!     `Status` response at construction time (the scope's identity
//!     the daemon already holds); `from_client` is a fresh `Uuid::new_v4`
//!     per process startup so multi-tab attribution stays distinguishable.
//!   - `InboxResponse.envelopes: Vec<Vec<u8>>` — raw airc-wire bytes;
//!     decoded via `airc_lib::decode_wire_event` to get a
//!     `TranscriptEvent` we can project to continuum's envelope shape.
//!   - `InboxRequest.since: Option<IpcCursor>` — `TranscriptCursor →
//!     IpcCursor` via the airc#1096 `impl From`.
//!   - `ResolveWire`/`ResolveWireResponse`/`PublishRequest.wire` —
//!     removed. The owner-core daemon owns its channels; clients no
//!     longer ask "where's the file for this channel" because there's
//!     no file (router is in-memory). Continuum's old "not joined"
//!     gate is similarly gone — the daemon enforces channel membership
//!     internally and returns a structured error if the scope isn't in
//!     the requested channel.

use std::path::PathBuf;
use std::sync::Arc;

use airc_core::{MentionTarget, RoomId};
use airc_ipc::{DaemonClient, InboxRequest, IpcDelivery, PublishRequest, PublishResponse};
use airc_lib::decode_wire_event;
use async_trait::async_trait;
use uuid::Uuid;

use crate::airc::event_transport::AircEventTransport;
use crate::airc::realtime::AircRealtimeDelivery;
use crate::airc::realtime_store::{
    AircRealtimePublishParams, AircRealtimePublishResult, AircRealtimeReplayParams,
    AircRealtimeReplayResult, AircRealtimeStore, InMemoryAircRealtimeStore, MAX_ROOM_REPLAY_LIMIT,
};
use crate::airc::realtime_wire::{
    body_for_envelope, envelope_from_event, frame_kind_for_delivery, headers_for_envelope,
};

#[async_trait]
pub trait AircDaemonClient: Send + Sync {
    async fn publish(&self, request: PublishRequest) -> Result<PublishResponse, String>;

    async fn inbox(&self, request: InboxRequest) -> Result<airc_ipc::InboxResponse, String>;
}

#[async_trait]
impl AircDaemonClient for DaemonClient {
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
    /// Stable per-process identity for `PublishRequest.from_peer`.
    /// Discovered from the daemon's `Status` response at
    /// `AircModule::discover_and_construct` time; `Uuid::nil()` when
    /// the daemon was unreachable or returned no identity (degraded
    /// mode — publishes still succeed but attribution is anonymous).
    from_peer: Uuid,
    /// Fresh per-process client id distinguishing this continuum-core
    /// instance from other tabs/agents sharing the same `from_peer`.
    from_client: Uuid,
}

impl DaemonAircEventTransport {
    /// Construct against a real daemon socket with anonymous identity.
    /// Prefer [`Self::with_identity`] when the caller has discovered
    /// the scope's peer id (e.g. via the daemon's Status response).
    pub fn new(socket_path: PathBuf) -> Self {
        Self::with_client(Arc::new(DaemonClient::new(socket_path)))
    }

    pub fn with_client(client: Arc<dyn AircDaemonClient>) -> Self {
        Self::with_identity(client, Uuid::nil(), Uuid::new_v4())
    }

    pub fn with_identity(
        client: Arc<dyn AircDaemonClient>,
        from_peer: Uuid,
        from_client: Uuid,
    ) -> Self {
        Self {
            client,
            from_peer,
            from_client,
        }
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

        // Body → opaque payload bytes. The daemon never parses; any
        // airc consumer reading our publishes uses Body::from_payload
        // to project back to a typed Body. Same shape airc-lib's chat
        // helpers use, so continuum's messages remain interop with
        // `airc msg`/`airc inbox` readers.
        let body = body_for_envelope(&envelope)?;
        let payload = body.to_payload();

        let publish = self
            .client
            .publish(PublishRequest {
                channel: envelope.room_id,
                from_peer: self.from_peer,
                from_client: self.from_client,
                kind: frame_kind_for_delivery(envelope.delivery).into(),
                delivery: ipc_delivery_for(envelope.delivery),
                target: MentionTarget::All.into(),
                correlation_id: None,
                coalesce_key: None,
                payload,
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
                // TranscriptCursor → IpcCursor via the airc#1096 From
                // impl. `.transpose()?` keeps the `Option<Result<_,_>>`
                // pattern of the old code; `.map(Into::into)` then
                // does the type conversion.
                since: params
                    .after_cursor
                    .as_ref()
                    .map(|cursor| cursor.to_airc())
                    .transpose()?
                    .map(Into::into),
                channel: Some(RoomId::from_uuid(params.room_id)),
                limit: Some(params.limit.unwrap_or(MAX_ROOM_REPLAY_LIMIT)),
                // Replay is a cursor-resume of the FULL wire (durable
                // transcript hydration), not a perception page — no kinds
                // filter here. Perception's message-only page lives in
                // persona/airc_source.rs (#297).
                kinds: None,
            })
            .await?;

        // IpcCursor → TranscriptCursor via the airc#1096 From impl.
        let newest = response.newest.map(|cursor| {
            crate::airc::realtime::AircReplayCursor::from_airc(params.room_id, cursor.into())
        });

        let projection = InMemoryAircRealtimeStore::new(MAX_ROOM_REPLAY_LIMIT);
        for envelope_bytes in response.envelopes {
            // Decode wire bytes → TranscriptEvent (airc_lib helper),
            // then project to continuum envelope. Malformed bytes are
            // skipped rather than failing the whole replay — one bad
            // event shouldn't lose the page (the old typed-event path
            // had the same skip-on-projection-error semantic).
            let event = match decode_wire_event(envelope_bytes) {
                Ok(event) => event,
                Err(error) => {
                    tracing::warn!(%error, "Skipping malformed airc envelope in replay");
                    continue;
                }
            };
            let Some(envelope) = envelope_from_event(&event)? else {
                continue;
            };
            projection.publish(AircRealtimePublishParams { envelope })?;
        }

        let mut replay = projection.replay(AircRealtimeReplayParams {
            after_cursor: None,
            ..params
        })?;
        replay.cursor = newest;
        Ok(replay)
    }
}

/// Map continuum's high-level realtime delivery enum to the v5 airc
/// `IpcDelivery` vocabulary. Reflects the substrate retention
/// guarantees: Durable persists to the ORM; EphemeralCoalesced is
/// the latest-wins presence/typing class; ReceiptOnly is the
/// request-leg of an RPC pair.
fn ipc_delivery_for(delivery: AircRealtimeDelivery) -> IpcDelivery {
    match delivery {
        AircRealtimeDelivery::Durable => IpcDelivery::Durable,
        AircRealtimeDelivery::EphemeralCoalesced => IpcDelivery::EphemeralLatest,
        // Control frames carry small state updates that the chat client
        // still needs after restart; route durable so they survive in
        // scrollback. The daemon's router will deliver live to anyone
        // currently attached; the durable copy backs replay/inbox.
        AircRealtimeDelivery::Control => IpcDelivery::Durable,
        // ReceiptOnly is an acknowledgement; modeled as the
        // request-response leg so the daemon correlates it with the
        // original publish without persisting it as chat content.
        AircRealtimeDelivery::ReceiptOnly => IpcDelivery::RequestResponse,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airc::realtime::{
        AircRealtimeEnvelope, AircRealtimePayload, AircRealtimePayloadRef, AircRealtimeSchema,
    };
    use crate::airc::realtime_wire::CONTINUUM_BODY_HINT;
    use airc_core::{Body, EventId};
    use airc_ipc::{IpcKind, IpcTarget};
    use airc_protocol::HEADER_FORGE_BODY_HINT;
    use parking_lot::Mutex;
    use serde_json::json;
    use uuid::Uuid;

    // Round-trip wire-encode of envelopes is exercised by airc-ipc's
    // own sdk_conversions tests + airc-lib's decode_wire_event tests;
    // here we focus on continuum's substrate-boundary behavior — the
    // shape of `PublishRequest` and `InboxRequest` we hand the daemon.
    #[derive(Default)]
    struct FakeDaemonClient {
        publishes: Mutex<Vec<PublishRequest>>,
        inbox_requests: Mutex<Vec<InboxRequest>>,
        inbox_newest: Mutex<Option<airc_ipc::IpcCursor>>,
    }

    #[async_trait]
    impl AircDaemonClient for FakeDaemonClient {
        async fn publish(&self, request: PublishRequest) -> Result<PublishResponse, String> {
            self.publishes.lock().push(request);
            Ok(PublishResponse {
                event_id: EventId::from_u128(0xfeed),
                epoch: 0,
                counter: 7,
                occurred_at_ms: 1000,
                channel_id: RoomId::from_u128(0xA1),
            })
        }

        async fn inbox(&self, request: InboxRequest) -> Result<airc_ipc::InboxResponse, String> {
            self.inbox_requests.lock().push(request);
            Ok(airc_ipc::InboxResponse {
                envelopes: Vec::new(), // empty: we test cursor/request shape, not decode
                newest: *self.inbox_newest.lock(),
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
    async fn publish_sends_v5_shape_to_daemon() {
        let fake = Arc::new(FakeDaemonClient::default());
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
        // v5 PublishRequest fields we set: kind (via FrameKind::into),
        // target (via MentionTarget::into), delivery (Durable for
        // EventBridge), payload (Body → opaque bytes via to_payload).
        assert_eq!(publishes[0].kind, IpcKind::Message);
        assert_eq!(publishes[0].target, IpcTarget::All);
        assert_eq!(publishes[0].delivery, IpcDelivery::Durable);
        assert!(!publishes[0].payload.is_empty());
        // Body round-trip: published payload bytes decode back via
        // Body::from_payload — proves the JSON envelope is preserved
        // for downstream readers (airc msg / airc inbox).
        let _decoded = Body::from_payload(&publishes[0].payload).expect("body roundtrips");
        assert_eq!(
            publishes[0]
                .headers
                .get(HEADER_FORGE_BODY_HINT)
                .map(String::as_str),
            Some(CONTINUUM_BODY_HINT)
        );
    }

    #[tokio::test]
    async fn publish_propagates_identity_into_request() {
        let fake = Arc::new(FakeDaemonClient::default());
        let peer = Uuid::from_u128(0xDEAD);
        let client = Uuid::from_u128(0xBEEF);
        let transport = DaemonAircEventTransport::with_identity(fake.clone(), peer, client);

        transport
            .publish(AircRealtimePublishParams {
                envelope: envelope("evt-1"),
            })
            .await
            .unwrap();

        let publishes = fake.publishes.lock();
        assert_eq!(publishes[0].from_peer, peer);
        assert_eq!(publishes[0].from_client, client);
    }

    #[tokio::test]
    async fn replay_passes_cursor_through_as_ipc_cursor() {
        let fake = Arc::new(FakeDaemonClient::default());
        let env = envelope("evt-1");
        let since_event = EventId::from_u128(0x10);
        let newest_event = EventId::from_u128(0x20);
        // Daemon hands us an IpcCursor in `newest`; we convert it
        // back to TranscriptCursor + pack into our AircReplayCursor
        // via airc#1096's From impls.
        *fake.inbox_newest.lock() = Some(airc_ipc::IpcCursor {
            epoch: 0,
            counter: 9,
            event_id: newest_event,
        });
        let transport = DaemonAircEventTransport::with_client(fake.clone());

        let replay = transport
            .replay(AircRealtimeReplayParams {
                room_id: env.room_id,
                after_cursor: Some(crate::airc::realtime::AircReplayCursor {
                    room_id: env.room_id,
                    lamport: 4,
                    event_id: since_event.to_string(),
                    observed_at_ms: None,
                }),
                limit: Some(10),
                include_presence: None,
                include_subscriptions: None,
                include_peer_manifests: None,
                include_capability_index: None,
                now_ms: None,
            })
            .await
            .unwrap();

        let requests = fake.inbox_requests.lock();
        assert_eq!(requests.len(), 1);
        // TranscriptCursor { lamport: 4, event_id: since_event } →
        // IpcCursor { epoch: 0, counter: 4, event_id: since_event }
        // (lamport < COUNTER_MASK so epoch packs as 0).
        let since = requests[0].since.expect("cursor passed through");
        assert_eq!(since.epoch, 0);
        assert_eq!(since.counter, 4);
        assert_eq!(since.event_id, since_event);
        let cursor = replay.cursor.unwrap();
        assert_eq!(cursor.lamport, 9);
        assert_eq!(cursor.event_id, newest_event.to_string());
    }
}
