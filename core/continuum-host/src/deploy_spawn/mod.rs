//! Deploy consumer spawning — shared types and public API.
//!
//! Each module holds the contract once; its `unix.rs` or `windows.rs` holds only the calls that
//! differ, chosen at the module boundary by the `#[cfg_attr]` re-export. Nothing in this crate
//! branches on the OS for these capabilities — callers always use [`spawn_deploy`] and let the
//! platform decide.

use std::path::{Path, PathBuf};

/// What one spawn needs: CLI path, arguments, working directory, and log destination.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpawnPlan {
    /// The CLI binary to run (e.g., the `continuum` executable).
    pub cli: PathBuf,
    /// Arguments to pass (e.g., `["deploy-consume"]`).
    pub args: Vec<String>,
    /// Working directory for the child process.
    pub cwd: PathBuf,
    /// Log file path for stdout/stderr capture.
    pub log: PathBuf,
}

/// The result of a spawn attempt. `child` is held only when the caller spawned the process
/// directly (a task run or scheduler has no child handle to hold).
#[derive(Debug)]
pub struct Spawned {
    /// The child's PID when this process spawned it; `None` when a scheduler ran it.
    pub pid: Option<u32>,
    /// Human-readable description of how the consumer was launched.
    pub mode: String,
    /// The child handle for direct spawns (task runs have none).
    pub child: Option<std::process::Child>,
}

/// A probe function that checks whether an OS task is runnable and launches it. Returns `Some(mode)`
/// when the scheduler launched the consumer (no child handle to return), or `None` when the caller
/// should fall back to a direct spawn. The callback receives the working directory so it can resolve
/// paths relative to the deploy target.
pub type ProbeFn = fn(&Path) -> Option<String>;

#[cfg_attr(unix, path = "unix.rs")]
#[cfg_attr(windows, path = "windows.rs")]
mod os;

/// Spawn the deploy consumer for `plan`. On Unix this launches a detached process in its own group;
/// on Windows it first tries the registered scheduler task (via `probe`) and falls back to a breakaway
/// spawn when no task is available. The probe closure runs platform-specific checks — it is passed as
/// a parameter so this crate does not depend on any Continuum subsystem that provides bounded probes.
pub fn spawn_deploy(plan: &SpawnPlan, probe: ProbeFn) -> Result<Spawned, String> {
    os::spawn(plan, probe)
}
