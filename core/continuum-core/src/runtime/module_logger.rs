//! ModuleLogger — Per-module segregated logging.
//!
//! Each module gets its own log file: .continuum/jtag/logs/system/modules/{name}.log
//! Automatic category prefixing. Zero configuration for module authors.
//!
//! Usage:
//! - ServiceModules: Use `ctx.logger("module_name")` from ModuleContext
//! - Library code: Use `ModuleLogger::for_component("component_name")` for any code
//!   that needs logging but isn't a ServiceModule (e.g., AI adapters, inference code)

use std::fmt;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct ModuleLogger {
    module_name: String,
    log_file: Mutex<Option<std::fs::File>>,
    log_path: PathBuf,
}

impl ModuleLogger {
    /// Create a logger for a ServiceModule (called by ModuleContext)
    pub fn new(module_name: &'static str) -> Self {
        Self::for_component(module_name)
    }

    /// Create a logger for any component (adapters, libraries, etc.)
    /// This is the general-purpose constructor for non-module code.
    pub fn for_component(component_name: &str) -> Self {
        let file_name = format!("{}.log", component_name);
        // No resolvable home means no log file: the logger still works, it just has
        // nowhere durable to write (never fall back to the checkout/current dir).
        let log_dir = dirs::home_dir().map(|home| {
            home.join(".continuum")
                .join("jtag")
                .join("logs")
                .join("system")
                .join("modules")
        });
        let log_path = match &log_dir {
            Some(dir) => dir.join(&file_name),
            None => PathBuf::from(&file_name),
        };

        // Ensure directory exists, then open the log file (append mode)
        let file = log_dir.and_then(|dir| {
            let _ = fs::create_dir_all(&dir);
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(&log_path)
                .ok()
        });

        Self {
            module_name: component_name.to_string(),
            log_file: Mutex::new(file),
            log_path,
        }
    }

    fn write(&self, level: &str, msg: &str) {
        self.write_fmt(level, format_args!("{msg}"));
    }

    fn write_fmt(&self, level: &str, args: fmt::Arguments<'_>) {
        let timestamp = chrono::Utc::now().to_rfc3339();

        if let Ok(mut guard) = self.log_file.lock() {
            if let Some(ref mut file) = *guard {
                let _ = writeln!(
                    file,
                    "[{}] [{}] [{}] {}",
                    timestamp, level, &self.module_name, args
                );
                let _ = file.flush();
            }
        }
    }

    pub fn debug(&self, msg: &str) {
        self.write("DEBUG", msg);
    }

    pub fn debug_fmt(&self, args: fmt::Arguments<'_>) {
        self.write_fmt("DEBUG", args);
    }

    pub fn info(&self, msg: &str) {
        self.write("INFO", msg);
    }

    pub fn info_fmt(&self, args: fmt::Arguments<'_>) {
        self.write_fmt("INFO", args);
    }

    pub fn warn(&self, msg: &str) {
        self.write("WARN", msg);
    }

    pub fn warn_fmt(&self, args: fmt::Arguments<'_>) {
        self.write_fmt("WARN", args);
    }

    pub fn error(&self, msg: &str) {
        self.write("ERROR", msg);
    }

    pub fn error_fmt(&self, args: fmt::Arguments<'_>) {
        self.write_fmt("ERROR", args);
    }

    /// Structured timing log for performance analysis
    pub fn timing(&self, operation: &str, duration_ms: u64) {
        self.write_fmt("TIMING", format_args!("{operation} took {duration_ms}ms"));
    }

    /// Timing with metadata
    pub fn timing_with_meta(&self, operation: &str, duration_ms: u64, meta: &str) {
        self.write_fmt(
            "TIMING",
            format_args!("{operation} took {duration_ms}ms | {meta}"),
        );
    }

    pub fn log_path(&self) -> &PathBuf {
        &self.log_path
    }
}
