//! Runtime — lifecycle orchestration for the modular runtime.
//!
//! Creates the registry, message bus, and shared compute cache.
//! Modules register, initialize, then the runtime serves IPC requests.
//!
//! This is the top-level coordinator — like CBAR's RenderingEngine
//! that owns the CBP_Analyzer pipeline and orchestrates frame flow.

use super::registry::ModuleRegistry;
use super::message_bus::MessageBus;
use super::shared_compute::SharedCompute;
use super::module_context::ModuleContext;
use super::service_module::{ServiceModule, CommandResult};
use std::sync::Arc;
use tokio::task::JoinHandle;
use tracing::{info, warn, error};

/// Expected modules that MUST be registered for a complete runtime.
/// Adding a module here ensures it cannot be forgotten during registration.
/// The server will fail to start if any expected module is missing.
pub const EXPECTED_MODULES: &[&str] = &[
    "health",     // Phase 1: stateless health checks
    "cognition",  // Phase 2: persona cognition engines
    "channel",    // Phase 2: persona channel registries
    "models",     // Phase 3: async model discovery
    "memory",     // Phase 3: persona memory manager
    "rag",        // Phase 3: batched RAG composition
    "voice",      // Phase 3: voice service, call manager
    "code",       // Phase 3: file engines, shell sessions
    "data",       // Phase 4: database ORM operations
    "logger",     // Phase 4a: structured logging
    "search",     // Phase 4b: BM25, TF-IDF, vector search
    "embedding",  // Phase 4c: fastembed vector generation
    "runtime",    // RuntimeModule: metrics and control
    "mcp",        // MCP server: dynamic tool discovery
];

pub struct Runtime {
    /// Registry uses interior mutability (DashMap + RwLock).
    /// Safe to share via Arc — register() takes &self.
    registry: Arc<ModuleRegistry>,
    bus: Arc<MessageBus>,
    compute: Arc<SharedCompute>,
}

impl Runtime {
    pub fn new() -> Self {
        Self {
            registry: Arc::new(ModuleRegistry::new()),
            bus: Arc::new(MessageBus::new()),
            compute: Arc::new(SharedCompute::new()),
        }
    }

    /// Register a module. Auto-wires command routing from its config.
    /// Like CBAR's appendAnalyzer() — one call, everything connected.
    pub fn register(&self, module: Arc<dyn ServiceModule>) {
        let config = module.config();
        info!("  Registering module: {} (priority: {:?}, commands: {:?})",
            config.name, config.priority, config.command_prefixes);

        // Wire event subscriptions into the message bus
        for pattern in config.event_subscriptions {
            self.bus.subscribe(pattern, config.name, false);
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
                    Ok(_) => info!("  {} initialized", name),
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
                if let Some(interval) = config.tick_interval {
                    let module_name = config.name;
                    let module = module.clone();
                    info!("Starting tick loop for '{}' (interval: {:?})", module_name, interval);

                    let handle = tokio::spawn(async move {
                        let mut ticker = tokio::time::interval(interval);
                        // First tick fires immediately — skip it so we don't
                        // tick before the system is fully warmed up
                        ticker.tick().await;

                        loop {
                            ticker.tick().await;
                            if let Err(e) = module.tick().await {
                                error!("Tick error in '{}': {}", module_name, e);
                            }
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

        // Execute command
        let result = module.handle_command(&full_cmd, params).await;

        // Record timing (automatic for ALL commands)
        if let Some(metrics) = metrics {
            let tracker = metrics.start_command(command, queued_at);
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

        // Use sync channel to bridge async -> sync safely
        let (tx, rx) = std::sync::mpsc::sync_channel(1);

        rt_handle.spawn(async move {
            let result = module.handle_command(&full_cmd, params).await;
            let _ = tx.send(result);
        });

        // Wait for result from the tokio task - NO TIMEOUT.
        // Voice/TTS commands can run indefinitely for streaming audio.
        // If the task panics, recv() returns Err(RecvError).
        let result = match rx.recv() {
            Ok(result) => result,
            Err(_) => {
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
            warn!("Unexpected module registered (not in EXPECTED_MODULES): {}", name);
        }

        // Fail if any expected modules are missing
        if !missing.is_empty() {
            let missing_list = missing.join(", ");
            error!("Missing required modules: {}", missing_list);
            error!("Expected {} modules, found {}", EXPECTED_MODULES.len(), registered.len());
            error!("Add missing module registrations in ipc/mod.rs or update EXPECTED_MODULES in runtime.rs");
            return Err(format!(
                "Module registration incomplete: missing [{}]. Server cannot start.",
                missing_list
            ));
        }

        info!("✅ All {} expected modules registered", EXPECTED_MODULES.len());
        Ok(())
    }
}
