//! ServingDaemonModule — the ever-present ServiceModule that decides, and
//! continuously re-decides, how THIS host serves persona inference.
//!
//! It is the control loop around the pure [`crate::inference::planning::serving_plan`]
//! classifier. On each tick it takes an honest snapshot of the host — the
//! GPU/UMA serving budget plus the model footprints actually on disk — runs
//! [`plan_serving`], and publishes the resulting [`ServingPlan`] (which base
//! model, how many continuous-batching lanes, how many models to keep warm) on
//! a `watch` channel for the scheduler + spawner to read.
//!
//! This is the "ebb and flow" seam Joel describes: as pressure shifts and
//! demand changes, the plan is recomputed and republished — a huge MoE coder
//! pages in and the general model drops to one lane; demand falls, it flows
//! back. Driving the scheduler/spawner from the published plan and reacting to
//! the `PressureBroker` snapshot are the NEXT slices; this slice establishes
//! the loop, the published decision, and the `serving/plan` query command so
//! personas + operators can see what the substrate decided and why.
//!
//! Shape per docs/architecture/CONCURRENCY-STYLE-GUIDE.md: a ServiceModule with
//! a `tick_interval` that the runtime drives via `tick()`. No bespoke thread,
//! no lock held across await — the `watch::Sender` is the only shared state and
//! its `send` takes `&self`. cbar's pipeline-stage pattern in Rust dress.

pub mod academy_batch;
pub mod commands;
mod footprint;

use super::serving_consumer::{FootprintFn, ServingConsumer, SERVING_CONSUMER_ID};
use crate::capacity::placement::PlacementRequest;
use crate::cognition::model_resolver::types::HwCapabilityTier;
use crate::inference::planning::serving_plan::footprint::{footprint_for, footprint_from_parts};
use crate::inference::planning::serving_plan::{
    host_budget_from, plan_serving, plan_serving_at_rest, plan_serving_stable, HostBudget,
    HostBudgetInputs, ModelFootprint, ServingDemand, ServingPlan,
};
use crate::gpu::GpuMemoryManager;
use crate::inference::lane_registry::LaneRecord;
use crate::inference::llama_server::{
    ensure_model_serving_if_current, serving_v1_url, AdapterEntry, EnsureOutcome,
    LlamaServerControl, LlamaServerProcess, ServingSnapshot, ServingTarget, READY_TIMEOUT,
};
use crate::model_registry::live::{Availability, CatalogSnapshot, ModelCatalog};
use crate::model_registry::types::Model;
use crate::persona::hw_tier_descriptor::HwTierCategory;
use crate::resources::{LeaseBoard, ResourceDaemon, ResourceKind};
use crate::runtime::message_bus::MessageBus;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::system_resources::SystemResourceMonitor;
use continuum_base::time::EpochMs;
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// How often the daemon re-evaluates the serving plan. 5s matches the other
/// pressure-class ticks (pressure-broker, ai_provider) in the runtime. The
/// next slice makes the re-plan also fire on PressureBroker watch edges so it
/// reacts faster than the cadence under sudden pressure.
const TICK: Duration = Duration::from_secs(5);

/// Resolves a base-model id (as named in a [`ServingPlan`]) back to its full
/// [`Model`] struct. Production resolves through the global registry; tests
/// inject a fake so the reconcile WIRING can be exercised without a populated
/// registry. The resolved `Model` is carried straight onto the [`ServingTarget`]
/// — resolve once, pass the struct, never re-fetch ([[pass-the-model-struct-no-param-hell]]).
type ModelResolver = Arc<dyn Fn(&str) -> Option<Model> + Send + Sync>;

/// How the planner learns about a lane inherited from a PREVIOUS GENERATION of this core —
/// a past form of ourself, still resident, still holding its weights + KV.
///
/// This exists because of a measured boot defect (#438, glass-boxed 2026-08-19 from the probe
/// ledger). A core crashed with a healthy Qwen3.8-27B serving 4 lanes at a 25,075 window
/// (`serving.grow served_lanes=4 target_window=25075`). The successor booted while that lane was
/// STILL ALIVE holding ~45 GB, so the very first plan sampled `usable_gb=6` — an entirely HONEST
/// reading of free VRAM — and, seeing 6 GB, selected a 7B, then a 14B at `MIN_SERVE_CTX`. The old
/// lane was reclaimed 1.2 SECONDS LATER (`serving.lane_reclaim outcome=Reclaimed { pid: 11341 }`),
/// freeing all 45 GB; by then the downshift had already been actuated and the 14B was resident.
/// Ten seconds on, the planner was asking for the 27B again at `usable_gb=29..47` and could not
/// get back — a model swap needs the relaunch the plan had already spent.
///
/// The mechanism is NOT a bogus sample and NOT a reap-ordering race in the reclaim itself. It is
/// that BOTH anti-thrash defenses key on an incumbent, and boot has none:
///   * [`plan_serving_stable`]'s at-rest credit (which exists precisely so "a model's OWN
///     load/residency can never flap it out") never fires, because it credits back the
///     INCUMBENT's weights and `plan_tx` is empty on a fresh boot;
///   * [`downshift_gate`]'s sustained-ticks debounce (#368) is a no-op for the same reason —
///     with no incumbent there is nothing to debounce against.
///
/// So the one moment when a transient squeeze is GUARANTEED — the successor overlapping its
/// predecessor — is the exact moment every defense against transients is disabled. The comment
/// on [`ServingDaemonModule::publish_plan`] stated this as intent ("Boot's first plan has no
/// incumbent → plain selection"); it is the bug.
///
/// The fix is Joel's own framing of the governor: know our footprint, the system with us removed,
/// and **any past forms of ourself** — then work within that. An inherited lane is the third
/// category. It is ours, its memory is ours to reclaim, and it is therefore the incumbent the
/// first plan should reason against, not an external squeeze to flee from.
///
/// A seam and not a direct call to [`crate::inference::lane_registry::live_lane`] for the same
/// reason [`DecodeAgeSource`] is one: that function reads a filesystem singleton (the pidfile
/// plus `~/.continuum`), so under `cargo test` — one process, shared HOME — it would report the
/// OPERATOR's real 27B into unrelated unit tests. That exact leak was caught by measurement in
/// `serving_consumer` and fixed the same way.
type InheritedLaneSource = Arc<dyn Fn() -> Option<LaneRecord> + Send + Sync>;

/// How the heartbeat learns whether a REAL generation delivered tokens recently — the input
/// to the "trust busy lanes, probe quiet ones" short-circuit in
/// [`ServingDaemonModule::spawn_health_heartbeat_if_due`]. Milliseconds since the last real
/// decode, or `None` for "no decode observed yet, go probe".
///
/// This is a seam and not a direct call to
/// [`crate::inference::llama_server::ms_since_real_decode`] because that function reads a
/// PROCESS-GLOBAL atomic. Under `cargo test` the whole crate shares one process, so an
/// unrelated test that stamps the global (`llama_server`'s own `note_real_decode` coverage
/// does exactly that) silently converts every later heartbeat test into a short-circuit —
/// a red that a filtered local run can never reproduce, because the filter excludes the
/// test doing the stamping. Owning the evidence source per-daemon makes each test's answer
/// its own; production still reads the global, through the default below.
type DecodeAgeSource = Arc<dyn Fn() -> Option<u64> + Send + Sync>;
type LeasedInSentSource =
    Arc<dyn Fn() -> Vec<crate::inference::resource_admission::PromptSample> + Send + Sync>;

/// Where the heartbeat reads "consecutive REAL generations that failed on the live lane
/// since the last success" (#363). Defaults to the llama-server process-global stamped by
/// the adapter's local-lane error paths; tests inject their own so a parallel test that
/// stamps the global cannot leak order-dependence in
/// ([[a-process-global-read-inside-a-decision-makes-tests-order-dependent]]).
type RealFailsSource = Arc<dyn Fn() -> u64 + Send + Sync>;

pub struct ServingDaemonModule {
    gpu: Arc<GpuMemoryManager>,
    /// Live system memory monitor — the budget comes from what's actually FREE
    /// right now (`available_bytes`), not total capacity. On unified memory
    /// this drops when anything else grabs memory (a game, a build), so the
    /// plan ebbs and flows organically.
    system: Arc<SystemResourceMonitor>,
    /// The ONE per-machine resource authority (#56). Serving reads the GOVERNED
    /// VRAM headroom — capacity net of every external consumer (Bevy, LiveKit)
    /// and every outstanding lease — from this board, instead of a fraction of
    /// *total* VRAM blind to those consumers. That blindness was the
    /// `host_budget()` OOM bug; the board's `available(Vram)` is the precise
    /// net-of-everyone ceiling that fixes it. Mandatory (no Option): a host
    /// with no governor has no honest VRAM ceiling, and serving must fail
    /// closed rather than over-commit.
    resource_daemon: Arc<ResourceDaemon>,
    /// The plan's memory budget, SUSTAINED: every instantaneous reading of the last
    /// `READY_TIMEOUT`, so the plan grows only into headroom that has outlived one relaunch
    /// and shrinks the moment pressure arrives (`system_resources::sustained`).
    budget_readings: parking_lot::Mutex<crate::system_resources::sustained::Sustained>,
    /// The published decision. `None` until the first successful plan. Held as
    /// the module's only shared state; `send` takes `&self` so `tick()` can
    /// publish without interior-mutability gymnastics.
    plan_tx: watch::Sender<ServingPlanSnapshot>,
    /// Existing read-only wire/grid projection; never used to authorize reconciliation.
    plan_view_tx: watch::Sender<Option<ServingPlan>>,
    /// The serving-control leaf: owns the supervised `llama-server` child and
    /// reconciles it to the plan. A trait object so tests inject a fake; in
    /// production it is a `LlamaServerProcess` (which kills its child on Drop —
    /// so the daemon owning it means the daemon owns the server's lifetime).
    server: Arc<dyn LlamaServerControl>,
    /// The published serving state — which model is live, is it ready, on what
    /// `/v1` url. Adapters read THIS instead of probing the process; a grid
    /// allocator reads it to contract `(model, genome)` leases. Node down →
    /// empties; node up → republished. The grid seam.
    serving_tx: watch::Sender<ServingSnapshot>,
    /// Gate so at most one reconcile (which may spawn + wait for model load) is
    /// in flight. A tick that finds a reconcile already running skips — no
    /// stacked relaunches thrashing the GPU.
    reconciling: Arc<AtomicBool>,
    /// When the in-flight reconcile began (ms; 0 = none) and which await it is in
    /// (`ReconcileStep`), written by the operation, read by whoever skips on the gate.
    reconcile_started_ms: Arc<AtomicU64>,
    reconcile_step: Arc<AtomicU8>,
    /// When the gate skip last spoke (ms), so a busy reconcile is said once per
    /// [`RECONCILE_BUSY_SAY_EVERY`], never per tick.
    reconcile_busy_said_ms: Arc<AtomicU64>,
    /// Set once this core's planner has published its first decision (a plan or an honest
    /// none). Until then the authority may not yet hold a physical reading — a source
    /// with no reading reports 0 used, so the whole card reads free — and training
    /// admission is refused (card aae8af55, the boot-empty window).
    plan_published: Arc<AtomicBool>,
    academy_batch: parking_lot::Mutex<Option<academy_batch::BatchSlot>>,
    verified_target: watch::Sender<Option<VerifiedServingCapture>>,
    /// Reconcile-tick counter driving the slow liveness HEARTBEAT (fires when
    /// `% HEALTH_PROBE_EVERY_TICKS == 0`). See [`Self::spawn_health_heartbeat_if_due`].
    health_ticks: Arc<AtomicU64>,
    /// Consecutive ticks with NO published plan while a lane is LIVE (owned or inherited)
    /// — the 2026-09-26 18:22Z shape: a refused demotion left plan None beside a healthy
    /// engine, and no receipt could see it (the reconcile receipt needs an operation in
    /// flight; there was none). Said once per [`RECONCILE_BUSY_SAY_EVERY`] after
    /// [`PLAN_NONE_LIVE_LANE_TICKS`]; reset the tick a plan exists.
    plan_none_live_lane_ticks: Arc<AtomicU64>,
    plan_none_said_ms: Arc<AtomicU64>,
    /// Consecutive failed decode heartbeats — the hysteresis counter that keeps a
    /// merely-BUSY lane from being reaped. Reset to 0 on any passing probe or when the
    /// lane isn't believed-ready. Relaunch only once it reaches [`HEALTH_FAILS_TO_RELAUNCH`].
    health_fails: Arc<AtomicU8>,
    /// At most one heartbeat decode-probe in flight (a probe can take up to
    /// `DECODE_SMOKE_TIMEOUT` under load, longer than one [`TICK`]); a tick that finds one
    /// running skips, exactly like `reconciling`.
    health_probing: Arc<AtomicBool>,
    /// A prefill-knee `/slots` read is in flight ([`Self::spawn_prefill_knee_read`]).
    prefill_knee_reading: Arc<AtomicBool>,
    /// Set by the liveness heartbeat when it declares the live lane WEDGED, read+cleared by
    /// the next [`Self::reconcile_to_plan`]. It forces `ensure_model_serving`'s decode probe
    /// even on a child we own — otherwise the "trusted thereafter" short-circuit would
    /// re-adopt the poisoned-but-alive lane forever instead of relaunching it (#175).
    force_relaunch: Arc<AtomicBool>,
    /// The message bus, captured at `initialize`. Set once; `None` in tests
    /// constructed via `with_control` without a live runtime, so the emit is a
    /// silent no-op there. The daemon publishes [`ServingSnapshot`] changes on
    /// `SERVING_SNAPSHOT_EVENT` so any subscriber (local or grid) gets the live
    /// serving state pushed — no point-to-point receiver plumbing.
    bus: OnceLock<Arc<MessageBus>>,
    /// Maps a planned base-model id → its full [`Model`] for the reconcile to
    /// carry onto the [`ServingTarget`]. Defaults to the global registry; tests
    /// override it ([`Self::set_model_resolver`]).
    model_resolver: ModelResolver,
    /// Where the liveness heartbeat gets "how long since a real decode" from. Defaults to
    /// the llama-server process-global; tests own it ([`Self::set_decode_age_source`]).
    decode_age: DecodeAgeSource,
    /// The prompt sizes of the generates this seat served for OTHER nodes — the
    /// leased-in sample ring. Production reads `resource_admission`'s process-global ring;
    /// tests own it ([`Self::set_leased_in_sent_source`]): a ring shared by every test in
    /// the binary made one test's 56k samples another test's requirement (CI, 2026-09-20).
    leased_in_sent: LeasedInSentSource,
    /// The `/slots` activity fingerprint observed at the LAST missed smoke probe (L11).
    /// A changed fingerprint at the next miss proves the serve loop advanced between the
    /// two looks — work the adapter stamps can't see (ghost turns from a dead core's
    /// clients, a fresh boot's un-stamped atomics) — so the miss is queue contention,
    /// not wedge evidence. Cleared on every smoke success so a stale value can never
    /// exonerate a later freeze. `std::sync::Mutex` — held only for a copy, never
    /// across an await.
    last_miss_slots_fp: Arc<std::sync::Mutex<Option<u64>>>,
    frozen_slots: Arc<parking_lot::Mutex<Option<FrozenSlots>>>,
    /// Where the first plan after boot learns about a PAST FORM OF OURSELF still holding VRAM.
    /// Defaults to the lane registry; tests own it ([`Self::set_inherited_lane`]). See
    /// [`InheritedLaneSource`] for the measured defect this closes (#438).
    inherited_lane: InheritedLaneSource,
    /// Where the heartbeat reads the real-turn failure streak (#363). Sustained real
    /// failure is wedge evidence that OUTRANKS a passing smoke probe — the undersized-slot
    /// wedge class rejects the fleet's real prompts while a tiny probe still succeeds.
    real_fails: RealFailsSource,
    /// The last HEALTHY lane's (window, lanes) — the #363 prevention half. The #175
    /// sticky floor reads the live snapshot, but declaring a wedge EMPTIES that snapshot
    /// first, so every wedge-heal relaunch bypassed the floor by construction and spawned
    /// at whatever the teardown-transient plan said (the dying predecessor still holds
    /// its memory when the successor's budget samples free RAM — 2026-08-07: plan dipped
    /// to 9984 mid-teardown, successor spawned undersized, every 12k prompt rejected).
    /// These survive the empty snapshot; 0 = no healthy lane observed yet. Memory-safe:
    /// the predecessor's window IS the memory the successor inherits, and the existing
    /// overflow protection remains the backstop if it genuinely no longer fits.
    last_healthy_window: Arc<AtomicU32>,
    /// The discrete footprint arm's baseline: the TROUGH of device VRAM in use between
    /// the predecessor's reclaim and the successor's load. Armed to `u64::MAX` at spawn,
    /// lowered to the minimum sampled value on every tick until the lane is ready; the
    /// delta at a later tick is then the lane's own bytes. A single read at the spawn
    /// instant was the first cut and it read the DYING predecessor's ~24 GB off the
    /// board's sampled figure (2026-09-17 13:1xZ: delta 0) — a stale sample, not a
    /// baseline. `MAX` = armed but never lowered; 0 = no spawn observed (an adopted
    /// lane); both make the arm say could-not-look rather than charge the desktop.
    spawn_baseline_vram: Arc<AtomicU64>,
    /// What the serving engine reports it holds on a discrete GPU, recorded on the footprint
    /// tick and credited to serving on the memory board (card 741f5eec). Shared with the
    /// `ServingConsumer` this daemon registers.
    engine_device_credit: Arc<crate::modules::serving_consumer::EngineDeviceCredit>,
    last_healthy_lanes: Arc<AtomicU32>,
    /// The LIVE model universe — the SAME `Arc<ModelCatalog>` the `models/*`
    /// command surface mutates. The daemon plans off this snapshot, NOT the
    /// immutable seed registry, so a model acquired at runtime (`models/pull`
    /// flips it to [`Availability::Ready`] with its real on-disk path) becomes a
    /// serving candidate on the very next tick — no reboot. This is the consumer
    /// side of the rich API: serving reacts to the universe changing.
    catalog: Arc<ModelCatalog>,
    /// The persisted operator pin, as a handle this daemon and its `serving/pin` ·
    /// `serving/unpin` commands share. Handed in at construction so a test daemon
    /// owns a tempdir store and can never read or write the machine's real pin.
    pin_store: crate::modules::serving_pin_store::ServingPinStore,
    /// K3 expert-residency state for the CURRENTLY-served MoE model, `(model_id, context)`.
    /// Built lazily on the first reconcile that serves a given MoE model and rebuilt when the
    /// served model changes; `None` for a dense model or before the first MoE reconcile. A
    /// `std::sync::Mutex` because [`Self::reconcile_to_plan`] is synchronous and this is never
    /// held across an await. The pager inside owns the live-hit observer + the gate seed; each
    /// reconcile ticks it to decide the served expert-layer placement.
    moe_serving:
        std::sync::Mutex<Option<(String, Option<crate::capacity::moe_serving::MoeServingContext>)>>,
    /// Sticky publisher state for the governed host-cache lease (#287 slice 2) — the
    /// never-thrash layer between the per-tick raw derivation
    /// ([`host_cache_lease_bytes`](crate::capacity::host_cache_lease::host_cache_lease_bytes))
    /// and the plan-file write her per-token mtime poll consumes. Sub-band KV jitter
    /// never republishes; a material shrink publishes NOW; grow-back publishes past
    /// the band (#214). Same sync-Mutex discipline as `moe_serving` (never across await).
    host_cache_lease: std::sync::Mutex<crate::capacity::host_cache_lease::StickyLease>,
    /// Sticky publisher state for the DEVICE-side expert-slot budget (#305;
    /// BigMama's measured ask): free-VRAM-after-device-fit, from the live
    /// resource board (the serve's own residency is already a registered
    /// consumer per #79, so the board's available IS post-fit), less the
    /// placement margin. Same band + same never-thrash discipline as the
    /// host lease — raw VRAM flutters every tick; the published budget
    /// moves only on material change. Warm coverage scales directly with
    /// this (measured 2026-08-02: 13.8% @ 1.9 GiB → 65.7% @ 30 GiB).
    device_budget_lease: std::sync::Mutex<crate::capacity::host_cache_lease::StickyLease>,
    /// The artifact path currently registered in the process-wide
    /// [`serving_active_artifacts`](crate::system_resources::serving_active_artifacts)
    /// set (#302 invariant 1: the NvmeServingTierPool must never migrate the
    /// resident model out from under the engine). Tracked so a model change
    /// releases the old registration exactly once.
    active_artifact: Arc<std::sync::Mutex<Option<std::path::PathBuf>>>,
    /// The pin actuator's observation state (#281): tails the fork's
    /// routed-expert trace, owns the bandit, and remembers the last
    /// published pin list (the write-churn gate). `None` until the first
    /// MoE lease publish; rebuilt on geometry change. Same sync-Mutex
    /// discipline as `moe_serving` (never held across await).
    moe_trace_tail: std::sync::Mutex<Option<crate::capacity::trace_tail::MoeTraceTail>>,
    /// Division actuator (#2 of the resident/cache split, contract 2026-08-03): tier
    /// catalog discovered from the `--resident-only` manifests + warm-started
    /// `DivisionBandit` + the trace-tail reward watermark, for the ACTIVE MoE serve.
    /// Chooses which RESIDENT precision the next relaunch should load (`resident_tier`
    /// on the governed plan); the LIVE device budget stays #305's board-derived axis —
    /// one budget authority, one tier authority. Rebuilt on model change; `None` until
    /// a MoE serve exists. Same sync-Mutex discipline (never held across await).
    division: Arc<std::sync::Mutex<Option<crate::capacity::division_actuation::DivisionActuator>>>,
    /// The resident-override path the LAST reconcile applied to the spawn — ground truth
    /// for which tier the RUNNING serve actually loaded. Division rewards credit this
    /// tier, never the bandit's latest (unlaunched) choice — two-speed honesty.
    served_resident: Arc<std::sync::Mutex<Option<std::path::PathBuf>>>,
    /// MEASUREMENT-ONLY, off by default: an explicit forced VRAM budget for K3 expert
    /// placement, read ONCE at construction from `K3_MEASURE_FORCE_EXPERT_BUDGET_BYTES`. When
    /// `Some`, it OVERRIDES the governed ceiling so a model that would otherwise fit is driven
    /// to spill expert layers onto CPU — the only way to measure the B-gate (does cold-expert
    /// CPU-compute dominate decode?) without a model that genuinely overflows. It is an
    /// experimental control, NOT a substrate threshold: `None` in all normal operation, and
    /// every placement made under it is flagged on the `serving.k3_placement` probe so the
    /// numbers are never mistaken for real capacity. [[k3-slice2-A-vs-B-decision]]
    measure_force_expert_budget_bytes: Option<u64>,
    /// One coherent revisioned pin/suppression authority, shared with commands and
    /// pressure reclaim. Readers never reconstruct intent from separate watches.
    intent: ServingIntent,
    /// How many minds actually need a concurrent serving lane — the persona
    /// floor, set by the boot wiring BEFORE the first plan and updated if the
    /// population changes. Lanes come from DEMAND ([`plan_serving`] docs):
    /// llama-server splits `-c` evenly across slots, so every lane nobody
    /// asked for divides every mind's window for nothing (the 4-slots-for-2-
    /// personas starvation, 2026-07-10). Default 1 — window-first.
    lane_demand: Arc<std::sync::atomic::AtomicU32>,
    /// Consecutive ticks the PLAN has exceeded the live lane by at least
    /// [`REHOME_MIN_GAIN_PCT`]. Reset the moment it does not. Sustained-ness is
    /// what separates a real capacity change from memory jitter — see
    /// [`REHOME_SUSTAINED_TICKS`].
    rehome_streak: Arc<std::sync::atomic::AtomicU32>,
    /// Consecutive ticks a sustained re-home has been deferred by a steady hold.
    rehome_held_ticks: Arc<std::sync::atomic::AtomicU32>,
    /// The host's swap-in-use at the last grow decision (card 628dc958): a grow-back
    /// re-home is refused while this is rising. `u64::MAX` = no reading yet.
    last_swap_used: Arc<std::sync::atomic::AtomicU64>,

    /// How many CONSECUTIVE ticks the lane has been below plan while declining to
    /// re-home — a clock for the decline probe's cadence and nothing else.
    ///
    /// It exists because #3751 tried to throttle that probe off `rehome_streak`
    /// and silently did not throttle at all. `rehome_streak` counts consecutive
    /// ticks of a QUALIFYING GAIN; the declined state is precisely the state where
    /// no gain qualifies, so it is pinned at 0 for the whole episode and
    /// `streak <= 1` stayed true forever. Measured after that "fix" merged: 411
    /// identical lines, one per 5 s tick. A domain counter is not a logging clock —
    /// its reset rule belongs to the domain, and here the domain resets it exactly
    /// when we wanted it to count.
    decline_log_ticks: Arc<std::sync::atomic::AtomicU32>,
    /// The highest plan window seen in the current re-home streak (0 = none yet).
    /// "Sustained" requires the plan to have stopped CLIMBING: a new high resets
    /// the streak, so the boot staircase coalesces into one relaunch at its top.
    /// A plan that only alternates below a high it already reached is settled
    /// enough: both values want the bigger lane (2026-10-07: a lane at 2,304 under
    /// a plan alternating 33k/70k never re-homed for an hour, because every rise
    /// reset the streak).
    rehome_plan_high: Arc<std::sync::atomic::AtomicU64>,
    /// The last [`REHOME_SUSTAINED_TICKS`] plan windows the re-home decision saw: the span of
    /// evidence a streak needs. A re-home fires at the best of THESE, not at the streak's
    /// all-time high, so one transient high cannot make the fire tick unreachable (see
    /// [`at_recent_plan_high`]).
    rehome_recent_plans: Arc<parking_lot::Mutex<std::collections::VecDeque<u64>>>,
    /// L10 (#438): consecutive plan ticks wanting a DIFFERENT base model than the
    /// ready incumbent, and which model that was. A model swap re-homes every
    /// persona to different weights, so it earns the same sustained-streak bar as
    /// the window re-home above — a one-tick nothing-fits fallback (bogus
    /// usable_bytes sample) must never actuate a swap.
    model_change_streak: Arc<std::sync::atomic::AtomicU32>,
    pending_model_change: Arc<std::sync::Mutex<Option<String>>>,
    /// Consecutive ticks the fresh plan has wanted a LESS capable base model
    /// than the incumbent (a DOWNSHIFT). Reset the moment it stops. See
    /// [`DOWNSHIFT_SUSTAINED_TICKS`] — #368's second occurrence: a ~6-second
    /// RAM transient at solve launch read as usable_gb 17→0, one depressed tick
    /// flipped the PLAN to the 0.5B, and from then on hysteresis defended the
    /// WRONG incumbent. A brain flip must outlast jitter to be believed.
    downshift_streak: Arc<std::sync::atomic::AtomicU32>,
    /// (decision-fingerprint, served_window) of the last `serving.plan` probe
    /// actually emitted. The plan recomputes every tick, but the PROBE fires
    /// only when a DECISION changes or the window moves past a relative
    /// deadband (event-based law: emit on change, never per tick). Measured
    /// 2026-08-14: per-tick emission put 1,585 identical rows into 600s of
    /// probe stream — 51% of ALL rows — which rotated real history away in
    /// minutes and made every incident archeological (#399). Steady state is
    /// now silence; the live value stays queryable on demand via `serving/plan`.
    last_plan_probe: Arc<std::sync::Mutex<Option<(String, u32)>>>,
    /// Ticks remaining before another window re-home may fire. Charged to
    /// [`REHOME_COOLDOWN_TICKS`] when one fires, decremented every reconcile.
    /// Counted in ticks rather than wall-clock deliberately: the rate limit is on
    /// the daemon's own decision cadence, so it stays exact under a stopped clock,
    /// a suspended host, or a test that drives ticks by hand.
    rehome_cooldown: Arc<std::sync::atomic::AtomicU32>,
    /// The `ready_verified_at_ms` stamp of the launch the reconcile last settled on. A
    /// stamp it has not seen is a lane that just came READY, and a launch settles AFTER
    /// it is ready — see [`ready_edge`]. 0 = none yet.
    settled_ready_at_ms: Arc<std::sync::atomic::AtomicU64>,
    /// The hold live work puts on the engine, as last published ([`hold`]): kept here, not read
    /// back from the snapshot, because other publishers replace the snapshot whole, and the
    /// hold's start must survive them.
    serving_hold: Arc<parking_lot::Mutex<Option<crate::inference::llama_server::serving_hold::ServingHold>>>,
    /// Per-persona MEASURED turn demand — the window half of `serving_demand()`.
    /// Personas write (their deliberation faculty records every turn's unclamped
    /// cost); this daemon reads the ceiling when it plans. Replaces the
    /// `BOOTSTRAP_WORKING_SET` constant that used to stand in for this measurement
    /// and capped every citizen at 8192 tokens of a 128k-capable model.
    working_set: crate::cognition::working_set::WorkingSetRegistry,
    /// The long-lived vision SIDECAR lane (#106, `inference::vision_sidecar`):
    /// a small VL model serving beside a text-only mind so every persona has
    /// eyes. Owned here so it lives across reconciles and its child dies with
    /// the daemon (`EphemeralServingLane` Drop-kills). A tokio Mutex because
    /// [`vision_sidecar::ensure_sidecar`] awaits (spawn + `/props` verify)
    /// inside the reconcile task while holding the slot.
    vision_sidecar:
        Arc<tokio::sync::Mutex<Option<crate::inference::llama_server::EphemeralServingLane>>>,
}

impl ServingDaemonModule {
    pub fn new(
        gpu: Arc<GpuMemoryManager>,
        system: Arc<SystemResourceMonitor>,
        resource_daemon: Arc<ResourceDaemon>,
        catalog: Arc<ModelCatalog>,
        pin_store: crate::modules::serving_pin_store::ServingPinStore,
    ) -> Self {
        Self::with_control(
            gpu,
            system,
            resource_daemon,
            Arc::new(LlamaServerProcess::new()),
            catalog,
            pin_store,
        )
    }

    /// Construct with an injected serving control. Production uses
    /// [`Self::new`] (real `LlamaServerProcess`); tests inject a fake to drive
    /// the reconcile decision without a live process. `catalog` is the SAME live
    /// universe the `models/*` commands mutate — the daemon plans off its
    /// snapshot so runtime acquisitions become servable without a reboot.
    pub fn with_control(
        gpu: Arc<GpuMemoryManager>,
        system: Arc<SystemResourceMonitor>,
        resource_daemon: Arc<ResourceDaemon>,
        server: Arc<dyn LlamaServerControl>,
        catalog: Arc<ModelCatalog>,
        pin_store: crate::modules::serving_pin_store::ServingPinStore,
    ) -> Self {
        let (plan_tx, _rx) = watch::channel(ServingPlanSnapshot {
            intent_revision: 0,
            plan: None,
        });
        let (plan_view_tx, _) = watch::channel(None);
        let (serving_tx, _srx) = watch::channel(ServingSnapshot::empty());
        let (verified_target, _) = watch::channel(None);
        // The operator's pin is durable intent: seed the channel from the store so the
        // FIRST plan is computed under it (cards 3160b3d0 / 9552a01e — every reboot
        // used to lose the pin and the boot planner served whatever fit).
        // A RESTORED PIN IS VERIFIED BEFORE IT IS HONOURED.
        //
        // `serving/pin` fit-gates at set time, so a pin to an undownloaded model cannot
        // be created through the command. A pin RESTORED from disk has had no such
        // check — the world moved since it was written (weights evicted, a drive
        // unmounted, the file edited, a downgrade that wrote a row this build reads
        // differently), and the planner honours it absolutely: every downloaded
        // candidate is refused as "another model is pinned" and NOTHING loads.
        //
        // Measured twice on this host (2026-09-09/10, #3955): a pin to
        // `AtomicChat/Qwen3.8-Flash-Next-GGUF`, `availability: not_downloaded`, took the
        // node to zero lanes and zero citizens with NOTHING red — the planner correctly
        // honoured the pin, correctly refused non-pinned candidates, correctly declined
        // to load absent weights, and the machine was useless. Every component behaved
        // as designed. Only `serving/unpin` by hand recovered it.
        //
        // The pin FILE is deliberately left alone: it is durable operator intent, and
        // the model may be pulled later. It is simply not honoured this boot. So the
        // node self-heals to autonomic serving, says loudly why, and re-honours the pin
        // on the first boot after the weights arrive.
        let stored_pin = pin_store.load();
        let honoured = match stored_pin {
            None => {
                continuum_base::probe!(
                    class = "serving.pin.none_stored",
                    "no stored serving pin — the planner chooses"
                );
                None
            }
            Some(pin) => {
                let snapshot = catalog.snapshot();
                let servable = snapshot
                    .get(&pin.model_id)
                    .is_some_and(|entry| entry.status.availability == Availability::Ready);
                if servable {
                    continuum_base::probe!(
                        class = "serving.pin.restored",
                        model_id = %pin.model_id,
                        set_at_ms = pin.set_at_ms,
                        "serving pin restored from the state dir before the first plan"
                    );
                    Some(pin.model_id)
                } else {
                    continuum_base::probe!(
                        class = "serving.pin.unservable_ignored",
                        model_id = %pin.model_id,
                        set_at_ms = pin.set_at_ms,
                        "the stored serving pin names a model this host cannot serve (absent                          from the catalog, or catalogued but not downloaded) — IGNORING it for                          this boot so the planner can serve something real. The pin file is                          kept: pull the weights and it is honoured again on the next boot, or                          run serving/unpin to drop the intent."
                    );
                    None
                }
            }
        };
        let intent = ServingIntent::new(honoured);
        Self {
            verified_target,
            gpu,
            system,
            resource_daemon,
            budget_readings: parking_lot::Mutex::new(
                crate::system_resources::sustained::Sustained::new(
                    crate::inference::llama_server::READY_TIMEOUT,
                ),
            ),
            plan_tx,
            plan_view_tx,
            server,
            serving_tx,
            reconciling: Arc::new(AtomicBool::new(false)),
            reconcile_started_ms: Arc::new(AtomicU64::new(0)),
            reconcile_step: Arc::new(AtomicU8::new(0)),
            reconcile_busy_said_ms: Arc::new(AtomicU64::new(0)),
            plan_published: Arc::new(AtomicBool::new(false)),
            academy_batch: parking_lot::Mutex::new(None),
            health_ticks: Arc::new(AtomicU64::new(0)),
            plan_none_live_lane_ticks: Arc::new(AtomicU64::new(0)),
            plan_none_said_ms: Arc::new(AtomicU64::new(0)),
            health_fails: Arc::new(AtomicU8::new(0)),
            health_probing: Arc::new(AtomicBool::new(false)),
            prefill_knee_reading: Arc::new(AtomicBool::new(false)),
            force_relaunch: Arc::new(AtomicBool::new(false)),
            bus: OnceLock::new(),
            // Production resolver: the global registry. `try_global` (not the
            // panicking `global`) so a not-yet-initialized registry resolves to
            // None → honest empty snapshot, never a panic in the reconcile path.
            model_resolver: Arc::new(|id: &str| {
                crate::model_registry::try_global().and_then(|r| r.model(id).cloned())
            }),
            decode_age: Arc::new(crate::inference::llama_server::ms_since_real_work),
            leased_in_sent: Arc::new(crate::inference::resource_admission::leased_in_sent_samples),
            last_miss_slots_fp: Arc::new(std::sync::Mutex::new(None)),
            frozen_slots: Arc::new(parking_lot::Mutex::new(None)),
            inherited_lane: Arc::new(crate::inference::lane_registry::live_lane),
            real_fails: Arc::new(crate::inference::llama_server::consecutive_real_decode_failures),
            last_healthy_window: Arc::new(AtomicU32::new(0)),
            spawn_baseline_vram: Arc::new(AtomicU64::new(0)),
            engine_device_credit: Arc::new(
                crate::modules::serving_consumer::EngineDeviceCredit::default(),
            ),
            last_healthy_lanes: Arc::new(AtomicU32::new(0)),
            catalog,
            pin_store,
            intent,
            lane_demand: Arc::new(std::sync::atomic::AtomicU32::new(1)),
            rehome_streak: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            rehome_held_ticks: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            last_swap_used: Arc::new(std::sync::atomic::AtomicU64::new(u64::MAX)), // MAX = no reading yet: unknown is never "rising"
            decline_log_ticks: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            rehome_plan_high: Arc::new(std::sync::atomic::AtomicU64::new(0)), // 0 = no plan seen in this streak yet
            rehome_recent_plans: Arc::new(parking_lot::Mutex::new(
                std::collections::VecDeque::new(),
            )),
            model_change_streak: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            pending_model_change: Arc::new(std::sync::Mutex::new(None)),
            rehome_cooldown: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            settled_ready_at_ms: Arc::new(std::sync::atomic::AtomicU64::new(0)),
            serving_hold: Arc::new(parking_lot::Mutex::new(None)),
            downshift_streak: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            last_plan_probe: Arc::new(std::sync::Mutex::new(None)),
            working_set: crate::cognition::working_set::global(),
            moe_serving: std::sync::Mutex::new(None),
            active_artifact: Arc::new(std::sync::Mutex::new(None)),
            moe_trace_tail: std::sync::Mutex::new(None),
            division: Arc::new(std::sync::Mutex::new(None)),
            served_resident: Arc::new(std::sync::Mutex::new(None)),
            host_cache_lease: std::sync::Mutex::new(
                crate::capacity::host_cache_lease::StickyLease::new(HOST_CACHE_LEASE_BAND_DIVISOR),
            ),
            device_budget_lease: std::sync::Mutex::new(
                crate::capacity::host_cache_lease::StickyLease::new(HOST_CACHE_LEASE_BAND_DIVISOR),
            ),
            // Read ONCE here (single config entry point, no per-tick I/O). Off by default.
            measure_force_expert_budget_bytes: continuum_base::config_env::read(
                "K3_MEASURE_FORCE_EXPERT_BUDGET_BYTES",
            )
            .and_then(|s| s.trim().parse::<u64>().ok()),
            vision_sidecar: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }
}

mod policy;
use policy::{
    at_recent_plan_high, below_floor_refuses, decline_should_log, downshift_gate, floor_gate,
    grow_refused_while_paging, hold_overridden_by_shortfall, host_floor_of,
    incumbent_adoption_plan, incumbent_for_plan, lane_over_its_knee, model_change_gate,
    rehome_gain_evidence, roster_short_of_slots, DownshiftVerdict, ModelChangeGate,
    DOWNSHIFT_SUSTAINED_TICKS, REHOME_COOLDOWN_TICKS, REHOME_HOLD_DEFER_TICKS,
    REHOME_HOLD_OVERRIDE_PCT, REHOME_MIN_GAIN_PCT, REHOME_SUSTAINED_TICKS,
};
pub use policy::{
    knee_model, ready_edge, settles_this_tick, FloorVerdict, COLD_BOOT_BELOW_FLOOR_GRACE,
};
mod health;
use health::FrozenSlots;
#[cfg(test)]
use health::{FROZEN_ENGINE_MS, HEALTH_FAILS_TO_RELAUNCH, HEALTH_PROBE_EVERY_TICKS};
mod publication;
#[cfg(test)]
use publication::SERVING_SNAPSHOT_EVENT;
pub use publication::{published_lanes, VerifiedServingCapture};
use publication::{snapshot_from_outcome, verified_capture};
mod placement;
pub use placement::sidecar_window_for_headroom;
use placement::{
    sidecar_admissible_bytes, sidecar_planned_headroom_bytes, EXPERT_PLACEMENT_MARGIN_BYTES,
};
mod moe_cache;
#[cfg(test)]
use moe_cache::moe_host_cache_lease_inputs;
use moe_cache::{
    everyday_drive_mode, host_os_floor_bytes, serve_host_bytes, HOST_CACHE_LEASE_BAND_DIVISOR,
};
mod intent;
use intent::ServingPlanSnapshot;
pub use intent::{PinFit, PinFitChecker, ServingIntent, ServingIntentSnapshot};
mod lifecycle;
#[cfg(test)]
use lifecycle::reconcile_wedged;
use crate::inference::serving_lifecycle::{LifecycleGate, ReconcileStep, ServingOperation};
use lifecycle::{
    reconcile_wedged_at, ForcedProbe, PLAN_NONE_LIVE_LANE_TICKS, RECONCILE_BUSY_SAY_EVERY,
    RECONCILE_WEDGE_BOUND,
};

mod demand;
use crate::inference::lane_demand::lane_demand_overridden;
use demand::LAST_LEASED_IN_PEAK;
pub use demand::{resident_lane_demand, sustained_leased_in};
mod budget;
pub use budget::{governed_host_budget, live_host_budget, perf_cores, physical_usable_bytes};
#[cfg(test)]
use budget::{lane_admission_decision, pin_fit_decision};
pub use crate::inference::llama_server::serving_pool_kind;
use crate::inference::serving_holds::{serving_held_steady, serving_ludicrous_active};
#[cfg(test)]
use crate::inference::serving_holds::{ServingLudicrousHold, ServingSteadyHold};

mod device_budget;
#[cfg(test)]
use device_budget::governed_pool_ceiling;
use device_budget::{
    device_delta_beyond_weights, engine_props, governed_vram_ceiling,
    governed_vram_ceiling_or_report, vram_physical_used,
};
pub use device_budget::{plan_fill, plan_fraction, seed_device_vram_prior};
mod consumer;
pub use consumer::attribute_serving;
#[cfg(test)]
use consumer::serving_footprint_fn;
mod prompt_cache;
#[cfg(test)]
use prompt_cache::prompt_cache_decision;
/// What the sizing decided, for the ONE caller that builds a [`ServingTarget`]: the grant
/// to hand the engine, and the ONE-CONVERSATION floor to judge a RUNNING engine by. Both
/// come from the same `PromptCacheDecision`, so the spawn question ("how big should this
/// cache be?") and the running-engine question ("is the grant it still holds big enough?")
/// can never be answered from two different derivations.
pub(crate) struct PromptCacheGrant {
    pub(crate) mib: u32,
    pub(crate) one_conversation_mib: u32,
}
pub use prompt_cache::{
    active_cache_states, admitted_lanes, rotating_minds, rotation_window_ms, with_measured_state,
};
use prompt_cache::{derived_prompt_cache_mib, say_on_change};
mod catalog;
use catalog::servable_candidates;
#[cfg(test)]
use catalog::servable_candidates_with_refusals;
pub use catalog::{
    can_serve_minds, candidates_from_snapshot, detect_tier, RefusedCandidate, REFUSED_INELIGIBLE,
    REFUSED_NOT_PINNED, REFUSED_SUPPRESSED,
};
mod launch_window;
use launch_window::adopted_prompt_cache_mib;
#[cfg(test)]
use launch_window::{sticky_served_window, wedge_heal_floor};
mod launch;
mod planning;
mod hold;
mod reconcile;
mod tick;

#[cfg(test)]
pub(crate) mod tests;
