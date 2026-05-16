//! In-process realtime adapter for AIRC envelopes.
//!
//! This is the Continuum-side substrate surface before external AIRC transport
//! is attached. It keeps hot-path behavior Rust-owned: delivery validation,
//! bounded replay, receipt suppression, and coalesced ephemeral presence.

use crate::airc::realtime::{
    AircPresenceEvent, AircRealtimeDelivery, AircRealtimeEnvelope, AircRealtimePayload,
    AircReplayCursor,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use ts_rs::TS;

pub const DEFAULT_ROOM_REPLAY_LIMIT: usize = 100;
pub const MAX_ROOM_REPLAY_LIMIT: usize = 500;
pub const DEFAULT_EVENTS_PER_ROOM: usize = 2_000;

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/airc/AircRealtimePublishParams.ts"
)]
pub struct AircRealtimePublishParams {
    pub envelope: AircRealtimeEnvelope,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/airc/AircRealtimePublishResult.ts"
)]
pub struct AircRealtimePublishResult {
    pub ok: bool,
    pub event_id: String,
    pub room_id: String,
    pub delivery: AircRealtimeDelivery,
    pub stored_for_replay: bool,
    #[ts(optional)]
    pub coalesced_presence_key: Option<String>,
    pub replay_depth: usize,
    pub active_presence_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/airc/AircRealtimeReplayParams.ts"
)]
pub struct AircRealtimeReplayParams {
    pub room_id: String,
    #[ts(optional)]
    pub after_event_id: Option<String>,
    #[ts(optional)]
    pub limit: Option<usize>,
    #[ts(optional)]
    pub include_presence: Option<bool>,
    #[ts(optional)]
    pub now_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/airc/AircRealtimeReplayResult.ts"
)]
pub struct AircRealtimeReplayResult {
    pub room_id: String,
    pub events: Vec<AircRealtimeEnvelope>,
    #[ts(optional)]
    pub cursor: Option<AircReplayCursor>,
    pub active_presence: Vec<AircPresenceEvent>,
}

pub trait AircRealtimeStore: Send + Sync {
    fn publish(
        &self,
        params: AircRealtimePublishParams,
    ) -> Result<AircRealtimePublishResult, String>;
    fn replay(&self, params: AircRealtimeReplayParams) -> Result<AircRealtimeReplayResult, String>;
}

#[derive(Debug)]
pub struct InMemoryAircRealtimeStore {
    max_events_per_room: usize,
    inner: Mutex<AircRealtimeState>,
}

#[derive(Debug, Default)]
struct AircRealtimeState {
    rooms: HashMap<String, VecDeque<AircRealtimeEnvelope>>,
    presence: HashMap<String, AircRealtimeEnvelope>,
}

impl Default for InMemoryAircRealtimeStore {
    fn default() -> Self {
        Self::new(DEFAULT_EVENTS_PER_ROOM)
    }
}

impl InMemoryAircRealtimeStore {
    pub fn new(max_events_per_room: usize) -> Self {
        Self {
            max_events_per_room: max_events_per_room.max(1),
            inner: Mutex::new(AircRealtimeState::default()),
        }
    }
}

impl AircRealtimeStore for InMemoryAircRealtimeStore {
    fn publish(
        &self,
        params: AircRealtimePublishParams,
    ) -> Result<AircRealtimePublishResult, String> {
        let envelope = params.envelope;
        envelope.validate_delivery()?;

        let mut state = self.inner.lock();
        state.prune_expired_presence(envelope.created_at_ms);

        let room_id = envelope.room_id.clone();
        let event_id = envelope.event_id.clone();
        let delivery = envelope.delivery;
        let mut coalesced_presence_key = None;

        let stored_for_replay = match &envelope.payload {
            AircRealtimePayload::Presence { event } => {
                let key = event.coalesce_key();
                state.presence.insert(key.clone(), envelope.clone());
                coalesced_presence_key = Some(key);
                !matches!(delivery, AircRealtimeDelivery::EphemeralCoalesced)
            }
            AircRealtimePayload::Receipt { .. } => false,
            AircRealtimePayload::ExistingSchema { .. }
            | AircRealtimePayload::Subscription { .. }
            | AircRealtimePayload::MediaControl { .. } => true,
        };

        if stored_for_replay {
            state.push_replay(envelope, self.max_events_per_room);
        }

        let replay_depth = state
            .rooms
            .get(&room_id)
            .map(VecDeque::len)
            .unwrap_or_default();
        let active_presence_count = state.active_presence_for_room(&room_id).len();

        Ok(AircRealtimePublishResult {
            ok: true,
            event_id,
            room_id,
            delivery,
            stored_for_replay,
            coalesced_presence_key,
            replay_depth,
            active_presence_count,
        })
    }

    fn replay(&self, params: AircRealtimeReplayParams) -> Result<AircRealtimeReplayResult, String> {
        validate_room_id(&params.room_id)?;

        let limit = params
            .limit
            .unwrap_or(DEFAULT_ROOM_REPLAY_LIMIT)
            .clamp(1, MAX_ROOM_REPLAY_LIMIT);
        let mut state = self.inner.lock();
        if let Some(now_ms) = params.now_ms {
            state.prune_expired_presence(now_ms);
        }

        let events = state.replay_room(&params.room_id, params.after_event_id.as_deref(), limit);
        let cursor = events.last().map(|event| AircReplayCursor {
            room_id: params.room_id.clone(),
            last_seen_event_id: event.event_id.clone(),
            last_seen_at_ms: Some(event.created_at_ms),
        });
        let active_presence = if params.include_presence.unwrap_or(false) {
            state
                .active_presence_for_room(&params.room_id)
                .into_iter()
                .collect()
        } else {
            Vec::new()
        };

        Ok(AircRealtimeReplayResult {
            room_id: params.room_id,
            events,
            cursor,
            active_presence,
        })
    }
}

impl AircRealtimeState {
    fn push_replay(&mut self, envelope: AircRealtimeEnvelope, max_events_per_room: usize) {
        let room = self.rooms.entry(envelope.room_id.clone()).or_default();
        room.push_back(envelope);
        while room.len() > max_events_per_room {
            room.pop_front();
        }
    }

    fn replay_room(
        &self,
        room_id: &str,
        after_event_id: Option<&str>,
        limit: usize,
    ) -> Vec<AircRealtimeEnvelope> {
        let Some(room) = self.rooms.get(room_id) else {
            return Vec::new();
        };
        let start = after_event_id
            .and_then(|id| room.iter().position(|event| event.event_id == id))
            .map(|idx| idx + 1)
            .unwrap_or(0);
        room.iter().skip(start).take(limit).cloned().collect()
    }

    fn active_presence_for_room(&self, room_id: &str) -> Vec<AircPresenceEvent> {
        self.presence
            .values()
            .filter(|envelope| envelope.room_id == room_id)
            .filter_map(|envelope| match &envelope.payload {
                AircRealtimePayload::Presence { event } => Some(event.clone()),
                _ => None,
            })
            .collect()
    }

    fn prune_expired_presence(&mut self, now_ms: u64) {
        self.presence.retain(|_, envelope| match &envelope.payload {
            AircRealtimePayload::Presence { event } => !event.is_expired_at(now_ms),
            _ => true,
        });
    }
}

fn validate_room_id(room_id: &str) -> Result<(), String> {
    if room_id.trim().is_empty() {
        Err("room_id must not be empty".to_string())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airc::realtime::{
        AircPresenceState, AircRealtimePayloadRef, AircRealtimeSchema, AircSubscriptionAction,
        AircSubscriptionEvent,
    };
    use serde_json::json;

    fn durable_event(id: &str, room: &str, created_at_ms: u64) -> AircRealtimeEnvelope {
        AircRealtimeEnvelope::new(
            id.to_string(),
            room.to_string(),
            "node-a".to_string(),
            created_at_ms,
            AircRealtimePayload::ExistingSchema {
                payload: AircRealtimePayloadRef::inline(
                    AircRealtimeSchema::ChatTranscript,
                    json!({"text": id}),
                ),
            },
        )
    }

    fn typing_event(id: &str, started_at_ms: u64, expires_at_ms: u64) -> AircRealtimeEnvelope {
        AircRealtimeEnvelope::new(
            id.to_string(),
            "general".to_string(),
            "persona-1".to_string(),
            started_at_ms,
            AircRealtimePayload::Presence {
                event: AircPresenceEvent {
                    room_id: "general".to_string(),
                    subject_id: "persona-1".to_string(),
                    display_name: None,
                    state: AircPresenceState::Typing,
                    started_at_ms,
                    expires_at_ms: Some(expires_at_ms),
                    call_id: None,
                },
            },
        )
    }

    #[test]
    fn durable_events_replay_from_cursor() {
        let store = InMemoryAircRealtimeStore::new(10);
        for idx in 1..=3 {
            store
                .publish(AircRealtimePublishParams {
                    envelope: durable_event(&format!("evt-{idx}"), "general", idx),
                })
                .unwrap();
        }

        let result = store
            .replay(AircRealtimeReplayParams {
                room_id: "general".to_string(),
                after_event_id: Some("evt-1".to_string()),
                limit: Some(10),
                include_presence: None,
                now_ms: None,
            })
            .unwrap();

        assert_eq!(
            result
                .events
                .iter()
                .map(|event| event.event_id.as_str())
                .collect::<Vec<_>>(),
            ["evt-2", "evt-3"]
        );
        assert_eq!(
            result.cursor.unwrap().last_seen_event_id,
            "evt-3".to_string()
        );
    }

    #[test]
    fn ephemeral_presence_coalesces_and_expires_without_replay_pollution() {
        let store = InMemoryAircRealtimeStore::new(10);
        let first = store
            .publish(AircRealtimePublishParams {
                envelope: typing_event("typing-1", 100, 200),
            })
            .unwrap();
        let second = store
            .publish(AircRealtimePublishParams {
                envelope: typing_event("typing-2", 120, 240),
            })
            .unwrap();

        assert!(!first.stored_for_replay);
        assert!(!second.stored_for_replay);
        assert_eq!(second.active_presence_count, 1);

        let live = store
            .replay(AircRealtimeReplayParams {
                room_id: "general".to_string(),
                after_event_id: None,
                limit: None,
                include_presence: Some(true),
                now_ms: Some(239),
            })
            .unwrap();
        assert!(live.events.is_empty());
        assert_eq!(live.active_presence.len(), 1);
        assert_eq!(live.active_presence[0].started_at_ms, 120);

        let expired = store
            .replay(AircRealtimeReplayParams {
                room_id: "general".to_string(),
                after_event_id: None,
                limit: None,
                include_presence: Some(true),
                now_ms: Some(240),
            })
            .unwrap();
        assert!(expired.active_presence.is_empty());
    }

    #[test]
    fn receipt_only_messages_are_not_replayed() {
        let store = InMemoryAircRealtimeStore::new(10);
        let mut receipt = AircRealtimeEnvelope::new(
            "receipt-1".to_string(),
            "general".to_string(),
            "peer-1".to_string(),
            10,
            AircRealtimePayload::Receipt {
                receipt: crate::airc::realtime::AircReceipt {
                    event_id: "evt-1".to_string(),
                    peer_id: "peer-1".to_string(),
                    received_at_ms: 10,
                    replay_cursor: None,
                },
            },
        );
        receipt.delivery = AircRealtimeDelivery::ReceiptOnly;

        let result = store
            .publish(AircRealtimePublishParams { envelope: receipt })
            .unwrap();
        assert!(!result.stored_for_replay);

        let replay = store
            .replay(AircRealtimeReplayParams {
                room_id: "general".to_string(),
                after_event_id: None,
                limit: None,
                include_presence: None,
                now_ms: None,
            })
            .unwrap();
        assert!(replay.events.is_empty());
    }

    #[test]
    fn control_messages_are_replayable_for_reconnect() {
        let store = InMemoryAircRealtimeStore::new(10);
        let envelope = AircRealtimeEnvelope::new(
            "sub-1".to_string(),
            "general".to_string(),
            "browser-1".to_string(),
            10,
            AircRealtimePayload::Subscription {
                event: AircSubscriptionEvent {
                    action: AircSubscriptionAction::Subscribe,
                    room_id: "general".to_string(),
                    subscriber_id: "browser-1".to_string(),
                    topic: "presence".to_string(),
                    cursor: None,
                },
            },
        );

        let publish = store
            .publish(AircRealtimePublishParams { envelope })
            .unwrap();
        assert_eq!(publish.delivery, AircRealtimeDelivery::Control);
        assert!(publish.stored_for_replay);
    }
}
