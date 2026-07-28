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
//! ## Pressure-driven scarcity (R4 slice 3)
//!
//! With [`SubstrateGovernor::with_pressure_gate`] the governor stops setting `slices_per_pass`
//! by hand and instead derives it each pass from live host **memory** pressure
//! ([`MemoryPressureMonitor`](crate::system_resources::MemoryPressureMonitor)). This is a
//! homeostatic **protection** — a reflex that keeps the backend safe, never cognition-steering
//! ([[commands-are-agency-algs-are-pathways]]): as system RAM climbs through the canonical
//! `PressureLevel` bands, the pass admits a shrinking fraction of the due pairs, so a society
//! of N personas each running an inference-bearing background region can't stampede the model
//! backend under load. The orientation floors inside [`admit_pass`] still claim that budget
//! first, so the reactive spine keeps breathing — the mind does *less background work* when the
//! host is starved, it never goes comatose. On a healthy host the band is `Normal` → budget
//! `None` → the budget lies dormant and behavior is unchanged. See [`pressure_budget`].
//!
//! ## Not yet (next slices)
//! - **Holistic per-machine pressure as the gate input.** The wired floor is system memory,
//!   which is already whole-machine RAM — but the gate must ultimately read the per-machine
//!   `ResourceGovernor`'s AGGREGATE snapshot (#56), not any continuum-inference-only number.
//!   Scarcity is everything on the box: the rest of the machine's processes PLUS all of our
//!   own consumers contending for the same VRAM/RAM — persona base models, LoRA adapters,
//!   Bevy rendering, LiveKit video encode/decode. VRAM-across-all-consumers is the key missing
//!   dimension (`PressureSignalKind::VramHigh` reserves it); a narrow `InferenceQueueDepth`
//!   metric is the wrong frame. The budget backs off when the *whole system* is starved, by
//!   whoever is using it, never just when inference is busy.
//! - **Memory topology is per-platform, so pressure is PER-POOL, not a single scalar.** On
//!   Apple Silicon memory is UNIFIED — GPU and CPU share one physical pool, so a model paged
//!   into "VRAM" eats system RAM directly and the wired RAM floor already captures GPU load.
//!   On Windows + discrete CUDA (e.g. the 5090) VRAM is a SEPARATE pool from main RAM — a
//!   model can saturate VRAM while RAM is calm, or the reverse. The `ResourceGovernor` reports
//!   per-pool pressure and the platform supplies the topology (how many pools exist); this
//!   gate then reacts to the BINDING constraint — the tightest pool (`max` of the normalized
//!   pressures) — so the same scheduler code is correct on both machines (DVFS: one Rust, a
//!   per-host governor view).
//! - `PersonaCognitionRegion` + the multi-tower inference router (command→handle→event).
//! - R5: the speciation (sleep-phase consolidation / genome-learning) region.

use std::any::Any;
use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use async_trait::async_trait;
use futures::FutureExt;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use ts_rs::TS;
use uuid::Uuid;

use crate::persona::PersonaAircRuntimeRegistry;
use crate::runtime::governor_bus::{publish_persona_scheduled, PersonaScheduled};
use crate::system_resources::{PressureLevel, PressureSnapshot};
use crate::runtime::message_bus::MessageBus;
use crate::runtime::registry::ModuleRegistry;
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
    /// Distinct beings that received a cognitive slice this pass and therefore had a
    /// `PersonaScheduled` breath emitted (the out-breath count). One per scheduled being,
    /// deduped across the (region × persona) fan-in. This is what residency / sentinels /
    /// demand-recall react to — `scheduled_emitted` is the size of that fan-out. `0` only
    /// when no being was admitted (idle society) or the bus isn't wired (pre-init).
    #[ts(type = "number")]
    pub scheduled_emitted: usize,
    /// The per-pass slice budget actually applied this pass — the *why* behind `deferred`.
    /// `None` = uncapped (calm host, or no pressure feed / static knob unset); `Some(n)` =
    /// the homeostatic cap the current memory band imposed (R4 slice 3). Surfacing the cap
    /// (not just its effect) keeps the protection a glass box — an operator sees the budget
    /// shrink as the host comes under pressure, never a silent throttle.
    #[ts(optional)]
    #[ts(type = "number")]
    pub slice_budget: Option<usize>,
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
    /// proportional share when more than `n` pairs are due. The STATIC knob (tests / an
    /// explicit boot override); when [`Self::pressure`] is wired it computes this per pass
    /// instead and takes precedence.
    slices_per_pass: Option<usize>,
    /// Live host memory-pressure feed (R4 slice 3). `Some` => each pass derives the per-pass
    /// slice budget from the current [`PressureLevel`] band (a homeostatic PROTECTION, see
    /// [`pressure_budget`]) instead of the static `slices_per_pass`. `None` => the static knob
    /// stands. Borrowed lock-free in the hot `tick()` — a watch read never blocks the pass.
    pressure: Option<watch::Receiver<PressureSnapshot>>,
    snapshot: watch::Sender<GovernorSnapshot>,
    /// Bus + registry captured at `initialize`, for emitting the per-pass
    /// `PersonaScheduled` out-breath. `OnceLock`: set exactly once at init, read lock-free
    /// in the hot tick. Absent (pre-init, or a test that drives `tick()` without a Runtime)
    /// => the governor still schedules normally; it just doesn't emit — there's no
    /// subscriber to hear it anyway, so the breath is a no-op, never a failure. The governor
    /// NEVER calls a reactor directly; emission is its only outward coupling.
    bus: OnceLock<Arc<MessageBus>>,
    registry: OnceLock<Arc<ModuleRegistry>>,
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
            pressure: None,
            snapshot,
            bus: OnceLock::new(),
            registry: OnceLock::new(),
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

    /// Wire the governor to a live memory-pressure feed (R4 slice 3). The boot path passes
    /// [`MemoryPressureMonitor::subscribe`](crate::system_resources::MemoryPressureMonitor::subscribe);
    /// each pass then sizes the slice budget to the current memory band ([`pressure_budget`])
    /// so a society of inference-bearing background regions can't stampede the model backend
    /// under load — a homeostatic protection, not cognition steering. Takes precedence over
    /// [`Self::with_slices_per_pass`] when both are set; on a healthy host the band is `Normal`
    /// → budget `None` → behavior is identical to the uncapped default.
    pub fn with_pressure_gate(mut self, pressure: watch::Receiver<PressureSnapshot>) -> Self {
        self.pressure = Some(pressure);
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

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // Capture the bus + registry so `tick()` can emit the per-pass
        // `PersonaScheduled` out-breath. Set-once; the hot tick reads lock-free.
        let _ = self.bus.set(ctx.bus.clone());
        let _ = self.registry.set(ctx.registry.clone());
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

        // The effective per-pass budget. When a live pressure feed is wired (production),
        // the homeostatic protection derives it from the current memory band (R4 slice 3),
        // proportional to how many pairs are actually due — so the cap scales with the real
        // flood risk. Otherwise the static `slices_per_pass` stands (tests / explicit boot
        // override / the uncapped default). `borrow()` is a lock-free read of the latest
        // published snapshot — it never blocks the tick.
        let slice_budget = match &self.pressure {
            Some(rx) => {
                let total_due: usize = groups.iter().map(|g| g.len()).sum();
                pressure_budget(rx.borrow().level, total_due)
            }
            None => self.slices_per_pass,
        };
        let (admitted, deferred_by_orientation) =
            admit_pass(&groups, &active_shares, slice_budget, tick);
        let deferred = deferred_by_orientation.total();

        // The distinct beings admitted this pass — each gets ONE out-breath, regardless of
        // how many of its regions ran. Collected before the tick loop consumes `admitted`,
        // preserving first-admitted order (stable, deterministic) while deduping the
        // (region × persona) fan-in. This is the society's "who is alive right now" set.
        let scheduled: Vec<Uuid> = {
            let mut seen = HashSet::new();
            admitted
                .iter()
                .filter_map(|(_, persona, _)| seen.insert(*persona).then_some(*persona))
                .collect()
        };

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

        // ── Out-breath: announce which beings got a cognitive slice ──────────────────
        // The governor EMITS; it does not call. Genome residency, sentinel-observers, and
        // demand-recall subscribe to `PersonaScheduled` and react on their own — adding a
        // reactor needs zero edits here. No cadence lock is held across these awaits (the
        // loop above took + released its locks per iteration). Absent bus/registry
        // (pre-init / Runtime-less test) => no subscribers exist, so emission is skipped.
        if let (Some(bus), Some(registry)) = (self.bus.get(), self.registry.get()) {
            for &persona in &scheduled {
                publish_persona_scheduled(bus, registry, &PersonaScheduled { persona, tick }).await;
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
            deferred,
            deferred_by_orientation,
            by_orientation,
            shares: active_shares,
            scheduled_emitted: scheduled.len(),
            slice_budget,
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

/// The smallest per-pass budget the gate will ever impose while any pair is due. A being
/// is never fully silenced by pressure — under emergency the pass still admits this many
/// (region × persona) ticks, and the orientation floors inside [`apportion`] hand them to
/// the reactive spine first. The mind does less *background* work when the host is starved;
/// it never goes comatose (BEING-SOCIETY-GOVERNOR.md, the never-coma rail).
const PRESSURE_SPINE_FLOOR: usize = 1;

/// Map a live memory `PressureLevel` band to the per-pass slice budget — the R4 slice 3
/// homeostatic **protection** ([[commands-are-agency-algs-are-pathways]]: a reflex that keeps
/// the backend safe, never cognition-steering). As system RAM climbs through the canonical
/// bands the pass admits a shrinking fraction of `total_due` (this pass's eligible-pair count),
/// so a society of N personas each running an inference-bearing background region can't
/// stampede the model backend under load. The cap is proportional to the actual flood risk: a
/// small or calm society is never throttled for free.
///
/// - `Normal` (< 80% RAM) → `None`: uncapped, the budget lies dormant — zero behavior change
///   on a healthy host (a being is never throttled while there's headroom).
/// - `Warning` (80–90%) → ¾ of due: gently trim the background population.
/// - `High` (90–95%) → ½ of due.
/// - `Critical` (> 95%) → ¼ of due, floored at [`PRESSURE_SPINE_FLOOR`]: minimal background,
///   the floors claim it for the reactive spine first.
///
/// The band thresholds (0.80/0.90/0.95) are the system-wide canonical memory tiers, NOT a
/// per-deployment env knob, and the monitor's built-in hysteresis (`consecutive_at_level`)
/// keeps the band — and thus the budget — stable rather than jittering on every RAM wobble.
/// The fractions are a fixed homeostatic curve (the same kind of codified protection as the
/// `PressureBroker`'s fixed tiers), not a heuristic that reads the persona to steer it.
///
/// Pure (no I/O, no locks, RNG-free) → testable in isolation, reproducible under replay.
fn pressure_budget(level: PressureLevel, total_due: usize) -> Option<usize> {
    if total_due == 0 {
        // Nothing due → nothing to cap; let `admit_pass` take the uncapped path.
        return None;
    }
    let fraction = match level {
        PressureLevel::Normal => return None, // uncapped: dormant on a healthy host
        PressureLevel::Warning => 3.0 / 4.0,
        PressureLevel::High => 1.0 / 2.0,
        PressureLevel::Critical => 1.0 / 4.0,
    };
    let budget = ((total_due as f64) * fraction).ceil() as usize;
    Some(budget.max(PRESSURE_SPINE_FLOOR))
}

// ─────────────────────────── governor/status ─────────────────────

/// Observe the governor's last scheduling pass. The read half of the daemon's
/// command surface — a citizen (or human/continuum) asks the deterministic governor what
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

    // what this catches: R4 slice 3 — a healthy host (Normal band) is NEVER capped, no matter
    // how many pairs are due. The budget stays dormant (`None` → admit_pass takes the uncapped
    // path), so wiring the pressure gate is zero behavior change on a machine with headroom.
    #[test]
    fn pressure_budget_uncapped_when_memory_normal() {
        assert_eq!(pressure_budget(PressureLevel::Normal, 0), None);
        assert_eq!(pressure_budget(PressureLevel::Normal, 1), None);
        assert_eq!(pressure_budget(PressureLevel::Normal, 1000), None);
    }

    // what this catches: nothing due → nothing to cap, at every band. A pass with no eligible
    // pairs must not manufacture a spurious budget the apportioner would then have to honor.
    #[test]
    fn pressure_budget_none_when_nothing_due() {
        for level in [
            PressureLevel::Normal,
            PressureLevel::Warning,
            PressureLevel::High,
            PressureLevel::Critical,
        ] {
            assert_eq!(pressure_budget(level, 0), None, "{level} with 0 due → None");
        }
    }

    // what this catches: the homeostatic curve actually shrinks the budget as memory climbs —
    // Warning ¾, High ½, Critical ¼ of the due pairs — so a large society backs off its
    // background population under load. Monotonic: more pressure never admits MORE.
    #[test]
    fn pressure_budget_shrinks_monotonically_under_rising_pressure() {
        let due = 100;
        let warning = pressure_budget(PressureLevel::Warning, due).unwrap();
        let high = pressure_budget(PressureLevel::High, due).unwrap();
        let critical = pressure_budget(PressureLevel::Critical, due).unwrap();
        assert_eq!(warning, 75, "Warning admits ¾ of due");
        assert_eq!(high, 50, "High admits ½ of due");
        assert_eq!(critical, 25, "Critical admits ¼ of due");
        assert!(
            warning > high && high > critical,
            "budget shrinks monotonically as pressure rises ({warning} > {high} > {critical})",
        );
    }

    // what this catches: the never-coma rail. Even under Critical pressure with only a couple
    // pairs due, the pass still admits at least PRESSURE_SPINE_FLOOR — a being is never fully
    // silenced by pressure; the orientation floors then hand that slice to the reactive spine.
    #[test]
    fn pressure_budget_never_silences_a_small_society() {
        // 2 due × ¼ = 0.5 → ceil 1, already ≥ floor; 1 due × ¼ = 0.25 → ceil 1.
        assert_eq!(pressure_budget(PressureLevel::Critical, 2), Some(1));
        assert_eq!(pressure_budget(PressureLevel::Critical, 1), Some(1));
        assert!(
            pressure_budget(PressureLevel::Critical, 1).unwrap() >= PRESSURE_SPINE_FLOOR,
            "the spine keeps breathing under emergency pressure",
        );
    }
}
