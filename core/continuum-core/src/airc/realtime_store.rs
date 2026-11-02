//! In-process realtime adapter for AIRC envelopes.
//!
//! This is the Continuum-side substrate surface before external AIRC transport
//! is attached. It keeps hot-path behavior Rust-owned: delivery validation,
//! bounded replay, receipt suppression, and coalesced ephemeral presence.

use crate::routing::airc_realtime::{
    AircPeerManifest, AircPresenceEvent, AircRealtimeDelivery, AircRealtimeEnvelope,
    AircRealtimePayload, AircReplayCursor, AircSubscriptionAction, AircSubscriptionEvent,
};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use ts_rs::TS;
use uuid::Uuid;

pub const DEFAULT_EVENTS_PER_ROOM: usize = 2_000;

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircRealtimePublishParams.ts"
)]
pub struct AircRealtimePublishParams {
    pub envelope: AircRealtimeEnvelope,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircRealtimePublishResult.ts"
)]
pub struct AircRealtimePublishResult {
    pub ok: bool,
    #[ts(type = "string")]
    pub event_id: Uuid,
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

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircRealtimeReplayParams.ts"
)]
pub struct AircRealtimeReplayParams {
    /// The room (airc channel) id whose realtime state to replay — the room you
    /// want the snapshot for. Full id or the short form shown in rosters.
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
    export_to = "../../../protocol/typescript/airc/AircCapabilityIndexEntry.ts"
)]
pub struct AircCapabilityIndexEntry {
    pub capability_id: String,
    pub peer_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircRealtimeReplayResult.ts"
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

        // A page is what the caller asks for, bounded by what this store retains (it can return no
        // more); with no ask, the whole retained window. No second number chosen here.
        let limit = params.limit.unwrap_or(self.max_events_per_room).clamp(1, self.max_events_per_room);
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
        manifests.sort_by_key(|m| m.peer_id.as_uuid());
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
                .push(manifest.peer_id.to_string());
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
mod tests;
