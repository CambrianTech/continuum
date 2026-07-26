//! Serving-side expert pager — the stateful DRIVER the llama backend holds and ticks.
//!
//! The pure brain lives elsewhere: [`plan_expert_residency_budgeted`] ranks experts,
//! [`expert_pager_step`] runs the plan→reconcile→relaunch-decision cycle, and
//! [`LiveExpertObserver`] tallies which experts actually fire on the compute thread. This
//! is the small amount of STATE that lives between ticks and glues them to the serving
//! control loop:
//!
//! - it OWNS the [`LiveExpertObserver`] `Arc` — [`observer`](ServingExpertPager::observer)
//!   hands the SAME `Arc` to `ContextParams.expert_observer` so the tally the tick reads is
//!   the one the backend's compute thread writes.
//! - it OWNS the [`RelaunchPager`] residency set across ticks (the observer only tallies;
//!   the pager remembers what's placed).
//! - it applies the DECAY the observer explicitly defers to the caller (`snapshot_hits` is a
//!   cumulative snapshot, not a drain) so the profile tracks the CURRENT task, not all
//!   history — a lane that switches from coding to chat sheds the coding experts.
//!
//! It has NO task of its own: the serving backend calls [`tick`](ServingExpertPager::tick)
//! from its EXISTING serving loop (no new tokio task — [[concurrency-style-guide]]). The
//! side effects it can't own (attaching the observer, doing the buft-override relaunch when
//! `relaunch_needed`, then calling [`mark_relaunched`](ServingExpertPager::mark_relaunched))
//! stay in the backend; this returns the DECISION.
//!
//! Backend integration is four lines:
//! ```ignore
//! let mut sp = ServingExpertPager::new(gguf_id, expert_bytes, margin, threshold, gate_seed);
//! params.expert_observer = Some(sp.observer());          // attach the tally
//! // ...each serving tick:
//! let out = sp.tick(&live_system_profile, predicted)?;   // predicted from M5's predictor (empty ok)
//! if out.relaunch_needed { relaunch_with_placement(sp.resident()); sp.mark_relaunched(); }
//! ```

use std::collections::HashMap;
use std::sync::Arc;

use super::expert_observer::LiveExpertObserver;
use super::expert_reconcile::{expert_pager_step, PagerError, PagerStepOutcome, RelaunchPager};
use super::expert_residency::{ExpertActivationProfile, ExpertId};
use super::system_profile::SystemProfile;
use crate::genome::working_set::PageRef;

/// Retention per tick for the hit EWMA. `decayed = decayed * ALPHA + delta_this_tick`. At
/// 0.9 a stopped expert keeps ~35% weight after 10 idle ticks and ~1% after ~44, so a task
/// switch fully turns over the hot set within tens of ticks — fast enough to re-page for a
/// new task, slow enough that one quiet turn doesn't evict a still-hot expert.
const DEFAULT_DECAY_ALPHA: f64 = 0.9;

/// The stateful serving-side driver. Constructed once per served MoE context; ticked from
/// the serving loop. Not `Clone` — there is exactly one per context (it owns the residency
/// truth for that context).
pub struct ServingExpertPager {
    /// The tally handed to the backend AND read here — one `Arc`, so writes on the compute
    /// thread are visible to the tick without any copy.
    observer: Arc<LiveExpertObserver>,
    /// The model whose experts these are — names the [`PageRef`]s the pager holds.
    gguf_id: String,
    /// One expert's on-disk/in-VRAM byte size (from the GGUF ingest). 0 = unmeasured → the
    /// planner's safe all-warm cold-start (never a false hot pin).
    expert_bytes: u64,
    /// VRAM headroom kept free below the serving budget (measurement error + jitter).
    margin_bytes: u64,
    /// Residency-churn a relaunch is worth (see [`RelaunchPager::relaunch_needed`]).
    relaunch_threshold: usize,
    /// The static GGUF gate-magnitude seed (empty = ride hits alone). Read-only after
    /// construction; folded into every tick's activation profile as the cold-start prior.
    gate_magnitude: HashMap<ExpertId, f32>,
    /// The residency set + served set across ticks.
    pager: RelaunchPager,
    /// EWMA of hit RATE per expert (decayed cumulative deltas) — the profile's `hits`.
    decayed: HashMap<ExpertId, f64>,
    /// Last cumulative snapshot, to derive this tick's delta (the observer only grows).
    last_raw: HashMap<ExpertId, u64>,
    /// Retention factor (see [`DEFAULT_DECAY_ALPHA`]).
    alpha: f64,
}

impl ServingExpertPager {
    /// Construct a driver for one served MoE context. Creates its own observer; hand it to
    /// the backend via [`observer`](Self::observer).
    pub fn new(
        gguf_id: impl Into<String>,
        expert_bytes: u64,
        margin_bytes: u64,
        relaunch_threshold: usize,
        gate_magnitude: HashMap<ExpertId, f32>,
    ) -> Self {
        Self {
            observer: LiveExpertObserver::new(),
            gguf_id: gguf_id.into(),
            expert_bytes,
            margin_bytes,
            relaunch_threshold,
            gate_magnitude,
            pager: RelaunchPager::new(),
            decayed: HashMap::new(),
            last_raw: HashMap::new(),
            alpha: DEFAULT_DECAY_ALPHA,
        }
    }

    /// The observer to attach to `ContextParams.expert_observer`. The backend MUST attach
    /// this exact `Arc` (not a fresh one) or the tick reads an empty tally forever.
    pub fn observer(&self) -> Arc<LiveExpertObserver> {
        Arc::clone(&self.observer)
    }

    /// The residency set the next relaunch should place — the backend reads this to build
    /// the buft-override placement args, then calls [`mark_relaunched`](Self::mark_relaunched).
    pub fn resident(&self) -> &std::collections::HashSet<PageRef> {
        self.pager.resident()
    }

    /// One serving-tick pass. Folds the decayed live hits, the predictor's forward-looking
    /// `predicted` prefetch signal (empty = no predictor), and the static gate seed into an
    /// [`ExpertActivationProfile`], runs [`expert_pager_step`] against the LIVE system
    /// profile, and returns the outcome. The caller acts on `relaunch_needed`.
    pub fn tick(
        &mut self,
        profile: &SystemProfile,
        predicted: HashMap<ExpertId, f32>,
    ) -> Result<PagerStepOutcome, PagerError> {
        let activation = ExpertActivationProfile {
            gate_magnitude: self.gate_magnitude.clone(),
            hits: self.decay_and_snapshot_hits(),
            predicted,
        };
        expert_pager_step(
            profile,
            &activation,
            &self.gguf_id,
            self.expert_bytes,
            self.margin_bytes,
            &mut self.pager,
            self.relaunch_threshold,
        )
    }

    /// Record that the backend relaunched the served context with the current residency set;
    /// [`relaunch_needed`](RelaunchPager::relaunch_needed) is false again until it churns.
    pub fn mark_relaunched(&mut self) {
        self.pager.mark_relaunched();
    }

    /// Advance the hit EWMA one tick and return it as integer `hits` for the profile.
    ///
    /// The observer's `snapshot_hits` is CUMULATIVE (monotonic per expert). We take this
    /// tick's delta (new - last), decay all prior weight by `alpha`, then add the fresh
    /// deltas — so an expert that stops firing decays toward 0 and drops out of the hot set
    /// (task-switch adaptation), while a currently-hot expert holds its rank. Rounded to the
    /// integer `hits` scale the planner's priority expects; sub-1 residue is pruned so a
    /// long-idle expert leaves the map entirely rather than lingering as noise.
    fn decay_and_snapshot_hits(&mut self) -> HashMap<ExpertId, u64> {
        let raw = self.observer.snapshot_hits();
        // Decay all existing weight first (covers experts that fired 0 times this tick).
        for v in self.decayed.values_mut() {
            *v *= self.alpha;
        }
        // Add this tick's fresh activity as the un-decayed term.
        for (&e, &now) in &raw {
            let before = self.last_raw.get(&e).copied().unwrap_or(0);
            let delta = now.saturating_sub(before) as f64;
            if delta > 0.0 {
                *self.decayed.entry(e).or_insert(0.0) += delta;
            }
        }
        self.last_raw = raw;
        // Drop fully-decayed experts so the map (and the plan) tracks only live demand.
        self.decayed.retain(|_, v| *v >= 0.5);
        self.decayed
            .iter()
            .map(|(&e, &v)| (e, v.round() as u64))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capacity::expert_reconcile::expert_page_ref;
    use crate::capacity::DeviceCapacity;
    use crate::governor::types::{HardwareClass, PowerSource, TargetSilicon, ThermalClass};
    use llama::ExpertObserver;

    const GB: u64 = 1024 * 1024 * 1024;
    const GGUF: &str = "unsloth/Qwen3-Coder-30B-A3B-Instruct-GGUF";

    fn eid(layer: u32, expert: u32) -> ExpertId {
        ExpertId { layer, expert }
    }

    // A 5090-shaped box: 32 GiB VRAM, 30 GiB live-free → 24 GiB *budgeted* (0.80). Its
    // serving budget fits a small number of experts, forcing the hot set to be a SUBSET —
    // which is the whole point of residency.
    fn small_budget_profile() -> SystemProfile {
        SystemProfile::from_parts(
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
                gpu_free_bytes_live: 30 * GB,
                system_ram_free_bytes: 100 * GB,
            },
            vec![],
            24,
        )
    }

    // what this catches: the observer→driver wiring — the Arc the driver hands out is the
    // SAME one the tick reads, so activity recorded via the backend callback shows up in the
    // pager's plan. If observer() returned a fresh Arc (the easy bug), the tick would read an
    // empty tally forever and the pager would never page anything hot.
    #[test]
    fn observer_arc_is_shared_so_ticks_see_recorded_activity() {
        let mut sp = ServingExpertPager::new(GGUF, 2 * GB, 2 * GB, 0, HashMap::new());
        let obs = sp.observer();
        // Simulate the backend's compute thread firing experts 0..3 heavily.
        for _ in 0..50 {
            obs.observe(0, &[0, 1, 2]);
        }
        let out = sp.tick(&small_budget_profile(), HashMap::new()).unwrap();
        assert!(out.hot_experts > 0, "recorded activity produced a hot set");
        assert!(
            out.relaunch_needed,
            "first non-empty residency set needs a relaunch (served set was empty)"
        );
    }

    // what this catches: TASK-SWITCH adaptation via decay. A lane that hammers experts 0..2
    // then switches to experts 5..7 must, after enough ticks, evict the old set and pin the
    // new one — otherwise the hot set is frozen to the first task and the second task
    // cold-misses every token. If decay is dropped (raw cumulative hits), the old experts'
    // huge accumulated counts would pin them forever and this fails.
    #[test]
    fn decay_lets_the_hot_set_follow_a_task_switch() {
        let mut sp = ServingExpertPager::new(GGUF, 8 * GB, 2 * GB, 0, HashMap::new());
        let obs = sp.observer();
        // Task A: experts 0,1,2 fire hard for several ticks.
        for _ in 0..10 {
            for _ in 0..100 {
                obs.observe(0, &[0, 1, 2]);
            }
            sp.tick(&small_budget_profile(), HashMap::new()).unwrap();
        }
        let out_a = sp.tick(&small_budget_profile(), HashMap::new()).unwrap();
        assert!(out_a.hot_experts > 0, "task A produced a hot set");

        // Task B: now experts 5,6,7 fire hard; 0,1,2 go silent. Run enough ticks to decay.
        for _ in 0..40 {
            for _ in 0..100 {
                obs.observe(0, &[5, 6, 7]);
            }
            sp.tick(&small_budget_profile(), HashMap::new()).unwrap();
        }
        // The resident set must now include the task-B experts and have shed task-A ones.
        let resident = sp.resident();
        let has_b = [5u32, 6, 7]
            .iter()
            .any(|&x| resident.contains(&expert_page_ref(GGUF, eid(0, x))));
        let has_a = [0u32, 1, 2]
            .iter()
            .all(|&x| resident.contains(&expert_page_ref(GGUF, eid(0, x))));
        assert!(has_b, "task-B experts paged into residency after the switch");
        assert!(!has_a, "task-A experts decayed out of residency (not all still pinned)");
    }

    // what this catches: mark_relaunched clears the relaunch signal — after the backend
    // relaunches, a tick with no NEW activity must NOT ask for another relaunch (else the
    // serving loop relaunches every tick forever, killing throughput).
    #[test]
    fn mark_relaunched_clears_the_signal_until_the_set_churns() {
        let mut sp = ServingExpertPager::new(GGUF, 2 * GB, 2 * GB, 0, HashMap::new());
        let obs = sp.observer();
        for _ in 0..50 {
            obs.observe(0, &[0, 1]);
        }
        let out = sp.tick(&small_budget_profile(), HashMap::new()).unwrap();
        assert!(out.relaunch_needed, "set changed → relaunch");
        sp.mark_relaunched();
        // Same activity, no churn: no second relaunch.
        let out2 = sp.tick(&small_budget_profile(), HashMap::new()).unwrap();
        assert!(
            !out2.relaunch_needed,
            "no churn after mark_relaunched → no relaunch"
        );
    }
}
