//! Signed contract event envelope wrapper.
//!
//! Roadmap item L1-6 (see docs/grid/GRID-MIGRATION-ROADMAP.md).
//! Spec: GRID-BUS-ARCHITECTURE §4.4 + MULTI-PEER-COMMANDS §7.
//!
//! Every contract event on the wire is a `SignedContractEvent<P>` where
//! `P` is one of the 8 payload types from `event_classes.rs`. The
//! envelope carries:
//!   - `event_name`: which class (`contract:proposed`, etc.) — pinned
//!     into the signed bytes so an envelope can't be relabeled.
//!   - `payload`: the typed event-specific fields.
//!   - `signer_pubkey`: the 32-byte ed25519 public key (hex-encoded on
//!     the wire). Verifies the signature.
//!   - `signature`: 64-byte ed25519 signature (hex-encoded on the wire)
//!     over `canonical_hash(event_name, payload)`.
//!   - `signed_at_unix_ms`: signer's wall-clock at sign time (audit-only;
//!     replay does NOT consult clock skew between peers).
//!
//! The signed bytes pin `event_name` + `payload` together so a
//! malicious replay can't take a valid `bid` signature and present it
//! as a `proposed`. The envelope itself carries the signature; verify
//! recomputes the canonical hash from `(event_name, payload)` and
//! checks against the signer's pubkey.

use crate::contracts::signing::{
    canonical_hash, ContractSigningKey, ContractVerifyingKey, SigningError,
};
use serde::{Deserialize, Serialize};

/// Canonical "what gets signed" intermediate. Carries `event_name`
/// alongside the payload so the signature pins both — relabeling
/// attacks (taking a bid sig and presenting it as a proposed) fail
/// signature verification.
///
/// Private to this module — callers go through `SignedContractEvent::sign`
/// + `::verify`, not by constructing this directly.
#[derive(Debug, Serialize)]
struct SignedBody<'a, P: Serialize> {
    event_name: &'a str,
    payload: &'a P,
}

/// A typed, signed contract event envelope.
///
/// Generic over the payload type `P` so each of the 8 event classes
/// gets its own concrete type at the use site — no `Vec<u8>` opaque
/// payloads, no `serde_json::Value` runtime-type dispatch.
///
/// Wire format (camelCase JSON):
/// ```json
/// {
///   "eventName": "contract:proposed",
///   "payload": { ... payload fields ... },
///   "signerPubkeyHex": "ab12...",
///   "signatureHex": "cd34...",
///   "signedAtUnixMs": 1779800000000
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SignedContractEvent<P> {
    pub event_name: String,
    pub payload: P,
    /// Hex-encoded 32-byte ed25519 public key. ts-rs sees this as
    /// `string` via the host envelope module's manual mapping —
    /// signing keys never cross the wire, only pubkeys.
    pub signer_pubkey_hex: String,
    /// Hex-encoded 64-byte ed25519 signature over the canonical
    /// (event_name, payload) hash.
    pub signature_hex: String,
    /// Wall-clock at sign time. Audit-only; verify does NOT consult.
    pub signed_at_unix_ms: i64,
}

impl<P> SignedContractEvent<P>
where
    P: Serialize,
{
    /// Build a fresh signed envelope. Computes the canonical hash of
    /// `(event_name, payload)`, signs it with `signing_key`, and
    /// returns the populated envelope.
    pub fn sign(
        event_name: impl Into<String>,
        payload: P,
        signing_key: &ContractSigningKey,
        signed_at_unix_ms: i64,
    ) -> Result<Self, SigningError> {
        let event_name = event_name.into();
        let body = SignedBody {
            event_name: &event_name,
            payload: &payload,
        };
        let hash = canonical_hash(&body)?;
        let signature = signing_key.sign(&hash);
        let pubkey = signing_key.verifying_key();
        Ok(Self {
            event_name,
            payload,
            signer_pubkey_hex: hex_encode(&pubkey.to_bytes()),
            signature_hex: hex_encode(&signature),
            signed_at_unix_ms,
        })
    }
}

impl<P> SignedContractEvent<P>
where
    P: Serialize + for<'de> Deserialize<'de>,
{
    /// Verify the envelope's signature.
    ///
    /// Recomputes `canonical_hash(event_name, payload)` from THIS
    /// envelope's fields — does NOT trust any cached digest. Decodes
    /// the embedded pubkey + signature, checks the ed25519 verify.
    ///
    /// Returns `Ok(verified_pubkey)` on success — the caller then
    /// cross-checks the verified pubkey against the L1-4
    /// `presence:peer-manifest` index to confirm the signer's identity
    /// matches what they claim in the payload (`proposer_id`,
    /// `bidder_id`, etc.). That cross-check is L1-6 Phase B and lives
    /// in a downstream replay handler — this layer just gives back
    /// "yes, this 32-byte pubkey signed these bytes."
    pub fn verify(&self) -> Result<ContractVerifyingKey, SigningError> {
        let pubkey_bytes = hex_decode(&self.signer_pubkey_hex)?;
        let signature_bytes = hex_decode(&self.signature_hex)?;
        let pubkey = ContractVerifyingKey::from_bytes(&pubkey_bytes)?;

        // Reconstruct the SAME body shape that sign() hashed.
        let body = SignedBody {
            event_name: &self.event_name,
            payload: &self.payload,
        };
        let hash = canonical_hash(&body)?;

        pubkey.verify(&hash, &signature_bytes)?;
        Ok(pubkey)
    }
}

// ─── Hex encoding helpers ─────────────────────────────────────────────────
//
// Keep tiny + local rather than pulling in the `hex` crate just for this.
// 32-byte pubkeys + 64-byte signatures both round-trip exactly.

fn hex_encode(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(nibble(b >> 4));
        s.push(nibble(b & 0x0F));
    }
    s
}

fn hex_decode(s: &str) -> Result<Vec<u8>, SigningError> {
    if !s.len().is_multiple_of(2) {
        return Err(SigningError::PayloadSerialization(format!(
            "hex string length {} is not even",
            s.len(),
        )));
    }
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(s.len() / 2);
    for chunk in bytes.chunks(2) {
        let hi = un_nibble(chunk[0])?;
        let lo = un_nibble(chunk[1])?;
        out.push((hi << 4) | lo);
    }
    Ok(out)
}

fn nibble(n: u8) -> char {
    match n {
        0..=9 => (b'0' + n) as char,
        10..=15 => (b'a' + n - 10) as char,
        _ => unreachable!("nibble fits in 4 bits"),
    }
}

fn un_nibble(c: u8) -> Result<u8, SigningError> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(SigningError::PayloadSerialization(format!(
            "invalid hex char: 0x{c:02x}",
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::event_classes::{
        ContractBidPayload, ContractProposedPayload, EVENT_CONTRACT_BID, EVENT_CONTRACT_PROPOSED,
    };

    fn sample_proposed() -> ContractProposedPayload {
        ContractProposedPayload {
            contract_id: "c-l1-6-test-001".into(),
            proposer_id: "peer-a".into(),
            alloy_hash: "sha256:dead...beef".into(),
            bid_currency: "".into(),
            max_bid: 0,
            expiry_unix_ms: 1_779_800_000_000,
            required_capability: "inference:ping".into(),
        }
    }

    fn sample_bid() -> ContractBidPayload {
        ContractBidPayload {
            contract_id: "c-l1-6-test-001".into(),
            bidder_id: "peer-b".into(),
            bid_amount: 0,
            max_latency_ms: 100,
            bid_expiry_unix_ms: 1_779_810_000_000,
        }
    }

    #[test]
    fn sign_then_verify_roundtrips() {
        let sk = ContractSigningKey::generate();

        let envelope = SignedContractEvent::sign(
            EVENT_CONTRACT_PROPOSED,
            sample_proposed(),
            &sk,
            1_779_800_000_000,
        )
        .unwrap();

        let verified_pubkey = envelope.verify().expect("fresh envelope must verify");
        assert_eq!(verified_pubkey.to_bytes(), sk.verifying_key().to_bytes());
    }

    #[test]
    fn relabeling_attack_fails() {
        // Sign a payload as `contract:bid`, then relabel the envelope
        // to `contract:proposed` and try to verify — must fail.

        let sk = ContractSigningKey::generate();

        let envelope =
            SignedContractEvent::sign(EVENT_CONTRACT_BID, sample_bid(), &sk, 1_779_800_000_000)
                .unwrap();

        let mut tampered = envelope.clone();
        tampered.event_name = EVENT_CONTRACT_PROPOSED.into();

        let err = tampered.verify().unwrap_err();
        assert!(matches!(err, SigningError::VerificationFailed { .. }));
    }

    #[test]
    fn payload_mutation_fails_verify() {
        let sk = ContractSigningKey::generate();

        let envelope = SignedContractEvent::sign(
            EVENT_CONTRACT_PROPOSED,
            sample_proposed(),
            &sk,
            1_779_800_000_000,
        )
        .unwrap();

        let mut tampered = envelope.clone();
        tampered.payload.max_bid = 9999;

        let err = tampered.verify().unwrap_err();
        assert!(matches!(err, SigningError::VerificationFailed { .. }));
    }

    #[test]
    fn signature_mutation_fails_verify() {
        let sk = ContractSigningKey::generate();

        let envelope = SignedContractEvent::sign(
            EVENT_CONTRACT_PROPOSED,
            sample_proposed(),
            &sk,
            1_779_800_000_000,
        )
        .unwrap();

        let mut tampered = envelope.clone();
        // Flip the LAST hex char so the byte mutates without changing length.
        let last = tampered.signature_hex.pop().unwrap();
        let flipped = if last == '0' { '1' } else { '0' };
        tampered.signature_hex.push(flipped);

        let err = tampered.verify().unwrap_err();
        assert!(matches!(err, SigningError::VerificationFailed { .. }));
    }

    #[test]
    fn pubkey_swap_fails_verify() {
        let sk_a = ContractSigningKey::generate();
        let sk_b = ContractSigningKey::generate();

        let envelope = SignedContractEvent::sign(
            EVENT_CONTRACT_PROPOSED,
            sample_proposed(),
            &sk_a,
            1_779_800_000_000,
        )
        .unwrap();

        let mut tampered = envelope.clone();
        tampered.signer_pubkey_hex = hex_encode(&sk_b.verifying_key().to_bytes());

        let err = tampered.verify().unwrap_err();
        assert!(matches!(err, SigningError::VerificationFailed { .. }));
    }

    #[test]
    fn envelope_round_trips_through_json() {
        let sk = ContractSigningKey::generate();

        let envelope = SignedContractEvent::sign(
            EVENT_CONTRACT_PROPOSED,
            sample_proposed(),
            &sk,
            1_779_800_000_000,
        )
        .unwrap();

        let json = serde_json::to_string(&envelope).unwrap();
        let restored: SignedContractEvent<ContractProposedPayload> =
            serde_json::from_str(&json).unwrap();

        // Restored envelope still verifies — wire round-trip is bit-exact.
        let verified_pubkey = restored.verify().unwrap();
        assert_eq!(verified_pubkey.to_bytes(), sk.verifying_key().to_bytes());
    }

    #[test]
    fn hex_helpers_round_trip() {
        let original: Vec<u8> = (0u8..=255u8).collect();
        let encoded = hex_encode(&original);
        let decoded = hex_decode(&encoded).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn hex_decode_rejects_bad_input() {
        assert!(hex_decode("abc").is_err()); // odd length
        assert!(hex_decode("xy").is_err()); // non-hex chars
    }
}
