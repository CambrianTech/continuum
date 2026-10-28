use super::*;

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
    /// the command factory supplied by the composition root.
    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        (self.services.commands)(self.registry.clone())
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
