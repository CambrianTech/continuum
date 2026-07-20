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

use crate::cognition::model_resolver::types::HwCapabilityTier;
use crate::cognition::serving_plan::{
    plan_serving, plan_serving_stable, HostBudget, ModelFootprint, ServingPlan, MIN_SERVE_CTX,
};
use crate::gpu::GpuMemoryManager;
use crate::inference::llama_server::{
    ensure_model_serving, serving_v1_url, AdapterEntry, EnsureOutcome, LlamaServerControl,
    LlamaServerProcess, ServingSnapshot, ServingTarget,
};
use crate::persona::hw_tier_descriptor::HwTierCategory;
use crate::model_registry::live::{Availability, CatalogSnapshot, ModelCatalog};
use crate::model_registry::types::{Capability, Model};
use crate::resources::{LeaseBoard, ResourceDaemon, ResourceKind};
use super::serving_consumer::{FootprintFn, ServingConsumer, SERVING_CONSUMER_ID};
use crate::runtime::message_bus::MessageBus;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::system_resources::SystemResourceMonitor;
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::sync::watch;
use tokio::task::JoinHandle;

/// Fraction of total VRAM/UMA we treat as ours to serve from — the rest is
/// OS + non-inference headroom (Bevy avatars, embeddings, the OS itself). A
/// single source of truth for "how much is actually ours."
const SERVING_BUDGET_FRACTION: f64 = 0.80;

/// How often the daemon re-evaluates the serving plan. 5s matches the other
/// pressure-class ticks (pressure-broker, ai_provider) in the runtime. The
/// next slice makes the re-plan also fire on PressureBroker watch edges so it
/// reacts faster than the cadence under sudden pressure.
const TICK: Duration = Duration::from_secs(5);

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

/// Resolves a base-model id (as named in a [`ServingPlan`]) back to its full
/// [`Model`] struct. Production resolves through the global registry; tests
/// inject a fake so the reconcile WIRING can be exercised without a populated
/// registry. The resolved `Model` is carried straight onto the [`ServingTarget`]
/// — resolve once, pass the struct, never re-fetch ([[pass-the-model-struct-no-param-hell]]).
type ModelResolver = Arc<dyn Fn(&str) -> Option<Model> + Send + Sync>;

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
    /// The LIVE model universe — the SAME `Arc<ModelCatalog>` the `models/*`
    /// command surface mutates. The daemon plans off this snapshot, NOT the
    /// immutable seed registry, so a model acquired at runtime (`models/pull`
    /// flips it to [`Availability::Ready`] with its real on-disk path) becomes a
    /// serving candidate on the very next tick — no reboot. This is the consumer
    /// side of the rich API: serving reacts to the universe changing.
    catalog: Arc<ModelCatalog>,
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
            catalog,
            suppressed,
            pinned,
            lane_demand: Arc::new(std::sync::atomic::AtomicU32::new(1)),
        }
    }

    /// Set the lane DEMAND (how many minds need a concurrent lane — the
    /// persona floor). The boot wiring calls this before the first
    /// [`Self::compute_plan`]; the next tick replans if it changes.
    pub fn set_lane_demand(&self, demand: u32) {
        self.lane_demand
            .store(demand.max(1), Ordering::Relaxed);
    }

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
    pub fn register_planner_on_authority_tick(self: &Arc<Self>) {
        let weak = Arc::downgrade(self);
        self.resource_daemon.on_tick(Arc::new(move |_board: &LeaseBoard| {
            if let Some(module) = weak.upgrade() {
                module.recompute();
            }
        }));
    }

    fn lane_demand(&self) -> u32 {
        self.lane_demand.load(Ordering::Relaxed).max(1)
    }

    /// Test seam: override how planned model ids resolve to [`Model`] structs,
    /// so the reconcile wiring runs without a populated global registry.
    #[cfg(test)]
    fn set_model_resolver(&mut self, resolver: ModelResolver) {
        self.model_resolver = resolver;
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
    fn register_as_consumer(&self) {
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
            Arc::new(crate::modules::serving_tier_down::CatalogTierDownPolicy::new(
                candidates,
            )),
        );
        self.resource_daemon.add_consumer(Arc::new(consumer));
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
        // ALSO apply the static SERVING_BUDGET_FRACTION here — stacking it under the mode
        // fraction double-discounted the everyday budget to ~0.64 (24.5GB of a 38GB board)
        // and left the more-capable 32B coder + full context unused, WITHOUT buying safety
        // (the concurrent-prefill compute buffer is reserved separately, window-scaled, in
        // the serving_plan fixpoint). The pin fit-gate still uses `live_host_budget`'s raw
        // 0.80 directly — "can this model physically fit" is a different question than "how
        // much should the shared base claim now." [[verify-real-device-numbers-not-a-clamp-premise]]
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
        let live = governed_vram_ceiling(&self.resource_daemon).unwrap_or(0);
        let mode = crate::provisioning::serving_mode_for_pressure(available);
        // Observability: emit ONLY on a mode TRANSITION so the dynamic scaling is visible
        // without spamming the hot plan tick ([[never-blind-feedback-driven-iteration]]).
        // This is the seam a learned / LLM policy will report through — watch it kick down
        // under load, and later watch a smarter policy make a better call.
        {
            use std::sync::atomic::{AtomicU8, Ordering};
            static LAST_MODE: AtomicU8 = AtomicU8::new(u8::MAX);
            let m = mode as u8;
            if LAST_MODE.swap(m, Ordering::Relaxed) != m {
                eprintln!("🎛 serving mode → {:?} ({} GiB free)", mode, available / (1 << 30));
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
        Arc::new(move |model: &Model| {
            let budget = live_host_budget(&system, &resource_daemon);
            let budget_bytes = budget.usable_bytes;
            let footprint = footprint_for(model);
            let weights_bytes = footprint.as_ref().map(|f| f.weights_bytes).unwrap_or(0);
            // footprint None = no GGUF on disk → not servable at all (plan None).
            // footprint Some but over budget → plan_serving degrades honestly
            // with fits_on_gpu=false, which the command reads to refuse loud.
            // Fit verdict at ONE lane — "can this model hold a lane at all";
            // the live plan sizes lanes from demand separately.
            let plan = footprint.and_then(|f| plan_serving(budget, std::slice::from_ref(&f), 1));
            PinFit {
                plan,
                weights_bytes,
                budget_bytes,
            }
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
            self.lane_demand(),
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
    /// No plan → publish the empty snapshot (no servable model = nothing live).
    /// Already serving the desired model & ready → no-op. A reconcile already
    /// in flight → skip (the gate). Otherwise spawn the reconcile.
    fn reconcile_to_plan(&self) -> Option<JoinHandle<()>> {
        // Pull the desired model id, the host-fit PER-LANE served window, AND
        // the lane count out of the plan in one borrow — both are the planner's
        // single source of truth (task #50). We carry them on the ServingTarget
        // so llama-server's `-c` (= window × lanes) and `--parallel` (= lanes)
        // match exactly what was planned: each slot gets one full served window.
        let (desired, served_ctx, lanes) = match self.plan_tx.borrow().as_ref() {
            Some(plan) => (
                plan.base_model_id.clone(),
                plan.served_context_window,
                plan.lanes,
            ),
            None => {
                // Nothing servable on disk → publish "nothing live" so readers
                // (and a grid allocator) see the gap and route elsewhere.
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
                let starved = live.served_context_window > 0
                    && live.served_context_window.saturating_mul(2) <= served_ctx;
                if !starved {
                    return None;
                }
                crate::probe!(
                    class = "serving.reconcile",
                    live_window = live.served_context_window,
                    plan_window = served_ctx,
                    lanes,
                    "re-homing starved lane: served window froze at ≤ half the planned window",
                );
            }
        }

        // #175 sticky window: we're past the no-relaunch guard, so a relaunch WILL
        // happen (a genome page-in / model change). Don't let the startup LoRA-load
        // cascade ratchet the per-slot window DOWN on a same-lane relaunch and strand
        // the personas pinned to the earlier, larger slot — keep the incumbent window
        // when lanes are unchanged (memory-safe; a lane change legitimately resizes KV).
        let served_ctx = sticky_served_window(served_ctx, lanes, &self.serving_tx.borrow());

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
            if self.serving_tx.borrow().active_model.is_some() {
                let empty = ServingSnapshot::empty();
                Self::emit_serving(self.bus.get(), &empty);
                let _ = self.serving_tx.send_replace(empty);
            }
            return None;
        };
        let target = ServingTarget {
            model,
            context_window: served_ctx,
            lanes,
            adapters: desired_adapters,
            // The living persona lane: GPU-resident for throughput (every
            // offloadable layer). [[LanePlacement]].
            placement: crate::inference::llama_server::LanePlacement::Gpu,
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
        Some(tokio::spawn(async move {
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
            let snapshot = snapshot_from_outcome(
                &outcome,
                &desired,
                &desired_adapter_paths,
                served_window,
                target.lanes,
            );
            crate::probe!(
                class = "serving.reconcile",
                desired = desired.as_str(),
                ready = snapshot.ready,
                active = snapshot.active_model.as_deref().unwrap_or("<none>"),
                served_window = snapshot.served_context_window,
                "serving reconcile complete",
            );
            // Emit on the bus first (fan-out to every subscriber + the grid),
            // then update the in-process watch view.
            Self::emit_serving(bus.as_ref(), &snapshot);
            let _ = serving_tx.send_replace(snapshot);
            reconciling.store(false, Ordering::Release);
        }))
    }

    /// The liveness HEARTBEAT (#175 self-heal). On a cadence far slower than [`TICK`],
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
        let bus = self.bus.get().cloned();
        Some(tokio::spawn(async move {
            // A real one-token generation through the live slots — the ONLY probe that
            // distinguishes a healthy lane from an OOM-poisoned one (control-plane reads
            // stay 200 on a wedged backend). `decode_smoke_ok` is already bounded by
            // `DECODE_SMOKE_TIMEOUT`, so a wedged compute path resolves to `false` fast.
            let ok = server.decode_smoke_ok().await;
            if ok {
                health_fails.store(0, Ordering::Relaxed);
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
                // Sustained no-decode = wedged, not transiently busy. Arm the force-probe
                // (so the next reconcile re-proves decode instead of re-adopting the owned
                // wedged child) AND publish not-ready (so the reconcile's no-op guard is
                // skipped). Reset the streak so we don't re-trigger before the relaunch
                // lands and republishes ready.
                force_relaunch.store(true, Ordering::Release);
                health_fails.store(0, Ordering::Relaxed);
                let empty = ServingSnapshot::empty();
                Self::emit_serving(bus.as_ref(), &empty);
                let _ = serving_tx.send_replace(empty);
                crate::probe!(
                    class = "serving.health",
                    action = "relaunch",
                    "flipped serving snapshot not-ready after sustained decode failure — \
                     reconcile will kill+respawn the wedged lane (#175 self-heal)",
                );
            }
            health_probing.store(false, Ordering::Release);
        }))
    }

    /// Pure publish step: run the classifier on the given inputs, publish the
    /// result, log it. Split from `recompute` so it's testable without the
    /// global registry / live GPU.
    fn publish_plan(&self, budget: HostBudget, candidates: &[ModelFootprint]) {
        // Hysteresis: pass the currently-served model as the incumbent so a
        // transient free-memory dip doesn't thrash the served model. Boot's
        // first plan has no incumbent (plan_tx holds None) → plain selection.
        let incumbent = self
            .plan_tx
            .borrow()
            .as_ref()
            .map(|p| p.base_model_id.clone());
        match plan_serving_stable(budget, candidates, incumbent.as_deref(), self.lane_demand()) {
            Some(plan) => {
                crate::probe!(
                    class = "serving.plan",
                    base_model = plan.base_model_id.as_str(),
                    lanes = plan.lanes,
                    resident = plan.resident_models,
                    fits_on_gpu = plan.fits_on_gpu,
                    usable_gb = (budget.usable_bytes / 1_000_000_000),
                    candidates = candidates.len(),
                    "serving plan recomputed",
                );
                // Publish the LIVE lane count to the admission gate so its directed-turn
                // reservation semaphores size by what's actually served (`--parallel
                // plan.lanes`), not the `MAX_LANES` ceiling — exact once the plan can serve
                // fewer lanes than the ceiling (#139 compute-buffer fit). ONE source of truth.
                crate::cognition::resource_admission::set_served_lane_count(plan.lanes as usize);
                // And the prefill throttle's demand facts (#56): the served model's per-spike
                // transient compute buffer + the lane count. Published HERE, next to the lane
                // count, so both gates read the one plan — no second path.
                let spike = candidates
                    .iter()
                    .find(|c| c.model_id == plan.base_model_id)
                    .map(|f| f.compute_buffer_per_lane())
                    .unwrap_or(0);
                crate::cognition::prefill_throttle::publish_serving(spike, plan.lanes as usize);
                // send_replace keeps the latest even with no live receivers yet.
                let _ = self.plan_tx.send_replace(Some(plan));
            }
            None => {
                // No servable model on disk. Publish None and say so loudly —
                // the spawner gates on a model being present (no silent serve).
                crate::probe!(
                    class = "serving.plan",
                    candidates = 0usize,
                    "no servable model on disk — serving plan empty",
                );
                let _ = self.plan_tx.send_replace(None);
            }
        }
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
}

/// Serving budget from LIVE free memory, capped at physical VRAM, minus headroom.
/// Pure for tests. Takes [`HostBudgetInputs`] by reference so the byte-count fields
/// are named at every call site (never transposable).
pub fn host_budget_from(inputs: &HostBudgetInputs) -> HostBudget {
    let live = inputs.available_bytes.min(inputs.total_vram_bytes);
    let usable = (live as f64 * SERVING_BUDGET_FRACTION) as u64;
    HostBudget {
        usable_bytes: usable,
        perf_cores: inputs.perf_cores.max(1),
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
    let vram_ceiling = governed_vram_ceiling(resource_daemon).unwrap_or(0);
    host_budget_from(&HostBudgetInputs {
        available_bytes: vram_ceiling,
        total_vram_bytes: vram_ceiling,
        perf_cores: perf_cores(),
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
pub fn governed_host_budget(resource_daemon: &ResourceDaemon) -> HostBudget {
    let available = governed_vram_ceiling(resource_daemon).unwrap_or(0);
    host_budget_from(&HostBudgetInputs {
        available_bytes: available,
        total_vram_bytes: available,
        perf_cores: perf_cores(),
    })
}

/// The governed VRAM ceiling RIGHT NOW: the resource authority's `available(Vram)`
/// — capacity (free + ours − reserve, scanned live by the GpuMonitor) minus what
/// is already leased. `None` when VRAM is ungoverned (no live monitor → the kind
/// never appears on the board), which the caller treats as fail-closed (cap at 0,
/// serving refuses rather than over-committing blind). A lock-free `watch`
/// snapshot read, never the governor's accounting lock — safe on the hot tick.
fn governed_vram_ceiling(resource_daemon: &ResourceDaemon) -> Option<u64> {
    resource_daemon
        .board()
        .kinds
        .iter()
        .find(|k| k.kind == ResourceKind::Vram)
        .map(|k| k.available_bytes)
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

pub fn footprint_for(model: &Model) -> Option<ModelFootprint> {
    let path = crate::model_registry::artifacts::resolve_gguf_for_model(model)?;
    let weights_bytes = std::fs::metadata(&path).ok()?.len();
    footprint_from_parts(
        &model.id,
        weights_bytes,
        model.context_window,
        model.has(Capability::ToolUse),
    )
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
) -> ServingSnapshot {
    match outcome {
        EnsureOutcome::AlreadyServing | EnsureOutcome::Spawned { .. }
            if served_context_window > 0 =>
        {
            ServingSnapshot {
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
            }
        }
        // Ready outcome but the served window was unreadable (0) → do NOT publish
        // a ready snapshot with a zero window; that would poison every binding
        // persona's budget. Publish "nothing live"; the server stays up and the
        // next reconcile re-reads /props.
        EnsureOutcome::AlreadyServing | EnsureOutcome::Spawned { .. } => {
            ServingSnapshot::empty()
        }
        EnsureOutcome::Degraded { .. } => ServingSnapshot::empty(),
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
        // Register serving as a MEASURED ResourceConsumer with the one per-machine
        // authority (#79). See `register_as_consumer` — this is monitor-not-reserve:
        // no lease acquired, `available` math untouched, the authority simply stops
        // being blind to the ~multi-GB model actually resident.
        self.register_as_consumer();
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
    use super::*;

    const GB: u64 = 1_000_000_000;

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
        });
        assert!(b.usable_bytes < 40 * GB, "must reserve headroom");
        assert!(b.usable_bytes >= 30 * GB, "but most of free is ours: {}", b.usable_bytes);

        // Organic: less free memory → smaller budget (a game grabbed memory).
        let busy = host_budget_from(&HostBudgetInputs {
            available_bytes: 6 * GB,
            total_vram_bytes: 53 * GB,
            perf_cores: 6,
        });
        assert!(busy.usable_bytes < b.usable_bytes, "less free → smaller budget");

        // Never plan above physical VRAM even if the OS reports more free RAM
        // (unified memory: free RAM can exceed the VRAM serving ceiling).
        let capped = host_budget_from(&HostBudgetInputs {
            available_bytes: 100 * GB,
            total_vram_bytes: 53 * GB,
            perf_cores: 6,
        });
        assert!(capped.usable_bytes <= 53 * GB, "capped at physical VRAM");

        assert_eq!(
            host_budget_from(&HostBudgetInputs {
                available_bytes: 8 * GB,
                total_vram_bytes: 8 * GB,
                perf_cores: 0,
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
    fn footprint_from_parts_is_footprint_aware() {
        let fp = footprint_from_parts("present", 3 * GB, 8192, true).unwrap();
        assert_eq!(fp.model_id, "present");
        assert_eq!(fp.weights_bytes, 3 * GB);
        assert!(fp.kv_per_token > 0);
        assert_eq!(fp.context_window, 8192, "carries the model's trained ceiling, no clamp");
        assert!(fp.capability_rank >= 5, "3GB + tool bonus, got {}", fp.capability_rank);

        // A leaner non-tool model ranks below the bigger tool-capable one.
        let small = footprint_from_parts("small", 1 * GB, 4096, false).unwrap();
        assert!(small.capability_rank < fp.capability_rank);

        assert!(footprint_from_parts("empty", 0, 8192, false).is_none(), "no weights → not servable");
    }

    // what this catches: an M5 Pro (or any capable silicon) must NOT classify
    // as Compat (which would force n_gpu_layers=0 = CPU); unknown hardware
    // stays on the safe LCD/CPU fallback.
    #[test]
    fn detect_tier_classifies_silicon() {
        use crate::persona::hw_tier_descriptor::HwTierCategory;
        assert_eq!(detect_tier("Apple M5 Pro").1, HwTierCategory::MSeriesPro);
        assert_eq!(detect_tier("Apple M2").1, HwTierCategory::MSeries);
        assert_eq!(detect_tier("NVIDIA GeForce RTX 5090").1, HwTierCategory::Cuda);
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

        let budget = HostBudget { usable_bytes: 45 * GB, perf_cores: 6 };
        let candidates = vec![
            footprint_from_parts("small", GB, 4096, false).unwrap(),
            footprint_from_parts("coder-14b", 9 * GB, 8192, true).unwrap(),
        ];
        daemon.publish_plan(budget, &candidates);
        let plan = rx.borrow().clone().expect("plan published");
        assert_eq!(plan.base_model_id, "coder-14b", "most capable that fits");
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
    }

    impl FakeServer {
        /// A healthy fake: serve() outcome = `ok`, decode heartbeat passes.
        fn healthy(serves: Arc<AtomicUsize>, ok: bool) -> Self {
            Self {
                serves,
                ok,
                smoke_ok: Arc::new(AtomicBool::new(true)),
            }
        }
    }

    #[async_trait]
    impl LlamaServerControl for FakeServer {
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
        async fn decode_smoke_ok(&self) -> bool {
            // Driven by `smoke_ok` so a test can wedge the COMPUTE path (the #175
            // liveness-heartbeat tests) independently of the control plane; defaults
            // true (a healthy fake decodes).
            self.smoke_ok.load(Ordering::Relaxed)
        }
    }

    /// A minimal [`Model`] for reconcile-wiring tests — only `id` is load-bearing
    /// (the FakeServer ignores the rest); the planned served window rides on the
    /// `ServingTarget`, not here.
    fn fake_model(id: &str) -> Model {
        use crate::model_registry::types::{Arch, MultiPartyChatStrategy};
        Model {
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
            vec![Arc::new(MockCapacitySource::new(ResourceKind::Vram, 53 * GB))],
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
        let found = after
            .iter()
            .find(|f| f.model_id == id)
            .expect("a freshly-pulled Ready model becomes a serving candidate on the next snapshot");
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
        );
        assert_eq!(up.active_model.as_deref(), Some("coder-14b"));
        assert!(up.ready);
        assert!(up.base_url.ends_with("/v1"));
        assert_eq!(up.adapters, genes, "live snapshot carries the loaded genome set");
        assert_eq!(
            up.served_context_window, 11008,
            "ready snapshot carries the real per-slot window personas budget to"
        );
        assert_eq!(
            up.lanes, 4,
            "ready snapshot carries the --parallel lane count for total-KV accounting"
        );

        let already =
            snapshot_from_outcome(&EnsureOutcome::AlreadyServing, "coder-14b", &genes, 11008, 4);
        assert_eq!(already.active_model.as_deref(), Some("coder-14b"));
        assert!(already.ready);
        assert_eq!(already.served_context_window, 11008);
        assert_eq!(already.lanes, 4);

        // Ready outcome but the served window was unreadable (0) → publish the gap,
        // NOT a ready snapshot with a zero window a persona would budget against.
        let windowless = snapshot_from_outcome(
            &EnsureOutcome::Spawned { model: "m".into() },
            "coder-14b",
            &genes,
            0,
            4,
        );
        assert_eq!(windowless.active_model, None, "ready-but-no-window → nothing live");
        assert!(!windowless.ready);
        assert_eq!(windowless.served_context_window, 0);
        assert_eq!(windowless.lanes, 0, "empty snapshot carries no lanes");

        let degraded = snapshot_from_outcome(
            &EnsureOutcome::Degraded { reason: "x".into() },
            "coder-14b",
            &genes,
            11008,
            4,
        );
        assert_eq!(degraded.active_model, None, "degraded → nothing live");
        assert!(!degraded.ready);
        assert!(degraded.adapters.is_empty(), "degraded → no genome claimed");
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
        assert_eq!(sticky_served_window(31_744, 2, &live(true, 49_664, 2)), 49_664);
        assert_eq!(sticky_served_window(31_744, 2, &live(true, 49_664, 1)), 31_744);
        assert_eq!(sticky_served_window(60_000, 2, &live(true, 49_664, 2)), 60_000);
        assert_eq!(sticky_served_window(31_744, 2, &live(false, 0, 0)), 31_744);
    }

    // what this catches: a published plan drives a reconcile that brings the
    // server up and publishes a ready ServingSnapshot for that model — the
    // plan→reality wiring. Regression here = the daemon decides but never acts.
    #[tokio::test]
    async fn reconcile_brings_planned_model_up() {
        let serves = Arc::new(AtomicUsize::new(0));
        let daemon = daemon_with(Arc::new(FakeServer::healthy(serves.clone(), true)));

        // Publish a plan (most-capable fitting model = coder-14b).
        let budget = HostBudget { usable_bytes: 45 * GB, perf_cores: 6 };
        let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true).unwrap()];
        daemon.publish_plan(budget, &candidates);

        let handle = daemon.reconcile_to_plan().expect("a reconcile should be spawned");
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
            active_model: Some("coder-14b".into()),
            ready: true,
            base_url: serving_v1_url(),
            adapters: Vec::new(),
            served_context_window: 11008,
            lanes: 4,
        });
        let budget = HostBudget { usable_bytes: 45 * GB, perf_cores: 6 };
        let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true).unwrap()];
        daemon.publish_plan(budget, &candidates);

        assert!(daemon.reconcile_to_plan().is_none(), "already serving → no reconcile");
        assert_eq!(serves.load(Ordering::SeqCst), 0, "no relaunch");
    }

    /// Believe-ready snapshot fixture for the liveness-heartbeat tests.
    fn ready_snapshot() -> ServingSnapshot {
        ServingSnapshot {
            active_model: Some("coder-14b".into()),
            ready: true,
            base_url: serving_v1_url(),
            adapters: Vec::new(),
            served_context_window: 11008,
            lanes: 4,
        }
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

        let budget = HostBudget { usable_bytes: 45 * GB, perf_cores: 6 };
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
            active_model: Some("coder-14b".into()),
            ready: true,
            base_url: serving_v1_url(),
            adapters: Vec::new(),
            served_context_window: plan_window / 4,
            lanes: 4,
        });
        daemon
            .reconcile_to_plan()
            .expect("starved window must trigger a re-home")
            .await
            .unwrap();
        assert_eq!(serves.load(Ordering::SeqCst), 1, "one relaunch");

        // Within hysteresis (> half the plan) → hold, no churn.
        let _ = daemon.serving_tx.send_replace(ServingSnapshot {
            active_model: Some("coder-14b".into()),
            ready: true,
            base_url: serving_v1_url(),
            adapters: Vec::new(),
            served_context_window: plan_window / 2 + 256,
            lanes: 4,
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
            active_model: Some("stale".into()),
            ready: true,
            base_url: serving_v1_url(),
            adapters: Vec::new(),
            served_context_window: 11008,
            lanes: 4,
        });
        let budget = HostBudget { usable_bytes: 45 * GB, perf_cores: 6 };
        daemon.publish_plan(budget, &[]); // no candidates → plan None

        assert!(daemon.reconcile_to_plan().is_none(), "no plan → no reconcile spawned");
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

        let budget = HostBudget { usable_bytes: 45 * GB, perf_cores: 6 };
        let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true).unwrap()];
        daemon.publish_plan(budget, &candidates);

        daemon.reconcile_to_plan().expect("spawned").await.unwrap();
        let snap = daemon.subscribe_serving().borrow().clone();
        assert!(!snap.ready, "degraded → not ready");
        assert_eq!(snap.active_model, None);
        assert!(!daemon.reconciling.load(Ordering::SeqCst), "gate cleared for retry");

        // Gate cleared → a retry actually spawns again.
        daemon.reconcile_to_plan().expect("retry spawned").await.unwrap();
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

        let budget = HostBudget { usable_bytes: 45 * GB, perf_cores: 6 };
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
        assert_eq!(resolve(id, 8192, 2), 0, "no weights on disk → nothing to attribute");
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
            !daemon.resource_daemon.consumer_ids().contains(&SERVING_CONSUMER_ID.to_string()),
            "not registered until the daemon wires itself in"
        );
        daemon.register_as_consumer();
        assert!(
            daemon.resource_daemon.consumer_ids().contains(&SERVING_CONSUMER_ID.to_string()),
            "serving must register itself as a measured consumer with the authority"
        );
    }
}
