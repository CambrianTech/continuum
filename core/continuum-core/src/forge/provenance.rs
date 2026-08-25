//! Genome commons trust spine — rung 1: **provenance**.
//!
//! Every shared gene/lesson artifact carries an unforgeable answer to "who made
//! this, and what did it descend from" — a citizen Ed25519 signature over the
//! artifact's content hash, plus the hashes of its parent alloys. Lineage is thus a
//! hash-linked DAG *made of the artifacts themselves*: there is no registry to own
//! and no index to seize (reticulum). A consumer walks `parent_alloy_hashes` to the
//! root and verifies each hop's signature; an unsigned or broken-chain artifact is
//! untrusted by construction.
//!
//! Reuses the contract signing primitive ([`crate::contracts::signing`]) — one
//! Ed25519 implementation in the tree, the same key kind that makes a citizen itself
//! on the grid ([[the-grid-identity-spine-durable-id-fluid-location]]). This module
//! adds NO crypto; it defines the signed envelope and the walk/verify logic.
//!
//! See `docs/genome/GENOME-COMMONS-TRUST-SPINE.md`. Purity (the interrogation gate)
//! and integrity (consumer-side A/B) are the sibling rungs; this one is their anchor
//! because both attach their receipts to a *signed* identity.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::contracts::signing::{
    canonical_hash, ContractSigningKey, ContractVerifyingKey, SigningError,
};

/// The signed provenance block stamped onto a shared artifact at publish time.
///
/// `content_hash` is the SHA-256 of the artifact's canonical bytes (the same bytes
/// the alloy hash covers); the signature is over the tuple
/// `(content_hash, parent_alloy_hashes)` so neither the payload nor the claimed
/// lineage can be altered without breaking it. The pubkey travels with the block so
/// a consumer needs nothing but the artifact to verify — no key server, no lookup.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/forge/GenomeProvenance.ts")]
pub struct GenomeProvenance {
    /// The forging citizen's public key, hex-encoded (32-byte compressed point).
    /// This IS the identity a consumer trusts + the reputation overlay accrues to.
    pub signer_pubkey_hex: String,
    /// SHA-256 (hex) of the artifact's canonical content — the thing signed.
    pub content_hash_hex: String,
    /// Alloy hashes of every direct parent (empty for a root gene). Walking these
    /// to their roots IS browsing the provider graph — public by construction.
    #[serde(default)]
    pub parent_alloy_hashes: Vec<String>,
    /// The 64-byte ed25519 signature over `(content_hash, parents)`, hex-encoded.
    pub signature_hex: String,
}

/// What the signature covers — canonicalized, then hashed, then signed. Kept as its
/// own serializable tuple so signer and verifier hash byte-identical input (the
/// determinism the contract layer already relies on).
#[derive(Serialize)]
struct SignedPayload<'a> {
    content_hash_hex: &'a str,
    parent_alloy_hashes: &'a [String],
}

impl GenomeProvenance {
    /// Stamp provenance: hash the artifact's canonical bytes, bind the declared
    /// parents, and sign the pair with the citizen's key. `artifact_canonical` is
    /// the exact bytes the alloy hash is computed over (pass the same serialization).
    pub fn sign(
        signing_key: &ContractSigningKey,
        artifact_canonical: &[u8],
        parent_alloy_hashes: Vec<String>,
    ) -> Result<Self, SigningError> {
        let content_hash_hex = hex(&sha256(artifact_canonical));
        let payload = SignedPayload {
            content_hash_hex: &content_hash_hex,
            parent_alloy_hashes: &parent_alloy_hashes,
        };
        // Hash-then-sign, same shape as the contract envelope path.
        let digest = canonical_hash(&payload)?;
        let signature = signing_key.sign(&digest);
        Ok(Self {
            signer_pubkey_hex: hex(&signing_key.verifying_key().to_bytes()),
            content_hash_hex,
            parent_alloy_hashes,
            signature_hex: hex(&signature),
        })
    }

    /// Verify this block against the artifact's canonical bytes: the content hash
    /// must match the bytes AND the signature must verify under the embedded pubkey
    /// over `(content_hash, parents)`. Returns the verified signer key on success —
    /// a caller can never treat a failed verify as success (the error type forces
    /// it), the same discipline as [`ContractVerifyingKey::verify`].
    pub fn verify(
        &self,
        artifact_canonical: &[u8],
    ) -> Result<ContractVerifyingKey, SigningError> {
        // 1. The claimed content hash must actually be this artifact's hash —
        //    otherwise a valid signature over a DIFFERENT payload could be replayed
        //    onto foreign bytes.
        let actual = hex(&sha256(artifact_canonical));
        if actual != self.content_hash_hex {
            return Err(SigningError::VerificationFailed {
                bytes_signed: artifact_canonical.len(),
            });
        }
        // 2. The signature must verify over the same canonical payload the signer hashed.
        let key = ContractVerifyingKey::from_bytes(&unhex(&self.signer_pubkey_hex)?)?;
        let payload = SignedPayload {
            content_hash_hex: &self.content_hash_hex,
            parent_alloy_hashes: &self.parent_alloy_hashes,
        };
        let digest = canonical_hash(&payload)?;
        key.verify(&digest, &unhex(&self.signature_hex)?)?;
        Ok(key)
    }
}

/// One hop in a verified lineage walk — an artifact's alloy hash and the signer that
/// vouched for it. A consumer's browse/trust surface renders these root-first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/forge/LineageHop.ts")]
pub struct LineageHop {
    pub alloy_hash: String,
    pub signer_pubkey_hex: String,
}

fn sha256(bytes: &[u8]) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    let mut h = Sha256::new();
    h.update(bytes);
    h.finalize().into()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(s: &str) -> Result<Vec<u8>, SigningError> {
    if s.len() % 2 != 0 {
        return Err(SigningError::InvalidPublicKey); // odd-length hex is malformed input, never a valid key/sig
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| SigningError::InvalidPublicKey))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the provenance spine's whole contract — a citizen-signed
    // artifact verifies under its own key over its own bytes+parents, and EVERY
    // tamper (foreign bytes, altered lineage, wrong key) is rejected. This is the
    // anchor the purity + integrity rungs attach receipts to; if it can be forged,
    // the commons has no trust floor.
    #[test]
    fn signed_artifact_verifies_and_every_tamper_is_rejected() {
        let key = ContractSigningKey::generate();
        let bytes = br#"{"gene":"tb21-terminal-discipline","forged_at_ms":123}"#;
        let parents = vec!["sha256:aa61c4bdf463847c".to_string()];
        let prov = GenomeProvenance::sign(&key, bytes, parents.clone()).unwrap();

        // Honest verify returns the signer.
        let signer = prov.verify(bytes).expect("clean provenance verifies");
        assert_eq!(signer.to_bytes(), key.verifying_key().to_bytes());

        // Foreign bytes (a valid signature replayed onto a different artifact) → reject.
        assert!(prov.verify(br#"{"gene":"something-else"}"#).is_err());

        // Altered lineage (claim a different parent than was signed) → reject.
        let mut lineage_tamper = prov.clone();
        lineage_tamper.parent_alloy_hashes = vec!["sha256:deadbeef".to_string()];
        assert!(lineage_tamper.verify(bytes).is_err());

        // Wrong signer key substituted → reject.
        let other = ContractSigningKey::generate();
        let mut key_tamper = prov.clone();
        key_tamper.signer_pubkey_hex = hex(&other.verifying_key().to_bytes());
        assert!(key_tamper.verify(bytes).is_err());

        // A root gene (no parents) is a legitimate, verifiable shape.
        let root = GenomeProvenance::sign(&key, bytes, Vec::new()).unwrap();
        assert!(root.parent_alloy_hashes.is_empty());
        assert!(root.verify(bytes).is_ok());
    }
}
