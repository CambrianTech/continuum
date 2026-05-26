//! Typed realtime envelopes for routing Continuum chat, presence, subscriptions,
//! and LiveKit control metadata through AIRC.
//!
//! These types are the Rust contract at the AIRC boundary. They intentionally
//! wrap existing Continuum payload schemas instead of redefining JTAG, Grid, or
//! LiveKit messages.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

/// Delivery handling requested from the AIRC substrate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../shared/generated/airc/AircRealtimeDelivery.ts"
)]
pub enum AircRealtimeDelivery {
    /// Persist, index, acknowledge, and make available for replay.
    Durable,
    /// Keep the newest value per key and expire it instead of replaying forever.
    EphemeralCoalesced,
    /// Carry acknowledgement state only; do not project as user-visible content.
    ReceiptOnly,
    /// Control-plane message such as subscribe/unsubscribe or WebRTC session state.
    Control,
}

/// Existing Continuum schema carried by an AIRC realtime envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../shared/generated/airc/AircRealtimeSchema.ts"
)]
pub enum AircRealtimeSchema {
    /// `src/system/core/types/JTAGTypes.ts` `JTAGMessage`.
    JtagMessage,
    /// `src/system/events/shared/EventSystemTypes.ts` `EventBridgePayload`.
    EventBridgePayload,
    /// `continuum-core::modules::grid::frame::GridFrame`.
    GridFrame,
    /// `livekit-protocol::BridgeCommand`.
    LiveKitBridgeCommand,
    /// `livekit-protocol::BridgeEvent`.
    LiveKitBridgeEvent,
    /// A bounded transcript/chat payload projected into Continuum UI or memory.
    ChatTranscript,
}

/// Handle to a payload already defined by a Continuum schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/airc/AircRealtimePayloadRef.ts"
)]
pub struct AircRealtimePayloadRef {
    pub schema: AircRealtimeSchema,
    #[ts(optional)]
    pub schema_version: Option<String>,
    /// Inline JSON for small control/event payloads. Heavy media stays out of AIRC.
    #[ts(optional, type = "unknown")]
    pub inline: Option<Value>,
    /// Content-addressed or local object-store pointer for larger payloads.
    #[ts(optional)]
    pub artifact_ref: Option<String>,
    #[ts(optional)]
    pub digest: Option<String>,
}

impl AircRealtimePayloadRef {
    pub fn inline(schema: AircRealtimeSchema, inline: Value) -> Self {
        Self {
            schema,
            schema_version: None,
            inline: Some(inline),
            artifact_ref: None,
            digest: None,
        }
    }

    pub fn is_pointer_only(&self) -> bool {
        self.inline.is_none() && self.artifact_ref.is_some()
    }
}

/// Presence states used by chat, avatars, and rooms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../shared/generated/airc/AircPresenceState.ts"
)]
pub enum AircPresenceState {
    Online,
    Away,
    Active,
    Typing,
    Thinking,
    Speaking,
    Listening,
    InCall,
    Muted,
    Disconnected,
}

impl AircPresenceState {
    pub fn is_ephemeral(self) -> bool {
        matches!(
            self,
            Self::Active | Self::Typing | Self::Thinking | Self::Speaking | Self::Listening
        )
    }

    pub fn as_key(self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Away => "away",
            Self::Active => "active",
            Self::Typing => "typing",
            Self::Thinking => "thinking",
            Self::Speaking => "speaking",
            Self::Listening => "listening",
            Self::InCall => "in_call",
            Self::Muted => "muted",
            Self::Disconnected => "disconnected",
        }
    }
}

/// Presence update that AIRC can coalesce by `room_id + subject_id + state`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/airc/AircPresenceEvent.ts"
)]
pub struct AircPresenceEvent {
    #[ts(type = "string")]
    pub room_id: Uuid,
    pub subject_id: String,
    #[ts(optional)]
    pub display_name: Option<String>,
    pub state: AircPresenceState,
    pub started_at_ms: u64,
    #[ts(optional)]
    pub expires_at_ms: Option<u64>,
    #[ts(optional)]
    pub call_id: Option<String>,
}

impl AircPresenceEvent {
    pub fn coalesce_key(&self) -> String {
        format!(
            "presence:{}:{}:{}",
            self.room_id,
            self.subject_id,
            self.state.as_key()
        )
    }

    pub fn delivery(&self) -> AircRealtimeDelivery {
        if self.state.is_ephemeral() || self.expires_at_ms.is_some() {
            AircRealtimeDelivery::EphemeralCoalesced
        } else {
            AircRealtimeDelivery::Durable
        }
    }

    pub fn is_expired_at(&self, now_ms: u64) -> bool {
        self.expires_at_ms
            .map(|expires_at| now_ms >= expires_at)
            .unwrap_or(false)
    }
}

/// Subscribe/unsubscribe/cursor command for bounded event delivery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../shared/generated/airc/AircSubscriptionAction.ts"
)]
pub enum AircSubscriptionAction {
    Subscribe,
    Unsubscribe,
    Replay,
    Ack,
}

/// Cursor for replay/resume across reconnects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/airc/AircReplayCursor.ts"
)]
pub struct AircReplayCursor {
    #[ts(type = "string")]
    pub room_id: Uuid,
    pub last_seen_event_id: String,
    #[ts(optional)]
    pub last_seen_at_ms: Option<u64>,
}

/// Subscription control-plane payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/airc/AircSubscriptionEvent.ts"
)]
pub struct AircSubscriptionEvent {
    pub action: AircSubscriptionAction,
    #[ts(type = "string")]
    pub room_id: Uuid,
    pub subscriber_id: String,
    pub topic: String,
    #[ts(optional)]
    pub cursor: Option<AircReplayCursor>,
}

impl AircSubscriptionEvent {
    pub fn coalesce_key(&self) -> String {
        format!(
            "subscription:{}:{}:{}",
            self.room_id, self.subscriber_id, self.topic
        )
    }
}

/// WebRTC/LiveKit control-plane metadata. Binary audio/video never rides here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/airc/AircMediaControlEvent.ts"
)]
pub struct AircMediaControlEvent {
    pub call_id: String,
    #[ts(optional)]
    pub user_id: Option<String>,
    pub action: String,
    #[ts(optional)]
    pub livekit_payload: Option<AircRealtimePayloadRef>,
}

impl AircMediaControlEvent {
    pub fn references_livekit_schema(&self) -> bool {
        self.livekit_payload
            .as_ref()
            .map(|payload| {
                matches!(
                    payload.schema,
                    AircRealtimeSchema::LiveKitBridgeCommand
                        | AircRealtimeSchema::LiveKitBridgeEvent
                )
            })
            .unwrap_or(true)
    }
}

/// Capability advertised by a peer in a room.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/airc/AircPeerCapability.ts"
)]
pub struct AircPeerCapability {
    pub id: String,
    #[ts(optional)]
    pub label: Option<String>,
    #[ts(optional)]
    pub version: Option<String>,
}

/// Room-scoped peer manifest used for discovery and capability routing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/airc/AircPeerManifest.ts"
)]
pub struct AircPeerManifest {
    pub peer_id: String,
    #[ts(optional)]
    pub display_name: Option<String>,
    #[ts(type = "Array<string>")]
    pub room_ids: Vec<Uuid>,
    pub capabilities: Vec<AircPeerCapability>,
    pub advertised_at_ms: u64,
    #[ts(optional)]
    pub expires_at_ms: Option<u64>,
}

impl AircPeerManifest {
    pub fn coalesce_key(&self) -> String {
        format!("peer_manifest:{}", self.peer_id)
    }

    pub fn is_expired_at(&self, now_ms: u64) -> bool {
        self.expires_at_ms
            .map(|expires_at| now_ms >= expires_at)
            .unwrap_or(false)
    }

    pub fn advertises_room(&self, room_id: Uuid) -> bool {
        self.room_ids.contains(&room_id)
    }
}

/// Acknowledgement and receipt state for durable delivery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../shared/generated/airc/AircReceipt.ts")]
pub struct AircReceipt {
    pub event_id: String,
    pub peer_id: String,
    pub received_at_ms: u64,
    #[ts(optional)]
    pub replay_cursor: Option<AircReplayCursor>,
}

/// Realtime payload carried by AIRC.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../shared/generated/airc/AircRealtimePayload.ts"
)]
pub enum AircRealtimePayload {
    ExistingSchema { payload: AircRealtimePayloadRef },
    Presence { event: AircPresenceEvent },
    PeerManifest { manifest: AircPeerManifest },
    Subscription { event: AircSubscriptionEvent },
    MediaControl { event: AircMediaControlEvent },
    Receipt { receipt: AircReceipt },
}

impl AircRealtimePayload {
    pub fn delivery(&self) -> AircRealtimeDelivery {
        match self {
            Self::ExistingSchema { payload } => match payload.schema {
                AircRealtimeSchema::LiveKitBridgeCommand
                | AircRealtimeSchema::LiveKitBridgeEvent => AircRealtimeDelivery::Control,
                _ => AircRealtimeDelivery::Durable,
            },
            Self::Presence { event } => event.delivery(),
            Self::PeerManifest { .. } => AircRealtimeDelivery::EphemeralCoalesced,
            Self::Subscription { .. } | Self::MediaControl { .. } => AircRealtimeDelivery::Control,
            Self::Receipt { .. } => AircRealtimeDelivery::ReceiptOnly,
        }
    }
}

/// Top-level realtime envelope persisted or transmitted by AIRC.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/airc/AircRealtimeEnvelope.ts"
)]
pub struct AircRealtimeEnvelope {
    pub event_id: String,
    #[ts(type = "string")]
    pub room_id: Uuid,
    pub source_id: String,
    #[ts(optional)]
    pub target_id: Option<String>,
    pub created_at_ms: u64,
    pub delivery: AircRealtimeDelivery,
    pub payload: AircRealtimePayload,
    #[ts(optional)]
    pub trace_id: Option<String>,
}

impl AircRealtimeEnvelope {
    pub fn new(
        event_id: String,
        room_id: Uuid,
        source_id: String,
        created_at_ms: u64,
        payload: AircRealtimePayload,
    ) -> Self {
        let delivery = payload.delivery();
        Self {
            event_id,
            room_id,
            source_id,
            target_id: None,
            created_at_ms,
            delivery,
            payload,
            trace_id: None,
        }
    }

    pub fn validate_delivery(&self) -> Result<(), String> {
        let expected = self.payload.delivery();
        if self.delivery == expected {
            Ok(())
        } else {
            Err(format!(
                "delivery {:?} does not match payload semantics {:?}",
                self.delivery, expected
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn typing_presence_is_ephemeral_and_expirable() {
        let room_id = Uuid::from_u128(0xA1);
        let event = AircPresenceEvent {
            room_id,
            subject_id: "persona-1".to_string(),
            display_name: None,
            state: AircPresenceState::Typing,
            started_at_ms: 1000,
            expires_at_ms: Some(4000),
            call_id: None,
        };

        assert_eq!(event.delivery(), AircRealtimeDelivery::EphemeralCoalesced);
        assert!(!event.is_expired_at(3999));
        assert!(event.is_expired_at(4000));
        assert_eq!(
            event.coalesce_key(),
            format!("presence:{room_id}:persona-1:typing")
        );
    }

    #[test]
    fn jtag_and_grid_payloads_stay_durable() {
        for schema in [
            AircRealtimeSchema::JtagMessage,
            AircRealtimeSchema::EventBridgePayload,
            AircRealtimeSchema::GridFrame,
            AircRealtimeSchema::ChatTranscript,
        ] {
            let payload = AircRealtimePayload::ExistingSchema {
                payload: AircRealtimePayloadRef::inline(schema, json!({"ok": true})),
            };
            assert_eq!(payload.delivery(), AircRealtimeDelivery::Durable);
        }
    }

    #[test]
    fn livekit_control_is_control_plane_and_references_existing_schema() {
        let event = AircMediaControlEvent {
            call_id: "call-1".to_string(),
            user_id: Some("persona-1".to_string()),
            action: "join_room".to_string(),
            livekit_payload: Some(AircRealtimePayloadRef::inline(
                AircRealtimeSchema::LiveKitBridgeCommand,
                json!({"type": "JoinRoom", "call_id": "call-1"}),
            )),
        };

        assert!(event.references_livekit_schema());

        let payload = AircRealtimePayload::MediaControl { event };
        assert_eq!(payload.delivery(), AircRealtimeDelivery::Control);
    }

    #[test]
    fn peer_manifest_is_ephemeral_room_scoped_capability_advertisement() {
        let general = Uuid::from_u128(0xA1);
        let cambriantech = Uuid::from_u128(0xA2);
        let useideem = Uuid::from_u128(0xA3);
        let manifest = AircPeerManifest {
            peer_id: "peer-continuum-1".to_string(),
            display_name: Some("Continuum GPU Host".to_string()),
            room_ids: vec![general, cambriantech],
            capabilities: vec![AircPeerCapability {
                id: "continuum.lora.invoke".to_string(),
                label: Some("LoRA invocation".to_string()),
                version: Some("1".to_string()),
            }],
            advertised_at_ms: 1_000,
            expires_at_ms: Some(10_000),
        };

        assert_eq!(manifest.coalesce_key(), "peer_manifest:peer-continuum-1");
        assert!(manifest.advertises_room(general));
        assert!(!manifest.advertises_room(useideem));
        assert!(!manifest.is_expired_at(9_999));
        assert!(manifest.is_expired_at(10_000));

        let payload = AircRealtimePayload::PeerManifest { manifest };
        assert_eq!(payload.delivery(), AircRealtimeDelivery::EphemeralCoalesced);
    }

    #[test]
    fn envelope_delivery_must_match_payload_semantics() {
        let payload = AircRealtimePayload::Receipt {
            receipt: AircReceipt {
                event_id: "evt-1".to_string(),
                peer_id: "peer-1".to_string(),
                received_at_ms: 10,
                replay_cursor: None,
            },
        };

        let mut envelope = AircRealtimeEnvelope::new(
            "receipt-1".to_string(),
            Uuid::from_u128(0xA1),
            "peer-1".to_string(),
            11,
            payload,
        );
        assert_eq!(envelope.delivery, AircRealtimeDelivery::ReceiptOnly);
        assert!(envelope.validate_delivery().is_ok());

        envelope.delivery = AircRealtimeDelivery::Durable;
        assert!(envelope.validate_delivery().is_err());
    }
}
