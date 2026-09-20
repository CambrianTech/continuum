//! Expert reconcile — the slice-1 executor for the K3 pager.
//!
//! Turns an [`ExpertResidencyPlan`] (which experts should be hot, from the observed
//! activation profile via [`plan_expert_residency_budgeted`](super::expert_pager::plan_expert_residency_budgeted))
//! into page-in / evict operations on a persona's resident expert set, and decides when
//! the served context must RELAUNCH to adopt a materially-changed set.
//!
//! ## The stable seam vs the swappable impl (M5's slice-1/slice-2 split)
//!
//! [`ExpertPager::page_in`] / [`ExpertPager::evict`] have the SAME signature across
//! slices — that is the whole point. llama.cpp's public API exposes no runtime tensor
//! accessor on a loaded model, so a LIVE per-expert RAM→VRAM upload needs a
//! vendored-llama fork (`get_tensor`) — that is slice 2 (ceiling). Slice 1's
//! [`RelaunchPager`] impl instead accumulates page-ins into the next-relaunch RESIDENCY
//! SET (experts placed at LOAD time via buft-override), and a materially-changed set
//! triggers a RELAUNCH of the served context (the `serving_daemon` relaunch pattern).
//! When the fork lands, the SAME `page_in` body swaps to a live `load_expert(layer,
//! expert, bytes)` call — the trait, the reconciliation, and the D→RAM staging are
//! unchanged. Nothing built here is wasted by the slice-2 upgrade.

use std::collections::HashSet;

use super::expert_pager::plan_expert_residency_budgeted;
use super::expert_residency::{ExpertActivationProfile, ExpertId, ExpertResidencyPlan};
use super::SystemProfile;
use crate::genome::expert_ingest::expert_set_artifact_id;
use crate::genome::working_set::{PageKind, PageOffset, PageRef};

/// Map an [`ExpertId`] to its genome [`PageRef`]: the per-layer expert-set artifact
/// (keyed by `(gguf_id, layer)`), with the expert selected by index. THE one place the
/// `(layer, expert)` → page addressing lives, so observe, reconcile, and the genome
/// pager all agree on how an expert is named.
pub fn expert_page_ref(gguf_id: &str, expert: ExpertId) -> PageRef {
    PageRef {
        kind: PageKind::MoEExpert,
        artifact: expert_set_artifact_id(gguf_id, expert.layer),
        offset: PageOffset::Expert {
            expert_index: expert.expert,
        },
    }
}

/// A single reconcile operation, emitted by diffing the plan's hot set against what's
/// currently resident.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReconcileOp {
    PageIn(PageRef),
    Evict(PageRef),
}

/// Diff the residency plan's HOT experts against the currently-resident page set and
/// emit the ops to make `resident == hot`. Pure — the caller applies the ops to an
/// [`ExpertPager`]. `current` is the set of expert PageRefs currently resident (from the
/// persona's working set). Page-ins first (bring the newly-hot in) then evicts (drop the
/// no-longer-hot), so a caller that applies them in order never dips below the union.
pub fn reconcile_ops(
    plan: &ExpertResidencyPlan,
    gguf_id: &str,
    current: &HashSet<PageRef>,
) -> Vec<ReconcileOp> {
    // An empty hot set means "no residency plan yet" — cold-start, or unsized experts
    // (`plan_expert_residency` returns `hot: []` when `expert_bytes` is unknown and it
    // warms everything). It does NOT mean "evict every resident expert". Reconciling to
    // an empty target would evict the whole set and relaunch the MoE with ZERO experts
    // placed — degenerate, it can't serve. Hold the current residency until a real plan
    // arrives ([[fallbacks-are-illegal-fail-loud]]: don't silently blank the model).
    if plan.hot.is_empty() {
        return Vec::new();
    }

    let target: HashSet<PageRef> = plan
        .hot
        .iter()
        .map(|&e| expert_page_ref(gguf_id, e))
        .collect();

    let mut ops = Vec::new();
    for p in target.difference(current) {
        ops.push(ReconcileOp::PageIn(p.clone()));
    }
    for p in current.difference(&target) {
        ops.push(ReconcileOp::Evict(p.clone()));
    }
    ops
}

/// The STABLE mutation seam. `page_in`/`evict` signatures are identical across slices;
/// only the impl body differs (slice 1 = relaunch-set accumulation, slice 2 = live
/// `load_expert` upload once the vendored-llama fork exposes a runtime tensor accessor).
pub trait ExpertPager {
    /// Make `page` resident. Slice 1: add it to the next-relaunch residency set. Slice 2:
    /// stage its bytes (D→RAM) + `load_expert` (RAM→VRAM).
    fn page_in(&mut self, page: PageRef) -> Result<(), PagerError>;
    /// Drop `page` from residency. Slice 1: remove from the residency set. Slice 2: free
    /// its VRAM tensor region.
    fn evict(&mut self, page: PageRef) -> Result<(), PagerError>;
}

/// A pager operation failed. Fail-loud: a pager that can't place a page it was told to
/// is a real precondition gap the caller must surface, never a silent skip.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagerError(pub String);

impl std::fmt::Display for PagerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl std::error::Error for PagerError {}

/// Apply a sequence of reconcile ops to a pager (page-in the newly hot, evict the gone).
/// Fail-loud: a `page_in`/`evict` error aborts and propagates (a pager that can't place
/// is a real gap). Returns the count applied on success.
pub fn apply_reconcile(
    pager: &mut impl ExpertPager,
    ops: Vec<ReconcileOp>,
) -> Result<usize, PagerError> {
    let n = ops.len();
    for op in ops {
        match op {
            ReconcileOp::PageIn(p) => pager.page_in(p)?,
            ReconcileOp::Evict(p) => pager.evict(p)?,
        }
    }
    Ok(n)
}

/// Slice-1 [`ExpertPager`]: accumulates page-ins/evicts into a RESIDENCY SET and tracks
/// which set the currently-served context was launched with. There is no live upload
/// (llama.cpp has no runtime tensor accessor); instead a materially-changed residency
/// set triggers a RELAUNCH of the served context with the new set placed at load time.
/// When the vendored-llama fork lands, [`ExpertPager::page_in`] swaps to a live
/// `load_expert` call (slice 2) and expert churn no longer forces a relaunch.
#[derive(Debug, Clone, Default)]
pub struct RelaunchPager {
    /// The residency set the NEXT (re)launch will place. Mutated by `page_in`/`evict`.
    resident: HashSet<PageRef>,
    /// The set the CURRENTLY-served context was launched with. `relaunch_needed` compares
    /// `resident` against this.
    served: HashSet<PageRef>,
}

impl RelaunchPager {
    pub fn new() -> Self {
        Self::default()
    }

    /// The set the next relaunch will place.
    pub fn resident(&self) -> &HashSet<PageRef> {
        &self.resident
    }

    /// Does the accumulated residency set differ MATERIALLY from what's currently served
    /// — enough to justify a relaunch? Relaunching a llama-server is expensive (kill +
    /// reload every weight), so a few experts drifting in/out per turn is noise; only a
    /// churn exceeding `threshold` earns the relaunch. `threshold = 0` relaunches on ANY
    /// change — including the first launch, when `served` is empty and any non-empty set
    /// must be placed.
    pub fn relaunch_needed(&self, threshold: usize) -> bool {
        self.resident.symmetric_difference(&self.served).count() > threshold
    }

    /// Record that the served context was (re)launched with the current residency set.
    /// After this, `relaunch_needed` is false until the set churns again.
    pub fn mark_relaunched(&mut self) {
        self.served = self.resident.clone();
    }
}

impl ExpertPager for RelaunchPager {
    fn page_in(&mut self, page: PageRef) -> Result<(), PagerError> {
        // Slice 1: accumulate into the next-relaunch residency set (no live upload —
        // the fork that would let us upload into a loaded model is slice 2).
        self.resident.insert(page);
        Ok(())
    }

    fn evict(&mut self, page: PageRef) -> Result<(), PagerError> {
        self.resident.remove(&page);
        Ok(())
    }
}

/// The outcome of one expert-pager cycle: the ops applied this pass and whether the
/// served context now needs a relaunch to adopt the changed residency set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagerStepOutcome {
    /// The page-in/evict ops the reconcile emitted (already applied to the pager).
    pub ops: Vec<ReconcileOp>,
    /// True when the accumulated residency set changed materially vs the served set —
    /// the serving tick should relaunch the context with the new expert placement.
    pub relaunch_needed: bool,
    /// How many experts the plan wants HOT this pass (for telemetry / the probe).
    pub hot_experts: usize,
}

/// ONE expert-pager execution cycle — the pure heart of the pager driver.
///
/// Composes the merged primitives into the single call the serving tick makes each pass:
/// the observed [`ExpertActivationProfile`] (M5's observe/predictor writes it) →
/// [`plan_expert_residency_budgeted`] (hot/warm/cold within the 0.80 serving VRAM budget)
/// → [`reconcile_ops`] (diff the hot set vs what the pager holds) → [`apply_reconcile`]
/// (page-in new / evict gone on the [`RelaunchPager`]) → [`RelaunchPager::relaunch_needed`].
///
/// Pure + deterministic: no I/O, no clock, no serving-daemon coupling — the serving tick
/// owns the side effects (calling this, then relaunching when `relaunch_needed`). That
/// keeps the pager's DECISION logic testable in isolation from the serving control loop it
/// runs inside (the same split as [`plan_serving`](crate::cognition::serving_plan) vs the
/// serving_daemon reconcile). Empty activation / unsized experts flow through the planner's
/// safe cold-start (all-warm) + the reconcile empty-hot guard — never evict-to-zero.
#[allow(clippy::too_many_arguments)]
pub fn expert_pager_step(
    profile: &SystemProfile,
    activation: &ExpertActivationProfile,
    gguf_id: &str,
    expert_bytes: u64,
    margin_bytes: u64,
    pager: &mut RelaunchPager,
    relaunch_threshold: usize,
) -> Result<PagerStepOutcome, PagerError> {
    let plan = plan_expert_residency_budgeted(profile, activation, expert_bytes, margin_bytes);
    let hot_experts = plan.hot.len();
    // Diff the plan's hot set against what the pager currently holds resident, then apply.
    let current = pager.resident().clone();
    let ops = reconcile_ops(&plan, gguf_id, &current);
    apply_reconcile(pager, ops.clone())?;
    Ok(PagerStepOutcome {
        ops,
        relaunch_needed: pager.relaunch_needed(relaunch_threshold),
        hot_experts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capacity::expert_residency::ExpertResidencyPlan;

    const GGUF: &str = "unsloth/Qwen3-Coder-30B-A3B-Instruct-GGUF";

    fn eid(layer: u32, expert: u32) -> ExpertId {
        ExpertId { layer, expert }
    }

    fn plan(hot: &[ExpertId]) -> ExpertResidencyPlan {
        ExpertResidencyPlan {
            hot: hot.to_vec(),
            warm: Vec::new(),
            cold: Vec::new(),
        }
    }

    fn refs(gguf: &str, experts: &[ExpertId]) -> HashSet<PageRef> {
        experts.iter().map(|&e| expert_page_ref(gguf, e)).collect()
    }

    // what this catches: an expert's PageRef is deterministic and DISTINCT per
    // (layer, expert) — observe, reconcile, and the genome pager must all name the same
    // expert the same way, or the pager pages in the wrong weights.
    #[test]
    fn expert_page_ref_is_distinct_and_deterministic() {
        assert_eq!(
            expert_page_ref(GGUF, eid(0, 3)),
            expert_page_ref(GGUF, eid(0, 3))
        );
        assert_ne!(
            expert_page_ref(GGUF, eid(0, 3)),
            expert_page_ref(GGUF, eid(0, 4))
        );
        assert_ne!(
            expert_page_ref(GGUF, eid(0, 3)),
            expert_page_ref(GGUF, eid(1, 3))
        );
        // MoEExpert kind + the right expert index.
        let p = expert_page_ref(GGUF, eid(2, 7));
        assert_eq!(p.kind, PageKind::MoEExpert);
        assert_eq!(p.offset, PageOffset::Expert { expert_index: 7 });
    }

    // what this catches: reconciliation makes resident == hot — page-in the newly hot,
    // evict the no-longer-hot, touch nothing already correct. A drift here = the pager
    // holding stale experts or missing hot ones.
    #[test]
    fn reconcile_pages_in_new_and_evicts_gone() {
        // Currently resident: experts 1,2. New plan hot: 2,3. → page-in 3, evict 1, keep 2.
        let current = refs(GGUF, &[eid(0, 1), eid(0, 2)]);
        let p = plan(&[eid(0, 2), eid(0, 3)]);
        let ops = reconcile_ops(&p, GGUF, &current);

        assert!(ops.contains(&ReconcileOp::PageIn(expert_page_ref(GGUF, eid(0, 3)))));
        assert!(ops.contains(&ReconcileOp::Evict(expert_page_ref(GGUF, eid(0, 1)))));
        // expert 2 stays — no op for it.
        assert!(!ops.iter().any(
            |op| matches!(op, ReconcileOp::PageIn(p) | ReconcileOp::Evict(p)
                if *p == expert_page_ref(GGUF, eid(0, 2)))
        ));
        assert_eq!(ops.len(), 2);
    }

    // what this catches: THE EMPTY-HOT DEGENERATE. An empty hot set (cold-start, or
    // unsized experts where plan_expert_residency returns hot=[]) must NOT evict the
    // resident set — that would relaunch the MoE with ZERO experts placed, which can't
    // serve. Reconcile holds current residency until a real plan arrives. Regression
    // here = a sizing hiccup silently blanking a live model.
    #[test]
    fn empty_hot_plan_holds_residency_never_evicts_to_zero() {
        let current = refs(GGUF, &[eid(0, 1), eid(0, 2)]);
        let ops = reconcile_ops(&plan(&[]), GGUF, &current);
        assert!(
            ops.is_empty(),
            "empty hot = no plan yet: hold residency, never evict to zero (got {ops:?})"
        );
    }

    // what this catches: THE SLICE-1 RELAUNCH SEMANTICS. page_in accumulates into the
    // residency set; the first non-empty set needs a relaunch (nothing served yet); a
    // small drift under threshold does NOT relaunch (expensive); a wholesale change DOES;
    // and mark_relaunched clears the need. This is the observe→plan→relaunch loop's brain.
    #[test]
    fn relaunch_pager_accumulates_and_relaunches_only_on_material_change() {
        let mut pager = RelaunchPager::new();
        // First launch: apply a hot set → served is empty → any set needs a relaunch.
        let ops = reconcile_ops(
            &plan(&[eid(0, 1), eid(0, 2), eid(0, 3)]),
            GGUF,
            &HashSet::new(),
        );
        apply_reconcile(&mut pager, ops).unwrap();
        assert_eq!(pager.resident().len(), 3);
        assert!(
            pager.relaunch_needed(0),
            "first non-empty residency set must relaunch"
        );

        pager.mark_relaunched();
        assert!(
            !pager.relaunch_needed(0),
            "after relaunch, served == resident → no relaunch"
        );

        // Small drift: swap ONE expert (churn = 2: one out, one in). Under threshold 2 → no
        // relaunch (churn must EXCEED threshold); a churn of 3+ would.
        let current = pager.resident().clone();
        let ops = reconcile_ops(&plan(&[eid(0, 1), eid(0, 2), eid(0, 4)]), GGUF, &current);
        apply_reconcile(&mut pager, ops).unwrap();
        assert!(
            !pager.relaunch_needed(2),
            "a 1-expert swap (churn 2) is noise under threshold 2"
        );
        assert!(
            pager.relaunch_needed(1),
            "the same swap DOES exceed threshold 1"
        );
    }

    // what this catches: expert_pager_step composes the WHOLE cycle into one call —
    // plan (budgeted) → reconcile → apply → relaunch decision. First pass on a fresh
    // pager pages in the hot experts and needs a relaunch (served set empty); after
    // mark_relaunched, the SAME activation yields no new ops and no relaunch (idempotent,
    // settled). This is the pure heart the serving tick calls each pass — a regression =
    // the pager thrashing (relaunching every tick) or never converging.
    #[test]
    fn expert_pager_step_runs_the_full_cycle_and_settles() {
        use crate::capacity::expert_residency::ExpertActivationProfile;
        use crate::capacity::{DeviceCapacity, SystemProfile};
        use crate::governor::types::{HardwareClass, PowerSource, TargetSilicon, ThermalClass};
        use std::collections::HashMap;
        const GB: u64 = 1024 * 1024 * 1024;

        let profile = SystemProfile::from_parts(
            HardwareClass {
                silicon: TargetSilicon::NvidiaCuda,
                silicon_model: "test".into(),
                vram_mb: 32 * 1024,
                system_ram_mb: 128 * 1024,
                power_source: PowerSource::Plugged,
                thermal_class: ThermalClass::Workstation,
                battery_pct: None,
                thermal_headroom_pct: None,
            },
            DeviceCapacity {
                gpu_total_bytes: 32 * GB,
                gpu_free_bytes_live: 30 * GB, // → 24 GiB budgeted (0.80)
                system_ram_free_bytes: 100 * GB,
            },
            vec![],
            24,
        );
        // 8 experts firing on layer 0, descending hit counts → a clear hot ranking.
        let mut hits = HashMap::new();
        for e in 0..8u32 {
            hits.insert(eid(0, e), (8 - e) as u64 * 100);
        }
        let activation = ExpertActivationProfile {
            gate_magnitude: HashMap::new(),
            hits,
            predicted: HashMap::new(),
        };

        let mut pager = RelaunchPager::new();
        // First pass: pages the hot experts in (≤6 fit 24 GiB at 4 GiB each), needs relaunch.
        let out = expert_pager_step(&profile, &activation, GGUF, 4 * GB, 0, &mut pager, 0).unwrap();
        assert!(
            (1..=6).contains(&out.hot_experts),
            "hot set within the budgeted VRAM, got {}",
            out.hot_experts
        );
        assert!(!out.ops.is_empty(), "first pass pages experts in");
        assert!(
            out.relaunch_needed,
            "first non-empty residency set needs a relaunch"
        );

        pager.mark_relaunched();
        // Second pass, SAME activation: nothing changed → no ops, no relaunch (settled).
        let out2 =
            expert_pager_step(&profile, &activation, GGUF, 4 * GB, 0, &mut pager, 0).unwrap();
        assert!(out2.ops.is_empty(), "stable activation → no churn");
        assert!(!out2.relaunch_needed, "settled → no relaunch");
    }
}
