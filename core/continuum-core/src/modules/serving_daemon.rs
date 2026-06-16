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
use crate::persona::hw_tier_descriptor::HwTierCategory;
use crate::model_registry::types::{Capability, Model};
use crate::model_registry::Registry;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::system_resources::SystemResourceMonitor;
use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::watch;

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
}

impl ServingDaemonModule {
    pub fn new(gpu: Arc<GpuMemoryManager>, system: Arc<SystemResourceMonitor>) -> Self {
        let (plan_tx, _rx) = watch::channel(None);
        Self {
            gpu,
            system,
            plan_tx,
        }
    }

    /// Subscribe to the published serving plan. Consumers (scheduler, spawner)
    /// hold the receiver and react to plan changes — the ebb/flow seam.
    pub fn subscribe(&self) -> watch::Receiver<Option<ServingPlan>> {
        self.plan_tx.subscribe()
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

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        // Plan once at boot so the decision is published before the first tick.
        self.recompute();
        Ok(())
    }

    async fn tick(&self) -> Result<(), String> {
        self.recompute();
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
}
