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

pub mod executor;
pub mod interpolation;
pub mod logs;
pub mod steps;
pub mod types;

pub use types::*;

use async_trait::async_trait;
use dashmap::DashMap;
use parking_lot::RwLock;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use crate::runtime::{
    message_bus::MessageBus, CommandResult, ModuleConfig, ModuleContext, ModulePriority,
    ModuleRegistry, ServiceModule,
};
use crate::utils::params::Params;

/// Global sentinel module reference for shutdown from signal handlers.
/// Set during server startup, read during SIGTERM/SIGINT.
static GLOBAL_SENTINEL: std::sync::OnceLock<Arc<SentinelModule>> = std::sync::OnceLock::new();

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
            logs_base_dir: RwLock::new({
                let home = dirs::home_dir().expect("Failed to resolve home directory");
                home.join(".continuum")
                    .join("jtag")
                    .join("logs")
                    .join("system")
                    .join("sentinels")
            }),
            max_concurrent: 6,
            bus: RwLock::new(None),
            registry: RwLock::new(None),
        }
    }

    /// Generate a unique handle ID
    fn generate_handle_id() -> String {
        uuid::Uuid::new_v4().to_string()[..8].to_string()
    }

    /// Get logs directory for a handle
    fn logs_dir(&self, handle: &str) -> PathBuf {
        self.logs_base_dir.read().join(handle)
    }

    /// Minimum available system memory (bytes) required to start a new sentinel.
    /// Below this threshold, the system refuses new sentinels to prevent OOM crashes.
    /// 2GB headroom prevents macOS from running out of application memory when
    /// Candle model loads, LoRA training, and LLM inference stack up.
    const MIN_AVAILABLE_MEMORY_BYTES: u64 = 2 * 1024 * 1024 * 1024; // 2 GB

    /// Run a sentinel (async execution) — handles both shell commands and pipelines
    async fn run_sentinel(&self, params: Value) -> Result<CommandResult, String> {
        use crate::runtime;
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
        if let Ok(mem) = crate::runtime::command_executor::execute_json("system/memory", Value::Null).await {
            let available = mem.get("available_bytes")
                .and_then(|v| v.as_u64())
                .unwrap_or(u64::MAX);
            if available < Self::MIN_AVAILABLE_MEMORY_BYTES {
                let available_gb = available as f64 / (1024.0 * 1024.0 * 1024.0);
                let threshold_gb = Self::MIN_AVAILABLE_MEMORY_BYTES as f64 / (1024.0 * 1024.0 * 1024.0);
                return Err(format!(
                    "Insufficient system memory: {:.1}GB available, {:.1}GB required. \
                     Cancel existing sentinels or wait for completion.",
                    available_gb, threshold_gb
                ));
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
        let timeout_secs = p.u64_or("timeout", 600);
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
            start_time: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
            end_time: None,
            exit_code: None,
            error: None,
            working_dir: working_dir.to_string_lossy().to_string(),
            logs_dir: logs_dir.to_string_lossy().to_string(),
        };

        // Parse escalation metadata (if caller wants persona inbox routing)
        let escalation =
            if p.str_opt("parentPersonaId").is_some() || p.str_opt("entityId").is_some() {
                Some(SentinelEscalation {
                    parent_persona_id: p.str_opt("parentPersonaId").map(|s| s.to_string()),
                    entity_id: p.str_opt("entityId").map(|s| s.to_string()),
                    sentinel_name: p.str_or("sentinelName", "unnamed").to_string(),
                    escalation_rules: p.json_opt("escalationRules"),
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

            // Execute based on type
            let result: Result<(i32, String), String> = if let Some(pipeline) = pipeline {
                log.info(&format!(
                    "[{handle_id_clone}] Executing pipeline with {} steps",
                    pipeline.steps.len()
                ));

                tokio::time::timeout(
                    Duration::from_secs(timeout_secs),
                    executor::execute_pipeline(
                        logs_base_dir.clone(),
                        pipeline,
                        handle_id_clone.clone(),
                        working_dir_clone.clone(),
                        bus.clone(),
                        registry.clone(),
                    ),
                )
                .await
                .map_err(|_| format!("Pipeline timeout after {timeout_secs}s"))
                .and_then(|r| r)
            } else {
                tokio::time::timeout(
                    Duration::from_secs(timeout_secs),
                    executor::execute_isolated(
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
                    ),
                )
                .await
                .map_err(|_| format!("Timeout after {timeout_secs}s"))
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

                // Push completion to TypeScript for persona escalation.
                // Rust owns the lifecycle — TS just receives and routes.
                if let Some(ref esc) = escalation_clone {
                    let escalation_payload = json!({
                        "handle": handle_id_clone,
                        "status": final_status,
                        "durationMs": duration_ms,
                        "error": error_msg,
                        "parentPersonaId": esc.parent_persona_id,
                        "entityId": esc.entity_id,
                        "sentinelName": esc.sentinel_name,
                        "escalationRules": esc.escalation_rules,
                    });
                    // Fire-and-forget — escalation failure shouldn't block sentinel cleanup
                    let _ = crate::runtime::command_executor::execute_ts_json(
                        "sentinel/escalate",
                        escalation_payload,
                    )
                    .await;
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
    async fn get_status(&self, params: Value) -> Result<CommandResult, String> {
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
    async fn list_handles(&self, _params: Value) -> Result<CommandResult, String> {
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
    async fn cancel_sentinel(&self, params: Value) -> Result<CommandResult, String> {
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
    async fn await_sentinel(&self, params: Value) -> Result<CommandResult, String> {
        let p = Params::new(&params);
        let handle_id = p.str("handle")?;
        let timeout_secs = p.u64_or("timeout", 600);

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
    async fn execute_pipeline_command(&self, params: Value) -> Result<CommandResult, String> {
        let handle_id = Self::generate_handle_id();

        let p = Params::new(&params);
        let pipeline: Pipeline = p.json("pipeline").or_else(|_| {
            serde_json::from_value::<Pipeline>(params.clone())
                .map_err(|e| format!("Failed to parse pipeline: {e}"))
        })?;

        let logs_base_dir = self.logs_base_dir.read().clone();
        let bus = self.bus.read().clone();
        let registry = self.registry.read().clone();

        let result = executor::execute_pipeline_direct(
            &logs_base_dir,
            &handle_id,
            pipeline,
            bus.as_ref(),
            registry.as_ref(),
        )
        .await;

        Ok(CommandResult::Json(
            serde_json::to_value(&result).unwrap_or(json!({"error": "serialization failed"})),
        ))
    }
}

/// Reap dead sentinels older than this from the registry
const REAP_AGE_SECS: u64 = 3600; // 1 hour

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
        let log = crate::runtime::logger("sentinel");
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
                        unsafe {
                            libc::kill(-pid, libc::SIGTERM);
                        }
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

    /// Reap completed/failed sentinels older than REAP_AGE_SECS from the registry.
    /// Prevents the DashMap from growing forever.
    pub fn reap_dead(&self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let mut reaped = Vec::new();
        for entry in self.sentinels.iter() {
            if entry.handle.status != SentinelStatus::Running {
                if let Some(end_time) = entry.handle.end_time {
                    if now.saturating_sub(end_time) > REAP_AGE_SECS * 1000 {
                        reaped.push(entry.key().clone());
                    }
                }
            }
        }

        for handle_id in &reaped {
            self.sentinels.remove(handle_id);
        }

        if !reaped.is_empty() {
            let log = crate::runtime::logger("sentinel");
            log.info(&format!("Reaped {} dead sentinels", reaped.len()));
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
        let log = crate::runtime::logger("sentinel");
        log.info("SentinelModule initialized with pipeline support");

        *self.bus.write() = Some(Arc::clone(&ctx.bus));
        *self.registry.write() = Some(Arc::clone(&ctx.registry));

        // logs_base_dir already set to $HOME/.continuum/... in new() — no cwd override needed

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
            "sentinel/logs/list" => logs::list_logs(&logs_base_dir, params).await,
            "sentinel/logs/read" => logs::read_log(&logs_base_dir, params).await,
            "sentinel/logs/tail" => logs::tail_log(&logs_base_dir, params).await,
            _ => Err(format!("Unknown sentinel command: {command}")),
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
