//! Serving-lane admission: lane pools, ambient turn permits and directed-turn reservation.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// Try to claim an ambient-turn slot. `Some(permit)` → run the ambient turn (hold the
/// permit for the turn's lifetime; it releases on drop). `None` → all ambient slots
/// are busy; the caller yields this ambient turn. Non-blocking (never waits).
pub fn try_hold_ambient_turn() -> Option<tokio::sync::OwnedSemaphorePermit> {
    LANES.try_hold_ambient_turn()
}

/// The ambient permit for `persona`'s musing tail: a plain try, unless she is starving —
/// then a bounded wait, so the pool is still the pool but the race is not rigged against
/// the idle forever. Probe `admission.lane.idle_share` names every granted wait.
pub async fn hold_ambient_turn_for(
    persona: uuid::Uuid,
    now_ms: u64,
) -> Option<tokio::sync::OwnedSemaphorePermit> {
    if let Some(p) = LANES.try_hold_ambient_turn() {
        return Some(p);
    }
    let last = LAST_TURN_MS.lock().get(&persona).copied();
    if !starving_since(last, now_ms, *BOOT_MS) {
        return None;
    }
    let started = std::time::Instant::now();
    match tokio::time::timeout(
        std::time::Duration::from_millis(IDLE_SHARE_WAIT_MS),
        LANES.acquire_resized(
            &LANES.ambient_semaphore(),
            &LANES.ambient_installed,
            LaneBudget::NonDirected,
        ),
    )
    .await
    {
        Ok(permit) => {
            crate::probe!(
                class = "admission.lane.idle_share",
                persona = %persona,
                waited_ms = started.elapsed().as_millis() as u64,
                idle_ms = now_ms.saturating_sub(last.unwrap_or(*BOOT_MS)), // JUSTIFIED unwrap_or: no turn yet = idle since boot
                "a starving idle mind waited for an ambient permit and got one — the minimum share"
            );
            Some(permit)
        }
        _ => None,
    }
}

// ── Serving-lane reservation for directed turns (#139) ──────────────────────────
//
// The ambient PERMIT above bounds how many ambient TURNS run at once (1). But ONE
// permitted turn is not one model call: a single `drive_to_settle` makes many calls over
// minutes (act → observe → act), so the permit-holder alone can occupy several physical
// decode lanes (`llama --parallel`), and an addressed (directed) question then queues
// INSIDE the serving process behind it. Glass-boxed 2026-07-15: a directed turn sat
// 8+ minutes behind one 197s idle self-tick + one 213s ambient turn on the two lanes;
// its latency was lane-QUEUE, not decode (a free-lane turn is ~30-60s).
//
// (That glass-box predates the permit reaching the self-tick. Both non-directed paths are
// permit-gated today — `service_loop.rs` acquires at the self-tick gate AND at the
// message-ambient gate, and they share the one ambient pool. The reservation below is
// still required, because the thing it bounds is CALLS-per-turn, which no turn-level
// permit can see. Corrected 2026-08-17: this paragraph had claimed "an idle self-tick is
// not ambient-permit-gated at all", contradicting the self-tick gate's own comment block
// in the same tree — a stale premise sitting under a live design argument.)
//
// Neither the gauge nor the turn-level permit can fix this — the reservation must live
// at the LANE the model call actually consumes. So every model call acquires a lane
// here, priced by priority:
//   - directed → acquires from the FULL pool (MAX_LANES). Waits only if MAX_LANES other
//                directed calls already hold every lane (all lanes serving live work).
//   - non-directed (ambient + idle self-tick) → additionally bounded to (MAX_LANES-1),
//                so at least ONE lane is always reserved for a directed call.
// No deadlock: directed never touches the non-directed cap, and non-directed holds at
// most MAX_LANES-1 physical lanes, so a directed acquire always finds a free lane unless
// every lane already serves directed. On a single-lane machine there is nothing to
// reserve, so the non-directed budget floors at 1 (no starvation, no false guarantee).
// [[conversational-latency-is-a-misdirection-budget]] [[never-thrash-sticky-hysteresis-on-every-lane]]

/// The LIVE `--parallel` slot count of the running serving target, published by the
/// serving daemon on every reconcile (`set_served_lane_count`). 0 until serving is up.
/// This is the ground truth for how many physical decode lanes exist RIGHT NOW —
/// distinct from `serving_plan::MAX_LANES`, which is only the safety ceiling. Before the
/// compute-buffer fit term (#139), lanes were always == MAX_LANES so the constant was
/// exact; now the plan serves DEMAND (e.g. 4 lanes on this box while MAX_LANES = 6), so
/// sizing the admission semaphores by the constant would over-admit by the difference
/// and weaken the directed-turn reservation. Sizing by THIS keeps the reservation exact.
/// Every piece of admission state that used to be six separate process-global statics
/// (`SERVED_LANE_COUNT`, `SERVING_LANES`, `NONDIRECTED_LANES`, their two installed
/// counters, and the resize lock) plus the ambient-turn permits — owned by ONE type.
///
/// Why (2026-08-06): scattered mutable statics made the tests order-dependent, so they took
/// a `TEST_SERIAL` lock whose own comment admitted it was insufficient — "TEST_SERIAL only
/// serializes THIS module, but a cross-module guard could bump it mid-loop". This file had
/// already learned the lesson once and applied it to the in-flight Gauge, stating the
/// principle outright: *the race is gone at the SOURCE, not merely serialized*. That cure
/// reached one of three cases. This applies it to the other two, and to the sibling
/// [`crate::cognition::prefill_throttle`] the same day.
///
/// Lazy semaphores capture the latest lane count on first use. If first use precedes
/// serving, a later publication collects surplus boot permits without interrupting
/// running work. Subsequent serving ticks retry any held shrink debt.
pub struct LaneAdmission {
    /// LIVE `--parallel` count; 0 until serving publishes one.
    pub(super) served: AtomicUsize,
    pub(super) serving: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>>,
    pub(super) nondirected: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>>,
    pub(super) serving_installed: AtomicUsize,
    pub(super) nondirected_installed: AtomicUsize,
    /// Serializes the rare live semaphore reconcile so two concurrent re-plans can't
    /// double-count a grow.
    pub(super) resize_lock: Mutex<()>,
    pub(super) ambient: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>>,
    pub(super) ambient_installed: AtomicUsize,
    /// Held-work callers queued for the non-directed budget right now. While it is
    /// non-zero an AMBIENT caller (a mind with no card in hand) does not enter the
    /// queue — work first (2026-09-15, [`LanePriority`]).
    pub(super) work_waiting: AtomicUsize,
    /// Fires on every permit release so a parked ambient caller re-checks.
    pub(super) released: tokio::sync::Notify,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum LaneBudget {
    Physical,
    NonDirected,
}

/// Who is asking for a lane, priced by what the grid gets back for it.
///
/// Measured 2026-09-15 04:20–05:00Z on the M5 (16 minds, 4 lanes @58k): 33 lane holds
/// at 302 s p50 / 617 s p90, 14 acts, 0 writes, and only 5 of them message turns —
/// the rest were self-cycle turns, and a mind musing with no card queued for the same
/// non-directed budget as a mind holding a card mid-edit. The health receipt read
/// READING for the hour. A lane is the scarce thing; it goes to the mind that will
/// write first.
///
/// - `Directed` — a human or a mention named her: the full pool, one lane always reserved.
/// - `Work` — she holds a card in progress (`holds_live_work`): the non-directed budget,
///   ahead of every ambient caller.
/// - `Ambient` — an undirected turn with no card in hand (musing, wandering, a dream):
///   the non-directed budget only while no `Work` caller is queued.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LanePriority {
    Directed,
    Work,
    Ambient,
}

impl LanePriority {
    /// The prior two-class policy's flag, for the probes and the reserved-lane rule.
    pub fn is_directed(self) -> bool {
        matches!(self, Self::Directed)
    }
}

/// The process-global admission gate — ONE INSTANCE of [`LaneAdmission`], not a separate
/// implementation of it. Tests build their own with [`LaneAdmission::new`].
pub(super) static LANES: LaneAdmission = LaneAdmission::new();

/// Physical decode lanes on the shared serving target — the LIVE served count when
/// serving has come up, else the planner's `MAX_LANES` ceiling as the boot-time fallback.
/// Floored at 1. Read live by the saturation gauge; captured at lazy-init by the lane
/// semaphores (serving is always up before the first persona model call, so the init
/// captures the real count, not the ceiling).
/// Publish the running serving target's live lane count (the plan's `--parallel`). The
/// serving daemon calls this on every reconcile. Sizes the admission semaphores exactly:
/// the lazy-init captures it at boot, and a LATER increase (roster grows, more personas
/// demand lanes) grows the live semaphores via `add_permits`. Decreases collect idle
/// permits immediately, as in `PrefillThrottle`; held permits become shrink debt.
/// The serving daemon retries idle debt every tick; acquisition also retires debt
/// before granting a transferred permit, so queued demand cannot defeat shrink.
/// No running permit is revoked or awaited by the publisher.
pub fn set_served_lane_count(lanes: usize) {
    LANES.set_served_lane_count(lanes);
}

/// Reconcile installed permits without waiting for running calls. The difference
/// between installed and a smaller target is debt retried on the next serving tick.
/// Caller holds the owner resize lock, matching `PrefillThrottle`'s accounting.
pub(super) fn resize_semaphore_to(sem: &tokio::sync::Semaphore, installed: &AtomicUsize, target: usize) {
    let cur = installed.load(Ordering::Acquire);
    if target > cur {
        sem.add_permits(target - cur);
        installed.store(target, Ordering::Release);
    } else if target < cur {
        let forgotten = sem.forget_permits(cur - target);
        installed.store(cur - forgotten, Ordering::Release);
    }
}

/// The lane count a sibling gate should boot with before any plan publishes — the same
/// live-count-else-ceiling read the lane semaphores lazy-init from. Used by the prefill
/// throttle (#56) so both gates start from the ONE number.
pub fn boot_lane_count() -> usize {
    LANES.lane_count()
}


impl LaneAdmission {
    // A released semaphore permit may transfer directly to a queued waiter,
    // leaving nothing idle for the periodic resize to collect. Retire that debt
    // before granting new work, under the same lock as target publication.
    pub(super) fn permit_or_retire(
        &self,
        permit: tokio::sync::OwnedSemaphorePermit,
        installed: &AtomicUsize,
        budget: LaneBudget,
    ) -> Option<tokio::sync::OwnedSemaphorePermit> {
        let _guard = self
            .resize_lock
            .lock()
            .expect("lane-resize lock never poisoned");
        let target = match budget {
            LaneBudget::Physical => self.lane_count(),
            LaneBudget::NonDirected => self.nondirected_budget(),
        };
        let count = installed.load(Ordering::Acquire);
        if count > target {
            permit.forget();
            installed.store(count - 1, Ordering::Release);
            drop(_guard);
            crate::probe!(
                class = "admission.lane.shrink_debt",
                pool = ?budget,
                desired = target as u64,
                installed = (count - 1) as u64,
                debt = (count - 1 - target) as u64,
                "retired a transferred permit before admitting queued work"
            );
            None
        } else {
            Some(permit)
        }
    }

    pub(super) fn try_acquire_resized(
        &self,
        sem: &std::sync::Arc<tokio::sync::Semaphore>,
        installed: &AtomicUsize,
        budget: LaneBudget,
    ) -> Option<tokio::sync::OwnedSemaphorePermit> {
        loop {
            let permit = sem.clone().try_acquire_owned().ok()?;
            if let Some(permit) = self.permit_or_retire(permit, installed, budget) {
                return Some(permit);
            }
        }
    }

    pub(super) async fn acquire_resized(
        &self,
        sem: &std::sync::Arc<tokio::sync::Semaphore>,
        installed: &AtomicUsize,
        budget: LaneBudget,
    ) -> tokio::sync::OwnedSemaphorePermit {
        loop {
            let permit = sem
                .clone()
                .acquire_owned()
                .await
                .expect("lane semaphore never closed");
            if let Some(permit) = self.permit_or_retire(permit, installed, budget) {
                return permit;
            }
        }
    }
    /// A fresh, INDEPENDENT admission gate. `const` so the process-global can be a `static`;
    /// tests call it to get their own and therefore run order-independently and in parallel.
    pub const fn new() -> Self {
        Self {
            served: AtomicUsize::new(0),
            serving: std::sync::OnceLock::new(),
            nondirected: std::sync::OnceLock::new(),
            serving_installed: AtomicUsize::new(0),
            nondirected_installed: AtomicUsize::new(0),
            resize_lock: Mutex::new(()),
            ambient: std::sync::OnceLock::new(),
            ambient_installed: AtomicUsize::new(0),
            work_waiting: AtomicUsize::new(0),
            released: tokio::sync::Notify::const_new(),
        }
    }

    /// Physical decode lanes: the LIVE served count once serving is up, else the planner's
    /// ceiling as the boot fallback. Floored at 1.
    pub(super) fn lane_count(&self) -> usize {
        match self.served.load(Ordering::Acquire) {
            0 => (crate::cognition::serving_plan::MAX_LANES as usize).max(1),
            n => n,
        }
    }

    /// Lanes a non-directed (work / ambient) call may occupy: every lane. A lane held back for
    /// directed turns idled 80% of the time on the M5 (2026-10-20: slot 1 busy in 6 of 30
    /// samples, 3 directed grants against 39 work grants in 10 minutes) while four minds queued
    /// behind the other. Directed priority needs no held lane: a directed call waits on the
    /// physical lane itself, queued work waits on this budget first, so the next freed lane
    /// goes to the directed call. Its cost, measured live: a directed call can wait for the
    /// shortest act in flight (`directed_lane_wait_ms`).
    pub(super) fn nondirected_budget(&self) -> usize {
        self.lane_count()
    }

    /// See [`set_served_lane_count`].
    pub fn set_served_lane_count(&self, lanes: usize) {
        let lanes = lanes.max(1);
        let _guard = self
            .resize_lock
            .lock()
            .expect("lane-resize lock never poisoned");
        let before = (
            self.served.load(Ordering::Acquire),
            self.serving_installed.load(Ordering::Acquire),
            self.nondirected_installed.load(Ordering::Acquire),
            self.ambient_installed.load(Ordering::Acquire),
        );
        self.served.store(lanes, Ordering::Release);
        if let Some(sem) = self.serving.get() {
            resize_semaphore_to(sem, &self.serving_installed, lanes);
        }
        if let Some(sem) = self.nondirected.get() {
            resize_semaphore_to(sem, &self.nondirected_installed, lanes);
        }
        // The ambient-turn pool rides the SAME budget, so it grows on the same signal.
        // Missing this is how a pool installed at the boot floor would stay there for the
        // process's life while serving grew underneath it.
        if let Some(sem) = self.ambient.get() {
            resize_semaphore_to(sem, &self.ambient_installed, lanes);
        }
        let after = (
            lanes,
            self.serving_installed.load(Ordering::Acquire),
            self.nondirected_installed.load(Ordering::Acquire),
            self.ambient_installed.load(Ordering::Acquire),
        );
        drop(_guard);
        if after != before {
            crate::probe!(
                class = "admission.lane.resized",
                desired = lanes as u64,
                installed = after.1 as u64,
                debt = after.1.saturating_sub(lanes) as u64,
                nondirected_installed = after.2 as u64,
                ambient_installed = after.3 as u64,
                "lane admission reconciled; installed includes held shrink debt, zero means uninitialized"
            );
        }
    }

    pub(super) fn serving_lanes(&self) -> &std::sync::Arc<tokio::sync::Semaphore> {
        self.serving.get_or_init(|| {
            let n = self.lane_count();
            self.serving_installed.store(n, Ordering::Release);
            std::sync::Arc::new(tokio::sync::Semaphore::new(n))
        })
    }

    pub(super) fn nondirected_lanes(&self) -> &std::sync::Arc<tokio::sync::Semaphore> {
        self.nondirected.get_or_init(|| {
            let n = self.nondirected_budget();
            self.nondirected_installed.store(n, Ordering::Release);
            std::sync::Arc::new(tokio::sync::Semaphore::new(n))
        })
    }

    /// See [`try_hold_ambient_turn`]. Lazy like its siblings, and for the same load-bearing
    /// reason: it must capture the budget at FIRST USE, once serving has published a real
    /// lane count — never at construction, when only the boot ceiling is known.
    /// The ambient pool itself, for a bounded wait (see `hold_ambient_turn_for`).
    pub fn ambient_semaphore(&self) -> std::sync::Arc<tokio::sync::Semaphore> {
        self.ambient
            .get_or_init(|| {
                let n = self.nondirected_budget();
                self.ambient_installed.store(n, Ordering::Release);
                std::sync::Arc::new(tokio::sync::Semaphore::new(n))
            })
            .clone()
    }

    pub fn try_hold_ambient_turn(&self) -> Option<tokio::sync::OwnedSemaphorePermit> {
        self.try_acquire_resized(
            &self.ambient_semaphore(),
            &self.ambient_installed,
            LaneBudget::NonDirected,
        )
        .or_else(|| {
            // Count the miss. The permit is non-blocking, so a None here IS the yield.
            AMBIENT_YIELDS.fetch_add(1, Ordering::Relaxed);
            None
        })
    }

    /// See [`acquire_serving_lane`] — the reservation policy lives HERE so the global and a
    /// test instance can never drift into two different policies.
    pub async fn acquire_serving_lane(
        &'static self,
        persona: Option<uuid::Uuid>,
        priority: LanePriority,
        expected_occupancy: Option<std::time::Duration>,
    ) -> ServingLanePermit {
        let queued_at = std::time::Instant::now();
        let directed = priority.is_directed();
        // Work and ambient calls take the non-directed budget (every lane) first, then the
        // physical lane; a directed call takes the physical lane directly, so it is first in
        // line for the next lane any holder releases.
        let (nondirected, lent_lane): (Option<tokio::sync::OwnedSemaphorePermit>, Option<tokio::sync::OwnedSemaphorePermit>) = match priority {
            LanePriority::Directed => (None, None),
            LanePriority::Work => (Some(self.acquire_nondirected_as_work().await), None),
            LanePriority::Ambient => (Some(self.acquire_nondirected_as_ambient().await), None),
        };
        let lane = match lent_lane {
            Some(lane) => lane,
            None => {
                self.acquire_resized(
                    self.serving_lanes(),
                    &self.serving_installed,
                    LaneBudget::Physical,
                )
                .await
            }
        };
        crate::probe!(
            class = "admission.lane.granted",
            persona = ?persona,
            waited_ms = queued_at.elapsed().as_millis() as u64,
            directed,
            priority = ?priority,
            now_available = self.serving_lanes().available_permits() as u64,
            "serving-lane permit granted"
        );
        ServingLanePermit {
            _lane: lane,
            _nondirected: nondirected,
            _inflight: InflightModelCall::enter(),
            granted_at: std::time::Instant::now(),
            persona,
            directed,
            priority,
            released: &self.released,
        }
    }

    /// A held-work caller queues in the non-directed semaphore in arrival order, and
    /// is COUNTED while it waits so ambient callers stand aside.
    pub(super) async fn acquire_nondirected_as_work(&self) -> tokio::sync::OwnedSemaphorePermit {
        let sem = self.nondirected_lanes().clone();
        if let Some(p) =
            self.try_acquire_resized(&sem, &self.nondirected_installed, LaneBudget::NonDirected)
        {
            return p;
        }
        tracing::info!(
            probe_class = "serving.lane.nondirected_waiting",
            "held-work model call waiting — a lane is reserved for directed turns (#139)"
        );
        // COUNTED BY A GUARD, NOT BY HAND (2026-09-15, 20:35Z: sixteen minds idle for 50
        // minutes with all seven lanes free). A parked work wait is CANCELLED whenever
        // the turn's future is dropped — `delib.gate.yielded_to_directed` selects away
        // from it, the settle deadline ends the drive, a despawn aborts the loop — and a
        // by-hand `fetch_sub` after the await never ran, so `work_waiting` read 1 for
        // the rest of the process and every ambient caller parked forever on a pool
        // that was empty. Cancellation is a normal exit here; the guard makes it one.
        let _counted = WorkWaiting::enter(&self.work_waiting);
        self.acquire_resized(&sem, &self.nondirected_installed, LaneBudget::NonDirected)
            .await
    }

    /// An ambient caller never enters the semaphore's queue: it takes a permit only when
    /// one is free AND no held-work caller is waiting, otherwise it parks on the release
    /// signal (a 1 s tick guards against a missed wake). Bounded fairness comes from the
    /// work queue draining: every release re-checks, and work waiters are counted down
    /// as they are served.
    pub(super) async fn acquire_nondirected_as_ambient(&self) -> tokio::sync::OwnedSemaphorePermit {
        let sem = self.nondirected_lanes().clone();
        let mut yielded = false;
        loop {
            if self.work_waiting.load(Ordering::Acquire) == 0 {
                if let Some(p) = self.try_acquire_resized(
                    &sem,
                    &self.nondirected_installed,
                    LaneBudget::NonDirected,
                ) {
                    return p;
                }
            } else if !yielded {
                yielded = true;
                crate::probe!(
                    class = "admission.lane.ambient_yielded_to_work",
                    work_waiting = self.work_waiting.load(Ordering::Acquire) as u64,
                    "an ambient turn stands aside — a held card is waiting for the lane"
                );
            }
            let _ =
                tokio::time::timeout(std::time::Duration::from_secs(1), self.released.notified())
                    .await;
        }
    }

    /// Held-work callers queued right now — a gauge for the gate probes.
    pub fn work_waiting(&self) -> usize {
        self.work_waiting.load(Ordering::Acquire)
    }
}

/// RAII count of a held-work caller waiting for the non-directed budget: incremented
/// on entry, decremented on drop — so a wait that is cancelled mid-await (the only way
/// a parked future ever leaves without a permit) is uncounted the instant it goes.
pub(super) struct WorkWaiting<'a>(&'a AtomicUsize);

impl<'a> WorkWaiting<'a> {
    pub(super) fn enter(counter: &'a AtomicUsize) -> Self {
        counter.fetch_add(1, Ordering::AcqRel);
        Self(counter)
    }
}

impl Drop for WorkWaiting<'_> {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

impl Default for LaneAdmission {
    fn default() -> Self {
        Self::new()
    }
}

/// Acquire a serving lane for a model call, priced by priority (#139). `directed`
/// callers take from the full lane pool; non-directed callers first claim the
/// (MAX_LANES-1) non-directed budget, guaranteeing a directed caller always finds a free
/// physical lane. Awaits only under genuine contention; the returned permit releases
/// every lane on drop.
/// How many physical serving-lane permits are free RIGHT NOW — a gauge for the
/// gate probes, so a starved waiter names the count instead of implying it.
pub fn serving_lane_permits_available() -> usize {
    LANES.serving_lanes().available_permits()
}

/// `expected_occupancy`: the call's own bound on how long it will hold the lane (prefill
/// of the uncached prompt + decode, at this box's measured rates), or `None` = unknown. Only
/// a WORK call uses it — the reserved lane may be lent to it inside the directed wait
/// budget (`reserve_lendable`). Directed and ambient callers pass `None`.
pub async fn acquire_serving_lane(
    persona: Option<uuid::Uuid>,
    priority: LanePriority,
    expected_occupancy: Option<std::time::Duration>,
) -> ServingLanePermit {
    LANES.acquire_serving_lane(persona, priority, expected_occupancy).await
}
