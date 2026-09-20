//! The consumer interface — what a leaseholder implements to honor a reclaim.
//!
//! This is the seam the directive describes: *"you are given leases as a
//! resourcemanager interface implementer. We will ask for it back... We are
//! taking this back soon (from handle, interface callback), perform your
//! cleanup, then tell us it's gone."* The authority does not yank bytes — it
//! **asks**, patiently, via [`ResourceConsumer::reclaim`], and waits for the
//! holder to confirm what it actually freed. Mobile's `didReceiveMemoryWarning`,
//! not a SIGKILL.
//!
//! Serving, Bevy, and LiveKit each implement this. None of them is "the AI
//! authority" — they are peer consumers (render, voice, inference) that lease
//! the same physical bytes.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::lease::ResourceKind;

/// What a consumer reports it is holding right now — its self-declared
/// footprint, reconciled by the daemon against the hardware scan. `detail` is
/// human/grid-facing ("qwen3-coder-30b weights", "render target pool").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/resources/ConsumerFootprint.ts"
)]
pub struct ConsumerFootprint {
    pub kind: ResourceKind,
    #[ts(type = "number")]
    pub bytes: u64,
    pub detail: String,
}

/// Why the authority is asking for bytes back. The consumer can reason about
/// urgency: `Shutdown` means release everything; `Pressure` means another peer
/// needs room now; `Rebalance` is housekeeping it may partially defer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/resources/ReclaimReason.ts"
)]
pub enum ReclaimReason {
    Pressure,
    Rebalance,
    Shutdown,
}

/// The ask: free at least `target_bytes` of `kind` by `deadline_ms`. Patient —
/// `deadline_ms` is a real grace window the consumer may use to finish an
/// in-flight frame / inference before releasing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/resources/ReclaimRequest.ts"
)]
pub struct ReclaimRequest {
    pub kind: ResourceKind,
    #[ts(type = "number")]
    pub target_bytes: u64,
    #[ts(type = "number")]
    pub deadline_ms: u64,
    pub reason: ReclaimReason,
}

/// How the reclaim resolved. `Deferred` is honest backpressure — "I heard you,
/// cleanup is in flight, ask again": the authority keeps the lease alive and
/// re-asks, it does not assume freed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/resources/ReclaimStatus.ts"
)]
pub enum ReclaimStatus {
    Released,
    Partial,
    Deferred,
    Refused,
}

/// The consumer's confirmed answer: how many bytes it actually freed, and the
/// disposition. The authority trusts `freed_bytes` only after the matching
/// lease `release` lands — this is the report, the ledger mutation is separate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/resources/ReclaimOutcome.ts"
)]
pub struct ReclaimOutcome {
    #[ts(type = "number")]
    pub freed_bytes: u64,
    pub status: ReclaimStatus,
    #[ts(optional)]
    pub detail: Option<String>,
}

impl ReclaimOutcome {
    pub fn released(freed_bytes: u64) -> Self {
        Self {
            freed_bytes,
            status: ReclaimStatus::Released,
            detail: None,
        }
    }

    pub fn refused(detail: impl Into<String>) -> Self {
        Self {
            freed_bytes: 0,
            status: ReclaimStatus::Refused,
            detail: Some(detail.into()),
        }
    }
}

/// Implemented by every subsystem that leases physical bytes. Object-safe so the
/// authority holds `Arc<dyn ResourceConsumer>` for serving, Bevy, and LiveKit
/// uniformly. `reclaim` is async + patient: a long cleanup returns `Deferred`
/// rather than blocking the authority's tick.
#[async_trait]
pub trait ResourceConsumer: Send + Sync {
    /// Stable id matching the `consumer_id` on its leases ("serving", "bevy",
    /// "livekit").
    fn consumer_id(&self) -> &str;

    /// What this consumer believes it currently holds. The daemon reconciles
    /// this against the hardware scan to catch drift (leaked or untracked bytes).
    fn footprint(&self) -> Vec<ConsumerFootprint>;

    /// Honor a reclaim ask. May release fully, partially, defer (cleanup in
    /// flight), or refuse (with a named reason — fail-loud, never a silent
    /// zero). The authority awaits this but bounds it with the request deadline.
    async fn reclaim(&self, request: ReclaimRequest) -> ReclaimOutcome;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    struct FakePool {
        id: String,
        held: AtomicU64,
    }

    #[async_trait]
    impl ResourceConsumer for FakePool {
        fn consumer_id(&self) -> &str {
            &self.id
        }
        fn footprint(&self) -> Vec<ConsumerFootprint> {
            vec![ConsumerFootprint {
                kind: ResourceKind::Vram,
                bytes: self.held.load(Ordering::SeqCst),
                detail: "fake pool".into(),
            }]
        }
        async fn reclaim(&self, request: ReclaimRequest) -> ReclaimOutcome {
            let before = self.held.load(Ordering::SeqCst);
            let freed = before.min(request.target_bytes);
            self.held.store(before - freed, Ordering::SeqCst);
            ReclaimOutcome::released(freed)
        }
    }

    // what this catches: the trait is object-safe behind Arc<dyn> and the
    // async reclaim actually mutates the holder and reports honest freed bytes.
    // If reclaim were not object-safe (e.g. generic), the authority could not
    // hold a heterogeneous Vec<Arc<dyn ResourceConsumer>> of serving+bevy+livekit.
    #[tokio::test]
    async fn dyn_consumer_reclaims_and_reports_honestly() {
        let pool: Arc<dyn ResourceConsumer> = Arc::new(FakePool {
            id: "bevy".into(),
            held: AtomicU64::new(5_000),
        });
        assert_eq!(pool.consumer_id(), "bevy");
        assert_eq!(pool.footprint()[0].bytes, 5_000);

        let outcome = pool
            .reclaim(ReclaimRequest {
                kind: ResourceKind::Vram,
                target_bytes: 3_000,
                deadline_ms: 50,
                reason: ReclaimReason::Pressure,
            })
            .await;
        assert_eq!(outcome.freed_bytes, 3_000);
        assert_eq!(outcome.status, ReclaimStatus::Released);
        assert_eq!(pool.footprint()[0].bytes, 2_000);
    }

    // what this catches: a refusal is zero-bytes and names why — never a silent
    // freed=0 the authority might misread as "released nothing, all good".
    #[test]
    fn refused_outcome_is_zero_and_named() {
        let outcome = ReclaimOutcome::refused("render loop mid-frame");
        assert_eq!(outcome.freed_bytes, 0);
        assert_eq!(outcome.status, ReclaimStatus::Refused);
        assert_eq!(outcome.detail.as_deref(), Some("render loop mid-frame"));
    }
}
