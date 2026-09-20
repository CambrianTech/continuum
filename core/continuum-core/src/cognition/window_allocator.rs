//! The window allocator: lanes × window from DECLARED REQUIREMENTS against a budget,
//! in one pass, by reference — never from a median of what a starved window let
//! through.
//!
//! Joel, 2026-09-20, reading the 5090 at 2 × 2048 refusing 2,932 prompts (p50 56k):
//! *"2k context is a completely waste of a lane. Persona should have 50–300k. Context
//! must go along with required governed size."* and *"It doesn't govern honestly or
//! reason out what best fits."*
//!
//! The planner it replaces derived the served window every tick from the median of
//! the prompts turns SENT — a figure bounded by the window itself — floored at the
//! hardware runnability floor (2048) and re-derived from registries twice per tick.
//! Each outage added a constant. This module is the model of the problem instead:
//!
//! **Inputs** — what each mind REQUIRES (`LaneRequirement`, a governed size like
//! memory: declared on the mind, or until that is wired, the untrimmed demand she
//! assembles; a mind with no turn yet is `Unknown` and resolves to the largest
//! requirement on the seat, else the model's own trained window — there is no
//! constant here, and there must not be: a floor is the next 2048), the candidate
//! models' footprints, and the budget the governor hands serving.
//!
//! **Objective**, in order — (1) every served mind holds her requirement; (2) as
//! many minds served warmly as fit; (3) the most capable model that satisfies (1)
//! and (2) for at least one mind. **Never** a lane below a requirement: the
//! allocator sheds minds (they queue or place on the grid) before it shrinks the
//! window, and sheds the model before it serves nobody.
//!
//! **One window per engine** (llama-server's `--parallel N` slots share a context
//! size), so the served window is the largest requirement among the minds served,
//! and the minds served are chosen smallest-requirement-first — the packing that
//! serves the most minds at their requirement within the budget.
//!
//! **Once, by reference.** `allocate` is pure over `&[LaneRequirement]`,
//! `&[ModelFootprint]`, `&HostBudget`; the daemon computes it when an input changes
//! and hands the one `Allocation` on. Measurement (sent sizes, refusals) is
//! telemetry that may raise a requirement; it is not what the window is derived from.
//!
//! The arithmetic (KV per token, the prefill compute reserve, the fixed per-lane
//! bytes) is [`ModelFootprint`]'s own — reused, not re-derived.

use super::serving_plan::{HostBudget, ModelFootprint, MAX_LANES};
use uuid::Uuid;

/// Where a requirement came from — carried so a receipt can say whether the mind
/// declared it, the substrate measured it, or nothing was known.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RequirementSource {
    /// Declared on the mind (her recipe/genome): the governed size.
    Declared,
    /// The untrimmed demand her turns assemble — what she NEEDS, never what a window
    /// let her send.
    MeasuredDemand,
    /// No turn measured yet: resolved by the allocator to the largest requirement on
    /// the seat, else the model's trained window (what fits at one lane).
    Unknown,
}

/// What one mind requires of a lane.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct LaneRequirement {
    pub persona: Uuid,
    /// Tokens the lane must hold for one of her turns: prompt + completion reserve.
    /// `None` = not yet measured (`RequirementSource::Unknown`).
    pub window: Option<u32>,
    pub source: RequirementSource,
}

impl LaneRequirement {
    /// A requirement from the untrimmed demand a mind assembles, with headroom. Zero
    /// (no turn yet) is `Unknown`, never a number. The interim source until the
    /// declared field is wired; the allocator does not distinguish.
    pub fn from_demand(persona: Uuid, demand_tokens: u32, headroom: f64) -> Self {
        if demand_tokens == 0 {
            return Self { persona, window: None, source: RequirementSource::Unknown };
        }
        Self { persona, window: Some((demand_tokens as f64 * headroom) as u32), source: RequirementSource::MeasuredDemand }
    }

    pub fn declared(persona: Uuid, window: u32) -> Self {
        Self { persona, window: Some(window), source: RequirementSource::Declared }
    }

    /// The window this requirement resolves to: its own, else the largest known on
    /// the seat, else the most `fp` can hold at `lanes` within `budget` (its trained
    /// window bounded by the fit) — the honest cold start, not a number.
    fn resolve(&self, largest_known: Option<u32>, fp: &ModelFootprint, budget: u64, lanes: u32) -> u32 {
        self.window.or(largest_known).unwrap_or_else(|| fp.window_within(budget, lanes))
    }
}

/// The allocator's answer: one engine geometry and who it serves.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Allocation {
    pub model_id: String,
    pub capability_rank: u8,
    /// The served per-slot window: the largest requirement among `served`.
    pub window: u32,
    pub lanes: u32,
    /// Minds with a warm slot at (≥) their requirement, smallest-requirement first.
    pub served: Vec<Uuid>,
    /// Minds the budget could not hold at their requirement: queue or grid-place.
    /// Never served at less.
    pub unserved: Vec<Uuid>,
    /// Bytes this geometry costs on the device (weights + KV + compute reserve).
    pub device_bytes: u64,
}

/// Why nothing could be allocated.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum Unallocatable {
    NoCandidates,
    NoRequirements,
    /// No candidate holds even ONE lane at the smallest requirement within the budget.
    NothingFitsOneLane { budget_bytes: u64, smallest_requirement: u32 },
}

/// Bytes a geometry costs: the footprint's own arithmetic.
fn device_bytes(fp: &ModelFootprint, window: u32, lanes: u32) -> u64 {
    fp.resident_bytes(window, lanes).saturating_add(fp.prefill_compute_reserve(window, lanes))
}

/// The most minds this model serves at their requirements within `budget`: walk the
/// requirements smallest-first, and keep adding a mind while the geometry
/// (lanes = count, window = her requirement, the largest so far) still fits.
/// Returns (served count, window) or None when not even one fits.
fn pack(fp: &ModelFootprint, sorted: &[&LaneRequirement], largest_known: Option<u32>, budget: u64, lane_cap: u32) -> Option<(u32, u32)> {
    let mut best: Option<(u32, u32)> = None;
    for (i, req) in sorted.iter().enumerate() {
        let lanes = (i as u32) + 1;
        if lanes > lane_cap {
            break;
        }
        let need = req.resolve(largest_known, fp, budget, lanes);
        let window = need.min(fp.context_window);
        if window < need {
            // The model cannot hold this mind's turn at all; nobody after her can be
            // served by it either (sorted ascending).
            break;
        }
        if device_bytes(fp, window, lanes) > budget {
            break;
        }
        best = Some((lanes, window));
    }
    best
}

/// Allocate lanes × window for `requirements` from `candidates` within `budget`.
///
/// Objective order: every served mind at her requirement; then the most minds; then
/// the most capable model. A model that serves fewer minds than a less capable one
/// loses — a warm slot for the roster outranks capability the roster cannot use
/// (the same trade `plan_serving` made; kept). Ties go to capability.
pub fn allocate(
    requirements: &[LaneRequirement],
    candidates: &[ModelFootprint],
    budget: &HostBudget,
) -> Result<Allocation, Unallocatable> {
    if candidates.is_empty() {
        return Err(Unallocatable::NoCandidates);
    }
    if requirements.is_empty() {
        return Err(Unallocatable::NoRequirements);
    }
    // Unknowns resolve to the largest known requirement on the seat (they sort after
    // it); with nothing known they resolve per candidate to its trained window.
    let largest_known = requirements.iter().filter_map(|r| r.window).max();
    let mut sorted: Vec<&LaneRequirement> = requirements.iter().collect();
    sorted.sort_by_key(|r| (r.window.unwrap_or(u32::MAX), r.persona));
    let lane_cap = (requirements.len() as u32).min(budget.perf_cores.max(1)).min(MAX_LANES).max(1);

    let mut best: Option<(&ModelFootprint, u32, u32)> = None;
    for fp in candidates {
        let Some((lanes, window)) = pack(fp, &sorted, largest_known, budget.usable_bytes, lane_cap) else { continue };
        let better = match best {
            None => true,
            Some((b, bl, _)) => lanes > bl || (lanes == bl && fp.capability_rank > b.capability_rank),
        };
        if better {
            best = Some((fp, lanes, window));
        }
    }
    let Some((fp, lanes, window)) = best else {
        return Err(Unallocatable::NothingFitsOneLane {
            budget_bytes: budget.usable_bytes,
            smallest_requirement: largest_known.unwrap_or(0), // unwrap_or: nothing measured = 0 in the receipt, "unknown" spelled as the number it is
        });
    };
    let served: Vec<Uuid> = sorted.iter().take(lanes as usize).map(|r| r.persona).collect();
    let unserved: Vec<Uuid> = sorted.iter().skip(lanes as usize).map(|r| r.persona).collect();
    Ok(Allocation {
        model_id: fp.model_id.clone(),
        capability_rank: fp.capability_rank,
        window,
        lanes,
        served,
        unserved,
        device_bytes: device_bytes(fp, window, lanes),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const GB: u64 = 1 << 30;

    /// The 27B the 5090 and the M5 serve: 19 GB of weights, ~33 KB/token KV (2.2 GB
    /// per 67k lane, measured), 256k trained window.
    fn qwen27b() -> ModelFootprint {
        ModelFootprint {
            model_id: "qwen-27b".into(),
            weights_bytes: 19 * GB,
            kv_per_token: 33_000,
            context_window: 262_144,
            capability_rank: 9,
            fixed_per_lane_bytes: 0,
        }
    }

    fn small7b() -> ModelFootprint {
        ModelFootprint {
            model_id: "small-7b".into(),
            weights_bytes: 5 * GB,
            kv_per_token: 8_000,
            context_window: 131_072,
            capability_rank: 4,
            fixed_per_lane_bytes: 0,
        }
    }

    fn budget(gb: u64) -> HostBudget {
        HostBudget { usable_bytes: gb * GB, perf_cores: 8 }
    }

    fn minds(windows: &[u32]) -> Vec<LaneRequirement> {
        windows.iter().map(|w| LaneRequirement::declared(Uuid::new_v4(), *w)).collect()
    }

    // what this catches: the 5090 as it stood (2026-09-20) — two coders at ~56k with
    // headroom, a 30 GB serving budget under a 19 GB model. The allocator serves BOTH
    // at their requirement (2 × 70k, ~26 GB), never 2 × 2048; and the window is the
    // largest served requirement, one per engine.
    #[test]
    fn the_5090_serves_two_coders_at_their_requirement_never_a_2k_lane() {
        let reqs = minds(&[70_000, 70_000]);
        let a = allocate(&reqs, &[qwen27b()], &budget(30)).expect("fits");
        assert_eq!((a.lanes, a.window), (2, 70_000));
        assert_eq!(a.served.len(), 2);
        assert!(a.unserved.is_empty());
        assert!(a.device_bytes <= 30 * GB, "{} GB", a.device_bytes / GB);
    }

    // what this catches: lanes are shed BEFORE the window. Five minds at 70k on the
    // same 30 GB box: only two fit at their requirement; three are UNSERVED (queue /
    // grid), none is served at less.
    #[test]
    fn a_squeeze_sheds_minds_never_shrinks_a_lane_below_its_requirement() {
        let reqs = minds(&[70_000, 70_000, 70_000, 70_000, 70_000]);
        let a = allocate(&reqs, &[qwen27b()], &budget(30)).expect("fits");
        assert_eq!(a.lanes, 2);
        assert_eq!(a.window, 70_000);
        assert_eq!(a.unserved.len(), 3);
    }

    // what this catches: smallest-requirement-first packing serves the MOST minds — a
    // 256k outlier does not size everyone's lane; she is served only if the box has
    // room for her after the others, and the window is then hers.
    #[test]
    fn the_outlier_is_served_last_and_does_not_size_the_others_lanes() {
        let reqs = minds(&[256_000, 70_000, 70_000]);
        let a = allocate(&reqs, &[qwen27b()], &budget(30)).expect("fits");
        assert_eq!(a.lanes, 2, "two at 70k; the 256k mind would need 3 lanes at 256k");
        assert_eq!(a.window, 70_000);
        assert_eq!(a.unserved.len(), 1);
        let roomy = allocate(&reqs, &[qwen27b()], &budget(60)).expect("fits");
        assert_eq!((roomy.lanes, roomy.window), (3, 256_000), "with room, all three at the largest requirement");
    }

    // what this catches: there is NO constant floor (Joel: "you're hard coding
    // constants again"). A mind with no turn yet is Unknown — it resolves to the
    // largest requirement on the seat, and with nothing known on the seat to the
    // model's own trained window, which the budget then bounds at one lane. A
    // measured demand is the requirement, with headroom, whatever its size.
    #[test]
    fn an_unmeasured_mind_resolves_to_the_seat_or_the_model_never_a_constant() {
        let unknown = LaneRequirement::from_demand(Uuid::new_v4(), 0, 1.25);
        assert_eq!((unknown.window, unknown.source), (None, RequirementSource::Unknown));
        let measured = LaneRequirement::from_demand(Uuid::new_v4(), 56_057, 1.25);
        assert_eq!(measured.window, Some(70_071));
        // Beside a measured 70k coder, the unknown takes 70k: two lanes at 70k.
        let a = allocate(&[measured.clone(), unknown.clone()], &[qwen27b()], &budget(30)).expect("fits");
        assert_eq!((a.lanes, a.window), (2, 70_071));
        // Alone, the unknown takes the model's trained window bounded by one lane's fit:
        // the 27B in 30 GB holds ~ (30-19) GB / per-token cost — far above any floor.
        let alone = allocate(&[unknown], &[qwen27b()], &budget(30)).expect("fits");
        assert_eq!(alone.lanes, 1);
        assert!(alone.window > 200_000, "the trained window, not a constant: {}", alone.window);
    }

    // what this catches: the model is shed before the roster is — a box that cannot
    // hold one 27B lane at the requirement serves the 7B at the requirement, and a box
    // that holds nothing says so with the numbers (never a silent 2k lane).
    #[test]
    fn the_model_is_shed_before_a_mind_is_served_at_less() {
        let reqs = minds(&[70_000, 70_000]);
        let tight = allocate(&reqs, &[qwen27b(), small7b()], &budget(16)).expect("the 7b fits");
        assert_eq!(tight.model_id, "small-7b");
        assert_eq!((tight.lanes, tight.window), (2, 70_000));
        let nothing = allocate(&reqs, &[qwen27b(), small7b()], &budget(4)).unwrap_err();
        assert!(matches!(nothing, Unallocatable::NothingFitsOneLane { smallest_requirement: 70_000, .. }), "{nothing:?}");
    }

    // what this catches: more minds served outranks a more capable model that serves
    // fewer — a warm slot for the roster beats capability the roster cannot use.
    #[test]
    fn more_minds_served_outranks_capability() {
        let reqs = minds(&[70_000, 70_000, 70_000]);
        let a = allocate(&reqs, &[qwen27b(), small7b()], &budget(30)).expect("fits");
        assert_eq!(a.model_id, "small-7b", "the 7b serves all three; the 27b only two");
        assert_eq!(a.lanes, 3);
    }
}
