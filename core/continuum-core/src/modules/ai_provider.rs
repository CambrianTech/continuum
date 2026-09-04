//! AIProviderModule — Adapter-based AI provider system
//!
//! Uses the adapter pattern (like ORM) for pluggable AI providers.
//! Single entry point for all text generation with tool calling support.
//!
//! Supported providers (via adapters):
//! - DeepSeek (deepseek-chat, deepseek-reasoner)
//! - Anthropic (claude-sonnet-4-5, claude-opus-4, claude-3-5-haiku)
//! - OpenAI (gpt-4, gpt-4o)
//! - Together AI (llama-3.1-70b)
//! - Groq (llama-3.1-8b-instant)
//! - Fireworks (deepseek-v3)
//! - XAI (grok-3)
//! - Google (gemini-2.0-flash)
//!
//! Commands:
//! - ai/generate: Generate text with optional tool calling
//! - ai/providers/list: List available providers
//! - ai/providers/health: Check provider health

use crate::ai::{
    adapter::{AIProviderAdapter, InferenceDevice},
    AdapterRegistry, AnthropicAdapter, OpenAICompatibleAdapter, RoutingInfo, TextGenerationRequest,
    TextGenerationResponse,
};
use crate::runtime::{
    CommandResult, LateBound, ModuleConfig, ModuleContext, ModuleLogger, ModulePriority,
    ServiceModule,
};
use crate::secrets::get_secret;
use async_trait::async_trait;
use once_cell::sync::Lazy;
use serde_json::Value;
use std::any::Any;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{OnceCell, RwLock};

/// Provider ID for the Docker Model Runner adapter — single source of truth
/// shared between init-time registration and the watchdog tick.
const DMR_PROVIDER_ID: &str = "docker-model-runner";

/// How often the watchdog probes DMR. Five seconds is the same cadence
/// as the PressureBroker tick — fast enough to recover within ~one
/// chat turn after Docker Desktop restarts; slow enough that the probe
/// (a one-second TCP connect) is essentially free relative to the tick.
const DMR_TICK_INTERVAL: Duration = Duration::from_secs(5);

/// Consecutive failed-probe ticks before the watchdog escalates from
/// "transient blip" to "this is broken — tell the user." At 5s ticks,
/// 6 = 30 seconds, which is the threshold the resource architecture
/// uses for "loud failure, never silent."
const DMR_DOWN_WARN_THRESHOLD_TICKS: u64 = 6;

/// Inline wait at boot for the serving snapshot to report a READY model before
/// registering the gateway adapter on the spot. Sized for the warm path — a
/// core restart over an already-resident model, or an adoption of a healthy
/// server — where the daemon's snapshot is ready within a couple of ticks, so
/// boot reports `✓ inference` cleanly with zero window where `select()` misses
/// the gateway. Deliberately SHORT: a cold large GGUF will not finish loading
/// in this window, and we do NOT want boot to block on it — that case hands off
/// to the reactive watcher below ([[fallbacks-are-illegal-fail-loud]], task #71).
const GATEWAY_FAST_PATH_WAIT: Duration = Duration::from_secs(8);

// (GATEWAY_REACTIVE_CAP retired with the one-shot watcher: the persistent
// gateway-sync task, card ed3661c4, follows the serving snapshot for the life
// of the process — there is no wait to cap; a wedged serving plan stays the
// serving daemon's loud failure to own.)

/// One DMR endpoint discovered by `probe_dmr`. The base_url is None for
/// localhost — the adapter's default constructor already points at
/// `localhost:12434`. A `Some(url)` means the in-container variant
/// where we resolved `model-runner.docker.internal`.
#[derive(Debug, Clone)]
struct DmrEndpoint {
    base_url: Option<String>,
}

/// Global singleton registry - survives module recreation on server restart
static GLOBAL_REGISTRY: Lazy<Arc<RwLock<AdapterRegistry>>> =
    Lazy::new(|| Arc::new(RwLock::new(AdapterRegistry::new())));

/// Track if we've done first-time initialization
static INITIALIZED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Public accessor for the global adapter registry.
/// Used by the HTTP inference endpoint to share adapters with AIProviderModule.
pub fn global_registry() -> Arc<RwLock<AdapterRegistry>> {
    GLOBAL_REGISTRY.clone()
}

/// AIProviderModule - ServiceModule implementation for AI inference
pub struct AIProviderModule {
    registry: Arc<RwLock<AdapterRegistry>>,
    log: OnceCell<Arc<ModuleLogger>>,
    /// GPU memory manager — passed to CandleAdapter for VRAM allocation tracking.
    gpu_manager: Option<Arc<crate::gpu::memory_manager::GpuMemoryManager>>,
    /// DMR watchdog state — counts consecutive down-probe ticks so we can
    /// escalate from quiet recovery to loud user-visible failure at the
    /// 30-second threshold. Atomic so the tick (`&self`) updates it
    /// without taking a write lock on the module.
    dmr_consecutive_down_ticks: Arc<AtomicU64>,
    /// Substrate-wide command executor — installed by `start_server` after
    /// the executor is built. Used by the TS fallthrough for unmigrated
    /// `ai/*` commands (task #224 replaced the deleted free helper).
    executor: LateBound<crate::runtime::CommandExecutor>,
}

/// Largest context this host can safely serve for `model`: its trained window, capped by what
/// the GOVERNED memory budget can hold at this model's real KV bytes/token. The replacement for
/// the old `KV_SAFE_CONTEXT_CEILING` placeholder — same authority `plan_serving` and the eval
/// lane size against, never a raw probe and never a constant.
///
/// Returns the trained window unchanged when the host is ungoverned or the model can't be sized:
/// an unknown budget must not become an invented ceiling.
fn kv_safe_context(model: &crate::model_registry::Model) -> u32 {
    let trained = model.context_window;
    let Some(fp) = crate::modules::serving_daemon::footprint_for(model) else {
        return trained;
    };
    if fp.kv_per_token == 0 {
        return trained;
    }
    let Some(budget) = crate::resources::ResourceDaemon::global()
        .map(|d| crate::modules::serving_daemon::governed_host_budget(&d).usable_bytes)
        .filter(|b| *b > 0)
    else {
        return trained;
    };
    let Some(kv_budget) = budget
        .checked_sub(fp.weights_bytes)
        .and_then(|b| b.checked_sub(fp.compute_buffer_per_lane()))
    else {
        // Weights alone exceed the budget — the serving planner refuses this model anyway;
        // don't also invent a window here.
        return trained;
    };
    let fits = (kv_budget / fp.kv_per_token).min(u32::MAX as u64) as u32;
    trained
        .min(fits)
        .max(crate::cognition::serving_plan::MIN_SERVE_CTX)
}

impl AIProviderModule {
    pub fn new() -> Self {
        Self {
            registry: GLOBAL_REGISTRY.clone(),
            log: OnceCell::new(),
            gpu_manager: None,
            dmr_consecutive_down_ticks: Arc::new(AtomicU64::new(0)),
            executor: LateBound::new("ai-provider::executor"),
        }
    }

    /// Create with GPU memory manager for VRAM-aware local inference.
    pub fn with_gpu_manager(
        gpu_manager: Arc<crate::gpu::memory_manager::GpuMemoryManager>,
    ) -> Self {
        Self {
            registry: GLOBAL_REGISTRY.clone(),
            log: OnceCell::new(),
            gpu_manager: Some(gpu_manager),
            dmr_consecutive_down_ticks: Arc::new(AtomicU64::new(0)),
            executor: LateBound::new("ai-provider::executor"),
        }
    }

    /// Probe DMR (Docker Model Runner) reachability and return its endpoint
    /// if reachable. Single source of truth for "is DMR up?" — used by both
    /// init-time registration and the watchdog tick, so the two never drift
    /// on what counts as "available."
    ///
    /// Returns `Some(DmrEndpoint)` when reachable, `None` otherwise. Tries
    /// localhost (host-native Docker Desktop) first, falls back to the
    /// container-internal DNS name if `/.dockerenv` exists. Uses short
    /// connect timeouts so a slow DNS or firewall block can't stall the
    /// tick.
    fn probe_dmr() -> Option<DmrEndpoint> {
        let localhost_ok = std::net::TcpStream::connect_timeout(
            &"127.0.0.1:12434".parse().unwrap(),
            Duration::from_secs(1),
        )
        .is_ok();
        if localhost_ok {
            return Some(DmrEndpoint { base_url: None });
        }

        // Not on localhost — check if we're inside a Docker container.
        // model-runner.docker.internal resolves from inside Docker
        // Desktop containers on Mac, Linux, and Windows (WSL2).
        if !std::path::Path::new("/.dockerenv").exists() {
            return None;
        }
        use std::net::ToSocketAddrs;
        let internal_ok = "model-runner.docker.internal:80"
            .to_socket_addrs()
            .ok()
            .and_then(|mut addrs| addrs.next())
            .map(|addr| std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(2)).is_ok())
            .unwrap_or(false);
        if internal_ok {
            Some(DmrEndpoint {
                base_url: Some("http://model-runner.docker.internal/engines/llama.cpp".to_string()),
            })
        } else {
            None
        }
    }

    /// Build a DMR adapter for the given endpoint. Same construction path
    /// used by both init-time registration and watchdog re-registration —
    /// the two never produce different-shaped adapters.
    fn build_dmr_adapter(endpoint: &DmrEndpoint) -> Box<dyn AIProviderAdapter> {
        // Note: returns `Box` because the watchdog re-registration
        // path needs ownership of a Sized adapter to call
        // `initialize` on it before wrapping in `Arc`. The
        // init-then-register caller does:
        //   let mut a = Self::build_dmr_adapter(...);
        //   a.initialize().await?;
        //   registry.register(Arc::from(a), priority);
        // — `Arc::from(Box<dyn T>)` is a zero-copy ownership flip per
        // [[init-once-handle-then-lease-zero-copy-refs]].
        let adapter = if let Some(url) = &endpoint.base_url {
            OpenAICompatibleAdapter::from_registry("docker-model-runner")
                .with_runtime_base_url(url.clone())
        } else {
            OpenAICompatibleAdapter::from_registry("docker-model-runner")
        };
        Box::new(adapter)
    }
}

/// Build the user-visible error message when `select()` returns None.
/// Distinguishes:
///   - "no providers at all" (config issue — surfaces config.env hint)
///   - "asked for local but DMR is down" (Docker Desktop needs to be running)
///   - "asked for a specific provider/model that isn't here" (existing message)
///
/// Hoisted out of both `ai/generate` and the convenience `generate_text` so
/// the two paths report the same diagnosis.
pub(crate) fn select_failure_message(
    registry: &AdapterRegistry,
    requested_provider: Option<&str>,
    requested_model: Option<&str>,
) -> String {
    let available = registry.available();
    if available.is_empty() {
        return "No AI providers configured. Add API keys to ~/.continuum/config.env, \
                or start Docker Desktop for local AI."
            .to_string();
    }
    // The "local" sentinel means "give me whatever the best local adapter is."
    // If the user asked for that and DMR isn't in the registry, the watchdog
    // either (a) hasn't seen DMR come up yet or (b) saw it crash and dropped
    // it. Either way, the actionable message is "start Docker Desktop."
    let asked_local = requested_provider == Some("local");
    let dmr_registered = registry.is_registered(DMR_PROVIDER_ID);
    if asked_local && !dmr_registered {
        return format!(
            "Local AI is unavailable — Docker Desktop is not running or Docker Model \
             Runner isn't enabled. To enable: docker desktop enable model-runner --tcp=12434. \
             Other available providers: {:?}",
            available
        );
    }
    // A refusal the caller can ACT on. `available` is provider ids; a caller
    // who named a model (or nothing) needs the model ids the lanes actually
    // serve, or they are stuck guessing — the weak-node consumer case, where
    // the caller has no local registry at all (card a466fdd4).
    let served: Vec<String> = registry
        .served_models()
        .into_iter()
        .map(|(provider, model)| format!("{provider}={model}"))
        .collect();
    match (requested_provider, requested_model) {
        (Some(provider), _) => format!(
            "Provider {provider:?} is not available (model={requested_model:?}). \
             Available providers: {available:?}; served models (provider=model): {served:?}."
        ),
        (None, Some(model)) => format!(
            "Model {model:?} is not served by any available provider. \
             Served models (provider=model): {served:?}. \
             Pass `model` as one of those, or `provider` as one of {available:?}."
        ),
        (None, None) => format!(
            "No provider or model specified — the substrate never picks one for you. \
             Pass `provider` as one of {available:?}, or `model` as one of the served \
             models (provider=model): {served:?}."
        ),
    }
}

/// Build + initialize the llama-server gateway adapter pointed at a ready
/// serving snapshot's `base_url`. ONE construction site shared by the inline
/// fast-path and the reactive watcher (compression — the adapter is wired the
/// same way whether it registers at boot or seconds later). Returns the
/// initialized adapter ready to register, or a Display error on init failure.
async fn build_gateway_adapter(
    base_url: String,
    active_model: Option<&str>,
) -> Result<OpenAICompatibleAdapter, String> {
    let mut a = OpenAICompatibleAdapter::from_registry(crate::inference::llama_server::PROVIDER_ID)
        .with_runtime_base_url(base_url);
    a.initialize().await.map_err(|e| e.to_string())?;
    // The snapshot's active_model ALWAYS selects, whatever /v1/models claimed —
    // the daemon's reconcile verified it against the live process; the catalog
    // is derived and can misname the model (Windows alias-mangling put the GGUF
    // path in data[].id and select() refused a healthy lane, 5090 2026-07-24).
    if let Some(model) = active_model {
        a.ensure_runtime_model(model);
    }
    Ok(a)
}

// Re-open the AIProviderModule impl block so the rest of the methods
// (parse_request, response_to_json, etc.) stay where they were.
impl AIProviderModule {
    /// The persistent gateway-sync task (card ed3661c4). Follows the serving
    /// daemon's `watch` snapshot for the LIFE of the process: whenever a READY
    /// snapshot's `(base_url, active_model)` differs from what the registry
    /// currently advertises, rebuild the gateway adapter against the live
    /// server (fresh `/v1/models` catalog) and REPLACE the registration.
    /// `initial` seeds the already-registered pair from the boot fast-path so
    /// a warm boot doesn't churn one redundant re-register.
    ///
    /// Failure shape: a build/init error while ready (server mid-warm) retries
    /// on a short interval — bounded work, no lock held across waits, and the
    /// registry keeps its previous (possibly stale) entry until the rebuild
    /// succeeds, at which point it is atomically swapped under the write lock.
    fn spawn_gateway_sync(
        registry_arc: Arc<RwLock<AdapterRegistry>>,
        initial: Option<(String, Option<String>)>,
    ) {
        tokio::spawn(async move {
            use crate::runtime::boot_status::{boot_status, BootStatusKind};
            const RETRY: std::time::Duration = std::time::Duration::from_secs(3);
            // The daemon installs the watch at its own init; poll briefly
            // until it exists (boot ordering, not a failure state).
            let mut rx = loop {
                match crate::inference::llama_server::serving_state_receiver() {
                    Some(rx) => break rx,
                    None => tokio::time::sleep(RETRY).await,
                }
            };
            let mut synced = initial;
            let mut announced_first = synced.is_some();
            loop {
                let snap = rx.borrow_and_update().clone();
                if snap.ready {
                    let want = (snap.base_url.clone(), snap.active_model.clone());
                    if synced.as_ref() != Some(&want) {
                        match build_gateway_adapter(want.0.clone(), want.1.as_deref()).await {
                            Ok(a) => {
                                let mut reg = registry_arc.write().await;
                                // Replace, never append: deregister sweeps the
                                // base key AND any #N collision duplicates.
                                reg.deregister(crate::inference::llama_server::PROVIDER_ID);
                                reg.register(Arc::new(a), 9);
                                drop(reg);
                                let short = want
                                    .1
                                    .as_deref()
                                    .map(|m| m.rsplit('/').next().unwrap_or(m))
                                    .unwrap_or("(unknown)");
                                if !announced_first {
                                    announced_first = true;
                                    boot_status(
                                        "inference",
                                        BootStatusKind::Ok,
                                        &format!(
                                            "inference gateway registered (sync) — serving {short} @ {}",
                                            want.0
                                        ),
                                    );
                                } else {
                                    crate::probe!(
                                        class = "ai.gateway_resync",
                                        base_url = want.0.as_str(),
                                        model = short,
                                        "gateway adapter re-synced to the live serving snapshot",
                                    );
                                }
                                synced = Some(want);
                            }
                            Err(e) => {
                                crate::probe!(
                                    class = "ai.gateway_resync_retry",
                                    base_url = want.0.as_str(),
                                    error = e.as_str(),
                                    "gateway rebuild against ready snapshot failed — retrying",
                                );
                                tokio::time::sleep(RETRY).await;
                                continue; // re-read the snapshot and retry
                            }
                        }
                    }
                }
                if rx.changed().await.is_err() {
                    break; // daemon dropped its sender — process shutdown
                }
            }
        });
    }

    /// Get logger (panics if called before initialize)
    fn log(&self) -> &ModuleLogger {
        self.log
            .get()
            .expect("AIProviderModule not initialized")
            .as_ref()
    }

    /// Register all available adapters
    async fn register_adapters(&self) -> Result<(), String> {
        // Check global flag to prevent re-initialization (survives module recreation)
        if INITIALIZED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            self.log()
                .info("Adapters already initialized (global), skipping re-registration");
            return Ok(());
        }

        let mut registry = self.registry.write().await;

        // Priority order (lower = higher priority):
        // 0: DeepSeek (best price/performance)
        // 1: Anthropic (best reasoning)
        // 2: OpenAI
        // 3: Groq (fast)
        // 4: Together
        // 5: Fireworks
        // 6: XAI
        // 7: Google
        //
        // HeuristicInferenceAdapter is NOT auto-registered here.
        //
        // Per [[no-fallbacks-ever]] and [[no-if-statements-use-llms-for-
        // cognition]] (Joel, 2026-06-01): "You mix this fake shit in and
        // it's going live ALL THE TIME. The fake shit is a CHOSEN model
        // adapter no other form. Declaration." Previously this module
        // unconditionally registered the heuristic adapter at priority 99
        // with the comment "never auto-selects over real adapters" — that
        // assumption was wrong. Any production code path that called
        // `select()` without specifying a model could end up at the
        // heuristic. The structural fix: heuristic adapter is gated
        // behind `cfg(any(test, feature = "test-fixtures"))` so production
        // binaries cannot link it; tests that legitimately want it
        // register it explicitly in their setup code (no global default
        // registration, no silent availability).

        // Per task #162's Box→Arc migration: init-then-register
        // pattern. The registry stores ready-to-serve adapters; we
        // pay the initialize() cost here BEFORE registration. If an
        // adapter's initialize fails (e.g. API key validation
        // failure), we log and skip — per [[no-fallbacks-ever]] we
        // surface the failure rather than substituting a degraded
        // adapter, and other providers can still register.
        //
        // The pattern: `let mut a = X::new(); a.initialize().await?;
        // registry.register(Arc::new(a), priority);` — eight repeats
        // for the cloud-API set below.

        // Only register adapters that have API keys configured.
        if get_secret("DEEPSEEK_API_KEY").is_some() {
            self.log().info("Registering DeepSeek adapter");
            let mut a = OpenAICompatibleAdapter::from_registry("deepseek");
            match a.initialize().await {
                Ok(()) => registry.register(Arc::new(a), 0),
                Err(e) => self
                    .log()
                    .warn(&format!("DeepSeek initialize failed: {e} — not registered")),
            }
        }

        if get_secret("ANTHROPIC_API_KEY").is_some() {
            self.log().info("Registering Anthropic adapter");
            let mut a = AnthropicAdapter::new();
            match a.initialize().await {
                Ok(()) => registry.register(Arc::new(a), 1),
                Err(e) => self.log().warn(&format!(
                    "Anthropic initialize failed: {e} — not registered"
                )),
            }
        }

        if get_secret("OPENAI_API_KEY").is_some() {
            self.log().info("Registering OpenAI adapter");
            let mut a = OpenAICompatibleAdapter::from_registry("openai");
            match a.initialize().await {
                Ok(()) => registry.register(Arc::new(a), 2),
                Err(e) => self
                    .log()
                    .warn(&format!("OpenAI initialize failed: {e} — not registered")),
            }
        }

        if get_secret("GROQ_API_KEY").is_some() {
            self.log().info("Registering Groq adapter");
            let mut a = OpenAICompatibleAdapter::from_registry("groq");
            match a.initialize().await {
                Ok(()) => registry.register(Arc::new(a), 3),
                Err(e) => self
                    .log()
                    .warn(&format!("Groq initialize failed: {e} — not registered")),
            }
        }

        if get_secret("TOGETHER_API_KEY").is_some() {
            self.log().info("Registering Together adapter");
            let mut a = OpenAICompatibleAdapter::from_registry("together");
            match a.initialize().await {
                Ok(()) => registry.register(Arc::new(a), 4),
                Err(e) => self
                    .log()
                    .warn(&format!("Together initialize failed: {e} — not registered")),
            }
        }

        if get_secret("FIREWORKS_API_KEY").is_some() {
            self.log().info("Registering Fireworks adapter");
            let mut a = OpenAICompatibleAdapter::from_registry("fireworks");
            match a.initialize().await {
                Ok(()) => registry.register(Arc::new(a), 5),
                Err(e) => self.log().warn(&format!(
                    "Fireworks initialize failed: {e} — not registered"
                )),
            }
        }

        if get_secret("XAI_API_KEY").is_some() {
            self.log().info("Registering XAI adapter");
            let mut a = OpenAICompatibleAdapter::from_registry("xai");
            match a.initialize().await {
                Ok(()) => registry.register(Arc::new(a), 6),
                Err(e) => self
                    .log()
                    .warn(&format!("XAI initialize failed: {e} — not registered")),
            }
        }

        if get_secret("GOOGLE_API_KEY").is_some() {
            self.log().info("Registering Google adapter");
            let mut a = OpenAICompatibleAdapter::from_registry("google");
            match a.initialize().await {
                Ok(()) => registry.register(Arc::new(a), 7),
                Err(e) => self
                    .log()
                    .warn(&format!("Google initialize failed: {e} — not registered")),
            }
        }

        if get_secret("MISTRAL_API_KEY").is_some() {
            self.log().info("Registering Mistral adapter");
            let mut a = OpenAICompatibleAdapter::from_registry("mistral");
            match a.initialize().await {
                Ok(()) => registry.register(Arc::new(a), 8),
                Err(e) => self
                    .log()
                    .warn(&format!("Mistral initialize failed: {e} — not registered")),
            }
        }

        // llama-server — the local OpenAI-compatible serving gateway. Registered
        // when the serving daemon has a READY model (Contract A), NOT on the
        // presence of an API key: it's a local endpoint with no credential. Its
        // base_url comes from the daemon's reconciled snapshot (the single source
        // of truth for where the gateway lives), and its live /v1/models catalog
        // decides which models it serves. Additive priority for now — making the
        // gateway the *preferred* route is a separate routing-policy change.
        // No served model → no registration → the boot-status block below fails
        // loud (no local fallback), per the no-fallback rule.
        // Gateway registration is REACTIVE, not one-shot (task #71). A cold large
        // GGUF can take longer than any fixed boot bound to finish warming its slot
        // graphs; the old `await_ready_serving(DEFAULT_SERVING_WAIT)` here timed out
        // and NEVER registered, leaving the core permanently gatewayless even though
        // the daemon brought the model up moments later. Now: a SHORT inline wait
        // registers on the spot for the warm-restart / adoption case; if the model
        // is still loading, a detached watcher registers the instant the daemon's
        // snapshot reports ready — however long the cold load takes. This is NOT a
        // fallback: there is still exactly one inference path (the gateway); it
        // simply registers when its backend is actually ready. The serving daemon
        // remains the loud-failure owner if it can never bring a model up, and an
        // inference call arriving before registration fails loud at `select()` (no
        // stand-in). [[fallbacks-are-illegal-fail-loud]]
        let mut gateway_registered = false;
        let mut gateway_pending = false;
        let mut gateway_synced: Option<(String, Option<String>)> = None;
        if let Some(snap) =
            crate::inference::llama_server::await_ready_serving(GATEWAY_FAST_PATH_WAIT).await
        {
            self.log().info("Registering llama-server gateway adapter");
            match build_gateway_adapter(snap.base_url.clone(), snap.active_model.as_deref()).await {
                Ok(a) => {
                    // Idempotent registration (the 5090 #2 mystery, 2026-07-25):
                    // EVERY gateway registration site deregister-sweeps first, so
                    // no path ordering can ever mint a collision-suffixed twin —
                    // whichever fired first, last-writer replaces.
                    registry.deregister(crate::inference::llama_server::PROVIDER_ID);
                    registry.register(Arc::new(a), 9);
                    gateway_registered = true;
                    gateway_synced = Some((snap.base_url, snap.active_model));
                }
                Err(e) => self.log().warn(&format!(
                    "llama-server initialize failed: {e} — not registered"
                )),
            }
        }
        // Persistent gateway SYNC (card ed3661c4): the adapter must TRACK the
        // daemon's ServingSnapshot, never cache it. The one-shot reactive
        // watcher this replaces registered at first-ready and went away — so a
        // model swap, a relaunch onto a scanned port (the 5090 stale-server
        // repro, 2026-07-24), or a window regrow left the gateway advertising a
        // DEAD server's catalog and select() honestly refusing the model that
        // WAS serving. Now one detached task follows the watch forever: on any
        // (base_url, active_model) change while ready it rebuilds the adapter
        // against the LIVE server (fresh /v1/models catalog) and REPLACES the
        // registration (deregister sweeps `#N` duplicates — never a second
        // entry). This also retires the task-#71 registration race: the sync
        // task IS the reactive registrar, for the first ready and every one
        // after. Same bug class as the frozen-window clamp: a stale cache of a
        // live value; the fix is the same — one live source, followed.
        Self::spawn_gateway_sync(self.registry.clone(), gateway_synced);
        if !gateway_registered {
            // Model still cold-loading; the persistent sync task spawned above
            // registers the instant the daemon's snapshot reports ready —
            // however long the cold load takes — and keeps it synced forever.
            gateway_pending = true;
        }

        // In-process llama.cpp adapter — bypasses DMR's container Metal toolchain,
        // which on M5 Pro fails to compile the tensor-API source (`has tensor=false`)
        // and falls back to a degraded path running at 22 tok/s. Our host-built
        // vendored llama.cpp compiles Metal correctly and measures 33 tok/s on the
        // same hardware (50% improvement, smoke test:
        // tests/llamacpp_metal_throughput.rs). Priority 0 — wins over DMR for
        // model IDs we own (continuum-ai/qwen3.5-*). DMR remains the runtime for
        // anything else.
        //
        // Registered eagerly when the GGUF file exists on disk. We intentionally
        // do NOT register a stub adapter that would silently fail later — per the
        // no-fallback rule, callers asking for our forge model should get either
        // a working in-process backend or a hard error at select() time naming
        // exactly which file is missing.
        // Register one in-process adapter PER llamacpp-local model row
        // whose GGUF (and, for multimodal, mmproj) is on disk. Each
        // adapter binds to a single GGUF — that's the backend's design
        // (one model per backend) — so multiple llamacpp-local rows
        // (text + vision + audio + future variants) need one adapter
        // each. Routing in AdapterRegistry::select picks by model id,
        // so they don't collide.
        //
        // Earlier shape called `LlamaCppAdapter::new()` for "the default"
        // and then iterated for the rest, but `new()` picks via HashMap
        // iteration order which is non-deterministic — caused a bug
        // where qwen3.5 got registered twice and qwen2-vl was skipped.
        // Now we iterate ALL rows uniformly.
        // NO silent local-inference fallback. Our OWN llama-server is THE inference
        // path (Unsloth excised); it serves our forged GGUF over /v1. The in-process
        // llama.cpp adapter is OPT-IN ONLY (CONTINUUM_LOCAL_LLAMA=1) so the core never
        // registers a local backend by default and never silently REVERTS to local when
        // the gateway is absent — a missing gateway fails loud at select(), it does not
        // get papered over with local inference ([[no-fallbacks-ever]]).
        let local_llama_opt_in =
            crate::config_env::read("CONTINUUM_LOCAL_LLAMA").as_deref() == Some("1");
        if let Some(reg_arc) = crate::model_registry::try_global().filter(|_| local_llama_opt_in) {
            for model_meta in reg_arc.models_for_provider(crate::inference::LLAMACPP_PROVIDER_ID) {
                let Some(gguf_path) = model_meta.gguf_local_path.clone() else {
                    self.log().info(&format!(
                        "Skipping in-process adapter for `{}` — artifact resolver found no local GGUF. \
                         Pull the model identified by gguf_hint or run the model download flow.",
                        model_meta.id
                    ));
                    continue;
                };
                if !gguf_path.exists() {
                    self.log().info(&format!(
                        "Skipping in-process adapter for `{}` — GGUF missing at {}. \
                         Install must pull this artifact for first-launch parity.",
                        model_meta.id,
                        gguf_path.display()
                    ));
                    continue;
                }
                // For vision/audio rows the mmproj is also required.
                // backend.generate_with_image / generate_with_audio
                // returns a clean error when mmproj is absent — we log
                // the gap upfront so install scripts catch it before
                // a real user hits "model declares Vision but mmproj
                // missing" at request time. Resolve via the ONE resolver
                // (declared path OR the projector beside the GGUF in the
                // HF cache), so a self-provisioned sibling reads as present
                // and we don't false-warn on a model that will serve fine.
                let needs_mmproj = model_meta.has(crate::model_registry::types::Capability::Vision)
                    || model_meta.has(crate::model_registry::types::Capability::AudioInput);
                if needs_mmproj
                    && crate::model_registry::artifacts::resolve_mmproj_for_model(model_meta)
                        .is_none()
                {
                    self.log().info(&format!(
                        "Adapter `{}` declares Vision/AudioInput but no mmproj projector \
                         resolves — none declared, and none sits beside the GGUF in the HF \
                         cache. Multimodal calls will hard-error. Pull the model's `*-GGUF` \
                         repo (its projector ships alongside the GGUF) or add \
                         `mmproj_local_path` to the row.",
                        model_meta.id
                    ));
                }
                self.log().info(&format!(
                    "Registering in-process llama.cpp adapter for model `{}`",
                    model_meta.id
                ));
                // Serve at min(model's trained context, a conservative
                // device-independent KV ceiling). The trained context comes
                // from the Model row (`context_window`, hydrated from the
                // GGUF `context_length` header) — never a per-model constant
                // baked in here. A model advertising a very large trained
                // window would otherwise allocate a multi-GB F16 KV cache
                // per seq on load and reliably fail first-decode with
                // `llama_decode returned -3` on any device that can't fit
                // tens of GB of scratch, so we cap it; a model whose trained
                // window is already below the ceiling serves at its real
                // value rather than a fictitious larger one.
                //
                // The cap is now THE REAL BUDGET, not a placeholder. It used to be
                // `const KV_SAFE_CONTEXT_CEILING: u32 = 32_768`, whose own comment said it
                // was standing in "until task #79 lands: available VRAM (via the
                // ResourceGovernor lease) divided by THIS model's KV bytes/token". #79
                // landed — `footprint_for` gives the per-model KV rate and
                // `governed_host_budget` gives the leased budget — and the placeholder was
                // never removed, so every locally-registered GGUF kept serving cognition at
                // 32k no matter what the machine or the model could hold. A placeholder
                // that outlives its own stated precondition is just a clamp.
                // [[never-hardcode-a-context-window-4k-defaults-destroy-the-moe-thesis]]
                //
                // Degrades honestly: an ungoverned host, or a model whose GGUF can't be
                // sized, serves at the model's own trained window — never a fresh invented
                // ceiling. Over-allocation is what this guards (a huge trained window would
                // allocate multi-GB F16 KV per seq and fail first-decode with
                // `llama_decode returned -3`), and the governed budget guards it correctly.
                let effective_context = kv_safe_context(&model_meta);
                let adapter_base = crate::inference::LlamaCppAdapter::with_model_id(
                    gguf_path.clone(),
                    model_meta.id.clone(),
                )
                .with_context_length(effective_context);

                // Probe the GGUF architecture at registration time and
                // enable multi-seq continuous batching when safe (per
                // task #110 / batching_probe.rs). Coordinator-managed
                // lane multiplexing (per task #109) requires
                // n_seq_max>1 in the in-backend scheduler. Standard
                // transformers (Llama / Qwen-2.5 / Gemma-2 / Mistral /
                // ...) classify as SafeForMultiSeq; qwen3 / mamba /
                // rwkv / jamba / etc. classify as SingleSeqOnly and
                // we keep them at 1. Default n_seq_max for safe
                // architectures is 4 — matches the realistic-floor
                // coordinator config (4 concurrent lanes). The probe
                // is cheap (GGUF header only, no weights), runs once
                // per adapter registration.
                const N_SEQ_MAX_FOR_SAFE_MULTISEQ: u32 = 4;
                let adapter = match crate::inference::batching_probe::probe_gguf_batching_safety(
                    &gguf_path,
                ) {
                    Ok(verdict) if verdict.safe_for_multi_seq() => {
                        self.log().info(&format!(
                            "Architecture `{}` is safe for multi-seq batching; enabling n_seq_max={} \
                             for coordinator-managed lane multiplexing",
                            verdict.arch(),
                            N_SEQ_MAX_FOR_SAFE_MULTISEQ
                        ));
                        adapter_base.with_n_seq_max(N_SEQ_MAX_FOR_SAFE_MULTISEQ)
                    }
                    Ok(verdict) => {
                        self.log().info(&format!(
                            "Architecture `{}` not safe for multi-seq batching ({}); \
                             keeping n_seq_max=1",
                            verdict.arch(),
                            match &verdict {
                                crate::inference::batching_probe::BatchingSafety::SingleSeqOnly { reason, .. } => reason.as_str(),
                                _ => "architecture not in curated safe list",
                            }
                        ));
                        adapter_base
                    }
                    Err(err) => {
                        self.log().warn(&format!(
                            "Batching probe failed for `{}`: {err} — keeping n_seq_max=1 \
                             (conservative default)",
                            model_meta.id
                        ));
                        adapter_base
                    }
                };
                // Priority 0 — wins over DMR for the model ids it
                // claims. Init-then-register per #162: the adapter
                // is built by `build_llamacpp_adapter` already in a
                // ready state (no separate initialize() call is
                // wired through the current build flow; the
                // adapter's constructor handles model load).
                let mut adapter = adapter;
                if let Err(e) = adapter.initialize().await {
                    self.log().warn(&format!(
                        "in-process llama.cpp initialize failed: {e} — not registered"
                    ));
                } else {
                    registry.register(Arc::new(adapter), 0);
                }
            }
        } else {
            self.log().info(
                "In-process llama.cpp adapter NOT registered — model_registry not initialized. \
                 Local chat will route to DMR or cloud only.",
            );
        }

        // Docker Model Runner — preferred local provider when reachable. Routes
        // to llama.cpp-metal/cuda or vllm-metal depending on platform, all running
        // host-native via Docker Desktop. ~50 tok/s on M5 (Qwen2.5-7B Q4_K_M),
        // beats Candle's ~10 tok/s by 5x because Candle's Metal path goes through
        // ggml-via-candle while Model Runner is direct llama.cpp-metal.
        //
        // Initial probe + register; ongoing health is the watchdog `tick()`'s
        // job (DMR_TICK_INTERVAL = 5s). If Docker Desktop crashes mid-session,
        // the watchdog deregisters the DMR adapter so `select()` immediately
        // surfaces the right hard error to the user instead of failing in
        // generate_text against a now-unreachable endpoint.
        match Self::probe_dmr() {
            Some(endpoint) => {
                let desc = endpoint
                    .base_url
                    .as_deref()
                    .unwrap_or("localhost:12434 (host-native)");
                self.log().info(&format!(
                    "Registering Docker Model Runner adapter ({})",
                    desc
                ));
                // Priority 1 — sits BELOW the in-process llama.cpp
                // adapter (priority 0) so DMR only wins for models
                // LlamaCppAdapter doesn't claim. Critical on Mac M5
                // where DMR's container Metal toolchain is degraded
                // vs the host-built bundled llama.cpp (verified
                // 2026-04-19: 33 tok/s container vs 47 tok/s
                // in-process for the same forge model).
                let mut dmr = Self::build_dmr_adapter(&endpoint);
                if let Err(e) = dmr.initialize().await {
                    self.log().warn(&format!(
                        "DMR adapter initialize failed: {e} — not registered"
                    ));
                } else {
                    registry.register(Arc::from(dmr), 1);
                }
            }
            None => {
                self.log().info(
                    "Docker Model Runner not reachable on localhost:12434 \
                     (nor model-runner.docker.internal inside container). \
                     Watchdog will keep probing; will register automatically \
                     once Docker Desktop comes up. To enable: \
                     docker desktop enable model-runner --tcp=12434",
                );
            }
        }

        // DwarfStar (ds4) sidecar — the V4-Flash deliberator lane (#306).
        // Same probe-then-register shape as DMR: an OPERATOR-managed local
        // OpenAI-compatible endpoint we consume, never spawn (#179 interop
        // doctrine). Registered at priority 2 — below in-process llama.cpp
        // (0) and DMR (1): ds4 only wins for the models it exclusively
        // claims (deepseek-v4 prefix), so it can never shadow a lane. If
        // the sidecar isn't up at boot it simply isn't registered; watchdog
        // parity (re-register when it appears, deregister when it dies)
        // follows once the lifecycle is governed.
        let ds4_up = std::net::TcpStream::connect_timeout(
            &"127.0.0.1:8901".parse().unwrap(),
            Duration::from_secs(1),
        )
        .is_ok();
        if ds4_up {
            self.log()
                .info("Registering DwarfStar (ds4) sidecar adapter (localhost:8901)");
            let mut ds4 = Box::new(OpenAICompatibleAdapter::from_registry("ds4"))
                as Box<dyn AIProviderAdapter>;
            if let Err(e) = ds4.initialize().await {
                self.log().warn(&format!(
                    "ds4 adapter initialize failed: {e} — not registered"
                ));
            } else {
                registry.register(Arc::from(ds4), 2);
            }
        } else {
            self.log().info(
                "ds4 sidecar not reachable on localhost:8901 — deepseek-v4-flash \
                 unavailable this boot (launch ds4-server and reboot, or wait for \
                 watchdog parity)",
            );
        }

        // Candle is NOT registered in the AI provider's inference registry.
        // Candle is a TRAINING framework (LoRA fine-tuning, autodiff, safetensors).
        // It does not belong in the same registry as inference providers.
        // Training callers access Candle through the training/plasticity module
        // directly — NOT through the AI provider's adapter selection.
        //
        // Previously registered here "at lowest priority" with the excuse that
        // it would "never be picked for chat." That's wrong — it showed up
        // in the available providers list, confused error messages, and violated
        // separation of concerns. Training and inference are different activities
        // with different registries.

        // Per task #162: no `registry.initialize_all()` here —
        // each adapter is initialized inline above before being
        // wrapped in Arc and registered. The registry stores
        // ready-to-serve adapters; lifecycle is the constructor's
        // responsibility.

        let available = registry.available();
        self.log().info(&format!(
            "AIProviderModule initialized with {} providers: {:?}",
            available.len(),
            available
        ));

        if available.is_empty() {
            self.log()
                .warn("No providers available! Add API keys to ~/.continuum/config.env");
        }

        // Intentional boot assertion: announce the ACTIVE inference path so a silent
        // fallback can NEVER go unnoticed again ("we didn't even know it wasn't
        // serving"). Registration only means key-present + adapter-configured — it
        // does NOT mean a model is being SERVED. So we READ the serving daemon's
        // published ServingSnapshot (the same `watch` seam personas bind on in
        // supervisor slice 2) rather than issuing our own /v1/models probe:
        // subscribers READ the snapshot, they do NOT each probe. The daemon owns
        // serving health; this block owns "is there a served model for the gateway
        // path". A missing/empty/unready serving plan surfaces LOUD, never falsely
        // reported ✓. [[fallbacks-are-illegal-fail-loud]]
        {
            use crate::runtime::boot_status::{boot_status, BootStatusKind};
            let local_note = if local_llama_opt_in {
                " (+ CONTINUUM_LOCAL_LLAMA opt-in ALSO active)"
            } else {
                " — sole inference path"
            };
            if gateway_pending {
                // The model is still cold-loading; the reactive watcher spawned
                // above will register the gateway the instant the daemon reports
                // ready (and emit its own ✓/✗ then). This boot line is honest about
                // the in-progress state — NOT a false ✓, NOT a premature ✗.
                boot_status(
                    "inference",
                    BootStatusKind::Degraded,
                    "serving model still loading — inference gateway will register REACTIVELY when \
                     the daemon reports ready (task #71). The gateway is the sole inference path; \
                     calls before it registers fail loud at select(). No local fallback.",
                );
            } else if !gateway_registered {
                boot_status(
                    "inference",
                    BootStatusKind::Failed,
                    "inference gateway NOT registered — the serving daemon brought up NO ready model \
                     within the readiness bound. Inference gateway REQUIRED; no local fallback.",
                );
            } else {
                // Read the daemon's reconciled snapshot (bounded wait for its first
                // reconcile so a boot race still resolves). One source of truth for
                // "what is served" — no redundant HTTP probe of our own.
                match crate::inference::llama_server::await_ready_serving(
                    crate::inference::llama_server::DEFAULT_SERVING_WAIT,
                )
                .await
                {
                    Some(snap) => {
                        let model = snap.active_model.as_deref().unwrap_or("(unknown)");
                        let short = model.rsplit('/').next().unwrap_or(model);
                        boot_status(
                            "inference",
                            if local_llama_opt_in {
                                BootStatusKind::Degraded
                            } else {
                                BootStatusKind::Ok
                            },
                            &format!(
                                "inference gateway @ {base} — serving {short}{local_note}",
                                base = snap.base_url
                            ),
                        );
                    }
                    None => boot_status(
                        "inference",
                        BootStatusKind::Failed,
                        &format!(
                            "serving daemon brought up NO ready model within {secs}s — load one \
                             (UNSLOTH_MODEL / unsloth Studio) or check the serving plan. \
                             Inference gateway REQUIRED; no local fallback.{local_note}",
                            secs = crate::inference::llama_server::DEFAULT_SERVING_WAIT.as_secs()
                        ),
                    ),
                }
            }
        }

        Ok(())
    }
}

impl Default for AIProviderModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for AIProviderModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "ai_provider",
            priority: ModulePriority::Normal,
            command_prefixes: &["ai/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            // Local inference adapters fan out into GPU/ORT/llama threadpools.
            // Letting every persona call ai/generate concurrently saturates the
            // machine and lowers throughput. Queue at the runtime boundary; the
            // backend scheduler can batch/serialize work deliberately.
            max_concurrency: 1,
            // DMR watchdog cadence — see DMR_TICK_INTERVAL. The runtime's
            // `start_tick_loops` spawns one tokio task that calls `tick()`
            // on this interval; on every fire we probe DMR and reconcile
            // the registry.
            tick_interval: Some(DMR_TICK_INTERVAL),
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // Store logger for this module
        let _ = self.log.set(ctx.logger("ai_provider"));
        self.register_adapters().await
    }

    /// Watchdog tick — reconcile the registered state of DMR with what's
    /// actually reachable on the wire.
    ///
    /// State machine (each tick is one transition):
    ///
    ///   currently registered   probe up   action
    ///   ───────────────────   ────────   ────────────────────────────────
    ///   true                   true       no-op (steady-state happy path)
    ///   true                   false      DEREGISTER + log warn (Docker
    ///                                     just crashed; subsequent
    ///                                     `select()` will surface the
    ///                                     correct hard error)
    ///   false                  true       REGISTER + log info (Docker
    ///                                     Desktop just came back; reset
    ///                                     the consecutive-down counter)
    ///   false                  false      increment consecutive_down;
    ///                                     log a loud warn at the
    ///                                     30-second threshold so the
    ///                                     situation is diagnosable
    ///
    /// All adapter mutations go through the existing `registry.register`
    /// + new `registry.deregister`. No special-case state on the module
    /// beyond the consecutive-down tick counter.
    async fn tick(&self) -> Result<(), String> {
        let probe = Self::probe_dmr();
        // Reading is_registered first under a read lock keeps the common
        // steady-state path lock-free against the inference path.
        let currently_registered = self.registry.read().await.is_registered(DMR_PROVIDER_ID);

        match (currently_registered, probe) {
            (true, Some(_)) => {
                // Steady-state happy path: DMR is up and registered. Reset
                // the down-counter in case we were transiently flapping
                // (probe failed mid-tick last time but recovered now).
                self.dmr_consecutive_down_ticks.store(0, Ordering::Release);
            }
            (true, None) => {
                // DMR was registered but is no longer reachable. Deregister
                // immediately so the very next inference request fails loud
                // at `select()` instead of at `generate_text` with an
                // arbitrary connection error.
                let mut registry = self.registry.write().await;
                if registry.deregister(DMR_PROVIDER_ID) {
                    self.log().warn(
                        "Docker Model Runner became unreachable — \
                         deregistered. Local AI is unavailable until \
                         Docker Desktop comes back. Watchdog will \
                         re-register automatically.",
                    );
                }
                self.dmr_consecutive_down_ticks
                    .fetch_add(1, Ordering::AcqRel);
            }
            (false, Some(endpoint)) => {
                // Recovery path: Docker Desktop just came back. Build the
                // adapter, INITIALIZE IT (fetch /v1/models to populate the
                // live runtime catalog so supports_model can answer
                // honestly — without this, the freshly-registered adapter
                // returns false for every supports_model query and select()
                // hard-errors even though DMR is back), THEN register.
                //
                // If init fails (DMR is up but the model-list fetch errors
                // — common transient state in the first second after Docker
                // restarts), skip THIS tick and let the next one retry.
                // The adapter stays unregistered until init succeeds, which
                // is the safer state than registering a half-initialized
                // adapter that will silently reject every request.
                let mut adapter = Self::build_dmr_adapter(&endpoint);
                let desc = endpoint
                    .base_url
                    .as_deref()
                    .unwrap_or("localhost:12434 (host-native)");
                if let Err(e) = adapter.initialize().await {
                    self.log().warn(&format!(
                        "DMR is reachable ({desc}) but adapter.initialize() \
                         failed — will retry on next tick. Cause: {e}"
                    ));
                    // Don't increment down-counter: TCP probe succeeded; this
                    // is an init transient. Next tick will see "still false,
                    // probe still up" and re-attempt.
                    return Ok(());
                }
                let mut registry = self.registry.write().await;
                // Priority 1 here mirrors the init-time registration —
                // DMR sits below the in-process llama.cpp adapter so it
                // only wins for models LlamaCppAdapter doesn't claim.
                // Box→Arc via `Arc::from` per task #162's Arc-native
                // registry — zero-copy ownership flip.
                registry.register(Arc::from(adapter), 1);
                self.log().info(&format!(
                    "Docker Model Runner reachable again — re-registered ({}). \
                     Local AI is available.",
                    desc
                ));
                self.dmr_consecutive_down_ticks.store(0, Ordering::Release);
            }
            (false, None) => {
                // Still down. Escalate to a loud user-visible warning at
                // the 30-second threshold so a stalled Docker Desktop is
                // diagnosable rather than silently degrading every chat
                // turn. After warning, suppress repeats — same threshold
                // re-checked when the counter wraps past 6 multiples.
                let prev = self
                    .dmr_consecutive_down_ticks
                    .fetch_add(1, Ordering::AcqRel);
                let now = prev + 1;
                if now == DMR_DOWN_WARN_THRESHOLD_TICKS {
                    self.log().warn(
                        "Docker Model Runner has been unreachable for ≥30s. \
                         Docker Desktop needs to be running for local AI. \
                         Will keep probing every 5s.",
                    );
                }
            }
        }
        Ok(())
    }

    /// The migrated read-only `ai/*` introspection commands as typed self-routing
    /// objects on the ONE registry. Each shares this module's `AdapterRegistry`;
    /// the executor routes their names straight here (winning over the legacy
    /// `ai/` prefix arm), and their `CommandSpec` descriptors flow into
    /// `command_registry()` → the persona tool surface + grid ACL. The
    /// `ai/generate` inference seam and the `_ => execute_ts` legacy TS-forward
    /// remain in `handle_command` (separate concerns). See
    /// [`crate::commands::ai`].
    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        crate::commands::ai::command_objects(self.registry.clone())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        // All typed `ai/*` commands (generate + introspection) now self-route through
        // the registry via `commands()`. Anything still arriving here is an unmigrated
        // `ai/*` name: forward it directly to TypeScript over the Unix socket.
        // MUST use execute_ts (not execute) to bypass the Rust registry — otherwise the
        // `ai/` prefix matches back to this module → infinite recursion. (This legacy
        // TS-forward retires in Wave Z.)
        let log = crate::runtime::logger("ai_provider");
        log.info(&format!(
            "Forwarding '{}' to TypeScript via Unix socket (bypassing registry)",
            command
        ));
        match self.executor.get() {
            Some(exec) => exec.execute_ts(command, params).await,
            None => Err(
                "AIProviderModule: CommandExecutor not installed; cannot forward to TS".to_string(),
            ),
        }
    }

    fn install_executor(&self, executor: Arc<crate::runtime::CommandExecutor>) {
        self.executor.install(executor);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ============================================================================
// STANDALONE GENERATE FUNCTION (for internal use by other modules)
// ============================================================================

/// Generate text using the best available provider
/// This is a convenience function for internal use (e.g., AgentModule)
pub async fn generate_text(
    registry: &AdapterRegistry,
    request: TextGenerationRequest,
) -> Result<TextGenerationResponse, String> {
    // Device = `Auto` — this convenience helper trusts what the
    // caller specified via `request.provider` + `request.model`.
    // The registered adapter is the authority on its own device
    // class; layering a `Gpu` filter on top of an already-named
    // (provider, model) pair wrongly excludes CPU-only adapters
    // even when they are the only ones claiming the requested
    // model. Used by cognition::analyze and other internal
    // callers that don't have a device opinion.
    let (provider_id, adapter) = registry
        .select(
            request.provider.as_deref(),
            request.model.as_deref(),
            InferenceDevice::Auto,
        )
        .ok_or_else(|| {
            select_failure_message(
                registry,
                request.provider.as_deref(),
                request.model.as_deref(),
            )
        })?;

    let mut response = adapter.generate_text(request).await?;

    // Add routing info
    response.routing = Some(RoutingInfo {
        provider: provider_id.to_string(),
        is_local: adapter.capabilities().is_local,
        routing_reason: "generate_text_call".to_string(),
        adapters_applied: vec![],
        model_mapped: None,
        model_requested: response
            .routing
            .as_ref()
            .and_then(|r| r.model_requested.clone()),
    });

    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;

    // what this catches: the refusal for an unknown/omitted model must name the
    // model ids the registry actually serves, not just provider ids — a grid
    // consumer with no local registry was refused with a list it could not act
    // on (card a466fdd4, IntelMac → 5090, 2026-09-04).
    #[test]
    fn select_failure_message_names_served_models_for_model_and_no_specifier() {
        let heuristic = HeuristicInferenceAdapter::new();
        let provider = heuristic.provider_id().to_string();
        let model = heuristic.default_model().to_string();
        let mut registry = AdapterRegistry::new();
        registry.register(Arc::new(heuristic), 0);

        let by_model = select_failure_message(&registry, None, Some("no-such-model"));
        assert!(by_model.contains("no-such-model"), "{by_model}");
        assert!(by_model.contains(&format!("{provider}={model}")), "{by_model}");

        let unspecified = select_failure_message(&registry, None, None);
        assert!(unspecified.contains("never picks one"), "{unspecified}");
        assert!(unspecified.contains(&format!("{provider}={model}")), "{unspecified}");

        let bad_provider = select_failure_message(&registry, Some("ghost"), None);
        assert!(bad_provider.contains("\"ghost\""), "{bad_provider}");
        assert!(bad_provider.contains(&provider), "{bad_provider}");
    }
}
