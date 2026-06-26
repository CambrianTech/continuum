//! `ServingConsumer` — serving's face to the [`ResourceGovernor`](crate::resources).
//!
//! The serving daemon holds the heaviest residency on the box: a multi-GB model
//! in VRAM/UMA for the length of a serving epoch. Under this task (#56) that
//! residency is no longer self-granted — it is a *lease* from the one
//! per-machine authority, and serving is a peer consumer alongside Bevy and
//! LiveKit. This is the half the authority calls: when another peer needs the
//! bytes (a game grabs VRAM, an avatar scene loads), the governor *asks* serving
//! to free some, and serving answers honestly.
//!
//! # The one freeing lever, used honestly
//!
//! Serving's only public way to free VRAM is the suppress set
//! ([`ServingDaemonModule::suppress_sender`](super::serving_daemon::ServingDaemonModule::suppress_sender)):
//! mark the active model id as unloaded, and the daemon's own reconcile drops it
//! on its next tick — VRAM freed live, no restart. That unload is **async and
//! multi-second** (kill the child, wait for the GPU to release), so this
//! consumer never claims bytes are free before they are:
//!
//! - First ask → suppress the active model, remember the footprint we expect to
//!   reclaim, answer [`ReclaimStatus::Deferred`] ("unload scheduled, ask again").
//! - Re-ask while the snapshot still shows that model active → the unload is in
//!   flight → `Deferred` again. The authority keeps the lease alive and re-asks.
//! - Re-ask once the snapshot no longer shows that model → the GPU released it →
//!   answer [`ReclaimStatus::Released`] with the exact footprint, and the
//!   governor shrinks the lease by that scan-confirmed delta.
//!
//! This is whole-lease-granular by construction: serving's lease *is* the active
//! model, and suppress frees the whole thing. A future tier-down (swap to a
//! smaller base under pressure and report [`ReclaimStatus::Partial`]) is the
//! upgrade/downgrade seam noted in [`crate::resources`]; it slots in here without
//! changing the trait, but suppress-and-unload is the honest first lever.
//!
//! # No new task, no parallel allocator
//!
//! This is a thin adapter over handles the daemon already publishes
//! (`subscribe_serving`, `suppress_sender`) plus a footprint resolver the daemon
//! supplies from its catalog. It owns no tick, no thread, no lock across an
//! await — the governor's daemon drives it. The acquire-on-load half (serving
//! *taking* the lease, and `host_budget` becoming governed headroom) is the
//! sibling slice that converges the two allocators.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use parking_lot::Mutex;
use tokio::sync::watch;

use crate::inference::llama_server::ServingSnapshot;
use crate::resources::{
    ConsumerFootprint, ReclaimOutcome, ReclaimReason, ReclaimRequest, ReclaimStatus,
    ResourceConsumer, ResourceKind,
};

/// Resolves an active model id to its resident VRAM bytes. The daemon supplies
/// this from its live catalog + footprint estimator (the same numbers that feed
/// the serving plan — one footprint authority, not two). Injectable so the
/// consumer is testable without a populated registry.
pub type FootprintFn = Arc<dyn Fn(&str) -> u64 + Send + Sync>;

/// The `consumer_id` serving's leases carry. Matches the id the acquire-on-load
/// half will mint leases under, so the authority's asks route back here.
pub const SERVING_CONSUMER_ID: &str = "serving";

/// Serving's [`ResourceConsumer`] adapter — see the module docs.
pub struct ServingConsumer {
    /// Live serving state: which model is active + ready. Read (never blocks) to
    /// report footprint and to confirm an unload actually landed.
    serving: watch::Receiver<ServingSnapshot>,
    /// The free seam: insert an id → the daemon unloads it next reconcile.
    suppress: watch::Sender<Arc<HashSet<String>>>,
    /// active model id → resident VRAM bytes.
    footprint_of: FootprintFn,
    /// Models we've suppressed and are waiting to confirm freed, with the bytes
    /// we expect to reclaim once the snapshot shows them gone. Brief-locked; no
    /// await is ever held across this guard.
    pending: Mutex<HashMap<String, u64>>,
}

impl ServingConsumer {
    pub fn new(
        serving: watch::Receiver<ServingSnapshot>,
        suppress: watch::Sender<Arc<HashSet<String>>>,
        footprint_of: FootprintFn,
    ) -> Self {
        Self {
            serving,
            suppress,
            footprint_of,
            pending: Mutex::new(HashMap::new()),
        }
    }

    /// The model the box is serving right now, if any is fully live. `None` while
    /// nothing is loaded or a load is still coming ready — in both cases serving
    /// holds no reclaimable VRAM yet.
    fn active_ready(&self) -> Option<String> {
        let snap = self.serving.borrow();
        if snap.ready {
            snap.active_model.clone()
        } else {
            None
        }
    }

    /// Add `id` to the suppress set (COW insert), so the daemon's reconcile
    /// unloads it. Idempotent — re-suppressing an already-suppressed id is a
    /// no-op the daemon ignores.
    fn suppress_model(&self, id: &str) {
        self.suppress.send_modify(|set| {
            if !set.contains(id) {
                let mut next = HashSet::clone(set);
                next.insert(id.to_string());
                *set = Arc::new(next);
            }
        });
    }
}

#[async_trait]
impl ResourceConsumer for ServingConsumer {
    fn consumer_id(&self) -> &str {
        SERVING_CONSUMER_ID
    }

    fn footprint(&self) -> Vec<ConsumerFootprint> {
        match self.active_ready() {
            Some(id) => {
                let bytes = (self.footprint_of)(&id);
                vec![ConsumerFootprint {
                    kind: ResourceKind::Vram,
                    bytes,
                    detail: format!("{id} weights resident"),
                }]
            }
            None => Vec::new(),
        }
    }

    async fn reclaim(&self, request: ReclaimRequest) -> ReclaimOutcome {
        // Serving only holds VRAM. An ask for any other kind is honestly refused
        // (named, never a silent freed=0) so the authority does not misread it.
        if request.kind != ResourceKind::Vram {
            return ReclaimOutcome::refused(format!(
                "serving holds no {:?}, only Vram",
                request.kind
            ));
        }

        // First: did an unload we already scheduled now land? If the snapshot no
        // longer shows a model we suppressed, the GPU released it — report the
        // exact footprint we cached and clear it. This is the Deferred→Released
        // transition the authority re-asks for.
        {
            let mut pending = self.pending.lock();
            if !pending.is_empty() {
                let active = self.active_ready();
                let landed: Vec<String> = pending
                    .keys()
                    .filter(|id| active.as_deref() != Some(id.as_str()))
                    .cloned()
                    .collect();
                if !landed.is_empty() {
                    let freed: u64 = landed.iter().filter_map(|id| pending.remove(id)).sum();
                    return ReclaimOutcome {
                        freed_bytes: freed,
                        status: ReclaimStatus::Released,
                        detail: Some("unload landed — VRAM released".into()),
                    };
                }
                // Still in flight — honest backpressure, re-ask next tick.
                return ReclaimOutcome {
                    freed_bytes: 0,
                    status: ReclaimStatus::Deferred,
                    detail: Some("unload in flight".into()),
                };
            }
        }

        // Nothing pending → start an unload of the active model to free its whole
        // footprint. Whole-lease-granular: serving's lease IS the active model.
        let Some(active) = self.active_ready() else {
            // We hold nothing reclaimable — already free, honestly zero.
            return ReclaimOutcome::released(0);
        };
        let expect = (self.footprint_of)(&active);
        self.suppress_model(&active);
        self.pending.lock().insert(active.clone(), expect);

        // Shutdown wants everything gone but the unload is still async — Deferred
        // is the honest first answer regardless of reason; the GPU has not
        // released yet. The authority re-asks and gets Released once it lands.
        let detail = match request.reason {
            ReclaimReason::Shutdown => "shutdown unload scheduled",
            _ => "unload scheduled",
        };
        ReclaimOutcome {
            freed_bytes: 0,
            status: ReclaimStatus::Deferred,
            detail: Some(detail.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A hand-driven serving snapshot + suppress pair, standing in for the daemon.
    // The test plays the daemon's role: when it sees a model suppressed, it
    // "unloads" by clearing the snapshot — exactly what the real reconcile does,
    // minus the multi-second llama-server kill.
    fn rig(active: &str, bytes: u64) -> (ServingConsumer, watch::Sender<ServingSnapshot>) {
        let (serving_tx, serving_rx) = watch::channel(ServingSnapshot {
            active_model: Some(active.to_string()),
            ready: true,
            base_url: "http://localhost:0/v1".into(),
            adapters: Vec::new(),
        });
        let (suppress_tx, _srx) = watch::channel(Arc::new(HashSet::new()));
        let footprint_of: FootprintFn = Arc::new(move |_id: &str| bytes);
        let consumer = ServingConsumer::new(serving_rx, suppress_tx, footprint_of);
        (consumer, serving_tx)
    }

    fn ask() -> ReclaimRequest {
        ReclaimRequest {
            kind: ResourceKind::Vram,
            target_bytes: 8_000,
            deadline_ms: 1_000,
            reason: ReclaimReason::Pressure,
        }
    }

    // what this catches: footprint reports the active model's resident VRAM only
    // while it is live+ready. If serving reported a footprint for a model that
    // is not actually resident, the daemon's scan-vs-footprint reconcile would
    // see phantom bytes; if it reported nothing while a model is live, the
    // authority would think serving is reclaimable-free and over-grant into it.
    #[tokio::test]
    async fn footprint_is_the_active_ready_model_only() {
        let (consumer, serving_tx) = rig("qwen3-coder-30b", 18_000);
        let fp = consumer.footprint();
        assert_eq!(fp.len(), 1);
        assert_eq!(fp[0].kind, ResourceKind::Vram);
        assert_eq!(fp[0].bytes, 18_000);

        // not-ready (a load still coming up) → holds nothing reclaimable yet
        serving_tx.send_modify(|s| s.ready = false);
        assert!(consumer.footprint().is_empty());

        // nothing loaded → empty
        serving_tx.send_modify(|s| {
            s.ready = true;
            s.active_model = None;
        });
        assert!(consumer.footprint().is_empty());
    }

    // what this catches: the honest Deferred→Released handshake across an async
    // unload. Serving must NOT claim bytes freed on the first ask (the VRAM is
    // still resident until the reconcile lands) — claiming early is exactly the
    // over-grant OOM this whole task exists to prevent. It suppresses, defers,
    // and only reports Released — with the real footprint — once the snapshot
    // confirms the model is gone.
    #[tokio::test]
    async fn suppress_then_defer_then_release_when_unload_lands() {
        let (consumer, serving_tx) = rig("qwen3-coder-30b", 18_000);

        // First ask: suppresses + defers, frees nothing yet.
        let first = consumer.reclaim(ask()).await;
        assert_eq!(first.status, ReclaimStatus::Deferred);
        assert_eq!(first.freed_bytes, 0);
        assert!(consumer.suppress.borrow().contains("qwen3-coder-30b"));

        // Re-ask while still resident (reconcile not done): still deferred.
        let second = consumer.reclaim(ask()).await;
        assert_eq!(second.status, ReclaimStatus::Deferred);
        assert_eq!(second.freed_bytes, 0);

        // The daemon's reconcile unloads it → snapshot clears.
        serving_tx.send_modify(|s| s.active_model = None);

        // Now the ask resolves Released with the exact footprint.
        let third = consumer.reclaim(ask()).await;
        assert_eq!(third.status, ReclaimStatus::Released);
        assert_eq!(third.freed_bytes, 18_000);

        // And it's no longer pending — a further ask reports nothing left.
        let fourth = consumer.reclaim(ask()).await;
        assert_eq!(fourth.status, ReclaimStatus::Released);
        assert_eq!(fourth.freed_bytes, 0);
    }

    // what this catches: an ask for a kind serving doesn't hold is refused with a
    // named reason, never a silent freed=0 the authority could misread as "freed
    // nothing, all fine" and stop asking the consumer that actually holds it.
    #[tokio::test]
    async fn non_vram_ask_is_refused_and_named() {
        let (consumer, _tx) = rig("m", 1_000);
        let out = consumer
            .reclaim(ReclaimRequest {
                kind: ResourceKind::Ram,
                target_bytes: 1_000,
                deadline_ms: 100,
                reason: ReclaimReason::Pressure,
            })
            .await;
        assert_eq!(out.status, ReclaimStatus::Refused);
        assert_eq!(out.freed_bytes, 0);
        assert!(out.detail.unwrap().contains("Ram"));
    }
}
