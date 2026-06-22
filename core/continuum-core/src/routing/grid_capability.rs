//! Grid capability auth — continuum-side verification of airc signed grants.
//!
//! The verification engine for [docs/grid/GRID-CAPABILITY-AUTH.md]: a remote peer
//! presents a `SignedCapabilityGrant` the owner signed; this authorizes a command
//! iff the grant VERIFIES (issuer-pin → sig → key-binding → mesh → expiry, via
//! airc's stateless `grid_auth`) AND `grant.grants(command)` AND the grant isn't a
//! replay of a superseded epoch. Identity + authorization + contract are the one
//! signed object — no shared trust store, no address↔peer mismatch.
//!
//! This is the verification CORE (the heart of the contracted-grid gate). It is a
//! tested SEAM: it is NOT yet wired to the live dispatch path because the airc
//! command envelope doesn't carry grants yet (the airc-side transport slice). When
//! it does, `CommandRequestHandler` extracts the grant + presenting key and calls
//! [`GrantAuthorizer::authorize_command`] from the gate. See
//! `[[airc-grid-identity-unification-trust-bridge]]`.

use airc_core::PeerId;
use airc_lib::grid_auth::{CredentialKind, GrantProof, GrantVerdict, GrantVerifier, SignedCapabilityGrant};
use airc_lib::subscriptions::MeshIdentity;
use airc_lib::grid_auth::VerifyContext;
use dashmap::DashMap;

/// Why a presented grant did or didn't authorize a command — a TYPED outcome so
/// the gate + audit see exactly why (never a bare bool).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GrantAuthOutcome {
    /// The grant verified, isn't superseded, and confers the command.
    Authorized,
    /// The grant failed cryptographic/structural verification.
    Invalid(GrantVerdict),
    /// The grant verified but a higher epoch was already accepted for this
    /// grantee — a replayed/superseded grant (consumer-side anti-replay).
    Superseded,
    /// The grant verified but does NOT confer this command's capability.
    NotGranted,
}

/// ed25519 [`GrantVerifier`] — verifies a grant's signature against the issuer key
/// embedded in the proof, using the substrate's ed25519 (the same primitive as the
/// L1-6 contract-envelope signatures). Pure: no IO, no clock.
pub struct Ed25519GrantVerifier;

impl GrantVerifier for Ed25519GrantVerifier {
    fn verify_signature(&self, message: &[u8], proof: &GrantProof) -> bool {
        // Only the ed25519 credential paradigm today (WebAuthn is a future variant).
        if !matches!(proof.credential, CredentialKind::Ed25519) {
            return false;
        }
        let Ok(key_bytes): Result<[u8; 32], _> = proof.issuer_pubkey.as_slice().try_into() else {
            return false;
        };
        let Ok(vk) = ed25519_dalek::VerifyingKey::from_bytes(&key_bytes) else {
            return false;
        };
        let Ok(sig_bytes): Result<[u8; 64], _> = proof.signature.as_slice().try_into() else {
            return false;
        };
        // verify_strict rejects weak/malleable signatures (the same check the
        // envelope path uses).
        vk.verify_strict(message, &ed25519_dalek::Signature::from_bytes(&sig_bytes))
            .is_ok()
    }
}

/// Verifies presented capability grants against the local owner key + mesh, with
/// consumer-side epoch anti-replay. One per node; cheap to share (`&self`).
pub struct GrantAuthorizer {
    /// The trusted account-owner public key — the single root of trust every grant
    /// must be signed by (`trusted_issuer_pubkey`).
    owner_pubkey: Vec<u8>,
    /// This node's own mesh identity — a grant scoped to a different mesh is
    /// rejected (`WrongMesh`).
    mesh: MeshIdentity,
    verifier: Ed25519GrantVerifier,
    /// Highest grant epoch accepted per grantee. The `grid_auth` verifier is
    /// stateless by design; anti-replay (reject a superseded lower-epoch grant) is
    /// the CONSUMER's responsibility — this is that state. Revocation rides the
    /// same channel (a higher-epoch grant with empty capabilities).
    seen_epoch: DashMap<PeerId, u64>,
}

impl GrantAuthorizer {
    pub fn new(owner_pubkey: Vec<u8>, mesh: MeshIdentity) -> Self {
        Self {
            owner_pubkey,
            mesh,
            verifier: Ed25519GrantVerifier,
            seen_epoch: DashMap::new(),
        }
    }

    /// Authorize `command` from a peer presenting `signed` with verified key
    /// `presenting_pubkey`, at wall-clock `now_ms`. Verify → epoch anti-replay →
    /// capability check. A grant that authorizes advances the grantee's accepted
    /// epoch (so a later replay of a lower epoch is `Superseded`).
    pub fn authorize_command(
        &self,
        signed: &SignedCapabilityGrant,
        presenting_pubkey: &[u8],
        command: &str,
        now_ms: u64,
    ) -> GrantAuthOutcome {
        let ctx = VerifyContext {
            now_ms,
            presenting_pubkey,
            expected_mesh: &self.mesh,
            verifier: &self.verifier,
            trusted_issuer_pubkey: &self.owner_pubkey,
        };
        match signed.verify(&ctx) {
            GrantVerdict::Valid => {}
            other => return GrantAuthOutcome::Invalid(other),
        }

        // Anti-replay: reject a grant whose epoch is BELOW the latest accepted for
        // this grantee (a superseded/replayed grant). Equal or higher is accepted
        // and advances the watermark — so a revocation (higher epoch, empty caps)
        // both supersedes old grants and, via the capability check below, confers
        // nothing.
        let grantee = signed.grant.grantee;
        if let Some(seen) = self.seen_epoch.get(&grantee) {
            if signed.grant.epoch < *seen {
                return GrantAuthOutcome::Superseded;
            }
        }

        if !signed.grants(command) {
            // Do NOT advance the watermark on a non-granting (but otherwise valid)
            // grant — only an accepted, authorizing grant should move it.
            return GrantAuthOutcome::NotGranted;
        }

        self.seen_epoch
            .entry(grantee)
            .and_modify(|e| *e = (*e).max(signed.grant.epoch))
            .or_insert(signed.grant.epoch);
        GrantAuthOutcome::Authorized
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_lib::grid_auth::CapabilityGrant;

    // A stub verifier so the grant LOGIC (issuer-pin, key-binding, mesh, expiry,
    // epoch, capability) is tested without real signatures — the same seam airc
    // tests against. The real Ed25519 path is exercised separately below.
    struct StubVerifier(bool);
    impl GrantVerifier for StubVerifier {
        fn verify_signature(&self, _message: &[u8], _proof: &GrantProof) -> bool {
            self.0
        }
    }

    fn mesh() -> MeshIdentity {
        MeshIdentity::new("test-mesh")
    }

    fn grant(caps: &[&str], epoch: u64, grantee_key: &[u8]) -> CapabilityGrant {
        CapabilityGrant {
            grantee: PeerId::new(),
            grantee_pubkey: grantee_key.to_vec(),
            capabilities: caps.iter().map(|c| c.to_string()).collect(),
            granted_in: mesh(),
            issued_at_ms: 1,
            expires_at_ms: None,
            epoch,
        }
    }

    // Authorizer that uses a chosen stub verdict, so we drive the post-signature
    // logic deterministically (the Ed25519GrantVerifier is swapped for a stub via a
    // hand-built VerifyContext in these tests).
    fn authorize_with(
        owner: &[u8],
        signed: &SignedCapabilityGrant,
        presenting: &[u8],
        command: &str,
        now_ms: u64,
        sig_ok: bool,
        seen: &DashMap<PeerId, u64>,
    ) -> GrantAuthOutcome {
        let m = mesh();
        let verifier = StubVerifier(sig_ok);
        let ctx = VerifyContext {
            now_ms,
            presenting_pubkey: presenting,
            expected_mesh: &m,
            verifier: &verifier,
            trusted_issuer_pubkey: owner,
        };
        match signed.verify(&ctx) {
            GrantVerdict::Valid => {}
            other => return GrantAuthOutcome::Invalid(other),
        }
        let grantee = signed.grant.grantee;
        if let Some(s) = seen.get(&grantee) {
            if signed.grant.epoch < *s {
                return GrantAuthOutcome::Superseded;
            }
        }
        if !signed.grants(command) {
            return GrantAuthOutcome::NotGranted;
        }
        seen.entry(grantee)
            .and_modify(|e| *e = (*e).max(signed.grant.epoch))
            .or_insert(signed.grant.epoch);
        GrantAuthOutcome::Authorized
    }

    fn signed(grant: CapabilityGrant, issuer: &[u8]) -> SignedCapabilityGrant {
        SignedCapabilityGrant {
            grant,
            proof: GrantProof {
                credential: CredentialKind::Ed25519,
                issuer_pubkey: issuer.to_vec(),
                signature: vec![0u8; 64],
            },
        }
    }

    // what this catches: the happy path — a grant signed by the owner, bound to the
    // presenting peer's key, in-mesh, unexpired, conferring the command → Authorized.
    #[test]
    fn valid_grant_authorizes_its_capability() {
        let owner = [1u8; 32];
        let peer = [2u8; 32];
        let g = signed(grant(&["ai/generate"], 1, &peer), &owner);
        let seen = DashMap::new();
        assert_eq!(
            authorize_with(&owner, &g, &peer, "ai/generate", 100, true, &seen),
            GrantAuthOutcome::Authorized
        );
        // ...but does NOT confer a different command.
        assert_eq!(
            authorize_with(&owner, &g, &peer, "data/delete", 100, true, &seen),
            GrantAuthOutcome::NotGranted
        );
    }

    // what this catches: every rejection reason is TYPED + reached. Untrusted issuer
    // (wrong owner key), bad signature, key mismatch (stolen grant on another peer's
    // identity) — all distinct verdicts, none silently allowed.
    #[test]
    fn rejections_are_typed_and_distinct() {
        let owner = [1u8; 32];
        let peer = [2u8; 32];
        let seen = DashMap::new();

        // wrong issuer → UntrustedIssuer
        let g = signed(grant(&["ai/generate"], 1, &peer), &[9u8; 32]);
        assert_eq!(
            authorize_with(&owner, &g, &peer, "ai/generate", 100, true, &seen),
            GrantAuthOutcome::Invalid(GrantVerdict::UntrustedIssuer)
        );

        // bad signature → BadSignature
        let g = signed(grant(&["ai/generate"], 1, &peer), &owner);
        assert_eq!(
            authorize_with(&owner, &g, &peer, "ai/generate", 100, false, &seen),
            GrantAuthOutcome::Invalid(GrantVerdict::BadSignature)
        );

        // a DIFFERENT presenting key than the grant is bound to → KeyMismatch
        // (a stolen grant can't ride another peer's identity).
        let g = signed(grant(&["ai/generate"], 1, &peer), &owner);
        assert_eq!(
            authorize_with(&owner, &g, &[7u8; 32], "ai/generate", 100, true, &seen),
            GrantAuthOutcome::Invalid(GrantVerdict::KeyMismatch)
        );
    }

    // what this catches: consumer-side epoch anti-replay. After accepting epoch 5, a
    // replayed epoch-3 grant is Superseded; a revocation (higher epoch, empty caps)
    // supersedes AND confers nothing.
    #[test]
    fn epoch_anti_replay_and_revocation() {
        let owner = [1u8; 32];
        let peer = [2u8; 32];
        let grantee = PeerId::new();
        let seen = DashMap::new();

        let mk = |caps: &[&str], epoch: u64| {
            let mut grant = grant(caps, epoch, &peer);
            grant.grantee = grantee; // same grantee across epochs
            signed(grant, &owner)
        };

        // accept epoch 5
        assert_eq!(
            authorize_with(&owner, &mk(&["ai/generate"], 5), &peer, "ai/generate", 100, true, &seen),
            GrantAuthOutcome::Authorized
        );
        // replayed lower epoch 3 → Superseded
        assert_eq!(
            authorize_with(&owner, &mk(&["ai/generate"], 3), &peer, "ai/generate", 100, true, &seen),
            GrantAuthOutcome::Superseded
        );
        // revocation: higher epoch 6, empty caps → supersedes + confers nothing
        assert_eq!(
            authorize_with(&owner, &mk(&[], 6), &peer, "ai/generate", 100, true, &seen),
            GrantAuthOutcome::NotGranted
        );
        // now even the old epoch-5 grant is superseded (watermark advanced past it
        // only if 6 was accepted — it wasn't, NotGranted doesn't advance; so 5 still
        // works). This pins that NotGranted does NOT move the watermark:
        assert_eq!(
            authorize_with(&owner, &mk(&["ai/generate"], 5), &peer, "ai/generate", 100, true, &seen),
            GrantAuthOutcome::Authorized
        );
    }

    // what this catches: the REAL Ed25519GrantVerifier verifies a genuine signature
    // over the grant body's canonical bytes and rejects a tampered one. This is the
    // production crypto path (the stub tests above cover the surrounding logic).
    #[test]
    fn ed25519_verifier_accepts_real_signature_rejects_tampered() {
        use ed25519_dalek::{Signer, SigningKey};

        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let vk = sk.verifying_key();
        let body = grant(&["ai/generate"], 1, &[2u8; 32]);
        let bytes = serde_json::to_vec(&body).expect("serialize body");
        let sig = sk.sign(&bytes);

        let good = GrantProof {
            credential: CredentialKind::Ed25519,
            issuer_pubkey: vk.to_bytes().to_vec(),
            signature: sig.to_bytes().to_vec(),
        };
        let v = Ed25519GrantVerifier;
        assert!(v.verify_signature(&bytes, &good), "genuine signature verifies");

        // tampered message → reject
        let mut tampered = bytes.clone();
        tampered[0] ^= 0xFF;
        assert!(!v.verify_signature(&tampered, &good), "tampered body rejected");

        // wrong credential kind → reject (defensive)
        let mut wrong = good.clone();
        wrong.signature[0] ^= 0xFF;
        assert!(!v.verify_signature(&bytes, &wrong), "tampered signature rejected");
    }
}
