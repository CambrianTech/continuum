//! ed25519 signing primitives for L1-6 contract event envelopes.
//!
//! Roadmap item L1-6 (see docs/grid/GRID-MIGRATION-ROADMAP.md).
//! Spec: GRID-BUS-ARCHITECTURE §4.4 + MULTI-PEER-COMMANDS §7.
//!
//! Matches the `ed25519-dalek = "2"` choice in `airc-protocol` so peer
//! signing keys advertised through L1-4's `presence:peer-manifest` use
//! the SAME byte layout that this module verifies. No re-encoding,
//! no protocol bridging.
//!
//! Scope (Phase A — buildable independent of L1-4):
//!   - Key types: `ContractSigningKey` (private), `ContractVerifyingKey` (public).
//!   - `sign(payload_bytes)` / `verify(payload_bytes, sig, pubkey)`.
//!   - `canonical_hash(payload)`: SHA-256 of the canonicalized payload
//!     bytes — the deterministic substance the signature commits to.
//!   - Errors are explicit (`SigningError`); no silent fail-soft paths.
//!
//! Phase B (deferred to a follow-up PR once L1-4 lands):
//!   - Pubkey lookup against the per-peer manifest index.
//!   - Verify-on-replay handler that pulls pubkeys at event-receipt time.

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// Length in bytes of an ed25519 signature.
pub const SIGNATURE_LEN: usize = 64;

/// Length in bytes of an ed25519 public key.
pub const PUBLIC_KEY_LEN: usize = 32;

/// Length in bytes of the SHA-256 canonical hash.
pub const CANONICAL_HASH_LEN: usize = 32;

/// Errors raised by L1-6 signing / verification.
///
/// Every variant carries enough context for a debugger to root-cause —
/// per the global never-swallow-evidence rule, callers must surface
/// these (not silently fall back to "not verified").
#[derive(Debug, Error)]
pub enum SigningError {
    #[error("ed25519 signature is the wrong length: expected {expected}, got {got}")]
    SignatureLength { expected: usize, got: usize },

    #[error("ed25519 public key is the wrong length: expected {expected}, got {got}")]
    PublicKeyLength { expected: usize, got: usize },

    #[error("ed25519 public key bytes are not a valid point on the curve")]
    InvalidPublicKey,

    #[error("ed25519 signature verification failed for {bytes_signed} bytes of payload")]
    VerificationFailed { bytes_signed: usize },

    #[error("payload serialization failed during canonical-hash computation: {0}")]
    PayloadSerialization(String),
}

/// A privately-held ed25519 signing key. Wrapper around
/// `ed25519_dalek::SigningKey` so future migrations (HSM, secure enclave)
/// can swap the backing store without touching call sites.
///
/// Not `Serialize` / `Deserialize` on purpose — signing keys are
/// per-process secrets, never on the wire. The corresponding
/// [`ContractVerifyingKey`] IS serializable (it's the public half).
pub struct ContractSigningKey {
    inner: SigningKey,
}

impl std::fmt::Debug for ContractSigningKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Don't print key bytes. Show only the corresponding pubkey
        // (which is public anyway) so logs aren't useless.
        f.debug_struct("ContractSigningKey")
            .field("verifying_key", &self.verifying_key())
            .finish()
    }
}

impl ContractSigningKey {
    /// Generate a fresh keypair using the OS CSPRNG (`rand::rngs::OsRng`).
    ///
    /// Wrapped here (rather than exposing a generic RNG parameter) so
    /// callers don't accidentally pass `thread_rng()` — which is fast
    /// but NOT a CSPRNG and therefore unsuitable for long-lived
    /// signing keys. The OS RNG is the right default for every L1-6
    /// keygen path; HSM-backed key import goes through `from_bytes`.
    pub fn generate() -> Self {
        use rand::rngs::OsRng;
        Self {
            inner: SigningKey::generate(&mut OsRng),
        }
    }

    /// Construct from raw 32 bytes (e.g. loaded from disk / HSM).
    /// Used by call sites that already have the secret material.
    pub fn from_bytes(bytes: &[u8; 32]) -> Self {
        Self {
            inner: SigningKey::from_bytes(bytes),
        }
    }

    /// The corresponding public key — safe to share with peers (this is
    /// what L1-4's `presence:peer-manifest` advertises).
    pub fn verifying_key(&self) -> ContractVerifyingKey {
        ContractVerifyingKey {
            inner: self.inner.verifying_key(),
        }
    }

    /// Sign the canonical bytes. Returns the 64-byte ed25519 signature.
    ///
    /// Determinism: ed25519 signatures are deterministic per (key,
    /// message). Two signs of the same payload by the same key produce
    /// byte-identical signatures — important for replay-equivalence
    /// checks in the L1-6 audit-replay path.
    pub fn sign(&self, canonical_bytes: &[u8]) -> [u8; SIGNATURE_LEN] {
        self.inner.sign(canonical_bytes).to_bytes()
    }
}

/// The public half of a signing key — appears on the wire (in
/// `presence:peer-manifest` and in signed envelopes' `signer_pubkey`
/// field). Verifies signatures.
///
/// The on-wire representation is the 32-byte compressed point, base64
/// encoded by serde when crossing the JSON boundary. ts-rs sees it as
/// `string` (handled by the `#[ts(type = "string")]` attribute on the
/// envelope wrapper that contains it).
#[derive(Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractVerifyingKey {
    /// Stored as the compressed-Edwards-point byte form. Round-trips
    /// through JSON as a 32-byte sequence (or base64 if encoded that
    /// way by the wrapper).
    inner: VerifyingKey,
}

impl std::fmt::Debug for ContractVerifyingKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let bytes = self.to_bytes();
        // Show first 4 + last 4 bytes hex for log identity without
        // overwhelming output. Public bytes — no secrecy concern.
        write!(
            f,
            "ContractVerifyingKey({:02x}{:02x}{:02x}{:02x}..{:02x}{:02x}{:02x}{:02x})",
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[28], bytes[29], bytes[30], bytes[31],
        )
    }
}

impl ContractVerifyingKey {
    /// Construct from raw 32 bytes. Validates the point is on-curve.
    /// Returns `InvalidPublicKey` on bad bytes (e.g. tampered manifest).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, SigningError> {
        if bytes.len() != PUBLIC_KEY_LEN {
            return Err(SigningError::PublicKeyLength {
                expected: PUBLIC_KEY_LEN,
                got: bytes.len(),
            });
        }
        let mut arr = [0u8; PUBLIC_KEY_LEN];
        arr.copy_from_slice(bytes);
        let inner = VerifyingKey::from_bytes(&arr).map_err(|_| SigningError::InvalidPublicKey)?;
        Ok(Self { inner })
    }

    /// 32-byte compressed-Edwards-point form. Round-trippable via
    /// `from_bytes`.
    pub fn to_bytes(&self) -> [u8; PUBLIC_KEY_LEN] {
        self.inner.to_bytes()
    }

    /// Verify a signature over the canonical bytes. Returns
    /// `VerificationFailed` (not `Ok(false)`) on mismatch so callers
    /// can't accidentally treat a failed verify as success — the only
    /// way past this call is a real cryptographic match.
    pub fn verify(
        &self,
        canonical_bytes: &[u8],
        signature_bytes: &[u8],
    ) -> Result<(), SigningError> {
        if signature_bytes.len() != SIGNATURE_LEN {
            return Err(SigningError::SignatureLength {
                expected: SIGNATURE_LEN,
                got: signature_bytes.len(),
            });
        }
        let mut arr = [0u8; SIGNATURE_LEN];
        arr.copy_from_slice(signature_bytes);
        let sig = Signature::from_bytes(&arr);
        self.inner.verify(canonical_bytes, &sig).map_err(|_| {
            SigningError::VerificationFailed {
                bytes_signed: canonical_bytes.len(),
            }
        })
    }
}

/// Compute the canonical SHA-256 hash of a payload that's about to be
/// signed.
///
/// Why a separate "canonical" step: ed25519 signs whatever bytes you
/// hand it. If we signed `serde_json::to_vec(&payload)` directly, two
/// serializers (or two builds with different feature flags) could
/// produce non-identical byte sequences for the same logical payload,
/// breaking verification. Canonicalization pins the byte sequence to
/// the SORTED-KEYS JSON form (`serde_json`'s default with a key-sorted
/// `BTreeMap` round-trip), then hashes — peers always sign the same
/// 32-byte digest regardless of build.
///
/// Returns the 32-byte SHA-256 of the canonical bytes.
pub fn canonical_hash<T: Serialize>(payload: &T) -> Result<[u8; CANONICAL_HASH_LEN], SigningError> {
    // 1. Serialize to JSON value (handles any T: Serialize).
    let value =
        serde_json::to_value(payload).map_err(|e| SigningError::PayloadSerialization(e.to_string()))?;
    // 2. Reserialize through BTreeMap-backed Value to get key-sorted output.
    //    serde_json's Value uses BTreeMap when the `preserve_order`
    //    feature is OFF (default). So `to_vec(&value)` yields keys in
    //    lexicographic order. This is the canonical form.
    let canonical_bytes = serde_json::to_vec(&value)
        .map_err(|e| SigningError::PayloadSerialization(e.to_string()))?;
    // 3. SHA-256 the canonical bytes.
    let mut hasher = Sha256::new();
    hasher.update(&canonical_bytes);
    let digest = hasher.finalize();
    let mut out = [0u8; CANONICAL_HASH_LEN];
    out.copy_from_slice(&digest);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize)]
    struct DummyPayload {
        contract_id: String,
        bid_zmw: u64,
        peer: String,
    }

    fn dummy() -> DummyPayload {
        DummyPayload {
            contract_id: "c-001".into(),
            bid_zmw: 42,
            peer: "peer-a".into(),
        }
    }

    #[test]
    fn keygen_then_sign_then_verify_roundtrips() {
          
        let sk = ContractSigningKey::generate();
        let vk = sk.verifying_key();

        let hash = canonical_hash(&dummy()).unwrap();
        let sig = sk.sign(&hash);

        vk.verify(&hash, &sig).expect("fresh signature must verify");
    }

    #[test]
    fn pubkey_round_trips_through_bytes() {
          
        let sk = ContractSigningKey::generate();
        let vk = sk.verifying_key();

        let bytes = vk.to_bytes();
        let restored = ContractVerifyingKey::from_bytes(&bytes).unwrap();
        assert_eq!(vk.to_bytes(), restored.to_bytes());

        // Restored key still verifies signatures.
        let hash = canonical_hash(&dummy()).unwrap();
        let sig = sk.sign(&hash);
        restored.verify(&hash, &sig).unwrap();
    }

    #[test]
    fn bad_signature_bytes_fail_loud() {
          
        let sk = ContractSigningKey::generate();
        let vk = sk.verifying_key();

        let hash = canonical_hash(&dummy()).unwrap();
        let mut sig = sk.sign(&hash);
        // Flip a single bit. Per ed25519, this MUST fail.
        sig[0] ^= 0x01;

        let err = vk.verify(&hash, &sig).unwrap_err();
        assert!(matches!(err, SigningError::VerificationFailed { .. }));
    }

    #[test]
    fn wrong_payload_fails_loud() {
          
        let sk = ContractSigningKey::generate();
        let vk = sk.verifying_key();

        let hash = canonical_hash(&dummy()).unwrap();
        let sig = sk.sign(&hash);

        // Sign payload A, verify against payload B — must fail.
        let other_hash = canonical_hash(&DummyPayload {
            contract_id: "c-001".into(),
            bid_zmw: 43, // <-- changed
            peer: "peer-a".into(),
        })
        .unwrap();
        assert_ne!(hash, other_hash);
        let err = vk.verify(&other_hash, &sig).unwrap_err();
        assert!(matches!(err, SigningError::VerificationFailed { .. }));
    }

    #[test]
    fn cross_key_verify_fails_loud() {
          
        let sk_a = ContractSigningKey::generate();
        let sk_b = ContractSigningKey::generate();

        let hash = canonical_hash(&dummy()).unwrap();
        let sig_by_a = sk_a.sign(&hash);

        // B's pubkey must NOT verify A's signature.
        let err = sk_b.verifying_key().verify(&hash, &sig_by_a).unwrap_err();
        assert!(matches!(err, SigningError::VerificationFailed { .. }));
    }

    #[test]
    fn signature_is_deterministic() {
          
        let sk = ContractSigningKey::generate();

        let hash = canonical_hash(&dummy()).unwrap();
        let sig1 = sk.sign(&hash);
        let sig2 = sk.sign(&hash);
        assert_eq!(sig1, sig2, "ed25519 must be deterministic for replay-equivalence");
    }

    #[test]
    fn canonical_hash_stable_across_field_order() {
        // Even if a struct is serialized with fields in a different
        // declaration order, the canonical hash must agree (because
        // serde_json's default Value uses BTreeMap → key-sorted output).
        #[derive(Serialize)]
        struct Order1 {
            a: u32,
            z: u32,
        }
        #[derive(Serialize)]
        struct Order2 {
            z: u32,
            a: u32,
        }
        let h1 = canonical_hash(&Order1 { a: 1, z: 2 }).unwrap();
        let h2 = canonical_hash(&Order2 { z: 2, a: 1 }).unwrap();
        assert_eq!(h1, h2, "canonical hash MUST be order-insensitive");
    }

    #[test]
    fn signature_length_validation() {
          
        let vk = ContractSigningKey::generate().verifying_key();
        let err = vk.verify(b"anything", &[0u8; 63]).unwrap_err();
        assert!(matches!(err, SigningError::SignatureLength { expected: 64, got: 63 }));
    }

    #[test]
    fn pubkey_length_validation() {
        let err = ContractVerifyingKey::from_bytes(&[0u8; 31]).unwrap_err();
        assert!(matches!(err, SigningError::PublicKeyLength { expected: 32, got: 31 }));
    }

    // NOTE: Point-validation (rejecting 32 bytes that decompress off-curve)
    // is delegated to `ed25519_dalek::VerifyingKey::from_bytes` — its own
    // test suite covers curve-membership. We don't duplicate that here.
    // Tampered-input coverage is exercised end-to-end by the envelope tests
    // (`pubkey_swap_fails_verify` etc.), and length-mismatch is covered by
    // `pubkey_length_validation` above.
}
