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
        let lanes = super::resource_admission::boot_lane_count();
        PrefillThrottle {
            sem: Arc::new(tokio::sync::Semaphore::new(lanes)),
            installed: AtomicUsize::new(lanes),
            spike_bytes: AtomicU64::new(0),
            want_lanes: AtomicUsize::new(lanes),
            grow_streak: AtomicUsize::new(0),
            reconcile_lock: Mutex::new(()),
        }
    })
}

/// Publish the served plan's demand facts: the per-prefill transient spike and the lane
/// count. Called by the serving daemon on every plan publish, right where it publishes the
/// lane count — ONE source of truth, no second path.
pub fn publish_serving(spike_bytes: u64, lanes: usize) {
    let t = throttle();
    t.spike_bytes.store(spike_bytes, Ordering::Release);
    t.want_lanes.store(lanes.max(1), Ordering::Release);
}

/// Re-derive the concurrent-prefill grant from LIVE free GPU bytes and apply it to the gate.
/// Called on the serving daemon's tick (cheap: watch read + atomics + at most one semaphore
/// op). Returns the applied target for probes/tests.
///
/// The fit rule is the SAME `capacity::FitPolicy` the simulator proves scenarios against —
/// sim == prod at the policy seam. Safety margin: one spike of headroom, so measurement
/// jitter of one lane never turns a fit into an overflow.
pub fn reconcile(gpu_free_bytes_live: u64) -> usize {
    let t = throttle();
    let spike = t.spike_bytes.load(Ordering::Acquire);
    let want = t.want_lanes.load(Ordering::Acquire).max(1);
    if spike == 0 {
        // No plan published yet — no fit facts, hold at the lane count (pre-throttle behavior).
        return t.apply(want);
    }
    let cap = DeviceCapacity {
        // The fit rule only reads the live-free axis; the other axes are not sourced here.
        gpu_total_bytes: 0,
        gpu_free_bytes_live,
        system_ram_free_bytes: 0,
    };
    let req = LeaseRequest {
        consumer: "prefill".into(),
        want_concurrency: want as u32,
        spike_bytes: spike,
    };
    let grant = FitPolicy { safety_margin_bytes: spike }.grant(&cap, &req);
    t.apply(grant.concurrency as usize)
}

impl PrefillThrottle {
    /// Apply a target permit count, asymmetrically: SHRINK is instant (collect what's idle
    /// now, leave the rest as debt later reconciles drain as in-flight prefills finish);
    /// GROW only lands after [`GROW_STICKINESS_TICKS`] consecutive reconciles wanted it —
    /// sustained headroom, not one optimistic reading. Never blocks, never revokes running
    /// work, never thrashes on a boundary-riding live number.
    fn apply(&self, target: usize) -> usize {
        let target = target.max(1); // a resident model may always run ONE prefill (residency decision)
        let _g = self.reconcile_lock.lock().expect("prefill reconcile lock never poisoned");
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
                self.installed.store(installed - forgotten, Ordering::Release);
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
    throttle()
        .sem
        .clone()
        .acquire_owned()
        .await
        .expect("prefill semaphore is never closed")
}

/// Permits currently installed (post-debt) — for probes and tests.
pub fn installed_permits() -> usize {
    throttle().installed.load(Ordering::Acquire)
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1024 * 1024 * 1024;

    /// These are the only tests that touch the process-global throttle, and they MUST NOT
    /// run concurrently: both hold permits from the one semaphore, so test A's shrink can
    /// steal the permits test B is awaiting → deadlock (observed live, exit 144). Each test
    /// takes this lock first, then establishes its full state (publish + reconcile), so
    /// order doesn't matter.
    static TEST_SERIAL: Mutex<()> = Mutex::new(());

    /// Force a known state regardless of sibling-test order: instant-shrink to the floor
    /// (works because shrink has no hysteresis), then grow to `n` by sustaining headroom.
    fn grow_to(n: usize, free: u64) -> usize {
        assert_eq!(reconcile(0), 1, "floor reset is instant");
        let mut last = 1;
        for _ in 0..GROW_STICKINESS_TICKS {
            last = reconcile(free);
        }
        assert_eq!(last, n, "sustained headroom grows to the fit");
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
        let _serial = TEST_SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        publish_serving(2 * GB, 4);
        grow_to(4, 13 * GB);

        // Game opens: shrink lands on the FIRST reconcile that sees the pressure.
        assert_eq!(reconcile(7 * GB), 2, "shrink is instant: (7−2)/2 = 2");

        // Game closes: one good reading does NOT regrow (boundary-riding wobble)…
        assert_eq!(reconcile(13 * GB), 2, "one optimistic tick is not recovery");
        assert_eq!(reconcile(13 * GB), 2, "nor two");
        // …and a dip in between resets the streak — the signal must be SUSTAINED.
        assert_eq!(reconcile(7 * GB), 2, "a relapse resets the grow streak");
        assert_eq!(reconcile(13 * GB), 2);
        assert_eq!(reconcile(13 * GB), 2);
        assert_eq!(reconcile(13 * GB), 4, "three consecutive good ticks → regrown to demand");

        // The applied grant is enforced, not advisory: under pressure only 2 slots grant.
        assert_eq!(reconcile(7 * GB), 2);
        let a = acquire_prefill_slot().await;
        let b = acquire_prefill_slot().await;
        assert!(
            throttle().sem.clone().try_acquire_owned().is_err(),
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
        let _serial = TEST_SERIAL.lock().unwrap_or_else(|p| p.into_inner());
        publish_serving(2 * GB, 4);
        grow_to(4, 13 * GB);

        // 3 prefills in flight; heavy pressure wants target 1.
        let a = acquire_prefill_slot().await;
        let b = acquire_prefill_slot().await;
        let c = acquire_prefill_slot().await;
        // (4−2)/2 = 1: only the 1 idle permit is collectable now → installed 4→3, debt 2.
        assert_eq!(reconcile(4 * GB), 3, "collects the idle permit; in-flight can't be revoked");

        drop(a); // one prefill finishes → its permit returns → collectable
        assert_eq!(reconcile(4 * GB), 2, "debt drains as calls finish");
        drop(b);
        assert_eq!(reconcile(4 * GB), 1, "down to the target — one lane always runs");

        // Floor: even under absurd pressure the gate never goes below 1 (a resident model
        // may always run one prefill — going below is a residency decision, not admission).
        drop(c);
        assert_eq!(reconcile(0), 1, "never below one");
    }
}
