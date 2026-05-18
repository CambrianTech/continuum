//! Runtime — lifecycle orchestration for the modular runtime.
//!
//! Creates the registry, message bus, and shared compute cache.
//! Modules register, initialize, then the runtime serves IPC requests.
//!
//! This is the top-level coordinator — like CBAR's RenderingEngine
//! that owns the CBP_Analyzer pipeline and orchestrates frame flow.

use super::message_bus::MessageBus;
use super::module_context::ModuleContext;
use super::registry::ModuleRegistry;
use super::service_module::{CommandResult, ServiceModule};
use super::shared_compute::SharedCompute;
use dashmap::DashMap;
use std::sync::Arc;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

/// Expected modules that MUST be registered for a complete runtime.
/// Adding a module here ensures it cannot be forgotten during registration.
/// The server will fail to start if any expected module is missing.
pub const EXPECTED_MODULES: &[&str] = &[
    "gpu",               // Phase 0: GPU memory management
    "health",            // Phase 1: stateless health checks
    "cognition",         // Phase 2: persona cognition engines
    "channel",           // Phase 2: persona channel registries
    "models",            // Phase 3: async model discovery
    "memory",            // Phase 3: persona memory manager
    "rag",               // Phase 3: batched RAG composition
    "live",              // Phase 3: live experience (voice, video, transport)
    "code",              // Phase 3: file engines, shell sessions
    "data",              // Phase 4: database ORM operations
    "logger",            // Phase 4a: structured logging
    "search",            // Phase 4b: BM25, TF-IDF, vector search
    "embedding",         // Phase 4c: fastembed vector generation
    "grid",              // Grid transport: inter-node routing (Tailscale, Reticulum)
    "runtime",           // RuntimeModule: metrics and control
    "mcp",               // MCP server: dynamic tool discovery
    "system",            // System resources: CPU, memory, process monitoring
    "avatar",            // Avatar snapshots: Bevy 3D renders → PNG
    "dataset",           // Dataset import/management for Academy training
    "persona_allocator", // Hardware-aware persona allocation decisions
    "inference-llm",     // Phase 5: local LLM generation (MODULE-CATALOG §II)
];

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

    /// Start periodic tick loops for modules that declare a tick_interval.
    /// Each module with a tick_interval gets its own tokio task that calls tick()
    /// at the specified cadence. This replaces TypeScript's per-persona setIntervals.
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

                    let handle = tokio::spawn(async move {
                        // Initial delay — don't tick before system is warmed up
                        tokio::time::sleep(initial_interval).await;

                        loop {
                            if let Err(e) = module.tick().await {
                                error!("Tick error in '{}': {}", module_name, e);
                            }
                            // Re-read interval from module config each iteration.
                            // This allows dynamic cadence changes (e.g. via channel/tick-config).
                            let interval =
                                module.config().tick_interval.unwrap_or(initial_interval);
                            tokio::time::sleep(interval).await;
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

    /// Verify all expected modules are registered.
    /// Fails with a clear error if any module is missing.
    /// Call after all registrations to ensure nothing was forgotten.
    pub fn verify_registration(&self) -> Result<(), String> {
        let registered: Vec<String> = self.registry.module_names();
        let mut missing: Vec<&str> = Vec::new();
        let mut unexpected: Vec<String> = Vec::new();

        // Check for missing expected modules
        for expected in EXPECTED_MODULES {
            if !registered.iter().any(|r| r == *expected) {
                missing.push(expected);
            }
        }

        // Check for unexpected registered modules (not necessarily an error, just a warning)
        for registered_name in &registered {
            if !EXPECTED_MODULES.contains(&registered_name.as_str()) {
                unexpected.push(registered_name.clone());
            }
        }

        // Log warnings for unexpected modules
        for name in &unexpected {
            warn!(
                "Unexpected module registered (not in EXPECTED_MODULES): {}",
                name
            );
        }

        // Fail if any expected modules are missing
        if !missing.is_empty() {
            let missing_list = missing.join(", ");
            error!("Missing required modules: {}", missing_list);
            error!(
                "Expected {} modules, found {}",
                EXPECTED_MODULES.len(),
                registered.len()
            );
            error!("Add missing module registrations in ipc/mod.rs or update EXPECTED_MODULES in runtime.rs");
            return Err(format!(
                "Module registration incomplete: missing [{}]. Server cannot start.",
                missing_list
            ));
        }

        info!(
            "✅ All {} expected modules registered",
            EXPECTED_MODULES.len()
        );
        Ok(())
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
            .publish(
                "anything/at/all",
                serde_json::json!({}),
                runtime.registry(),
            )
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
