//! `airc/realtime-publish` — publish a typed AIRC realtime envelope (presence,
//! peer manifest, subscription, media control, receipt, or a wrapped existing
//! schema) into the Rust replay/presence adapter.
//!
//! Dep-holding: captures the module's `Arc<dyn AircEventTransport>` seam. Gated
//! `Privileged` — the envelope carries trust-relevant fields (`source_id`
//! attribution, `PeerManifest` signing keys), so writing one is not a surface
//! untrusted callers should reach.

use std::sync::Arc;

use crate::airc::{AircEventTransport, AircRealtimePublishParams, AircRealtimePublishResult};

crate::action_command! {
    /// Publish a typed AIRC realtime envelope into the Rust replay/presence
    /// adapter. The envelope's delivery semantics (durable, ephemeral-coalesced,
    /// receipt-only, control) are derived from its payload kind. Returns the
    /// stored event id, the resolved delivery, and live room counters.
    pub struct AircRealtimePublish { event_transport: Arc<dyn AircEventTransport> }
    name: "airc/realtime-publish",
    access: Privileged,
    params: AircRealtimePublishParams,
    output: AircRealtimePublishResult,
    run(this, _ctx, p) => {
        Ok(this.event_transport.publish(p).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airc::{
        AircPresenceEvent, AircPresenceState, AircRealtimeEnvelope, AircRealtimePayload,
        InMemoryAircRealtimeStore, StoreAircEventTransport,
    };
    use crate::sdk_codegen::{ActionCommand, Ctx};
    use uuid::Uuid;

    const TEST_ROOM_ID: Uuid = Uuid::from_u128(0xA1);

    fn typing_envelope() -> AircRealtimeEnvelope {
        AircRealtimeEnvelope::new(
            "typing-1".to_string(),
            TEST_ROOM_ID,
            "persona-1".to_string(),
            100,
            AircRealtimePayload::Presence {
                event: AircPresenceEvent {
                    room_id: TEST_ROOM_ID,
                    subject_id: "persona-1".to_string(),
                    display_name: None,
                    state: AircPresenceState::Typing,
                    started_at_ms: 100,
                    expires_at_ms: Some(500),
                    call_id: None,
                },
            },
        )
    }

    // what this catches: name/access wiring — publishing trust-relevant envelopes
    // is Privileged, not AiSafe.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(AircRealtimePublish::NAME, "airc/realtime-publish");
        assert!(matches!(
            AircRealtimePublish::ACCESS,
            crate::sdk_codegen::AccessLevel::Privileged
        ));
    }

    // what this catches: the command routes the envelope through the event
    // transport seam and returns the adapter's result. A typing-presence envelope
    // is ephemeral-coalesced (NOT stored for replay) and registers one active
    // presence — the substrate's delivery derivation, surfaced end-to-end.
    #[tokio::test]
    async fn publish_routes_through_transport_and_reports_counters() {
        let transport: Arc<dyn AircEventTransport> = Arc::new(StoreAircEventTransport::new(
            Arc::new(InMemoryAircRealtimeStore::default()),
        ));
        let cmd = AircRealtimePublish {
            event_transport: transport,
        };
        let result = cmd
            .run(
                &Ctx::default(),
                AircRealtimePublishParams {
                    envelope: typing_envelope(),
                },
            )
            .await
            .expect("publish must succeed");
        assert!(result.ok);
        assert_eq!(result.event_id, "typing-1");
        assert!(!result.stored_for_replay, "typing presence is ephemeral");
        assert_eq!(result.active_presence_count, 1);
    }
}
