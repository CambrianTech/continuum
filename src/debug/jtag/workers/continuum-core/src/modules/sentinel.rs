//! Sentinel Module — Concurrent, fault-tolerant build/task execution
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

use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;
use tokio::sync::mpsc;
use ts_rs::TS;

use crate::runtime::{
    CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule,
    message_bus::MessageBus,
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

/// Internal state for a running sentinel
struct RunningSentinel {
    handle: SentinelHandle,
    /// Channel to send cancellation signal
    cancel_tx: Option<mpsc::Sender<()>>,
}

/// Sentinel Module - manages concurrent sentinel execution
pub struct SentinelModule {
    /// Active sentinels by handle ID
    sentinels: Arc<DashMap<String, RunningSentinel>>,
    /// Base directory for sentinel workspaces
    workspaces_dir: RwLock<PathBuf>,
    /// Maximum concurrent sentinels
    max_concurrent: usize,
    /// Message bus for event emission (set during initialize)
    bus: RwLock<Option<Arc<MessageBus>>>,
}

impl SentinelModule {
    pub fn new() -> Self {
        Self {
            sentinels: Arc::new(DashMap::new()),
            workspaces_dir: RwLock::new(PathBuf::from(".sentinel-workspaces")),
            max_concurrent: 4, // Conservative default - can be tuned
            bus: RwLock::new(None),
        }
    }

    /// Generate a unique handle ID
    fn generate_handle_id() -> String {
        uuid::Uuid::new_v4().to_string()[..8].to_string()
    }

    /// Get workspace directory for a handle
    fn workspace_dir(&self, handle: &str) -> PathBuf {
        self.workspaces_dir.read().join(handle)
    }

    /// Get logs directory for a handle
    fn logs_dir(&self, handle: &str) -> PathBuf {
        self.workspace_dir(handle).join("logs")
    }

    /// Run a sentinel (async execution)
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

        log.info(&format!(
            "Starting sentinel {} (type={}, cmd={} {:?})",
            handle_id, sentinel_type, command, args
        ));

        // Spawn execution task
        let sentinels = Arc::clone(&self.sentinels);
        let handle_id_clone = handle_id.clone();
        let working_dir_clone = working_dir.clone();
        let sentinel_type_clone = sentinel_type.clone();

        // Clone self fields needed for the task
        let workspaces_dir = self.workspaces_dir.read().clone();
        let bus = self.bus.read().clone();

        tokio::spawn(async move {
            let log = runtime::logger("sentinel");

            // Emit start event
            if let Some(ref bus) = bus {
                bus.publish_async_only(&format!("sentinel:{}:status", handle_id_clone), json!({
                    "handle": handle_id_clone,
                    "type": sentinel_type_clone,
                    "status": "running",
                    "phase": "starting",
                }));
            }

            // Execute with timeout
            let result = tokio::time::timeout(
                Duration::from_secs(timeout_secs),
                Self::execute_isolated_static(
                    workspaces_dir,
                    handle_id_clone.clone(),
                    command,
                    args,
                    working_dir_clone,
                    env,
                    cancel_rx,
                    bus.clone(),
                ),
            )
            .await;

            // Update handle status
            if let Some(mut entry) = sentinels.get_mut(&handle_id_clone) {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;

                let (final_status, error_msg) = match result {
                    Ok(Ok((exit_code, _output))) => {
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
                    Ok(Err(e)) => {
                        entry.handle.status = if e == "Cancelled" {
                            SentinelStatus::Cancelled
                        } else {
                            SentinelStatus::Failed
                        };
                        entry.handle.error = Some(e.clone());
                        log.error(&format!("Sentinel {} failed: {}", handle_id_clone, e));
                        (if e == "Cancelled" { "cancelled" } else { "failed" }, Some(e))
                    }
                    Err(_) => {
                        entry.handle.status = SentinelStatus::Failed;
                        entry.handle.error = Some(format!("Timeout after {}s", timeout_secs));
                        log.error(&format!("Sentinel {} timed out after {}s", handle_id_clone, timeout_secs));
                        ("failed", Some(format!("Timeout after {}s", timeout_secs)))
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

    /// Static version of execute_isolated for use in spawned tasks
    async fn execute_isolated_static(
        workspaces_dir: PathBuf,
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

        // Create logs directory
        let logs_dir = workspaces_dir.join(&handle_id).join("logs");
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
            if path.extension().map(|e| e == "log").unwrap_or(false) {
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

        let log_path = self.logs_dir(handle_id).join(format!("{}.log", stream));

        if !log_path.exists() {
            return Err(format!("Log stream not found: {}", stream));
        }

        let content = tokio::fs::read_to_string(&log_path)
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

        let log_path = self.logs_dir(handle_id).join(format!("{}.log", stream));

        if !log_path.exists() {
            return Err(format!("Log stream not found: {}", stream));
        }

        let content = tokio::fs::read_to_string(&log_path)
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
        log.info("SentinelModule initialized");

        // Store the message bus for event emission
        *self.bus.write() = Some(Arc::clone(&ctx.bus));

        // Set workspaces directory relative to current directory
        // In production, this will be the JTAG root (where npm start runs)
        if let Ok(cwd) = std::env::current_dir() {
            *self.workspaces_dir.write() = cwd.join(".sentinel-workspaces");
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
