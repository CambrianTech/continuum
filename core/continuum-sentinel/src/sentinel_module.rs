use super::*;

impl SentinelModule {
    /// Graceful shutdown — like iOS willTerminate / Android onDestroy.
    ///
    /// 1. Broadcast "system:shutdown" event so TypeScript can save persona state
    /// 2. Send cancel to all running sentinels (they'll save checkpoints if they can)
    /// 3. SIGTERM all process groups (graceful — lets training save checkpoints)
    /// 4. Clean up PID files
    ///
    /// Non-blocking: doesn't wait for processes to exit (caller sleeps 2s then exits).
    pub fn shutdown_all(&self) {
        let log = continuum_kernel::runtime::logger("sentinel");
        let mut killed = 0;

        // Phase 1: Broadcast shutdown event so TS side can save persona state, flush logs
        if let Some(ref bus) = *self.bus.read() {
            bus.publish_async_only(
                "system:shutdown",
                json!({
                    "reason": "server_shutdown",
                    "gracePeriodMs": 2000,
                }),
            );
            log.info("Broadcast system:shutdown — TS side has 2s to save state");
        }

        // Phase 2: Cancel running sentinels via channel (triggers checkpoint save in training)
        for mut entry in self.sentinels.iter_mut() {
            if entry.handle.status == SentinelStatus::Running {
                if let Some(tx) = entry.cancel_tx.take() {
                    let _ = tx.try_send(());
                }
                entry.handle.status = SentinelStatus::Cancelled;
                entry.handle.error = Some("Server shutdown".to_string());
                killed += 1;
            }
        }

        // Phase 3: SIGTERM all process groups (kills wrapper → micromamba → python tree)
        let logs_dir = self.logs_base_dir.read().clone();
        if let Ok(entries) = std::fs::read_dir(&logs_dir) {
            for entry in entries.flatten() {
                let pid_path = entry.path().join("pid");
                if let Ok(pid_str) = std::fs::read_to_string(&pid_path) {
                    if let Ok(pid) = pid_str.trim().parse::<i32>() {
                        // Graceful process-group/tree kill — one cross-platform
                        // definition lives in executor.rs (Unix: kill(-pgid,
                        // SIGTERM); Windows: taskkill /T).
                        executor::kill_process_group(Some(pid as u32));
                        std::fs::remove_file(&pid_path).ok();
                        killed += 1;
                    }
                }
            }
        }

        if killed > 0 {
            log.info(&format!(
                "Shutdown: signalled {killed} sentinel process groups"
            ));
        }
    }

    /// Resume a pipeline from a durable checkpoint
    pub(super) async fn resume_from_checkpoint(&self, params: Value) -> Result<CommandResult, String> {
        let p = Params::new(&params);
        let handle_id = p.str("handle")?;
        let log = continuum_kernel::runtime::logger("sentinel");

        let cp = checkpoint::load_checkpoint(handle_id)?
            .ok_or_else(|| format!("No checkpoint found for handle: {handle_id}"))?;

        match cp.status {
            PipelineStatus::Interrupted
            | PipelineStatus::Paused
            | PipelineStatus::BudgetExhausted => {}
            other => {
                return Err(format!(
                    "Cannot resume pipeline in status {:?} — only Interrupted, Paused, or BudgetExhausted",
                    other
                ));
            }
        }

        log.info(&format!(
            "Resuming pipeline '{}' from step {} (handle={handle_id})",
            cp.pipeline_name.as_deref().unwrap_or("unnamed"),
            cp.step_index
        ));

        // Update checkpoint to Running
        let mut updated_cp = cp.clone();
        updated_cp.status = PipelineStatus::Running;
        updated_cp.last_checkpoint_at = chrono::Utc::now().to_rfc3339();
        checkpoint::save_checkpoint(handle_id, &updated_cp)?;

        let bus = self.bus.read().clone();
        let registry = self.registry.read().clone();
        let logs_base_dir = self.logs_base_dir.read().clone();
        let handle_id_owned = handle_id.to_string();

        // Create sentinel handle for the resumed pipeline
        let (cancel_tx, _cancel_rx) = mpsc::channel(1);
        let handle = SentinelHandle {
            id: handle_id_owned.clone(),
            sentinel_type: "pipeline".to_string(),
            status: SentinelStatus::Running,
            progress: ((cp.step_index as f64 / cp.pipeline.steps.len() as f64) * 100.0) as u8,
            start_time: continuum_base::time::EpochMs::now().millis(),
            end_time: None,
            exit_code: None,
            error: None,
            working_dir: cp.working_dir.clone(),
            logs_dir: logs_base_dir
                .join(&handle_id_owned)
                .to_string_lossy()
                .to_string(),
        };

        let (completion_tx, completion_rx) = tokio::sync::watch::channel(false);
        self.sentinels.insert(
            handle_id_owned.clone(),
            RunningSentinel {
                handle: handle.clone(),
                cancel_tx: Some(cancel_tx),
                escalation: cp.escalation.clone(),
                completion_tx: Some(completion_tx),
                completion_rx,
            },
        );

        let sentinels = Arc::clone(&self.sentinels);
        let escalation_clone = cp.escalation.clone();
        let executor_for_task = self.executor.cloned();

        tokio::spawn(async move {
            let log = continuum_kernel::runtime::logger("sentinel");
            let registry = match registry {
                Some(r) => r,
                None => {
                    log.error(&format!("[{handle_id_owned}] Cannot resume: no registry"));
                    return;
                }
            };

            let steps_log_path = logs_base_dir.join(&handle_id_owned).join("steps.jsonl");
            let pipeline_ctx = PipelineContext {
                handle_id: &handle_id_owned,
                registry: &registry,
                bus: bus.as_ref(),
                steps_log_path: Some(&steps_log_path),
                executor: executor_for_task.as_ref(),
            };

            let mut ctx = ExecutionContext {
                step_results: cp.step_results.clone(),
                inputs: cp.pipeline.inputs.clone(),
                working_dir: PathBuf::from(&cp.working_dir),
                named_outputs: std::collections::HashMap::new(),
            };

            let mut budget = cp.budget_consumed.clone();
            let limits = cp.budget_limits.clone();
            let start_time = std::time::Instant::now();
            let mut failed = false;
            let mut error_msg: Option<String> = None;

            // Resume from checkpoint step_index
            for i in cp.step_index..cp.pipeline.steps.len() {
                let step = &cp.pipeline.steps[i];
                let step_type = step_type_name(step);

                log.info(&format!(
                    "[{handle_id_owned}] Resuming step {}/{}: {step_type}",
                    i + 1,
                    cp.pipeline.steps.len()
                ));

                match steps::execute_step(step, i, &mut ctx, &pipeline_ctx).await {
                    Ok(result) => {
                        if !result.success {
                            failed = true;
                            error_msg = result.error.clone();
                        }
                        ctx.step_results.push(result);

                        // Update budget and checkpoint after each step
                        budget.elapsed_secs =
                            start_time.elapsed().as_secs() + cp.budget_consumed.elapsed_secs;

                        let mut updated_cp = PipelineCheckpoint {
                            sentinel_handle: handle_id_owned.clone(),
                            pipeline_name: cp.pipeline_name.clone(),
                            step_index: i + 1,
                            step_results: ctx.step_results.clone(),
                            budget_consumed: budget.clone(),
                            budget_limits: limits.clone(),
                            started_at: cp.started_at.clone(),
                            last_checkpoint_at: chrono::Utc::now().to_rfc3339(),
                            status: if failed {
                                PipelineStatus::Failed
                            } else {
                                PipelineStatus::Running
                            },
                            pipeline: cp.pipeline.clone(),
                            working_dir: cp.working_dir.clone(),
                            escalation: cp.escalation.clone(),
                        };

                        // Check budget limits
                        if let Some(max_secs) = limits.max_time_secs {
                            if budget.elapsed_secs >= max_secs {
                                updated_cp.status = PipelineStatus::BudgetExhausted;
                                let _ = checkpoint::save_checkpoint(&handle_id_owned, &updated_cp);
                                log.warn(&format!(
                                    "[{handle_id_owned}] Budget exhausted: time limit {max_secs}s"
                                ));
                                error_msg =
                                    Some(format!("Budget exhausted: time limit {max_secs}s"));
                                failed = true;
                            }
                        }
                        if let Some(max_iters) = limits.max_iterations {
                            if budget.iterations >= max_iters {
                                updated_cp.status = PipelineStatus::BudgetExhausted;
                                let _ = checkpoint::save_checkpoint(&handle_id_owned, &updated_cp);
                                log.warn(&format!(
                                    "[{handle_id_owned}] Budget exhausted: iteration limit {max_iters}"
                                ));
                                error_msg =
                                    Some(format!("Budget exhausted: iteration limit {max_iters}"));
                                failed = true;
                            }
                        }

                        let _ = checkpoint::save_checkpoint(&handle_id_owned, &updated_cp);

                        if failed {
                            break;
                        }
                    }
                    Err(e) => {
                        log.error(&format!("[{handle_id_owned}] Step {i} error: {e}"));
                        failed = true;
                        error_msg = Some(e.clone());
                        ctx.step_results.push(StepResult {
                            step_index: i,
                            step_type: step_type.to_string(),
                            success: false,
                            duration_ms: 0,
                            output: None,
                            error: Some(e),
                            exit_code: None,
                            data: Value::Null,
                        });
                        break;
                    }
                }
            }

            // Final checkpoint
            let final_status = if failed {
                PipelineStatus::Failed
            } else {
                PipelineStatus::Completed
            };
            let final_cp = PipelineCheckpoint {
                sentinel_handle: handle_id_owned.clone(),
                pipeline_name: cp.pipeline_name.clone(),
                step_index: cp.pipeline.steps.len(),
                step_results: ctx.step_results.clone(),
                budget_consumed: budget,
                budget_limits: limits,
                started_at: cp.started_at.clone(),
                last_checkpoint_at: chrono::Utc::now().to_rfc3339(),
                status: final_status,
                pipeline: cp.pipeline.clone(),
                working_dir: cp.working_dir.clone(),
                escalation: cp.escalation.clone(),
            };
            let _ = checkpoint::save_checkpoint(&handle_id_owned, &final_cp);

            // Update sentinel handle status
            if let Some(mut entry) = sentinels.get_mut(&handle_id_owned) {
                entry.handle.status = if failed {
                    SentinelStatus::Failed
                } else {
                    SentinelStatus::Completed
                };
                entry.handle.exit_code = Some(if failed { 1 } else { 0 });
                entry.handle.end_time = Some(
                    continuum_base::time::EpochMs::now().millis(),
                );
                if let Some(ref err) = error_msg {
                    entry.handle.error = Some(err.clone());
                }
                if let Some(tx) = entry.completion_tx.take() {
                    let _ = tx.send(true);
                }
            }

            // Emit completion
            if let Some(ref bus) = bus {
                bus.publish_async_only(
                    "sentinel:pipeline:complete",
                    json!({
                        "handle": handle_id_owned,
                        "success": !failed,
                        "resumed": true,
                    }),
                );
            }

            // Substrate-native escalation dispatch (task #225). Resumed
            // pipelines lose track of duration_ms across the restart
            // boundary — pass `None` so the dispatcher renders the
            // memory content as "unknown" duration rather than fabricating
            // a misleading interval.
            if let Some(ref esc) = escalation_clone {
                if let Some(executor) = executor_for_task.as_ref() {
                    let terminal = if failed {
                        escalation::SentinelTerminalStatus::Failed
                    } else {
                        escalation::SentinelTerminalStatus::Completed
                    };
                    escalation::dispatch(
                        executor,
                        escalation::SentinelEscalationEvent {
                            handle: handle_id_owned.clone(),
                            status: terminal,
                            duration_ms: None,
                            error: None,
                            escalation: esc.clone(),
                        },
                    )
                    .await;
                }
            }
        });

        Ok(CommandResult::Json(json!({
            "handle": handle_id,
            "status": "running",
            "resumed": true,
            "resumeFromStep": cp.step_index,
        })))
    }

    /// List all durable checkpoints
    pub(super) async fn list_checkpoints_command(&self, _params: Value) -> Result<CommandResult, String> {
        let checkpoints = checkpoint::list_checkpoints()?;
        Ok(CommandResult::Json(json!({
            "checkpoints": checkpoints,
            "total": checkpoints.len(),
        })))
    }

    /// Extend budget limits for a running or paused pipeline
    pub(super) async fn extend_budget(&self, params: Value) -> Result<CommandResult, String> {
        let p = Params::new(&params);
        let handle_id = p.str("handle")?;

        let mut cp = checkpoint::load_checkpoint(handle_id)?
            .ok_or_else(|| format!("No checkpoint found for handle: {handle_id}"))?;

        // Merge new limits (only override fields that are provided)
        if let Some(v) = p.f64_opt("maxTimeSecs") {
            cp.budget_limits.max_time_secs = Some(v as u64);
        }
        if let Some(v) = p.f64_opt("maxCostUsd") {
            cp.budget_limits.max_cost_usd = Some(v);
        }
        if let Some(v) = p.f64_opt("maxTokens") {
            cp.budget_limits.max_tokens = Some(v as u64);
        }
        if let Some(v) = p.f64_opt("maxIterations") {
            cp.budget_limits.max_iterations = Some(v as u32);
        }

        cp.last_checkpoint_at = chrono::Utc::now().to_rfc3339();
        checkpoint::save_checkpoint(handle_id, &cp)?;

        Ok(CommandResult::Json(json!({
            "handle": handle_id,
            "budgetLimits": cp.budget_limits,
        })))
    }

    /// Approve or reject a pending approval step
    pub(super) async fn approve_command(&self, params: Value) -> Result<CommandResult, String> {
        let p = Params::new(&params);
        let handle_id = p.str("handle")?;
        let approved = p.bool_or("approved", true);
        let reason = p.str_opt("reason").map(|s| s.to_string());
        let approver_id = p.str_opt("approverId").map(|s| s.to_string());

        // Find and resolve the pending approval
        if let Some((_, tx)) = steps::approve::PENDING_APPROVALS.remove(handle_id) {
            let decision = steps::approve::ApprovalDecision {
                approved,
                reason: reason.clone(),
                approver_id: approver_id.clone(),
            };
            tx.send(decision).map_err(|_| {
                format!("Failed to send approval decision — pipeline may have been cancelled")
            })?;

            Ok(CommandResult::Json(json!({
                "handle": handle_id,
                "approved": approved,
                "reason": reason,
                "approverId": approver_id,
            })))
        } else {
            Err(format!("No pending approval found for handle: {handle_id}"))
        }
    }

}

impl Default for SentinelModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for SentinelModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "sentinel",
            priority: ModulePriority::Normal,
            command_prefixes: &["sentinel/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 8,
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        let log = continuum_kernel::runtime::logger("sentinel");

        *self.bus.write() = Some(Arc::clone(&ctx.bus));
        *self.registry.write() = Some(Arc::clone(&ctx.registry));

        // Scan for orphaned pipelines (were Running when process died)
        // Mark as Interrupted, emit events, and AUTO-RESUME.
        // Training runs for days/weeks — a restart should NOT kill it.
        match checkpoint::recover_interrupted() {
            Ok(interrupted) => {
                if !interrupted.is_empty() {
                    log.info(&format!(
                        "Found {} interrupted pipeline(s) — auto-resuming: {:?}",
                        interrupted.len(),
                        interrupted
                    ));
                    // Emit events for monitoring
                    if let Some(ref bus) = *self.bus.read() {
                        for handle in &interrupted {
                            bus.publish_async_only(
                                "sentinel:pipeline:interrupted",
                                json!({
                                    "handle": handle,
                                    "message": "Pipeline interrupted by restart — auto-resuming",
                                }),
                            );
                        }
                    }
                    // Auto-resume each interrupted pipeline after a brief delay
                    // (let the rest of the system initialize first)
                    let handles_to_resume = interrupted.clone();
                    let registry_clone = self.registry.read().clone();
                    let bus_clone = self.bus.read().clone();
                    let logs_dir = self.logs_base_dir.read().clone();
                    let sentinels = Arc::clone(&self.sentinels);
                    let executor_for_resume = self.executor.cloned();
                    tokio::spawn(async move {
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        let log = continuum_kernel::runtime::logger("sentinel");
                        for handle_id in handles_to_resume {
                            log.info(&format!(
                                "[{handle_id}] Auto-resuming interrupted pipeline..."
                            ));
                            match checkpoint::load_checkpoint(&handle_id) {
                                Ok(Some(mut cp)) => {
                                    cp.status = PipelineStatus::Running;
                                    let _ = checkpoint::save_checkpoint(&handle_id, &cp);

                                    let pipeline = cp.pipeline.clone();
                                    let working_dir = std::path::PathBuf::from(&cp.working_dir);
                                    let step_index = cp.step_index;

                                    // Register as running sentinel so sentinel/status works
                                    let (cancel_tx, _cancel_rx) = tokio::sync::mpsc::channel(1);
                                    let (completion_tx, completion_rx) =
                                        tokio::sync::watch::channel(false);
                                    let now = continuum_base::time::EpochMs::now().millis();
                                    let handle = SentinelHandle {
                                        id: handle_id.clone(),
                                        sentinel_type: "pipeline".to_string(),
                                        status: SentinelStatus::Running,
                                        progress: 0,
                                        start_time: now,
                                        end_time: None,
                                        exit_code: None,
                                        error: None,
                                        logs_dir: logs_dir.to_string_lossy().to_string(),
                                        working_dir: working_dir.to_string_lossy().to_string(),
                                    };
                                    sentinels.insert(
                                        handle_id.clone(),
                                        RunningSentinel {
                                            handle: handle.clone(),
                                            cancel_tx: Some(cancel_tx),
                                            escalation: cp.escalation.clone(),
                                            completion_tx: Some(completion_tx),
                                            completion_rx,
                                        },
                                    );

                                    log.info(&format!(
                                        "[{handle_id}] Registered + resuming from step {step_index} of {}",
                                        pipeline.steps.len()
                                    ));
                                    let _ = executor::execute_pipeline(
                                        logs_dir.clone(),
                                        pipeline,
                                        handle_id.clone(),
                                        working_dir,
                                        bus_clone.clone(),
                                        registry_clone.clone(),
                                        executor_for_resume.clone(),
                                    )
                                    .await;

                                    // Mark complete
                                    if let Some(mut entry) = sentinels.get_mut(&handle_id) {
                                        entry.handle.status = SentinelStatus::Completed;
                                        if let Some(tx) = entry.completion_tx.take() {
                                            let _ = tx.send(true);
                                        }
                                    }
                                }
                                Ok(None) => {
                                    log.warn(&format!(
                                        "[{handle_id}] No checkpoint found — cannot resume"
                                    ));
                                }
                                Err(e) => {
                                    log.warn(&format!(
                                        "[{handle_id}] Failed to load checkpoint: {e}"
                                    ));
                                }
                            }
                        }
                    });
                }
            }
            Err(e) => {
                log.warn(&format!("Failed to scan for interrupted checkpoints: {e}"));
            }
        }

        log.info("SentinelModule initialized with pipeline + checkpoint support");
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        let logs_base_dir = self.logs_base_dir.read().clone();

        match command {
            "sentinel/execute" | "sentinel/run" => self.run_sentinel(params).await,
            "sentinel/await" => self.await_sentinel(params).await,
            "sentinel/status" => self.get_status(params).await,
            "sentinel/list" => self.list_handles(params).await,
            "sentinel/cancel" => self.cancel_sentinel(params).await,
            "sentinel/pipeline" => self.execute_pipeline_command(params).await,
            "sentinel/resume" => self.resume_from_checkpoint(params).await,
            "sentinel/list-checkpoints" => self.list_checkpoints_command(params).await,
            "sentinel/extend-budget" => self.extend_budget(params).await,
            "sentinel/approve" => self.approve_command(params).await,
            "sentinel/logs/list" => logs::list_logs(&logs_base_dir, params).await,
            "sentinel/logs/read" => logs::read_log(&logs_base_dir, params).await,
            "sentinel/logs/tail" => logs::tail_log(&logs_base_dir, params).await,

            _ => Err(format!("Unknown sentinel command: {command}")),
        }
    }

    fn install_executor(&self, executor: Arc<continuum_kernel::runtime::CommandExecutor>) {
        self.executor.install(executor);
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
