//! Policy ownership for the serving daemon.

use super::*;

/// How many consecutive ticks a plan must name a DIFFERENT model before a ready lane
/// swaps to it (the window and lane re-home this once also gated is gone: one launch,
/// 2026-11-03). It only has to outlast jitter, measured on the plan itself: across 6,322 live
/// `serving.plan` samples (probes.jsonl, 2026-08-06) the longest run of consecutive
/// qualifying rises was **2**, at a 10%, 15% or 20% margin alike — zero runs of 3 in
/// the entire sample. Real plan jitter spikes for a sample or two and falls back; it
/// does not climb. So 3 is the first value that provably rejects every observed
/// noise event, and 2 would not have. A dip RESETS the streak, so noise cannot
/// accumulate, while a genuine capacity change is honoured in 15s instead of being
/// stranded forever.
///
/// Lowering this to 2 re-admits the entire measured jitter tail. Do not, without
/// re-running that measurement against fresher receipts.
pub(super) const MODEL_CHANGE_SUSTAINED_TICKS: u32 = 3;

/// Consecutive plan ticks a base-model DOWNSHIFT must persist before it is
/// adopted. #368 (2nd occurrence, 2026-08-08): a ~6-second RAM transient at
/// agent/solve launch collapsed the planner's budget to zero for ONE tick, the
/// plan flipped to the 0.5B, reconcile actuated the flip (tore down Devstral,
/// re-homed every citizen), and the civilization lost 47 minutes to a phantom
/// model. The tear-down/spin-up/re-home cycle costs minutes, so reacting to a
/// squeeze shorter than [`TICK`]×this is ALWAYS a net loss — the highest-value
/// activity in the system must never be evicted by its own setup ripple (Joel:
/// "system working against rather than for the best activity work").
/// [[never-thrash-sticky-hysteresis-on-every-lane]]
pub(super) const DOWNSHIFT_SUSTAINED_TICKS: u32 = 3;

/// Which model the plan should treat as the incumbent, given what we have already PUBLISHED and
/// what we INHERITED from a previous generation of this core.
///
/// A published plan always wins: once this core has decided, its own decision is the incumbent
/// and the inherited lane is either already adopted or already reclaimed. The inherited lane
/// only speaks for the window where `plan_tx` is still empty — the first plan after boot — which
/// is precisely the window the #438 downshift lives in.
///
/// Deliberately does NOT verify that the inherited model is still on disk or still servable:
/// [`plan_serving_stable`] already refuses an incumbent that is not among the candidates
/// (falling straight through to a plain plan), so a stale record degrades to today's behaviour
/// instead of pinning a model that no longer exists.
pub(super) fn incumbent_for_plan(
    published: Option<String>,
    inherited: Option<&LaneRecord>,
) -> Option<String> {
    published.or_else(|| inherited.map(|rec| rec.model.clone()))
}

/// L10 (#438) pure gate: may the reconcile commit a BASE-MODEL swap this tick?
///
/// A swap tears down the lane and re-homes every persona to different weights, so a
/// plan that names a different model than the READY incumbent must persist for
/// `needs` consecutive ticks. Changing the *target* mid-streak restarts the count
/// (a plan flapping 0.5B->27B->0.5B never accumulates); a tick where plan == live
/// clears any pending change. Boot / wedge-recovery (no ready incumbent) commits
/// immediately — recovery must not wait out a streak.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ModelChangeGate {
    /// Not a ready-lane model change (same model, or no ready incumbent) — proceed.
    NotAChange,
    /// A change, but not yet sustained — do nothing this tick.
    Defer { streak: u32 },
    /// Sustained disagreement — commit the swap (streak consumed).
    Commit { streak: u32 },
}

pub(super) fn model_change_gate(
    live_ready: bool,
    live_model: Option<&str>,
    desired: &str,
    pending: &mut Option<String>,
    streak: &std::sync::atomic::AtomicU32,
    needs: u32,
) -> ModelChangeGate {
    use std::sync::atomic::Ordering;
    if !(live_ready && live_model.is_some() && live_model != Some(desired)) {
        streak.store(0, Ordering::Relaxed);
        *pending = None;
        return ModelChangeGate::NotAChange;
    }
    let n = if pending.as_deref() == Some(desired) {
        streak.fetch_add(1, Ordering::Relaxed).saturating_add(1)
    } else {
        *pending = Some(desired.to_string());
        streak.store(1, Ordering::Relaxed);
        1
    };
    if n < needs {
        ModelChangeGate::Defer { streak: n }
    } else {
        streak.store(0, Ordering::Relaxed);
        *pending = None;
        ModelChangeGate::Commit { streak: n }
    }
}

/// The model whose decode knee bounds the plan: the one a lane is serving, else the one
/// this box served last (card 29e4ab34 — the boot window before a lane reports its model
/// is not "no knee", it is the remembered model's knee). `remembered` is read lazily: it
/// is a file, and it is only consulted in the boot window, never on every plan tick.
pub fn knee_model(
    active: Option<String>,
    remembered: impl FnOnce() -> Option<String>,
) -> Option<String> {
    active.or_else(remembered)
}

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
    let demand = demand
        .clone()
        .with_sticky_window(geometry.map(|(window, _)| window));
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
