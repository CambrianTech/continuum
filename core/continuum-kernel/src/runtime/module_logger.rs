//! ModuleLogger — Per-module segregated logging.
//!
//! Each module's lines go to its category, `modules/{name}`, through the one route every
//! `clog_*` line takes ([`crate::logging::clog_layer`]): the logger module's off-thread writer,
//! which owns the file (`<continuum home>/jtag/logs/system/modules/{name}.log`) and its size bound.
//! This type held a file of its own before (card c7be807a, 2026-11-02): opened on the caller's
//! thread, never rotated, and shared by name with the writer's `modules/{name}` file, so one
//! module's log reached 2.7 GB on the IntelMac and 6.2 GB on the M5.
//!
//! DEBUG lines go to `tracing` at debug, which the production filter (`info`) drops unless
//! `RUST_LOG` asks for them: the command executor's per-call routing line was the whole of its
//! 2.7 GB (17,963 of the last 17,964 lines).
//!
//! Usage:
//! - ServiceModules: Use `ctx.logger("module_name")` from ModuleContext
//! - Library code: Use `ModuleLogger::for_component("component_name")` for any code
//!   that needs logging but isn't a ServiceModule (e.g., AI adapters, inference code)

use std::fmt;

use tracing::Level;

use crate::logging::CLOG_TARGET;

pub struct ModuleLogger {
    module_name: String,
    category: String,
}

impl ModuleLogger {
    /// Create a logger for a ServiceModule (called by ModuleContext)
    pub fn new(module_name: &'static str) -> Self {
        Self::for_component(module_name)
    }

    /// Create a logger for any component (adapters, libraries, etc.)
    /// This is the general-purpose constructor for non-module code.
    pub fn for_component(component_name: &str) -> Self {
        Self {
            module_name: component_name.to_string(),
            category: format!("modules/{component_name}"),
        }
    }

    fn emit(&self, level: Level, args: fmt::Arguments<'_>) {
        let (category, component) = (&self.category, &self.module_name);
        // `event!` takes its level as a constant: one arm per level this logger writes.
        match level {
            Level::ERROR => tracing::event!(target: CLOG_TARGET, Level::ERROR, category = %category, component = %component, "{args}"),
            Level::WARN => tracing::event!(target: CLOG_TARGET, Level::WARN, category = %category, component = %component, "{args}"),
            _ => tracing::event!(target: CLOG_TARGET, Level::INFO, category = %category, component = %component, "{args}"),
        }
    }

    pub fn debug(&self, msg: &str) {
        tracing::debug!(module = %self.module_name, "{msg}");
    }

    pub fn info(&self, msg: &str) {
        self.emit(Level::INFO, format_args!("{msg}"));
    }

    pub fn info_fmt(&self, args: fmt::Arguments<'_>) {
        self.emit(Level::INFO, args);
    }

    pub fn warn(&self, msg: &str) {
        self.emit(Level::WARN, format_args!("{msg}"));
    }

    pub fn warn_fmt(&self, args: fmt::Arguments<'_>) {
        self.emit(Level::WARN, args);
    }

    pub fn error(&self, msg: &str) {
        self.emit(Level::ERROR, format_args!("{msg}"));
    }

    /// Structured timing log for performance analysis
    pub fn timing(&self, operation: &str, duration_ms: u64) {
        self.emit(Level::INFO, format_args!("TIMING {operation} took {duration_ms}ms"));
    }

    /// Timing with metadata
    pub fn timing_with_meta(&self, operation: &str, duration_ms: u64, meta: &str) {
        self.emit(Level::INFO, format_args!("TIMING {operation} took {duration_ms}ms | {meta}"));
    }
}
