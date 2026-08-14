//! Contract replay verification against AIRC peer manifests.
//!
//! L1-6 Phase A verifies that an ed25519 key signed a contract event.
//! This module closes Phase B: the verified key must also be the key
//! advertised by the peer manifest for the participant that claims to
//! have signed the event.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::airc::{
    AircPeerManifest, AircRealtimeEnvelope, AircRealtimePayload, AircRealtimeReplayResult,
    AircRealtimeSchema,
};
use crate::contracts::{
    ContractAcceptedPayload, ContractBidPayload, ContractDeliveredPayload, ContractDisputedPayload,
    ContractExecutingPayload, ContractPaidPayload, ContractProposedPayload,
    ContractVerifiedPayload, SignedContractEvent, EVENT_CONTRACT_ACCEPTED, EVENT_CONTRACT_BID,
    EVENT_CONTRACT_DELIVERED, EVENT_CONTRACT_DISPUTED, EVENT_CONTRACT_EXECUTING,
    EVENT_CONTRACT_PAID, EVENT_CONTRACT_PROPOSED, EVENT_CONTRACT_VERIFIED,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedContractEvent {
    pub replay_event_id: String,
    pub room_id: uuid::Uuid,
    pub contract_id: String,
    pub event_name: String,
    pub signer_peer_id: String,
    pub signer_pubkey_hex: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractVerificationError {
    MalformedContractEvent {
        event_id: String,
        event_name: String,
        reason: String,
    },
    SignatureRejected {
        event_id: String,
        event_name: String,
        reason: String,
    },
    MissingPeerManifest {
        event_id: String,
        event_name: String,
        signer_peer_id: String,
    },
    ManifestPubkeyMismatch {
        event_id: String,
        event_name: String,
        signer_peer_id: String,
        manifest_pubkey_hex: String,
        event_pubkey_hex: String,
    },
    SourcePeerMismatch {
        event_id: String,
        event_name: String,
        source_id: String,
        signer_peer_id: String,
    },
}

impl std::fmt::Display for ContractVerificationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MalformedContractEvent {
                event_id,
                event_name,
                reason,
            } => write!(
                f,
                "contract event {event_id} ({event_name}) is malformed: {reason}",
            ),
            Self::SignatureRejected {
                event_id,
                event_name,
                reason,
            } => write!(
                f,
                "contract event {event_id} ({event_name}) signature rejected: {reason}",
            ),
            Self::MissingPeerManifest {
                event_id,
                event_name,
                signer_peer_id,
            } => write!(
                f,
                "contract event {event_id} ({event_name}) signer {signer_peer_id} has no active peer manifest",
            ),
            Self::ManifestPubkeyMismatch {
                event_id,
                event_name,
                signer_peer_id,
                ..
            } => write!(
                f,
                "contract event {event_id} ({event_name}) signer {signer_peer_id} pubkey does not match peer manifest",
            ),
            Self::SourcePeerMismatch {
                event_id,
                event_name,
                source_id,
                signer_peer_id,
            } => write!(
                f,
                "contract event {event_id} ({event_name}) source_id {source_id} does not match signer {signer_peer_id}",
            ),
        }
    }
}

impl std::error::Error for ContractVerificationError {}

pub fn verify_contract_replay(
    replay: &AircRealtimeReplayResult,
) -> Result<Vec<VerifiedContractEvent>, ContractVerificationError> {
    let manifests = PeerManifestIndex::new(&replay.active_peer_manifests);
    let mut verified = Vec::new();
    for event in &replay.events {
        if let Some(contract) = parse_contract_event(event)? {
            verify_manifest_binding(&manifests, event, &contract)?;
            verified.push(contract);
        }
    }
    Ok(verified)
}

struct PeerManifestIndex<'a> {
    by_peer_id: HashMap<String, &'a AircPeerManifest>,
}

impl<'a> PeerManifestIndex<'a> {
    fn new(manifests: &'a [AircPeerManifest]) -> Self {
        Self {
            by_peer_id: manifests
                .iter()
                .map(|manifest| (manifest.peer_id.to_string(), manifest))
                .collect(),
        }
    }

    fn get(&self, peer_id: &str) -> Option<&AircPeerManifest> {
        self.by_peer_id.get(peer_id).copied()
    }
}

fn parse_contract_event(
    event: &AircRealtimeEnvelope,
) -> Result<Option<VerifiedContractEvent>, ContractVerificationError> {
    let Some(value) = inline_event_bridge_payload(event) else {
        return Ok(None);
    };
    let Some(event_name) = value.get("eventName").and_then(Value::as_str) else {
        return Ok(None);
    };

    let verified = match event_name {
        EVENT_CONTRACT_PROPOSED => {
            parse_and_verify::<ContractProposedPayload>(event, event_name, value, |payload| {
                (&payload.contract_id, &payload.proposer_id)
            })?
        }
        EVENT_CONTRACT_BID => {
            parse_and_verify::<ContractBidPayload>(event, event_name, value, |payload| {
                (&payload.contract_id, &payload.bidder_id)
            })?
        }
        EVENT_CONTRACT_ACCEPTED => {
            parse_and_verify::<ContractAcceptedPayload>(event, event_name, value, |payload| {
                (&payload.contract_id, &payload.proposer_id)
            })?
        }
        EVENT_CONTRACT_EXECUTING => {
            parse_and_verify::<ContractExecutingPayload>(event, event_name, value, |payload| {
                (&payload.contract_id, &payload.executor_id)
            })?
        }
        EVENT_CONTRACT_DELIVERED => {
            parse_and_verify::<ContractDeliveredPayload>(event, event_name, value, |payload| {
                (&payload.contract_id, &payload.executor_id)
            })?
        }
        EVENT_CONTRACT_VERIFIED => {
            parse_and_verify::<ContractVerifiedPayload>(event, event_name, value, |payload| {
                (&payload.contract_id, &payload.verifier_id)
            })?
        }
        EVENT_CONTRACT_PAID => {
            parse_and_verify::<ContractPaidPayload>(event, event_name, value, |payload| {
                (&payload.contract_id, &payload.payer_id)
            })?
        }
        EVENT_CONTRACT_DISPUTED => {
            parse_and_verify::<ContractDisputedPayload>(event, event_name, value, |payload| {
                (&payload.contract_id, &payload.disputer_id)
            })?
        }
        _ => return Ok(None),
    };

    Ok(Some(verified))
}

fn inline_event_bridge_payload(event: &AircRealtimeEnvelope) -> Option<&Value> {
    match &event.payload {
        AircRealtimePayload::ExistingSchema { payload }
            if payload.schema == AircRealtimeSchema::EventBridgePayload =>
        {
            payload.inline.as_ref()
        }
        _ => None,
    }
}

fn parse_and_verify<P>(
    event: &AircRealtimeEnvelope,
    event_name: &str,
    value: &Value,
    signer_fields: impl for<'a> FnOnce(&'a P) -> (&'a String, &'a String),
) -> Result<VerifiedContractEvent, ContractVerificationError>
where
    P: Serialize + for<'de> Deserialize<'de>,
{
    let signed =
        serde_json::from_value::<SignedContractEvent<P>>(value.clone()).map_err(|error| {
            ContractVerificationError::MalformedContractEvent {
                event_id: event.event_id.clone(),
                event_name: event_name.to_string(),
                reason: error.to_string(),
            }
        })?;
    signed
        .verify()
        .map_err(|error| ContractVerificationError::SignatureRejected {
            event_id: event.event_id.clone(),
            event_name: event_name.to_string(),
            reason: error.to_string(),
        })?;
    let (contract_id, signer_peer_id) = signer_fields(&signed.payload);
    Ok(VerifiedContractEvent {
        replay_event_id: event.event_id.clone(),
        room_id: event.room_id,
        contract_id: contract_id.clone(),
        event_name: signed.event_name,
        signer_peer_id: signer_peer_id.clone(),
        signer_pubkey_hex: signed.signer_pubkey_hex,
    })
}

fn verify_manifest_binding(
    manifests: &PeerManifestIndex<'_>,
    envelope: &AircRealtimeEnvelope,
    contract: &VerifiedContractEvent,
) -> Result<(), ContractVerificationError> {
    let manifest = manifests.get(&contract.signer_peer_id).ok_or_else(|| {
        ContractVerificationError::MissingPeerManifest {
            event_id: envelope.event_id.clone(),
            event_name: contract.event_name.clone(),
            signer_peer_id: contract.signer_peer_id.clone(),
        }
    })?;

    if !manifest
        .signing_pubkey_hex
        .eq_ignore_ascii_case(&contract.signer_pubkey_hex)
    {
        return Err(ContractVerificationError::ManifestPubkeyMismatch {
            event_id: envelope.event_id.clone(),
            event_name: contract.event_name.clone(),
            signer_peer_id: contract.signer_peer_id.clone(),
            manifest_pubkey_hex: manifest.signing_pubkey_hex.clone(),
            event_pubkey_hex: contract.signer_pubkey_hex.clone(),
        });
    }

    if envelope.source_id != contract.signer_peer_id {
        return Err(ContractVerificationError::SourcePeerMismatch {
            event_id: envelope.event_id.clone(),
            event_name: contract.event_name.clone(),
            source_id: envelope.source_id.clone(),
            signer_peer_id: contract.signer_peer_id.clone(),
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::airc::{
        AircPeerCapability, AircRealtimeDelivery, AircRealtimePayloadRef, AircReplayCursor,
    };
    use crate::contracts::{ContractSigningKey, EVENT_CONTRACT_PROPOSED};
    use airc_core::PeerId;

    fn room() -> uuid::Uuid {
        uuid::Uuid::from_u128(0xA1)
    }

    fn proposed_payload(peer_id: &str) -> ContractProposedPayload {
        ContractProposedPayload {
            contract_id: "contract-1".to_string(),
            proposer_id: test_peer_str(peer_id),
            alloy_hash: "sha256:contract".to_string(),
            bid_currency: "".to_string(),
            max_bid: 0,
            expiry_unix_ms: 1_779_800_000_000,
            required_capability: "continuum.lora.invoke".to_string(),
        }
    }

    /// One derivation for a test peer's identity, used by BOTH the manifest and
    /// the event that claims to come from it. The manifest is looked up BY the
    /// event's signer id, so if only one side is a `PeerId` the lookup silently
    /// misses and every verification test fails as "MissingPeerManifest" —
    /// which is what happened when `peer_id` was typed and the fixtures were
    /// converted one side at a time.
    fn test_peer_id(name: &str) -> PeerId {
        PeerId::from_uuid(uuid::Uuid::new_v5(
            &uuid::Uuid::NAMESPACE_OID,
            name.as_bytes(),
        ))
    }

    /// The canonical string form of a test peer — what an event carries in its
    /// `source_id` / `proposer_id`, since those are still wire strings.
    fn test_peer_str(name: &str) -> String {
        test_peer_id(name).as_uuid().to_string()
    }

    fn manifest(peer_id: &str, key: &ContractSigningKey) -> AircPeerManifest {
        let pubkey_hex =
            SignedContractEvent::sign(EVENT_CONTRACT_PROPOSED, proposed_payload(peer_id), key, 1)
                .unwrap()
                .signer_pubkey_hex;
        AircPeerManifest {
            peer_id: test_peer_id(peer_id),
            display_name: None,
            room_ids: vec![room()],
            capabilities: vec![AircPeerCapability {
                id: "continuum.lora.invoke".to_string(),
                label: None,
                version: None,
            }],
            signing_pubkey_hex: pubkey_hex,
            advertised_at_ms: 1,
            expires_at_ms: None,
        }
    }

    fn signed_contract_event(peer_id: &str, key: &ContractSigningKey) -> AircRealtimeEnvelope {
        let signed =
            SignedContractEvent::sign(EVENT_CONTRACT_PROPOSED, proposed_payload(peer_id), key, 2)
                .unwrap();
        AircRealtimeEnvelope {
            event_id: "event-1".to_string(),
            room_id: room(),
            source_id: test_peer_str(peer_id),
            target_id: None,
            created_at_ms: 2,
            delivery: AircRealtimeDelivery::Durable,
            payload: AircRealtimePayload::ExistingSchema {
                payload: AircRealtimePayloadRef::inline(
                    AircRealtimeSchema::EventBridgePayload,
                    serde_json::to_value(signed).unwrap(),
                ),
            },
            trace_id: None,
        }
    }

    fn replay(
        events: Vec<AircRealtimeEnvelope>,
        active_peer_manifests: Vec<AircPeerManifest>,
    ) -> AircRealtimeReplayResult {
        AircRealtimeReplayResult {
            room_id: room(),
            events,
            cursor: Some(AircReplayCursor {
                room_id: room(),
                lamport: 1,
                event_id: "event-1".to_string(),
                observed_at_ms: Some(2),
            }),
            active_presence: Vec::new(),
            active_subscriptions: Vec::new(),
            active_peer_manifests,
            capability_index: Vec::new(),
        }
    }

    #[test]
    fn verifies_contract_event_against_peer_manifest_pubkey() {
        let key = ContractSigningKey::generate();
        let peer_id = "peer-a";
        let result = verify_contract_replay(&replay(
            vec![signed_contract_event(peer_id, &key)],
            vec![manifest(peer_id, &key)],
        ))
        .unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].contract_id, "contract-1");
        assert_eq!(result[0].event_name, EVENT_CONTRACT_PROPOSED);
        assert_eq!(result[0].signer_peer_id, test_peer_str(peer_id));
    }

    #[test]
    fn rejects_contract_event_without_peer_manifest() {
        let key = ContractSigningKey::generate();
        let error = verify_contract_replay(&replay(
            vec![signed_contract_event("peer-a", &key)],
            Vec::new(),
        ))
        .unwrap_err();

        assert!(matches!(
            error,
            ContractVerificationError::MissingPeerManifest { .. }
        ));
    }

    #[test]
    fn rejects_contract_event_when_manifest_pubkey_differs() {
        let signer = ContractSigningKey::generate();
        let other = ContractSigningKey::generate();
        let error = verify_contract_replay(&replay(
            vec![signed_contract_event("peer-a", &signer)],
            vec![manifest("peer-a", &other)],
        ))
        .unwrap_err();

        assert!(matches!(
            error,
            ContractVerificationError::ManifestPubkeyMismatch { .. }
        ));
    }

    #[test]
    fn rejects_contract_event_when_source_id_is_not_signer() {
        let key = ContractSigningKey::generate();
        let mut event = signed_contract_event("peer-a", &key);
        event.source_id = "peer-b".to_string();
        let error = verify_contract_replay(&replay(vec![event], vec![manifest("peer-a", &key)]))
            .unwrap_err();

        assert!(matches!(
            error,
            ContractVerificationError::SourcePeerMismatch { .. }
        ));
    }

    #[test]
    fn ignores_non_contract_event_bridge_payloads() {
        let event = AircRealtimeEnvelope::new(
            "event-2".to_string(),
            room(),
            "peer-a".to_string(),
            2,
            AircRealtimePayload::ExistingSchema {
                payload: AircRealtimePayloadRef::inline(
                    AircRealtimeSchema::EventBridgePayload,
                    serde_json::json!({"eventName": "chat:posted", "payload": {}}),
                ),
            },
        );

        let result = verify_contract_replay(&replay(vec![event], Vec::new())).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn rejects_tampered_contract_event_signature() {
        let key = ContractSigningKey::generate();
        let mut event = signed_contract_event("peer-a", &key);
        if let AircRealtimePayload::ExistingSchema { payload } = &mut event.payload {
            payload.inline.as_mut().unwrap()["payload"]["maxBid"] = serde_json::json!(10);
        }

        let error = verify_contract_replay(&replay(vec![event], vec![manifest("peer-a", &key)]))
            .unwrap_err();

        assert!(matches!(
            error,
            ContractVerificationError::SignatureRejected { .. }
        ));
    }
}
