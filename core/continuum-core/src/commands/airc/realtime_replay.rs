//! `airc/realtime-replay` — replay bounded AIRC realtime envelopes for a room,
//! optionally including active coalesced presence, subscriptions, peer manifests,
//! and a capability-to-peer index.
//!
//! Dep-holding: captures the module's `Arc<dyn AircEventTransport>` seam. Gated
//! `AiSafe` — read-only over a room's realtime state, the natural way a citizen
//! catches up on presence/history after (re)joining.

use std::sync::Arc;

use crate::airc::{AircEventTransport, AircRealtimeReplayParams, AircRealtimeReplayResult};

crate::action_command! {
    /// Replay a bounded window of AIRC realtime envelopes for a room. Optionally
    /// folds in the active coalesced presence, subscriber projections, peer
    /// manifests, and a capability→peer index. The replay limit is clamped by the
    /// Rust adapter. Read-only.
    pub struct AircRealtimeReplay { event_transport: Arc<dyn AircEventTransport> }
    name: "airc/realtime-replay",
    access: AiSafe,
    params: AircRealtimeReplayParams,
    output: AircRealtimeReplayResult,
    run(this, _ctx, p) => {
        Ok(this.event_transport.replay(p).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airc::{
        AircPresenceEvent, AircPresenceState, AircRealtimeEnvelope, AircRealtimePayload,
        AircRealtimePublishParams, InMemoryAircRealtimeStore, StoreAircEventTransport,
    };
    use crate::sdk_codegen::{ActionCommand, Ctx};
    use uuid::Uuid;

    const TEST_ROOM_ID: Uuid = Uuid::from_u128(0xA1);

    // what this catches: name/access wiring — replay is read-only room state, so
    // it is AiSafe (the read sibling of the Privileged publish).
    #[test]
    fn name_and_access_wired() {
        assert_eq!(AircRealtimeReplay::NAME, "airc/realtime-replay");
        assert!(matches!(
            AircRealtimeReplay::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }

    // what this catches: a presence envelope published into the store is visible
    // through the replay command — active presence is folded in when requested,
    // and the ephemeral typing event is NOT in the durable events list. Exercises
    // the publish→replay roundtrip through the same transport seam the command holds.
    #[tokio::test]
    async fn replay_surfaces_active_presence_for_room() {
        let transport: Arc<dyn AircEventTransport> = Arc::new(StoreAircEventTransport::new(
            Arc::new(InMemoryAircRealtimeStore::default()),
        ));

        // Seed the store with a typing-presence envelope.
        transport
            .publish(AircRealtimePublishParams {
                envelope: AircRealtimeEnvelope::new(
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
                ),
            })
            .await
            .expect("seed publish must succeed");

        let cmd = AircRealtimeReplay {
            event_transport: transport,
        };
        let result = cmd
            .run(
                &Ctx::default(),
                AircRealtimeReplayParams {
                    room_id: TEST_ROOM_ID,
                    after_cursor: None,
                    limit: None,
                    include_presence: Some(true),
                    include_subscriptions: None,
                    include_peer_manifests: None,
                    include_capability_index: None,
                    now_ms: Some(499),
                },
            )
            .await
            .expect("replay must succeed");
        assert_eq!(result.room_id, TEST_ROOM_ID);
        assert!(
            result.events.is_empty(),
            "ephemeral typing presence is not a durable replay event"
        );
        assert_eq!(result.active_presence.len(), 1);
    }
}
