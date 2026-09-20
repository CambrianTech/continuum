//! Grid capability auth — continuum-side verification of airc signed grants.
//!
//! The verification engine for [docs/grid/GRID-CAPABILITY-AUTH.md]: a remote peer
//! presents a `SignedCapabilityGrant` the owner signed; this authorizes a command
//! iff the grant VERIFIES (issuer-pin → sig → key-binding → mesh → expiry, via
//! airc's stateless `grid_auth`) AND the grant confers the command AND the grant
//! isn't a replay of a superseded epoch. Identity + authorization + contract are
//! the one signed object — no shared trust store, no address↔peer mismatch.
//!
//! This is the verification CORE (the heart of the contracted-grid gate). It is a
//! tested SEAM, NOT yet wired to the live dispatch path because the airc command
//! envelope doesn't carry grants yet (the airc-side transport slice). See
//! `[[airc-grid-identity-unification-trust-bridge]]`.
//!
//! ## HARD GATES before wiring to live dispatch (adversarial review 2026-06-21)
//! - **Persist the epoch watermark.** `seen_epoch` is in-memory; a node restart
//!   would reopen the entire replay window (the grid expects mundane restarts).
//!   The watermark MUST be durable (per-grantee) before this gates real traffic.
//! - **Bound the watermark store.** It's owner-bounded today (only owner-signed
//!   grantees insert), but the for-sale grid implies many transient grantees —
//!   add expiry-aligned eviction (an entry is dead once all its grants would be
//!   `Expired`).
//! - **Presenting key from the AUTHENTICATED sender only.** When
//!   `CommandRequestHandler` calls [`GrantAuthorizer::authorize_command`], the
//!   `presenting_pubkey` MUST be the transport-verified sender key, NEVER the
//!   grant's own `grantee_pubkey` (which would be self-certifying). Consider a
//!   newtype so the two keys can't be transposed.

use std::sync::Arc;

use airc_core::PeerId;
use airc_lib::grid_auth::VerifyContext;
use airc_lib::grid_auth::{
    CredentialKind, GrantProof, GrantVerdict, GrantVerifier, SignedCapabilityGrant,
};
use airc_lib::subscriptions::MeshIdentity;

use super::epoch_watermark::{EpochWatermarkStore, InMemoryEpochWatermark, WatermarkDecision};

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
    /// The grant verified (and is current) but does NOT confer this command.
    NotGranted,
    /// The anti-replay watermark store could not be consulted (e.g. the durable
    /// store errored). Fail-CLOSED: the caller MUST deny — never authorize a grant
    /// whose replay status is unknown. The string is for audit, not control flow.
    WatermarkUnavailable(String),
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
        // verify_strict rejects weak/malleable/torsion signatures (the anti-
        // malleability hardening a capability token wants; same as the envelope path).
        vk.verify_strict(message, &ed25519_dalek::Signature::from_bytes(&sig_bytes))
            .is_ok()
    }
}

/// Does any capability in `capabilities` confer `command`? BOUNDARY-AWARE prefix
/// match (consistent with the command-ACL's prefix rules): a capability matches the
/// command exactly OR is a path-prefix of it on a `/` boundary. So `"ai/generate"`
/// confers `ai/generate` and `ai/generate/stream`, but NOT `ai/generatex` (no bare
/// `starts_with`, which would be the classic prefix-without-boundary over-grant).
///
/// `pub(crate)` so the gate's grant fast-path
/// ([`GridTrustAuthPolicy::gate`](crate::routing::GridTrustAuthPolicy)) re-checks a
/// caller's verified `granted_capabilities` through the SAME matching rule — one
/// source of truth for capability→command conferral, never a divergent copy.
pub(crate) fn confers(capabilities: &[String], command: &str) -> bool {
    capabilities
        .iter()
        .any(|c| command == c || command.starts_with(&format!("{c}/")))
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
    /// Signature verifier — injectable so tests drive the REAL `authorize_command`
    /// with a stub (no duplicated decision logic). Production = `Ed25519GrantVerifier`.
    verifier: Arc<dyn GrantVerifier + Send + Sync>,
    /// Consumer-side anti-replay state: the highest grant epoch accepted per grantee.
    /// The `grid_auth` verifier is stateless by design; rejecting a superseded
    /// lower-epoch grant is the CONSUMER's responsibility — this is that state,
    /// behind the [`EpochWatermarkStore`] seam so production can be DURABLE (survive
    /// restart) + BOUNDED (the security review's hard gate), while tests use the
    /// in-memory impl. Latest epoch is authoritative (airc's model): a revocation is
    /// a higher-epoch grant with empty capabilities.
    watermark: Arc<dyn EpochWatermarkStore>,
}

impl GrantAuthorizer {
    /// Production constructor: real ed25519 verifier + IN-MEMORY watermark. The
    /// in-memory watermark is NOT durable — callers gating LIVE grant traffic must
    /// use [`with_watermark`](Self::with_watermark) with a
    /// [`SqliteEpochWatermark`](super::epoch_watermark::SqliteEpochWatermark) so a
    /// restart can't reopen the replay window (the review hard gate).
    pub fn new(owner_pubkey: Vec<u8>, mesh: MeshIdentity) -> Self {
        Self::with_verifier(owner_pubkey, mesh, Arc::new(Ed25519GrantVerifier))
    }

    /// Production constructor with an explicit (durable) watermark store — the
    /// live-traffic path. Pairs the real ed25519 verifier with the caller's store.
    pub fn with_watermark(
        owner_pubkey: Vec<u8>,
        mesh: MeshIdentity,
        watermark: Arc<dyn EpochWatermarkStore>,
    ) -> Self {
        Self {
            owner_pubkey,
            mesh,
            verifier: Arc::new(Ed25519GrantVerifier),
            watermark,
        }
    }

    /// Construct with an explicit verifier (production passes `Ed25519GrantVerifier`;
    /// tests pass a stub to exercise the decision logic without real signatures).
    /// Watermark defaults to in-memory; use [`with_verifier_and_watermark`](Self::with_verifier_and_watermark)
    /// to inject a durable store too.
    pub fn with_verifier(
        owner_pubkey: Vec<u8>,
        mesh: MeshIdentity,
        verifier: Arc<dyn GrantVerifier + Send + Sync>,
    ) -> Self {
        Self::with_verifier_and_watermark(
            owner_pubkey,
            mesh,
            verifier,
            Arc::new(InMemoryEpochWatermark::new()),
        )
    }

    /// Full constructor — explicit verifier AND watermark store. The other
    /// constructors are conveniences over this.
    pub fn with_verifier_and_watermark(
        owner_pubkey: Vec<u8>,
        mesh: MeshIdentity,
        verifier: Arc<dyn GrantVerifier + Send + Sync>,
        watermark: Arc<dyn EpochWatermarkStore>,
    ) -> Self {
        Self {
            owner_pubkey,
            mesh,
            verifier,
            watermark,
        }
    }

    /// Authorize `command` from a peer presenting `signed` with verified key
    /// `presenting_pubkey`, at wall-clock `now_ms`: verify → atomic epoch
    /// anti-replay (durable) → capability check.
    ///
    /// Async because the durable watermark check runs off the executor
    /// (`spawn_blocking`). The verify + confers steps are pure; only the watermark
    /// consult is I/O. Fail-CLOSED: a watermark error yields
    /// [`WatermarkUnavailable`](GrantAuthOutcome::WatermarkUnavailable), never an
    /// authorization.
    pub async fn authorize_command(
        &self,
        signed: &SignedCapabilityGrant,
        presenting_pubkey: &[u8],
        command: &str,
        now_ms: u64,
    ) -> GrantAuthOutcome {
        // Scope the VerifyContext so its `&dyn GrantVerifier` borrow is DROPPED
        // before the `.await` below — otherwise the future would hold a non-Sync
        // reference across the await point and stop being `Send` (it must be Send to
        // run on the multi-threaded handler runtime). verify() is pure + sync, so
        // nothing is lost by completing it first.
        let verdict = {
            let ctx = VerifyContext {
                now_ms,
                presenting_pubkey,
                expected_mesh: &self.mesh,
                verifier: &*self.verifier,
                trusted_issuer_pubkey: &self.owner_pubkey,
            };
            signed.verify(&ctx)
        };
        match verdict {
            GrantVerdict::Valid => {}
            other => return GrantAuthOutcome::Invalid(other),
        }

        // Anti-replay against the (durable) watermark. The store's check-and-advance
        // is ONE atomic critical section, so a superseded epoch can never pass its
        // check while a higher epoch commits in the gap. Latest epoch is
        // authoritative, so ANY accepted grant advances the watermark — which is
        // what makes a revocation (higher-epoch empty-caps grant) supersede older
        // real-caps grants. A store error fails CLOSED (deny), never open.
        match self
            .watermark
            .check_and_advance(signed.grant.grantee, signed.grant.epoch, now_ms)
            .await
        {
            Ok(WatermarkDecision::Accepted) => {}
            Ok(WatermarkDecision::Superseded) => return GrantAuthOutcome::Superseded,
            Err(e) => return GrantAuthOutcome::WatermarkUnavailable(e.to_string()),
        }

        // The grant is current; does it confer THIS command? (Owner-gated commands
        // are simply never in any remote grant's capability list.)
        if confers(&signed.grant.capabilities, command) {
            GrantAuthOutcome::Authorized
        } else {
            GrantAuthOutcome::NotGranted
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_lib::grid_auth::CapabilityGrant;

    // A stub verifier so the grant LOGIC (issuer-pin, key-binding, mesh, expiry,
    // epoch, capability) is driven through the REAL `authorize_command` without real
    // signatures. The genuine Ed25519 path is exercised separately below.
    struct StubVerifier(bool);
    impl GrantVerifier for StubVerifier {
        fn verify_signature(&self, _message: &[u8], _proof: &GrantProof) -> bool {
            self.0
        }
    }

    fn mesh() -> MeshIdentity {
        MeshIdentity::new("test-mesh")
    }

    /// An authorizer whose signature verifier is the stub (so we exercise the real
    /// decision path: verify → atomic epoch → capability).
    fn authorizer(owner: &[u8], sig_ok: bool) -> GrantAuthorizer {
        GrantAuthorizer::with_verifier(owner.to_vec(), mesh(), Arc::new(StubVerifier(sig_ok)))
    }

    fn grant(caps: &[&str], epoch: u64, grantee_key: &[u8], grantee: PeerId) -> CapabilityGrant {
        CapabilityGrant {
            grantee,
            grantee_pubkey: grantee_key.to_vec(),
            capabilities: caps.iter().map(|c| c.to_string()).collect(),
            granted_in: mesh(),
            issued_at_ms: 1,
            expires_at_ms: None,
            epoch,
        }
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

    // what this catches: the happy path through the REAL authorize_command — a grant
    // signed by the owner, bound to the presenting key, in-mesh, unexpired,
    // conferring the command → Authorized; a different command → NotGranted.
    #[tokio::test]
    async fn valid_grant_authorizes_its_capability() {
        let owner = [1u8; 32];
        let peer = [2u8; 32];
        let g = signed(grant(&["ai/generate"], 1, &peer, PeerId::new()), &owner);
        let a = authorizer(&owner, true);
        assert_eq!(
            a.authorize_command(&g, &peer, "ai/generate", 100).await,
            GrantAuthOutcome::Authorized
        );
        assert_eq!(
            a.authorize_command(&g, &peer, "data/delete", 100).await,
            GrantAuthOutcome::NotGranted
        );
    }

    // what this catches: BOUNDARY-AWARE capability match — `ai/generate` confers the
    // sub-command `ai/generate/stream` (matches the ACL's prefix semantics) but NOT
    // `ai/generatex` (no bare starts_with over-grant).
    #[tokio::test]
    async fn capability_match_is_boundary_aware() {
        let owner = [1u8; 32];
        let peer = [2u8; 32];
        let g = signed(grant(&["ai/generate"], 1, &peer, PeerId::new()), &owner);
        let a = authorizer(&owner, true);
        assert_eq!(
            a.authorize_command(&g, &peer, "ai/generate/stream", 100)
                .await,
            GrantAuthOutcome::Authorized,
            "a capability confers its sub-commands on a / boundary"
        );
        assert_eq!(
            a.authorize_command(&g, &peer, "ai/generatex", 100).await,
            GrantAuthOutcome::NotGranted,
            "but NOT a different command sharing the prefix without a boundary"
        );
    }

    // what this catches: every rejection reason is TYPED + reached through the real
    // method. Untrusted issuer, bad signature, key mismatch (stolen grant on another
    // peer's identity) — distinct verdicts, none silently allowed.
    #[tokio::test]
    async fn rejections_are_typed_and_distinct() {
        let owner = [1u8; 32];
        let peer = [2u8; 32];

        // wrong issuer → UntrustedIssuer
        let g = signed(grant(&["ai/generate"], 1, &peer, PeerId::new()), &[9u8; 32]);
        assert_eq!(
            authorizer(&owner, true)
                .authorize_command(&g, &peer, "ai/generate", 100)
                .await,
            GrantAuthOutcome::Invalid(GrantVerdict::UntrustedIssuer)
        );

        // bad signature → BadSignature (stub returns false)
        let g = signed(grant(&["ai/generate"], 1, &peer, PeerId::new()), &owner);
        assert_eq!(
            authorizer(&owner, false)
                .authorize_command(&g, &peer, "ai/generate", 100)
                .await,
            GrantAuthOutcome::Invalid(GrantVerdict::BadSignature)
        );

        // a DIFFERENT presenting key than the grant is bound to → KeyMismatch.
        let g = signed(grant(&["ai/generate"], 1, &peer, PeerId::new()), &owner);
        assert_eq!(
            authorizer(&owner, true)
                .authorize_command(&g, &[7u8; 32], "ai/generate", 100)
                .await,
            GrantAuthOutcome::Invalid(GrantVerdict::KeyMismatch)
        );
    }

    // what this catches: epoch anti-replay AND that revocation ACTUALLY revokes.
    // Latest epoch is authoritative, so any valid grant advances the watermark —
    // a revocation (higher epoch, empty caps) supersedes the old real-caps grant.
    #[tokio::test]
    async fn epoch_anti_replay_and_revocation_supersedes() {
        let owner = [1u8; 32];
        let peer = [2u8; 32];
        let grantee = PeerId::new();
        let a = authorizer(&owner, true);
        let mk = |caps: &[&str], epoch: u64| signed(grant(caps, epoch, &peer, grantee), &owner);

        // accept epoch 5
        assert_eq!(
            a.authorize_command(&mk(&["ai/generate"], 5), &peer, "ai/generate", 100)
                .await,
            GrantAuthOutcome::Authorized
        );
        // replayed lower epoch 3 → Superseded
        assert_eq!(
            a.authorize_command(&mk(&["ai/generate"], 3), &peer, "ai/generate", 100)
                .await,
            GrantAuthOutcome::Superseded
        );
        // revocation: higher epoch 6, empty caps → advances watermark, confers nothing
        assert_eq!(
            a.authorize_command(&mk(&[], 6), &peer, "ai/generate", 100)
                .await,
            GrantAuthOutcome::NotGranted
        );
        // the revoked epoch-5 grant is now SUPERSEDED — revocation actually revoked.
        assert_eq!(
            a.authorize_command(&mk(&["ai/generate"], 5), &peer, "ai/generate", 100)
                .await,
            GrantAuthOutcome::Superseded,
            "after a higher-epoch revocation, the old grant no longer authorizes"
        );
    }

    // what this catches: the REAL Ed25519GrantVerifier verifies a genuine signature
    // over the grant body's canonical bytes, and rejects tampered bytes, a tampered
    // signature, AND malformed (wrong-length / wrong-credential) proofs — the
    // attacker-controlled-bytes branches, none of which may panic.
    #[test]
    fn ed25519_verifier_accepts_real_and_rejects_malformed() {
        use ed25519_dalek::{Signer, SigningKey};

        let sk = SigningKey::from_bytes(&[7u8; 32]);
        let vk = sk.verifying_key();
        let body = grant(&["ai/generate"], 1, &[2u8; 32], PeerId::new());
        let bytes = serde_json::to_vec(&body).expect("serialize body");
        let sig = sk.sign(&bytes);
        let v = Ed25519GrantVerifier;

        let good = GrantProof {
            credential: CredentialKind::Ed25519,
            issuer_pubkey: vk.to_bytes().to_vec(),
            signature: sig.to_bytes().to_vec(),
        };
        assert!(
            v.verify_signature(&bytes, &good),
            "genuine signature verifies"
        );

        // tampered message → reject
        let mut tampered = bytes.clone();
        tampered[0] ^= 0xFF;
        assert!(
            !v.verify_signature(&tampered, &good),
            "tampered body rejected"
        );

        // tampered signature → reject
        let mut bad_sig = good.clone();
        bad_sig.signature[0] ^= 0xFF;
        assert!(
            !v.verify_signature(&bytes, &bad_sig),
            "tampered signature rejected"
        );

        // wrong-length key → reject, no panic
        let short_key = GrantProof {
            credential: CredentialKind::Ed25519,
            issuer_pubkey: vec![0u8; 31],
            signature: good.signature.clone(),
        };
        assert!(
            !v.verify_signature(&bytes, &short_key),
            "wrong-length key rejected"
        );

        // wrong-length signature → reject, no panic
        let short_sig = GrantProof {
            credential: CredentialKind::Ed25519,
            issuer_pubkey: good.issuer_pubkey.clone(),
            signature: vec![0u8; 10],
        };
        assert!(
            !v.verify_signature(&bytes, &short_sig),
            "wrong-length signature rejected"
        );
    }

    /// Concurrency proof for the atomic epoch watermark (the TOCTOU fix). Gated
    /// behind `stress-tests` per the test doctrine.
    #[cfg(feature = "stress-tests")]
    mod stress {
        use super::*;
        use crate::routing::epoch_watermark::SqliteEpochWatermark;
        use std::sync::Arc as StdArc;

        // what this catches: under heavy concurrent presentation of MANY epochs for
        // the SAME grantee — through the REAL durable (SQLite) watermark — the
        // serialized transaction keeps the decision consistent (every outcome a clean
        // Authorized/Superseded, never corrupt), and afterwards a replay below the max
        // is Superseded while the max-epoch grant still authorizes. A regression to a
        // non-atomic check-and-advance would let a superseded epoch slip an Authorized
        // through. (The store's own monotonicity is proven in epoch_watermark stress.)
        #[tokio::test(flavor = "multi_thread", worker_threads = 8)]
        async fn concurrent_same_grantee_epochs_stay_monotonic() {
            const N: u64 = 200;
            let owner = [1u8; 32];
            let peer = [2u8; 32];
            let grantee = PeerId::new();
            let a = StdArc::new(GrantAuthorizer::with_verifier_and_watermark(
                owner.to_vec(),
                mesh(),
                Arc::new(StubVerifier(true)),
                Arc::new(SqliteEpochWatermark::in_memory().expect("open")),
            ));

            let mut handles = Vec::new();
            for epoch in 1..=N {
                let a = a.clone();
                handles.push(tokio::spawn(async move {
                    let g = signed(grant(&["ai/generate"], epoch, &peer, grantee), &owner);
                    let out = a.authorize_command(&g, &peer, "ai/generate", 100).await;
                    // Every outcome is a clean verdict — never corrupt.
                    assert!(matches!(
                        out,
                        GrantAuthOutcome::Authorized | GrantAuthOutcome::Superseded
                    ));
                }));
            }
            for h in handles {
                h.await.expect("task");
            }
            // A replay below the settled max is superseded…
            let below = signed(grant(&["ai/generate"], N - 1, &peer, grantee), &owner);
            assert_eq!(
                a.authorize_command(&below, &peer, "ai/generate", 100).await,
                GrantAuthOutcome::Superseded
            );
            // …and the max-epoch grant still authorizes (never below the watermark).
            let g = signed(grant(&["ai/generate"], N, &peer, grantee), &owner);
            assert_eq!(
                a.authorize_command(&g, &peer, "ai/generate", 100).await,
                GrantAuthOutcome::Authorized
            );
        }
    }
}
