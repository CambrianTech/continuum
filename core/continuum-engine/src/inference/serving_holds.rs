//! The serving HOLDS an activity takes on the live lane: STEADY (no optional re-home while
//! an exam runs on it) and LUDICROUS (serve at Performance for a benchmark's whole run).
//! Engine facts with RAII guards: an eval in the mind takes them, the serving daemon reads
//! them on its tick, and neither names the other's module.

use std::sync::atomic::{AtomicUsize, Ordering};

/// Process-global count of active "hold the live serving lane STEADY" requests. A
/// living-persona benchmark/eval runs ON the shared lane as a co-tenant decode slot —
/// `resources::placement::Placement::ShareLane` (same base already resident ⇒ no second
/// weight copy). The one thing that breaks it is a grow-back RE-HOME relaunch mid-exam:
/// it connection-refuses every in-flight generation (glass-boxed 2026-07-20: hard-rs 0/8,
/// zero output tokens, the lane bounced under the exam). While any hold is active the
/// daemon SKIPS the OPTIONAL grow-back re-home for an already-correctly-served lane. It
/// does NOT suppress a model/genome CHANGE or a pressure-driven shrink — a real resource
/// emergency still preempts (rare, correct). Zero-cost when no eval runs.
static SERVING_STEADY_HOLDS: AtomicUsize = AtomicUsize::new(0);

/// RAII: hold the live serving lane steady (suppress the grow-back re-home) until dropped.
/// A living-persona eval binds one for its whole run — the concrete form of `ShareLane`'s
/// "pin the lane for the demand's duration" clause ([[benchmark-is-a-governor-preemption-lease]],
/// the co-tenant/steady case: no second weight copy, just don't bounce the lane).
#[must_use = "the hold releases the instant this guard drops — bind it for the eval's lifetime"]
pub struct ServingSteadyHold {
    /// WHO pinned the lane — a run id, "eval", etc. Carried so the acquire and
    /// release EVENTS name the holder (Joel 2026-08-08: "emit events for
    /// everything — need to know"): a suppressed relaunch with no event is
    /// indistinguishable from a planner that never wanted one, and a leaked
    /// hold with no holder name is unattributable.
    holder: String,
}

impl ServingSteadyHold {
    pub fn acquire(holder: impl Into<String>) -> Self {
        let holder = holder.into();
        let holds = SERVING_STEADY_HOLDS.fetch_add(1, Ordering::AcqRel) + 1;
        continuum_base::probe!(
            target: "continuum_core::modules::serving_daemon", class = "serving.lane.hold",
            action = "acquired",
            holder = %holder,
            holds,
            "serving lane pinned STEADY (optional grow-back re-home suppressed while held)"
        );
        Self { holder }
    }
}

impl Drop for ServingSteadyHold {
    fn drop(&mut self) {
        let holds = SERVING_STEADY_HOLDS.fetch_sub(1, Ordering::AcqRel) - 1;
        continuum_base::probe!(
            target: "continuum_core::modules::serving_daemon", class = "serving.lane.hold",
            action = "released",
            holder = %self.holder,
            holds,
            "serving lane steady-hold released"
        );
    }
}

/// True while at least one caller holds the live lane steady — the reconcile's grow-back
/// re-home is suppressed for an already-correctly-served lane.
pub fn serving_held_steady() -> bool {
    SERVING_STEADY_HOLDS.load(Ordering::Acquire) > 0
}

/// LUDICROUS mode (Joel 2026-07-21: "extreme mode for benchmarks or ludicrous lol"). A
/// benchmark / project / "the fight" wants the biggest window the model+machine can give —
/// not the timid pressure-derived fraction. While any caller holds this, the serving daemon's `host_budget`
/// forces `PowerMode::Performance` (fraction 0.96 — nearly the whole GPU, less the process's own overhead), OVERRIDING
/// `serving_mode_for_pressure`'s conservative read (which on UMA under-reports free memory
/// and floored a 47872-capable model to 2048 with ~28GB idle). This is the drive mode
/// following the ACTIVITY, not just the pressure: a declared Ludicrous intent → serve
/// maximal. Non-thrashing: one grow-relaunch when the hold is taken, one shrink when it
/// drops — bookends around the exam, not a flap. Zero-cost when nothing holds it.
/// [[serving-mode-follows-activity-ludicrous-to-dream]] [[benchmark-window-must-be-big-not-a-clamped-prompt]]
static SERVING_LUDICROUS_HOLDS: AtomicUsize = AtomicUsize::new(0);

/// RAII: while held, serving plans at `PowerMode::Performance` (the whole GPU, biggest
/// window). A benchmark binds one for its whole run so the exam is measured on the largest
/// window the model+machine allow — never a starved boot-window. Drop reverts to the live
/// pressure-adaptive mode.
#[must_use = "the Ludicrous hold releases the instant this guard drops — bind it for the benchmark's lifetime"]
pub struct ServingLudicrousHold(());

impl ServingLudicrousHold {
    pub fn acquire() -> Self {
        SERVING_LUDICROUS_HOLDS.fetch_add(1, Ordering::AcqRel);
        Self(())
    }
}

impl Drop for ServingLudicrousHold {
    fn drop(&mut self) {
        SERVING_LUDICROUS_HOLDS.fetch_sub(1, Ordering::AcqRel);
    }
}

/// True while at least one caller demands Ludicrous (Performance) serving — the serving daemon's `host_budget`
/// pins `PowerMode::Performance` regardless of the pressure read.
pub fn serving_ludicrous_active() -> bool {
    SERVING_LUDICROUS_HOLDS.load(Ordering::Acquire) > 0
}
