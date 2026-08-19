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
//! # The gentler lever: tier-down instead of going dark
//!
//! Suppress-and-unload is whole-lease-granular — serving goes dark until a
//! re-load. Under [`Pressure`] there is a gentler answer: re-home to a SMALLER
//! base ([`pin`] the target so the daemon's reconcile swaps to it, #105), keep
//! answering, and report [`ReclaimStatus::Partial`] with only the freed delta
//! (old resident − new resident). WHICH smaller model — if any — is a decision,
//! and it is not baked in here: an injected [`TierDownPolicy`] chooses, and the
//! same async handshake carries out whatever it picks (decline → full unload;
//! choose → pin + Partial). See [`super::serving_tier_down`]. `Shutdown` and
//! `Rebalance` skip the policy — both want the lease gone from this box, which a
//! smaller model here does not serve.
//!
//! [`Pressure`]: crate::resources::ReclaimReason::Pressure
//! [`pin`]: ServingConsumer::pin_model
//! [`TierDownPolicy`]: super::serving_tier_down::TierDownPolicy
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

use crate::cognition::serving_plan::MIN_SERVE_CTX;
use crate::inference::llama_server::ServingSnapshot;
use crate::modules::serving_tier_down::{TierDownContext, TierDownPolicy};
use crate::resources::{
    ConsumerFootprint, ReclaimOutcome, ReclaimReason, ReclaimRequest, ReclaimStatus,
    ResourceConsumer, ResourceKind,
};

/// Resolves an active model id + its live serving shape (served per-slot window,
/// lane count) to its TOTAL resident VRAM bytes — weights PLUS the KV-cache of
/// every lane at that window (#79). The daemon supplies this from its live
/// catalog + footprint estimator (the same numbers that feed the serving plan —
/// one footprint authority, not two). The window/lanes come from the
/// [`ServingSnapshot`] (llama.cpp's own `/props` truth), never a recomputed plan
/// value. Injectable so the consumer is testable without a populated registry.
pub type FootprintFn = Arc<dyn Fn(&str, u32, u32) -> u64 + Send + Sync>;

/// The `consumer_id` serving's leases carry. Matches the id the acquire-on-load
/// half will mint leases under, so the authority's asks route back here.
pub const SERVING_CONSUMER_ID: &str = "serving";

/// A reclaim serving has scheduled and is waiting for the snapshot to confirm.
/// Keyed (in [`ServingConsumer::pending`]) by the OLD model id that must vanish
/// from the snapshot before we report freed — a full unload clears it; a
/// tier-down swaps it for the smaller model, so either way the old id leaving is
/// the confirmation that the async cleanup landed.
struct Pending {
    /// Bytes to report freed once it lands. Full unload → the whole footprint;
    /// tier-down → the delta (old resident − new resident, since serving still
    /// holds the smaller model).
    freed_on_land: u64,
    /// The terminal disposition once it lands. `Released` when serving now holds
    /// nothing (unload); `Partial` when it shrank but still holds the smaller
    /// model (tier-down).
    status_on_land: ReclaimStatus,
}

/// Serving's [`ResourceConsumer`] adapter — see the module docs.
pub struct ServingConsumer {
    /// Live serving state: which model is active + ready. Read (never blocks) to
    /// report footprint and to confirm an unload/swap actually landed.
    serving: watch::Receiver<ServingSnapshot>,
    /// The full-unload seam: insert an id → the daemon unloads it next reconcile.
    suppress: watch::Sender<Arc<HashSet<String>>>,
    /// The re-home seam: set a smaller model id → the daemon's reconcile swaps to
    /// it (candidates intersect to the pin), freeing the delta without going dark
    /// (#105). The tier-down lever.
    pin: watch::Sender<Option<String>>,
    /// HIGH-WATER RESIDENCY (#438, measured 2026-08-19). The bytes this consumer has
    /// held and has NOT been shown releasing. See `footprint` for why a plan-derived
    /// number alone under-reports by a whole model during every reshape.
    held_high_water: std::sync::atomic::AtomicU64,
    /// The `ready_verified_at_ms` of the last snapshot we accepted as PROOF the previous
    /// process is gone. Evidence, not a timer: the high-water decays only when readiness
    /// is re-verified at the new shape.
    decayed_at_verified_ms: std::sync::atomic::AtomicU64,
    /// active model id + live shape → resident VRAM bytes (weights + per-lane KV).
    footprint_of: FootprintFn,
    /// The swappable intelligence that chooses whether/where to tier down under
    /// pressure. `DeclineTierDown` until a real selection policy is authored — the
    /// consumer's handshake is identical regardless.
    tier_down: Arc<dyn TierDownPolicy>,
    /// Reclaims we've scheduled and are waiting to confirm freed. Brief-locked; no
    /// await is ever held across this guard.
    pending: Mutex<HashMap<String, Pending>>,
}

impl ServingConsumer {
    pub fn new(
        serving: watch::Receiver<ServingSnapshot>,
        suppress: watch::Sender<Arc<HashSet<String>>>,
        pin: watch::Sender<Option<String>>,
        footprint_of: FootprintFn,
        tier_down: Arc<dyn TierDownPolicy>,
    ) -> Self {
        Self {
            serving,
            suppress,
            pin,
            held_high_water: std::sync::atomic::AtomicU64::new(0),
            decayed_at_verified_ms: std::sync::atomic::AtomicU64::new(0),
            footprint_of,
            tier_down,
            pending: Mutex::new(HashMap::new()),
        }
    }

    /// The model the box is serving right now WITH the shape needed to size its
    /// residency: `(model_id, served_per_slot_window, lanes)`. `None` while
    /// nothing is loaded or a load is still coming ready — in both cases serving
    /// holds no reclaimable VRAM yet. The window + lane count are the process's
    /// own truth (from the snapshot's `/props` read), so the KV term is charged
    /// against what is actually resident, never a recomputed plan value.
    fn active_ready(&self) -> Option<(String, u32, u32)> {
        let snap = self.serving.borrow();
        if snap.ready {
            snap.active_model
                .clone()
                .map(|id| (id, snap.served_context_window, snap.lanes))
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

    /// Force-pin `id` (the re-home target), so the daemon's reconcile intersects
    /// its candidates to that one model and swaps to it — the tier-down carry-out.
    /// Unlike suppress this does NOT go dark: the daemon serves the smaller model.
    fn pin_model(&self, id: &str) {
        self.pin.send_modify(|p| {
            if p.as_deref() != Some(id) {
                *p = Some(id.to_string());
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
        use std::sync::atomic::Ordering;

        // WHAT THE PLAN SAYS is served right now. Zero while a lane is loading or
        // relaunching, because `active_ready` gates on `ready`.
        let planned = match self.active_ready() {
            Some((id, window, lanes)) => Some((
                (self.footprint_of)(&id, window, lanes),
                format!("{id} weights+KV resident ({lanes} lane(s) × {window} ctx)"),
            )),
            None => None,
        };
        // A LOADING LANE STILL HOLDS BYTES. `active_ready()` is None until `/props`
        // answers, but the weights are mmap'd from the moment the process starts. Charge
        // them at a MINIMUM shape — one lane at the floor window — which under-states the
        // eventual KV but names the dominant term (weights) immediately. Conservative in
        // the safe direction: it grows to the true figure the moment readiness lands.
        let planned = planned.or_else(|| {
            let snap = self.serving.borrow();
            snap.loading_model.clone().map(|id| {
                let bytes = (self.footprint_of)(&id, MIN_SERVE_CTX, 1);
                (
                    bytes,
                    format!("{id} loading — weights resident, KV not yet allocated"),
                )
            })
        });
        let planned_bytes = planned.as_ref().map(|(b, _)| *b).unwrap_or(0);

        // THE HIGH-WATER REQUIRES A PROOF CHANNEL. `ready_verified_at_ms` is stamped the
        // moment readiness is CONFIRMED for the lane now serving, and a verification we
        // have not yet consumed is proof the previous process is gone. If the snapshot
        // carries NO stamp at all, there is no mechanism that could ever prove release —
        // and a claim that can never be falsified would strand this machine's capacity
        // forever. So: no proof channel, no claim. Report the plan and nothing more.
        //
        // That is the same rule as everywhere else today — the failure direction must
        // move toward LESS trust. Under-reporting for a moment is recoverable; holding an
        // unfalsifiable claim on a third of the box is not.
        let Some(verified) = self.serving.borrow().ready_verified_at_ms else {
            return planned
                .map(|(bytes, detail)| {
                    vec![ConsumerFootprint {
                        kind: ResourceKind::Vram,
                        bytes,
                        detail,
                    }]
                })
                .unwrap_or_default();
        };

        // DECAY ON EVIDENCE, NEVER ON A TIMER.
        let consumed = self.decayed_at_verified_ms.load(Ordering::Relaxed);
        if verified > consumed && planned.is_some() {
            self.decayed_at_verified_ms.store(verified, Ordering::Relaxed);
            self.held_high_water.store(planned_bytes, Ordering::Relaxed);
        } else if planned_bytes > self.held_high_water.load(Ordering::Relaxed) {
            self.held_high_water.store(planned_bytes, Ordering::Relaxed);
        }

        let held = self.held_high_water.load(Ordering::Relaxed);
        if held == 0 {
            // Never held anything and nothing is planned — a real, honest nothing.
            return Vec::new();
        }

        // REPORT WHAT WE HOLD, NOT WHAT WE INTEND. Measured live 2026-08-19: mid-reshape
        // the board read `serving 25.93 GB (1 lane)` against `phys 47.25 GB` — the old
        // 4-lane process was still resident while the snapshot had already flipped to the
        // new shape. The 21.3 GB gap became "unowned", unowned reads as immovable,
        // `available` collapsed to 8.42 GB, the 27B no longer fit, and the planner took a
        // 0.5B. Under-reporting our own residency is indistinguishable from someone else
        // holding it, and it is strictly more dangerous: it invites over-commit against
        // bytes we have not released.
        let detail = match &planned {
            Some((b, d)) if *b >= held => d.clone(),
            Some((_, d)) => format!("{d} — reporting prior residency, not yet confirmed released"),
            None => "prior residency held while no lane is ready (loading or relaunching)"
                .to_string(),
        };
        vec![ConsumerFootprint {
            kind: ResourceKind::Vram,
            bytes: held,
            detail,
        }]
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

        // First: did a reclaim we already scheduled now land? If the snapshot no
        // longer shows the OLD model (unload cleared it, or a tier-down swapped it
        // for the smaller one), the async cleanup landed — report the cached freed
        // bytes with its cached disposition and clear it. This is the
        // Deferred→(Released|Partial) transition the authority re-asks for.
        {
            let mut pending = self.pending.lock();
            if !pending.is_empty() {
                let active = self.active_ready();
                let active_id = active.as_ref().map(|(id, _, _)| id.as_str());
                let landed: Vec<String> = pending
                    .keys()
                    .filter(|id| active_id != Some(id.as_str()))
                    .cloned()
                    .collect();
                if !landed.is_empty() {
                    let mut freed = 0u64;
                    // Released only if EVERY landed reclaim fully released; a
                    // tier-down landing means serving still holds the smaller
                    // model, so the honest aggregate is Partial.
                    let mut status = ReclaimStatus::Released;
                    for id in &landed {
                        if let Some(p) = pending.remove(id) {
                            freed = freed.saturating_add(p.freed_on_land);
                            if p.status_on_land == ReclaimStatus::Partial {
                                status = ReclaimStatus::Partial;
                            }
                        }
                    }
                    let detail = match status {
                        ReclaimStatus::Partial => "tier-down landed — VRAM partially freed",
                        _ => "unload landed — VRAM released",
                    };
                    return ReclaimOutcome {
                        freed_bytes: freed,
                        status,
                        detail: Some(detail.into()),
                    };
                }
                // Still in flight — honest backpressure, re-ask next tick.
                return ReclaimOutcome {
                    freed_bytes: 0,
                    status: ReclaimStatus::Deferred,
                    detail: Some("cleanup in flight".into()),
                };
            }
        }

        // Nothing pending → decide the lever. We need the active model + its live
        // shape to size what we hold.
        let Some((active, window, lanes)) = self.active_ready() else {
            // We hold nothing reclaimable — already free, honestly zero.
            return ReclaimOutcome::released(0);
        };
        let current = (self.footprint_of)(&active, window, lanes);

        // Under Pressure another peer needs room NOW but serving need not go dark:
        // offer the tier-down policy the chance to re-home to a smaller base and
        // free only the delta. Shutdown wants everything gone; Rebalance is moving
        // the lease OFF this box — a smaller model here serves neither, so both
        // skip straight to the full unload.
        if request.reason == ReclaimReason::Pressure {
            let ctx = TierDownContext {
                active_model: &active,
                current_bytes: current,
                served_window: window,
                lanes,
                request: &request,
            };
            if let Some(td) = self.tier_down.choose(&ctx) {
                // Trust the policy's target but VERIFY it is a genuine shrink to a
                // real model — never carry out a phantom or a lateral/up "tier
                // down" (fail loud by declining to the honest full unload).
                if !td.target_model.is_empty() && td.resident_after < current {
                    let freed = current - td.resident_after;
                    self.pin_model(&td.target_model);
                    self.pending.lock().insert(
                        active.clone(),
                        Pending {
                            freed_on_land: freed,
                            status_on_land: ReclaimStatus::Partial,
                        },
                    );
                    return ReclaimOutcome {
                        freed_bytes: 0,
                        status: ReclaimStatus::Deferred,
                        detail: Some(format!(
                            "tier-down {active} → {} scheduled (frees {freed} bytes)",
                            td.target_model
                        )),
                    };
                }
                // Non-shrinking proposal → ignore it, fall through to full unload.
            }
        }

        // No tier-down → unload the active model to free its whole footprint.
        // Whole-lease-granular: serving's lease IS the active model.
        self.suppress_model(&active);
        self.pending.lock().insert(
            active.clone(),
            Pending {
                freed_on_land: current,
                status_on_land: ReclaimStatus::Released,
            },
        );

        // The unload is still async — Deferred is the honest first answer
        // regardless of reason; the GPU has not released yet. The authority
        // re-asks and gets Released once it lands.
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
    use crate::modules::serving_tier_down::{DeclineTierDown, TierDown};

    // A hand-driven serving snapshot + suppress pair, standing in for the daemon.
    // The test plays the daemon's role: when it sees a model suppressed, it
    // "unloads" by clearing the snapshot — exactly what the real reconcile does,
    // minus the multi-second llama-server kill. Wired with `DeclineTierDown`, so
    // the only lever is a full unload — the tier-down path has its own rig below.
    fn rig(active: &str, bytes: u64) -> (ServingConsumer, watch::Sender<ServingSnapshot>) {
        let (serving_tx, serving_rx) = watch::channel(ServingSnapshot {
            loading_model: None,
            // test fixture: no live readiness was ever CONFIRMED here.
            ready_verified_at_ms: None,
            active_model: Some(active.to_string()),
            ready: true,
            base_url: "http://localhost:0/v1".into(),
            adapters: Vec::new(),
            served_context_window: 11008,
            lanes: 4,
            degraded_reason: None,
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
        });
        let (suppress_tx, _srx) = watch::channel(Arc::new(HashSet::new()));
        let (pin_tx, _prx) = watch::channel(None);
        // Flat resident estimate — the shape (window, lanes) is ignored here so the
        // reclaim handshake tests assert against a stable footprint. The test that
        // the window/lanes actually REACH this fn lives separately below.
        let footprint_of: FootprintFn = Arc::new(move |_id: &str, _window: u32, _lanes: u32| bytes);
        let consumer = ServingConsumer::new(
            serving_rx,
            suppress_tx,
            pin_tx,
            footprint_of,
            Arc::new(DeclineTierDown),
        );
        (consumer, serving_tx)
    }

    /// A tier-down policy that always proposes re-homing to a fixed smaller model
    /// at a fixed resident size — the outlier that exercises the Partial handshake.
    struct AlwaysTierDown {
        target: String,
        resident_after: u64,
    }
    impl TierDownPolicy for AlwaysTierDown {
        fn choose(&self, _ctx: &TierDownContext) -> Option<TierDown> {
            Some(TierDown {
                target_model: self.target.clone(),
                resident_after: self.resident_after,
            })
        }
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
    /// A rig whose footprint actually SCALES with the lane count — the shared `rig`
    /// deliberately returns a flat number ("the shape is ignored here"), which is right
    /// for the reclaim-handshake tests and useless for these: with a flat resolver a
    /// 4-lane and a 1-lane serve are the same size, so a shrink is unobservable. I wrote
    /// three tests against the flat rig before reading it.
    fn shape_rig(bytes_per_lane: u64) -> (ServingConsumer, watch::Sender<ServingSnapshot>) {
        let (serving_tx, serving_rx) = watch::channel(ServingSnapshot {
            loading_model: None,
            ready_verified_at_ms: Some(1_000),
            active_model: Some("qwen3.8-27b".into()),
            ready: true,
            base_url: "http://localhost:0/v1".into(),
            adapters: Vec::new(),
            served_context_window: 22_528,
            lanes: 4,
            degraded_reason: None,
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
        });
        let (suppress_tx, _srx) = watch::channel(Arc::new(HashSet::new()));
        let (pin_tx, _prx) = watch::channel(None);
        let footprint_of: FootprintFn =
            Arc::new(move |_id: &str, _w: u32, lanes: u32| bytes_per_lane * lanes as u64);
        let consumer = ServingConsumer::new(
            serving_rx,
            suppress_tx,
            pin_tx,
            footprint_of,
            Arc::new(DeclineTierDown),
        );
        (consumer, serving_tx)
    }

    // what this catches: a high-water that can never be released. If the snapshot carries
    // no `ready_verified_at_ms`, nothing can ever prove the old process died — holding a
    // claim on those bytes would strand them for the life of the process. No proof
    // channel, no claim.
    #[tokio::test]
    async fn without_a_verification_stamp_no_high_water_is_claimed() {
        let (consumer, serving_tx) = rig("qwen3-coder-30b", 18_000); // stamp is None
        assert_eq!(consumer.footprint()[0].bytes, 18_000);
        serving_tx.send_modify(|s| s.ready = false);
        assert!(
            consumer.footprint().is_empty(),
            "with no way to ever prove release, we must not claim bytes we cannot give back"
        );
    }

    // what this catches: THE COLD-LOAD HOLE, measured live 2026-08-19 AFTER the
    // high-water fix and NOT covered by it. On a first boot the consumer has no prior
    // residency and `active_ready()` is None the whole way up, so the board read
    // `serving 0.00 GB` while physical climbed 29.90 → 36.88 GB. The cause was upstream:
    // the snapshot was binary, so throughout the load window it FORGOT what it was
    // loading and the consumer had nothing to name. Now it does.
    #[tokio::test]
    async fn a_cold_load_charges_the_model_being_brought_up() {
        let (consumer, serving_tx) = shape_rig(6_000_000_000);
        serving_tx.send_modify(|s| {
            // Cold: never ready, no prior residency, no verification stamp — exactly the
            // first boot of a fresh install.
            s.ready = false;
            s.active_model = None;
            s.ready_verified_at_ms = None;
            s.loading_model = Some("qwen3.8-27b".into());
        });
        let fp = consumer.footprint();
        assert_eq!(fp.len(), 1, "a loading lane must be attributed, not silent");
        assert!(fp[0].bytes > 0, "the weights are resident from spawn");
        assert!(fp[0].detail.contains("loading"));
    }

    // what this catches: THE MEASURED #438 COLLAPSE. Mid-reshape the board read
    // `serving 25.93 GB (1 lane)` against `phys 47.25 GB` — the old 4-lane process was
    // still resident while the snapshot had already flipped to the new shape. The 21.3 GB
    // gap read as unowned, unowned reads as immovable, `available` fell to 8.42 GB, the
    // 27B stopped fitting, and the planner took a 0.5B. Shrinking the SHAPE must not
    // shrink what we claim to HOLD until something proves the bytes came back.
    #[tokio::test]
    async fn a_shrinking_shape_does_not_shrink_reported_residency_without_proof() {
        let (consumer, serving_tx) = shape_rig(6_000_000_000);
        let wide = consumer.footprint()[0].bytes;
        assert!(wide > 0, "a ready 4-lane serve must attribute something");

        // The plan reshapes to 1 lane. No NEW readiness verification yet — the old
        // process may still be resident, and nothing has shown us otherwise.
        serving_tx.send_modify(|s| s.lanes = 1);
        let during = consumer.footprint();
        assert_eq!(
            during[0].bytes, wide,
            "must still claim the wider residency until a verification proves release"
        );
        assert!(during[0].detail.contains("not yet confirmed released"));
    }

    // what this catches: the OTHER half — `active_ready()` returns None while a lane
    // loads or relaunches, and the old code returned an EMPTY vec. Zero attributed while
    // gigabytes are demonstrably resident is the same defect wearing a different face.
    #[tokio::test]
    async fn a_loading_lane_still_reports_the_bytes_it_has_not_released() {
        let (consumer, serving_tx) = shape_rig(6_000_000_000);
        let held = consumer.footprint()[0].bytes;

        serving_tx.send_modify(|s| s.ready = false); // relaunching
        let during = consumer.footprint();
        assert_eq!(during.len(), 1, "must NOT go silent while bytes are resident");
        assert_eq!(during[0].bytes, held);
        assert!(during[0].detail.contains("loading or relaunching"));
    }

    // what this catches: the opposite failure — never decaying, which strands capacity
    // forever. A FRESH readiness verification is proof the previous process is gone, so
    // the claim must collapse to the live plan. Evidence, not a timer.
    #[tokio::test]
    async fn a_new_readiness_verification_releases_the_high_water() {
        let (consumer, serving_tx) = shape_rig(6_000_000_000);
        let wide = consumer.footprint()[0].bytes;

        serving_tx.send_modify(|s| s.lanes = 1);
        assert_eq!(consumer.footprint()[0].bytes, wide, "no proof yet");

        // The new 1-lane process verifies ready → the old one is gone.
        serving_tx.send_modify(|s| s.ready_verified_at_ms = Some(2_000));
        let after = consumer.footprint()[0].bytes;
        assert!(
            after < wide,
            "a confirmed relaunch must release the claim, or capacity is stranded forever"
        );
    }

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

    // what this catches: the served per-slot window AND lane count from the live
    // snapshot reach the FootprintFn, so serving's residency is charged as
    // weights + lanes × KV(window) — not weights alone (the pre-#79 under-report
    // that let serving's own KV masquerade as external contention on the board).
    // If active_ready() dropped either the window or the lanes, the resident
    // estimate would collapse back to weights-only and this fails.
    #[tokio::test]
    async fn footprint_passes_served_window_and_lanes_to_resolver() {
        let seen: Arc<Mutex<Option<(String, u32, u32)>>> = Arc::new(Mutex::new(None));
        let seen_w = seen.clone();
        let (serving_tx, serving_rx) = watch::channel(ServingSnapshot {
            loading_model: None,
            // test fixture: no live readiness was ever CONFIRMED here.
            ready_verified_at_ms: None,
            active_model: Some("coder-14b".into()),
            ready: true,
            base_url: "http://localhost:0/v1".into(),
            adapters: Vec::new(),
            served_context_window: 11008,
            lanes: 3,
            degraded_reason: None,
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
        });
        let footprint_of: FootprintFn = Arc::new(move |id: &str, window: u32, lanes: u32| {
            *seen_w.lock() = Some((id.to_string(), window, lanes));
            // weights(1000) + lanes × kv_per_token(10) × window
            1000 + lanes as u64 * 10 * window as u64
        });
        let (suppress_tx, _srx) = watch::channel(Arc::new(HashSet::new()));
        let (pin_tx, _prx) = watch::channel(None);
        let consumer = ServingConsumer::new(
            serving_rx,
            suppress_tx,
            pin_tx,
            footprint_of,
            Arc::new(DeclineTierDown),
        );

        let fp = consumer.footprint();
        assert_eq!(fp.len(), 1);
        assert_eq!(
            *seen.lock(),
            Some(("coder-14b".into(), 11008, 3)),
            "the snapshot's window + lane count must reach the resolver"
        );
        assert_eq!(
            fp[0].bytes,
            1000 + 3 * 10 * 11008,
            "resident folds per-lane KV"
        );
        assert!(fp[0].detail.contains("3 lane(s) × 11008 ctx"));

        // A re-home to a single lane at a smaller window shrinks the charged KV.
        serving_tx.send_modify(|s| {
            s.lanes = 1;
            s.served_context_window = 4096;
        });
        let fp = consumer.footprint();
        assert_eq!(*seen.lock(), Some(("coder-14b".into(), 4096, 1)));
        assert_eq!(fp[0].bytes, 1000 + 1 * 10 * 4096);
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

    // Build a consumer with an injected tier-down policy, exposing the pin
    // receiver so the test can watch the re-home target the consumer sets. Plays
    // the daemon's role by hand — no llama-server, no catalog.
    fn tier_down_rig(
        active: &str,
        current: u64,
        policy: Arc<dyn TierDownPolicy>,
    ) -> (
        ServingConsumer,
        watch::Sender<ServingSnapshot>,
        watch::Receiver<Option<String>>,
    ) {
        let (serving_tx, serving_rx) = watch::channel(ServingSnapshot {
            loading_model: None,
            // test fixture: no live readiness was ever CONFIRMED here.
            ready_verified_at_ms: None,
            active_model: Some(active.to_string()),
            ready: true,
            base_url: "http://localhost:0/v1".into(),
            adapters: Vec::new(),
            served_context_window: 11008,
            lanes: 4,
            degraded_reason: None,
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
        });
        let (suppress_tx, _srx) = watch::channel(Arc::new(HashSet::new()));
        let (pin_tx, pin_rx) = watch::channel(None);
        let footprint_of: FootprintFn = Arc::new(move |_id: &str, _w: u32, _l: u32| current);
        let consumer = ServingConsumer::new(serving_rx, suppress_tx, pin_tx, footprint_of, policy);
        (consumer, serving_tx, pin_rx)
    }

    // what this catches: THE tier-down lever (#79) — under Pressure, when a policy
    // chooses to re-home to a smaller base, serving PINS that model (does not go
    // dark via suppress), defers across the async swap, and reports Partial with
    // the freed DELTA (old − new) once the snapshot shows the smaller model
    // active. Reporting Released, or the whole footprint, or suppressing instead
    // of pinning would each be a lie: serving still holds the smaller model.
    #[tokio::test]
    async fn pressure_tier_down_pins_smaller_then_reports_partial() {
        // active holds 18_000; policy re-homes to "coder-7b" resident at 6_000 →
        // freed delta = 12_000, and serving keeps serving the 6_000 model.
        let policy = Arc::new(AlwaysTierDown {
            target: "coder-7b".into(),
            resident_after: 6_000,
        });
        let (consumer, serving_tx, pin_rx) = tier_down_rig("coder-30b", 18_000, policy);

        // First ask under Pressure: pins the smaller model, defers, frees nothing
        // yet (the swap is async), and does NOT suppress (never goes dark).
        let first = consumer.reclaim(ask()).await;
        assert_eq!(first.status, ReclaimStatus::Deferred);
        assert_eq!(first.freed_bytes, 0);
        assert_eq!(
            pin_rx.borrow().as_deref(),
            Some("coder-7b"),
            "re-home pinned"
        );
        assert!(
            !consumer.suppress.borrow().contains("coder-30b"),
            "tier-down pins, never suppresses — serving must not go dark"
        );

        // Re-ask while the old model is still active (reconcile not done): deferred.
        let second = consumer.reclaim(ask()).await;
        assert_eq!(second.status, ReclaimStatus::Deferred);

        // The daemon's reconcile swaps to the smaller model → snapshot shows it.
        serving_tx.send_modify(|s| s.active_model = Some("coder-7b".into()));

        // Now the ask resolves Partial with the freed DELTA (18_000 − 6_000).
        let third = consumer.reclaim(ask()).await;
        assert_eq!(third.status, ReclaimStatus::Partial);
        assert_eq!(third.freed_bytes, 12_000);

        // Cleared — a further ask starts fresh (would tier down the 7b next time).
        let fourth = consumer.reclaim(ask()).await;
        assert_eq!(
            fourth.status,
            ReclaimStatus::Deferred,
            "new cycle, not stuck"
        );
    }

    // what this catches: reason gating. Shutdown wants EVERYTHING gone and
    // Rebalance is moving the lease OFF this box — a smaller model here serves
    // neither, so the tier-down policy is not even consulted; both go straight to
    // the full-unload (suppress) path. If tier-down fired on Shutdown, serving
    // would keep holding a smaller model when the authority needs it fully gone.
    #[tokio::test]
    async fn shutdown_and_rebalance_skip_tier_down_and_fully_unload() {
        for reason in [ReclaimReason::Shutdown, ReclaimReason::Rebalance] {
            let policy = Arc::new(AlwaysTierDown {
                target: "coder-7b".into(),
                resident_after: 6_000,
            });
            let (consumer, _tx, pin_rx) = tier_down_rig("coder-30b", 18_000, policy);
            let out = consumer
                .reclaim(ReclaimRequest {
                    kind: ResourceKind::Vram,
                    target_bytes: 8_000,
                    deadline_ms: 1_000,
                    reason,
                })
                .await;
            assert_eq!(out.status, ReclaimStatus::Deferred);
            assert!(
                pin_rx.borrow().is_none(),
                "{reason:?} must not pin/tier-down"
            );
            assert!(
                consumer.suppress.borrow().contains("coder-30b"),
                "{reason:?} suppresses for a full unload"
            );
        }
    }

    // what this catches: the phantom-guard. A policy that proposes a NON-shrinking
    // target (resident_after >= current, or an empty id) is ignored and the
    // consumer falls through to the honest full unload — it never carries out a
    // lateral/up "tier-down" that would free nothing while claiming it did.
    #[tokio::test]
    async fn non_shrinking_tier_down_is_ignored_and_falls_through_to_unload() {
        // resident_after (20_000) > current (18_000): not a shrink.
        let policy = Arc::new(AlwaysTierDown {
            target: "bigger-not-smaller".into(),
            resident_after: 20_000,
        });
        let (consumer, _tx, pin_rx) = tier_down_rig("coder-30b", 18_000, policy);
        let out = consumer.reclaim(ask()).await;
        assert_eq!(out.status, ReclaimStatus::Deferred);
        assert!(
            pin_rx.borrow().is_none(),
            "non-shrink proposal must not pin"
        );
        assert!(
            consumer.suppress.borrow().contains("coder-30b"),
            "falls through to full unload"
        );
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
