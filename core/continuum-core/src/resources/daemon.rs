//! The resource daemon — the async shell over the deterministic [governor].
//!
//! [governor]: super::governor::ResourceGovernor
//!
//! Everything the governor forbids itself (the clock, the hardware scan, the
//! consumer callbacks, async) lives HERE, and nowhere else. The daemon is the
//! one tokio task that:
//!
//! 1. reads each [`CapacitySource`](super::capacity::CapacitySource) and feeds
//!    `governor.set_capacity` (the scan ingest),
//! 2. ticks `governor.reconcile_for_demand` to get a [`PlannedReclaim`] list,
//! 3. drives each plan's async [`ResourceConsumer::reclaim`] **concurrently**,
//!    bounded by the request deadline, and folds the honest
//!    [`ReclaimOutcome`](super::consumer::ReclaimOutcome) back via
//!    `governor.apply_reclaim_outcome`,
//! 4. publishes the [`LeaseBoard`] on a `watch` channel for `resources/*`
//!    commands and grid gossip,
//! 5. registers per-kind [`LeasePoolView`]s with the
//!    [`PressureBroker`](crate::paging::PressureBroker) so cross-resource relief
//!    reaches the lease pool through the ONE existing orchestrator (never a
//!    parallel manager — CONCURRENCY-STYLE-GUIDE forbidden move #6).
//!
//! # Obsessively non-blocking (the design center)
//!
//! Per Joel: *"never block unless absolutely necessary. Anything not immediate
//! is async thread. You can't make a system that slows down all its constituent
//! parts or callers."* So:
//!
//! - **Lease accounting** (`acquire`/`release`/`renew`/`reserve`) takes a
//!   `parking_lot::Mutex` for a pure in-memory, microsecond-scale critical
//!   section — no I/O, no await held. It contends only with the tick's equally
//!   brief snapshot section. A persona acquiring a lease per reply is never
//!   slowed by a reclaim in flight.
//! - **`ResourcePool::evict_at_least`** (the broker calls it synchronously) does
//!   NOT perform the reclaim. It posts `(kind, want)` to the daemon's queue and
//!   returns immediately. The actual async reclaim happens on the daemon's own
//!   tick. The sync caller never waits on an async consumer.
//! - **The reclaim fan-out** runs every plan concurrently (`join_all`), each
//!   bounded by `timeout` + isolated by `catch_unwind`. One slow or panicking
//!   consumer never serializes or stalls the others.
//! - **Hot reads** (`board`, `is_over_budget`) hit a `watch` borrow and an
//!   atomic — they never touch the accounting mutex.
//!
//! The governor mutex is sync (`parking_lot`), not `tokio::sync::Mutex`, *because*
//! we never hold it across an await — each tick phase drops the guard before any
//! `.await`. Holding a sync lock across await is the bug this shape avoids.

use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::{Mutex, RwLock};
use tokio::sync::{mpsc, watch};

use crate::paging::pool::{ResourcePool, ResourcePoolEntry};
use crate::paging::PressureBroker;
use crate::runtime::daemon::{
    guarded, spawn_daemon, Daemon, DaemonChannel, Guarded, QuarantineLedger,
    DEFAULT_QUARANTINE_LIMIT,
};
use crate::{clog_info, clog_warn};

use super::capacity::CapacitySource;
use super::consumer::{ConsumerFootprint, ReclaimOutcome, ResourceConsumer};
use super::governor::{GovernorConfig, ResourceGovernor};
use super::ledger::LeaseBoard;
use super::lease::{LeaseError, LeaseRequest, ReclaimPolicy, ResourceKind, ResourceLease};

/// Default daemon cadence. Faster than the 5 s `PressureBroker` tick because the
/// daemon also drives lease *expirations*, which want sub-second resolution; one
/// tick is a mutex + reconcile over a handful of leases (cheap). Code, not env.
const DEFAULT_TICK_MS: u64 = 1_000;

/// Floor on the reclaim grace budget. A `Hard`/expired reclaim gets a `now`
/// deadline (zero grace), but a consumer still needs a real moment to actually
/// free — so even a yank-tolerant reclaim is granted at least this long before
/// the daemon treats it as timed-out. Bounded so a hung consumer can't stall the
/// fan-out.
const MIN_RECLAIM_BUDGET_MS: u64 = 100;

/// Materiality threshold for re-emitting the `resource_drift` probe. The reconcile
/// tick runs every second; a stable untracked residency (e.g. serving holding the
/// base model without a lease) would otherwise log an identical drift 1/sec forever.
/// The probe re-fires only when the drift moves by at least this much since its last
/// emission — enough to catch a real allocation change (a model load/unload, a lane
/// spin-up) while ignoring per-tick jitter. The live board still carries the exact
/// current drift every tick for readers.
const DRIFT_REPORT_DELTA_BYTES: u64 = 64 * 1024 * 1024;

/// Daemon policy knobs. Constructed in code (defaults via `Default`); never env
/// vars. The `governor` field carries the dwell/grace the inner core applies.
#[derive(Debug, Clone, Copy)]
pub struct DaemonConfig {
    pub tick_interval: Duration,
    /// Minimum grace handed to a consumer even on a zero-deadline reclaim.
    pub min_reclaim_budget: Duration,
    pub governor: GovernorConfig,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            tick_interval: Duration::from_millis(DEFAULT_TICK_MS),
            min_reclaim_budget: Duration::from_millis(MIN_RECLAIM_BUDGET_MS),
            governor: GovernorConfig::default(),
        }
    }
}

/// True if any kind on the published board holds more than its scanned ceiling.
/// This is the daemon's gate, derived purely from the snapshot it publishes — the
/// [`DaemonChannel`] recomputes it on every `publish`, so there is no separate
/// over-budget flag to keep in sync (the old hand-maintained `AtomicBool` is
/// gone). Shared by the channel's gate derivation and any reader.
fn board_over_budget(board: &LeaseBoard) -> bool {
    // Over-budget is the un-inversion condition: what is REALLY spoken for —
    // `max(granted, physical_used)` — exceeds the FIXED ceiling. Keying off
    // `granted` alone would be blind to a game grabbing VRAM (granted stays 0 while
    // physical usage climbs past capacity); the physical axis is exactly what makes
    // that oversubscription visible so the reconcile can react.
    board
        .kinds
        .iter()
        .any(|k| k.granted_bytes.max(k.physical_used_bytes) > k.capacity_bytes)
}

/// The single per-machine resource authority's runtime shell.
///
/// A [`Daemon`] — but a *hybrid* one: it publishes its [`LeaseBoard`] both from
/// its tick (reconcile) AND from synchronous lease ops (`acquire`/`release`),
/// because a caller must see its lease on the board immediately, before the next
/// tick. That is exactly why the publish channel is *embedded* ([`DaemonChannel`])
/// rather than handed only to `tick`: a pure poller couldn't surface a
/// synchronous mutation. The runner ([`spawn_daemon`]) owns the loop + per-tick
/// panic isolation; this type owns the accounting and the lease API.
pub struct ResourceDaemon {
    /// The deterministic accounting core. `parking_lot::Mutex` because every
    /// critical section is sync and short — NEVER held across an await.
    governor: Mutex<ResourceGovernor>,
    /// Scan sources, one per managed kind. Read off-lock each tick.
    sources: Vec<Arc<dyn CapacitySource>>,
    /// Leaseholders, by registration order. Grows via `add_consumer`.
    consumers: RwLock<Vec<Arc<dyn ResourceConsumer>>>,
    /// Three-strikes quarantine for misbehaving consumers — the shared base
    /// policy, replacing the hand-rolled `consecutive_panics`/`disabled` triad.
    quarantine: Mutex<QuarantineLedger>,
    /// The embedded publish channel: latest board + lock-free over-budget gate.
    /// Hot readers borrow it, never the governor lock; the tick AND the
    /// synchronous lease ops publish through it.
    channel: DaemonChannel<LeaseBoard>,
    /// The sync→async bridge: `ResourcePool::evict_at_least` posts `(kind, want)`
    /// here and returns; the tick drains it into the reconcile demand.
    evict_tx: mpsc::UnboundedSender<(ResourceKind, u64)>,
    /// Receiver half of the evict bridge, owned behind a brief sync lock so the
    /// `&self` tick (the [`Daemon`] contract) can drain it. Only the single
    /// daemon task ever touches it; the lock is uncontended and never held across
    /// an await.
    evict_rx: Mutex<mpsc::UnboundedReceiver<(ResourceKind, u64)>>,
    config: DaemonConfig,
    /// Last-emitted `resource_drift` bytes per kind (indexed by [`kind_idx`]), so the
    /// drift probe fires on a MATERIAL CHANGE, not every tick. A persistent untracked
    /// residency (e.g. serving holding the base model with no lease) is real, but an
    /// identical drift re-logged 1/sec is noise, not a new event — the live board still
    /// carries the current drift for readers every tick.
    last_drift_bytes: Mutex<[u64; 3]>,
}

static GLOBAL_RESOURCE_DAEMON: std::sync::OnceLock<std::sync::Arc<ResourceDaemon>> =
    std::sync::OnceLock::new();

impl ResourceDaemon {
    /// Publish THE per-machine resource authority process-globally (first writer wins —
    /// the boot path). Doctrine-aligned: there is exactly ONE authority per machine
    /// (#56), so a global read handle is the honest shape, same precedent as
    /// `MessageBus::set_global` / `PersonaAircRuntimeRegistry::set_global`. Lets
    /// host-independent bodies (the detached eval sampling per-task VRAM for the
    /// efficiency axis) read the live board without a threaded handle.
    pub fn set_global(daemon: std::sync::Arc<ResourceDaemon>) {
        let _ = GLOBAL_RESOURCE_DAEMON.set(daemon);
    }

    /// The process-global authority, if boot published it (None in bare unit tests).
    pub fn global() -> Option<std::sync::Arc<ResourceDaemon>> {
        GLOBAL_RESOURCE_DAEMON.get().cloned()
    }

    /// Start the daemon on its own tokio task. Returns the handle subsystems use
    /// to lease, subscribe to the board, and (via `register_with_broker`) wire
    /// cross-resource relief.
    pub fn start(
        sources: Vec<Arc<dyn CapacitySource>>,
        consumers: Vec<Arc<dyn ResourceConsumer>>,
        config: DaemonConfig,
    ) -> Arc<Self> {
        let mut governor = ResourceGovernor::with_default_arbiter(config.governor);
        // Prime capacity AND physical usage from the scan sources NOW, so the
        // authority knows both its fixed ceilings and what is already resident the
        // moment it exists — a lease acquired before the first tick is admitted
        // against the real global remainder (`capacity − max(granted, used)`), not
        // a boot-time zero that would over-grant into memory already spoken for.
        for src in &sources {
            governor.set_capacity(src.kind(), src.ceiling_bytes());
            governor.set_physical_used(src.kind(), src.used_bytes());
        }
        let channel = DaemonChannel::new(governor.board(), board_over_budget);
        let (evict_tx, evict_rx) = mpsc::unbounded_channel();

        let daemon = Arc::new(Self {
            governor: Mutex::new(governor),
            sources,
            consumers: RwLock::new(consumers),
            quarantine: Mutex::new(QuarantineLedger::new(DEFAULT_QUARANTINE_LIMIT)),
            channel,
            evict_tx,
            evict_rx: Mutex::new(evict_rx),
            config,
            last_drift_bytes: Mutex::new([0; 3]),
        });

        // The canonical runner owns the loop: interval + Skip + PER-TICK
        // catch_unwind. A panicking tick is isolated and the daemon keeps ticking
        // against last-good state — strictly better than the old whole-loop catch
        // that stopped the daemon on the first stray panic. (Consumer panics are
        // additionally caught per-reclaim by `guarded` and quarantined.) We don't
        // need the returned handle — the daemon exposes its own board/subscribe.
        let _ = spawn_daemon(daemon.clone());

        daemon
    }

    // ---- non-blocking lease accounting (brief sync lock, no await held) -----

    /// Grant a lease (or fail-loud refusal). Microsecond in-memory critical
    /// section; publishes the fresh board after releasing the lock.
    pub fn acquire(&self, req: &LeaseRequest) -> Result<ResourceLease, LeaseError> {
        let now = now_ms();
        let (lease, board) = {
            let mut g = self.governor.lock();
            let lease = g.acquire(req, now)?;
            (lease, g.board())
        };
        // Publish synchronously (outside the tick) so a caller sees its lease on
        // the board immediately — the property the embedded channel exists for.
        self.channel.publish(board);
        Ok(lease)
    }

    pub fn release(&self, lease_id: &str) -> Result<ResourceLease, LeaseError> {
        let (lease, board) = {
            let mut g = self.governor.lock();
            let lease = g.release(lease_id)?;
            (lease, g.board())
        };
        self.channel.publish(board);
        Ok(lease)
    }

    /// [`acquire`](Self::acquire), but hand back an RAII [`LeaseGuard`] that
    /// releases the bytes when it drops. This is the OS-`malloc`/RAII shape for a
    /// consumer that reserves VRAM/RAM for a BOUNDED job — an ephemeral eval lane,
    /// a forge run: ask the board for the slot, and if granted, hold the guard for
    /// the job's lifetime. The bytes return to the board automatically on an early
    /// `?` return, a normal end, or a panic — no manual `release` to forget. The
    /// lease TTL is the backstop if the whole PROCESS dies (SIGKILL) without ever
    /// running the guard's `Drop`. An `Err(InsufficientCapacity)` is the honest
    /// "no room" answer the caller uses to spill to CPU or defer — never an OOM.
    pub fn acquire_guarded(
        self: &std::sync::Arc<Self>,
        req: &LeaseRequest,
    ) -> Result<LeaseGuard, LeaseError> {
        let lease = self.acquire(req)?;
        Ok(LeaseGuard {
            daemon: std::sync::Arc::clone(self),
            lease_id: lease.lease_id,
            released: false,
        })
    }

    pub fn renew(&self, lease_id: &str, expires_at_ms: u64) -> Result<(), LeaseError> {
        let now = now_ms();
        let mut g = self.governor.lock();
        g.renew(lease_id, expires_at_ms, now)
    }

    pub fn reserve(&self, consumer_id: impl Into<String>, kind: ResourceKind, min_bytes: u64) {
        let mut g = self.governor.lock();
        g.reserve(consumer_id, kind, min_bytes);
    }

    /// Register a leaseholder after startup — no restart (the directive's
    /// "never restart a daemon to manage resources").
    pub fn add_consumer(&self, consumer: Arc<dyn ResourceConsumer>) {
        let id = consumer.consumer_id().to_string();
        // A re-registering consumer clears any stale quarantine from a prior
        // incarnation — it gets a fresh chance, the conservative-but-not-permanent
        // default.
        self.quarantine.lock().clear(&id);
        self.consumers.write().push(consumer);
        clog_info!("🧮 ResourceDaemon: consumer '{id}' registered");
    }

    /// The ids of every registered leaseholder, in registration order. A cheap
    /// read of the registry (not the accounting lock) — used to confirm a
    /// consumer is wired in (e.g. serving registering itself at boot) and, on
    /// the grid, to enumerate what a node measures without waiting for a board
    /// tick. Quarantined consumers are still listed; quarantine gates polling,
    /// not membership.
    pub fn consumer_ids(&self) -> Vec<String> {
        self.consumers
            .read()
            .iter()
            .map(|c| c.consumer_id().to_string())
            .collect()
    }

    // ---- hot, lock-free reads ----------------------------------------------

    /// Latest board — a `watch` borrow via the embedded channel, never the
    /// accounting lock.
    pub fn board(&self) -> LeaseBoard {
        self.channel.snapshot()
    }

    /// Subscribe to board changes (for `resources/*` watch commands / grid).
    pub fn subscribe(&self) -> watch::Receiver<LeaseBoard> {
        self.channel.handle().subscribe()
    }

    /// Is any kind currently over its scanned ceiling? Lock-free atomic read —
    /// the channel's gate, derived from the published board.
    pub fn is_over_budget(&self) -> bool {
        self.channel.is_gated()
    }

    // ---- broker integration (the ONE orchestrator, not a parallel one) ------

    /// Register a per-kind [`LeasePoolView`] with the broker so cross-resource
    /// pressure relief reaches the lease pool. Each view's `evict_at_least` is
    /// the non-blocking queue post.
    pub fn register_with_broker(self: &Arc<Self>, broker: &PressureBroker) {
        for kind in ResourceKind::ALL {
            broker.register(Arc::new(LeasePoolView {
                kind,
                tier_name: format!("leases-{}", kind.label()),
                daemon: Arc::downgrade(self),
            }));
        }
    }

    // ---- the reconcile cycle (driven by the Daemon contract below) ----------

    /// One reconcile cycle. Structured so the governor lock is taken twice, each
    /// time briefly and never across an await; everything async (the consumer
    /// reclaims) happens between, lock-free. Publishes the board through the
    /// embedded channel post-plan and post-reclaim; the channel re-derives the
    /// over-budget gate from each board, so there is no separate flag to maintain.
    async fn reconcile(&self) {
        let now = now_ms();

        // 1. Drain broker evict asks (non-blocking) into a per-kind demand. Take
        //    the largest ask per kind this tick; it is a one-shot relief request
        //    — if the pool still needs room the broker re-asks on its own tick.
        //    Brief sync lock on the receiver — only this daemon task touches it,
        //    so it is uncontended and never held across an await.
        let mut demand: HashMap<ResourceKind, u64> = HashMap::new();
        {
            let mut evict_rx = self.evict_rx.lock();
            while let Ok((kind, want)) = evict_rx.try_recv() {
                let e = demand.entry(kind).or_insert(0);
                *e = (*e).max(want);
            }
        }

        // 2. Read every source's fixed ceiling AND live physical usage OFF the
        //    accounting lock (cached monitor reads). Both feed the governor below;
        //    the ceiling is near-constant, the usage moves with external pressure.
        let scans: Vec<(ResourceKind, u64, u64)> = self
            .sources
            .iter()
            .map(|s| (s.kind(), s.ceiling_bytes(), s.used_bytes()))
            .collect();

        // 2.5 Snapshot live (non-quarantined) consumers ONCE — reused for both the
        //     footprint poll (this step) and the reclaim fan-out (step 4).
        //     Quarantine only mutates in the fold at the end of this tick, so the
        //     snapshot is stable for the whole cycle. Clone the Arcs out from under
        //     the consumers lock BEFORE taking the quarantine lock so the two locks
        //     never nest.
        let all: Vec<Arc<dyn ResourceConsumer>> = self.consumers.read().iter().cloned().collect();
        let live: Vec<(String, Arc<dyn ResourceConsumer>)> = {
            let q = self.quarantine.lock();
            all.into_iter()
                .filter(|c| !q.is_quarantined(c.consumer_id()))
                .map(|c| (c.consumer_id().to_string(), c))
                .collect()
        };

        // Poll each live consumer's self-declared footprint OFF-lock. This is the
        // MEASUREMENT axis — pure monitoring, EVERY tick, independent of whether
        // any reclaim is planned. `footprint()` is a cheap synchronous read; a
        // panicking one is isolated per-consumer so a single broken consumer can
        // never abort the whole reconcile tick. On panic we skip that consumer
        // (preserving its last-good measurement) rather than wiping it.
        let measured: Vec<(String, Vec<ConsumerFootprint>)> = live
            .iter()
            .filter_map(|(id, c)| {
                match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| c.footprint())) {
                    Ok(fps) => Some((id.clone(), fps)),
                    Err(_) => {
                        clog_warn!(
                            "🧮 ResourceConsumer '{id}' panicked in footprint() — keeping \
                             last-good measurement this tick"
                        );
                        None
                    }
                }
            })
            .collect();

        // 3. Brief lock: ingest capacity + measured footprints, compute per-kind
        //    pressure, plan the reclaims, snapshot the board. No await inside.
        let (plans, board) = {
            let mut g = self.governor.lock();
            for (kind, ceil, used) in &scans {
                g.set_capacity(*kind, *ceil);
                // The un-inversion ground truth: everything physically resident,
                // netted against the fixed ceiling to yield `available`.
                g.set_physical_used(*kind, *used);
            }
            // Feed the self-declared attribution axis: each consumer's freshly-polled
            // residency. Surfaces per-consumer on the board (the WHO); the physical
            // total above is what drives `available`.
            for (id, fps) in measured {
                g.set_measured(&id, fps);
            }

            // Per-kind contention for the arbiter: committed / ceiling, clamped to
            // the [0,1] the arbiter contract expects. `committed` = max(granted,
            // physical_used), so external pressure raises contention even with no
            // lease — the arbiter weighs victims against true oversubscription, not
            // just our own grants. Precomputed into a Copy array so the reconcile
            // closures don't borrow the guard.
            let mut pmap = [0.0f64; 3];
            for kind in ResourceKind::ALL {
                let ceil = g.capacity(kind);
                if ceil > 0 {
                    pmap[kind_idx(kind)] = (g.committed(kind) as f64 / ceil as f64).clamp(0.0, 1.0);
                }
            }

            let plans = g.reconcile_for_demand(
                now,
                |k| pmap[kind_idx(k)],
                |k| demand.get(&k).copied().unwrap_or(0),
            );
            (plans, g.board())
        };

        // Drift report: per kind, measured (self-declared residency) vs granted
        // (what leases account for). A positive drift = bytes physically held that
        // NO lease tracks — exactly serving holding a resident model with no
        // lease, the "0 tracked while the GPU is full" blindness this task fixes.
        // Reporting-only through the observability seam; it steers nothing.
        {
            let mut last = self.last_drift_bytes.lock();
            for k in &board.kinds {
                let drift = k.measured_bytes.saturating_sub(k.granted_bytes);
                let idx = kind_idx(k.kind);
                // Emit only on a MATERIAL change since the last emission — a stable
                // untracked residency is one event, not one-per-second. The board
                // (published every tick below) stays the live source of truth.
                if drift_should_report(drift, &mut last[idx]) {
                    crate::probe!(
                        class = "resource_drift",
                        kind = k.kind.label(),
                        measured_bytes = k.measured_bytes,
                        granted_bytes = k.granted_bytes,
                        drift_bytes = drift,
                        "untracked residency: measured exceeds leased"
                    );
                }
            }
        }

        // Publish post-plan board so readers (and the gate) stay fresh even when
        // calm.
        self.channel.publish(board);

        if plans.is_empty() {
            return;
        }

        // 4. Fan out the reclaims CONCURRENTLY off-lock over the `live` snapshot
        //    from step 2.5. A plan for an unregistered/quarantined consumer is
        //    logged, never silently dropped.
        let mut futs = Vec::with_capacity(plans.len());
        for plan in &plans {
            let Some((_, consumer)) = live.iter().find(|(id, _)| id == &plan.consumer_id) else {
                clog_warn!(
                    "🧮 ResourceDaemon: planned reclaim for unavailable consumer '{}' \
                     (lease {}) — held bytes re-surface next tick",
                    plan.consumer_id,
                    plan.lease_id
                );
                continue;
            };
            let consumer = consumer.clone();
            let consumer_id = plan.consumer_id.clone();
            let lease_id = plan.lease_id.clone();
            let req = plan.request.clone();
            // Grace budget = deadline − now, floored so even a zero-grace (Hard /
            // expired) reclaim gets a real, bounded moment to free.
            let budget_ms = plan
                .request
                .deadline_ms
                .saturating_sub(now)
                .max(self.config.min_reclaim_budget.as_millis() as u64);
            let budget = Duration::from_millis(budget_ms);

            // `guarded` bounds the reclaim by the grace budget and isolates a
            // panic — the shared fan-out kernel, classifying complete/panic/timeout
            // so we quarantine a crash but merely re-ask a slow consumer.
            futs.push(async move {
                let outcome = guarded(budget, consumer.reclaim(req)).await;
                (consumer_id, lease_id, outcome)
            });
        }

        let results = futures::future::join_all(futs).await;

        // 5. Fold results via the shared quarantine ledger: a completed reclaim
        //    is authoritative (any status) and resets the streak; a panic is a
        //    strike (bytes stay held, re-surface next tick); a timeout is patient
        //    backpressure, NOT a strike.
        let mut outcomes: Vec<(String, ReclaimOutcome)> = Vec::new();
        {
            let mut q = self.quarantine.lock();
            for (consumer_id, lease_id, outcome) in results {
                match outcome {
                    Guarded::Completed(o) => {
                        q.record_success(&consumer_id);
                        outcomes.push((lease_id, o));
                    }
                    Guarded::Panicked => {
                        let quarantined = q.record_failure(&consumer_id);
                        clog_warn!(
                            "🧮 ResourceConsumer '{consumer_id}' panicked during reclaim of \
                             {lease_id}{}",
                            if quarantined { " — quarantined" } else { "" }
                        );
                    }
                    Guarded::TimedOut => {
                        clog_warn!(
                            "🧮 ResourceConsumer '{consumer_id}' did not free {lease_id} within \
                             grace — re-asking next tick"
                        );
                    }
                }
            }
        }

        if outcomes.is_empty() {
            return;
        }

        // 6. Brief lock: apply the byte deltas, publish the reconciled board. The
        //    channel re-derives the over-budget gate against the POST-reclaim
        //    grants, so a freed lease clears the gate in the same tick, not one
        //    tick late.
        let board = {
            let mut g = self.governor.lock();
            for (lease_id, outcome) in &outcomes {
                if let Err(e) = g.apply_reclaim_outcome(lease_id, outcome) {
                    clog_warn!("🧮 apply_reclaim_outcome failed for {lease_id}: {e:?}");
                }
            }
            g.board()
        };
        self.channel.publish(board);
    }
}

/// RAII grant handle from [`ResourceDaemon::acquire_guarded`]: the held bytes
/// return to the board when this drops. The byte-residency companion to a scope
/// guard — a bounded consumer (eval lane, forge run) holds it for the job and the
/// reservation is released on any exit path (early `?`, normal end, panic). The
/// lease's TTL backstops a process death that never runs `Drop` (SIGKILL). Not
/// `Clone` (one guard per grant); dropping releases exactly once.
pub struct LeaseGuard {
    daemon: std::sync::Arc<ResourceDaemon>,
    lease_id: String,
    released: bool,
}

impl LeaseGuard {
    /// The board-visible id of the held lease — for evidence/telemetry lines.
    pub fn lease_id(&self) -> &str {
        &self.lease_id
    }

    /// The bytes this guard holds, read live off the board (0 if already gone).
    pub fn bytes(&self) -> u64 {
        self.daemon
            .board()
            .leases
            .iter()
            .find(|l| l.lease_id == self.lease_id)
            .map(|l| l.bytes)
            .unwrap_or(0)
    }

    /// Release the reservation early (idempotent). `Drop` calls this if the guard
    /// is dropped without an explicit release.
    pub fn release(mut self) {
        self.release_inner();
    }

    fn release_inner(&mut self) {
        if self.released {
            return;
        }
        self.released = true;
        // Best-effort: a MissingLease (already released / TTL-expired and swept)
        // is benign — the bytes are already back on the board. Fail-loud in the
        // log, never panic on a drop path.
        if let Err(e) = self.daemon.release(&self.lease_id) {
            clog_warn!(
                "🧮 LeaseGuard: release of lease '{}' failed (likely already expired): {e:?}",
                self.lease_id
            );
        }
    }
}

impl Drop for LeaseGuard {
    fn drop(&mut self) {
        self.release_inner();
    }
}

#[async_trait]
impl Daemon for ResourceDaemon {
    type Snapshot = LeaseBoard;

    fn name(&self) -> &'static str {
        "resources"
    }

    fn cadence(&self) -> Duration {
        // Faster than the 5 s broker tick because the daemon also drives lease
        // expirations, which want sub-second resolution.
        self.config.tick_interval
    }

    fn channel(&self) -> &DaemonChannel<LeaseBoard> {
        &self.channel
    }

    async fn tick(&self) {
        self.reconcile().await;
    }
}

/// A per-kind [`ResourcePool`] facade over the daemon, registered with the
/// `PressureBroker`. It exposes the lease pool's capacity/usage/snapshot for one
/// kind and, crucially, turns the broker's synchronous `evict_at_least` into a
/// non-blocking queue post — the actual reclaim happens on the daemon's async
/// tick, so the broker's thread is never parked on a consumer callback.
pub struct LeasePoolView {
    kind: ResourceKind,
    tier_name: String,
    daemon: Weak<ResourceDaemon>,
}

impl ResourcePool for LeasePoolView {
    fn tier_name(&self) -> &str {
        &self.tier_name
    }

    fn capacity_bytes(&self) -> u64 {
        self.daemon
            .upgrade()
            .map(|d| d.governor.lock().capacity(self.kind))
            .unwrap_or(0)
    }

    fn usage_bytes(&self) -> u64 {
        // The broker's pressure = usage/capacity, so this must be the honest
        // committed number (max(granted, physical_used)), not just our grants —
        // otherwise external VRAM pressure would read as zero usage and the broker
        // would never ask us to relieve for a launching game.
        self.daemon
            .upgrade()
            .map(|d| d.governor.lock().committed(self.kind))
            .unwrap_or(0)
    }

    /// NON-BLOCKING. Queue the relief ask and return best-effort `0` — the
    /// daemon's own tick performs the async reclaim. The broker reads the freed
    /// bytes on the next snapshot, not from this return value.
    fn evict_at_least(&self, want_bytes: u64) -> u64 {
        if let Some(d) = self.daemon.upgrade() {
            let _ = d.evict_tx.send((self.kind, want_bytes));
        }
        0
    }

    fn snapshot(&self) -> Vec<ResourcePoolEntry> {
        let Some(d) = self.daemon.upgrade() else {
            return Vec::new();
        };
        let board = d.board();
        board
            .leases
            .iter()
            .filter(|l| l.kind == self.kind)
            .map(|l| ResourcePoolEntry {
                key: l.lease_id.clone(),
                size_bytes: l.bytes,
                pinned_count: u32::from(l.reclaim_policy == ReclaimPolicy::Pinned),
                loaded_at: l.acquired_at_ms,
                last_access_at: l.acquired_at_ms,
                access_count: 0,
            })
            .collect()
    }
}

// ---- small helpers ----------------------------------------------------------

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

fn kind_idx(kind: ResourceKind) -> usize {
    match kind {
        ResourceKind::Vram => 0,
        ResourceKind::Ram => 1,
        ResourceKind::Disk => 2,
    }
}

/// Decide whether an untracked-residency drift is worth re-emitting on the
/// `resource_drift` probe, updating the per-kind last-reported value in place. Fires
/// on the FIRST drift and whenever it moves by at least [`DRIFT_REPORT_DELTA_BYTES`]
/// since the last emission; a stable drift stays silent (no 1/sec flood); a resolved
/// drift (→ 0) silently resets so the NEXT untracked residency re-fires.
fn drift_should_report(drift: u64, last: &mut u64) -> bool {
    if drift > 0 && drift.abs_diff(*last) >= DRIFT_REPORT_DELTA_BYTES {
        *last = drift;
        true
    } else {
        if drift == 0 {
            *last = 0;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::capacity::MockCapacitySource;
    use crate::resources::consumer::{ConsumerFootprint, ReclaimRequest, ReclaimStatus};
    use std::sync::atomic::{AtomicU64, Ordering};

    // what this catches: the drift probe must fire on a MATERIAL CHANGE, not every
    // tick — a stable untracked residency logged 1/sec is the flood this fixes (live
    // 2026-07-14: 8976 identical lines). First drift fires; an unchanged/tiny-jitter
    // drift stays silent; a material move re-fires; a resolved drift resets so the
    // next residency fires again.
    #[test]
    fn drift_probe_reports_on_material_change_not_every_tick() {
        let gb: u64 = 1024 * 1024 * 1024;
        let mut last = 0u64;
        // First untracked residency → fires.
        assert!(drift_should_report(2 * gb, &mut last), "first drift fires");
        // Same drift next tick → silent (this is the 1/sec flood we're killing).
        assert!(!drift_should_report(2 * gb, &mut last), "stable drift stays silent");
        // Sub-threshold jitter → still silent.
        assert!(!drift_should_report(2 * gb + 1024 * 1024, &mut last), "1MiB jitter is not material");
        // A material move (a lane spin-up) → re-fires.
        assert!(drift_should_report(2 * gb + DRIFT_REPORT_DELTA_BYTES, &mut last), "material move re-fires");
        // Drift resolves → silent, but state resets…
        assert!(!drift_should_report(0, &mut last), "resolution is silent");
        assert_eq!(last, 0, "resolved drift resets the baseline");
        // …so a returning residency fires again.
        assert!(drift_should_report(2 * gb, &mut last), "a returning residency re-fires");
    }

    /// A scriptable consumer: holds bytes, frees per a configurable response so a
    /// scenario can make it Release / tier-down-Partial / Defer / panic on cue.
    /// This is the rung-2 fake-consumer half of the test ladder (the
    /// `CapacitySource` mock is the capacity half).
    ///
    /// The ledger reclaims **whole leases** — `request.target_bytes` is the whole
    /// victim lease, not a fraction. A consumer that wants to keep some bytes
    /// answers the whole-lease ask with a *Partial* tier-down: it frees a chunk
    /// of its own choosing and reports exactly that. The modes model the four
    /// honest responses to that ask:
    /// - `"release"` — free everything asked (full eviction).
    /// - `"partial"` — tier-down: free a fixed `partial_chunk`, keep the rest.
    /// - `"defer"`   — free nothing now (draining); patient backpressure.
    /// - `"panic"`   — a broken consumer; must be isolated + quarantined.
    struct ScriptedConsumer {
        id: String,
        held: AtomicU64,
        /// Bytes a `"partial"` reclaim frees per ask — the tier-down step size.
        partial_chunk: AtomicU64,
        mode: Mutex<&'static str>,
    }

    impl ScriptedConsumer {
        fn new(id: &str, held: u64, mode: &'static str) -> Arc<Self> {
            Arc::new(Self {
                id: id.into(),
                held: AtomicU64::new(held),
                partial_chunk: AtomicU64::new(0),
                mode: Mutex::new(mode),
            })
        }
        fn with_partial_chunk(self: Arc<Self>, chunk: u64) -> Arc<Self> {
            self.partial_chunk.store(chunk, Ordering::SeqCst);
            self
        }
        fn set_mode(&self, m: &'static str) {
            *self.mode.lock() = m;
        }
        fn held(&self) -> u64 {
            self.held.load(Ordering::SeqCst)
        }
        fn free_bytes(&self, want: u64) -> u64 {
            let before = self.held();
            let freed = before.min(want);
            self.held.store(before - freed, Ordering::SeqCst);
            freed
        }
    }

    #[async_trait::async_trait]
    impl ResourceConsumer for ScriptedConsumer {
        fn consumer_id(&self) -> &str {
            &self.id
        }
        fn footprint(&self) -> Vec<ConsumerFootprint> {
            vec![ConsumerFootprint {
                kind: ResourceKind::Vram,
                bytes: self.held(),
                detail: "scripted".into(),
            }]
        }
        async fn reclaim(&self, request: ReclaimRequest) -> ReclaimOutcome {
            let mode = *self.mode.lock();
            match mode {
                "panic" => panic!("scripted panic"),
                "defer" => ReclaimOutcome {
                    freed_bytes: 0,
                    status: ReclaimStatus::Deferred,
                    detail: Some("draining".into()),
                },
                "partial" => {
                    // Tier-down: free a fixed chunk of the whole-lease ask, keep
                    // the rest alive at lower fidelity.
                    let freed = self.free_bytes(self.partial_chunk.load(Ordering::SeqCst));
                    ReclaimOutcome {
                        freed_bytes: freed,
                        status: ReclaimStatus::Partial,
                        detail: Some("tier-down".into()),
                    }
                }
                _ => {
                    // Full release: free everything asked (the whole lease).
                    let freed = self.free_bytes(request.target_bytes);
                    ReclaimOutcome::released(freed)
                }
            }
        }
    }

    fn req(consumer: &str, bytes: u64, ttl_ms: u64, policy: ReclaimPolicy) -> LeaseRequest {
        LeaseRequest {
            consumer_id: consumer.into(),
            kind: ResourceKind::Vram,
            bytes,
            ttl_ms,
            reclaim_policy: policy,
        }
    }

    // what this catches: the RAII lease guard (#56/G1) — `acquire_guarded` reserves
    // bytes that show on the board, and dropping the guard returns them WITHOUT a
    // manual `release`. This is the primitive the ephemeral eval lane holds for its
    // lifetime; if Drop didn't release, every eval would permanently leak its VRAM
    // reservation off the board and the box would starve after a few benchmarks.
    // Also pins the honest refusal: a request past the ceiling is InsufficientCapacity,
    // not an over-grant (the "pressure, not OOM" answer the caller spills to CPU on).
    #[tokio::test]
    async fn lease_guard_reserves_then_releases_on_drop() {
        let src = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 10_000));
        let daemon = ResourceDaemon::start(
            vec![src],
            vec![],
            DaemonConfig {
                tick_interval: Duration::from_millis(20),
                min_reclaim_budget: Duration::from_millis(100),
                governor: GovernorConfig { min_dwell_ms: 0, graceful_grace_ms: 50 },
            },
        );
        let vram_available = |d: &ResourceDaemon| {
            d.board()
                .kinds
                .iter()
                .find(|k| k.kind == ResourceKind::Vram)
                .map(|k| k.available_bytes)
                .unwrap_or(0)
        };
        assert_eq!(vram_available(&daemon), 10_000, "starts with the full ceiling free");

        {
            let guard = daemon
                .acquire_guarded(&req("eval-lane", 6_000, 60_000, ReclaimPolicy::Pinned))
                .expect("6GB fits under the 10GB ceiling");
            assert_eq!(guard.bytes(), 6_000, "guard reports the held bytes");
            assert_eq!(daemon.board().leases.len(), 1, "the reservation is on the board");
            assert_eq!(vram_available(&daemon), 4_000, "available drops by the held bytes");

            // A second ask that exceeds the remaining 4GB is refused HONESTLY (the
            // caller spills to CPU on exactly this) — never an over-grant.
            match daemon.acquire_guarded(&req("eval-lane-2", 5_000, 60_000, ReclaimPolicy::Pinned)) {
                Err(LeaseError::InsufficientCapacity { available, requested, .. }) => {
                    assert_eq!(available, 4_000);
                    assert_eq!(requested, 5_000);
                }
                Err(e) => panic!("expected InsufficientCapacity, got a different error: {e:?}"),
                Ok(_) => panic!("expected InsufficientCapacity — the board over-granted past its ceiling"),
            }
        } // guard drops here → release

        assert_eq!(daemon.board().leases.len(), 0, "drop released the reservation — no leak");
        assert_eq!(vram_available(&daemon), 10_000, "the full ceiling is free again after drop");
    }

    // Poll the board until a predicate holds or we exhaust attempts (the daemon
    // ticks asynchronously). Keeps tests deterministic without sleeping a fixed
    // wall-clock budget that would flake on slow CI.
    async fn wait_until(daemon: &ResourceDaemon, mut pred: impl FnMut(&LeaseBoard) -> bool) -> bool {
        for _ in 0..200 {
            if pred(&daemon.board()) {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        false
    }

    // what this catches: rung-2 end-to-end — the async daemon reads a capacity
    // source, detects an over-budget condition when the scan shrinks under live
    // grants, drives the consumer's async reclaim CONCURRENTLY off its tick, and
    // folds the freed bytes back so the board settles within the ceiling. The
    // ledger reclaims the whole lease; the consumer here fully releases it. This
    // is the whole shell over the (separately unit-tested) deterministic
    // governor: if the scan→reconcile→reclaim→apply wiring is wrong, it is wrong
    // here.
    #[tokio::test]
    async fn daemon_full_release_claws_back_to_ceiling() {
        let src = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 10_000));
        let serving = ScriptedConsumer::new("serving", 8_000, "release");
        let daemon = ResourceDaemon::start(
            vec![src.clone()],
            vec![serving.clone()],
            DaemonConfig {
                tick_interval: Duration::from_millis(20),
                min_reclaim_budget: Duration::from_millis(100),
                governor: GovernorConfig { min_dwell_ms: 0, graceful_grace_ms: 50 },
            },
        );

        // serving leases 8GB (graceful) — within the 10GB ceiling.
        daemon
            .acquire(&req("serving", 8_000, 60_000, ReclaimPolicy::Graceful))
            .unwrap();
        assert_eq!(daemon.board().leases.len(), 1);

        // A scan shrinks VRAM to 5GB (a game grabbed 5GB) → over budget. The
        // ledger's only victim is the whole 8GB lease; serving fully releases it.
        src.set_ceiling(5_000);

        let settled =
            wait_until(&daemon, |b| b.leases.iter().map(|l| l.bytes).sum::<u64>() == 0).await;
        assert!(settled, "daemon should drive the reclaim and free the lease");
        assert_eq!(serving.held(), 0, "consumer actually freed its bytes");
        assert!(!daemon.is_over_budget(), "back within budget after reclaim");
    }

    // what this catches: the tier-down path — a consumer answers the whole-lease
    // ask by freeing only a chunk (Partial) and living on smaller. The daemon
    // must fold the honest partial delta (lease shrinks by exactly what was
    // freed, stays alive) and then STOP asking once granted is back within the
    // ceiling — no over-reclaim, no thrash. This is adaptive quality scaling
    // (qwen-30B → qwen-7B) reduced to its byte handshake.
    #[tokio::test]
    async fn daemon_partial_tierdown_shrinks_lease_and_settles() {
        let src = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 10_000));
        // Frees a fixed 3GB chunk per ask — exactly the overage when squeezed to 5GB.
        let serving = ScriptedConsumer::new("serving", 8_000, "partial").with_partial_chunk(3_000);
        let daemon = ResourceDaemon::start(
            vec![src.clone()],
            vec![serving.clone()],
            DaemonConfig {
                tick_interval: Duration::from_millis(20),
                min_reclaim_budget: Duration::from_millis(100),
                governor: GovernorConfig { min_dwell_ms: 0, graceful_grace_ms: 50 },
            },
        );

        let lease = daemon
            .acquire(&req("serving", 8_000, 60_000, ReclaimPolicy::Graceful))
            .unwrap();
        src.set_ceiling(5_000); // 3GB over budget

        // Wait on the actual reclaim effect — the lease shrunk to its freed size —
        // not on the over-budget flag, which reads false at t=0 before the daemon
        // has noticed the squeeze.
        let settled = wait_until(&daemon, |b| {
            b.leases.iter().any(|l| l.lease_id == lease.lease_id && l.bytes == 5_000)
        })
        .await;
        assert!(settled, "tier-down should shrink the lease to its freed size");
        assert_eq!(serving.held(), 5_000, "consumer tier-down freed exactly the overage");
        assert!(!daemon.is_over_budget(), "granted back within the ceiling — settled, no thrash");
    }

    // what this catches: a Deferred reclaim is patient backpressure across the
    // async boundary — the daemon must NOT yank or drop the bytes, the lease
    // stays fully held, and a later mode-flip to release lets the next tick
    // actually reclaim. Proves the two-phase handshake survives the real timer.
    #[tokio::test]
    async fn deferred_reclaim_holds_then_releases_when_consumer_is_ready() {
        let src = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 4_000));
        let serving = ScriptedConsumer::new("serving", 0, "defer");
        let daemon = ResourceDaemon::start(
            vec![src.clone()],
            vec![serving.clone()],
            DaemonConfig {
                tick_interval: Duration::from_millis(20),
                min_reclaim_budget: Duration::from_millis(100),
                governor: GovernorConfig { min_dwell_ms: 0, graceful_grace_ms: 0 },
            },
        );

        let lease = daemon
            .acquire(&req("serving", 4_000, 60_000, ReclaimPolicy::Graceful))
            .unwrap();
        serving.held.store(4_000, Ordering::SeqCst);

        // Squeeze to 1GB → 3GB over budget, but the consumer defers.
        src.set_ceiling(1_000);
        // First wait for the daemon to NOTICE the squeeze (over_budget latches),
        // then prove across several further ticks that the deferred lease is never
        // yanked — a bounded negative assertion, not a fixed-time busy-wait.
        assert!(
            wait_until(&daemon, |_| daemon.is_over_budget()).await,
            "daemon should detect the over-budget squeeze"
        );
        for _ in 0..5 {
            tokio::time::sleep(daemon.config.tick_interval).await;
            assert_eq!(
                daemon.board().leases.iter().map(|l| l.bytes).sum::<u64>(),
                4_000,
                "deferred → bytes never yanked"
            );
        }
        assert!(daemon.is_over_budget(), "still over budget while deferring");

        // Consumer becomes ready → next tick reclaims the overage.
        serving.set_mode("release");
        let settled = wait_until(&daemon, |b| b.leases.iter().map(|l| l.bytes).sum::<u64>() <= 1_000).await;
        assert!(settled, "once ready, the daemon reclaims to the ceiling");
        let _ = lease;
    }

    // what this catches: the MEASUREMENT axis wired end-to-end through the daemon
    // tick — a consumer holding VRAM with NO lease (serving's real posture) must
    // surface on the published board as measured_bytes + an attribution, polled
    // from footprint() every tick, WITHOUT any lease and WITHOUT disturbing
    // `available`. This is the "0 tracked while the GPU is full" fix: if the
    // footprint poll or set_measured wiring is wrong, the board stays blind here.
    #[tokio::test]
    async fn footprint_poll_surfaces_measured_residency_without_a_lease() {
        let src = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 24_000));
        // Holds 18GB resident (a loaded model) but leases nothing.
        let serving = ScriptedConsumer::new("serving", 18_000, "release");
        let daemon = ResourceDaemon::start(
            vec![src.clone()],
            vec![serving.clone()],
            DaemonConfig {
                tick_interval: Duration::from_millis(20),
                min_reclaim_budget: Duration::from_millis(100),
                governor: GovernorConfig { min_dwell_ms: 0, graceful_grace_ms: 50 },
            },
        );

        // No acquire — serving holds bytes but never leased. The tick's footprint
        // poll must still attribute the residency on the board.
        let surfaced = wait_until(&daemon, |b| !b.attributions.is_empty()).await;
        assert!(surfaced, "footprint poll should attribute measured residency each tick");

        let board = daemon.board();
        assert_eq!(board.attributions.len(), 1);
        assert_eq!(board.attributions[0].consumer_id, "serving");
        assert_eq!(board.attributions[0].bytes, 18_000);
        assert_eq!(board.attributions[0].kind, ResourceKind::Vram);

        let vram = board.kinds.iter().find(|k| k.kind == ResourceKind::Vram).unwrap();
        assert_eq!(vram.granted_bytes, 0, "nothing leased");
        assert_eq!(vram.measured_bytes, 18_000, "but 18GB measured-resident");
        // available is the honest free-based remainder — capacity − granted, NOT
        // reduced by the measured residency. Measurement reports; it never reserves.
        assert_eq!(vram.available_bytes, 24_000, "available untouched by measurement");
        assert!(!daemon.is_over_budget(), "measured residency is not an over-budget condition");
    }

    // what this catches: the un-inversion wired end-to-end through the daemon tick —
    // EXTERNAL physical pressure (a game grabbing VRAM) contracts `available` even
    // with zero leases, because the tick feeds `used_bytes()` into `physical_used`
    // and `available = capacity − max(granted, physical_used)`. Before the
    // un-inversion the board would read `available == capacity` here (blind to the
    // grab) and the daemon would happily commit into memory the game already took —
    // the OOM the whole task fixes. When the grab exceeds the FIXED ceiling the
    // daemon goes over-budget off physical usage alone (no lease shrank), and since
    // none of the overage is a lease we hold there is nothing safe to reclaim — the
    // daemon stays alive and simply refuses new demand (we can't evict a game).
    #[tokio::test]
    async fn external_physical_pressure_contracts_available_without_a_lease() {
        let src = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 24_000));
        let daemon = ResourceDaemon::start(
            vec![src.clone()],
            Vec::new(),
            DaemonConfig {
                tick_interval: Duration::from_millis(20),
                min_reclaim_budget: Duration::from_millis(100),
                governor: GovernorConfig { min_dwell_ms: 0, graceful_grace_ms: 50 },
            },
        );

        // A game grabs 21GB of VRAM: physical_used = 21_000, ceiling fixed at 24_000.
        src.set_used(21_000);
        let contracted =
            wait_until(&daemon, |b| b.kinds.iter().any(|k| k.physical_used_bytes == 21_000)).await;
        assert!(contracted, "the tick must feed external physical usage onto the board");

        let board = daemon.board();
        let vram = board.kinds.iter().find(|k| k.kind == ResourceKind::Vram).unwrap();
        assert_eq!(vram.capacity_bytes, 24_000, "ceiling is fixed — the grab did not move it");
        assert_eq!(vram.granted_bytes, 0, "we hold no lease");
        assert_eq!(vram.physical_used_bytes, 21_000, "but 21GB is physically resident");
        assert_eq!(vram.external_bytes, 21_000, "all of it external — no consumer of ours claims it");
        assert_eq!(vram.available_bytes, 3_000, "available = 24k − max(0, 21k) = 3k, NOT the blind 24k");
        assert!(!daemon.is_over_budget(), "21k < 24k ceiling — tight, not over");

        // The game grabs more, past the ceiling: physical_used = 26_000 > 24_000.
        src.set_used(26_000);
        let over = wait_until(&daemon, |_| daemon.is_over_budget()).await;
        assert!(over, "physical usage over the fixed ceiling is an over-budget condition on its own");
        let board = daemon.board();
        let vram = board.kinds.iter().find(|k| k.kind == ResourceKind::Vram).unwrap();
        assert_eq!(vram.available_bytes, 0, "committed exceeds capacity → zero to commit");
        // Nothing safe to reclaim (no lease of ours) — the daemon stays alive and
        // keeps publishing, it does not thrash trying to evict a game's memory.
        assert!(daemon.board().leases.is_empty(), "no lease existed to be reclaimed");
    }

    // what this catches: a panicking consumer is isolated (catch_unwind) and
    // quarantined after the threshold — one bad leaseholder can never crash the
    // daemon task or stall reclaims for the others. The daemon stays alive and
    // keeps publishing the board.
    #[tokio::test]
    async fn panicking_consumer_is_isolated_and_quarantined() {
        let src = Arc::new(MockCapacitySource::new(ResourceKind::Vram, 4_000));
        let bad = ScriptedConsumer::new("bad", 4_000, "panic");
        let daemon = ResourceDaemon::start(
            vec![src.clone()],
            vec![bad.clone()],
            DaemonConfig {
                tick_interval: Duration::from_millis(20),
                min_reclaim_budget: Duration::from_millis(50),
                governor: GovernorConfig { min_dwell_ms: 0, graceful_grace_ms: 0 },
            },
        );
        daemon
            .acquire(&req("bad", 4_000, 60_000, ReclaimPolicy::Graceful))
            .unwrap();
        src.set_ceiling(1_000); // over budget → daemon keeps asking, consumer keeps panicking

        // Wait for the daemon to NOTICE the squeeze (a live tick processed the new
        // ceiling) — not just for the acquire-published board to show the lease.
        assert!(
            wait_until(&daemon, |_| daemon.is_over_budget()).await,
            "daemon should detect the over-budget squeeze"
        );
        // Across several panic-reclaim ticks the daemon stays alive and keeps
        // publishing a coherent board: the bad lease is still accounted (fail-loud,
        // not silently lost) and never yanked despite the consumer panicking.
        for _ in 0..5 {
            tokio::time::sleep(daemon.config.tick_interval).await;
        }
        assert!(
            daemon.board().leases.iter().any(|l| l.consumer_id == "bad"),
            "panicking consumer's bytes stay accounted, never yanked"
        );
        assert_eq!(bad.held(), 4_000, "panicking consumer freed nothing");
        assert!(daemon.is_over_budget(), "still over budget — nothing was freed");
    }
}
