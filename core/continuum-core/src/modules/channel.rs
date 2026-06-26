//! ChannelModule — wraps per-persona ChannelRegistry + PersonaState DashMap state.
//!
//! Validates the ServiceModule trait handles stateful per-persona DashMap isolation —
//! together with CognitionModule, these two prove the most different pattern from
//! stateless HealthModule.
//!
//! Commands: NONE. The old `channel/*` command surface (enqueue/dequeue/status/
//! service-cycle{,-full}/clear/tick-config) was the retired TypeScript persona
//! loop's task-queue control plane — zero callers today, and service-cycle-full
//! drove the heuristic `fast_path_decision` slated for deletion. Those arms were
//! deleted (retire-as-you-go) rather than migrated onto the typed registry. The
//! live surface is the background `tick()` below: per-persona task polling +
//! self-task generation, a lifecycle concern, not a command.

use crate::persona::channel_items::TaskQueueItem;
use crate::persona::self_task_generator::SelfTaskGenerator;
use crate::persona::{ChannelRegistry, PersonaCognition, PersonaState};
use crate::runtime::{
    CommandResult, LateBound, ModuleConfig, ModuleContext, ModulePriority, ServiceModule,
};
use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;
use std::time::{Duration, Instant};
use ts_rs::TS;
use uuid::Uuid;

/// Configuration for the channel tick loop — exposed to TypeScript via ts-rs.
///
/// Controls how often the background tick fires and which responsibilities are enabled.
/// Adjustable at runtime via `channel/tick-config` command, allowing TypeScript to
/// tune scheduling for different scenarios (gaming = fast tick, idle = slow tick).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/runtime/ChannelTickConfig.ts"
)]
pub struct ChannelTickConfig {
    /// Tick interval in milliseconds (default: 60000 = 60s).
    /// Lower values = more responsive task polling, higher CPU.
    /// Gaming: 1000-5000ms. Background: 60000-120000ms.
    #[ts(type = "number")]
    pub tick_interval_ms: u64,
    /// Whether to poll pending tasks from the database each tick.
    pub task_poll_enabled: bool,
    /// Whether to generate self-tasks (memory consolidation, skill audit, etc).
    pub self_task_enabled: bool,
}

impl Default for ChannelTickConfig {
    fn default() -> Self {
        Self {
            tick_interval_ms: 60_000,
            task_poll_enabled: true,
            self_task_enabled: true,
        }
    }
}

/// Shared state for channel module — per-persona registries and states.
pub struct ChannelState {
    /// Per-persona channel registries + states.
    pub registries: Arc<DashMap<Uuid, (ChannelRegistry, PersonaState)>>,
    /// Unified per-persona cognition (shared with CognitionModule).
    /// Used for fast-path decision in service-cycle-full.
    pub personas: Arc<DashMap<Uuid, PersonaCognition>>,
    /// Per-persona self-task generators (lazily created on first tick).
    pub self_task_generators: DashMap<Uuid, tokio::sync::Mutex<SelfTaskGenerator>>,
    /// Tick configuration — adjustable at runtime via channel/tick-config command.
    pub tick_config: std::sync::RwLock<ChannelTickConfig>,
    /// Circuit breaker for DB-backed tick work. One failing Postgres path should
    /// not fan out into N personas × M queries every tick.
    pub db_tick_backoff: std::sync::Mutex<DbTickBackoff>,
}

#[derive(Debug, Default)]
pub struct DbTickBackoff {
    pub consecutive_failures: u32,
    pub backoff_until: Option<Instant>,
}

impl ChannelState {
    pub fn new(personas: Arc<DashMap<Uuid, PersonaCognition>>) -> Self {
        Self {
            registries: Arc::new(DashMap::new()),
            personas,
            self_task_generators: DashMap::new(),
            tick_config: std::sync::RwLock::new(ChannelTickConfig::default()),
            db_tick_backoff: std::sync::Mutex::new(DbTickBackoff::default()),
        }
    }

    /// Create from existing DashMaps (for gradual migration from ServerState).
    pub fn from_existing(
        registries: Arc<DashMap<Uuid, (ChannelRegistry, PersonaState)>>,
        personas: Arc<DashMap<Uuid, PersonaCognition>>,
    ) -> Self {
        Self {
            registries,
            personas,
            self_task_generators: DashMap::new(),
            tick_config: std::sync::RwLock::new(ChannelTickConfig::default()),
            db_tick_backoff: std::sync::Mutex::new(DbTickBackoff::default()),
        }
    }
}

pub struct ChannelModule {
    state: Arc<ChannelState>,
    executor: LateBound<crate::runtime::CommandExecutor>,
}

impl ChannelModule {
    pub fn new(state: Arc<ChannelState>) -> Self {
        Self {
            state,
            executor: LateBound::new("channel::executor"),
        }
    }

    /// Executor accessor for the tick body. Returns an error string when the
    /// executor hasn't been installed yet (boot race) so the tick can record
    /// it via `record_db_tick_failure` instead of panicking.
    fn executor_or_err(&self) -> Result<Arc<crate::runtime::CommandExecutor>, String> {
        self.executor
            .cloned()
            .ok_or_else(|| "channel tick: CommandExecutor not yet installed".to_string())
    }

    fn tick_db_handle_from_env(override_value: Option<String>) -> String {
        override_value
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "main".to_string())
    }

    fn tick_db_handle() -> String {
        Self::tick_db_handle_from_env(std::env::var("CONTINUUM_DB_URL").ok())
    }
}

#[async_trait]
impl ServiceModule for ChannelModule {
    fn config(&self) -> ModuleConfig {
        let tick_ms = self
            .state
            .tick_config
            .read()
            .map(|c| c.tick_interval_ms)
            .unwrap_or(60_000);
        ModuleConfig {
            name: "channel",
            priority: ModulePriority::High,
            command_prefixes: &["channel/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: Some(Duration::from_millis(tick_ms)),
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // RETIRED: the `channel/*` command surface (enqueue, dequeue, status,
        // service-cycle, service-cycle-full, clear, tick-config) was the old
        // TypeScript persona loop's task-queue control plane — it has ZERO callers
        // now that the loop is the Workspace/Faculty organism, and
        // service-cycle-full drove `fast_path_decision` (the heuristic gating slated
        // for deletion in task #9). Per the migration's retire-as-you-go curation
        // ([[command-migration-retire-as-you-go]]), the dead commands are deleted
        // rather than migrated onto the typed registry. The background `tick()`
        // (task polling + self-task generation) stays — it's a live lifecycle
        // concern, not a command. Fail loud on any stray invocation.
        Err(format!(
            "channel command surface is retired; '{command}' has no handler"
        ))
    }

    /// Periodic tick: runs ALL background work for ALL personas in one batch.
    /// Replaces 30+ TypeScript setIntervals (10 personas × 3 timers each) with ONE Rust tick.
    ///
    /// Work performed per tick:
    /// 1. Poll pending tasks from DB → enqueue into channel registries
    /// 2. Self-task generation (memory consolidation, skill audit, resume work, learning)
    /// 3. Training readiness checks (threshold → trigger genome/job-create via TS)
    ///
    /// Cadence controlled by ChannelTickConfig (adjustable via channel/tick-config).
    async fn tick(&self) -> Result<(), String> {
        let log = crate::runtime::logger("channel-tick");

        // Read config snapshot (cheap: std::sync::RwLock read, no contention)
        let config = self
            .state
            .tick_config
            .read()
            .map(|c| c.clone())
            .unwrap_or_default();

        // Use DataModule's main handle by default so fresh installs stay SQLite-first.
        // CONTINUUM_DB_URL remains an explicit deployment override.
        let db_path = Self::tick_db_handle();

        // Collect persona IDs to avoid holding DashMap ref across await
        let persona_ids: Vec<Uuid> = self
            .state
            .registries
            .iter()
            .map(|entry| *entry.key())
            .collect();

        if persona_ids.is_empty() {
            return Ok(());
        }

        if (config.task_poll_enabled || config.self_task_enabled) && self.should_skip_db_tick() {
            return Ok(());
        }

        let executor = match self.executor_or_err() {
            Ok(e) => e,
            Err(e) => {
                self.record_db_tick_failure(&e);
                return Ok(());
            }
        };
        let mut total_enqueued = 0u32;
        let mut total_self_tasks = 0u32;

        for persona_id in &persona_ids {
            // ── 1. Poll pending tasks ──────────────────────────────────────
            if config.task_poll_enabled {
                let query_result = executor
                    .execute_json(
                        "data/query",
                        serde_json::json!({
                            "dbPath": db_path,
                            "collection": "tasks",
                            "filter": {
                                "assigneeId": { "$eq": persona_id.to_string() },
                                "status": { "$eq": "pending" }
                            },
                            "limit": 10
                        }),
                    )
                    .await;

                match query_result {
                    Ok(result_json) => {
                        if let Some(records) = result_json.get("data").and_then(|d| d.as_array()) {
                            for record in records {
                                if let Some(item) =
                                    Self::record_to_task_queue_item(record, persona_id)
                                {
                                    if let Some(mut entry) =
                                        self.state.registries.get_mut(persona_id)
                                    {
                                        let (registry, _state) = entry.value_mut();
                                        if registry.route(std::sync::Arc::new(item)).is_ok() {
                                            total_enqueued += 1;
                                        }
                                    }
                                }
                            }
                        }
                    }
                    Err(e) => {
                        self.record_db_tick_failure(&format!("task poll failed: {e}"));
                        return Ok(());
                    }
                }
            }

            // ── 2. Self-task generation ────────────────────────────────────
            if config.self_task_enabled {
                // Ensure generator exists (lazy init)
                if !self.state.self_task_generators.contains_key(persona_id) {
                    self.state.self_task_generators.insert(
                        *persona_id,
                        tokio::sync::Mutex::new(SelfTaskGenerator::new(*persona_id)),
                    );
                }

                if let Some(generator_entry) = self.state.self_task_generators.get(persona_id) {
                    let mut generator = generator_entry.lock().await;
                    match generator.generate_and_persist(&db_path, &executor).await {
                        Ok(tasks) => {
                            let count = tasks.len() as u32;
                            if count > 0 {
                                for task_json in &tasks {
                                    if let Some(item) =
                                        Self::json_to_task_queue_item(task_json, persona_id)
                                    {
                                        if let Some(mut entry) =
                                            self.state.registries.get_mut(persona_id)
                                        {
                                            let (registry, _state) = entry.value_mut();
                                            let _ = registry.route(std::sync::Arc::new(item));
                                        }
                                    }
                                }
                                total_self_tasks += count;
                            }
                        }
                        Err(e) => {
                            self.record_db_tick_failure(&format!(
                                "self-task gen failed for {persona_id}: {e}"
                            ));
                            return Ok(());
                        }
                    }
                }
            }

            // ── 2b. Enrollment opportunity detection ─────────────────────
            // Uses genome coverage report to find domains with activity but no adapter.
            // Creates enroll-academy tasks when gap meets threshold.
            if config.self_task_enabled {
                if let Some(generator_entry) = self.state.self_task_generators.get(persona_id) {
                    let generator = generator_entry.lock().await;
                    if let Some(persona) = self.state.personas.get(persona_id) {
                        let enrollment_tasks =
                            generator.detect_enrollment_opportunities(&persona.genome_engine);
                        if !enrollment_tasks.is_empty() {
                            for task_json in &enrollment_tasks {
                                if let Some(item) =
                                    Self::json_to_task_queue_item(task_json, persona_id)
                                {
                                    if let Some(mut entry) =
                                        self.state.registries.get_mut(persona_id)
                                    {
                                        let (registry, _state) = entry.value_mut();
                                        let _ = registry.route(std::sync::Arc::new(item));
                                    }
                                }
                            }
                            total_self_tasks += enrollment_tasks.len() as u32;
                            log.info(&format!(
                                "Enrollment opportunities for {}: {} tasks",
                                persona_id,
                                enrollment_tasks.len()
                            ));
                        }
                    }
                }
            }

            // Training readiness check used to live here. Removed in
            // task #227 — the trigger was structurally dead:
            //
            // - It sent `{personaId, trainingExamples}` to the TS
            //   `genome/job-create` validator, which requires
            //   `provider` + `configuration`. Every fire-and-forget
            //   call silently rejected. The log line lied that it
            //   "triggered" training; nothing started.
            // - The TS path itself is cloud-provider-only (OpenAI /
            //   Fireworks / DeepSeek / Mistral / Together). The
            //   substrate's actual training story is local Candle +
            //   teacher-synthesized curricula + matrix-dojo layer
            //   paging — a fundamentally different shape that needs
            //   its own ServiceModule, not this fire-and-forget hop.
            //
            // When the substrate-native local training trigger
            // crystallizes (per the LoRA paging + matrix-dojo
            // doctrines) it lands as a typed `genome/*` ServiceModule
            // and the channel tick fires into THAT, not into a TS
            // command.
        }

        self.record_db_tick_success();

        if total_enqueued > 0 || total_self_tasks > 0 {
            log.info(&format!(
                "Tick: {} personas, polled {} tasks, generated {} self-tasks",
                persona_ids.len(),
                total_enqueued,
                total_self_tasks
            ));
        }

        Ok(())
    }

    fn install_executor(&self, executor: Arc<crate::runtime::CommandExecutor>) {
        self.executor.install(executor);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl ChannelModule {
    fn should_skip_db_tick(&self) -> bool {
        let Ok(backoff) = self.state.db_tick_backoff.lock() else {
            return false;
        };

        backoff
            .backoff_until
            .map(|until| Instant::now() < until)
            .unwrap_or(false)
    }

    fn record_db_tick_success(&self) {
        if let Ok(mut backoff) = self.state.db_tick_backoff.lock() {
            backoff.consecutive_failures = 0;
            backoff.backoff_until = None;
        }
    }

    fn record_db_tick_failure(&self, reason: &str) {
        let log = crate::runtime::logger("channel-tick");
        if let Ok(mut backoff) = self.state.db_tick_backoff.lock() {
            backoff.consecutive_failures = backoff.consecutive_failures.saturating_add(1);
            let delay_secs = match backoff.consecutive_failures {
                1 => 60,
                2 => 120,
                3 => 300,
                _ => 600,
            };
            backoff.backoff_until = Some(Instant::now() + Duration::from_secs(delay_secs));
            log.warn(&format!(
                "DB-backed tick disabled for {delay_secs}s after {} consecutive failure(s): {reason}",
                backoff.consecutive_failures
            ));
        } else {
            log.warn(&format!("DB-backed tick failed: {reason}"));
        }
    }

    /// Convert a DB record (from data/query result) to a TaskQueueItem.
    fn record_to_task_queue_item(record: &Value, persona_id: &Uuid) -> Option<TaskQueueItem> {
        let record_id = record
            .get("id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());
        let data = record.get("data")?;
        Self::data_to_task_queue_item(data, record_id, persona_id)
    }

    /// Convert a self-task JSON (from SelfTaskGenerator) to a TaskQueueItem.
    fn json_to_task_queue_item(task_json: &Value, persona_id: &Uuid) -> Option<TaskQueueItem> {
        let task_id = task_json
            .get("id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());
        Self::data_to_task_queue_item(task_json, task_id, persona_id)
    }

    /// Shared conversion logic: task data JSON → TaskQueueItem.
    fn data_to_task_queue_item(
        data: &Value,
        task_id: Option<Uuid>,
        persona_id: &Uuid,
    ) -> Option<TaskQueueItem> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        Some(TaskQueueItem {
            id: Uuid::new_v4(),
            task_id: task_id.unwrap_or_else(Uuid::new_v4),
            assignee_id: *persona_id,
            created_by: data
                .get("createdBy")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
                .unwrap_or(*persona_id),
            task_domain: data
                .get("domain")
                .and_then(|v| v.as_str())
                .unwrap_or("self")
                .to_string(),
            task_type: data
                .get("taskType")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            context_id: data
                .get("contextId")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
                .unwrap_or(*persona_id),
            description: data
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            priority: data.get("priority").and_then(|v| v.as_f64()).unwrap_or(0.5) as f32,
            status: "pending".to_string(),
            timestamp: data
                .get("timestamp")
                .and_then(|v| v.as_u64())
                .unwrap_or(now_ms),
            enqueued_at: now_ms,
            due_date: data.get("dueDate").and_then(|v| v.as_u64()),
            estimated_duration: data.get("estimatedDuration").and_then(|v| v.as_u64()),
            depends_on: Vec::new(),
            blocked_by: Vec::new(),
            related_task_ids: Vec::new(),
            consolidated_count: 1,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::ChannelModule;

    #[test]
    fn tick_db_handle_defaults_to_main() {
        assert_eq!(ChannelModule::tick_db_handle_from_env(None), "main");
    }

    #[test]
    fn tick_db_handle_ignores_blank_override() {
        assert_eq!(
            ChannelModule::tick_db_handle_from_env(Some("  ".to_string())),
            "main"
        );
    }

    #[test]
    fn tick_db_handle_preserves_explicit_override() {
        let db_url = "postgres://user@localhost:5432/continuum".to_string();

        assert_eq!(
            ChannelModule::tick_db_handle_from_env(Some(db_url.clone())),
            db_url
        );
    }
}
