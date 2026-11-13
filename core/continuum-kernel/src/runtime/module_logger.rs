//! ModuleLogger — Per-module segregated logging.
//!
//! Each module's lines go to its category, `modules/{name}`, through the one route every
//! `clog_*` line takes ([`crate::logging::clog_layer`]): the logger module's off-thread writer,
//! which owns the file (`<continuum home>/jtag/logs/system/modules/{name}.log`) and its size bound.
//! This type held a file of its own before (card c7be807a, 2026-11-02): opened on the caller's
//! thread, never rotated, and shared by name with the writer's `modules/{name}` file, so one
//! module's log reached 2.7 GB on the IntelMac and 6.2 GB on the M5.
//!
//! DEBUG and per-call TIMING lines go to `tracing` at debug, which the production filter (`info`)
//! drops unless `RUST_LOG` asks for them: the command executor's per-call routing line was the
//! whole of its 2.7 GB (17,963 of the last 17,964 lines), and the M5's data.log was per-query
//! TIMING lines.
//!
//! Usage:
//! - ServiceModules: Use `ctx.logger("module_name")` from ModuleContext
//! - Library code: Use `ModuleLogger::for_component("component_name")` for any code
//!   that needs logging but isn't a ServiceModule (e.g., AI adapters, inference code)

use std::fmt;

use crate::logging::CLOG_TARGET;

/// The levels a module's file takes. DEBUG and TIMING are not among them: they go to `tracing` at
/// debug, which the production filter drops.
#[derive(Clone, Copy)]
enum Level {
    Info,
    Warn,
    Error,
}

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
            Level::Error => tracing::event!(target: CLOG_TARGET, tracing::Level::ERROR, category = %category, component = %component, "{args}"),
            Level::Warn => tracing::event!(target: CLOG_TARGET, tracing::Level::WARN, category = %category, component = %component, "{args}"),
            Level::Info => tracing::event!(target: CLOG_TARGET, tracing::Level::INFO, category = %category, component = %component, "{args}"),
        }
    }

    pub fn debug(&self, msg: &str) {
        tracing::debug!(module = %self.module_name, "{msg}");
    }

    pub fn info(&self, msg: &str) {
        self.emit(Level::Info, format_args!("{msg}"));
    }

    pub fn info_fmt(&self, args: fmt::Arguments<'_>) {
        self.emit(Level::Info, args);
    }

    pub fn warn(&self, msg: &str) {
        self.emit(Level::Warn, format_args!("{msg}"));
    }

    pub fn warn_fmt(&self, args: fmt::Arguments<'_>) {
        self.emit(Level::Warn, args);
    }

    pub fn error(&self, msg: &str) {
        self.emit(Level::Error, format_args!("{msg}"));
    }

    /// One operation's duration, at debug: a per-call line (the M5's data.log held 31,341
    /// per-query TIMING lines in its last 3 MB, Fable on #5710), so production drops it.
    pub fn timing(&self, operation: &str, duration_ms: u64) {
        tracing::debug!(module = %self.module_name, operation, duration_ms, "timing");
    }

    /// One operation's duration with its context, at debug, like [`Self::timing`].
    pub fn timing_with_meta(&self, operation: &str, duration_ms: u64, meta: &str) {
        tracing::debug!(module = %self.module_name, operation, duration_ms, meta, "timing");
    }
}
