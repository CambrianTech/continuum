use super::*;

/// Global sentinel module reference for shutdown from signal handlers.
/// Set during server startup, read during SIGTERM/SIGINT.
pub(super) static GLOBAL_SENTINEL: std::sync::OnceLock<Arc<SentinelModule>> = std::sync::OnceLock::new();

/// Register the sentinel module globally so signal handlers can shut it down.
pub fn register_for_shutdown(module: Arc<SentinelModule>) {
    GLOBAL_SENTINEL.set(module).ok();
}

/// Shutdown all running sentinels. Safe to call from signal handlers.
pub fn shutdown_all_sentinels() {
    if let Some(module) = GLOBAL_SENTINEL.get() {
        module.shutdown_all();
    }
}

/// Sentinel Module - manages concurrent sentinel execution and pipeline interpretation
pub struct SentinelModule {
    /// Active sentinels by handle ID
    pub(super) sentinels: Arc<DashMap<String, RunningSentinel>>,
    /// Base directory for sentinel logs (.continuum/jtag/logs/system/sentinels)
    pub(super) logs_base_dir: RwLock<PathBuf>,
    /// Maximum concurrent sentinels
    pub(super) max_concurrent: usize,
    /// Message bus for event emission (set during initialize)
    pub(super) bus: RwLock<Option<Arc<MessageBus>>>,
    /// Module registry for inter-module calls (set during initialize)
    pub(super) registry: RwLock<Option<Arc<ModuleRegistry>>>,
    /// Substrate-wide command executor — installed by `start_server`
    /// after the executor is built (task #224).
    pub(super) executor: LateBound<continuum_kernel::runtime::CommandExecutor>,
}

impl SentinelModule {
    pub fn new() -> Self {
        Self {
            sentinels: Arc::new(DashMap::new()),
            logs_base_dir: RwLock::new({
                let home = continuum_kernel::paths::continuum_home().expect("Failed to resolve home directory"); // JUSTIFIED: module constructed once at boot; no HOME means no log storage, boot-fatal by design
                home.join("jtag")
                    .join("logs")
                    .join("system")
                    .join("sentinels")
            }),
            max_concurrent: 6,
            bus: RwLock::new(None),
            registry: RwLock::new(None),
            executor: LateBound::new("sentinel::executor"),
        }
    }

    /// Borrow the installed executor or return an error string. Sentinel
    /// call sites generally tolerate a `None` executor (the memory check
    /// is a guard, not load-bearing); this helper makes the intent
    /// explicit at the call site.
    pub(super) fn executor_or_err(&self) -> Result<Arc<continuum_kernel::runtime::CommandExecutor>, String> {
        self.executor
            .cloned()
            .ok_or_else(|| "sentinel: CommandExecutor not yet installed".to_string())
    }

    /// Generate a unique handle ID
    pub(super) fn generate_handle_id() -> String {
        uuid::Uuid::new_v4().to_string()[..8].to_string()
    }

    /// Get logs directory for a handle
    pub(super) fn logs_dir(&self, handle: &str) -> PathBuf {
        self.logs_base_dir.read().join(handle)
    }

    /// Minimum available system memory (bytes) required to start a new sentinel.
    /// Below this threshold, the system refuses new sentinels to prevent OOM crashes.
    /// 2GB headroom prevents macOS from running out of application memory when
    /// Candle model loads, LoRA training, and LLM inference stack up.
    pub(super) const MIN_AVAILABLE_MEMORY_BYTES: u64 = 2 * 1024 * 1024 * 1024; // 2 GB

    /// Run a sentinel (async execution) — handles both shell commands and pipelines
    pub(super) async fn run_sentinel(&self, params: Value) -> Result<CommandResult, String> {
        use continuum_kernel::runtime;
        let log = runtime::logger("sentinel");

        // Check concurrent limit
        let active_count = self
            .sentinels
            .iter()
            .filter(|s| s.handle.status == SentinelStatus::Running)
            .count();
        if active_count >= self.max_concurrent {
            return Err(format!(
                "Maximum concurrent sentinels ({}) reached. Wait for completion or cancel existing.",
                self.max_concurrent
            ));
        }

        // Check system memory pressure before starting a new sentinel.
        // Candle model loads + LoRA training can easily exhaust RAM if unchecked.
        if let Ok(executor) = self.executor_or_err() {
            if let Ok(mem) = executor.execute_json("system/memory", Value::Null).await {
                let available = mem
                    .get("available_bytes")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(u64::MAX);
                if available < Self::MIN_AVAILABLE_MEMORY_BYTES {
                    let available_gb = available as f64 / (1024.0 * 1024.0 * 1024.0);
                    let threshold_gb =
                        Self::MIN_AVAILABLE_MEMORY_BYTES as f64 / (1024.0 * 1024.0 * 1024.0);
                    return Err(format!(
                        "Insufficient system memory: {:.1}GB available, {:.1}GB required. \
                         Cancel existing sentinels or wait for completion.",
                        available_gb, threshold_gb
                    ));
                }
            }
        }

        // Parse params
        let p = Params::new(&params);

        let sentinel_type = p.str_or("type", "build").to_string();
        let working_dir = p
            .str_opt("workingDir")
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());
        let command = p.str_or("cmd", "npm").to_string();
        let args: Vec<String> = p
            .json_opt("args")
            .unwrap_or_else(|| vec!["run".to_string(), "build".to_string()]);
        // timeout=0 means no timeout (Academy sessions can run for hours/days).
        // Default 600s (10 min) for ad-hoc sentinels.
        let raw_timeout = p.u64_or("timeout", 600);
        let timeout_secs = if raw_timeout == 0 {
            u64::MAX / 2
        } else {
            raw_timeout
        };
        let env: HashMap<String, String> = p.json_or("env");

        // Check if this is a pipeline execution
        let pipeline_json = env.get("PIPELINE_JSON").cloned();

        let pipeline: Option<Pipeline> =
            if let Some(ref json_str) = pipeline_json.filter(|_| sentinel_type == "pipeline") {
                match serde_json::from_str::<Pipeline>(json_str) {
                    Ok(p) => Some(p),
                    Err(e) => {
                        return Err(format!("Failed to parse PIPELINE_JSON: {e}"));
                    }
                }
            } else {
                None
            };

        // Generate handle
        let handle_id = Self::generate_handle_id();
        let logs_dir = self.logs_dir(&handle_id);

        let (cancel_tx, cancel_rx) = mpsc::channel(1);
        let handle = SentinelHandle {
            id: handle_id.clone(),
            sentinel_type: sentinel_type.clone(),
            status: SentinelStatus::Running,
            progress: 0,
            start_time: continuum_base::time::EpochMs::now().millis(),
            end_time: None,
            exit_code: None,
            error: None,
            working_dir: working_dir.to_string_lossy().to_string(),
            logs_dir: logs_dir.to_string_lossy().to_string(),
        };

        // Parse escalation metadata (if caller wants persona inbox routing).
        // `escalationRules` is now typed `Vec<EscalationRule>` (task #225 —
        // dropped the `Value` pass-through when the substrate took ownership
        // of the schema). Malformed rules are dropped with a warning; the
        // dispatcher falls back to `default_escalation_rules()`.
        let escalation =
            if p.str_opt("parentPersonaId").is_some() || p.str_opt("entityId").is_some() {
                let parsed_rules = p.json_opt("escalationRules").and_then(|raw| {
                    serde_json::from_value::<Vec<types::EscalationRule>>(raw).map_or_else(
                        |e| {
                            continuum_kernel::runtime::logger("sentinel").warn(&format!(
                                "escalationRules param failed typed parse ({e}); \
                                 dispatcher will use defaults"
                            ));
                            None
                        },
                        Some,
                    )
                });
                Some(SentinelEscalation {
                    parent_persona_id: p.str_opt("parentPersonaId").map(|s| s.to_string()),
                    entity_id: p.str_opt("entityId").map(|s| s.to_string()),
                    sentinel_name: p.str_or("sentinelName", "unnamed").to_string(),
                    escalation_rules: parsed_rules,
                })
            } else {
                None
            };

        let (completion_tx, completion_rx) = tokio::sync::watch::channel(false);
        self.sentinels.insert(
            handle_id.clone(),
            RunningSentinel {
                handle: handle.clone(),
                cancel_tx: Some(cancel_tx),
                escalation: escalation.clone(),
                completion_tx: Some(completion_tx),
                completion_rx,
            },
        );

        let mode_str = if pipeline.is_some() {
            "pipeline"
        } else {
            "shell"
        };
        log.info(&format!(
            "Starting sentinel {handle_id} (type={sentinel_type}, mode={mode_str}, cmd={command} {args:?})"
        ));

        // Clone fields for the spawned task
        let sentinels = Arc::clone(&self.sentinels);
        let handle_id_clone = handle_id.clone();
        let working_dir_clone = working_dir.clone();
        let sentinel_type_clone = sentinel_type.clone();
        let logs_base_dir = self.logs_base_dir.read().clone();
        let bus = self.bus.read().clone();
        let registry = self.registry.read().clone();
        let escalation_clone = escalation;
        let executor_for_task = self.executor.cloned();

        tokio::spawn(async move {
            let log = runtime::logger("sentinel");

            // Emit start event
            if let Some(ref bus) = bus {
                bus.publish_async_only(
                    &format!("sentinel:{handle_id_clone}:status"),
                    json!({
                        "handle": handle_id_clone,
                        "type": sentinel_type_clone,
                        "status": "running",
                        "phase": "starting",
                        "mode": if pipeline.is_some() { "pipeline" } else { "shell" },
                    }),
                );
            }

            // Execute based on type.
            // timeout_secs == 0 means no timeout (run indefinitely — for long training).
            let result: Result<(i32, String), String> = if let Some(pipeline) = pipeline {
                log.info(&format!(
                    "[{handle_id_clone}] Executing pipeline with {} steps (timeout: {})",
                    pipeline.steps.len(),
                    if timeout_secs == 0 {
                        "none".to_string()
                    } else {
                        format!("{timeout_secs}s")
                    }
                ));

                let future = executor::execute_pipeline(
                    logs_base_dir.clone(),
                    pipeline,
                    handle_id_clone.clone(),
                    working_dir_clone.clone(),
                    bus.clone(),
                    registry.clone(),
                    executor_for_task.clone(),
                );

                if timeout_secs == 0 {
                    future.await
                } else {
                    tokio::time::timeout(Duration::from_secs(timeout_secs), future)
                        .await
                        .map_err(|_| format!("Pipeline timeout after {timeout_secs}s"))
                        .and_then(|r| r)
                }
            } else {
                let future = executor::execute_isolated(
                    executor::IsolatedProcessConfig {
                        logs_base_dir,
                        handle_id: handle_id_clone.clone(),
                        command,
                        args,
                        working_dir: working_dir_clone,
                        env,
                    },
                    cancel_rx,
                    bus.clone(),
                );

                if timeout_secs == 0 {
                    future.await
                } else {
                    tokio::time::timeout(Duration::from_secs(timeout_secs), future)
                        .await
                        .map_err(|_| format!("Timeout after {timeout_secs}s"))
                        .and_then(|r| r)
                }
            };

            // Update handle status
            if let Some(mut entry) = sentinels.get_mut(&handle_id_clone) {
                let now = continuum_base::time::EpochMs::now().millis();

                let (final_status, error_msg) = match result {
                    Ok((exit_code, _output)) => {
                        entry.handle.status = if exit_code == 0 {
                            SentinelStatus::Completed
                        } else {
                            SentinelStatus::Failed
                        };
                        entry.handle.exit_code = Some(exit_code);
                        entry.handle.progress = 100;
                        log.info(&format!(
                            "Sentinel {handle_id_clone} completed with exit code {exit_code}"
                        ));
                        (
                            if exit_code == 0 {
                                "completed"
                            } else {
                                "failed"
                            },
                            None,
                        )
                    }
                    Err(e) => {
                        entry.handle.status = if e == "Cancelled" {
                            SentinelStatus::Cancelled
                        } else {
                            SentinelStatus::Failed
                        };
                        entry.handle.error = Some(e.clone());
                        log.error(&format!("Sentinel {handle_id_clone} failed: {e}"));
                        (
                            if e == "Cancelled" {
                                "cancelled"
                            } else {
                                "failed"
                            },
                            Some(e),
                        )
                    }
                };
                entry.handle.end_time = Some(now);
                entry.cancel_tx = None;

                let duration_ms = entry
                    .handle
                    .end_time
                    .unwrap_or(0)
                    .saturating_sub(entry.handle.start_time);

                if let Some(ref bus) = bus {
                    let mut payload = json!({
                        "handle": handle_id_clone,
                        "type": sentinel_type_clone,
                        "status": final_status,
                        "exitCode": entry.handle.exit_code,
                    });
                    if let Some(ref err) = error_msg {
                        payload["error"] = json!(err);
                    }
                    bus.publish_async_only(&format!("sentinel:{handle_id_clone}:status"), payload);
                    bus.publish_async_only(
                        "sentinel:complete",
                        json!({
                            "handle": handle_id_clone,
                            "type": sentinel_type_clone,
                            "success": final_status == "completed",
                        }),
                    );
                }

                // Signal completion to any awaiting callers (replaces TS polling loop)
                if let Some(tx) = entry.completion_tx.take() {
                    let _ = tx.send(true);
                }

                // Dispatch the substrate-native escalation pipeline
                // (task #225 — replaces the deleted TS round-trip).
                // The fire-and-forget contract is preserved: dispatch
                // logs errors per stage; sentinel cleanup never blocks.
                if let Some(ref esc) = escalation_clone {
                    if let Some(executor) = executor_for_task.as_ref() {
                        let terminal = match final_status {
                            "completed" => escalation::SentinelTerminalStatus::Completed,
                            "cancelled" => escalation::SentinelTerminalStatus::Cancelled,
                            _ => escalation::SentinelTerminalStatus::Failed,
                        };
                        escalation::dispatch(
                            executor,
                            escalation::SentinelEscalationEvent {
                                handle: handle_id_clone.clone(),
                                status: terminal,
                                duration_ms: Some(duration_ms),
                                error: error_msg.clone(),
                                escalation: esc.clone(),
                            },
                        )
                        .await;
                    }
                }
            }
        });

        Ok(CommandResult::Json(json!({
            "handle": handle_id,
            "status": "running",
            "logsDir": logs_dir.to_string_lossy(),
        })))
    }

    /// Get sentinel status
    pub(super) async fn get_status(&self, params: Value) -> Result<CommandResult, String> {
        let p = Params::new(&params);
        let handle_id = p.str("handle")?;

        if let Some(entry) = self.sentinels.get(handle_id) {
            Ok(CommandResult::Json(json!({
                "handle": entry.handle,
            })))
        } else {
            Err(format!("Sentinel handle not found: {handle_id}"))
        }
    }

    /// List all sentinel handles
    pub(super) async fn list_handles(&self, _params: Value) -> Result<CommandResult, String> {
        let handles: Vec<SentinelHandle> = self
            .sentinels
            .iter()
            .map(|entry| entry.handle.clone())
            .collect();

        Ok(CommandResult::Json(json!({
            "handles": handles,
            "total": handles.len(),
        })))
    }

    /// Cancel a running sentinel
    pub(super) async fn cancel_sentinel(&self, params: Value) -> Result<CommandResult, String> {
        let p = Params::new(&params);
        let handle_id = p.str("handle")?;

        if let Some(mut entry) = self.sentinels.get_mut(handle_id) {
            if entry.handle.status == SentinelStatus::Running {
                if let Some(cancel_tx) = entry.cancel_tx.take() {
                    cancel_tx.send(()).await.ok();
                    entry.handle.status = SentinelStatus::Cancelled;
                    return Ok(CommandResult::Json(json!({
                        "handle": handle_id,
                        "status": "cancelled",
                    })));
                }
            }
            return Err(format!("Sentinel {handle_id} is not running"));
        }

        Err(format!("Sentinel handle not found: {handle_id}"))
    }

    /// Await sentinel completion — blocks until done, no polling.
    /// Uses tokio::sync::watch channel instead of a sleep/poll loop.
    pub(super) async fn await_sentinel(&self, params: Value) -> Result<CommandResult, String> {
        let p = Params::new(&params);
        let handle_id = p.str("handle")?;
        let raw_timeout = p.u64_or("timeout", 600);
        let timeout_secs = if raw_timeout == 0 {
            u64::MAX / 2
        } else {
            raw_timeout
        };

        // Clone the watch receiver while holding the DashMap ref briefly
        let mut rx = {
            let entry = self
                .sentinels
                .get(handle_id)
                .ok_or_else(|| format!("Sentinel handle not found: {handle_id}"))?;

            // Already done? Return immediately.
            if entry.handle.status != SentinelStatus::Running {
                return Ok(CommandResult::Json(json!({
                    "handle": entry.handle,
                })));
            }

            entry.completion_rx.clone()
        };

        // Await completion signal with timeout — zero polling
        let result =
            tokio::time::timeout(Duration::from_secs(timeout_secs), rx.wait_for(|done| *done))
                .await;

        match result {
            Ok(Ok(_)) => {
                // Completed — read final status
                if let Some(entry) = self.sentinels.get(handle_id) {
                    Ok(CommandResult::Json(json!({
                        "handle": entry.handle,
                    })))
                } else {
                    Err(format!(
                        "Sentinel {handle_id} completed but handle was cleaned up"
                    ))
                }
            }
            Ok(Err(_)) => {
                // Watch channel closed without sending — shouldn't happen
                Err(format!(
                    "Sentinel {handle_id} watch channel closed unexpectedly"
                ))
            }
            Err(_) => {
                // Timeout
                Err(format!(
                    "Await timeout after {timeout_secs}s for sentinel {handle_id}"
                ))
            }
        }
    }

    /// Execute a pipeline (direct, synchronous path — not spawned)
    pub(super) async fn execute_pipeline_command(&self, params: Value) -> Result<CommandResult, String> {
        let handle_id = Self::generate_handle_id();

        let p = Params::new(&params);
        let pipeline: Pipeline = p.json("pipeline").or_else(|_| {
            serde_json::from_value::<Pipeline>(params.clone())
                .map_err(|e| format!("Failed to parse pipeline: {e}"))
        })?;

        let logs_base_dir = self.logs_base_dir.read().clone();
        let bus = self.bus.read().clone();
        let registry = self.registry.read().clone();
        let executor_for_call = self.executor.cloned();

        let result = executor::execute_pipeline_direct(
            &logs_base_dir,
            &handle_id,
            pipeline,
            bus.as_ref(),
            registry.as_ref(),
            executor_for_call.as_ref(),
        )
        .await;

        Ok(CommandResult::Json(
            serde_json::to_value(&result).unwrap_or(json!({"error": "serialization failed"})),
        ))
    }
}
