//! Resource leases — the byte-residency grant primitive.
//!
//! A [`ResourceLease`] is the handle the [resource authority](super) hands a
//! consumer: "you hold N bytes of this `kind` until `expires_at_ms`; renew to
//! keep it, or I will ask for it back." It is the **byte-residency** sibling of
//! this module's [`ThroughputLease`](super::ThroughputLease): the throughput
//! lease admits a *transient job* to a concurrency lane (cost-units, slots);
//! this lease tracks *durable bytes* a subsystem holds (a loaded model sits in
//! VRAM for hours). Same Graceful/Hard/Pinned revocation vocabulary; different
//! axis — slots-to-run vs bytes-resident.
//!
//! These types are pure values — no I/O, no async, no locks. The ledger that
//! tracks them lives in [`super::ledger`]; the interface a consumer implements
//! to honor a reclaim lives in [`super::consumer`].

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// The three physical resource axes one machine (or one container) hands out.
/// Ports/handles can join later; these are the memory/disk axes the authority
/// must account for first (the ones that OOM or ENOSPC a node).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/resources/ResourceKind.ts"
)]
pub enum ResourceKind {
    /// GPU memory. On UMA (Apple Silicon) this overlaps `Ram` physically; the
    /// authority's scan layer is responsible for not double-counting.
    Vram,
    /// System RAM (resident set the consumer holds).
    Ram,
    /// On-disk footprint (model weights, caches, spill).
    Disk,
}

impl ResourceKind {
    pub const ALL: [ResourceKind; 3] = [ResourceKind::Vram, ResourceKind::Ram, ResourceKind::Disk];

    pub fn label(self) -> &'static str {
        match self {
            ResourceKind::Vram => "vram",
            ResourceKind::Ram => "ram",
            ResourceKind::Disk => "disk",
        }
    }
}

/// How willing a lease is to be reclaimed under pressure. Mirrors this module's
/// [`ThroughputLeaseRevocationPolicy`](super::ThroughputLeaseRevocationPolicy)
/// so the two axes speak the same revocation vocabulary. Convergence onto one
/// shared enum is deferred (same pattern this module already follows for
/// `cognition::*` re-exports) — noted, not forced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/resources/ReclaimPolicy.ts"
)]
pub enum ReclaimPolicy {
    /// The authority asks first (async callback), gives a deadline, and waits
    /// for the consumer to confirm. The default — patient RTOS, not preempt.
    Graceful,
    /// The authority may revoke immediately; the consumer tolerates a yank
    /// (e.g. a re-derivable cache). Still honored via the callback, but with a
    /// zero-grace deadline.
    Hard,
    /// Do not reclaim while active. The render loop's realtime targets and a
    /// persona's in-flight inference lease pin this. Only released on expiry or
    /// explicit `release`.
    Pinned,
}

/// A live grant: subsystem `consumer_id` holds `bytes` of `kind` until
/// `expires_at_ms`. Pure value; the ledger owns the collection of these.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/resources/ResourceLease.ts"
)]
pub struct ResourceLease {
    /// Caller-minted unique id. The ledger stays pure (no randomness inside);
    /// the daemon mints the id and passes it in.
    pub lease_id: String,
    /// Which subsystem holds it ("serving", "bevy", "livekit").
    pub consumer_id: String,
    pub kind: ResourceKind,
    #[ts(type = "number")]
    pub bytes: u64,
    #[ts(type = "number")]
    pub acquired_at_ms: u64,
    #[ts(type = "number")]
    pub expires_at_ms: u64,
    pub reclaim_policy: ReclaimPolicy,
}

impl ResourceLease {
    pub fn is_expired(&self, now_ms: u64) -> bool {
        now_ms >= self.expires_at_ms
    }

    /// Can pressure take this back right now? Expired leases always yes (the
    /// bytes are overdue); active `Pinned` leases never; everything else yes.
    pub fn is_reclaimable(&self, now_ms: u64) -> bool {
        self.is_expired(now_ms) || self.reclaim_policy != ReclaimPolicy::Pinned
    }

    /// Reclaim ordering rank — lower is reclaimed first. Mirrors the
    /// `disruption_rank` ladder in `paging::lease_revocation`: expired bytes
    /// are the safest to take (rank 0), then `Hard` (rank 1), then `Graceful`
    /// (rank 2). Active `Pinned` returns `None` — never eligible.
    pub fn reclaim_rank(&self, now_ms: u64) -> Option<u8> {
        if self.is_expired(now_ms) {
            return Some(0);
        }
        match self.reclaim_policy {
            ReclaimPolicy::Hard => Some(1),
            ReclaimPolicy::Graceful => Some(2),
            ReclaimPolicy::Pinned => None,
        }
    }
}

/// A consumer's ask: "grant me `bytes` of `kind` for `ttl_ms`, reclaimable
/// under this policy." The authority decides yes/no against scanned capacity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/resources/LeaseRequest.ts"
)]
pub struct LeaseRequest {
    pub consumer_id: String,
    pub kind: ResourceKind,
    #[ts(type = "number")]
    pub bytes: u64,
    #[ts(type = "number")]
    pub ttl_ms: u64,
    pub reclaim_policy: ReclaimPolicy,
}

/// Why a lease operation failed. Fail-loud — the authority never silently
/// over-grants or silently no-ops a missing lease.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", tag = "error")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/resources/LeaseError.ts"
)]
pub enum LeaseError {
    /// Not enough free capacity of `kind` to satisfy the request. The authority
    /// returns this rather than over-committing (the bug task #56 fixes:
    /// serving claiming total VRAM blind to bevy/livekit → OOM).
    InsufficientCapacity {
        kind: ResourceKind,
        #[ts(type = "number")]
        requested: u64,
        #[ts(type = "number")]
        available: u64,
    },
    DuplicateLease {
        lease_id: String,
    },
    MissingLease {
        lease_id: String,
    },
    ExpiredLease {
        lease_id: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lease(policy: ReclaimPolicy, expires_at_ms: u64) -> ResourceLease {
        ResourceLease {
            lease_id: "l1".into(),
            consumer_id: "serving".into(),
            kind: ResourceKind::Vram,
            bytes: 1024,
            acquired_at_ms: 100,
            expires_at_ms,
            reclaim_policy: policy,
        }
    }

    // what this catches: the reclaim ladder is the load-bearing ordering the
    // daemon uses to pick victims. If expired stops out-ranking Hard/Graceful,
    // or active Pinned stops being ineligible (None), pressure would take the
    // wrong bytes first — the render loop's pinned targets before overdue ones.
    #[test]
    fn reclaim_rank_orders_expired_then_hard_then_graceful_pinned_never() {
        // expired beats everything regardless of its policy
        assert_eq!(lease(ReclaimPolicy::Pinned, 150).reclaim_rank(200), Some(0));
        assert_eq!(lease(ReclaimPolicy::Hard, 9_999).reclaim_rank(200), Some(1));
        assert_eq!(
            lease(ReclaimPolicy::Graceful, 9_999).reclaim_rank(200),
            Some(2)
        );
        // active pinned is never eligible
        assert_eq!(lease(ReclaimPolicy::Pinned, 9_999).reclaim_rank(200), None);
    }

    // what this catches: is_reclaimable is the gate select_to_reclaim filters
    // on. An active Pinned lease must be off-limits; an expired one (even
    // Pinned) must be fair game (its bytes are overdue).
    #[test]
    fn pinned_is_reclaimable_only_once_expired() {
        assert!(!lease(ReclaimPolicy::Pinned, 9_999).is_reclaimable(200));
        assert!(lease(ReclaimPolicy::Pinned, 150).is_reclaimable(200));
        assert!(lease(ReclaimPolicy::Graceful, 9_999).is_reclaimable(200));
    }

    // what this catches: wire stability — ResourceKind/ReclaimPolicy serialize
    // kebab-case for the grid command surface + TS bindings. Every remote
    // caller parses these strings; a rename here breaks the wire silently.
    #[test]
    fn kind_and_policy_serialize_kebab_case() {
        assert_eq!(
            serde_json::to_string(&ResourceKind::Vram).unwrap(),
            "\"vram\""
        );
        assert_eq!(
            serde_json::to_string(&ResourceKind::Disk).unwrap(),
            "\"disk\""
        );
        assert_eq!(
            serde_json::to_string(&ReclaimPolicy::Graceful).unwrap(),
            "\"graceful\""
        );
    }

    // what this catches: LeaseError is a tagged union on `kind` so the TS side
    // can discriminate. InsufficientCapacity must carry the numbers a caller
    // needs to back off (requested vs available), not just a string.
    #[test]
    fn lease_error_round_trips_tagged() {
        let e = LeaseError::InsufficientCapacity {
            kind: ResourceKind::Vram,
            requested: 8192,
            available: 4096,
        };
        let j = serde_json::to_string(&e).unwrap();
        assert!(
            j.contains("\"error\":\"insufficientCapacity\""),
            "tag missing: {j}"
        );
        // the resource axis still rides along under its own `kind` field
        assert!(
            j.contains("\"kind\":\"vram\""),
            "resource kind missing: {j}"
        );
        let back: LeaseError = serde_json::from_str(&j).unwrap();
        assert_eq!(back, e);
    }
}
