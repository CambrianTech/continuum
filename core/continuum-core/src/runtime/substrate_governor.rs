//! `SubstrateGovernor` — the deterministic daemon that schedules cognitive
//! regions, exposed + tunable as commands.
//!
//! ## What it is (slice 1)
//!
//! A `ServiceModule` (so the runtime's existing periodic ticker drives it on
//! cadence, with `catch_unwind` + quarantine) that owns the registered
//! [`BrainRegion`]s and, each governor tick, ticks every region **once per live
//! persona** ([`RegionContext::for_persona`]). It is **algorithmic and runs
//! itself** — independent of any "fuzzy" persona cognition — and its state is
//! **observable + tunable by commands** (`governor/status` here; tuning verbs land
//! as more commands), so a persona, a human widget, or `cu` all steer it the same
//! way. That is the whole design: control is inherent because it's commands
//! ([[control-and-collaboration-are-inherent-in-commands]]).
//!
//! ## Flood-safety
//!
//! Slice 1 schedules only memory-class regions (the hippocampus) — **no
//! inference**, so N personas × ticks can't melt the model backend. Each region
//! tick is wrapped in `catch_unwind` + a timeout so one hung/panicking region can
//! never take down the governor or another persona's tick. When
//! `PersonaCognitionRegion` (the inference-driving demand brain) lands, the
//! supply-side router + leases gate *placement* — the governor still never
//! silences a persona ([[persona-demand-system-supply-never-coma]]).
//!
//! ## Adaptive cadence (R1)
//!
//! Each pass, the governor consults a [`CadenceTable`] before ticking a
//! `(region, persona)` pair: a pair that asked to slow down (or to `Sleep`) is
//! *skipped* until it's due again, and the hint it returns tunes its next spacing.
//! `Sleep` is a low-cadence re-check **floor**, never removal — the mind never goes
//! comatose (BEING-SOCIETY-GOVERNOR.md, rail R1). This is the within-class causal
//! arbitration the orientation budget (R2+) sits on top of.
//!
//! ## Orientation telemetry + budget (R2/R3)
//!
//! Each region declares an [`Orientation`] class (R2). Every pass tallies what time
//! was *spent* per class ([`OrientationCounts`] in the snapshot). On top of that, R3
//! adds the *policy*: when the pass can't afford every due pair, an
//! [`OrientationShares`] vector decides which classes get the scarce slices, drawn by
//! deterministic stride scheduling ([`apportion`]) with spine-fixed floors so a flood
//! of reactive work can never starve interiority or growth — and a quiet society can
//! never starve responsiveness. The scarcity is expressed as `slices_per_pass`: `None`
//! (the unconstrained-machine default) admits every due pair and the budget lies
//! dormant; `Some(budget)` engages the proportional share. The within-class causal
//! arbitration (R1) still picks *when* each pair is due; the budget only picks *which
//! class* spends a contended pass.
//!
//! ## Adaptive share control (R4)
//!
//! With [`SubstrateGovernor::with_adaptive_shares`] the governor closes the loop: each pass
//! it reads the active shares from a [`ShareController`], applies them, then feeds the pass's
//! *measured* per-class deferral back to the controller, which reallocates the free ticket
//! pool toward the starved classes (within the spine floors) for the next pass. Without the
//! builder the shares are the static open-loop prior — the policy is swappable at one seam
//! ([[self-improvement-is-a-control-loop]]). On an uncapped/calm society there's no deferral,
//! so the controller holds the prior: zero behavior change on a capable host.
//!
//! ## Not yet (next slices)
//! - R4 slice 3: drive `slices_per_pass` from live host pressure (the scarcity knob is still
//!   set by hand / boot; the share *mix* above it is now measured + adaptive).
//! - `PersonaCognitionRegion` + the multi-tower inference router (command→handle→event).
//! - R5: the speciation (sleep-phase consolidation / genome-learning) region.

use std::any::Any;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use async_trait::async_trait;
use futures::FutureExt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use ts_rs::TS;
use uuid::Uuid;

use crate::persona::PersonaAircRuntimeRegistry;
use crate::runtime::{
    apportion, orientation_index, BrainRegion, CadenceHint, CadenceTable, CommandResult,
    ModuleConfig, ModuleContext, ModulePriority, Orientation, OrientationCounts, OrientationShares,
    RegionContext, ServiceModule, ShareController, ORIENTATIONS,
};
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx, DynCommand};

/// Governor base cadence. Moderate (memory consolidation is not realtime); the
/// per-region adaptive cadence (from `CadenceHint`) refines this later. Chosen
/// off the cadence ladder, not invented per-call.
const GOVERNOR_TICK: Duration = Duration::from_millis(1000);

/// Safety ceiling on a single region tick. A region that exceeds this is timed
/// out (its work resumes next tick from fresh state) — the governor never blocks
/// indefinitely on one region (RTOS rule: handlers never hang the scheduler).
const REGION_TICK_TIMEOUT: Duration = Duration::from_secs(5);

/// Observable snapshot of the governor's last scheduling pass — what `governor/status`
/// returns and what the `watch` channel publishes lock-free to any consumer.
#[derive(Debug, Clone, Default, Serialize, TS)]
pub struct GovernorSnapshot {
    /// Monotonic governor tick number.
    #[ts(type = "number")]
    pub tick: u64,
    /// Region ids currently scheduled.
    pub regions: Vec<String>,
    /// Live personas the last pass scheduled across.
    #[ts(type = "number")]
    pub live_personas: usize,
    /// (region × persona) ticks the last pass actually ran.
    #[ts(type = "number")]
    pub ticked: usize,
    /// Items regions pre-staged across the last pass (sum of TickOutcome.published).
    #[ts(type = "number")]
    pub published: usize,
    /// Region ticks that timed out or panicked last pass (caught, not fatal).
    #[ts(type = "number")]
    pub faults: usize,
    /// (region × persona) pairs not yet eligible this pass — resting on their adaptive
    /// cadence (incl. Sleep's re-check floor). High `skipped` vs `ticked` = a calm,
    /// well-paced society, not a stalled one.
    #[ts(type = "number")]
    pub skipped: usize,
    /// (region × persona) pairs that WERE due but lost the contended pass to the
    /// orientation budget (R3). `0` whenever `slices_per_pass` is `None` or the budget
    /// covered every due pair; positive only under real scarcity. A deferred pair stays
    /// eligible next pass (it isn't re-scheduled), so the budget bumps it, never drops it.
    #[ts(type = "number")]
    pub deferred: usize,
    /// The deferred pairs split by orientation class — WHERE the contention bites (R4
    /// slice 1). This is the measured signal the share controller will close its loop on:
    /// a class with sustained per-class deferral has unmet demand and wants more tickets;
    /// a class that never defers is well-served. `deferred_by_orientation.total() ==
    /// deferred`, mirroring `ticked == by_orientation.total()`.
    pub deferred_by_orientation: OrientationCounts,
    /// Ticks that actually ran last pass, split by orientation class. The local-first
    /// metric the orientation-budget economy reads (R2): is the society's time going to
    /// stimulus, to interiority, or to growth? `ticked == by_orientation.total()`.
    pub by_orientation: OrientationCounts,
    /// The share policy ACTUALLY applied this pass — telemetry of *what the budget should
    /// be*, next to `by_orientation`'s *what it was*. With an adaptive controller (R4) this
    /// shifts pass-to-pass as it tracks measured deferral; otherwise it's the static prior.
    pub shares: OrientationShares,
}

/// The scheduling daemon. Holds the regions + the live-persona registry; the
/// runtime ticks it via `ServiceModule::tick`.
pub struct SubstrateGovernor {
    regions: Vec<Arc<dyn BrainRegion>>,
    personas: PersonaAircRuntimeRegistry,
    tick_seq: AtomicU64,
    /// Per-(region, persona) adaptive cadence — the within-class causal arbitration
    /// (BEING-SOCIETY-GOVERNOR.md R1). Only the single tick task touches it; the lock
    /// is taken briefly and NEVER held across an `.await` (concurrency-style-guide rule).
    cadence: Mutex<CadenceTable>,
    /// The orientation budget policy (R3): how a contended pass is split across classes.
    /// The static open-loop prior + the seed for the adaptive controller. When `controller`
    /// is `None` this IS the active policy every pass; when `Some`, it's just the seed.
    shares: OrientationShares,
    /// The R4 adaptive share controller, installed by [`Self::with_adaptive_shares`]. `None`
    /// (default) = static `shares`. When present, the tick reads its current shares, then
    /// feeds back the measured per-class deferral so it tunes the next pass. Only the single
    /// tick task touches it; the lock is brief and NEVER held across an `.await`.
    controller: Option<Mutex<ShareController>>,
    /// Slice budget per pass — the scarcity knob. `None` (default) = admit every due
    /// pair (unconstrained machine; the budget is dormant). `Some(n)` engages the
    /// proportional share when more than `n` pairs are due. Set by R4 from live pressure.
    slices_per_pass: Option<usize>,
    snapshot: watch::Sender<GovernorSnapshot>,
}

impl SubstrateGovernor {
    /// Build with the regions to schedule + the live-persona registry. Regions are
    /// injected (not discovered) so the boot path owns what runs — add a region by
    /// passing it here; the governor needs no edit. Defaults to the open-loop share
    /// policy and an uncapped budget (every due pair runs); tune via the builders.
    pub fn new(
        regions: Vec<Arc<dyn BrainRegion>>,
        personas: PersonaAircRuntimeRegistry,
    ) -> Self {
        let (snapshot, _) = watch::channel(GovernorSnapshot::default());
        Self {
            regions,
            personas,
            tick_seq: AtomicU64::new(0),
            cadence: Mutex::new(CadenceTable::new()),
            shares: OrientationShares::default(),
            controller: None,
            slices_per_pass: None,
            snapshot,
        }
    }

    /// Override the static orientation share policy (R4 / tests). Floors are enforced by
    /// [`OrientationShares`] itself, so this can't construct a starving policy. With an
    /// adaptive controller installed this sets the controller's *seed* — call it before
    /// [`Self::with_adaptive_shares`].
    pub fn with_shares(mut self, shares: OrientationShares) -> Self {
        self.shares = shares;
        self
    }

    /// Install the R4 adaptive share controller, seeded from the current `shares`. The
    /// governor then tunes the orientation mix from measured per-class deferral each pass,
    /// within the spine floors — replacing the static open-loop prior with a closed loop.
    pub fn with_adaptive_shares(mut self) -> Self {
        self.controller = Some(Mutex::new(ShareController::new(self.shares)));
        self
    }

    /// Set the per-pass slice budget — the scarcity knob (R4 / tests). `Some(n)` engages
    /// the proportional share once more than `n` pairs are due in a pass.
    pub fn with_slices_per_pass(mut self, budget: Option<usize>) -> Self {
        self.slices_per_pass = budget;
        self
    }
}

#[async_trait]
impl ServiceModule for SubstrateGovernor {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "substrate-governor",
            priority: ModulePriority::Normal,
            command_prefixes: &[],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: Some(GOVERNOR_TICK),
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(
        &self,
        command: &str,
        _params: serde_json::Value,
    ) -> Result<CommandResult, String> {
        // The governor's commands are typed objects (see `commands`); nothing is
        // prefix-routed here.
        Err(format!("substrate-governor: '{command}' is a typed command object"))
    }

    /// One scheduling pass, in two phases:
    ///
    /// 1. **Collect due pairs**, grouped by orientation class — a single brief lock on
    ///    the [`CadenceTable`] decides eligibility for every `(region, persona)`.
    /// 2. **Apportion the pass under the orientation budget** ([`admit_pass`]), then tick
    ///    the admitted pairs. Each region tick is `catch_unwind` + timeout isolated, so a
    ///    fault in one pair never aborts the pass or kills the governor.
    ///
    /// Pairs that were due but lost the contended pass are *deferred* (counted, not
    /// re-scheduled) so they're first in line next pass — the budget bumps, never drops.
    /// Publishes a snapshot for lock-free observation + the `governor/status` command.
    async fn tick(&self) -> Result<(), String> {
        let tick = self.tick_seq.fetch_add(1, Ordering::Relaxed);
        let personas = self.personas.live_personas();

        // Prune cadence entries for personas that left, so the table can't grow
        // unbounded over a long-lived process. Lock taken + released here; never held
        // across an await (concurrency-style-guide rule).
        self.cadence.lock().unwrap().retain_personas(&personas);

        // ── Phase 1: collect due pairs, grouped by orientation class ──────────────
        // One lock spanning a purely synchronous loop (no await inside) → released
        // before any region tick. `groups[i]` holds the due pairs for `ORIENTATIONS[i]`.
        let mut groups: [Vec<(usize, Uuid, Orientation)>; 3] =
            [Vec::new(), Vec::new(), Vec::new()];
        let mut skipped = 0usize;
        {
            let cadence = self.cadence.lock().unwrap();
            for (region_idx, region) in self.regions.iter().enumerate() {
                let orientation = region.orientation(); // static class, read once per region
                let oi = orientation_index(orientation);
                for &persona_id in &personas {
                    if cadence.eligible((region_idx, persona_id), tick) {
                        groups[oi].push((region_idx, persona_id, orientation));
                    } else {
                        skipped += 1;
                    }
                }
            }
        }

        // ── Phase 2: apportion under the orientation budget, then tick the admitted ──
        // The active policy is the controller's current shares (R4) if installed, else the
        // static prior. Brief lock, released before any region tick.
        let active_shares = match &self.controller {
            Some(c) => c.lock().unwrap().shares(),
            None => self.shares,
        };
        let (admitted, deferred_by_orientation) =
            admit_pass(&groups, &active_shares, self.slices_per_pass, tick);
        let deferred = deferred_by_orientation.total();

        // Close the R4 loop: feed this pass's measured per-class deferral back so the
        // controller reallocates for the NEXT pass. Brief lock, before any await below.
        if let Some(c) = &self.controller {
            c.lock().unwrap().observe(deferred_by_orientation);
        }

        let mut ticked = 0usize;
        let mut published = 0usize;
        let mut faults = 0usize;
        let mut by_orientation = OrientationCounts::default();

        for (region_idx, persona_id, orientation) in admitted {
            let region = &self.regions[region_idx];
            let key = (region_idx, persona_id);
            let ctx = RegionContext::for_persona(tick, persona_id);
            // Isolate the region tick: timeout (never hang the scheduler) +
            // catch_unwind (a panicking region is quarantined to this call,
            // not propagated to kill the governor's own tick).
            let fut = std::panic::AssertUnwindSafe(region.tick(&ctx)).catch_unwind();
            let hint = match tokio::time::timeout(REGION_TICK_TIMEOUT, fut).await {
                Ok(Ok(outcome)) => {
                    ticked += 1;
                    by_orientation.record(orientation);
                    published += outcome.published;
                    // The region's own next-cadence wish (None == Hold).
                    outcome.cadence_hint
                }
                Ok(Err(_panic)) => {
                    faults += 1;
                    tracing::warn!(
                        region = %region.id().0,
                        persona_id = %persona_id,
                        "substrate-governor: region tick PANICKED — caught, governor stays up"
                    );
                    // Back a crash-looping pair off (but never remove it) so a fault
                    // can't hammer the scheduler every pass.
                    Some(CadenceHint::Slower)
                }
                Err(_elapsed) => {
                    faults += 1;
                    tracing::warn!(
                        region = %region.id().0,
                        persona_id = %persona_id,
                        timeout_ms = REGION_TICK_TIMEOUT.as_millis() as u64,
                        "substrate-governor: region tick TIMED OUT — skipped this pass"
                    );
                    Some(CadenceHint::Slower)
                }
            };

            // Record the hint → schedule next eligibility. Lock taken + released here,
            // AFTER the await; never held across it.
            self.cadence.lock().unwrap().record(key, tick, hint);
        }

        let snap = GovernorSnapshot {
            tick,
            regions: self.regions.iter().map(|r| r.id().0.to_string()).collect(),
            live_personas: personas.len(),
            ticked,
            published,
            faults,
            skipped,
            deferred,
            deferred_by_orientation,
            by_orientation,
            shares: active_shares,
        };
        // send_replace: always-current, no backlog; readers borrow lock-free.
        let _ = self.snapshot.send_replace(snap);
        Ok(())
    }

    /// The governor's command surface — observe (and later tune) the daemon. This
    /// is what makes it steerable by a persona / human / cu identically.
    fn commands(&self) -> Vec<Arc<dyn DynCommand>> {
        vec![Arc::new(GovernorStatusCommand {
            snapshot: self.snapshot.subscribe(),
        })]
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Decide which due pairs to tick this pass under the orientation budget (R3), and
/// measure where the contention bit (R4 slice 1). `groups[i]` holds the eligible pairs
/// for `ORIENTATIONS[i]`. Returns the flat admit list and the **per-class** deferred
/// count (eligible minus admitted, by class).
///
/// - `slices_per_pass == None` (or a budget ≥ the number of due pairs): admit everything
///   — the unconstrained-machine path, budget dormant, deferral zero across the board.
/// - `Some(budget)` under contention: [`apportion`] splits the budget across classes by
///   stride (floors guaranteed), then each class contributes its share, **rotated by the
///   tick number** so the same pairs aren't always the ones served across passes.
///
/// The per-class deferral is the measured demand signal R4's controller closes its loop
/// on — not a scalar, because the whole point is to see *which* class is starved.
///
/// Pure (no I/O, no locks, RNG-free), so the whole budget-enforcement decision is
/// testable without standing up live personas, and reproducible under replay.
fn admit_pass(
    groups: &[Vec<(usize, Uuid, Orientation)>; 3],
    shares: &OrientationShares,
    slices_per_pass: Option<usize>,
    tick: u64,
) -> (Vec<(usize, Uuid, Orientation)>, OrientationCounts) {
    let eligible = OrientationCounts {
        reactive: groups[0].len(),
        self_directed: groups[1].len(),
        speciation: groups[2].len(),
    };
    let total_eligible = eligible.total();

    let admit = match slices_per_pass {
        Some(budget) if total_eligible > budget => apportion(shares, eligible, budget),
        _ => eligible, // uncapped, or budget already covers everyone
    };

    let mut out = Vec::with_capacity(admit.total());
    let mut deferred = OrientationCounts::default();
    for (oi, group) in groups.iter().enumerate() {
        let orientation = ORIENTATIONS[oi];
        if group.is_empty() {
            continue;
        }
        let take = admit.get(orientation).min(group.len());
        // Rotate the starting point by the tick so a capped class doesn't always serve
        // its first members (cross-pass fairness within the class).
        let start = (tick as usize) % group.len();
        for k in 0..take {
            out.push(group[(start + k) % group.len()]);
        }
        // Whatever this class couldn't fit this pass is its measured unmet demand.
        for _ in 0..(group.len() - take) {
            deferred.record(orientation);
        }
    }

    (out, deferred)
}

// ─────────────────────────── governor/status ─────────────────────

/// Observe the governor's last scheduling pass. The read half of the daemon's
/// command surface — a citizen (or human/cu) asks the deterministic governor what
/// it's doing. `AiSafe`: read-only telemetry.
pub struct GovernorStatusCommand {
    snapshot: watch::Receiver<GovernorSnapshot>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
pub struct GovernorStatusParams {}

#[async_trait]
impl ActionCommand for GovernorStatusCommand {
    const NAME: &'static str = "governor/status";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Report the substrate governor's last scheduling pass: regions, live personas, ticks run, \
         items published, faults. Read-only view of the cognitive scheduler.";
    type Params = GovernorStatusParams;
    type Output = GovernorSnapshot;

    async fn run(&self, _ctx: &Ctx, _p: GovernorStatusParams) -> Result<GovernorSnapshot, CommandError> {
        Ok(self.snapshot.borrow().clone())
    }
}

crate::register_command!(GovernorStatusCommand);

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a class group of `n` synthetic pairs for `orientation`, region index `ri`.
    fn group(ri: usize, orientation: Orientation, n: usize) -> Vec<(usize, Uuid, Orientation)> {
        (0..n)
            .map(|k| (ri, Uuid::from_u128(k as u128 + 1), orientation))
            .collect()
    }

    // what this catches: the unconstrained-machine path. With no slice budget every due
    // pair is admitted and nothing is deferred — the orientation budget stays dormant
    // until scarcity is declared (a being on a capable host is never throttled for free).
    #[test]
    fn admit_pass_admits_all_when_uncapped() {
        let groups = [
            group(0, Orientation::Reactive, 3),
            group(1, Orientation::SelfDirected, 2),
            group(2, Orientation::Speciation, 1),
        ];
        let (admitted, deferred) =
            admit_pass(&groups, &OrientationShares::first_best_guess(), None, 0);
        assert_eq!(admitted.len(), 6);
        assert_eq!(deferred.total(), 0);
    }

    // what this catches: a budget that covers every due pair behaves exactly like the
    // uncapped path — no spurious deferral at the boundary.
    #[test]
    fn admit_pass_admits_all_when_budget_covers() {
        let groups = [
            group(0, Orientation::Reactive, 2),
            group(1, Orientation::SelfDirected, 2),
            group(2, Orientation::Speciation, 2),
        ];
        let (admitted, deferred) =
            admit_pass(&groups, &OrientationShares::first_best_guess(), Some(6), 0);
        assert_eq!(admitted.len(), 6);
        assert_eq!(deferred.total(), 0);
    }

    // what this catches: real contention. A budget below the due count admits exactly the
    // budget and defers the rest; the split honors the share policy (reactive 7 outweighs
    // self_directed 2 outweighs speciation 1). This is the budget actually biting.
    #[test]
    fn admit_pass_defers_under_contention_and_follows_shares() {
        let groups = [
            group(0, Orientation::Reactive, 100),
            group(1, Orientation::SelfDirected, 100),
            group(2, Orientation::Speciation, 100),
        ];
        let (admitted, deferred) =
            admit_pass(&groups, &OrientationShares::first_best_guess(), Some(10), 0);
        assert_eq!(admitted.len(), 10);
        assert_eq!(deferred.total(), 290);

        let mut by = OrientationCounts::default();
        for (_, _, o) in &admitted {
            by.record(*o);
        }
        assert_eq!(by.total(), 10);
        assert!(by.reactive > by.self_directed, "reactive (7) gets the largest share");
        assert!(by.self_directed >= by.speciation, "self_directed (2) ≥ speciation (1)");
    }

    // what this catches: cross-pass fairness. When a class is capped, the rotation by tick
    // means a later pass serves *different* members of that class — no pair is permanently
    // starved behind the budget while others always win.
    #[test]
    fn admit_pass_rotates_capped_class_across_ticks() {
        // One class, 4 due pairs, budget admits only 2 → which 2 must rotate with tick.
        let groups = [
            group(0, Orientation::Reactive, 4),
            Vec::new(),
            Vec::new(),
        ];
        let shares = OrientationShares::new(1, 1, 0);
        let (pass0, _) = admit_pass(&groups, &shares, Some(2), 0);
        let (pass1, _) = admit_pass(&groups, &shares, Some(2), 1);
        let ids = |v: &[(usize, Uuid, Orientation)]| v.iter().map(|p| p.1).collect::<Vec<_>>();
        assert_eq!(pass0.len(), 2);
        assert_eq!(pass1.len(), 2);
        assert_ne!(ids(&pass0), ids(&pass1), "rotation serves different members each pass");
    }

    // what this catches: a constrained node with speciation OFF (0 tickets) never schedules
    // growth work even when it's due — the deferral is declared (counted), not a silent
    // mix-in, and the surviving budget goes to the floored classes.
    #[test]
    fn admit_pass_never_schedules_zero_share_class() {
        let groups = [
            group(0, Orientation::Reactive, 5),
            group(1, Orientation::SelfDirected, 5),
            group(2, Orientation::Speciation, 5),
        ];
        let shares = OrientationShares::new(1, 1, 0); // speciation off
        let (admitted, deferred) = admit_pass(&groups, &shares, Some(4), 0);
        assert_eq!(admitted.len(), 4);
        assert_eq!(deferred.total(), 11);
        // The measured signal must show ALL 5 speciation pairs deferred — a 0-ticket
        // class registers as pure unmet demand, exactly what a controller would read to
        // decide whether growth is being starved by policy vs. simply not due.
        assert_eq!(deferred.get(Orientation::Speciation), 5, "all growth demand deferred");
        assert!(
            admitted.iter().all(|(_, _, o)| *o != Orientation::Speciation),
            "0-ticket class is never admitted"
        );
    }

    // what this catches: R4 slice 1 — the per-class deferral measurement is correct, the
    // signal a controller steers on. Under contention the breakdown must equal
    // eligible-minus-admitted for EACH class (not just sum right), and total back to the
    // scalar. A wrong split here = the controller reallocates toward the wrong class.
    #[test]
    fn admit_pass_reports_per_class_deferral() {
        let groups = [
            group(0, Orientation::Reactive, 10),
            group(1, Orientation::SelfDirected, 10),
            group(2, Orientation::Speciation, 10),
        ];
        // first_best_guess = 7/2/1; budget 10 of 30 due → 7 reactive, 2 self_directed,
        // 1 speciation admitted, so deferral is the complement per class.
        let (admitted, deferred) =
            admit_pass(&groups, &OrientationShares::first_best_guess(), Some(10), 0);

        let mut admitted_by = OrientationCounts::default();
        for (_, _, o) in &admitted {
            admitted_by.record(*o);
        }
        for o in ORIENTATIONS {
            assert_eq!(
                deferred.get(o),
                10 - admitted_by.get(o),
                "per-class deferral = eligible - admitted for {o:?}"
            );
        }
        assert_eq!(deferred.total(), 20, "20 of 30 deferred");
        assert_eq!(deferred.total(), 30 - admitted.len());
    }
}
