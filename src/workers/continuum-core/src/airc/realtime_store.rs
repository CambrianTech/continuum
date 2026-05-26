//! In-process realtime adapter for AIRC envelopes.
//!
//! This is the Continuum-side substrate surface before external AIRC transport
//! is attached. It keeps hot-path behavior Rust-owned: delivery validation,
//! bounded replay, receipt suppression, and coalesced ephemeral presence.

use crate::airc::realtime::{
    AircPeerManifest, AircPresenceEvent, AircRealtimeDelivery, AircRealtimeEnvelope,
    AircRealtimePayload, AircReplayCursor, AircSubscriptionAction, AircSubscriptionEvent,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use ts_rs::TS;
use uuid::Uuid;

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
    #[ts(type = "string")]
    pub room_id: Uuid,
    pub delivery: AircRealtimeDelivery,
    pub stored_for_replay: bool,
    #[ts(optional)]
    pub coalesced_presence_key: Option<String>,
    pub replay_depth: usize,
    pub active_presence_count: usize,
    pub active_subscription_count: usize,
    pub active_peer_manifest_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/airc/AircRealtimeReplayParams.ts"
)]
pub struct AircRealtimeReplayParams {
    #[ts(type = "string")]
    pub room_id: Uuid,
    #[ts(optional)]
    pub after_cursor: Option<AircReplayCursor>,
    #[ts(optional)]
    pub limit: Option<usize>,
    #[ts(optional)]
    pub include_presence: Option<bool>,
    #[ts(optional)]
    pub include_subscriptions: Option<bool>,
    #[ts(optional)]
    pub include_peer_manifests: Option<bool>,
    #[ts(optional)]
    pub include_capability_index: Option<bool>,
    #[ts(optional)]
    pub now_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/airc/AircCapabilityIndexEntry.ts"
)]
pub struct AircCapabilityIndexEntry {
    pub capability_id: String,
    pub peer_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/airc/AircRealtimeReplayResult.ts"
)]
pub struct AircRealtimeReplayResult {
    #[ts(type = "string")]
    pub room_id: Uuid,
    pub events: Vec<AircRealtimeEnvelope>,
    #[ts(optional)]
    pub cursor: Option<AircReplayCursor>,
    pub active_presence: Vec<AircPresenceEvent>,
    pub active_subscriptions: Vec<AircSubscriptionEvent>,
    pub active_peer_manifests: Vec<AircPeerManifest>,
    pub capability_index: Vec<AircCapabilityIndexEntry>,
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
    rooms: HashMap<Uuid, VecDeque<StoredRealtimeEnvelope>>,
    room_lamports: HashMap<Uuid, u64>,
    presence: HashMap<String, AircRealtimeEnvelope>,
    peer_manifests: HashMap<String, AircRealtimeEnvelope>,
    subscriptions: HashMap<String, AircSubscriptionEvent>,
}

#[derive(Debug, Clone)]
struct StoredRealtimeEnvelope {
    envelope: AircRealtimeEnvelope,
    cursor: AircReplayCursor,
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
        validate_room_id(envelope.room_id)?;
        envelope.validate_delivery()?;

        let mut state = self.inner.lock();
        state.prune_expired_presence(envelope.created_at_ms);

        let room_id = envelope.room_id;
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
            AircRealtimePayload::PeerManifest { manifest } => {
                let key = manifest.coalesce_key();
                state.peer_manifests.insert(key.clone(), envelope.clone());
                coalesced_presence_key = Some(key);
                false
            }
            AircRealtimePayload::Subscription { event } => {
                state.apply_subscription(event);
                true
            }
            AircRealtimePayload::Receipt { .. } => false,
            AircRealtimePayload::ExistingSchema { .. }
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
        let active_presence_count = state.active_presence_for_room(room_id).len();
        let active_subscription_count = state.active_subscriptions_for_room(room_id).len();
        let active_peer_manifest_count = state.active_peer_manifests_for_room(room_id).len();

        Ok(AircRealtimePublishResult {
            ok: true,
            event_id,
            room_id,
            delivery,
            stored_for_replay,
            coalesced_presence_key,
            replay_depth,
            active_presence_count,
            active_subscription_count,
            active_peer_manifest_count,
        })
    }

    fn replay(&self, params: AircRealtimeReplayParams) -> Result<AircRealtimeReplayResult, String> {
        validate_room_id(params.room_id)?;

        let limit = params
            .limit
            .unwrap_or(DEFAULT_ROOM_REPLAY_LIMIT)
            .clamp(1, MAX_ROOM_REPLAY_LIMIT);
        let mut state = self.inner.lock();
        if let Some(now_ms) = params.now_ms {
            state.prune_expired_presence(now_ms);
        }

        let events = state.replay_room(params.room_id, params.after_cursor.as_ref(), limit);
        let cursor = events.last().map(|event| event.cursor.clone());
        let active_presence = if params.include_presence.unwrap_or(false) {
            state
                .active_presence_for_room(params.room_id)
                .into_iter()
                .collect()
        } else {
            Vec::new()
        };
        let active_subscriptions = if params.include_subscriptions.unwrap_or(false) {
            state.active_subscriptions_for_room(params.room_id)
        } else {
            Vec::new()
        };
        let active_peer_manifests = if params.include_peer_manifests.unwrap_or(false) {
            state.active_peer_manifests_for_room(params.room_id)
        } else {
            Vec::new()
        };
        let capability_index = if params.include_capability_index.unwrap_or(false) {
            capability_index_for_manifests(&active_peer_manifests)
        } else {
            Vec::new()
        };

        Ok(AircRealtimeReplayResult {
            room_id: params.room_id,
            events: events.into_iter().map(|event| event.envelope).collect(),
            cursor,
            active_presence,
            active_subscriptions,
            active_peer_manifests,
            capability_index,
        })
    }
}

impl AircRealtimeState {
    fn push_replay(&mut self, envelope: AircRealtimeEnvelope, max_events_per_room: usize) {
        let next_lamport = self.room_lamports.entry(envelope.room_id).or_default();
        *next_lamport += 1;
        let cursor = AircReplayCursor {
            room_id: envelope.room_id,
            lamport: *next_lamport,
            event_id: envelope.event_id.clone(),
            observed_at_ms: Some(envelope.created_at_ms),
        };
        let room = self.rooms.entry(envelope.room_id).or_default();
        room.push_back(StoredRealtimeEnvelope { envelope, cursor });
        while room.len() > max_events_per_room {
            room.pop_front();
        }
    }

    fn replay_room(
        &self,
        room_id: Uuid,
        after_cursor: Option<&AircReplayCursor>,
        limit: usize,
    ) -> Vec<StoredRealtimeEnvelope> {
        let Some(room) = self.rooms.get(&room_id) else {
            return Vec::new();
        };
        room.iter()
            .filter(|event| {
                after_cursor
                    .map(|cursor| cursor.strictly_before(&event.cursor))
                    .unwrap_or(true)
            })
            .take(limit)
            .cloned()
            .collect()
    }

    fn active_presence_for_room(&self, room_id: Uuid) -> Vec<AircPresenceEvent> {
        self.presence
            .values()
            .filter(|envelope| envelope.room_id == room_id)
            .filter_map(|envelope| match &envelope.payload {
                AircRealtimePayload::Presence { event } => Some(event.clone()),
                _ => None,
            })
            .collect()
    }

    fn apply_subscription(&mut self, event: &AircSubscriptionEvent) {
        let key = event.coalesce_key();
        match event.action {
            AircSubscriptionAction::Subscribe | AircSubscriptionAction::Replay => {
                self.subscriptions.insert(key, event.clone());
            }
            AircSubscriptionAction::Unsubscribe => {
                self.subscriptions.remove(&key);
            }
            AircSubscriptionAction::Ack => {}
        }
    }

    fn active_subscriptions_for_room(&self, room_id: Uuid) -> Vec<AircSubscriptionEvent> {
        let mut subscriptions = self
            .subscriptions
            .values()
            .filter(|event| event.room_id == room_id)
            .cloned()
            .collect::<Vec<_>>();
        subscriptions.sort_by(|a, b| {
            a.subscriber_id
                .cmp(&b.subscriber_id)
                .then_with(|| a.topic.cmp(&b.topic))
        });
        subscriptions
    }

    fn active_peer_manifests_for_room(&self, room_id: Uuid) -> Vec<AircPeerManifest> {
        let mut manifests = self
            .peer_manifests
            .values()
            .filter_map(|envelope| match &envelope.payload {
                AircRealtimePayload::PeerManifest { manifest } => Some(manifest.clone()),
                _ => None,
            })
            .filter(|manifest| manifest.advertises_room(room_id))
            .collect::<Vec<_>>();
        manifests.sort_by(|a, b| a.peer_id.cmp(&b.peer_id));
        manifests
    }

    fn prune_expired_presence(&mut self, now_ms: u64) {
        self.presence.retain(|_, envelope| match &envelope.payload {
            AircRealtimePayload::Presence { event } => !event.is_expired_at(now_ms),
            _ => true,
        });
        self.peer_manifests
            .retain(|_, envelope| match &envelope.payload {
                AircRealtimePayload::PeerManifest { manifest } => !manifest.is_expired_at(now_ms),
                _ => true,
            });
    }
}

fn capability_index_for_manifests(manifests: &[AircPeerManifest]) -> Vec<AircCapabilityIndexEntry> {
    let mut index: HashMap<String, Vec<String>> = HashMap::new();
    for manifest in manifests {
        for capability in &manifest.capabilities {
            index
                .entry(capability.id.clone())
                .or_default()
                .push(manifest.peer_id.clone());
        }
    }

    let mut entries = index
        .into_iter()
        .map(|(capability_id, mut peer_ids)| {
            peer_ids.sort();
            peer_ids.dedup();
            AircCapabilityIndexEntry {
                capability_id,
                peer_ids,
            }
        })
        .collect::<Vec<_>>();
    entries.sort_by(|a, b| a.capability_id.cmp(&b.capability_id));
    entries
}

fn validate_room_id(room_id: Uuid) -> Result<(), String> {
    if room_id.is_nil() {
        Err("room_id must not be the nil UUID".to_string())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airc::realtime::{
        AircPeerCapability, AircPresenceState, AircRealtimePayloadRef, AircRealtimeSchema,
        AircSubscriptionAction, AircSubscriptionEvent,
    };
    use serde_json::json;

    const GENERAL: Uuid = Uuid::from_u128(0xA1);
    const CAMBRIANTECH: Uuid = Uuid::from_u128(0xA2);
    const OTHER: Uuid = Uuid::from_u128(0xA3);

    fn durable_event(id: &str, room: Uuid, created_at_ms: u64) -> AircRealtimeEnvelope {
        AircRealtimeEnvelope::new(
            id.to_string(),
            room,
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
            GENERAL,
            "persona-1".to_string(),
            started_at_ms,
            AircRealtimePayload::Presence {
                event: AircPresenceEvent {
                    room_id: GENERAL,
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

    fn peer_manifest_event(
        id: &str,
        peer_id: &str,
        rooms: &[Uuid],
        capabilities: &[&str],
        advertised_at_ms: u64,
        expires_at_ms: Option<u64>,
    ) -> AircRealtimeEnvelope {
        AircRealtimeEnvelope::new(
            id.to_string(),
            GENERAL,
            peer_id.to_string(),
            advertised_at_ms,
            AircRealtimePayload::PeerManifest {
                manifest: AircPeerManifest {
                    peer_id: peer_id.to_string(),
                    display_name: Some(peer_id.to_string()),
                    room_ids: rooms.to_vec(),
                    capabilities: capabilities
                        .iter()
                        .map(|id| AircPeerCapability {
                            id: (*id).to_string(),
                            label: None,
                            version: None,
                        })
                        .collect(),
                    // Structural-only sample pubkey (passes hex/length
                    // checks; not a real key). Multi-peer tests should
                    // pass per-peer overrides if equality matters.
                    signing_pubkey_hex:
                        "1112131415161718191a1b1c1d1e1f202122232425262728292a2b2c2d2e2f30"
                            .to_string(),
                    advertised_at_ms,
                    expires_at_ms,
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
                    envelope: durable_event(&format!("evt-{idx}"), GENERAL, idx),
                })
                .unwrap();
        }

        let result = store
            .replay(AircRealtimeReplayParams {
                room_id: GENERAL,
                after_cursor: Some(AircReplayCursor {
                    room_id: GENERAL,
                    lamport: 1,
                    event_id: "evt-1".to_string(),
                    observed_at_ms: Some(1),
                }),
                limit: Some(10),
                include_presence: None,
                include_subscriptions: None,
                include_peer_manifests: None,
                include_capability_index: None,
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
        assert_eq!(result.cursor.unwrap().event_id, "evt-3".to_string());
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
                room_id: GENERAL,
                after_cursor: None,
                limit: None,
                include_presence: Some(true),
                include_subscriptions: None,
                include_peer_manifests: None,
                include_capability_index: None,
                now_ms: Some(239),
            })
            .unwrap();
        assert!(live.events.is_empty());
        assert_eq!(live.active_presence.len(), 1);
        assert_eq!(live.active_presence[0].started_at_ms, 120);

        let expired = store
            .replay(AircRealtimeReplayParams {
                room_id: GENERAL,
                after_cursor: None,
                limit: None,
                include_presence: Some(true),
                include_subscriptions: None,
                include_peer_manifests: None,
                include_capability_index: None,
                now_ms: Some(240),
            })
            .unwrap();
        assert!(expired.active_presence.is_empty());
    }

    #[test]
    fn peer_manifest_coalesces_indexes_capabilities_and_stays_out_of_replay() {
        let store = InMemoryAircRealtimeStore::new(10);
        let first = store
            .publish(AircRealtimePublishParams {
                envelope: peer_manifest_event(
                    "manifest-1",
                    "peer-a",
                    &[GENERAL],
                    &["continuum.lora.invoke"],
                    100,
                    Some(500),
                ),
            })
            .unwrap();
        let second = store
            .publish(AircRealtimePublishParams {
                envelope: peer_manifest_event(
                    "manifest-2",
                    "peer-a",
                    &[GENERAL, CAMBRIANTECH],
                    &["continuum.lora.invoke", "continuum.chat.turn"],
                    150,
                    Some(600),
                ),
            })
            .unwrap();
        store
            .publish(AircRealtimePublishParams {
                envelope: peer_manifest_event(
                    "manifest-3",
                    "peer-b",
                    &[GENERAL],
                    &["continuum.lora.invoke"],
                    160,
                    Some(600),
                ),
            })
            .unwrap();

        assert!(!first.stored_for_replay);
        assert!(!second.stored_for_replay);
        assert_eq!(
            second.coalesced_presence_key.as_deref(),
            Some("peer_manifest:peer-a")
        );
        assert_eq!(second.active_peer_manifest_count, 1);

        let result = store
            .replay(AircRealtimeReplayParams {
                room_id: GENERAL,
                after_cursor: None,
                limit: None,
                include_presence: None,
                include_subscriptions: None,
                include_peer_manifests: Some(true),
                include_capability_index: Some(true),
                now_ms: Some(599),
            })
            .unwrap();

        assert!(result.events.is_empty());
        assert_eq!(
            result
                .active_peer_manifests
                .iter()
                .map(|manifest| manifest.peer_id.as_str())
                .collect::<Vec<_>>(),
            ["peer-a", "peer-b"]
        );
        assert_eq!(result.capability_index.len(), 2);
        assert_eq!(
            result.capability_index[0].capability_id,
            "continuum.chat.turn"
        );
        assert_eq!(
            result.capability_index[0].peer_ids,
            vec!["peer-a".to_string()]
        );
        assert_eq!(
            result.capability_index[1].capability_id,
            "continuum.lora.invoke"
        );
        assert_eq!(
            result.capability_index[1].peer_ids,
            vec!["peer-a".to_string(), "peer-b".to_string()]
        );

        let expired = store
            .replay(AircRealtimeReplayParams {
                room_id: GENERAL,
                after_cursor: None,
                limit: None,
                include_presence: None,
                include_subscriptions: None,
                include_peer_manifests: Some(true),
                include_capability_index: Some(true),
                now_ms: Some(600),
            })
            .unwrap();
        assert!(expired.active_peer_manifests.is_empty());
        assert!(expired.capability_index.is_empty());
    }

    #[test]
    fn receipt_only_messages_are_not_replayed() {
        let store = InMemoryAircRealtimeStore::new(10);
        let mut receipt = AircRealtimeEnvelope::new(
            "receipt-1".to_string(),
            GENERAL,
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
                room_id: GENERAL,
                after_cursor: None,
                limit: None,
                include_presence: None,
                include_subscriptions: None,
                include_peer_manifests: None,
                include_capability_index: None,
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
            GENERAL,
            "browser-1".to_string(),
            10,
            AircRealtimePayload::Subscription {
                event: AircSubscriptionEvent {
                    action: AircSubscriptionAction::Subscribe,
                    room_id: GENERAL,
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

    #[test]
    fn subscription_events_project_active_room_subscribers() {
        let store = InMemoryAircRealtimeStore::new(10);
        for (id, room, subscriber, topic) in [
            ("sub-1", GENERAL, "browser-1", "presence"),
            ("sub-2", GENERAL, "persona-1", "media"),
            ("sub-3", OTHER, "browser-2", "presence"),
        ] {
            store
                .publish(AircRealtimePublishParams {
                    envelope: subscription_event(
                        id,
                        room,
                        subscriber,
                        topic,
                        AircSubscriptionAction::Subscribe,
                    ),
                })
                .unwrap();
        }

        let result = store
            .replay(AircRealtimeReplayParams {
                room_id: GENERAL,
                after_cursor: None,
                limit: None,
                include_presence: None,
                include_subscriptions: Some(true),
                include_peer_manifests: None,
                include_capability_index: None,
                now_ms: None,
            })
            .unwrap();

        assert_eq!(result.active_subscriptions.len(), 2);
        assert_eq!(result.active_subscriptions[0].subscriber_id, "browser-1");
        assert_eq!(result.active_subscriptions[1].subscriber_id, "persona-1");
    }

    #[test]
    fn unsubscribe_removes_active_subscription_but_remains_replayable() {
        let store = InMemoryAircRealtimeStore::new(10);
        store
            .publish(AircRealtimePublishParams {
                envelope: subscription_event(
                    "sub-1",
                    GENERAL,
                    "browser-1",
                    "presence",
                    AircSubscriptionAction::Subscribe,
                ),
            })
            .unwrap();
        let unsubscribe = store
            .publish(AircRealtimePublishParams {
                envelope: subscription_event(
                    "unsub-1",
                    GENERAL,
                    "browser-1",
                    "presence",
                    AircSubscriptionAction::Unsubscribe,
                ),
            })
            .unwrap();

        assert_eq!(unsubscribe.active_subscription_count, 0);

        let result = store
            .replay(AircRealtimeReplayParams {
                room_id: GENERAL,
                after_cursor: None,
                limit: None,
                include_presence: None,
                include_subscriptions: Some(true),
                include_peer_manifests: None,
                include_capability_index: None,
                now_ms: None,
            })
            .unwrap();

        assert!(result.active_subscriptions.is_empty());
        assert_eq!(
            result
                .events
                .iter()
                .map(|event| event.event_id.as_str())
                .collect::<Vec<_>>(),
            ["sub-1", "unsub-1"]
        );
    }

    #[test]
    fn publish_rejects_nil_room_id() {
        let store = InMemoryAircRealtimeStore::new(10);
        let error = store
            .publish(AircRealtimePublishParams {
                envelope: durable_event("evt-1", Uuid::nil(), 1),
            })
            .unwrap_err();

        assert_eq!(error, "room_id must not be the nil UUID");
    }

    fn subscription_event(
        id: &str,
        room: Uuid,
        subscriber: &str,
        topic: &str,
        action: AircSubscriptionAction,
    ) -> AircRealtimeEnvelope {
        AircRealtimeEnvelope::new(
            id.to_string(),
            room,
            subscriber.to_string(),
            10,
            AircRealtimePayload::Subscription {
                event: AircSubscriptionEvent {
                    action,
                    room_id: room,
                    subscriber_id: subscriber.to_string(),
                    topic: topic.to_string(),
                    cursor: None,
                },
            },
        )
    }
}
