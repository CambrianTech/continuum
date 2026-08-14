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

use dashmap::DashMap;
use std::sync::Arc;
use std::sync::OnceLock;

pub mod airc_interceptor;
pub mod artifact_handle;
pub mod boot_mode;
pub mod boot_status;
pub mod brain_region;
pub mod cadence_table;
pub mod cell_shapes;
pub mod command_envelope;
pub mod command_events;
pub mod command_executor;
pub mod command_interceptor;
pub mod control;
pub mod core_bind_guard;
pub mod core_ipc_transport;
pub mod deploy_provenance;
pub mod daemon;
pub mod governor_bus;
pub mod grid_interceptor;
pub mod handle;
pub mod in_process_transport;
pub mod late_bound;
pub mod message_bus;
pub mod module_context;
/// Per-module TDD harness — boots a single module in isolation. Test-only.
#[cfg(any(test, feature = "test-fixtures"))]
pub mod module_harness;
pub mod module_logger;
pub mod module_metrics;
pub mod orientation_shares;
pub mod per_key_gate;
pub mod provided_provider;
pub mod ready_buffer;
pub mod region_telemetry;
pub mod registry;
#[allow(clippy::module_inception)]
pub mod runtime;
pub mod service_module;
pub mod share_controller;
pub mod shared_compute;
pub mod substrate_governor;

pub use boot_mode::{extract_boot_mode, BootMode, BootModeParseError};

pub use airc_interceptor::AircInterceptor;
pub use artifact_handle::{ArtifactKey, ArtifactSelector, Cadence};
pub use brain_region::{
    BrainRegion, CadenceHint, ComputeClass, MemoryClass, Orientation, PersonaLifecycle,
    PressureLevel, PressureProfile, PressureSignalKind, RegionContext, RegionError, RegionId,
    RegionSignal, SleepPhase, TickOutcome,
};
pub use cadence_table::{CadenceKey, CadenceTable};
pub use cell_shapes::{HandleRef, LambdaPlaceholder, StreamPlaceholder};
pub use command_envelope::{CommandRequest, CommandResponse};
pub use command_events::{CommandCompletedEvent, COMMAND_COMPLETED_TOPIC};
pub use command_executor::CommandExecutor;
pub use command_interceptor::{CommandInterceptor, InterceptorOutcome};
pub use control::{ModuleInfo, RuntimeControl};
pub use daemon::{
    guarded, spawn_daemon, Daemon, DaemonChannel, DaemonHandle, Guarded, QuarantineLedger,
    DEFAULT_QUARANTINE_LIMIT,
};
pub use governor_bus::{publish_persona_scheduled, PersonaScheduled, PERSONA_SCHEDULED_KEY};
pub use grid_interceptor::GridInterceptor;
pub use handle::Handle;
pub use in_process_transport::InProcessTransport;
pub use late_bound::LateBound;
pub use message_bus::{BusEvent, MessageBus};
pub use module_context::ModuleContext;
pub use module_logger::ModuleLogger;
pub use module_metrics::{CommandTiming, ModuleMetrics, ModuleStats};
pub use orientation_shares::{
    apportion, orientation_index, OrientationCounts, OrientationShares, ORIENTATIONS,
};
pub use per_key_gate::{Lease, PerKeyGate};
pub use provided_provider::{
    ProvidedCommandInterceptor, ProvidedCommandProvider, ProviderRegistry,
};
pub use ready_buffer::{DashMapReadyBuffer, ReadyBuffer};
pub use region_telemetry::RegionTelemetry;
pub use registry::ModuleRegistry;
pub use runtime::Runtime;
pub use service_module::{
    CommandResult, CommandSchema, ModuleConfig, ModulePriority, ParamSchema, ServiceModule,
};
pub use share_controller::ShareController;
pub use shared_compute::SharedCompute;
pub use substrate_governor::{GovernorSnapshot, SubstrateGovernor};

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
