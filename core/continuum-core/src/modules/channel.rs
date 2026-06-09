//! ChannelModule — wraps per-persona ChannelRegistry + PersonaState DashMap state.
//!
//! Validates the ServiceModule trait handles stateful per-persona DashMap isolation —
//! together with CognitionModule, these two prove the most different pattern from
//! stateless HealthModule.
//!
//! Handles: channel/enqueue, channel/dequeue, channel/status,
//!          channel/service-cycle, channel/service-cycle-full, channel/clear

use crate::log_info;
use crate::logging::TimingGuard;
use crate::persona::channel_items::TaskQueueItem;
use crate::persona::channel_types::DOMAIN_PRIORITY_ORDER;
use crate::persona::self_task_generator::SelfTaskGenerator;
use crate::persona::{
    ActivityDomain, ChannelEnqueueRequest, ChannelRegistry, InboxMessage, Modality,
    PersonaCognition, PersonaState, SenderType,
};
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::utils::params::Params;
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
    executor: std::sync::OnceLock<Arc<crate::runtime::CommandExecutor>>,
}

impl ChannelModule {
    pub fn new(state: Arc<ChannelState>) -> Self {
        Self {
            state,
            executor: std::sync::OnceLock::new(),
        }
    }

    /// Executor accessor for the tick body. Returns an error string when the
    /// executor hasn't been installed yet (boot race) so the tick can record
    /// it via `record_db_tick_failure` instead of panicking.
    fn executor_or_err(&self) -> Result<Arc<crate::runtime::CommandExecutor>, String> {
        self.executor
            .get()
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

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        let p = Params::new(&params);

        match command {
            "channel/enqueue" => {
                let _timer = TimingGuard::new("module", "channel_enqueue");
                let persona_uuid = p.uuid("persona_id")?;
                let item = p.value("item").ok_or("Missing item")?;

                // Parse the item as ChannelEnqueueRequest
                let enqueue_request: ChannelEnqueueRequest =
                    serde_json::from_value(item.clone())
                        .map_err(|e| format!("Invalid item: {e}"))?;

                let queue_item = enqueue_request.to_queue_item()?;

                let mut entry = self
                    .state
                    .registries
                    .entry(persona_uuid)
                    .or_insert_with(|| (ChannelRegistry::new(), PersonaState::new()));
                let (registry, _state) = entry.value_mut();

                match registry.route(queue_item) {
                    Ok(domain) => {
                        let status = registry.status();
                        Ok(CommandResult::Json(serde_json::json!({
                            "routed_to": domain,
                            "status": status,
                        })))
                    }
                    Err(e) => Err(e),
                }
            }

            "channel/dequeue" => {
                let _timer = TimingGuard::new("module", "channel_dequeue");
                let persona_uuid = p.uuid("persona_id")?;
                let domain_str = p.str_opt("domain");

                let mut entry = match self.state.registries.get_mut(&persona_uuid) {
                    Some(r) => r,
                    None => return Err(format!("No channel registry for {persona_uuid}")),
                };
                let (registry, _state) = entry.value_mut();

                // Parse optional domain filter
                let target_domain: Option<ActivityDomain> = match domain_str {
                    Some(d) => {
                        let domain: ActivityDomain =
                            serde_json::from_value(serde_json::json!(d))
                                .map_err(|e| format!("Invalid domain '{d}': {e}"))?;
                        Some(domain)
                    }
                    None => None,
                };

                let item = match target_domain {
                    Some(d) => registry.get_mut(d).and_then(|ch| ch.pop()),
                    None => {
                        // Pop from highest-priority channel that has work
                        let mut popped = None;
                        for &d in DOMAIN_PRIORITY_ORDER {
                            if let Some(ch) = registry.get_mut(d) {
                                if let Some(item) = ch.pop() {
                                    popped = Some(item);
                                    break;
                                }
                            }
                        }
                        popped
                    }
                };

                match item {
                    Some(queue_item) => {
                        let json = queue_item.to_json();
                        Ok(CommandResult::Json(serde_json::json!({
                            "item": json,
                            "dequeued": true,
                        })))
                    }
                    None => Ok(CommandResult::Json(serde_json::json!({
                        "item": null,
                        "dequeued": false,
                    }))),
                }
            }

            "channel/status" => {
                let _timer = TimingGuard::new("module", "channel_status");
                let persona_uuid = p.uuid("persona_id")?;

                let entry = match self.state.registries.get(&persona_uuid) {
                    Some(r) => r,
                    None => {
                        // Return empty status if no registry exists yet
                        return Ok(CommandResult::Json(serde_json::json!({
                            "channels": [],
                            "total_size": 0,
                            "has_urgent_work": false,
                            "has_work": false,
                        })));
                    }
                };
                let (registry, _state) = entry.value();

                let status = registry.status();
                Ok(CommandResult::Json(
                    serde_json::to_value(&status).unwrap_or_default(),
                ))
            }

            "channel/service-cycle" => {
                let _timer = TimingGuard::new("module", "channel_service_cycle");
                let persona_uuid = p.uuid("persona_id")?;

                let mut entry = self
                    .state
                    .registries
                    .entry(persona_uuid)
                    .or_insert_with(|| (ChannelRegistry::new(), PersonaState::new()));
                let (registry, state) = entry.value_mut();

                let result = registry.service_cycle(state);
                Ok(CommandResult::Json(
                    serde_json::to_value(&result).unwrap_or_default(),
                ))
            }

            "channel/service-cycle-full" => {
                let _timer = TimingGuard::new("module", "channel_service_cycle_full");
                let persona_uuid = p.uuid("persona_id")?;

                // Step 1: Service cycle — consolidate, schedule, return next item
                let service_result = {
                    let mut entry = self
                        .state
                        .registries
                        .entry(persona_uuid)
                        .or_insert_with(|| (ChannelRegistry::new(), PersonaState::new()));
                    let (registry, state) = entry.value_mut();
                    registry.service_cycle(state)
                };

                // Step 2: If item returned, run fast_path_decision in the SAME call
                let decision = if service_result.should_process {
                    if let Some(ref item_json) = service_result.item {
                        // Reconstruct InboxMessage from queue item JSON using Params
                        let ip = Params::new(item_json);
                        let inbox_msg = InboxMessage {
                            id: ip.uuid_opt("id").unwrap_or_default(),
                            room_id: ip.uuid_opt("roomId").unwrap_or_default(),
                            sender_id: ip.uuid_opt("senderId").unwrap_or_default(),
                            sender_name: ip.str_or("senderName", "Unknown").to_string(),
                            sender_type: match ip.str_or("senderType", "human") {
                                "persona" => SenderType::Persona,
                                "agent" => SenderType::Agent,
                                "system" => SenderType::System,
                                _ => SenderType::Human,
                            },
                            content: ip.str_or("content", "").to_string(),
                            timestamp: ip.u64_or("timestamp", 0),
                            priority: ip.f32_or("priority", 0.5),
                            source_modality: ip.str_opt("itemType").and_then(|t| {
                                if t == "voice" {
                                    Some(Modality::Voice)
                                } else {
                                    None
                                }
                            }),
                            voice_session_id: ip.uuid_opt("voiceSessionId"),
                        };

                        // Get cognition engine for fast-path decision
                        if let Some(persona) = self.state.personas.get(&persona_uuid) {
                            let decision = persona.engine.fast_path_decision(&inbox_msg);
                            Some(serde_json::json!({
                                "should_respond": decision.should_respond,
                                "confidence": decision.confidence,
                                "reason": decision.reason,
                                "decision_time_ms": decision.decision_time_ms,
                                "fast_path_used": decision.fast_path_used,
                            }))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                } else {
                    None
                };

                // Return flat structure matching TypeScript's expected format
                Ok(CommandResult::Json(serde_json::json!({
                    "should_process": service_result.should_process,
                    "item": service_result.item,
                    "channel": service_result.channel,
                    "wait_ms": service_result.wait_ms,
                    "stats": service_result.stats,
                    "decision": decision,
                })))
            }

            "channel/clear" => {
                let _timer = TimingGuard::new("module", "channel_clear");
                let persona_uuid = p.uuid("persona_id")?;

                if let Some(mut entry) = self.state.registries.get_mut(&persona_uuid) {
                    let (registry, _state) = entry.value_mut();
                    registry.clear_all();
                }

                log_info!("module", "channel", "Cleared channels for {}", persona_uuid);
                Ok(CommandResult::Json(serde_json::json!({ "cleared": true })))
            }

            "channel/tick-config" => {
                let _timer = TimingGuard::new("module", "channel_tick_config");

                // If params include config fields, update the tick config
                let has_updates = params.get("tick_interval_ms").is_some()
                    || params.get("task_poll_enabled").is_some()
                    || params.get("self_task_enabled").is_some();

                if has_updates {
                    if let Ok(mut config) = self.state.tick_config.write() {
                        if let Some(v) = params.get("tick_interval_ms").and_then(|v| v.as_u64()) {
                            config.tick_interval_ms = v.max(100); // Floor: 100ms
                        }
                        if let Some(v) = params.get("task_poll_enabled").and_then(|v| v.as_bool()) {
                            config.task_poll_enabled = v;
                        }
                        if let Some(v) = params.get("self_task_enabled").and_then(|v| v.as_bool()) {
                            config.self_task_enabled = v;
                        }
                        log_info!("module", "channel", "Tick config updated: {:?}", *config);
                    }
                }

                // Return current config
                let config = self
                    .state
                    .tick_config
                    .read()
                    .map(|c| c.clone())
                    .unwrap_or_default();
                Ok(CommandResult::Json(
                    serde_json::to_value(&config).unwrap_or_else(|_| serde_json::json!({})),
                ))
            }

            _ => Err(format!("Unknown channel command: {command}")),
        }
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
                                        if registry.route(Box::new(item)).is_ok() {
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
                                            let _ = registry.route(Box::new(item));
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
                                        let _ = registry.route(Box::new(item));
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
        let _ = self.executor.set(executor);
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
