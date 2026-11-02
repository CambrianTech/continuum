//! Serving-lane admission: lane pools and directed-turn reservation.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// Whether a directed line is pending for any citizen on this node — the reserved lane's
/// re-arm signal. Attention is the mind's, so the mind installs the reader at boot; the
/// lane only asks.
pub type DirectedPending = fn() -> bool;

static DIRECTED_PENDING: parking_lot::RwLock<Option<DirectedPending>> = parking_lot::RwLock::new(None);

/// Install the one directed-pending reader. A second install replaces the first.
pub fn install_directed_pending(reader: DirectedPending) {
    *DIRECTED_PENDING.write() = Some(reader);
}

/// Pending as the installed reader says; with none installed the answer is UNKNOWN, read as
/// pending, so the reserve is never lent on a guess (unknown occupancy lends nothing either).
fn directed_pending() -> bool {
    (*DIRECTED_PENDING.read()).is_none_or(|reader| reader())
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
/// reached one of three cases. This applies it to the other two.
///
/// Lazy semaphores capture the latest lane count on first use. If first use precedes
/// serving, a later publication collects surplus boot permits without interrupting
/// running work. Subsequent serving ticks retry any held shrink debt.
pub struct LaneAdmission {
    pub(super) progress: parking_lot::Mutex<super::progress::ProgressLedger>,
    /// LIVE `--parallel` count; 0 until serving publishes one.
    pub(super) served: AtomicUsize,
    pub(super) serving: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>>,
    pub(super) nondirected: std::sync::OnceLock<std::sync::Arc<tokio::sync::Semaphore>>,
    pub(super) serving_installed: AtomicUsize,
    pub(super) nondirected_installed: AtomicUsize,
    /// Serializes the rare live semaphore reconcile so two concurrent re-plans can't
    /// double-count a grow.
    pub(super) resize_lock: Mutex<()>,
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
///   ahead of ambient callers until their initial cohort has passed.
/// - `Ambient` — an undirected turn with no card in hand (musing, wandering, a dream):
///   the non-directed budget opportunistically, then by retained age ticket.
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

pub fn resident_seated(persona: uuid::Uuid) {
    LANES.resident_seated(persona);
}

pub fn resident_departed(persona: uuid::Uuid) {
    LANES.resident_departed(persona);
}

/// Physical decode lanes on the shared serving target — the LIVE served count when
/// serving has come up, else the planner's `MAX_LANES` ceiling as the boot-time fallback.
/// Floored at 1. Read live by the saturation gauge; captured at lazy-init by the lane
/// semaphores (serving is always up before the first persona model call, so the init
/// captures the real count, not the ceiling).
/// Publish the running serving target's live lane count (the plan's `--parallel`). The
/// serving daemon calls this on every reconcile. Sizes the admission semaphores exactly:
/// the lazy-init captures it at boot, and a LATER increase (roster grows, more personas
/// demand lanes) grows the live semaphores via `add_permits`. Decreases collect idle
/// permits immediately, as a resized semaphore must; held permits become shrink debt.
/// The serving daemon retries idle debt every tick; acquisition also retires debt
/// before granting a transferred permit, so queued demand cannot defeat shrink.
/// No running permit is revoked or awaited by the publisher.
pub fn set_served_lane_count(lanes: usize) {
    LANES.set_served_lane_count(lanes);
}

/// Reconcile installed permits without waiting for running calls. The difference
/// between installed and a smaller target is debt retried on the next serving tick.
/// Caller holds the owner resize lock, the installed-permit accounting.
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
            continuum_base::probe!(
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
            progress: parking_lot::Mutex::new(super::progress::ProgressLedger::new()),
            served: AtomicUsize::new(0),
            serving: std::sync::OnceLock::new(),
            nondirected: std::sync::OnceLock::new(),
            serving_installed: AtomicUsize::new(0),
            nondirected_installed: AtomicUsize::new(0),
            resize_lock: Mutex::new(()),
            work_waiting: AtomicUsize::new(0),
            released: tokio::sync::Notify::const_new(),
        }
    }

    /// Physical decode lanes: the LIVE served count once serving is up, else the planner's
    /// ceiling as the boot fallback. Floored at 1.
    pub(super) fn lane_count(&self) -> usize {
        match self.served.load(Ordering::Acquire) {
            0 => (crate::inference::planning::serving_plan::MAX_LANES as usize).max(1),
            n => n,
        }
    }

    /// Lanes a non-directed (ambient / idle) call may occupy: all but one, floored at 1 so a
    /// single-lane machine still lets idle work run rather than starving it forever.
    pub(super) fn nondirected_budget(&self) -> usize {
        self.lane_count().saturating_sub(1).max(1)
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
        );
        self.served.store(lanes, Ordering::Release);
        if let Some(sem) = self.serving.get() {
            resize_semaphore_to(sem, &self.serving_installed, lanes);
        }
        if let Some(sem) = self.nondirected.get() {
            resize_semaphore_to(
                sem,
                &self.nondirected_installed,
                lanes.saturating_sub(1).max(1),
            );
        }
        let after = (
            lanes,
            self.serving_installed.load(Ordering::Acquire),
            self.nondirected_installed.load(Ordering::Acquire),
        );
        drop(_guard);
        if after != before {
            continuum_base::probe!(
                class = "admission.lane.resized",
                desired = lanes as u64,
                installed = after.1 as u64,
                debt = after.1.saturating_sub(lanes) as u64,
                nondirected_installed = after.2 as u64,
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

    /// See [`acquire_serving_lane`] — the reservation policy lives HERE so the global and a
    /// test instance can never drift into two different policies.
    pub async fn acquire_serving_lane(
        &'static self,
        persona: Option<uuid::Uuid>,
        priority: LanePriority,
        expected_occupancy: Option<std::time::Duration>,
    ) -> ServingLanePermit {
        let (_, acquire) = self.serving_lane_request(persona, priority, expected_occupancy);
        acquire.await
    }

    /// One request and a read-only observer of its actual blocking budget.
    pub fn serving_lane_request(
        &'static self,
        persona: Option<uuid::Uuid>,
        priority: LanePriority,
        expected_occupancy: Option<std::time::Duration>,
    ) -> (super::LaneWait, impl std::future::Future<Output = ServingLanePermit> + Send) {
        let (observer, progress) = self.observe(persona, priority);
        let aging = observer.clone();
        let acquire = async move {
        let queued_at = std::time::Instant::now();
        let directed = priority.is_directed();
        // Non-directed reserves within the (lanes-1) budget FIRST, so the physical-lane
        // acquire below can never let non-directed work starve a directed caller —
        // EXCEPT that while nothing directed is pending a work call is LENT the reserved
        // lane (`reserve_lendable`): a slot never idles for a mention. THE LOAN NEVER PARKS
        // (Cormac's condition on #4250): it is a try-acquire on a physical lane that is
        // free RIGHT NOW — a lent call that had to wait on the FIFO physical semaphore
        // would put a later directed arrival behind its whole occupancy (2× the budget
        // with one parked, 3× with two). No free lane = no loan; she takes the ordinary
        // Work path and waits for the non-directed budget like before. The loan ends
        // when the permit drops; a directed line pending at the next acquire re-arms.
        let (nondirected, lent_lane) = match priority {
            LanePriority::Directed => (None, None),
            LanePriority::Work => {
                match aging.try_budget(false) {
                    Some(p) => (Some(p), None),
                    None => {
                        let reserve_exists = self.nondirected_budget() < self.lane_count();
                        let lendable = reserve_exists && reserve_lendable(directed_pending());
                        let lent = if lendable {
                            self.try_acquire_resized(
                                self.serving_lanes(),
                                &self.serving_installed,
                                LaneBudget::Physical,
                            )
                        } else {
                            None
                        };
                        match lent {
                            Some(lane) => {
                                continuum_base::probe!(
                                    class = "admission.lane.reserve_lent_to_work",
                                    expected_ms = ?expected_occupancy.map(|d| d.as_millis()),
                                    "the reserved lane is lent to work while nothing directed is pending: no slot idles for a mention; a directed arrival takes the next lane released"
                                );
                                (None, Some(lane))
                            }
                            None => (Some(self.acquire_ticketed_budget(&aging, false).await), None),
                        }
                    }
                }
            }
            LanePriority::Ambient => (Some(self.acquire_aged_ambient(&aging).await), None),
        };
        progress.physical(nondirected.is_some());
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
        continuum_base::probe!(
            class = "admission.lane.granted",
            persona = ?persona,
            waited_ms = queued_at.elapsed().as_millis() as u64,
            directed,
            priority = ?priority,
            now_available = self.serving_lanes().available_permits() as u64,
            "serving-lane permit granted"
        );
        progress.granted();
        ServingLanePermit {
            _progress: progress,
            _lane: lane,
            _nondirected: nondirected,
            _inflight: InflightModelCall::enter(),
            granted_at: std::time::Instant::now(),
            persona,
            directed,
            priority,
            released: &self.released,
        }
        };
        (observer, acquire)
    }

    /// Active work and aged residents compete by ticket; only the winner tries
    /// the semaphore, so cancellation cannot leave an unobserved permit owner.
    async fn acquire_ticketed_budget(&self, observer: &super::LaneWait, aged: bool) -> tokio::sync::OwnedSemaphorePermit {
        let _counted = WorkWaiting::enter(&self.work_waiting);
        loop {
            let notified = self.released.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if let Some(permit) = observer.try_budget(aged) { return permit; }
            tokio::select! {
                _ = notified => {},
                _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {},
            }
        }
    }

    /// An ambient caller never enters the semaphore's queue: it takes a permit only when
    /// one is free AND no held-work caller is waiting, otherwise it parks on the release
    /// signal (a 1 s tick guards against a missed wake). This opportunistic path alone
    /// can starve; acquire_aged_ambient promotes into ticket order after the initial cohort.
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
                continuum_base::probe!(
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

    /// Let the initial cohort go first, then retain a ticket rather than waiting for
    /// an indefinitely replenished work queue to empty. Directed capacity is unchanged.
    async fn acquire_aged_ambient(&self, observer: &super::LaneWait) -> tokio::sync::OwnedSemaphorePermit {
        let cohort = self.work_waiting().saturating_add(self.nondirected_budget()).max(1) as u64;
        let origin = observer.ambient_origin(cohort);
        let opportunistic = self.acquire_nondirected_as_ambient();
        tokio::pin!(opportunistic);
        loop {
            if observer.ambient_aged(origin) {
                return self.acquire_ticketed_budget(observer, true).await;
            }
            tokio::select! {
                permit = &mut opportunistic => return permit,
                _ = self.released.notified() => {},
                _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {},
            }
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

/// Observe the process gate without changing its queue policy.
pub fn serving_lane_request(persona: Option<uuid::Uuid>, priority: LanePriority, expected: Option<std::time::Duration>) -> (super::LaneWait, impl std::future::Future<Output = ServingLanePermit> + Send) {
    LANES.serving_lane_request(persona, priority, expected)
}
