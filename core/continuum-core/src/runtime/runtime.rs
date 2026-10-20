//! Runtime — lifecycle orchestration for the modular runtime.
//!
//! Creates the registry, message bus, and shared compute cache.
//! Modules register, initialize, then the runtime serves IPC requests.
//!
//! This is the top-level coordinator — like CBAR's RenderingEngine
//! that owns the CBP_Analyzer pipeline and orchestrates frame flow.

use super::boot_mode::BootMode;
use super::command_interceptor::{CommandInterceptor, InterceptorOutcome};
use super::message_bus::MessageBus;
use super::module_context::ModuleContext;
use super::registry::ModuleRegistry;
use super::service_module::{CommandResult, ServiceModule};
use super::shared_compute::SharedCompute;
use crate::airc::AircDiscovery;
use dashmap::DashMap;
use futures::FutureExt;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;
use tokio::task::JoinHandle;
use tracing::{error, info, warn};

// Module manifest lives in the `module_manifest` mod below
// (MODULES_CORE / MODULES_PERSONA_HOSTING / ALL_KNOWN_MODULES /
// required_modules) so all module-state truth is in one place
// and the snapshot test has one anchor point.

mod shutdown_phase;
pub use shutdown_phase::*;
mod runtime;
pub use runtime::*;
mod service_profile;
pub use service_profile::*;
mod the_receipt_survives_every_observer_going_away;
mod recording_module;
use recording_module::*;
