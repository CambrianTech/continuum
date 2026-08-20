//! ServingDaemonModule — the ever-present ServiceModule that decides, and
//! continuously re-decides, how THIS host serves persona inference.
//!
//! It is the control loop around the pure [`crate::cognition::serving_plan`]
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

use super::serving_consumer::{FootprintFn, ServingConsumer, SERVING_CONSUMER_ID};
use crate::capacity::placement::PlacementRequest;
use crate::cognition::model_resolver::types::HwCapabilityTier;
use crate::cognition::serving_plan::{
    plan_serving, plan_serving_stable, HostBudget, ModelFootprint, ServingDemand, ServingPlan,
    MIN_SERVE_CTX,
};
use crate::gpu::GpuMemoryManager;
use crate::inference::lane_registry::LaneRecord;
use crate::inference::llama_server::{
    ensure_model_serving, serving_v1_url, AdapterEntry, EnsureOutcome, LlamaServerControl,
    LlamaServerProcess, ServingSnapshot, ServingTarget, READY_TIMEOUT,
};
use crate::model_registry::live::{Availability, CatalogSnapshot, ModelCatalog};
use crate::model_registry::types::{Capability, Model};
use crate::persona::hw_tier_descriptor::HwTierCategory;
use crate::resources::{LeaseBoard, ResourceDaemon, ResourceKind};
use crate::runtime::message_bus::MessageBus;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::system_resources::SystemResourceMonitor;
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, AtomicU8, AtomicUsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// How often the daemon re-evaluates the serving plan. 5s matches the other
/// pressure-class ticks (pressure-broker, ai_provider) in the runtime. The
/// next slice makes the re-plan also fire on PressureBroker watch edges so it
/// reacts faster than the cadence under sudden pressure.
const TICK: Duration = Duration::from_secs(5);

/// The margin, in percent, by which the plan must exceed the LIVE lane before a
/// re-home is worth killing in-flight turns for.
///
/// RELATIVE, not absolute: the same 2048-token shortfall is a third of a small
/// lane and noise on a large one, so an absolute bar means something different at
/// every window size.
///
/// MEASURED against 6,322 live `serving.plan` samples (probes.jsonl, 2026-08-06),
/// after BigMama flagged that she had proposed 15% from the shape of the problem
/// with no data behind it: consecutive-sample plan jitter runs p50 0%, p90 1%, with
/// a fat tail (p99 22%, max 106%). 15% sits an order of magnitude above the p90
/// noise floor while staying far under the 60% shortfall of the real incident.
///
/// Note which parameter is actually load-bearing, because it is NOT this one: at a
/// 10%, 15% OR 20% bar the longest run of consecutive qualifying rises in the whole
/// sample was 2. The jitter tail is a SPIKE, not a sustained climb, so
/// [`REHOME_SUSTAINED_TICKS`] is what rejects it and this margin only has to clear
/// the p90. Widen it only with evidence; loosening the tick count is the change that
/// would actually let noise through.
const REHOME_MIN_GAIN_PCT: u32 = 15;

/// How many consecutive ticks the plan must exceed the lane by
/// [`REHOME_MIN_GAIN_PCT`] before a re-home is justified.
///
/// Sustained-ness is the anti-thrash property the old single-sample `live * 2 <=
/// plan` ratio was reaching for, stated directly — and it is deliberately SHORT.
/// It is not what limits the relaunch RATE (that is [`REHOME_COOLDOWN_TICKS`],
/// enforced independently below); it only has to outlast jitter.
///
/// THE load-bearing parameter, and measured as such: across 6,322 live
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
const REHOME_SUSTAINED_TICKS: u32 = 3;

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
const DOWNSHIFT_SUSTAINED_TICKS: u32 = 3;

/// Ticks a re-home must wait before another may fire, enforced INDEPENDENTLY of
/// the sustained-delta test above.
///
/// The two guards answer different questions and must not be collapsed into one:
/// sustained-delta asks "is this gain real?", the cooldown asks "may we pay for it
/// yet?". A relaunch kills every in-flight turn on the lane, so the rate limit has
/// to hold even when the evidence is perfect. Anchored to a lane's own readiness
/// budget ([`READY_TIMEOUT`], 90s) in ticks: never start a second re-home before
/// the first could possibly have come up and proven itself.
/// [[never-thrash-sticky-hysteresis-on-every-lane]]
const REHOME_COOLDOWN_TICKS: u32 = (READY_TIMEOUT.as_secs() / TICK.as_secs()) as u32;

/// Slow liveness HEARTBEAT cadence (#175 self-heal): reconcile ticks between decode
/// smoke-probes of the LIVE lane. The reconcile fast-path trusts the published `ready`
/// snapshot and never re-probes a child it owns ([`crate::inference::llama_server`] —
/// "trusted thereafter, no per-tick decode load"), so a Metal-GPU-OOM-POISONED backend —
/// which answers every control-plane read (`/health`, `/v1/models`, `/props`) with 200
/// while every `llama_decode` 500s (`kIOGPUCommandBufferCallbackErrorOutOfMemory`) — stays
/// `ready:true` FOREVER and the persona substrate is bricked until a human reboots. This
/// heartbeat re-verifies the actual COMPUTE path on a cadence far slower than [`TICK`], so
/// it costs one tiny generation per minute, not one per tick.
const HEALTH_PROBE_EVERY_TICKS: u64 = 12; // 12 × 5s ≈ 60s

/// Consecutive failed heartbeats before the lane is declared WEDGED and flipped
/// not-ready (which the reconcile turns into a kill+respawn). Pure hysteresis: ONE failed
/// probe can be a merely-BUSY lane (a saturated slot times out the 10s decode probe during
/// a wake burst), and reaping a healthy-but-busy lane is the exact thrash that got the
/// prior watchdog reverted (commit 93753d812). Two consecutive failures ≈ 2 minutes of
/// sustained no-decode = genuinely poisoned, not transiently loaded.
const HEALTH_FAILS_TO_RELAUNCH: u8 = 2;

/// Bus topic the live [`ServingSnapshot`] is emitted on whenever it changes.
/// Subscribers (embedding, supervisor, inference_session, ai_provider) declare
/// this in their `event_subscriptions` and cache the latest in `handle_event`
/// instead of probing the process — the cbar pipeline-stage shape: one organ
/// emits its state, everything that needs it subscribes. Because the bus spans
/// the grid, a remote lease allocator subscribes to the SAME topic. Routed by
/// name (no body parse in middleware), payload fans out as a shared pointer,
/// emitted only on a rare state change — never on the token hot path.
const SERVING_SNAPSHOT_EVENT: &str = "serving.snapshot";

/// VRAM headroom held below the governed serving budget when fitting expert LAYERS for K3
/// placement. `layer_bytes` sizes the stacked expert blob, but the `-ot`'d layer also carries
/// its router + norms and the per-layer size can drift; this margin keeps the fit honest. 512 MiB.
const EXPERT_PLACEMENT_MARGIN_BYTES: u64 = 512 * 1024 * 1024;
/// Hot-layer churn (symmetric difference vs the served set) that justifies a llama-server
/// RESPAWN. A respawn reloads every weight (seconds), so a 1–2 layer flap must not trigger one;
/// only a real residency shift (a task change moving > 2 expert layers) does.
const RELAUNCH_LAYER_CHURN_THRESHOLD: usize = 2;
/// Sticky-lease hysteresis band for the host-cache budget: 1/8 of the published
/// value (move only on >12.5% change). Same band shape as the plan hysteresis —
/// KV-sample flutter must not churn the plan file's mtime under her per-token poll.
const HOST_CACHE_LEASE_BAND_DIVISOR: u64 = 8;
/// Recency-window tokens carried on the published plan document. Inert while
/// `pin_list` is empty (v1 publishes budget-only); matches the golden v1 wire
/// fixture so the C++ consumer's parse sees the shape it was validated against.
// context-budget-exempt: the EXPERT PAGER's recency window (thousands of tokens of routing history for the pin list), part of the published plan wire format the C++ consumer validates against — not the prompt
const MOE_PLAN_WINDOW_K: u32 = 8;

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
fn incumbent_for_plan(published: Option<String>, inherited: Option<&LaneRecord>) -> Option<String> {
    published.or_else(|| inherited.map(|rec| rec.model.clone()))
}

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

/// Where the heartbeat reads "consecutive REAL generations that failed on the live lane
/// since the last success" (#363). Defaults to the llama-server process-global stamped by
/// the adapter's local-lane error paths; tests inject their own so a parallel test that
/// stamps the global cannot leak order-dependence in
/// ([[a-process-global-read-inside-a-decision-makes-tests-order-dependent]]).
type RealFailsSource = Arc<dyn Fn() -> u64 + Send + Sync>;

/// The verdict for force-serving one specific model on this host RIGHT NOW —
/// what [`ServingDaemonModule::pin_fit_checker`] returns and `serving/pin` gates
/// on. Carries the numbers so the command can fail loud with a NAMED shortfall
/// ("needs ~XGB, host budget is ~YGB") rather than a bare refusal.
pub struct PinFit {
    /// The host-fit plan for the model alone. `None` when the model has no GGUF
    /// on disk (not downloaded → not servable). `Some` with `fits_on_gpu = false`
    /// when it is on disk but won't fit a lane in the current budget.
    pub plan: Option<ServingPlan>,
    /// The model's on-disk weight bytes (0 when not downloaded).
    pub weights_bytes: u64,
    /// The host's usable serving budget right now, in bytes.
    pub budget_bytes: u64,
}

/// The cloneable synchronous fit-gate the `serving/pin` command holds — given a
/// candidate [`Model`], can this host force-serve it right now. Built by
/// [`ServingDaemonModule::pin_fit_checker`] over the live budget so it agrees
/// with the autonomic planner by construction.
pub type PinFitChecker = Arc<dyn Fn(&Model) -> PinFit + Send + Sync>;

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
    /// The published decision. `None` until the first successful plan. Held as
    /// the module's only shared state; `send` takes `&self` so `tick()` can
    /// publish without interior-mutability gymnastics.
    plan_tx: watch::Sender<Option<ServingPlan>>,
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
    /// Reconcile-tick counter driving the slow liveness HEARTBEAT (fires when
    /// `% HEALTH_PROBE_EVERY_TICKS == 0`). See [`Self::spawn_health_heartbeat_if_due`].
    health_ticks: Arc<AtomicU64>,
    /// Consecutive failed decode heartbeats — the hysteresis counter that keeps a
    /// merely-BUSY lane from being reaped. Reset to 0 on any passing probe or when the
    /// lane isn't believed-ready. Relaunch only once it reaches [`HEALTH_FAILS_TO_RELAUNCH`].
    health_fails: Arc<AtomicU8>,
    /// At most one heartbeat decode-probe in flight (a probe can take up to
    /// `DECODE_SMOKE_TIMEOUT` under load, longer than one [`TICK`]); a tick that finds one
    /// running skips, exactly like `reconciling`.
    health_probing: Arc<AtomicBool>,
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
    /// The `/slots` activity fingerprint observed at the LAST missed smoke probe (L11).
    /// A changed fingerprint at the next miss proves the serve loop advanced between the
    /// two looks — work the adapter stamps can't see (ghost turns from a dead core's
    /// clients, a fresh boot's un-stamped atomics) — so the miss is queue contention,
    /// not wedge evidence. Cleared on every smoke success so a stale value can never
    /// exonerate a later freeze. `std::sync::Mutex` — held only for a copy, never
    /// across an await.
    last_miss_slots_fp: Arc<std::sync::Mutex<Option<u64>>>,
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
    last_healthy_lanes: Arc<AtomicU32>,
    /// The LIVE model universe — the SAME `Arc<ModelCatalog>` the `models/*`
    /// command surface mutates. The daemon plans off this snapshot, NOT the
    /// immutable seed registry, so a model acquired at runtime (`models/pull`
    /// flips it to [`Availability::Ready`] with its real on-disk path) becomes a
    /// serving candidate on the very next tick — no reboot. This is the consumer
    /// side of the rich API: serving reacts to the universe changing.
    catalog: Arc<ModelCatalog>,
    /// K3 expert-residency state for the CURRENTLY-served MoE model, `(model_id, context)`.
    /// Built lazily on the first reconcile that serves a given MoE model and rebuilt when the
    /// served model changes; `None` for a dense model or before the first MoE reconcile. A
    /// `std::sync::Mutex` because [`Self::reconcile_to_plan`] is synchronous and this is never
    /// held across an await. The pager inside owns the live-hit observer + the gate seed; each
    /// reconcile ticks it to decide the served expert-layer placement.
    moe_serving:
        std::sync::Mutex<Option<(String, crate::capacity::moe_serving::MoeServingContext)>>,
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
    active_artifact: std::sync::Mutex<Option<std::path::PathBuf>>,
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
    division: std::sync::Mutex<Option<crate::capacity::division_actuation::DivisionActuator>>,
    /// The resident-override path the LAST reconcile applied to the spawn — ground truth
    /// for which tier the RUNNING serve actually loaded. Division rewards credit this
    /// tier, never the bandit's latest (unlaunched) choice — two-speed honesty.
    served_resident: std::sync::Mutex<Option<std::path::PathBuf>>,
    /// MEASUREMENT-ONLY, off by default: an explicit forced VRAM budget for K3 expert
    /// placement, read ONCE at construction from `K3_MEASURE_FORCE_EXPERT_BUDGET_BYTES`. When
    /// `Some`, it OVERRIDES the governed ceiling so a model that would otherwise fit is driven
    /// to spill expert layers onto CPU — the only way to measure the B-gate (does cold-expert
    /// CPU-compute dominate decode?) without a model that genuinely overflows. It is an
    /// experimental control, NOT a substrate threshold: `None` in all normal operation, and
    /// every placement made under it is flagged on the `serving.k3_placement` probe so the
    /// numbers are never mistaken for real capacity. [[k3-slice2-A-vs-B-decision]]
    measure_force_expert_budget_bytes: Option<u64>,
    /// Model ids the operator has explicitly UNLOADED — the VRAM-axis "free".
    /// The daemon is holistically in charge of VRAM, so freeing a lane is a
    /// runtime act, never a restart: `serving/unload` inserts an id here, the
    /// next plan recompute excludes it from candidates, and the reconcile drops
    /// it (relaunch to the next-best fit, or empty) — VRAM freed live.
    /// `serving/load` removes it, permitting the planner to serve it again when
    /// it fits the budget. COW `Arc<HashSet>` on a watch so the command writes
    /// and the plan reads the same authority lock-free; the planner still owns
    /// the decision (this only ever EXCLUDES, never forces).
    suppressed: watch::Sender<Arc<HashSet<String>>>,
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
    /// Per-persona MEASURED turn demand — the window half of `serving_demand()`.
    /// Personas write (their deliberation faculty records every turn's unclamped
    /// cost); this daemon reads the ceiling when it plans. Replaces the
    /// `BOOTSTRAP_WORKING_SET` constant that used to stand in for this measurement
    /// and capped every citizen at 8192 tokens of a 128k-capable model.
    working_set: crate::cognition::working_set::WorkingSetRegistry,
    /// The operator/persona's explicit FORCE-serve pin — the "hard pin" the
    /// `serving/load` doc names as the future verb, the mechanism behind
    /// promote/demote (`serving/pin` ↔ `serving/unpin`). `None` = autonomic
    /// best-fit (the planner picks the most-capable model that fits). `Some(id)`
    /// = the planner's candidate set is INTERSECTED to just this model, so the
    /// reconcile serves exactly it (or nothing, honestly, if it no longer fits).
    /// The dual of `suppressed`: suppress SUBTRACTS from candidates, pin
    /// INTERSECTS to one. Same lock-free `watch` seam; the daemon still owns the
    /// reconcile. The fit-gate lives in `serving/pin` (it refuses loud BEFORE
    /// pinning when the model won't fit a lane), so a set pin is always a model
    /// that fit at pin time; budget can still shift under it, and then the plan
    /// degrades honestly (`fits_on_gpu = false`) rather than over-committing.
    pinned: watch::Sender<Option<String>>,
    /// The long-lived vision SIDECAR lane (#106, `inference::vision_sidecar`):
    /// a small VL model serving beside a text-only mind so every persona has
    /// eyes. Owned here so it lives across reconciles and its child dies with
    /// the daemon (`EphemeralServingLane` Drop-kills). A tokio Mutex because
    /// [`vision_sidecar::ensure_sidecar`] awaits (spawn + `/props` verify)
    /// inside the reconcile task while holding the slot.
    vision_sidecar:
        Arc<tokio::sync::Mutex<Option<crate::inference::llama_server::EphemeralServingLane>>>,
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
enum ModelChangeGate {
    /// Not a ready-lane model change (same model, or no ready incumbent) — proceed.
    NotAChange,
    /// A change, but not yet sustained — do nothing this tick.
    Defer { streak: u32 },
    /// Sustained disagreement — commit the swap (streak consumed).
    Commit { streak: u32 },
}

fn model_change_gate(
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

impl ServingDaemonModule {
    pub fn new(
        gpu: Arc<GpuMemoryManager>,
        system: Arc<SystemResourceMonitor>,
        resource_daemon: Arc<ResourceDaemon>,
        catalog: Arc<ModelCatalog>,
    ) -> Self {
        Self::with_control(
            gpu,
            system,
            resource_daemon,
            Arc::new(LlamaServerProcess::new()),
            catalog,
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
    ) -> Self {
        let (plan_tx, _rx) = watch::channel(None);
        let (serving_tx, _srx) = watch::channel(ServingSnapshot::empty());
        let (suppressed, _urx) = watch::channel(Arc::new(HashSet::new()));
        let (pinned, _prx) = watch::channel(None);
        Self {
            gpu,
            system,
            resource_daemon,
            plan_tx,
            server,
            serving_tx,
            reconciling: Arc::new(AtomicBool::new(false)),
            health_ticks: Arc::new(AtomicU64::new(0)),
            health_fails: Arc::new(AtomicU8::new(0)),
            health_probing: Arc::new(AtomicBool::new(false)),
            force_relaunch: Arc::new(AtomicBool::new(false)),
            bus: OnceLock::new(),
            // Production resolver: the global registry. `try_global` (not the
            // panicking `global`) so a not-yet-initialized registry resolves to
            // None → honest empty snapshot, never a panic in the reconcile path.
            model_resolver: Arc::new(|id: &str| {
                crate::model_registry::try_global().and_then(|r| r.model(id).cloned())
            }),
            decode_age: Arc::new(crate::inference::llama_server::ms_since_real_work),
            last_miss_slots_fp: Arc::new(std::sync::Mutex::new(None)),
            inherited_lane: Arc::new(crate::inference::lane_registry::live_lane),
            real_fails: Arc::new(crate::inference::llama_server::consecutive_real_decode_failures),
            last_healthy_window: Arc::new(AtomicU32::new(0)),
            last_healthy_lanes: Arc::new(AtomicU32::new(0)),
            catalog,
            suppressed,
            pinned,
            lane_demand: Arc::new(std::sync::atomic::AtomicU32::new(1)),
            rehome_streak: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            model_change_streak: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            pending_model_change: Arc::new(std::sync::Mutex::new(None)),
            rehome_cooldown: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            downshift_streak: Arc::new(std::sync::atomic::AtomicU32::new(0)),
            last_plan_probe: Arc::new(std::sync::Mutex::new(None)),
            working_set: crate::cognition::working_set::global(),
            moe_serving: std::sync::Mutex::new(None),
            active_artifact: std::sync::Mutex::new(None),
            moe_trace_tail: std::sync::Mutex::new(None),
            division: std::sync::Mutex::new(None),
            served_resident: std::sync::Mutex::new(None),
            host_cache_lease: std::sync::Mutex::new(
                crate::capacity::host_cache_lease::StickyLease::new(HOST_CACHE_LEASE_BAND_DIVISOR),
            ),
            device_budget_lease: std::sync::Mutex::new(
                crate::capacity::host_cache_lease::StickyLease::new(HOST_CACHE_LEASE_BAND_DIVISOR),
            ),
            // Read ONCE here (single config entry point, no per-tick I/O). Off by default.
            measure_force_expert_budget_bytes: crate::config_env::read(
                "K3_MEASURE_FORCE_EXPERT_BUDGET_BYTES",
            )
            .and_then(|s| s.trim().parse::<u64>().ok()),
            vision_sidecar: Arc::new(tokio::sync::Mutex::new(None)),
        }
    }

    /// Set the lane DEMAND (how many minds need a concurrent lane — the
    /// persona floor). The boot wiring calls this before the first
    /// [`Self::compute_plan`]; the next tick replans if it changes.
    pub fn set_lane_demand(&self, demand: u32) {
        // Register the demand state process-globally the first time boot sets it, so a
        // measurement preemption lease (quiesce_all / quiesce_others) can drop the
        // warm-slot demand to the ACTIVE (non-quiesced) count for its duration and
        // release on drop — without threading a ServingDaemon handle into the persona
        // registry. Idempotent: `set` after the first call is a no-op.
        let state = LANE_DEMAND.get_or_init(|| LaneDemandState {
            cell: self.lane_demand.clone(),
            base: std::sync::Mutex::new(demand.max(1)),
            overrides: std::sync::Mutex::new(Vec::new()),
            next_id: std::sync::atomic::AtomicU64::new(1),
        });
        *state.base.lock().expect("lane-demand base lock poisoned") = demand.max(1);
        state.recompute();
    }
}

/// The lane-demand authority: one BASE (the boot-wired persona floor) plus a registry of
/// measurement OVERRIDES, with the effective value recomputed on every change — never a
/// swap/restore pair. The naive swap/restore version had a real interleaving bug: with two
/// overlapping leases (eval's `quiesce_all` + a solve's `quiesce_others`), the first drop
/// restored the pre-lease value OVER the still-held second lease, and the second drop then
/// restored the FIRST lease's override — leaving the whole fleet's demand stuck at 1 with
/// nobody quiesced until the (boot-only) `set_lane_demand` ever ran again. Order-independent
/// recompute makes overlap correct by construction: effective = max(overrides) while any are
/// held (never starve a concurrent measurement below what it asked for), else base.
struct LaneDemandState {
    /// The live cell the planner reads (`ServingDaemonModule::lane_demand`).
    cell: Arc<std::sync::atomic::AtomicU32>,
    /// The boot-wired persona floor — restored whenever the last override releases.
    base: std::sync::Mutex<u32>,
    /// Active measurement overrides as `(lease_id, active_minds)`.
    overrides: std::sync::Mutex<Vec<(u64, u32)>>,
    next_id: std::sync::atomic::AtomicU64,
}

impl LaneDemandState {
    /// Recompute the effective demand from base + overrides and publish it to the cell.
    fn recompute(&self) {
        let overrides = self
            .overrides
            .lock()
            .expect("lane-demand overrides lock poisoned");
        let effective = overrides
            .iter()
            .map(|(_, active)| *active)
            .max()
            .unwrap_or_else(|| *self.base.lock().expect("lane-demand base lock poisoned"));
        drop(overrides);
        self.cell.store(effective.max(1), Ordering::Relaxed);
    }

    /// Add an override and return its lease id.
    fn acquire(&self, active: u32) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        self.overrides
            .lock()
            .expect("lane-demand overrides lock poisoned")
            .push((id, active.max(1)));
        self.recompute();
        id
    }

    /// Remove the override with `id` (unknown id = no-op) and recompute.
    fn release(&self, id: u64) {
        self.overrides
            .lock()
            .expect("lane-demand overrides lock poisoned")
            .retain(|(oid, _)| *oid != id);
        self.recompute();
    }
}

/// Registered by the first [`ServingDaemonModule::set_lane_demand`] at boot. `None` before
/// boot (unit tests / tools that never stood a daemon) → every override is a no-op, so the
/// quiesce lease stays pure and daemon-free-testable.
/// [[measured-work-gets-an-exclusive-warm-slot-quiesce-others]]
static LANE_DEMAND: std::sync::OnceLock<LaneDemandState> = std::sync::OnceLock::new();

/// Add a measurement override: the fleet's warm-slot demand becomes `active` (minds that
/// need a warm slot now; floored at 1 — a measurement still needs one lane) until released.
/// Returns the lease id to pass to [`release_lane_demand`]; `None` before the daemon booted.
pub fn quiesce_lane_demand(active: u32) -> Option<u64> {
    LANE_DEMAND.get().map(|state| state.acquire(active))
}

/// Release the override acquired by [`quiesce_lane_demand`]. Idempotent (an unknown id is
/// a no-op) and order-independent — the effective demand is recomputed from what remains.
pub fn release_lane_demand(id: u64) {
    if let Some(state) = LANE_DEMAND.get() {
        state.release(id);
    }
}

impl ServingDaemonModule {
    /// The current lane demand (≥ 1).
    /// Register serving's autonomic PLANNER to run on the memory authority's tick
    /// (MEMORY-AUTHORITY-DAEMON slice 1b). The lane plan — which model, how many lanes,
    /// what per-slot window — is a MEMORY decision, so it must be computed in the ONE
    /// place that owns memory (the `ResourceDaemon` tick), NOT on serving's own tick
    /// sampling `host_budget()`. `recompute()` (compute the plan from the live host +
    /// publish it to `plan_tx`) now runs as an `on_tick` observer; serving's own tick
    /// keeps only `reconcile_to_plan()` (bring llama-server in line with the published
    /// plan — reacting to the authority's decision, not deciding). Weak handle so the
    /// observer never keeps the module alive past shutdown. Called once at wiring time,
    /// after the module is Arc-wrapped.
    /// Tell the memory authority who we are and how big the device is — **before** the planner
    /// is allowed to run on its tick.
    ///
    /// # Why this is wiring-time and not `initialize` (#438, measured 2026-08-19)
    ///
    /// `governed_vram_ceiling` budgets serving through
    /// [`budget_for_replacing`](crate::resources::ResourceDaemon::budget_for_replacing) —
    /// available PLUS serving's own resident bytes, because a consumer choosing its successor
    /// releases what it holds. That add-back is what makes a past form of ourself count as OURS
    /// rather than as an external squeeze. It is keyed on serving being a *registered consumer*:
    /// with no registration there is no footprint to add back, and the credit silently becomes 0.
    ///
    /// These two calls used to live in `initialize`, which the runtime invokes AFTER the module
    /// is registered — while [`Self::register_planner_on_authority_tick`] attaches the planner at
    /// wiring time. The authority could therefore tick a plan in the gap, and did: a successor
    /// core booting on top of its predecessor's resident 27B read `usable_gb = 0` on a 53 GiB
    /// board for ~0.6 s, because its own 44 GB were filed under nobody. The downshift debounce
    /// held the plan and nothing was actuated — but a guard covering for a budget that is
    /// momentarily lying is not the same as the budget being honest
    /// ([[hand-rolled-ops-are-waste-that-masks-the-real-defect]]).
    ///
    /// Ordering, not duplication: `seed_device_vram_prior` is a set-once prior and
    /// `add_consumer` clears any stale quarantine for the id, so calling this early is safe.
    /// The device's total VRAM is a static hardware fact available immediately — there is no
    /// reason to learn it later than the first tick that plans against it.
    pub fn declare_to_memory_authority(&self) {
        seed_device_vram_prior(self.gpu.total_vram_bytes());
        self.register_as_consumer();
        // Same ordering law as the two lines above, for the OTHER half of the plan's inputs.
        // Those two make our own footprint visible to the authority before a plan may tick;
        // this makes the host's earned DEMAND visible in the same window. Without it the
        // first plans run with `ceiling() == None` and serve BOOTSTRAP_WORKING_SET — measured
        // 2026-08-20 as a 27B held at 16,384 while a 31,834-token peak sat on disk.
        self.working_set.rehydrate_all();
    }

    pub fn register_planner_on_authority_tick(self: &Arc<Self>) {
        let weak = Arc::downgrade(self);
        self.resource_daemon
            .on_tick(Arc::new(move |_board: &LeaseBoard| {
                if let Some(module) = weak.upgrade() {
                    module.recompute();
                }
            }));
    }

    fn lane_demand(&self) -> u32 {
        self.lane_demand.load(Ordering::Relaxed).max(1)
    }

    /// Both axes of what this host's minds are asking for — lanes AND the window
    /// they have actually demanded. The window term is MEASURED
    /// ([`crate::cognition::working_set`]); `None` there means no turn has been
    /// assembled yet on this host, and [`ServingDemand::new`] names the cold-start
    /// decision in one place.
    fn serving_demand(&self) -> ServingDemand {
        ServingDemand::new(self.lane_demand(), self.working_set.ceiling())
    }

    /// The registry personas report their turn demand into. Cheap clone — handed to
    /// each spawned mind's deliberation faculty so the measurement reaches the planner.
    pub fn working_set(&self) -> crate::cognition::working_set::WorkingSetRegistry {
        self.working_set.clone()
    }

    /// Test seam: override how planned model ids resolve to [`Model`] structs,
    /// so the reconcile wiring runs without a populated global registry.
    #[cfg(test)]
    fn set_model_resolver(&mut self, resolver: ModelResolver) {
        self.model_resolver = resolver;
    }

    /// Test seam: own the inherited-lane evidence instead of reading the operator's real
    /// pidfile + registry. See [`InheritedLaneSource`] for why this MUST be injected — the
    /// production default reads a filesystem singleton, and the identical leak was already
    /// caught by measurement in `serving_consumer`.
    #[cfg(test)]
    fn set_inherited_lane(&mut self, source: InheritedLaneSource) {
        self.inherited_lane = source;
    }

    /// Test seam: own the heartbeat's real-decode evidence instead of inheriting whatever
    /// the process-global happens to hold. See [`DecodeAgeSource`] for why this exists.
    #[cfg(test)]
    fn set_decode_age_source(&mut self, source: DecodeAgeSource) {
        self.decode_age = source;
    }

    /// Test seam: own the heartbeat's real-turn-failure evidence (#363) instead of
    /// inheriting the process-global stamped by other tests' adapters. Same rationale
    /// as [`Self::set_decode_age_source`].
    #[cfg(test)]
    fn set_real_fails_source(&mut self, source: RealFailsSource) {
        self.real_fails = source;
    }

    /// Emit the live snapshot on the bus. Routed by topic name (cheap match, no
    /// body parse in middleware); the payload fans out as a shared pointer. A
    /// no-op when the bus isn't set (tests). The watch is the in-process
    /// materialized view; THIS is the canonical fan-out that reaches every
    /// subscriber, including a grid allocator on a remote node.
    fn emit_serving(bus: Option<&Arc<MessageBus>>, snapshot: &ServingSnapshot) {
        if let Some(bus) = bus {
            if let Ok(payload) = serde_json::to_value(snapshot) {
                bus.publish_async_only(SERVING_SNAPSHOT_EVENT, payload);
            }
        }
    }

    /// Subscribe to the published serving plan. Consumers (scheduler, spawner)
    /// hold the receiver and react to plan changes — the ebb/flow seam.
    pub fn subscribe(&self) -> watch::Receiver<Option<ServingPlan>> {
        self.plan_tx.subscribe()
    }

    /// Subscribe to the published serving SNAPSHOT — the live `(active_model,
    /// ready, base_url)` state. Inference adapters hold this and point at
    /// `base_url` once `ready`; a grid allocator holds it to contract leases.
    pub fn subscribe_serving(&self) -> watch::Receiver<ServingSnapshot> {
        self.serving_tx.subscribe()
    }

    /// Register serving as a MEASURED [`ResourceConsumer`] with the one
    /// per-machine authority (#79) — monitor-not-reserve. Serving does NOT
    /// acquire a lease here; it keeps loading through its own plan/reconcile
    /// loop. It hands the governor exactly the two handles the authority needs:
    ///
    /// - the serving snapshot + a footprint resolver, so the governor's reconcile
    ///   tick background-polls serving's resident VRAM and *attributes* it on the
    ///   board (the fix for the `granted:0 while the GPU is full` blindness), and
    /// - the suppress-set writer, so when a peer needs the bytes the authority can
    ///   ASK serving to free the active model (whole-lease-granular unload), and
    ///   serving answers honestly across the async unload.
    ///
    /// `available = capacity − granted` is untouched — this only surfaces the
    /// measured axis. The footprint resolver is the SAME catalog + footprint
    /// estimator that feeds the serving plan, so there is ONE footprint authority
    /// on the box, not two.
    pub fn register_as_consumer(&self) {
        // The live servable-model candidates, sized to whatever shape serving is running,
        // so the tier-down ranker re-homes only to a model the autonomic plan itself could
        // serve (same suppress/pin/eligibility filter → one catalog, never two). Reads the
        // catalog snapshot + suppress/pin watches at reclaim time; no lock held across the
        // async handshake (it returns an owned Vec).
        let catalog = self.catalog.clone();
        let suppressed_rx = self.suppressed.subscribe();
        let pinned_rx = self.pinned.subscribe();
        let candidates: crate::modules::serving_tier_down::TierCandidatesFn =
            Arc::new(move |window: u32, lanes: u32| {
                let snap = catalog.snapshot();
                let sup = suppressed_rx.borrow();
                let pin = pinned_rx.borrow();
                servable_candidates(&snap, &**sup, &pin)
                    .into_iter()
                    .map(|f| {
                        // Peak (weights + KV + prefill compute reserve), the SAME number
                        // the board attributes to serving — so a shrink target's freed
                        // bytes are measured against serving's true footprint, not a
                        // resident figure that omits the compute buffer (#56/G5).
                        let resident_bytes = f.peak_resident_bytes(window, lanes);
                        crate::modules::serving_tier_down::TierCandidate {
                            model_id: f.model_id,
                            capability_rank: f.capability_rank,
                            resident_bytes,
                        }
                    })
                    .collect()
            });
        let consumer = ServingConsumer::new(
            self.subscribe_serving(),
            self.suppress_sender(),
            self.pin_sender(),
            serving_footprint_fn(self.catalog.clone()),
            // #56: under a VRAM reclaim (a game grabbed the GPU, a peer needs the bytes),
            // shrink to the most-capable smaller model that frees enough — "take our own
            // capacity down to yield, keep answering" — instead of the whole-lease dark.
            // The autonomic plan grows back up when pressure clears. Falls through to a
            // full unload only when no smaller model frees enough.
            Arc::new(crate::modules::serving_tier_down::CatalogTierDownPolicy::new(candidates)),
        );
        self.resource_daemon.add_consumer(Arc::new(consumer));

        // THE VISION HOLDER declares itself (#106/#395/#56). Measured on the live board
        // before this: vram physUsed 50.95G / attributed 6.81G / UNOWNED 44.14G — the
        // governor could see the bytes were gone and not who had them, and unowned reads
        // as immovable, so a model that physically fits was refused. The vision provider
        // was one of the largest unowned blocks (~9.4G for the VL-7B sidecar).
        //
        // Monitor-only, and deliberately so: it declares the bytes but refuses reclaim
        // out loud, because the release path is the serving reconcile
        // (`if main_sees { sidecar = None }`), not an inbound handler. Declaring a
        // reclaim it cannot perform would have the authority plan against a release that
        // silently never happens.
        let vision_source = Arc::new(crate::modules::serving_footprints::CatalogFootprintSource::vision(
            self.catalog.clone(),
            self.subscribe_serving(),
        ));
        crate::probe!(
            class = "resources.footprint.wired",
            holder = "vision",
            source = "CatalogFootprintSource::vision",
            "vision footprint source registered with the authority",
        );
        self.resource_daemon
            .add_consumer(Arc::new(crate::modules::serving_footprints::MonitoredHolder::new(
                vision_source,
                "vision provider (sidecar or main lane)",
                "no on-demand release: vision residency is dropped by the serving \
                 reconcile when the main lane can see (#106/#395)",
            )));
    }

    /// Honest serving budget for this host, RIGHT NOW — from the live free
    /// memory the monitor reports, capped at the device's physical VRAM. This
    /// is the organic signal: free memory drops under load, the budget shrinks,
    /// the plan picks fewer lanes / a smaller model; load clears, it flows back.
    /// Delegates to the free [`live_host_budget`] so the autonomic tick and the
    /// `serving/pin` fit-gate compute the budget from the ONE source.
    fn host_budget(&self) -> HostBudget {
        // The autonomic plan's VRAM budget = the LIVE governed available (net of every
        // external consumer + the 512MB driver reserve the GpuCapacitySource already holds)
        // × the pressure-adaptive drive mode. ONE reserve, the DYNAMIC one: Performance
        // (hard task + room) floors the whole GPU (1.0), Comfort is the everyday 0.80, and
        // Eco (memory starved — a game opened, a crowded call) drops to 0.55 so the base
        // claims less and the rest of the call's KV + render still fit. #56/G8: we do NOT
        // ALSO apply the operator VRAM-headroom fraction here — stacking it under the mode
        // fraction double-discounted the everyday budget to ~0.64 (24.5GB of a 38GB board)
        // and left the more-capable 32B coder + full context unused, WITHOUT buying safety
        // (the concurrent-prefill compute buffer is reserved separately, window-scaled, in
        // the serving_plan fixpoint). The pin fit-gate still applies the operator headroom
        // (`config_env::vram_headroom()`, default 0.80) via `host_budget_from` — "can this
        // model physically fit" is a different question than "how much should the shared base
        // claim now." [[verify-real-device-numbers-not-a-clamp-premise]]
        // The governed VRAM board is the ONE authority for available VRAM: its `available`
        // is free-VRAM ALREADY netted over every measured consumer + external pressure AND
        // already ≤ physical VRAM. Trust it directly — do NOT `.min()` it against the raw
        // system-RAM "available" figure. On UMA (Apple Silicon) macOS's vm_stat under-reports
        // available RAM (wired/cached/compressed counted as used) FAR below the VRAM the GPU
        // can actually use, so the old `available.min(vram_ceiling)` starved a 42.7GB governed
        // budget down to macOS's 18GB reading — LESS than Devstral's own 14GB weights — and
        // floored a 128k-capable model to MIN_SERVE_CTX (2048) with ~28GB sitting free
        // (glass-boxed 2026-07-20). The raw-probe-overriding-the-board clamp is exactly the
        // anti-pattern the memory-authority arc exists to kill. Pressure sensing stays live
        // via the drive mode below (which still reads system available for the fraction).
        let available = self.system.snapshot().memory.available_bytes;
        let live = governed_vram_ceiling_or_report(&self.resource_daemon, "host_budget");
        // LUDICROUS override: a declared benchmark/exam intent floors the whole GPU
        // (Performance, fraction 1.0) — the biggest window the model+machine allow, past the
        // conservative pressure read (which on UMA under-reports free memory). The drive mode
        // follows the ACTIVITY, not just the pressure. Otherwise the live pressure-adaptive
        // mode (a game opening still drops us to Eco). [[serving-mode-follows-activity-ludicrous-to-dream]]
        let mode = if serving_ludicrous_active() {
            crate::provisioning::model_catalog::PowerMode::Performance
        } else {
            crate::provisioning::serving_mode_for_pressure(available)
        };
        // Observability: emit ONLY on a mode TRANSITION so the dynamic scaling is visible
        // without spamming the hot plan tick ([[never-blind-feedback-driven-iteration]]).
        // This is the seam a learned / LLM policy will report through — watch it kick down
        // under load, and later watch a smarter policy make a better call.
        {
            use std::sync::atomic::{AtomicU8, Ordering};
            static LAST_MODE: AtomicU8 = AtomicU8::new(u8::MAX);
            let m = mode as u8;
            if LAST_MODE.swap(m, Ordering::Relaxed) != m {
                eprintln!(
                    "🎛 serving mode → {:?} ({} GiB free)",
                    mode,
                    available / (1 << 30)
                );
            }
        }
        HostBudget {
            usable_bytes: (live as f64 * mode.serving_fraction()) as u64,
            perf_cores: perf_cores(),
        }
    }

    /// The servable candidates RIGHT NOW: the on-disk Ready models, MINUS any an
    /// operator has explicitly unloaded (`serving/unload`), and — when a force-pin
    /// is set (`serving/pin`) — INTERSECTED to just the pinned model. The single
    /// chokepoint every plan flows through, so a suppressed model can never be
    /// planned and a pin can never be escaped: pin set → the planner has exactly
    /// one candidate, so the reconcile serves that model or (if it has dropped off
    /// disk) nothing. Suppress subtracts; pin intersects; the planner still owns
    /// the choice among whatever remains.
    fn live_candidates(&self) -> Vec<ModelFootprint> {
        let suppressed = self.suppressed.borrow();
        let pinned = self.pinned.borrow();
        servable_candidates(&self.catalog.snapshot(), &**suppressed, &pinned)
    }

    /// A clone of the suppress-set writer, for the `serving/unload` ·
    /// `serving/load` commands to mutate the VRAM-axis allocation ledger. The
    /// daemon stays the authority: the commands only edit the exclude-set; the
    /// plan + reconcile (owned here) turn that into an actual load/unload.
    pub fn suppress_sender(&self) -> watch::Sender<Arc<HashSet<String>>> {
        self.suppressed.clone()
    }

    /// A clone of the force-pin writer, for the `serving/pin` · `serving/unpin`
    /// commands (the promote/demote mechanism). The daemon stays the authority:
    /// the command sets/clears one model id; `live_candidates` intersects to it
    /// and the plan + reconcile (owned here) turn that into the actual swap. Dual
    /// of [`Self::suppress_sender`].
    pub fn pin_sender(&self) -> watch::Sender<Option<String>> {
        self.pinned.clone()
    }

    /// The synchronous fit-gate `serving/pin` holds: given a candidate model,
    /// does it fit a serving lane on THIS host right now? Built from the same
    /// [`live_host_budget`] + [`footprint_for`] + [`plan_serving`] the autonomic
    /// tick uses, so the pin's "will it fit" verdict can never disagree with what
    /// the next reconcile would actually do. The command refuses loud BEFORE
    /// pinning when this says no — never a silent best-fit fallback.
    pub fn pin_fit_checker(&self) -> PinFitChecker {
        let system = self.system.clone();
        let resource_daemon = self.resource_daemon.clone();
        // Read the incumbent so a pin's fit-check models the SWAP it performs, not
        // co-residence (below).
        let serving_tx = self.serving_tx.clone();
        let model_resolver = self.model_resolver.clone();
        Arc::new(move |model: &Model| {
            let base = live_host_budget(&system, &resource_daemon);
            // The incumbent a pin would EVICT — its footprint credits back into the
            // budget (see [`pin_fit_decision`]). `live_host_budget` reads the live
            // system, so the eviction-crediting fit logic is split into a pure,
            // unit-testable helper below.
            let incumbent = serving_tx
                .borrow()
                .active_model
                .clone()
                .and_then(|id| (model_resolver)(&id))
                .and_then(|m| footprint_for(&m));
            pin_fit_decision(base, footprint_for(model), incumbent.as_ref())
        })
    }

    /// Compute the current serving plan from the live host snapshot + on-disk
    /// models, WITHOUT relying on a tick having run. The boot path calls this
    /// to drive the spawner before the tick loop starts — single source of
    /// truth for "what model + how many lanes."
    pub fn compute_plan(&self) -> Option<ServingPlan> {
        plan_serving(
            self.host_budget(),
            &self.live_candidates(),
            self.serving_demand(),
        )
    }

    /// The detected hardware tier for this host, for the persona spawner's
    /// `n_gpu_layers` + roster shape. Same GPU detection that feeds the serving
    /// plan — one hardware authority, not two.
    pub fn detected_tier(&self) -> (HwCapabilityTier, HwTierCategory, &'static str) {
        detect_tier(self.gpu.gpu_name())
    }

    /// Recompute the plan from the live host snapshot + on-disk models, publish
    /// it, and log the decision. Idempotent — safe to call on init and tick.
    fn recompute(&self) {
        // Don't re-plan while a relaunch of the serving lane is IN FLIGHT (#216). During the
        // kill→respawn, the GPU scan (`physical_used`) transiently drops as the old lane exits
        // and rises as the new one loads, so `host_budget()` — which reads the board's
        // `available = capacity − max(granted, physical_used)` — sees a PHANTOM free-VRAM
        // spike/dip. A plan recomputed off that transient flaps the window and triggers ANOTHER
        // relaunch: the thrash loop (glass-boxed 2026-07-20 — board available swung 12.9↔41GB,
        // window 44800↔7680, back-to-back relaunches). Serving's own in-flight churn must not
        // feed back into its own plan. Hold the last plan until the reconcile settles; the gate
        // clears via RAII (#214) the instant the relaunch finishes OR fails, so re-planning
        // resumes promptly against the STABLE post-relaunch budget — no thrash, no stall. (The
        // deeper fix — serving holding an explicit board lease so `granted` pins its residency
        // and the scan transient never reaches `available` at all — is the #56 consumers-LEASE
        // residual; this breaks the feedback loop cleanly in the meantime.)
        if self.reconciling.load(Ordering::Acquire) {
            return;
        }
        let budget = self.host_budget();
        self.publish_plan(budget, &self.live_candidates());
    }

    /// Bring the running `llama-server` in line with the published plan. FAST —
    /// it only decides whether a reconcile is needed and, if so, spawns a
    /// detached task to do the (possibly multi-second) relaunch + readiness
    /// wait. The tick must never block on model load, so the slow part runs off
    /// the tick. Returns the spawned `JoinHandle` (for tests to await
    /// deterministically) or `None` when no reconcile was started.
    ///
    /// K3 expert-layer placement for the model this reconcile will serve.
    ///
    /// For a MoE model it (re)builds the per-model [`MoeServingContext`](crate::capacity::moe_serving::MoeServingContext)
    /// on a model change (rare — a swap already relaunches), ticks its pager against the
    /// governed VRAM budget to decide which expert LAYERS fit, and returns the DEBOUNCED
    /// placement: `committed_placement` only changes when the hot-layer churn passes
    /// [`RELAUNCH_LAYER_CHURN_THRESHOLD`], so the serving target stays byte-stable across ticks
    /// that don't warrant a respawn (and the launcher's target-diff doesn't fire spuriously).
    ///
    /// `None` — served with no `-ot` override, exactly as before — for a dense model, an
    /// unresolved GGUF, or a zero budget. Pure of I/O except the one GGUF read on a model
    /// change; the sync `Mutex` is never held across an await (this whole method is sync).
    fn compute_expert_placement(&self, model: &Model) -> Option<PlacementRequest> {
        // MEASUREMENT knob (off by default): a forced budget OVERRIDES the governed ceiling so
        // a fits-in-VRAM model spills expert layers to CPU, making the B-gate measurable. Every
        // placement made under it is flagged on the probe below — never mistaken for capacity.
        let forced = self.measure_force_expert_budget_bytes;
        let budget = forced.unwrap_or_else(|| {
            governed_vram_ceiling_or_report(&self.resource_daemon, "compute_expert_placement")
        });
        if budget == 0 {
            return None;
        }
        let mut guard = self.moe_serving.lock().ok()?;
        let stale = guard.as_ref().map(|(id, _)| id.as_str()) != Some(model.id.as_str());
        if stale {
            let ctx =
                crate::model_registry::artifacts::resolve_gguf_for_model(model).and_then(|gguf| {
                    crate::capacity::moe_serving::moe_serving_context(
                        &gguf,
                        &model.id,
                        EXPERT_PLACEMENT_MARGIN_BYTES,
                        RELAUNCH_LAYER_CHURN_THRESHOLD,
                    )
                });
            *guard = ctx.map(|c| (model.id.clone(), c));
        }
        let (_, ctx) = guard.as_mut()?;
        let outcome = ctx
            .pager
            .tick_layer_placement(budget, ctx.n_experts_per_layer, ctx.n_layers);
        // Glass-box the K3 residency decision (Joel: the K3 path is observability-first).
        // Every load-bearing quantity a breakpoint would want: what fit, out of how many, on
        // what budget, and whether it moves the served process this tick.
        crate::probe!(
            class = "serving.k3_placement",
            model = model.id.as_str(),
            budget_bytes = budget,
            // TRUE when the budget is the measurement override, not the governed ceiling — so a
            // reader knows the hot/cold split is under an artificial constraint (the B-gate sweep).
            measurement_forced = forced.is_some(),
            n_layers = ctx.n_layers,
            n_experts_per_layer = ctx.n_experts_per_layer,
            hot_layers = outcome.request.hot_layers.len(),
            needs_relaunch = outcome.needs_relaunch,
            "K3 expert-layer placement: fit {} of {} layers hot on {} GiB budget{}",
            outcome.request.hot_layers.len(),
            ctx.n_layers,
            budget / (1024 * 1024 * 1024),
            if forced.is_some() {
                " [MEASUREMENT-FORCED]"
            } else {
                ""
            },
        );
        if outcome.needs_relaunch {
            ctx.pager.mark_layer_relaunched(&outcome.request.hot_layers);
            ctx.committed_placement = Some(outcome.request);
        }
        ctx.committed_placement.clone()
    }

    /// Device-fit resident-override (#29): does this model's RESIDENT (non-expert)
    /// tier fit the governed VRAM budget as-shipped, or must the launcher source a
    /// precision-shrunk resident from a device-fit override GGUF? The resident-FIT
    /// decision turns ONLY on `resident_bytes` vs the budget minus a fixed compute
    /// reserve — per-layer KV (M5's #2107 `ModelCapabilities`) drives the CONTEXT +
    /// expert-VRAM split, NOT this, so KV is deliberately not consulted here. `None`
    /// = resident fits as-shipped (dense / small MoE, served normally) OR no fitting
    /// override is cached yet (a >VRAM-resident MoE like K3 until the resolver #35
    /// discovers/generates one — then the launch OOMs LOUD rather than silently
    /// mis-serving, [[no-masking-fallbacks-my-style-tell]]).
    fn compute_resident_override(&self, model: &Model) -> Option<std::path::PathBuf> {
        let budget = governed_vram_ceiling(&self.resource_daemon)?;
        if budget == 0 {
            return None;
        }
        // Expert-tensor total — present only for a MoE serve (a dense model's
        // resident always places on GPU normally, no device-fit split). Read under
        // the same never-across-await sync Mutex as the placement path.
        let expert_bytes_total = {
            let guard = self.moe_serving.lock().ok()?;
            match guard.as_ref() {
                Some((id, ctx)) if id.as_str() == model.id.as_str() => ctx.expert_bytes_total,
                _ => return None,
            }
        };
        let fp = footprint_for(model)?;
        let resident_bytes = fp.weights_bytes.saturating_sub(expert_bytes_total);
        let model_id = model.id.clone();
        let inputs = crate::capacity::device_fit::DeviceFitInputs {
            resident_bytes,
            vram_budget_bytes: budget,
            kv_bytes_per_token: 0, // not consulted for the resident-fit decision — see doc
            compute_reserve_bytes: crate::capacity::device_fit::default_compute_reserve_bytes(
                budget,
            ),
            desired_context: model.context_window,
            model_max_context: model.context_window,
            lanes: 1,
        };
        let plan = crate::capacity::device_fit::plan_device_fit(&inputs, |usable| {
            crate::model_registry::artifacts::resolve_device_fit_override(&model_id, usable)
        });
        crate::probe!(
            class = "serving.device_fit",
            model = model.id.as_str(),
            resident_bytes,
            budget_bytes = budget,
            gpu_servable = plan.is_gpu_servable(),
            has_override = plan.resident.override_path().is_some(),
            "device-fit resident tier: {}",
            match &plan.resident {
                crate::capacity::device_fit::ResidentFit::Native =>
                    "fits as-shipped (native, all resident on GPU)".to_string(),
                crate::capacity::device_fit::ResidentFit::Override(o) => format!(
                    "device-fit override {} GiB resident from {}",
                    o.resident_bytes / (1024 * 1024 * 1024),
                    o.path.display()
                ),
                crate::capacity::device_fit::ResidentFit::Unfittable {
                    resident_bytes,
                    usable_bytes,
                } => format!(
                    "UNFITTABLE: resident {} GiB > {} GiB usable — route to grid or generate an override (#35)",
                    resident_bytes / (1024 * 1024 * 1024),
                    usable_bytes / (1024 * 1024 * 1024)
                ),
            },
        );
        plan.resident.override_path().cloned()
    }

    /// #287 slice 2 — the governed host-cache lease, derived LIVE on the tick and
    /// published to the per-port plan file her `ResidencyCache` mtime-polls. This is
    /// the loop-closer that retires the `GGML_MOE_HOST_CACHE_GB=40` scratchpad
    /// constant: the budget is arithmetic over the serve's REAL working set
    /// ([`host_cache_lease_bytes`](crate::capacity::host_cache_lease::host_cache_lease_bytes)),
    /// re-derived every tick because KV grows as slots fill, and published through
    /// the sticky band so sub-band flutter never churns the file. Publishes only for
    /// a READY MoE serve (the moe context exists for the active model) — a dense
    /// model or a warming lane writes nothing.
    /// Track the resident model's on-disk artifact in the process-wide
    /// active set (#302 invariant 1). Registered BEFORE spawn (the pool
    /// must not migrate a GGUF mid-load), swapped on model change (old
    /// registration released exactly once), cleared when nothing serves.
    /// Prefix containment in [`ActiveArtifactSet::protects`] means the
    /// file path protects its per-model dir and vice versa, whichever
    /// layout the artifact uses.
    fn set_active_artifact(&self, path: Option<std::path::PathBuf>) {
        let Ok(mut current) = self.active_artifact.lock() else {
            return; // poisoned: the set fails SAFE (protects everything)
        };
        if *current == path {
            return;
        }
        let set = crate::system_resources::serving_active_artifacts();
        if let Some(old) = current.take() {
            set.release(&old);
        }
        if let Some(new) = path {
            set.register(new.clone());
            *current = Some(new);
        }
    }

    fn publish_moe_host_cache_lease(&self) {
        let live = self.serving_tx.borrow().clone();
        if !live.ready || live.served_context_window == 0 {
            return;
        }
        let Some(active) = live.active_model.clone() else {
            return;
        };
        let Some(port) = crate::inference::llama_server::port_of_base_url(&live.base_url) else {
            return;
        };
        // Expert geometry for the ACTIVE model — present only for a MoE serve.
        // Read under the same never-across-await sync Mutex as the placement path.
        // top_k + experts-per-layer feed the retention verdict below: the lease
        // is only USEFUL when it exceeds one token's expert working set.
        let (expert_bytes_total, n_experts_per_layer, top_k, n_layers) = {
            let Ok(guard) = self.moe_serving.lock() else {
                return;
            };
            match guard.as_ref() {
                Some((id, ctx)) if *id == active => (
                    ctx.expert_bytes_total,
                    ctx.n_experts_per_layer,
                    ctx.top_k,
                    ctx.n_layers,
                ),
                _ => return, // dense model (or stale context) — no governed cache to lease
            }
        };
        let Some(model) = (self.model_resolver)(&active) else {
            return;
        };
        let Some(fp) = footprint_for(&model) else {
            return;
        };
        let physical = self.system.memory().total_bytes;
        if physical == 0 {
            return;
        }
        let Some(inputs) = moe_host_cache_lease_inputs(
            &active,
            fp.weights_bytes,
            expert_bytes_total,
            model.context_window,
            live.served_context_window,
            live.lanes,
            physical,
            system_commit_charge_bytes(),
        ) else {
            return;
        };
        let derived = crate::capacity::host_cache_lease::host_cache_lease_bytes(&inputs);
        // Sticky band on the BUDGET axis only: `budget_moved` says the
        // published value changed; a held budget reuses the last published
        // value so the ACTUATOR axis (pins, below) can still trigger a
        // write. The no-churn property now lives on the write decision —
        // nothing changed on either axis ⇒ no write, no mtime churn.
        let (budget_bytes, budget_moved) = {
            let Ok(mut sticky) = self.host_cache_lease.lock() else {
                return;
            };
            match sticky.observe(derived) {
                Some(published) => (published, true),
                None => (sticky.published_bytes(), false),
            }
        };
        if budget_bytes == 0 {
            return; // nothing governed yet — never publish a zero lease
        }
        // DEVICE axis (#305, BigMama's measured ask): the copy-stream's
        // expert-slot budget = live free-VRAM-after-fit less the placement
        // margin. The board's `available(Vram)` is already net of the
        // resident serve (#79 registers it as a consumer), so this is
        // exactly "size to the free VRAM AFTER device-fit, not a fixed
        // number". Sticky like the host axis; a zero publishes as ABSENT
        // (the mechanism keeps its own sizing), never as a zero budget.
        let device_derived = governed_vram_ceiling(&self.resource_daemon)
            .unwrap_or(0)
            .saturating_sub(EXPERT_PLACEMENT_MARGIN_BYTES);
        let (device_budget_bytes, device_moved) = {
            let Ok(mut sticky) = self.device_budget_lease.lock() else {
                return;
            };
            match sticky.observe(device_derived) {
                Some(published) => (published, true),
                None => (sticky.published_bytes(), false),
            }
        };
        let Some(gb) = crate::inference::llama_server::moe_glass_box_paths(port) else {
            return;
        };
        // Pin actuator (#281): drain the fork's routed-expert trace into
        // the bandit and carry its hot list on the governed plan. The pin
        // budget is retention-derived — at most half the lease's worth of
        // experts (the recency window keeps the other half) — so a lease
        // too small to retain anything publishes budget-only (v1 shape).
        let (
            pins,
            boundary_crossed,
            tokens_observed,
            pins_moved,
            repeat_recall,
            delta_recall,
            coverage,
        ) = {
            let Ok(mut tail_guard) = self.moe_trace_tail.lock() else {
                return;
            };
            let tail = tail_guard
                .get_or_insert_with(|| crate::capacity::trace_tail::MoeTraceTail::new(n_layers));
            tail.ensure_geometry(n_layers);
            tail.drain(&gb.trace);
            let ceiling = crate::capacity::trace_tail::pin_ceiling(
                budget_bytes,
                expert_bytes_total,
                n_layers,
                n_experts_per_layer,
            );
            let pins = tail.pin_list(ceiling);
            let moved = tail.pins_changed(&pins);
            (
                pins,
                tail.boundary_crossed,
                tail.tokens_observed,
                moved,
                tail.repeat_recall_x100(),
                tail.predicted_delta_recall_x100(),
                tail.schedulable_coverage_x100(),
            )
        };
        // Division actuation (#2 of the resident/cache split, contract 2026-08-03): the
        // warm-started bandit over the discovered `--resident-only` tiers picks which
        // RESIDENT precision the next relaunch should load, and the trace-tail token
        // delta feeds back the measured decode tok/s for the tier actually serving.
        // The tier label is a fourth plan axis; the device budget above remains the
        // one budget authority (a smaller resident frees VRAM the board then sees).
        let division = self.division_tick(
            &active,
            inputs.weights_host_bytes,
            expert_bytes_total,
            n_experts_per_layer,
            top_k,
            n_layers,
            tokens_observed,
        );
        let tier_moved = division.as_ref().is_some_and(|(_, moved)| *moved);
        if !budget_moved && !pins_moved && !boundary_crossed && !device_moved && !tier_moved {
            return; // no axis changed — no write, no mtime churn
        }
        let pins_count = pins.len();
        let mut doc = crate::capacity::plan_file::PlanFileDocument::new(
            budget_bytes,
            MOE_PLAN_WINDOW_K,
            pins,
        );
        if device_budget_bytes > 0 {
            doc = doc.with_device_budget(device_budget_bytes);
        }
        if let Some((tier_label, _)) = &division {
            doc = doc.with_resident_tier(tier_label.clone());
        }
        // Retention verdict (#287): does the published lease retain even one
        // token's expert working set? Below 100 (×100 fixed-point) the cache
        // buys NOTHING — every token evicts the previous token's experts (the
        // resident=0 thrash) — and the lease is honest about it instead of
        // looking healthy while thrashing silently.
        let per_token_ws = crate::capacity::host_cache_lease::per_token_expert_working_set_bytes(
            expert_bytes_total,
            n_experts_per_layer,
            top_k,
        );
        let retention_x100 =
            crate::capacity::host_cache_lease::retention_tokens_x100(budget_bytes, per_token_ws);
        if retention_x100 < 100 {
            crate::probe!(
                class = "serving.moe_host_cache_retention",
                model = active.as_str(),
                budget_bytes,
                per_token_ws_bytes = per_token_ws,
                retention_tokens_x100 = retention_x100,
                "lease retains LESS than one token's expert working set — the \
                 cache thrashes (zero cross-token reuse); free host RAM or accept \
                 streaming-only decode",
            );
        }
        match crate::capacity::plan_file::write_plan_file(&gb.plan, &doc) {
            Ok(()) => crate::probe!(
                class = "serving.moe_host_cache_lease",
                model = active.as_str(),
                budget_bytes,
                device_budget_bytes,
                device_derived_bytes = device_derived,
                derived_bytes = derived,
                weights_host_bytes = inputs.weights_host_bytes,
                live_kv_bytes = inputs.live_kv_bytes,
                per_token_ws_bytes = per_token_ws,
                retention_tokens_x100 = retention_x100,
                pins = pins_count as u64,
                trace_tokens = tokens_observed,
                warm_start = boundary_crossed,
                repeat_recall_x100 = repeat_recall.unwrap_or(0) as u64,
                predicted_delta_recall_x100 = delta_recall.unwrap_or(0) as u64,
                schedulable_coverage_x100 = coverage.unwrap_or(0) as u64,
                plan = gb.plan.display().to_string().as_str(),
                "published governed host-cache lease: {} GiB (raw {} GiB, retains \
                 {}.{:02} tokens' expert set)",
                budget_bytes / (1024 * 1024 * 1024),
                derived / (1024 * 1024 * 1024),
                retention_x100 / 100,
                retention_x100 % 100,
            ),
            Err(e) => crate::probe!(
                class = "serving.moe_host_cache_lease",
                model = active.as_str(),
                budget_bytes,
                error = e.to_string().as_str(),
                "plan-file write FAILED — actuator frozen at last published budget",
            ),
        }
    }

    /// One division-actuation tick (#2 of the resident/cache split, contract
    /// 2026-08-03). Ensures the actuator exists for the ACTIVE model (tier discovery +
    /// warm-start, rebuilt on model change), feeds the measured decode reward from the
    /// trace-tail token watermark, and returns the chosen resident-tier label + whether
    /// it moved. `None` = no feasible division (no governed VRAM, or every tier starves
    /// the cache) — the plan carries no `resident_tier` and spawn behavior is unchanged.
    fn division_tick(
        &self,
        active: &str,
        native_resident_bytes: u64,
        expert_bytes_total: u64,
        n_experts_per_layer: u32,
        top_k: u32,
        n_layers: u32,
        tokens_observed: u64,
    ) -> Option<(String, bool)> {
        use crate::capacity::division_actuation as da;
        let shape =
            da::shape_from_geometry(expert_bytes_total, n_layers, n_experts_per_layer, top_k)?;
        let Ok(mut guard) = self.division.lock() else {
            return None;
        };
        let stale = guard
            .as_ref()
            .map(|a| a.model_id() != active)
            .unwrap_or(true);
        if stale {
            // Live-anchored budget: the board's free-VRAM-after-fit (the #305 device
            // axis — already net of the resident serve and the placement margin) IS the
            // native tier's expert-cache pool, and a precision-shrunk resident grows it
            // by exactly its shrink delta. Expressed in the policy's HardwareBudget
            // shape as vram_total = live_free + native_resident with kv/reserve
            // pre-netted to 0 — one budget derivation, no parallel arithmetic.
            let live_free = governed_vram_ceiling(&self.resource_daemon)?
                .saturating_sub(EXPERT_PLACEMENT_MARGIN_BYTES);
            if live_free == 0 {
                return None;
            }
            let tiers = da::discover_resident_tiers(
                &crate::model_registry::artifacts::device_fit_cache_dir(active),
                native_resident_bytes,
            );
            let n_tiers = tiers.len();
            let hw = expert_pager_policy::division::HardwareBudget {
                vram_total_bytes: live_free.saturating_add(native_resident_bytes),
                kv_bytes: 0,
                compute_reserve_bytes: 0,
            };
            let served = self.served_resident.lock().ok().and_then(|g| g.clone());
            *guard = da::DivisionActuator::build(
                active,
                tiers,
                &hw,
                &shape,
                // v1 curve: the measured K3 trace-replay points. Per-model measured
                // curves ride the same seam once other MoEs report theirs.
                &expert_pager_policy::division::CoverageModel::k3_measured(),
                served.as_deref(),
            );
            crate::probe!(
                class = "serving.division",
                model = active,
                tiers = n_tiers as u64,
                feasible = guard.is_some(),
                live_free_bytes = live_free,
                native_resident_bytes,
                "division actuator (re)built: {} tier(s) in catalog, feasible={}",
                n_tiers,
                guard.is_some(),
            );
        }
        let act = guard.as_mut()?;
        if let Some(tok_s) = act.observe_tick(tokens_observed, std::time::Instant::now()) {
            crate::probe!(
                class = "serving.division_reward",
                model = active,
                served_tier = act.served_tier_label(),
                tok_s_x100 = (tok_s * 100.0) as u64,
                "measured decode {:.2} tok/s credited to served tier '{}'",
                tok_s,
                act.served_tier_label(),
            );
        }
        let (label, moved) = act.choose()?;
        if moved {
            crate::probe!(
                class = "serving.division",
                model = active,
                chosen_tier = label.as_str(),
                served_tier = act.served_tier_label(),
                "division choice MOVED: next relaunch should load resident tier '{}' \
                 (two-speed — no relaunch is triggered by this publish)",
                label,
            );
        }
        Some((label, moved))
    }

    /// No plan → publish the empty snapshot (no servable model = nothing live).
    /// Already serving the desired model & ready → no-op. A reconcile already
    /// in flight → skip (the gate). Otherwise spawn the reconcile.
    fn reconcile_to_plan(&self) -> Option<JoinHandle<()>> {
        // External serving pin (misfit / grid design): when the operator pinned an
        // EXTERNAL OpenAI-compatible endpoint via `LLAMA_SERVER_BASE_URL`, this node
        // does NOT own a local GPU serving lane — it ADOPTS the pinned endpoint. We
        // spawn / reclaim NOTHING (that would fight a co-located engine, e.g. a K3
        // llama-server, for its port), but we MUST publish the endpoint's ready
        // ServingSnapshot to `SERVING_STATE` so every consumer sees a ready lane:
        //   - `await_ready_serving` (persona-host gate + adapter factory),
        //   - the adapter's pre-generate model-guard via `current_serving()` — which
        //     refuses to generate unless the request's model == the resident model,
        //     read straight from the published snapshot (an empty snapshot → every
        //     turn "model is not the active served model", the bug this fixes).
        // Trust once-ready (a genuine wedge surfaces LOUD on a real turn); re-probe
        // only while not-yet-ready. The reachability probe is control-plane-fast, so
        // this publishes within a tick and never overlaps.
        if crate::inference::llama_server::external_serving_pin().is_some() {
            if self.serving_tx.borrow().ready {
                return None;
            }
            let serving_tx = self.serving_tx.clone();
            let bus = self.bus.get().cloned();
            return Some(tokio::spawn(async move {
                if let Some(snap) = crate::inference::llama_server::probe_external_serving(
                    crate::inference::llama_server::DEFAULT_SERVING_WAIT,
                )
                .await
                {
                    Self::emit_serving(bus.as_ref(), &snap);
                    let _ = serving_tx.send_replace(snap);
                }
            }));
        }
        // Pull the desired model id, the host-fit PER-LANE served window, AND
        // the lane count out of the plan in one borrow — both are the planner's
        // single source of truth (task #50). We carry them on the ServingTarget
        // so llama-server's `-c` (= window × lanes) and `--parallel` (= lanes)
        // match exactly what was planned: each slot gets one full served window.
        let (desired, served_ctx, lanes) = match self.plan_tx.borrow().as_ref() {
            Some(plan) => (
                plan.base_model.model_id.clone(),
                plan.served_context_window,
                plan.lanes,
            ),
            None => {
                // Nothing servable on disk → publish "nothing live" so readers
                // (and a grid allocator) see the gap and route elsewhere.
                self.set_active_artifact(None);
                if self.serving_tx.borrow().active_model.is_some() {
                    let empty = ServingSnapshot::empty();
                    Self::emit_serving(self.bus.get(), &empty);
                    let _ = self.serving_tx.send_replace(empty);
                }
                return None;
            }
        };

        // The trained genome layers registered for this base model — which genes
        // are loadable into the serving catalog (the `--lora` set). Read from the
        // producer-written manifest, keyed by the CONTINUUM base id (the HF base
        // in each gene's PEFT config never matches the served id, so a directory
        // scan can't make this association — see forge::adapter_manifest). An
        // unreadable/corrupt manifest degrades to base-only HERE (logged loud) so
        // the reconcile tick never panics; a MISSING gene file still fails loud at
        // spawn (AdapterNotFound). Empty = base model only, the legitimate state.
        let desired_adapters: Vec<AdapterEntry> = match crate::forge::adapter_manifest::load() {
            Ok(all) => crate::forge::adapter_manifest::for_base(&all, &desired)
                .into_iter()
                .map(|a| AdapterEntry {
                    alias: a.alias,
                    path: a.path,
                })
                .collect(),
            Err(e) => {
                crate::probe!(
                    class = "serving.adapters",
                    desired = desired.as_str(),
                    error = e.as_str(),
                    "adapter manifest unreadable — serving base model only this tick",
                );
                Vec::new()
            }
        };
        let mut desired_adapter_paths: Vec<String> = desired_adapters
            .iter()
            .map(|a| a.path.to_string_lossy().into_owned())
            .collect();
        desired_adapter_paths.sort();

        {
            let live = self.serving_tx.borrow();
            // Already serving the right model AND the right genome set → no
            // relaunch. The genome comparison is what makes a freshly-trained gene
            // take effect: same model id + new gene = set change = relaunch.
            if live.ready
                && live.active_model.as_deref() == Some(desired.as_str())
                && live.adapters == desired_adapter_paths
            {
                // …UNLESS the running server's per-slot window froze at HALF or
                // less of what the current plan affords. A lane spawned under
                // transient memory pressure (a benchmark server, a build) keeps
                // its starved window forever otherwise — glass-boxed 2026-07-10:
                // 14,700 recomputes wandering 3.6k↔22k while the living lane
                // stayed frozen at 3.8k and the room degenerated into a greeting
                // loop. 2× is deliberate hysteresis (doubling rule): the plan
                // breathes with every consumer, and a relaunch kills in-flight
                // turns, so only a step-change worth of headroom justifies one.
                // One-directional by design — over-served vs a dipped plan is
                // the pressure broker's reclaim problem (#79), not a relaunch.
                // SUSTAINED-DELTA re-home (replaces the single-sample `live * 2 <=
                // plan` ratio; BigMama's call 2026-08-06, she owns the guard).
                //
                // The old ratio was the cheapest thing that could not thrash against a
                // plan derived from FREE MEMORY, which wandered 3.6k<->22k — its own
                // comment says "the plan breathes with every consumer", and that was
                // true. Demand-derived planning made the plan a peak-tracking
                // MEASUREMENT instead, and against a stable signal a 2x bar strands
                // everything in the 50-99% band forever: measured live, plan 26,323 vs
                // lane 16,384 (62%) held indefinitely while every persona's binding
                // read the stale window.
                //
                // TWO independent guards, deliberately not collapsed into one
                // predicate:
                //
                //   1. SUSTAINED DELTA — is the gain REAL? The plan must exceed the
                //      lane by [`REHOME_MIN_GAIN_PCT`] for [`REHOME_SUSTAINED_TICKS`]
                //      consecutive ticks. A dip RESETS the streak, so jitter can never
                //      accumulate into a relaunch.
                //   2. COOLDOWN — may we PAY for it yet? At most one re-home per
                //      [`REHOME_COOLDOWN_TICKS`], checked separately and answered
                //      first. A relaunch kills every in-flight turn on the lane, so
                //      the rate limit must hold even when the evidence is perfect.
                //
                // Conflating them is how a guard ends up firing on a burst of good
                // evidence. (BigMama's spec 2026-08-06 — she owns this guard; the
                // outage it came from is hers.)
                let cooling = self.rehome_cooldown.load(Ordering::Relaxed);
                if cooling > 0 {
                    self.rehome_cooldown.store(cooling - 1, Ordering::Relaxed);
                    // Lose the streak while cooling rather than banking it: otherwise
                    // the instant the cooldown lapses we relaunch on evidence gathered
                    // a minute and a half ago. Re-proving costs 3 ticks and keeps
                    // "sustained" meaning sustained-NOW.
                    self.rehome_streak.store(0, Ordering::Relaxed);
                }
                let gain = served_ctx.saturating_sub(live.served_context_window);
                // Relative margin: gain/live >= 15%, in integer arithmetic that cannot
                // overflow a u32 window (`gain * 100` on a 1M-token window is ~1e8).
                let worth_it = live.served_context_window > 0
                    && gain.saturating_mul(100)
                        >= live
                            .served_context_window
                            .saturating_mul(REHOME_MIN_GAIN_PCT);
                let streak = if worth_it && cooling == 0 {
                    self.rehome_streak
                        .fetch_add(1, Ordering::Relaxed)
                        .saturating_add(1)
                } else {
                    // Not merely "don't count" — RESET. One tick that does not want
                    // the bigger window is enough to prove the demand was not
                    // sustained, and a streak that survives dips is a streak that
                    // eventually fires on noise.
                    self.rehome_streak.store(0, Ordering::Relaxed);
                    0
                };
                let starved = streak >= REHOME_SUSTAINED_TICKS && cooling == 0;
                // NOTE: the guards are re-armed at the FIRE point below, not here —
                // `starved` only means the evidence qualifies. A later suppression
                // (an eval holding the lane steady) still returns without relaunching,
                // and charging a cooldown for a relaunch that never happened would
                // rate-limit us out of the one we actually owe.
                if !starved {
                    // A lane running BELOW the plan but above the 2x bar declines to
                    // grow — silently, until this probe. That silence is the defect:
                    // measured 2026-08-06 the plan sat stable at 26,323 while the
                    // live lane served 16,384 (62% — comfortably above half), every
                    // persona's binding read the stale 16,384, and nothing anywhere
                    // said so. It looked like the demand-derived window had "grown
                    // then reverted"; it had never taken hold, and the only thing
                    // that ever applied a grown plan was a reboot.
                    //
                    // The 2x rule itself is NOT being changed here — it came from a
                    // real outage (a lane frozen at 3.8k while plans wandered
                    // 3.6k<->22k) and a relaunch kills in-flight turns
                    // ([[never-thrash-sticky-hysteresis-on-every-lane]]). But its
                    // stated premise is that "the plan breathes with every consumer",
                    // and demand-derived planning made the plan STABLE, so against it
                    // the bar now strands a third of the window indefinitely. That is
                    // a design call on a guard written from an incident, so it is
                    // surfaced for the humans who own that history rather than
                    // quietly re-tuned by me (#332/#333).
                    if live.served_context_window < served_ctx {
                        crate::probe!(
                            class = "serving.reconcile.window",
                            decision = "declined",
                            live_window = live.served_context_window,
                            plan_window = served_ctx,
                            live_lanes = live.lanes,
                            plan_lanes = lanes,
                            shortfall = gain,
                            streak,
                            needs_streak = REHOME_SUSTAINED_TICKS,
                            min_gain_pct = REHOME_MIN_GAIN_PCT,
                            cooling,
                            "lane is serving BELOW plan and has not yet earned a re-home: the gain must persist, not just appear",
                        );
                    }
                    return None;
                }
                // A living-persona eval is a co-tenant decode slot on THIS lane
                // (`ShareLane`). Growing its window means a relaunch, and a relaunch
                // connection-refuses the exam's in-flight generations (hard-rs 0/8,
                // 2026-07-20). The grow-back is OPTIONAL headroom; the exam is not.
                // Hold the lane steady until the eval drops its guard — a model/genome
                // change or a pressure shrink above still runs (this only gates the
                // starved GROW re-home). [[benchmark-is-a-governor-preemption-lease]]
                if serving_held_steady() {
                    crate::probe!(
                        class = "serving.reconcile.window",
                        decision = "declined",
                        live_window = live.served_context_window,
                        plan_window = served_ctx,
                        live_lanes = live.lanes,
                        plan_lanes = lanes,
                        shortfall = gain,
                        streak,
                        needs_streak = REHOME_SUSTAINED_TICKS,
                        min_gain_pct = REHOME_MIN_GAIN_PCT,
                        cooling,
                        "grow-back re-home suppressed: an eval holds the lane steady (co-tenant slot, no relaunch)",
                    );
                    // Streak deliberately NOT reset: the demand is genuinely sustained,
                    // the exam is simply first in line. When it drops its lease the
                    // re-home fires on the next tick instead of re-proving from zero.
                    return None;
                }
                // SYMMETRIC RECEIPT (BigMama's requirement, 2026-08-06): the defect was
                // that DECLINING was silent. A fix that makes RELAUNCHING silent has
                // moved the silence, not removed it. Same probe class, same fields, both
                // outcomes — so one query over `serving.reconcile.window` reads the
                // lane's whole decision history, not half of it.
                crate::probe!(
                    class = "serving.reconcile.window",
                    decision = "re-homing",
                    live_window = live.served_context_window,
                    plan_window = served_ctx,
                    live_lanes = live.lanes,
                    plan_lanes = lanes,
                    shortfall = gain,
                    streak,
                    needs_streak = REHOME_SUSTAINED_TICKS,
                    min_gain_pct = REHOME_MIN_GAIN_PCT,
                    // Always 0 here (a re-home cannot fire while cooling) — carried
                    // anyway so BOTH decisions emit the IDENTICAL field set and one
                    // query over the class gets a uniform schema instead of a shape
                    // that changes with the outcome. Symmetry is the point.
                    cooling,
                    "re-homing lane: the plan exceeded it by a real margin for a sustained run of ticks",
                );
                // Both guards re-armed at the moment we actually commit: the next
                // re-home must earn a fresh streak AND outlast a fresh cooldown.
                self.rehome_streak.store(0, Ordering::Relaxed);
                self.rehome_cooldown
                    .store(REHOME_COOLDOWN_TICKS, Ordering::Relaxed);
            }
        }

        // L10 (#438, live 2026-08-15 14:13): a MODEL CHANGE on a READY lane must earn
        // the same sustained streak the window re-home above already requires. The
        // asymmetry was the bug: growing a window (mild) needed REHOME_SUSTAINED_TICKS
        // of consistent evidence, while swapping the base model (a full teardown that
        // also re-homes every persona to different WEIGHTS) actuated on a single plan
        // tick. One bogus usable_bytes sample during the incumbent's own load made the
        // planner's nothing-fits arm name the SMALLEST candidate, the swap fired
        // immediately, and citizens spoke qwen2.5-0.5B template salad into the round's
        // room — durable garbage every healthy peer then perceived as conversation.
        // With this gate the plan must want the DIFFERENT model for the same sustained
        // run of ticks; a one-tick transient can never re-home minds. Deliberately
        // scoped: only when the lane is READY and serving something else — boot,
        // wedge-recovery, and genome page-ins (same model, adapter change) stay
        // immediate, and a change to a DIFFERENT different model resets the streak.
        {
            let live = self.serving_tx.borrow();
            let mut pending = self
                .pending_model_change
                .lock()
                .unwrap_or_else(|p| p.into_inner());
            let verdict = model_change_gate(
                live.ready,
                live.active_model.as_deref(),
                desired.as_str(),
                &mut pending,
                &self.model_change_streak,
                REHOME_SUSTAINED_TICKS,
            );
            match verdict {
                ModelChangeGate::Defer { streak } => {
                    crate::probe!(
                        class = "serving.reconcile.model",
                        decision = "deferred",
                        live_model = live.active_model.as_deref().unwrap_or("<none>"),
                        plan_model = desired.as_str(),
                        streak = streak as u64,
                        needs_streak = REHOME_SUSTAINED_TICKS as u64,
                        "plan wants a DIFFERENT model on a ready lane — the swap must \
                         persist across ticks before it re-homes minds (#438: a one-tick \
                         bogus-sample fallback once served citizens a 0.5B)",
                    );
                    return None;
                }
                ModelChangeGate::Commit { streak } => {
                    crate::probe!(
                        class = "serving.reconcile.model",
                        decision = "swapping",
                        live_model = live.active_model.as_deref().unwrap_or("<none>"),
                        plan_model = desired.as_str(),
                        streak = streak as u64,
                        "sustained plan disagreement — committing the model swap",
                    );
                }
                ModelChangeGate::NotAChange => {}
            }
        }

        // #175 sticky window: we're past the no-relaunch guard, so a relaunch WILL
        // happen (a genome page-in / model change). Don't let the startup LoRA-load
        // cascade ratchet the per-slot window DOWN on a same-lane relaunch and strand
        // the personas pinned to the earlier, larger slot — keep the incumbent window
        // when lanes are unchanged (memory-safe; a lane change legitimately resizes KV).
        let served_ctx = sticky_served_window(served_ctx, lanes, &self.serving_tx.borrow());
        // #363 prevention: the WEDGE-HEAL floor. The sticky floor above reads the live
        // snapshot, but declaring a wedge EMPTIES that snapshot first — so the very
        // relaunch the detector triggers used to spawn at whatever the teardown-transient
        // plan said (the dying predecessor still holds its memory when the successor's
        // budget samples free RAM). Floor the successor to the last HEALTHY lane's window
        // when the lane count is unchanged; the predecessor's window is exactly the
        // memory being freed, so the successor inherits it, and the existing overflow
        // protection stays the backstop if the world genuinely shrank.
        let floor_w = self.last_healthy_window.load(Ordering::Relaxed);
        let floor_l = self.last_healthy_lanes.load(Ordering::Relaxed);
        let live_ready = self.serving_tx.borrow().ready;
        let floored = wedge_heal_floor(served_ctx, lanes, live_ready, floor_w, floor_l);
        if floored != served_ctx {
            crate::probe!(
                class = "serving.reconcile.window",
                decision = "wedge-heal-floor",
                live_window = 0u32,
                plan_window = served_ctx,
                floored_to = floored,
                plan_lanes = lanes,
                "spawn-at-transient guarded: successor floored to the last healthy \
                 lane's window instead of a teardown-dip plan (#363)",
            );
        }
        let served_ctx = floored;

        // Resolve the full Model struct ONCE, here, and carry it on the target —
        // no re-fetch downstream ([[pass-the-model-struct-no-param-hell]]). If
        // the registry can't produce the model the plan named, fail loud (empty
        // snapshot) rather than serving something else.
        let Some(model) = (self.model_resolver)(&desired) else {
            crate::probe!(
                class = "serving.reconcile",
                desired = desired.as_str(),
                "plan named a model the registry can't resolve — publishing empty snapshot",
            );
            self.set_active_artifact(None);
            if self.serving_tx.borrow().active_model.is_some() {
                let empty = ServingSnapshot::empty();
                Self::emit_serving(self.bus.get(), &empty);
                let _ = self.serving_tx.send_replace(empty);
            }
            return None;
        };
        // K3 expert placement: for a MoE model, the ServingExpertPager plans which expert
        // LAYERS fit the governed VRAM budget and attaches them; the launcher `-ot`s the
        // cold complement to CPU. Computed BEFORE the struct literal moves `model`. `None`
        // for a dense model (or before the pager has committed a placement) — served exactly
        // as before, no override.
        let expert_placement = self.compute_expert_placement(&model);
        // Device-fit resident-override (#29): computed from the model's resident
        // (non-expert) footprint vs the governed VRAM budget. `Some(path)` when
        // resident overflows as-shipped and a cached device-fit override fits (the
        // launcher sources resident from it via `LLAMA_RESIDENT_OVERRIDE`); `None`
        // when resident fits natively OR no override is cached yet (resolver #35).
        let resident_override = self.compute_resident_override(&model);
        // Division reward attribution (two-speed honesty): record which resident the
        // spawn ACTUALLY loads so measured tok/s credits the serving tier, never the
        // bandit's latest unlaunched choice.
        if let Ok(mut g) = self.served_resident.lock() {
            *g = resident_override.clone();
        }
        if let Ok(mut d) = self.division.lock() {
            if let Some(act) = d.as_mut() {
                act.set_served_resident(resident_override.as_deref());
            }
        }
        // #302 invariant 1: mark the model's artifact ACTIVE before any spawn
        // touches it — the NvmeServingTierPool must never migrate the GGUF the
        // engine is loading or serving. Model change swaps the registration.
        self.set_active_artifact(model.gguf_local_path.clone());
        let target = ServingTarget {
            model,
            context_window: served_ctx,
            lanes,
            adapters: desired_adapters,
            // The living persona lane: GPU-resident for throughput (every
            // offloadable layer). [[LanePlacement]].
            placement: crate::inference::llama_server::LanePlacement::Gpu,
            expert_placement,
            resident_override,
        };

        // One reconcile at a time. If the swap finds `true`, another is already
        // running; skip rather than stack relaunches.
        if self.reconciling.swap(true, Ordering::AcqRel) {
            return None;
        }

        // Consume any pending force-relaunch the liveness heartbeat raised: it means the
        // heartbeat already saw the live lane fail decode, so this reconcile must re-prove
        // decode even on a child we own (else the owned-child trust re-adopts the wedged
        // lane forever, #175). Read+clear here so exactly one reconcile acts on it.
        let force_probe = self.force_relaunch.swap(false, Ordering::AcqRel);
        let server = self.server.clone();
        let serving_tx = self.serving_tx.clone();
        let reconciling = self.reconciling.clone();
        let bus = self.bus.get().cloned();
        let sidecar_slot = self.vision_sidecar.clone();
        let system = self.system.clone();
        let last_healthy_window = self.last_healthy_window.clone();
        let last_healthy_lanes = self.last_healthy_lanes.clone();
        // RAII gate-clear (#214): the `reconciling` flag was set `true` at the top of this
        // reconcile and MUST clear even if the relaunch task panics or is cancelled
        // mid-await — otherwise ONE failed relaunch (an OOM spawn under a memory squeeze, a
        // subprocess error, a panic in `ensure_model_serving`) strands the flag `true`, and
        // then EVERY future reconcile skips at the `swap(true)` gate above, freezing serving
        // at its current (possibly floored) window forever. Glass-boxed 2026-07-20: after a
        // benchmark squeeze released and VRAM returned to 55GB free, serving stayed frozen at
        // 2048 because the gate leaked on the churn's failed relaunch. `Drop` runs on panic
        // AND on the happy path, so the gate self-heals by construction — a stuck flag can
        // never outlive the task that set it.
        struct GateClear(Arc<AtomicBool>);
        impl Drop for GateClear {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }
        Some(tokio::spawn(async move {
            let _gate = GateClear(reconciling);
            let outcome = ensure_model_serving(server.as_ref(), &target, force_probe).await;
            // For a ready outcome, read the REAL per-slot window the running
            // server serves from its own `/props` — the authoritative model
            // metadata every persona budgets its prompt to. llama.cpp pads the
            // launch per-slot `-c/--parallel` window UP to a 256-multiple
            // internally, and the planner RE-computes its own window each tick
            // against live memory, drifting ABOVE the running server's frozen
            // slot; budgeting to that drifted value overflows the slot (500
            // "Compute error"). So we pass the process's own truth through, not
            // the plan's `served_ctx`. A read failure on a ready server yields 0,
            // which `snapshot_from_outcome` turns into "publish the gap" (not a
            // ready snapshot with a guessed window) — it self-heals next tick.
            let served_window = match &outcome {
                EnsureOutcome::AlreadyServing | EnsureOutcome::Spawned { .. } => {
                    match server.served_context_window().await {
                        Ok(n) => n,
                        Err(e) => {
                            crate::probe!(
                                class = "serving.reconcile",
                                desired = desired.as_str(),
                                error = %e,
                                "server ready but /props served window unreadable — \
                                 publishing the gap (no guessed window; retries next tick)",
                            );
                            0
                        }
                    }
                }
                EnsureOutcome::Degraded { .. } => 0,
            };
            // WHICH engine is answering (#, 2026-08-09). Read from the same `/props`
            // as the window, from the live process, so `serving/status` can answer
            // "is my fork fix in the binary that is actually running?" without the
            // mtime archaeology that produced a wrong attribution once already.
            // A read failure is NOT a degrade: identity is diagnostic, and a lane
            // that decodes fine while its `build_info` is unreadable is still a
            // working lane. It publishes `None` — unknown, never guessed.
            let engine_build = match &outcome {
                EnsureOutcome::AlreadyServing | EnsureOutcome::Spawned { .. } => {
                    server.engine_build().await.unwrap_or_else(|e| {
                        crate::probe!(
                            class = "serving.reconcile",
                            desired = desired.as_str(),
                            error = %e,
                            "server ready but /props build_info unreadable — engine \
                             identity unknown this tick (not a degrade; retries next tick)",
                        );
                        None
                    })
                }
                EnsureOutcome::Degraded { .. } => None,
            };
            // #106 vision readiness: for a ready lane, resolve the node's VERIFIED
            // vision endpoint. First the MAIN lane — the row's declared Vision, the
            // resolved mmproj, and the server's own `/props modalities` must all
            // agree. A text-only mind then gets a SIDECAR attempt (a small VL lane
            // beside it, `inference::vision_sidecar`) so personas are not blind just
            // because their mind's model doesn't see. Every gap publishes no
            // endpoint WITH the reason probed loud, so the observe path fails
            // honestly instead of POSTing pixels a text-only lane would drop.
            let vision = match &outcome {
                EnsureOutcome::AlreadyServing | EnsureOutcome::Spawned { .. } => {
                    let declares_vision =
                        target.model.has(crate::model_registry::Capability::Vision);
                    let mmproj_resolved =
                        crate::model_registry::artifacts::resolve_mmproj_for_model(&target.model)
                            .is_some();
                    let props = match server.multimodal_support().await {
                        Ok(p) => p,
                        Err(e) => {
                            crate::probe!(
                                class = "serving.vision.props_unreadable",
                                desired = desired.as_str(),
                                error = %e,
                                "ready lane but /props modalities unreadable — \
                                 publishing vision_ready=false (unverified ≠ sight)",
                            );
                            None
                        }
                    };
                    let main_sees = match crate::inference::llama_server::vision_lane_ready(
                        declares_vision,
                        mmproj_resolved,
                        props,
                    ) {
                        Ok(ready) => ready,
                        Err(why) => {
                            crate::probe!(
                                class = "serving.vision.not_ready",
                                desired = desired.as_str(),
                                why = why.as_str(),
                                "vision-capable row is serving without verified sight",
                            );
                            false
                        }
                    };
                    if main_sees {
                        // A VL mind: the main lane IS the vision endpoint. Any
                        // sidecar from a previous plan is redundant — drop it
                        // (its Drop kills the child, RAM freed).
                        *sidecar_slot.lock().await = None;
                        Some(crate::inference::vision_sidecar::SidecarLane {
                            base_url: serving_v1_url(),
                            model_id: desired.clone(),
                        })
                    } else {
                        use crate::inference::vision_sidecar as sidecar;
                        let rows: Vec<crate::model_registry::types::Model> =
                            crate::model_registry::try_global()
                                .map(|r| r.models().cloned().collect())
                                .unwrap_or_default();
                        let candidate = sidecar::find_candidate(&rows, Some(desired.as_str()));
                        match &candidate {
                            Err(skipped) => {
                                crate::probe!(
                                    class = "serving.vision.sidecar_no_candidate",
                                    skipped = skipped.join("; ").as_str(),
                                    "text-only mind and no on-disk VL row — personas \
                                     have no local vision endpoint (models/pull a \
                                     *-VL-*-GGUF repo to give them eyes)",
                                );
                                None
                            }
                            Ok(cand) => {
                                // Gate on the LIVE free-memory read, never a cached
                                // plan figure (the eval-lane second-model SIGKILL
                                // class).
                                let free = system.snapshot().memory.available_bytes;
                                match sidecar::plan_sidecar(false, Ok(cand), free) {
                                    sidecar::SidecarVerdict::Spawn => {
                                        let mut slot = sidecar_slot.lock().await;
                                        match sidecar::ensure_sidecar(&mut slot, cand).await {
                                            Ok(lane) => {
                                                crate::probe!(
                                                    class = "serving.vision.sidecar_up",
                                                    model = lane.model_id.as_str(),
                                                    base_url = lane.base_url.as_str(),
                                                    "vision sidecar verified — personas can see",
                                                );
                                                Some(lane)
                                            }
                                            Err(why) => {
                                                crate::probe!(
                                                    class = "serving.vision.sidecar_failed",
                                                    model = cand.model.id.as_str(),
                                                    why = why.as_str(),
                                                    "vision sidecar could not come up",
                                                );
                                                None
                                            }
                                        }
                                    }
                                    sidecar::SidecarVerdict::NoBudget {
                                        need_bytes,
                                        free_bytes,
                                    } => {
                                        crate::probe!(
                                            class = "serving.vision.sidecar_no_budget",
                                            model = cand.model.id.as_str(),
                                            need_bytes,
                                            free_bytes,
                                            "vision sidecar refused: not enough free host \
                                             memory beside the live lane",
                                        );
                                        None
                                    }
                                    // plan_sidecar was called with
                                    // main_vision_ready=false and Ok(cand), so the
                                    // remaining variants cannot arise; publish no
                                    // endpoint rather than assert in the reconcile.
                                    _ => None,
                                }
                            }
                        }
                    }
                }
                EnsureOutcome::Degraded { .. } => None,
            };
            let snapshot = snapshot_from_outcome(
                &outcome,
                &desired,
                &desired_adapter_paths,
                served_window,
                target.lanes,
                vision,
                engine_build,
            );
            crate::probe!(
                class = "serving.reconcile",
                desired = desired.as_str(),
                ready = snapshot.ready,
                active = snapshot.active_model.as_deref().unwrap_or("<none>"),
                served_window = snapshot.served_context_window,
                // On the reconcile line because that is where an operator already
                // looks when serving behaves unexpectedly, and "which engine" is
                // the first question a surprising behaviour raises.
                engine = snapshot.engine_build.as_deref().unwrap_or("<unreported>"),
                "serving reconcile complete",
            );
            // #363: remember the last HEALTHY lane's shape in a record that SURVIVES
            // the wedge-empty — the wedge-heal floor in the next reconcile reads it.
            if snapshot.ready && snapshot.served_context_window > 0 {
                last_healthy_window.store(snapshot.served_context_window, Ordering::Relaxed);
                last_healthy_lanes.store(snapshot.lanes, Ordering::Relaxed);
            }
            // Emit on the bus first (fan-out to every subscriber + the grid),
            // then update the in-process watch view.
            Self::emit_serving(bus.as_ref(), &snapshot);
            let _ = serving_tx.send_replace(snapshot);
            // #350: from here on, an empty snapshot means "we looked and nothing is
            // serving" — a real fault. BEFORE this first publish it only meant "the
            // daemon has not finished starting", and readers could not tell the two
            // apart, so boot noise was indistinguishable from a broken lane. Marked
            // AFTER the publish so a reader that sees `has_reconciled()` is guaranteed
            // to also see the published snapshot, never a torn in-between.
            crate::inference::llama_server::mark_first_reconcile();
            // `_gate` (GateClear) clears `reconciling` on drop here — and, crucially, also
            // on any panic/cancel above, which the explicit store used to miss.
        }))
    }

    /// The liveness HEARTBEAT (#175 self-heal) — see also [`judge_smoke_miss`] (L11),
    /// which decides whether a missed probe is contention or wedge evidence.
    /// On a cadence far slower than [`TICK`],
    /// decode-probe the LIVE lane the daemon currently believes is `ready`. The reconcile
    /// fast-path trusts that published `ready` and never re-probes a child it owns, so a
    /// Metal-GPU-OOM-POISONED backend — which answers every control-plane read (`/health`,
    /// `/v1/models`, `/props`) with 200 while every `llama_decode` 500s
    /// (`kIOGPUCommandBufferCallbackErrorOutOfMemory`) — stays `ready:true` forever and the
    /// persona substrate is bricked until a human reboots. This re-verifies the actual
    /// COMPUTE path and, after [`HEALTH_FAILS_TO_RELAUNCH`] CONSECUTIVE failures (hysteresis
    /// so a merely-busy lane is never reaped), flips the published snapshot to not-ready.
    /// The very next [`Self::reconcile_to_plan`] sees `ready == false`, skips its no-op
    /// guard, and kill+respawns the lane — the ONLY recovery llama.cpp offers ("recreate
    /// the backend to recover"). Runs off the tick as its own bounded task so a slow decode
    /// never stalls the 5s tick; returns the handle so tests can await it. `None` when it
    /// isn't a heartbeat tick, nothing ready is believed live, or a reconcile/probe is
    /// already in flight (never race the reconcile's own kill/swap).
    fn spawn_health_heartbeat_if_due(&self) -> Option<JoinHandle<()>> {
        // Slow-cadence gate: only every Nth tick runs a probe.
        if self.health_ticks.fetch_add(1, Ordering::Relaxed) % HEALTH_PROBE_EVERY_TICKS != 0 {
            return None;
        }
        // Only meaningful when we BELIEVE we have a ready live lane to verify. Not-ready
        // (booting / mid-relaunch) → nothing to probe; reset the streak so a fresh lane
        // that becomes ready starts clean.
        let believe_ready = {
            let s = self.serving_tx.borrow();
            s.ready && s.active_model.is_some()
        };
        if !believe_ready {
            self.health_fails.store(0, Ordering::Relaxed);
            return None;
        }
        // #363: SUSTAINED REAL-TURN FAILURE OUTRANKS EVERY TRUST PATH BELOW. The
        // 2026-08-07 blackout (25 min, every citizen turn dead, serving/status
        // ready:true throughout) was a wedge class neither trust path can see:
        // an undersized/mid-stream-dying lane REJECTS the fleet's real prompts
        // while (a) partial streams may stamp recent-decode trust and (b) a tiny
        // 1-token smoke probe still PASSES — "can generate" is not "can serve the
        // actual working set". Real failures are the honest evidence, stamped at
        // the adapter's local-lane error paths, and the same threshold as the
        // decode heartbeat keeps one hysteresis constant, not two.
        let real_fails = (self.real_fails)();
        if real_fails >= HEALTH_FAILS_TO_RELAUNCH as u64 {
            crate::inference::llama_server::reset_real_decode_failures();
            self.health_fails.store(0, Ordering::Relaxed);
            crate::probe!(
                class = "serving.health",
                ok = false,
                via = "real_turn_failures",
                consecutive = real_fails,
                threshold = HEALTH_FAILS_TO_RELAUNCH as u64,
                "consecutive REAL generations failed on the live lane — wedge evidence \
                 that outranks a passing smoke probe (#363 undersized/mid-stream class)",
            );
            Self::declare_lane_wedged(
                &self.force_relaunch,
                &self.serving_tx,
                self.bus.get(),
                "sustained real-turn failures (smoke probe may still pass)",
            );
            return None;
        }
        // DELIVERY BEATS PROBING. `decode_smoke_ok` is a real multi-token generation through
        // the LIVE slots, so it competes for the same slots as actual work. A lane saturated by
        // long prefills cannot hand it a slot, the miss reads as "no decode", and
        // HEALTH_FAILS_TO_RELAUNCH misses relaunch a lane that was never wedged — just busy.
        // Glass-boxed on the SWE bench (v13): health failed twice mid-run and the relaunch left
        // every downstream generate refusing with `serving: <none>`, killing the run.
        //
        // If a real generation produced tokens within this probe interval, the compute path is
        // PROVEN alive by work that already happened — a synthetic probe can add nothing and
        // can only steal a slot from the thing proving it. Probe QUIET lanes; trust busy ones.
        // The window is the probe cadence itself (no second constant to drift), and a fresh
        // boot with no observed decode yet falls through and probes as before.
        // [[a-benchmark-zero-is-a-claim-about-the-harness-until-proven-otherwise]]
        let probe_window_ms = TICK.as_millis() as u64 * HEALTH_PROBE_EVERY_TICKS;
        if let Some(since) = (self.decode_age)() {
            if since <= probe_window_ms {
                self.health_fails.store(0, Ordering::Relaxed);
                crate::probe!(
                    class = "serving.health",
                    ok = true,
                    via = "real_work",
                    ms_since_work = since,
                    "lane proven alive by real work (decode OR prefill advance — L9: a lane \
                     saturated in prefill produces no tokens but is NOT quiet) — skipped the \
                     smoke probe rather than contend for a slot with the work that proves it",
                );
                return None;
            }
        }
        // Never race the reconcile's own spawn/kill/swap, and never stack heartbeat probes.
        if self.reconciling.load(Ordering::Acquire) {
            return None;
        }
        if self.health_probing.swap(true, Ordering::AcqRel) {
            return None;
        }
        let server = self.server.clone();
        let serving_tx = self.serving_tx.clone();
        let health_fails = self.health_fails.clone();
        let health_probing = self.health_probing.clone();
        let force_relaunch = self.force_relaunch.clone();
        let last_miss_fp = self.last_miss_slots_fp.clone();
        let bus = self.bus.get().cloned();
        Some(tokio::spawn(async move {
            // A real one-token generation through the live slots — the ONLY probe that
            // distinguishes a healthy lane from an OOM-poisoned one (control-plane reads
            // stay 200 on a wedged backend). `decode_smoke_ok` is already bounded by
            // `DECODE_SMOKE_TIMEOUT`, so a wedged compute path resolves to `false` fast.
            let ok = server.decode_smoke_ok().await;
            if ok {
                health_fails.store(0, Ordering::Relaxed);
                // A success invalidates the miss-time fingerprint — a stale one must
                // never exonerate a LATER freeze.
                *last_miss_fp.lock().unwrap() = None;
                health_probing.store(false, Ordering::Release);
                return;
            }
            // L11: before counting the miss, ask the server's OWN /slots whether the
            // serve loop advanced since the LAST miss. The adapter stamps (L9) are
            // blind to work done for clients that no longer exist — measured
            // 2026-08-16 boot: an adopted lane grinding through a dead core's ghost
            // turns ate 2 smoke misses and got killed, three times, 23 minutes. A
            // changed fingerprint = queue contention (probe couldn't get a slot
            // because real work held them); frozen = wedge evidence, same threshold
            // and latency as before.
            let cur_fp = server.slots_activity_fingerprint().await;
            let prev_fp = {
                let mut guard = last_miss_fp.lock().unwrap();
                std::mem::replace(&mut *guard, cur_fp)
            };
            if matches!(
                crate::inference::llama_server::judge_smoke_miss(prev_fp, cur_fp),
                crate::inference::llama_server::SmokeMissVerdict::AliveViaSlotProgress
            ) {
                health_fails.store(0, Ordering::Relaxed);
                crate::probe!(
                    class = "serving.health",
                    ok = true,
                    via = "slot_progress",
                    "smoke probe missed but /slots advanced between misses — the serve \
                     loop is doing real work the adapter stamps can't see (ghost turns, \
                     other clients); contention, not a wedge (L11)",
                );
                health_probing.store(false, Ordering::Release);
                return;
            }
            let n = health_fails.fetch_add(1, Ordering::Relaxed) + 1;
            crate::probe!(
                class = "serving.health",
                ok = false,
                consecutive = n as u64,
                threshold = HEALTH_FAILS_TO_RELAUNCH as u64,
                "live lane failed the decode heartbeat (control-plane may still 200 — a \
                 poisoned Metal backend); #175 self-heal",
            );
            if n >= HEALTH_FAILS_TO_RELAUNCH {
                // Sustained no-decode = wedged, not transiently busy. Reset the streak so we
                // don't re-trigger before the relaunch lands and republishes ready.
                health_fails.store(0, Ordering::Relaxed);
                Self::declare_lane_wedged(
                    &force_relaunch,
                    &serving_tx,
                    bus.as_ref(),
                    "sustained decode failure",
                );
            }
            health_probing.store(false, Ordering::Release);
        }))
    }

    /// Declare the live lane WEDGED: arm the force-probe and publish not-ready.
    ///
    /// BOTH steps are load-bearing and neither works alone, which is why this is one
    /// function instead of two lines copied per reporter:
    /// - `force_relaunch` makes the next reconcile re-prove decode even on a child we OWN;
    ///   without it the "trusted thereafter" short-circuit re-adopts the wedged lane forever.
    /// - the empty snapshot makes the reconcile RUN at all; its no-op guard returns early on
    ///   `ready && same model && same adapters` — *before* it ever consumes the force flag.
    ///
    /// Two independent reporters call this, because they answer different questions:
    /// - the decode heartbeat: "can this lane produce a token?" (a poisoned Metal backend)
    /// - the stderr wedge watcher: "is a slot reporting an impossible state?" (2026-08-05)
    ///
    /// The second exists because the first cannot see a single wedged SLOT: the other slots
    /// still decode, and a lane that delivered any token inside the probe window is trusted
    /// without probing at all. The 4.1-hour wedge was delivering 0.14 tok/s the whole time —
    /// alive by every liveness measure, and making no progress by the only one that mattered.
    fn declare_lane_wedged(
        force_relaunch: &Arc<AtomicBool>,
        serving_tx: &watch::Sender<ServingSnapshot>,
        bus: Option<&Arc<MessageBus>>,
        reason: &str,
    ) {
        force_relaunch.store(true, Ordering::Release);
        let empty = ServingSnapshot::empty();
        Self::emit_serving(bus, &empty);
        let _ = serving_tx.send_replace(empty);
        crate::probe!(
            class = "serving.health",
            action = "relaunch",
            reason = reason,
            "flipped serving snapshot not-ready — reconcile will kill+respawn the wedged \
             lane (#175 self-heal)",
        );
    }

    /// Consume any wedge the live lane's stderr watcher raised and escalate it.
    ///
    /// Polled on the daemon tick rather than pushed, because the lifecycle authority must
    /// stay HERE: the watcher runs inside the log sink, and a log sink that could reap a
    /// serving process would be a second owner of the lane's life.
    fn take_reported_wedge(&self) {
        let Some(flag) = self.server.wedge_flag() else {
            return;
        };
        if !flag.take() {
            return;
        }
        Self::declare_lane_wedged(
            &self.force_relaunch,
            &self.serving_tx,
            self.bus.get(),
            "a slot reported impossible progress (>1.0)",
        );
    }

    /// Pure publish step: run the classifier on the given inputs, publish the
    /// result, log it. Split from `recompute` so it's testable without the
    /// global registry / live GPU.
    fn publish_plan(&self, budget: HostBudget, candidates: &[ModelFootprint]) {
        // Hysteresis: pass the currently-served model as the incumbent so a
        // transient free-memory dip doesn't thrash the served model.
        //
        // Boot's first plan used to have no incumbent (plan_tx holds None) and fall through to
        // plain selection — and THAT was #438. A successor core boots while its predecessor's
        // lane is still resident, so the one moment a squeeze is GUARANTEED is the one moment
        // both the at-rest credit and the downshift debounce are disabled. Measured: the 27B's
        // successor sampled `usable_gb=6`, downshifted to a 14B at MIN_SERVE_CTX, and reclaimed
        // the 27B 1.2s later — freeing 45 GB it could no longer use. An inherited lane is not an
        // external squeeze; it is a past form of ourself, and it is the incumbent.
        let incumbent = incumbent_for_plan(
            self.plan_tx
                .borrow()
                .as_ref()
                .map(|p| p.base_model.model_id.clone()),
            (self.inherited_lane)().as_ref(),
        );
        let demand = self.serving_demand();
        match plan_serving_stable(budget, candidates, incumbent.as_deref(), demand) {
            Some(plan) => {
                // DOWNSHIFT DEBOUNCE (#368): `plan_serving_stable`'s at-rest credit
                // only shields the incumbent from its OWN residency — an external
                // squeeze deep enough that even the credited budget can't hold it
                // forces `fresh` through, and `fresh` at a zeroed budget picks the
                // smallest model on disk. That is correct for a REAL eviction and
                // catastrophic for a transient (one depressed tick re-brained the
                // whole citizenry onto the 0.5B, then hysteresis defended the wrong
                // incumbent). So a downshift only takes effect after it has been
                // wanted for [`DOWNSHIFT_SUSTAINED_TICKS`] consecutive ticks; until
                // then the previous plan simply stands. Same shape as the rehome
                // streak guard above — sustained-ness separates capacity change
                // from jitter. Upshifts and same-model replans are never held.
                match downshift_gate(&plan, incumbent.as_deref(), candidates) {
                    DownshiftVerdict::NotADownshift => {
                        self.downshift_streak.store(0, Ordering::Relaxed);
                    }
                    DownshiftVerdict::Downshift => {
                        let streak = self
                            .downshift_streak
                            .fetch_add(1, Ordering::Relaxed)
                            .saturating_add(1);
                        if streak < DOWNSHIFT_SUSTAINED_TICKS {
                            crate::probe!(
                                class = "serving.plan",
                                decision = "downshift-held",
                                incumbent = incumbent.as_deref().unwrap_or("<none>"),
                                wanted = plan.base_model.model_id.as_str(),
                                streak,
                                needs_streak = DOWNSHIFT_SUSTAINED_TICKS,
                                usable_gb = (budget.usable_bytes / 1_000_000_000),
                                "fresh plan wants a LESS capable base — holding the \
                                 incumbent plan until the squeeze proves sustained (#368)",
                            );
                            return;
                        }
                        // Sustained: a real squeeze. Adopt, and re-arm the gate.
                        self.downshift_streak.store(0, Ordering::Relaxed);
                    }
                }
                // The per-prefill transient spike, derived BEFORE the probe so the
                // probe can carry it. This value decides the prefill grant's
                // deadband (#415) and prices "can local serve one more?" in
                // capacity::lease — and until 2026-08-13 it appeared in NEITHER,
                // so a flapping grant was unfalsifiable in exactly the way the
                // served_window comment below warns about. `None` here is NOT the
                // same fact as a measured zero: it means the served base model was
                // absent from its own candidate list, which is a planner
                // inconsistency, not "this model has no compute buffer".
                let spike_of_served = candidates
                    .iter()
                    .find(|c| c.model_id == plan.base_model.model_id)
                    .map(|f| f.compute_buffer_per_lane());
                // Emit-on-change gate (#399): the plan recomputes every tick but
                // the probe fires only when what it would SAY differs from the
                // last emission. Per-tick emission was 51% of the entire probe
                // stream — identical rows rotating real history away in minutes.
                //
                // The trigger is DECISIONS ONLY. Continuously-MEASURED fields
                // (usable_gb, served/demand windows) ride the row as payload but
                // never trigger it: on a live box free memory churns ±1-2 GB and
                // the derived window wobbles ~1k tokens every tick, so any
                // quantization of them still flaps at bucket boundaries — two
                // attempts measured live (1k buckets → 146 rows/3min; GB
                // granularity → 563 rows/15min). A WINDOW change only emits past
                // a relative deadband (>1/8 from the last-emitted value), the
                // same shape as the #415 prefill deadband: a real re-plan
                // (16384→26368) clears it, measurement wobble never does.
                let decisions = format!(
                    "{}|{}|{}|{}|{}|{}|{}",
                    plan.base_model.model_id,
                    plan.lanes,
                    plan.resident_models,
                    plan.fits_on_gpu,
                    candidates.len(),
                    spike_of_served.unwrap_or(0),
                    demand.lanes,
                );
                let plan_probe_changed = {
                    let mut last = self
                        .last_plan_probe
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    let window_moved = |prev: u32| -> bool {
                        plan.served_context_window.abs_diff(prev) > prev / 8
                    };
                    match last.as_ref() {
                        Some((d, w)) if *d == decisions && !window_moved(*w) => false,
                        _ => {
                            *last = Some((decisions, plan.served_context_window));
                            true
                        }
                    }
                };
                if plan_probe_changed {
                    crate::probe!(
                        class = "serving.plan",
                        base_model = plan.base_model.model_id.as_str(),
                        lanes = plan.lanes,
                        resident = plan.resident_models,
                        fits_on_gpu = plan.fits_on_gpu,
                        usable_gb = (budget.usable_bytes / 1_000_000_000),
                        candidates = candidates.len(),
                        // #415: the deadband's deciding input. Zero DISABLES the
                        // deadband entirely (reconcile short-circuits), so a grant
                        // that flaps while this reads 0 is a different defect from one
                        // that flaps with a real spike — and they were indistinguishable.
                        spike_bytes = spike_of_served.unwrap_or(0), // JUSTIFIED unwrap_or: probe-only, and 0 is not a fabricated measurement — it is the documented DISABLED sentinel the deadband already keys on, with `spike_known` on the next line carrying the Option's provenance so a reader can still tell "no spike" from "no data"
                        spike_known = spike_of_served.is_some(),
                        // WHICH bound decided the window. Without this the plan is
                        // unfalsifiable: a window that does not grow after demand rises
                        // looks identical whether the HOST could not fit more, the
                        // measured DEMAND did not ask for more, or the reconcile held
                        // within hysteresis — three different bugs with one appearance.
                        // Read 2026-08-06 when 11 turns measured over_window ≥ 1.09 and
                        // the served window sat unmoved at 16384; nothing on hand could
                        // say why, which is a hole in the glass box
                        // ([[a-probe-that-can-only-fail-is-worse-than-no-probe]]).
                        served_window = plan.served_context_window,
                        demand_window = demand.window_tokens,
                        demand_lanes = demand.lanes,
                        // `bootstrap` is a THIRD state, not a flavour of `demand`
                        // (2026-08-20). A cold plan reporting `demand` claims the minds
                        // asked for 16384 when none had asked for anything — and that is
                        // the one reading this field exists to rule out. Ordered so the
                        // unmeasured case wins: with no measurement there is no demand
                        // bound to be within, so calling it demand-bound is vacuous.
                        bound_by = if !demand.measured {
                            "bootstrap"
                        } else if plan.served_context_window >= demand.window_tokens {
                            "demand"
                        } else {
                            "host-fit"
                        },
                        demand_measured = demand.measured,
                        "serving plan recomputed",
                    );
                    // Warm-slot oversubscription for the ADOPTED plan (#266) —
                    // probed here (not in the pure planner, which runs twice per
                    // tick on different budgets and floods) and only when the
                    // plan-fingerprint changed, so a standing condition emits at
                    // its transitions, not per tick (#399).
                    if (plan.lanes as u32) < demand.lanes {
                        crate::probe!(
                            class = "serving.plan",
                            decision = "warm-slot-oversubscribed",
                            resident_personas = demand.lanes,
                            warm_slots = plan.lanes,
                            without_warm_slot = demand.lanes - plan.lanes as u32,
                            per_slot_floor =
                                crate::cognition::serving_plan::BOOTSTRAP_WORKING_SET,
                            "adopted plan cannot warmly host all resident personas at the \
                             full-turn window floor — unslotted minds re-prefill cold every \
                             turn until tiered off or grid-placed (#266)",
                        );
                    }
                }
                // Publish the LIVE lane count to the admission gate so its directed-turn
                // reservation semaphores size by what's actually served (`--parallel
                // plan.lanes`), not the `MAX_LANES` ceiling — exact once the plan can serve
                // fewer lanes than the ceiling (#139 compute-buffer fit). ONE source of truth.
                crate::cognition::resource_admission::set_served_lane_count(plan.lanes as usize);
                // And the prefill throttle's demand facts (#56): the served model's per-spike
                // transient compute buffer + the lane count. Published HERE, next to the lane
                // count, so both gates read the one plan — no second path.
                // A MISSING spike is not a measured zero. Publishing 0 silently
                // disables the prefill deadband (#415) and prices lease admission
                // as if prefill were free — two safety mechanisms turned off by an
                // absent signal rather than a decision
                // ([[removing-a-hardcode-is-not-automatically-an-improvement]]:
                // a missing signal must be loud, never a default that happens to
                // parse). Still publishes 0 so behaviour is unchanged pending the
                // #415 decision on what a deadband means with no spike — but it can
                // no longer happen invisibly.
                if spike_of_served.is_none() && plan_probe_changed {
                    crate::probe!(
                        class = "serving.plan.spike_missing",
                        base_model = plan.base_model.model_id.as_str(),
                        candidates = candidates.len(),
                        "served base model absent from its own candidate list — per-prefill \
                         spike unknown, publishing 0, which DISABLES the prefill deadband",
                    );
                }
                let spike = spike_of_served.unwrap_or(0);
                crate::cognition::prefill_throttle::publish_serving(spike, plan.lanes as usize);
                // send_replace keeps the latest even with no live receivers yet.
                let _ = self.plan_tx.send_replace(Some(plan));
            }
            None => {
                // No servable model on disk. Publish None and say so loudly —
                // the spawner gates on a model being present (no silent serve).
                // Same emit-on-change gate as the Some arm: loud ONCE at the
                // transition, silent while the condition persists (#399).
                let entered_empty = {
                    let mut last = self
                        .last_plan_probe
                        .lock()
                        .unwrap_or_else(|poisoned| poisoned.into_inner());
                    if last.as_ref().map(|(d, _)| d.as_str()) == Some("<no-servable-model>") {
                        false
                    } else {
                        *last = Some(("<no-servable-model>".to_string(), 0));
                        true
                    }
                };
                if entered_empty {
                    crate::probe!(
                        class = "serving.plan",
                        candidates = 0usize,
                        "no servable model on disk — serving plan empty",
                    );
                }
                let _ = self.plan_tx.send_replace(None);
            }
        }
    }
}

/// Verdict of [`downshift_gate`]: does adopting this plan REDUCE the served
/// base model's capability while the incumbent is still on disk?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DownshiftVerdict {
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
fn downshift_gate(
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

/// The live host readings a serving budget is derived from. A NAMED struct, not
/// three positional args, so the two byte counts (`available_bytes` and
/// `total_vram_bytes` are both `u64`) can never be silently transposed at a call
/// site — the compiler cannot catch `host_budget_from(total, available, ..)` on
/// positionals, but `HostBudgetInputs { available_bytes, total_vram_bytes, .. }`
/// names each ([[structs-by-reference-not-massive-param-lists]]). A new input
/// (e.g. a device-tier hint) becomes ONE added field with a default, not a fourth
/// positional every caller must thread.
#[derive(Debug, Clone, Copy)]
pub struct HostBudgetInputs {
    /// Monitor's current free memory (already net of everything else running);
    /// we never plan above what's free.
    pub available_bytes: u64,
    /// Physical VRAM ceiling — we never plan above what the device has.
    pub total_vram_bytes: u64,
    /// Performance-core proxy for the lane cap (floored at 1 inside).
    pub perf_cores: u32,
    /// Fraction of the board this host treats as ours to serve from (`usable =
    /// live × this`). The operator VRAM-headroom policy: default 0.80 (leave 20%
    /// for the OS + Bevy + embeddings), a dedicated foundry sets 1.0. Live callers
    /// pass `config_env::vram_headroom()`; tests pass an explicit fraction so
    /// `host_budget_from` stays pure + environment-independent.
    pub budget_fraction: f64,
}

/// Serving budget from LIVE free memory, capped at physical VRAM, minus headroom.
/// Pure for tests — the headroom fraction is an INPUT (`budget_fraction`), never
/// read from config here, so a test can't go environment-dependent. Takes
/// [`HostBudgetInputs`] by reference so the byte-count fields are named at every
/// call site (never transposable).
pub fn host_budget_from(inputs: &HostBudgetInputs) -> HostBudget {
    let live = inputs.available_bytes.min(inputs.total_vram_bytes);
    let usable = (live as f64 * inputs.budget_fraction) as u64;
    HostBudget {
        usable_bytes: usable,
        perf_cores: inputs.perf_cores.max(1),
    }
}

/// Pure pin fit-decision — split from the live-budget read so it is unit-testable.
/// A pin SWAPS: `serve()` kills the incumbent llama-server child, THEN launches the
/// candidate — never co-resident — so the candidate only needs to fit AFTER the
/// incumbent's VRAM is reclaimed. `base` is the live budget WITH the incumbent's
/// weights still counted as USED; crediting `incumbent`'s weights back models the
/// eviction. Without it, a swap DOWN to a model that fits alone but not alongside
/// the outgoing one is falsely denied (glass-boxed 2026-07-21: pin Devstral 14.3GB
/// refused at "budget ~12.1GB" while a 32B teacher was resident, though evicting it
/// frees ~20GB — the stronger-teacher swap-and-back the Academy needs). WEIGHTS
/// only (deterministic): the incumbent's variable KV is freed too, so this stays
/// conservative — a `fits_on_gpu` verdict here always holds in the real
/// post-eviction budget. `candidate = None` ⇒ no GGUF on disk ⇒ not servable.
fn pin_fit_decision(
    mut base: HostBudget,
    candidate: Option<ModelFootprint>,
    incumbent: Option<&ModelFootprint>,
) -> PinFit {
    if let Some(inc) = incumbent {
        base.usable_bytes = base.usable_bytes.saturating_add(inc.weights_bytes);
    }
    let budget_bytes = base.usable_bytes;
    let weights_bytes = candidate.as_ref().map(|f| f.weights_bytes).unwrap_or(0);
    // Fit verdict at ONE lane — "can this model hold a lane at all"; the live plan
    // sizes lanes from demand separately. footprint None → not servable (plan None);
    // footprint Some but over budget → plan_serving degrades with fits_on_gpu=false,
    // which `serving/pin` reads to refuse loud.
    let plan = candidate
        .and_then(|f| plan_serving(base, std::slice::from_ref(&f), ServingDemand::new(1, None)));
    PinFit {
        plan,
        weights_bytes,
        budget_bytes,
    }
}

/// The live serving budget for this host RIGHT NOW: free memory (capped at
/// physical VRAM, minus headroom) against the GOVERNED VRAM ceiling. A free
/// function so BOTH the daemon's autonomic tick ([`ServingDaemonModule::host_budget`])
/// and the `serving/pin` fit-gate derive the budget from the ONE source — a pin's
/// "will it fit" can never disagree with what the next tick would actually do.
pub fn live_host_budget(
    _system: &SystemResourceMonitor,
    resource_daemon: &ResourceDaemon,
) -> HostBudget {
    // Board-authoritative, same as the autonomic tick: the governed VRAM `available`
    // is the ONE source. We deliberately do NOT min it against `system` RAM
    // available — on UMA that raw vm_stat figure under-reports and would reject a model
    // that physically fits (the same clamp that floored the served window to 2048;
    // glass-boxed 2026-07-20). `_system` stays in the signature for call-site stability.
    let vram_ceiling =
        governed_vram_ceiling_or_report(resource_daemon, "board_authoritative_host_budget");
    host_budget_from(&HostBudgetInputs {
        available_bytes: vram_ceiling,
        total_vram_bytes: vram_ceiling,
        perf_cores: perf_cores(),
        budget_fraction: crate::config_env::vram_headroom(),
    })
}

/// The serving host budget from the GOVERNED board ALONE — the memory authority's
/// pre-staged snapshot (Vram `available`, netted over every measured consumer + external
/// pressure), never a raw GPU probe. This is the ONE budget an ephemeral eval/train lane
/// sizes against at spawn (MEMORY-AUTHORITY-DAEMON slice 2): reading the board is reading a
/// snapshot the authority maintains on ITS tick, not sampling memory on the spawn hot path.
/// Board-only (no `SystemResourceMonitor` needed), so any consumer holding the
/// `Arc<ResourceDaemon>` sizes a lane through the SAME `plan_serving` the autonomic serving
/// plan uses — one budget, one authority, no duplicate calc.
/// Process-global count of active "hold the live serving lane STEADY" requests. A
/// living-persona benchmark/eval runs ON the shared lane as a co-tenant decode slot —
/// `resources::placement::Placement::ShareLane` (same base already resident ⇒ no second
/// weight copy). The one thing that breaks it is a grow-back RE-HOME relaunch mid-exam:
/// it connection-refuses every in-flight generation (glass-boxed 2026-07-20: hard-rs 0/8,
/// zero output tokens, the lane bounced under the exam). While any hold is active the
/// daemon SKIPS the OPTIONAL grow-back re-home for an already-correctly-served lane. It
/// does NOT suppress a model/genome CHANGE or a pressure-driven shrink — a real resource
/// emergency still preempts (rare, correct). Zero-cost when no eval runs.
static SERVING_STEADY_HOLDS: AtomicUsize = AtomicUsize::new(0);

/// RAII: hold the live serving lane steady (suppress the grow-back re-home) until dropped.
/// A living-persona eval binds one for its whole run — the concrete form of `ShareLane`'s
/// "pin the lane for the demand's duration" clause ([[benchmark-is-a-governor-preemption-lease]],
/// the co-tenant/steady case: no second weight copy, just don't bounce the lane).
#[must_use = "the hold releases the instant this guard drops — bind it for the eval's lifetime"]
pub struct ServingSteadyHold {
    /// WHO pinned the lane — a run id, "eval", etc. Carried so the acquire and
    /// release EVENTS name the holder (Joel 2026-08-08: "emit events for
    /// everything — need to know"): a suppressed relaunch with no event is
    /// indistinguishable from a planner that never wanted one, and a leaked
    /// hold with no holder name is unattributable.
    holder: String,
}

impl ServingSteadyHold {
    pub fn acquire(holder: impl Into<String>) -> Self {
        let holder = holder.into();
        let holds = SERVING_STEADY_HOLDS.fetch_add(1, Ordering::AcqRel) + 1;
        crate::probe!(
            class = "serving.lane.hold",
            action = "acquired",
            holder = %holder,
            holds,
            "serving lane pinned STEADY (optional grow-back re-home suppressed while held)"
        );
        Self { holder }
    }
}

impl Drop for ServingSteadyHold {
    fn drop(&mut self) {
        let holds = SERVING_STEADY_HOLDS.fetch_sub(1, Ordering::AcqRel) - 1;
        crate::probe!(
            class = "serving.lane.hold",
            action = "released",
            holder = %self.holder,
            holds,
            "serving lane steady-hold released"
        );
    }
}

/// True while at least one caller holds the live lane steady — the reconcile's grow-back
/// re-home is suppressed for an already-correctly-served lane.
pub fn serving_held_steady() -> bool {
    SERVING_STEADY_HOLDS.load(Ordering::Acquire) > 0
}

/// LUDICROUS mode (Joel 2026-07-21: "extreme mode for benchmarks or ludicrous lol"). A
/// benchmark / project / "the fight" wants the biggest window the model+machine can give —
/// not the timid pressure-derived fraction. While any caller holds this, [`host_budget`]
/// forces `PowerMode::Performance` (fraction 1.0 — "floors the whole GPU"), OVERRIDING
/// `serving_mode_for_pressure`'s conservative read (which on UMA under-reports free memory
/// and floored a 47872-capable model to 2048 with ~28GB idle). This is the drive mode
/// following the ACTIVITY, not just the pressure: a declared Ludicrous intent → serve
/// maximal. Non-thrashing: one grow-relaunch when the hold is taken, one shrink when it
/// drops — bookends around the exam, not a flap. Zero-cost when nothing holds it.
/// [[serving-mode-follows-activity-ludicrous-to-dream]] [[benchmark-window-must-be-big-not-a-clamped-prompt]]
static SERVING_LUDICROUS_HOLDS: AtomicUsize = AtomicUsize::new(0);

/// RAII: while held, serving plans at `PowerMode::Performance` (the whole GPU, biggest
/// window). A benchmark binds one for its whole run so the exam is measured on the largest
/// window the model+machine allow — never a starved boot-window. Drop reverts to the live
/// pressure-adaptive mode.
#[must_use = "the Ludicrous hold releases the instant this guard drops — bind it for the benchmark's lifetime"]
pub struct ServingLudicrousHold(());

impl ServingLudicrousHold {
    pub fn acquire() -> Self {
        SERVING_LUDICROUS_HOLDS.fetch_add(1, Ordering::AcqRel);
        Self(())
    }
}

impl Drop for ServingLudicrousHold {
    fn drop(&mut self) {
        SERVING_LUDICROUS_HOLDS.fetch_sub(1, Ordering::AcqRel);
    }
}

/// True while at least one caller demands Ludicrous (Performance) serving — [`host_budget`]
/// pins `PowerMode::Performance` regardless of the pressure read.
/// Last VRAM ceiling the governed board actually reported this process, and when.
/// The middle rung of the substitute-value ladder: when the board goes quiet, an old
/// REAL number keeps the planner honest where a fabricated zero floors it.
static LAST_GOOD_VRAM_CEILING: AtomicU64 = AtomicU64::new(0);
static LAST_GOOD_VRAM_CEILING_AT_MS: AtomicU64 = AtomicU64::new(0);

/// The device's PHYSICAL VRAM, seeded once at boot from the GPU monitor. A static
/// hardware fact, known long before any pressure sample — which is precisely why a cold
/// boot never has to guess zero. 0 = no GPU reported a size.
static DEVICE_VRAM_TOTAL: AtomicU64 = AtomicU64::new(0);

/// Epoch milliseconds for the ladder's staleness arithmetic. Local because `now_ms` is
/// currently hand-rolled in five files tree-wide; adding a sixth private copy here would
/// be worse than reusing one, and hoisting all five is its own change, not this one.
fn epoch_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        // JUSTIFIED unwrap_or: the error case is a system clock set before 1970, and the
        // 0 it yields is not a fabricated quantity — it is the sentinel this module
        // already defines as "never recorded" (`LAST_GOOD_VRAM_CEILING_AT_MS == 0`), which
        // makes the last-known rung correctly UNAVAILABLE rather than falsely fresh. The
        // failure direction is toward less trust, not more, which is the only direction a
        // default may ever move a decision.
        .unwrap_or(0)
}

/// Seed the cold-boot prior. Called once as the serving daemon starts, BEFORE the first
/// plan, so the very first tick already has a real number to stand on.
pub fn seed_device_vram_prior(total_bytes: u64) {
    DEVICE_VRAM_TOTAL.store(total_bytes, Ordering::Relaxed);
    crate::probe!(
        class = "serving.vram_ceiling_prior.seeded",
        total_bytes = total_bytes,
        gib = format!("{:.2}", total_bytes as f64 / 1024.0 / 1024.0 / 1024.0).as_str(),
        "cold-boot VRAM prior seeded from the device",
    );
}

pub fn serving_ludicrous_active() -> bool {
    SERVING_LUDICROUS_HOLDS.load(Ordering::Acquire) > 0
}

pub fn governed_host_budget(resource_daemon: &ResourceDaemon) -> HostBudget {
    let available = governed_vram_ceiling_or_report(resource_daemon, "governed_host_budget");
    host_budget_from(&HostBudgetInputs {
        available_bytes: available,
        total_vram_bytes: available,
        perf_cores: perf_cores(),
        budget_fraction: crate::config_env::vram_headroom(),
    })
}

/// The governed VRAM ceiling RIGHT NOW: the resource authority's `available(Vram)`
/// — capacity (free + ours − reserve, scanned live by the GpuMonitor) minus what
/// is already leased. `None` when VRAM is ungoverned (no live monitor → the kind
/// never appears on the board), which the caller treats as fail-closed (cap at 0,
/// serving refuses rather than over-committing blind). A lock-free `watch`
/// snapshot read, never the governor's accounting lock — safe on the hot tick.
fn governed_vram_ceiling(resource_daemon: &ResourceDaemon) -> Option<u64> {
    // Serving budgets from ITS OWN view of the board — global available minus
    // every OTHER consumer's unmet reservation floor (`available_for`, the same
    // math `acquire` enforces) — never the reservation-blind global number.
    // #225 (Joel 2026-08-08: "the budgeter just has all its parts figure it
    // out"): budgeting from the global figure let the serving window grow over
    // the embed lane's 1792 MiB floor, leaving 604 MiB governed-available for a
    // faculty cognition needs every turn — embedding fully dead while serving
    // sat comfortable. The board row's existence still gates None ("governor
    // hasn't reported" stays distinct from "zero bytes free").
    resource_daemon
        .board()
        .kinds
        .iter()
        .find(|k| k.kind == ResourceKind::Vram)
        // THE LAST FABRICATION POINT IN THE CHAIN (#438). A row can EXIST while its
        // capacity reads 0 — `ledger.rs`'s `capacity.get(&kind).unwrap_or(0)` invents
        // that zero for a kind whose capacity source has not reported yet, which at boot
        // is simply true for a second. The ceiling ladder then classified it `Measured(0)`
        // and passed it straight through, correctly by its own rules, because a zero that
        // arrives as a number is indistinguishable from one that was observed.
        //
        // A governed VRAM kind with ZERO CAPACITY is never a true fact about a machine
        // that reports a GPU: capacity is the device's size, not its free space, and a
        // device does not shrink to nothing. So a zero-capacity row is treated as NO
        // READING — the ladder falls to last-known, then to the device prior — while a
        // zero AVAILABLE on a real capacity stays a genuine measurement (a full GPU).
        .filter(|k| k.capacity_bytes > 0)
        // budget_for_replacing, NOT available_for. The serving planner's whole job is to
        // decide what should be resident NEXT, and the swap releases what is resident now
        // — so counting serving's own weights against serving's own plan is circular. It
        // was measured doing exactly that on 2026-08-19: 30.25 GB resident subtracted from
        // a 55.1 GB ceiling handed the planner 16.9 GB, a 27B does not fit in 16.9 GB, so
        // it abandoned the 27B it was ALREADY RUNNING for a 7B, then a 0.5B. `acquire`
        // still uses available_for — a lease must never be granted against bytes not yet
        // released; only the replace-myself DECISION gets the add-back.
        .map(|_| resource_daemon.budget_for_replacing(SERVING_CONSUMER_ID, ResourceKind::Vram))
}

/// The governed VRAM ceiling for a planner that has no way to represent "unknown",
/// with the absence RECORDED rather than swallowed.
///
/// `governed_vram_ceiling` returns `None` for exactly one reason: the board carries
/// no `Vram` row — the governor has not reported yet (boot race) or this node tracks
/// no VRAM at all. That is **not** the same fact as "zero bytes free", and the four
/// call sites that used to write `.unwrap_or(0)` could not tell them apart. A
/// silently-substituted 0 makes a 64GB machine look like it has no GPU, which floors
/// the served window and refuses lanes — indistinguishable from, and byte-identical
/// to, the #213/#214 symptom this file's own comments were written to explain.
///
/// 0 remains the right CONSERVATIVE value to plan with (never over-commit VRAM we
/// cannot confirm). What was wrong was that the substitution left no trace. Now
/// every unknown ceiling names its call site on the wire, so a machine that plans
/// like it has no GPU can be told apart from one that actually has none — in one
/// grep, instead of a night.
///
/// Joel, 2026-08-05: "this philosophy of 'errors are bad' has ruined most of my
/// projects" — a fallback value is only legitimate when the fallback is VISIBLE.
fn governed_vram_ceiling_or_report(resource_daemon: &ResourceDaemon, site: &'static str) -> u64 {
    // THE SUBSTITUTE-VALUE LADDER (#438). This used to be `.unwrap_or(0)`, and that zero
    // — which no monitor ever measured — is what handed the planner `usable_gb = 0` at
    // boot and got a 0.5B spawned on a 64 GB machine. See
    // [`crate::resources::ceiling_prior`] for why 0 is safe for a GRANTER and
    // catastrophic for a PLANNER, and why every rung below beats it.
    // DISCRIMINATOR (#438): if a Vram row exists but carries zero capacity, say so with
    // the numbers, so the next boot PROVES the mechanism instead of leaving it inferred.
    if let Some(row) = resource_daemon
        .board()
        .kinds
        .iter()
        .find(|k| k.kind == ResourceKind::Vram)
    {
        if row.capacity_bytes == 0 {
            crate::probe!(
                class = "serving.vram_capacity_absent",
                site = site,
                capacity_bytes = row.capacity_bytes,
                physical_used_bytes = row.physical_used_bytes,
                available_bytes = row.available_bytes,
                "a Vram row exists with ZERO CAPACITY — the capacity source has not \
                 reported. Falling to the prior ladder; this is NOT evidence of a GPU \
                 that shrank to nothing.",
            );
        }
    }

    let board = governed_vram_ceiling(resource_daemon);

    // Record every real reading so the NEXT silent tick has something honest to stand on.
    if let Some(bytes) = board {
        LAST_GOOD_VRAM_CEILING.store(bytes, Ordering::Relaxed);
        LAST_GOOD_VRAM_CEILING_AT_MS.store(epoch_ms(), Ordering::Relaxed);
    }

    let last_at = LAST_GOOD_VRAM_CEILING_AT_MS.load(Ordering::Relaxed);
    let reading = crate::resources::ceiling_prior::decide(crate::resources::CeilingEvidence {
        board_bytes: board,
        last_good: (last_at > 0).then(|| {
            (
                LAST_GOOD_VRAM_CEILING.load(Ordering::Relaxed),
                epoch_ms().saturating_sub(last_at),
            )
        }),
        device_total_bytes: match DEVICE_VRAM_TOTAL.load(Ordering::Relaxed) {
            0 => None,
            n => Some(n),
        },
    });

    // Only NON-measured rungs are worth a probe row: a healthy board reading every tick
    // is the steady state and would drown the stream (#399). A prior in use is exactly
    // the condition an operator needs to see, so it always speaks.
    if reading.provenance != crate::resources::Provenance::Measured {
        crate::probe!(
            class = "serving.vram_ceiling_prior",
            site = site,
            bytes = reading.bytes,
            gib = format!("{:.2}", reading.bytes as f64 / 1024.0 / 1024.0 / 1024.0).as_str(),
            provenance = format!("{:?}", reading.provenance).as_str(),
            why = reading.note,
            "planning against a SUBSTITUTE ceiling, not a measurement",
        );
    }

    // A NAMED BRANCH, NOT `unwrap_or(0)`. The value is the same; the difference is that
    // the zero is now a decision someone wrote down and a reviewer can argue with,
    // instead of a default that reads like punctuation. `unwrap_or` states no reason and
    // leaves no place to put one — which is exactly how the original zero survived
    // review at six call sites.
    match reading.usable_bytes() {
        Some(bytes) => bytes,
        // No board, no history, no device. Planning against 0 makes the planner decline,
        // and declining is the honest outcome when nothing on this machine can say how
        // much VRAM exists. Unreachable on any host with a GPU — it is the ladder's last
        // rung, not (as before) the first thing a cold boot hit.
        None => 0,
    }
}

/// Performance-core proxy for the lane cap. `num_cpus::get_physical()` is the
/// portable floor; on Apple Silicon it over-counts (efficiency cores), but the
/// `MAX_LANES` ceiling in the classifier is the binding cap on capable boxes,
/// so this only matters on tiny machines where physical-core count is a fine
/// proxy. Refined to true P-core detection in a later slice.
///
/// `pub` so off-daemon planners that build a one-shot [`HostBudget`] (e.g. the
/// ephemeral eval lane sizing its `-c` via [`plan_serving`]) derive the lane cap
/// from the SAME proxy, never a second constant.
pub fn perf_cores() -> u32 {
    num_cpus::get_physical().max(1) as u32
}

/// Build a footprint for one registry model IFF its GGUF is actually on disk —
/// we can only serve what's present. `None` for cloud models, models with no
/// resolved local file, or anything we can't stat.
///
/// Footprint estimates are honest where we can be (weights = real file size)
/// and COARSE where the registry lacks the data (per-lane KV, capability rank).
/// Refine when the registry carries arch internals (layers × kv_heads ×
/// head_dim) and a real capability score; the classifier consumes whatever
/// precision we give it without changing shape.
/// The [`FootprintFn`] serving hands the [`ResourceGovernor`](crate::resources):
/// resolve an active model id + its live serving shape (served per-slot window,
/// lane count) → total PEAK resident bytes in VRAM, from the SAME live catalog +
/// [`footprint_for`] estimator the serving plan uses (one footprint authority,
/// not two). An id the catalog doesn't know, or a model with no on-disk weights,
/// resolves to `0` — nothing resident to attribute. This reports the FULL PEAK
/// (#56/G5): weights + the KV-cache of every lane at the served window + the
/// concurrent-prefill compute reserve of every lane — i.e. `peak_resident_bytes`,
/// which equals the plan's `chosen_cost` exactly. Folding in the KV term (#79)
/// stopped serving's own KV masquerading as external; folding in the compute
/// reserve (G5) stops the board over-reporting free VRAM by the prefill buffer,
/// which is the bytes a SECOND consumer (the eval lane, a train job) would grab
/// out from under serving's next prefill → the concurrent-OOM.
fn serving_footprint_fn(catalog: Arc<ModelCatalog>) -> FootprintFn {
    Arc::new(move |id: &str, served_window: u32, lanes: u32| {
        catalog
            .snapshot()
            .get(id)
            .and_then(|live| footprint_for(&live.model))
            .map(|fp| fp.peak_resident_bytes(served_window, lanes))
            .unwrap_or(0)
    })
}

/// Conservative host floor for the OS + every process we don't own: an eighth of
/// physical RAM, floored at 2 GiB. DERIVED from the box (never env-tunable, never a
/// flat constant that breaks on an 8GB laptop or a 512GB server): on the incident's
/// 63GB box it reserves ~7.9GB — the conservative side of the ~6GB her measurements
/// attribute to OS+other, and over-reserving only shrinks the cache (safe), while
/// under-reserving re-creates the pagefile thrash. Follow-up (named, not silent):
/// derive from measured non-serving usage once mmap attribution is solved — the
/// free-memory monitor can't be used here because mmap'd weight pages report
/// "available" while load-bearing (see host_cache_lease module doc).
fn host_os_floor_bytes(physical_bytes: u64) -> u64 {
    (physical_bytes / 8).max(2 * 1024 * 1024 * 1024)
}

/// Windows only: the system-wide commit charge (`GetPerformanceInfo`), the number
/// the pagefile-overcommit clamp binds against — a Windows box under load can have
/// commit far above what a "free RAM" read suggests, and pagefile overcommit
/// thrashes SILENTLY (the 2026-08-01 incident: 95.9GB commit on a 63GB box, fetch
/// collapsed 2.5GB/s→205MB/s with no OOM anywhere). `None` on read failure — the
/// lease then falls back to the headroom bound alone, which is honest but looser.
#[cfg(windows)]
fn system_commit_charge_bytes() -> Option<u64> {
    use windows_sys::Win32::System::ProcessStatus::{GetPerformanceInfo, PERFORMANCE_INFORMATION};
    // SAFETY: PERFORMANCE_INFORMATION is a plain C struct; zeroed is a valid
    // initial state, and GetPerformanceInfo only writes within `cb` bytes.
    let mut info: PERFORMANCE_INFORMATION = unsafe { std::mem::zeroed() };
    info.cb = std::mem::size_of::<PERFORMANCE_INFORMATION>() as u32;
    let ok = unsafe { GetPerformanceInfo(&mut info, info.cb) };
    (ok != 0).then(|| (info.CommitTotal as u64).saturating_mul(info.PageSize as u64))
}

/// macOS/Linux: commit is not the binding regime (no silent pagefile ballooning of
/// this shape) — the lease's headroom arithmetic is the bound. See the law's doc.
#[cfg(not(windows))]
fn system_commit_charge_bytes() -> Option<u64> {
    None
}

/// Assemble the host-cache lease inputs for a MoE serve from quantities the planner
/// ALREADY budgets — every term justified, none invented, none read from a
/// free-memory monitor (the mmap subtlety: file-backed weight pages report
/// "available" while load-bearing):
///
/// - `weights_host_bytes` = GGUF file size MINUS the expert-tensor total. Expert
///   bytes are exactly what the governed cache holds (funded BY the lease) or sit
///   hot in VRAM — they are never host working set. Without the split, a 663GB
///   streaming MoE would "cost" 663GB of host RAM and the lease would derive 0.
/// - KV and compute-buffer terms come from the planner's OWN laws (`kv_at`,
///   `prefill_compute_reserve` — one formula, one place), but computed over a
///   footprint built from the HOST-RESIDENT mass, not the full file: those
///   heuristics scale with weight bytes as a proxy for layer/graph size, and for
///   a streaming MoE the dense/attention share is the mass that actually sizes
///   the KV stack and prefill graph (663GB would yield an absurd 68GB KV
///   estimate and a 41GB buffer, deriving a permanent zero lease).
/// - `live_kv_bytes` uses the LIVE served window × LIVE lane count from the
///   snapshot (llama.cpp allocates one full window per slot) — the term whose
///   omission thrashed the 63GB box to 95.9GB commit.
///
/// `None` when the host share is zero (a degenerate all-expert read — no honest
/// footprint to derive from; publish nothing rather than a guess).
fn moe_host_cache_lease_inputs(
    model_id: &str,
    file_weights_bytes: u64,
    expert_bytes_total: u64,
    model_context_window: u32,
    served_window: u32,
    lanes: u32,
    physical_bytes: u64,
    commit_charge_bytes: Option<u64>,
) -> Option<crate::capacity::host_cache_lease::HostCacheLeaseInputs> {
    let weights_host_bytes = file_weights_bytes.saturating_sub(expert_bytes_total);
    let fp = footprint_from_parts(model_id, weights_host_bytes, model_context_window, false)?;
    let lanes = lanes.max(1);
    Some(crate::capacity::host_cache_lease::HostCacheLeaseInputs {
        physical_bytes,
        weights_host_bytes,
        live_kv_bytes: fp.kv_at(served_window).saturating_mul(lanes as u64),
        compute_buffer_bytes: fp.prefill_compute_reserve(served_window, lanes),
        os_floor_bytes: host_os_floor_bytes(physical_bytes),
        commit_charge_bytes,
    })
}

pub fn footprint_for(model: &Model) -> Option<ModelFootprint> {
    // THE ROW IS THE SOURCE OF TRUTH. `weights_bytes` is stamped once, where the GGUF
    // path is resolved (`artifacts::hydrate_artifact_sizes`). This function runs on the
    // governor's accounting tick, so re-`stat`ing the file per call put filesystem I/O
    // on a hot path for a number that cannot change while the path is valid.
    //
    // The stat REMAINS as the fallback, and it is not dead code: rows built by hand in
    // tests, fixtures, and any construction path that never went through the resolver
    // carry `weights_bytes: None`. Falling back keeps those honest rather than sizing
    // them at zero — but the fallback is the exception, not the steady state.
    let weights_bytes = match model.weights_bytes {
        Some(n) => n,
        None => {
            let path = crate::model_registry::artifacts::resolve_gguf_for_model(model)?;
            std::fs::metadata(&path).ok()?.len()
        }
    };
    let mut fp = footprint_from_parts(
        &model.id,
        weights_bytes,
        model.context_window,
        model.has(Capability::ToolUse),
    )?;
    // KV CACHE QUANTIZATION (#232): a lane running quantized KV holds proportionally
    // fewer bytes/token, so the plan can size a BIGGER window into the same budget —
    // this is what turns the launcher's opt-in q8_0 flag into an actual window GROWTH.
    // Divide the f16 rate by the quant factor; default (f16 / unset) → 1 → byte-identical.
    // Keep the config key in sync with the launcher arg in inference/llama_server.rs —
    // one SERVING_KV_CACHE_TYPE key, two consumers (launcher flag + this fit-math rate).
    fp.kv_per_token = (fp.kv_per_token / kv_cache_quant_divisor()).max(1);
    Some(fp)
}

/// The resident-KV divisor implied by `SERVING_KV_CACHE_TYPE`, so the plan sizes the
/// served window against the KV the lane WILL actually hold, not the f16 default. (#232)
fn kv_cache_quant_divisor() -> u64 {
    kv_divisor_for(crate::config_env::read("SERVING_KV_CACHE_TYPE").as_deref())
}

/// Pure KV-rate divisor for a cache-type string (testable without env). CONSERVATIVE by
/// design: q8_0 ≈ half of f16 → 2; q4_0/q4_1 ≈ a third → 3 (under the ideal ~3.5×, so the
/// plan never over-grows the window past the real KV and OOMs). Anything else / f16 → 1
/// (no change). Over-reserve is a smaller window (safe); under-reserve is an OOM (fatal).
fn kv_divisor_for(cache_type: Option<&str>) -> u64 {
    match cache_type.map(|s| s.trim().to_ascii_lowercase()).as_deref() {
        Some("q8_0") => 2,
        Some("q4_0") | Some("q4_1") => 3,
        _ => 1,
    }
}

/// Pure footprint estimate from the fields that drive it — split out from the
/// fs/registry IO so the (coarse, tunable) heuristics are unit-testable.
/// `None` when there are no weights to serve.
fn footprint_from_parts(
    id: &str,
    weights_bytes: u64,
    context_window: u32,
    tool_capable: bool,
) -> Option<ModelFootprint> {
    if weights_bytes == 0 {
        return None;
    }
    // Coarse per-token KV RATE: scale with model size (more weights ≈ more
    // layers ≈ more KV/token). ~weights/20k bytes per token, floored. The
    // planner multiplies this by the window IT derives from the host budget —
    // we do NOT pre-collapse it against an assumed serving window here (no
    // PLANNED_CTX clamp; the served window is the planner's call, task #50).
    // Realistic per-token KV. These Qwen coders use grouped-query attention (few KV
    // heads), so KV is SMALL: ~200 KB/token for the 14B, ~260 KB/token for the 32B. The
    // old `weights/20_000` gave ~1 MB/token for the 32B — 4× too high — which made a 32B
    // that comfortably fits a 64 GB box read as ~32 GB of KV at 32k ctx and get falsely
    // rejected by the fit-gate, so the planner kept a weaker model. `weights/80_000`
    // (~110 KB for 14B, ~250 KB for 32B) tracks real Q4 GQA KV; still coarse, still floored.
    let kv_per_token = (weights_bytes / 80_000).max(20_000);

    // Coarse capability rank: GB of weights (bigger ≈ more capable within a
    // family), +bonus for tool/code capability. Saturates into u8.
    let gb = (weights_bytes / 1_000_000_000).min(250) as u16;
    let tool_bonus = if tool_capable { 2 } else { 0 };
    let capability_rank = gb.saturating_add(tool_bonus).min(255) as u8;

    Some(ModelFootprint {
        model_id: id.to_string(),
        weights_bytes,
        kv_per_token,
        // The model's trained ceiling, carried straight through — the planner
        // caps the served window to it. Floored at MIN_SERVE_CTX so a registry
        // entry with a bogus 0 window can't degrade below runnable.
        context_window: context_window.max(MIN_SERVE_CTX),
        capability_rank,
    })
}

/// The servable candidates from the LIVE model universe — every model that is
/// [`Availability::Ready`] (its artifact is on disk, or it was just pulled and
/// flipped Ready with its real path) and yields a footprint. Planning off the
/// snapshot (not the immutable seed) is what makes a runtime acquisition
/// servable without a reboot: `models/pull` writes the path + Ready into this
/// same snapshot, and the next tick's `candidates_from_snapshot` includes it.
/// A `NotDownloaded` model is correctly excluded — we only offer what we can
/// actually serve right now. The footprint resolves through the live model's
/// `gguf_local_path` (which `resolve_gguf` prefers when present), so the bytes
/// counted are the bytes that will be loaded.
/// The servable candidate set for THIS host right now: on-disk models, minus the
/// operator-suppressed set, minus the persona-ineligible benchmark/opponent rows (a
/// PIN bypasses eligibility — pinning IS operator consent, #142). The ONE definition of
/// "what the autonomic plan may serve," shared by [`ServingDaemonModule::live_candidates`]
/// (the plan) and the tier-down ranker (#56, so a shrink can only re-home to a model the
/// plan would itself have picked — never a divergent second catalog).
fn servable_candidates(
    snapshot: &CatalogSnapshot,
    suppressed: &HashSet<String>,
    pinned: &Option<String>,
) -> Vec<ModelFootprint> {
    let ineligible: HashSet<String> = snapshot
        .models
        .values()
        .filter(|live| !live.model.persona_serving_eligible)
        .map(|live| live.model.id.clone())
        .collect();
    candidates_from_snapshot(snapshot)
        .into_iter()
        .filter(|c| !suppressed.contains(&c.model_id))
        .filter(|c| match pinned.as_ref() {
            Some(p) => p == &c.model_id,
            None => !ineligible.contains(&c.model_id),
        })
        .collect()
}

pub fn candidates_from_snapshot(snapshot: &CatalogSnapshot) -> Vec<ModelFootprint> {
    snapshot
        .models
        .values()
        .filter(|live| live.status.availability == Availability::Ready)
        .filter_map(|live| footprint_for(&live.model))
        .collect()
}

/// Classify the detected GPU into the persona spawner's tier inputs — the
/// `n_gpu_layers` + roster decision. Retires the old hardcoded `CpuOnly +
/// Compat` (which clamped an M5 Pro to the LCD 0.5B on CPU). Conservative:
/// hardware we don't positively recognize falls back to `Compat`/`CpuOnly`
/// (the safe path), so the clamp only lifts when we KNOW the silicon.
/// `tier_category` is load-bearing (drives `n_gpu_layers`); `HwCapabilityTier`
/// is currently informational, so coarse generation mapping suffices.
pub fn detect_tier(gpu_name: &str) -> (HwCapabilityTier, HwTierCategory, &'static str) {
    let g = gpu_name.to_lowercase();
    if g.contains("apple") {
        let cap = if g.contains("m5") {
            HwCapabilityTier::M5UmaProMax
        } else if g.contains("m4") {
            HwCapabilityTier::M4UmaProMax
        } else if g.contains("m3") {
            HwCapabilityTier::M3UmaProMax
        } else if g.contains("m2") {
            HwCapabilityTier::M2UmaProMax
        } else {
            HwCapabilityTier::M1Uma16Gb
        };
        return if g.contains("pro") || g.contains("max") || g.contains("ultra") {
            (cap, HwTierCategory::MSeriesPro, "apple-mseries-pro")
        } else {
            (cap, HwTierCategory::MSeries, "apple-mseries")
        };
    }
    if g.contains("nvidia") || g.contains("geforce") || g.contains("rtx") || g.contains("cuda") {
        return (HwCapabilityTier::Sm89, HwTierCategory::Cuda, "nvidia-cuda");
    }
    // Intel-Mac discrete Metal is a known garbled-token path; unknown/CPU
    // hardware stays on the safe LCD/CPU tier.
    (HwCapabilityTier::CpuOnly, HwTierCategory::Compat, "compat")
}

/// Map a reconcile [`EnsureOutcome`] (+ the real per-slot window read from
/// `/props`) to the published [`ServingSnapshot`]. Pure (no IO) so the mapping
/// #175 sticky per-slot window on relaunch. The startup LoRA-load cascade relaunches
/// the living lane once per persona (each genome-set change is a relaunch), and each
/// relaunch recomputes `-c` against now-lower free memory, ratcheting the per-slot
/// window DOWN — which strands the personas pinned to the earlier, larger slot (they
/// then overflow it → the poisoned-slot "Compute error"). Keep the incumbent window
/// when we relaunch at the SAME lane count: its KV is already resident and a same-lane
/// genome reload adds only a tiny LoRA delta, so preserving it is memory-safe. A
/// LANE-count change legitimately resizes per-slot KV (2 slots need half the window
/// each) — keeping the old window across a lane INCREASE would multiply resident KV
/// and risk OOM, so only stick when `live.lanes == plan_lanes`. A LARGER plan window
/// (memory freed) is never held down here — the "starved lane" grow check above owns
/// growing. Pure so the invariant is unit-tested without a live gateway.
fn sticky_served_window(plan_window: u32, plan_lanes: u32, live: &ServingSnapshot) -> u32 {
    if live.ready && live.lanes == plan_lanes && live.served_context_window > plan_window {
        live.served_context_window
    } else {
        plan_window
    }
}

/// The #363 wedge-heal floor — [`sticky_served_window`]'s sibling for the case sticky
/// cannot see: the live snapshot was EMPTIED by a wedge declaration (or a cold boot after
/// a crash), so `live.ready` is false and sticky passes the plan through untouched. If a
/// healthy lane with the SAME lane count was observed before, the successor spawns at
/// least that big — the plan's momentary dip is the dying predecessor's memory not yet
/// freed, and that memory is precisely what the successor inherits. Applies ONLY when
/// live is not ready (when it is, sticky already owns the decision) and only same-lanes
/// (a lane-count change legitimately resizes KV).
fn wedge_heal_floor(
    plan_window: u32,
    plan_lanes: u32,
    live_ready: bool,
    last_healthy_window: u32,
    last_healthy_lanes: u32,
) -> u32 {
    if !live_ready && last_healthy_lanes == plan_lanes && last_healthy_window > plan_window {
        last_healthy_window
    } else {
        plan_window
    }
}

/// is unit-tested directly. A live/spawned model is `ready` with the served base
/// url AND the real served window; a degraded reconcile — OR a ready server whose
/// window we could not read (`served_context_window == 0`) — publishes "nothing
/// live" rather than a half-true "ready but no/guessed window"
/// ([[fallbacks-are-illegal-fail-loud]]). The window-0 guard is what keeps a
/// persona from ever binding a broken prompt budget: it would rather see the gap
/// (and the next tick re-reads `/props` via `AlreadyServing` → self-heals) than
/// budget against a zero window.
fn snapshot_from_outcome(
    outcome: &EnsureOutcome,
    desired: &str,
    adapters: &[String],
    served_context_window: u32,
    lanes: u32,
    // The VERIFIED vision endpoint on this node, if any (#106): the main lane
    // itself when its model sees, or the sidecar lane. `Some` ⇒ its `/props`
    // confirmed sight; the snapshot's `vision_ready`/`vision_base_url`/
    // `vision_model` are all projected from this ONE value, so an address can
    // never be published without the verified flag (or vice versa).
    vision: Option<crate::inference::vision_sidecar::SidecarLane>,
    // WHICH engine answered this reconcile — llama.cpp's `/props.build_info`.
    // `None` = nothing served, or a build that cannot say what it is.
    engine_build: Option<String>,
) -> ServingSnapshot {
    match outcome {
        EnsureOutcome::AlreadyServing | EnsureOutcome::Spawned { .. }
            if served_context_window > 0 =>
        {
            ServingSnapshot {
                // A lane with a verified window is serving, not loading.
                loading_model: None,
                // The claim carries the age of its evidence. Stamped HERE, at the
                // moment the reconcile confirmed readiness — not at read time, which
                // is what let `ready:true` survive a SIGKILLed process for as long as
                // nobody republished. A reader compares this against now and decides
                // whether the claim is still worth trusting.
                //
                // `.ok()`, NOT `.unwrap_or(0)`: if the clock is before the epoch we
                // cannot attest to WHEN this was verified, and `Some(0)` would be a
                // fabricated attestation (verified in 1970) rather than an absent one.
                // `None` already means "never confirmed" in this field's contract, so
                // the honest answer is available — take it. Inventing a plausible
                // number is the worse half of swallowing an error: it doesn't just
                // hide the failure, it feeds fiction to every downstream reader.
                ready_verified_at_ms: std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_millis() as u64)
                    .ok(),
                active_model: Some(desired.to_string()),
                ready: true,
                base_url: serving_v1_url(),
                // The genome set now live — feeds the next reconcile's relaunch
                // guard and gives readers the active genome without probing.
                adapters: adapters.to_vec(),
                // The real per-slot window the process serves — every persona
                // budgets its prompt to THIS (task #50, the drift fix).
                served_context_window,
                // The `--parallel` slot count — lets a reader (the resource
                // authority's footprint(), a grid allocator) charge total resident
                // KV as `lanes × kv_at(served_context_window)` (#79).
                lanes,
                degraded_reason: None,
                // The reconcile's verified multimodal verdict (#106): the ONE
                // `vision` value projects all three fields, so readiness and
                // the routing address can never disagree. The observe path
                // gates on THIS, never on a row's declaration alone.
                vision_ready: vision.is_some(),
                vision_base_url: vision.as_ref().map(|v| v.base_url.clone()),
                vision_model: vision.map(|v| v.model_id),
                // The engine's own account of itself, carried so a reader never has
                // to infer it from a binary's mtime (2026-08-09).
                engine_build,
            }
        }
        // Ready outcome but the served window was unreadable (0) → do NOT publish
        // a ready snapshot with a zero window; that would poison every binding
        // persona's budget. Publish "nothing live"; the server stays up and the
        // next reconcile re-reads /props.
        // LOADING. The lane is up (spawned or already serving) but `/props` has not yet
        // given us a real window, so it is not READY. This arm used to return
        // `ServingSnapshot::empty()` and DISCARD `desired` — throwing away the only
        // knowledge of what is being brought up, for the whole load window. Measured on a
        // cold boot 2026-08-19: physical climbed 29.90 → 36.88 GB while serving attributed
        // 0.00 GB, because the consumer had nothing to name. Those bytes read as unowned,
        // and a plan computed in that window sizes against a machine that looks two thirds
        // full of someone else's memory.
        //
        // The bytes exist from spawn, not from readiness. Name the model so the consumer
        // can charge them.
        EnsureOutcome::AlreadyServing | EnsureOutcome::Spawned { .. } => ServingSnapshot {
            loading_model: Some(desired.to_string()),
            ..ServingSnapshot::empty()
        },
        // A Degraded reconcile PUBLISHES its reason — the spawn/probe failure
        // (e.g. a missing llama-server binary, its path in the text) reaches
        // `serving/status` instead of dying as an anonymous empty snapshot
        // (live repro 2026-07-24: Windows spawn failed every tick, status
        // showed null/false with no why).
        EnsureOutcome::Degraded { reason } => ServingSnapshot::degraded(reason.clone()),
    }
}

#[async_trait]
impl ServiceModule for ServingDaemonModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "serving-daemon",
            priority: ModulePriority::Normal,
            command_prefixes: &["serving/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: Some(TICK),
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // Capture the bus so snapshot changes fan out to subscribers (the
        // cbar-stage shape). Set-once; ignore a re-init.
        let _ = self.bus.set(ctx.bus.clone());
        // Install our serving-state watch as the process-wide readable seam so
        // free functions + adapters read "what's live" as a pointer instead of
        // each probing /v1/models. Set-once (singleton daemon).
        let _ = crate::inference::llama_server::install_serving_state(self.subscribe_serving());
        // NOTE: the VRAM prior is seeded and serving is registered as a ResourceConsumer at
        // WIRING time ([`declare_to_memory_authority`], called from `ipc/mod.rs` before
        // `register_planner_on_authority_tick`) — NOT here. `initialize` runs after the module
        // is handed to the runtime, and the authority's tick can fire a plan in between; a plan
        // computed while serving is not yet a registered consumer gets NO add-back for its own
        // residency, which is the #438 `usable_gb = 0` window. Both calls are idempotent, so
        // this is an ordering constraint, not a duplication hazard.
        // Reap orphaned llama-server lanes left by a crashed/SIGKILLed predecessor
        // BEFORE reconciling to the new plan — a mid-eval crash leaves ephemeral
        // lanes (their own scanned ports, no pidfile) holding ~6 GB each with zero
        // reclaim record but the registry. Sweeping first frees that VRAM so the
        // new live lane comes up without competing against dead siblings.
        for outcome in crate::inference::lane_registry::sweep_orphans() {
            crate::probe!(
                class = "serving.lane_registry.sweep",
                outcome = format!("{outcome:?}").as_str(),
                "boot lane-registry sweep",
            );
        }
        // Same reclaim, one level up: a detached benchmark run is a tokio task INSIDE the core,
        // so a restart kills it with no child process to find and no error to report. Any run
        // still marked `running` belonged to the core we just replaced — journal it as
        // killed-by-restart so a poller sees a cause instead of waiting forever on a file that
        // will never appear (#137's train-job shape, applied to benchmarks).
        for run_id in crate::cognition::swe_bench::reap_orphaned_solve_runs() {
            crate::probe!(
                class = "benchmark.swe.orphan_reaped",
                run_id = run_id.as_str(),
                "benchmark run orphaned by a core restart — journaled as failed",
            );
        }
        // Plan once at boot so the decision is published before the first tick,
        // then kick the first reconcile so the server comes up promptly rather
        // than waiting a full tick interval. The reconcile runs detached.
        self.recompute();
        let _ = self.reconcile_to_plan();
        Ok(())
    }

    async fn tick(&self) -> Result<(), String> {
        // The plan is DECIDED on the memory authority's tick now (MEMORY-AUTHORITY-DAEMON:
        // `register_planner_on_authority_tick` runs `recompute()` as an `on_tick` observer,
        // publishing to `plan_tx`) — serving no longer samples memory on its own tick. This
        // tick only RECONCILES: bring the running server in line with the authority's
        // published plan. Fast-to-decide; the slow relaunch spawns off the tick.
        // BEFORE the reconcile, so a wedge reported since the last tick is already published
        // not-ready when the reconcile reads the snapshot — otherwise the escalation waits a
        // full tick behind the very guard it exists to defeat.
        self.take_reported_wedge();
        let _ = self.reconcile_to_plan();
        // Liveness heartbeat (#175 self-heal): on a slow cadence, re-verify that the lane
        // we believe is `ready` can ACTUALLY decode — the reconcile trusts the published
        // `ready` forever and would never notice an OOM-poisoned backend otherwise. Off the
        // tick, hysteresis-gated; a sustained failure flips not-ready and the next reconcile
        // respawns the lane.
        let _ = self.spawn_health_heartbeat_if_due();
        // The cheap knob on the reaction ladder (#56): re-derive the concurrent-prefill
        // grant from the board's LIVE VRAM availability (lock-free watch read, already net
        // of external pressure + grants). Flexes both directions every tick — the instant
        // valve for the 2026-07-16 compute-buffer OOM; re-planning above stays the slow,
        // hysteresis-guarded knob. Ungoverned VRAM (no monitor) → no live number → the
        // throttle holds at the lane count rather than guessing.
        if let Some(available) = governed_vram_ceiling(&self.resource_daemon) {
            // Probes on CHANGE only, inside the throttle — a steady grant is silence.
            let _granted = crate::cognition::prefill_throttle::reconcile(available);
        }
        // #287: for a ready MoE serve, re-derive the governed host-cache lease from the
        // LIVE working set (KV grows as slots fill) and publish it through the sticky
        // band to the per-port plan file — the actuator her ResidencyCache polls.
        self.publish_moe_host_cache_lease();
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // The full `serving/*` surface (plan · status · load · unload) is migrated to
        // the typed registry (`commands/serving/*`, wired via `commands()`). Fail loud
        // — no silent fallback.
        Err(format!(
            "serving-daemon command surface is migrated to the typed registry; \
             '{command}' has no legacy handler"
        ))
    }

    /// The typed `serving/*` family on the ONE command registry, so a persona /
    /// operator / grid peer is OFFERED serving control + inspection as real tools:
    /// the VRAM-axis deallocation pair (`serving/unload` frees a lane, `serving/load`
    /// permits it again) and the two read surfaces (`serving/plan` = intent,
    /// `serving/status` = reality). They share the daemon's suppress-set writer, its
    /// published serving snapshot, and its serving-plan receiver; the daemon's own
    /// plan/reconcile loop turns the suppress-set edits into actual (un)loads.
    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        crate::commands::serving::command_objects(
            self.suppress_sender(),
            self.pin_sender(),
            self.pin_fit_checker(),
            self.subscribe_serving(),
            self.subscribe(),
            self.catalog.clone(),
        )
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

#[cfg(test)]
mod tests {

    // what this catches (2026-08-15 14:13, round-killer L10 / #438 live): a plan tick
    // whose usable_bytes sample was transiently bogus named the SMALLEST candidate
    // (qwen2.5-0.5B) via the nothing-fits arm, and the reconcile swapped the READY
    // 27B lane on that single tick — citizens spoke template-token salad into the
    // bench room as durable messages every healthy peer then perceived as
    // conversation. A base-model swap re-homes minds; it must earn the same
    // sustained streak as the (milder) window re-home, target changes must restart
    // the count, and recovery (no ready incumbent) must never wait.
    #[test]
    fn model_swap_on_a_ready_lane_needs_a_sustained_streak() {
        use super::{model_change_gate, ModelChangeGate};
        use std::sync::atomic::AtomicU32;
        let streak = AtomicU32::new(0);
        let mut pending = None;
        // One bogus tick wanting the 0.5B: deferred, never actuates.
        assert_eq!(
            model_change_gate(true, Some("27b"), "0.5b", &mut pending, &streak, 3),
            ModelChangeGate::Defer { streak: 1 }
        );
        // Next tick the sample healed and the plan wants the incumbent again:
        // pending clears, nothing happened.
        assert_eq!(
            model_change_gate(true, Some("27b"), "27b", &mut pending, &streak, 3),
            ModelChangeGate::NotAChange
        );
        assert!(pending.is_none());
        // Sustained genuine disagreement commits on the Nth consecutive tick.
        assert_eq!(
            model_change_gate(true, Some("27b"), "48b", &mut pending, &streak, 3),
            ModelChangeGate::Defer { streak: 1 }
        );
        assert_eq!(
            model_change_gate(true, Some("27b"), "48b", &mut pending, &streak, 3),
            ModelChangeGate::Defer { streak: 2 }
        );
        assert_eq!(
            model_change_gate(true, Some("27b"), "48b", &mut pending, &streak, 3),
            ModelChangeGate::Commit { streak: 3 }
        );
        // A flapping target restarts the count — 0.5b/48b alternation never commits.
        assert_eq!(
            model_change_gate(true, Some("27b"), "0.5b", &mut pending, &streak, 3),
            ModelChangeGate::Defer { streak: 1 }
        );
        assert_eq!(
            model_change_gate(true, Some("27b"), "48b", &mut pending, &streak, 3),
            ModelChangeGate::Defer { streak: 1 }
        );
        // No ready incumbent (boot / post-wedge): commit immediately — recovery
        // must not wait out a streak.
        let mut p2 = None;
        assert_eq!(
            model_change_gate(false, Some("27b"), "48b", &mut p2, &streak, 3),
            ModelChangeGate::NotAChange
        );
        assert_eq!(
            model_change_gate(true, None, "48b", &mut p2, &streak, 3),
            ModelChangeGate::NotAChange
        );
    }
    use super::*;

    const GB: u64 = 1_000_000_000;

    // what this catches (#287 slice 2): the governed host-cache lease inputs are
    // assembled from the HOST-RESIDENT mass — expert bytes (cache-funded, or hot in
    // VRAM) are subtracted BEFORE any weight-scaled heuristic runs, KV tracks the
    // LIVE window × lanes, and the OS floor derives from the box. K3-on-63GB
    // incident shape: full-file mass would derive a permanent ZERO lease (68GB KV
    // + 41GB buffer estimates off 663GB); the split derives a real budget.
    #[test]
    fn moe_lease_inputs_use_host_resident_mass_not_file_mass() {
        let file = 663 * GB;
        let experts = 640 * GB;
        let physical = 63 * GB;
        let inputs =
            moe_host_cache_lease_inputs("k3", file, experts, 262_144, 4096, 2, physical, None)
                .expect("host share is real");
        assert_eq!(
            inputs.weights_host_bytes,
            23 * GB,
            "dense/attention share only"
        );
        assert_eq!(
            inputs.live_kv_bytes,
            (23 * GB / 80_000) * 4096 * 2,
            "KV law over the HOST mass × live window × lanes"
        );
        assert_eq!(
            inputs.os_floor_bytes,
            physical / 8,
            "floor derives from the box"
        );
        let lease = crate::capacity::host_cache_lease::host_cache_lease_bytes(&inputs);
        assert!(
            lease > 20 * GB && lease < 30 * GB,
            "governed lease lands in a sane band on the incident shape, got {lease}"
        );

        // Blind to the expert split, the working set "costs" the whole file → the
        // lease saturates to zero and the cache is permanently off. The subtraction
        // is load-bearing, not cosmetic.
        let blind =
            moe_host_cache_lease_inputs("k3", file, 0, 262_144, 4096, 2, physical, None).unwrap();
        assert_eq!(
            crate::capacity::host_cache_lease::host_cache_lease_bytes(&blind),
            0,
            "full-file mass must starve the lease — proves why the split exists"
        );

        // Degenerate all-expert read → no honest host footprint → publish nothing.
        assert!(
            moe_host_cache_lease_inputs("k3", file, file, 262_144, 4096, 2, physical, None)
                .is_none()
        );
    }

    // what this catches: a pin SWAPS — serve() kills the incumbent llama-server
    // child, THEN launches the candidate (never co-resident) — so the fit-check must
    // credit the outgoing model's weights back into the budget. Without it, swapping
    // DOWN to a model that fits ALONE but not ALONGSIDE the incumbent is falsely
    // denied (glass-boxed 2026-07-21: pin Devstral 14GB refused while a 20GB 32B
    // teacher was resident, though evicting it frees enough). Regression for the
    // stronger-teacher swap-and-back the Academy needs.
    // what this catches: the swap/restore interleaving bug in the measurement demand
    // override (regression for the #2238 follow-up). With TWO overlapping quiesce
    // leases, releasing in ACQUISITION order under the old swap/restore scheme first
    // restored the pre-lease base over the still-held second lease, then restored the
    // FIRST lease's override — leaving the fleet's warm-slot demand stuck at the
    // measurement value with nobody quiesced. The authority must recompute from what
    // remains: any-order release, base restored only when the LAST override lifts.
    #[test]
    fn overlapping_demand_overrides_release_in_any_order() {
        let cell = Arc::new(AtomicU32::new(0));
        let state = LaneDemandState {
            cell: cell.clone(),
            base: std::sync::Mutex::new(4),
            overrides: std::sync::Mutex::new(Vec::new()),
            next_id: std::sync::atomic::AtomicU64::new(1),
        };
        state.recompute();
        assert_eq!(
            cell.load(Ordering::Relaxed),
            4,
            "base floor before any lease"
        );

        // Overlap: eval quiesce_all (active 0 → floored 1), then a solve's
        // quiesce_others (active 1) while the first is still held.
        let eval = state.acquire(0);
        assert_eq!(cell.load(Ordering::Relaxed), 1);
        let solve = state.acquire(1);
        assert_eq!(cell.load(Ordering::Relaxed), 1);

        // The killer interleaving: release in ACQUISITION order. The solve still
        // holds its lease, so demand must STAY at the measurement value...
        state.release(eval);
        assert_eq!(
            cell.load(Ordering::Relaxed),
            1,
            "first release must not restore the base over a still-held lease"
        );
        // ...and only the LAST release restores the base floor.
        state.release(solve);
        assert_eq!(
            cell.load(Ordering::Relaxed),
            4,
            "last release restores the base — never a stale override"
        );

        // Idempotent: releasing an unknown/stale id changes nothing.
        state.release(solve);
        assert_eq!(cell.load(Ordering::Relaxed), 4);

        // Concurrent measurements with different needs: never starve the larger one.
        let a = state.acquire(2);
        let b = state.acquire(1);
        assert_eq!(cell.load(Ordering::Relaxed), 2, "max of held overrides");
        state.release(a);
        assert_eq!(cell.load(Ordering::Relaxed), 1);
        state.release(b);
        assert_eq!(cell.load(Ordering::Relaxed), 4);
    }

    #[test]
    fn pin_swap_down_credits_the_evicted_incumbents_weights() {
        let base = HostBudget {
            usable_bytes: 12 * GB,
            perf_cores: 10,
        };
        let footprint = |id: &str, weights_gb: u64, rank: u8| ModelFootprint {
            model_id: id.into(),
            weights_bytes: weights_gb * GB,
            kv_per_token: 100_000, // ~0.2GB KV at 2048 ctx — small, not the binding term
            context_window: 32768,
            capability_rank: rank,
        };
        let candidate = footprint("devstral-24b", 14, 8);
        let incumbent = footprint("qwen-32b", 20, 10);

        // No incumbent credited → the 14GB candidate does NOT fit the raw 12GB budget.
        let no_credit = pin_fit_decision(base, Some(candidate.clone()), None);
        assert!(
            no_credit.plan.map(|p| !p.fits_on_gpu).unwrap_or(true),
            "candidate must not fit the raw 12GB budget"
        );

        // Crediting the evicted 20GB incumbent lifts the budget to 32GB → it fits.
        let credited = pin_fit_decision(base, Some(candidate), Some(&incumbent));
        assert_eq!(
            credited.budget_bytes,
            12 * GB + 20 * GB,
            "the evicted incumbent's weights are credited back into the pin budget"
        );
        assert!(
            credited.plan.expect("a plan is produced").fits_on_gpu,
            "candidate must fit its lane once the incumbent is evicted"
        );
    }

    // what this catches: the ServingSteadyHold RAII gauge — acquiring sets
    // serving_held_steady() true; dropping clears it; nesting is reference-counted so a
    // concurrent second eval doesn't release the first's hold. This is the gate that stops
    // the grow-back re-home from relaunching the lane under a running eval (hard-rs 0/8).
    // regression for the 2026-07-20 shared-lane bounce.
    #[test]
    fn serving_steady_hold_is_refcounted_and_raii() {
        assert!(!serving_held_steady(), "no hold at rest");
        {
            let _h1 = ServingSteadyHold::acquire("test");
            assert!(serving_held_steady(), "one hold ⇒ steady");
            {
                let _h2 = ServingSteadyHold::acquire("test");
                assert!(serving_held_steady(), "two holds ⇒ still steady");
            }
            assert!(
                serving_held_steady(),
                "inner drop must not release the outer hold"
            );
        }
        assert!(
            !serving_held_steady(),
            "all holds dropped ⇒ grow-back resumes"
        );
    }

    // what this catches: the ServingLudicrousHold RAII gauge — while held, serving plans at
    // PowerMode::Performance (the whole GPU, biggest window), overriding the timid pressure
    // read; refcounted so concurrent benchmarks compose; RAII-released so serving reverts to
    // the live pressure-adaptive mode. This is the "extreme mode for benchmarks" gate that
    // stops a starved eco boot-window from being what the exam is measured on.
    // [[serving-mode-follows-activity-ludicrous-to-dream]]
    #[test]
    fn serving_ludicrous_hold_is_refcounted_and_raii() {
        assert!(!serving_ludicrous_active(), "no ludicrous demand at rest");
        {
            let _h1 = ServingLudicrousHold::acquire();
            assert!(
                serving_ludicrous_active(),
                "one hold ⇒ Performance override active"
            );
            {
                let _h2 = ServingLudicrousHold::acquire();
                assert!(serving_ludicrous_active(), "two holds ⇒ still active");
            }
            assert!(
                serving_ludicrous_active(),
                "inner drop must not release the outer hold"
            );
        }
        assert!(
            !serving_ludicrous_active(),
            "all holds dropped ⇒ back to pressure-adaptive mode"
        );
    }

    // what this catches: the budget is LIVE (tracks free memory, not capacity),
    // capped at physical VRAM, with headroom, cores floored at 1 — the organic
    // "free memory drops → budget drops" behavior.
    #[test]
    fn host_budget_tracks_live_free_memory() {
        // 40GB free on a 53GB device → budget from the 40, with headroom.
        let b = host_budget_from(&HostBudgetInputs {
            available_bytes: 40 * GB,
            total_vram_bytes: 53 * GB,
            perf_cores: 6,
            budget_fraction: 0.80,
        });
        assert!(b.usable_bytes < 40 * GB, "must reserve headroom");
        assert!(
            b.usable_bytes >= 30 * GB,
            "but most of free is ours: {}",
            b.usable_bytes
        );

        // Organic: less free memory → smaller budget (a game grabbed memory).
        let busy = host_budget_from(&HostBudgetInputs {
            available_bytes: 6 * GB,
            total_vram_bytes: 53 * GB,
            perf_cores: 6,
            budget_fraction: 0.80,
        });
        assert!(
            busy.usable_bytes < b.usable_bytes,
            "less free → smaller budget"
        );

        // Never plan above physical VRAM even if the OS reports more free RAM
        // (unified memory: free RAM can exceed the VRAM serving ceiling).
        let capped = host_budget_from(&HostBudgetInputs {
            available_bytes: 100 * GB,
            total_vram_bytes: 53 * GB,
            perf_cores: 6,
            budget_fraction: 0.80,
        });
        assert!(capped.usable_bytes <= 53 * GB, "capped at physical VRAM");

        assert_eq!(
            host_budget_from(&HostBudgetInputs {
                available_bytes: 8 * GB,
                total_vram_bytes: 8 * GB,
                perf_cores: 0,
                budget_fraction: 0.80,
            })
            .perf_cores,
            1,
            "cores floored at 1"
        );
    }

    // what this catches: footprint estimate is honest about weights (passed
    // through), tool capability bumps the rank, KV is non-zero, and zero
    // weights → no footprint (we only offer what we can actually serve).
    #[test]
    fn kv_divisor_reflects_cache_type_conservatively() {
        // what this catches: the #232 KV-quant fit-math coupling — the served window grows
        // only when the lane actually runs quantized KV, and CONSERVATIVELY so the plan
        // never over-grows past the real KV and OOMs. f16/unset/unknown must never scale.
        assert_eq!(kv_divisor_for(None), 1, "unset never scales the window");
        assert_eq!(
            kv_divisor_for(Some("f16")),
            1,
            "explicit f16 is the no-op default"
        );
        assert_eq!(kv_divisor_for(Some("q8_0")), 2, "q8_0 ~ half of f16");
        assert_eq!(
            kv_divisor_for(Some("  Q8_0 ")),
            2,
            "trimmed + case-insensitive"
        );
        assert_eq!(
            kv_divisor_for(Some("q4_0")),
            3,
            "q4_0 conservative, under the ideal ~3.5x"
        );
        assert_eq!(
            kv_divisor_for(Some("garbage")),
            1,
            "unknown type → no grow, never a bogus OOM"
        );
    }

    #[test]
    fn footprint_from_parts_is_footprint_aware() {
        let fp = footprint_from_parts("present", 3 * GB, 8192, true).unwrap();
        assert_eq!(fp.model_id, "present");
        assert_eq!(fp.weights_bytes, 3 * GB);
        assert!(fp.kv_per_token > 0);
        assert_eq!(
            fp.context_window, 8192,
            "carries the model's trained ceiling, no clamp"
        );
        assert!(
            fp.capability_rank >= 5,
            "3GB + tool bonus, got {}",
            fp.capability_rank
        );

        // A leaner non-tool model ranks below the bigger tool-capable one.
        let small = footprint_from_parts("small", 1 * GB, 4096, false).unwrap();
        assert!(small.capability_rank < fp.capability_rank);

        assert!(
            footprint_from_parts("empty", 0, 8192, false).is_none(),
            "no weights → not servable"
        );
    }

    // what this catches: an M5 Pro (or any capable silicon) must NOT classify
    // as Compat (which would force n_gpu_layers=0 = CPU); unknown hardware
    // stays on the safe LCD/CPU fallback.
    #[test]
    fn detect_tier_classifies_silicon() {
        use crate::persona::hw_tier_descriptor::HwTierCategory;
        assert_eq!(detect_tier("Apple M5 Pro").1, HwTierCategory::MSeriesPro);
        assert_eq!(detect_tier("Apple M2").1, HwTierCategory::MSeries);
        assert_eq!(
            detect_tier("NVIDIA GeForce RTX 5090").1,
            HwTierCategory::Cuda
        );
        assert_eq!(detect_tier("llvmpipe").1, HwTierCategory::Compat);
    }

    // what this catches: the daemon publishes the classifier's decision to its
    // watch channel — Some(plan) with the most-capable fitting model when there
    // are candidates, None (no silent serve) when there are none.
    #[tokio::test]
    async fn publish_plan_drives_the_watch() {
        let gpu = Arc::new(GpuMemoryManager::simulated("Apple M5 Pro", 53 * GB));
        let system = Arc::new(SystemResourceMonitor::new());
        let daemon = ServingDaemonModule::new(gpu, system, test_resource_daemon(), test_catalog());
        let rx = daemon.subscribe();
        assert!(rx.borrow().is_none(), "starts unpublished");

        let budget = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        let candidates = vec![
            footprint_from_parts("small", GB, 4096, false).unwrap(),
            footprint_from_parts("coder-14b", 9 * GB, 8192, true).unwrap(),
        ];
        daemon.publish_plan(budget, &candidates);
        let plan = rx.borrow().clone().expect("plan published");
        assert_eq!(
            plan.base_model.model_id, "coder-14b",
            "most capable that fits"
        );
        assert!(plan.fits_on_gpu);

        // No candidates → None published (no silent serve).
        daemon.publish_plan(budget, &[]);
        assert!(rx.borrow().is_none(), "empty candidates → no plan");
    }

    // what this catches: the `serving/*` surface (plan · status · load · unload) is
    // migrated to the typed registry; the legacy handle_command is gone, so for any
    // command name it must fail loud — never silently fall back to an empty result.
    #[tokio::test]
    async fn legacy_handle_command_fails_loud() {
        let gpu = Arc::new(GpuMemoryManager::simulated("Apple M5 Pro", 53 * GB));
        let system = Arc::new(SystemResourceMonitor::new());
        let daemon = ServingDaemonModule::new(gpu, system, test_resource_daemon(), test_catalog());
        let err = daemon
            .handle_command("serving/plan", serde_json::json!({}))
            .await
            .expect_err("legacy handler must fail loud after migration");
        assert!(
            err.contains("migrated to the typed registry"),
            "error must name the migration: {err}"
        );
    }

    // what this catches: the daemon contributes the full typed serving surface,
    // sharing its OWN watch receivers + catalog (so the read surfaces report the
    // daemon's live decision/snapshot). A regression that drops a verb is caught.
    #[tokio::test]
    async fn contributes_the_full_serving_surface() {
        let gpu = Arc::new(GpuMemoryManager::simulated("Apple M5 Pro", 53 * GB));
        let system = Arc::new(SystemResourceMonitor::new());
        let daemon = ServingDaemonModule::new(gpu, system, test_resource_daemon(), test_catalog());
        let mut names: Vec<&str> = daemon.commands().iter().map(|c| c.name()).collect();
        names.sort_unstable();
        assert_eq!(
            names,
            vec![
                "serving/load",
                "serving/pin",
                "serving/plan",
                "serving/status",
                "serving/unload",
                "serving/unpin"
            ]
        );
    }

    use crate::inference::llama_server::LlamaServerError;
    use std::sync::atomic::AtomicUsize;

    /// Fake serving control: counts serve() calls and reports nothing live
    /// (Unreachable), so reconcile always decides to (re)serve. A daemon-level
    /// stub for the reconcile WIRING; the reconcile DECISION itself is tested in
    /// `inference::llama_server` against its own fake.
    struct FakeServer {
        serves: Arc<AtomicUsize>,
        ok: bool,
        /// Drives [`LlamaServerControl::decode_smoke_ok`] so a test can wedge the lane's
        /// COMPUTE path (probe → false) independently of its control plane. Defaults true
        /// (a healthy lane decodes). See [`FakeServer::healthy`].
        smoke_ok: Arc<AtomicBool>,
        /// The stderr watcher's report channel, as the real `LlamaServerProcess` exposes it.
        /// A test raises this to stand in for a slot printing impossible progress.
        wedge: crate::inference::wedge::WedgeFlag,
        /// Drives [`LlamaServerControl::slots_activity_fingerprint`] (L11). 0 = the
        /// endpoint is unreadable (`None`); any other value is the fingerprint. A test
        /// bumps it to stand in for the serve loop advancing between smoke misses.
        slots_fp: Arc<AtomicU64>,
    }

    impl FakeServer {
        /// A healthy fake: serve() outcome = `ok`, decode heartbeat passes.
        fn healthy(serves: Arc<AtomicUsize>, ok: bool) -> Self {
            Self {
                serves,
                ok,
                smoke_ok: Arc::new(AtomicBool::new(true)),
                wedge: Default::default(),
                slots_fp: Default::default(),
            }
        }
    }

    #[async_trait]
    impl LlamaServerControl for FakeServer {
        fn wedge_flag(&self) -> Option<crate::inference::wedge::WedgeFlag> {
            Some(self.wedge.clone())
        }

        async fn active_model(&self) -> Result<Option<String>, LlamaServerError> {
            Err(LlamaServerError::Unreachable("test: nothing up".into()))
        }
        async fn active_adapters(&self) -> Result<Vec<String>, LlamaServerError> {
            Ok(Vec::new())
        }
        async fn serve(&self, _target: &ServingTarget) -> Result<(), LlamaServerError> {
            self.serves.fetch_add(1, Ordering::SeqCst);
            if self.ok {
                Ok(())
            } else {
                Err(LlamaServerError::Spawn("test boom".into()))
            }
        }
        async fn served_context_window(&self) -> Result<u32, LlamaServerError> {
            // After a successful (test) serve the daemon reads the real per-slot
            // window; a fixed non-zero value stands in for the live `/props` read
            // so the reconcile publishes a READY snapshot carrying a real window.
            Ok(11008)
        }
        async fn served_lanes(&self) -> Result<u32, LlamaServerError> {
            // 0 = "this fake's /props names no slot count", i.e. nothing to compare
            // on the lane axis. These daemon tests assert the window/readiness path,
            // so the lane operand stays inert exactly as it was before it was probed.
            Ok(0)
        }
        async fn decode_smoke_ok(&self) -> bool {
            // Driven by `smoke_ok` so a test can wedge the COMPUTE path (the #175
            // liveness-heartbeat tests) independently of the control plane; defaults
            // true (a healthy fake decodes).
            self.smoke_ok.load(Ordering::Relaxed)
        }
        async fn slots_activity_fingerprint(&self) -> Option<u64> {
            match self.slots_fp.load(Ordering::Relaxed) {
                0 => None,
                v => Some(v),
            }
        }
    }

    /// A minimal [`Model`] for reconcile-wiring tests — only `id` is load-bearing
    /// (the FakeServer ignores the rest); the planned served window rides on the
    /// `ServingTarget`, not here.
    fn fake_model(id: &str) -> Model {
        use crate::model_registry::types::{Arch, MultiPartyChatStrategy};
        Model {
            weights_bytes: None,
            mmproj_bytes: None,
            id: id.to_string(),
            name: None,
            provider: "llamacpp-local".to_string(),
            arch: Arch::Qwen2,
            context_window: 262_144,
            max_output_tokens: 4096,
            tokens_per_second: 0.0,
            capabilities: std::collections::BTreeSet::new(),
            cost_input_per_1k: 0.0,
            cost_output_per_1k: 0.0,
            gguf_hint: None,
            hf_source: None,
            gguf_local_path: None,
            chat_template: None,
            stop_sequences: Vec::new(),
            multi_party_strategy: MultiPartyChatStrategy::ProperChatMlSingleParty,
            mmproj_local_path: None,
            parameter_count: 0,
            sampling: crate::model_registry::types::ModelSampling::default(),
            persona_serving_eligible: true,
        }
    }

    /// A live catalog seeded from the real registry — the shared universe the
    /// daemon plans off. Tests that drive `publish_plan` directly pass candidates
    /// explicitly and never touch it; the reconcile-wiring tests just need it to
    /// exist so the daemon can be constructed.
    fn test_catalog() -> Arc<ModelCatalog> {
        let reg = crate::model_registry::catalog::registry().expect("Rust catalog must validate");
        Arc::new(ModelCatalog::from_registry(&reg))
    }

    /// A resource authority for daemon construction in tests — primed with a
    /// generous mock VRAM ceiling so that if a test ever reaches `host_budget()`
    /// it reads a real governed number rather than the fail-closed 0. Calls
    /// `ResourceDaemon::start`, which spawns its tick task, so every test that
    /// constructs a daemon through `daemon_with` must be `#[tokio::test]`.
    fn test_resource_daemon() -> Arc<ResourceDaemon> {
        use crate::resources::{DaemonConfig, MockCapacitySource};
        ResourceDaemon::start(
            vec![Arc::new(MockCapacitySource::new(
                ResourceKind::Vram,
                53 * GB,
            ))],
            vec![],
            DaemonConfig::default(),
        )
    }

    fn daemon_with(server: Arc<dyn LlamaServerControl>) -> ServingDaemonModule {
        let gpu = Arc::new(GpuMemoryManager::simulated("Apple M5 Pro", 53 * GB));
        let system = Arc::new(SystemResourceMonitor::new());
        let mut daemon = ServingDaemonModule::with_control(
            gpu,
            system,
            test_resource_daemon(),
            server,
            test_catalog(),
        );
        // Resolve any planned id to a fake Model so reconcile can build a
        // ServingTarget without a populated global registry.
        daemon.set_model_resolver(Arc::new(|id: &str| Some(fake_model(id))));
        // NO real decode observed — the fresh-boot state every heartbeat test below assumes.
        // Left to the default this reads a process-global that ANY other test in this binary
        // can stamp (`llama_server`'s `note_real_decode` coverage does), which silently turns
        // the trust short-circuit on and reds these tests by scheduling order alone.
        daemon.set_decode_age_source(Arc::new(|| None));
        // NO inherited lane — the fresh-machine state every plan test below assumes. Left to the
        // default this reads the OPERATOR's real pidfile + lane registry, so a developer with a
        // 27B serving would silently hand every planning test an incumbent it never declared.
        // Same class of leak as the decode-age global above; caught by measurement in
        // `serving_consumer` before it could ever be caught by a red.
        daemon.set_inherited_lane(Arc::new(|| None));
        daemon
    }

    // what this catches: THE slice-6 no-reboot servability contract. A model that
    // is NotDownloaded is NOT offered as a serving candidate; the instant a pull
    // flips it Ready with a real on-disk path (`attach_local_artifact`), the SAME
    // live snapshot yields it as a candidate. Proves serving plans off the LIVE
    // catalog, not the immutable seed — a runtime acquisition becomes servable on
    // the next tick with no restart. Regresses the convergence gap where serving
    // re-derived candidates from `model_registry::global()` and never saw a pull.
    #[test]
    fn pulled_model_becomes_a_candidate_without_reboot() {
        use crate::model_registry::live::ModelStatus;
        use std::io::Write;

        let catalog = test_catalog();
        let id = "slice6-convergence-probe";
        // A fresh local model with no artifact on disk → NotDownloaded.
        catalog.register(
            fake_model(id),
            ModelStatus {
                availability: Availability::NotDownloaded,
                verified: None,
            },
        );

        // Before the pull: not on disk ⇒ never offered for serving.
        let before = candidates_from_snapshot(&catalog.snapshot());
        assert!(
            !before.iter().any(|f| f.model_id == id),
            "a NotDownloaded model must never be a serving candidate"
        );

        // The pull lands the artifact: a real, non-empty GGUF on disk.
        let mut gguf = tempfile::Builder::new()
            .suffix(".gguf")
            .tempfile()
            .expect("temp gguf");
        gguf.write_all(&[0u8; 4096]).expect("write weights");
        gguf.flush().expect("flush");
        assert!(
            catalog.attach_local_artifact(id, gguf.path().to_path_buf(), None),
            "model is present, artifact attaches + flips Ready"
        );

        // After the pull: the SAME live snapshot now yields it as a candidate,
        // with the footprint reflecting the real on-disk bytes — no reboot.
        let after = candidates_from_snapshot(&catalog.snapshot());
        let found = after.iter().find(|f| f.model_id == id).expect(
            "a freshly-pulled Ready model becomes a serving candidate on the next snapshot",
        );
        assert_eq!(
            found.weights_bytes, 4096,
            "footprint counts the real bytes that will be loaded"
        );
    }

    // what this catches: the VRAM-axis deallocation. `serving/unload` inserts a
    // model id into the daemon's suppress-set; `live_candidates` must then EXCLUDE
    // that model even though it is Ready on disk, so the planner can no longer pick
    // it and the next reconcile frees its lane. `serving/load` (un-suppress)
    // restores it as a candidate. Without this filter an unloaded model would be
    // re-planned on the very next tick and its VRAM would never free — the
    // imperative-suppress contract the operator chose.
    #[tokio::test]
    async fn live_candidates_honors_the_suppress_set() {
        use crate::model_registry::live::ModelStatus;
        use std::io::Write;

        let serves = Arc::new(AtomicUsize::new(0));
        let daemon = daemon_with(Arc::new(FakeServer::healthy(serves, true)));
        let id = "suppress-probe";

        // Land a Ready model so it IS a candidate to begin with.
        let mut gguf = tempfile::Builder::new()
            .suffix(".gguf")
            .tempfile()
            .expect("temp gguf");
        gguf.write_all(&[0u8; 4096]).expect("write weights");
        gguf.flush().expect("flush");
        daemon.catalog.register(
            fake_model(id),
            ModelStatus {
                availability: Availability::NotDownloaded,
                verified: None,
            },
        );
        assert!(
            daemon
                .catalog
                .attach_local_artifact(id, gguf.path().to_path_buf(), None),
            "ready model lands on disk"
        );
        assert!(
            daemon.live_candidates().iter().any(|f| f.model_id == id),
            "a Ready model is a candidate before unload"
        );

        // serving/unload: pin it OFF → excluded from candidates → lane frees.
        daemon.suppress_sender().send_modify(|s| {
            Arc::make_mut(s).insert(id.to_string());
        });
        assert!(
            !daemon.live_candidates().iter().any(|f| f.model_id == id),
            "a suppressed model is excluded → planner drops it → VRAM frees"
        );

        // serving/load: permit it again → returns as a candidate (planner decides).
        daemon.suppress_sender().send_modify(|s| {
            Arc::make_mut(s).remove(id);
        });
        assert!(
            daemon.live_candidates().iter().any(|f| f.model_id == id),
            "an un-suppressed model returns as a candidate"
        );
    }

    // what this catches: #142 — the autonomic planner conscripting a benchmark
    // OPPONENT as the citizens' model. A Ready GGUF whose catalog row opts out
    // (`persona_serving_eligible: false`) must be invisible to the autonomic
    // plan (the tick picked Hermes-4.3 twice, 2026-07-12, purely because it was
    // the largest Ready artifact), while an explicit `serving/pin` — operator
    // consent — still serves it (how the benchmark matrix brings one up).
    #[tokio::test]
    async fn ineligible_rows_never_join_the_autonomic_plan_but_pin_bypasses() {
        use crate::model_registry::live::ModelStatus;
        use std::io::Write;

        let serves = Arc::new(AtomicUsize::new(0));
        let daemon = daemon_with(Arc::new(FakeServer::healthy(serves, true)));
        let id = "opponent-probe";

        let mut opponent = fake_model(id);
        opponent.persona_serving_eligible = false;
        let mut gguf = tempfile::Builder::new()
            .suffix(".gguf")
            .tempfile()
            .expect("temp gguf");
        gguf.write_all(&[0u8; 4096]).expect("write weights");
        gguf.flush().expect("flush");
        daemon.catalog.register(
            opponent,
            ModelStatus {
                availability: Availability::NotDownloaded,
                verified: None,
            },
        );
        assert!(
            daemon
                .catalog
                .attach_local_artifact(id, gguf.path().to_path_buf(), None),
            "opponent GGUF lands on disk and flips Ready"
        );

        // Ready on disk, but ineligible → the autonomic path must never see it.
        assert!(
            !daemon.live_candidates().iter().any(|f| f.model_id == id),
            "an ineligible Ready model is excluded from the autonomic plan"
        );

        // Explicit pin = operator consent → eligibility is bypassed.
        daemon.pin_sender().send_replace(Some(id.to_string()));
        assert!(
            daemon.live_candidates().iter().any(|f| f.model_id == id),
            "a pinned ineligible model serves — pin is consent"
        );

        // Unpin → back off the autonomic plan.
        daemon.pin_sender().send_replace(None);
        assert!(
            !daemon.live_candidates().iter().any(|f| f.model_id == id),
            "unpin returns the opponent to benchmark-only invisibility"
        );
    }

    // what this catches: the EnsureOutcome → ServingSnapshot mapping. A live or
    // spawned model is ready with the served base url AND the real per-slot window
    // it carries through to personas; a degraded reconcile — OR a ready server
    // whose served window was unreadable (0) — publishes "nothing live", never a
    // half-true ready-with-no-model or ready-with-zero-window (the drift fix:
    // budgeting against a zero/guessed window is exactly what we refuse).
    #[test]
    fn snapshot_mapping_is_honest() {
        let genes = vec!["/genes/a.gguf".to_string()];
        let up = snapshot_from_outcome(
            &EnsureOutcome::Spawned { model: "m".into() },
            "coder-14b",
            &genes,
            11008,
            4,
            Some(crate::inference::vision_sidecar::SidecarLane {
                base_url: "http://127.0.0.1:58091/v1".to_string(),
                model_id: "vl-7b".to_string(),
            }),
            Some("b6789-a28ee566c".to_string()),
        );
        assert_eq!(up.active_model.as_deref(), Some("coder-14b"));
        assert!(up.ready);
        assert!(up.base_url.ends_with("/v1"));
        assert_eq!(
            up.adapters, genes,
            "live snapshot carries the loaded genome set"
        );
        assert_eq!(
            up.served_context_window, 11008,
            "ready snapshot carries the real per-slot window personas budget to"
        );
        assert_eq!(
            up.lanes, 4,
            "ready snapshot carries the --parallel lane count for total-KV accounting"
        );
        assert!(
            up.vision_ready,
            "the reconcile's verified multimodal verdict must survive into the snapshot \
             — the observe path gates on THIS field (#106)"
        );
        assert_eq!(
            up.vision_base_url.as_deref(),
            Some("http://127.0.0.1:58091/v1"),
            "the ONE vision value projects the routing address with the flag — an \
             address can never publish without verified readiness"
        );
        assert_eq!(up.vision_model.as_deref(), Some("vl-7b"));
        // what this catches (2026-08-09): dropping the engine's own identity on the
        // way to the snapshot. `/props` carries `build_info` and the daemon reads it,
        // but if it does not SURVIVE to here, "which engine is running?" falls back to
        // comparing binary mtimes across machines — which is how a Rust CORE build
        // number got read as the engine's and misattributed a wedge.
        assert_eq!(
            up.engine_build.as_deref(),
            Some("b6789-a28ee566c"),
            "the engine's own build_info must reach the published snapshot"
        );

        let already = snapshot_from_outcome(
            &EnsureOutcome::AlreadyServing,
            "coder-14b",
            &genes,
            11008,
            4,
            None,
            // An engine too old to publish `build_info` — serving is unaffected.
            None,
        );
        assert_eq!(already.active_model.as_deref(), Some("coder-14b"));
        assert!(already.ready);
        assert_eq!(already.served_context_window, 11008);
        assert_eq!(already.lanes, 4);
        assert!(
            !already.vision_ready,
            "a text lane (no verified endpoint) must never read as sighted"
        );
        assert!(
            already.vision_base_url.is_none() && already.vision_model.is_none(),
            "no verified endpoint → no address, no model (None-iff-not-ready)"
        );
        // what this catches: inventing an identity for an engine that did not give
        // one. A build too old to publish `build_info` must read as UNKNOWN, never as
        // a plausible-looking string a reader would then try to look up.
        assert_eq!(
            already.engine_build, None,
            "an engine that cannot say what it is reads as unknown, not as a guess"
        );

        // Ready outcome but the served window was unreadable (0) → publish the gap,
        // NOT a ready snapshot with a zero window a persona would budget against.
        let windowless = snapshot_from_outcome(
            &EnsureOutcome::Spawned { model: "m".into() },
            "coder-14b",
            &genes,
            0,
            4,
            None,
            Some("b6789-a28ee566c".to_string()),
        );
        assert_eq!(
            windowless.active_model, None,
            "ready-but-no-window → nothing live"
        );
        assert!(!windowless.ready);
        assert_eq!(windowless.served_context_window, 0);
        assert_eq!(windowless.lanes, 0, "empty snapshot carries no lanes");
        // what this catches: a snapshot that says nothing is live while still naming
        // an engine. Both halves would be read together ("not live, but running
        // b6789?"), and the contradiction is worse than the absence — a reader would
        // reasonably conclude the lane is up and the flag is stale.
        assert_eq!(
            windowless.engine_build, None,
            "a not-live snapshot claims no engine, even when one answered the probe"
        );

        let degraded = snapshot_from_outcome(
            &EnsureOutcome::Degraded { reason: "x".into() },
            "coder-14b",
            &genes,
            11008,
            4,
            None,
            Some("b6789-a28ee566c".to_string()),
        );
        assert_eq!(degraded.active_model, None, "degraded → nothing live");
        assert!(!degraded.ready);
        assert!(degraded.adapters.is_empty(), "degraded → no genome claimed");
        // regression for the 2026-07-24 Windows repro: the spawn-failure reason
        // must SURVIVE into the published snapshot — serving/status saying only
        // null/false while spawn fails every tick is the silent-failure lie.
        assert_eq!(
            degraded.degraded_reason.as_deref(),
            Some("x"),
            "degraded reason must reach the snapshot"
        );
        assert_eq!(
            windowless.degraded_reason, None,
            "a windowless-ready snapshot is not degraded — no reason claimed"
        );
    }

    // what this catches (#175 sticky window): the LoRA-load relaunch cascade must not
    // ratchet the per-slot window down and strand earlier-pinned personas. A same-lane
    // genome relaunch KEEPS the larger incumbent window; a lane-count change lets the
    // plan window through (resizing KV is legitimate; keeping the old window across a
    // lane increase would OOM); a larger plan window is never held down (the
    // starved-grow check owns growing); nothing-serving-yet → the plan window stands.
    #[test]
    fn sticky_window_holds_the_incumbent_only_on_a_same_lane_relaunch() {
        let live = |ready: bool, window: u32, lanes: u32| ServingSnapshot {
            ready,
            served_context_window: window,
            lanes,
            ..ServingSnapshot::empty()
        };
        assert_eq!(
            sticky_served_window(31_744, 2, &live(true, 49_664, 2)),
            49_664
        );
        assert_eq!(
            sticky_served_window(31_744, 2, &live(true, 49_664, 1)),
            31_744
        );
        assert_eq!(
            sticky_served_window(60_000, 2, &live(true, 49_664, 2)),
            60_000
        );
        assert_eq!(sticky_served_window(31_744, 2, &live(false, 0, 0)), 31_744);
    }

    // what this catches (#363 wedge-heal floor): a wedge declaration EMPTIES the live
    // snapshot, which disarms the sticky floor above — so the very relaunch the wedge
    // detector triggers used to spawn at a teardown-transient plan (measured live: plan
    // dipped to 9,984 while the dying 34,048 predecessor still held its memory; every
    // subsequent 12k prompt was rejected). The successor must inherit the last HEALTHY
    // window when lanes are unchanged; a live-ready lane defers to sticky; a lane-count
    // change resizes legitimately; and no-history (cold first boot) passes the plan.
    #[test]
    fn wedge_heal_floor_holds_the_last_healthy_window_across_the_empty_snapshot() {
        // The measured blackout shape: not ready (wedge emptied it), same lanes,
        // transient plan far below the last healthy window → floored.
        assert_eq!(wedge_heal_floor(9_984, 2, false, 34_048, 2), 34_048);
        // Live lane still ready → sticky owns the decision; floor stands down.
        assert_eq!(wedge_heal_floor(9_984, 2, true, 34_048, 2), 9_984);
        // Lane-count change legitimately resizes KV → plan through.
        assert_eq!(wedge_heal_floor(9_984, 4, false, 34_048, 2), 9_984);
        // Plan already >= last healthy → never held down.
        assert_eq!(wedge_heal_floor(41_216, 2, false, 34_048, 2), 41_216);
        // No healthy lane ever observed (cold boot) → plan through.
        assert_eq!(wedge_heal_floor(9_984, 2, false, 0, 0), 9_984);
    }

    /// The #438 boot geometry, in the shape taken from the live ledger (2026-08-19): a 27B whose
    /// weights are ~19 GB is still resident from the previous generation, so the successor's
    /// first sample of free VRAM reads ~6 GB. Only something small fits that; the 27B does not.
    fn boot_squeeze() -> (HostBudget, Vec<ModelFootprint>) {
        (
            HostBudget {
                usable_bytes: 6 * GB,
                perf_cores: 6,
            },
            vec![
                footprint_from_parts("qwen3-27b", 19 * GB, 8192, true).unwrap(),
                footprint_from_parts("coder-4b", 3 * GB, 8192, true).unwrap(),
            ],
        )
    }

    fn inherited_27b() -> LaneRecord {
        LaneRecord {
            pid: 11341,
            port: 58057,
            role: crate::inference::lane_registry::LaneRole::Live,
            model: "qwen3-27b".into(),
            context_window: 25_075,
            lanes: 4,
        }
    }

    // what this catches: THE #438 boot downshift, measured live 2026-08-19. A successor core
    // boots while its predecessor's 27B still holds ~45 GB, so the first plan honestly samples
    // ~6 GB free and — with no incumbent — plainly selects the biggest thing that fits: a 14B at
    // the floor. The predecessor was then reclaimed 1.2s later, freeing memory the plan had
    // already spent. Counting the inherited lane as the incumbent re-arms the at-rest credit
    // that exists for exactly this ("a model's OWN residency can never flap it out"), so the
    // 27B holds. Regression here = every crash-and-restart silently demotes the served model.
    #[tokio::test]
    async fn a_lane_inherited_from_a_previous_generation_is_the_incumbent_the_first_plan_defends() {
        let mut daemon = daemon_with(Arc::new(FakeServer::healthy(
            Arc::new(AtomicUsize::new(0)),
            true,
        )));
        daemon.set_inherited_lane(Arc::new(|| Some(inherited_27b())));

        let (budget, candidates) = boot_squeeze();
        daemon.publish_plan(budget, &candidates);

        let plan = daemon.plan_tx.borrow().clone().expect("a plan");
        assert_eq!(
            plan.base_model.model_id, "qwen3-27b",
            "the inherited 27B is ours and its bytes are ours to reclaim — the successor must \
             not flee its own predecessor onto a 14B"
        );
    }

    // what this catches: the NEGATIVE control for the test above — proof the fixture really is
    // a squeeze and the assertion is not vacuous. Same budget, same candidates, no inherited
    // lane: the planner correctly downshifts, because on a genuinely 6 GB machine a 27B does
    // not fit. If this ever also returns the 27B, the test above proves nothing.
    #[tokio::test]
    async fn without_a_past_form_of_ourself_a_six_gigabyte_box_really_does_downshift() {
        let daemon = daemon_with(Arc::new(FakeServer::healthy(
            Arc::new(AtomicUsize::new(0)),
            true,
        )));

        let (budget, candidates) = boot_squeeze();
        daemon.publish_plan(budget, &candidates);

        let plan = daemon.plan_tx.borrow().clone().expect("a plan");
        assert_eq!(
            plan.base_model.model_id, "coder-4b",
            "with no incumbent to credit, 6 GB genuinely cannot hold a 19 GB model"
        );
    }

    // what this catches: precedence. Once THIS core has published a decision, its own plan is
    // the incumbent and a stale registry record must never override it — otherwise a leftover
    // .lane file would pin the model forever against every later replan.
    #[test]
    fn a_published_plan_outranks_an_inherited_lane() {
        assert_eq!(
            incumbent_for_plan(Some("coder-14b".into()), Some(&inherited_27b())).as_deref(),
            Some("coder-14b"),
        );
        // …and with nothing published, the inherited lane speaks.
        assert_eq!(
            incumbent_for_plan(None, Some(&inherited_27b())).as_deref(),
            Some("qwen3-27b"),
        );
        // …and a true cold boot still has no incumbent at all.
        assert_eq!(incumbent_for_plan(None, None), None);
    }

    // what this catches: a published plan drives a reconcile that brings the
    // server up and publishes a ready ServingSnapshot for that model — the
    // plan→reality wiring. Regression here = the daemon decides but never acts.
    #[tokio::test]
    async fn reconcile_brings_planned_model_up() {
        let serves = Arc::new(AtomicUsize::new(0));
        let daemon = daemon_with(Arc::new(FakeServer::healthy(serves.clone(), true)));

        // Publish a plan (most-capable fitting model = coder-14b).
        let budget = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true).unwrap()];
        daemon.publish_plan(budget, &candidates);

        let handle = daemon
            .reconcile_to_plan()
            .expect("a reconcile should be spawned");
        handle.await.unwrap();

        let snap = daemon.subscribe_serving().borrow().clone();
        assert_eq!(snap.active_model.as_deref(), Some("coder-14b"));
        assert!(snap.ready);
        assert_eq!(serves.load(Ordering::SeqCst), 1, "served exactly once");
    }

    // what this catches: once the desired model is ready, a subsequent reconcile
    // is a NO-OP (no relaunch) — otherwise every tick would thrash the
    // GPU-warm server.
    #[tokio::test]
    async fn reconcile_is_noop_when_already_serving() {
        let serves = Arc::new(AtomicUsize::new(0));
        let daemon = daemon_with(Arc::new(FakeServer::healthy(serves.clone(), true)));

        // Pretend coder-14b is already up and ready.
        let _ = daemon.serving_tx.send_replace(ServingSnapshot {
            // test fixture: no live readiness was ever CONFIRMED here.
            ready_verified_at_ms: None,
            active_model: Some("coder-14b".into()),
            ready: true,
            base_url: serving_v1_url(),
            adapters: Vec::new(),
            served_context_window: 11008,
            lanes: 4,
            degraded_reason: None,
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
            // test fixture: no engine identity claimed.
            engine_build: None,
            loading_model: None,
        });
        let budget = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true).unwrap()];
        daemon.publish_plan(budget, &candidates);

        assert!(
            daemon.reconcile_to_plan().is_none(),
            "already serving → no reconcile"
        );
        assert_eq!(serves.load(Ordering::SeqCst), 0, "no relaunch");
    }

    /// A lane whose live per-slot window sits at `live_pct` of what the plan
    /// actually affords, ready to have `reconcile_to_plan` driven at it repeatedly.
    /// Returns the daemon and the REAL planned window.
    ///
    /// Two things this fixture exists to get right, both of which a hand-written
    /// pair of numbers gets wrong:
    ///
    /// 1. A plan is capped by MEASURED demand ([`ServingDemand`]). With nothing
    ///    recorded it falls back to [`BOOTSTRAP_WORKING_SET`] — 16,384, which is
    ///    exactly the number the live lane was stranded at, so a fixture that skips
    ///    this reproduces a plan == live and no gain to sustain at all.
    /// 2. The planner is free to serve less than the model's max context (host fit,
    ///    lane split). Asserting against a window we merely ASKED for tests the
    ///    fixture; deriving the live window from the plan we GOT tests the guard.
    fn lane_under_plan(
        serves: Arc<AtomicUsize>,
        plan_model_ctx: u32,
        live_pct: u32,
    ) -> (ServingDaemonModule, u32) {
        let daemon = daemon_with(Arc::new(FakeServer::healthy(serves, true)));
        // A measured peak is what lets a plan exceed the cold-start prior.
        daemon
            .working_set()
            .record_in_memory(uuid::Uuid::new_v4(), plan_model_ctx, 0);
        let budget = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        let candidates =
            vec![footprint_from_parts("coder-14b", 9 * GB, plan_model_ctx, true).unwrap()];
        daemon.publish_plan(budget, &candidates);
        let plan_window = daemon
            .plan_tx
            .borrow()
            .as_ref()
            .expect("plan published")
            .served_context_window;
        let _ = daemon.serving_tx.send_replace(ServingSnapshot {
            ready_verified_at_ms: None,
            active_model: Some("coder-14b".into()),
            ready: true,
            base_url: serving_v1_url(),
            adapters: Vec::new(),
            served_context_window: plan_window * live_pct / 100,
            lanes: 4,
            degraded_reason: None,
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
            // test fixture: no engine identity claimed.
            engine_build: None,
            loading_model: None,
        });
        (daemon, plan_window)
    }

    // what this catches: the failure the OLD single-sample `live * 2 <= plan` rule
    // could not see. Measured live 2026-08-06: plan 26,323 vs lane 16,384 — a 62%
    // lane, comfortably above half — held indefinitely while every persona's binding
    // read the stale window and the demand loop's output was silently discarded.
    // A real, PERSISTENT gain must eventually re-home no matter where it sits in the
    // 50-99% band. (BigMama's call; she owns the guard the ratio came from.)
    #[tokio::test]
    async fn a_sustained_gain_above_the_old_half_bar_eventually_re_homes() {
        let serves = Arc::new(AtomicUsize::new(0));
        // 62% of plan — the live incident's exact ratio, comfortably above the old
        // half bar and therefore invisible to the rule this replaces.
        let (daemon, plan_window) = lane_under_plan(serves.clone(), 65_536, 62);
        // Guard the FIXTURE, not just the guard: if a measured demand had not raised
        // the plan above the cold-start prior, plan == live, there would be no gain to
        // sustain, and every assertion below would pass for the wrong reason. That is
        // exactly how this test failed its first time out.
        assert!(
            plan_window > daemon.serving_tx.borrow().served_context_window,
            "fixture is vacuous: the plan must actually exceed the lane"
        );

        // Below the streak it must hold — a relaunch kills in-flight turns, so an
        // instant re-home on first sight is exactly the thrash we are avoiding.
        for tick in 1..REHOME_SUSTAINED_TICKS {
            assert!(
                daemon.reconcile_to_plan().is_none(),
                "tick {tick}: must not re-home before the gain has proven itself sustained"
            );
        }
        assert_eq!(
            serves.load(Ordering::SeqCst),
            0,
            "no relaunch during the streak"
        );

        assert!(
            daemon.reconcile_to_plan().is_some(),
            "a gain that persisted for a full lane-readiness window MUST re-home — \
             this is the 62%-forever case the 2x ratio never caught"
        );
    }

    // what this catches: the property the 2x ratio was RIGHT about, which the
    // replacement must not lose. A plan that jitters above the bar and falls back
    // must never accumulate its way to a relaunch — the outage this guard exists for
    // was a lane relaunching against a plan wandering 3.6k<->22k.
    #[tokio::test]
    async fn a_jittering_gain_never_accumulates_into_a_re_home() {
        let serves = Arc::new(AtomicUsize::new(0));
        let (daemon, plan_window) = lane_under_plan(serves.clone(), 65_536, 62);
        let live_window = daemon.serving_tx.borrow().served_context_window;
        let budget = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };

        // Alternate: plenty of headroom, then none. Far MORE ticks than the streak
        // needs in total — but never consecutively.
        for _ in 0..(REHOME_SUSTAINED_TICKS * 3) {
            assert!(
                daemon.reconcile_to_plan().is_none(),
                "wanting: still below streak"
            );
            // A dip: the plan momentarily affords no more than the lane already has.
            let small = vec![footprint_from_parts("coder-14b", 9 * GB, live_window, true).unwrap()];
            daemon.publish_plan(budget, &small);
            assert!(daemon.reconcile_to_plan().is_none(), "dip: nothing to gain");
            // …and back up.
            let big = vec![footprint_from_parts("coder-14b", 9 * GB, plan_window, true).unwrap()];
            daemon.publish_plan(budget, &big);
        }
        assert_eq!(
            serves.load(Ordering::SeqCst),
            0,
            "jitter must NEVER re-home, however long it goes on — one dip resets the streak"
        );
    }

    // what this catches: the cooldown collapsing into the sustained-delta test. The
    // two answer different questions — "is the gain real?" vs "may we pay for it
    // yet?" — and a relaunch kills every in-flight turn on the lane, so the rate
    // limit has to hold even when the evidence is perfect. If the cooldown were
    // merely "reset the streak", 3 more qualifying ticks would re-fire in 15s
    // instead of 90. (BigMama's requirement 1 of 2, 2026-08-06.)
    #[tokio::test]
    async fn a_second_re_home_waits_out_the_cooldown_however_good_the_evidence_is() {
        let serves = Arc::new(AtomicUsize::new(0));
        let (daemon, _plan) = lane_under_plan(serves.clone(), 65_536, 62);
        let starved = daemon.serving_tx.borrow().clone();

        let mut decision = None;
        for _ in 0..REHOME_SUSTAINED_TICKS {
            decision = daemon.reconcile_to_plan();
        }
        decision
            .expect("evidence qualifies by the third tick")
            .await
            .unwrap();
        assert_eq!(
            serves.load(Ordering::SeqCst),
            1,
            "the first re-home fires on evidence"
        );

        // Re-pin the lane to its starved window: the relaunch is faked, so nothing
        // actually resized. That makes the gain as compelling on every following tick
        // as it was on the first — only the cooldown stands between us and a storm.
        let _ = daemon.serving_tx.send_replace(starved);
        for tick in 0..REHOME_COOLDOWN_TICKS {
            assert!(
                daemon.reconcile_to_plan().is_none(),
                "cooldown tick {tick}: perfect evidence must still not buy a second relaunch"
            );
        }
        assert_eq!(
            serves.load(Ordering::SeqCst),
            1,
            "exactly one relaunch per cooldown window"
        );

        // Cooldown spent — the gain must now re-prove itself from zero before firing.
        for tick in 0..(REHOME_SUSTAINED_TICKS - 1) {
            assert!(
                daemon.reconcile_to_plan().is_none(),
                "post-cooldown tick {tick}: the streak was lost while cooling, re-prove it"
            );
        }
        assert!(
            daemon.reconcile_to_plan().is_some(),
            "once cooled AND re-proven, the still-stranded lane re-homes again"
        );
    }

    // what this catches: a margin expressed as an absolute token count. The same
    // 2,048-token shortfall is a third of a small lane and rounding error on a large
    // one — an absolute bar means something different at every window size, so the
    // relative margin is the property, not an implementation detail of it.
    #[tokio::test]
    async fn the_margin_is_relative_so_the_same_shortfall_decides_differently_by_lane_size() {
        // A lane at 93% of plan: the shortfall is ~7.5% of what it serves — real
        // tokens, but under the bar however long it persists.
        let quiet = Arc::new(AtomicUsize::new(0));
        let (near_plan, _) = lane_under_plan(quiet.clone(), 65_536, 93);
        for _ in 0..(REHOME_SUSTAINED_TICKS * 3) {
            near_plan.reconcile_to_plan();
        }
        assert_eq!(
            quiet.load(Ordering::SeqCst),
            0,
            "a 7.5% shortfall is not worth killing in-flight turns for, however long it holds"
        );

        // A lane at 67% of the SAME plan: the shortfall is ~49% of what it serves —
        // half a mind again, a step change.
        let loud = Arc::new(AtomicUsize::new(0));
        let (far_below, _) = lane_under_plan(loud.clone(), 65_536, 67);
        let mut decision = None;
        for _ in 0..REHOME_SUSTAINED_TICKS {
            decision = far_below.reconcile_to_plan();
        }
        decision.expect("a 49% shortfall qualifies").await.unwrap();
        assert_eq!(
            loud.load(Ordering::SeqCst),
            1,
            "half the lane's own window again IS a step change — re-home"
        );
    }

    /// Believe-ready snapshot fixture for the liveness-heartbeat tests.
    fn ready_snapshot() -> ServingSnapshot {
        ServingSnapshot {
            // test fixture: no live readiness was ever CONFIRMED here.
            ready_verified_at_ms: None,
            active_model: Some("coder-14b".into()),
            ready: true,
            base_url: serving_v1_url(),
            adapters: Vec::new(),
            served_context_window: 11008,
            lanes: 4,
            degraded_reason: None,
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
            // test fixture: no engine identity claimed.
            engine_build: None,
            loading_model: None,
        }
    }

    // what this catches: THE WIRING. The 2026-08-05 outage was not a missing detector — the
    // reap actuator already existed and was tested. What was missing was anything that CALLED
    // it, and a detector that reports into a void is indistinguishable from no detector at
    // all. This drives the whole chain the live system uses: the stderr watcher raises the
    // flag → the daemon tick takes it → the lane is published NOT-ready.
    //
    // The not-ready assertion is the load-bearing half. Arming `force_relaunch` alone is
    // silently useless: `reconcile_to_plan` returns early on `ready && same model && same
    // adapters` BEFORE it ever reads the flag, so a wedged-but-"ready" lane would sit there
    // exactly as it did for four hours. regression for the 172 GB log outage
    #[tokio::test]
    async fn a_reported_wedge_flips_the_lane_not_ready() {
        let fake = Arc::new(FakeServer::healthy(Arc::new(AtomicUsize::new(0)), true));
        let daemon = daemon_with(fake.clone());
        let _ = daemon.serving_tx.send_replace(ready_snapshot());

        // Nothing reported yet: a healthy lane is left alone.
        daemon.take_reported_wedge();
        assert!(
            daemon.serving_tx.borrow().ready,
            "an unreported lane must never be reaped"
        );

        // The stderr watcher sees `progress = 1.10` four times and raises.
        fake.wedge.raise();
        daemon.take_reported_wedge();

        assert!(
            !daemon.serving_tx.borrow().ready,
            "a reported wedge must publish NOT-ready, or the reconcile's no-op guard \
             returns before it ever consumes the force-relaunch flag"
        );
        assert!(
            daemon.force_relaunch.load(Ordering::Acquire),
            "the reconcile must be told to re-prove decode on a child we own"
        );

        // The report is consumed exactly once — a second tick must not re-escalate a lane
        // that is already being relaunched.
        let _ = daemon.serving_tx.send_replace(ready_snapshot());
        daemon.take_reported_wedge();
        assert!(
            daemon.serving_tx.borrow().ready,
            "take clears the flag; one report is one escalation"
        );
    }

    // what this catches: #175 self-heal (detection + recovery). A lane the daemon believes
    // is `ready` but whose COMPUTE path is wedged — the decode heartbeat fails while the
    // control plane still 200s, the exact Metal-OOM-poison shape — is flipped NOT-ready
    // after HEALTH_FAILS_TO_RELAUNCH consecutive heartbeats (which the reconcile then turns
    // into a kill+respawn). Before this the owned lane was trusted forever and the persona
    // substrate stayed bricked until a human reboot. regression for #175
    #[tokio::test]
    async fn health_heartbeat_flips_ready_after_sustained_decode_failure() {
        let serves = Arc::new(AtomicUsize::new(0));
        let smoke = Arc::new(AtomicBool::new(false)); // wedged compute path
        let daemon = daemon_with(Arc::new(FakeServer {
            serves,
            ok: true,
            smoke_ok: smoke.clone(),
            wedge: Default::default(),
                slots_fp: Default::default(),
        }));
        let _ = daemon.serving_tx.send_replace(ready_snapshot());

        // First heartbeat (tick 0) fails once — hysteresis holds, still ready.
        if let Some(h) = daemon.spawn_health_heartbeat_if_due() {
            h.await.unwrap();
        }
        assert!(
            daemon.serving_tx.borrow().ready,
            "one failed probe is not enough — a merely-busy lane must not be reaped"
        );

        // Force the NEXT call to be a heartbeat tick; the second failure reaches threshold.
        daemon.health_ticks.store(0, Ordering::Relaxed);
        if let Some(h) = daemon.spawn_health_heartbeat_if_due() {
            h.await.unwrap();
        }
        let s = daemon.serving_tx.borrow();
        assert!(
            !s.ready,
            "sustained decode failure flips the lane not-ready so the reconcile respawns it (#175)"
        );
        assert!(
            s.active_model.is_none(),
            "not-ready is published as the empty gap the reconcile relaunches from"
        );
    }

    // what this catches: hysteresis RESET. A lane that fails a heartbeat once then RECOVERS
    // (probe passes) must have its failure streak cleared — so an isolated failure minutes
    // later never accumulates with a stale one into a spurious reap.
    #[tokio::test]
    async fn health_heartbeat_streak_resets_on_a_passing_probe() {
        let serves = Arc::new(AtomicUsize::new(0));
        let smoke = Arc::new(AtomicBool::new(false));
        let daemon = daemon_with(Arc::new(FakeServer {
            serves,
            ok: true,
            smoke_ok: smoke.clone(),
            wedge: Default::default(),
                slots_fp: Default::default(),
        }));
        let _ = daemon.serving_tx.send_replace(ready_snapshot());

        // Fail once.
        if let Some(h) = daemon.spawn_health_heartbeat_if_due() {
            h.await.unwrap();
        }
        assert!(daemon.serving_tx.borrow().ready);

        // Lane recovers; next heartbeat passes → streak resets.
        smoke.store(true, Ordering::Relaxed);
        daemon.health_ticks.store(0, Ordering::Relaxed);
        if let Some(h) = daemon.spawn_health_heartbeat_if_due() {
            h.await.unwrap();
        }
        assert!(daemon.serving_tx.borrow().ready);

        // Fail once more — because the streak reset, this is the FIRST failure again, so
        // it must NOT reap.
        smoke.store(false, Ordering::Relaxed);
        daemon.health_ticks.store(0, Ordering::Relaxed);
        if let Some(h) = daemon.spawn_health_heartbeat_if_due() {
            h.await.unwrap();
        }
        assert!(
            daemon.serving_tx.borrow().ready,
            "a passing probe reset the streak — one later failure is not 'sustained'"
        );
    }

    // what this catches: the SLOW cadence. The heartbeat must NOT probe every tick (that
    // would burn one GPU decode per 5s tick, the load-per-tick the trust short-circuit
    // exists to avoid); it fires only every HEALTH_PROBE_EVERY_TICKS.
    #[tokio::test]
    async fn health_heartbeat_only_fires_on_the_slow_cadence() {
        let serves = Arc::new(AtomicUsize::new(0));
        let smoke = Arc::new(AtomicBool::new(false));
        let daemon = daemon_with(Arc::new(FakeServer {
            serves,
            ok: true,
            smoke_ok: smoke.clone(),
            wedge: Default::default(),
                slots_fp: Default::default(),
        }));
        let _ = daemon.serving_tx.send_replace(ready_snapshot());

        // Tick 0 is due.
        let first = daemon.spawn_health_heartbeat_if_due();
        assert!(first.is_some(), "tick 0 probes");
        if let Some(h) = first {
            h.await.unwrap();
        }
        // The next HEALTH_PROBE_EVERY_TICKS-1 ticks must NOT probe, so one failure can't be
        // compounded to the reap threshold faster than the cadence.
        for _ in 1..HEALTH_PROBE_EVERY_TICKS {
            assert!(
                daemon.spawn_health_heartbeat_if_due().is_none(),
                "non-heartbeat ticks skip the probe"
            );
        }
        assert!(
            daemon.serving_tx.borrow().ready,
            "only one probe ran across the cadence window → hysteresis holds"
        );
    }

    // what this catches: DELIVERY BEATS PROBING — the trust short-circuit that was
    // untestable until the decode-age evidence became a per-daemon seam. A lane that
    // delivered real tokens inside the probe window is PROVEN alive by work that already
    // happened, so the heartbeat must not spend a slot re-proving it; a lane whose last
    // decode is older than the window has no live evidence and must probe. Regresses the
    // measured SWE-bench kill (v13): a busy lane failed two synthetic probes purely by
    // losing the slot race, the relaunch left every downstream generate refusing with
    // `serving: <none>`, and the run died — the recovery being scored as a capability zero
    // ([[a-benchmark-zero-is-a-claim-about-the-harness-until-proven-otherwise]]).
    #[tokio::test]
    async fn a_lane_proven_alive_by_real_decode_is_not_probed() {
        let window_ms = TICK.as_millis() as u64 * HEALTH_PROBE_EVERY_TICKS;
        let serves = Arc::new(AtomicUsize::new(0));

        // Fresh decode INSIDE the window → trusted, no probe.
        let mut busy = daemon_with(Arc::new(FakeServer {
            serves: serves.clone(),
            ok: true,
            smoke_ok: Arc::new(AtomicBool::new(false)),
            wedge: Default::default(),
                slots_fp: Default::default(),
        }));
        busy.set_decode_age_source(Arc::new(move || Some(window_ms / 2)));
        let _ = busy.serving_tx.send_replace(ready_snapshot());
        assert!(
            busy.spawn_health_heartbeat_if_due().is_none(),
            "tokens came out half a window ago — the compute path is already proven, so the \
             heartbeat must not contend for a slot with the work proving it"
        );
        // And the trust RESETS the streak: evidence of life is evidence, not a skipped verdict.
        assert_eq!(busy.health_fails.load(Ordering::Relaxed), 0);

        // Stale decode OUTSIDE the window → no live evidence, probe as usual.
        let mut quiet = daemon_with(Arc::new(FakeServer {
            serves,
            ok: true,
            smoke_ok: Arc::new(AtomicBool::new(true)),
            wedge: Default::default(),
                slots_fp: Default::default(),
        }));
        quiet.set_decode_age_source(Arc::new(move || Some(window_ms + 1)));
        let _ = quiet.serving_tx.send_replace(ready_snapshot());
        let probe = quiet.spawn_health_heartbeat_if_due();
        assert!(
            probe.is_some(),
            "the last token predates the probe window — nothing proves this lane alive, so it \
             must be probed rather than trusted"
        );
        if let Some(h) = probe {
            h.await.unwrap();
        }
    }

    // what this catches: the #363 blackout class — a lane that REJECTS the fleet's
    // real prompts (undersized slot / mid-stream death) while both trust paths stay
    // green: recent-decode trust says alive, the 1-token smoke probe would pass.
    // Sustained REAL-turn failure must outrank both and declare the wedge, flipping
    // the snapshot not-ready + arming force_relaunch — without ever spending a probe.
    // Regression for the 2026-08-07 25-minute room blackout (serving/status
    // ready:true throughout, 4 citizens' turns all dying mid-stream).
    #[tokio::test]
    async fn sustained_real_turn_failures_outrank_a_passing_probe() {
        let serves = Arc::new(AtomicUsize::new(0));
        let mut d = daemon_with(Arc::new(FakeServer {
            serves,
            ok: true,
            // The smoke probe WOULD pass — that is the point: it must not get the
            // chance to vouch for a lane the real workload proves broken.
            smoke_ok: Arc::new(AtomicBool::new(true)),
            wedge: Default::default(),
                slots_fp: Default::default(),
        }));
        // Fresh decode trust too (a partial stream can stamp it) — must ALSO be outranked.
        let window_ms = TICK.as_millis() as u64 * HEALTH_PROBE_EVERY_TICKS;
        d.set_decode_age_source(Arc::new(move || Some(window_ms / 2)));
        d.set_real_fails_source(Arc::new(|| HEALTH_FAILS_TO_RELAUNCH as u64));
        let _ = d.serving_tx.send_replace(ready_snapshot());

        assert!(
            d.spawn_health_heartbeat_if_due().is_none(),
            "the wedge verdict is immediate — no probe task is spawned"
        );
        assert!(
            !d.serving_tx.borrow().ready,
            "sustained real failures must flip the snapshot not-ready even though the \
             smoke probe would pass and decode trust is fresh"
        );
        assert!(
            d.force_relaunch.load(Ordering::Acquire),
            "the reconcile must be forced to re-prove the lane, not re-adopt it"
        );
    }

    // what this catches (L11 — the 2026-08-16 23-minute boot kill-loop): after a
    // reboot, the adopted lane ground through the dead core's GHOST turns — real
    // work no adapter stream observes, so decode-age trust (L9) was blind, two
    // smoke misses counted as wedge evidence, and a healthy lane was killed three
    // times. The heartbeat must consult the server's OWN /slots between misses:
    // an ADVANCING fingerprint exonerates the miss (streak resets, lane stays
    // ready); a FROZEN fingerprint keeps counting and wedges at the same
    // threshold/latency as before.
    #[tokio::test]
    async fn a_smoke_miss_on_a_lane_with_advancing_slots_is_not_wedge_evidence() {
        let serves = Arc::new(AtomicUsize::new(0));
        let slots_fp = Arc::new(AtomicU64::new(1));
        let mut d = daemon_with(Arc::new(FakeServer {
            serves,
            ok: true,
            // Every smoke probe MISSES — the ghost work holds the slots.
            smoke_ok: Arc::new(AtomicBool::new(false)),
            wedge: Default::default(),
            slots_fp: slots_fp.clone(),
        }));
        // Pin both process-global evidence sources to inert test values — the
        // globals are stamped by unrelated tests under full-suite parallelism
        // (the #7 isolation class), which would silently skip the probe path.
        d.set_decode_age_source(Arc::new(|| None));
        d.set_real_fails_source(Arc::new(|| 0));
        let _ = d.serving_tx.send_replace(ready_snapshot());

        // Drive a heartbeat probe to completion (the cadence gate only fires
        // every Nth call; bounded by that cadence).
        let probe_once = |d: &ServingDaemonModule| {
            let mut h = None;
            for _ in 0..(HEALTH_PROBE_EVERY_TICKS as usize + 1) {
                if let Some(handle) = d.spawn_health_heartbeat_if_due() {
                    h = Some(handle);
                    break;
                }
            }
            h.expect("a heartbeat probe must fire within one cadence window")
        };

        // Miss #1: first miss has nothing to compare — it counts (streak = 1).
        probe_once(&d).await.unwrap();
        assert_eq!(d.health_fails.load(Ordering::Relaxed), 1);
        // The serve loop ADVANCES between misses (ghost work / other clients).
        slots_fp.store(2, Ordering::Relaxed);
        // Miss #2: fingerprint moved → exonerated, streak resets, still ready.
        probe_once(&d).await.unwrap();
        assert_eq!(
            d.health_fails.load(Ordering::Relaxed),
            0,
            "slot progress between misses proves the serve loop alive — the miss \
             must reset the streak, not feed the relaunch threshold"
        );
        assert!(d.serving_tx.borrow().ready, "lane must stay ready");
        assert!(!d.force_relaunch.load(Ordering::Acquire));

        // Now FREEZE the fingerprint (the 2026-08-05 wedge signature): two more
        // misses with no slot movement must wedge exactly as before L11.
        probe_once(&d).await.unwrap();
        assert_eq!(d.health_fails.load(Ordering::Relaxed), 1);
        probe_once(&d).await.unwrap();
        assert!(
            !d.serving_tx.borrow().ready,
            "a frozen /slots across the miss window is the real wedge — detection \
             latency must be unchanged"
        );
        assert!(d.force_relaunch.load(Ordering::Acquire));
    }

    // what this catches: a lane whose per-slot window froze at ≤ HALF the
    // currently-planned window gets RE-HOMED (relaunched) even though model +
    // genome match. A server spawned under transient memory pressure (a scratch
    // benchmark server) otherwise keeps its starved window forever — 2026-07-10:
    // the living Devstral froze at 3.8k/slot on a 131k model while 14,700 plan
    // recomputes wandered above it, and the room degenerated into a greeting
    // loop. Within 2× of the plan → hysteresis holds, no relaunch churn.
    #[tokio::test]
    async fn reconcile_re_homes_a_starved_window_but_holds_within_hysteresis() {
        let serves = Arc::new(AtomicUsize::new(0));
        let daemon = daemon_with(Arc::new(FakeServer::healthy(serves.clone(), true)));

        let budget = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true).unwrap()];
        daemon.publish_plan(budget, &candidates);
        let plan_window = daemon
            .plan_tx
            .borrow()
            .as_ref()
            .expect("plan published")
            .served_context_window;

        // Same model + genome, but the live slot froze far below the plan.
        let _ = daemon.serving_tx.send_replace(ServingSnapshot {
            // test fixture: no live readiness was ever CONFIRMED here.
            ready_verified_at_ms: None,
            active_model: Some("coder-14b".into()),
            ready: true,
            base_url: serving_v1_url(),
            adapters: Vec::new(),
            served_context_window: plan_window / 4,
            lanes: 4,
            degraded_reason: None,
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
            // test fixture: no engine identity claimed.
            engine_build: None,
            loading_model: None,
        });
        // A quarter of the plan is a 300% shortfall — far past the margin — but the
        // gain must still PERSIST before it buys a relaunch (BigMama's sustained-delta
        // replacement for the original 2x ratio, 2026-08-06).
        for tick in 1..REHOME_SUSTAINED_TICKS {
            assert!(
                daemon.reconcile_to_plan().is_none(),
                "tick {tick}: even a starved lane must prove the gain is sustained"
            );
        }
        daemon
            .reconcile_to_plan()
            .expect("starved window must trigger a re-home")
            .await
            .unwrap();
        assert_eq!(serves.load(Ordering::SeqCst), 1, "one relaunch");

        // Within hysteresis (> half the plan) → hold, no churn.
        let _ = daemon.serving_tx.send_replace(ServingSnapshot {
            // test fixture: no live readiness was ever CONFIRMED here.
            ready_verified_at_ms: None,
            active_model: Some("coder-14b".into()),
            ready: true,
            base_url: serving_v1_url(),
            adapters: Vec::new(),
            served_context_window: plan_window / 2 + 256,
            lanes: 4,
            degraded_reason: None,
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
            // test fixture: no engine identity claimed.
            engine_build: None,
            loading_model: None,
        });
        assert!(
            daemon.reconcile_to_plan().is_none(),
            "within 2\u{d7} of the plan the lane holds — no relaunch churn"
        );
        assert_eq!(serves.load(Ordering::SeqCst), 1, "still one relaunch");
    }

    // what this catches: no servable plan (empty registry) publishes the empty
    // snapshot so readers (and a grid allocator) see "nothing live here" and can
    // route the lease elsewhere — the Intel-Mac/weak-node path.
    #[tokio::test]
    async fn no_plan_publishes_empty_snapshot() {
        let serves = Arc::new(AtomicUsize::new(0));
        let daemon = daemon_with(Arc::new(FakeServer::healthy(serves.clone(), true)));

        // Seed a live snapshot, then publish an empty plan → must clear to empty.
        let _ = daemon.serving_tx.send_replace(ServingSnapshot {
            // test fixture: no live readiness was ever CONFIRMED here.
            ready_verified_at_ms: None,
            active_model: Some("stale".into()),
            ready: true,
            base_url: serving_v1_url(),
            adapters: Vec::new(),
            served_context_window: 11008,
            lanes: 4,
            degraded_reason: None,
            vision_ready: false,
            vision_base_url: None,
            vision_model: None,
            // test fixture: no engine identity claimed.
            engine_build: None,
            loading_model: None,
        });
        let budget = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        daemon.publish_plan(budget, &[]); // no candidates → plan None

        assert!(
            daemon.reconcile_to_plan().is_none(),
            "no plan → no reconcile spawned"
        );
        let snap = daemon.subscribe_serving().borrow().clone();
        assert_eq!(snap.active_model, None, "stale snapshot cleared to empty");
        assert!(!snap.ready);
        assert_eq!(serves.load(Ordering::SeqCst), 0);
    }

    // what this catches: a degraded reconcile (serve fails) publishes "nothing
    // live" and clears the in-flight gate, so the NEXT reconcile can retry
    // rather than being permanently locked out.
    #[tokio::test]
    async fn degraded_reconcile_clears_gate_and_publishes_empty() {
        let serves = Arc::new(AtomicUsize::new(0));
        let daemon = daemon_with(Arc::new(FakeServer::healthy(serves.clone(), false)));

        let budget = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true).unwrap()];
        daemon.publish_plan(budget, &candidates);

        daemon.reconcile_to_plan().expect("spawned").await.unwrap();
        let snap = daemon.subscribe_serving().borrow().clone();
        assert!(!snap.ready, "degraded → not ready");
        assert_eq!(snap.active_model, None);
        assert!(
            !daemon.reconciling.load(Ordering::SeqCst),
            "gate cleared for retry"
        );

        // Gate cleared → a retry actually spawns again.
        daemon
            .reconcile_to_plan()
            .expect("retry spawned")
            .await
            .unwrap();
        assert_eq!(serves.load(Ordering::SeqCst), 2, "retried after degrade");
    }

    // what this catches: a reconcile emits the live snapshot on the BUS (topic
    // serving.snapshot), not just the in-process watch — the cbar fan-out that
    // lets any subscriber (and a remote grid allocator) get serving state pushed
    // without point-to-point plumbing. Regression = silent watch-only updates
    // that no subscriber ever sees.
    #[tokio::test]
    async fn reconcile_emits_snapshot_on_the_bus() {
        let serves = Arc::new(AtomicUsize::new(0));
        let daemon = daemon_with(Arc::new(FakeServer::healthy(serves.clone(), true)));
        let bus = Arc::new(MessageBus::new());
        let _ = daemon.bus.set(bus.clone());

        let budget = HostBudget {
            usable_bytes: 45 * GB,
            perf_cores: 6,
        };
        let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true).unwrap()];
        daemon.publish_plan(budget, &candidates);
        daemon.reconcile_to_plan().expect("spawned").await.unwrap();

        let event = bus
            .find_recent_event(SERVING_SNAPSHOT_EVENT)
            .expect("serving.snapshot must be emitted on the bus");
        let snap: ServingSnapshot = serde_json::from_value(event.payload).unwrap();
        assert_eq!(snap.active_model.as_deref(), Some("coder-14b"));
        assert!(snap.ready, "emitted snapshot reflects the live model");
    }

    // what this catches: the footprint resolver serving hands the authority reads
    // the SAME live catalog + estimator the serving plan uses (one footprint
    // authority) — a Ready model resolves to its real on-disk weights, and an id
    // the catalog doesn't know resolves to 0 (nothing resident to attribute, never
    // a phantom the governor would over-account). If this drifted from the plan's
    // footprint the board would attribute a different number than the planner sized
    // against.
    #[test]
    fn serving_footprint_fn_resolves_live_catalog_weights() {
        use crate::model_registry::live::ModelStatus;
        use std::io::Write;

        let catalog = test_catalog();
        let id = "footprint-probe";
        catalog.register(
            fake_model(id),
            ModelStatus {
                availability: Availability::NotDownloaded,
                verified: None,
            },
        );
        let resolve = serving_footprint_fn(catalog.clone());

        // NotDownloaded (no on-disk weights) → nothing resident yet. Window/lanes
        // are the live serving shape; with no weights they resolve to 0 anyway.
        assert_eq!(
            resolve(id, 8192, 2),
            0,
            "no weights on disk → nothing to attribute"
        );
        // An id the catalog has never heard of → 0, never a phantom.
        assert_eq!(resolve("never-registered", 8192, 2), 0);

        // Land the artifact: real bytes on disk, flips Ready.
        let mut gguf = tempfile::Builder::new()
            .suffix(".gguf")
            .tempfile()
            .expect("temp gguf");
        gguf.write_all(&[0u8; 4096]).expect("write weights");
        gguf.flush().expect("flush");
        assert!(catalog.attach_local_artifact(id, gguf.path().to_path_buf(), None));

        // Same resolver, same live catalog → now reports the real PEAK resident bytes:
        // weights (4096) + the KV of every lane at the served window + the concurrent-
        // prefill compute reserve of every lane (#56/G5). kv_per_token floors at 20_000,
        // so 2 lanes × 20_000 × 8192 tokens of KV on top of 4096 weights, PLUS the
        // window-scaled compute reserve. Charging the KV stops it masquerading as
        // external (#79); charging the compute reserve stops the board over-reporting
        // free by the prefill buffer (G5). The compute-reserve term comes from the
        // footprint's own method so this expectation can't drift from the plan's sizing.
        let fp = footprint_from_parts(id, 4096, 8192, false).expect("footprint");
        let kv_per_token = 20_000u64; // (4096 / 80_000).max(20_000)
        let expect = 4096 + 2 * kv_per_token * 8192 + fp.prefill_compute_reserve(8192, 2);
        assert_eq!(
            resolve(id, 8192, 2),
            expect,
            "resolves real weights + per-lane KV + prefill compute reserve (peak), no reboot"
        );
        assert_eq!(
            resolve(id, 8192, 2),
            fp.peak_resident_bytes(8192, 2),
            "resolver reports peak_resident_bytes — the plan's chosen_cost, not resident-only"
        );
        // A no-window snapshot (nothing served yet) still charges weights + every lane's
        // compute-buffer FLOOR (the reserve exists even before a window is chosen).
        assert_eq!(
            resolve(id, 0, 2),
            4096 + fp.prefill_compute_reserve(0, 2),
            "no served window → weights + per-lane compute floor",
        );
    }

    // what this catches: THE slice-1 production-wiring gap — ServingConsumer was
    // defined + unit-tested but never CONSTRUCTED in the boot path, so the
    // authority polled nothing and stayed blind to serving's multi-GB residency.
    // register_as_consumer (called from initialize) must register a consumer under
    // SERVING_CONSUMER_ID with the one per-machine authority. Regresses deleting
    // that registration — which would silently reopen the granted:0-while-full hole.
    #[tokio::test]
    async fn register_as_consumer_wires_serving_into_the_authority() {
        let gpu = Arc::new(GpuMemoryManager::simulated("Apple M5 Pro", 53 * GB));
        let system = Arc::new(SystemResourceMonitor::new());
        let daemon = ServingDaemonModule::new(gpu, system, test_resource_daemon(), test_catalog());

        assert!(
            !daemon
                .resource_daemon
                .consumer_ids()
                .contains(&SERVING_CONSUMER_ID.to_string()),
            "not registered until the daemon wires itself in"
        );
        daemon.register_as_consumer();
        assert!(
            daemon
                .resource_daemon
                .consumer_ids()
                .contains(&SERVING_CONSUMER_ID.to_string()),
            "serving must register itself as a measured consumer with the authority"
        );
    }

    // what this catches (#368, 2nd occurrence): a ONE-tick budget collapse must
    // classify as a Downshift (so publish_plan HOLDS the incumbent plan until
    // the squeeze proves sustained) — while same-model, upshift, and
    // incumbent-gone cases adopt immediately. The live failure: usable_gb hit 0
    // for ~6 seconds at solve launch, the fresh plan named the 0.5B, and the
    // reconcile re-brained every citizen onto it before the dip had even passed.
    #[test]
    fn downshift_gate_classifies_the_transient_lobotomy() {
        let footprint = |id: &str, weights_gb: u64, rank: u8| ModelFootprint {
            model_id: id.into(),
            weights_bytes: weights_gb * GB,
            kv_per_token: 100_000,
            context_window: 32768,
            capability_rank: rank,
        };
        let devstral = footprint("devstral-24b", 14, 8);
        let tiny = footprint("qwen-0.5b", 1, 1);
        let plan_for = |id: &str| ServingPlan {
            base_model: footprint(id, 1, 1),
            served_context_window: 2048,
            lanes: 1,
            grid_overflow_lanes: 0,
            resident_models: 1,
            fits_on_gpu: true,
            rationale: String::new(),
        };
        let both = vec![devstral.clone(), tiny.clone()];

        // The incident shape: incumbent Devstral, fresh wants the 0.5B → HOLD.
        assert_eq!(
            downshift_gate(&plan_for("qwen-0.5b"), Some("devstral-24b"), &both),
            DownshiftVerdict::Downshift,
        );
        // Same model → nothing to debounce.
        assert_eq!(
            downshift_gate(&plan_for("devstral-24b"), Some("devstral-24b"), &both),
            DownshiftVerdict::NotADownshift,
        );
        // UPSHIFT (0.5B incumbent, fresh wants Devstral) must adopt immediately —
        // holding it would have kept the citizens lobotomized on purpose.
        assert_eq!(
            downshift_gate(&plan_for("devstral-24b"), Some("qwen-0.5b"), &both),
            DownshiftVerdict::NotADownshift,
        );
        // Incumbent vanished from disk → holding a plan naming a ghost is worse
        // than any downshift; adopt what is actually present.
        assert_eq!(
            downshift_gate(&plan_for("qwen-0.5b"), Some("devstral-24b"), &[tiny]),
            DownshiftVerdict::NotADownshift,
        );
        // No incumbent (boot) → plain adoption.
        assert_eq!(
            downshift_gate(&plan_for("qwen-0.5b"), None, &both),
            DownshiftVerdict::NotADownshift,
        );
    }

    // what this catches (#225, Joel 2026-08-08 "the budgeter just has all its
    // parts figure it out"): serving's plan budget (`governed_vram_ceiling`)
    // reads serving's OWN view of the board — global available minus every
    // OTHER consumer's unmet reservation floor — so the plan can no longer size
    // the window into the embed lane's slice and starve a faculty cognition
    // needs every turn. Regression: reading the reservation-blind global
    // `available_bytes` again would make this budget ignore the floor.
    #[tokio::test]
    async fn serving_budget_plans_around_other_consumers_floors() {
        use crate::resources::{DaemonConfig, GovernorConfig, MockCapacitySource, ResourceDaemon};
        let src = Arc::new(MockCapacitySource::new(
            crate::resources::ResourceKind::Vram,
            10_000,
        ));
        let daemon = ResourceDaemon::start(
            vec![src],
            vec![],
            DaemonConfig {
                tick_interval: std::time::Duration::from_millis(20),
                min_reclaim_budget: std::time::Duration::from_millis(100),
                governor: GovernorConfig {
                    min_dwell_ms: 0,
                    graceful_grace_ms: 50,
                },
            },
        );
        // No floors → serving sees the whole board.
        assert_eq!(governed_vram_ceiling(&daemon), Some(10_000));
        // The embed lane claims its standing floor → serving's plannable view
        // shrinks by exactly that slice; the board's Vram row still exists so
        // the None ("governor hasn't reported") semantics are untouched.
        daemon.reserve("embed", crate::resources::ResourceKind::Vram, 1_800);
        assert_eq!(governed_vram_ceiling(&daemon), Some(8_200));
        // Serving's own hypothetical floor would NOT count against itself.
        daemon.reserve(
            SERVING_CONSUMER_ID,
            crate::resources::ResourceKind::Vram,
            3_000,
        );
        assert_eq!(governed_vram_ceiling(&daemon), Some(8_200));
    }
}
