//! CognitionModule — wraps PersonaCognitionEngine per-persona DashMap state.
//!
//! Validates the ServiceModule trait handles stateful per-persona DashMap isolation —
//! the MOST DIFFERENT pattern from stateless HealthModule.
//!
//! Handles: cognition/create-engine, cognition/calculate-priority,
//!          cognition/fast-path-decision, cognition/enqueue-message, cognition/get-state,
//!          cognition/text-similarity, cognition/check-semantic-loop,
//!          inbox/create

use crate::runtime::{ServiceModule, ModuleConfig, ModulePriority, CommandResult, ModuleContext};
use crate::persona::{PersonaCognitionEngine, PersonaInbox, InboxMessage, SenderType, Modality};
use crate::persona::text_analysis;
use crate::persona::text_analysis::LoopDetector;
use crate::rag::RagEngine;
use crate::logging::TimingGuard;
use crate::log_info;
use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::Value;
use std::any::Any;
use std::sync::Arc;
use uuid::Uuid;

/// Shared state for cognition module — per-persona engines and inboxes.
pub struct CognitionState {
    /// Per-persona cognition engines.
    pub engines: Arc<DashMap<Uuid, PersonaCognitionEngine>>,
    /// Per-persona inboxes.
    pub inboxes: Arc<DashMap<Uuid, PersonaInbox>>,
    /// Shared RAG engine.
    pub rag_engine: Arc<RagEngine>,
    /// Per-persona response loop detector (Phase 2).
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
        match command {
            "cognition/create-engine" => {
                let _timer = TimingGuard::new("module", "cognition_create_engine");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let persona_name = params.get("persona_name")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_name")?;

                let persona_uuid = Uuid::parse_str(persona_id)
                    .map_err(|e| format!("Invalid persona_id: {e}"))?;

                let (_, shutdown_rx) = tokio::sync::watch::channel(false);
                let engine = PersonaCognitionEngine::new(
                    persona_uuid,
                    persona_name.to_string(),
                    self.state.rag_engine.clone(),
                    shutdown_rx,
                );

                self.state.engines.insert(persona_uuid, engine);

                log_info!("module", "cognition", "Created cognition engine for {}", persona_id);
                Ok(CommandResult::Json(serde_json::json!({ "created": true })))
            }

            "cognition/calculate-priority" => {
                let _timer = TimingGuard::new("module", "cognition_calculate_priority");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let content = params.get("content")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing content")?;
                let sender_type_str = params.get("sender_type")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing sender_type")?;
                let is_voice = params.get("is_voice")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                let room_id = params.get("room_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing room_id")?;
                let timestamp = params.get("timestamp")
                    .and_then(|v| v.as_u64())
                    .ok_or("Missing timestamp")?;

                let persona_uuid = Uuid::parse_str(persona_id)
                    .map_err(|e| format!("Invalid persona_id: {e}"))?;
                let room_uuid = Uuid::parse_str(room_id)
                    .map_err(|e| format!("Invalid room_id: {e}"))?;

                let sender = match sender_type_str {
                    "human" => SenderType::Human,
                    "persona" => SenderType::Persona,
                    "agent" => SenderType::Agent,
                    "system" => SenderType::System,
                    _ => return Err(format!("Invalid sender_type: {}", sender_type_str)),
                };

                let engine = self.state.engines.get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition engine for {}", persona_id))?;

                let score = engine.calculate_priority(content, sender, is_voice, room_uuid, timestamp);

                Ok(CommandResult::Json(serde_json::json!({
                    "score": score.score,
                    "factors": {
                        "recency_score": score.factors.recency_score,
                        "mention_score": score.factors.mention_score,
                        "room_score": score.factors.room_score,
                        "sender_score": score.factors.sender_score,
                        "voice_boost": score.factors.voice_boost,
                    }
                })))
            }

            "cognition/fast-path-decision" => {
                let _timer = TimingGuard::new("module", "cognition_fast_path_decision");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let message = params.get("message")
                    .ok_or("Missing message")?;

                let persona_uuid = Uuid::parse_str(persona_id)
                    .map_err(|e| format!("Invalid persona_id: {e}"))?;

                let inbox_msg = parse_inbox_message(message)?;

                let engine = self.state.engines.get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition engine for {}", persona_id))?;

                let decision = engine.fast_path_decision(&inbox_msg);

                Ok(CommandResult::Json(serde_json::json!({
                    "should_respond": decision.should_respond,
                    "confidence": decision.confidence,
                    "reason": decision.reason,
                    "decision_time_ms": decision.decision_time_ms,
                    "fast_path_used": decision.fast_path_used,
                })))
            }

            "cognition/enqueue-message" => {
                let _timer = TimingGuard::new("module", "cognition_enqueue_message");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let message = params.get("message")
                    .ok_or("Missing message")?;

                let persona_uuid = Uuid::parse_str(persona_id)
                    .map_err(|e| format!("Invalid persona_id: {e}"))?;

                let inbox_msg = parse_inbox_message(message)?;

                let inbox = self.state.inboxes
                    .entry(persona_uuid)
                    .or_insert_with(|| PersonaInbox::new(persona_uuid));
                inbox.enqueue(inbox_msg);

                let queue_size = inbox.len();

                Ok(CommandResult::Json(serde_json::json!({
                    "enqueued": true,
                    "queue_size": queue_size,
                })))
            }

            "cognition/get-state" => {
                let _timer = TimingGuard::new("module", "cognition_get_state");

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;

                let persona_uuid = Uuid::parse_str(persona_id)
                    .map_err(|e| format!("Invalid persona_id: {e}"))?;

                let engine = self.state.engines.get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition engine for {}", persona_id))?;

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

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                // Note: capacity parameter is ignored - PersonaInbox doesn't support it
                let _capacity = params.get("capacity")
                    .and_then(|v| v.as_u64())
                    .map(|c| c as usize);

                let persona_uuid = Uuid::parse_str(persona_id)
                    .map_err(|e| format!("Invalid persona_id: {e}"))?;

                // Create inbox with persona_uuid
                let inbox = PersonaInbox::new(persona_uuid);

                self.state.inboxes.insert(persona_uuid, inbox);

                log_info!("module", "cognition", "Created inbox for {}", persona_id);
                Ok(CommandResult::Json(serde_json::json!({ "created": true })))
            }

            // ================================================================
            // Text Analysis commands (Phase 1: Similarity)
            // ================================================================

            "cognition/text-similarity" => {
                let _timer = TimingGuard::new("module", "cognition_text_similarity");

                let text1 = params.get("text1")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing text1")?;
                let text2 = params.get("text2")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing text2")?;

                let start = std::time::Instant::now();

                let ngram_similarity = text_analysis::jaccard_ngram_similarity(text1, text2);
                let char_similarity = text_analysis::jaccard_char_bigram_similarity(text1, text2);

                let elapsed_us = start.elapsed().as_micros() as u64;

                Ok(CommandResult::Json(serde_json::json!({
                    "ngram_similarity": ngram_similarity,
                    "char_similarity": char_similarity,
                    "compute_time_us": elapsed_us,
                })))
            }

            "cognition/check-semantic-loop" => {
                let _timer = TimingGuard::new("module", "cognition_check_semantic_loop");

                let response_text = params.get("response_text")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing response_text")?;
                let max_history = params.get("max_history")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize)
                    .unwrap_or(10);

                let history_arr = params.get("history")
                    .and_then(|v| v.as_array())
                    .ok_or("Missing history array")?;

                let history: Vec<text_analysis::ConversationMessage> = history_arr.iter()
                    .filter_map(|item| {
                        let role = item.get("role")?.as_str()?;
                        let content = item.get("content")?.as_str()?;
                        let name = item.get("name").and_then(|n| n.as_str()).map(String::from);
                        Some(text_analysis::ConversationMessage {
                            role: role.to_string(),
                            content: content.to_string(),
                            name,
                        })
                    })
                    .collect();

                let result = text_analysis::check_semantic_loop(
                    response_text,
                    &history,
                    max_history,
                );

                Ok(CommandResult::Json(serde_json::json!({
                    "should_block": result.should_block,
                    "similarity": result.similarity,
                    "reason": result.reason,
                })))
            }

            // ================================================================
            // Validation commands (Phase 2: Combined validation gates)
            // ================================================================

            "cognition/validate-response" => {
                let _timer = TimingGuard::new("module", "cognition_validate_response");
                let start = std::time::Instant::now();

                let persona_id = params.get("persona_id")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing persona_id")?;
                let response_text = params.get("response_text")
                    .and_then(|v| v.as_str())
                    .ok_or("Missing response_text")?;
                let has_tool_calls = params.get("has_tool_calls")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                let persona_uuid = Uuid::parse_str(persona_id)
                    .map_err(|e| format!("Invalid persona_id: {e}"))?;

                // Gate 1: Garbage detection (skip if has native tool calls — empty text + tools is valid)
                let garbage_result = if has_tool_calls {
                    text_analysis::GarbageCheckResult {
                        is_garbage: false,
                        reason: text_analysis::GarbageReason::None,
                        details: String::new(),
                        score: 0.0,
                    }
                } else {
                    text_analysis::is_garbage(response_text)
                };

                if garbage_result.is_garbage {
                    let total_us = start.elapsed().as_micros() as u64;
                    return Ok(CommandResult::Json(serde_json::json!({
                        "passed": false,
                        "gate_failed": "garbage",
                        "garbage_result": garbage_result,
                        "is_response_loop": false,
                        "loop_duplicate_count": 0,
                        "has_truncated_tool_call": false,
                        "semantic_result": { "should_block": false, "similarity": 0.0, "reason": "" },
                        "total_time_us": total_us,
                    })));
                }

                // Gate 2: Response loop detection (skip if has tool calls)
                let (is_loop, dup_count) = if has_tool_calls {
                    (false, 0)
                } else {
                    self.state.loop_detector.check_response_loop(persona_uuid, response_text)
                };

                if is_loop {
                    let total_us = start.elapsed().as_micros() as u64;
                    return Ok(CommandResult::Json(serde_json::json!({
                        "passed": false,
                        "gate_failed": "response_loop",
                        "garbage_result": garbage_result,
                        "is_response_loop": true,
                        "loop_duplicate_count": dup_count,
                        "has_truncated_tool_call": false,
                        "semantic_result": { "should_block": false, "similarity": 0.0, "reason": "" },
                        "total_time_us": total_us,
                    })));
                }

                // Gate 3: Truncated tool call detection
                let truncated = text_analysis::has_truncated_tool_call(response_text);
                if truncated {
                    let total_us = start.elapsed().as_micros() as u64;
                    return Ok(CommandResult::Json(serde_json::json!({
                        "passed": false,
                        "gate_failed": "truncated_tool_call",
                        "garbage_result": garbage_result,
                        "is_response_loop": false,
                        "loop_duplicate_count": dup_count,
                        "has_truncated_tool_call": true,
                        "semantic_result": { "should_block": false, "similarity": 0.0, "reason": "" },
                        "total_time_us": total_us,
                    })));
                }

                // Gate 4: Semantic loop detection
                let history_arr = params.get("conversation_history")
                    .and_then(|v| v.as_array());

                let semantic_result = if let Some(arr) = history_arr {
                    let history: Vec<text_analysis::ConversationMessage> = arr.iter()
                        .filter_map(|item| {
                            let role = item.get("role")?.as_str()?;
                            let content = item.get("content")?.as_str()?;
                            let name = item.get("name").and_then(|n| n.as_str()).map(String::from);
                            Some(text_analysis::ConversationMessage {
                                role: role.to_string(),
                                content: content.to_string(),
                                name,
                            })
                        })
                        .collect();
                    text_analysis::check_semantic_loop(response_text, &history, 10)
                } else {
                    text_analysis::SemanticLoopResult {
                        should_block: false,
                        similarity: 0.0,
                        reason: "No conversation history provided".to_string(),
                    }
                };

                let passed = !semantic_result.should_block;
                let gate_failed = if semantic_result.should_block {
                    Some("semantic_loop")
                } else {
                    None
                };

                let total_us = start.elapsed().as_micros() as u64;
                Ok(CommandResult::Json(serde_json::json!({
                    "passed": passed,
                    "gate_failed": gate_failed,
                    "garbage_result": garbage_result,
                    "is_response_loop": false,
                    "loop_duplicate_count": dup_count,
                    "has_truncated_tool_call": false,
                    "semantic_result": semantic_result,
                    "total_time_us": total_us,
                })))
            }

            _ => Err(format!("Unknown cognition command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any { self }
}

/// Parse an InboxMessage from JSON value.
fn parse_inbox_message(value: &Value) -> Result<InboxMessage, String> {
    let id = value.get("id")
        .and_then(|v| v.as_str())
        .ok_or("Missing message.id")?;
    let room_id = value.get("room_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing message.room_id")?;
    let sender_id = value.get("sender_id")
        .and_then(|v| v.as_str())
        .ok_or("Missing message.sender_id")?;
    let sender_name = value.get("sender_name")
        .and_then(|v| v.as_str())
        .ok_or("Missing message.sender_name")?;
    let sender_type_str = value.get("sender_type")
        .and_then(|v| v.as_str())
        .ok_or("Missing message.sender_type")?;
    let content = value.get("content")
        .and_then(|v| v.as_str())
        .ok_or("Missing message.content")?;
    let timestamp = value.get("timestamp")
        .and_then(|v| v.as_u64())
        .ok_or("Missing message.timestamp")?;
    let priority = value.get("priority")
        .and_then(|v| v.as_f64())
        .map(|p| p as f32)
        .unwrap_or(0.5);

    Ok(InboxMessage {
        id: Uuid::parse_str(id).map_err(|e| format!("Invalid id: {e}"))?,
        room_id: Uuid::parse_str(room_id).map_err(|e| format!("Invalid room_id: {e}"))?,
        sender_id: Uuid::parse_str(sender_id).map_err(|e| format!("Invalid sender_id: {e}"))?,
        sender_name: sender_name.to_string(),
        sender_type: match sender_type_str {
            "human" => SenderType::Human,
            "persona" => SenderType::Persona,
            "agent" => SenderType::Agent,
            "system" => SenderType::System,
            _ => return Err(format!("Invalid sender_type: {}", sender_type_str)),
        },
        content: content.to_string(),
        timestamp,
        priority,
        source_modality: value.get("source_modality")
            .and_then(|v| v.as_str())
            .map(|m| match m {
                "voice" => Modality::Voice,
                _ => Modality::Chat,
            }),
        voice_session_id: value.get("voice_session_id")
            .and_then(|v| v.as_str())
            .and_then(|s| Uuid::parse_str(s).ok()),
    })
}
