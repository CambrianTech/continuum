//! Issue (mint) a capability grant — the owner side of the contracted grid.
//!
//! The symmetric counterpart of
//! [`build_grant_authorizer`](crate::persona::command_inbound_pump::build_grant_authorizer)
//! (which VERIFIES presented grants): [`issue_grant`] SIGNS a grant the owner hands
//! out. An owner selling compute calls this to mint a grant for a buyer conferring
//! exactly the capabilities sold (e.g. `["ai/generate"]`), delivers the returned
//! base64 blob to the buyer, and the buyer presents it
//! ([`PresentedGrantStore`](super::presented_grant_store::PresentedGrantStore)).
//!
//! The grant is bound to the AUTHENTICATED grantee: its key is read from the
//! owner's own enrolment (`airc.peer_public_key(grantee)`), so a grant minted here
//! verifies on the receiver against the same key the transport authenticates the
//! sender with. The mesh is the owner's own, and the signature is the owner's own
//! identity key (the trusted issuer) — all sourced from the one `airc` handle so
//! issuer, mesh, and grantee-key can't drift from what the verifier checks.

use airc_core::PeerId;
use airc_lib::grid_auth::CapabilityGrant;
use airc_lib::{Airc, AircError};
use base64::{engine::general_purpose::STANDARD, Engine};
use uuid::Uuid;

/// What to grant, to whom. The owner (the `airc` handle passed to [`issue_grant`])
/// is the implicit issuer; the mesh + grantee key are resolved from it.
#[derive(Debug, Clone)]
pub struct IssueGrantParams {
    /// The peer the grant is for. Must be enrolled in the owner's registry so the
    /// grant can be bound to its authenticated key.
    pub grantee: PeerId,
    /// Capability tags the grant confers — the SAME vocabulary the command ACL +
    /// the verifier's `confers()` match on (e.g. `"ai/generate"`, `"compute/run"`).
    pub capabilities: Vec<String>,
    /// When the grant expires (epoch-ms), or `None` for no expiry. A paid grant
    /// SHOULD set this — the lease is time-bounded.
    pub expires_at_ms: Option<u64>,
    /// Monotonic per grantee. Re-issue with a higher epoch to update; revoke by
    /// issuing a higher epoch with empty `capabilities`.
    pub epoch: u64,
}

/// Why minting a grant failed. Fail-closed: a partial/unsigned grant is never
/// returned.
#[derive(Debug, thiserror::Error)]
pub enum IssueGrantError {
    /// The grantee is not enrolled in the owner's registry — there is no
    /// authenticated key to bind the grant to. Trust the peer first.
    #[error("grantee {0} is not enrolled — cannot bind the grant to its authenticated key")]
    GranteeNotEnrolled(Uuid),
    /// Could not resolve the owner's local mesh identity (the grant's `granted_in`).
    #[error("resolve local mesh identity: {0}")]
    Mesh(#[source] AircError),
    /// The owner's key could not sign the grant body.
    #[error("sign grant: {0}")]
    Sign(#[source] serde_json::Error),
    /// The signed grant could not be serialized to its base64 wire blob.
    #[error("serialize signed grant: {0}")]
    Encode(#[source] serde_json::Error),
}

/// Mint a grant the owner (`airc`) signs for `params.grantee`, returning the base64
/// `SignedCapabilityGrant` blob the grantee presents on
/// `HEADER_AIRC_CAPABILITY_GRANT`. `issued_at_ms` is the caller's clock (kept out of
/// the function so issuance is deterministic + testable).
pub async fn issue_grant(
    airc: &Airc,
    issued_at_ms: u64,
    params: IssueGrantParams,
) -> Result<String, IssueGrantError> {
    let grantee_pubkey = airc
        .peer_public_key(params.grantee)
        .ok_or(IssueGrantError::GranteeNotEnrolled(params.grantee.0))?
        .to_vec();
    let mesh = airc.mesh_identity().await.map_err(IssueGrantError::Mesh)?;
    let grant = CapabilityGrant {
        grantee: params.grantee,
        grantee_pubkey,
        capabilities: params.capabilities,
        granted_in: mesh,
        issued_at_ms,
        expires_at_ms: params.expires_at_ms,
        epoch: params.epoch,
    };
    let signed = airc.sign_grant(grant).map_err(IssueGrantError::Sign)?;
    let bytes = serde_json::to_vec(&signed).map_err(IssueGrantError::Encode)?;
    Ok(STANDARD.encode(bytes))
}
