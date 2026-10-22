use super::*;

/// Maximum output lines retained per execution. Once exceeded, oldest lines
/// are drained (cursor positions adjusted accordingly). 10K lines ≈ 1MB text.
pub(super) const MAX_OUTPUT_LINES_PER_EXECUTION: usize = 10_000;

/// Maximum completed execution history entries per session.
pub(super) const MAX_HISTORY_ENTRIES: usize = 500;

/// Completed output retained between commands, per session. Active executions
/// keep their existing streaming limits; this is not a process memory ceiling.
pub(super) const MAX_COMPLETED_OUTPUT_BYTES: usize = 8 * 1024 * 1024;

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
    /// True once this execution's task has POSITIVE proof its direct child exited: a `wait` that
    /// returned its status (after an exit, a kill or a timeout), or a child that never started.
    /// A kill is confirmed by this, never by the `Killed` status, which is set before the child
    /// dies (`code/release-hands`).
    pub process_ended: bool,
    /// Why the task could NOT prove the child exited (its final `wait` failed). Never read as
    /// an exit: a release that meets it stays unresolved and keeps the context's handles.
    pub process_end_error: Option<String>,
    /// What the shell had started that was still running after the kill, by identity. Not
    /// assumed gone: a release re-kills and re-checks each one, and keeps the context while any
    /// is still present.
    pub unconfirmed_descendants: Vec<crate::inference::lane_process::ProcessIdentity>,
    /// Cursor: index of next stdout line to return on poll/watch.
    pub(super) stdout_cursor: usize,
    /// Cursor: index of next stderr line to return on poll/watch.
    pub(super) stderr_cursor: usize,
    /// Notified whenever new output lines arrive or execution finishes.
    /// Used by `watch()` to block without polling.
    pub output_notify: Arc<Notify>,
    /// Compiled sentinel filter rules (empty = pass all lines through as Info).
    pub sentinel: CompiledSentinel,
}

// Process custody cannot live only in bounded shell history: GC may drop a
// completed-looking execution whose descendants have not been confirmed ended.
pub(super) struct ShellCargoCustody {
    pub(super) state: Arc<Mutex<ExecutionState>>,
    pub(super) admission: Arc<super::super::hands::HandsLease>,
    pub(super) kept: crate::system_resources::disk_reporters::Kept,
    pub(super) completed: bool,
}

impl Drop for ShellCargoCustody {
    fn drop(&mut self) {
        let state = match self.state.lock() {
            Ok(state) => state,
            Err(_) => {
                self.admission.keep_unknown("shell process custody state is poisoned".into());
                self.admission.keep_held(&[self.kept.clone()]);
                return;
            }
        };
        // Spawn and publishing its PID are synchronous, with no cancellation point.
        if state.pid.is_none() { return; }
        if !self.completed {
            self.admission.keep_unknown("shell task cancelled before its process and output lifetime completed".into());
            self.admission.keep_held(&[self.kept.clone()]);
            return;
        }
        let end = crate::inference::lane_process::TreeEnd {
            reaped: if state.process_ended && state.process_end_error.is_none() { Ok(()) }
                else { Err(state.process_end_error.clone().unwrap_or_else(|| "shell task ended without confirming its child".into())) },
            survivors: state.unconfirmed_descendants.clone(),
            unwalked: None,
        };
        if !end.confirmed() {
            self.admission.keep_unconfirmed(&end);
            self.admission.keep_held(&[self.kept.clone()]);
        }
    }
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
    pub(super) rules: Vec<(Regex, OutputClassification, SentinelAction)>,
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
        let ts = continuum_base::time::EpochMs::now().millis();

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
/// Shell execution state bound to a workspace.
///
/// Retains explicitly configured cwd/env across command executions; changes made
/// inside a child shell are local to that process.
/// Each command runs in its own isolated process (bash -c "...").
/// Process-global bus handle for pushed shell completions — set once at boot
/// (ipc wiring), read by the detached exit-fold task which holds no self. Same
/// shape as the lane-demand / roster globals; None (tests, early boot) = the
/// old poll-only behavior, never an error.
pub(super) static SHELL_COMPLETION_BUS: std::sync::OnceLock<
    std::sync::Arc<crate::runtime::message_bus::MessageBus>,
> = std::sync::OnceLock::new();

/// Wire the completion bus (boot, once). Idempotent; later calls are no-ops.
pub fn set_shell_completion_bus(bus: std::sync::Arc<crate::runtime::message_bus::MessageBus>) {
    let _ = SHELL_COMPLETION_BUS.set(bus);
}

pub struct ShellSession {
    pub(super) id: String,
    pub(super) persona_id: String,
    pub(super) workspace_root: PathBuf,
    pub(super) cwd: PathBuf,
    pub(super) env: HashMap<String, String>,
    pub(super) executions: HashMap<String, Arc<Mutex<ExecutionState>>>,
    pub(super) history: Vec<ShellHistoryEntry>,
    pub(super) total_executions: u32,
    /// What this session's children may touch (card d598c806). `None` = the
    /// operator's own hands, unconfined; a citizen's session is confined by the
    /// workspace authority before her first command.
    pub(super) confinement: Option<crate::system_resources::confinement::Confinement>,
}
