//! The governor — the deterministic reconcile core (mechanism + policy joined,
//! still no clock and no I/O).
//!
//! This is the piece that does the *accounting and the eviction decision* the
//! authority is for, kept deliberately free of async, hardware, and the
//! wall-clock so it can be driven as scripted multi-consumer scenarios on one
//! machine — the first rung of the test ladder (start → grant → report →
//! pressure → evict → release), long before any real GPU or second computer.
//!
//! Split of responsibility:
//! - [`super::ledger::ResourceLeaseLedger`] — pure byte bookkeeping + safety
//!   bounds (over-commit guard, floors, dwell).
//! - [`super::arbiter::LeaseArbiter`] — the swappable value/urgency policy that
//!   orders eviction victims.
//! - **This** `ResourceGovernor` — joins them: mints deterministic lease ids,
//!   passes the caller's `now_ms`, and on each [`reconcile`](ResourceGovernor::reconcile)
//!   tick computes who must give bytes back (over-budget overage + overdue
//!   expirations), returning a [`PlannedReclaim`] list for the daemon to drive.
//! - The async **daemon** (next slice) is the only part that touches the clock,
//!   the GPU/RAM/disk scan sources, and the consumer callbacks. It is a thin
//!   shell over this core.
//!
//! The reclaim handshake stays two-phase and patient: `reconcile` only *plans*
//! who to ask; bytes free only when the daemon feeds a consumer's
//! [`ReclaimOutcome`] back through [`apply_reclaim_outcome`](ResourceGovernor::apply_reclaim_outcome).
//! Defer/Refuse leave the bytes held and re-surface next tick — never a yank.

use super::arbiter::{LeaseArbiter, TieredArbiter};
use super::consumer::{ConsumerFootprint, ReclaimOutcome, ReclaimReason, ReclaimRequest};
use super::lease::{LeaseError, LeaseRequest, ReclaimPolicy, ResourceKind, ResourceLease};
use super::ledger::{LeaseBoard, ResourceLeaseLedger};

/// Policy values the governor applies — the dwell window that breaks reclaim
/// thrash and the grace period a `Graceful` consumer gets to free on its own
/// terms. Constructed in code (defaults via `Default`); the daemon may override
/// from `SubstrateGovernor` pressure later, but these are never env vars.
#[derive(Debug, Clone, Copy)]
pub struct GovernorConfig {
    /// An active lease younger than this is off-limits to reclaim — anti-thrash
    /// hysteresis (a fresh grant lives at least this long before it can be
    /// taken back).
    pub min_dwell_ms: u64,
    /// How long a `Graceful` reclaim's deadline sits in the future — the
    /// consumer's window to shed on its own terms before the ask is overdue.
    /// `Hard` and overdue-expired reclaims get an immediate (now) deadline.
    pub graceful_grace_ms: u64,
}

impl Default for GovernorConfig {
    fn default() -> Self {
        Self {
            min_dwell_ms: 2_000,
            graceful_grace_ms: 1_000,
        }
    }
}

/// One planned reclaim the daemon will drive: ask `consumer_id` (via its
/// [`ResourceConsumer`](super::consumer::ResourceConsumer)) to free the bytes
/// of `lease_id`. The `lease_id` is carried so the daemon can map the
/// resulting [`ReclaimOutcome`] back to the exact lease to shrink/release —
/// the bare `ReclaimRequest` is consumer-addressed, not lease-addressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedReclaim {
    pub lease_id: String,
    pub consumer_id: String,
    pub request: ReclaimRequest,
}

/// The single per-machine accounting authority. Owns the ledger + the arbiter,
/// mints lease ids deterministically (no randomness — reproducible scenarios),
/// and is the thing the daemon ticks.
pub struct ResourceGovernor {
    ledger: ResourceLeaseLedger,
    arbiter: Box<dyn LeaseArbiter>,
    config: GovernorConfig,
    /// Monotonic counter for deterministic lease ids. The daemon does not need
    /// a UUID here — uniqueness within one authority is enough, and a counter
    /// keeps scenario tests reproducible.
    next_seq: u64,
}

impl ResourceGovernor {
    pub fn new(arbiter: Box<dyn LeaseArbiter>, config: GovernorConfig) -> Self {
        Self {
            ledger: ResourceLeaseLedger::new(),
            arbiter,
            config,
            next_seq: 0,
        }
    }

    /// The common case: the dumb-fast [`TieredArbiter`] policy.
    pub fn with_default_arbiter(config: GovernorConfig) -> Self {
        Self::new(Box::new(TieredArbiter::default()), config)
    }

    // ---- scan ingest -------------------------------------------------------

    /// Daemon feeds the scanned ceiling for a kind each tick (from GpuMonitor /
    /// SystemResourceMonitor / DiskPressureMonitor). Capacity can *shrink* under
    /// live grants when something outside a lease grabs memory — that overage is
    /// exactly what `reconcile` then has to claw back.
    pub fn set_capacity(&mut self, kind: ResourceKind, bytes: u64) {
        self.ledger.set_capacity(kind, bytes);
    }

    /// Daemon feeds a consumer's freshly-polled footprint each tick — the
    /// self-declared ATTRIBUTION ingest, sibling of `set_capacity`. Surfaces on
    /// the board (per-consumer attribution + `measured_bytes`) and the drift
    /// probe. See [`ResourceLeaseLedger::set_measured`].
    pub fn set_measured(&mut self, consumer_id: &str, footprints: Vec<ConsumerFootprint>) {
        self.ledger.set_measured(consumer_id, footprints);
    }

    /// Daemon feeds physical usage for a kind each tick — `total − free` from the
    /// monitor via [`CapacitySource::used_bytes`](super::capacity::CapacitySource).
    /// The un-inversion's ground truth: this (not `set_measured`) is what nets
    /// against the fixed ceiling to yield `available`.
    pub fn set_physical_used(&mut self, kind: ResourceKind, bytes: u64) {
        self.ledger.set_physical_used(kind, bytes);
    }

    /// Total self-declared residency of a kind across all measured consumers.
    pub fn measured(&self, kind: ResourceKind) -> u64 {
        self.ledger.measured(kind)
    }

    /// Bytes of a kind physically resident across everyone, as last scanned.
    pub fn physical_used(&self, kind: ResourceKind) -> u64 {
        self.ledger.physical_used(kind)
    }

    /// What is really spoken for of a kind: `max(granted, physical_used)`. The
    /// daemon reads this for its per-kind contention signal, and reconcile claws
    /// back the amount by which it exceeds the fixed ceiling.
    pub fn committed(&self, kind: ResourceKind) -> u64 {
        self.ledger.committed(kind)
    }

    // ---- lease lifecycle (passthrough with id minting) ---------------------

    /// Grant a lease, minting its id. Refuses (fail-loud) if it would exceed
    /// scanned-available capacity — the over-commit guard. The daemon may, on
    /// `InsufficientCapacity`, reclaim and retry; this method itself never
    /// over-grants.
    pub fn acquire(
        &mut self,
        req: &LeaseRequest,
        now_ms: u64,
    ) -> Result<ResourceLease, LeaseError> {
        let lease_id = self.mint_id(&req.consumer_id);
        self.ledger.acquire(req, lease_id, now_ms)
    }

    pub fn renew(
        &mut self,
        lease_id: &str,
        expires_at_ms: u64,
        now_ms: u64,
    ) -> Result<(), LeaseError> {
        self.ledger.renew(lease_id, expires_at_ms, now_ms)
    }

    pub fn release(&mut self, lease_id: &str) -> Result<ResourceLease, LeaseError> {
        self.ledger.release(lease_id)
    }

    pub fn reserve(&mut self, consumer_id: impl Into<String>, kind: ResourceKind, min_bytes: u64) {
        self.ledger.reserve(consumer_id, kind, min_bytes);
    }

    /// Headroom THIS consumer may plan against — the ledger's
    /// [`available_for`](super::ledger::LeaseLedger::available_for): global
    /// available minus every OTHER consumer's unmet reservation floor. The
    /// budget-side twin of the guard `acquire` already enforces: a consumer that
    /// PLANS from this number never sizes itself into bytes it would then be
    /// refused (#225 — serving planned from reservation-blind `available`, grew
    /// its window over the embed lane's floor, and embedding went dead).
    pub fn available_for(&self, consumer_id: &str, kind: ResourceKind) -> u64 {
        self.ledger.available_for(consumer_id, kind)
    }

    /// The replace-myself budget — see
    /// [`ResourceLeaseLedger::budget_for_replacing`](super::ledger::ResourceLeaseLedger::budget_for_replacing).
    /// A consumer planning its own successor must not have its own residency counted
    /// against it, or it can never choose anything as large as what it is running.
    pub fn budget_for_replacing(&self, consumer_id: &str, kind: ResourceKind) -> u64 {
        self.ledger.budget_for_replacing(consumer_id, kind)
    }

    // ---- the tick ----------------------------------------------------------

    /// The per-tick decision. For each kind, compute how many bytes must come
    /// back: the over-budget overage (`max(granted, physical_used) − capacity`,
    /// when our grants plus everything physically resident exceed the fixed
    /// ceiling — a game grabbing VRAM shows up here as rising `physical_used`, not
    /// a shrinking ceiling) OR the bytes held by overdue-expired leases —
    /// whichever is larger, so expirations are always driven even when under budget.
    /// Selects victims via the arbiter (respecting floors + dwell in the ledger)
    /// and returns a [`PlannedReclaim`] per victim. Pure: `now_ms` and the
    /// per-kind `pressure` are supplied; nothing here reads a clock or does I/O.
    ///
    /// Returns an empty plan when nothing is over budget and nothing is overdue.
    /// If a kind is over budget but every byte is protected (all pinned-active /
    /// floored), no victim is planned for it — the daemon then fails loud on the
    /// demand rather than yanking a protected holder.
    pub fn reconcile(
        &self,
        now_ms: u64,
        pressure: impl Fn(ResourceKind) -> f64,
    ) -> Vec<PlannedReclaim> {
        // No external demand — the over-budget + overdue-expiry path only.
        self.reconcile_for_demand(now_ms, pressure, |_| 0)
    }

    /// `reconcile` plus an external **reclaim demand** per kind — the seam the
    /// daemon uses to honor a [`PressureBroker`](crate::paging::PressureBroker)
    /// `evict_at_least` ask. A cross-resource relief request ("another tier
    /// needs room — shed N bytes of this kind even though you are within your
    /// own budget") folds in as a third target source alongside the over-budget
    /// overage and overdue expirations. It still flows through the SAME victim
    /// selector, so floors / dwell / pinned-active protection are honored
    /// identically — a broker ask can never breach a protection the over-budget
    /// path could not. `demand(kind) == 0` makes this exactly `reconcile`.
    pub fn reconcile_for_demand(
        &self,
        now_ms: u64,
        pressure: impl Fn(ResourceKind) -> f64,
        demand: impl Fn(ResourceKind) -> u64,
    ) -> Vec<PlannedReclaim> {
        let mut plans = Vec::new();
        for kind in ResourceKind::ALL {
            let capacity = self.ledger.capacity(kind);
            // Oversubscription against the FIXED ceiling: the larger of what we
            // granted and what is physically resident (external grabs + our
            // unleased residency), minus the ceiling. When it is all external and
            // none of it is a lease we hold, `select_to_reclaim` finds nothing
            // safe to take and the daemon leaves it (refuses new demand) — we
            // cannot reclaim a game's memory, only our own leases.
            let overage = self.ledger.committed(kind).saturating_sub(capacity);
            let expired_bytes: u64 = self
                .ledger
                .expire(now_ms)
                .iter()
                .filter(|l| l.kind == kind)
                .map(|l| l.bytes)
                .fold(0u64, |a, b| a.saturating_add(b));
            let target = overage.max(expired_bytes).max(demand(kind));
            if target == 0 {
                continue;
            }
            let Some(victims) = self.ledger.select_to_reclaim(
                kind,
                target,
                now_ms,
                self.config.min_dwell_ms,
                self.arbiter.as_ref(),
                pressure(kind),
            ) else {
                // Over budget but nothing safe to take — leave it; the daemon
                // escalates (refuse new demand) rather than breach a protection.
                continue;
            };
            for (lease_id, bytes) in victims {
                let Some(lease) = self.ledger.lease(&lease_id) else {
                    continue;
                };
                let deadline_ms =
                    self.deadline_for(lease.reclaim_policy, lease.is_expired(now_ms), now_ms);
                plans.push(PlannedReclaim {
                    lease_id: lease_id.clone(),
                    consumer_id: lease.consumer_id.clone(),
                    request: ReclaimRequest {
                        kind,
                        target_bytes: bytes,
                        deadline_ms,
                        reason: ReclaimReason::Pressure,
                    },
                });
            }
        }
        plans
    }

    /// Fold a consumer's reclaim response back into the ledger. ONE path,
    /// honest accounting: the lease shrinks by exactly the bytes the consumer
    /// reported freeing. A full `Released` frees all its bytes (lease removed);
    /// a `Partial` (tier-downgrade) frees some and the lease lives on smaller;
    /// `Deferred`/`Refused` report `freed_bytes == 0` → a no-op, the bytes stay
    /// held and re-surface on the next `reconcile`. The status is advisory
    /// (logging / escalation); the byte delta is authoritative. Fail-loud on a
    /// stale lease id.
    pub fn apply_reclaim_outcome(
        &mut self,
        lease_id: &str,
        outcome: &ReclaimOutcome,
    ) -> Result<u64, LeaseError> {
        self.ledger.shrink(lease_id, outcome.freed_bytes)
    }

    // ---- reporting ---------------------------------------------------------

    /// The board the daemon publishes and the `resources/*` commands return.
    pub fn board(&self) -> LeaseBoard {
        self.ledger.board()
    }

    pub fn granted(&self, kind: ResourceKind) -> u64 {
        self.ledger.granted(kind)
    }

    pub fn available(&self, kind: ResourceKind) -> u64 {
        self.ledger.available(kind)
    }

    /// The scanned ceiling currently in effect for a kind — what the daemon last
    /// fed via [`set_capacity`](Self::set_capacity). The broker pool view reports
    /// this as its `capacity_bytes`.
    pub fn capacity(&self, kind: ResourceKind) -> u64 {
        self.ledger.capacity(kind)
    }

    // ---- internals ---------------------------------------------------------

    fn mint_id(&mut self, consumer_id: &str) -> String {
        let id = format!("{consumer_id}-{}", self.next_seq);
        self.next_seq += 1;
        id
    }

    fn deadline_for(&self, policy: ReclaimPolicy, expired: bool, now_ms: u64) -> u64 {
        if expired {
            return now_ms; // overdue — the deadline already passed
        }
        match policy {
            ReclaimPolicy::Graceful => now_ms.saturating_add(self.config.graceful_grace_ms),
            // Hard tolerates an immediate yank; active Pinned is never planned
            // (the ledger filters it), so this arm is only reached for Hard.
            ReclaimPolicy::Hard | ReclaimPolicy::Pinned => now_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::consumer::ReclaimStatus;

    fn req(
        consumer: &str,
        kind: ResourceKind,
        bytes: u64,
        ttl_ms: u64,
        policy: ReclaimPolicy,
    ) -> LeaseRequest {
        LeaseRequest {
            consumer_id: consumer.into(),
            kind,
            bytes,
            ttl_ms,
            reclaim_policy: policy,
        }
    }

    fn no_pressure(_k: ResourceKind) -> f64 {
        0.0
    }

    // what this catches: the full single-machine lifecycle the authority exists
    // for, emulated with three consumers and no hardware — start (set capacity),
    // accounting (grant + board math), pressure (a scan shrinks capacity under
    // live grants), eviction (reconcile plans a victim respecting pinned), and
    // the two-phase reclaim (the consumer downgrades → Partial → the lease
    // shrinks, never gets yanked). This is rung-1 of the test ladder: if the
    // accounting/eviction logic is wrong, it is wrong here, deterministically,
    // before any Docker or second computer.
    #[test]
    fn scenario_three_consumers_pressure_evicts_the_only_eligible_holder() {
        let mut gov = ResourceGovernor::with_default_arbiter(GovernorConfig {
            min_dwell_ms: 0,
            graceful_grace_ms: 1_000,
        });
        gov.set_capacity(ResourceKind::Vram, 10_000);

        // bevy (render loop, pinned), serving (elastic inference, graceful),
        // livekit (live call, pinned) — together exactly fill 10GB.
        let bevy = gov
            .acquire(
                &req(
                    "bevy",
                    ResourceKind::Vram,
                    4_000,
                    60_000,
                    ReclaimPolicy::Pinned,
                ),
                100,
            )
            .unwrap();
        let serving = gov
            .acquire(
                &req(
                    "serving",
                    ResourceKind::Vram,
                    4_000,
                    60_000,
                    ReclaimPolicy::Graceful,
                ),
                100,
            )
            .unwrap();
        let _livekit = gov
            .acquire(
                &req(
                    "livekit",
                    ResourceKind::Vram,
                    2_000,
                    60_000,
                    ReclaimPolicy::Pinned,
                ),
                100,
            )
            .unwrap();

        // reporting: full board, nothing free
        assert_eq!(gov.granted(ResourceKind::Vram), 10_000);
        assert_eq!(gov.available(ResourceKind::Vram), 0);
        assert_eq!(gov.board().leases.len(), 3);

        // calm tick: nothing over budget, nothing expired → empty plan
        assert!(gov.reconcile(1_000, no_pressure).is_empty());

        // PRESSURE: a scan reports VRAM dropped to 8GB (something outside a
        // lease grabbed 2GB). Now granted (10GB) > capacity (8GB) by 2GB.
        gov.set_capacity(ResourceKind::Vram, 8_000);
        let plan = gov.reconcile(1_000, no_pressure);

        // Only serving's graceful lease is reclaimable (both others pinned).
        // Its whole 4GB lease is the single victim covering the 2GB overage.
        assert_eq!(plan.len(), 1, "exactly one victim planned");
        assert_eq!(plan[0].lease_id, serving.lease_id);
        assert_eq!(plan[0].consumer_id, "serving");
        assert_eq!(plan[0].request.kind, ResourceKind::Vram);
        assert_eq!(plan[0].request.target_bytes, 4_000);
        // graceful → deadline is in the future (grace window), not immediate
        assert_eq!(plan[0].request.deadline_ms, 1_000 + 1_000);

        // The bevy/livekit pins were never planned — protected under pressure.
        assert!(!plan.iter().any(|p| p.lease_id == bevy.lease_id));

        // TWO-PHASE reclaim: serving downgrades its model rather than dying,
        // freeing exactly the 2GB asked (Partial), staying alive at 2GB.
        let outcome = ReclaimOutcome {
            freed_bytes: 2_000,
            status: ReclaimStatus::Partial,
            detail: None,
        };
        let remaining = gov
            .apply_reclaim_outcome(&serving.lease_id, &outcome)
            .unwrap();
        assert_eq!(
            remaining, 2_000,
            "serving lives on at the smaller footprint"
        );

        // accounting reconciled: back within the 8GB ceiling, serving still present
        assert_eq!(gov.granted(ResourceKind::Vram), 8_000);
        assert_eq!(gov.available(ResourceKind::Vram), 0);
        assert!(gov
            .board()
            .leases
            .iter()
            .any(|l| l.lease_id == serving.lease_id));
    }

    // what this catches: expirations are driven even when we are UNDER budget.
    // A lease whose TTL lapsed is overdue — the holder promised it back by then
    // — so reconcile must plan its reclaim with an immediate (already-passed)
    // deadline regardless of headroom, and a full Released frees the bytes.
    #[test]
    fn reconcile_drives_overdue_expirations_under_budget_and_release_frees() {
        let mut gov = ResourceGovernor::with_default_arbiter(GovernorConfig::default());
        gov.set_capacity(ResourceKind::Ram, 10_000);
        // short-lived 3GB lease; lots of headroom (not over budget)
        let cache = gov
            .acquire(
                &req(
                    "serving",
                    ResourceKind::Ram,
                    3_000,
                    500,
                    ReclaimPolicy::Graceful,
                ),
                0,
            )
            .unwrap();
        assert!(
            gov.reconcile(100, no_pressure).is_empty(),
            "still live → nothing to do"
        );

        // past its TTL (expired at 500): overdue even though 7GB is free
        let plan = gov.reconcile(1_000, no_pressure);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].lease_id, cache.lease_id);
        assert_eq!(
            plan[0].request.deadline_ms, 1_000,
            "overdue → immediate deadline"
        );

        // consumer releases it fully → bytes freed, lease gone
        let outcome = ReclaimOutcome {
            freed_bytes: 3_000,
            status: ReclaimStatus::Released,
            detail: None,
        };
        gov.apply_reclaim_outcome(&cache.lease_id, &outcome)
            .unwrap();
        assert_eq!(gov.granted(ResourceKind::Ram), 0);
        assert!(gov.board().leases.is_empty());
    }

    // what this catches: a Deferred reclaim is honest backpressure, not a yank.
    // The consumer could not free in time; freed_bytes is 0, the lease stays
    // fully held, and the next reconcile re-plans it (the daemon retries) — the
    // bytes are never silently dropped from the ledger.
    #[test]
    fn deferred_outcome_keeps_bytes_held_and_replans_next_tick() {
        let mut gov = ResourceGovernor::with_default_arbiter(GovernorConfig {
            min_dwell_ms: 0,
            graceful_grace_ms: 0,
        });
        gov.set_capacity(ResourceKind::Vram, 4_000);
        let lease = gov
            .acquire(
                &req(
                    "serving",
                    ResourceKind::Vram,
                    4_000,
                    60_000,
                    ReclaimPolicy::Graceful,
                ),
                0,
            )
            .unwrap();

        // capacity collapses to 1GB → 3GB over budget
        gov.set_capacity(ResourceKind::Vram, 1_000);
        let plan = gov.reconcile(10, no_pressure);
        assert_eq!(plan.len(), 1);

        // consumer can't free yet → Deferred, 0 bytes
        let deferred = ReclaimOutcome {
            freed_bytes: 0,
            status: ReclaimStatus::Deferred,
            detail: Some("draining".into()),
        };
        let remaining = gov
            .apply_reclaim_outcome(&lease.lease_id, &deferred)
            .unwrap();
        assert_eq!(remaining, 4_000, "still fully held — never yanked");
        assert_eq!(gov.granted(ResourceKind::Vram), 4_000);

        // next tick re-plans the same lease (the daemon will ask again)
        let plan2 = gov.reconcile(20, no_pressure);
        assert_eq!(plan2.len(), 1);
        assert_eq!(plan2[0].lease_id, lease.lease_id);
    }

    // what this catches: min-dwell at the governor level — a freshly granted
    // lease is shielded from reclaim even under budget pressure, so the
    // authority can't page a consumer in and rip it back out a tick later. The
    // overage simply goes un-actioned until the dwell elapses (escalation, not
    // thrash).
    #[test]
    fn fresh_lease_within_dwell_is_not_planned_even_over_budget() {
        let mut gov = ResourceGovernor::with_default_arbiter(GovernorConfig {
            min_dwell_ms: 5_000,
            graceful_grace_ms: 0,
        });
        gov.set_capacity(ResourceKind::Vram, 8_000);
        let lease = gov
            .acquire(
                &req(
                    "serving",
                    ResourceKind::Vram,
                    8_000,
                    60_000,
                    ReclaimPolicy::Graceful,
                ),
                1_000,
            )
            .unwrap();

        // squeeze to 4GB at t=2000 (held only 1000ms < 5000ms dwell) → protected
        gov.set_capacity(ResourceKind::Vram, 4_000);
        assert!(
            gov.reconcile(2_000, no_pressure).is_empty(),
            "within dwell → no thrash"
        );

        // at t=6500 (held 5500ms ≥ dwell) → now eligible
        let plan = gov.reconcile(6_500, no_pressure);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].lease_id, lease.lease_id);
    }
}
