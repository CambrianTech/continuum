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

pub mod service_module;
pub mod registry;
pub mod message_bus;
pub mod shared_compute;
pub mod module_context;
pub mod module_logger;
pub mod module_metrics;
pub mod control;
pub mod runtime;

pub use service_module::{ServiceModule, ModuleConfig, ModulePriority, CommandResult};
pub use registry::ModuleRegistry;
pub use message_bus::MessageBus;
pub use shared_compute::SharedCompute;
pub use module_context::ModuleContext;
pub use module_logger::ModuleLogger;
pub use module_metrics::{ModuleMetrics, ModuleStats, CommandTiming};
pub use control::{RuntimeControl, ModuleInfo};
pub use runtime::Runtime;
