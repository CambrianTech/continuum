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
//! - `inbox/drain-frame`: Drain a bounded same-room persona work frame
//! - `cognition/admit-inbox-message`: Run admission gate on an InboxMessage (#1121 PR-4)
//! - `cognition/recall-engrams`: Query the persona's admitted engram store (#1121 PR-5)
//! - `cognition/full-evaluate`: Unified 6-gate evaluation (replaces 5 TS gates)
//! - `cognition/track-response`: Track response for rate limiting
//! - `cognition/set-sleep-mode`: Set voluntary sleep mode
//! - `cognition/configure-rate-limiter`: Configure rate limiter params
//! - `cognition/select-model`: 4-tier model priority chain
//! - `cognition/sync-adapters`: Sync adapter registry from TypeScript
//! - `cognition/genome-activate-skill`: LRU eviction + skill activation
//! - `cognition/genome-sync`: Sync full adapter state from TypeScript
//! - `cognition/genome-state`: Get current genome paging state
//! - `cognition/genome-evict-under-pressure`: Drive eviction to target pressure (broker lever)
//! - `cognition/check-adequacy`: Batch adequacy check
//! - `inbox/create`: Create persona inbox (alias for create-engine)
//!
//! Uses `Params` helper for typed parameter extraction.

use crate::gpu::GpuMemoryManager;
use crate::log_info;
use crate::logging::TimingGuard;
use crate::persona::evaluator;
use crate::persona::message_cache::{CachedMessage, SenderCategory};
use crate::persona::model_selection;
use crate::persona::text_analysis;
use crate::persona::text_analysis::LoopDetector;
use crate::persona::GenomeAdapterInfo;
use crate::persona::{AdapterInfo, ModelSelectionRequest};
use crate::persona::{InboxMessage, Modality, PersonaCognition, SenderType};
use crate::persona::{RecentResponse, SleepMode};
use crate::rag::RagEngine;
use crate::runtime;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::utils::params::Params;
use async_trait::async_trait;
use dashmap::DashMap;
use serde_json::{json, Value};
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
    /// GPU memory manager — real VRAM budgets for genome paging.
    pub gpu_manager: Option<Arc<GpuMemoryManager>>,
}

impl CognitionState {
    pub fn new(rag_engine: Arc<RagEngine>) -> Self {
        Self {
            personas: Arc::new(DashMap::new()),
            rag_engine,
            loop_detector: LoopDetector::new(),
            gpu_manager: None,
        }
    }

    pub fn with_gpu_manager(mut self, manager: Arc<GpuMemoryManager>) -> Self {
        self.gpu_manager = Some(manager);
        self
    }

    /// Per-persona inference budget from GPU manager, or 200MB fallback.
    pub fn per_persona_budget_mb(&self) -> f32 {
        match &self.gpu_manager {
            Some(mgr) => {
                let persona_count = self.personas.len();
                mgr.per_persona_inference_budget_mb(persona_count)
            }
            None => 200.0,
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
/// Uses GPU manager's per-persona budget when available, 200MB otherwise.
macro_rules! get_or_create_persona {
    ($self:expr, $persona_uuid:expr) => {
        $self
            .state
            .personas
            .entry($persona_uuid)
            .or_insert_with(|| {
                let budget = $self.state.per_persona_budget_mb();
                PersonaCognition::with_budget(
                    $persona_uuid,
                    String::new(),
                    $self.state.rag_engine.clone(),
                    budget,
                )
            })
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
            // Persona response can invoke RAG, embeddings, and generation.
            // Keep a single cognition response in flight until the pressure
            // broker can perform explicit multi-persona batching.
            max_concurrency: 1,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        // No init needed. Recipes are JSON data walked by the host
        // (TS recipe loader for the chat path today; future Rust
        // executor for non-Node hosts). The cognition layer just
        // exposes `cognition/respond` and trusts callers to pass
        // `signal` + `personaContext` shaped correctly.
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
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

                log_info!(
                    "module",
                    "cognition",
                    "Created cognition for {}",
                    persona_uuid
                );
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
                let persona = self
                    .state
                    .personas
                    .get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition for {persona_uuid}"))?;

                let score = persona
                    .engine
                    .calculate_priority(content, sender, is_voice, room_uuid, timestamp);
                Ok(CommandResult::Json(
                    serde_json::to_value(&score).map_err(|e| format!("Serialize error: {e}"))?,
                ))
            }

            "cognition/fast-path-decision" => {
                let _timer = TimingGuard::new("module", "cognition_fast_path_decision");
                let persona_uuid = p.uuid("persona_id")?;
                let message = p.value("message").ok_or("Missing message")?;
                let inbox_msg = parse_inbox_message(message)?;

                let persona = self
                    .state
                    .personas
                    .get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition for {persona_uuid}"))?;

                let decision = persona.engine.fast_path_decision(&inbox_msg);
                Ok(CommandResult::Json(
                    serde_json::to_value(&decision).map_err(|e| format!("Serialize error: {e}"))?,
                ))
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

                let persona = self
                    .state
                    .personas
                    .get(&persona_uuid)
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

            "inbox/drain-frame" => {
                let _timer = TimingGuard::new("module", "inbox_drain_frame");
                let persona_uuid = p.uuid("persona_id")?;
                let window_ms = p.u64_or("window_ms", 80);
                let max_items_u64 = p.u64_or("max_items", 16);
                let max_items = usize::try_from(max_items_u64)
                    .map_err(|_| format!("max_items too large: {max_items_u64}"))?;

                let persona = self
                    .state
                    .personas
                    .get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition for {persona_uuid}"))?;

                let frame = persona.inbox.drain_frame(window_ms, max_items);

                Ok(CommandResult::Json(
                    serde_json::to_value(&frame).map_err(|e| format!("Serialize error: {e}"))?,
                ))
            }

            // ================================================================
            // Admission Gate (continuum#1121 PR-4)
            // ================================================================
            // Run the persona's admission gate over an InboxMessage. Returns
            // the typed AdmissionDecision (Admit/Drop/Quarantine) or a typed
            // error. Records side-effects (admitted engram → store, content_hash
            // → dedup record, AIRC event_id → replay-protection record).
            //
            // Caller responsibility: TS/IPC layer chooses WHEN to call this
            // (typically per drained inbox frame). Persona state must already
            // exist (created via cognition/create-engine or get_or_create_persona!).
            "cognition/admit-inbox-message" => {
                let _timer = TimingGuard::new("module", "cognition_admit_inbox_message");
                let persona_uuid = p.uuid("persona_id")?;
                let message_value = p.value("message").ok_or("Missing message")?;
                let inbox_msg = parse_inbox_message(message_value)?;

                let persona = self
                    .state
                    .personas
                    .get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition for {persona_uuid}"))?;

                // The TS-IPC `cognition/admit-inbox-message` caller wants
                // the trace seam-count back in the response (it surfaces
                // funnel telemetry to the TS observer), so this site DOES
                // build a trace and passes Some. The in-process inline
                // gate (`run_inline_admission_gate` below) passes None
                // because it doesn't propagate the trace anywhere.
                let mut trace = crate::persona::trace::CognitionTrace::new();
                match persona.admission.admit(&inbox_msg, Some(&mut trace)) {
                    Ok(decision) => Ok(CommandResult::Json(serde_json::json!({
                        "decision": decision,
                        "engram_count": persona.admission.engram_count(),
                        "trace_seam_count": trace.seam_count(),
                    }))),
                    // TODO(#1121 PR-5+): return the typed `AdmissionError`
                    // as JSON via serde so TS callers can pattern-match
                    // on the variant (`EnvelopeVerificationFailed`,
                    // `TrustBoundaryRejected`, `ReplayDetected`, etc.).
                    // The current `format!()` flattens to a string, losing
                    // the discriminant. Caller can still parse the prefix
                    // for now; PR-5 swaps to `Err(serde_json::to_string(&err)?)`
                    // or a CommandResult error variant that preserves shape.
                    // (claude-tab-2 review nit on #1155.)
                    Err(err) => Err(format!("admission error: {err}")),
                }
            }

            // ================================================================
            // Engram Recall Surface (continuum#1121 PR-5)
            // ================================================================
            // Query the persona's admitted-engram store. Modes:
            //   - kind=recent + limit  → newest-first N engrams
            //   - kind=by_id + id      → exact lookup by uuid
            //   - kind=by_keyword + keyword + limit → case-insensitive substring
            //   - kind=by_origin + origin (chat|airc|tool|self_reflection) + limit
            // Defaults to kind=recent + limit=10 if no kind given.
            //
            // v1 backs against the in-memory engram Vec from PR-4. PR-6+
            // swaps to ORM-backed store with the same API.
            "cognition/recall-engrams" => {
                let _timer = TimingGuard::new("module", "cognition_recall_engrams");
                let persona_uuid = p.uuid("persona_id")?;
                let kind = p.str_opt("kind").unwrap_or("recent");
                let limit_u64 = p.u64_or("limit", 10);
                let limit = usize::try_from(limit_u64)
                    .map_err(|_| format!("limit too large: {limit_u64}"))?;

                let persona = self
                    .state
                    .personas
                    .get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition for {persona_uuid}"))?;

                let engrams = match kind {
                    "recent" => persona.admission.recall_recent(limit),
                    "by_id" => {
                        let id = p.uuid("id")?;
                        persona.admission.recall_by_id(id).into_iter().collect()
                    }
                    "by_keyword" => {
                        let keyword = p.str("keyword")?;
                        persona.admission.recall_by_keyword(keyword, limit)
                    }
                    "by_origin" => {
                        let origin_str = p.str("origin")?;
                        let origin_kind = match origin_str {
                            "chat" => crate::persona::EngramOriginKind::Chat,
                            "airc" => crate::persona::EngramOriginKind::Airc,
                            "tool" => crate::persona::EngramOriginKind::Tool,
                            "self_reflection" => {
                                crate::persona::EngramOriginKind::SelfReflection
                            }
                            other => {
                                return Err(format!(
                                    "unknown origin kind '{other}'; expected one of: \
                                     chat, airc, tool, self_reflection"
                                ))
                            }
                        };
                        persona.admission.recall_by_origin_kind(origin_kind, limit)
                    }
                    other => {
                        return Err(format!(
                            "unknown recall kind '{other}'; expected one of: \
                             recent, by_id, by_keyword, by_origin"
                        ))
                    }
                };

                Ok(CommandResult::Json(serde_json::json!({
                    "engrams": engrams,
                    "count": engrams.len(),
                })))
            }

            // ================================================================
            // Vision Describe (continuum#1276 — TS→Rust oxidizer)
            // ================================================================
            // Migrated from `system/vision/VisionInferenceProvider.ts` (176 LOC).
            // Selects a vision-capable model from the model registry, builds the
            // describe prompt, dispatches `ai/generate` with multimodal content,
            // and parses the response. The TS file becomes a thin shim that
            // calls this IPC. Outlier-validation pair with codex's #1284
            // (structured-decision shape: AIDecisionService.evaluateGating).
            "cognition/vision-describe" => {
                let _timer = TimingGuard::new("module", "cognition_vision_describe");
                let request: crate::cognition::vision_describe::VisionDescribeRequest =
                    serde_json::from_value(params)
                        .map_err(|e| format!("invalid vision-describe params: {e}"))?;
                let result = crate::cognition::vision_describe::describe_image(request).await?;
                Ok(CommandResult::Json(serde_json::to_value(result).map_err(
                    |e| format!("vision-describe serialize result: {e}"),
                )?))
            }

            // ================================================================
            // Message Deduplication (single source of truth in Rust)
            // ================================================================
            "cognition/has-evaluated" => {
                let persona_uuid = p.uuid("persona_id")?;
                let message_uuid = p.uuid("message_id")?;

                let persona = self
                    .state
                    .personas
                    .get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition for {persona_uuid}"))?;

                let evaluated = persona.engine.has_evaluated_message(message_uuid);
                Ok(CommandResult::Json(
                    serde_json::json!({ "evaluated": evaluated }),
                ))
            }

            "cognition/mark-evaluated" => {
                let persona_uuid = p.uuid("persona_id")?;
                let message_uuid = p.uuid("message_id")?;

                let persona = self
                    .state
                    .personas
                    .get(&persona_uuid)
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
                Ok(CommandResult::Json(
                    serde_json::to_value(&result).map_err(|e| format!("Serialize error: {e}"))?,
                ))
            }

            "cognition/check-semantic-loop" => {
                let _timer = TimingGuard::new("module", "cognition_check_semantic_loop");
                let response_text = p.str("response_text")?;
                let max_history = p.u64_or("max_history", 10) as usize;
                let history = parse_conversation_history(&params, "history")?;

                let result =
                    text_analysis::check_semantic_loop(response_text, &history, max_history);
                Ok(CommandResult::Json(
                    serde_json::to_value(&result).map_err(|e| format!("Serialize error: {e}"))?,
                ))
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
                Ok(CommandResult::Json(
                    serde_json::to_value(&result).map_err(|e| format!("Serialize error: {e}"))?,
                ))
            }

            "cognition/check-mentions" => {
                let _timer = TimingGuard::new("module", "cognition_check_mentions");
                let start = std::time::Instant::now();
                let message_text = p.str("message_text")?;
                let display_name = p.str("persona_display_name")?;
                let unique_id = p.str_opt("persona_unique_id").unwrap_or("");

                let result = text_analysis::MentionCheckResult {
                    is_persona_mentioned: text_analysis::is_persona_mentioned(
                        message_text,
                        display_name,
                        unique_id,
                    ),
                    has_directed_mention: text_analysis::has_directed_mention(message_text),
                    compute_time_us: start.elapsed().as_micros() as u64,
                };
                Ok(CommandResult::Json(
                    serde_json::to_value(&result).map_err(|e| format!("Serialize error: {e}"))?,
                ))
            }

            "cognition/clean-response" => {
                let _timer = TimingGuard::new("module", "cognition_clean_response");
                let start = std::time::Instant::now();
                let response_text = p.str("response_text")?;

                let clean_result = text_analysis::clean_response(response_text);
                let result = text_analysis::CleanedResponse {
                    was_cleaned: clean_result.text != response_text.trim(),
                    text: clean_result.text,
                    thinking: clean_result.thinking,
                    compute_time_us: start.elapsed().as_micros() as u64,
                };
                Ok(CommandResult::Json(
                    serde_json::to_value(&result).map_err(|e| format!("Serialize error: {e}"))?,
                ))
            }

            // ================================================================
            // Unified Evaluation (6-gate pipeline, single lock)
            // ================================================================
            "cognition/full-evaluate" => {
                let _timer = TimingGuard::new("module", "cognition_full_evaluate");
                let persona_uuid = p.uuid("persona_id")?;

                // Single lock — atomic access to engine + rate_limiter + sleep_state
                let persona = self
                    .state
                    .personas
                    .get(&persona_uuid)
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
                    &persona.message_cache,
                    now_ms,
                );

                log_info!(
                    "module",
                    "cognition",
                    "full-evaluate {}: respond={}, gate={}, confidence={:.2} ({:.2}ms)",
                    persona_uuid,
                    result.should_respond,
                    result.gate,
                    result.confidence,
                    result.decision_time_ms
                );

                Ok(CommandResult::Json(
                    serde_json::to_value(&result).map_err(|e| format!("Serialize error: {e}"))?,
                ))
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
                    "module",
                    "cognition",
                    "track-response {}: room={}, count={}",
                    persona_uuid,
                    room_uuid,
                    count
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
                    "module",
                    "cognition",
                    "set-sleep-mode {}: {} → {:?} (reason: {})",
                    persona_uuid,
                    previous,
                    mode,
                    reason
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
                    "module",
                    "cognition",
                    "configure-rate-limiter {}: min_seconds={}, max_responses={}",
                    persona_uuid,
                    min_seconds,
                    max_responses
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
                let task_domain = params
                    .get("task_domain")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let request = ModelSelectionRequest {
                    persona_id: persona_uuid,
                    task_domain,
                };

                let persona = get_or_create_persona!(self, persona_uuid);
                let result = model_selection::select_model(&request, &persona.adapter_registry)
                    .map_err(|e| e.to_string())?;

                Ok(CommandResult::Json(
                    serde_json::to_value(&result).map_err(|e| format!("Serialize error: {e}"))?,
                ))
            }

            "cognition/sync-adapters" => {
                let _timer = TimingGuard::new("module", "cognition_sync_adapters");
                let persona_uuid = p.uuid("persona_id")?;
                let adapters_json = params
                    .get("adapters")
                    .and_then(|v| v.as_array())
                    .ok_or("Missing adapters array")?;

                let mut persona = get_or_create_persona!(self, persona_uuid);

                // Replace entire adapter set (full sync, not incremental)
                persona.adapter_registry.adapters.clear();

                for adapter_val in adapters_json {
                    let adapter: AdapterInfo = serde_json::from_value(adapter_val.clone())
                        .map_err(|e| format!("Invalid adapter: {e}"))?;
                    persona
                        .adapter_registry
                        .adapters
                        .insert(adapter.name.clone(), adapter);
                }

                let count = persona.adapter_registry.adapters.len();

                log_info!(
                    "module",
                    "cognition",
                    "sync-adapters {}: synced {} adapters",
                    persona_uuid,
                    count
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
                let gpu_budget = self.state.per_persona_budget_mb();
                // 0 or missing = use GPU-detected budget
                let ts_budget = p.f32_or("memory_budget_mb", 0.0);
                let memory_budget_mb = if ts_budget > 0.0 {
                    ts_budget
                } else {
                    gpu_budget
                };

                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                let mut persona = get_or_create_persona!(self, persona_uuid);
                persona.genome_engine.memory_budget_mb = memory_budget_mb;
                let result = persona.genome_engine.activate_skill(&skill_name, now_ms);

                log_info!(
                    "module",
                    "cognition",
                    "genome-activate-skill {}: {} activated={}, evicted={:?}, to_load={:?} ({:.0}μs)",
                    persona_uuid,
                    skill_name,
                    result.activated,
                    result.evicted,
                    result.to_load,
                    result.decision_time_us
                );

                Ok(CommandResult::Json(
                    serde_json::to_value(&result).map_err(|e| format!("Serialize error: {e}"))?,
                ))
            }

            "cognition/genome-sync" => {
                let _timer = TimingGuard::new("module", "cognition_genome_sync");
                let persona_uuid = p.uuid("persona_id")?;
                let gpu_budget = self.state.per_persona_budget_mb();
                // 0 or missing = use GPU-detected budget
                let ts_budget = p.f32_or("memory_budget_mb", 0.0);
                let memory_budget_mb = if ts_budget > 0.0 {
                    ts_budget
                } else {
                    gpu_budget
                };
                let adapters_json = params
                    .get("adapters")
                    .and_then(|v| v.as_array())
                    .ok_or("Missing adapters array")?;

                let adapters: Vec<GenomeAdapterInfo> = adapters_json
                    .iter()
                    .filter_map(|v| serde_json::from_value(v.clone()).ok())
                    .collect();

                let adapter_count = adapters.len();
                let active_count = adapters.iter().filter(|a| a.is_loaded).count();

                let mut persona = get_or_create_persona!(self, persona_uuid);
                persona.genome_engine.memory_budget_mb = memory_budget_mb;
                persona.genome_engine.sync_state(adapters);

                log_info!(
                    "module",
                    "cognition",
                    "genome-sync {}: {} adapters ({} active), budget={}MB, used={}MB",
                    persona_uuid,
                    adapter_count,
                    active_count,
                    persona.genome_engine.memory_budget_mb,
                    persona.genome_engine.memory_used_mb
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

                let persona = self
                    .state
                    .personas
                    .get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition for {persona_uuid}"))?;

                let state = persona.genome_engine.state();
                Ok(CommandResult::Json(
                    serde_json::to_value(&state).map_err(|e| format!("Serialize error: {e}"))?,
                ))
            }

            // The PressureBroker lever — drive eviction down to a target
            // pressure ratio without an activate_skill call. Uses the same
            // formula and victim selection as activate_skill's implicit
            // eviction; respects critical-adapter protection (priority > 0.9).
            // Returns bytes_freed + post-eviction state. When the broker
            // singleton lands and registers per-persona ResourcePool
            // wrappers, this command is what those wrappers will call;
            // until then it's manually testable for verification.
            "cognition/genome-evict-under-pressure" => {
                let _timer = TimingGuard::new("module", "cognition_genome_evict_under_pressure");
                let persona_uuid = p.uuid("persona_id")?;
                let target_pressure = p.f32_or("target_pressure", 0.75);

                let mut persona = get_or_create_persona!(self, persona_uuid);
                let pressure_before = persona.genome_engine.memory_pressure();
                let bytes_freed = persona.genome_engine.evict_under_pressure(target_pressure);
                let pressure_after = persona.genome_engine.memory_pressure();

                log_info!(
                    "module",
                    "cognition",
                    "genome-evict-under-pressure {}: target={:.2} pressure {:.2} → {:.2}, freed {} bytes",
                    persona_uuid,
                    target_pressure,
                    pressure_before,
                    pressure_after,
                    bytes_freed
                );

                Ok(CommandResult::Json(json!({
                    "personaId": persona_uuid.to_string(),
                    "targetPressure": target_pressure,
                    "pressureBefore": pressure_before,
                    "pressureAfter": pressure_after,
                    "bytesFreed": bytes_freed,
                })))
            }

            // =================================================================
            // Persona response (shared cognition pipeline entry point)
            // =================================================================
            // The single external IPC command for persona response. Replaces
            // the old TS PersonaResponseGenerator orchestration. Internally
            // runs cognition::analyze (cached, shared across responders for
            // the same message) → cognition::score_persona for THIS persona
            // only → if should_respond, calls persona::response::respond
            // which builds the prompt, runs inference, strips/emits <think>
            // blocks, and returns the visible speech.
            //
            // PRG.ts becomes a thin shim that calls this. The chat path's
            // per-persona iteration calls into this once per persona; the
            // cognition cache means the analysis runs once per message
            // even when called M times.
            //
            // See docs/architecture/SHARED-COGNITION.md for the full picture
            // and PERSONA-COGNITION-RUST-MIGRATION.md for why this command
            // exists in Rust rather than TS.
            "cognition/respond" => {
                let _timer = TimingGuard::new("module", "cognition_respond");

                // Wire shape: caller sends `{ signal, personaContext }`.
                // No `recipe` field — recipes are JSON data walked by the
                // host (TS recipe loader for chat today; future portable
                // walker for non-Node hosts). The cognition layer just
                // projects (signal, ctx) → RespondInput, runs respond(),
                // and returns the response. Output post-processing
                // (substitute / intercept) is the walker's concern, not
                // cognition's.
                //
                // No fallback path. Old `{recipe, signal, personaContext}`
                // shape parses fine here (extra `recipe` field ignored)
                // but callers should drop it.
                let signal: crate::persona::cognition_io::Signal = p.json("signal")?;
                let ctx: crate::persona::cognition_io::PersonaContext = p.json("personaContext")?;

                let mut input = crate::persona::cognition_io::build_respond_input(&signal, &ctx)?;

                // ── Hot-path admission gate (continuum#1211 PR-1) ──
                // Run admission BEFORE inference so the persona's
                // engram store grows from real chat turns. Without
                // this call the admission machinery (#1121 PR-1..5) is
                // plumbed end-to-end but never reached on the chat
                // path — personas accumulate zero memory.
                //
                // Forensic-not-destructive: a missing AdmissionState
                // (persona never had `cognition/create-engine` called)
                // is logged and skipped, NOT a chat-blocking error.
                // The persona still responds; it just doesn't grow
                // memory until the engine is created.
                run_inline_admission_gate(&self.state, &signal, &ctx);

                // ── Hot-path recall surface (continuum#1211 PR-2) ──
                // After admission gate, populate input.recalled_engrams
                // with the persona's most-recently-admitted memory so
                // prompt_assembly can render a `[Recent Memory]` block
                // in the system prompt. Closes the engram loop:
                // admit (PR-1) → store → recall (PR-2) → context →
                // model sees its own memory.
                //
                // Cap = 5 most-recent engrams. The number is a budget
                // policy: enough to ground the persona in continuity
                // ("yes the user mentioned teal earlier") without
                // dominating the prompt. Future tunable via per-persona
                // AdmissionConfig; v1 is a hardcoded sensible default.
                //
                // Empty when persona has no AdmissionState (same
                // forensic-skip path as the gate above) OR no admitted
                // engrams yet (cold-start). Both are normal early-life
                // states; a no-recall persona is unchanged from
                // pre-PR-2 behavior. Prompt_assembly skips rendering
                // when the list is empty (no `[Recent Memory]` header
                // appears).
                const RECALL_LIMIT: usize = 5;
                if let Some(persona) = self.state.personas.get(&ctx.persona_id) {
                    input.recalled_engrams = persona
                        .admission
                        .recall_recent(RECALL_LIMIT)
                        .into_iter()
                        .map(|e| e.content)
                        .collect();
                }

                // Diagnostic: log what media survived the projection.
                // Vision routing was failing 2026-04-21 and this stays
                // as the in-flight tap to confirm media shape arriving
                // at cognition matches what the host believed it sent.
                if !input.message_media.is_empty() {
                    let shape: Vec<String> = input
                        .message_media
                        .iter()
                        .map(|item| {
                            let has_b64 = item.base64.as_deref().map(|s| s.len()).unwrap_or(0);
                            let has_desc = item.description.is_some();
                            format!("{}(b64={}, desc={})", item.item_type, has_b64, has_desc)
                        })
                        .collect();
                    runtime::logger("cognition").info_fmt(format_args!(
                        "cognition/respond: message_media count={} shapes=[{}]",
                        input.message_media.len(),
                        shape.join(", ")
                    ));
                }

                let response = crate::persona::response::respond(input).await?;

                Ok(CommandResult::Json(
                    serde_json::to_value(&response).map_err(|e| format!("Serialize error: {e}"))?,
                ))
            }

            // =================================================================
            // Recipe/RAG turn batching boundary
            // =================================================================
            // Pure planning command: no ORM, no inference, no file I/O. The host
            // supplies the trigger, candidate personas, and active RAG sources;
            // Rust returns deterministic keys + fan-out/admission policy so Node
            // stays a wrapper instead of inventing per-persona batching behavior.
            "cognition/plan-turn-batch" => {
                let _timer = TimingGuard::new("module", "cognition_plan_turn_batch");
                let request: crate::cognition::RecipeTurnBatchRequest = p.json("request")?;
                let plan = crate::cognition::plan_turn_batch(request);

                Ok(CommandResult::Json(
                    serde_json::to_value(&plan).map_err(|e| format!("Serialize error: {e}"))?,
                ))
            }

            // =================================================================
            // Domain Classification (adapter-aware keyword scoring)
            // =================================================================
            "cognition/classify-domain" => {
                let _timer = TimingGuard::new("module", "cognition_classify_domain");
                let persona_uuid = p.uuid("persona_id")?;
                let text = p.str("text")?;

                let persona = self
                    .state
                    .personas
                    .get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition for {persona_uuid}"))?;

                let result = persona.domain_classifier.classify(text);

                log_info!(
                    "module",
                    "cognition",
                    "classify-domain {}: '{}...' → domain={}, confidence={:.2}, adapter={:?} ({:.0}μs)",
                    persona_uuid,
                    &text[..text.len().min(40)],
                    result.domain,
                    result.confidence,
                    result.adapter_name,
                    result.decision_time_us
                );

                Ok(CommandResult::Json(
                    serde_json::to_value(&result).map_err(|e| format!("Serialize error: {e}"))?,
                ))
            }

            "cognition/sync-domain-classifier" => {
                let _timer = TimingGuard::new("module", "cognition_sync_domain_classifier");
                let persona_uuid = p.uuid("persona_id")?;

                let mut persona = get_or_create_persona!(self, persona_uuid);

                // Build adapter list from genome engine state
                let state = persona.genome_engine.state();
                let all_adapters: Vec<_> = state
                    .active_adapters
                    .iter()
                    .chain(state.available_adapters.iter())
                    .cloned()
                    .collect();

                persona.domain_classifier.sync_from_adapters(&all_adapters);

                let summary = persona.domain_classifier.domain_summary();
                let covered = summary.iter().filter(|(_, has)| *has).count();

                log_info!(
                    "module",
                    "cognition",
                    "sync-domain-classifier {}: {} domains ({} with adapters)",
                    persona_uuid,
                    summary.len(),
                    covered
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
                let keywords_json = params
                    .get("keywords")
                    .and_then(|v| v.as_array())
                    .ok_or("Missing keywords array")?;

                let keywords: Vec<String> = keywords_json
                    .iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect();

                let keyword_count = keywords.len();
                let mut persona = get_or_create_persona!(self, persona_uuid);
                persona
                    .domain_classifier
                    .register_domain_keywords(&domain, keywords);

                log_info!(
                    "module",
                    "cognition",
                    "register-domain-keywords {}: added {} keywords to domain '{}'",
                    persona_uuid,
                    keyword_count,
                    domain
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

                let persona = self
                    .state
                    .personas
                    .get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition for {persona_uuid}"))?;

                let report = persona.genome_engine.coverage_report();

                log_info!(
                    "module",
                    "cognition",
                    "genome-coverage-report {}: {} covered, {} gaps, ratio={:.2}",
                    persona_uuid,
                    report.covered.len(),
                    report.gaps.len(),
                    report.coverage_ratio
                );

                Ok(CommandResult::Json(
                    serde_json::to_value(&report).map_err(|e| format!("Serialize error: {e}"))?,
                ))
            }

            // =================================================================
            // GPU Budget Query (for TypeScript genome initialization)
            // =================================================================
            "cognition/gpu-budget" => {
                let per_persona = self.state.per_persona_budget_mb();
                let gpu_info = self
                    .state
                    .gpu_manager
                    .as_ref()
                    .map(|mgr| {
                        let stats = mgr.stats();
                        serde_json::json!({
                            "gpu_name": stats.gpu_name,
                            "total_vram_mb": stats.total_vram_mb,
                            "inference_budget_mb": stats.inference.budget_mb,
                            "persona_count": self.state.personas.len(),
                            "per_persona_budget_mb": per_persona,
                            "pressure": stats.pressure,
                        })
                    })
                    .unwrap_or_else(|| {
                        serde_json::json!({
                            "gpu_name": "unknown",
                            "total_vram_mb": 0,
                            "inference_budget_mb": 0,
                            "persona_count": self.state.personas.len(),
                            "per_persona_budget_mb": per_persona,
                            "pressure": 0.0,
                        })
                    });

                Ok(CommandResult::Json(gpu_info))
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
                    input,
                    output,
                    feedback,
                    task_success,
                );

                Ok(CommandResult::Json(
                    serde_json::to_value(&result).map_err(|e| format!("Serialize error: {e}"))?,
                ))
            }

            // =================================================================
            // Post-Inference Adequacy Check
            // =================================================================
            "cognition/check-adequacy" => {
                let _timer = TimingGuard::new("module", "cognition_check_adequacy");
                let original_text = p.str("original_text")?.to_string();
                let responses_json = params
                    .get("responses")
                    .and_then(|v| v.as_array())
                    .ok_or("Missing responses array")?;

                let responses: Vec<RecentResponse> = responses_json
                    .iter()
                    .filter_map(|v| serde_json::from_value(v.clone()).ok())
                    .collect();

                let result = evaluator::check_response_adequacy(&original_text, &responses);

                log_info!(
                    "module",
                    "cognition",
                    "check-adequacy: adequate={}, confidence={:.2}, responder={:?} ({:.0}μs, {} responses checked)",
                    result.is_adequate,
                    result.confidence,
                    result.responder_name,
                    result.check_time_us,
                    responses.len()
                );

                Ok(CommandResult::Json(
                    serde_json::to_value(&result).map_err(|e| format!("Serialize error: {e}"))?,
                ))
            }

            // =================================================================
            // Message Cache (echo chamber + post-inference adequacy)
            // =================================================================
            "cognition/cache-message" => {
                let _timer = TimingGuard::new("module", "cognition_cache_message");
                let persona_uuid = p.uuid("persona_id")?;
                let room_uuid = p.uuid("room_id")?;

                let msg = CachedMessage {
                    id: p.uuid("message_id")?,
                    sender_id: p.uuid("sender_id")?,
                    sender_type: if p.str_or("sender_type", "human") == "human" {
                        SenderCategory::Human
                    } else {
                        SenderCategory::AI
                    },
                    sender_name: p.str("sender_name")?.to_string(),
                    content_text: p.str_or("content", "").to_string(),
                    timestamp_ms: p.u64("timestamp")?,
                };

                let mut persona = get_or_create_persona!(self, persona_uuid);
                persona.message_cache.push(room_uuid, msg);

                Ok(CommandResult::Json(serde_json::json!({
                    "success": true,
                    "cached": true
                })))
            }

            // =================================================================
            // Content Deduplication
            // =================================================================
            "cognition/check-content-dedup" => {
                let _timer = TimingGuard::new("module", "cognition_check_content_dedup");
                let persona_uuid = p.uuid("persona_id")?;
                let room_uuid = p.uuid("room_id")?;
                let content = p.str("content")?;

                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                let persona = self
                    .state
                    .personas
                    .get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition for {persona_uuid}"))?;

                let result = persona
                    .content_dedup
                    .is_duplicate(content, room_uuid, now_ms);

                Ok(CommandResult::Json(serde_json::json!({
                    "success": true,
                    "is_duplicate": result.is_duplicate,
                    "check_time_us": result.check_time_us
                })))
            }

            "cognition/record-content" => {
                let _timer = TimingGuard::new("module", "cognition_record_content");
                let persona_uuid = p.uuid("persona_id")?;
                let room_uuid = p.uuid("room_id")?;
                let content = p.str("content")?;

                let now_ms = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as u64;

                let mut persona = get_or_create_persona!(self, persona_uuid);
                persona.content_dedup.record(content, room_uuid, now_ms);

                Ok(CommandResult::Json(serde_json::json!({
                    "success": true,
                    "recorded": true
                })))
            }

            _ => Err(format!("Unknown cognition command: {command}")),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
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
fn parse_conversation_history(
    params: &Value,
    key: &str,
) -> Result<Vec<text_analysis::ConversationMessage>, String> {
    let arr = params
        .get(key)
        .and_then(|v| v.as_array())
        .ok_or_else(|| format!("Missing {key} array"))?;
    Ok(parse_messages(arr))
}

/// Parse ConversationMessage array from an optional JSON field.
fn parse_conversation_history_optional(
    params: &Value,
    key: &str,
) -> Vec<text_analysis::ConversationMessage> {
    params
        .get(key)
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

/// Outcome of the inline admission gate. Made testable by extracting
/// from the `cognition/respond` IPC handler — claude-tab-2 review nit
/// #3 on PR #1213 (the forensic-skip path was untested as inline code).
///
/// Logged for the same funnel-metric grep-ability as the underlying
/// `AdmissionDecision::label()` (#1213 nit #2 — label moved to live
/// next to the type in `persona/engram.rs`).
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum InlineAdmissionOutcome {
    /// Admission ran and produced a decision. Variant carried so
    /// callers (today: hot-path log) can branch on `admit` vs
    /// `drop`/`quarantine` without re-walking the full enum.
    Decided(&'static str),
    /// Admission machinery itself errored (envelope verify, replay,
    /// etc.). Carried so the warn log reads the typed cause.
    MachineryError(String),
    /// Persona had no `AdmissionState` — `cognition/create-engine`
    /// was never called for this persona. Forensic-not-destructive:
    /// log + continue, don't block the chat turn.
    NoPersona,
}

/// Run the admission gate inline as a pre-step to `respond()`. Side
/// effects: AdmissionState's engram store grows on Admit; a warn log
/// fires on MachineryError or NoPersona. Returns the typed outcome
/// for caller-side telemetry / unit tests (claude-tab-2 review nit
/// #3 on PR #1213).
///
/// **Hot-path log discipline (claude-tab-2 review nit #1):** the
/// steady-state `Admit` decision does NOT log — every chat turn for
/// every persona would otherwise pay a `format!` allocation that
/// nobody reads. The engram store growth itself is observable via
/// `cognition/recall-engrams` (#1121 PR-5) for funnel telemetry.
/// Drop and Quarantine decisions DO log at info because they're the
/// unhappy paths a debugger needs to find. Errors and missing-state
/// log at warn.
pub(crate) fn run_inline_admission_gate(
    state: &CognitionState,
    signal: &crate::persona::cognition_io::Signal,
    ctx: &crate::persona::cognition_io::PersonaContext,
) -> InlineAdmissionOutcome {
    let inbox_msg = crate::persona::cognition_io::signal_to_inbox_message(signal, ctx);
    let Some(persona) = state.personas.get(&ctx.persona_id) else {
        runtime::logger("cognition").warn_fmt(format_args!(
            "cognition/respond: no AdmissionState for persona={} \
             — skipping admission (call cognition/create-engine first \
             to enable memory accumulation)",
            ctx.persona_id,
        ));
        return InlineAdmissionOutcome::NoPersona;
    };

    // Pass `None` for the trace — the inline gate doesn't propagate
    // it anywhere (the cognition/respond IPC handler doesn't surface
    // an admission trace seam to its caller; the recorder doesn't
    // capture admission seams as part of the per-turn fixture). With
    // `None`, the admission codepath skips `record_seam` entirely:
    // no `now_ms()` syscall, no `serde_json::json!` Map allocation,
    // no String allocations for seam name/metadata. Cuts ~7
    // allocations per chat turn per persona. The TS-IPC
    // `cognition/admit-inbox-message` handler still passes `Some` —
    // it surfaces the seam count in the response.
    match persona.admission.admit(&inbox_msg, None) {
        Ok(decision) => {
            let label = decision.label();
            // Skip Admit — common case, no allocation. Drop +
            // Quarantine are the noteworthy outcomes a debugger wants
            // to grep for; log those at info. Engram count piggy-
            // backs the unhappy-path log so funnel monitoring can
            // join "% drops" against "engram store size" without a
            // separate query.
            if label != "admit" {
                runtime::logger("cognition").info_fmt(format_args!(
                    "cognition/respond: admission decision={label} \
                     engrams={} (persona={})",
                    persona.admission.engram_count(),
                    ctx.persona_id,
                ));
            }
            InlineAdmissionOutcome::Decided(label)
        }
        Err(err) => {
            let err_string = err.to_string();
            runtime::logger("cognition").warn_fmt(format_args!(
                "cognition/respond: admission error \
                 (continuing without memory grow): {err_string} \
                 (persona={})",
                ctx.persona_id,
            ));
            InlineAdmissionOutcome::MachineryError(err_string)
        }
    }
}

// ─── Tests for the inline admission gate (claude-tab-2 review nit
// #3 on PR #1213) ────────────────────────────────────────────────────
//
// The inline admission gate inside the `cognition/respond` IPC
// handler used to live as inline code, untestable without a full
// IPC fixture. Extracting `run_inline_admission_gate` made it a
// callable function; these tests exercise the forensic-skip branch
// (no AdmissionState for the persona) so a future refactor can't
// silently change the behavior to an error-and-block (which would
// make every chat turn for an uncreated persona fail).
//
// Tests use a real `CognitionState` constructed with an empty
// `RagEngine` — same shape `persona::evaluator::tests` uses. No
// mocks; the substrate is small enough to construct as-is.
#[cfg(test)]
mod inline_admission_tests {
    use super::*;
    use crate::cognition::RecentMessage;
    use crate::persona::cognition_io::{Signal, SignalKind, SignalOriginator};
    use std::sync::Arc;

    /// Build a minimal Signal + PersonaContext pair for the test.
    /// Both are wire-shape types; the test mirrors what `cognition/respond`
    /// receives over IPC at the inline-gate site.
    fn fixture(persona_id: Uuid) -> (Signal, crate::persona::cognition_io::PersonaContext) {
        let signal = Signal {
            kind: SignalKind::ChatMessage,
            text: "hello world".to_string(),
            media: vec![],
            originator: SignalOriginator::User { user_id: Uuid::new_v4() },
            timestamp_ms: 1_715_625_600_000,
            message_id: Some(Uuid::new_v4()),
        };
        let ctx = crate::persona::cognition_io::PersonaContext {
            persona_id,
            display_name: "Test Persona".to_string(),
            specialty: "general".to_string(),
            model: "test-model".to_string(),
            capabilities: vec![],
            system_prompt: String::new(),
            recent_history: Vec::<RecentMessage>::new(),
            known_specialties: vec![],
            other_persona_names: vec![],
            room_id: Some(Uuid::new_v4()),
            is_voice: false,
        };
        (signal, ctx)
    }

    /// What this catches: the forensic-not-destructive missing-
    /// AdmissionState branch returns `NoPersona` and continues
    /// (no panic, no error propagated). If a future edit changes
    /// the `let Some(persona) = ...` to a `?` or an `expect()`,
    /// this test fails and surfaces the regression at unit-test
    /// time rather than during a live chat-roundtrip smoke.
    #[test]
    fn missing_admission_state_returns_no_persona_no_panic() {
        let rag_engine = Arc::new(crate::rag::RagEngine::new());
        let state = CognitionState::new(rag_engine);
        // Note: state.personas is empty — no `cognition/create-engine`
        // was ever called for this persona, modeling the bootstrap
        // edge case where a chat turn lands before the engine is up.
        let persona_id = Uuid::new_v4();
        let (signal, ctx) = fixture(persona_id);

        let outcome = run_inline_admission_gate(&state, &signal, &ctx);
        assert_eq!(outcome, InlineAdmissionOutcome::NoPersona);
        // Verify the state DashMap stays empty — the gate is a
        // pure no-op when there's no AdmissionState to mutate.
        assert_eq!(state.personas.len(), 0);
    }

    /// What this catches: when the persona DOES have AdmissionState,
    /// the gate runs admission and returns `Decided(...)`. The label
    /// is one of the documented variants — guards against
    /// `AdmissionDecision::label` ever returning a fresh slug that
    /// would silently break log-grep dashboards.
    #[test]
    fn admission_with_persona_returns_decided_variant() {
        let rag_engine = Arc::new(crate::rag::RagEngine::new());
        let state = CognitionState::new(rag_engine.clone());
        let persona_id = Uuid::new_v4();
        // Materialize the persona state — same path
        // `cognition/create-engine` takes during bootstrap.
        state.personas.insert(
            persona_id,
            crate::persona::PersonaCognition::new(
                persona_id,
                "Test Persona".to_string(),
                rag_engine,
            ),
        );

        let (signal, ctx) = fixture(persona_id);
        let outcome = run_inline_admission_gate(&state, &signal, &ctx);
        match outcome {
            InlineAdmissionOutcome::Decided(label) => {
                assert!(
                    matches!(label, "admit" | "drop" | "quarantine"),
                    "label must be one of the documented slugs, got: {label}",
                );
            }
            other => panic!("expected Decided, got {other:?}"),
        }
    }
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
        voice_session_id: p
            .str_opt("voice_session_id")
            .and_then(|s| Uuid::parse_str(s).ok()),
    })
}
