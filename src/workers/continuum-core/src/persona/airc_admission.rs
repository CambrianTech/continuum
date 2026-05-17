//! AIRC envelope -> persona admission candidate conversion.
//!
//! This is the protocol edge for continuum#1121's AIRC memory path. It
//! converts a signed AIRC message envelope into an `AdmissionCandidate` with
//! `EngramOrigin::Airc` provenance. It does not persist the engram and does
//! not decide whether the message is memorable; those remain the
//! `AdmissionGate`/recipe/store responsibilities.

use serde::{Deserialize, Serialize};
use thiserror::Error;
use ts_rs::TS;

use super::admission::AdmissionCandidate;
use super::engram::{AircMessageRef, EngramKind, EngramOrigin, TrustState};
use super::inbox_admission::content_hash_sha256;

/// Signed AIRC message envelope material needed for memory admission.
///
/// The trust tier is caller-supplied because trust is about the sender's
/// standing in the polity, not which client binary emitted the bytes.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/persona/AircAdmissionEnvelope.ts"
)]
pub struct AircAdmissionEnvelope {
    pub room_id: String,
    pub message_id: String,
    pub sender_id: String,
    #[ts(type = "number")]
    pub sent_at_ms: u64,
    #[ts(type = "number")]
    pub received_at_ms: u64,
    pub content: String,
    pub content_hash: String,
    pub signature: String,
    #[serde(default)]
    pub proof_refs: Vec<String>,
    pub schema_version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub client_name: Option<String>,
    pub trust_state: TrustState,
    #[serde(default)]
    pub recall_keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error, TS)]
#[serde(tag = "error", content = "detail")]
#[ts(
    export,
    export_to = "../../../shared/generated/persona/AircAdmissionConversionError.ts"
)]
pub enum AircAdmissionConversionError {
    #[error("AIRC admission envelope field is empty: {field}")]
    EmptyField { field: &'static str },
    #[error("AIRC admission content_hash mismatch: expected {expected}, got {actual}")]
    ContentHashMismatch { expected: String, actual: String },
}

/// Convert signed AIRC envelope metadata into the protocol-compatible
/// provenance reference carried by `EngramOrigin::Airc`.
pub fn airc_envelope_to_ref(
    envelope: &AircAdmissionEnvelope,
) -> Result<AircMessageRef, AircAdmissionConversionError> {
    validate_required(envelope)?;
    let expected = content_hash_sha256(&envelope.content);
    if envelope.content_hash != expected {
        return Err(AircAdmissionConversionError::ContentHashMismatch {
            expected,
            actual: envelope.content_hash.clone(),
        });
    }

    Ok(AircMessageRef {
        transport: "airc".to_string(),
        room_id: envelope.room_id.clone(),
        message_id: envelope.message_id.clone(),
        sender_id: envelope.sender_id.clone(),
        sent_at_ms: envelope.sent_at_ms,
        received_at_ms: envelope.received_at_ms,
        content_hash: envelope.content_hash.clone(),
        signature: envelope.signature.clone(),
        proof_refs: envelope.proof_refs.clone(),
        schema_version: envelope.schema_version.clone(),
        client_name: envelope.client_name.clone(),
    })
}

/// Convert a signed AIRC envelope into the candidate consumed by the
/// admission gate. The output is still only a candidate: the persona's
/// admission recipe decides whether it becomes an engram.
pub fn airc_envelope_to_candidate(
    envelope: &AircAdmissionEnvelope,
) -> Result<AdmissionCandidate, AircAdmissionConversionError> {
    let reference = airc_envelope_to_ref(envelope)?;
    let recall_keys = airc_recall_keys(envelope);

    Ok(AdmissionCandidate {
        content: envelope.content.clone(),
        kind: EngramKind::Episodic,
        origin: EngramOrigin::Airc(reference),
        trust_state: envelope.trust_state,
        recall_keys,
        content_hash: envelope.content_hash.clone(),
    })
}

fn validate_required(envelope: &AircAdmissionEnvelope) -> Result<(), AircAdmissionConversionError> {
    for (field, value) in [
        ("room_id", envelope.room_id.as_str()),
        ("message_id", envelope.message_id.as_str()),
        ("sender_id", envelope.sender_id.as_str()),
        ("content", envelope.content.as_str()),
        ("content_hash", envelope.content_hash.as_str()),
        ("signature", envelope.signature.as_str()),
        ("schema_version", envelope.schema_version.as_str()),
    ] {
        if value.trim().is_empty() {
            return Err(AircAdmissionConversionError::EmptyField { field });
        }
    }
    Ok(())
}

fn airc_recall_keys(envelope: &AircAdmissionEnvelope) -> Vec<String> {
    let mut keys = Vec::with_capacity(envelope.recall_keys.len() + 2);
    keys.push(format!("airc:room:{}", envelope.room_id));
    keys.push(format!("airc:sender:{}", envelope.sender_id));
    keys.extend(
        envelope
            .recall_keys
            .iter()
            .filter(|key| !key.trim().is_empty())
            .cloned(),
    );
    keys
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::{
        AdmissionConfig, AdmissionContext, AdmissionDecision, AdmissionDropReason, AdmissionError,
        AdmissionGate, HeuristicIsMemorable, SeenContentLookup, SeenEventLookup,
    };
    use std::collections::HashMap;
    use std::sync::Mutex;
    use uuid::Uuid;

    const FIXED_SENT_MS: u64 = 1_715_625_600_000;
    const FIXED_RECEIVED_MS: u64 = 1_715_625_601_000;

    #[derive(Default)]
    struct SeenContent(Mutex<HashMap<String, Uuid>>);

    impl SeenContentLookup for SeenContent {
        fn find_by_content_hash(&self, hash: &str) -> Option<Uuid> {
            self.0.lock().unwrap().get(hash).copied()
        }
    }

    #[derive(Default)]
    struct SeenEvents(Mutex<HashMap<String, u64>>);

    impl SeenEventLookup for SeenEvents {
        fn first_seen_ms(&self, event_id: &str) -> Option<u64> {
            self.0.lock().unwrap().get(event_id).copied()
        }
    }

    fn envelope(content: &str) -> AircAdmissionEnvelope {
        AircAdmissionEnvelope {
            room_id: "cambriantech".to_string(),
            message_id: "msg-abc-123".to_string(),
            sender_id: "airc-8a5e".to_string(),
            sent_at_ms: FIXED_SENT_MS,
            received_at_ms: FIXED_RECEIVED_MS,
            content: content.to_string(),
            content_hash: content_hash_sha256(content),
            signature: "sig-base64".to_string(),
            proof_refs: vec!["proof:one".to_string()],
            schema_version: "v1".to_string(),
            client_name: Some("third-party-emitter".to_string()),
            trust_state: TrustState::ApprovedPeer,
            recall_keys: vec!["design".to_string()],
        }
    }

    #[test]
    fn airc_envelope_to_ref_preserves_protocol_fields() {
        let env = envelope("durable design note for admission");
        let reference = airc_envelope_to_ref(&env).expect("valid envelope");

        assert_eq!(reference.transport, "airc");
        assert_eq!(reference.room_id, env.room_id);
        assert_eq!(reference.message_id, env.message_id);
        assert_eq!(reference.sender_id, env.sender_id);
        assert_eq!(reference.sent_at_ms, FIXED_SENT_MS);
        assert_eq!(reference.received_at_ms, FIXED_RECEIVED_MS);
        assert_eq!(reference.content_hash, env.content_hash);
        assert_eq!(reference.signature, env.signature);
        assert_eq!(reference.proof_refs, vec!["proof:one".to_string()]);
        assert_eq!(reference.schema_version, "v1");
        assert_eq!(
            reference.client_name,
            Some("third-party-emitter".to_string())
        );
    }

    #[test]
    fn airc_envelope_to_candidate_builds_airc_origin() {
        let env = envelope("this message should become an airc-origin candidate");
        let candidate = airc_envelope_to_candidate(&env).expect("valid candidate");

        assert_eq!(candidate.content, env.content);
        assert_eq!(candidate.kind, EngramKind::Episodic);
        assert_eq!(candidate.trust_state, TrustState::ApprovedPeer);
        assert_eq!(candidate.content_hash, env.content_hash);
        assert_eq!(
            candidate.recall_keys,
            vec![
                "airc:room:cambriantech".to_string(),
                "airc:sender:airc-8a5e".to_string(),
                "design".to_string()
            ]
        );
        assert!(matches!(candidate.origin, EngramOrigin::Airc(_)));
    }

    #[test]
    fn client_name_does_not_change_trust_state() {
        let mut env = envelope("trust comes from polity state, not client name");
        env.client_name = Some("official-airc".to_string());
        let official = airc_envelope_to_candidate(&env).expect("official candidate");

        env.client_name = Some("independent-client".to_string());
        let independent = airc_envelope_to_candidate(&env).expect("independent candidate");

        assert_eq!(official.trust_state, independent.trust_state);
        assert_eq!(independent.trust_state, TrustState::ApprovedPeer);
    }

    #[test]
    fn content_hash_mismatch_refuses_conversion() {
        let mut env = envelope("tamper-detect this content");
        env.content_hash = "sha256:not-the-content".to_string();

        match airc_envelope_to_candidate(&env) {
            Err(AircAdmissionConversionError::ContentHashMismatch { expected, actual }) => {
                assert_eq!(expected, content_hash_sha256("tamper-detect this content"));
                assert_eq!(actual, "sha256:not-the-content");
            }
            other => panic!("expected hash mismatch, got {other:?}"),
        }
    }

    #[test]
    fn empty_required_field_refuses_conversion() {
        let mut env = envelope("missing signatures are structural errors");
        env.signature.clear();

        match airc_envelope_to_candidate(&env) {
            Err(AircAdmissionConversionError::EmptyField { field }) => {
                assert_eq!(field, "signature");
            }
            other => panic!("expected empty signature field error, got {other:?}"),
        }
    }

    #[test]
    fn converted_candidate_admits_through_structural_gate() {
        let env = envelope("a durable architecture decision from an approved airc peer");
        let candidate = airc_envelope_to_candidate(&env).expect("valid candidate");
        let content = SeenContent::default();
        let events = SeenEvents::default();
        let config = AdmissionConfig::permissive_v1();
        let ctx = AdmissionContext::new(&config, &content, &events);

        let decision =
            AdmissionGate::admit(&candidate, &HeuristicIsMemorable::default_v1(), &ctx, None)
                .expect("approved airc candidate should pass structural gate");

        match decision {
            AdmissionDecision::Admit { engram, .. } => {
                assert!(matches!(engram.origin, EngramOrigin::Airc(_)));
                assert_eq!(engram.content, env.content);
                assert_eq!(engram.trust_state_at_admission, TrustState::ApprovedPeer);
            }
            other => panic!("expected Admit, got {other:?}"),
        }
    }

    #[test]
    fn converted_candidate_uses_message_id_for_replay_refusal() {
        let env = envelope("replay protection should key by airc message id");
        let candidate = airc_envelope_to_candidate(&env).expect("valid candidate");
        let content = SeenContent::default();
        let events = SeenEvents::default();
        events
            .0
            .lock()
            .unwrap()
            .insert("msg-abc-123".to_string(), FIXED_RECEIVED_MS);
        let config = AdmissionConfig::permissive_v1();
        let ctx = AdmissionContext::new(&config, &content, &events);

        match AdmissionGate::admit(&candidate, &HeuristicIsMemorable::default_v1(), &ctx, None) {
            Err(AdmissionError::ReplayDetected {
                event_id,
                previously_seen_at_ms,
            }) => {
                assert_eq!(event_id, "msg-abc-123");
                assert_eq!(previously_seen_at_ms, FIXED_RECEIVED_MS);
            }
            other => panic!("expected replay refusal, got {other:?}"),
        }
    }

    #[test]
    fn converted_candidate_preserves_policy_drop_result() {
        let env = envelope("short");
        let candidate = airc_envelope_to_candidate(&env).expect("valid candidate");
        let content = SeenContent::default();
        let events = SeenEvents::default();
        let config = AdmissionConfig::permissive_v1();
        let ctx = AdmissionContext::new(&config, &content, &events);

        match AdmissionGate::admit(&candidate, &HeuristicIsMemorable::default_v1(), &ctx, None)
            .expect("short content is a policy decision, not conversion failure")
        {
            AdmissionDecision::Drop {
                reason: AdmissionDropReason::NotMemorable { .. },
            } => {}
            other => panic!("expected Drop::NotMemorable, got {other:?}"),
        }
    }

    #[test]
    fn export_bindings_airc_admission_envelope() {
        let cfg = ts_rs::Config::default();
        AircAdmissionEnvelope::export_all(&cfg).unwrap();
    }

    #[test]
    fn export_bindings_airc_admission_conversion_error() {
        let cfg = ts_rs::Config::default();
        AircAdmissionConversionError::export_all(&cfg).unwrap();
    }
}
