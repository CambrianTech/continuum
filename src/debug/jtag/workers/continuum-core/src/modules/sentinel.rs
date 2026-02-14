//! Sentinel Module — Concurrent, fault-tolerant build/task execution with pipeline support
//!
//! Sentinels are autonomous agents that can run builds, tests, and other
//! long-running processes with proper isolation and logging.
//!
//! Key Design Principles:
//! - **Process Isolation**: Each sentinel runs in a child process (crash isolation)
//! - **Non-blocking**: Heavy processes (Xcode, cargo) don't block the runtime
//! - **Fault Tolerant**: One sentinel failure doesn't cascade to others
//! - **Concurrent**: Multiple sentinels can run in parallel
//! - **Observable**: All output streamed to logs in real-time
//! - **Event-driven**: Emits sentinel:{handle}:log events for real-time streaming
//! - **Pipeline Support**: Multi-step pipelines with LLM, conditions, loops
//!
//! Pipeline Execution:
//! - Shell steps: Execute child processes
//! - LLM steps: Call AIProviderModule directly (no IPC deadlock)
//! - Command steps: Route to any module via ModuleRegistry
//! - Conditions: Evaluate expressions against execution context
//! - Loops: Iterate with variable substitution

use async_trait::async_trait;
use dashmap::DashMap;
use futures::future::BoxFuture;
use futures::FutureExt;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use ts_rs::TS;
use regex::Regex;

use crate::runtime::{
    CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule,
    message_bus::MessageBus, ModuleRegistry,
};

/// Sentinel execution handle
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/sentinel/SentinelHandle.ts")]
#[serde(rename_all = "camelCase")]
pub struct SentinelHandle {
    pub id: String,
    pub sentinel_type: String,
    pub status: SentinelStatus,
    pub progress: u8,
    pub start_time: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end_time: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub working_dir: String,
    pub logs_dir: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/sentinel/SentinelStatus.ts")]
#[serde(rename_all = "lowercase")]
pub enum SentinelStatus {
    Running,
    Completed,
    Failed,
    Cancelled,
}

/// Log stream info
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/sentinel/LogStreamInfo.ts")]
#[serde(rename_all = "camelCase")]
pub struct LogStreamInfo {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub modified_at: String,
}

// ============================================================================
// PIPELINE TYPES — Multi-step execution with LLM, conditions, loops
// ============================================================================

/// A single step in a pipeline
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/sentinel/PipelineStep.ts")]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum PipelineStep {
    /// Execute a shell command
    Shell {
        cmd: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timeout_secs: Option<u64>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "workingDir")]
        working_dir: Option<String>,
    },

    /// LLM inference (calls AIProviderModule directly)
    Llm {
        prompt: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        model: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        provider: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "maxTokens")]
        max_tokens: Option<u32>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        temperature: Option<f32>,
        #[serde(default, skip_serializing_if = "Option::is_none", rename = "systemPrompt")]
        system_prompt: Option<String>,
    },

    /// Call any command via ModuleRegistry
    Command {
        command: String,
        #[serde(default)]
        #[ts(type = "Record<string, unknown>")]
        params: Value,
    },

    /// Conditional execution
    Condition {
        #[serde(rename = "if")]
        condition: String,
        #[serde(rename = "then")]
        then_steps: Vec<PipelineStep>,
        #[serde(default, rename = "else")]
        else_steps: Vec<PipelineStep>,
    },

    /// Loop with count
    Loop {
        count: usize,
        steps: Vec<PipelineStep>,
    },
}

/// A complete pipeline definition
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/sentinel/Pipeline.ts")]
#[serde(rename_all = "camelCase")]
pub struct Pipeline {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub steps: Vec<PipelineStep>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_secs: Option<u64>,
    #[serde(default)]
    #[ts(type = "Record<string, unknown>")]
    pub inputs: HashMap<String, Value>,
}

/// Result of a single step execution
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/sentinel/StepResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct StepResult {
    pub step_index: usize,
    pub step_type: String,
    pub success: bool,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_code: Option<i32>,
    /// Full result data for complex outputs
    #[serde(default, skip_serializing_if = "Value::is_null")]
    #[ts(type = "unknown")]
    pub data: Value,
}

/// Result of pipeline execution
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/sentinel/PipelineResult.ts")]
#[serde(rename_all = "camelCase")]
pub struct PipelineResult {
    pub handle: String,
    pub success: bool,
    pub total_duration_ms: u64,
    pub steps_completed: usize,
    pub steps_total: usize,
    pub step_results: Vec<StepResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Execution context for variable interpolation
#[derive(Debug, Clone, Default)]
struct ExecutionContext {
    /// Results from previous steps (by index)
    step_results: Vec<StepResult>,
    /// Pipeline inputs
    inputs: HashMap<String, Value>,
    /// Working directory
    working_dir: PathBuf,
}

/// Internal state for a running sentinel
struct RunningSentinel {
    handle: SentinelHandle,
    /// Channel to send cancellation signal
    cancel_tx: Option<mpsc::Sender<()>>,
}

/// Sentinel Module - manages concurrent sentinel execution and pipeline interpretation
pub struct SentinelModule {
    /// Active sentinels by handle ID
    sentinels: Arc<DashMap<String, RunningSentinel>>,
    /// Base directory for sentinel logs (.continuum/jtag/logs/system/sentinels)
    logs_base_dir: RwLock<PathBuf>,
    /// Maximum concurrent sentinels
    max_concurrent: usize,
    /// Message bus for event emission (set during initialize)
    bus: RwLock<Option<Arc<MessageBus>>>,
    /// Module registry for inter-module calls (set during initialize)
    registry: RwLock<Option<Arc<ModuleRegistry>>>,
}

impl SentinelModule {
    pub fn new() -> Self {
        Self {
            sentinels: Arc::new(DashMap::new()),
            logs_base_dir: RwLock::new(PathBuf::from(".continuum/jtag/logs/system/sentinels")),
            max_concurrent: 4, // Conservative default - can be tuned
            bus: RwLock::new(None),
            registry: RwLock::new(None),
        }
    }

    /// Generate a unique handle ID
    fn generate_handle_id() -> String {
        uuid::Uuid::new_v4().to_string()[..8].to_string()
    }

    /// Get logs directory for a handle (in .continuum/jtag/logs/system/sentinels/)
    fn logs_dir(&self, handle: &str) -> PathBuf {
        self.logs_base_dir.read().join(handle)
    }

    /// Run a sentinel (async execution) - handles both shell commands and pipelines
    async fn run_sentinel(&self, params: Value) -> Result<CommandResult, String> {
        use crate::runtime;
        let log = runtime::logger("sentinel");

        // Check concurrent limit
        let active_count = self.sentinels.iter().filter(|s| s.handle.status == SentinelStatus::Running).count();
        if active_count >= self.max_concurrent {
            return Err(format!(
                "Maximum concurrent sentinels ({}) reached. Wait for completion or cancel existing.",
                self.max_concurrent
            ));
        }

        // Parse params
        let sentinel_type = params.get("type")
            .and_then(|v| v.as_str())
            .unwrap_or("build")
            .to_string();

        let working_dir = params.get("workingDir")
            .and_then(|v| v.as_str())
            .map(PathBuf::from)
            .unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

        let command = params.get("cmd")
            .and_then(|v| v.as_str())
            .unwrap_or("npm")
            .to_string();

        let args: Vec<String> = params.get("args")
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_else(|| vec!["run".to_string(), "build".to_string()]);

        let timeout_secs = params.get("timeout")
            .and_then(|v| v.as_u64())
            .unwrap_or(600); // 10 min default

        let env: HashMap<String, String> = params.get("env")
            .and_then(|v| v.as_object())
            .map(|obj| {
                obj.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        // Check if this is a pipeline execution (PIPELINE_JSON in env)
        let pipeline_json = env.get("PIPELINE_JSON").cloned();
        let is_pipeline = sentinel_type == "pipeline" && pipeline_json.is_some();

        // Parse pipeline if present
        let pipeline: Option<Pipeline> = if is_pipeline {
            match serde_json::from_str::<Pipeline>(pipeline_json.as_ref().unwrap()) {
                Ok(p) => Some(p),
                Err(e) => {
                    return Err(format!("Failed to parse PIPELINE_JSON: {}", e));
                }
            }
        } else {
            None
        };

        // Generate handle
        let handle_id = Self::generate_handle_id();
        let logs_dir = self.logs_dir(&handle_id);

        // Create handle
        let (cancel_tx, cancel_rx) = mpsc::channel(1);
        let handle = SentinelHandle {
            id: handle_id.clone(),
            sentinel_type: sentinel_type.clone(),
            status: SentinelStatus::Running,
            progress: 0,
            start_time: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64,
            end_time: None,
            exit_code: None,
            error: None,
            working_dir: working_dir.to_string_lossy().to_string(),
            logs_dir: logs_dir.to_string_lossy().to_string(),
        };

        // Register sentinel
        self.sentinels.insert(handle_id.clone(), RunningSentinel {
            handle: handle.clone(),
            cancel_tx: Some(cancel_tx),
        });

        let mode_str = if is_pipeline { "pipeline" } else { "shell" };
        log.info(&format!(
            "Starting sentinel {} (type={}, mode={}, cmd={} {:?})",
            handle_id, sentinel_type, mode_str, command, args
        ));

        // Spawn execution task
        let sentinels = Arc::clone(&self.sentinels);
        let handle_id_clone = handle_id.clone();
        let working_dir_clone = working_dir.clone();
        let sentinel_type_clone = sentinel_type.clone();

        // Clone self fields needed for the task
        let logs_base_dir = self.logs_base_dir.read().clone();
        let bus = self.bus.read().clone();
        let registry = self.registry.read().clone();

        tokio::spawn(async move {
            let log = runtime::logger("sentinel");

            // Emit start event
            if let Some(ref bus) = bus {
                bus.publish_async_only(&format!("sentinel:{}:status", handle_id_clone), json!({
                    "handle": handle_id_clone,
                    "type": sentinel_type_clone,
                    "status": "running",
                    "phase": "starting",
                    "mode": if pipeline.is_some() { "pipeline" } else { "shell" },
                }));
            }

            // Execute based on type
            let result: Result<(i32, String), String> = if let Some(pipeline) = pipeline {
                // PIPELINE EXECUTION - multi-step with LLM, conditions, loops
                log.info(&format!("[{}] Executing pipeline with {} steps", handle_id_clone, pipeline.steps.len()));

                tokio::time::timeout(
                    Duration::from_secs(timeout_secs),
                    Self::execute_pipeline_static(
                        logs_base_dir.clone(),
                        pipeline,
                        handle_id_clone.clone(),
                        working_dir_clone.clone(),
                        bus.clone(),
                        registry.clone(),
                    ),
                )
                .await
                .map_err(|_| format!("Pipeline timeout after {}s", timeout_secs))
                .and_then(|r| r)
            } else {
                // SHELL EXECUTION - single command
                tokio::time::timeout(
                    Duration::from_secs(timeout_secs),
                    Self::execute_isolated_static(
                        logs_base_dir,
                        handle_id_clone.clone(),
                        command,
                        args,
                        working_dir_clone,
                        env,
                        cancel_rx,
                        bus.clone(),
                    ),
                )
                .await
                .map_err(|_| format!("Timeout after {}s", timeout_secs))
                .and_then(|r| r)
            };

            // Update handle status
            if let Some(mut entry) = sentinels.get_mut(&handle_id_clone) {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;

                let (final_status, error_msg) = match result {
                    Ok((exit_code, _output)) => {
                        entry.handle.status = if exit_code == 0 {
                            SentinelStatus::Completed
                        } else {
                            SentinelStatus::Failed
                        };
                        entry.handle.exit_code = Some(exit_code);
                        entry.handle.progress = 100;
                        log.info(&format!("Sentinel {} completed with exit code {}", handle_id_clone, exit_code));
                        (if exit_code == 0 { "completed" } else { "failed" }, None)
                    }
                    Err(e) => {
                        entry.handle.status = if e == "Cancelled" {
                            SentinelStatus::Cancelled
                        } else {
                            SentinelStatus::Failed
                        };
                        entry.handle.error = Some(e.clone());
                        log.error(&format!("Sentinel {} failed: {}", handle_id_clone, e));
                        (if e == "Cancelled" { "cancelled" } else { "failed" }, Some(e))
                    }
                };
                entry.handle.end_time = Some(now);
                entry.cancel_tx = None;

                // Emit completion event
                if let Some(ref bus) = bus {
                    let mut payload = json!({
                        "handle": handle_id_clone,
                        "type": sentinel_type_clone,
                        "status": final_status,
                        "exitCode": entry.handle.exit_code,
                    });
                    if let Some(err) = error_msg {
                        payload["error"] = json!(err);
                    }
                    bus.publish_async_only(&format!("sentinel:{}:status", handle_id_clone), payload);
                    bus.publish_async_only("sentinel:complete", json!({
                        "handle": handle_id_clone,
                        "type": sentinel_type_clone,
                        "success": final_status == "completed",
                    }));
                }
            }
        });

        Ok(CommandResult::Json(json!({
            "handle": handle_id,
            "status": "running",
            "logsDir": logs_dir.to_string_lossy(),
        })))
    }

    /// Static pipeline execution for use in spawned tasks
    async fn execute_pipeline_static(
        logs_base_dir: PathBuf,
        pipeline: Pipeline,
        handle_id: String,
        working_dir: PathBuf,
        bus: Option<Arc<MessageBus>>,
        registry: Option<Arc<ModuleRegistry>>,
    ) -> Result<(i32, String), String> {
        use crate::runtime;
        use tokio::io::AsyncWriteExt;
        let log = runtime::logger("sentinel");

        let registry = registry.ok_or("Pipeline execution requires module registry")?;
        let start_time = Instant::now();
        let pipeline_name = pipeline.name.as_deref().unwrap_or("unnamed");

        // Create logs directory for this pipeline
        let logs_dir = logs_base_dir.join(&handle_id);
        if let Err(e) = tokio::fs::create_dir_all(&logs_dir).await {
            log.warn(&format!("[{}] Failed to create logs dir: {}", handle_id, e));
        }
        let steps_log_path = logs_dir.join("steps.jsonl");

        log.info(&format!("[{}] Pipeline '{}' starting with {} steps",
            handle_id, pipeline_name, pipeline.steps.len()));

        // Create execution context
        let mut ctx = ExecutionContext {
            step_results: Vec::new(),
            inputs: pipeline.inputs.clone(),
            working_dir: pipeline.working_dir.clone().map(PathBuf::from).unwrap_or(working_dir),
        };

        // Execute steps
        let mut last_output = String::new();
        let mut failed = false;
        let mut error_msg: Option<String> = None;

        for (i, step) in pipeline.steps.iter().enumerate() {
            let step_type = step_type_name(step);
            log.info(&format!("[{}] Step {}/{}: {}", handle_id, i + 1, pipeline.steps.len(), step_type));

            // Emit step progress
            if let Some(ref bus) = bus {
                bus.publish_async_only(&format!("sentinel:{}:progress", handle_id), json!({
                    "handle": handle_id,
                    "step": i,
                    "totalSteps": pipeline.steps.len(),
                    "stepType": step_type,
                    "phase": "executing",
                }));
            }

            match Self::execute_step_static(step, i, &mut ctx, &handle_id, &registry, bus.as_ref()).await {
                Ok(result) => {
                    if result.success {
                        last_output = result.output.clone().unwrap_or_default();
                        log.info(&format!("[{}] Step {} succeeded", handle_id, i));
                    } else {
                        log.error(&format!("[{}] Step {} failed: {:?}", handle_id, i, result.error));
                        failed = true;
                        error_msg = result.error.clone();
                    }
                    // Write step result to steps.jsonl
                    if let Ok(json_line) = serde_json::to_string(&result) {
                        if let Ok(mut file) = tokio::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&steps_log_path)
                            .await
                        {
                            let _ = file.write_all(format!("{}\n", json_line).as_bytes()).await;
                        }
                    }
                    ctx.step_results.push(result);
                    if failed {
                        break;
                    }
                }
                Err(e) => {
                    log.error(&format!("[{}] Step {} error: {}", handle_id, i, e));
                    failed = true;
                    error_msg = Some(e.clone());
                    let error_result = StepResult {
                        step_index: i,
                        step_type: step_type.to_string(),
                        success: false,
                        duration_ms: 0,
                        output: None,
                        error: Some(e),
                        exit_code: None,
                        data: Value::Null,
                    };
                    // Write error result to steps.jsonl
                    if let Ok(json_line) = serde_json::to_string(&error_result) {
                        if let Ok(mut file) = tokio::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open(&steps_log_path)
                            .await
                        {
                            let _ = file.write_all(format!("{}\n", json_line).as_bytes()).await;
                        }
                    }
                    ctx.step_results.push(error_result);
                    break;
                }
            }
        }

        let total_duration_ms = start_time.elapsed().as_millis() as u64;

        // Emit pipeline completion
        if let Some(ref bus) = bus {
            bus.publish_async_only("sentinel:pipeline:complete", json!({
                "handle": handle_id,
                "name": pipeline_name,
                "success": !failed,
                "stepsCompleted": ctx.step_results.len(),
                "stepsTotal": pipeline.steps.len(),
                "durationMs": total_duration_ms,
            }));
        }

        log.info(&format!("[{}] Pipeline '{}' completed: success={}, duration={}ms",
            handle_id, pipeline_name, !failed, total_duration_ms));

        if failed {
            Err(error_msg.unwrap_or_else(|| "Pipeline failed".to_string()))
        } else {
            Ok((0, last_output))
        }
    }

    /// Static step execution for use in spawned tasks
    fn execute_step_static<'a>(
        step: &'a PipelineStep,
        index: usize,
        ctx: &'a mut ExecutionContext,
        handle_id: &'a str,
        registry: &'a Arc<ModuleRegistry>,
        bus: Option<&'a Arc<MessageBus>>,
    ) -> BoxFuture<'a, Result<StepResult, String>> {
        async move {
            match step {
                PipelineStep::Shell { cmd, args, timeout_secs, working_dir } => {
                    Self::execute_shell_step_static(cmd, args, timeout_secs.unwrap_or(300), working_dir.as_ref(), index, ctx, handle_id).await
                }
                PipelineStep::Llm { prompt, model, provider, max_tokens, temperature, system_prompt } => {
                    Self::execute_llm_step_static(prompt, model.as_deref(), provider.as_deref(), *max_tokens, *temperature, system_prompt.as_deref(), index, ctx, registry).await
                }
                PipelineStep::Command { command, params } => {
                    Self::execute_command_step_static(command, params, index, ctx, registry).await
                }
                PipelineStep::Condition { condition, then_steps, else_steps } => {
                    Self::execute_condition_step_static(condition, then_steps, else_steps, index, ctx, handle_id, registry, bus).await
                }
                PipelineStep::Loop { count, steps } => {
                    Self::execute_loop_step_static(*count, steps, index, ctx, handle_id, registry, bus).await
                }
            }
        }.boxed()
    }

    /// Static shell step execution
    async fn execute_shell_step_static(
        cmd: &str,
        args: &[String],
        timeout_secs: u64,
        working_dir_override: Option<&String>,
        index: usize,
        ctx: &mut ExecutionContext,
        handle_id: &str,
    ) -> Result<StepResult, String> {
        use crate::runtime;
        let log = runtime::logger("sentinel");
        let start = Instant::now();

        // Interpolate args
        let interpolated_args: Vec<String> = args.iter()
            .map(|arg| Self::interpolate_static(arg, ctx))
            .collect();

        let work_dir = working_dir_override
            .map(|p| PathBuf::from(Self::interpolate_static(p, ctx)))
            .unwrap_or_else(|| ctx.working_dir.clone());

        log.info(&format!("[{}] Shell: {} {:?} in {:?}", handle_id, cmd, interpolated_args, work_dir));

        // Execute command - if cmd contains spaces and no args, run through shell
        let (actual_cmd, actual_args): (&str, Vec<String>) = if cmd.contains(' ') && interpolated_args.is_empty() {
            ("/bin/sh", vec!["-c".to_string(), cmd.to_string()])
        } else {
            (cmd, interpolated_args)
        };

        let output = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            Command::new(actual_cmd)
                .args(&actual_args)
                .current_dir(&work_dir)
                .output()
        ).await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match output {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code().unwrap_or(-1);
                let success = exit_code == 0;

                Ok(StepResult {
                    step_index: index,
                    step_type: "shell".to_string(),
                    success,
                    duration_ms,
                    output: Some(stdout.clone()),
                    error: if success { None } else { Some(stderr) },
                    exit_code: Some(exit_code),
                    data: json!({
                        "stdout": stdout,
                        "stderr": String::from_utf8_lossy(&output.stderr),
                        "exitCode": exit_code,
                    }),
                })
            }
            Ok(Err(e)) => {
                Err(format!("Failed to execute command: {}", e))
            }
            Err(_) => {
                Err(format!("Command timed out after {}s", timeout_secs))
            }
        }
    }

    /// Static LLM step execution
    async fn execute_llm_step_static(
        prompt: &str,
        model: Option<&str>,
        provider: Option<&str>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        system_prompt: Option<&str>,
        index: usize,
        ctx: &mut ExecutionContext,
        registry: &Arc<ModuleRegistry>,
    ) -> Result<StepResult, String> {
        use crate::runtime;
        let log = runtime::logger("sentinel");
        let start = Instant::now();

        // Interpolate prompt
        let interpolated_prompt = Self::interpolate_static(prompt, ctx);
        let interpolated_system = system_prompt.map(|s| Self::interpolate_static(s, ctx));

        log.info(&format!("LLM step: model={:?}, provider={:?}, prompt_len={}",
            model, provider, interpolated_prompt.len()));

        // Build params for ai/generate
        let mut params = json!({
            "prompt": interpolated_prompt,
        });

        if let Some(m) = model {
            params["model"] = json!(m);
        }
        if let Some(p) = provider {
            params["provider"] = json!(p);
        }
        if let Some(t) = max_tokens {
            params["max_tokens"] = json!(t);
        }
        if let Some(temp) = temperature {
            params["temperature"] = json!(temp);
        }
        if let Some(sys) = interpolated_system {
            params["system_prompt"] = json!(sys);
        }

        // Route to ai/generate via registry
        let (module, cmd) = registry.route_command("ai/generate")
            .ok_or("ai module not found in registry")?;

        let result = module.handle_command(&cmd, params).await?;

        let duration_ms = start.elapsed().as_millis() as u64;

        // Extract result
        match result {
            CommandResult::Json(json) => {
                let success = json.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
                let text = json.get("text").and_then(|v| v.as_str()).map(|s| s.to_string());
                let error = json.get("error").and_then(|v| v.as_str()).map(|s| s.to_string());

                Ok(StepResult {
                    step_index: index,
                    step_type: "llm".to_string(),
                    success,
                    duration_ms,
                    output: text,
                    error,
                    exit_code: None,
                    data: json,
                })
            }
            CommandResult::Binary { .. } => {
                Err("Unexpected binary response from ai/generate".to_string())
            }
        }
    }

    /// Static command step execution
    async fn execute_command_step_static(
        command: &str,
        params: &Value,
        index: usize,
        ctx: &mut ExecutionContext,
        registry: &Arc<ModuleRegistry>,
    ) -> Result<StepResult, String> {
        use crate::runtime;
        let log = runtime::logger("sentinel");
        let start = Instant::now();

        // Interpolate params
        let interpolated_params = Self::interpolate_value_static(params, ctx);

        log.info(&format!("Command step: {}", command));

        // Route to module
        let (module, cmd) = registry.route_command(command)
            .ok_or_else(|| format!("Command not found in registry: {}", command))?;

        let result = module.handle_command(&cmd, interpolated_params).await?;

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            CommandResult::Json(json) => {
                let success = json.get("success").and_then(|v| v.as_bool()).unwrap_or(true);
                let error = json.get("error").and_then(|v| v.as_str()).map(|s| s.to_string());

                Ok(StepResult {
                    step_index: index,
                    step_type: "command".to_string(),
                    success,
                    duration_ms,
                    output: None,
                    error,
                    exit_code: None,
                    data: json,
                })
            }
            CommandResult::Binary { data, .. } => {
                Ok(StepResult {
                    step_index: index,
                    step_type: "command".to_string(),
                    success: true,
                    duration_ms,
                    output: Some(format!("<binary {} bytes>", data.len())),
                    error: None,
                    exit_code: None,
                    data: json!({ "binarySize": data.len() }),
                })
            }
        }
    }

    /// Static condition step execution
    async fn execute_condition_step_static(
        condition: &str,
        then_steps: &[PipelineStep],
        else_steps: &[PipelineStep],
        index: usize,
        ctx: &mut ExecutionContext,
        handle_id: &str,
        registry: &Arc<ModuleRegistry>,
        bus: Option<&Arc<MessageBus>>,
    ) -> Result<StepResult, String> {
        let start = Instant::now();

        // Evaluate condition
        let interpolated = Self::interpolate_static(condition, ctx);
        let condition_result = Self::evaluate_condition_static(&interpolated);

        let steps_to_run = if condition_result { then_steps } else { else_steps };

        // Execute chosen branch
        for (i, step) in steps_to_run.iter().enumerate() {
            let sub_result = Self::execute_step_static(step, ctx.step_results.len(), ctx, handle_id, registry, bus).await?;
            if !sub_result.success {
                return Ok(StepResult {
                    step_index: index,
                    step_type: "condition".to_string(),
                    success: false,
                    duration_ms: start.elapsed().as_millis() as u64,
                    output: None,
                    error: sub_result.error,
                    exit_code: None,
                    data: json!({
                        "conditionResult": condition_result,
                        "branch": if condition_result { "then" } else { "else" },
                        "failedStep": i,
                    }),
                });
            }
            ctx.step_results.push(sub_result);
        }

        Ok(StepResult {
            step_index: index,
            step_type: "condition".to_string(),
            success: true,
            duration_ms: start.elapsed().as_millis() as u64,
            output: None,
            error: None,
            exit_code: None,
            data: json!({
                "conditionResult": condition_result,
                "branch": if condition_result { "then" } else { "else" },
                "stepsExecuted": steps_to_run.len(),
            }),
        })
    }

    /// Static loop step execution
    async fn execute_loop_step_static(
        count: usize,
        steps: &[PipelineStep],
        index: usize,
        ctx: &mut ExecutionContext,
        handle_id: &str,
        registry: &Arc<ModuleRegistry>,
        bus: Option<&Arc<MessageBus>>,
    ) -> Result<StepResult, String> {
        let start = Instant::now();

        for iteration in 0..count {
            // Add iteration to context
            ctx.inputs.insert("iteration".to_string(), json!(iteration));

            for step in steps {
                let sub_result = Self::execute_step_static(step, ctx.step_results.len(), ctx, handle_id, registry, bus).await?;
                if !sub_result.success {
                    return Ok(StepResult {
                        step_index: index,
                        step_type: "loop".to_string(),
                        success: false,
                        duration_ms: start.elapsed().as_millis() as u64,
                        output: None,
                        error: sub_result.error,
                        exit_code: None,
                        data: json!({
                            "iteration": iteration,
                            "totalIterations": count,
                        }),
                    });
                }
                ctx.step_results.push(sub_result);
            }
        }

        Ok(StepResult {
            step_index: index,
            step_type: "loop".to_string(),
            success: true,
            duration_ms: start.elapsed().as_millis() as u64,
            output: None,
            error: None,
            exit_code: None,
            data: json!({
                "iterationsCompleted": count,
            }),
        })
    }

    /// Static interpolation for spawned tasks
    fn interpolate_static(template: &str, ctx: &ExecutionContext) -> String {
        let re = Regex::new(r"\{\{([^}]+)\}\}").unwrap();

        re.replace_all(template, |caps: &regex::Captures| {
            let path = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            Self::resolve_path_static(path, ctx)
        }).to_string()
    }

    /// Static value interpolation
    fn interpolate_value_static(value: &Value, ctx: &ExecutionContext) -> Value {
        match value {
            Value::String(s) => Value::String(Self::interpolate_static(s, ctx)),
            Value::Array(arr) => Value::Array(arr.iter().map(|v| Self::interpolate_value_static(v, ctx)).collect()),
            Value::Object(obj) => {
                let mut new_obj = serde_json::Map::new();
                for (k, v) in obj {
                    new_obj.insert(k.clone(), Self::interpolate_value_static(v, ctx));
                }
                Value::Object(new_obj)
            }
            _ => value.clone(),
        }
    }

    /// Static path resolution
    fn resolve_path_static(path: &str, ctx: &ExecutionContext) -> String {
        let parts: Vec<&str> = path.split('.').collect();
        if parts.is_empty() {
            return format!("{{{{{}}}}}", path);
        }

        match parts[0] {
            "steps" => {
                if parts.len() < 2 {
                    return "".to_string();
                }
                let index: usize = parts[1].parse().unwrap_or(usize::MAX);
                if index >= ctx.step_results.len() {
                    return "".to_string();
                }
                let result = &ctx.step_results[index];

                if parts.len() == 2 {
                    return result.output.clone().unwrap_or_default();
                }

                match parts[2] {
                    "output" => result.output.clone().unwrap_or_default(),
                    "success" => result.success.to_string(),
                    "error" => result.error.clone().unwrap_or_default(),
                    "exitCode" | "exit_code" => result.exit_code.map(|c| c.to_string()).unwrap_or_default(),
                    "data" => {
                        if parts.len() > 3 {
                            let mut current = &result.data;
                            for part in &parts[3..] {
                                current = current.get(*part).unwrap_or(&Value::Null);
                            }
                            match current {
                                Value::String(s) => s.clone(),
                                _ => current.to_string(),
                            }
                        } else {
                            result.data.to_string()
                        }
                    }
                    _ => "".to_string(),
                }
            }
            "input" | "inputs" => {
                if parts.len() < 2 {
                    return "".to_string();
                }
                ctx.inputs.get(parts[1])
                    .map(|v| match v {
                        Value::String(s) => s.clone(),
                        _ => v.to_string(),
                    })
                    .unwrap_or_default()
            }
            "env" => {
                if parts.len() < 2 {
                    return "".to_string();
                }
                std::env::var(parts[1]).unwrap_or_default()
            }
            _ => format!("{{{{{}}}}}", path),
        }
    }

    /// Static condition evaluation
    fn evaluate_condition_static(condition: &str) -> bool {
        let trimmed = condition.trim();

        if trimmed == "true" {
            return true;
        }
        if trimmed == "false" {
            return false;
        }

        // Non-empty string is truthy
        if !trimmed.is_empty() && trimmed != "0" && trimmed != "null" && trimmed != "undefined" {
            return true;
        }

        false
    }

    /// Static version of execute_isolated for use in spawned tasks
    async fn execute_isolated_static(
        logs_base_dir: PathBuf,
        handle_id: String,
        command: String,
        args: Vec<String>,
        working_dir: PathBuf,
        env: HashMap<String, String>,
        cancel_rx: mpsc::Receiver<()>,
        bus: Option<Arc<MessageBus>>,
    ) -> Result<(i32, String), String> {
        use crate::runtime;
        let log = runtime::logger("sentinel");

        // Create logs directory in .continuum/jtag/logs/system/sentinels/{handle}/
        let logs_dir = logs_base_dir.join(&handle_id);
        tokio::fs::create_dir_all(&logs_dir)
            .await
            .map_err(|e| format!("Failed to create logs dir: {e}"))?;

        let stdout_path = logs_dir.join("stdout.log");
        let stderr_path = logs_dir.join("stderr.log");
        let combined_path = logs_dir.join("combined.log");

        log.info(&format!(
            "Executing sentinel {}: {} {:?} in {:?}",
            handle_id, command, args, working_dir
        ));

        // Spawn child process
        let mut child = Command::new(&command)
            .args(&args)
            .current_dir(&working_dir)
            .envs(&env)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| format!("Failed to spawn process: {e}"))?;

        // Open log files
        let stdout_file = tokio::fs::File::create(&stdout_path)
            .await
            .map_err(|e| format!("Failed to create stdout log: {e}"))?;
        let stderr_file = tokio::fs::File::create(&stderr_path)
            .await
            .map_err(|e| format!("Failed to create stderr log: {e}"))?;
        let combined_file = tokio::fs::File::create(&combined_path)
            .await
            .map_err(|e| format!("Failed to create combined log: {e}"))?;

        let mut stdout_writer = tokio::io::BufWriter::new(stdout_file);
        let mut stderr_writer = tokio::io::BufWriter::new(stderr_file);
        let mut combined_writer = tokio::io::BufWriter::new(combined_file);

        // Take stdout/stderr from child
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();

        let mut stdout_reader = BufReader::new(stdout).lines();
        let mut stderr_reader = BufReader::new(stderr).lines();

        let mut cancel_rx = cancel_rx;
        let mut last_output = String::new();
        let mut stdout_closed = false;
        let mut stderr_closed = false;

        // Stream output to logs
        loop {
            tokio::select! {
                biased; // Check cancellation first

                _ = cancel_rx.recv() => {
                    log.warn(&format!("Sentinel {} cancelled", handle_id));
                    child.kill().await.ok();
                    return Err("Cancelled".to_string());
                }

                line = stdout_reader.next_line(), if !stdout_closed => {
                    match line {
                        Ok(Some(line)) => {
                            use tokio::io::AsyncWriteExt;
                            let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                            let timestamped = format!("[{}] [STDOUT] {}\n", timestamp, line);
                            stdout_writer.write_all(line.as_bytes()).await.ok();
                            stdout_writer.write_all(b"\n").await.ok();
                            combined_writer.write_all(timestamped.as_bytes()).await.ok();
                            last_output = line.clone();

                            // Emit log event
                            if let Some(ref bus) = bus {
                                bus.publish_async_only(&format!("sentinel:{}:log", handle_id), json!({
                                    "handle": handle_id,
                                    "stream": "stdout",
                                    "chunk": line,
                                    "timestamp": timestamp,
                                    "sourceType": "stdout",
                                }));
                            }
                        }
                        Ok(None) => {
                            // EOF - mark as closed so we don't poll it anymore
                            stdout_closed = true;
                        }
                        Err(e) => {
                            log.warn(&format!("stdout read error: {e}"));
                            stdout_closed = true;
                        }
                    }
                }

                line = stderr_reader.next_line(), if !stderr_closed => {
                    match line {
                        Ok(Some(line)) => {
                            use tokio::io::AsyncWriteExt;
                            let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
                            let timestamped = format!("[{}] [STDERR] {}\n", timestamp, line);
                            stderr_writer.write_all(line.as_bytes()).await.ok();
                            stderr_writer.write_all(b"\n").await.ok();
                            combined_writer.write_all(timestamped.as_bytes()).await.ok();

                            if line.contains("error") || line.contains("Error") || line.contains("ERROR") {
                                log.warn(&format!("[{}] {}", handle_id, line));
                            }

                            // Emit log event
                            if let Some(ref bus) = bus {
                                bus.publish_async_only(&format!("sentinel:{}:log", handle_id), json!({
                                    "handle": handle_id,
                                    "stream": "stderr",
                                    "chunk": line,
                                    "timestamp": timestamp,
                                    "sourceType": "stderr",
                                }));
                            }
                        }
                        Ok(None) => {
                            // EOF - mark as closed so we don't poll it anymore
                            stderr_closed = true;
                        }
                        Err(e) => {
                            log.warn(&format!("stderr read error: {e}"));
                            stderr_closed = true;
                        }
                    }
                }

                status = child.wait() => {
                    use tokio::io::AsyncWriteExt;
                    stdout_writer.flush().await.ok();
                    stderr_writer.flush().await.ok();
                    combined_writer.flush().await.ok();

                    match status {
                        Ok(exit_status) => {
                            let code = exit_status.code().unwrap_or(-1);
                            log.info(&format!("Sentinel {} exited with code {}", handle_id, code));
                            return Ok((code, last_output));
                        }
                        Err(e) => return Err(format!("Process wait failed: {e}")),
                    }
                }
            }
        }
    }

    /// Get sentinel status
    async fn get_status(&self, params: Value) -> Result<CommandResult, String> {
        let handle_id = params.get("handle")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: handle")?;

        if let Some(entry) = self.sentinels.get(handle_id) {
            Ok(CommandResult::Json(json!({
                "handle": entry.handle,
            })))
        } else {
            Err(format!("Sentinel handle not found: {}", handle_id))
        }
    }

    /// List all sentinel handles
    async fn list_handles(&self, _params: Value) -> Result<CommandResult, String> {
        let handles: Vec<SentinelHandle> = self.sentinels
            .iter()
            .map(|entry| entry.handle.clone())
            .collect();

        Ok(CommandResult::Json(json!({
            "handles": handles,
            "total": handles.len(),
        })))
    }

    /// Cancel a running sentinel
    async fn cancel_sentinel(&self, params: Value) -> Result<CommandResult, String> {
        let handle_id = params.get("handle")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: handle")?;

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
            return Err(format!("Sentinel {} is not running", handle_id));
        }

        Err(format!("Sentinel handle not found: {}", handle_id))
    }

    /// List log streams for a handle
    async fn list_logs(&self, params: Value) -> Result<CommandResult, String> {
        let handle_id = params.get("handle")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: handle")?;

        let logs_dir = self.logs_dir(handle_id);

        if !logs_dir.exists() {
            return Ok(CommandResult::Json(json!({
                "handle": handle_id,
                "streams": [],
            })));
        }

        let mut streams = Vec::new();
        let mut entries = tokio::fs::read_dir(&logs_dir)
            .await
            .map_err(|e| format!("Failed to read logs dir: {e}"))?;

        while let Some(entry) = entries.next_entry().await.map_err(|e| format!("Read error: {e}"))? {
            let path = entry.path();
            // Include .log files AND .jsonl files (for pipeline step results)
            let ext = path.extension().and_then(|e| e.to_str());
            if ext == Some("log") || ext == Some("jsonl") {
                let metadata = tokio::fs::metadata(&path)
                    .await
                    .map_err(|e| format!("Metadata error: {e}"))?;

                let modified = metadata.modified()
                    .map(|t| {
                        let datetime: chrono::DateTime<chrono::Utc> = t.into();
                        datetime.format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
                    })
                    .unwrap_or_default();

                streams.push(LogStreamInfo {
                    name: path.file_stem().unwrap_or_default().to_string_lossy().to_string(),
                    path: path.to_string_lossy().to_string(),
                    size: metadata.len(),
                    modified_at: modified,
                });
            }
        }

        Ok(CommandResult::Json(json!({
            "handle": handle_id,
            "logsDir": logs_dir.to_string_lossy(),
            "streams": streams,
        })))
    }

    /// Read a log stream
    async fn read_log(&self, params: Value) -> Result<CommandResult, String> {
        let handle_id = params.get("handle")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: handle")?;

        let stream = params.get("stream")
            .and_then(|v| v.as_str())
            .unwrap_or("combined");

        let offset = params.get("offset")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        let limit = params.get("limit")
            .and_then(|v| v.as_u64())
            .unwrap_or(1000) as usize;

        // Try .log first, then .jsonl for pipeline step results
        let logs_dir = self.logs_dir(handle_id);
        let log_path = logs_dir.join(format!("{}.log", stream));
        let jsonl_path = logs_dir.join(format!("{}.jsonl", stream));

        let actual_path = if log_path.exists() {
            log_path
        } else if jsonl_path.exists() {
            jsonl_path
        } else {
            return Err(format!("Log stream not found: {}", stream));
        };

        let content = tokio::fs::read_to_string(&actual_path)
            .await
            .map_err(|e| format!("Failed to read log: {e}"))?;

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let selected_lines: Vec<&str> = lines
            .into_iter()
            .skip(offset)
            .take(limit)
            .collect();

        let truncated = offset + selected_lines.len() < total_lines;

        Ok(CommandResult::Json(json!({
            "handle": handle_id,
            "stream": stream,
            "content": selected_lines.join("\n"),
            "lineCount": selected_lines.len(),
            "totalLines": total_lines,
            "offset": offset,
            "truncated": truncated,
        })))
    }

    /// Tail a log stream (last N lines)
    async fn tail_log(&self, params: Value) -> Result<CommandResult, String> {
        let handle_id = params.get("handle")
            .and_then(|v| v.as_str())
            .ok_or("Missing required parameter: handle")?;

        let stream = params.get("stream")
            .and_then(|v| v.as_str())
            .unwrap_or("combined");

        let lines_count = params.get("lines")
            .and_then(|v| v.as_u64())
            .unwrap_or(20) as usize;

        // Try .log first, then .jsonl for pipeline step results
        let logs_dir = self.logs_dir(handle_id);
        let log_path = logs_dir.join(format!("{}.log", stream));
        let jsonl_path = logs_dir.join(format!("{}.jsonl", stream));

        let actual_path = if log_path.exists() {
            log_path
        } else if jsonl_path.exists() {
            jsonl_path
        } else {
            return Err(format!("Log stream not found: {}", stream));
        };

        let content = tokio::fs::read_to_string(&actual_path)
            .await
            .map_err(|e| format!("Failed to read log: {e}"))?;

        let lines: Vec<&str> = content.lines().collect();
        let total_lines = lines.len();

        let start = total_lines.saturating_sub(lines_count);
        let tail_lines: Vec<&str> = lines.into_iter().skip(start).collect();

        Ok(CommandResult::Json(json!({
            "handle": handle_id,
            "stream": stream,
            "content": tail_lines.join("\n"),
            "lineCount": tail_lines.len(),
            "totalLines": total_lines,
        })))
    }

    // =========================================================================
    // PIPELINE EXECUTION
    // =========================================================================

    /// Execute a pipeline (multi-step with LLM, conditions, loops)
    async fn execute_pipeline(&self, params: Value) -> Result<CommandResult, String> {
        use crate::runtime;
        let log = runtime::logger("sentinel");

        let start_time = Instant::now();
        let handle_id = Self::generate_handle_id();

        // Parse pipeline
        let pipeline: Pipeline = serde_json::from_value(params.get("pipeline").cloned().unwrap_or(params.clone()))
            .map_err(|e| format!("Failed to parse pipeline: {}", e))?;

        let pipeline_name = pipeline.name.as_deref().unwrap_or("unnamed");
        log.info(&format!("Starting pipeline '{}' (handle={}), {} steps",
            pipeline_name, handle_id, pipeline.steps.len()));

        // Create execution context
        let working_dir = pipeline.working_dir
            .map(PathBuf::from)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."));

        let mut ctx = ExecutionContext {
            step_results: Vec::new(),
            inputs: pipeline.inputs.clone(),
            working_dir,
        };

        // Create logs directory for this pipeline
        let logs_dir = self.logs_dir(&handle_id);
        tokio::fs::create_dir_all(&logs_dir).await.ok();

        // Execute steps
        let mut success = true;
        let mut error_msg: Option<String> = None;

        for (i, step) in pipeline.steps.iter().enumerate() {
            log.info(&format!("[{}] Executing step {}: {:?}", handle_id, i, step_type_name(step)));

            match self.execute_step(step, i, &mut ctx, &handle_id).await {
                Ok(result) => {
                    if !result.success {
                        log.warn(&format!("[{}] Step {} failed: {:?}", handle_id, i, result.error));
                        success = false;
                        error_msg = result.error.clone();
                        ctx.step_results.push(result);
                        break;
                    }
                    ctx.step_results.push(result);
                }
                Err(e) => {
                    log.error(&format!("[{}] Step {} error: {}", handle_id, i, e));
                    success = false;
                    error_msg = Some(e.clone());
                    ctx.step_results.push(StepResult {
                        step_index: i,
                        step_type: step_type_name(step).to_string(),
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

        let total_duration_ms = start_time.elapsed().as_millis() as u64;

        let result = PipelineResult {
            handle: handle_id.clone(),
            success,
            total_duration_ms,
            steps_completed: ctx.step_results.len(),
            steps_total: pipeline.steps.len(),
            step_results: ctx.step_results,
            error: error_msg,
        };

        // Emit completion event
        if let Some(ref bus) = *self.bus.read() {
            bus.publish_async_only("sentinel:pipeline:complete", json!({
                "handle": handle_id,
                "name": pipeline_name,
                "success": success,
                "durationMs": total_duration_ms,
            }));
        }

        log.info(&format!("Pipeline '{}' completed: success={}, duration={}ms",
            pipeline_name, success, total_duration_ms));

        Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or(json!({"error": "serialization failed"}))))
    }

    /// Execute a single step (returns BoxFuture to handle recursion)
    fn execute_step<'a>(
        &'a self,
        step: &'a PipelineStep,
        index: usize,
        ctx: &'a mut ExecutionContext,
        handle_id: &'a str,
    ) -> BoxFuture<'a, Result<StepResult, String>> {
        async move {
            match step {
                PipelineStep::Shell { cmd, args, timeout_secs, working_dir } => {
                    self.execute_shell_step(cmd, args, timeout_secs.unwrap_or(300), working_dir.as_ref(), index, ctx, handle_id).await
                }
                PipelineStep::Llm { prompt, model, provider, max_tokens, temperature, system_prompt } => {
                    self.execute_llm_step(prompt, model.as_deref(), provider.as_deref(), *max_tokens, *temperature, system_prompt.as_deref(), index, ctx).await
                }
                PipelineStep::Command { command, params } => {
                    self.execute_command_step(command, params, index, ctx).await
                }
                PipelineStep::Condition { condition, then_steps, else_steps } => {
                    self.execute_condition_step(condition, then_steps, else_steps, index, ctx, handle_id).await
                }
                PipelineStep::Loop { count, steps } => {
                    self.execute_loop_step(*count, steps, index, ctx, handle_id).await
                }
            }
        }.boxed()
    }

    /// Execute a shell step
    async fn execute_shell_step(
        &self,
        cmd: &str,
        args: &[String],
        timeout_secs: u64,
        working_dir_override: Option<&String>,
        index: usize,
        ctx: &mut ExecutionContext,
        handle_id: &str,
    ) -> Result<StepResult, String> {
        use crate::runtime;
        let log = runtime::logger("sentinel");
        let start = Instant::now();

        // Interpolate args
        let interpolated_args: Vec<String> = args.iter()
            .map(|arg| self.interpolate(arg, ctx))
            .collect();

        let work_dir = working_dir_override
            .map(|p| PathBuf::from(self.interpolate(p, ctx)))
            .unwrap_or_else(|| ctx.working_dir.clone());

        log.info(&format!("[{}] Shell: {} {:?} in {:?}", handle_id, cmd, interpolated_args, work_dir));

        // Execute command - if cmd contains spaces and no args, run through shell
        let (actual_cmd, actual_args): (&str, Vec<String>) = if cmd.contains(' ') && interpolated_args.is_empty() {
            ("/bin/sh", vec!["-c".to_string(), cmd.to_string()])
        } else {
            (cmd, interpolated_args)
        };

        let output = tokio::time::timeout(
            Duration::from_secs(timeout_secs),
            Command::new(actual_cmd)
                .args(&actual_args)
                .current_dir(&work_dir)
                .output()
        ).await;

        let duration_ms = start.elapsed().as_millis() as u64;

        match output {
            Ok(Ok(output)) => {
                let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                let exit_code = output.status.code().unwrap_or(-1);
                let success = exit_code == 0;

                Ok(StepResult {
                    step_index: index,
                    step_type: "shell".to_string(),
                    success,
                    duration_ms,
                    output: Some(stdout.clone()),
                    error: if success { None } else { Some(stderr) },
                    exit_code: Some(exit_code),
                    data: json!({
                        "stdout": stdout,
                        "stderr": String::from_utf8_lossy(&output.stderr),
                        "exitCode": exit_code,
                    }),
                })
            }
            Ok(Err(e)) => {
                Err(format!("Failed to execute command: {}", e))
            }
            Err(_) => {
                Err(format!("Command timed out after {}s", timeout_secs))
            }
        }
    }

    /// Execute an LLM step (calls AIProviderModule directly via registry)
    async fn execute_llm_step(
        &self,
        prompt: &str,
        model: Option<&str>,
        provider: Option<&str>,
        max_tokens: Option<u32>,
        temperature: Option<f32>,
        system_prompt: Option<&str>,
        index: usize,
        ctx: &mut ExecutionContext,
    ) -> Result<StepResult, String> {
        use crate::runtime;
        let log = runtime::logger("sentinel");
        let start = Instant::now();

        // Get registry
        let registry = self.registry.read()
            .as_ref()
            .ok_or("SentinelModule not initialized - missing registry")?
            .clone();

        // Interpolate prompt
        let interpolated_prompt = self.interpolate(prompt, ctx);
        let interpolated_system = system_prompt.map(|s| self.interpolate(s, ctx));

        log.info(&format!("LLM step: model={:?}, provider={:?}, prompt_len={}",
            model, provider, interpolated_prompt.len()));

        // Build params for ai/generate
        let mut params = json!({
            "prompt": interpolated_prompt,
        });

        if let Some(m) = model {
            params["model"] = json!(m);
        }
        if let Some(p) = provider {
            params["provider"] = json!(p);
        }
        if let Some(t) = max_tokens {
            params["max_tokens"] = json!(t);
        }
        if let Some(temp) = temperature {
            params["temperature"] = json!(temp);
        }
        if let Some(sys) = interpolated_system {
            params["system_prompt"] = json!(sys);
        }

        // Route to ai/generate via registry
        let (module, cmd) = registry.route_command("ai/generate")
            .ok_or("ai module not found in registry")?;

        let result = module.handle_command(&cmd, params).await?;

        let duration_ms = start.elapsed().as_millis() as u64;

        // Extract result
        match result {
            CommandResult::Json(json) => {
                let success = json.get("success").and_then(|v| v.as_bool()).unwrap_or(false);
                let text = json.get("text").and_then(|v| v.as_str()).map(|s| s.to_string());
                let error = json.get("error").and_then(|v| v.as_str()).map(|s| s.to_string());

                Ok(StepResult {
                    step_index: index,
                    step_type: "llm".to_string(),
                    success,
                    duration_ms,
                    output: text,
                    error,
                    exit_code: None,
                    data: json,
                })
            }
            CommandResult::Binary { .. } => {
                Err("Unexpected binary response from ai/generate".to_string())
            }
        }
    }

    /// Execute a command step (routes to any module)
    async fn execute_command_step(
        &self,
        command: &str,
        params: &Value,
        index: usize,
        ctx: &mut ExecutionContext,
    ) -> Result<StepResult, String> {
        use crate::runtime;
        let log = runtime::logger("sentinel");
        let start = Instant::now();

        // Get registry
        let registry = self.registry.read()
            .as_ref()
            .ok_or("SentinelModule not initialized - missing registry")?
            .clone();

        // Interpolate params
        let interpolated_params = self.interpolate_value(params, ctx);

        log.info(&format!("Command step: {}", command));

        // Route to module
        let (module, cmd) = registry.route_command(command)
            .ok_or_else(|| format!("Command not found in registry: {}", command))?;

        let result = module.handle_command(&cmd, interpolated_params).await?;

        let duration_ms = start.elapsed().as_millis() as u64;

        match result {
            CommandResult::Json(json) => {
                let success = json.get("success").and_then(|v| v.as_bool()).unwrap_or(true);
                let error = json.get("error").and_then(|v| v.as_str()).map(|s| s.to_string());

                Ok(StepResult {
                    step_index: index,
                    step_type: "command".to_string(),
                    success,
                    duration_ms,
                    output: None,
                    error,
                    exit_code: None,
                    data: json,
                })
            }
            CommandResult::Binary { data, .. } => {
                Ok(StepResult {
                    step_index: index,
                    step_type: "command".to_string(),
                    success: true,
                    duration_ms,
                    output: Some(format!("<binary {} bytes>", data.len())),
                    error: None,
                    exit_code: None,
                    data: json!({ "binarySize": data.len() }),
                })
            }
        }
    }

    /// Execute a condition step
    async fn execute_condition_step(
        &self,
        condition: &str,
        then_steps: &[PipelineStep],
        else_steps: &[PipelineStep],
        index: usize,
        ctx: &mut ExecutionContext,
        handle_id: &str,
    ) -> Result<StepResult, String> {
        let start = Instant::now();

        // Evaluate condition
        let interpolated = self.interpolate(condition, ctx);
        let condition_result = self.evaluate_condition(&interpolated, ctx);

        let steps_to_run = if condition_result { then_steps } else { else_steps };

        // Execute chosen branch
        for (i, step) in steps_to_run.iter().enumerate() {
            let sub_result = self.execute_step(step, ctx.step_results.len(), ctx, handle_id).await?;
            if !sub_result.success {
                return Ok(StepResult {
                    step_index: index,
                    step_type: "condition".to_string(),
                    success: false,
                    duration_ms: start.elapsed().as_millis() as u64,
                    output: None,
                    error: sub_result.error,
                    exit_code: None,
                    data: json!({
                        "conditionResult": condition_result,
                        "branch": if condition_result { "then" } else { "else" },
                        "failedStep": i,
                    }),
                });
            }
            ctx.step_results.push(sub_result);
        }

        Ok(StepResult {
            step_index: index,
            step_type: "condition".to_string(),
            success: true,
            duration_ms: start.elapsed().as_millis() as u64,
            output: None,
            error: None,
            exit_code: None,
            data: json!({
                "conditionResult": condition_result,
                "branch": if condition_result { "then" } else { "else" },
                "stepsExecuted": steps_to_run.len(),
            }),
        })
    }

    /// Execute a loop step
    async fn execute_loop_step(
        &self,
        count: usize,
        steps: &[PipelineStep],
        index: usize,
        ctx: &mut ExecutionContext,
        handle_id: &str,
    ) -> Result<StepResult, String> {
        let start = Instant::now();

        for iteration in 0..count {
            // Add iteration to context
            ctx.inputs.insert("iteration".to_string(), json!(iteration));

            for step in steps {
                let sub_result = self.execute_step(step, ctx.step_results.len(), ctx, handle_id).await?;
                if !sub_result.success {
                    return Ok(StepResult {
                        step_index: index,
                        step_type: "loop".to_string(),
                        success: false,
                        duration_ms: start.elapsed().as_millis() as u64,
                        output: None,
                        error: sub_result.error,
                        exit_code: None,
                        data: json!({
                            "iteration": iteration,
                            "totalIterations": count,
                        }),
                    });
                }
                ctx.step_results.push(sub_result);
            }
        }

        Ok(StepResult {
            step_index: index,
            step_type: "loop".to_string(),
            success: true,
            duration_ms: start.elapsed().as_millis() as u64,
            output: None,
            error: None,
            exit_code: None,
            data: json!({
                "iterationsCompleted": count,
            }),
        })
    }

    // =========================================================================
    // INTERPOLATION
    // =========================================================================

    /// Interpolate variables in a string: {{steps.0.output}}, {{input.name}}, etc.
    fn interpolate(&self, template: &str, ctx: &ExecutionContext) -> String {
        let re = Regex::new(r"\{\{([^}]+)\}\}").unwrap();

        re.replace_all(template, |caps: &regex::Captures| {
            let path = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            self.resolve_path(path, ctx)
        }).to_string()
    }

    /// Interpolate variables in a JSON value recursively
    fn interpolate_value(&self, value: &Value, ctx: &ExecutionContext) -> Value {
        match value {
            Value::String(s) => Value::String(self.interpolate(s, ctx)),
            Value::Array(arr) => Value::Array(arr.iter().map(|v| self.interpolate_value(v, ctx)).collect()),
            Value::Object(obj) => {
                let mut new_obj = serde_json::Map::new();
                for (k, v) in obj {
                    new_obj.insert(k.clone(), self.interpolate_value(v, ctx));
                }
                Value::Object(new_obj)
            }
            _ => value.clone(),
        }
    }

    /// Resolve a variable path like "steps.0.output" or "input.name"
    fn resolve_path(&self, path: &str, ctx: &ExecutionContext) -> String {
        let parts: Vec<&str> = path.split('.').collect();
        if parts.is_empty() {
            return format!("{{{{{}}}}}", path);
        }

        match parts[0] {
            "steps" => {
                if parts.len() < 2 {
                    return "".to_string();
                }
                let index: usize = parts[1].parse().unwrap_or(usize::MAX);
                if index >= ctx.step_results.len() {
                    return "".to_string();
                }
                let result = &ctx.step_results[index];

                if parts.len() == 2 {
                    return result.output.clone().unwrap_or_default();
                }

                match parts[2] {
                    "output" => result.output.clone().unwrap_or_default(),
                    "success" => result.success.to_string(),
                    "error" => result.error.clone().unwrap_or_default(),
                    "exitCode" | "exit_code" => result.exit_code.map(|c| c.to_string()).unwrap_or_default(),
                    "data" => {
                        if parts.len() > 3 {
                            // Navigate into data
                            let mut current = &result.data;
                            for part in &parts[3..] {
                                current = current.get(*part).unwrap_or(&Value::Null);
                            }
                            match current {
                                Value::String(s) => s.clone(),
                                _ => current.to_string(),
                            }
                        } else {
                            result.data.to_string()
                        }
                    }
                    _ => "".to_string(),
                }
            }
            "input" | "inputs" => {
                if parts.len() < 2 {
                    return "".to_string();
                }
                ctx.inputs.get(parts[1])
                    .map(|v| match v {
                        Value::String(s) => s.clone(),
                        _ => v.to_string(),
                    })
                    .unwrap_or_default()
            }
            "env" => {
                if parts.len() < 2 {
                    return "".to_string();
                }
                std::env::var(parts[1]).unwrap_or_default()
            }
            _ => format!("{{{{{}}}}}", path),
        }
    }

    /// Evaluate a condition expression
    fn evaluate_condition(&self, condition: &str, _ctx: &ExecutionContext) -> bool {
        let trimmed = condition.trim();

        // Boolean literals
        if trimmed == "true" {
            return true;
        }
        if trimmed == "false" {
            return false;
        }

        // Non-empty string is truthy
        if !trimmed.is_empty() && trimmed != "0" && trimmed != "null" && trimmed != "undefined" {
            return true;
        }

        false
    }
}

/// Get the type name of a step for logging
fn step_type_name(step: &PipelineStep) -> &'static str {
    match step {
        PipelineStep::Shell { .. } => "shell",
        PipelineStep::Llm { .. } => "llm",
        PipelineStep::Command { .. } => "command",
        PipelineStep::Condition { .. } => "condition",
        PipelineStep::Loop { .. } => "loop",
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
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        let log = crate::runtime::logger("sentinel");
        log.info("SentinelModule initialized with pipeline support");

        // Store the message bus for event emission
        *self.bus.write() = Some(Arc::clone(&ctx.bus));

        // Store the module registry for inter-module calls (LLM, data, etc.)
        *self.registry.write() = Some(Arc::clone(&ctx.registry));

        // Set logs directory relative to current directory (JTAG root)
        if let Ok(cwd) = std::env::current_dir() {
            *self.logs_base_dir.write() = cwd.join(".continuum/jtag/logs/system/sentinels");
        }

        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        match command {
            // Core execution commands
            "sentinel/execute" | "sentinel/run" => self.run_sentinel(params).await,
            "sentinel/status" => self.get_status(params).await,
            "sentinel/list" => self.list_handles(params).await,
            "sentinel/cancel" => self.cancel_sentinel(params).await,

            // Pipeline execution (multi-step with LLM, conditions, loops)
            "sentinel/pipeline" => self.execute_pipeline(params).await,

            // Log commands
            "sentinel/logs/list" => self.list_logs(params).await,
            "sentinel/logs/read" => self.read_log(params).await,
            "sentinel/logs/tail" => self.tail_log(params).await,

            _ => Err(format!("Unknown sentinel command: {}", command)),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_handle_id() {
        let id = SentinelModule::generate_handle_id();
        assert_eq!(id.len(), 8);
        assert!(id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'));
    }

    #[test]
    fn test_sentinel_status_serialization() {
        let status = SentinelStatus::Running;
        let json = serde_json::to_string(&status).unwrap();
        assert_eq!(json, "\"running\"");
    }
}
