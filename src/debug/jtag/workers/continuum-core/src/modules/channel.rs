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
    PersonaCognitionEngine, InboxMessage, SenderType, Modality,
};
use crate::persona::channel_types::DOMAIN_PRIORITY_ORDER;
use crate::persona::channel_items::TaskQueueItem;
use crate::persona::self_task_generator::SelfTaskGenerator;
use crate::logging::TimingGuard;
use crate::utils::params::Params;
use crate::log_info;
use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

/// Shared state for channel module — per-persona registries and states.
pub struct ChannelState {
    /// Per-persona channel registries + states.
    pub registries: Arc<DashMap<Uuid, (ChannelRegistry, PersonaState)>>,
    /// Reference to cognition engines for service-cycle-full fast-path decision.
    pub cognition_engines: Arc<DashMap<Uuid, PersonaCognitionEngine>>,
    /// Per-persona self-task generators (lazily created on first tick).
    pub self_task_generators: DashMap<Uuid, tokio::sync::Mutex<SelfTaskGenerator>>,
}

impl ChannelState {
    pub fn new(cognition_engines: Arc<DashMap<Uuid, PersonaCognitionEngine>>) -> Self {
        Self {
            registries: Arc::new(DashMap::new()),
            cognition_engines,
            self_task_generators: DashMap::new(),
        }
    }

    /// Create from existing DashMaps (for gradual migration from ServerState).
    pub fn from_existing(
        registries: Arc<DashMap<Uuid, (ChannelRegistry, PersonaState)>>,
        cognition_engines: Arc<DashMap<Uuid, PersonaCognitionEngine>>,
    ) -> Self {
        Self {
            registries,
            cognition_engines,
            self_task_generators: DashMap::new(),
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
        ModuleConfig {
            name: "channel",
            priority: ModulePriority::High,
            command_prefixes: &["channel/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: Some(Duration::from_secs(60)),
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
                        if let Some(engine) = self.state.cognition_engines.get(&persona_uuid) {
                            let decision = engine.fast_path_decision(&inbox_msg);
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
    /// Runs every 60s (configured via tick_interval in ModuleConfig).
    async fn tick(&self) -> Result<(), String> {
        let log = crate::runtime::logger("channel-tick");

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

            // ── 2. Self-task generation ────────────────────────────────────
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
                            // Enqueue generated self-tasks into channel registry
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

            // ── 3. Training readiness check ────────────────────────────────
            // Check accumulated training data count → trigger training if threshold met.
            // This queries the training_data collection and triggers genome/job-create via TS.
            // The actual training pipeline stays in TypeScript (file I/O, JSONL, Ollama).
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

                // Threshold: 50 unconsumed training examples → trigger training
                if count >= 50 {
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
