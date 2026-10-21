//! Holds ownership for the serving daemon.

use super::*;

/// The serving host budget from the GOVERNED board ALONE — the memory authority's
/// pre-staged snapshot (Vram `available`, netted over every measured consumer + external
/// pressure), never a raw GPU probe. This is the ONE budget an ephemeral eval/train lane
/// sizes against at spawn (MEMORY-AUTHORITY-DAEMON slice 2): reading the board is reading a
/// snapshot the authority maintains on ITS tick, not sampling memory on the spawn hot path.
/// Board-only (no `SystemResourceMonitor` needed), so any consumer holding the
/// `Arc<ResourceDaemon>` sizes a lane through the SAME `plan_serving` the autonomic serving
/// plan uses — one budget, one authority, no duplicate calc.
/// Process-global count of active "hold the live serving lane STEADY" requests. A
/// living-persona benchmark/eval runs ON the shared lane as a co-tenant decode slot —
/// `resources::placement::Placement::ShareLane` (same base already resident ⇒ no second
/// weight copy). The one thing that breaks it is a grow-back RE-HOME relaunch mid-exam:
/// it connection-refuses every in-flight generation (glass-boxed 2026-07-20: hard-rs 0/8,
/// zero output tokens, the lane bounced under the exam). While any hold is active the
/// daemon SKIPS the OPTIONAL grow-back re-home for an already-correctly-served lane. It
/// does NOT suppress a model/genome CHANGE or a pressure-driven shrink — a real resource
/// emergency still preempts (rare, correct). Zero-cost when no eval runs.
pub(super) static SERVING_STEADY_HOLDS: AtomicUsize = AtomicUsize::new(0);

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
    pub(super) holder: String,
}

impl ServingSteadyHold {
    pub fn acquire(holder: impl Into<String>) -> Self {
        let holder = holder.into();
        let holds = SERVING_STEADY_HOLDS.fetch_add(1, Ordering::AcqRel) + 1;
        crate::probe!(
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
        crate::probe!(
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
/// not the timid pressure-derived fraction. While any caller holds this, [`host_budget`]
/// forces `PowerMode::Performance` (fraction 0.96 — nearly the whole GPU, less the process's own overhead), OVERRIDING
/// `serving_mode_for_pressure`'s conservative read (which on UMA under-reports free memory
/// and floored a 47872-capable model to 2048 with ~28GB idle). This is the drive mode
/// following the ACTIVITY, not just the pressure: a declared Ludicrous intent → serve
/// maximal. Non-thrashing: one grow-relaunch when the hold is taken, one shrink when it
/// drops — bookends around the exam, not a flap. Zero-cost when nothing holds it.
/// [[serving-mode-follows-activity-ludicrous-to-dream]] [[benchmark-window-must-be-big-not-a-clamped-prompt]]
pub(super) static SERVING_LUDICROUS_HOLDS: AtomicUsize = AtomicUsize::new(0);

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

/// True while at least one caller demands Ludicrous (Performance) serving — [`host_budget`]
/// pins `PowerMode::Performance` regardless of the pressure read.
/// Last VRAM ceiling the governed board actually reported this process, and when.
/// The middle rung of the substitute-value ladder: when the board goes quiet, an old
/// REAL number keeps the planner honest where a fabricated zero floors it.
/// The pool the main lane's bytes live in — the ONE decision every serving site
/// keyed by a resource kind reads through: the plan's ceiling, the replace-myself
/// credit, the credited-once check and the consumer's own attribution. A CPU-placed
/// lane (`--n-gpu-layers 0`, #3729's Intel-Mac arm) puts weights AND KV in system
/// RAM, so all four belong to the `Ram` row, not the `Vram` row of a card holding
/// none of them. #4169 made the physical bound follow the placement; this makes the
/// PLAN follow it. Card 7c72c0f0 (Intel Mac, 2026-09-17): nine citizens sized against
/// a 5.7 GB Vram row on a box with 34.4 GB of RAM and 0 MB of VRAM in use — one warm
/// slot, eight exiled to a remote lane that timed out at 600 s.
pub fn serving_pool_kind() -> ResourceKind {
    pool_kind_for(crate::inference::llama_server::main_lane_placement())
}

/// Pure so the mapping is assertable without config: where the placement puts the
/// bytes is which row governs them.
pub fn pool_kind_for(placement: crate::inference::llama_server::LanePlacement) -> ResourceKind {
    use crate::inference::llama_server::LanePlacement;
    match placement {
        LanePlacement::Cpu => ResourceKind::Ram,
        LanePlacement::Gpu => ResourceKind::Vram,
    }
}

pub fn serving_ludicrous_active() -> bool {
    SERVING_LUDICROUS_HOLDS.load(Ordering::Acquire) > 0
}
