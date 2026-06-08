//! Runtime — lifecycle orchestration for the modular runtime.
//!
//! Creates the registry, message bus, and shared compute cache.
//! Modules register, initialize, then the runtime serves IPC requests.
//!
//! This is the top-level coordinator — like CBAR's RenderingEngine
//! that owns the CBP_Analyzer pipeline and orchestrates frame flow.

use super::boot_mode::BootMode;
use super::message_bus::MessageBus;
use super::module_context::ModuleContext;
use super::registry::ModuleRegistry;
use super::service_module::{CommandResult, ServiceModule};
use super::shared_compute::SharedCompute;
use crate::airc::AircDiscovery;
use dashmap::DashMap;
use futures::FutureExt;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

// Module manifest lives in the `module_manifest` mod below
// (MODULES_CORE / MODULES_PERSONA_HOSTING / ALL_KNOWN_MODULES /
// required_modules) so all module-state truth is in one place
// and the snapshot test has one anchor point.

pub struct Runtime {
    /// Registry uses interior mutability (DashMap + RwLock).
    /// Safe to share via Arc — register() takes &self.
    registry: Arc<ModuleRegistry>,
    bus: Arc<MessageBus>,
    compute: Arc<SharedCompute>,
    concurrency_limits: Arc<DashMap<&'static str, Arc<Semaphore>>>,
}

impl Default for Runtime {
    fn default() -> Self {
        Self::new()
    }
}

impl Runtime {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(ModuleRegistry::new()),
            bus: Arc::new(MessageBus::new()),
            compute: Arc::new(SharedCompute::new()),
            concurrency_limits: Arc::new(DashMap::new()),
        }
    }

    /// Register a module. Auto-wires command routing from its config.
    /// Like CBAR's appendAnalyzer() — one call, everything connected.
    pub fn register(&self, module: Arc<dyn ServiceModule>) {
        let config = module.config();
        info!(
            "  Registering module: {} (priority: {:?}, commands: {:?})",
            config.name, config.priority, config.command_prefixes
        );

        // Wire event subscriptions into the message bus
        for pattern in config.event_subscriptions {
            self.bus.subscribe(pattern, config.name, false);
        }

        // PIECE-2 PR-3 follow-up: wire artifact_subscriptions through
        // MessageBus::subscribe_artifact (Exact AND Prefix supported).
        //
        // Original PR-3 (#1339) routed only Exact through bus.subscribe
        // and emitted warn! for Prefix because the bus's glob_matches
        // uses colon-segmented patterns incompatible with the
        // slash-convention ArtifactKey. This follow-up adds a dedicated
        // artifact subscriber path on MessageBus that uses
        // ArtifactSelector::matches directly, so Prefix("cognition/")
        // matches any key starting with that string without forcing a
        // separator translation that doesn't exist cleanly. Event
        // subscriptions (event_subscriptions on the bus) keep their
        // colon-segmented glob path unchanged — the two subscriber
        // lists coexist on the same MessageBus.
        //
        // Delivery is synchronous through the dedicated path because
        // on_artifact_available is contract-bound to cheap-and-return.
        // The bus calls handle_event with event_name = key; the default
        // handle_event impl in service_module.rs auto-dispatches to
        // on_artifact_available when the incoming key matches one of
        // this module's artifact_subscriptions. Modules that override
        // handle_event keep full control.
        //
        // Cadence routing split (per airc design check w/ vhsm-scope
        // airc-8a5e, 2026-05-16 19:58Z):
        //   Cadence::EventDriven | OnArtifact → this bus path
        //   Cadence::Periodic                 → existing tick_interval path
        //   Cadence::Mixed                    → both
        // We always wire artifact subscriptions when
        // artifact_subscriptions is non-empty; the tick_interval path
        // is wired separately by start_tick_loops.
        for selector in module.artifact_subscriptions() {
            self.bus.subscribe_artifact(selector, config.name);
        }

        if config.max_concurrency > 0 {
            self.concurrency_limits.insert(
                config.name,
                Arc::new(Semaphore::new(config.max_concurrency)),
            );
        }

        self.registry.register(module);
    }

    /// Initialize all registered modules.
    /// Provides each module with a ModuleContext for inter-module communication.
    pub async fn initialize(&self) -> Result<(), String> {
        let ctx = ModuleContext::new(
            self.registry.clone(),
            self.bus.clone(),
            self.compute.clone(),
            tokio::runtime::Handle::current(),
        );

        let modules = self.registry.list_modules();
        info!("Initializing {} modules...", modules.len());

        for name in &modules {
            if let Some(module) = self.registry.get_by_name(name) {
                match module.initialize(&ctx).await {
                    Ok(_) => {
                        info!("  {} initialized", name);
                    }
                    Err(e) => {
                        error!("  {} initialization failed: {}", name, e);
                        return Err(format!("Module '{}' failed to initialize: {}", name, e));
                    }
                }
            }
        }

        info!("All {} modules initialized", modules.len());
        Ok(())
    }

    /// Await a named module's `ready_edge()`. Returns `Ok(())` as soon as
    /// the module's ready watch publishes `true`. Returns `Err(...)` if
    /// the named module is not registered, or if the watch sender drops
    /// before publishing ready (the module's task died before becoming
    /// healthy — substrate is in trouble; the operator's repair is upstream).
    ///
    /// Modules that don't override `ready_edge()` are considered ready
    /// immediately after `initialize()` completes — `wait_for_ready` for
    /// those returns `Ok(())` synchronously.
    ///
    /// This is the substrate's canonical "wait for X to come up" primitive.
    /// Replaces ad-hoc `oneshot::Sender` threaded through start_server-style
    /// APIs, polling loops with sleeps, and bespoke atomic flags. Per
    /// [[docs/architecture/CONCURRENCY-STYLE-GUIDE.md]]: signals replace
    /// races.
    pub async fn wait_for_ready(&self, module_name: &str) -> Result<(), String> {
        let module = self
            .registry
            .get_by_name(module_name)
            .ok_or_else(|| format!("module '{module_name}' not registered"))?;
        let mut rx = match module.ready_edge() {
            Some(rx) => rx,
            None => return Ok(()), // default: ready after initialize
        };
        // Already-ready fast path — borrow_and_update marks the value
        // as seen so the subsequent `changed()` only fires on a NEW change.
        if *rx.borrow_and_update() {
            crate::probe!(
                class = "ready.observed",
                module = module_name,
                fast_path = true
            );
            return Ok(());
        }
        crate::probe!(class = "ready.awaiting", module = module_name);
        loop {
            if rx.changed().await.is_err() {
                let err = format!(
                    "module '{module_name}' ready watch closed before publishing ready"
                );
                crate::probe!(class = "ready.watch_closed", module = module_name);
                return Err(err);
            }
            if *rx.borrow_and_update() {
                crate::probe!(class = "ready.observed", module = module_name);
                return Ok(());
            }
        }
    }

    /// Start periodic tick loops for modules that declare a tick_interval.
    /// Each module with a tick_interval gets its own tokio task that calls
    /// `tick()` at the specified cadence.
    ///
    /// RTOS guarantees per [[docs/architecture/CONCURRENCY-STYLE-GUIDE.md]]:
    /// - Uses `tokio::time::interval` (not `sleep` in a loop) so cadence
    ///   does not drift under load.
    /// - Each tick body is `catch_unwind`'d so a panic in one tick does
    ///   not silently kill the loop. Counts consecutive panics; a module
    ///   is quarantined after `TICK_QUARANTINE_AFTER` in a row, same
    ///   shape `MemoryPressureMonitor` uses for its reporters.
    /// - Late ticks coalesce (`MissedTickBehavior::Skip`) — a tick body
    ///   that runs longer than the interval does not stack up backlog.
    /// - `probe!(class = "tick.…", module = name, …)` emits on every
    ///   meaningful seam so operators see what's actually happening.
    pub fn start_tick_loops(&self) -> Vec<JoinHandle<()>> {
        let mut handles = Vec::new();
        let modules = self.registry.list_modules();

        for name in &modules {
            if let Some(module) = self.registry.get_by_name(name) {
                let config = module.config();
                if let Some(initial_interval) = config.tick_interval {
                    let module_name = config.name;
                    let module = module.clone();
                    info!(
                        "Starting tick loop for '{}' (interval: {:?})",
                        module_name, initial_interval
                    );
                    crate::probe!(
                        class = "tick.spawn",
                        module = module_name,
                        interval_ms = initial_interval.as_millis() as u64
                    );

                    // Outer catch_unwind protects the *runner* itself —
                    // anything genuinely catastrophic (poisoned lock,
                    // bad future state) still emits one final event
                    // before the loop dies.
                    let handle = tokio::spawn(async move {
                        let result = AssertUnwindSafe(run_tick_loop_for(
                            module,
                            module_name,
                            initial_interval,
                        ))
                        .catch_unwind()
                        .await;
                        if let Err(panic) = result {
                            error!(
                                "Tick loop for '{}' aborted with panic in runner: {:?}",
                                module_name, panic
                            );
                            crate::probe!(
                                class = "tick.aborted",
                                module = module_name,
                                reason = "runner_panic"
                            );
                        }
                    });

                    handles.push(handle);
                }
            }
        }

        if !handles.is_empty() {
            info!("Started {} tick loops", handles.len());
        }
        handles
    }

    /// Route a command through the registry (async version).
    /// Returns None if no module handles this command.
    ///
    /// AUTOMATIC METRICS: Every command is timed and recorded.
    pub async fn route_command(
        &self,
        command: &str,
        params: serde_json::Value,
    ) -> Option<Result<CommandResult, String>> {
        let (module, full_cmd) = self.registry.route_command(command)?;
        let module_name = module.config().name;

        // Get metrics tracker for this module
        let metrics = self.registry.get_metrics(module_name);
        let queued_at = std::time::Instant::now();

        let permit = match self.concurrency_limits.get(module_name) {
            Some(limit) => match limit.clone().acquire_owned().await {
                Ok(permit) => Some(permit),
                Err(_) => {
                    return Some(Err(format!(
                        "Runtime concurrency limiter for module '{module_name}' is closed"
                    )));
                }
            },
            None => None,
        };

        let tracker = metrics
            .as_ref()
            .map(|metrics| metrics.start_command(command, queued_at));

        // Execute command
        let result = module.handle_command(&full_cmd, params).await;
        drop(permit);

        // Record timing (automatic for ALL commands)
        if let (Some(metrics), Some(tracker)) = (metrics, tracker) {
            let timing = tracker.finish(result.is_ok());
            metrics.record(timing);
        }

        Some(result)
    }

    /// Route a command synchronously (for use from rayon threads).
    /// Spawns async work on tokio and bridges via sync channel.
    /// This avoids "Cannot start a runtime from within a runtime" panics.
    ///
    /// AUTOMATIC METRICS: Every command is timed and recorded.
    /// Module authors don't need to add timing code — the runtime handles it.
    pub fn route_command_sync(
        &self,
        command: &str,
        params: serde_json::Value,
        rt_handle: &tokio::runtime::Handle,
    ) -> Option<Result<CommandResult, String>> {
        let (module, full_cmd) = self.registry.route_command(command)?;
        let module_name = module.config().name;

        // Get metrics tracker for this module (created at registration)
        let metrics = self.registry.get_metrics(module_name);
        let queued_at = std::time::Instant::now();
        let limit = self
            .concurrency_limits
            .get(module_name)
            .map(|entry| entry.clone());

        // Use sync channel to bridge async -> sync safely
        let (tx, rx) = std::sync::mpsc::sync_channel(1);

        rt_handle.spawn(async move {
            let permit = match limit {
                Some(limit) => match limit.acquire_owned().await {
                    Ok(permit) => Some(permit),
                    Err(_) => {
                        let _ = tx.send(Err(format!(
                            "Runtime concurrency limiter for module '{module_name}' is closed"
                        )));
                        return;
                    }
                },
                None => None,
            };
            let result = module.handle_command(&full_cmd, params).await;
            drop(permit);
            let _ = tx.send(result);
        });

        // 60s timeout — generous enough for legitimate long operations (vector backfill,
        // large queries), strict enough to prevent rayon thread starvation.
        // Voice/TTS streaming uses a different code path (binary frames, dedicated connections).
        // The ORMRustClient timeout is 30s, so the client always times out first with a clean
        // error. This 60s is a safety net to free the rayon thread.
        let result = match rx.recv_timeout(std::time::Duration::from_secs(60)) {
            Ok(result) => result,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                error!("Command timed out after 60s (rayon safety net): {command}");
                Err(format!("Command timed out after 60s: {command}"))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                error!("Command handler task panicked or was cancelled: {command}");
                Err(format!("Command handler failed: {command}"))
            }
        };

        // Record timing (automatic for ALL commands)
        if let Some(metrics) = metrics {
            let tracker = metrics.start_command(command, queued_at);
            let timing = tracker.finish(result.is_ok());
            metrics.record(timing);
        }

        Some(result)
    }

    /// Get a reference to the registry for direct module lookup.
    pub fn registry(&self) -> &ModuleRegistry {
        &self.registry
    }

    /// Get the Arc<ModuleRegistry> for sharing across threads.
    pub fn registry_arc(&self) -> Arc<ModuleRegistry> {
        self.registry.clone()
    }

    /// Get a reference to the message bus.
    pub fn bus(&self) -> &MessageBus {
        &self.bus
    }

    /// Get the Arc<MessageBus> for sharing across threads.
    /// Used by long-lived publishers (e.g. LocalWorkingSetManager
    /// constructed via `with_bus` per genome PR-5) that hold their
    /// own Arc and call `bus.publish` without going through the
    /// Runtime each time.
    pub fn bus_arc(&self) -> Arc<MessageBus> {
        self.bus.clone()
    }

    /// Get a reference to the shared compute cache.
    pub fn compute(&self) -> &SharedCompute {
        &self.compute
    }

    /// Shutdown all modules gracefully.
    pub async fn shutdown(&self) {
        let modules = self.registry.list_modules();
        info!("Shutting down {} modules...", modules.len());

        for name in &modules {
            if let Some(module) = self.registry.get_by_name(name) {
                match module.shutdown().await {
                    Ok(_) => info!("  {} shutdown complete", name),
                    Err(e) => warn!("  {} shutdown error: {}", name, e),
                }
            }
        }

        info!("All modules shut down");
    }

    /// Verify all required modules are registered for the given
    /// `(discovery, mode)` pair.
    ///
    /// The required set is CONDITIONAL — not a flat list — because
    /// modules like `persona_instance_manager` and
    /// `persona-rag-inspect` are only constructible when AIRC is
    /// `Healthy`. Slice A's flat list put them in `EXPECTED_MODULES`
    /// unconditionally, which meant `verify_registration` failed
    /// boot in the very `InferenceOnly` mode Slice A's PR body
    /// claimed to support (R1#1 BLOCK). This API takes the typed
    /// state and computes the expected set at call time, so the
    /// contradiction is structurally impossible.
    ///
    /// The substrate's ONE source of truth for what modules exist is
    /// the `MODULES: &[(&str, ModuleCategory)]` slice — one entry per
    /// registered module, tagged with its category. `required_modules`
    /// filters this list at call time; there is no parallel list to
    /// drift against. The `expected_modules_snapshot` integration test
    /// (A.2.2) cross-checks `MODULES` against actual registrations at
    /// boot. R2#3 + R3#3 BLOCK from Slice A's review is structurally
    /// closed by the single-source-of-truth design.
    pub fn verify_registration(
        &self,
        discovery: &AircDiscovery,
        mode: BootMode,
    ) -> Result<(), String> {
        let registered: Vec<String> = self.registry.module_names();
        let required = required_modules(discovery, mode);
        let required_count = required.len();
        let mut missing: Vec<&'static str> = Vec::new();

        for expected in &required {
            if !registered.iter().any(|r| r == expected) {
                missing.push(*expected);
            }
        }

        if !missing.is_empty() {
            let missing_list = missing.join(", ");
            error!(
                discovery = ?discovery.kind(),
                mode = ?mode.label(),
                "Missing required modules for (discovery={}, mode={}): [{}]",
                discovery.kind(),
                mode.label(),
                missing_list
            );
            error!(
                "Expected {} modules, found {}",
                required_count,
                registered.len()
            );
            error!(
                "Add missing module registrations in ipc/mod.rs (the registration \
                 site emits the module name; MODULES with its category tag is the \
                 single source of truth — required_modules() filters it)"
            );
            return Err(format!(
                "Module registration incomplete for (discovery={}, mode={}): \
                 missing [{}]. Server cannot start.",
                discovery.kind(),
                mode.label(),
                missing_list
            ));
        }

        info!(
            "✅ All {} required modules registered (discovery={}, mode={})",
            required_count,
            discovery.kind(),
            mode.label()
        );
        Ok(())
    }
}

/// Quarantine threshold for the per-module tick loop. After this many
/// CONSECUTIVE panics from `tick()`, the loop exits and the module no
/// longer ticks for the rest of the process lifetime.
///
/// Three matches `MemoryPressureMonitor`'s reporter quarantine — one
/// substrate-wide convention so operators reading probe events don't
/// have to remember per-module thresholds.
const TICK_QUARANTINE_AFTER: u32 = 3;

/// Per-module tick loop runner — the body wrapped by `start_tick_loops`.
///
/// One tick task per registered module with `tick_interval`. RTOS shape:
/// `interval` (not sleep-loop), `MissedTickBehavior::Skip` (no backlog),
/// per-tick `catch_unwind` (one bad tick doesn't kill the loop), panic
/// counter with quarantine. Each meaningful seam emits a `probe!` so the
/// JSONL probe sink records what actually happened on each module's tick
/// — operators (and replay) see tick spans, panics, errors, and cadence
/// changes without grep-prowling text logs.
async fn run_tick_loop_for(
    module: Arc<dyn ServiceModule>,
    module_name: &'static str,
    initial_interval: Duration,
) {
    // `tokio::time::interval` ticks at fixed wall-clock periods rather
    // than `now() + period` after each body — cadence does not drift
    // when the tick body runs longer than expected. `Skip` collapses
    // missed ticks instead of stacking them up, which is the right
    // policy for periodic refresh work (next tick uses fresh state,
    // not a stale snapshot from N ticks ago).
    let mut current_interval = initial_interval;
    let mut ticker = tokio::time::interval(current_interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Skip the first immediate tick — modules expect a delay before
    // their first periodic tick (same shape as the prior `sleep(d)`
    // before the loop entered).
    ticker.tick().await;

    let log = crate::runtime::logger(&format!("tick.{module_name}"));
    let mut consecutive_panics: u32 = 0;
    let mut ticks_total: u64 = 0;

    loop {
        ticker.tick().await;
        ticks_total = ticks_total.saturating_add(1);

        // Each tick body is its own catch_unwind boundary. A panic
        // inside `tick()` increments the counter; three in a row
        // quarantines the module (loop exits, task ends). An `Err`
        // return is a recoverable module-level error and does NOT
        // count toward quarantine — only panics do.
        let tick_result = AssertUnwindSafe(module.tick()).catch_unwind().await;

        match tick_result {
            Ok(Ok(())) => {
                consecutive_panics = 0;
            }
            Ok(Err(e)) => {
                consecutive_panics = 0;
                warn!("Tick error in '{}': {}", module_name, e);
                log.warn_fmt(format_args!("tick error: {e}"));
                crate::probe!(
                    class = "tick.error",
                    module = module_name,
                    error = %e
                );
            }
            Err(panic) => {
                consecutive_panics = consecutive_panics.saturating_add(1);
                let panic_msg = panic_message(&*panic);
                error!(
                    "Tick panic in '{}' ({}/{}): {}",
                    module_name, consecutive_panics, TICK_QUARANTINE_AFTER, panic_msg
                );
                log.warn_fmt(format_args!(
                    "tick panicked ({consecutive_panics}/{TICK_QUARANTINE_AFTER}): {panic_msg}"
                ));
                crate::probe!(
                    class = "tick.panic",
                    module = module_name,
                    count = consecutive_panics,
                    reason = %panic_msg
                );
                if consecutive_panics >= TICK_QUARANTINE_AFTER {
                    error!(
                        "Module '{}' quarantined after {} consecutive panics; tick loop exits",
                        module_name, TICK_QUARANTINE_AFTER
                    );
                    log.warn_fmt(format_args!(
                        "module quarantined after {TICK_QUARANTINE_AFTER} consecutive panics; tick loop exits"
                    ));
                    crate::probe!(
                        class = "tick.quarantined",
                        module = module_name,
                        ticks_total = ticks_total
                    );
                    return;
                }
            }
        }

        // Re-read interval each iteration so modules can dynamically
        // adjust cadence (e.g. back off under pressure). If the period
        // changed, rebuild the ticker — `Interval` doesn't expose a
        // setter for its period, and rebuilding is cheap on the slow
        // path (not the hot path that the broker tuner would touch).
        let new_interval = module.config().tick_interval.unwrap_or(initial_interval);
        if new_interval != current_interval {
            crate::probe!(
                class = "tick.cadence_changed",
                module = module_name,
                old_ms = current_interval.as_millis() as u64,
                new_ms = new_interval.as_millis() as u64
            );
            current_interval = new_interval;
            ticker = tokio::time::interval(current_interval);
            ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            // Skip immediate first tick so cadence change doesn't
            // double-fire.
            ticker.tick().await;
        }
    }
}

/// Best-effort string extraction from a `catch_unwind` payload.
/// `panic!("...")` payloads land as `&'static str` or `String`; anything
/// exotic gets a generic placeholder so the probe still carries a label.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

/// Category of a substrate module — load-bearing for `verify_registration`'s
/// conditional dispatch on `(AircDiscovery, BootMode)`.
///
/// Per the compression principle ([[host-the-seemingly-impossible]]): one
/// list of `(name, category)` pairs replaces what was originally going to
/// be three drifting constants. The set returned by `required_modules`
/// is computed by filtering this single list — there is no parallel
/// list to drift against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleCategory {
    /// Required by every boot regardless of discovery state or mode.
    /// Infrastructure, resource governance, transport, AI dispatch.
    /// Missing → substrate cannot serve commands at all.
    Core,
    /// Requires AIRC `Healthy`. Only registered when discovery
    /// succeeded across all four sub-steps (socket + room + channel +
    /// Status RPC liveness) AND the boot mode requires persona
    /// hosting. `--mode=inference-only` operators do not register
    /// these.
    PersonaHosting,
}

/// The substrate's ONE source of truth for "what modules exist + what
/// category each is in." `required_modules(&discovery, mode)` filters
/// this list; `ALL_KNOWN_MODULES()` derives the snapshot baseline from
/// it. There is no second list to drift against — the compression
/// principle made structural.
///
/// When you add a new `runtime.register(NewModule)` call site in
/// `ipc/mod.rs`, add one row here with its category. The drift catcher
/// `category_dispatch_consistency` verifies every row appears in the
/// expected required set for at least one `(discovery, mode)` pair —
/// so a row that's never reachable surfaces in CI. The
/// `expected_modules_snapshot` integration test (A.2.2) cross-checks
/// this list against actual registrations at boot.
pub const MODULES: &[(&str, ModuleCategory)] = &[
    // Infrastructure / health
    ("health", ModuleCategory::Core),
    ("auth", ModuleCategory::Core),
    ("system", ModuleCategory::Core),
    ("events", ModuleCategory::Core),
    ("logger", ModuleCategory::Core),
    ("runtime", ModuleCategory::Core),
    ("mcp", ModuleCategory::Core),
    ("data", ModuleCategory::Core),
    // Resource governance
    ("gpu", ModuleCategory::Core),
    ("resource-broker", ModuleCategory::Core),
    ("pressure-broker", ModuleCategory::Core),
    // AI / inference
    ("inference", ModuleCategory::Core),
    ("inference-llm", ModuleCategory::Core),
    ("ai_provider", ModuleCategory::Core),
    ("embedding", ModuleCategory::Core),
    ("search", ModuleCategory::Core),
    ("tool-parsing", ModuleCategory::Core),
    ("vision", ModuleCategory::Core),
    ("models", ModuleCategory::Core),
    ("memory", ModuleCategory::Core),
    ("rag", ModuleCategory::Core),
    // Persona substrate (always-built — cognition/allocator work
    // without an AIRC daemon)
    ("cognition", ModuleCategory::Core),
    ("channel", ModuleCategory::Core),
    ("persona_allocator", ModuleCategory::Core),
    ("agent", ModuleCategory::Core),
    // Persona substrate (AIRC-Healthy-conditional — registers only
    // when discovery succeeded across all four sub-steps)
    ("persona_instance_manager", ModuleCategory::PersonaHosting),
    ("persona-rag-inspect", ModuleCategory::PersonaHosting),
    // Forge / sentinel / plasticity / training
    ("forge", ModuleCategory::Core),
    ("sentinel", ModuleCategory::Core),
    ("plasticity", ModuleCategory::Core),
    ("dataset", ModuleCategory::Core),
    ("vdd", ModuleCategory::Core),
    ("cargo", ModuleCategory::Core),
    ("code", ModuleCategory::Core),
    // Substrate transports
    ("airc", ModuleCategory::Core),
    ("grid", ModuleCategory::Core),
    // Live presence — Slice B' splits these into renderer + voice
    // sidecars
    ("live", ModuleCategory::Core),
    ("avatar", ModuleCategory::Core),
];

/// Compute the required-module set for a given `(discovery, mode)` —
/// derived by filtering `MODULES` rather than maintained as a parallel
/// list.
///
/// `InferenceOnly` mode → only `Core` modules (operator opted out of
/// persona hosting).
/// `FullCitizen` / `FailFast` mode + `Healthy` AIRC → `Core` +
/// `PersonaHosting`.
/// `FullCitizen` / `FailFast` mode + degraded AIRC → the substrate
/// refuses boot BEFORE reaching `verify_registration` (see
/// `ipc/mod.rs::start_server`), so this function never sees that
/// pair in practice. Defensively, it returns the same set as
/// `(InferenceOnly, *)` so a stray invocation produces a coherent
/// error message rather than complaining about modules that cannot
/// be registered.
pub fn required_modules(discovery: &AircDiscovery, mode: BootMode) -> Vec<&'static str> {
    let needs_persona_hosting = mode.requires_persona_hosting() && discovery.can_host_personas();
    MODULES
        .iter()
        .filter(|(_, cat)| match cat {
            ModuleCategory::Core => true,
            ModuleCategory::PersonaHosting => needs_persona_hosting,
        })
        .map(|(name, _)| *name)
        .collect()
}

/// Every module name the substrate knows about — derived from
/// `MODULES`, not a parallel list. Used by the
/// `expected_modules_snapshot` integration test (A.2.2) as the
/// snapshot baseline.
pub fn all_known_modules() -> Vec<&'static str> {
    MODULES.iter().map(|(name, _)| *name).collect()
}

// MODULES is the substrate's ONE source of truth — `required_modules`
// and `all_known_modules` both derive from it. No parallel list to
// drift against.

#[cfg(test)]
mod conditional_modules_tests {
    use super::*;
    use crate::airc::{AircDiscovery, DiscoveryFailure, PartialDiscovery};
    use airc_core::RoomId;
    use std::path::PathBuf;
    use uuid::Uuid;

    fn healthy() -> AircDiscovery {
        AircDiscovery::Healthy {
            socket: PathBuf::from("/tmp/x.sock"),
            default_room: RoomId::from_uuid(Uuid::new_v4()),
            room_name: "general".into(),
            peer_id: Uuid::new_v4(),
        }
    }

    fn degraded() -> AircDiscovery {
        AircDiscovery::Degraded {
            reason: DiscoveryFailure::NoDefaultRoom,
            partial: PartialDiscovery::default(),
        }
    }

    /// FullCitizen + Healthy → must include persona hosting.
    #[test]
    fn full_citizen_healthy_requires_persona_hosting_modules() {
        let req = required_modules(&healthy(), BootMode::FullCitizen);
        assert!(req.contains(&"persona_instance_manager"));
        assert!(req.contains(&"persona-rag-inspect"));
        assert!(req.contains(&"inference"));
        assert!(req.contains(&"airc"));
    }

    /// InferenceOnly → must NOT require persona hosting modules.
    /// This is the case Slice A broke (R1#1).
    #[test]
    fn inference_only_does_not_require_persona_hosting_modules() {
        let req = required_modules(&healthy(), BootMode::InferenceOnly);
        assert!(!req.contains(&"persona_instance_manager"));
        assert!(!req.contains(&"persona-rag-inspect"));
        // Core inference must still be there
        assert!(req.contains(&"inference"));
        assert!(req.contains(&"embedding"));
        // airc module itself stays (it provides queue commands etc.
        // even when there's no live daemon to attach to)
        assert!(req.contains(&"airc"));
    }

    /// FailFast + Healthy → persona hosting required (strictest).
    #[test]
    fn fail_fast_healthy_requires_persona_hosting_modules() {
        let req = required_modules(&healthy(), BootMode::FailFast);
        assert!(req.contains(&"persona_instance_manager"));
        assert!(req.contains(&"persona-rag-inspect"));
    }

    /// FullCitizen + Degraded would normally bail before reaching
    /// verify_registration. If it somehow does reach, return the
    /// core-only set (no persona hosting modules required) so the
    /// resulting error message is coherent instead of complaining
    /// about modules that couldn't be registered against a
    /// degraded daemon.
    #[test]
    fn full_citizen_degraded_uses_core_only_set() {
        let req = required_modules(&degraded(), BootMode::FullCitizen);
        assert!(!req.contains(&"persona_instance_manager"));
        assert!(!req.contains(&"persona-rag-inspect"));
    }

    /// Drift catcher: every entry in `MODULES` MUST have a unique
    /// name. Duplicate registrations would make the conditional
    /// dispatch ambiguous (which category wins?).
    #[test]
    fn modules_list_has_unique_names() {
        let mut names: Vec<&str> = MODULES.iter().map(|(n, _)| *n).collect();
        let original_len = names.len();
        names.sort();
        names.dedup();
        assert_eq!(
            names.len(),
            original_len,
            "MODULES contains duplicate module names — every name must appear at most once"
        );
    }

    /// `required_modules` returns the correct size for each
    /// `(discovery, mode)` cell.
    ///
    /// `Healthy + FullCitizen` should include both Core and
    /// PersonaHosting categories. Every other cell should include
    /// only Core. This is the structural test the convergent
    /// review demanded as a R2#3+R3#3 closer: the dispatch is
    /// derived from the single source of truth, so any change to
    /// the category of a row immediately changes what this test
    /// expects to see, not a parallel list that drifts.
    #[test]
    fn required_modules_size_matches_category_dispatch() {
        let core_count = MODULES
            .iter()
            .filter(|(_, c)| matches!(c, ModuleCategory::Core))
            .count();
        let hosting_count = MODULES
            .iter()
            .filter(|(_, c)| matches!(c, ModuleCategory::PersonaHosting))
            .count();
        let all_count = core_count + hosting_count;

        assert_eq!(
            required_modules(&healthy(), BootMode::FullCitizen).len(),
            all_count,
            "FullCitizen + Healthy must include both Core and PersonaHosting"
        );
        assert_eq!(
            required_modules(&healthy(), BootMode::InferenceOnly).len(),
            core_count,
            "InferenceOnly must include only Core"
        );
        assert_eq!(
            required_modules(&degraded(), BootMode::FullCitizen).len(),
            core_count,
            "Degraded must include only Core regardless of mode"
        );
        assert_eq!(
            required_modules(&degraded(), BootMode::InferenceOnly).len(),
            core_count,
        );
    }

    /// `all_known_modules()` must match the names projected from
    /// `MODULES`. Derivation correctness — if the projection ever
    /// diverges (e.g. someone adds a filter the const doesn't have),
    /// this catches it.
    #[test]
    fn all_known_modules_derives_from_modules() {
        let expected: Vec<&str> = MODULES.iter().map(|(n, _)| *n).collect();
        assert_eq!(all_known_modules(), expected);
    }

    /// Every Core module must be present in EVERY required-modules
    /// result. Catches any future regression where someone
    /// accidentally categorizes a load-bearing infrastructure
    /// module as `PersonaHosting`.
    #[test]
    fn every_core_module_appears_in_every_required_set() {
        let cells = [
            (healthy(), BootMode::FullCitizen),
            (healthy(), BootMode::InferenceOnly),
            (healthy(), BootMode::FailFast),
            (degraded(), BootMode::FullCitizen),
            (degraded(), BootMode::InferenceOnly),
            (degraded(), BootMode::FailFast),
        ];
        for (discovery, mode) in cells {
            let req = required_modules(&discovery, mode);
            for (name, cat) in MODULES {
                if matches!(cat, ModuleCategory::Core) {
                    assert!(
                        req.contains(name),
                        "Core module {name:?} missing from required set for \
                         (discovery={}, mode={})",
                        discovery.kind(),
                        mode.label()
                    );
                }
            }
        }
    }

    /// PersonaHosting modules must appear ONLY when the dispatch
    /// is `(Healthy, requires_persona_hosting)`. The R1#1 BLOCK
    /// regression — having them in the required set under
    /// `--mode=inference-only` — is structurally impossible to
    /// reintroduce because this test pins both polarities.
    #[test]
    fn persona_hosting_modules_appear_only_when_dispatched() {
        let host_modules: Vec<&str> = MODULES
            .iter()
            .filter(|(_, c)| matches!(c, ModuleCategory::PersonaHosting))
            .map(|(n, _)| *n)
            .collect();

        for name in &host_modules {
            assert!(required_modules(&healthy(), BootMode::FullCitizen).contains(name));
            assert!(required_modules(&healthy(), BootMode::FailFast).contains(name));
        }

        let must_not_contain = [
            (healthy(), BootMode::InferenceOnly),
            (degraded(), BootMode::FullCitizen),
            (degraded(), BootMode::InferenceOnly),
            (degraded(), BootMode::FailFast),
        ];
        for (discovery, mode) in must_not_contain {
            let req = required_modules(&discovery, mode);
            for name in &host_modules {
                assert!(
                    !req.contains(name),
                    "PersonaHosting module {name:?} unexpectedly required for \
                     (discovery={}, mode={}) — this would reintroduce R1#1",
                    discovery.kind(),
                    mode.label()
                );
            }
        }
    }
}

#[cfg(test)]
mod piece_2_pr3_dispatch_tests {
    //! PIECE-2 PR-3 dispatch tests.
    //!
    //! Proves the registration → bus.subscribe → handle_event →
    //! on_artifact_available chain wires correctly for both
    //! ArtifactSelector::Exact and ArtifactSelector::Prefix (via the
    //! dedicated artifact-subscriber path on MessageBus added in the
    //! follow-up to PR-3), and that modules NOT opted-in see no
    //! artifact dispatch (backwards-compat guarantee).
    //!
    //! Test fixture: a tracking module that records every
    //! on_artifact_available call into a shared Vec the test asserts
    //! against after publishing.
    use super::*;
    use crate::runtime::artifact_handle::{ArtifactKey, ArtifactSelector};
    use crate::runtime::service_module::{
        CommandResult, ModuleConfig, ModulePriority, ServiceModule,
    };
    use async_trait::async_trait;
    use parking_lot::Mutex;
    use std::any::Any;
    use std::sync::Arc;

    struct RecordingModule {
        name: &'static str,
        subscriptions: Vec<ArtifactSelector>,
        received: Arc<Mutex<Vec<(ArtifactKey, serde_json::Value)>>>,
    }

    impl RecordingModule {
        fn new(
            name: &'static str,
            subscriptions: Vec<ArtifactSelector>,
        ) -> (Arc<Self>, Arc<Mutex<Vec<(ArtifactKey, serde_json::Value)>>>) {
            let received = Arc::new(Mutex::new(Vec::new()));
            let module = Arc::new(Self {
                name,
                subscriptions,
                received: received.clone(),
            });
            (module, received)
        }
    }

    #[async_trait]
    impl ServiceModule for RecordingModule {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: self.name,
                priority: ModulePriority::Normal,
                command_prefixes: &[],
                event_subscriptions: &[],
                needs_dedicated_thread: false,
                max_concurrency: 0,
                tick_interval: None,
            }
        }
        async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
            Ok(())
        }
        async fn handle_command(
            &self,
            _command: &str,
            _params: serde_json::Value,
        ) -> Result<CommandResult, String> {
            Err("not handled".to_string())
        }
        fn artifact_subscriptions(&self) -> Vec<ArtifactSelector> {
            self.subscriptions.clone()
        }
        async fn on_artifact_available(
            &self,
            key: &ArtifactKey,
            value: serde_json::Value,
        ) -> Result<(), String> {
            self.received.lock().push((key.clone(), value));
            Ok(())
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    /// What this catches: ArtifactSelector::Exact translates to a
    /// literal bus pattern. Publishing the matching key delivers via
    /// the default handle_event → on_artifact_available chain;
    /// publishing a non-matching key does not.
    #[tokio::test]
    async fn exact_selector_delivers_only_matching_key() {
        let runtime = Runtime::new();
        let (module, received) = RecordingModule::new(
            "exact-recorder",
            vec![ArtifactSelector::Exact(ArtifactKey::from(
                "paging/broker.snapshot",
            ))],
        );
        runtime.register(module);

        runtime
            .bus()
            .publish(
                "paging/broker.snapshot",
                serde_json::json!({"pressure": 0.42}),
                runtime.registry(),
            )
            .await;

        // Different key — not delivered.
        runtime
            .bus()
            .publish(
                "cognition/rate_proposals.result",
                serde_json::json!({"foo": "bar"}),
                runtime.registry(),
            )
            .await;

        // Prefix-shaped collision — not delivered (Exact must be
        // string-equality, not prefix-equality).
        runtime
            .bus()
            .publish(
                "paging/broker.snapshot.delta",
                serde_json::json!({"foo": "bar"}),
                runtime.registry(),
            )
            .await;

        let calls = received.lock().clone();
        assert_eq!(
            calls.len(),
            1,
            "exact selector should deliver only the literal match; got {:?}",
            calls
                .iter()
                .map(|(k, _)| k.as_str().to_string())
                .collect::<Vec<_>>()
        );
        assert_eq!(calls[0].0.as_str(), "paging/broker.snapshot");
        assert_eq!(calls[0].1["pressure"], 0.42);
    }

    /// What this catches (PR-3 follow-up): ArtifactSelector::Prefix
    /// now actually delivers. Original PR-3 (#1339) pinned this as
    /// no-op because the routing crammed ArtifactKeys through the
    /// bus's colon-segmented glob_matches. This follow-up adds a
    /// dedicated artifact-subscriber path on MessageBus that uses
    /// ArtifactSelector::matches directly, so Prefix("cognition/")
    /// matches anything starting with that string.
    ///
    /// Also asserts that a non-matching key is NOT delivered — the
    /// bound on the prefix matters, it's not a wildcard.
    #[tokio::test]
    async fn prefix_selector_delivers_matching_keys_and_skips_others() {
        let runtime = Runtime::new();
        let (module, received) = RecordingModule::new(
            "prefix-recorder",
            vec![ArtifactSelector::Prefix("cognition/".to_string())],
        );
        runtime.register(module);

        runtime
            .bus()
            .publish(
                "cognition/rate_proposals.result",
                serde_json::json!({"score": 0.7}),
                runtime.registry(),
            )
            .await;
        runtime
            .bus()
            .publish(
                "cognition/generate_recipe.result",
                serde_json::json!({"recipe_id": "abc"}),
                runtime.registry(),
            )
            .await;

        // Non-matching key — must NOT deliver.
        runtime
            .bus()
            .publish(
                "paging/broker.snapshot",
                serde_json::json!({"pressure": 0.1}),
                runtime.registry(),
            )
            .await;

        let calls = received.lock().clone();
        let delivered_keys: Vec<String> =
            calls.iter().map(|(k, _)| k.as_str().to_string()).collect();
        assert_eq!(
            calls.len(),
            2,
            "Prefix selector should deliver both cognition/* keys; got {:?}",
            delivered_keys
        );
        assert!(delivered_keys.contains(&"cognition/rate_proposals.result".to_string()));
        assert!(delivered_keys.contains(&"cognition/generate_recipe.result".to_string()));
        assert!(
            !delivered_keys.contains(&"paging/broker.snapshot".to_string()),
            "Prefix is a bound, not a wildcard — keys outside the prefix must not deliver"
        );
    }

    /// What this catches: a module that declares NO artifact_subscriptions
    /// receives NOTHING. Backwards-compat: every existing module
    /// (HealthModule, PressureBrokerModule, …) keeps its current
    /// behavior — the new default handle_event is a no-op for
    /// non-opted-in modules.
    #[tokio::test]
    async fn module_without_artifact_subscriptions_receives_nothing() {
        let runtime = Runtime::new();
        let (module, received) = RecordingModule::new("non-opted-in", vec![]);
        runtime.register(module);

        runtime
            .bus()
            .publish(
                "paging/broker.snapshot",
                serde_json::json!({}),
                runtime.registry(),
            )
            .await;
        runtime
            .bus()
            .publish("anything/at/all", serde_json::json!({}), runtime.registry())
            .await;

        assert!(
            received.lock().is_empty(),
            "module with empty subscriptions must receive nothing"
        );
    }

    /// What this catches: two modules with different subscription
    /// sets each receive ONLY their matching events. Multi-subscriber
    /// isolation.
    #[tokio::test]
    async fn multi_module_isolation_each_gets_only_matching_artifacts() {
        let runtime = Runtime::new();
        let (a, received_a) = RecordingModule::new(
            "module-a",
            vec![ArtifactSelector::Exact(ArtifactKey::from(
                "persona/inbox.frame_ready",
            ))],
        );
        let (b, received_b) = RecordingModule::new(
            "module-b",
            vec![ArtifactSelector::Exact(ArtifactKey::from(
                "paging/broker.snapshot",
            ))],
        );
        runtime.register(a);
        runtime.register(b);

        runtime
            .bus()
            .publish(
                "persona/inbox.frame_ready",
                serde_json::json!({"id": "frame-1"}),
                runtime.registry(),
            )
            .await;
        runtime
            .bus()
            .publish(
                "paging/broker.snapshot",
                serde_json::json!({"pressure": 0.5}),
                runtime.registry(),
            )
            .await;

        let a_keys: Vec<String> = received_a
            .lock()
            .iter()
            .map(|(k, _)| k.as_str().to_string())
            .collect();
        let b_keys: Vec<String> = received_b
            .lock()
            .iter()
            .map(|(k, _)| k.as_str().to_string())
            .collect();
        assert_eq!(a_keys, vec!["persona/inbox.frame_ready".to_string()]);
        assert_eq!(b_keys, vec!["paging/broker.snapshot".to_string()]);
    }
}

#[cfg(test)]
mod ready_edge_tests {
    //! RTOS Slice A.1 — `ServiceModule::ready_edge` + `Runtime::wait_for_ready`.
    //!
    //! Pins the four interesting paths:
    //!   1. Default impl returns None → wait_for_ready resolves immediately.
    //!   2. Override returns Some with already-true watch → fast path Ok.
    //!   3. Override returns Some with false → wait_for_ready awaits the
    //!      transition to true.
    //!   4. Override's sender drops without publishing true → wait_for_ready
    //!      returns Err (substrate is in trouble; the caller decides).
    //!   5. Unknown module name → Err.
    use super::*;
    use crate::runtime::service_module::{
        CommandResult, ModuleConfig, ModulePriority, ServiceModule,
    };
    use async_trait::async_trait;
    use std::any::Any;
    use std::sync::Arc;
    use tokio::sync::watch;

    /// Minimal module fixture. Optionally exposes a ready watch.
    struct ReadyModule {
        name: &'static str,
        ready_rx: Option<watch::Receiver<bool>>,
    }

    impl ReadyModule {
        fn without_ready_edge(name: &'static str) -> Arc<Self> {
            Arc::new(Self {
                name,
                ready_rx: None,
            })
        }

        /// Construct with a sender the test retains, so the test can
        /// publish `true` (or drop the sender) at the right moment.
        fn with_sender(name: &'static str, initial: bool) -> (Arc<Self>, watch::Sender<bool>) {
            let (tx, rx) = watch::channel(initial);
            let module = Arc::new(Self {
                name,
                ready_rx: Some(rx),
            });
            (module, tx)
        }
    }

    #[async_trait]
    impl ServiceModule for ReadyModule {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: self.name,
                priority: ModulePriority::Normal,
                command_prefixes: &[],
                event_subscriptions: &[],
                needs_dedicated_thread: false,
                max_concurrency: 0,
                tick_interval: None,
            }
        }
        async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
            Ok(())
        }
        async fn handle_command(
            &self,
            _: &str,
            _: serde_json::Value,
        ) -> Result<CommandResult, String> {
            Err("not handled".into())
        }
        fn ready_edge(&self) -> Option<watch::Receiver<bool>> {
            self.ready_rx.clone()
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    /// What this catches: a module without a custom ready_edge falls
    /// through the runtime's "ready after initialize" semantics
    /// immediately. Regression here would block the entire substrate
    /// boot waiting on the ~30 existing modules that take the default.
    #[tokio::test]
    async fn default_ready_edge_resolves_immediately() {
        let runtime = Runtime::new();
        runtime.register(ReadyModule::without_ready_edge("no-edge"));
        let result =
            tokio::time::timeout(std::time::Duration::from_millis(50), runtime.wait_for_ready("no-edge"))
                .await
                .expect("default ready_edge must NOT block");
        assert!(result.is_ok());
    }

    /// What this catches: when the watch is already `true` at the call,
    /// wait_for_ready returns without calling `.changed()` — the fast
    /// path. Regression here = spurious wakeup on first poll.
    #[tokio::test]
    async fn fast_path_when_already_ready() {
        let runtime = Runtime::new();
        let (module, _tx) = ReadyModule::with_sender("already-ready", true);
        runtime.register(module);
        let result = tokio::time::timeout(
            std::time::Duration::from_millis(50),
            runtime.wait_for_ready("already-ready"),
        )
        .await
        .expect("fast path must NOT block");
        assert!(result.is_ok());
    }

    /// What this catches: wait_for_ready actually awaits the watch's
    /// transition from false → true. Regression here = it returns
    /// before the module is actually ready, which is the exact race
    /// the primitive exists to eliminate.
    #[tokio::test]
    async fn awaits_transition_to_true() {
        let runtime = Runtime::new();
        let (module, tx) = ReadyModule::with_sender("not-yet", false);
        runtime.register(module);

        // Publish ready after a short delay; wait_for_ready must observe.
        let _publisher = tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            let _ = tx.send(true);
            // Keep tx alive so wait_for_ready sees the change before drop.
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        });

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(200),
            runtime.wait_for_ready("not-yet"),
        )
        .await
        .expect("wait_for_ready must observe within timeout");
        assert!(result.is_ok());
    }

    /// What this catches: if the module's sender drops without ever
    /// publishing true, wait_for_ready surfaces the error rather than
    /// hanging forever. The substrate's job is to refuse to lie about
    /// readiness; silent hang would mask the actual failure.
    #[tokio::test]
    async fn returns_err_when_sender_dropped_before_ready() {
        let runtime = Runtime::new();
        let (module, tx) = ReadyModule::with_sender("doomed", false);
        runtime.register(module);

        // Drop the sender without publishing true.
        drop(tx);

        let result = tokio::time::timeout(
            std::time::Duration::from_millis(100),
            runtime.wait_for_ready("doomed"),
        )
        .await
        .expect("must return without hanging");
        assert!(
            result.is_err(),
            "dropped sender before ready → Err, got Ok(()) — substrate just lied"
        );
    }

    /// What this catches: an unknown module name returns Err rather
    /// than hanging forever or panicking. Operators typo module names;
    /// the failure must be visible.
    #[tokio::test]
    async fn unknown_module_returns_err() {
        let runtime = Runtime::new();
        let result = runtime.wait_for_ready("does-not-exist").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not registered"));
    }
}
