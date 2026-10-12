//! GATES: whether a plan may serve below the host floor, downshift, admit a lane, or pin a model.
//! Split out of serving_daemon.rs (card dbc9711c, BigMama's step 6).

use super::*;

/// Verdict of [`downshift_gate`]: does adopting this plan REDUCE the served
/// base model's capability while the incumbent is still on disk?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DownshiftVerdict {
    /// Same model, an upshift, no incumbent, or incumbent gone from disk —
    /// adopt immediately, nothing to debounce.
    NotADownshift,
    /// The fresh plan wants a LESS capable base than a still-present incumbent.
    /// Only a sustained run of these justifies acting (#368).
    Downshift,
}

/// Pure classification — split from [`ServingDaemonModule::publish_plan`] so the
/// debounce decision is unit-testable without a daemon. A downshift requires the
/// incumbent to still be a candidate (still on disk): if its weights vanished,
/// holding a plan that names them would be serving a ghost, so that case adopts
/// `fresh` immediately (mirrors `plan_serving_stable`'s own disk check).
/// The host floor as a pure function of the physical budget, what is on disk, and the
/// demand: [`plan_serving`]'s own choice at that budget. Reusing the selection law is
/// the point — the floor and the plan disagree only because their BUDGETS differ, never
/// because a second ranking was invented.
pub(super) fn host_floor_of(
    physical: HostBudget,
    on_disk: &[ModelFootprint],
    demand: &ServingDemand,
) -> Option<ModelFootprint> {
    plan_serving(&physical, on_disk, &demand)
        .filter(|p| p.fits_on_gpu)
        .map(|p| p.base_model)
}

/// Verdict of [`floor_gate`]: may this plan be adopted on this host at all?
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FloorVerdict {
    /// At or above the host floor (or the host has no GPU floor — CPU serving).
    AtOrAbove,
    /// The plan's base ranks below what the host's physical capacity serves.
    Below { floor: String, wanted: String },
}

/// Pure classification — a plan whose base is LESS capable than the host floor is a
/// demotion below physical capacity, whatever the live budget said this tick.
pub(super) fn floor_gate(plan: &ServingPlan, floor: Option<&ModelFootprint>) -> FloorVerdict {
    match floor {
        Some(floor) if plan.base_model.capability_rank < floor.capability_rank => {
            FloorVerdict::Below {
                floor: floor.model_id.clone(),
                wanted: plan.base_model.model_id.clone(),
            }
        }
        _ => FloorVerdict::AtOrAbove,
    }
}

/// How long a below-floor plan with NO incumbent is held before it is served. Bounded
/// from both sides by two incidents a day apart: the 5090's teardown transient (ONE low
/// reading on a 53 GB card while a predecessor died — latched into an 80-minute 0.5B
/// plan once) resolves within a tick or two; the Intel tier's real shortfall held the
/// same floor/wanted pair for 40+ minutes and stayed dark for 90. Anything from ~30 s to
/// ~5 min satisfies both; the value is not delicate and needs no tuning (Cormac's
/// sizing, 2026-09-17).
pub const COLD_BOOT_BELOW_FLOOR_GRACE: std::time::Duration = std::time::Duration::from_secs(180);

/// Does a below-floor plan get REFUSED?
///
/// The floor exists so a capable host does not SINK below its capability — but sinking
/// needs something to sink FROM. With an incumbent: always refuse (a host never sinks
/// below its floor, unchanged). With none: refuse while the shortfall is younger than
/// [`COLD_BOOT_BELOW_FLOOR_GRACE`] — a teardown transient must not latch a small model
/// onto a big card — and past it SERVE, because refusing into "the previous plan stands"
/// with no previous plan boots the node dark (card 48f5438a: resident 0, active_model
/// None while a servable 1.5B sat on disk). A viable base beats no base.
/// The plan that ADOPTS an inherited engine when a demotion was refused and there is no
/// previous plan to retain: the incumbent alone, at the geometry it is running (its
/// last steady record), against the PHYSICAL budget — its residency is ours. `None`
/// only when the incumbent is not a candidate or cannot fit one lane physically.
pub(super) fn incumbent_adoption_plan(
    physical: &HostBudget,
    candidates: &[ModelFootprint],
    incumbent: &str,
    demand: &ServingDemand,
    geometry: Option<(u32, u32)>,
) -> Option<ServingPlan> {
    let footprint = candidates.iter().find(|c| c.model_id == incumbent)?;
    let demand = demand.clone().with_boot_geometry(geometry);
    plan_serving(physical, std::slice::from_ref(footprint), &demand)
}

pub(super) fn below_floor_refuses(
    verdict: &FloorVerdict,
    has_incumbent: bool,
    below_for: std::time::Duration,
) -> bool {
    matches!(verdict, FloorVerdict::Below { .. })
        && (has_incumbent || below_for < COLD_BOOT_BELOW_FLOOR_GRACE)
}

pub(super) fn downshift_gate(
    plan: &ServingPlan,
    incumbent: Option<&str>,
    candidates: &[ModelFootprint],
) -> DownshiftVerdict {
    let Some(inc_id) = incumbent else {
        return DownshiftVerdict::NotADownshift;
    };
    if plan.base_model.model_id == inc_id {
        return DownshiftVerdict::NotADownshift;
    }
    let (Some(inc), Some(new)) = (
        candidates.iter().find(|m| m.model_id == inc_id),
        candidates
            .iter()
            .find(|m| m.model_id == plan.base_model.model_id),
    ) else {
        return DownshiftVerdict::NotADownshift;
    };
    if new.capability_rank < inc.capability_rank {
        DownshiftVerdict::Downshift
    } else {
        DownshiftVerdict::NotADownshift
    }
}
