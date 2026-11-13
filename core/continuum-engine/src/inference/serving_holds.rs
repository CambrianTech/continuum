//! The serving HOLD an activity takes on the live lane: LUDICROUS (serve at Performance for a
//! benchmark's whole run). The STEADY hold (no re-home under an exam) went with the re-home:
//! the engine launches once and nothing relaunches it for size (2026-11-03).
//! Engine facts with RAII guards: an eval in the mind takes them, the serving daemon reads
//! them on its tick, and neither names the other's module.

use std::sync::atomic::{AtomicUsize, Ordering};

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
