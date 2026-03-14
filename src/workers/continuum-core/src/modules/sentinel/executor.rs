//! Pipeline and isolated process execution
//!
//! Single implementations replacing the duplicated static/instance methods.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::time::Instant;
use sysinfo::System;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;

use super::steps;
use super::types::{
    step_type_name, ExecutionContext, Pipeline, PipelineContext, PipelineResult, StepResult,
};
use crate::runtime::{self, message_bus::MessageBus, ModuleRegistry};

/// Execute a multi-step pipeline with LLM, conditions, loops
pub async fn execute_pipeline(
    logs_base_dir: PathBuf,
    pipeline: Pipeline,
    handle_id: String,
    working_dir: PathBuf,
    bus: Option<Arc<MessageBus>>,
    registry: Option<Arc<ModuleRegistry>>,
) -> Result<(i32, String), String> {
    let log = runtime::logger("sentinel");

    let registry = registry.ok_or("Pipeline execution requires module registry")?;
    let start_time = Instant::now();
    let pipeline_name = pipeline.name.as_deref().unwrap_or("unnamed");

    // Create logs directory for this pipeline
    let logs_dir = logs_base_dir.join(&handle_id);
    if let Err(e) = tokio::fs::create_dir_all(&logs_dir).await {
        log.warn(&format!("[{handle_id}] Failed to create logs dir: {e}"));
    }
    let steps_log_path = logs_dir.join("steps.jsonl");

    log.info(&format!(
        "[{}] Pipeline '{}' starting with {} steps",
        handle_id,
        pipeline_name,
        pipeline.steps.len()
    ));

    // Create execution context
    let mut ctx = ExecutionContext {
        step_results: Vec::new(),
        inputs: pipeline.inputs.clone(),
        working_dir: pipeline
            .working_dir
            .clone()
            .map(PathBuf::from)
            .unwrap_or(working_dir),
        named_outputs: HashMap::new(),
    };

    // Execute steps
    let mut last_output = String::new();
    let mut failed = false;
    let mut error_msg: Option<String> = None;

    for (i, step) in pipeline.steps.iter().enumerate() {
        let step_type = step_type_name(step);
        log.info(&format!(
            "[{}] Step {}/{}: {}",
            handle_id,
            i + 1,
            pipeline.steps.len(),
            step_type
        ));

        // Emit step progress
        if let Some(ref bus) = bus {
            bus.publish_async_only(
                &format!("sentinel:{handle_id}:progress"),
                json!({
                    "handle": handle_id,
                    "step": i,
                    "totalSteps": pipeline.steps.len(),
                    "stepType": step_type,
                    "phase": "executing",
                }),
            );
        }

        let pipeline_ctx = PipelineContext {
            handle_id: &handle_id,
            registry: &registry,
            bus: bus.as_ref(),
            steps_log_path: Some(&steps_log_path),
        };

        match steps::execute_step(step, i, &mut ctx, &pipeline_ctx).await {
            Ok(result) => {
                if result.success {
                    last_output = result.output.clone().unwrap_or_default();
                    log.info(&format!("[{handle_id}] Step {i} succeeded"));
                } else {
                    log.error(&format!(
                        "[{handle_id}] Step {i} failed: {:?}",
                        result.error
                    ));
                    failed = true;
                    error_msg = result.error.clone();
                }

                // Write the top-level step result to steps.jsonl.
                // Sub-step results (from loops, conditions, parallel branches) are
                // flushed in real-time by those step executors directly via steps_log_path.
                if let Ok(json_line) = serde_json::to_string(&result) {
                    if let Ok(mut file) = tokio::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&steps_log_path)
                        .await
                    {
                        let _ = file.write_all(format!("{json_line}\n").as_bytes()).await;
                    }
                }
                ctx.step_results.push(result);
                if failed {
                    break;
                }
            }
            Err(e) => {
                log.error(&format!("[{handle_id}] Step {i} error: {e}"));
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
                if let Ok(json_line) = serde_json::to_string(&error_result) {
                    if let Ok(mut file) = tokio::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(&steps_log_path)
                        .await
                    {
                        let _ = file.write_all(format!("{json_line}\n").as_bytes()).await;
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
        bus.publish_async_only(
            "sentinel:pipeline:complete",
            json!({
                "handle": handle_id,
                "name": pipeline_name,
                "success": !failed,
                "stepsCompleted": ctx.step_results.len(),
                "stepsTotal": pipeline.steps.len(),
                "durationMs": total_duration_ms,
            }),
        );
    }

    log.info(&format!(
        "[{}] Pipeline '{}' completed: success={}, duration={}ms",
        handle_id, pipeline_name, !failed, total_duration_ms
    ));

    if failed {
        Err(error_msg.unwrap_or_else(|| "Pipeline failed".to_string()))
    } else {
        Ok((0, last_output))
    }
}

/// Configuration for an isolated child process execution
pub struct IsolatedProcessConfig {
    pub logs_base_dir: PathBuf,
    pub handle_id: String,
    pub command: String,
    pub args: Vec<String>,
    pub working_dir: PathBuf,
    pub env: HashMap<String, String>,
}

/// Execute an isolated child process with stdout/stderr streaming to logs.
///
/// Architecture: three concurrent tasks (pipe readers + memory watchdog) oversee the child.
/// Pipe readers run as independent tokio tasks so they NEVER block on each other or on
/// the memory watchdog. This prevents pipe buffer deadlocks where a child process stalls
/// because its stdout/stderr pipe is full while the reader is busy with something else.
pub async fn execute_isolated(
    config: IsolatedProcessConfig,
    cancel_rx: mpsc::Receiver<()>,
    bus: Option<Arc<MessageBus>>,
) -> Result<(i32, String), String> {
    let IsolatedProcessConfig {
        logs_base_dir,
        handle_id,
        command,
        args,
        working_dir,
        env,
    } = config;
    let log = runtime::logger("sentinel");

    // Create logs directory
    let logs_dir = logs_base_dir.join(&handle_id);
    tokio::fs::create_dir_all(&logs_dir)
        .await
        .map_err(|e| format!("Failed to create logs dir: {e}"))?;

    let stdout_path = logs_dir.join("stdout.log");
    let stderr_path = logs_dir.join("stderr.log");
    let combined_path = logs_dir.join("combined.log");

    log.info(&format!(
        "Executing sentinel {handle_id}: {command} {args:?} in {working_dir:?}"
    ));

    // Spawn child process in its own process group.
    // This is CRITICAL: `kill(-pgid)` kills the entire tree (wrapper → micromamba → python).
    // Without this, `kill_on_drop` only kills the direct child, leaving grandchildren orphaned.
    let mut child = {
        let mut cmd = Command::new(&command);
        cmd.args(&args)
            .current_dir(&working_dir)
            .envs(&env)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        // SAFETY: pre_exec runs in the forked child before exec.
        // setsid() creates a new session + process group. Simple, async-signal-safe.
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        cmd.spawn()
            .map_err(|e| format!("Failed to spawn process: {e}"))?
    };

    let child_pid = child.id();

    // Write PID to sentinel logs dir so npm stop / shutdown can find orphans
    let pid_path = logs_dir.join("pid");
    if let Some(pid) = child_pid {
        if let Err(e) = tokio::fs::write(&pid_path, pid.to_string()).await {
            log.warn(&format!("Failed to write PID file {}: {e}", pid_path.display()));
        }
    }

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to capture stdout — not piped".to_string())?;
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| "Failed to capture stderr — not piped".to_string())?;

    // Channel for log lines from pipe readers → log writer (bounded to apply backpressure
    // to file I/O, not to pipe reads — pipe readers use unbounded sends so they never block)
    let (log_tx, mut log_rx) = mpsc::unbounded_channel::<LogLine>();

    // ── Stdout reader task: drains pipe as fast as possible, never blocks ──
    let stdout_tx = log_tx.clone();
    let stdout_handle_id = handle_id.clone();
    let stdout_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stdout).lines();
        let mut last_line = String::new();
        while let Ok(Some(line)) = reader.next_line().await {
            last_line = line.clone();
            if stdout_tx.send(LogLine::Stdout(line)).is_err() {
                break; // Receiver dropped
            }
        }
        let _ = stdout_handle_id; // used for identification if logging is needed
        last_line
    });

    // ── Stderr reader task: drains pipe as fast as possible, never blocks ──
    let stderr_tx = log_tx.clone();
    let stderr_handle_id = handle_id.clone();
    let stderr_task = tokio::spawn(async move {
        let mut reader = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = reader.next_line().await {
            if stderr_tx.send(LogLine::Stderr(line)).is_err() {
                break;
            }
        }
        let _ = stderr_handle_id;
    });

    // Drop our copy — when both reader tasks finish, the channel closes
    drop(log_tx);

    // ── Memory watchdog task: separate overseer, kills child if memory critical ──
    const MEMORY_CHECK_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);
    const CRITICAL_AVAILABLE_BYTES: u64 = 1_024 * 1_024 * 1_024; // 1 GB
    let (kill_tx, mut kill_rx) = mpsc::channel::<u64>(1); // sends available_mb on kill
    let watchdog_handle_id = handle_id.clone();
    let watchdog_bus = bus.clone();
    let watchdog_pid = child_pid;
    let watchdog_task = tokio::spawn(async move {
        let wlog = runtime::logger("sentinel");
        let mut interval = tokio::time::interval(MEMORY_CHECK_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        interval.tick().await; // skip immediate tick

        loop {
            interval.tick().await;
            let mut sys = System::new();
            sys.refresh_memory();
            let available = sys.total_memory().saturating_sub(sys.used_memory());
            if available < CRITICAL_AVAILABLE_BYTES {
                let available_mb = available / (1024 * 1024);
                wlog.error(&format!(
                    "MEMORY WATCHDOG: Only {}MB available. Killing sentinel {watchdog_handle_id}.",
                    available_mb,
                ));

                // Kill entire process group (wrapper + micromamba + python + children)
                if let Some(pid) = watchdog_pid {
                    let pgid = -(pid as i32); // negative = kill process group
                    std::process::Command::new("kill")
                        .args(["-TERM", "--", &pgid.to_string()])
                        .output()
                        .ok();
                }

                if let Some(ref bus) = watchdog_bus {
                    bus.publish_async_only(
                        &format!("sentinel:{watchdog_handle_id}:memory-kill"),
                        json!({
                            "handle": watchdog_handle_id,
                            "availableMb": available_mb,
                            "reason": "System memory critically low",
                        }),
                    );
                }

                kill_tx.send(available_mb).await.ok();
                return;
            }
        }
    });

    // ── Log writer + event emitter: processes lines from both pipes concurrently ──
    let writer_handle_id = handle_id.clone();
    let writer_bus = bus.clone();
    let writer_task = tokio::spawn(async move {
        let stdout_file = match tokio::fs::File::create(&stdout_path).await {
            Ok(f) => f,
            Err(_e) => {
                return String::new();
            }
        };
        let stderr_file = match tokio::fs::File::create(&stderr_path).await {
            Ok(f) => f,
            Err(_e) => {
                return String::new();
            }
        };
        let combined_file = match tokio::fs::File::create(&combined_path).await {
            Ok(f) => f,
            Err(_e) => {
                return String::new();
            }
        };

        let mut stdout_writer = tokio::io::BufWriter::new(stdout_file);
        let mut stderr_writer = tokio::io::BufWriter::new(stderr_file);
        let mut combined_writer = tokio::io::BufWriter::new(combined_file);
        let mut last_output = String::new();
        let wlog = runtime::logger("sentinel");
        let mut stdout_write_failed = false;
        let mut stderr_write_failed = false;

        while let Some(log_line) = log_rx.recv().await {
            let timestamp = chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string();
            match log_line {
                LogLine::Stdout(line) => {
                    let timestamped = format!("[{timestamp}] [STDOUT] {line}\n");
                    if let Err(e) = stdout_writer.write_all(line.as_bytes()).await
                        .and(stdout_writer.write_all(b"\n").await)
                        .and(combined_writer.write_all(timestamped.as_bytes()).await)
                        .and(combined_writer.flush().await)
                    {
                        if !stdout_write_failed {
                            wlog.warn(&format!("[{writer_handle_id}] Log write failed (stdout): {e}"));
                            stdout_write_failed = true;
                        }
                    }
                    last_output = line.clone();

                    if let Some(ref bus) = writer_bus {
                        // Parse structured memory reports from training processes
                        if line.starts_with("MEMORY_REPORT: ") {
                            if let Some(json_str) = line.strip_prefix("MEMORY_REPORT: ") {
                                if let Ok(report) = serde_json::from_str::<Value>(json_str) {
                                    bus.publish_async_only(&format!("sentinel:{writer_handle_id}:memory-report"), json!({
                                        "handle": writer_handle_id,
                                        "phase": report.get("phase").and_then(|v| v.as_str()).unwrap_or("unknown"),
                                        "device": report.get("device").and_then(|v| v.as_str()).unwrap_or("unknown"),
                                        "allocatedBytes": report.get("allocated_bytes").and_then(|v| v.as_u64()).unwrap_or(0),
                                        "peakBytes": report.get("peak_bytes").and_then(|v| v.as_u64()).unwrap_or(0),
                                        "processRssBytes": report.get("process_rss_bytes").and_then(|v| v.as_u64()).unwrap_or(0),
                                    }));
                                }
                            }
                        }

                        bus.publish_async_only(
                            &format!("sentinel:{writer_handle_id}:log"),
                            json!({
                                "handle": writer_handle_id,
                                "stream": "stdout",
                                "chunk": line,
                                "timestamp": timestamp,
                                "sourceType": "stdout",
                            }),
                        );
                    }
                }
                LogLine::Stderr(line) => {
                    let timestamped = format!("[{timestamp}] [STDERR] {line}\n");
                    if let Err(e) = stderr_writer.write_all(line.as_bytes()).await
                        .and(stderr_writer.write_all(b"\n").await)
                        .and(combined_writer.write_all(timestamped.as_bytes()).await)
                        .and(combined_writer.flush().await)
                    {
                        if !stderr_write_failed {
                            wlog.warn(&format!("[{writer_handle_id}] Log write failed (stderr): {e}"));
                            stderr_write_failed = true;
                        }
                    }

                    if line.contains("error") || line.contains("Error") || line.contains("ERROR") {
                        wlog.warn(&format!("[{writer_handle_id}] {line}"));
                    }

                    if let Some(ref bus) = writer_bus {
                        bus.publish_async_only(
                            &format!("sentinel:{writer_handle_id}:log"),
                            json!({
                                "handle": writer_handle_id,
                                "stream": "stderr",
                                "chunk": line,
                                "timestamp": timestamp,
                                "sourceType": "stderr",
                            }),
                        );
                    }
                }
            }
        }

        // Channel closed — both pipe readers finished
        if let Err(e) = stdout_writer.flush().await {
            wlog.warn(&format!("[{writer_handle_id}] Final stdout flush failed: {e}"));
        }
        if let Err(e) = stderr_writer.flush().await {
            wlog.warn(&format!("[{writer_handle_id}] Final stderr flush failed: {e}"));
        }
        if let Err(e) = combined_writer.flush().await {
            wlog.warn(&format!("[{writer_handle_id}] Final combined flush failed: {e}"));
        }
        last_output
    });

    // ── Main: wait for process exit, cancel, or memory kill ──
    let mut cancel_rx = cancel_rx;
    let result = tokio::select! {
        _ = cancel_rx.recv() => {
            log.warn(&format!("Sentinel {handle_id} cancelled — killing process group"));
            kill_process_group(child_pid);
            // Give 3s for graceful exit, then force
            match tokio::time::timeout(std::time::Duration::from_secs(3), child.wait()).await {
                Ok(_) => {}
                _ => { child.kill().await.ok(); }
            }
            Err("Cancelled".to_string())
        }

        available_mb = kill_rx.recv() => {
            // Memory watchdog already sent SIGTERM to process group
            match tokio::time::timeout(std::time::Duration::from_secs(5), child.wait()).await {
                Ok(Ok(status)) => log.info(&format!("Process exited gracefully after SIGTERM: {status}")),
                _ => {
                    kill_process_group_force(child_pid);
                    child.kill().await.ok();
                }
            }
            let mb = available_mb.unwrap_or(0);
            Err(format!(
                "Killed by memory watchdog: only {}MB available (threshold: {}MB)",
                mb, CRITICAL_AVAILABLE_BYTES / (1024 * 1024)
            ))
        }

        status = child.wait() => {
            match status {
                Ok(exit_status) => {
                    let code = exit_status.code().unwrap_or(-1);
                    log.info(&format!("Sentinel {handle_id} exited with code {code}"));
                    Ok(code)
                }
                Err(e) => Err(format!("Process wait failed: {e}")),
            }
        }
    };

    // Stop watchdog (it may still be running if process exited normally)
    watchdog_task.abort();

    // Wait for pipe readers to finish draining
    let last_stdout = stdout_task.await.unwrap_or_default();
    stderr_task.await.ok();

    // Wait for log writer to flush all buffered lines
    let last_written = writer_task.await.unwrap_or_default();

    // Clean up PID file
    if let Err(e) = tokio::fs::remove_file(&pid_path).await {
        // ENOENT is fine — PID file may not have been written
        if e.kind() != std::io::ErrorKind::NotFound {
            log.warn(&format!("Failed to remove PID file {}: {e}", pid_path.display()));
        }
    }

    match result {
        Ok(code) => {
            let last = if last_written.is_empty() {
                last_stdout
            } else {
                last_written
            };
            Ok((code, last))
        }
        Err(e) => Err(e),
    }
}

/// Kill an entire process group via SIGTERM (graceful)
fn kill_process_group(pid: Option<u32>) {
    if let Some(pid) = pid {
        // setsid() made the child its own session leader, so pid == pgid
        unsafe {
            libc::kill(-(pid as i32), libc::SIGTERM);
        }
    }
}

/// Kill an entire process group via SIGKILL (forced)
fn kill_process_group_force(pid: Option<u32>) {
    if let Some(pid) = pid {
        unsafe {
            libc::kill(-(pid as i32), libc::SIGKILL);
        }
    }
}

/// A log line from a pipe reader task
enum LogLine {
    Stdout(String),
    Stderr(String),
}

/// Execute a pipeline directly (synchronous path, not spawned).
/// Used by the `sentinel/pipeline` command.
pub async fn execute_pipeline_direct(
    logs_base_dir: &Path,
    handle_id: &str,
    pipeline: Pipeline,
    bus: Option<&Arc<MessageBus>>,
    registry: Option<&Arc<ModuleRegistry>>,
) -> PipelineResult {
    let log = runtime::logger("sentinel");
    let start_time = Instant::now();

    let registry = match registry {
        Some(r) => r.clone(),
        None => {
            return PipelineResult {
                handle: handle_id.to_string(),
                success: false,
                total_duration_ms: 0,
                steps_completed: 0,
                steps_total: pipeline.steps.len(),
                step_results: Vec::new(),
                error: Some("SentinelModule not initialized - missing registry".to_string()),
            };
        }
    };

    let pipeline_name = pipeline.name.as_deref().unwrap_or("unnamed");
    log.info(&format!(
        "Starting pipeline '{}' (handle={}), {} steps",
        pipeline_name,
        handle_id,
        pipeline.steps.len()
    ));

    let working_dir = pipeline
        .working_dir
        .clone()
        .map(PathBuf::from)
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));

    let mut ctx = ExecutionContext {
        step_results: Vec::new(),
        inputs: pipeline.inputs.clone(),
        working_dir,
        named_outputs: HashMap::new(),
    };

    // Create logs directory
    let logs_dir = logs_base_dir.join(handle_id);
    if let Err(e) = tokio::fs::create_dir_all(&logs_dir).await {
        log.warn(&format!("Failed to create logs directory {}: {e}", logs_dir.display()));
    }

    let mut success = true;
    let mut error_msg: Option<String> = None;

    for (i, step) in pipeline.steps.iter().enumerate() {
        log.info(&format!(
            "[{}] Executing step {}: {:?}",
            handle_id,
            i,
            step_type_name(step)
        ));

        let pipeline_ctx = PipelineContext {
            handle_id,
            registry: &registry,
            bus,
            steps_log_path: None,
        };

        match steps::execute_step(step, i, &mut ctx, &pipeline_ctx).await {
            Ok(result) => {
                if !result.success {
                    log.warn(&format!(
                        "[{handle_id}] Step {i} failed: {:?}",
                        result.error
                    ));
                    success = false;
                    error_msg = result.error.clone();
                    ctx.step_results.push(result);
                    break;
                }
                ctx.step_results.push(result);
            }
            Err(e) => {
                log.error(&format!("[{handle_id}] Step {i} error: {e}"));
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

    // Emit completion event
    if let Some(bus) = bus {
        bus.publish_async_only(
            "sentinel:pipeline:complete",
            json!({
                "handle": handle_id,
                "name": pipeline_name,
                "success": success,
                "durationMs": total_duration_ms,
            }),
        );
    }

    log.info(&format!(
        "Pipeline '{pipeline_name}' completed: success={success}, duration={total_duration_ms}ms"
    ));

    PipelineResult {
        handle: handle_id.to_string(),
        success,
        total_duration_ms,
        steps_completed: ctx.step_results.len(),
        steps_total: pipeline.steps.len(),
        step_results: ctx.step_results,
        error: error_msg,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modules::sentinel::types::{Pipeline, PipelineStep};
    use crate::runtime::{message_bus::MessageBus, ModuleRegistry};
    use serde_json::json;
    use std::sync::Arc;

    fn make_registry() -> Arc<ModuleRegistry> {
        Arc::new(ModuleRegistry::new())
    }

    fn make_bus() -> Arc<MessageBus> {
        Arc::new(MessageBus::new())
    }

    /// Test a simple linear pipeline: echo a → echo b → echo c
    #[tokio::test]
    async fn test_linear_pipeline() {
        let registry = make_registry();
        let bus = make_bus();
        let logs_dir = std::env::temp_dir().join("sentinel-test-linear");

        let pipeline = Pipeline {
            name: Some("linear-test".to_string()),
            steps: vec![
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["step-a".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["step-b".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["step-c".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
            ],
            working_dir: Some("/tmp".to_string()),
            timeout_secs: None,
            inputs: HashMap::new(),
        };

        let result = execute_pipeline_direct(
            &logs_dir,
            "test-linear",
            pipeline,
            Some(&bus),
            Some(&registry),
        )
        .await;

        assert!(result.success);
        assert_eq!(result.steps_completed, 3);
        assert_eq!(result.steps_total, 3);
        assert_eq!(result.step_results[0].output.as_deref(), Some("step-a\n"));
        assert_eq!(result.step_results[1].output.as_deref(), Some("step-b\n"));
        assert_eq!(result.step_results[2].output.as_deref(), Some("step-c\n"));
        assert!(result.error.is_none());
    }

    /// Test pipeline stops on first failure
    #[tokio::test]
    async fn test_pipeline_stops_on_failure() {
        let registry = make_registry();
        let bus = make_bus();
        let logs_dir = std::env::temp_dir().join("sentinel-test-fail");

        let pipeline = Pipeline {
            name: Some("fail-test".to_string()),
            steps: vec![
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["ok".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
                PipelineStep::Shell {
                    cmd: "/bin/sh".into(),
                    args: vec!["-c".into(), "exit 42".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["never-reached".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
            ],
            working_dir: Some("/tmp".to_string()),
            timeout_secs: None,
            inputs: HashMap::new(),
        };

        let result = execute_pipeline_direct(
            &logs_dir,
            "test-fail",
            pipeline,
            Some(&bus),
            Some(&registry),
        )
        .await;

        assert!(!result.success);
        assert_eq!(result.steps_completed, 2); // echo ok + failing step
        assert_eq!(result.steps_total, 3);
        assert!(result.error.is_some());
    }

    /// Test pipeline with condition branching
    #[tokio::test]
    async fn test_pipeline_with_condition() {
        let registry = make_registry();
        let bus = make_bus();
        let logs_dir = std::env::temp_dir().join("sentinel-test-cond");

        let mut inputs = HashMap::new();
        inputs.insert("should_build".to_string(), json!("true"));

        let pipeline = Pipeline {
            name: Some("cond-test".to_string()),
            steps: vec![
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["start".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
                PipelineStep::Condition {
                    condition: "{{input.should_build}}".to_string(),
                    then_steps: vec![PipelineStep::Shell {
                        cmd: "echo".into(),
                        args: vec!["building".into()],
                        timeout_secs: Some(10),
                        working_dir: None,
                        allow_failure: None,
                        env: None,
                    }],
                    else_steps: vec![PipelineStep::Shell {
                        cmd: "echo".into(),
                        args: vec!["skipping".into()],
                        timeout_secs: Some(10),
                        working_dir: None,
                        allow_failure: None,
                        env: None,
                    }],
                },
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["done".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
            ],
            working_dir: Some("/tmp".to_string()),
            timeout_secs: None,
            inputs,
        };

        let result = execute_pipeline_direct(
            &logs_dir,
            "test-cond",
            pipeline,
            Some(&bus),
            Some(&registry),
        )
        .await;

        assert!(result.success);
        // step 0: echo start, step 1: condition (which runs echo building as substep), step 2: echo done
        // But the condition step pushes its substep results into the context
        // So we get: echo start → (echo building pushed by condition) → condition result → echo done
        assert!(result.steps_completed >= 3);
    }

    /// Test pipeline with loop and variable interpolation
    #[tokio::test]
    async fn test_pipeline_with_loop() {
        let registry = make_registry();
        let bus = make_bus();
        let logs_dir = std::env::temp_dir().join("sentinel-test-loop");

        let pipeline = Pipeline {
            name: Some("loop-test".to_string()),
            steps: vec![PipelineStep::Loop {
                count: Some(3),
                steps: vec![PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["iteration-{{input.iteration}}".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                }],
                while_condition: None,
                until: None,
                max_iterations: None,
            }],
            working_dir: Some("/tmp".to_string()),
            timeout_secs: None,
            inputs: HashMap::new(),
        };

        let result = execute_pipeline_direct(
            &logs_dir,
            "test-loop",
            pipeline,
            Some(&bus),
            Some(&registry),
        )
        .await;

        assert!(result.success);
    }

    /// Test pipeline with parallel branches
    #[tokio::test]
    async fn test_pipeline_with_parallel() {
        let registry = make_registry();
        let bus = make_bus();
        let logs_dir = std::env::temp_dir().join("sentinel-test-parallel");

        let pipeline = Pipeline {
            name: Some("parallel-test".to_string()),
            steps: vec![
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["before-fork".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
                PipelineStep::Parallel {
                    branches: vec![
                        vec![PipelineStep::Shell {
                            cmd: "echo".into(),
                            args: vec!["branch-a".into()],
                            timeout_secs: Some(10),
                            working_dir: None,
                            allow_failure: None,
                            env: None,
                        }],
                        vec![PipelineStep::Shell {
                            cmd: "echo".into(),
                            args: vec!["branch-b".into()],
                            timeout_secs: Some(10),
                            working_dir: None,
                            allow_failure: None,
                            env: None,
                        }],
                    ],
                    fail_fast: false,
                },
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["after-join".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
            ],
            working_dir: Some("/tmp".to_string()),
            timeout_secs: None,
            inputs: HashMap::new(),
        };

        let result =
            execute_pipeline_direct(&logs_dir, "test-par", pipeline, Some(&bus), Some(&registry))
                .await;

        assert!(result.success);
        assert_eq!(result.steps_total, 3);
    }

    /// Test emit + watch composition across spawned task
    #[tokio::test]
    async fn test_emit_watch_composition() {
        let registry = make_registry();
        let bus = make_bus();
        let logs_dir = std::env::temp_dir().join("sentinel-test-emit-watch");

        // Use parallel to run emit and watch concurrently — emit fires, watch catches
        let pipeline = Pipeline {
            name: Some("emit-watch-test".to_string()),
            steps: vec![PipelineStep::Parallel {
                branches: vec![
                    // Branch 0: small delay then emit
                    vec![
                        PipelineStep::Shell {
                            cmd: "sleep".into(),
                            args: vec!["0.1".into()],
                            timeout_secs: Some(5),
                            working_dir: None,
                            allow_failure: None,
                            env: None,
                        },
                        PipelineStep::Emit {
                            event: "test:signal".to_string(),
                            payload: json!({"msg": "hello"}),
                        },
                    ],
                    // Branch 1: watch for the event
                    vec![PipelineStep::Watch {
                        event: "test:signal".to_string(),
                        timeout_secs: Some(5),
                    }],
                ],
                fail_fast: false,
            }],
            working_dir: Some("/tmp".to_string()),
            timeout_secs: None,
            inputs: HashMap::new(),
        };

        let result =
            execute_pipeline_direct(&logs_dir, "test-ew", pipeline, Some(&bus), Some(&registry))
                .await;

        assert!(result.success);
    }

    /// Test nested sentinel step (pipeline within pipeline)
    #[tokio::test]
    async fn test_nested_sentinel_pipeline() {
        let registry = make_registry();
        let bus = make_bus();
        let logs_dir = std::env::temp_dir().join("sentinel-test-nested");

        let pipeline = Pipeline {
            name: Some("parent".to_string()),
            steps: vec![
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["parent-start".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
                PipelineStep::Sentinel {
                    pipeline: Box::new(Pipeline {
                        name: Some("child".to_string()),
                        steps: vec![
                            PipelineStep::Shell {
                                cmd: "echo".into(),
                                args: vec!["child-a".into()],
                                timeout_secs: Some(10),
                                working_dir: None,
                                allow_failure: None,
                                env: None,
                            },
                            PipelineStep::Shell {
                                cmd: "echo".into(),
                                args: vec!["child-b".into()],
                                timeout_secs: Some(10),
                                working_dir: None,
                                allow_failure: None,
                                env: None,
                            },
                        ],
                        working_dir: None,
                        timeout_secs: None,
                        inputs: HashMap::new(),
                    }),
                },
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["parent-end".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
            ],
            working_dir: Some("/tmp".to_string()),
            timeout_secs: None,
            inputs: HashMap::new(),
        };

        let result = execute_pipeline_direct(
            &logs_dir,
            "test-nested",
            pipeline,
            Some(&bus),
            Some(&registry),
        )
        .await;

        assert!(result.success);
        assert_eq!(result.steps_total, 3);
    }

    /// Test pipeline with variable forwarding between steps
    #[tokio::test]
    async fn test_step_output_forwarding() {
        let registry = make_registry();
        let bus = make_bus();
        let logs_dir = std::env::temp_dir().join("sentinel-test-fwd");

        let pipeline = Pipeline {
            name: Some("forward-test".to_string()),
            steps: vec![
                // Step 0: produce output
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["hello-from-step-0".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
                // Step 1: reference step 0's output via interpolation
                PipelineStep::Shell {
                    cmd: "echo".into(),
                    args: vec!["got: {{steps.0.data.stdout}}".into()],
                    timeout_secs: Some(10),
                    working_dir: None,
                    allow_failure: None,
                    env: None,
                },
            ],
            working_dir: Some("/tmp".to_string()),
            timeout_secs: None,
            inputs: HashMap::new(),
        };

        let result =
            execute_pipeline_direct(&logs_dir, "test-fwd", pipeline, Some(&bus), Some(&registry))
                .await;

        assert!(result.success);
        assert_eq!(result.steps_completed, 2);
        // Step 1 should have interpolated step 0's stdout
        let step1_output = result.step_results[1].output.as_deref().unwrap_or("");
        assert!(
            step1_output.contains("hello-from-step-0"),
            "Expected forwarded output, got: {step1_output}"
        );
    }

    /// Test empty pipeline succeeds
    #[tokio::test]
    async fn test_empty_pipeline() {
        let registry = make_registry();
        let bus = make_bus();
        let logs_dir = std::env::temp_dir().join("sentinel-test-empty");

        let pipeline = Pipeline {
            name: Some("empty".to_string()),
            steps: vec![],
            working_dir: Some("/tmp".to_string()),
            timeout_secs: None,
            inputs: HashMap::new(),
        };

        let result = execute_pipeline_direct(
            &logs_dir,
            "test-empty",
            pipeline,
            Some(&bus),
            Some(&registry),
        )
        .await;

        assert!(result.success);
        assert_eq!(result.steps_completed, 0);
        assert_eq!(result.steps_total, 0);
    }

    /// Test pipeline without registry returns error
    #[tokio::test]
    async fn test_pipeline_requires_registry() {
        let bus = make_bus();
        let logs_dir = std::env::temp_dir().join("sentinel-test-noreg");

        let pipeline = Pipeline {
            name: Some("no-reg".to_string()),
            steps: vec![PipelineStep::Shell {
                cmd: "echo".into(),
                args: vec!["test".into()],
                timeout_secs: Some(10),
                working_dir: None,
                allow_failure: None,
                env: None,
            }],
            working_dir: Some("/tmp".to_string()),
            timeout_secs: None,
            inputs: HashMap::new(),
        };

        let result =
            execute_pipeline_direct(&logs_dir, "test-noreg", pipeline, Some(&bus), None).await;

        assert!(!result.success);
        assert!(result.error.as_ref().unwrap().contains("registry"));
    }
}
