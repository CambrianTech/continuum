//! The canonical daemon base — one RTOS loop, written once, for every monitor.
//!
//! # Why this exists
//!
//! [CONCURRENCY-STYLE-GUIDE.md] tells every new monitor to *"copy
//! `MemoryPressureMonitor`"* — own tokio task + `tokio::time::interval` +
//! `watch::Sender<Snapshot>` + atomic gate + `spawn_blocking`/timeout/quarantine.
//! That instruction was correct about the *shape* but wrong about the *mechanism*:
//! copying a 1000-line file per concern meant every standalone monitor re-hand-
//! rolled the loop and they drifted. They had. The old `MemoryPressureMonitor`
//! ticked with `loop { sleep().await }` (drifts under load — forbidden move #3) and
//! wrapped the *whole loop* in `catch_unwind` (one panic killed the monitor
//! forever); `ResourceDaemon` got `interval` + `Skip` right but also wrapped the
//! whole loop; `MetalMonitor` and the (since-deleted) `PressureBroker::spawn_tick`
//! had no isolation at all. Several copies, several behaviours, one buggy. All four
//! standalone monitors now ride this base; `spawn_tick` was dead and is gone.
//!
//! Note the deliberate scope: this base is for **standalone monitor concerns** that
//! own a tick loop and publish a [`DaemonChannel`] snapshot read lock-free by many
//! consumers. A `ServiceModule` that declares a `tick_interval` is the *other*
//! correct shape — its tick is driven by the runtime's `start_tick_loops` /
//! `run_tick_loop_for` runner, which is the module analog of [`spawn_daemon`] and
//! already gives the identical RTOS guarantees (`interval` + `Skip` + per-tick
//! `catch_unwind` + adaptive cadence). So `PressureBrokerModule` /
//! `ServingDaemonModule` are NOT on this base — they're already isolated by the
//! module runner. Two runners, one shape, no third hand-rolled copy.
//!
//! This module is the daemon analog of the command-registry collapse: as commands
//! collapsed onto one `ActionCommand` → `DynCommand` base + one runner, daemons
//! collapse onto one [`Daemon`] trait + one [`spawn_daemon`] runner. A daemon
//! declares its *concern* (cadence, the channel it publishes on, how to produce a
//! snapshot); the runner owns the *loop* (interval, panic isolation, drive) —
//! written once, correctly, and strictly better than every copy:
//!
//! - `tokio::time::interval` + `MissedTickBehavior::Skip` (never `sleep`, never a
//!   backlog of late ticks).
//! - **Per-tick** `catch_unwind`: a transient panic in one tick loses *that tick*,
//!   not the daemon. `parking_lot` locks don't poison and the guard drops on
//!   unwind, so the next tick runs against the last-good published snapshot.
//! - One [`DaemonChannel`] (watch + atomic gate) exposed through a uniform
//!   [`DaemonHandle`], so every reader (`resources/*` commands, the cognition hot
//!   path, grid gossip) talks to every daemon the same way.
//!
//! This is **not** CONCURRENCY-STYLE-GUIDE forbidden move #6 (a parallel
//! manager/coordinator). It is the missing *base* the "copy this file" instruction
//! is a workaround for — the architect's directed unification, exactly like the
//! command collapse.
//!
//! # Publish from anywhere, not just the tick
//!
//! The channel is *embedded in the daemon*, not handed only to `tick`. A pure
//! poller (`MemoryPressureMonitor`, `DiskPressureMonitor`, `MetalMonitor`)
//! publishes only from its tick. A hybrid authority like `ResourceDaemon`
//! publishes its board from the tick AND from synchronous `acquire`/`release`
//! lease ops (a caller must see its lease on the board immediately, before the
//! next tick). Both call `self.channel().publish(..)` — the embedded channel is
//! what makes the hybrid fit the same base as the poller without forcing.
//!
//! # The reclaim/report kernel
//!
//! Both fan-out daemons also call fallible, possibly-slow callees —
//! `MemoryReporter`s, `ResourceConsumer`s — bounded by a timeout, isolated by
//! `catch_unwind`, quarantined after N consecutive failures. That sub-pattern is
//! [`guarded`] + [`QuarantineLedger`] here, so neither site re-implements the
//! 3-strikes bookkeeping or the panic/timeout classification.
//!
//! [CONCURRENCY-STYLE-GUIDE.md]: ../../../../docs/architecture/CONCURRENCY-STYLE-GUIDE.md

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use futures::FutureExt;
use tokio::sync::watch;

use crate::clog_warn;

/// Consecutive failures before a fanned-out callee (reporter/consumer) is
/// quarantined. Mirrors the threshold both canonical daemons hand-rolled. Code,
/// not env.
pub const DEFAULT_QUARANTINE_LIMIT: u32 = 3;

/// A long-lived monitor concern. The implementor owns its own state (behind
/// interior mutability) and a [`DaemonChannel`], and declares: how often to tick,
/// which channel it publishes on, and how to produce the next snapshot. The
/// runner ([`spawn_daemon`]) owns everything else — the loop, the clock, and
/// per-tick panic isolation.
///
/// Object-safety is not required: the runner is generic over `D: Daemon`, so the
/// native-async tick is monomorphized. `#[async_trait]` keeps the `async fn tick`
/// ergonomic and consistent with the project's other async traits
/// (`ResourceConsumer`, `MemoryReporter`).
#[async_trait]
pub trait Daemon: Send + Sync + 'static {
    /// The state this daemon publishes. Cloned on every reader `borrow`, so keep
    /// it cheap (an `Arc` payload or a small struct), per the guide.
    type Snapshot: Clone + Send + Sync + 'static;

    /// Stable name for logs and probes (e.g. `"memory-pressure"`, `"resources"`).
    fn name(&self) -> &'static str;

    /// Tick cadence. Const-derived per the cadence ladder, never an env var. The
    /// runner applies `MissedTickBehavior::Skip`, so a slow tick coalesces rather
    /// than building a backlog.
    fn cadence(&self) -> Duration;

    /// The daemon's embedded publish channel. The runner mints the public
    /// [`DaemonHandle`] from it; the daemon publishes through it from `tick` and
    /// from any synchronous method that mutates published state.
    fn channel(&self) -> &DaemonChannel<Self::Snapshot>;

    /// Produce (and publish, via `self.channel()`) the next state. Runs on the
    /// daemon's own task inside a per-tick `catch_unwind`. A daemon may publish
    /// more than once per tick (e.g. a pre-reclaim board and a post-reclaim
    /// board). MUST NOT hold a lock across an internal `.await` (the guide's
    /// forbidden move #7).
    async fn tick(&self);
}

/// A daemon's embedded publish channel: a `watch` channel paired with a lock-free
/// gate derived from each published snapshot. Constructed by the daemon (so it
/// can publish from synchronous methods, not only from `tick`), and read through
/// the [`DaemonHandle`] it mints. The gate derivation is a pure `Fn(&S) -> bool`
/// supplied at construction (e.g. `|s| s.level == Critical`, `|b| b.over_budget`)
/// — code, never an env threshold.
pub struct DaemonChannel<S> {
    tx: watch::Sender<S>,
    rx: watch::Receiver<S>,
    gate: Arc<AtomicBool>,
    gate_of: Arc<dyn Fn(&S) -> bool + Send + Sync>,
}

impl<S: Clone + Send + Sync + 'static> DaemonChannel<S> {
    /// New channel seeded with `initial` (so a reader borrowing at t=0 is
    /// coherent) and a pure gate derivation applied to every published snapshot.
    pub fn new(initial: S, gate_of: impl Fn(&S) -> bool + Send + Sync + 'static) -> Self {
        let gated = gate_of(&initial);
        let (tx, rx) = watch::channel(initial);
        Self {
            tx,
            rx,
            gate: Arc::new(AtomicBool::new(gated)),
            gate_of: Arc::new(gate_of),
        }
    }

    /// Convenience for daemons whose snapshot never gates (pure telemetry).
    pub fn ungated(initial: S) -> Self {
        Self::new(initial, |_| false)
    }

    /// Publish a fresh snapshot: recompute the gate and notify all subscribers.
    /// Cheap (an atomic store + a `watch::send`); call it from `tick` or from any
    /// synchronous method that has genuinely-new state to surface.
    pub fn publish(&self, snapshot: S) {
        let gated = (self.gate_of)(&snapshot);
        self.gate.store(gated, Ordering::Relaxed);
        // `send` only errs if every receiver dropped — the channel holds one, so
        // this is effectively infallible while the daemon is referenced.
        let _ = self.tx.send(snapshot);
    }

    /// Mint a reader handle. The runner calls this once; daemons may also hand
    /// clones to subsystems.
    pub fn handle(&self) -> DaemonHandle<S> {
        DaemonHandle {
            rx: self.rx.clone(),
            gate: self.gate.clone(),
        }
    }

    /// The latest published snapshot (daemon-side read; same value the handle
    /// sees).
    pub fn snapshot(&self) -> S {
        self.rx.borrow().clone()
    }

    /// The current gate value (daemon-side read).
    pub fn is_gated(&self) -> bool {
        self.gate.load(Ordering::Relaxed)
    }
}

/// The uniform reader surface for any [`Daemon`]. Every daemon, regardless of
/// concern, hands callers one of these — so subscribing to memory pressure and
/// subscribing to the lease board read identically. Cloneable and lock-free.
#[derive(Clone)]
pub struct DaemonHandle<S> {
    rx: watch::Receiver<S>,
    gate: Arc<AtomicBool>,
}

impl<S: Clone> DaemonHandle<S> {
    /// A new receiver for change-driven consumers (`watch::Receiver::changed`).
    pub fn subscribe(&self) -> watch::Receiver<S> {
        self.rx.clone()
    }

    /// The latest published snapshot — a `watch` borrow + clone, never a daemon
    /// lock. Safe to call on any thread, including the cognition hot path.
    pub fn snapshot(&self) -> S {
        self.rx.borrow().clone()
    }

    /// The lock-free gate: `true` when the daemon's latest snapshot means
    /// callers should back off (memory critical, resources over budget). A
    /// single relaxed atomic load.
    pub fn is_gated(&self) -> bool {
        self.gate.load(Ordering::Relaxed)
    }
}

/// Spawn a [`Daemon`] on its own tokio task and return its [`DaemonHandle`].
///
/// This is the one place the RTOS loop is written. It:
/// 1. mints the handle from the daemon's embedded channel (already seeded with
///    `initial`, so readers are coherent before the first tick),
/// 2. drives `daemon.tick` on a `tokio::time::interval` with
///    `MissedTickBehavior::Skip` (the guide's mandate — never `sleep`),
/// 3. wraps **each tick** in `AssertUnwindSafe(...).catch_unwind()` so a panicking
///    tick is isolated and the daemon keeps ticking against last-good state.
///
/// The task lives as long as the daemon `Arc` it captures; it is a
/// fire-and-forget substrate task, like the monitors it replaces.
pub fn spawn_daemon<D: Daemon>(daemon: Arc<D>) -> DaemonHandle<D::Snapshot> {
    let handle = daemon.channel().handle();
    let name = daemon.name();
    let cadence = daemon.cadence();

    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(cadence);
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            // Per-tick isolation: a panic loses this tick, never the daemon.
            let result = AssertUnwindSafe(daemon.tick()).catch_unwind().await;
            if let Err(e) = result {
                clog_warn!(
                    "⏱️ daemon '{name}' tick panicked (isolated, continuing): {:?}",
                    e
                );
            }
        }
    });

    handle
}

/// The outcome of a [`guarded`] call to a fallible, possibly-slow callee.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Guarded<T> {
    /// The future completed within budget and did not panic.
    Completed(T),
    /// The future panicked — isolated by `catch_unwind`. Caller should count it
    /// toward quarantine.
    Panicked,
    /// The future exceeded its time budget — honest backpressure, NOT a fault.
    /// Caller should re-ask, not quarantine.
    TimedOut,
}

/// Run a callee's async work bounded by `budget` and isolated by `catch_unwind`,
/// classifying the result so the caller can treat a panic (fault → quarantine)
/// differently from a timeout (backpressure → re-ask). This is the kernel both
/// the reporter fan-out (`MemoryPressureMonitor`) and the consumer reclaim
/// fan-out (`ResourceDaemon`) share. The 100ms reporter budget and the
/// per-request reclaim deadline are both just the `budget` passed here.
pub async fn guarded<T>(budget: Duration, fut: impl Future<Output = T>) -> Guarded<T> {
    match tokio::time::timeout(budget, AssertUnwindSafe(fut).catch_unwind()).await {
        Ok(Ok(value)) => Guarded::Completed(value),
        Ok(Err(_panic)) => Guarded::Panicked,
        Err(_elapsed) => Guarded::TimedOut,
    }
}

/// Three-strikes quarantine bookkeeping for fanned-out callees, keyed by stable
/// id. A callee that panics `limit` consecutive times is quarantined (skipped in
/// future fan-outs) until something resets it; any success clears the streak.
/// Replaces the `consecutive_panics`/`bump_panic`/`reset_panics` triad both
/// canonical daemons hand-rolled, so the policy lives in exactly one place.
///
/// Timeouts are deliberately NOT failures here — they are patient backpressure;
/// the caller records them as neither success nor failure (leaves the streak
/// untouched), matching the canonical daemons' "re-ask, don't quarantine".
#[derive(Debug, Default)]
pub struct QuarantineLedger {
    limit: u32,
    streak: HashMap<String, u32>,
    quarantined: HashSet<String>,
}

impl QuarantineLedger {
    /// New ledger with the given consecutive-failure limit (use
    /// [`DEFAULT_QUARANTINE_LIMIT`] unless a concern needs otherwise).
    pub fn new(limit: u32) -> Self {
        Self {
            limit,
            streak: HashMap::new(),
            quarantined: HashSet::new(),
        }
    }

    /// Is this callee currently quarantined (should be skipped)?
    pub fn is_quarantined(&self, id: &str) -> bool {
        self.quarantined.contains(id)
    }

    /// Record a success — clears any failure streak (but does NOT un-quarantine;
    /// a quarantined callee stays out until explicitly cleared, the conservative
    /// default both daemons use).
    pub fn record_success(&mut self, id: &str) {
        self.streak.remove(id);
    }

    /// Record a failure (a panic). Returns `true` if this failure just crossed
    /// the threshold into quarantine (so the caller can log the transition once).
    pub fn record_failure(&mut self, id: &str) -> bool {
        let count = self.streak.entry(id.to_string()).or_insert(0);
        *count += 1;
        if *count >= self.limit && !self.quarantined.contains(id) {
            self.quarantined.insert(id.to_string());
            return true;
        }
        false
    }

    /// Explicitly clear a quarantine (e.g. a consumer re-registered). Resets both
    /// the streak and the quarantined flag.
    pub fn clear(&mut self, id: &str) {
        self.streak.remove(id);
        self.quarantined.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A deterministic, hardware-free daemon: each tick increments a counter and
    /// publishes it; it gates at `>= gate_at` and panics exactly on the tick whose
    /// count equals `panic_at` (0 = never). This is the no-hardware stand-in that
    /// exercises the runner's loop, the embedded channel's publish+gate, and
    /// per-tick panic isolation without any monitor's real device scan.
    struct CountingDaemon {
        channel: DaemonChannel<u64>,
        count: AtomicU64,
        panic_at: u64,
        cadence: Duration,
    }

    impl CountingDaemon {
        fn new(gate_at: u64, panic_at: u64, cadence_ms: u64) -> Arc<Self> {
            Arc::new(Self {
                channel: DaemonChannel::new(0, move |n: &u64| *n >= gate_at),
                count: AtomicU64::new(0),
                panic_at,
                cadence: Duration::from_millis(cadence_ms),
            })
        }
    }

    #[async_trait]
    impl Daemon for CountingDaemon {
        type Snapshot = u64;
        fn name(&self) -> &'static str {
            "counting"
        }
        fn cadence(&self) -> Duration {
            self.cadence
        }
        fn channel(&self) -> &DaemonChannel<u64> {
            &self.channel
        }
        async fn tick(&self) {
            let n = self.count.fetch_add(1, Ordering::SeqCst) + 1;
            self.channel.publish(n);
            if self.panic_at != 0 && n == self.panic_at {
                panic!("scripted tick panic at {n}");
            }
        }
    }

    async fn wait_until<S: Clone>(
        handle: &DaemonHandle<S>,
        mut pred: impl FnMut(&S) -> bool,
    ) -> bool {
        for _ in 0..200 {
            if pred(&handle.snapshot()) {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        false
    }

    // what this catches: the runner actually drives the loop — it mints the handle
    // from the embedded channel (seeded initial visible at t=0), ticks on the
    // interval, publishes each new snapshot through the channel, and derives the
    // gate from the channel's gate fn. If the interval/publish/gate wiring is
    // wrong, the snapshot never climbs or the gate never flips. This is the whole
    // point of the base: one correct loop.
    #[tokio::test]
    async fn runner_ticks_publishes_and_derives_gate() {
        let daemon = CountingDaemon::new(
            /*gate_at*/ 3, /*panic_at*/ 0, /*cadence_ms*/ 5,
        );
        let handle = spawn_daemon(daemon);

        // Seeded initial value is visible before any tick, never a panic/empty.
        assert_eq!(handle.snapshot(), 0);
        assert!(!handle.is_gated());

        // The loop climbs the counter and flips the gate at the threshold.
        assert!(
            wait_until(&handle, |n| *n >= 3).await,
            "loop should advance"
        );
        assert!(handle.is_gated(), "gate derives from snapshot >= gate_at");
    }

    // what this catches: per-tick panic isolation — the single most important
    // improvement over the copied pattern. The daemon panics on tick #2; if the
    // runner wrapped the whole loop (the old shape) the task would die at 2 and the
    // counter would freeze. Because each tick is isolated, the daemon survives and
    // keeps climbing well past the panic.
    #[tokio::test]
    async fn panicking_tick_is_isolated_daemon_keeps_running() {
        let daemon = CountingDaemon::new(
            /*gate_at*/ u64::MAX,
            /*panic_at*/ 2,
            /*cadence_ms*/ 5,
        );
        let handle = spawn_daemon(daemon);

        // Reaching 5 is only possible if the daemon survived the panic at 2.
        assert!(
            wait_until(&handle, |n| *n >= 5).await,
            "daemon must keep ticking past the panicking tick"
        );
    }

    // what this catches: the embedded channel publishes from a SYNCHRONOUS method,
    // not only from tick — the property that lets a hybrid authority (ResourceDaemon)
    // surface a lease on its board immediately on acquire, before the next tick.
    // A sink handed only to tick could not do this.
    #[tokio::test]
    async fn channel_publishes_from_outside_the_tick() {
        let channel = DaemonChannel::new(0u64, |n: &u64| *n >= 100);
        let handle = channel.handle();
        assert_eq!(handle.snapshot(), 0);
        channel.publish(42);
        assert_eq!(
            handle.snapshot(),
            42,
            "synchronous publish is visible at once"
        );
        assert!(!handle.is_gated());
        channel.publish(200);
        assert!(
            handle.is_gated(),
            "gate tracks the synchronously-published value"
        );
    }

    // what this catches: the fan-out kernel classifies the three outcomes a
    // fallible callee can have — completed value, panic (fault), timeout
    // (backpressure) — so a caller can quarantine on panic but merely re-ask on
    // timeout. Conflating timeout with panic would quarantine a merely-slow
    // consumer; conflating panic with completion would trust a crashed one.
    #[tokio::test]
    async fn guarded_classifies_complete_panic_and_timeout() {
        assert_eq!(
            guarded(Duration::from_millis(100), async { 7u32 }).await,
            Guarded::Completed(7)
        );
        assert_eq!(
            guarded(Duration::from_millis(100), async { panic!("boom") }).await,
            Guarded::<()>::Panicked
        );
        assert_eq!(
            guarded(Duration::from_millis(10), async {
                tokio::time::sleep(Duration::from_secs(10)).await;
            })
            .await,
            Guarded::<()>::TimedOut
        );
    }

    // what this catches: three-strikes quarantine bookkeeping — a callee crosses
    // into quarantine on exactly the Nth consecutive failure (the transition
    // reported once), a success resets the streak so failures must be CONSECUTIVE,
    // and an explicit clear lifts the quarantine. This is the policy both daemons
    // hand-rolled; centralizing it means they can't drift on the threshold.
    #[test]
    fn quarantine_ledger_trips_on_consecutive_failures_only() {
        let mut q = QuarantineLedger::new(3);

        assert!(!q.record_failure("x"), "1st failure: not yet quarantined");
        assert!(!q.record_failure("x"), "2nd failure: not yet");
        // A success resets the streak — failures must be consecutive.
        q.record_success("x");
        assert!(!q.is_quarantined("x"), "reset clears the streak");

        assert!(!q.record_failure("x"));
        assert!(!q.record_failure("x"));
        assert!(
            q.record_failure("x"),
            "3rd consecutive failure trips quarantine (transition reported once)"
        );
        assert!(q.is_quarantined("x"));
        // Re-crossing doesn't re-report the transition.
        assert!(!q.record_failure("x"), "already quarantined: no re-report");

        q.clear("x");
        assert!(!q.is_quarantined("x"), "explicit clear lifts quarantine");
    }
}
