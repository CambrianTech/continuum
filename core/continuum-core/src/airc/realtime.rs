//! Typed realtime envelopes for routing Continuum chat, presence, subscriptions,
//! and LiveKit control metadata through AIRC.
//!
//! These types are the Rust contract at the AIRC boundary. They intentionally
//! wrap existing Continuum payload schemas instead of redefining JTAG, Grid, or
//! LiveKit messages.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

/// Delivery handling requested from the AIRC substrate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircRealtimeDelivery.ts"
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircRealtimeSchema.ts"
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
    /// A node's live capacity offer (`capacity::gossip::CapacityOffer`) — grid
    /// presence-of-compute. EphemeralCoalesced: latest wins, never replayed
    /// (a stale capacity reading is a lie).
    GridCapacity,
}

/// Handle to a payload already defined by a Continuum schema.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircRealtimePayloadRef.ts"
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircPresenceState.ts"
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircPresenceEvent.ts"
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircSubscriptionAction.ts"
)]
pub enum AircSubscriptionAction {
    Subscribe,
    Unsubscribe,
    Replay,
    Ack,
}

/// Cursor for replay/resume across reconnects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircReplayCursor.ts"
)]
pub struct AircReplayCursor {
    #[ts(type = "string")]
    pub room_id: Uuid,
    pub lamport: u64,
    pub event_id: String,
    #[ts(optional)]
    pub observed_at_ms: Option<u64>,
}

impl AircReplayCursor {
    pub fn strictly_before(&self, other: &Self) -> bool {
        self.lamport < other.lamport
            || (self.lamport == other.lamport && self.event_id < other.event_id)
    }

    pub fn from_airc(room_id: Uuid, cursor: airc_core::TranscriptCursor) -> Self {
        Self {
            room_id,
            lamport: cursor.lamport,
            event_id: cursor.event_id.to_string(),
            observed_at_ms: None,
        }
    }

    pub fn to_airc(&self) -> Result<airc_core::TranscriptCursor, String> {
        let event_uuid = Uuid::parse_str(&self.event_id)
            .map_err(|error| format!("invalid AIRC replay cursor event_id: {error}"))?;
        Ok(airc_core::TranscriptCursor {
            lamport: self.lamport,
            event_id: airc_core::EventId::from_uuid(event_uuid),
        })
    }
}

/// Subscription control-plane payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircSubscriptionEvent.ts"
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircMediaControlEvent.ts"
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircPeerCapability.ts"
)]
pub struct AircPeerCapability {
    pub id: String,
    #[ts(optional)]
    pub label: Option<String>,
    #[ts(optional)]
    pub version: Option<String>,
}

/// Room-scoped peer manifest used for discovery and capability routing.
///
/// `signing_pubkey_hex` advertises the peer's ed25519 signing key so the
/// L1-6 contract event chain (and any other signed-envelope event class)
/// can do `peer_id → pubkey` lookups at verify time. The substrate-level
/// trust answer is "the manifest IS the directory" — no separate keyring,
/// no out-of-band cert exchange. A peer that mutates its own pubkey
/// publishes a fresh manifest; receivers that already have one for that
/// peer_id reject the mismatch loud (key rotation has to go through the
/// proper trust-rotation event class, not silent overwrite).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircPeerManifest.ts"
)]
pub struct AircPeerManifest {
    #[ts(type = "string")]
    #[schemars(with = "String")]
    pub peer_id: crate::identity::PeerId,
    #[ts(optional)]
    pub display_name: Option<String>,
    #[ts(type = "Array<string>")]
    pub room_ids: Vec<Uuid>,
    pub capabilities: Vec<AircPeerCapability>,
    /// 32-byte ed25519 public key, hex-encoded (64 lowercase chars,
    /// no `0x` prefix). Same encoding as
    /// `crate::contracts::SignedContractEvent::signer_pubkey_hex`,
    /// so the two interoperate without re-encoding. Required field —
    /// the manifest is the substrate trust directory; a manifest
    /// without a pubkey can't be used to verify anything the peer
    /// signs.
    pub signing_pubkey_hex: String,
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

    /// Validate the basic invariants of a manifest at construction /
    /// receipt time. Returns Err with a specific reason rather than
    /// silently accepting malformed data — per the never-swallow-evidence
    /// rule, a bad manifest must fail loud so the peer that sent it can
    /// be told why.
    pub fn validate(&self) -> Result<(), AircPeerManifestError> {
        // Typing `peer_id` as `PeerId` (transparent UUID) killed the BLANK case:
        // "" cannot be constructed or deserialized, and malformed input now fails at
        // parse with a serde error naming the field — louder than this check was.
        //
        // But it did NOT kill "unset": `Uuid::nil()` is still constructible and still
        // means nobody. The type narrowed the hole rather than closing it, so the
        // invariant keeps an explicit guard at its remaining expressible form.
        if self.peer_id.as_uuid().is_nil() {
            return Err(AircPeerManifestError::EmptyPeerId);
        }
        validate_signing_pubkey_hex(&self.signing_pubkey_hex)?;
        Ok(())
    }
}

/// Validation errors for an `AircPeerManifest`. Specific variants so
/// the L1-2 inbound subscriber can log + reject with actionable
/// diagnostics rather than a generic "bad manifest".
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AircPeerManifestError {
    EmptyPeerId,
    PubkeyWrongLength { expected: usize, got: usize },
    PubkeyNonHexChar { char: char, index: usize },
}

impl std::fmt::Display for AircPeerManifestError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyPeerId => f.write_str("peer_id must not be empty"),
            Self::PubkeyWrongLength { expected, got } => write!(
                f,
                "signing_pubkey_hex wrong length: expected {expected} hex chars (32 bytes), got {got}",
            ),
            Self::PubkeyNonHexChar { char, index } => write!(
                f,
                "signing_pubkey_hex contains non-hex character '{char}' at index {index}",
            ),
        }
    }
}

impl std::error::Error for AircPeerManifestError {}

/// `signing_pubkey_hex` must be exactly 64 lowercase-or-uppercase hex
/// characters (no `0x` prefix). The byte parse itself + curve-membership
/// validation is delegated to ed25519_dalek when a consumer parses; this
/// check is the cheap structural gate at substrate ingress.
fn validate_signing_pubkey_hex(hex: &str) -> Result<(), AircPeerManifestError> {
    const EXPECTED_LEN: usize = 64; // 32 bytes * 2 hex chars
    if hex.len() != EXPECTED_LEN {
        return Err(AircPeerManifestError::PubkeyWrongLength {
            expected: EXPECTED_LEN,
            got: hex.len(),
        });
    }
    for (i, c) in hex.chars().enumerate() {
        if !c.is_ascii_hexdigit() {
            return Err(AircPeerManifestError::PubkeyNonHexChar { char: c, index: i });
        }
    }
    Ok(())
}

/// Acknowledgement and receipt state for durable delivery.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/airc/AircReceipt.ts")]
pub struct AircReceipt {
    pub event_id: String,
    #[ts(type = "string")]
    #[schemars(with = "String")]
    pub peer_id: crate::identity::PeerId,
    pub received_at_ms: u64,
    #[ts(optional)]
    pub replay_cursor: Option<AircReplayCursor>,
}

/// Realtime payload carried by AIRC.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircRealtimePayload.ts"
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
                // Capacity offers are presence-of-compute: latest wins, never
                // replayed — a stale reading must not outlive its freshness.
                AircRealtimeSchema::GridCapacity => AircRealtimeDelivery::EphemeralCoalesced,
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
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/airc/AircRealtimeEnvelope.ts"
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
    use airc_core::PeerId;
    use serde_json::json;

    /// Sample ed25519 pubkey hex for test fixtures. 32 bytes (64 hex
    /// chars). Not a real key — purely structural so test manifests pass
    /// `validate_signing_pubkey_hex`. Use distinct values across peers
    /// in multi-peer tests so equality checks are meaningful.
    const TEST_PUBKEY_HEX: &str =
        "0102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f20";

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
    fn replay_cursor_orders_by_lamport_then_event_id() {
        let room_id = Uuid::from_u128(0xA1);
        let earlier = AircReplayCursor {
            room_id,
            lamport: 4,
            event_id: "00000000-0000-0000-0000-000000000001".to_string(),
            observed_at_ms: None,
        };
        let later_same_lamport = AircReplayCursor {
            room_id,
            lamport: 4,
            event_id: "00000000-0000-0000-0000-000000000002".to_string(),
            observed_at_ms: None,
        };
        let later_lamport = AircReplayCursor {
            room_id,
            lamport: 5,
            event_id: "00000000-0000-0000-0000-000000000000".to_string(),
            observed_at_ms: None,
        };

        assert!(earlier.strictly_before(&later_same_lamport));
        assert!(later_same_lamport.strictly_before(&later_lamport));
        assert!(!later_lamport.strictly_before(&earlier));
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
            peer_id: PeerId::from_uuid(uuid::Uuid::new_v5(
                &uuid::Uuid::NAMESPACE_OID,
                b"peer-continuum-1",
            )),
            display_name: Some("Continuum GPU Host".to_string()),
            room_ids: vec![general, cambriantech],
            capabilities: vec![AircPeerCapability {
                id: "continuum.lora.invoke".to_string(),
                label: Some("LoRA invocation".to_string()),
                version: Some("1".to_string()),
            }],
            signing_pubkey_hex: TEST_PUBKEY_HEX.to_string(),
            advertised_at_ms: 1_000,
            expires_at_ms: Some(10_000),
        };

        assert_eq!(
            manifest.coalesce_key(),
            format!(
                "peer_manifest:{}",
                PeerId::from_uuid(uuid::Uuid::new_v5(
                    &uuid::Uuid::NAMESPACE_OID,
                    b"peer-continuum-1"
                ))
                .as_uuid()
            )
        );
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
                peer_id: PeerId::from_uuid(uuid::Uuid::new_v5(
                    &uuid::Uuid::NAMESPACE_OID,
                    b"peer-1",
                )),
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

    fn manifest_with_pubkey(pubkey_hex: &str) -> AircPeerManifest {
        AircPeerManifest {
            peer_id: PeerId::from_uuid(uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, b"peer-1")),
            display_name: None,
            room_ids: vec![Uuid::from_u128(0xA1)],
            capabilities: vec![],
            signing_pubkey_hex: pubkey_hex.to_string(),
            advertised_at_ms: 1_000,
            expires_at_ms: None,
        }
    }

    #[test]
    fn manifest_validates_well_formed_pubkey() {
        manifest_with_pubkey(TEST_PUBKEY_HEX).validate().unwrap();
    }

    #[test]
    fn manifest_accepts_uppercase_hex() {
        // ASCII hex parsing allows both cases; the canonical form is
        // lowercase but the substrate must NOT reject an otherwise
        // valid uppercase pubkey just for case.
        let upper = TEST_PUBKEY_HEX.to_uppercase();
        manifest_with_pubkey(&upper).validate().unwrap();
    }

    #[test]
    fn manifest_rejects_wrong_length_pubkey() {
        let too_short = &TEST_PUBKEY_HEX[..62]; // 31 bytes' worth
        let err = manifest_with_pubkey(too_short).validate().unwrap_err();
        assert!(matches!(
            err,
            AircPeerManifestError::PubkeyWrongLength {
                expected: 64,
                got: 62
            }
        ));
    }

    #[test]
    fn manifest_rejects_non_hex_pubkey() {
        // Replace one char with 'z' (length stays 64).
        let mut bad: String = TEST_PUBKEY_HEX.to_string();
        bad.replace_range(10..11, "z");
        let err = manifest_with_pubkey(&bad).validate().unwrap_err();
        assert!(matches!(
            err,
            AircPeerManifestError::PubkeyNonHexChar {
                char: 'z',
                index: 10
            }
        ));
    }

    #[test]
    fn manifest_rejects_empty_peer_id() {
        // what this catches: an UNSET peer id must never validate. The
        // field is now `PeerId`, so "" is no longer expressible — the nil
        // UUID is the only remaining way to say "unset", and it is what
        // this must refuse. (Before the newtype the test typed `""`; that
        // spelling is gone, the invariant is not.)
        let mut m = manifest_with_pubkey(TEST_PUBKEY_HEX);
        m.peer_id = PeerId::from_uuid(uuid::Uuid::nil());
        let err = m.validate().unwrap_err();
        assert!(matches!(err, AircPeerManifestError::EmptyPeerId));
    }

    #[test]
    fn manifest_round_trips_through_json_with_pubkey() {
        // The pubkey field MUST appear on the wire in camelCase
        // (`signingPubkeyHex`) per the serde rename_all on
        // AircPeerManifest. Verify both the field name + the round-trip.
        let manifest = manifest_with_pubkey(TEST_PUBKEY_HEX);
        let json = serde_json::to_string(&manifest).unwrap();
        assert!(
            json.contains(r#""signingPubkeyHex":"#),
            "wire JSON must use camelCase field name; got: {json}",
        );
        let restored: AircPeerManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(restored, manifest);
    }
}
