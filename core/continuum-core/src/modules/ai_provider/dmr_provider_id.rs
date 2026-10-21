use super::*;

/// Provider ID for the Docker Model Runner adapter — single source of truth
/// shared between init-time registration and the watchdog tick.
pub(super) const DMR_PROVIDER_ID: &str = "docker-model-runner";

/// How often the watchdog probes DMR. Five seconds is the same cadence
/// as the PressureBroker tick — fast enough to recover within ~one
/// chat turn after Docker Desktop restarts; slow enough that the probe
/// (a one-second TCP connect) is essentially free relative to the tick.
pub(super) const DMR_TICK_INTERVAL: Duration = Duration::from_secs(5);

/// Consecutive failed-probe ticks before the watchdog escalates from
/// "transient blip" to "this is broken — tell the user." At 5s ticks,
/// 6 = 30 seconds, which is the threshold the resource architecture
/// uses for "loud failure, never silent."
pub(super) const DMR_DOWN_WARN_THRESHOLD_TICKS: u64 = 6;

/// Inline wait at boot for the serving snapshot to report a READY model before
/// registering the gateway adapter on the spot. Sized for the warm path — a
/// core restart over an already-resident model, or an adoption of a healthy
/// server — where the daemon's snapshot is ready within a couple of ticks, so
/// boot reports `✓ inference` cleanly with zero window where `select()` misses
/// the gateway. Deliberately SHORT: a cold large GGUF will not finish loading
/// in this window, and we do NOT want boot to block on it — that case hands off
/// to the reactive watcher below ([[fallbacks-are-illegal-fail-loud]], task #71).
// MEASURED 2026-09-04 on all three hosts (M5 8.1 s, 5090 host 10.1 s, Intel 8.0 s
// of ai_provider init): on a cold boot the local server is NEVER ready inside
// this window — the serving daemon launches it AFTER module init — so the wait
// was a fixed ~8 s tax on the PRE-BIND path of every boot on every machine,
// and the gateway registered through the reactive watcher anyway. Zero = "is
// it ready right now" (a surviving server on a warm reboot still takes the fast
// path); the cold boot hands off to the watcher immediately. A citizen's first
// select can miss the gateway for the watcher's few hundred ms — she boots,
// catches up, and grounds for far longer than that before her first turn.
pub(super) const GATEWAY_FAST_PATH_WAIT: Duration = Duration::ZERO;

// (GATEWAY_REACTIVE_CAP retired with the one-shot watcher: the persistent
// gateway-sync task, card ed3661c4, follows the serving snapshot for the life
// of the process — there is no wait to cap; a wedged serving plan stays the
// serving daemon's loud failure to own.)
/// One DMR endpoint discovered by `probe_dmr`. The base_url is None for
/// localhost — the adapter's default constructor already points at
/// `localhost:12434`. A `Some(url)` means the in-container variant
/// where we resolved `model-runner.docker.internal`.
#[derive(Debug, Clone)]
pub(super) struct DmrEndpoint {
    pub(super) base_url: Option<String>,
}

/// Global singleton registry - survives module recreation on server restart
pub(super) static GLOBAL_REGISTRY: Lazy<Arc<RwLock<AdapterRegistry>>> =
    Lazy::new(|| Arc::new(RwLock::new(AdapterRegistry::new())));

/// Track if we've done first-time initialization
pub(super) static INITIALIZED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Public accessor for the global adapter registry.
/// Used by the HTTP inference endpoint to share adapters with AIProviderModule.
pub fn global_registry() -> Arc<RwLock<AdapterRegistry>> {
    GLOBAL_REGISTRY.clone()
}

/// AIProviderModule - ServiceModule implementation for AI inference
pub struct AIProviderModule {
    pub(super) registry: Arc<RwLock<AdapterRegistry>>,
    pub(super) log: OnceCell<Arc<ModuleLogger>>,
    /// GPU memory manager — passed to CandleAdapter for VRAM allocation tracking.
    pub(super) gpu_manager: Option<Arc<crate::gpu::memory_manager::GpuMemoryManager>>,
    /// DMR watchdog state — counts consecutive down-probe ticks so we can
    /// escalate from quiet recovery to loud user-visible failure at the
    /// 30-second threshold. Atomic so the tick (`&self`) updates it
    /// without taking a write lock on the module.
    pub(super) dmr_consecutive_down_ticks: Arc<AtomicU64>,
    /// Substrate-wide command executor — installed by `start_server` after
    /// the executor is built. Used by the TS fallthrough for unmigrated
    /// `ai/*` commands (task #224 replaced the deleted free helper).
    pub(super) executor: LateBound<crate::runtime::CommandExecutor>,
}

/// Largest context this host can safely serve for `model`: its trained window, capped by what
/// the GOVERNED memory budget can hold at this model's real KV bytes/token. The replacement for
/// the old `KV_SAFE_CONTEXT_CEILING` placeholder — same authority `plan_serving` and the eval
/// lane size against, never a raw probe and never a constant.
///
/// Returns the trained window unchanged when the host is ungoverned or the model can't be sized:
/// an unknown budget must not become an invented ceiling.
pub(super) fn kv_safe_context(model: &crate::model_registry::Model) -> u32 {
    let trained = model.context_window;
    let Some(fp) = crate::cognition::serving_plan::footprint::footprint_for(model) else {
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
        .max(crate::inference::planning::MIN_SERVE_CTX)
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
    pub(super) fn probe_dmr() -> Option<DmrEndpoint> {
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
    pub(super) fn build_dmr_adapter(endpoint: &DmrEndpoint) -> Box<dyn AIProviderAdapter> {
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
        // "local" is the best-local-GPU sentinel, not an adapter id: DMR IS
        // registered here (the branch above handled the down case), so the
        // honest diagnosis is "no local adapter serves that model".
        (Some("local"), model) => format!(
            "No local adapter can serve model {model:?}. \
             Served models (provider=model): {served:?}. \
             Pass `model` as one of those, or `provider` as one of {available:?}."
        ),
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
pub(super) async fn build_gateway_adapter(
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
