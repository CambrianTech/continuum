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
//! ## Not yet (next slices)
//! - Orientation metadata on regions + proportional-share across classes (R2/R3).
//! - `PersonaCognitionRegion` + the multi-tower inference router (command→handle→event).
//! - The sleep-phase consolidation/learning region.

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

use crate::persona::PersonaAircRuntimeRegistry;
use crate::runtime::{
    BrainRegion, CadenceHint, CadenceTable, CommandResult, ModuleConfig, ModuleContext,
    ModulePriority, Orientation, RegionContext, ServiceModule,
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
    /// Ticks that actually ran last pass, split by orientation class. The local-first
    /// metric the orientation-budget economy reads (R2): is the society's time going to
    /// stimulus, to interiority, or to growth? `ticked == by_orientation.total()`.
    pub by_orientation: OrientationTally,
}

/// Ticks run last pass, grouped by [`Orientation`] — telemetry only (what time was
/// *spent* on), distinct from the share *policy* (what it should be) that lands in R3.
#[derive(Debug, Clone, Default, Serialize, TS)]
pub struct OrientationTally {
    /// Outward-facing work: perception, recall, responding.
    #[ts(type = "number")]
    pub reactive: usize,
    /// The being's own interiority: curiosity, projects, dream/consolidation.
    #[ts(type = "number")]
    pub self_directed: usize,
    /// Growing the self: speciation / LoRA-genome learning.
    #[ts(type = "number")]
    pub speciation: usize,
}

impl OrientationTally {
    /// Record one tick against its region's declared orientation class.
    fn record(&mut self, orientation: Orientation) {
        match orientation {
            Orientation::Reactive => self.reactive += 1,
            Orientation::SelfDirected => self.self_directed += 1,
            Orientation::Speciation => self.speciation += 1,
        }
    }

    /// Total ticks tallied — equals the snapshot's `ticked` (invariant the tests pin).
    pub fn total(&self) -> usize {
        self.reactive + self.self_directed + self.speciation
    }
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
    snapshot: watch::Sender<GovernorSnapshot>,
}

impl SubstrateGovernor {
    /// Build with the regions to schedule + the live-persona registry. Regions are
    /// injected (not discovered) so the boot path owns what runs — add a region by
    /// passing it here; the governor needs no edit.
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
            snapshot,
        }
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

    /// One scheduling pass: tick every region once per live persona. Each region
    /// tick is `catch_unwind` + timeout isolated, so a fault in one (region,persona)
    /// never aborts the pass or kills the governor. Publishes a snapshot for
    /// lock-free observation + the `governor/status` command.
    async fn tick(&self) -> Result<(), String> {
        let tick = self.tick_seq.fetch_add(1, Ordering::Relaxed);
        let personas = self.personas.live_personas();

        // Prune cadence entries for personas that left, so the table can't grow
        // unbounded over a long-lived process. Lock taken + released here; never held
        // across an await (concurrency-style-guide rule).
        self.cadence.lock().unwrap().retain_personas(&personas);

        let mut ticked = 0usize;
        let mut published = 0usize;
        let mut faults = 0usize;
        let mut skipped = 0usize;
        let mut by_orientation = OrientationTally::default();

        for (region_idx, region) in self.regions.iter().enumerate() {
            // Static class — read once per region, not per persona.
            let orientation = region.orientation();
            for &persona_id in &personas {
                let key = (region_idx, persona_id);

                // Is this pair due? Lock briefly, decide, release BEFORE the await.
                let eligible = self.cadence.lock().unwrap().eligible(key, tick);
                if !eligible {
                    skipped += 1;
                    continue;
                }

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

                // Record the hint → schedule next eligibility. Lock taken + released
                // here, AFTER the await; never held across it.
                self.cadence.lock().unwrap().record(key, tick, hint);
            }
        }

        let snap = GovernorSnapshot {
            tick,
            regions: self.regions.iter().map(|r| r.id().0.to_string()).collect(),
            live_personas: personas.len(),
            ticked,
            published,
            faults,
            skipped,
            by_orientation,
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

    // what this catches: the tally maps each Orientation to its own bucket and total()
    // equals the sum — the invariant the snapshot relies on (ticked == by_orientation
    // .total()), so orientation telemetry can never silently miscount the society's time.
    #[test]
    fn orientation_tally_records_each_class_and_totals() {
        let mut t = OrientationTally::default();
        t.record(Orientation::Reactive);
        t.record(Orientation::Reactive);
        t.record(Orientation::SelfDirected);
        t.record(Orientation::Speciation);
        t.record(Orientation::Speciation);
        t.record(Orientation::Speciation);

        assert_eq!(t.reactive, 2);
        assert_eq!(t.self_directed, 1);
        assert_eq!(t.speciation, 3);
        assert_eq!(t.total(), 6, "total must equal the sum of all classes");
    }
}
