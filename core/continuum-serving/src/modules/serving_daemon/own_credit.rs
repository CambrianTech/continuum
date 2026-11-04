//! Whether serving's own resident bytes are already inside a plan's budget: ONE typed fact, read
//! with the budget it describes and matched on by everyone who plans from that budget.
//!
//! The plan has two arms that differ by one engine footprint (`planning`): `at_rest` trusts that
//! the budget already holds serving's own bytes, `stable` credits the incumbent's weights itself.
//! Which arm ran used to be a bool read off the board (`serving_footprint_reported`) at another
//! moment than the budget, and the budget's drive mode was picked from a memory-pressure reading
//! that never saw the credit at all. The M5, 2026-11-04 16:41Z: a core-only restart onto an
//! engine grown to `-c 654,295` read system memory below the Eco line (the engine's own bytes
//! were most of what it was short of), and planned Eco's 0.55 of a budget that was that engine
//! alone: `credited_bytes=36,051,076,408 usable_bytes=19,828,092,024` (exactly 0.55 of it),
//! below the model's 20,419,565,568 bytes of weights. No candidate could speak, the starved plan
//! retired its record, and no resident was hosted for 9 minutes. The credit was on the board and
//! in the budget; the mode took 45% of it back. A fresh engine never showed it: at its boot the
//! memory it would fill was still free, so the pressure read Comfort.

use super::*;
use crate::capacity::power_mode::{serving_mode_for_pressure_from, PowerMode};
use crate::gpu::monitor::MemoryMode;
use crate::inference::planning::serving_plan::TracedPlan;

/// Whether a plan's budget already holds serving's own resident bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum OwnCredit {
    /// The replace-myself read added serving's reported footprint back: the plan credits
    /// nothing more (twice planned 2 x 70,093 on a 5090 whose fit was 2 x 47,098, 2026-09-17).
    Included { own_bytes: u64 },
    /// Serving has reported no footprint yet (the first ticks of a boot over an inherited lane,
    /// #438): the budget is free memory alone and the plan credits the incumbent itself.
    ToApply,
}

impl OwnCredit {
    /// The fact from the read that produced the budget: its own bytes, or none reported.
    pub(super) fn of_own_bytes(own_bytes: u64) -> Self {
        match own_bytes {
            0 => Self::ToApply,
            own_bytes => Self::Included { own_bytes },
        }
    }

    /// The planning arm this credit selects, for the `serving.plan.branch` probe.
    pub(super) fn arm(self) -> &'static str {
        match self {
            Self::Included { .. } => "at_rest",
            Self::ToApply => "stable",
        }
    }

    /// The bytes the budget already credited back (0 when none were).
    pub(super) fn own_bytes(self) -> u64 {
        match self {
            Self::Included { own_bytes } => own_bytes,
            Self::ToApply => 0,
        }
    }

    /// The memory free if serving released what it holds: the pressure reading in the budget's
    /// own replace-myself view, so a past form of ourself is never read as an external squeeze.
    /// Only bytes that live in system memory are added back to a system-memory reading; a
    /// discrete card's VRAM never was in it.
    pub(super) fn pressure_available(self, system_available: u64, own_in_system_memory: bool) -> u64 {
        match self {
            Self::Included { own_bytes } if own_in_system_memory => system_available.saturating_add(own_bytes),
            Self::Included { .. } | Self::ToApply => system_available,
        }
    }
}

/// Whether the serving pool's bytes are system memory: a CPU-placed lane (the Ram row) or a GPU
/// on unified memory. An unreported memory mode credits nothing, as before this fact existed.
pub(super) fn own_bytes_in_system_memory(pool: ResourceKind, memory: Option<MemoryMode>) -> bool {
    matches!(pool, ResourceKind::Ram) || matches!(memory, Some(MemoryMode::Unified))
}

/// The pressure drive mode, from the reading in the same view as the budget it scales.
pub(super) fn pressure_mode(
    system_available: u64,
    credit: OwnCredit,
    own_in_system_memory: bool,
    previous: Option<PowerMode>,
) -> PowerMode {
    serving_mode_for_pressure_from(credit.pressure_available(system_available, own_in_system_memory), previous)
}

/// The plan's budget, the credit fact about it, and the leases it was charged: one replace-myself
/// read, carried together so no reader pairs them across moments.
#[derive(Debug, Clone)]
pub(super) struct PlanBudget {
    pub(super) host: HostBudget,
    pub(super) credit: OwnCredit,
    pub(super) leases: Vec<crate::resources::lease::ResourceLease>,
    /// The reading a launch may commit a pool from: settled, or not yet.
    pub(super) launch: BudgetReading,
}

/// What the last plan sized against: its sustained `usable_bytes` and `credit`, and the `launch`
/// reading the one launch lays its pool out on (only when settled).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PlannedBudget {
    pub(super) usable_bytes: u64,
    pub(super) credit: OwnCredit,
    pub(super) launch: BudgetReading,
}

/// Whether a budget reading may size the engine's one KV pool. The pool is fixed at launch
/// (`--kv-unified`, `-c`) and kept for the engine's life, so it is committed only from a reading
/// that would not change, never from the transient of a view change.
///
/// The M5, 2026-11-04 20:15:16-18Z (87eccb3a7, no growth relaunch left to repair it): the cold
/// boot's first plans read `candidate_cannot_speak` x3, then `serving.plan.branch credited_bytes=0
/// usable_bytes=22,785,477,836` on a 128 GB machine, and the engine launched `-c 125640
/// --parallel 5` from it two seconds into the boot. It held 125,696 for its life and starved
/// three residents; at 18:49 the steady plan of the same machine had proved 523,520. The same
/// rising shape on 2026-10-18 15:27:47-15:28:09Z: 22.9, 29.8, 29.1, then 41.9 GB within 22 s.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum BudgetReading {
    /// The sustained minimum of the readings taken after the view's first window: what memory
    /// holds once the boot, load or teardown that changed the view has had one relaunch's
    /// duration to finish.
    Settled(u64),
    /// Inside a view's first window (a cold boot, an engine appearing or leaving): no pool is
    /// committed from it. A running engine of the same model keeps its own; nothing launches.
    Transient,
}

/// The plan budget's sustained minimum (one relaunch's duration, `budget.rs`), kept within one
/// credit view. A reading taken before serving's attribution landed is free memory alone; one
/// taken after holds the engine too. They are not the same quantity, so a minimum across them
/// held the plan at the uncredited floor for a whole window after the credit arrived. A change
/// of view starts the history over; within a view a fall still shows at once and a rise still
/// waits out the window.
///
/// The same history decides whether a reading is [`BudgetReading::Settled`]: a view's first
/// window is its transient (a boot, an engine loading or leaving; the window is the time an
/// engine is given to load), so the launch reading is the sustained minimum of only the readings
/// after it. The plan's own minimum still holds every reading: the plan is runtime and moves, the
/// launch pool does not.
#[derive(Debug)]
pub(super) struct CreditedReadings {
    window: std::time::Duration,
    view: Option<std::mem::Discriminant<OwnCredit>>,
    /// When the current view's first reading was taken.
    view_since_ms: u64,
    readings: crate::system_resources::sustained::Sustained,
    /// The current view's readings taken after its first window.
    settled: crate::system_resources::sustained::Sustained,
}

/// One observation of the plan budget: the plan's sustained minimum and the launch reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Observed {
    pub(super) sustained_min: u64,
    pub(super) launch: BudgetReading,
}

impl CreditedReadings {
    pub(super) fn new(window: std::time::Duration) -> Self {
        use crate::system_resources::sustained::Sustained;
        Self { window, view: None, view_since_ms: 0, readings: Sustained::new(window), settled: Sustained::new(window) }
    }

    /// Observe `value` at `now_ms` in `credit`'s view: the sustained minimum over the view's
    /// readings, and the launch reading (settled once the view has outlived its first window).
    pub(super) fn observe(&mut self, now_ms: u64, credit: OwnCredit, value: u64) -> Observed {
        use crate::system_resources::sustained::Sustained;
        let view = std::mem::discriminant(&credit);
        if self.view != Some(view) {
            self.view = Some(view);
            self.view_since_ms = now_ms;
            self.readings = Sustained::new(self.window);
            self.settled = Sustained::new(self.window);
        }
        let sustained_min = self.readings.observe(now_ms, value).min;
        let view_age = std::time::Duration::from_millis(now_ms.saturating_sub(self.view_since_ms));
        let launch = if view_age >= self.window {
            BudgetReading::Settled(self.settled.observe(now_ms, value).min)
        } else {
            BudgetReading::Transient
        };
        Observed { sustained_min, launch }
    }
}

/// The planning arm, chosen by the credit fact: the incumbent's bytes are credited exactly once,
/// by the ledger (`Included`) or here (`ToApply`), never both.
pub(super) fn plan_by_credit(
    credit: OwnCredit,
    budget: &HostBudget,
    candidates: &[ModelFootprint],
    incumbent: Option<&str>,
    demand: &ServingDemand,
) -> Option<TracedPlan> {
    match credit {
        OwnCredit::Included { .. } => plan_serving_at_rest_traced(budget, candidates, incumbent, demand),
        OwnCredit::ToApply => plan_serving_stable_traced(budget, candidates, incumbent, demand),
    }
}
