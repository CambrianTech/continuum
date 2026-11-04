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
}

/// What the last plan sized against: the launch lays its pool out on `usable_bytes`, and the one
/// upward growth trusts it only when `credit` says the engine's own bytes were in it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PlannedBudget {
    pub(super) usable_bytes: u64,
    pub(super) credit: OwnCredit,
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
