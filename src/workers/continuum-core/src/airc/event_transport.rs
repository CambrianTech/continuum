//! Typed event transport seam for Continuum realtime envelopes.
//!
//! Command modules and future bridge loops should depend on this trait,
//! not on a concrete store or a CLI command. The first implementation is
//! store-backed so tests and local runtime keep deterministic replay;
//! later implementations can publish to the AIRC SDK/daemon without
//! changing command surfaces.

use std::sync::Arc;

use crate::airc::realtime_store::{
    AircRealtimePublishParams, AircRealtimePublishResult, AircRealtimeReplayParams,
    AircRealtimeReplayResult, AircRealtimeStore,
};

pub trait AircEventTransport: Send + Sync {
    fn publish(
        &self,
        params: AircRealtimePublishParams,
    ) -> Result<AircRealtimePublishResult, String>;

    fn replay(&self, params: AircRealtimeReplayParams) -> Result<AircRealtimeReplayResult, String>;
}

#[derive(Clone)]
pub struct StoreAircEventTransport {
    store: Arc<dyn AircRealtimeStore>,
}

impl StoreAircEventTransport {
    pub fn new(store: Arc<dyn AircRealtimeStore>) -> Self {
        Self { store }
    }
}

impl AircEventTransport for StoreAircEventTransport {
    fn publish(
        &self,
        params: AircRealtimePublishParams,
    ) -> Result<AircRealtimePublishResult, String> {
        self.store.publish(params)
    }

    fn replay(&self, params: AircRealtimeReplayParams) -> Result<AircRealtimeReplayResult, String> {
        self.store.replay(params)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airc::{
        AircRealtimeEnvelope, AircRealtimePayload, AircRealtimePayloadRef, AircRealtimeSchema,
        InMemoryAircRealtimeStore,
    };
    use serde_json::json;
    use uuid::Uuid;

    #[test]
    fn store_transport_round_trips_without_cli_output_parsing() {
        let transport =
            StoreAircEventTransport::new(Arc::new(InMemoryAircRealtimeStore::default()));
        let room_id = Uuid::from_u128(0xA1);
        let envelope = AircRealtimeEnvelope::new(
            "evt-1".to_string(),
            room_id,
            "continuum".to_string(),
            100,
            AircRealtimePayload::ExistingSchema {
                payload: AircRealtimePayloadRef::inline(
                    AircRealtimeSchema::EventBridgePayload,
                    json!({"event": "persona.ready"}),
                ),
            },
        );

        let publish = transport
            .publish(AircRealtimePublishParams { envelope })
            .unwrap();
        assert!(publish.stored_for_replay);

        let replay = transport
            .replay(AircRealtimeReplayParams {
                room_id,
                after_event_id: None,
                limit: Some(10),
                include_presence: None,
                include_subscriptions: None,
                include_peer_manifests: None,
                include_capability_index: None,
                now_ms: None,
            })
            .unwrap();

        assert_eq!(replay.events.len(), 1);
        assert_eq!(replay.events[0].event_id, "evt-1");
    }
}
