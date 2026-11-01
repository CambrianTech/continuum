use super::*;

#[cfg(test)]
#[path = "self_shutdown_tests.rs"]
mod self_shutdown_tests;

#[cfg(test)]
mod background_custody_tests {
    use super::*;
    use crate::runtime::runtime::piece_2_pr3_dispatch_tests::RecordingModule;
    use std::sync::atomic::Ordering;

    #[tokio::test]
    async fn shutdown_waits_for_acceptance_and_terminal_storage_after_receipt_is_forgotten(
    ) -> Result<(), String> {
        use crate::orm::gated_adapter::GatedAdapter;
        use crate::runtime::tasks::TaskLedger;
        use crate::runtime::CommandExecutor;
        let runtime = Runtime::new();
        let (module, _) = RecordingModule::new("storage-boundary", Vec::new());
        runtime.register(module.clone());
        let (adapter, _directory) = crate::orm::store::fresh_adapter().await;
        let adapter = Arc::new(GatedAdapter::new(adapter));
        let ledger = Arc::new(TaskLedger::new(adapter.clone()).await?);
        let executor = Arc::new(
            CommandExecutor::new(runtime.registry.clone()).with_message_bus(runtime.bus.clone()),
        );
        executor.set_task_ledger(ledger.clone())?;
        let mut events = runtime.bus.receiver();
        let owner = uuid::Uuid::new_v4();
        let caller = crate::routing::CallerIdentity::local_persona(
            crate::identity::PeerId::from_uuid(owner),
        );
        let acceptance = adapter.pause_next_write();
        {
            let receipt = executor.dispatch_background(
                "test/unhandled",
                serde_json::Value::Null,
                Some(caller),
            );
            tokio::pin!(receipt);
            assert!(futures::poll!(receipt.as_mut()).is_pending());
            acceptance.entered.notified().await;
            // Drop the actual caller while its durable acceptance write is paused.
        }
        let shutdown = runtime.shutdown_within(std::time::Duration::from_secs(30));
        tokio::pin!(shutdown);
        assert!(futures::poll!(shutdown.as_mut()).is_pending());
        assert_eq!(module.saves.load(Ordering::SeqCst), 0);
        let terminal = adapter.pause_next_write();
        acceptance.release.notify_one();
        terminal.entered.notified().await;
        assert!(futures::poll!(shutdown.as_mut()).is_pending());
        assert_eq!(module.saves.load(Ordering::SeqCst), 0);
        terminal.release.notify_one();
        let receipt = shutdown.await;
        assert!(receipt.state_is_durable());
        assert_eq!(module.saves.load(Ordering::SeqCst), 1);
        let completion = events.recv().await.map_err(|error| error.to_string())?;
        let task = completion.payload["handle"]
            .as_str()
            .ok_or("missing completion handle")?;
        let task = uuid::Uuid::parse_str(task).map_err(|error| error.to_string())?;
        // The command was unhandled, but its failure is durable and deliverable;
        // module shutdown cannot overtake either storage boundary.
        assert!(matches!(
            ledger.pending(task, owner).await?,
            Some(crate::runtime::tasks::TaskEnd::Failed { .. })
        ));
        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_owner_retries_withheld_custody_without_repeating_module_stop(
    ) -> Result<(), String> {
        let runtime = Arc::new(Runtime::new());
        let (module, _) = RecordingModule::new("retry-custody-store", Vec::new());
        runtime.register(module.clone());
        let custody = runtime
            .registry
            .task_admission
            .admit_owned()
            .ok_or("admission refused")?;
        let operation = ShutdownOperation::new();
        let mut first = operation.begin(Some(runtime.clone()), std::time::Duration::ZERO);
        first.changed().await.map_err(|error| error.to_string())?;
        let receipt = first
            .borrow_and_update()
            .clone()
            .ok_or("missing withheld receipt")?;
        assert!(!receipt.tasks.is_quiet());
        assert_eq!(module.saves.load(Ordering::SeqCst), 0);
        drop(first);
        drop(custody);
        let mut retry = operation.begin(Some(runtime.clone()), std::time::Duration::ZERO);
        assert!(retry.borrow().is_none());
        retry.changed().await.map_err(|error| error.to_string())?;
        let receipt = retry
            .borrow_and_update()
            .clone()
            .ok_or("missing completed receipt")?;
        assert!(receipt.state_is_durable());
        let again = operation.begin(Some(runtime), std::time::Duration::ZERO);
        assert_eq!(again.borrow().as_ref(), Some(&receipt));
        assert_eq!(module.saves.load(Ordering::SeqCst), 1);
        Ok(())
    }

    #[tokio::test(start_paused = true)]
    async fn unfinished_custody_withholds_module_stop_and_a_later_drain_can_finish(
    ) -> Result<(), String> {
        let runtime = Runtime::new();
        let (module, _) = RecordingModule::new("custody-store", Vec::new());
        runtime.register(module.clone());
        let custody = runtime
            .registry
            .task_admission
            .admit_owned()
            .ok_or("admission refused")?;
        let receipt = runtime
            .shutdown_within(std::time::Duration::from_millis(10))
            .await;
        assert!(!receipt.state_is_durable());
        assert!(!receipt.tasks.is_quiet());
        assert!(receipt.modules.is_empty());
        assert_eq!(module.saves.load(Ordering::SeqCst), 0);
        assert!(runtime.registry.task_admission.admit_owned().is_none());
        drop(custody);
        let receipt = runtime
            .shutdown_within(std::time::Duration::from_millis(10))
            .await;
        assert!(receipt.state_is_durable());
        assert_eq!(module.saves.load(Ordering::SeqCst), 1);
        Ok(())
    }

    #[tokio::test]
    async fn cancelling_shutdown_wait_does_not_release_task_custody() -> Result<(), String> {
        let runtime = Runtime::new();
        let (module, _) = RecordingModule::new("cancel-custody-store", Vec::new());
        runtime.register(module.clone());
        let custody = runtime
            .registry
            .task_admission
            .admit_owned()
            .ok_or("admission refused")?;
        {
            let stop = runtime.shutdown_within(std::time::Duration::from_secs(1));
            tokio::pin!(stop);
            assert!(futures::poll!(stop.as_mut()).is_pending());
        }
        assert_eq!(module.saves.load(Ordering::SeqCst), 0);
        assert_eq!(runtime.registry.task_admission.in_flight(), 1);
        assert!(runtime.registry.task_admission.admit_owned().is_none());
        drop(custody);
        assert!(runtime
            .shutdown_within(std::time::Duration::from_secs(1))
            .await
            .state_is_durable());
        Ok(())
    }

    #[tokio::test]
    async fn failed_terminal_persistence_is_not_a_quiet_runtime() {
        let runtime = Runtime::new();
        let (module, _) = RecordingModule::new("failed-custody-store", Vec::new());
        runtime.register(module.clone());
        // Inject the execution owner's terminal-write failure fact; this tests
        // shutdown's consumption of it, not the storage adapter's fault handling.
        runtime
            .registry
            .task_persistence_failed
            .store(true, Ordering::Release);
        let receipt = runtime
            .shutdown_within(std::time::Duration::from_secs(1))
            .await;
        assert!(!receipt.state_is_durable());
        assert_eq!(module.saves.load(Ordering::SeqCst), 0);
        assert!(receipt.summary().contains("terminal persistence failed"));
    }
}

impl Runtime {
    /// `shutdown`, with the per-phase bound passed IN.
    ///
    /// A parameter rather than a constant because the TIMEOUT arms could not otherwise be
    /// reached: no test module takes two seconds, so `SaveTimedOut` and `JoinTimedOut`
    /// were only ever constructed as literals and asserted on. Deleting the timeout
    /// handling here would have left every one of those tests green — the outcomes were
    /// described by the suite and never produced by it. Found by IntelMac.
    pub(super) fn self_shutdown_refusal(&self) -> Option<ShutdownReceipt> {
        self.registry.task_admission.held_by_current_task().then(|| ShutdownReceipt {
            tasks: DrainOutcome::Unknown {
                reason: "an admitted task cannot shut down its own runtime; request shutdown outside its execution scope".into(),
            },
            modules: Vec::new(),
            total_ms: 0,
        })
    }

    pub(super) async fn shutdown_within(&self, per_phase: std::time::Duration) -> ShutdownReceipt {
        if let Some(receipt) = self.self_shutdown_refusal() {
            return receipt;
        }
        let started = std::time::Instant::now();
        // The same registry is held by every executor. Close before waiting so
        // acceptance cannot race past a zero count. Cancellation leaves it closed.
        let tasks = match tokio::time::timeout(per_phase, self.registry.task_admission.close_and_wait()).await {
            Err(_) => DrainOutcome::Unknown {
                reason: format!("background task custody did not settle within {}ms; modules remain running", per_phase.as_millis()),
            },
            Ok(()) if self.registry.task_persistence_failed.load(std::sync::atomic::Ordering::Acquire) => DrainOutcome::Unknown {
                reason: "background terminal persistence failed; durable acceptances require reconciliation".into(),
            },
            Ok(()) => DrainOutcome::Drained,
        };
        if !tasks.is_quiet() {
            return ShutdownReceipt {
                tasks,
                modules: Vec::new(),
                total_ms: started.elapsed().as_millis() as u64,
            };
        }
        let modules = self.registry.list_modules();
        info!(
            "Stopping {} modules (drain → save → join, parallel, 2s bound per phase)...",
            modules.len()
        );
        let futs = modules.iter().filter_map(|name| {
            self.registry.get_by_name(name).map(|module| {
                // Deref, not clone: `list_modules` yields `Vec<&'static str>`, so `name`
                // here is `&&'static str` and `.clone()` copied the OUTER reference —
                // it never cloned a string, and the borrow it produced only compiled
                // because the inner `&str` is 'static. Carried in from the pre-refactor
                // `shutdown()`; saying `*name` is what was always meant.
                let name: &'static str = *name;
                async move {
                    let t = std::time::Instant::now();
                    // 1. Stop taking new work and let in-flight work finish. A module
                    //    that cannot drain in time still gets its save attempted — a
                    //    mid-turn snapshot beats no snapshot — but the receipt says so.
                    let drain = match tokio::time::timeout(per_phase, module.drain()).await {
                        Ok(Ok(0)) => DrainOutcome::Drained,
                        Ok(Ok(in_flight)) => DrainOutcome::Incomplete { in_flight },
                        Ok(Err(e)) => DrainOutcome::Unknown { reason: e },
                        Err(_) => DrainOutcome::Unknown {
                            reason: format!("drain exceeded {}s", per_phase.as_secs()),
                        },
                    };

                    // 2. Save. This is the phase whose failure is DATA, not tidiness.
                    let saved = tokio::time::timeout(per_phase, module.save_state()).await;
                    let outcome = match saved {
                        Err(_) => ModuleStopOutcome::SaveTimedOut,
                        Ok(Err(e)) => ModuleStopOutcome::SaveFailed { error: e },
                        Ok(Ok(())) => {
                            // 3. Join, only meaningful once the state is durable.
                            match tokio::time::timeout(per_phase, module.shutdown()).await {
                                Err(_) => ModuleStopOutcome::JoinTimedOut,
                                Ok(Err(e)) => ModuleStopOutcome::JoinFailed { error: e },
                                Ok(Ok(())) => ModuleStopOutcome::Clean,
                            }
                        }
                    };
                    continuum_base::probe!(
                        class = "shutdown.step",
                        module = %name,
                        outcome = format!("{outcome:?}"),
                        drain = format!("{drain:?}"),
                        ms = t.elapsed().as_millis() as u64,
                        "module drain-save-join"
                    );
                    ModuleStop {
                        // `list_modules` yields &'static str; the receipt owns its names
                        // because it outlives the registry borrow and crosses the wire.
                        module: name.to_string(),
                        drain,
                        outcome,
                        ms: t.elapsed().as_millis() as u64,
                    }
                }
            })
        });
        let receipt = ShutdownReceipt {
            tasks,
            modules: futures::future::join_all(futs).await,
            total_ms: started.elapsed().as_millis() as u64,
        };
        info!("{}", receipt.summary());
        receipt
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
        discovery: &dyn MeshPresence,
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
pub(super) const TICK_QUARANTINE_AFTER: u32 = 3;

/// Per-module tick loop runner — the body wrapped by `start_tick_loops`.
///
/// One tick task per registered module with `tick_interval`. RTOS shape:
/// `interval` (not sleep-loop), `MissedTickBehavior::Skip` (no backlog),
/// per-tick `catch_unwind` (one bad tick doesn't kill the loop), panic
/// counter with quarantine. Each meaningful seam emits a `probe!` so the
/// JSONL probe sink records what actually happened on each module's tick
/// — operators (and replay) see tick spans, panics, errors, and cadence
/// changes without grep-prowling text logs.
pub(super) async fn run_tick_loop_for(
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
                continuum_base::probe!(
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
                continuum_base::probe!(
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
                    continuum_base::probe!(
                        class = "tick.quarantined",
                        module = module_name,
                        ticks_total = ticks_total
                    );
                    return;
                }
            }
        }

        // Re-read the cadence each iteration so a module can adjust it (back off
        // under pressure, an operator-tuned tick) — through `tick_interval_now()`, the
        // cheap read, NEVER `config()`: rebuilding the whole ModuleConfig per tick for
        // every module was card 948c30c2's row 4. `None` = the registered cadence
        // stands. If the period changed, rebuild the ticker — `Interval` doesn't
        // expose a setter for its period, and rebuilding is cheap on that slow path.
        let new_interval = module.tick_interval_now().unwrap_or(initial_interval);
        if new_interval != current_interval {
            continuum_base::probe!(
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
pub(super) fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&'static str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "<non-string panic payload>".to_string()
    }
}

/// Dispatch a command to a module's `handle_command`, wrapped in
/// `AssertUnwindSafe(...).catch_unwind()` so a panic inside the
/// handler converts to a typed `Err(String)` instead of poisoning
/// the dispatch task. Same RTOS-safe shape `start_tick_loops`,
/// `MemoryPressureMonitor`, `DiskPressureMonitor`, and the per-persona
/// `spawn_persona_service` use.
///
/// Why this is at the dispatch seam (not inside `handle_command`):
/// every module's `handle_command` lives behind the trait, and we
/// can't legislate `catch_unwind` discipline across the ~30 implementors
/// individually. Wrapping at the dispatch site protects every existing
/// module + every future module + the persona tool path that flows
/// through `Commands.execute` — one fix, blast radius bounded.
///
/// Panic events emit `probe!(class = "command.dispatch.panicked",
/// command, module, reason)` so the JsonlProbeFileSink + ProbeRouterLayer
/// already in tree surface them to operators on the canonical channel.
pub(crate) async fn dispatch_with_panic_guard(
    module: &Arc<dyn ServiceModule>,
    full_cmd: &str,
    params: serde_json::Value,
    module_name: &str,
) -> Result<CommandResult, String> {
    let result = AssertUnwindSafe(module.handle_command(full_cmd, params))
        .catch_unwind()
        .await;
    match result {
        Ok(r) => r,
        Err(panic) => {
            let panic_msg = panic_message(&*panic);
            error!(
                "Command '{}' panicked in module '{}': {}",
                full_cmd, module_name, panic_msg
            );
            continuum_base::probe!(
                class = "command.dispatch.panicked",
                command = full_cmd,
                module = module_name,
                reason = %panic_msg
            );
            Err(format!(
                "command '{full_cmd}' panicked in module '{module_name}': {panic_msg}"
            ))
        }
    }
}

/// Dispatch a self-routing [`DynCommand`](crate::sdk_codegen::DynCommand) object
/// under the same `catch_unwind` guard the module path uses. Persona tool calls
/// flow through here; a panicking handler converts to a typed `Err` instead of
/// poisoning the caller's task. The object owns its deps + knows its name, so the
/// guard needs nothing but the object and the params.
pub(crate) async fn dispatch_object_with_panic_guard(
    cmd: Arc<dyn crate::sdk_codegen::DynCommand>,
    params: serde_json::Value,
    caller: Option<crate::routing::CallerIdentity>,
) -> Result<CommandResult, String> {
    let name = cmd.name();
    let result = AssertUnwindSafe(cmd.invoke(params, caller))
        .catch_unwind()
        .await;
    match result {
        Ok(r) => r,
        Err(panic) => {
            let panic_msg = panic_message(&*panic);
            error!(
                "Command '{}' panicked in DynCommand object: {}",
                name, panic_msg
            );
            continuum_base::probe!(
                class = "command.dispatch.panicked",
                command = name,
                reason = %panic_msg
            );
            Err(format!("command '{name}' panicked: {panic_msg}"))
        }
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
/// The CONCERN a module belongs to — the decomposition dimension (orthogonal to
/// [`ModuleCategory`], which is the conditionality dimension). A process hosts a
/// *profile* = a set of `ServiceGroup`s; modules outside it are reached over the
/// bus (`route_command`). The groups partition `MODULES` exactly. See
/// docs/architecture/MODULAR-DECOMPOSITION.md.
///
/// `ServiceGroup` says WHERE a module can be placed (which process/container);
/// `ModuleCategory` says WHETHER it registers in a given `(discovery, mode)`.
/// One row carries both — still one source of truth, two views.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ServiceGroup {
    /// The minimal addressable substrate every node needs: commands/events/
    /// data/health/auth/system/runtime/mcp. A node is nothing without it.
    RuntimeShell,
    /// Per-node hardware governance (GPU, resource + pressure brokers).
    ResourceGov,
    /// The inference engine (llm/provider/embedding/vision/models/search/
    /// tool-parsing). Increasingly external  behind the AIProviderAdapter.
    Inference,
    /// The organism's brain — cognition/channel/allocator/agent/memory/rag +
    /// the persona-host modules.
    Cognition,
    /// Training / dev / self-improvement (forge/sentinel/plasticity/dataset/
    /// vdd/cargo/code). Bursty, GPU-heavy, schedulable separately.
    Forge,
    /// The bus itself (airc + grid transports).
    GridTransport,
    /// Avatar presence: Bevy render + LiveKit SFU. CO-LOCATED with the GPU (the
    /// readback→framebuffer→WebRTC transfer can't cross a process boundary).
    Live,
}

/// One module's placement metadata: its concern ([`ServiceGroup`]) + its
/// conditionality ([`ModuleCategory`]). Rows of [`MODULES`].
#[derive(Debug, Clone, Copy)]
pub struct ModuleSpec {
    pub name: &'static str,
    pub group: ServiceGroup,
    pub category: ModuleCategory,
}

impl ModuleSpec {
    pub(super) const fn new(
        name: &'static str,
        group: ServiceGroup,
        category: ModuleCategory,
    ) -> Self {
        Self {
            name,
            group,
            category,
        }
    }
}

pub const MODULES: &[ModuleSpec] = &[
    // RuntimeShell — the minimal addressable substrate
    ModuleSpec::new("health", ServiceGroup::RuntimeShell, ModuleCategory::Core),
    ModuleSpec::new("auth", ServiceGroup::RuntimeShell, ModuleCategory::Core),
    ModuleSpec::new("system", ServiceGroup::RuntimeShell, ModuleCategory::Core),
    ModuleSpec::new("events", ServiceGroup::RuntimeShell, ModuleCategory::Core),
    ModuleSpec::new("logger", ServiceGroup::RuntimeShell, ModuleCategory::Core),
    ModuleSpec::new("runtime", ServiceGroup::RuntimeShell, ModuleCategory::Core),
    ModuleSpec::new("mcp", ServiceGroup::RuntimeShell, ModuleCategory::Core),
    ModuleSpec::new("data", ServiceGroup::RuntimeShell, ModuleCategory::Core),
    // ResourceGov — hardware governance
    ModuleSpec::new("gpu", ServiceGroup::ResourceGov, ModuleCategory::Core),
    ModuleSpec::new(
        "resource-broker",
        ServiceGroup::ResourceGov,
        ModuleCategory::Core,
    ),
    ModuleSpec::new(
        "pressure-broker",
        ServiceGroup::ResourceGov,
        ModuleCategory::Core,
    ),
    // Inference — the engine, carried by the coordinator / handle / llm /
    // ai_provider modules below. (No bare `inference` module: listing one made
    // `required_modules()` demand a module that does not register, which
    // hard-failed boot with "missing [inference]".)
    ModuleSpec::new(
        "inference-coordinator",
        ServiceGroup::Inference,
        ModuleCategory::Core,
    ),
    ModuleSpec::new(
        "ai-inference-handle",
        ServiceGroup::Inference,
        ModuleCategory::Core,
    ),
    ModuleSpec::new(
        "inference-llm",
        ServiceGroup::Inference,
        ModuleCategory::Core,
    ),
    ModuleSpec::new("ai_provider", ServiceGroup::Inference, ModuleCategory::Core),
    ModuleSpec::new("embedding", ServiceGroup::Inference, ModuleCategory::Core),
    // (`search` was retired here: its commands migrated onto the DynCommand
    // registry in 9d96bb51c (#62, Wave 1) — they now live as stateless
    // self-routing commands under `commands/search/{vector,list,execute}`, so
    // there is no `SearchModule` ServiceModule to register. Leaving the spec in
    // made `required_modules()` demand a module that no longer registers, which
    // hard-failed boot with "missing [search]" — the same trap as the retired
    // `inference` shell above.)
    ModuleSpec::new("vision", ServiceGroup::Inference, ModuleCategory::Core),
    ModuleSpec::new("models", ServiceGroup::Inference, ModuleCategory::Core),
    // Cognition — the brain (always-built modules + the persona-host conditionals)
    ModuleSpec::new("memory", ServiceGroup::Cognition, ModuleCategory::Core),
    ModuleSpec::new("rag", ServiceGroup::Cognition, ModuleCategory::Core),
    ModuleSpec::new("cognition", ServiceGroup::Cognition, ModuleCategory::Core),
    ModuleSpec::new("channel", ServiceGroup::Cognition, ModuleCategory::Core),
    ModuleSpec::new(
        "persona_allocator",
        ServiceGroup::Cognition,
        ModuleCategory::Core,
    ),
    // (`agent` was retired here with `AgentModule`, a second autonomous loop beside the persona
    // minds (#5660). Its spec stayed and boot hard-failed with "missing [agent]" on the M5
    // (2026-10-31, canary 107e10ddc; rolled back by `continuum reboot`): the same trap as
    // `search` and `inference` above.)
    // Cognition — AIRC-Healthy-conditional persona hosting (registers only when
    // discovery succeeded across all four sub-steps)
    ModuleSpec::new(
        "persona_instance_manager",
        ServiceGroup::Cognition,
        ModuleCategory::PersonaHosting,
    ),
    ModuleSpec::new(
        "persona-rag-inspect",
        ServiceGroup::Cognition,
        ModuleCategory::PersonaHosting,
    ),
    // Forge — training / dev / self-improvement
    ModuleSpec::new("forge", ServiceGroup::Forge, ModuleCategory::Core),
    ModuleSpec::new("sentinel", ServiceGroup::Forge, ModuleCategory::Core),
    ModuleSpec::new("plasticity", ServiceGroup::Forge, ModuleCategory::Core),
    ModuleSpec::new("dataset", ServiceGroup::Forge, ModuleCategory::Core),
    ModuleSpec::new("vdd", ServiceGroup::Forge, ModuleCategory::Core),
    // (`cargo` was retired here: its commands migrated onto the DynCommand
    // registry in 98645f3d1 (#62), and the duplicate top-level `cargo/*` was
    // deleted in b19892b60 — `code/cargo/*` (carried by the `code` module below)
    // is now canonical. There is no `CargoModule` ServiceModule to register;
    // leaving the spec in hard-failed boot with "missing [cargo]".)
    ModuleSpec::new("code", ServiceGroup::Forge, ModuleCategory::Core),
    // GridTransport — the bus
    ModuleSpec::new("airc", ServiceGroup::GridTransport, ModuleCategory::Core),
    ModuleSpec::new("grid", ServiceGroup::GridTransport, ModuleCategory::Core),
    // Live — Bevy render + LiveKit SFU, CO-LOCATED with the GPU (Slice B' splits
    // these into renderer + voice sidecars within the same VM)
    ModuleSpec::new("live", ServiceGroup::Live, ModuleCategory::Core),
    ModuleSpec::new("avatar", ServiceGroup::Live, ModuleCategory::Core),
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
pub fn required_modules(discovery: &dyn MeshPresence, mode: BootMode) -> Vec<&'static str> {
    // The whole-substrate (monolith) profile — every group. Preserves the exact
    // historical behavior; slim processes call `required_modules_for_profile`.
    required_modules_for_profile(discovery, mode, &ServiceProfile::all())
}

/// The required-module set for a process hosting only `profile`'s service
/// groups. Filters `MODULES` by BOTH dimensions: the module's
/// [`ServiceGroup`] must be in the profile (placement) AND its
/// [`ModuleCategory`] conditionality must be satisfied for `(discovery, mode)`.
///
/// A slim process (e.g. `RuntimeShell + GridTransport`) boots only its groups;
/// commands for modules it doesn't host resolve over the bus (`route_command`).
/// `ServiceProfile::all()` reproduces the monolith exactly — decomposition is
/// optional placement, never forced fragmentation.
pub fn required_modules_for_profile(
    discovery: &dyn MeshPresence,
    mode: BootMode,
    profile: &ServiceProfile,
) -> Vec<&'static str> {
    let needs_persona_hosting = mode.requires_persona_hosting() && discovery.can_host_personas();
    MODULES
        .iter()
        .filter(|spec| profile.hosts(spec.group))
        .filter(|spec| match spec.category {
            ModuleCategory::Core => true,
            ModuleCategory::PersonaHosting => needs_persona_hosting,
        })
        .map(|spec| spec.name)
        .collect()
}

/// The modules in a [`ServiceGroup`] — the decomposition view of `MODULES`. A
/// process profile is a set of groups; `modules_in_group` enumerates each.
/// Derived from the single `MODULES` source (no parallel list).
pub fn modules_in_group(group: ServiceGroup) -> Vec<&'static str> {
    MODULES
        .iter()
        .filter(|spec| spec.group == group)
        .map(|spec| spec.name)
        .collect()
}

/// The [`ServiceGroup`] a module belongs to, or `None` if the name is unknown.
pub fn group_of(module: &str) -> Option<ServiceGroup> {
    MODULES
        .iter()
        .find(|spec| spec.name == module)
        .map(|spec| spec.group)
}

impl ServiceGroup {
    /// All groups in canonical order — the basis for `ServiceProfile::all()`
    /// and for iterating the taxonomy.
    pub const ALL: [ServiceGroup; 7] = [
        ServiceGroup::RuntimeShell,
        ServiceGroup::ResourceGov,
        ServiceGroup::Inference,
        ServiceGroup::Cognition,
        ServiceGroup::Forge,
        ServiceGroup::GridTransport,
        ServiceGroup::Live,
    ];

    /// The kebab-case wire label (`runtime-shell`, `grid-transport`, …) used in
    /// `--profile=...` and logs. Round-trips with [`ServiceGroup::from_str`].
    pub fn label(self) -> &'static str {
        match self {
            ServiceGroup::RuntimeShell => "runtime-shell",
            ServiceGroup::ResourceGov => "resource-gov",
            ServiceGroup::Inference => "inference",
            ServiceGroup::Cognition => "cognition",
            ServiceGroup::Forge => "forge",
            ServiceGroup::GridTransport => "grid-transport",
            ServiceGroup::Live => "live",
        }
    }
}

impl std::str::FromStr for ServiceGroup {
    type Err = ServiceProfileParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "runtime-shell" | "shell" => Ok(ServiceGroup::RuntimeShell),
            "resource-gov" | "resources" => Ok(ServiceGroup::ResourceGov),
            "inference" => Ok(ServiceGroup::Inference),
            "cognition" => Ok(ServiceGroup::Cognition),
            "forge" => Ok(ServiceGroup::Forge),
            "grid-transport" | "grid" => Ok(ServiceGroup::GridTransport),
            "live" => Ok(ServiceGroup::Live),
            other => Err(ServiceProfileParseError(other.to_string())),
        }
    }
}

/// The set of [`ServiceGroup`]s a process hosts — its placement profile. A node
/// declares one ("a good AWS template"); modules outside it are reached over the
/// bus. `RuntimeShell` is always included (a node is unaddressable without the
/// command/event/data shell) — so an operator can't accidentally compose a node
/// that can't even serve health.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceProfile {
    pub(super) groups: std::collections::HashSet<ServiceGroup>,
}

impl ServiceProfile {
    /// The whole substrate — every group. Reproduces the monolith exactly.
    pub fn all() -> Self {
        Self {
            groups: ServiceGroup::ALL.into_iter().collect(),
        }
    }

    /// A profile hosting exactly `groups` (+ the always-on `RuntimeShell`).
    pub fn from_groups(groups: impl IntoIterator<Item = ServiceGroup>) -> Self {
        let mut set: std::collections::HashSet<ServiceGroup> = groups.into_iter().collect();
        set.insert(ServiceGroup::RuntimeShell);
        Self { groups: set }
    }

    /// Does this profile host `group`?
    pub fn hosts(&self, group: ServiceGroup) -> bool {
        self.groups.contains(&group)
    }

    /// The hosted groups, canonical order (deterministic for logs/tests).
    pub fn groups(&self) -> Vec<ServiceGroup> {
        ServiceGroup::ALL
            .into_iter()
            .filter(|g| self.groups.contains(g))
            .collect()
    }
}

/// What the module census needs to know about this node's mesh connection: whether it can
/// host personas, and a name for the state in boot logs. `airc::AircDiscovery` implements it;
/// the kernel never names airc.
pub trait MeshPresence {
    fn can_host_personas(&self) -> bool;
    fn kind(&self) -> &'static str;
}
