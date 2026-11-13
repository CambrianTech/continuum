//! Capability grants this node HOLDS to present on outbound cross-grid commands —
//! the grantee side of the contracted grid.
//!
//! The flow: an owner node issues a grant (`grid/grant/issue` → `Airc::sign_grant`),
//! delivers the base64 blob to the grantee, and the grantee stores it HERE keyed by
//! the owner peer it will present to. [`AircTransport`](super::AircTransport) then
//! stamps it on the `HEADER_AIRC_CAPABILITY_GRANT` of every request to that owner,
//! and the owner's [`CommandRequestHandler`](super::CommandRequestHandler) verifies
//! it. The receiving side is symmetric: the owner verifies, the grantee presents.
//!
//! Keyed by TARGET peer (not by issuer) because presentation is "what do I show
//! THIS peer" — a grantee may hold different grants for different owners. The store
//! holds the opaque base64 blob; it does not parse or verify (that's the receiver's
//! job). Latest-wins on insert so a re-issued (higher-epoch) grant supersedes.

use airc_core::PeerId;
use dashmap::DashMap;

/// The grants a node holds to present, looked up by the target peer. Sync — the
/// lookup is on the outbound dispatch hot path and must not await.
pub trait PresentedGrantStore: Send + Sync + std::fmt::Debug {
    /// The base64 `SignedCapabilityGrant` to present to `target`, if one is held.
    fn grant_for(&self, target: PeerId) -> Option<String>;
}

/// In-memory presented-grant store — a `DashMap<target, base64 grant>`. The default
/// holder; a persona populates it when it accepts a grant an owner issued it.
#[derive(Debug, Default)]
pub struct InMemoryPresentedGrantStore {
    by_target: DashMap<PeerId, String>,
}

impl InMemoryPresentedGrantStore {
    pub fn new() -> Self {
        Self {
            by_target: DashMap::new(),
        }
    }

    /// Hold `grant_b64` to present to `target`. Replaces any prior grant for that
    /// target — latest wins, so a re-issued / higher-epoch grant supersedes the old
    /// one (consistent with the receiver's latest-epoch-authoritative anti-replay).
    pub fn insert(&self, target: PeerId, grant_b64: String) {
        self.by_target.insert(target, grant_b64);
    }

    /// Stop presenting any grant to `target`. Returns the removed blob if present.
    pub fn remove(&self, target: PeerId) -> Option<String> {
        self.by_target.remove(&target).map(|(_, v)| v)
    }

    /// How many targets this node currently holds a grant for.
    pub fn len(&self) -> usize {
        self.by_target.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_target.is_empty()
    }
}

impl PresentedGrantStore for InMemoryPresentedGrantStore {
    fn grant_for(&self, target: PeerId) -> Option<String> {
        self.by_target.get(&target).map(|r| r.value().clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peer(n: u128) -> PeerId {
        PeerId::from_uuid(uuid::Uuid::from_u128(n))
    }

    // what this catches: the store returns the held grant for a target, None for an
    // unknown target, and latest-wins on re-insert (a re-issued grant supersedes).
    #[test]
    fn holds_presents_and_supersedes_per_target() {
        let store = InMemoryPresentedGrantStore::new();
        assert!(
            store.grant_for(peer(1)).is_none(),
            "unknown target → nothing to present"
        );

        store.insert(peer(1), "grant-v1".to_string());
        store.insert(peer(2), "other".to_string());
        assert_eq!(store.grant_for(peer(1)).as_deref(), Some("grant-v1"));
        assert_eq!(store.grant_for(peer(2)).as_deref(), Some("other"));

        // re-issue for the same target → latest wins
        store.insert(peer(1), "grant-v2".to_string());
        assert_eq!(store.grant_for(peer(1)).as_deref(), Some("grant-v2"));

        // remove → stop presenting
        assert_eq!(store.remove(peer(1)).as_deref(), Some("grant-v2"));
        assert!(store.grant_for(peer(1)).is_none());
        assert_eq!(store.len(), 1, "peer(2)'s grant remains");
    }
}
