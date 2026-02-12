//! Modular Runtime Framework
//!
//! RTOS-inspired module system for the Continuum Core process.
//! Every service module implements ONE trait (ServiceModule), registers with
//! the runtime, and commands route automatically. Like CBAR's appendAnalyzer().
//!
//! Components:
//! - ServiceModule: The ONE trait every module implements
//! - ModuleRegistry: DashMap-based command routing (replaces 55-arm match)
//! - MessageBus: Inter-module pub/sub with glob patterns
//! - SharedCompute: Lazy-compute-once cache (like CBAR_VideoFrame)
//! - ModuleContext: Module's view of the runtime
//! - ModuleLogger: Per-module segregated logging
//! - ModuleMetrics: Built-in IPC performance monitoring
//! - RuntimeControl: Priority adjustment API for UI
//! - Runtime: Lifecycle orchestration
//!
//! Global Logging:
//! - Any code can call `runtime::logger("component")` to get a logger
//! - Logs go to `.continuum/jtag/logs/system/modules/{component}.log`
//! - No need to pass loggers through function parameters

use std::sync::Arc;
use dashmap::DashMap;
use std::sync::OnceLock;

pub mod service_module;
pub mod registry;
pub mod message_bus;
pub mod shared_compute;
pub mod module_context;
pub mod module_logger;
pub mod module_metrics;
pub mod control;
pub mod runtime;

pub use service_module::{ServiceModule, ModuleConfig, ModulePriority, CommandResult, CommandSchema, ParamSchema};
pub use registry::ModuleRegistry;
pub use message_bus::MessageBus;
pub use shared_compute::SharedCompute;
pub use module_context::ModuleContext;
pub use module_logger::ModuleLogger;
pub use module_metrics::{ModuleMetrics, ModuleStats, CommandTiming};
pub use control::{RuntimeControl, ModuleInfo};
pub use runtime::Runtime;

// ============================================================================
// Global Logger Access
// ============================================================================

/// Global logger cache - any code can get a logger by component name
static GLOBAL_LOGGERS: OnceLock<DashMap<String, Arc<ModuleLogger>>> = OnceLock::new();

fn loggers() -> &'static DashMap<String, Arc<ModuleLogger>> {
    GLOBAL_LOGGERS.get_or_init(DashMap::new)
}

/// Get a logger for any component. Creates one if it doesn't exist.
/// This is the global entry point for logging - no ModuleContext needed.
///
/// Usage from anywhere in the codebase:
/// ```rust
/// use crate::runtime;
/// runtime::logger("inference").info("Model loaded");
/// runtime::logger("candle").warn("GPU sync slow");
/// ```
pub fn logger(component: &str) -> Arc<ModuleLogger> {
    let cache = loggers();

    if let Some(logger) = cache.get(component) {
        return logger.clone();
    }

    let logger = Arc::new(ModuleLogger::for_component(component));
    cache.insert(component.to_string(), logger.clone());
    logger
}
