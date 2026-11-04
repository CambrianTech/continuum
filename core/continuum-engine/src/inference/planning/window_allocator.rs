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
//! **Objective**, in order — (1) every served mind holds her requirement; (2) the
//! most CAPABLE model that serves at least one mind that way (Joel, 2026-09-20:
//! "Kimi at 26b MUST be coding" — the coder's seat keeps her model; a mind the
//! model cannot hold beside her queues or places on the grid, it does not pull the
//! seat down to a lesser model); (3) among equally capable, the most minds. **Never**
//! a lane below a requirement: the allocator sheds minds before it shrinks the
//! window, and sheds the model only when it holds nobody.
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

/// A token count a mind's turns measured. Never zero: zero tokens is no measurement, and no
/// measurement is [`Measure::Unmeasured`], never a number.
pub type Tokens = std::num::NonZeroU32;

/// A test's literal token count. A zero literal is a test bug, caught when the test runs; production
/// code has no way to make a token count from nothing.
#[cfg(any(test, feature = "test-fixtures"))]
pub const fn test_tokens(n: u32) -> Tokens {
    match Tokens::new(n) {
        Some(tokens) => tokens,
        None => panic!("a test's token literal is never zero"),
    }
}

/// What a mind's turns have measured of what she requires of a lane. Nothing stands in for a
/// measurement that does not exist: an unmeasured mind is a variant with no number in it, so no
/// reader can size anything from her (Joel, 2026-11-03: "Undefined type values is how you do it.
/// And you require it to be set period").
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Measure {
    /// No view assembled yet. Her wish is recorded when her first view is built, before she asks
    /// for the engine, so she leaves this state without being served.
    Unmeasured,
    /// Her untrimmed assembly is measured (her TARGET, with headroom); no held turn has measured a
    /// floor yet, so the target is the floor.
    Wished { target: Tokens },
    /// A held turn measured her FLOOR: the median post-fit size of turns the fit held, no headroom.
    /// `need <= target` (a floor never exceeds what it floors; the constructor enforces it). The
    /// allocator never serves below the need, and serves the target when the budget holds it (card
    /// 70706a9e: Kimi's wish 99k x 1.25 read as "over the grid" at a 75,776 seat her turns fitted
    /// into every time; a requirement is a target, the need is the gate).
    Held { need: Tokens, target: Tokens },
    /// Declared on the mind (her recipe or genome): the governed size, target and floor alike.
    Declared { target: Tokens },
}

impl Measure {
    /// Her target, when one is measured.
    pub fn target(self) -> Option<Tokens> {
        match self {
            Self::Unmeasured => None,
            Self::Wished { target } | Self::Held { target, .. } | Self::Declared { target } => Some(target),
        }
    }

    /// Her floor, when one is measured: the held need, else the target (a wish or a declaration is
    /// its own floor).
    pub fn need(self) -> Option<Tokens> {
        match self {
            Self::Unmeasured => None,
            Self::Held { need, .. } => Some(need),
            Self::Wished { target } | Self::Declared { target } => Some(target),
        }
    }
}

/// What one mind requires of a lane.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct LaneRequirement {
    pub persona: Uuid,
    /// What her turns have measured: her target and floor, or nothing yet.
    pub measure: Measure,
    /// Her untrimmed assembly without headroom, when measured or declared: what a grant must cover
    /// before she grows (`capacity::window_grant`), so headroom is room to grow into, not a regrant
    /// on every act (Cormac on #5758).
    pub wish: Option<Tokens>,
    /// Her parked training examples' reservation (card ea157b1a): the window that trains them
    /// whole and the training graph it costs. Carried BESIDE her conversational measure, never
    /// folded into it: the allocator grows toward it only from what is left
    /// once the conversational geometry is packed, so learning can never mute her.
    pub training: Option<TrainingReserve>,
}

/// What a resident's waiting training needs of the budget: the window that trains her parked
/// examples whole, and, when a footprint of that very shape is known, its price. Carried beside
/// her conversational requirement; the allocator grows toward it only from what is left after
/// the conversational geometry is packed (Codex on #5556), and an unpriced one grows nothing.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TrainingReserve {
    pub window: u32,
    /// Private: read through [`Self::price_for`], so a price is never used for a model it was
    /// not measured on (Codex on #5557).
    price: Option<TrainingPrice>,
    /// The training job whose partition priced it (`None`: no partition priced this window):
    /// reservations deduplicate by job, and the job already charged as the live training holder
    /// is not reserved twice.
    pub job: Option<Uuid>,
    /// Bytes already excluded from this plan's budget for this exact job.
    #[serde(skip)]
    accounted_bytes: u64,
}

/// What training at a reservation's window costs, and exactly what it was measured FOR: the base
/// model and the training shape's footprint key (rank, targets, depth, walk, chunk, recompute).
/// `source` keeps the training plan's label: a lower bound or a prediction is never a whole run.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrainingPrice {
    pub bytes: u64,
    pub source: crate::inference::planning::training_plan::NeedSource,
    /// The base model id the footprint was measured on (the serving candidate's `model_id`).
    pub model: String,
    /// The footprint key of the priced training shape, for the receipt.
    pub shape: String,
}

impl TrainingReserve {
    pub fn new(window: u32, price: Option<TrainingPrice>, job: Option<Uuid>) -> Self {
        Self { window, price, job, accounted_bytes: 0 }
    }

    /// Set only from leases read atomically with this plan's budget. Never carry
    /// this credit into another budget snapshot or persist it with the demand.
    pub fn account_charged_bytes(&mut self, bytes: u64) {
        self.accounted_bytes = bytes;
    }

    /// The price when serving plans `candidate_model`, the model it was measured on; `None` for
    /// any other model, or when unpriced: then the reservation is unmeasured for that plan.
    pub fn price_for(&self, candidate_model: &str) -> Option<&TrainingPrice> {
        self.price.as_ref().filter(|p| p.model == candidate_model)
    }

    /// The price as recorded, whatever model it names: for a receipt, never for a plan.
    pub fn recorded_price(&self) -> Option<&TrainingPrice> {
        self.price.as_ref()
    }
}

impl LaneRequirement {
    /// A mind with no view assembled yet: no number, nothing to size from.
    pub fn unmeasured(persona: Uuid) -> Self {
        Self { persona, measure: Measure::Unmeasured, wish: None, training: None }
    }

    /// Her measured wish (the untrimmed demand her view assembled) with headroom: her target.
    pub fn wished(persona: Uuid, wish: Tokens, headroom: f64) -> Self {
        // Headroom only adds: the target is the wish plus what headroom asks beyond it.
        let beyond = ((f64::from(wish.get()) * headroom) as u32).saturating_sub(wish.get());
        Self { persona, measure: Measure::Wished { target: wish.saturating_add(beyond) }, wish: Some(wish), training: None }
    }

    pub fn declared(persona: Uuid, window: Tokens) -> Self {
        Self { persona, measure: Measure::Declared { target: window }, wish: Some(window), training: None }
    }

    /// With the floor her held turns measured. A need above the target is clamped to it (a floor
    /// cannot exceed what it floors); on an unmeasured mind a held need is both floor and target,
    /// since she was served at that size.
    pub fn held(mut self, need: Tokens) -> Self {
        self.measure = match self.measure {
            Measure::Unmeasured => Measure::Held { need, target: need },
            Measure::Wished { target } | Measure::Held { target, .. } | Measure::Declared { target } => {
                Measure::Held { need: need.min(target), target }
            }
        };
        // A held turn with no measured wish: what it held is what she wished for.
        self.wish = self.wish.or(Some(need));
        self
    }

    /// Carry her parked training examples' reservation (card ea157b1a). Her conversational
    /// target, source and need stay exactly as her turns measured them (Codex on #5557: a target
    /// overwritten here loses the geometry the allocator must preserve, and with no held turn her
    /// floor would become the training window). A zero window reserves nothing.
    pub fn with_training(mut self, reserve: Option<TrainingReserve>) -> Self {
        self.training = reserve.filter(|r| r.window > 0);
        self
    }

    /// Her target and floor in tokens, when measured. An unmeasured mind has neither and is not
    /// packed: she is measured by her first view, before she needs a lane.
    fn sized(&self) -> Option<(u32, u32)> {
        Some((self.measure.target()?.get(), self.measure.need()?.get()))
    }
}

/// The largest target this seat has measured: the window an undeclared grid role stands at, and
/// the shared window's growth target. `None` = nothing measured anywhere on the seat.
pub fn largest_known(requirements: &[LaneRequirement]) -> Option<u32> {
    requirements.iter().filter_map(|r| r.measure.target()).map(Tokens::get).max()
}

/// The allocator's answer: one engine geometry and who it serves.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Allocation {
    /// Why outstanding training did or did not leave room for window growth.
    pub training_growth: TrainingGrowth,
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
    /// The most lanes the allocator would consider, and what set that cap. `lanes < lane_cap`
    /// means the budget packed fewer; `lanes == lane_cap` means the cap decided.
    pub lane_cap: u32,
    pub lane_cap_by: crate::inference::planning::serving_plan::LaneCapBy,
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
/// `sorted` holds measured minds only, as (target, floor), ascending by target.
fn pack(fp: &ModelFootprint, sorted: &[(u32, u32)], budget: u64, lane_cap: u32) -> Option<(u32, u32)> {
    let mut best: Option<(u32, u32)> = None;
    // ONE window serves every mind packed so far, so it must clear the highest floor
    // among them, not only the current mind's.
    let mut floor_so_far = 0u32;
    for (i, &(target, need)) in sorted.iter().enumerate() {
        let lanes = (i as u32) + 1;
        if lanes > lane_cap {
            break;
        }
        floor_so_far = floor_so_far.max(need);
        // Serve at the target when the model and the budget hold it; otherwise at the
        // most this model holds for these lanes within the budget — but NEVER below
        // a floor. Below her floor a mind starves; between floor and target the fit
        // trims optional history and the turn is whole.
        let window = target.min(fp.context_window).min(fp.window_within(budget, lanes));
        if window < floor_so_far {
            // The model cannot hold this many lanes at every packed mind's need;
            // nobody after her can be served by it either (sorted ascending by target).
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
/// Objective order: every served mind at her requirement; then the most capable
/// model that serves at least one; then the most minds. The seat keeps its model.
///
/// `max_lanes` is the plan's own lane bound: the roster's demand clamped to the measured
/// knee (`decode_knee::knee_lanes`, the tighter of decode and prefill). Packing never
/// exceeds it, however many minds the memory would hold: the knee is a rate limit, and
/// memory knows nothing of rates (IntelMac, 2026-10-10: knee 2, this allocator packed 4,
/// and the seat relaunched past its knee; card b52e8da0).
pub fn allocate(
    requirements: &[LaneRequirement],
    candidates: &[ModelFootprint],
    budget: &HostBudget,
    max_lanes: u32,
) -> Result<Allocation, Unallocatable> {
    if candidates.is_empty() {
        return Err(Unallocatable::NoCandidates);
    }
    // Only measured minds are packed: an unmeasured one has no number to size from, and her first
    // view measures her before she asks for a lane.
    let mut sorted: Vec<(&LaneRequirement, (u32, u32))> = requirements.iter().filter_map(|r| Some((r, r.sized()?))).collect();
    sorted.sort_by_key(|(r, (target, _))| (*target, r.persona));
    let Some(&(_, (smallest_target, _))) = sorted.first() else {
        return Err(Unallocatable::NoRequirements);
    };
    let sizes: Vec<(u32, u32)> = sorted.iter().map(|(_, size)| *size).collect();
    // The lane cap and what set it, chosen together (ties to the earlier term), so a caller can
    // tell a cap from a budget refusal (Codex on #4971): with more minds than `lane_cap`, the
    // unserved were never offered a lane, whatever the budget.
    let (lane_cap, lane_cap_by) = [
        (sorted.len() as u32, crate::inference::planning::serving_plan::LaneCapBy::Requirements),
        (budget.perf_cores.max(1), crate::inference::planning::serving_plan::LaneCapBy::Cores),
        (MAX_LANES, crate::inference::planning::serving_plan::LaneCapBy::MaxLanes),
        (max_lanes, crate::inference::planning::serving_plan::LaneCapBy::Demand),
    ]
    .into_iter()
    .fold((u32::MAX, crate::inference::planning::serving_plan::LaneCapBy::Requirements), |best, term| if term.0 < best.0 { term } else { best });
    let lane_cap = lane_cap.max(1);

    let mut best: Option<(&ModelFootprint, u32, u32)> = None;
    for fp in candidates {
        let Some((lanes, window)) = pack(fp, &sizes, budget.usable_bytes, lane_cap) else { continue };
        let better = match best {
            None => true,
            Some((b, bl, _)) => {
                fp.capability_rank > b.capability_rank
                    || (fp.capability_rank == b.capability_rank && lanes > bl)
            }
        };
        if better {
            best = Some((fp, lanes, window));
        }
    }
    let Some((fp, lanes, window)) = best else {
        return Err(Unallocatable::NothingFitsOneLane {
            budget_bytes: budget.usable_bytes,
            smallest_requirement: smallest_target,
        });
    };
    // Packing decides how many warm slots fit. Queued minds still use those slots
    // in turn: do not strand spare memory at the smallest packed mind's target.
    // Grow only the chosen geometry, never its slot count or admission promises.
    let (window, training_growth) = grow_shared_window(
        requirements, fp, budget.usable_bytes, lanes, window,
    );
    let served: Vec<Uuid> = sorted.iter().take(lanes as usize).map(|(r, _)| r.persona).collect();
    // Unserved: the measured minds packing left out, then the unmeasured, who wait for their first view.
    let unserved: Vec<Uuid> = sorted
        .iter()
        .skip(lanes as usize)
        .map(|(r, _)| r.persona)
        .chain(requirements.iter().filter(|r| r.sized().is_none()).map(|r| r.persona))
        .collect();
    Ok(Allocation {
        model_id: fp.model_id.clone(),
        capability_rank: fp.capability_rank,
        window,
        lanes,
        served,
        unserved,
        device_bytes: device_bytes(fp, window, lanes),
        lane_cap,
        lane_cap_by,
        training_growth,
    })
}

/// The growth decision keeps a failed training reservation visible without revoking
/// conversational admission. Prices are estimates with provenance, not a fit proof.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TrainingGrowth {
    NoDemand,
    Unmeasured { reserved_bytes: u64 },
    Reserved { bytes: u64, target: u32, reached: bool },
    BaselineConflict { bytes: u64 },
}

fn grow_shared_window(
    requirements: &[LaneRequirement], fp: &ModelFootprint, budget: u64,
    lanes: u32, baseline: u32,
) -> (u32, TrainingGrowth) {
    let mut target = largest_known(requirements).map_or(baseline, |w| w.max(baseline));
    let mut prices = std::collections::BTreeMap::<Uuid, u64>::new();
    let mut training_target = 0;
    let mut unmeasured = false;
    for req in requirements {
        let Some(reserve) = &req.training else { continue };
        let (Some(price), Some(job)) = (reserve.price_for(&fp.model_id), reserve.job) else {
            // Unknown training cannot authorize training-driven growth, but must
            // not veto conversation growth or erase other jobs' priced reserves.
            unmeasured = true;
            continue;
        };
        if job.is_nil() {
            unmeasured = true;
            continue;
        }
        let outstanding = price.bytes.saturating_sub(reserve.accounted_bytes);
        prices.entry(job).and_modify(|b| *b = (*b).max(outstanding)).or_insert(outstanding);
        training_target = training_target.max(reserve.window);
    }
    target = target.max(training_target);
    let reserved = prices.values().fold(0u64, |sum, bytes| sum.saturating_add(*bytes));
    let residual = budget.saturating_sub(reserved);
    if device_bytes(fp, baseline, lanes) > residual {
        return (baseline, TrainingGrowth::BaselineConflict { bytes: reserved });
    }
    let window = target.min(fp.context_window).min(fp.window_within(residual, lanes));
    let decision = if unmeasured {
        TrainingGrowth::Unmeasured { reserved_bytes: reserved }
    } else if prices.is_empty() {
        TrainingGrowth::NoDemand
    } else {
        TrainingGrowth::Reserved { bytes: reserved, target: training_target, reached: window >= training_target }
    };
    (window.max(baseline), decision)
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::test_tokens as t;

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
        windows.iter().map(|w| LaneRequirement::declared(Uuid::new_v4(), t(*w))).collect()
    }

    // What this catches: training demand must reserve each job once without
    // revoking conversational admission; an unknown price is not a growth veto.
    #[test]
    fn growth_reserves_training_once_and_preserves_conversation_when_it_cannot_fit() {
        let fp = qwen27b();
        let baseline = 40_000;
        let target = 64_256;
        let bytes = 4 * GB;
        let price = TrainingPrice {
            bytes, source: crate::inference::planning::training_plan::NeedSource::LowerBound,
            model: fp.model_id.clone(), shape: "fixture-family".into(),
        };
        let reserve = TrainingReserve::new(target, Some(price.clone()), Some(Uuid::from_u128(7)));
        let reqs: Vec<_> = minds(&[baseline, baseline]).into_iter()
            .map(|r| r.with_training(Some(reserve.clone()))).collect();
        let roomy = fp.peak_resident_bytes(target, 1) + bytes;
        assert_eq!(grow_shared_window(&reqs, &fp, roomy, 1, baseline),
            (target, TrainingGrowth::Reserved { bytes, target, reached: true }));
        let tight = fp.peak_resident_bytes(baseline, 1);
        assert_eq!(grow_shared_window(&reqs, &fp, tight, 1, baseline),
            (baseline, TrainingGrowth::BaselineConflict { bytes }));
        // What this catches: the same live job's already-charged allocation
        // must not be reserved twice, while an uncovered remainder still is.
        let mut charged = reserve.clone();
        charged.account_charged_bytes(bytes);
        let charged_reqs = [LaneRequirement::declared(Uuid::new_v4(), t(baseline))
            .with_training(Some(charged.clone()))];
        assert_eq!(grow_shared_window(&charged_reqs, &fp, roomy - bytes, 1, baseline),
            (target, TrainingGrowth::Reserved { bytes: 0, target, reached: true }));
        charged.account_charged_bytes(bytes / 2);
        let partial_reqs = [LaneRequirement::declared(Uuid::new_v4(), t(baseline))
            .with_training(Some(charged))];
        assert_eq!(grow_shared_window(&partial_reqs, &fp, roomy - bytes / 2, 1, baseline),
            (target, TrainingGrowth::Reserved { bytes: bytes / 2, target, reached: true }));
        let partial = fp.peak_resident_bytes(50_000, 1) + bytes;
        assert_eq!(grow_shared_window(&reqs, &fp, partial, 1, baseline),
            (50_000, TrainingGrowth::Reserved { bytes, target, reached: false }));
        for unknown in [
            TrainingReserve::new(target, None, None),
            TrainingReserve::new(target, Some(TrainingPrice { model: "other-base".into(), ..price }), Some(Uuid::from_u128(7))),
        ] {
            let reqs = [LaneRequirement::declared(Uuid::new_v4(), t(55_000)).with_training(Some(unknown.clone()))];
            assert_eq!(grow_shared_window(&reqs, &fp, roomy, 1, baseline),
                (55_000, TrainingGrowth::Unmeasured { reserved_bytes: 0 }));
            let mixed = [reqs[0].clone(), LaneRequirement::declared(Uuid::new_v4(), t(baseline))
                .with_training(Some(reserve.clone()))];
            assert_eq!(grow_shared_window(&mixed, &fp, partial, 1, baseline),
                (50_000, TrainingGrowth::Unmeasured { reserved_bytes: bytes }));
        }
    }

    // The live 5090 could hold Kimi's training target in one slot, but packing
    // stopped at the smaller resident's target when a second slot did not fit.
    #[test]
    fn queued_target_uses_spare_capacity_without_promising_another_slot() {
        let fp = ModelFootprint {
            weights_bytes: 18_973_870_432,
            kv_per_token: 32_768,
            fixed_per_lane_bytes: 886_154_752,
            ..qwen27b()
        };
        let host = HostBudget { usable_bytes: 24_804_791_852, perf_cores: 8 };
        let reqs = minds(&[36_491, 64_256]);
        let a = allocate(&reqs, &[fp], &host, 2).expect("one slot fits");
        assert_eq!((a.lanes, a.window), (1, 64_256));
        assert_eq!(a.served, vec![reqs[0].persona]);
        assert_eq!(a.unserved, vec![reqs[1].persona]);
        assert_eq!(a.device_bytes, 23_677_817_846);
        assert!(a.device_bytes <= host.usable_bytes);
    }

    #[test]
    fn queued_target_cannot_take_memory_beyond_budget_or_mute_conversation() {
        let fp = qwen27b();
        let host = HostBudget {
            usable_bytes: fp.peak_resident_bytes(45_000, 1),
            perf_cores: 8,
        };
        let reqs = minds(&[36_491, 64_256]);
        let a = allocate(&reqs, &[fp], &host, 2).expect("conversation still fits");
        assert_eq!((a.lanes, a.window), (1, 45_000));
        assert_eq!(a.served, vec![reqs[0].persona]);
        assert_eq!(a.unserved, vec![reqs[1].persona]);
        assert!(a.device_bytes <= host.usable_bytes);
    }

    #[test]
    fn shared_target_respects_model_ceiling_and_measured_lane_cap() {
        let fp = ModelFootprint { context_window: 50_000, ..qwen27b() };
        let reqs = minds(&[36_491, 64_256]);
        let a = allocate(&reqs, &[fp], &budget(200), 1).expect("one slot fits");
        assert_eq!((a.lanes, a.window), (1, 50_000));
        assert_eq!(a.unserved, vec![reqs[1].persona]);
    }

    // what this catches (Codex on #4971): a cap read as a budget refusal. With a roomy budget,
    // one performance core and two minds, the second mind is unserved because the CORE cap
    // offered one lane, not because the budget refused it; the allocation says which.
    #[test]
    fn an_allocation_names_the_cap_that_limited_its_lanes() {
        let roomy_one_core = HostBudget { usable_bytes: 200 * GB, perf_cores: 1 };
        let a = allocate(&minds(&[40_000, 40_000]), &[qwen27b()], &roomy_one_core, 8).expect("allocation");
        assert_eq!((a.lanes, a.lane_cap, a.lane_cap_by), (1, 1, crate::inference::planning::serving_plan::LaneCapBy::Cores));
        assert_eq!(a.unserved.len(), 1, "unserved by the cap, whatever the budget");
        // On a tight budget the cap stays at two minds, and the BUDGET packs fewer.
        let a = allocate(&minds(&[40_000, 40_000]), &[qwen27b()], &budget(23), 8).expect("allocation");
        assert_eq!((a.lane_cap, a.lane_cap_by), (2, crate::inference::planning::serving_plan::LaneCapBy::Requirements));
        assert!(a.lanes < a.lane_cap, "the budget packed {} of {}", a.lanes, a.lane_cap);
    }

    // what this catches: the 5090 as it stood (2026-09-20) — two coders at ~56k with
    // headroom, a 30 GB serving budget under a 19 GB model. The allocator serves BOTH
    // at their requirement (2 × 70k, ~26 GB), never 2 × 2048; and the window is the
    // largest served requirement, one per engine.
    #[test]
    fn the_5090_serves_two_coders_at_their_requirement_never_a_2k_lane() {
        let reqs = minds(&[70_000, 70_000]);
        let a = allocate(&reqs, &[qwen27b()], &budget(30), u32::MAX).expect("fits");
        assert_eq!((a.lanes, a.window), (2, 70_000));
        assert_eq!(a.served.len(), 2);
        assert!(a.unserved.is_empty());
        assert!(a.device_bytes <= 30 * GB, "{} GB", a.device_bytes / GB);
    }

    // what this catches (card b52e8da0): memory packing past the measured knee. The
    // IntelMac, 2026-10-10: four minds on a 1.5B fit easily, the knee was 2, the
    // allocator packed 4, and the seat relaunched past its knee. The plan's bound wins
    // over what memory would hold; with no bound (u32::MAX) memory alone decides.
    #[test]
    fn the_knee_bounds_the_lanes_however_many_minds_memory_would_hold() {
        let reqs = minds(&[32_768, 32_768, 32_768, 32_768]);
        let a = allocate(&reqs, &[small7b()], &budget(60), 2).expect("fits");
        assert_eq!(a.lanes, 2, "the knee, not memory");
        assert_eq!(a.served.len(), 2);
        let free = allocate(&reqs, &[small7b()], &budget(60), u32::MAX).expect("fits");
        assert_eq!(free.lanes, 4, "unbounded, memory holds all four");
    }

    // what this catches: lanes are shed BEFORE the window. Five minds at 70k on the
    // same 30 GB box: only two fit at their requirement; three are UNSERVED (queue /
    // grid), none is served at less.
    #[test]
    fn a_squeeze_sheds_minds_never_shrinks_a_lane_below_its_requirement() {
        let reqs = minds(&[70_000, 70_000, 70_000, 70_000, 70_000]);
        let a = allocate(&reqs, &[qwen27b()], &budget(30), u32::MAX).expect("fits");
        assert_eq!(a.lanes, 2);
        assert_eq!(a.window, 70_000);
        assert_eq!(a.unserved.len(), 3);
    }

    // Packing still serves the most minds. A queued outlier may use spare window
    // capacity, but cannot evict a packed mind or create an unaffordable slot.
    #[test]
    fn the_outlier_uses_only_spare_capacity_after_packing() {
        let reqs = minds(&[256_000, 70_000, 70_000]);
        let a = allocate(&reqs, &[qwen27b()], &budget(30), u32::MAX).expect("fits");
        assert_eq!(a.lanes, 2, "two at 70k; the 256k mind would need 3 lanes at 256k");
        assert_eq!(a.window, qwen27b().window_within(budget(30).usable_bytes, 2));
        assert!(a.window >= 70_000 && a.window < 256_000);
        assert!(a.device_bytes <= budget(30).usable_bytes);
        assert_eq!(a.unserved.len(), 1);
        let roomy = allocate(&reqs, &[qwen27b()], &budget(60), u32::MAX).expect("fits");
        assert_eq!((roomy.lanes, roomy.window), (3, 256_000), "with room, all three at the largest requirement");
    }

    // what this catches: a number standing in for a mind not measured yet (Joel, 2026-11-03: "use
    // nothing not constants. Undefined type values is how you do it"). She was resolved to the seat's
    // largest requirement or the model's whole window; now she has no number and is not packed: the
    // measured minds are sized, she is unserved until her first view measures her, and a seat with
    // nobody measured has no requirement at all (never a fit or a borrowed size).
    #[test]
    fn an_unmeasured_mind_is_never_sized_from_anything_but_her_own_measure() {
        let unknown = LaneRequirement::unmeasured(Uuid::new_v4());
        assert_eq!((unknown.measure.target(), unknown.measure.need()), (None, None));
        let measured = LaneRequirement::wished(Uuid::new_v4(), t(56_057), 1.25);
        assert_eq!(measured.measure, Measure::Wished { target: t(70_071) });
        let a = allocate(&[measured.clone(), unknown.clone()], &[qwen27b()], &budget(30), u32::MAX).expect("fits");
        assert_eq!((a.lanes, a.window, a.served.clone()), (1, a.window, vec![measured.persona]), "only the measured mind is packed");
        assert_eq!(a.unserved, vec![unknown.persona], "the unmeasured mind waits for her first view, unsized");
        assert!(matches!(allocate(&[unknown], &[qwen27b()], &budget(30), u32::MAX), Err(Unallocatable::NoRequirements)), "nobody measured: nothing to allocate for");
    }

    // what this catches: the model is shed before the roster is — a box that cannot
    // hold one 27B lane at the requirement serves the 7B at the requirement, and a box
    // that holds nothing says so with the numbers (never a silent 2k lane).
    #[test]
    fn the_model_is_shed_before_a_mind_is_served_at_less() {
        let reqs = minds(&[70_000, 70_000]);
        let tight = allocate(&reqs, &[qwen27b(), small7b()], &budget(16), u32::MAX).expect("the 7b fits");
        assert_eq!(tight.model_id, "small-7b");
        assert_eq!((tight.lanes, tight.window), (2, 70_000));
        let nothing = allocate(&reqs, &[qwen27b(), small7b()], &budget(4), u32::MAX).unwrap_err();
        assert!(matches!(nothing, Unallocatable::NothingFitsOneLane { smallest_requirement: 70_000, .. }), "{nothing:?}");
    }

    // what this catches (Joel: "Kimi at 26b MUST be coding"): the seat keeps its most
    // capable model as long as it holds one mind at her requirement — a third coder
    // the 27B cannot hold beside the two queues or grid-places; she does not pull
    // the seat down to the 7B. The 7B is chosen only when the 27B holds nobody.
    #[test]
    fn capability_outranks_head_count_while_one_mind_is_held() {
        let reqs = minds(&[70_000, 70_000, 70_000]);
        let a = allocate(&reqs, &[qwen27b(), small7b()], &budget(30), u32::MAX).expect("fits");
        assert_eq!(a.model_id, "qwen-27b");
        assert_eq!((a.lanes, a.unserved.len()), (2, 1));
        // The 5090 as it stands: 22 GB handed to serving, q8 KV (32,768 B/token).
        // Kimi's typical 40k×1.25 = 50k fits ONE 27B lane (3.2 GB after weights: 1.6 GB KV
        // + 1.2 GB compute floor + the prefill rate); Sahar's 97k does not fit beside her —
        // Kimi keeps the 27B at her requirement, Sahar is unserved here (queue / grid).
        let mut q8 = qwen27b();
        q8.kv_per_token = 32_768;
        let one_lane = q8.window_within(budget(22).usable_bytes, 1);
        assert!(one_lane > 40_000 && one_lane < 90_000, "precondition: one 27B lane on this card is tens of k, not hundreds: {one_lane}");
        let kimi = one_lane - 1_000;
        let seat = vec![LaneRequirement::declared(Uuid::new_v4(), t(kimi)), LaneRequirement::declared(Uuid::new_v4(), t(97_268))];
        let a = allocate(&seat, &[q8, small7b()], &budget(22), u32::MAX).expect("fits");
        assert_eq!((a.model_id.as_str(), a.lanes, a.window), ("qwen-27b", 1, one_lane));
        assert_eq!(a.unserved.len(), 1);
    }
    // what this catches (card 70706a9e): a mind whose TARGET the budget cannot hold but
    // whose NEED it can is served between the two — at the most the model holds for
    // the lanes within the budget — never shed, never below the need. Without a need
    // the target is the floor and she is shed, exactly as before. And ONE window
    // serves every packed mind, so it must clear the highest floor among them.
    #[test]
    fn a_mind_is_served_between_her_need_and_her_target_never_below_the_need() {
        let fp = small7b();
        // A budget that holds ONE lane at ~80k by the model's own arithmetic — under
        // the 123,870 target, over the 74,000 need.
        let budget = HostBudget { usable_bytes: device_bytes(&fp, 80_000, 1), perf_cores: 8 };
        assert!(fp.window_within(budget.usable_bytes, 1) < 123_870 && fp.window_within(budget.usable_bytes, 1) >= 74_000, "fixture premise");
        let kimi = Uuid::new_v4();
        let wish_only = [LaneRequirement::wished(kimi, t(99_096), 1.25)];
        let sorted: Vec<(u32, u32)> = wish_only.iter().filter_map(LaneRequirement::sized).collect();
        assert_eq!(pack(&fp, &sorted, budget.usable_bytes, 4), None, "no need: the target is the floor, the budget cannot hold 123,870, she is shed (pre-70706a9e behaviour)");

        let with_need = [LaneRequirement::wished(kimi, t(99_096), 1.25).held(t(74_000))];
        let sorted: Vec<(u32, u32)> = with_need.iter().filter_map(LaneRequirement::sized).collect();
        let (lanes, window) = pack(&fp, &sorted, budget.usable_bytes, 4).expect("served");
        assert_eq!(lanes, 1);
        assert!((74_000..123_870).contains(&window), "served between need and target at what the budget holds: {window}");
        assert_eq!(window, fp.window_within(budget.usable_bytes, 1), "the window is the most the model holds here, not a constant");

        // A second mind whose floor is her target: the one shared window must clear it.
        let aiko = Uuid::new_v4();
        let two = [
            LaneRequirement::wished(aiko, t(24_226), 1.0),
            LaneRequirement::wished(kimi, t(99_096), 1.25).held(t(74_000)),
        ];
        let mut sorted: Vec<(u32, u32)> = two.iter().filter_map(LaneRequirement::sized).collect();
        sorted.sort_unstable();
        let (lanes, window) = pack(&fp, &sorted, budget.usable_bytes, 4).expect("at least aiko");
        assert!(window >= 24_226, "aiko's floor is her target and the shared window clears it: {window}");
        if lanes == 2 {
            assert!(window >= 74_000, "if kimi is packed too the shared window clears her need: {window}");
        }
    }

    // what this catches (card ea157b1a, gate 3; Codex on #5557): a training reservation folded
    // into her conversational target, so the allocator can no longer see the geometry it must
    // preserve, and with no held turn her floor becomes the training window: a mind that fit
    // for conversation muted by learning. The reservation travels beside, labeled.
    #[test]
    fn a_training_reserve_travels_beside_her_conversation_never_into_it() {
        let job = Uuid::from_u128(0x1e02);
        let price = TrainingPrice { bytes: 6_000_000_000, source: crate::inference::planning::training_plan::NeedSource::LowerBound, model: "kimi-base".into(), shape: "kimi-base|w64256|r8|attn_q,attn_v|exact|rc".into() };
        let measured = TrainingReserve::new(64_256, Some(price.clone()), Some(job));
        for her in [LaneRequirement::wished(Uuid::new_v4(), t(40_000), 1.25).held(t(30_000)), LaneRequirement::wished(Uuid::new_v4(), t(40_000), 1.25)] {
            let learning = her.clone().with_training(Some(measured.clone()));
            assert_eq!(learning.measure, her.measure, "conversation untouched: her target and floor are her conversation's, never the training window");
            assert_eq!(learning.training.as_ref(), Some(&measured), "the reservation travels to the allocator, its source labeled");
        }
        assert_eq!(measured.price_for("kimi-base"), Some(&price), "priced for the model it was measured on");
        assert_eq!(measured.price_for("another-base"), None, "the same window on another model is unmeasured, never this price");
        assert_eq!(TrainingReserve::new(64_256, None, None).price_for("kimi-base"), None);
        let her = LaneRequirement::wished(Uuid::new_v4(), t(40_000), 1.25);
        assert_eq!(her.clone().with_training(Some(TrainingReserve::new(0, Some(price), Some(job)))), her, "a zero window reserves nothing");
        assert_eq!(her.clone().with_training(None), her, "nothing parked, nothing changes");
    }
}
