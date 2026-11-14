use super::*;

/// The bound on every module's `load_state` at boot (in parallel, so the wall time is the
/// slowest single concern, never the sum), and the budget the persona seam derives its staged
/// read from (`cognition::handoff::STAGED_READ_BOUND`). The stop no longer uses it: each of
/// drain → save → join is awaited to completion, and only [`MODULE_NEVER_ANSWERED`] names a
/// module that never answers (card 2febbbe8).
// derived-or-floor: a floor, set when the phases were made parallel (2026-09-02); the
// shutdown receipt names any module that overruns it, which is the measurement that moves it.
pub const SHUTDOWN_PHASE: std::time::Duration = std::time::Duration::from_secs(2);

/// The stop's LAST RESORT: a module that has not finished drain → save → join by then never
/// answered, and its receipt row says `NeverAnswered { phase }` naming where it stopped. Every
/// phase before it is awaited on its own completion, so a slow module that answers is never cut
/// here. Sized inside the observers' budgets (the signal path waits 8 s, `system/shutdown` 15 s
/// past its settle) so the receipt naming the silent module always reaches them; it is the 6 s
/// the three 2 s phases already allowed in total, now spent on whichever phase needs it.
// derived-or-floor: the observers' budgets above it (signal 8 s, verb 15 s past settle) less the receipt's travel; the receipt names any module that reaches it, which is the measurement that moves it
// event-exempt: a wedged module (a task that never returns) emits no completion event; this is the error path that names it, after every completion event that did arrive
pub const MODULE_NEVER_ANSWERED: std::time::Duration = std::time::Duration::from_secs(6);

pub struct Runtime {
    /// Registry uses interior mutability (DashMap + RwLock).
    /// Safe to share via Arc — register() takes &self.
    pub(super) registry: Arc<ModuleRegistry>,
    pub(super) bus: Arc<MessageBus>,
    pub(super) compute: Arc<SharedCompute>,
    pub(super) concurrency_limits: Arc<DashMap<&'static str, Arc<Semaphore>>>,
    /// Connected `Provided`-command providers (eye-nodes), keyed by command name.
    /// The ONE registry the socket route (`route_command`) AND the
    /// `ProvidedCommandInterceptor` (in-process/persona route) both consult, so a
    /// `perception/observe` reaches the connected eye-node on either path. The IPC
    /// layer takes `provider_registry()` to bind an eye-node's provider on connect.
    pub(super) provider_registry: Arc<super::super::ProviderRegistry>,
    /// THE interceptor chain — one list, consulted by every dispatcher. The
    /// socket route (`route_command`: uu / IPC / MCP / the desktop) and the
    /// in-process `CommandExecutor` used to each carry half the contract: the
    /// executor had the interceptors (airc peer hop, grid dispatch) and the
    /// socket route had permits + metrics + provided routing. So a CLI
    /// `ai/generate {aircPeer}` ran LOCALLY on the wrong box with nothing
    /// saying the address was dropped (card 506a388c, 2026-09-05). One chain,
    /// owned here, shared by both.
    pub(super) interceptors: Arc<std::sync::RwLock<Vec<Arc<dyn CommandInterceptor>>>>,
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
            // Adopt the ONE process-global compute-once/share-many cache (not a
            // private instance) so a module reaching it via `ModuleContext.compute`
            // and a persona's `MediaPerceptionSource` reaching it via
            // `shared_compute::global()` share the SAME derivatives — the media
            // ingest that warms a frame and the perception source that reads it
            // cannot diverge. Content-addressed + pure, so one shared cache is
            // correct ([[media-is-compute-once-zero-copy-hardware-grade]]).
            compute: super::super::shared_compute::global(),
            concurrency_limits: Arc::new(DashMap::new()),
            interceptors: Arc::new(std::sync::RwLock::new(Vec::new())),
            provider_registry: Arc::new(super::super::ProviderRegistry::new()),
        }
    }

    /// The shared [`ProviderRegistry`](super::ProviderRegistry) of connected
    /// eye-nodes. The IPC layer clones this into both the
    /// `ProvidedCommandInterceptor` and its `ServerState` so a `provider/register`
    /// on any connection binds the SAME registry that `route_command` reads.
    pub fn provider_registry(&self) -> Arc<super::super::ProviderRegistry> {
        Arc::clone(&self.provider_registry)
    }

    /// Register a module. Auto-wires command routing from its config.
    /// Like CBAR's appendAnalyzer() — one call, everything connected.
    pub fn register(&self, module: Arc<dyn ServiceModule>) {
        let config = module.config();
        info!(
            "  Registering module: {} (priority: {:?}, commands: {:?})",
            config.name, config.priority, config.command_prefixes
        );

        // Wire event subscriptions into the message bus.
        //
        // LOUD CAVEAT (#140 post-mortem): these fire ONLY via the synchronous
        // `MessageBus::publish(..., registry)` path, which has no production
        // callers — live events ride `publish_async_only` (broadcast channel
        // only). Until a real dispatch tier exists, a module declaring
        // subscriptions here almost certainly wants a bus-receiver task instead
        // (dispatch_listener / chat persist-listener shape). Warn so the gap is
        // a signpost, never a silent void.
        for pattern in config.event_subscriptions {
            tracing::warn!(
                module = config.name,
                pattern,
                "event_subscriptions are dispatched only by the (currently unused)                  synchronous publish path — if this module expects live bus events,                  spawn a bus-receiver task from initialize() instead                  (see modules::chat::spawn_persist_listener)"
            );
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
            continuum_base::probe!(
                class = "ready.observed",
                module = module_name,
                fast_path = true
            );
            return Ok(());
        }
        continuum_base::probe!(class = "ready.awaiting", module = module_name);
        loop {
            if rx.changed().await.is_err() {
                let err =
                    format!("module '{module_name}' ready watch closed before publishing ready");
                continuum_base::probe!(class = "ready.watch_closed", module = module_name);
                return Err(err);
            }
            if *rx.borrow_and_update() {
                continuum_base::probe!(class = "ready.observed", module = module_name);
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
        let readiness = self.registry.readiness();
        // Open first: a module whose initialize returns after this pass starts its own loop
        // (module_init's late path), so neither path misses it and the claim stops both doing it.
        readiness.open_ticks();
        let mut handles = Vec::new();
        for name in self.registry.list_modules() {
            let Some(module) = self.registry.get_by_name(name) else { continue };
            if !readiness.admits_now(name) {
                // Never tick a module whose initialize has not returned; it ticks when it does.
                continuum_base::probe!(
                    class = "tick.deferred",
                    module = name,
                    "initialize has not returned; its tick loop starts when it does"
                );
                continue;
            }
            if readiness.claim_tick(name) {
                handles.extend(spawn_tick_loop(module));
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
        caller: Option<crate::routing::CallerIdentity>,
    ) -> Option<Result<CommandResult, String>> {
        // Meet the caller's dialect: a socket client (uu / IPC / MCP) may reach for
        // a command's trained alias (`read_file`), former name, or charset-legal
        // form (`code_read`). Resolve to the canonical name through the SAME
        // tool_dialect section the persona path uses, so every surface accepts the
        // same vocabulary. Idempotent on an already-canonical name — a no-op for the
        // common case. Non-recording (the persona path owns the tool-usage tally).
        let resolved = crate::runtime::extensions::resolve_command_name(command);
        let command = resolved.as_str();

        // Typed path wins: a registered DynCommand object routes DIRECTLY (O(1),
        // lock-free), ahead of the prefix table — same precedence the
        // CommandExecutor uses. This is the live socket route (uu / IPC), so the
        // consult must live here too until the dispatch paths are unified.
        // See docs/architecture/COMMAND-ORGANIZATION.md.
        //
        // `caller` is the connection's identity: `None` for the LOCAL Unix socket
        // (owner-by-locality) and a non-owner remote identity for a TCP-sourced
        // connection (the IPC server stamps it + ACL-gates the top-level command at
        // the boundary). Threading it means a command composing another over TCP
        // composes as the REMOTE caller, not silently as owner — no escalation.
        // Interceptors first, same contract as the executor: Handled wins,
        // Decline passes, an error is LOUD (never a silent local fallback —
        // that is exactly what a peer-addressed call running locally was).
        let chain: Vec<Arc<dyn CommandInterceptor>> = self
            .interceptors
            .read()
            .unwrap_or_else(|e| e.into_inner()) // unwrap_or_else: a poisoned lock still holds the list; read it
            .clone();
        for interceptor in chain {
            match interceptor
                .try_route(command, &params, caller.as_ref())
                .await
            {
                Ok(InterceptorOutcome::Handled(result)) => return Some(Ok(result)),
                Ok(InterceptorOutcome::Decline) => continue,
                Err(e) => {
                    return Some(Err(format!(
                        "interceptor '{}' refused '{command}': {e}",
                        interceptor.name()
                    )))
                }
            }
        }
        if let Some(cmd) = self.registry.route_object(command) {
            // A dep-holding object reaches its module's state: admitted as far as that module is.
            if let Err(refused) = self.registry.admit(self.registry.object_owner(command)).await {
                return Some(Err(refused.to_string()));
            }
            // NOTE (adversarial review 2026-06-21): the typed object path does NOT
            // pass through the per-MODULE concurrency limiter or ModuleMetrics below
            // — a DynCommand object is module-independent, so it has no module to key
            // those on. This is deliberate: per-command throughput leasing +
            // observability belong to the command framework (the executor already
            // emits `command:completed` on the in-process/persona route), not the
            // legacy per-module path. Migrated commands are absent from ModuleMetrics
            // on THIS local route (acceptable for the trivial commands migrated so
            // far; revisit before migrating a hot/contended command).
            return Some(dispatch_object_with_panic_guard(cmd, params, caller).await.map_err(String::from));
        }

        // Provided commands (perception/observe, interface/screenshot) have NO
        // ServiceModule — the headless substrate can't render/capture them. Route
        // through the ONE Provided decision shared with the CommandExecutor's
        // interceptor, so the SOCKET route (uu / IPC / MCP) reaches the eye-node by
        // the SAME infrastructure the in-process persona route uses — no path gains
        // a capability another lacks. Guard on the cheap set lookup so `params` is
        // moved (not cloned) on the common non-Provided path.
        if super::super::provided_provider::is_provided_command(command) {
            return super::super::provided_provider::route_provided(
                &self.provider_registry,
                command,
                params,
                caller.as_ref(),
            )
            .await
            .map(|result| result.map(CommandResult::Json));
        }

        let (module, full_cmd) = self.registry.route_command(command)?;
        let module_name = module.config().name;
        // A module whose initialize has not returned is waited on (bounded by the init deadline)
        // or, past it, refused by type: never a hang inside a half-initialized module.
        if let Err(refused) = self.registry.admit(Some(module_name)).await {
            return Some(Err(refused.to_string()));
        }

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

        // Execute command — wrapped in catch_unwind so a panicking
        // handler converts to typed Err(String) instead of poisoning
        // this dispatch task.
        let result = dispatch_with_panic_guard(&module, &full_cmd, params, module_name).await;
        drop(permit);

        // Record timing (automatic for ALL commands)
        if let (Some(metrics), Some(tracker)) = (metrics, tracker) {
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
    /// The shared interceptor chain (see the field doc). Hand this to every
    /// `CommandExecutor` so an interceptor registered once applies on every
    /// route into the node.
    pub fn interceptor_chain(&self) -> Arc<std::sync::RwLock<Vec<Arc<dyn CommandInterceptor>>>> {
        self.interceptors.clone()
    }

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

    /// Shutdown all modules — PARALLEL, AWAITED, RECEIPTED (the CBAR shape,
    /// Joel 2026-09-02: "they all follow the same pattern, they gracefully
    /// and quickly save and join threads"). Every module gets the SAME
    /// contract: each phase runs to its own completion; only a module that
    /// never answers is cut, by [`MODULE_NEVER_ANSWERED`], and named. Total
    /// wall time = the slowest module — never the SUM the old sequential loop paid.
    ///
    /// Until 2026-09-02 nothing on the production stop path called this at
    /// all: the SIGTERM handler killed sentinels, slept a flat 2s, and
    /// `_exit`ed — so no module ever saved on a real stop. The handler now
    /// runs this first ([`install_signal_shutdown`]).
    /// Stop every module: DRAIN, then save, then join — each phase awaited, all modules
    /// in parallel — and return a receipt saying what actually reached disk.
    ///
    /// The drain phase is new and it is the point. Suspending a module's tick is not a
    /// drain (`quiesce_all` stops each mind's self-tick and leaves room input arriving and
    /// active turns running), so without it `save_state` could be taken underneath a turn
    /// halfway through writing, and the result was indistinguishable from a clean save.
    pub async fn shutdown(&self) -> ShutdownReceipt {
        self.shutdown_within(MODULE_NEVER_ANSWERED).await
    }
}
