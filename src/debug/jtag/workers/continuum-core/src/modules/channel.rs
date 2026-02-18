//! ChannelModule — wraps per-persona ChannelRegistry + PersonaState DashMap state.
//!
//! Validates the ServiceModule trait handles stateful per-persona DashMap isolation —
//! together with CognitionModule, these two prove the most different pattern from
//! stateless HealthModule.
//!
//! Handles: channel/enqueue, channel/dequeue, channel/status,
//!          channel/service-cycle, channel/service-cycle-full, channel/clear

use crate::runtime::{ServiceModule, ModuleConfig, ModulePriority, CommandResult, ModuleContext};
use crate::persona::{
    ChannelRegistry, PersonaState, ChannelEnqueueRequest, ActivityDomain,
    PersonaCognition, InboxMessage, SenderType, Modality,
};
use crate::persona::channel_types::DOMAIN_PRIORITY_ORDER;
use crate::persona::channel_items::TaskQueueItem;
use crate::persona::self_task_generator::SelfTaskGenerator;
use crate::logging::TimingGuard;
use crate::utils::params::Params;
use crate::log_info;
use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;
use std::time::Duration;
use ts_rs::TS;
use uuid::Uuid;

/// Configuration for the channel tick loop — exposed to TypeScript via ts-rs.
///
/// Controls how often the background tick fires and which responsibilities are enabled.
/// Adjustable at runtime via `channel/tick-config` command, allowing TypeScript to
/// tune scheduling for different scenarios (gaming = fast tick, idle = slow tick).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/runtime/ChannelTickConfig.ts")]
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
    /// Whether to check training data readiness each tick.
    pub training_check_enabled: bool,
    /// Training data threshold before triggering genome/job-create (default: 50).
    #[ts(type = "number")]
    pub training_threshold: u64,
}

impl Default for ChannelTickConfig {
    fn default() -> Self {
        Self {
            tick_interval_ms: 60_000,
            task_poll_enabled: true,
            self_task_enabled: true,
            training_check_enabled: true,
            training_threshold: 50,
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
}

impl ChannelState {
    pub fn new(personas: Arc<DashMap<Uuid, PersonaCognition>>) -> Self {
        Self {
            registries: Arc::new(DashMap::new()),
            personas,
            self_task_generators: DashMap::new(),
            tick_config: std::sync::RwLock::new(ChannelTickConfig::default()),
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
        }
    }
}

pub struct ChannelModule {
    state: Arc<ChannelState>,
}

impl ChannelModule {
    pub fn new(state: Arc<ChannelState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl ServiceModule for ChannelModule {
    fn config(&self) -> ModuleConfig {
        let tick_ms = self.state.tick_config.read()
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

    async fn handle_command(
        &self,
        command: &str,
        params: Value,
    ) -> Result<CommandResult, String> {
        let p = Params::new(&params);

        match command {
            "channel/enqueue" => {
                let _timer = TimingGuard::new("module", "channel_enqueue");
                let persona_uuid = p.uuid("persona_id")?;
                let item = p.value("item").ok_or("Missing item")?;

                // Parse the item as ChannelEnqueueRequest
                let enqueue_request: ChannelEnqueueRequest = serde_json::from_value(item.clone())
                    .map_err(|e| format!("Invalid item: {e}"))?;

                let queue_item = enqueue_request.to_queue_item()?;

                let mut entry = self.state.registries
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
                        let domain: ActivityDomain = serde_json::from_value(serde_json::json!(d))
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
                    None => {
                        Ok(CommandResult::Json(serde_json::json!({
                            "item": null,
                            "dequeued": false,
                        })))
                    }
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
                Ok(CommandResult::Json(serde_json::to_value(&status).unwrap_or_default()))
            }

            "channel/service-cycle" => {
                let _timer = TimingGuard::new("module", "channel_service_cycle");
                let persona_uuid = p.uuid("persona_id")?;

                let mut entry = self.state.registries
                    .entry(persona_uuid)
                    .or_insert_with(|| (ChannelRegistry::new(), PersonaState::new()));
                let (registry, state) = entry.value_mut();

                let result = registry.service_cycle(state);
                Ok(CommandResult::Json(serde_json::to_value(&result).unwrap_or_default()))
            }

            "channel/service-cycle-full" => {
                let _timer = TimingGuard::new("module", "channel_service_cycle_full");
                let persona_uuid = p.uuid("persona_id")?;

                // Step 1: Service cycle — consolidate, schedule, return next item
                let service_result = {
                    let mut entry = self.state.registries
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
                            source_modality: ip.str_opt("itemType")
                                .and_then(|t| if t == "voice" { Some(Modality::Voice) } else { None }),
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
                    || params.get("self_task_enabled").is_some()
                    || params.get("training_check_enabled").is_some()
                    || params.get("training_threshold").is_some();

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
                        if let Some(v) = params.get("training_check_enabled").and_then(|v| v.as_bool()) {
                            config.training_check_enabled = v;
                        }
                        if let Some(v) = params.get("training_threshold").and_then(|v| v.as_u64()) {
                            config.training_threshold = v;
                        }
                        log_info!("module", "channel", "Tick config updated: {:?}", *config);
                    }
                }

                // Return current config
                let config = self.state.tick_config.read()
                    .map(|c| c.clone())
                    .unwrap_or_default();
                Ok(CommandResult::Json(serde_json::to_value(&config)
                    .unwrap_or_else(|_| serde_json::json!({}))))
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
        let config = self.state.tick_config.read()
            .map(|c| c.clone())
            .unwrap_or_default();

        // Resolve db_path once per tick (HOME-relative, same as TypeScript ServerConfig)
        let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
        let db_path = format!("{home}/.continuum/data/database.sqlite");

        // Collect persona IDs to avoid holding DashMap ref across await
        let persona_ids: Vec<Uuid> = self.state.registries.iter()
            .map(|entry| *entry.key())
            .collect();

        if persona_ids.is_empty() {
            return Ok(());
        }

        let executor = crate::runtime::command_executor::executor();
        let mut total_enqueued = 0u32;
        let mut total_self_tasks = 0u32;

        for persona_id in &persona_ids {
            // ── 1. Poll pending tasks ──────────────────────────────────────
            if config.task_poll_enabled {
                let query_result = executor.execute_json("data/query", serde_json::json!({
                    "dbPath": db_path,
                    "collection": "tasks",
                    "filter": {
                        "assigneeId": { "$eq": persona_id.to_string() },
                        "status": { "$eq": "pending" }
                    },
                    "limit": 10
                })).await;

                if let Ok(result_json) = query_result {
                    if let Some(records) = result_json.get("data").and_then(|d| d.as_array()) {
                        for record in records {
                            if let Some(item) = Self::record_to_task_queue_item(record, persona_id) {
                                if let Some(mut entry) = self.state.registries.get_mut(persona_id) {
                                    let (registry, _state) = entry.value_mut();
                                    if registry.route(Box::new(item)).is_ok() {
                                        total_enqueued += 1;
                                    }
                                }
                            }
                        }
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

                if let Some(gen_entry) = self.state.self_task_generators.get(persona_id) {
                    let mut gen = gen_entry.lock().await;
                    match gen.generate_and_persist(&db_path, &executor).await {
                        Ok(tasks) => {
                            let count = tasks.len() as u32;
                            if count > 0 {
                                for task_json in &tasks {
                                    if let Some(item) = Self::json_to_task_queue_item(task_json, persona_id) {
                                        if let Some(mut entry) = self.state.registries.get_mut(persona_id) {
                                            let (registry, _state) = entry.value_mut();
                                            let _ = registry.route(Box::new(item));
                                        }
                                    }
                                }
                                total_self_tasks += count;
                            }
                        }
                        Err(e) => log.warn(&format!("Self-task gen failed for {}: {}", persona_id, e)),
                    }
                }
            }

            // ── 2b. Enrollment opportunity detection ─────────────────────
            // Uses genome coverage report to find domains with activity but no adapter.
            // Creates enroll-academy tasks when gap meets threshold.
            if config.self_task_enabled {
                if let Some(gen_entry) = self.state.self_task_generators.get(persona_id) {
                    let gen = gen_entry.lock().await;
                    if let Some(persona) = self.state.personas.get(persona_id) {
                        let enrollment_tasks = gen.detect_enrollment_opportunities(&persona.genome_engine);
                        if !enrollment_tasks.is_empty() {
                            for task_json in &enrollment_tasks {
                                if let Some(item) = Self::json_to_task_queue_item(task_json, persona_id) {
                                    if let Some(mut entry) = self.state.registries.get_mut(persona_id) {
                                        let (registry, _state) = entry.value_mut();
                                        let _ = registry.route(Box::new(item));
                                    }
                                }
                            }
                            total_self_tasks += enrollment_tasks.len() as u32;
                            log.info(&format!("Enrollment opportunities for {}: {} tasks", persona_id, enrollment_tasks.len()));
                        }
                    }
                }
            }

            // ── 3. Training readiness check ────────────────────────────────
            if config.training_check_enabled {
                let training_result = executor.execute_json("data/count", serde_json::json!({
                    "dbPath": db_path,
                    "collection": "training_data",
                    "filter": {
                        "personaId": { "$eq": persona_id.to_string() },
                        "consumed": { "$eq": false }
                    }
                })).await;

                if let Ok(count_json) = training_result {
                    let count = count_json.get("data")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);

                    if count >= config.training_threshold {
                        log.info(&format!("Training threshold met for {} ({} examples), triggering genome/job-create", persona_id, count));
                        let _ = crate::runtime::command_executor::execute_ts_json(
                            "genome/job-create",
                            serde_json::json!({
                                "personaId": persona_id.to_string(),
                                "trainingExamples": count,
                            }),
                        ).await;
                    }
                }
            }
        }

        if total_enqueued > 0 || total_self_tasks > 0 {
            log.info(&format!(
                "Tick: {} personas, polled {} tasks, generated {} self-tasks",
                persona_ids.len(), total_enqueued, total_self_tasks
            ));
        }

        Ok(())
    }

    fn as_any(&self) -> &dyn Any { self }
}

impl ChannelModule {
    /// Convert a DB record (from data/query result) to a TaskQueueItem.
    fn record_to_task_queue_item(record: &Value, persona_id: &Uuid) -> Option<TaskQueueItem> {
        let record_id = record.get("id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok());
        let data = record.get("data")?;
        Self::data_to_task_queue_item(data, record_id, persona_id)
    }

    /// Convert a self-task JSON (from SelfTaskGenerator) to a TaskQueueItem.
    fn json_to_task_queue_item(task_json: &Value, persona_id: &Uuid) -> Option<TaskQueueItem> {
        let task_id = task_json.get("id")
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
            created_by: data.get("createdBy")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
                .unwrap_or(*persona_id),
            task_domain: data.get("domain")
                .and_then(|v| v.as_str())
                .unwrap_or("self")
                .to_string(),
            task_type: data.get("taskType")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string(),
            context_id: data.get("contextId")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
                .unwrap_or(*persona_id),
            description: data.get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            priority: data.get("priority")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.5) as f32,
            status: "pending".to_string(),
            timestamp: data.get("timestamp")
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
