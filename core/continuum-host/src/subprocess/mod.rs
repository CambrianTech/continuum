//! Subprocess spawning across the continuum: one adapter so every crate calls through host,
//! never `std::process::Command` or `tokio::process::Command` directly.
//!
//! The OS seam lives in [`exists`] — checking whether a binary is on PATH differs between
//! Unix (executable-in-`$PATH`) and Windows (`where.exe` + PATHEXT). Everything else — building,
//! spawning, capturing output, timing out — is identical across platforms and handled here.

#[cfg(windows)]
mod windows;

use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

/// Why a subprocess call failed.
#[derive(Debug)]
pub enum SpawnError {
    /// The binary was not found on PATH (checked via OS-specific `exists`).
    NotFound(String),
    /// The process timed out after [`RunConfig::timeout`].
    Timeout,
    /// An I/O error spawning or running the command.
    Io(std::io::Error),
}

impl fmt::Display for SpawnError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotFound(cmd) => write!(f, "command not found on PATH: {cmd}"),
            Self::Timeout => f.write_str("process timed out"),
            Self::Io(err) => write!(f, "I/O error: {err}"),
        }
    }
}

impl std::error::Error for SpawnError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            _ => None,
        }
    }
}

/// Captured output from a completed subprocess. Wraps `std::process::Output`.
#[derive(Debug, Clone)]
pub struct Output {
    /// The exit status (zero = success on POSIX; non-zero or failure otherwise).
    pub status: std::process::ExitStatus,
    /// Captured stdout bytes.
    pub stdout: Vec<u8>,
    /// Captured stderr bytes.
    pub stderr: Vec<u8>,
}

impl Output {
    /// Convert a `std::process::Output` (returned by tokio's async `.output()`) into ours.
    pub fn from_std(output: std::process::Output) -> Self {
        Self {
            status: output.status,
            stdout: output.stdout,
            stderr: output.stderr,
        }
    }

    /// Convenience: is the process exit-successful? (zero on POSIX, success code on Windows).
    pub fn success(&self) -> bool {
        self.status.success()
    }

    /// Exit code, if the process exited normally.
    pub fn exit_code(&self) -> Option<i32> {
        self.status.code()
    }
}

/// Configuration for a subprocess run. All fields are optional; defaults match
/// `std::process::Command` behaviour (no timeout, inherit environment, current dir).
#[derive(Debug, Clone, Default)]
pub struct RunConfig {
    /// Optional wall-clock timeout. When elapsed, the child is killed and [`SpawnError::Timeout`]
    /// is returned instead of the output.
    pub timeout: Option<Duration>,
    /// Working directory for the child process. If `None`, inherits from the parent.
    pub cwd: Option<PathBuf>,
    /// Environment variable overrides applied on top of (or replacing) the inherited environment.
    /// Each tuple is `(key, value)`. Keys are uppercased before comparison to match tokio's behaviour.
    pub envs: Vec<(String, String)>,
}

impl RunConfig {
    /// Apply config to a `tokio::process::Command` builder, returning it ready for `.spawn()` or
    /// `.output()`. Does NOT set the command name or arguments — those are caller-specific.
    pub fn apply_to(&self, cmd: &mut tokio::process::Command) {
        if let Some(cwd) = &self.cwd {
            cmd.current_dir(cwd);
        }
        for (key, value) in &self.envs {
            cmd.env(key, value);
        }
    }

    /// Build a configured command. The caller should set the program and arguments before calling
    /// this, or call [`Self::apply_to`] on an existing builder.
    pub fn build(&self, program: &str, args: &[&str]) -> tokio::process::Command {
        let mut cmd = tokio::process::Command::new(program);
        cmd.args(args);
        self.apply_to(&mut cmd);
        cmd
    }

    /// Run the command and capture output. Returns [`Output`] on success or a [`SpawnError`].
    pub async fn run(&self, program: &str, args: &[&str]) -> Result<Output, SpawnError> {
        let mut cmd = self.build(program, args);

        if let Some(timeout) = self.timeout {
            match tokio::time::timeout(timeout, cmd.output()).await {
                Ok(inner) => inner.map(Output::from_std).map_err(SpawnError::Io),
                Err(_) => Err(SpawnError::Timeout),
            }
        } else {
            cmd.output().await.map(Output::from_std).map_err(SpawnError::Io)
        }
    }

    /// Spawn a child process for async interaction (stdin/stdout piping, etc.). Returns the child.
    pub async fn spawn(&self, program: &str, args: &[&str]) -> Result<tokio::process::Child, SpawnError> {
        let mut cmd = self.build(program, args);
        cmd.spawn().map_err(SpawnError::Io)
    }
}

/// Check whether `binary` is reachable on PATH. This is the OS-differentiated seam: Unix checks
/// executable-in-`$PATH`; Windows uses `where.exe` + PATHEXT matching.
pub fn exists(binary: &str) -> bool {
    #[cfg(unix)]
    return unix::exists(binary);

    #[cfg(windows)]
    return windows::exists(binary);

    #[cfg(not(any(unix, windows)))]
    false
}

#[cfg(unix)]
mod unix;
