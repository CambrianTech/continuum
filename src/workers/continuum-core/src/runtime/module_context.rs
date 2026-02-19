//! ModuleContext — the module's view of the runtime.
//!
//! Provided to every module during initialize() and available throughout lifetime.
//! Enables inter-module communication without tight coupling:
//! - Query other modules via registry (like CBAR's getAnalyzerOfType<T>())
//! - Publish/subscribe events via message bus
//! - Share lazy-computed values via shared compute cache
//! - Per-module logging via logger factory

use super::registry::ModuleRegistry;
use super::message_bus::MessageBus;
use super::shared_compute::SharedCompute;
use super::module_logger::ModuleLogger;
use dashmap::DashMap;
use std::sync::Arc;

pub struct ModuleContext {
    /// Query other modules and their current state (best-effort, non-blocking).
    /// Like CBAR's renderer->getAnalyzerOfType<T>().
    pub registry: Arc<ModuleRegistry>,

    /// Publish events for other modules to consume.
    /// Subscribe during initialize() based on config().event_subscriptions.
    pub bus: Arc<MessageBus>,

    /// Shared lazy-compute cache (like CBAR_VideoFrame's lazy getters).
    /// Modules store expensive computed results here for others to reuse.
    pub compute: Arc<SharedCompute>,

    /// Tokio runtime handle for spawning async work from sync contexts.
    /// Used when a module needs to call async code from within a rayon task.
    pub runtime: tokio::runtime::Handle,

    /// Per-module logger cache - created on demand, one per module.
    loggers: DashMap<&'static str, Arc<ModuleLogger>>,
}

impl ModuleContext {
    pub fn new(
        registry: Arc<ModuleRegistry>,
        bus: Arc<MessageBus>,
        compute: Arc<SharedCompute>,
        runtime: tokio::runtime::Handle,
    ) -> Self {
        Self {
            registry,
            bus,
            compute,
            runtime,
            loggers: DashMap::new(),
        }
    }

    /// Get or create a logger for a module.
    /// Each module gets its own log file: .continuum/jtag/logs/system/modules/{name}.log
    pub fn logger(&self, module_name: &'static str) -> Arc<ModuleLogger> {
        self.loggers
            .entry(module_name)
            .or_insert_with(|| Arc::new(ModuleLogger::new(module_name)))
            .clone()
    }
}
