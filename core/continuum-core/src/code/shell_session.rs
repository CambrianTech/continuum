//! ShellSession — Persistent shell session per workspace.
//!
//! Provides a handle-based shell execution model:
//!   1. Create session (bound to workspace directory)
//!   2. Execute command → get execution handle immediately
//!   3. Poll execution handle → get new stdout/stderr chunks
//!   4. Or: execute with wait=true → block until complete
//!   5. Kill execution if needed
//!   6. Destroy session on cleanup
//!
//! Supports BOTH quick commands (wait=true → immediate result) and
//! long-running commands (poll repeatedly → streaming output).
//!
//! Each command runs in its own process for isolation. The session
//! maintains working directory and environment across executions.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use regex::Regex;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command as TokioCommand;
use tokio::sync::Notify;
use uuid::Uuid;

use super::shell_types::{
    ClassifiedLine, OutputClassification, SentinelAction, SentinelRule, ShellExecuteResponse,
    ShellExecutionStatus, ShellHistoryEntry, ShellPollResponse, ShellSessionInfo,
    ShellWatchResponse,
};
use crate::log_info;

/// Maximum output lines retained per execution. Once exceeded, oldest lines
/// are drained (cursor positions adjusted accordingly). 10K lines ≈ 1MB text.
const MAX_OUTPUT_LINES_PER_EXECUTION: usize = 10_000;

/// Maximum completed execution history entries per session.
const MAX_HISTORY_ENTRIES: usize = 500;

// ============================================================================
// Execution State (shared between tokio task and IPC handler)
// ============================================================================

/// Mutable state for a running or completed execution.
///
/// Written by the background tokio task (stdout/stderr lines, status).
/// Read by the IPC poll handler (cursor-based output retrieval) and watch handler.
pub struct ExecutionState {
    pub id: String,
    pub command: String,
    pub status: ShellExecutionStatus,
    pub stdout_lines: Vec<String>,
    pub stderr_lines: Vec<String>,
    pub exit_code: Option<i32>,
    pub pid: Option<u32>,
    pub started_at: u64,
    pub finished_at: Option<u64>,
    /// True once code/shell's inline window elapsed and the HANDLE was handed
    /// back to the caller (2026-08-24): completion must then be PUSHED — the
    /// exit fold publishes a command:completed event the dispatch listener
    /// folds into her working memory, so a finished build reaches her
    /// perception without her remembering to poll. False for in-window
    /// completions (the caller already holds the result).
    pub handed_back: bool,
    /// Cursor: index of next stdout line to return on poll/watch.
    stdout_cursor: usize,
    /// Cursor: index of next stderr line to return on poll/watch.
    stderr_cursor: usize,
    /// Notified whenever new output lines arrive or execution finishes.
    /// Used by `watch()` to block without polling.
    pub output_notify: Arc<Notify>,
    /// Compiled sentinel filter rules (empty = pass all lines through as Info).
    pub sentinel: CompiledSentinel,
}

impl std::fmt::Debug for ExecutionState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ExecutionState")
            .field("id", &self.id)
            .field("command", &self.command)
            .field("status", &self.status)
            .field("stdout_lines", &self.stdout_lines.len())
            .field("stderr_lines", &self.stderr_lines.len())
            .field("exit_code", &self.exit_code)
            .field("pid", &self.pid)
            .field("sentinel_rules", &self.sentinel.len())
            .finish()
    }
}

// ============================================================================
// Compiled Sentinel — pre-compiled regex rules for output classification
// ============================================================================

/// Pre-compiled sentinel rules for efficient per-line classification.
///
/// Regex patterns are compiled once when `set_sentinel()` is called,
/// then applied to every output line without re-compilation.
pub struct CompiledSentinel {
    rules: Vec<(Regex, OutputClassification, SentinelAction)>,
}

impl CompiledSentinel {
    /// Create an empty sentinel (passes all lines through as Info).
    pub fn empty() -> Self {
        Self { rules: Vec::new() }
    }

    /// Compile sentinel rules from wire format. Fails on invalid regex.
    pub fn compile(rules: &[SentinelRule]) -> Result<Self, String> {
        let mut compiled = Vec::with_capacity(rules.len());
        for rule in rules {
            let regex = Regex::new(&rule.pattern)
                .map_err(|e| format!("Invalid regex '{}': {}", rule.pattern, e))?;
            compiled.push((regex, rule.classification.clone(), rule.action.clone()));
        }
        Ok(Self { rules: compiled })
    }

    /// Number of active rules.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Check if sentinel has no rules.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }

    /// Classify a single output line. Returns None if the line should be suppressed.
    pub fn classify(&self, text: &str, stream: &str, line_num: u64) -> Option<ClassifiedLine> {
        let ts = now();

        if self.rules.is_empty() {
            // No sentinel configured — pass everything through as Info
            return Some(ClassifiedLine {
                text: text.to_string(),
                classification: OutputClassification::Info,
                line_number: line_num,
                stream: stream.to_string(),
                timestamp: ts,
            });
        }

        // First matching rule wins
        for (regex, classification, action) in &self.rules {
            if regex.is_match(text) {
                return match action {
                    SentinelAction::Emit => Some(ClassifiedLine {
                        text: text.to_string(),
                        classification: classification.clone(),
                        line_number: line_num,
                        stream: stream.to_string(),
                        timestamp: ts,
                    }),
                    SentinelAction::Suppress => None,
                };
            }
        }

        // No rule matched — emit as Verbose
        Some(ClassifiedLine {
            text: text.to_string(),
            classification: OutputClassification::Verbose,
            line_number: line_num,
            stream: stream.to_string(),
            timestamp: ts,
        })
    }
}

// ============================================================================
// Shell Session
// ============================================================================

/// A persistent shell session bound to a workspace.
///
/// Maintains working directory and environment across command executions.
/// Each command runs in its own isolated process (bash -c "...").
/// Process-global bus handle for pushed shell completions — set once at boot
/// (ipc wiring), read by the detached exit-fold task which holds no self. Same
/// shape as the lane-demand / roster globals; None (tests, early boot) = the
/// old poll-only behavior, never an error.
static SHELL_COMPLETION_BUS: std::sync::OnceLock<
    std::sync::Arc<crate::runtime::message_bus::MessageBus>,
> = std::sync::OnceLock::new();

/// Wire the completion bus (boot, once). Idempotent; later calls are no-ops.
pub fn set_shell_completion_bus(bus: std::sync::Arc<crate::runtime::message_bus::MessageBus>) {
    let _ = SHELL_COMPLETION_BUS.set(bus);
}

pub struct ShellSession {
    id: String,
    persona_id: String,
    workspace_root: PathBuf,
    cwd: PathBuf,
    env: HashMap<String, String>,
    executions: HashMap<String, Arc<Mutex<ExecutionState>>>,
    history: Vec<ShellHistoryEntry>,
    total_executions: u32,
}

impl ShellSession {
    /// Create a new shell session bound to a workspace directory.
    ///
    /// The workspace_root is canonicalized to resolve symlinks (required
    /// for reliable path containment checks on macOS where /var → /private/var).
    pub fn new(session_id: &str, persona_id: &str, workspace_root: &Path) -> Result<Self, String> {
        let canonical_root = workspace_root.canonicalize().map_err(|e| {
            format!(
                "Invalid workspace root '{}': {}",
                workspace_root.display(),
                e
            )
        })?;

        let cwd = canonical_root.clone();
        // Every session shares the machine's ONE cargo build cache. Citizen
        // layers make N isolated checkouts cheap (CoW — each stores only its
        // diff), but a `cargo build` inside a layer would otherwise materialize
        // a full per-layer target/ (~10 GB for continuum-core) and undo the
        // whole economy. Registry-dependency artifacts are path-independent, so
        // the shared cache dedupes nearly all of a build across peers; only the
        // small workspace leaf crates fingerprint per-path. Inherit the core's
        // own CARGO_TARGET_DIR when set, else the canonical shared location the
        // start script pins. A later explicit env set through the session still
        // overrides.
        let mut env = HashMap::new();
        let shared_target = std::env::var("CARGO_TARGET_DIR")
            .map(std::path::PathBuf::from)
            .ok()
            .or_else(|| dirs::home_dir().map(|h| h.join(".continuum/cache/cargo-target")));
        if let Some(t) = shared_target {
            env.insert("CARGO_TARGET_DIR".to_string(), t.display().to_string());
        }
        Ok(Self {
            id: session_id.to_string(),
            persona_id: persona_id.to_string(),
            workspace_root: canonical_root,
            cwd,
            env,
            executions: HashMap::new(),
            history: Vec::new(),
            total_executions: 0,
        })
    }

    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn persona_id(&self) -> &str {
        &self.persona_id
    }
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Set an environment variable for future commands.
    pub fn set_env(&mut self, key: String, value: String) {
        self.env.insert(key, value);
    }

    /// Change working directory. Validates the path stays within workspace.
    pub fn cd(&mut self, path: &str) -> Result<String, String> {
        let new_cwd = if Path::new(path).is_absolute() {
            PathBuf::from(path)
        } else {
            self.cwd.join(path)
        };

        let canonical = new_cwd
            .canonicalize()
            .map_err(|e| format!("Cannot cd to '{path}': {e}"))?;

        if !canonical.starts_with(&self.workspace_root) {
            return Err(format!(
                "Cannot cd to '{}': outside workspace boundary '{}'",
                path,
                self.workspace_root.display()
            ));
        }

        if !canonical.is_dir() {
            return Err(format!("Cannot cd to '{path}': not a directory"));
        }

        self.cwd = canonical.clone();
        Ok(canonical.display().to_string())
    }

    /// Get session info snapshot.
    pub fn info(&self) -> ShellSessionInfo {
        let active = self
            .executions
            .values()
            .filter(|e| {
                e.lock()
                    .map(|s| s.status == ShellExecutionStatus::Running)
                    .unwrap_or(false)
            })
            .count() as u32;

        ShellSessionInfo {
            session_id: self.id.clone(),
            persona_id: self.persona_id.clone(),
            cwd: self.cwd.display().to_string(),
            workspace_root: self.workspace_root.display().to_string(),
            active_executions: active,
            total_executions: self.total_executions,
        }
    }

    /// Start a command execution. Returns the execution ID immediately.
    ///
    /// The command runs asynchronously in a tokio task. Use `poll()` to
    /// retrieve output, or pass `wait=true` to `execute_and_wait()`.
    pub fn execute(
        &mut self,
        command: &str,
        timeout_ms: Option<u64>,
        rt_handle: &tokio::runtime::Handle,
    ) -> Result<String, String> {
        // Auto-GC: move completed executions to history before starting new one
        self.gc();

        let execution_id = Uuid::new_v4().to_string();
        let now_ms = now();

        let notify = Arc::new(Notify::new());
        let state = Arc::new(Mutex::new(ExecutionState {
            id: execution_id.clone(),
            command: command.to_string(),
            status: ShellExecutionStatus::Running,
            stdout_lines: Vec::new(),
            stderr_lines: Vec::new(),
            exit_code: None,
            pid: None,
            started_at: now_ms,
            finished_at: None,
            handed_back: false,
            stdout_cursor: 0,
            stderr_cursor: 0,
            output_notify: notify,
            sentinel: CompiledSentinel::empty(),
        }));

        self.executions.insert(execution_id.clone(), state.clone());
        self.total_executions += 1;

        // Spawn the process in a tokio task
        let cwd = self.cwd.clone();
        let env = self.env.clone();
        let cmd_str = command.to_string();

        rt_handle.spawn(async move {
            run_shell_command(state, &cmd_str, &cwd, &env, timeout_ms).await;
        });

        log_info!(
            "code",
            "shell",
            "Execution {} started: {}",
            &execution_id[..8],
            command
        );
        Ok(execution_id)
    }

    /// Execute a command and wait asynchronously until completion.
    ///
    /// For quick commands (git status, ls, etc.) where you want the result
    /// immediately rather than polling. This version is async-safe and won't
    /// block the tokio runtime.
    pub async fn execute_and_wait_async(
        &mut self,
        command: &str,
        timeout_ms: Option<u64>,
        rt_handle: &tokio::runtime::Handle,
    ) -> Result<ShellExecuteResponse, String> {
        let execution_id = self.execute(command, timeout_ms, rt_handle)?;

        // Get execution state and notify handle
        let state_arc = self
            .executions
            .get(&execution_id)
            .ok_or_else(|| "Execution vanished".to_string())?
            .clone();

        // Completion is REPORTED by the spawned `run_shell_command` task (status +
        // notify). If that task dies silently (a panic in a spawned task drops the
        // JoinHandle with no log line), status stays Running forever and a bare
        // `notified().await` hangs the caller — glass-boxed 2026-07-11: two eval
        // runs froze mid-exam on a `cargo test` act with NO child process alive.
        // So completion must be STRUCTURAL, not trusted: re-check state at least
        // every 2s regardless of notifications, and enforce a hard deadline
        // (caller timeout + grace; generous default — cargo builds are long) after
        // which we fail LOUD with the execution id instead of hanging
        // ([[fallbacks-are-illegal-fail-loud]], task #85).
        const RECHECK_EVERY: Duration = Duration::from_secs(2);
        const DEADLINE_GRACE_MS: u64 = 30_000;
        const DEFAULT_DEADLINE_MS: u64 = 600_000;
        let deadline = Duration::from_millis(
            timeout_ms.map_or(DEFAULT_DEADLINE_MS, |t| t.saturating_add(DEADLINE_GRACE_MS)),
        );
        let started = std::time::Instant::now();

        loop {
            let notify = {
                let s = state_arc
                    .lock()
                    .map_err(|e| format!("Lock poisoned: {e}"))?;
                if s.status != ShellExecutionStatus::Running {
                    return Ok(ShellExecuteResponse {
                        execution_id: s.id.clone(),
                        status: s.status.clone(),
                        stdout: Some(s.stdout_lines.join("\n")),
                        stderr: Some(s.stderr_lines.join("\n")),
                        exit_code: s.exit_code,
                    });
                }
                s.output_notify.clone()
            };

            if started.elapsed() >= deadline {
                return Err(format!(
                    "shell execution '{execution_id}' still reports Running after \
                     {}s with no completion signal — the runner task likely died \
                     without reporting; refusing to wait forever",
                    deadline.as_secs()
                ));
            }

            // Bounded wait: a notification wakes us immediately; a lost/never-sent
            // one degrades to a 2s re-check instead of an infinite hang.
            let _ = tokio::time::timeout(RECHECK_EVERY, notify.notified()).await;
        }
    }

    /// Execute a command and block until completion. Returns the full result.
    ///
    /// DEPRECATED: Use execute_and_wait_async instead. This version uses
    /// blocking sleep which can deadlock the tokio runtime.
    ///
    /// For quick commands (git status, ls, etc.) where you want the result
    /// immediately rather than polling.
    pub fn execute_and_wait(
        &mut self,
        command: &str,
        timeout_ms: Option<u64>,
        rt_handle: &tokio::runtime::Handle,
    ) -> Result<ShellExecuteResponse, String> {
        let execution_id = self.execute(command, timeout_ms, rt_handle)?;

        // Block this thread until the execution finishes
        let state_arc = self
            .executions
            .get(&execution_id)
            .ok_or_else(|| "Execution vanished".to_string())?
            .clone();

        // Poll until complete (on the current IPC thread)
        loop {
            {
                let s = state_arc
                    .lock()
                    .map_err(|e| format!("Lock poisoned: {e}"))?;
                if s.status != ShellExecutionStatus::Running {
                    return Ok(ShellExecuteResponse {
                        execution_id: s.id.clone(),
                        status: s.status.clone(),
                        stdout: Some(s.stdout_lines.join("\n")),
                        stderr: Some(s.stderr_lines.join("\n")),
                        exit_code: s.exit_code,
                    });
                }
            }
            // Yield briefly to let the tokio task progress
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Get the execution state arc for a given execution ID.
    /// Used to await completion without holding the DashMap lock.
    pub fn get_execution_state(&self, execution_id: &str) -> Option<Arc<Mutex<ExecutionState>>> {
        self.executions.get(execution_id).cloned()
    }

    /// Poll an execution for new output since the last poll.
    ///
    /// Returns new stdout/stderr lines and current status. Call repeatedly
    /// until `finished` is true. Cursor advances automatically — each line
    /// is returned exactly once across polls.
    pub fn poll(&self, execution_id: &str) -> Result<ShellPollResponse, String> {
        let state_arc = self
            .executions
            .get(execution_id)
            .ok_or_else(|| format!("No execution '{execution_id}'"))?;

        let mut state = state_arc
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;

        let new_stdout: Vec<String> = state.stdout_lines[state.stdout_cursor..].to_vec();
        let new_stderr: Vec<String> = state.stderr_lines[state.stderr_cursor..].to_vec();
        state.stdout_cursor = state.stdout_lines.len();
        state.stderr_cursor = state.stderr_lines.len();

        let finished = state.status != ShellExecutionStatus::Running;

        Ok(ShellPollResponse {
            execution_id: execution_id.to_string(),
            status: state.status.clone(),
            new_stdout,
            new_stderr,
            exit_code: state.exit_code,
            finished,
        })
    }

    /// Kill a running execution.
    ///
    /// Sets the kill flag; the background task detects it and terminates
    /// the child process. No-op if already finished.
    pub fn kill(&self, execution_id: &str) -> Result<(), String> {
        let state_arc = self
            .executions
            .get(execution_id)
            .ok_or_else(|| format!("No execution '{execution_id}'"))?;

        let mut state = state_arc
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;

        if state.status != ShellExecutionStatus::Running {
            return Ok(()); // Already done
        }

        // Signal kill — the tokio task will detect this and kill the child
        state.status = ShellExecutionStatus::Killed;
        state.finished_at = Some(now());

        // Also send SIGKILL via the stored PID for immediate effect
        if let Some(pid) = state.pid {
            kill_process(pid);
        }

        log_info!(
            "code",
            "shell",
            "Killed execution {}: {}",
            &execution_id[..8.min(execution_id.len())],
            state.command
        );
        Ok(())
    }

    /// Get history of completed executions.
    pub fn history(&self) -> &[ShellHistoryEntry] {
        &self.history
    }

    /// Garbage-collect completed executions, moving them to history.
    /// Called automatically before each new execution.
    pub fn gc(&mut self) {
        let completed_ids: Vec<String> = self
            .executions
            .iter()
            .filter_map(|(id, state)| {
                let s = state.lock().ok()?;
                if s.status != ShellExecutionStatus::Running {
                    Some(id.clone())
                } else {
                    None
                }
            })
            .collect();

        for id in completed_ids {
            if let Some(state_arc) = self.executions.remove(&id) {
                if let Ok(state) = state_arc.lock() {
                    self.history.push(ShellHistoryEntry {
                        execution_id: state.id.clone(),
                        command: state.command.clone(),
                        exit_code: state.exit_code,
                        started_at: state.started_at,
                        finished_at: state.finished_at,
                    });
                }
            }
        }

        // Cap history to prevent unbounded growth
        if self.history.len() > MAX_HISTORY_ENTRIES {
            let drain_count = self.history.len() - MAX_HISTORY_ENTRIES;
            self.history.drain(..drain_count);
        }
    }

    /// Kill all running executions and clear state.
    pub fn destroy(&mut self) {
        for (_, state_arc) in self.executions.iter() {
            if let Ok(mut state) = state_arc.lock() {
                if state.status == ShellExecutionStatus::Running {
                    state.status = ShellExecutionStatus::Killed;
                    state.finished_at = Some(now());
                    if let Some(pid) = state.pid {
                        kill_process(pid);
                    }
                }
            }
        }
        self.executions.clear();
    }

    // ════════════════════════════════════════════════════════════
    // Watch + Sentinel
    // ════════════════════════════════════════════════════════════

    /// Get execution state arc and notify handle for async watch.
    ///
    /// Returns clones that can be used after the DashMap lock is released.
    /// The caller MUST release any DashMap locks before awaiting on the Notify.
    pub fn get_watch_handles(
        &self,
        execution_id: &str,
    ) -> Result<(Arc<Mutex<ExecutionState>>, Arc<Notify>), String> {
        let exec_state = self
            .executions
            .get(execution_id)
            .ok_or_else(|| format!("No execution '{execution_id}'"))?
            .clone();
        let notify = exec_state
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?
            .output_notify
            .clone();
        Ok((exec_state, notify))
    }

    /// Configure sentinel filter rules on an execution.
    ///
    /// Rules are compiled to regexes immediately. Returns the count of rules applied.
    /// Pass an empty slice to clear sentinel (reverts to pass-all-as-Info).
    pub fn set_sentinel(
        &self,
        execution_id: &str,
        rules: &[SentinelRule],
    ) -> Result<usize, String> {
        let exec_state = self
            .executions
            .get(execution_id)
            .ok_or_else(|| format!("No execution '{execution_id}'"))?;

        let compiled = CompiledSentinel::compile(rules)?;
        let count = compiled.len();

        let mut state = exec_state
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;
        state.sentinel = compiled;
        Ok(count)
    }
}

/// Watch an execution for new output — blocks until output is available.
///
/// This is a free async function (not a method on ShellSession) because it must
/// be called AFTER releasing the DashMap lock. The caller extracts the handles
/// via `get_watch_handles()`, drops the DashMap ref, then calls this.
///
/// Uses `tokio::sync::Notify` — blocks without polling or timeouts.
/// Like `read()` on a Unix pipe: returns when data arrives.
pub async fn watch_execution(
    execution_id: &str,
    exec_state: Arc<Mutex<ExecutionState>>,
    notify: Arc<Notify>,
) -> Result<ShellWatchResponse, String> {
    loop {
        // Check for new data under the lock
        {
            let mut state = exec_state
                .lock()
                .map_err(|e| format!("Lock poisoned: {e}"))?;

            let has_new_stdout = state.stdout_cursor < state.stdout_lines.len();
            let has_new_stderr = state.stderr_cursor < state.stderr_lines.len();
            let is_finished = state.status != ShellExecutionStatus::Running;

            if has_new_stdout || has_new_stderr || is_finished {
                let lines = collect_and_classify(&mut state);
                return Ok(ShellWatchResponse {
                    execution_id: execution_id.to_string(),
                    lines,
                    finished: is_finished,
                    exit_code: state.exit_code,
                });
            }
        }
        // Lock released — safe to await
        // notify_one() stores a permit if nobody is waiting, so we won't
        // miss notifications between the lock release and this await.
        notify.notified().await;
    }
}

/// Collect new output lines since the cursors and classify them through sentinel rules.
fn collect_and_classify(state: &mut ExecutionState) -> Vec<ClassifiedLine> {
    let mut lines = Vec::new();

    // Collect stdout since cursor
    for i in state.stdout_cursor..state.stdout_lines.len() {
        if let Some(classified) =
            state
                .sentinel
                .classify(&state.stdout_lines[i], "stdout", i as u64)
        {
            lines.push(classified);
        }
    }
    state.stdout_cursor = state.stdout_lines.len();

    // Collect stderr since cursor
    for i in state.stderr_cursor..state.stderr_lines.len() {
        if let Some(classified) =
            state
                .sentinel
                .classify(&state.stderr_lines[i], "stderr", i as u64)
        {
            lines.push(classified);
        }
    }
    state.stderr_cursor = state.stderr_lines.len();

    lines
}

// ============================================================================
// Background Command Execution
// ============================================================================

/// Run a shell command asynchronously, streaming output into shared state.
///
/// This function runs in a tokio task. It:
/// 1. Spawns `bash -c "command"` with the session's cwd and env
/// 2. Reads stdout/stderr line-by-line into the shared ExecutionState
/// 3. Handles timeouts by killing the process
/// 4. Detects kill requests by checking the status flag
async fn run_shell_command(
    state: Arc<Mutex<ExecutionState>>,
    command: &str,
    cwd: &Path,
    env: &HashMap<String, String>,
    timeout_ms: Option<u64>,
) {
    // Resolve a REAL POSIX shell. `TokioCommand::new("bash")` finds
    // C:\Windows\System32\bash.exe on Windows — the WSL launcher, not a shell —
    // and every persona's `code/shell` died with
    //   execvpe(/bin/bash) failed: No such file or directory
    // while code/read, code/search and code/tree kept working, because only
    // execution goes through a shell. Observed live: two citizens cycling
    // through read-and-search all evening and never reaching execution. That
    // reads as an inert persona; it was a severed limb.
    let bash = match crate::shell_portable::locate_bash() {
        Ok(bash) => bash,
        Err(why) => {
            // Fail with the REASON, not with a spawn error from inside a child
            // that the caller cannot interpret.
            if let Ok(mut s) = state.lock() {
                s.status = ShellExecutionStatus::Failed;
                s.stderr_lines.push(format!("no usable shell: {why}"));
                s.finished_at = Some(now());
                s.output_notify.notify_one();
            }
            return;
        }
    };
    // Run as the USER'S LOGIN SHELL so every persona inherits the user's COMPLETE
    // environment — their PATH and every program they've installed (uv, cargo, node,
    // pyenv, git…) — exactly like Claude Code runs in the user's own shell. A bare
    // non-login `bash -c` only sees whatever env the CORE happened to launch with and
    // never sources the profile, so a persona could be missing the user's whole
    // toolchain (measured 2026-08-11: a login zsh resolves uv/cargo/node; a non-login
    // shell resolves none). On Unix use `$SHELL` (the user's own shell) as a login
    // shell; fall back to the located POSIX bash. Windows keeps the located bash (the
    // shell_portable WSL-shim guard) — its env model differs and $SHELL is absent.
    let (shell_bin, shell_flag): (PathBuf, &str) = if cfg!(windows) {
        (bash, "-c")
    } else {
        match std::env::var("SHELL")
            .ok()
            .filter(|s| !s.is_empty() && Path::new(s).exists())
        {
            Some(user_shell) => (PathBuf::from(user_shell), "-lc"),
            None => (bash, "-lc"),
        }
    };

    // Layer the per-task PATH prefix (the era venv's bin) ON TOP of the login-loaded
    // PATH. A login shell RE-SETS PATH from the profile, so an inherited PATH override
    // is clobbered (measured); instead the caller hands the prefix as
    // CONTINUUM_PATH_PREPEND and we prepend it AFTER the profile loads — the same thing
    // `source venv/bin/activate` does. No prefix → run the command verbatim. This is the
    // ONLY hand on PATH; the base environment is entirely the user's own.
    let effective_command = match env.get("CONTINUUM_PATH_PREPEND") {
        Some(prefix) if !prefix.is_empty() => {
            format!("export PATH=\"{prefix}:$PATH\"; {command}")
        }
        _ => command.to_string(),
    };

    let mut cmd = TokioCommand::new(&shell_bin);
    cmd.arg(shell_flag)
        .arg(&effective_command)
        .current_dir(cwd)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        // Don't inherit stdin — non-interactive
        .stdin(std::process::Stdio::null());

    // Apply session environment variables
    for (k, v) in env {
        cmd.env(k, v);
    }

    // Spawn the child process
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            if let Ok(mut s) = state.lock() {
                s.status = ShellExecutionStatus::Failed;
                s.stderr_lines.push(format!("Failed to spawn bash: {e}"));
                s.finished_at = Some(now());
                s.output_notify.notify_one();
            }
            return;
        }
    };

    // Store PID for external kill capability
    if let Some(pid) = child.id() {
        if let Ok(mut s) = state.lock() {
            s.pid = Some(pid);
        }
    }

    // Take stdout/stderr handles
    let stdout = child.stdout.take().expect("stdout piped");
    let stderr = child.stderr.take().expect("stderr piped");

    // Spawn line readers (notify watchers on each new line)
    let state_out = state.clone();
    let stdout_task = tokio::spawn(async move {
        let reader = BufReader::new(stdout);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Ok(mut s) = state_out.lock() {
                if s.status == ShellExecutionStatus::Killed {
                    break;
                }
                s.stdout_lines.push(line);
                // Cap output: drain oldest lines, adjusting cursor
                if s.stdout_lines.len() > MAX_OUTPUT_LINES_PER_EXECUTION {
                    let drain = s.stdout_lines.len() - MAX_OUTPUT_LINES_PER_EXECUTION;
                    s.stdout_lines.drain(..drain);
                    s.stdout_cursor = s.stdout_cursor.saturating_sub(drain);
                }
                s.output_notify.notify_one();
            }
        }
    });

    let state_err = state.clone();
    let stderr_task = tokio::spawn(async move {
        let reader = BufReader::new(stderr);
        let mut lines = reader.lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Ok(mut s) = state_err.lock() {
                if s.status == ShellExecutionStatus::Killed {
                    break;
                }
                s.stderr_lines.push(line);
                if s.stderr_lines.len() > MAX_OUTPUT_LINES_PER_EXECUTION {
                    let drain = s.stderr_lines.len() - MAX_OUTPUT_LINES_PER_EXECUTION;
                    s.stderr_lines.drain(..drain);
                    s.stderr_cursor = s.stderr_cursor.saturating_sub(drain);
                }
                s.output_notify.notify_one();
            }
        }
    });

    // Wait for process completion (with optional timeout and kill detection)
    let state_wait = state.clone();
    let exit_status = if let Some(timeout) = timeout_ms {
        tokio::select! {
            // Branch 1: Process completes
            result = child.wait() => {
                match result {
                    Ok(status) => Some(status),
                    Err(e) => {
                        if let Ok(mut s) = state_wait.lock() {
                            s.stderr_lines.push(format!("Process wait error: {e}"));
                        }
                        None
                    }
                }
            }
            // Branch 2: Timeout fires
            _ = tokio::time::sleep(Duration::from_millis(timeout)) => {
                // Check if already killed
                let already_done = state_wait.lock()
                    .map(|s| s.status != ShellExecutionStatus::Running)
                    .unwrap_or(false);

                if !already_done {
                    let _ = child.kill().await;
                    if let Ok(mut s) = state_wait.lock() {
                        if s.status == ShellExecutionStatus::Running {
                            s.status = ShellExecutionStatus::TimedOut;
                            s.stderr_lines.push(format!("Timed out after {timeout}ms"));
                            s.finished_at = Some(now());
                            s.output_notify.notify_one();
                        }
                    }
                }
                None
            }
        }
    } else {
        // No timeout — wait indefinitely, but check for kill
        let state_for_error = state.clone();
        let state_for_kill = state.clone();
        tokio::select! {
            result = child.wait() => {
                match result {
                    Ok(status) => Some(status),
                    Err(e) => {
                        if let Ok(mut s) = state_for_error.lock() {
                            s.stderr_lines.push(format!("Process wait error: {e}"));
                        }
                        None
                    }
                }
            }
            // Check kill flag periodically
            _ = poll_kill_flag(state_for_kill) => {
                let _ = child.kill().await;
                None
            }
        }
    };

    // Wait for output readers to drain
    let _ = stdout_task.await;
    let _ = stderr_task.await;

    // Update final state (if not already set by timeout/kill)
    if let Some(status) = exit_status {
        if let Ok(mut s) = state.lock() {
            if s.status == ShellExecutionStatus::Running {
                s.exit_code = status.code();
                s.status = if status.success() {
                    ShellExecutionStatus::Completed
                } else {
                    ShellExecutionStatus::Failed
                };
                s.finished_at = Some(now());
                // Wake any blocked watch() calls to deliver final status
                s.output_notify.notify_one();
                // PUSHED COMPLETION (2026-08-24): a handed-back execution's
                // finish previously reached her only if she remembered to
                // poll. Publish the same command:completed shape tracked
                // dispatches use; the dispatch listener folds it into her
                // working memory IF the handle was registered (apply.rs does,
                // on handback).
                if s.handed_back {
                    if let (Some(bus), Ok(handle)) =
                        (SHELL_COMPLETION_BUS.get(), uuid::Uuid::parse_str(&s.id))
                    {
                        let mut lines: Vec<&String> =
                            s.stdout_lines.iter().chain(s.stderr_lines.iter()).collect();
                        let cut = lines.len().saturating_sub(20);
                        let tail: String = lines
                            .split_off(cut)
                            .into_iter()
                            .map(String::as_str)
                            .collect::<Vec<_>>()
                            .join("\n");
                        let ev = crate::runtime::command_events::CommandCompletedEvent {
                            command_name: "code/shell".to_string(),
                            duration_ms: s.finished_at.unwrap_or(0).saturating_sub(s.started_at), // set 3 lines up; 0 → saturates to 0ms, display only
                            success: s.exit_code == Some(0),
                            error: (s.exit_code != Some(0))
                                .then(|| format!("exit {}", s.exit_code.map_or(-1, |c| c))),
                            handle: Some(handle),
                            result: Some(serde_json::json!({
                                "exitCode": s.exit_code,
                                "tail": tail,
                            })),
                        };
                        if let Ok(payload) = serde_json::to_value(&ev) { // bus fan-out boundary — the same encode tracked dispatches pay
                            bus.publish_async_only(
                                crate::runtime::command_events::COMMAND_COMPLETED_TOPIC,
                                payload,
                            );
                        }
                    }
                }

                log_info!(
                    "code",
                    "shell",
                    "Execution {} finished: exit={} cmd={}",
                    &s.id[..8],
                    s.exit_code.unwrap_or(-1), // killed/no-status path reads as conventional -1 in the error line
                    &s.command
                );
            }
        }
    }
}

/// Poll the kill flag on the execution state. Returns when kill is requested.
async fn poll_kill_flag(state: Arc<Mutex<ExecutionState>>) {
    loop {
        {
            if let Ok(s) = state.lock() {
                if s.status != ShellExecutionStatus::Running {
                    return;
                }
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// Kill a process by PID (best-effort, Unix only).
fn kill_process(pid: u32) {
    // Use kill command — works on macOS and Linux, no extra deps
    let _ = std::process::Command::new("kill")
        .args(["-9", &pid.to_string()])
        .output();
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn setup_workspace() -> (tempfile::TempDir, tokio::runtime::Runtime) {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src")).unwrap();
        fs::write(dir.path().join("src/main.ts"), "console.log('hello');").unwrap();
        let rt = tokio::runtime::Runtime::new().unwrap();
        (dir, rt)
    }

    #[test]
    fn test_session_creation() {
        let (dir, _rt) = setup_workspace();
        let session = ShellSession::new("test-session", "persona-1", dir.path()).unwrap();

        assert_eq!(session.id(), "test-session");
        assert_eq!(session.persona_id(), "persona-1");
        // cwd and workspace_root are canonicalized (macOS: /var → /private/var)
        let canonical = dir.path().canonicalize().unwrap();
        assert_eq!(session.cwd(), canonical);
        assert_eq!(session.workspace_root(), canonical);
    }

    // what this catches: the shared-build-cache invariant of the citizen-layer
    // economy (Joel 2026-07-10: layers are cheap ONLY if builds share ONE
    // cargo cache). A session must be born with CARGO_TARGET_DIR pointing at
    // the shared cache, so a peer's `cargo build` inside her CoW layer never
    // materializes a per-layer target/ (~10 GB each). An explicit session env
    // set must still override.
    #[test]
    fn session_inherits_the_shared_cargo_target_dir() {
        let (dir, _rt) = setup_workspace();
        let mut session = ShellSession::new("t", "p1", dir.path()).unwrap();
        let target = session
            .env
            .get("CARGO_TARGET_DIR")
            .expect("born with the shared cargo cache");
        assert!(
            target.contains("cargo-target") || std::env::var("CARGO_TARGET_DIR").is_ok(),
            "points at the shared cache: {target}"
        );
        session.set_env("CARGO_TARGET_DIR".into(), "/tmp/override".into());
        assert_eq!(
            session.env.get("CARGO_TARGET_DIR").unwrap(),
            "/tmp/override"
        );
    }

    #[test]
    fn test_cd_within_workspace() {
        let (dir, _rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        let result = session.cd("src");
        assert!(result.is_ok());
        assert!(session.cwd().ends_with("src"));
    }

    #[test]
    fn test_cd_outside_workspace_blocked() {
        let (dir, _rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        let result = session.cd("..");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside workspace"));
    }

    #[test]
    fn test_cd_nonexistent_blocked() {
        let (dir, _rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        let result = session.cd("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_execute_quick_command() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        let result = session.execute_and_wait("echo hello", Some(5000), rt.handle());
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.status, ShellExecutionStatus::Completed);
        assert_eq!(response.exit_code, Some(0));
        assert!(response.stdout.unwrap().contains("hello"));
    }

    #[test]
    fn test_execute_failing_command() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        let result = session.execute_and_wait("exit 42", Some(5000), rt.handle());
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.status, ShellExecutionStatus::Failed);
        assert_eq!(response.exit_code, Some(42));
    }

    #[test]
    fn test_execute_with_cwd() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        // cd into src, then run pwd
        session.cd("src").unwrap();
        let result = session.execute_and_wait("pwd", Some(5000), rt.handle());
        assert!(result.is_ok());

        let response = result.unwrap();
        let stdout = response.stdout.unwrap();
        assert!(
            stdout.contains("src"),
            "pwd should show src dir: {}",
            stdout
        );
    }

    #[test]
    fn test_execute_with_env() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        session.set_env("MY_VAR".to_string(), "hello_world".to_string());
        let result = session.execute_and_wait("echo $MY_VAR", Some(5000), rt.handle());
        assert!(result.is_ok());

        let response = result.unwrap();
        assert!(response.stdout.unwrap().contains("hello_world"));
    }

    #[test]
    fn test_poll_pattern() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        // Execute asynchronously
        let exec_id = session
            .execute(
                "echo line1; echo line2; echo line3",
                Some(5000),
                rt.handle(),
            )
            .unwrap();

        // Poll until finished
        let mut all_stdout = Vec::new();
        loop {
            std::thread::sleep(Duration::from_millis(50));
            let poll = session.poll(&exec_id).unwrap();
            all_stdout.extend(poll.new_stdout);
            if poll.finished {
                assert_eq!(poll.exit_code, Some(0));
                break;
            }
        }

        assert_eq!(all_stdout, vec!["line1", "line2", "line3"]);
    }

    #[test]
    fn test_timeout() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        // Command that sleeps longer than timeout
        let result = session.execute_and_wait("sleep 30", Some(500), rt.handle());
        assert!(result.is_ok());

        let response = result.unwrap();
        assert_eq!(response.status, ShellExecutionStatus::TimedOut);
    }

    #[test]
    fn test_kill_execution() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        // Start a long-running command
        let exec_id = session.execute("sleep 60", None, rt.handle()).unwrap();

        // Give it a moment to start
        std::thread::sleep(Duration::from_millis(200));

        // Kill it
        session.kill(&exec_id).unwrap();

        // Poll should show killed
        std::thread::sleep(Duration::from_millis(200));
        let poll = session.poll(&exec_id).unwrap();
        assert!(poll.finished);
        assert_eq!(poll.status, ShellExecutionStatus::Killed);
    }

    #[test]
    fn test_session_info() {
        let (dir, _rt) = setup_workspace();
        let session = ShellSession::new("test-session", "persona-1", dir.path()).unwrap();

        let info = session.info();
        assert_eq!(info.session_id, "test-session");
        assert_eq!(info.persona_id, "persona-1");
        assert_eq!(info.active_executions, 0);
        assert_eq!(info.total_executions, 0);
    }

    #[test]
    fn test_gc_moves_to_history() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        // Run a command to completion
        let _result = session.execute_and_wait("echo done", Some(5000), rt.handle());

        assert!(session.history().is_empty());

        // GC should move it to history
        session.gc();

        assert_eq!(session.history().len(), 1);
        assert_eq!(session.history()[0].command, "echo done");
        assert_eq!(session.history()[0].exit_code, Some(0));
    }

    #[test]
    fn test_destroy_kills_running() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        // Start long-running command
        let _exec_id = session.execute("sleep 60", None, rt.handle()).unwrap();

        std::thread::sleep(Duration::from_millis(200));

        // Destroy should kill it
        session.destroy();
        assert!(session.executions.is_empty());
    }

    #[test]
    fn test_multiple_executions() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        // Run multiple sequential commands
        let r1 = session
            .execute_and_wait("echo first", Some(5000), rt.handle())
            .unwrap();
        let r2 = session
            .execute_and_wait("echo second", Some(5000), rt.handle())
            .unwrap();
        let r3 = session
            .execute_and_wait("echo third", Some(5000), rt.handle())
            .unwrap();

        assert_eq!(r1.status, ShellExecutionStatus::Completed);
        assert_eq!(r2.status, ShellExecutionStatus::Completed);
        assert_eq!(r3.status, ShellExecutionStatus::Completed);
        assert!(r1.stdout.unwrap().contains("first"));
        assert!(r2.stdout.unwrap().contains("second"));
        assert!(r3.stdout.unwrap().contains("third"));
    }

    #[test]
    fn test_command_reads_workspace_files() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        let result = session
            .execute_and_wait("cat src/main.ts", Some(5000), rt.handle())
            .unwrap();

        assert_eq!(result.status, ShellExecutionStatus::Completed);
        assert!(result.stdout.unwrap().contains("console.log"));
    }

    #[test]
    fn test_stderr_capture() {
        let (dir, rt) = setup_workspace();
        let mut session = ShellSession::new("test", "p1", dir.path()).unwrap();

        let result = session
            .execute_and_wait("echo error_msg >&2", Some(5000), rt.handle())
            .unwrap();

        assert_eq!(result.status, ShellExecutionStatus::Completed);
        assert!(result.stderr.unwrap().contains("error_msg"));
    }
}
