#![cfg(test)]

use super::*;
use crate::routing::addressed_inbox::{AddressedFrame, AddressedPort};
use airc_core::Headers;
use airc_lib::grid_auth::{CapabilityGrant, CredentialKind, GrantProof};
use airc_lib::subscriptions::MeshIdentity;
use continuum_airc_protocol::{HEADER_GENERATE_ID, HEADER_GENERATE_VALID_UNTIL_MS};

#[tokio::test]
async fn generation_metadata_is_required_and_never_substituted() {
    let peer = PeerId::new();
    let mut rig = rig(&[(peer, TrustTier::OwnAccount)]);
    let id = Uuid::new_v4().to_string();
    let validity = (EpochMs::now().millis() + 60_000).to_string();
    for (generation, until, rule) in [
        (None, None, "missing_generation_stamp"),
        (Some(id.as_str()), None, "missing_generation_stamp"),
        (None, Some(validity.as_str()), "missing_generation_stamp"),
        (
            Some("not-a-uuid"),
            Some(validity.as_str()),
            "malformed_generation_stamp",
        ),
        (
            Some(id.as_str()),
            Some("not-an-epoch"),
            "malformed_generation_stamp",
        ),
        (Some(""), Some(""), "malformed_generation_stamp"),
    ] {
        let mut frame =
            FakeFrame::command(peer, rig.me, Some("ai/generate"), "ai/generate", json!({}));
        frame.headers.remove(HEADER_GENERATE_ID);
        frame.headers.remove(HEADER_GENERATE_VALID_UNTIL_MS);
        if let Some(value) = generation {
            frame
                .headers
                .insert(HEADER_GENERATE_ID.into(), value.into());
        }
        if let Some(value) = until {
            frame
                .headers
                .insert(HEADER_GENERATE_VALID_UNTIL_MS.into(), value.into());
        }
        deliver(&rig.handler, vec![frame]).await;
        let sent = rig.sent.try_recv().expect("typed refusal reply");
        assert!(
            refused_by(&sent, rule),
            "expected {rule}: {:?}",
            response(&sent)
        );
        assert_eq!(
            rig.runs.load(Ordering::SeqCst),
            0,
            "invalid metadata must not execute"
        );
        assert!(
            rig.sent.try_recv().is_err(),
            "no progress or additional replies"
        );
    }
}

struct KeyedPort {
    inner: Arc<FakePort>,
    peer: PeerId,
    key: Option<[u8; 32]>,
}

#[async_trait]
impl AddressedPort for KeyedPort {
    fn me(&self) -> PeerId {
        self.inner.me()
    }
    fn peer_public_key(&self, peer: PeerId) -> Option<[u8; 32]> {
        if peer == self.peer {
            self.key
        } else {
            None
        }
    }
    async fn reply(
        &self,
        request: &dyn AddressedFrame,
        headers: Headers,
        body: Body,
    ) -> Result<(), String> {
        self.inner.reply(request, headers, body).await
    }
    async fn stream(
        &self,
        request: &dyn AddressedFrame,
        headers: Headers,
        body: Body,
    ) -> Result<(), String> {
        self.inner.stream(request, headers, body).await
    }
}

#[tokio::test]
async fn own_account_cannot_ignore_a_presented_grant_failure() {
    let peer = PeerId::new();
    let owner = ed25519_dalek::SigningKey::from_bytes(&[17; 32])
        .verifying_key()
        .to_bytes();
    let key = ed25519_dalek::SigningKey::from_bytes(&[23; 32])
        .verifying_key()
        .to_bytes();
    let mesh = MeshIdentity::new("ingress-required-grant");
    let signed = SignedCapabilityGrant {
        grant: CapabilityGrant {
            grantee: peer,
            grantee_pubkey: key.to_vec(),
            capabilities: vec!["test/echo".into()],
            granted_in: mesh.clone(),
            issued_at_ms: 1,
            expires_at_ms: None,
            epoch: 1,
        },
        proof: GrantProof {
            credential: CredentialKind::Ed25519,
            issuer_pubkey: owner.to_vec(),
            signature: vec![0; 64],
        },
    };
    for (configured, presenting, rule) in [
        (false, Some(key), "grant_authorizer_unavailable"),
        (true, None, "presenting_key_unavailable"),
        (true, Some(key), "grant_rejected"),
    ] {
        let me = PeerId::new();
        let (port, mut sent) = FakePort::new(me);
        let runs = Arc::new(AtomicUsize::new(0));
        let registry = Arc::new(crate::runtime::ModuleRegistry::new());
        registry.register(Arc::new(Echo {
            runs: Arc::clone(&runs),
        }));
        let executor = Arc::new(CommandExecutor::new(registry).with_policy(allow_all()));
        let authorizer =
            configured.then(|| Arc::new(GrantAuthorizer::new(owner.to_vec(), mesh.clone())));
        let handler = CommandRequestHandler::with_seams(
            Arc::new(KeyedPort {
                inner: port,
                peer,
                key: presenting,
            }),
            Arc::new(Tiers(HashMap::from([(peer, TrustTier::OwnAccount)]))),
            executor,
            authorizer,
        );
        let mut frame = FakeFrame::command(peer, me, Some("test/echo"), "test/echo", json!({}));
        frame.headers.insert(
            HEADER_AIRC_CAPABILITY_GRANT.into(),
            STANDARD.encode(serde_json::to_vec(&signed).expect("grant JSON")),
        );
        deliver(&handler, vec![frame]).await;
        let refusal = sent.try_recv().expect("typed grant refusal");
        assert!(
            refused_by(&refusal, rule),
            "expected {rule}: {:?}",
            response(&refusal)
        );
        assert_eq!(
            runs.load(Ordering::SeqCst),
            0,
            "tier cannot rescue rejected grant"
        );
        // The same caller without a grant is legitimately authorized by its tier.
        deliver(
            &handler,
            vec![FakeFrame::command(
                peer,
                me,
                Some("test/echo"),
                "test/echo",
                json!({}),
            )],
        )
        .await;
        assert!(matches!(
            response(&sent.try_recv().expect("tier reply")),
            AircCommandResponse::Ok { .. }
        ));
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }
}
