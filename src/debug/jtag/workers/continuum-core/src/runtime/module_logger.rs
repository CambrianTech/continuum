//! ModuleLogger — Per-module segregated logging.
//!
//! Each module gets its own log file: .continuum/jtag/logs/system/modules/{name}.log
//! Automatic category prefixing. Zero configuration for module authors.
//!
//! Usage:
//! - ServiceModules: Use `ctx.logger("module_name")` from ModuleContext
//! - Library code: Use `ModuleLogger::for_component("component_name")` for any code
//!   that needs logging but isn't a ServiceModule (e.g., AI adapters, inference code)

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
        let log_dir = PathBuf::from(".continuum/jtag/logs/system/modules");
        let log_path = log_dir.join(format!("{}.log", component_name));

        // Ensure directory exists
        let _ = fs::create_dir_all(&log_dir);

        // Open log file (append mode)
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
            .ok();

        Self {
            module_name: component_name.to_string(),
            log_file: Mutex::new(file),
            log_path,
        }
    }

    fn write(&self, level: &str, msg: &str) {
        let timestamp = chrono::Utc::now().to_rfc3339();
        let line = format!("[{}] [{}] [{}] {}\n", timestamp, level, &self.module_name, msg);

        if let Ok(mut guard) = self.log_file.lock() {
            if let Some(ref mut file) = *guard {
                let _ = file.write_all(line.as_bytes());
                let _ = file.flush();
            }
        }
    }

    pub fn debug(&self, msg: &str) {
        self.write("DEBUG", msg);
    }

    pub fn info(&self, msg: &str) {
        self.write("INFO", msg);
    }

    pub fn warn(&self, msg: &str) {
        self.write("WARN", msg);
    }

    pub fn error(&self, msg: &str) {
        self.write("ERROR", msg);
    }

    /// Structured timing log for performance analysis
    pub fn timing(&self, operation: &str, duration_ms: u64) {
        self.write("TIMING", &format!("{} took {}ms", operation, duration_ms));
    }

    /// Timing with metadata
    pub fn timing_with_meta(&self, operation: &str, duration_ms: u64, meta: &str) {
        self.write("TIMING", &format!("{} took {}ms | {}", operation, duration_ms, meta));
    }

    pub fn log_path(&self) -> &PathBuf {
        &self.log_path
    }
}
