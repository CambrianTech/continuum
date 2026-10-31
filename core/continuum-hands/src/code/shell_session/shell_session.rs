use super::*;

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
        let shared_target = crate::paths::shared_cargo_target_dir();
        if let Some(t) = shared_target {
            let value = t.into_os_string().into_string()
                .map_err(|_| "shell session environment requires a UTF-8 Cargo target; refusing to alter its native path".to_string())?;
            env.insert("CARGO_TARGET_DIR".to_string(), value);
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
            confinement: None,
        })
    }

    /// Confine every later child of this session to `confinement`'s roots. Set by the
    /// workspace authority for a citizen; the operator's session never calls it.
    pub fn confine(&mut self, confinement: crate::system_resources::confinement::Confinement) {
        self.confinement = Some(confinement);
    }

    pub fn confinement(&self) -> Option<&crate::system_resources::confinement::Confinement> {
        self.confinement.as_ref()
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

        let target = match self.env.get("CARGO_TARGET_DIR").filter(|value| !value.is_empty()) {
            Some(target) => crate::paths::cargo::resolve_target(Path::new(target), &self.cwd),
            None => crate::paths::cargo::target_dir(&self.cwd)?,
        };
        let target_env = target.to_str()
            .ok_or_else(|| "shell session environment requires a UTF-8 Cargo target; refusing to alter its native path".to_string())?.to_owned();

        let execution_id = Uuid::new_v4().to_string();
        let now_ms = continuum_base::time::EpochMs::now().millis();

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
            process_ended: false,
            process_end_error: None,
            unconfirmed_descendants: Vec::new(),
            stdout_cursor: 0,
            stderr_cursor: 0,
            output_notify: notify,
            sentinel: CompiledSentinel::empty(),
        }));

        self.executions.insert(execution_id.clone(), state.clone());
        self.total_executions += 1;

        // Spawn the process in a tokio task
        let cwd = self.cwd.clone();
        let mut env = self.env.clone();
        env.insert("CARGO_TARGET_DIR".to_string(), target_env);
        let cmd_str = command.to_string();
        let confinement = self.confinement.clone();
        let who = self.persona_id.clone();

        // This task can outlive the command. Release must reach the existing
        // shell kill path before waiting for the task's process/output lifetime.
        let admission = crate::identity::hands_gate::current().map(|lease| Arc::new(lease.process_custody()));
        rt_handle.spawn(async move {
            let result = crate::identity::hands_gate::admitted(admission, crate::identity::hands_gate::scoped_admitted(async {
            let cargo_grant = match crate::system_resources::disk_reporters::admit_cargo_target(&target, &cwd, confinement.as_ref()).await {
                Ok(grant) => grant,
                Err(error) => {
                    if let Ok(mut state) = state.lock() {
                        state.status = ShellExecutionStatus::Failed;
                        state.stderr_lines.push(format!("Cargo cache ownership: {error}"));
                        state.finished_at = Some(continuum_base::time::EpochMs::now().millis());
                        state.process_ended = true; // admission refused before any child existed
                        state.output_notify.notify_one();
                    }
                    return;
                }
            };
            let Some(admission) = crate::identity::hands_gate::current() else {
                if let Ok(mut state) = state.lock() {
                    state.status = ShellExecutionStatus::Failed;
                    state.stderr_lines.push("Shell has no process custody admission".into());
                    state.process_ended = true;
                    state.finished_at = Some(continuum_base::time::EpochMs::now().millis());
                    state.output_notify.notify_one();
                }
                return;
            };
            let mut custody = ShellCargoCustody { state: state.clone(), admission, kept: cargo_grant.kept(), completed: false };
            run_shell_command(
                state.clone(),
                &cmd_str, &cwd, &env, timeout_ms, confinement.as_ref(), &who, cargo_grant.grant(),
            )
            .await;
            custody.completed = true;
            })).await;
            if let Err(error) = result {
                if let Ok(mut state) = state.lock() {
                    state.status = ShellExecutionStatus::Failed;
                    state.stderr_lines.push(format!("Shell process admission: {error}"));
                    state.finished_at = Some(continuum_base::time::EpochMs::now().millis());
                    state.process_ended = true;
                    state.output_notify.notify_one();
                }
            }
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
                        next: None,
                    });
                }
            }
            // Yield briefly to let the tokio task progress
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Owned handles, including retained terminal metadata after output eviction.
    /// This lets cross-workspace lookup report eviction rather than an unknown ID.
    pub fn execution_ids(&self) -> impl Iterator<Item = &str> {
        self.executions.keys().map(String::as_str).chain(
            self.history.iter()
                .filter(|entry| !self.executions.contains_key(&entry.execution_id))
                .map(|entry| entry.execution_id.as_str()),
        )
    }

    /// Resolve a full execution ID or unique hex prefix in this session only.
    /// Used to await completion without holding the DashMap lock.
    pub fn get_execution_state(&self, execution_id: &str) -> Result<Arc<Mutex<ExecutionState>>, String> {
        if let Some(state) = self.executions.get(execution_id) {
            return Ok(state.clone());
        }
        if self.history.iter().any(|entry| entry.execution_id == execution_id) {
            return Err(format!("Output for completed shell execution '{execution_id}' was evicted by the session retention limit; exit/timing metadata remains in shell history"));
        }
        let candidates: Vec<Uuid> = self
            .executions
            .keys()
            .map(|id| {
                Uuid::parse_str(id).map_err(|e| format!("invalid stored execution handle: {e}"))
            })
            .collect::<Result<_, _>>()?;
        let id = continuum_base::id_resolve::resolve_handle(execution_id, &candidates, "shell execution")?;
        self.executions.get(&id.to_string()).cloned()
            .ok_or_else(|| format!("No execution '{execution_id}' in this shell session"))
    }

    /// Poll an execution for new output since the last poll.
    ///
    /// Returns new stdout/stderr lines and current status. Call repeatedly
    /// until `finished` is true. Cursor advances automatically — each line
    /// is returned exactly once across polls.
    pub fn poll(&self, execution_id: &str) -> Result<ShellPollResponse, String> {
        let state_arc = self.get_execution_state(execution_id)?;

        let mut state = state_arc
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;

        let new_stdout: Vec<String> = state.stdout_lines[state.stdout_cursor..].to_vec();
        let new_stderr: Vec<String> = state.stderr_lines[state.stderr_cursor..].to_vec();
        state.stdout_cursor = state.stdout_lines.len();
        state.stderr_cursor = state.stderr_lines.len();

        let finished = state.status != ShellExecutionStatus::Running;

        Ok(ShellPollResponse {
            execution_id: state.id.clone(),
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
        let state_arc = self.get_execution_state(execution_id)?;

        let mut state = state_arc
            .lock()
            .map_err(|e| format!("Lock poisoned: {e}"))?;

        if state.status != ShellExecutionStatus::Running {
            return Ok(()); // Already done
        }

        // Signal kill — the tokio task will detect this and kill the child
        state.status = ShellExecutionStatus::Killed;
        state.finished_at = Some(continuum_base::time::EpochMs::now().millis());

        // The execution's own task, which owns the child, sees the flag at its next poll and
        // kills the shell and everything it started. No second kill by stored pid.

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

    /// Index completed executions once and retain their output for the bounded history
    /// lifetime. Running executions are never evicted; restart/destruction ends retention.
    pub fn gc(&mut self) {
        self.retain_completed(MAX_HISTORY_ENTRIES, MAX_COMPLETED_OUTPUT_BYTES);
    }

    pub(super) fn retain_completed(&mut self, max_entries: usize, max_output_bytes: usize) {
        let indexed: std::collections::HashSet<&str> = self.history.iter()
            .map(|entry| entry.execution_id.as_str()).collect();
        let completed: Vec<ShellHistoryEntry> = self.executions.iter()
            .filter_map(|(id, state)| {
                if indexed.contains(id.as_str()) { return None; }
                let state = state.lock().ok()?;
                if state.status == ShellExecutionStatus::Running { return None; }
                Some(ShellHistoryEntry {
                    execution_id: state.id.clone(), command: state.command.clone(),
                    exit_code: state.exit_code, started_at: state.started_at,
                    finished_at: state.finished_at,
                })
            }).collect();
        // HashMap order must not choose which completed receipt is oldest.
        self.history.extend(completed);
        self.history.sort_by(|a, b| {
            (a.finished_at, &a.execution_id).cmp(&(b.finished_at, &b.execution_id))
        });
        if self.history.len() > max_entries {
            let drain_count = self.history.len() - max_entries;
            for entry in self.history.drain(..drain_count) {
                self.executions.remove(&entry.execution_id);
            }
        }
        // Keep newest receipts within the byte allowance. Count allocations,
        // not just line count: a single output line can be arbitrarily large.
        // Eviction preserves history metadata and never selects active work.
        let mut retained_bytes = 0usize;
        let mut exhausted = false;
        for entry in self.history.iter().rev() {
            let Some(state) = self.executions.get(&entry.execution_id) else { continue; };
            let Ok(state) = state.lock() else { continue; };
            if state.status == ShellExecutionStatus::Running { continue; }
            let bytes = state.stdout_lines.iter().chain(&state.stderr_lines)
                .fold(0usize, |sum, line| sum.saturating_add(line.capacity()))
                .saturating_add((state.stdout_lines.capacity() + state.stderr_lines.capacity())
                    .saturating_mul(std::mem::size_of::<String>()));
            drop(state);
            if exhausted || bytes > max_output_bytes.saturating_sub(retained_bytes) {
                exhausted = true;
                self.executions.remove(&entry.execution_id);
            } else {
                retained_bytes += bytes;
            }
        }
    }

    /// Kill all running executions and clear state.
    pub fn destroy(&mut self) {
        for (_, state_arc) in self.executions.iter() {
            if let Ok(mut state) = state_arc.lock() {
                if state.status == ShellExecutionStatus::Running {
                    state.status = ShellExecutionStatus::Killed;
                    state.finished_at = Some(continuum_base::time::EpochMs::now().millis());
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
        let exec_state = self.get_execution_state(execution_id)?;
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
        let exec_state = self.get_execution_state(execution_id)?;

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
pub(super) fn collect_and_classify(state: &mut ExecutionState) -> Vec<ClassifiedLine> {
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
