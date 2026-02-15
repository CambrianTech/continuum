//! CognitionModule — per-persona cognitive state + text analysis IPC.
//!
//! Stateful per-persona DashMap isolation for cognition engines and inboxes,
//! plus stateless text analysis commands (similarity, validation, mentions, cleaning).
//!
//! Uses `Params` helper for typed parameter extraction.

use crate::runtime::{ServiceModule, ModuleConfig, ModulePriority, CommandResult, ModuleContext};
use crate::persona::{PersonaCognitionEngine, PersonaInbox, InboxMessage, SenderType, Modality};
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

/// Shared state for cognition module — per-persona engines and inboxes.
pub struct CognitionState {
    pub engines: Arc<DashMap<Uuid, PersonaCognitionEngine>>,
    pub inboxes: Arc<DashMap<Uuid, PersonaInbox>>,
    pub rag_engine: Arc<RagEngine>,
    pub loop_detector: LoopDetector,
}

impl CognitionState {
    pub fn new(rag_engine: Arc<RagEngine>) -> Self {
        Self {
            engines: Arc::new(DashMap::new()),
            inboxes: Arc::new(DashMap::new()),
            rag_engine,
            loop_detector: LoopDetector::new(),
        }
    }

    /// Create from existing DashMaps (for gradual migration from ServerState).
    pub fn from_existing(
        engines: Arc<DashMap<Uuid, PersonaCognitionEngine>>,
        inboxes: Arc<DashMap<Uuid, PersonaInbox>>,
        rag_engine: Arc<RagEngine>,
    ) -> Self {
        Self { engines, inboxes, rag_engine, loop_detector: LoopDetector::new() }
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
            // Persona Lifecycle (stateful, per-persona DashMap)
            // ================================================================

            "cognition/create-engine" => {
                let _timer = TimingGuard::new("module", "cognition_create_engine");
                let persona_uuid = p.uuid("persona_id")?;
                let persona_name = p.str("persona_name")?;

                let (_, shutdown_rx) = tokio::sync::watch::channel(false);
                let engine = PersonaCognitionEngine::new(
                    persona_uuid,
                    persona_name.to_string(),
                    self.state.rag_engine.clone(),
                    shutdown_rx,
                );

                self.state.engines.insert(persona_uuid, engine);
                log_info!("module", "cognition", "Created cognition engine for {}", persona_uuid);
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
                let engine = self.state.engines.get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition engine for {persona_uuid}"))?;

                let score = engine.calculate_priority(content, sender, is_voice, room_uuid, timestamp);
                Ok(CommandResult::Json(serde_json::to_value(&score)
                    .map_err(|e| format!("Serialize error: {e}"))?))
            }

            "cognition/fast-path-decision" => {
                let _timer = TimingGuard::new("module", "cognition_fast_path_decision");
                let persona_uuid = p.uuid("persona_id")?;
                let message = p.value("message").ok_or("Missing message")?;
                let inbox_msg = parse_inbox_message(message)?;

                let engine = self.state.engines.get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition engine for {persona_uuid}"))?;

                let decision = engine.fast_path_decision(&inbox_msg);
                Ok(CommandResult::Json(serde_json::to_value(&decision)
                    .map_err(|e| format!("Serialize error: {e}"))?))
            }

            "cognition/enqueue-message" => {
                let _timer = TimingGuard::new("module", "cognition_enqueue_message");
                let persona_uuid = p.uuid("persona_id")?;
                let message = p.value("message").ok_or("Missing message")?;
                let inbox_msg = parse_inbox_message(message)?;

                let inbox = self.state.inboxes
                    .entry(persona_uuid)
                    .or_insert_with(|| PersonaInbox::new(persona_uuid));
                inbox.enqueue(inbox_msg);

                Ok(CommandResult::Json(serde_json::json!({
                    "enqueued": true,
                    "queue_size": inbox.len(),
                })))
            }

            "cognition/get-state" => {
                let _timer = TimingGuard::new("module", "cognition_get_state");
                let persona_uuid = p.uuid("persona_id")?;

                let engine = self.state.engines.get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition engine for {persona_uuid}"))?;

                let state = engine.state();
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
                self.state.inboxes.insert(persona_uuid, PersonaInbox::new(persona_uuid));
                log_info!("module", "cognition", "Created inbox for {}", persona_uuid);
                Ok(CommandResult::Json(serde_json::json!({ "created": true })))
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
