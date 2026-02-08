/// Runtime — lifecycle orchestration for the modular runtime.
///
/// Creates the registry, message bus, and shared compute cache.
/// Modules register, initialize, then the runtime serves IPC requests.
///
/// This is the top-level coordinator — like CBAR's RenderingEngine
/// that owns the CBP_Analyzer pipeline and orchestrates frame flow.

use super::registry::ModuleRegistry;
use super::message_bus::MessageBus;
use super::shared_compute::SharedCompute;
use super::module_context::ModuleContext;
use super::service_module::{ServiceModule, CommandResult};
use std::sync::Arc;
use tracing::{info, warn, error};

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

    /// Route a command through the registry.
    /// Returns None if no module handles this command.
    pub async fn route_command(
        &self,
        command: &str,
        params: serde_json::Value,
    ) -> Option<Result<CommandResult, String>> {
        let (module, full_cmd) = self.registry.route_command(command)?;
        Some(module.handle_command(&full_cmd, params).await)
    }

    /// Route a command synchronously (for use from rayon threads).
    /// Spawns async work on tokio and bridges via sync channel.
    /// This avoids "Cannot start a runtime from within a runtime" panics.
    pub fn route_command_sync(
        &self,
        command: &str,
        params: serde_json::Value,
        rt_handle: &tokio::runtime::Handle,
    ) -> Option<Result<CommandResult, String>> {
        let (module, full_cmd) = self.registry.route_command(command)?;

        // Use sync channel to bridge async -> sync safely
        let (tx, rx) = std::sync::mpsc::sync_channel(1);

        rt_handle.spawn(async move {
            let result = module.handle_command(&full_cmd, params).await;
            let _ = tx.send(result);
        });

        // Wait for result from the tokio task with timeout.
        // If sqlite worker is backed up, fail gracefully instead of blocking forever.
        match rx.recv_timeout(std::time::Duration::from_secs(30)) {
            Ok(result) => Some(result),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                error!("Command handler timeout after 30s: {}", command);
                Some(Err(format!("Command timeout: {}", command)))
            }
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                error!("Command handler task was dropped");
                Some(Err("Command handler task was dropped".to_string()))
            }
        }
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
}
