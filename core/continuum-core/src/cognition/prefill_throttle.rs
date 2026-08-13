//! The instant safety valve (#56): concurrent-prefill admission derived from LIVE free GPU.
//!
//! This is the sim's `capacity::FitPolicy` running in production — the knob the 2026-07-16
//! compute-buffer OOM proved missing. The serving PLAN sizes lanes for the world it saw at
//! reconcile time; between reconciles, an external consumer (a game, the browser) can eat the
//! headroom those lanes assumed, and N concurrent prefill spikes (each drawing a transient
//! `ModelFootprint::compute_buffer_per_lane()` from free GPU) then overflow what is ACTUALLY
//! free. Reaction-cost ladder (design §4): re-planning lanes is the expensive knob (server
//! respawn, hysteresis); this throttle is the cheap one — how many of the served lanes may
//! PREFILL at once flexes instantly, both directions, with the live number.
//!
//! Shape: no new task, no new monitor. The serving daemon's existing tick calls
//! [`reconcile`] with the resource board's `available(Vram)` (a lock-free watch read that
//! already nets external pressure and grants); [`publish_serving`] gives the throttle its two
//! demand facts (per-spike bytes, served lanes) on every plan publish. Admission holds a
//! permit for the model-call window via `resource_admission::acquire_serving_lane`. Grow is
//! `add_permits` (instant); shrink is `forget_permits` (instant for idle permits; permits held
//! by in-flight prefills can't be revoked — you can't un-run a prefill — so the remainder is a
//! DEBT the next tick's reconcile retries, self-healing as calls finish).

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use crate::capacity::{AllocationPolicy, DeviceCapacity, FitPolicy, LeaseRequest};

/// Process-global throttle: the resource it gates (ONE shared serving target's transient
/// compute-buffer headroom) is process-global, same granularity argument as the lane gate.
static THROTTLE: OnceLock<PrefillThrottle> = OnceLock::new();

/// Consecutive reconciles a GROW must persist before it applies (≈15s at the serving
/// daemon's 5s cadence). Shrink is exempt — the safety direction is always instant.
/// Asymmetric hysteresis, observed necessary live 2026-07-16 within minutes of first boot:
/// `available(Vram)` rides the fit boundary tick to tick (UMA free wobbles with caches/KV)
/// and the grant flapped 1↔2 every 5s. Fail-safe fast, recover deliberate.
/// [[never-thrash-sticky-hysteresis-on-every-lane]]
const GROW_STICKINESS_TICKS: usize = 3;

pub struct PrefillThrottle {
    sem: Arc<tokio::sync::Semaphore>,
    /// Permits conceptually installed (a Semaphore only exposes AVAILABLE). Shrink debt is
    /// `installed − target` that `forget_permits` couldn't collect yet.
    installed: AtomicUsize,
    /// Transient compute-buffer bytes ONE concurrent prefill draws from free GPU — published
    /// from the served plan's footprint. 0 = unpublished (throttle stays at the lane count).
    spike_bytes: AtomicU64,
    /// The served lane count — the demand ceiling the fit is clamped to.
    want_lanes: AtomicUsize,
    /// Consecutive reconciles that wanted a grow — gate for [`GROW_STICKINESS_TICKS`].
    grow_streak: AtomicUsize,
    /// Serializes reconcile's read-modify-write on (installed, semaphore).
    reconcile_lock: Mutex<()>,
}

fn throttle() -> &'static PrefillThrottle {
    THROTTLE.get_or_init(|| {
        // Boot fallback: before any plan publishes, gate at the same count the lane
        // semaphores use, so behavior is exactly pre-throttle until facts arrive.
        PrefillThrottle::with_lanes(super::resource_admission::boot_lane_count())
    })
}

/// Publish the served plan's demand facts: the per-prefill transient spike and the lane
/// count. Called by the serving daemon on every plan publish, right where it publishes the
/// lane count — ONE source of truth, no second path.
pub fn publish_serving(spike_bytes: u64, lanes: usize) {
    throttle().publish_serving(spike_bytes, lanes);
}

/// Re-derive the concurrent-prefill grant from LIVE free GPU bytes and apply it to the gate.
/// Called on the serving daemon's tick (cheap: watch read + atomics + at most one semaphore
/// op). Returns the applied target for probes/tests.
///
/// The fit rule is the SAME `capacity::FitPolicy` the simulator proves scenarios against —
/// sim == prod at the policy seam. Safety margin: one spike of headroom, so measurement
/// jitter of one lane never turns a fit into an overflow.
/// The plan-published per-prefill spike bytes (0 = no plan yet). The SAME live
/// value the throttle fits with — exposed so the lane decision (#108 step 2)
/// prices "can local serve one more?" with one spike truth, never a second
/// estimate ([[the compression principle]]).
pub fn published_spike_bytes() -> u64 {
    throttle().published_spike_bytes()
}

pub fn reconcile(gpu_free_bytes_live: u64) -> usize {
    throttle().reconcile(gpu_free_bytes_live)
}

impl PrefillThrottle {
    /// An INDEPENDENT throttle over `lanes` permits.
    ///
    /// Production has exactly one (the process-global [`throttle`]) because the resource it
    /// gates — one serving target's transient compute-buffer headroom — really is
    /// process-global. Tests get their OWN, which is the entire point: the global is an
    /// instance of this type, not the definition of it.
    ///
    /// Why this constructor exists (2026-08-06): every behavior below used to be a free
    /// function reaching into a `OnceLock`, so the tests exercised shared state. The
    /// `OnceLock` is initialized ONCE, by whichever test touches it first, with that
    /// moment's `boot_lane_count()` — which made the suite ORDER-DEPENDENT. Adding unrelated
    /// tests elsewhere shifted scheduling and `shrink_debt_drains_as_inflight_prefills_finish`
    /// went red in the full suite while passing in isolation. That is not a flake: the
    /// assertions are `assert_eq!` on integer permit counts, and a predicate with no clock in
    /// it cannot fail from load. Second instance of this exact class in one day — the
    /// heartbeat's `ms_since_real_decode` was the first — so the cure is the same one:
    /// per-owner state, with the global as the production default.
    pub fn with_lanes(lanes: usize) -> Self {
        let lanes = lanes.max(1);
        Self {
            sem: Arc::new(tokio::sync::Semaphore::new(lanes)),
            installed: AtomicUsize::new(lanes),
            spike_bytes: AtomicU64::new(0),
            want_lanes: AtomicUsize::new(lanes),
            grow_streak: AtomicUsize::new(0),
            reconcile_lock: Mutex::new(()),
        }
    }

    /// See [`publish_serving`].
    pub fn publish_serving(&self, spike_bytes: u64, lanes: usize) {
        self.spike_bytes.store(spike_bytes, Ordering::Release);
        self.want_lanes.store(lanes.max(1), Ordering::Release);
    }

    /// See [`published_spike_bytes`].
    pub fn published_spike_bytes(&self) -> u64 {
        self.spike_bytes.load(Ordering::Acquire)
    }

    /// See [`reconcile`] — the fit rule lives HERE so the global and a test instance can
    /// never drift into two different policies.
    pub fn reconcile(&self, gpu_free_bytes_live: u64) -> usize {
        let spike = self.spike_bytes.load(Ordering::Acquire);
        let want = self.want_lanes.load(Ordering::Acquire).max(1);
        if spike == 0 {
            // No plan published yet — no fit facts, hold at the lane count (pre-throttle).
            return self.apply(want);
        }
        let cap = DeviceCapacity {
            // The fit rule only reads the live-free axis; the others are not sourced here.
            gpu_total_bytes: 0,
            gpu_free_bytes_live,
            system_ram_free_bytes: 0,
        };
        let req = LeaseRequest {
            consumer: "prefill".into(),
            want_concurrency: want as u32,
            spike_bytes: spike,
        };
        let grant = FitPolicy {
            safety_margin_bytes: spike,
        }
        .grant(&cap, &req);
        self.apply(grant.concurrency as usize)
    }

    /// See [`acquire_prefill_slot`].
    pub async fn acquire_prefill_slot(&self) -> tokio::sync::OwnedSemaphorePermit {
        self.sem
            .clone()
            .acquire_owned()
            .await
            .expect("prefill semaphore is never closed")
    }

    /// See [`installed_permits`].
    pub fn installed_permits(&self) -> usize {
        self.installed.load(Ordering::Acquire)
    }

    /// Apply a target permit count, asymmetrically: SHRINK is instant (collect what's idle
    /// now, leave the rest as debt later reconciles drain as in-flight prefills finish);
    /// GROW only lands after [`GROW_STICKINESS_TICKS`] consecutive reconciles wanted it —
    /// sustained headroom, not one optimistic reading. Never blocks, never revokes running
    /// work, never thrashes on a boundary-riding live number.
    fn apply(&self, target: usize) -> usize {
        let target = target.max(1); // a resident model may always run ONE prefill (residency decision)
        let _g = self
            .reconcile_lock
            .lock()
            .expect("prefill reconcile lock never poisoned");
        let installed = self.installed.load(Ordering::Acquire);
        if target > installed {
            // The recovery direction: deliberate. One tick of headroom is often UMA cache
            // wobble; demand the signal persist before paying the flap.
            let streak = self.grow_streak.fetch_add(1, Ordering::AcqRel) + 1;
            if streak >= GROW_STICKINESS_TICKS {
                self.sem.add_permits(target - installed);
                self.installed.store(target, Ordering::Release);
                self.grow_streak.store(0, Ordering::Release);
            }
        } else {
            self.grow_streak.store(0, Ordering::Release);
            if target < installed {
                // The safety direction: instant, always.
                let forgotten = self.sem.forget_permits(installed - target);
                self.installed
                    .store(installed - forgotten, Ordering::Release);
            }
        }
        let now = self.installed.load(Ordering::Acquire);
        // Glass box: the valve speaks ONLY when it moves — a steady grant is silence, a
        // shrink under pressure / regrow / debt-drain each leave one auditable line.
        if now != installed {
            crate::probe!(
                class = "serving.prefill_throttle",
                from = installed,
                to = now,
                target = target,
                "concurrent-prefill grant re-derived from live VRAM",
            );
        }
        now
    }
}

/// Acquire one concurrent-prefill slot for the model-call window. Await under pressure —
/// the call proceeds when a slot frees or reconcile grows the gate back.
pub async fn acquire_prefill_slot() -> tokio::sync::OwnedSemaphorePermit {
    throttle().acquire_prefill_slot().await
}

/// Permits currently installed (post-debt) — for probes and tests.
pub fn installed_permits() -> usize {
    throttle().installed_permits()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1024 * 1024 * 1024;

    /// Bring a FRESH throttle to `lanes` permits by sustaining headroom.
    ///
    /// No serialization lock and no floor-reset dance any more: each test owns its instance,
    /// so there is no sibling to race and no prior state to undo. The lock this replaced
    /// existed because both tests held permits from ONE process-global semaphore — test A's
    /// shrink could steal the permits test B was awaiting, deadlocking the suite (observed
    /// live, exit 144). Owning the state deletes that failure mode rather than scheduling
    /// around it, and it also deletes the ORDER-DEPENDENCE that made
    /// `shrink_debt_drains_as_inflight_prefills_finish` red in the full suite while green in
    /// isolation on 2026-08-06.
    fn grow_to(t: &PrefillThrottle, lanes: usize, free: u64) -> usize {
        let mut last = t.installed_permits();
        for _ in 0..GROW_STICKINESS_TICKS {
            last = t.reconcile(free);
        }
        assert_eq!(last, lanes, "sustained headroom grows to the fit");
        last
    }

    // what this catches: THE LIVE SAFETY VALVE — the exact 2026-07-16 OOM shape, gated, WITH
    // the asymmetry the first live boot proved necessary. Plan serves 4 lanes, ~2GB spikes.
    // A game eating the GPU (free → 7GB) shrinks admission to (7−2)/2 = 2 on the VERY NEXT
    // reconcile — the safety direction is instant, no plan re-publish, no server respawn.
    // The game closing regrows to 4 only after GROW_STICKINESS_TICKS consecutive reconciles
    // — one optimistic reading is UMA wobble, not recovery (observed live: the grant flapped
    // 1↔2 every 5s tick riding the fit boundary). If shrink loses its immediacy the OOM is
    // back; if grow loses its stickiness the thrash is back.
    #[tokio::test]
    async fn shrink_is_instant_grow_requires_sustained_headroom() {
        // OUR OWN throttle. No process-global, so these tests are order-INDEPENDENT and can
        // run in parallel — the whole reason `with_lanes` exists.
        let t = PrefillThrottle::with_lanes(1);
        t.publish_serving(2 * GB, 4);
        grow_to(&t, 4, 13 * GB);

        // Game opens: shrink lands on the FIRST reconcile that sees the pressure.
        assert_eq!(t.reconcile(7 * GB), 2, "shrink is instant: (7−2)/2 = 2");

        // Game closes: one good reading does NOT regrow (boundary-riding wobble)…
        assert_eq!(
            t.reconcile(13 * GB),
            2,
            "one optimistic tick is not recovery"
        );
        assert_eq!(t.reconcile(13 * GB), 2, "nor two");
        // …and a dip in between resets the streak — the signal must be SUSTAINED.
        assert_eq!(t.reconcile(7 * GB), 2, "a relapse resets the grow streak");
        assert_eq!(t.reconcile(13 * GB), 2);
        assert_eq!(t.reconcile(13 * GB), 2);
        assert_eq!(
            t.reconcile(13 * GB),
            4,
            "three consecutive good ticks → regrown to demand"
        );

        // The applied grant is enforced, not advisory: under pressure only 2 slots grant.
        assert_eq!(t.reconcile(7 * GB), 2);
        let a = t.acquire_prefill_slot().await;
        let b = t.acquire_prefill_slot().await;
        assert!(
            t.sem.clone().try_acquire_owned().is_err(),
            "a third concurrent prefill must wait while pressure holds"
        );
        drop((a, b));
    }

    // what this catches: shrink DEBT self-healing. Permits held by in-flight prefills can't
    // be revoked (you can't un-run a prefill), so a shrink under load collects what it can
    // and the remainder drains on later reconciles as calls finish. If the debt bookkeeping
    // drifted (double-forget or lost debt), the gate would end up permanently too tight or
    // too loose — both are real failures (starved serving / OOM window reopened).
    #[tokio::test]
    async fn shrink_debt_drains_as_inflight_prefills_finish() {
        // OUR OWN throttle. No process-global, so these tests are order-INDEPENDENT and can
        // run in parallel — the whole reason `with_lanes` exists.
        let t = PrefillThrottle::with_lanes(1);
        t.publish_serving(2 * GB, 4);
        grow_to(&t, 4, 13 * GB);

        // 3 prefills in flight; heavy pressure wants target 1.
        let a = t.acquire_prefill_slot().await;
        let b = t.acquire_prefill_slot().await;
        let c = t.acquire_prefill_slot().await;
        // (4−2)/2 = 1: only the 1 idle permit is collectable now → installed 4→3, debt 2.
        assert_eq!(
            t.reconcile(4 * GB),
            3,
            "collects the idle permit; in-flight can't be revoked"
        );

        drop(a); // one prefill finishes → its permit returns → collectable
        assert_eq!(t.reconcile(4 * GB), 2, "debt drains as calls finish");
        drop(b);
        assert_eq!(
            t.reconcile(4 * GB),
            1,
            "down to the target — one lane always runs"
        );

        // Floor: even under absurd pressure the gate never goes below 1 (a resident model
        // may always run one prefill — going below is a residency decision, not admission).
        drop(c);
        assert_eq!(t.reconcile(0), 1, "never below one");
    }
}
