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
    plan_serving, plan_serving_stable, HostBudget, ModelFootprint, ServingPlan,
};
use crate::gpu::GpuMemoryManager;
use crate::inference::llama_server::{
    ensure_model_serving, serving_v1_url, EnsureOutcome, LlamaServerControl, LlamaServerProcess,
    ServingSnapshot,
};
use crate::persona::hw_tier_descriptor::HwTierCategory;
use crate::model_registry::types::{Capability, Model};
use crate::model_registry::Registry;
use crate::runtime::message_bus::MessageBus;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::system_resources::SystemResourceMonitor;
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::sync::atomic::{AtomicBool, Ordering};
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

/// Per-lane context tokens we size the KV estimate against. A serving cap, not
/// the model's trained ceiling — lanes share the budget, so each is planned at
/// a sane working context rather than the full 256k.
const PLANNED_CTX_TOKENS: u64 = 8192;

/// Bus topic the live [`ServingSnapshot`] is emitted on whenever it changes.
/// Subscribers (embedding, supervisor, inference_session, ai_provider) declare
/// this in their `event_subscriptions` and cache the latest in `handle_event`
/// instead of probing the process — the cbar pipeline-stage shape: one organ
/// emits its state, everything that needs it subscribes. Because the bus spans
/// the grid, a remote lease allocator subscribes to the SAME topic. Routed by
/// name (no body parse in middleware), payload fans out as a shared pointer,
/// emitted only on a rare state change — never on the token hot path.
const SERVING_SNAPSHOT_EVENT: &str = "serving.snapshot";

pub struct ServingDaemonModule {
    gpu: Arc<GpuMemoryManager>,
    /// Live system memory monitor — the budget comes from what's actually FREE
    /// right now (`available_bytes`), not total capacity. On unified memory
    /// this drops when anything else grabs memory (a game, a build), so the
    /// plan ebbs and flows organically.
    system: Arc<SystemResourceMonitor>,
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
    /// The message bus, captured at `initialize`. Set once; `None` in tests
    /// constructed via `with_control` without a live runtime, so the emit is a
    /// silent no-op there. The daemon publishes [`ServingSnapshot`] changes on
    /// `SERVING_SNAPSHOT_EVENT` so any subscriber (local or grid) gets the live
    /// serving state pushed — no point-to-point receiver plumbing.
    bus: OnceLock<Arc<MessageBus>>,
}

impl ServingDaemonModule {
    pub fn new(gpu: Arc<GpuMemoryManager>, system: Arc<SystemResourceMonitor>) -> Self {
        Self::with_control(gpu, system, Arc::new(LlamaServerProcess::new()))
    }

    /// Construct with an injected serving control. Production uses
    /// [`Self::new`] (real `LlamaServerProcess`); tests inject a fake to drive
    /// the reconcile decision without a live process.
    pub fn with_control(
        gpu: Arc<GpuMemoryManager>,
        system: Arc<SystemResourceMonitor>,
        server: Arc<dyn LlamaServerControl>,
    ) -> Self {
        let (plan_tx, _rx) = watch::channel(None);
        let (serving_tx, _srx) = watch::channel(ServingSnapshot::empty());
        Self {
            gpu,
            system,
            plan_tx,
            server,
            serving_tx,
            reconciling: Arc::new(AtomicBool::new(false)),
            bus: OnceLock::new(),
        }
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

    /// Honest serving budget for this host, RIGHT NOW — from the live free
    /// memory the monitor reports, capped at the device's physical VRAM. This
    /// is the organic signal: free memory drops under load, the budget shrinks,
    /// the plan picks fewer lanes / a smaller model; load clears, it flows back.
    fn host_budget(&self) -> HostBudget {
        let available = self.system.snapshot().memory.available_bytes;
        host_budget_from(available, self.gpu.total_vram_bytes(), perf_cores())
    }

    /// Compute the current serving plan from the live host snapshot + on-disk
    /// models, WITHOUT relying on a tick having run. The boot path calls this
    /// to drive the spawner before the tick loop starts — single source of
    /// truth for "what model + how many lanes."
    pub fn compute_plan(&self) -> Option<ServingPlan> {
        let candidates = candidates_from_registry(crate::model_registry::global());
        plan_serving(self.host_budget(), &candidates)
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
        let candidates = candidates_from_registry(crate::model_registry::global());
        self.publish_plan(budget, &candidates);
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
        let desired = match self.plan_tx.borrow().as_ref() {
            Some(plan) => plan.base_model_id.clone(),
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

        {
            let live = self.serving_tx.borrow();
            if live.ready && live.active_model.as_deref() == Some(desired.as_str()) {
                return None; // already serving the right model — no relaunch
            }
        }

        // One reconcile at a time. If the swap finds `true`, another is already
        // running; skip rather than stack relaunches.
        if self.reconciling.swap(true, Ordering::AcqRel) {
            return None;
        }

        let server = self.server.clone();
        let serving_tx = self.serving_tx.clone();
        let reconciling = self.reconciling.clone();
        let bus = self.bus.get().cloned();
        Some(tokio::spawn(async move {
            let outcome = ensure_model_serving(server.as_ref(), &desired).await;
            let snapshot = snapshot_from_outcome(&outcome, &desired);
            crate::probe!(
                class = "serving.reconcile",
                desired = desired.as_str(),
                ready = snapshot.ready,
                active = snapshot.active_model.as_deref().unwrap_or("<none>"),
                "serving reconcile complete",
            );
            // Emit on the bus first (fan-out to every subscriber + the grid),
            // then update the in-process watch view.
            Self::emit_serving(bus.as_ref(), &snapshot);
            let _ = serving_tx.send_replace(snapshot);
            reconciling.store(false, Ordering::Release);
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
        match plan_serving_stable(budget, candidates, incumbent.as_deref()) {
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

/// Serving budget from LIVE free memory, capped at physical VRAM, minus
/// headroom. Pure for tests. `available_bytes` is the monitor's current free
/// memory (already net of everything else running); we never plan above what's
/// free, nor above what the device physically has.
pub fn host_budget_from(available_bytes: u64, total_vram_bytes: u64, perf_cores: u32) -> HostBudget {
    let live = available_bytes.min(total_vram_bytes);
    let usable = (live as f64 * SERVING_BUDGET_FRACTION) as u64;
    HostBudget {
        usable_bytes: usable,
        perf_cores: perf_cores.max(1),
    }
}

/// Performance-core proxy for the lane cap. `num_cpus::get_physical()` is the
/// portable floor; on Apple Silicon it over-counts (efficiency cores), but the
/// `MAX_LANES` ceiling in the classifier is the binding cap on capable boxes,
/// so this only matters on tiny machines where physical-core count is a fine
/// proxy. Refined to true P-core detection in a later slice.
fn perf_cores() -> u32 {
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
    // Coarse per-lane KV: scale per-token KV with model size (more weights ≈
    // more layers ≈ more KV/token), times the planned per-lane context (capped
    // at the model's trained window). ~weights/20k bytes per token, floored.
    let kv_per_token = (weights_bytes / 20_000).max(20_000);
    let ctx = PLANNED_CTX_TOKENS.min((context_window as u64).max(2048));
    let per_lane_kv_bytes = kv_per_token.saturating_mul(ctx);

    // Coarse capability rank: GB of weights (bigger ≈ more capable within a
    // family), +bonus for tool/code capability. Saturates into u8.
    let gb = (weights_bytes / 1_000_000_000).min(250) as u16;
    let tool_bonus = if tool_capable { 2 } else { 0 };
    let capability_rank = gb.saturating_add(tool_bonus).min(255) as u8;

    Some(ModelFootprint {
        model_id: id.to_string(),
        weights_bytes,
        per_lane_kv_bytes,
        capability_rank,
    })
}

/// All on-disk servable models in the registry as footprints.
pub fn candidates_from_registry(registry: &Registry) -> Vec<ModelFootprint> {
    registry.models().filter_map(footprint_for).collect()
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

/// Map a reconcile [`EnsureOutcome`] to the published [`ServingSnapshot`].
/// Pure (no IO) so the mapping is unit-tested directly. A live/spawned model is
/// `ready` with the served base url; a degraded reconcile publishes "nothing
/// live" — never a half-true "ready but no model" ([[fallbacks-are-illegal-fail-loud]]).
fn snapshot_from_outcome(outcome: &EnsureOutcome, desired: &str) -> ServingSnapshot {
    match outcome {
        EnsureOutcome::AlreadyServing | EnsureOutcome::Spawned { .. } => ServingSnapshot {
            active_model: Some(desired.to_string()),
            ready: true,
            base_url: serving_v1_url(),
        },
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
        // Plan once at boot so the decision is published before the first tick,
        // then kick the first reconcile so the server comes up promptly rather
        // than waiting a full tick interval. The reconcile runs detached.
        self.recompute();
        let _ = self.reconcile_to_plan();
        Ok(())
    }

    async fn tick(&self) -> Result<(), String> {
        // Re-decide the plan (fast), then bring the running server in line with
        // it. The reconcile is fast-to-decide and spawns the slow relaunch off
        // the tick, so the tick never blocks on model load.
        self.recompute();
        let _ = self.reconcile_to_plan();
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        match command {
            "serving/plan" => {
                // The current decision, for personas + operators to inspect.
                // The `rationale` field explains the "why" in plain words.
                let plan = self.plan_tx.borrow().clone();
                CommandResult::json(&plan)
            }
            "serving/status" => {
                // The live serving state — which model is actually up, ready,
                // and on what url. The "did the plan become reality?" view.
                let snapshot = self.serving_tx.borrow().clone();
                CommandResult::json(&snapshot)
            }
            other => Err(format!("serving-daemon: unknown command '{other}'")),
        }
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
        let b = host_budget_from(40 * GB, 53 * GB, 6);
        assert!(b.usable_bytes < 40 * GB, "must reserve headroom");
        assert!(b.usable_bytes >= 30 * GB, "but most of free is ours: {}", b.usable_bytes);

        // Organic: less free memory → smaller budget (a game grabbed memory).
        let busy = host_budget_from(6 * GB, 53 * GB, 6);
        assert!(busy.usable_bytes < b.usable_bytes, "less free → smaller budget");

        // Never plan above physical VRAM even if the OS reports more free RAM
        // (unified memory: free RAM can exceed the VRAM serving ceiling).
        let capped = host_budget_from(100 * GB, 53 * GB, 6);
        assert!(capped.usable_bytes <= 53 * GB, "capped at physical VRAM");

        assert_eq!(host_budget_from(8 * GB, 8 * GB, 0).perf_cores, 1, "cores floored at 1");
    }

    // what this catches: footprint estimate is honest about weights (passed
    // through), tool capability bumps the rank, KV is non-zero, and zero
    // weights → no footprint (we only offer what we can actually serve).
    #[test]
    fn footprint_from_parts_is_footprint_aware() {
        let fp = footprint_from_parts("present", 3 * GB, 8192, true).unwrap();
        assert_eq!(fp.model_id, "present");
        assert_eq!(fp.weights_bytes, 3 * GB);
        assert!(fp.per_lane_kv_bytes > 0);
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
        let daemon = ServingDaemonModule::new(gpu, system);
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

    use crate::inference::llama_server::LlamaServerError;
    use std::sync::atomic::AtomicUsize;

    /// Fake serving control: counts serve() calls and reports nothing live
    /// (Unreachable), so reconcile always decides to (re)serve. A daemon-level
    /// stub for the reconcile WIRING; the reconcile DECISION itself is tested in
    /// `inference::llama_server` against its own fake.
    struct FakeServer {
        serves: Arc<AtomicUsize>,
        ok: bool,
    }

    #[async_trait]
    impl LlamaServerControl for FakeServer {
        async fn active_model(&self) -> Result<Option<String>, LlamaServerError> {
            Err(LlamaServerError::Unreachable("test: nothing up".into()))
        }
        async fn serve(&self, _model_id: &str) -> Result<(), LlamaServerError> {
            self.serves.fetch_add(1, Ordering::SeqCst);
            if self.ok {
                Ok(())
            } else {
                Err(LlamaServerError::Spawn("test boom".into()))
            }
        }
    }

    fn daemon_with(server: Arc<dyn LlamaServerControl>) -> ServingDaemonModule {
        let gpu = Arc::new(GpuMemoryManager::simulated("Apple M5 Pro", 53 * GB));
        let system = Arc::new(SystemResourceMonitor::new());
        ServingDaemonModule::with_control(gpu, system, server)
    }

    // what this catches: the EnsureOutcome → ServingSnapshot mapping. A live or
    // spawned model is ready with the served base url; a degraded reconcile
    // publishes "nothing live", never a half-true ready-with-no-model.
    #[test]
    fn snapshot_mapping_is_honest() {
        let up = snapshot_from_outcome(&EnsureOutcome::Spawned { model: "m".into() }, "coder-14b");
        assert_eq!(up.active_model.as_deref(), Some("coder-14b"));
        assert!(up.ready);
        assert!(up.base_url.ends_with("/v1"));

        let already = snapshot_from_outcome(&EnsureOutcome::AlreadyServing, "coder-14b");
        assert_eq!(already.active_model.as_deref(), Some("coder-14b"));
        assert!(already.ready);

        let degraded =
            snapshot_from_outcome(&EnsureOutcome::Degraded { reason: "x".into() }, "coder-14b");
        assert_eq!(degraded.active_model, None, "degraded → nothing live");
        assert!(!degraded.ready);
    }

    // what this catches: a published plan drives a reconcile that brings the
    // server up and publishes a ready ServingSnapshot for that model — the
    // plan→reality wiring. Regression here = the daemon decides but never acts.
    #[tokio::test]
    async fn reconcile_brings_planned_model_up() {
        let serves = Arc::new(AtomicUsize::new(0));
        let daemon = daemon_with(Arc::new(FakeServer { serves: serves.clone(), ok: true }));

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
        let daemon = daemon_with(Arc::new(FakeServer { serves: serves.clone(), ok: true }));

        // Pretend coder-14b is already up and ready.
        let _ = daemon.serving_tx.send_replace(ServingSnapshot {
            active_model: Some("coder-14b".into()),
            ready: true,
            base_url: serving_v1_url(),
        });
        let budget = HostBudget { usable_bytes: 45 * GB, perf_cores: 6 };
        let candidates = vec![footprint_from_parts("coder-14b", 9 * GB, 8192, true).unwrap()];
        daemon.publish_plan(budget, &candidates);

        assert!(daemon.reconcile_to_plan().is_none(), "already serving → no reconcile");
        assert_eq!(serves.load(Ordering::SeqCst), 0, "no relaunch");
    }

    // what this catches: no servable plan (empty registry) publishes the empty
    // snapshot so readers (and a grid allocator) see "nothing live here" and can
    // route the lease elsewhere — the Intel-Mac/weak-node path.
    #[tokio::test]
    async fn no_plan_publishes_empty_snapshot() {
        let serves = Arc::new(AtomicUsize::new(0));
        let daemon = daemon_with(Arc::new(FakeServer { serves: serves.clone(), ok: true }));

        // Seed a live snapshot, then publish an empty plan → must clear to empty.
        let _ = daemon.serving_tx.send_replace(ServingSnapshot {
            active_model: Some("stale".into()),
            ready: true,
            base_url: serving_v1_url(),
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
        let daemon = daemon_with(Arc::new(FakeServer { serves: serves.clone(), ok: false }));

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
        let daemon = daemon_with(Arc::new(FakeServer { serves: serves.clone(), ok: true }));
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
}
