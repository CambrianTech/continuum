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

use continuum_base::time::EpochMs;
use std::collections::HashMap;
use std::sync::{Arc, Weak};
use std::time::Duration;

use async_trait::async_trait;
use parking_lot::{Mutex, RwLock};
use tokio::sync::{mpsc, watch};

use crate::paging::pool::{ResourcePool, ResourcePoolEntry};
use crate::paging::PressureBroker;
use crate::runtime::daemon::{guarded, spawn_daemon, Daemon, DaemonChannel, Guarded, QuarantineLedger, DEFAULT_QUARANTINE_LIMIT};
use crate::{clog_info, clog_warn};

use super::capacity::CapacitySource;
use super::consumer::{ConsumerFootprint, ReclaimOutcome, ResourceConsumer};
use super::governor::{GovernorConfig, ResourceGovernor};
use super::lease::{LeaseError, LeaseRequest, ReclaimPolicy, ResourceKind, ResourceLease};
use super::ledger::LeaseBoard;

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
    /// The base "decide under the authority's tick" pattern (MEMORY-AUTHORITY-DAEMON):
    /// closures that run EVERY tick with the authoritative fresh board. A consumer whose
    /// job needs a memory DECISION — serving's lane plan (model / lanes / window), an eval
    /// lane's window — registers here via [`on_tick`] instead of sampling memory on its own
    /// hot path, computes its allocation from THIS board, and publishes it on its own
    /// `watch` for its subscribers. One authority owns the tick; everyone else reacts to
    /// what it decides. Grows after startup (no restart), like `consumers`; each observer is
    /// panic-isolated so one broken consumer can't abort the reconcile tick.
    tick_observers: RwLock<Vec<Arc<dyn Fn(&LeaseBoard) + Send + Sync>>>,
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
            tick_observers: RwLock::new(Vec::new()),
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

    /// Publish the governor's CURRENT board while its lock is held, so boards reach
    /// readers in the order the state changed. Card 26df: every mutation used to
    /// build its board under the lock and publish it after letting go, so two
    /// acquires racing could publish out of order. The older board, missing the
    /// newer lease, landed last, and a guard reading it saw its own grant as 0.
    /// `publish` is an atomic store plus a `watch::send`, cheap enough to hold
    /// the lock across.
    fn publish_locked(&self, governor: &ResourceGovernor) {
        self.channel.publish(governor.board());
    }

    /// Grant a lease (or fail-loud refusal). Microsecond in-memory critical
    /// section; the fresh board is published before the lock is released.
    pub fn acquire(&self, req: &LeaseRequest) -> Result<ResourceLease, LeaseError> {
        let now = EpochMs::now().millis();
        let mut g = self.governor.lock();
        let lease = g.acquire(req, now)?;
        // Publish synchronously (outside the tick) so a caller sees its lease on
        // the board immediately — the property the embedded channel exists for.
        self.publish_locked(&g);
        Ok(lease)
    }

    pub fn release(&self, lease_id: &str) -> Result<ResourceLease, LeaseError> {
        let mut g = self.governor.lock();
        let lease = g.release(lease_id)?;
        self.publish_locked(&g);
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
        let now = EpochMs::now().millis();
        let mut g = self.governor.lock();
        g.renew(lease_id, expires_at_ms, now)
    }

    pub fn reserve(&self, consumer_id: impl Into<String>, kind: ResourceKind, min_bytes: u64) {
        let mut g = self.governor.lock();
        g.reserve(consumer_id, kind, min_bytes);
    }

    /// This consumer's plannable headroom: global available minus every OTHER
    /// consumer's unmet reservation floor — the same math `acquire` enforces,
    /// exposed for BUDGETING so a planner sizes itself only into bytes it could
    /// actually be granted (#225). A short governor-lock read, non-blocking.
    pub fn available_for(&self, consumer_id: &str, kind: ResourceKind) -> u64 {
        self.governor.lock().available_for(consumer_id, kind)
    }

    /// Bytes of a kind physically resident across everyone, as last scanned. What an engine
    /// training run's footprint is measured against (peak during the run minus before it).
    pub fn physical_used(&self, kind: ResourceKind) -> u64 {
        self.governor.lock().physical_used(kind)
    }

    /// The replace-myself budget: `available_for` plus the caller's OWN resident bytes,
    /// because a consumer choosing its successor releases what it holds as part of the
    /// swap. Without this the incumbent is counted against its own replacement and the
    /// plan can only ratchet down — see
    /// [`ResourceLeaseLedger::budget_for_replacing`](super::ledger::ResourceLeaseLedger::budget_for_replacing)
    /// for the measured incident.
    pub fn budget_for_replacing(&self, consumer_id: &str, kind: ResourceKind) -> u64 {
        self.governor.lock().budget_for_replacing(consumer_id, kind)
    }

    /// Replacement budget and the leases charged by the same accounting read.
    /// Consumers must keep these together when deriving a smaller planning budget;
    /// a later board/holder read cannot justify omitting a future reservation.
    pub fn replacement_budget_snapshot(
        &self, consumer_id: &str, kind: ResourceKind,
    ) -> (u64, Vec<super::lease::ResourceLease>) {
        let governor = self.governor.lock();
        let budget = governor.budget_for_replacing(consumer_id, kind);
        let board = governor.board();
        let capacity = board.kinds.iter().find(|row| row.kind == kind)
            .map_or(0, |row| row.capacity_bytes);
        let leases: Vec<_> = board.leases.into_iter()
            .filter(|lease| lease.kind == kind && lease.consumer_id != consumer_id)
            .collect();
        let charged = leases.iter().fold(0u64, |sum, lease| sum.saturating_add(lease.bytes));
        // An oversized own-residency add-back cannot erase another holder's
        // grant. Every returned lease is excluded even before physical sampling.
        (budget.min(capacity.saturating_sub(charged)), leases)
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

    /// Register a closure to run EVERY reconcile tick with the authoritative fresh
    /// board — the base "decide under the authority's tick" pattern
    /// (MEMORY-AUTHORITY-DAEMON). This is how a consumer whose job needs a memory
    /// DECISION — serving's lane plan (model / lanes / window), an eval lane's window —
    /// computes it in the ONE place that owns memory, WITHOUT sampling memory on its own
    /// hot path: it reads the passed `&LeaseBoard` (the live truth, netted over every
    /// measured consumer + external pressure) and publishes its allocation on its own
    /// `watch` for its subscribers. Code the pattern once here; every consumer that
    /// registers gets a smarter base for free as the authority improves. No restart
    /// (registrable after startup, like [`add_consumer`]); each observer is panic-isolated
    /// on the tick.
    pub fn on_tick(&self, observer: Arc<dyn Fn(&LeaseBoard) + Send + Sync>) {
        self.tick_observers.write().push(observer);
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
        let now = EpochMs::now().millis();

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
                    continuum_base::probe!(
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

        // Fan the fresh board out to every tick-observer BEFORE publishing it — the
        // "decide under the authority's tick" pattern. A consumer's memory DECISION
        // (serving's lane plan, an eval lane's window) runs HERE, on the one authority's
        // tick, reading THIS board — never sampling memory on its own hot path. Arcs are
        // cloned out from under the lock so the (possibly non-trivial) closures never hold
        // it; each is panic-isolated so one broken consumer can't abort the reconcile tick,
        // exactly like the `footprint()` poll above.
        {
            let observers: Vec<Arc<dyn Fn(&LeaseBoard) + Send + Sync>> =
                self.tick_observers.read().iter().cloned().collect();
            for obs in &observers {
                if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| obs(&board))).is_err() {
                    clog_warn!("🧮 ResourceDaemon: a tick-observer panicked — skipped this tick");
                }
            }
        }

        // Publish so readers (and the gate) stay fresh even when calm. The CURRENT
        // board, not the one captured above: an observer may have acquired since,
        // and that newer board must not be overwritten by this older one (card 26df).
        self.publish_locked(&self.governor.lock());

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
        let mut g = self.governor.lock();
        for (lease_id, outcome) in &outcomes {
            if let Err(e) = g.apply_reclaim_outcome(lease_id, outcome) {
                clog_warn!("🧮 apply_reclaim_outcome failed for {lease_id}: {e:?}");
            }
        }
        self.publish_locked(&g);
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

    /// The bytes this guard holds, read from the governor itself (0 once the lease
    /// is released or swept). Not the published board: a board is a copy, and a copy
    /// can lag the grant it describes (card 26df: an admitted 6.1 GB train run read
    /// its own grant as 0).
    pub fn bytes(&self) -> u64 {
        self.daemon.governor.lock().lease(&self.lease_id).map(|l| l.bytes).unwrap_or(0) // unwrap_or: no lease means nothing held, the documented answer once it is released or swept
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
mod tests;
