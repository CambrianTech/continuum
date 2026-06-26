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
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

use futures::FutureExt;
use parking_lot::{Mutex, RwLock};
use tokio::sync::{mpsc, watch};

use crate::paging::pool::{ResourcePool, ResourcePoolEntry};
use crate::paging::PressureBroker;
use crate::{clog_info, clog_warn};

use super::capacity::CapacitySource;
use super::consumer::{ReclaimOutcome, ResourceConsumer};
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

/// Consecutive reclaim panics before a consumer is quarantined (skipped in
/// future ticks). Mirrors `MemoryPressureMonitor`'s reporter quarantine.
const QUARANTINE_AFTER_PANICS: u32 = 3;

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

/// A registered consumer plus its quarantine bookkeeping. The daemon owns these
/// behind a `parking_lot::RwLock` so consumers can join after startup
/// (`add_consumer`) without restarting the task.
struct ConsumerEntry {
    consumer: Arc<dyn ResourceConsumer>,
    consecutive_panics: u32,
    disabled: bool,
}

/// The single per-machine resource authority's runtime shell.
pub struct ResourceDaemon {
    /// The deterministic accounting core. `parking_lot::Mutex` because every
    /// critical section is sync and short — NEVER held across an await.
    governor: Mutex<ResourceGovernor>,
    /// Scan sources, one per managed kind. Read off-lock each tick.
    sources: Vec<Arc<dyn CapacitySource>>,
    /// Leaseholders, by registration order. Grows via `add_consumer`.
    consumers: RwLock<Vec<ConsumerEntry>>,
    /// Latest published board — hot readers borrow this, never the governor lock.
    board_tx: watch::Sender<LeaseBoard>,
    board_rx: watch::Receiver<LeaseBoard>,
    /// Lock-free "any kind over its ceiling" flag for fast reads (e.g. a
    /// consumer deciding whether to grow). Updated each tick.
    over_budget: AtomicBool,
    /// The sync→async bridge: `ResourcePool::evict_at_least` posts `(kind, want)`
    /// here and returns; the tick drains it into the reconcile demand.
    evict_tx: mpsc::UnboundedSender<(ResourceKind, u64)>,
    config: DaemonConfig,
}

impl ResourceDaemon {
    /// Start the daemon on its own tokio task. Returns the handle subsystems use
    /// to lease, subscribe to the board, and (via `register_with_broker`) wire
    /// cross-resource relief.
    pub fn start(
        sources: Vec<Arc<dyn CapacitySource>>,
        consumers: Vec<Arc<dyn ResourceConsumer>>,
        config: DaemonConfig,
    ) -> Arc<Self> {
        let mut governor = ResourceGovernor::with_default_arbiter(config.governor);
        // Prime capacity from the scan sources NOW, so the authority knows its
        // ceilings the moment it exists — a lease acquired before the first tick
        // is admitted against real capacity, not a boot-time zero.
        for src in &sources {
            governor.set_capacity(src.kind(), src.ceiling_bytes());
        }
        let (board_tx, board_rx) = watch::channel(governor.board());
        let (evict_tx, evict_rx) = mpsc::unbounded_channel();

        let entries = consumers
            .into_iter()
            .map(|c| ConsumerEntry {
                consumer: c,
                consecutive_panics: 0,
                disabled: false,
            })
            .collect();

        let daemon = Arc::new(Self {
            governor: Mutex::new(governor),
            sources,
            consumers: RwLock::new(entries),
            board_tx,
            board_rx,
            over_budget: AtomicBool::new(false),
            evict_tx,
            config,
        });

        // Own task; AssertUnwindSafe + catch_unwind wraps the whole loop so a
        // panic in non-consumer code stops the daemon cleanly rather than
        // poisoning the runtime. (Consumer panics are caught per-reclaim and
        // quarantined — they never reach here.)
        {
            let d = daemon.clone();
            tokio::spawn(async move {
                let result = AssertUnwindSafe(Self::run_loop(d, evict_rx))
                    .catch_unwind()
                    .await;
                if let Err(e) = result {
                    clog_warn!("🧮 ResourceDaemon task panicked (daemon stopped): {:?}", e);
                }
            });
        }

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
        let _ = self.board_tx.send(board);
        Ok(lease)
    }

    pub fn release(&self, lease_id: &str) -> Result<ResourceLease, LeaseError> {
        let (lease, board) = {
            let mut g = self.governor.lock();
            let lease = g.release(lease_id)?;
            (lease, g.board())
        };
        let _ = self.board_tx.send(board);
        Ok(lease)
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
        self.consumers.write().push(ConsumerEntry {
            consumer,
            consecutive_panics: 0,
            disabled: false,
        });
        clog_info!("🧮 ResourceDaemon: consumer '{id}' registered");
    }

    // ---- hot, lock-free reads ----------------------------------------------

    /// Latest board — a `watch` borrow, never the accounting lock.
    pub fn board(&self) -> LeaseBoard {
        self.board_rx.borrow().clone()
    }

    /// Subscribe to board changes (for `resources/*` watch commands / grid).
    pub fn subscribe(&self) -> watch::Receiver<LeaseBoard> {
        self.board_rx.clone()
    }

    /// Is any kind currently over its scanned ceiling? Lock-free atomic read.
    pub fn is_over_budget(&self) -> bool {
        self.over_budget.load(Ordering::Relaxed)
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

    // ---- the tick -----------------------------------------------------------

    async fn run_loop(
        daemon: Arc<Self>,
        mut evict_rx: mpsc::UnboundedReceiver<(ResourceKind, u64)>,
    ) {
        let mut ticker = tokio::time::interval(daemon.config.tick_interval);
        // Coalesce late ticks — never build a backlog of reconciles.
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            ticker.tick().await;
            daemon.tick(&mut evict_rx).await;
        }
    }

    /// One reconcile cycle. Structured so the governor lock is taken twice, each
    /// time briefly and never across an await; everything async (the consumer
    /// reclaims) happens between, lock-free.
    async fn tick(&self, evict_rx: &mut mpsc::UnboundedReceiver<(ResourceKind, u64)>) {
        let now = now_ms();

        // 1. Drain broker evict asks (non-blocking) into a per-kind demand. Take
        //    the largest ask per kind this tick; it is a one-shot relief request
        //    — if the pool still needs room the broker re-asks on its own tick.
        let mut demand: HashMap<ResourceKind, u64> = HashMap::new();
        while let Ok((kind, want)) = evict_rx.try_recv() {
            let e = demand.entry(kind).or_insert(0);
            *e = (*e).max(want);
        }

        // 2. Read every ceiling OFF the accounting lock (cached monitor reads).
        let ceilings: Vec<(ResourceKind, u64)> = self
            .sources
            .iter()
            .map(|s| (s.kind(), s.ceiling_bytes()))
            .collect();

        // 3. Brief lock: ingest capacity, compute per-kind pressure, plan the
        //    reclaims, snapshot the board + over-budget flag. No await inside.
        let (plans, board, over) = {
            let mut g = self.governor.lock();
            for (kind, ceil) in &ceilings {
                g.set_capacity(*kind, *ceil);
            }

            // Per-kind contention for the arbiter: granted / ceiling, clamped to
            // the [0,1] the arbiter contract expects. Precomputed into a Copy
            // array so the reconcile closures don't borrow the guard.
            let mut pmap = [0.0f64; 3];
            for kind in ResourceKind::ALL {
                let ceil = g.capacity(kind);
                if ceil > 0 {
                    pmap[kind_idx(kind)] = (g.granted(kind) as f64 / ceil as f64).clamp(0.0, 1.0);
                }
            }

            let plans = g.reconcile_for_demand(
                now,
                |k| pmap[kind_idx(k)],
                |k| demand.get(&k).copied().unwrap_or(0),
            );
            (plans, g.board(), is_over(&g))
        };

        // Publish board + flag every tick so readers stay fresh even when calm.
        self.over_budget.store(over, Ordering::Relaxed);
        let _ = self.board_tx.send(board);

        if plans.is_empty() {
            return;
        }

        // 4. Snapshot live consumers (clone Arcs) under a brief read lock, then
        //    fan out the reclaims CONCURRENTLY off-lock. A plan for an
        //    unregistered/quarantined consumer is logged, never silently dropped.
        let live: Vec<(String, Arc<dyn ResourceConsumer>)> = {
            let cs = self.consumers.read();
            cs.iter()
                .filter(|e| !e.disabled)
                .map(|e| (e.consumer.consumer_id().to_string(), e.consumer.clone()))
                .collect()
        };

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

            futs.push(async move {
                let res =
                    tokio::time::timeout(budget, AssertUnwindSafe(consumer.reclaim(req)).catch_unwind())
                        .await;
                (consumer_id, lease_id, res)
            });
        }

        let results = futures::future::join_all(futs).await;

        // 5. Fold results: collect honest outcomes, update quarantine counters.
        let mut outcomes: Vec<(String, ReclaimOutcome)> = Vec::new();
        {
            let mut cs = self.consumers.write();
            for (consumer_id, lease_id, res) in results {
                match res {
                    // Reclaim returned a value (any status — Released/Partial/
                    // Deferred/Refused). The byte delta is authoritative.
                    Ok(Ok(outcome)) => {
                        reset_panics(&mut cs, &consumer_id);
                        outcomes.push((lease_id, outcome));
                    }
                    // Consumer panicked — isolated by catch_unwind. Count toward
                    // quarantine; its bytes stay held and re-surface next tick.
                    Ok(Err(_panic)) => {
                        let disabled = bump_panic(&mut cs, &consumer_id);
                        clog_warn!(
                            "🧮 ResourceConsumer '{consumer_id}' panicked during reclaim of \
                             {lease_id}{}",
                            if disabled { " — quarantined" } else { "" }
                        );
                    }
                    // Timed out — honest backpressure, NOT a panic (don't
                    // quarantine). Bytes stay held; next tick re-plans the ask.
                    Err(_elapsed) => {
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

        // 6. Brief lock: apply the byte deltas, recompute the over-budget flag
        //    against the POST-reclaim grants (so a freed lease clears the flag in
        //    the same tick, not one tick late), publish the reconciled board.
        let (board, over) = {
            let mut g = self.governor.lock();
            for (lease_id, outcome) in &outcomes {
                if let Err(e) = g.apply_reclaim_outcome(lease_id, outcome) {
                    clog_warn!("🧮 apply_reclaim_outcome failed for {lease_id}: {e:?}");
                }
            }
            (g.board(), is_over(&g))
        };
        self.over_budget.store(over, Ordering::Relaxed);
        let _ = self.board_tx.send(board);
    }
}

/// True if any kind currently holds more than its scanned ceiling. Shared by the
/// tick's plan phase and its post-reclaim recompute so both read the flag the
/// same way.
fn is_over(g: &ResourceGovernor) -> bool {
    ResourceKind::ALL
        .iter()
        .any(|&k| g.granted(k) > g.capacity(k))
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
        self.daemon
            .upgrade()
            .map(|d| d.governor.lock().granted(self.kind))
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

fn reset_panics(cs: &mut [ConsumerEntry], consumer_id: &str) {
    if let Some(e) = cs.iter_mut().find(|e| e.consumer.consumer_id() == consumer_id) {
        e.consecutive_panics = 0;
    }
}

/// Increment the consumer's panic counter; returns true if it just crossed into
/// quarantine.
fn bump_panic(cs: &mut [ConsumerEntry], consumer_id: &str) -> bool {
    if let Some(e) = cs.iter_mut().find(|e| e.consumer.consumer_id() == consumer_id) {
        e.consecutive_panics += 1;
        if e.consecutive_panics >= QUARANTINE_AFTER_PANICS && !e.disabled {
            e.disabled = true;
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resources::capacity::MockCapacitySource;
    use crate::resources::consumer::{ConsumerFootprint, ReclaimRequest, ReclaimStatus};
    use std::sync::atomic::AtomicU64;

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
