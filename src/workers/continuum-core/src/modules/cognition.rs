//! CognitionModule — per-persona cognitive state + text analysis IPC.
//!
//! Unified per-persona state: one DashMap<Uuid, PersonaCognition> holds all
//! cognitive state (engine, inbox, rate limiter, sleep, adapters, genome).
//! Single lock acquisition per command. Related state is cache-local.
//!
//! Stateless text analysis commands (similarity, validation, mentions, cleaning)
//! use no per-persona state.
//!
//! Commands:
//! - `cognition/create-engine`: Create all per-persona cognitive state
//! - `cognition/calculate-priority`: Priority scoring
//! - `cognition/fast-path-decision`: Fast-path respond/skip decision
//! - `cognition/enqueue-message`: Enqueue message to persona inbox
//! - `cognition/get-state`: Get persona cognitive state
//! - `cognition/full-evaluate`: Unified 6-gate evaluation (replaces 5 TS gates)
//! - `cognition/track-response`: Track response for rate limiting
//! - `cognition/set-sleep-mode`: Set voluntary sleep mode
//! - `cognition/configure-rate-limiter`: Configure rate limiter params
//! - `cognition/select-model`: 4-tier model priority chain
//! - `cognition/sync-adapters`: Sync adapter registry from TypeScript
//! - `cognition/genome-activate-skill`: LRU eviction + skill activation
//! - `cognition/genome-sync`: Sync full adapter state from TypeScript
//! - `cognition/genome-state`: Get current genome paging state
//! - `cognition/check-adequacy`: Batch adequacy check
//! - `inbox/create`: Create persona inbox (alias for create-engine)
//!
//! Uses `Params` helper for typed parameter extraction.

use crate::runtime::{ServiceModule, ModuleConfig, ModulePriority, CommandResult, ModuleContext};
use crate::persona::{PersonaCognition, InboxMessage, SenderType, Modality};
use crate::persona::{SleepMode, RecentResponse};
use crate::persona::{AdapterInfo, ModelSelectionRequest};
use crate::persona::GenomeAdapterInfo;
use crate::persona::evaluator;
use crate::persona::model_selection;
use crate::persona::text_analysis;
use crate::persona::text_analysis::LoopDetector;
use crate::rag::RagEngine;
use crate::logging::TimingGuard;
use crate::utils::params::Params;
use crate::log_info;
use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;
use uuid::Uuid;

/// Shared state for cognition module.
///
/// `personas` holds ALL per-persona cognitive state in a single DashMap.
/// One lock acquisition gives atomic access to engine + inbox + rate limiter +
/// sleep state + adapter registry + genome engine.
///
/// `rag_engine` and `loop_detector` are shared across all personas.
pub struct CognitionState {
    /// Unified per-persona state: 7 maps → 1.
    pub personas: Arc<DashMap<Uuid, PersonaCognition>>,
    /// Shared RAG engine (not per-persona).
    pub rag_engine: Arc<RagEngine>,
    /// Shared loop detector (not per-persona).
    pub loop_detector: LoopDetector,
}

impl CognitionState {
    pub fn new(rag_engine: Arc<RagEngine>) -> Self {
        Self {
            personas: Arc::new(DashMap::new()),
            rag_engine,
            loop_detector: LoopDetector::new(),
        }
    }
}

pub struct CognitionModule {
    state: Arc<CognitionState>,
}

impl CognitionModule {
    pub fn new(state: Arc<CognitionState>) -> Self {
        Self { state }
    }
}

/// Helper: get or create persona, returning mutable ref via DashMap entry API.
/// Used by commands that need to lazily create persona state.
macro_rules! get_or_create_persona {
    ($self:expr, $persona_uuid:expr) => {
        $self.state.personas
            .entry($persona_uuid)
            .or_insert_with(|| PersonaCognition::new(
                $persona_uuid,
                String::new(),
                $self.state.rag_engine.clone(),
            ))
    };
}

#[async_trait]
impl ServiceModule for CognitionModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "cognition",
            priority: ModulePriority::High,
            command_prefixes: &["cognition/", "inbox/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: None,
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
            // ================================================================
            // Persona Lifecycle
            // ================================================================

            "cognition/create-engine" => {
                let _timer = TimingGuard::new("module", "cognition_create_engine");
                let persona_uuid = p.uuid("persona_id")?;
                let persona_name = p.str("persona_name")?;

                let cognition = PersonaCognition::new(
                    persona_uuid,
                    persona_name.to_string(),
                    self.state.rag_engine.clone(),
                );
                self.state.personas.insert(persona_uuid, cognition);

                log_info!("module", "cognition", "Created cognition for {}", persona_uuid);
                Ok(CommandResult::Json(serde_json::json!({ "created": true })))
            }

            "cognition/calculate-priority" => {
                let _timer = TimingGuard::new("module", "cognition_calculate_priority");
                let persona_uuid = p.uuid("persona_id")?;
                let content = p.str("content")?;
                let sender_type_str = p.str("sender_type")?;
                let is_voice = p.bool_or("is_voice", false);
                let room_uuid = p.uuid("room_id")?;
                let timestamp = p.u64("timestamp")?;

                let sender = parse_sender_type(sender_type_str)?;
                let persona = self.state.personas.get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition for {persona_uuid}"))?;

                let score = persona.engine.calculate_priority(content, sender, is_voice, room_uuid, timestamp);
                Ok(CommandResult::Json(serde_json::to_value(&score)
                    .map_err(|e| format!("Serialize error: {e}"))?))
            }

            "cognition/fast-path-decision" => {
                let _timer = TimingGuard::new("module", "cognition_fast_path_decision");
                let persona_uuid = p.uuid("persona_id")?;
                let message = p.value("message").ok_or("Missing message")?;
                let inbox_msg = parse_inbox_message(message)?;

                let persona = self.state.personas.get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition for {persona_uuid}"))?;

                let decision = persona.engine.fast_path_decision(&inbox_msg);
                Ok(CommandResult::Json(serde_json::to_value(&decision)
                    .map_err(|e| format!("Serialize error: {e}"))?))
            }

            "cognition/enqueue-message" => {
                let _timer = TimingGuard::new("module", "cognition_enqueue_message");
                let persona_uuid = p.uuid("persona_id")?;
                let message = p.value("message").ok_or("Missing message")?;
                let inbox_msg = parse_inbox_message(message)?;

                let persona = get_or_create_persona!(self, persona_uuid);
                persona.inbox.enqueue(inbox_msg);

                Ok(CommandResult::Json(serde_json::json!({
                    "enqueued": true,
                    "queue_size": persona.inbox.len(),
                })))
            }

            "cognition/get-state" => {
                let _timer = TimingGuard::new("module", "cognition_get_state");
                let persona_uuid = p.uuid("persona_id")?;

                let persona = self.state.personas.get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition for {persona_uuid}"))?;

                let state = persona.engine.state();
                Ok(CommandResult::Json(serde_json::json!({
                    "energy": state.energy,
                    "attention": state.attention,
                    "mood": format!("{:?}", state.mood).to_lowercase(),
                    "inbox_load": state.inbox_load,
                    "last_activity_time": state.last_activity_time,
                    "response_count": state.response_count,
                    "compute_budget": state.compute_budget,
                    "service_cadence_ms": state.service_cadence_ms(),
                })))
            }

            "inbox/create" => {
                let _timer = TimingGuard::new("module", "inbox_create");
                let persona_uuid = p.uuid("persona_id")?;
                // Ensure persona exists with all state (inbox is part of PersonaCognition)
                get_or_create_persona!(self, persona_uuid);
                log_info!("module", "cognition", "Ensured inbox for {}", persona_uuid);
                Ok(CommandResult::Json(serde_json::json!({ "created": true })))
            }

            // ================================================================
            // Message Deduplication (single source of truth in Rust)
            // ================================================================

            "cognition/has-evaluated" => {
                let persona_uuid = p.uuid("persona_id")?;
                let message_uuid = p.uuid("message_id")?;

                let persona = self.state.personas.get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition for {persona_uuid}"))?;

                let evaluated = persona.engine.has_evaluated_message(message_uuid);
                Ok(CommandResult::Json(serde_json::json!({ "evaluated": evaluated })))
            }

            "cognition/mark-evaluated" => {
                let persona_uuid = p.uuid("persona_id")?;
                let message_uuid = p.uuid("message_id")?;

                let persona = self.state.personas.get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition for {persona_uuid}"))?;

                persona.engine.mark_message_evaluated(message_uuid);
                Ok(CommandResult::Json(serde_json::json!({ "marked": true })))
            }

            // ================================================================
            // Text Analysis (stateless pure compute + loop detector state)
            // ================================================================

            "cognition/text-similarity" => {
                let _timer = TimingGuard::new("module", "cognition_text_similarity");
                let text1 = p.str("text1")?;
                let text2 = p.str("text2")?;
                let start = std::time::Instant::now();

                let result = text_analysis::TextSimilarityResult {
                    ngram_similarity: text_analysis::jaccard_ngram_similarity(text1, text2),
                    char_similarity: text_analysis::jaccard_char_bigram_similarity(text1, text2),
                    compute_time_us: start.elapsed().as_micros() as u64,
                };
                Ok(CommandResult::Json(serde_json::to_value(&result)
                    .map_err(|e| format!("Serialize error: {e}"))?))
            }

            "cognition/check-semantic-loop" => {
                let _timer = TimingGuard::new("module", "cognition_check_semantic_loop");
                let response_text = p.str("response_text")?;
                let max_history = p.u64_or("max_history", 10) as usize;
                let history = parse_conversation_history(&params, "history")?;

                let result = text_analysis::check_semantic_loop(response_text, &history, max_history);
                Ok(CommandResult::Json(serde_json::to_value(&result)
                    .map_err(|e| format!("Serialize error: {e}"))?))
            }

            "cognition/validate-response" => {
                let _timer = TimingGuard::new("module", "cognition_validate_response");
                let persona_uuid = p.uuid("persona_id")?;
                let response_text = p.str("response_text")?;
                let has_tool_calls = p.bool_or("has_tool_calls", false);
                let history = parse_conversation_history_optional(&params, "conversation_history");

                let result = text_analysis::validate_response(
                    response_text,
                    persona_uuid,
                    has_tool_calls,
                    &history,
                    &self.state.loop_detector,
                );
                Ok(CommandResult::Json(serde_json::to_value(&result)
                    .map_err(|e| format!("Serialize error: {e}"))?))
            }

            "cognition/check-mentions" => {
                let _timer = TimingGuard::new("module", "cognition_check_mentions");
                let start = std::time::Instant::now();
                let message_text = p.str("message_text")?;
                let display_name = p.str("persona_display_name")?;
                let unique_id = p.str_opt("persona_unique_id").unwrap_or("");

                let result = text_analysis::MentionCheckResult {
                    is_persona_mentioned: text_analysis::is_persona_mentioned(message_text, display_name, unique_id),
                    has_directed_mention: text_analysis::has_directed_mention(message_text),
                    compute_time_us: start.elapsed().as_micros() as u64,
                };
                Ok(CommandResult::Json(serde_json::to_value(&result)
                    .map_err(|e| format!("Serialize error: {e}"))?))
            }

            "cognition/clean-response" => {
                let _timer = TimingGuard::new("module", "cognition_clean_response");
                let start = std::time::Instant::now();
                let response_text = p.str("response_text")?;

                let cleaned = text_analysis::clean_response(response_text);
                let result = text_analysis::CleanedResponse {
                    was_cleaned: cleaned != response_text.trim(),
                    text: cleaned,
                    compute_time_us: start.elapsed().as_micros() as u64,
                };
                Ok(CommandResult::Json(serde_json::to_value(&result)
                    .map_err(|e| format!("Serialize error: {e}"))?))
            }

            // ================================================================
            // Unified Evaluation (6-gate pipeline, single lock)
            // ================================================================

            "cognition/full-evaluate" => {
                let _timer = TimingGuard::new("module", "cognition_full_evaluate");
                let persona_uuid = p.uuid("persona_id")?;

                // Single lock — atomic access to engine + rate_limiter + sleep_state
                let persona = self.state.personas.get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition for {persona_uuid}"))?;

                let request = evaluator::FullEvaluateRequest {
                    persona_id: persona_uuid,
                    persona_name: p.str("persona_name")?.to_string(),
                    persona_unique_id: p.str_or("persona_unique_id", "").to_string(),
                    message_id: p.uuid("message_id")?,
                    room_id: p.uuid("room_id")?,
                    sender_id: p.uuid("sender_id")?,
                    sender_name: p.str("sender_name")?.to_string(),
                    sender_type: parse_sender_type(p.str("sender_type")?)?,
                    content: p.str("content")?.to_string(),
                    timestamp: p.u64("timestamp")?,
                    is_voice: p.bool_or("is_voice", false),
                    voice_session_id: p.uuid_opt("voice_session_id"),
                    sender_is_human: p.bool_or("sender_is_human", false),
                    topic_similarity: p.f32_opt("topic_similarity"),
                    recent_room_texts: p.json_opt("recent_room_texts"),
                };

                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                let result = evaluator::full_evaluate(
                    &request,
                    &persona.rate_limiter,
                    &persona.sleep_state,
                    &persona.engine,
                    now_ms,
                );

                log_info!(
                    "module", "cognition",
                    "full-evaluate {}: respond={}, gate={}, confidence={:.2} ({:.2}ms)",
                    persona_uuid, result.should_respond, result.gate, result.confidence, result.decision_time_ms
                );

                Ok(CommandResult::Json(serde_json::to_value(&result)
                    .map_err(|e| format!("Serialize error: {e}"))?))
            }

            "cognition/track-response" => {
                let _timer = TimingGuard::new("module", "cognition_track_response");
                let persona_uuid = p.uuid("persona_id")?;
                let room_uuid = p.uuid("room_id")?;

                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                let mut persona = get_or_create_persona!(self, persona_uuid);
                persona.rate_limiter.track_response(room_uuid, now_ms);

                let count = persona.rate_limiter.response_count(room_uuid);
                log_info!(
                    "module", "cognition",
                    "track-response {}: room={}, count={}",
                    persona_uuid, room_uuid, count
                );

                Ok(CommandResult::Json(serde_json::json!({
                    "tracked": true,
                    "response_count": count,
                })))
            }

            "cognition/set-sleep-mode" => {
                let _timer = TimingGuard::new("module", "cognition_set_sleep_mode");
                let persona_uuid = p.uuid("persona_id")?;
                let mode_str = p.str("mode")?;
                let reason = p.str_or("reason", "").to_string();
                let duration_minutes = p.f64_opt("duration_minutes");

                let mode = match mode_str {
                    "active" => SleepMode::Active,
                    "mentioned_only" => SleepMode::MentionedOnly,
                    "human_only" => SleepMode::HumanOnly,
                    "sleeping" => SleepMode::Sleeping,
                    "until_topic" => SleepMode::UntilTopic,
                    _ => return Err(format!("Invalid sleep mode: {mode_str}")),
                };

                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                let wake_at_ms = duration_minutes.map(|d| now_ms + (d * 60_000.0) as u64);

                let mut persona = get_or_create_persona!(self, persona_uuid);
                let previous = format!("{:?}", persona.sleep_state.mode);

                persona.sleep_state = crate::persona::evaluator::SleepState {
                    mode,
                    reason: reason.clone(),
                    set_at_ms: now_ms,
                    wake_at_ms,
                };

                log_info!(
                    "module", "cognition",
                    "set-sleep-mode {}: {} → {:?} (reason: {})",
                    persona_uuid, previous, mode, reason
                );

                Ok(CommandResult::Json(serde_json::json!({
                    "set": true,
                    "previous_mode": previous,
                    "new_mode": mode_str,
                    "wake_at_ms": wake_at_ms,
                })))
            }

            "cognition/configure-rate-limiter" => {
                let _timer = TimingGuard::new("module", "cognition_configure_rate_limiter");
                let persona_uuid = p.uuid("persona_id")?;
                let min_seconds = p.f64_or("min_seconds_between_responses", 10.0);
                let max_responses = p.u64_or("max_responses_per_session", 50) as u32;

                let mut persona = get_or_create_persona!(self, persona_uuid);
                persona.rate_limiter.min_seconds_between_responses = min_seconds;
                persona.rate_limiter.max_responses_per_session = max_responses;

                log_info!(
                    "module", "cognition",
                    "configure-rate-limiter {}: min_seconds={}, max_responses={}",
                    persona_uuid, min_seconds, max_responses
                );

                Ok(CommandResult::Json(serde_json::json!({
                    "configured": true,
                    "min_seconds_between_responses": min_seconds,
                    "max_responses_per_session": max_responses,
                })))
            }

            // =================================================================
            // Model Selection
            // =================================================================

            "cognition/select-model" => {
                let _timer = TimingGuard::new("module", "cognition_select_model");
                let persona_uuid = p.uuid("persona_id")?;
                let task_domain = params.get("task_domain")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let base_model = p.str("base_model")?.to_string();

                let request = ModelSelectionRequest {
                    persona_id: persona_uuid,
                    task_domain,
                    base_model,
                };

                let persona = get_or_create_persona!(self, persona_uuid);
                let result = model_selection::select_model(&request, &persona.adapter_registry);

                Ok(CommandResult::Json(serde_json::to_value(&result)
                    .map_err(|e| format!("Serialize error: {e}"))?))
            }

            "cognition/sync-adapters" => {
                let _timer = TimingGuard::new("module", "cognition_sync_adapters");
                let persona_uuid = p.uuid("persona_id")?;
                let adapters_json = params.get("adapters")
                    .and_then(|v| v.as_array())
                    .ok_or("Missing adapters array")?;

                let mut persona = get_or_create_persona!(self, persona_uuid);

                // Replace entire adapter set (full sync, not incremental)
                persona.adapter_registry.adapters.clear();

                for adapter_val in adapters_json {
                    let adapter: AdapterInfo = serde_json::from_value(adapter_val.clone())
                        .map_err(|e| format!("Invalid adapter: {e}"))?;
                    persona.adapter_registry.adapters.insert(adapter.name.clone(), adapter);
                }

                let count = persona.adapter_registry.adapters.len();

                log_info!(
                    "module", "cognition",
                    "sync-adapters {}: synced {} adapters",
                    persona_uuid, count
                );

                Ok(CommandResult::Json(serde_json::json!({
                    "synced": true,
                    "adapter_count": count,
                })))
            }

            // =================================================================
            // Genome Paging (LRU eviction + memory budget decisions)
            // =================================================================

            "cognition/genome-activate-skill" => {
                let _timer = TimingGuard::new("module", "cognition_genome_activate_skill");
                let persona_uuid = p.uuid("persona_id")?;
                let skill_name = p.str("skill_name")?.to_string();
                let memory_budget_mb = p.f32_or("memory_budget_mb", 200.0);

                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                let mut persona = get_or_create_persona!(self, persona_uuid);
                persona.genome_engine.memory_budget_mb = memory_budget_mb;
                let result = persona.genome_engine.activate_skill(&skill_name, now_ms);

                log_info!(
                    "module", "cognition",
                    "genome-activate-skill {}: {} activated={}, evicted={:?}, to_load={:?} ({:.0}μs)",
                    persona_uuid, skill_name, result.activated,
                    result.evicted, result.to_load, result.decision_time_us
                );

                Ok(CommandResult::Json(serde_json::to_value(&result)
                    .map_err(|e| format!("Serialize error: {e}"))?))
            }

            "cognition/genome-sync" => {
                let _timer = TimingGuard::new("module", "cognition_genome_sync");
                let persona_uuid = p.uuid("persona_id")?;
                let memory_budget_mb = p.f32_or("memory_budget_mb", 200.0);
                let adapters_json = params.get("adapters")
                    .and_then(|v| v.as_array())
                    .ok_or("Missing adapters array")?;

                let adapters: Vec<GenomeAdapterInfo> = adapters_json.iter()
                    .filter_map(|v| serde_json::from_value(v.clone()).ok())
                    .collect();

                let adapter_count = adapters.len();
                let active_count = adapters.iter().filter(|a| a.is_loaded).count();

                let mut persona = get_or_create_persona!(self, persona_uuid);
                persona.genome_engine.memory_budget_mb = memory_budget_mb;
                persona.genome_engine.sync_state(adapters);

                log_info!(
                    "module", "cognition",
                    "genome-sync {}: {} adapters ({} active), budget={}MB, used={}MB",
                    persona_uuid, adapter_count, active_count,
                    persona.genome_engine.memory_budget_mb, persona.genome_engine.memory_used_mb
                );

                Ok(CommandResult::Json(serde_json::json!({
                    "synced": true,
                    "adapter_count": adapter_count,
                    "active_count": active_count,
                    "memory_used_mb": persona.genome_engine.memory_used_mb,
                    "memory_pressure": persona.genome_engine.memory_pressure(),
                })))
            }

            "cognition/genome-state" => {
                let _timer = TimingGuard::new("module", "cognition_genome_state");
                let persona_uuid = p.uuid("persona_id")?;

                let persona = self.state.personas.get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition for {persona_uuid}"))?;

                let state = persona.genome_engine.state();
                Ok(CommandResult::Json(serde_json::to_value(&state)
                    .map_err(|e| format!("Serialize error: {e}"))?))
            }

            // =================================================================
            // Domain Classification (adapter-aware keyword scoring)
            // =================================================================

            "cognition/classify-domain" => {
                let _timer = TimingGuard::new("module", "cognition_classify_domain");
                let persona_uuid = p.uuid("persona_id")?;
                let text = p.str("text")?;

                let persona = self.state.personas.get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition for {persona_uuid}"))?;

                let result = persona.domain_classifier.classify(text);

                log_info!(
                    "module", "cognition",
                    "classify-domain {}: '{}...' → domain={}, confidence={:.2}, adapter={:?} ({:.0}μs)",
                    persona_uuid,
                    &text[..text.len().min(40)],
                    result.domain, result.confidence, result.adapter_name, result.decision_time_us
                );

                Ok(CommandResult::Json(serde_json::to_value(&result)
                    .map_err(|e| format!("Serialize error: {e}"))?))
            }

            "cognition/sync-domain-classifier" => {
                let _timer = TimingGuard::new("module", "cognition_sync_domain_classifier");
                let persona_uuid = p.uuid("persona_id")?;

                let mut persona = get_or_create_persona!(self, persona_uuid);

                // Build adapter list from genome engine state
                let state = persona.genome_engine.state();
                let all_adapters: Vec<_> = state.active_adapters.iter()
                    .chain(state.available_adapters.iter())
                    .cloned()
                    .collect();

                persona.domain_classifier.sync_from_adapters(&all_adapters);

                let summary = persona.domain_classifier.domain_summary();
                let covered = summary.iter().filter(|(_, has)| *has).count();

                log_info!(
                    "module", "cognition",
                    "sync-domain-classifier {}: {} domains ({} with adapters)",
                    persona_uuid, summary.len(), covered
                );

                Ok(CommandResult::Json(serde_json::json!({
                    "synced": true,
                    "total_domains": summary.len(),
                    "covered_domains": covered,
                })))
            }

            "cognition/register-domain-keywords" => {
                let _timer = TimingGuard::new("module", "cognition_register_domain_keywords");
                let persona_uuid = p.uuid("persona_id")?;
                let domain = p.str("domain")?.to_string();
                let keywords_json = params.get("keywords")
                    .and_then(|v| v.as_array())
                    .ok_or("Missing keywords array")?;

                let keywords: Vec<String> = keywords_json.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();

                let keyword_count = keywords.len();
                let mut persona = get_or_create_persona!(self, persona_uuid);
                persona.domain_classifier.register_domain_keywords(&domain, keywords);

                log_info!(
                    "module", "cognition",
                    "register-domain-keywords {}: added {} keywords to domain '{}'",
                    persona_uuid, keyword_count, domain
                );

                Ok(CommandResult::Json(serde_json::json!({
                    "registered": true,
                    "domain": domain,
                    "keywords_added": keyword_count,
                })))
            }

            // =================================================================
            // Domain Activity Tracking & Gap Detection
            // =================================================================

            "cognition/genome-record-activity" => {
                let _timer = TimingGuard::new("module", "cognition_genome_record_activity");
                let persona_uuid = p.uuid("persona_id")?;
                let domain = p.str("domain")?.to_string();
                let success = p.bool_or("success", true);

                let mut persona = get_or_create_persona!(self, persona_uuid);
                persona.genome_engine.record_activity(&domain, success);

                Ok(CommandResult::Json(serde_json::json!({
                    "recorded": true,
                    "domain": domain,
                    "success": success,
                })))
            }

            "cognition/genome-coverage-report" => {
                let _timer = TimingGuard::new("module", "cognition_genome_coverage_report");
                let persona_uuid = p.uuid("persona_id")?;

                let persona = self.state.personas.get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition for {persona_uuid}"))?;

                let report = persona.genome_engine.coverage_report();

                log_info!(
                    "module", "cognition",
                    "genome-coverage-report {}: {} covered, {} gaps, ratio={:.2}",
                    persona_uuid, report.covered.len(), report.gaps.len(), report.coverage_ratio
                );

                Ok(CommandResult::Json(serde_json::to_value(&report)
                    .map_err(|e| format!("Serialize error: {e}"))?))
            }

            // =================================================================
            // Interaction Quality Scoring
            // =================================================================

            "cognition/score-interaction" => {
                let _timer = TimingGuard::new("module", "cognition_score_interaction");
                let input = p.str("input")?;
                let output = p.str("output")?;
                let feedback = p.str_opt("feedback");
                let task_success = p.bool_opt("task_success");

                let result = crate::persona::domain_classifier::score_interaction_quality(
                    input, output, feedback, task_success,
                );

                Ok(CommandResult::Json(serde_json::to_value(&result)
                    .map_err(|e| format!("Serialize error: {e}"))?))
            }

            // =================================================================
            // Post-Inference Adequacy Check
            // =================================================================

            "cognition/check-adequacy" => {
                let _timer = TimingGuard::new("module", "cognition_check_adequacy");
                let original_text = p.str("original_text")?.to_string();
                let responses_json = params.get("responses")
                    .and_then(|v| v.as_array())
                    .ok_or("Missing responses array")?;

                let responses: Vec<RecentResponse> = responses_json.iter()
                    .filter_map(|v| serde_json::from_value(v.clone()).ok())
                    .collect();

                let result = evaluator::check_response_adequacy(&original_text, &responses);

                log_info!(
                    "module", "cognition",
                    "check-adequacy: adequate={}, confidence={:.2}, responder={:?} ({:.0}μs, {} responses checked)",
                    result.is_adequate, result.confidence,
                    result.responder_name, result.check_time_us, responses.len()
                );

                Ok(CommandResult::Json(serde_json::to_value(&result)
                    .map_err(|e| format!("Serialize error: {e}"))?))
            }

            _ => Err(format!("Unknown cognition command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any { self }
}

// ============================================================================
// Parsing helpers
// ============================================================================

fn parse_sender_type(s: &str) -> Result<SenderType, String> {
    match s {
        "human" => Ok(SenderType::Human),
        "persona" => Ok(SenderType::Persona),
        "agent" => Ok(SenderType::Agent),
        "system" => Ok(SenderType::System),
        _ => Err(format!("Invalid sender_type: {s}")),
    }
}

/// Parse ConversationMessage array from a required JSON field.
fn parse_conversation_history(params: &Value, key: &str) -> Result<Vec<text_analysis::ConversationMessage>, String> {
    let arr = params.get(key)
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("Missing {key} array"))?;
    Ok(parse_messages(arr))
}

/// Parse ConversationMessage array from an optional JSON field.
fn parse_conversation_history_optional(params: &Value, key: &str) -> Vec<text_analysis::ConversationMessage> {
    params.get(key)
        .and_then(|v| v.as_array())
        .map(|arr| parse_messages(arr))
        .unwrap_or_default()
}

fn parse_messages(arr: &[Value]) -> Vec<text_analysis::ConversationMessage> {
    arr.iter()
        .filter_map(|item| {
            Some(text_analysis::ConversationMessage {
                role: item.get("role")?.as_str()?.to_string(),
                content: item.get("content")?.as_str()?.to_string(),
                name: item.get("name").and_then(|n| n.as_str()).map(String::from),
            })
        })
        .collect()
}

/// Parse an InboxMessage from JSON value.
fn parse_inbox_message(value: &Value) -> Result<InboxMessage, String> {
    let p = Params::new(value);

    Ok(InboxMessage {
        id: p.uuid("id")?,
        room_id: p.uuid("room_id")?,
        sender_id: p.uuid("sender_id")?,
        sender_name: p.str("sender_name")?.to_string(),
        sender_type: parse_sender_type(p.str("sender_type")?)?,
        content: p.str("content")?.to_string(),
        timestamp: p.u64("timestamp")?,
        priority: p.f32_or("priority", 0.5),
        source_modality: p.str_opt("source_modality").map(|m| match m {
            "voice" => Modality::Voice,
            _ => Modality::Chat,
        }),
        voice_session_id: p.str_opt("voice_session_id")
            .and_then(|s| Uuid::parse_str(s).ok()),
    })
}
