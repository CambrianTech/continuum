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
//! - `cognition/should-respond`: Rust-owned AI gating decision
//! - `cognition/check-redundancy`: Rust-owned draft redundancy decision
//! - `cognition/full-evaluate`: Unified 6-gate evaluation (replaces 5 TS gates)
//! - `cognition/track-response`: Track response for rate limiting
//! - `cognition/set-sleep-mode`: Set voluntary sleep mode
//! - `cognition/configure-rate-limiter`: Configure rate limiter params
//! - `cognition/select-model`: 4-tier model priority chain
//! - `cognition/sync-adapters`: Sync adapter registry from TypeScript
//! - `cognition/check-adequacy`: Batch adequacy check
//!
//! The `cognition/genome-*` family (activate-skill, sync, state, evict-under-pressure,
//! record-activity, coverage-report) migrated to the typed DynCommand registry —
//! see `commands/cognition/` and `command_objects` there.
//! - `inbox/create`: Create persona inbox (alias for create-engine)
//!
//! Uses `Params` helper for typed parameter extraction.

use crate::gpu::GpuMemoryManager;
use crate::log_info;
use crate::logging::TimingGuard;
use crate::persona::evaluator;
use crate::persona::text_analysis::LoopDetector;
use crate::persona::{
    InboxMessage, Modality, PersonaCognition, PersonaInboxFrame, PersonaTurnFrame,
    PersonaTurnFrameReplayRecord, SenderType,
};
use crate::persona::RecentResponse;
use crate::rag::RagEngine;
use crate::runtime;
use crate::runtime::{
    CommandResult, LateBound, ModuleConfig, ModuleContext, ModulePriority, ModuleRegistry,
    ServiceModule,
};
use crate::utils::params::Params;
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
    /// GPU memory manager — real VRAM budgets for genome paging.
    pub gpu_manager: Option<Arc<GpuMemoryManager>>,
    /// Rust module registry for in-process cognition -> inference dispatch.
    ///
    /// This is intentionally NOT the global command executor: `persona/turn-execute`
    /// must fail loudly if the Rust inference module is absent instead of falling
    /// through to TypeScript.
    pub module_registry: Option<Arc<ModuleRegistry>>,
}

impl CognitionState {
    pub fn new(rag_engine: Arc<RagEngine>) -> Self {
        Self {
            personas: Arc::new(DashMap::new()),
            rag_engine,
            loop_detector: LoopDetector::new(),
            gpu_manager: None,
            module_registry: None,
        }
    }

    pub fn with_gpu_manager(mut self, manager: Arc<GpuMemoryManager>) -> Self {
        self.gpu_manager = Some(manager);
        self
    }

    pub fn with_module_registry(mut self, registry: Arc<ModuleRegistry>) -> Self {
        self.module_registry = Some(registry);
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

    /// Get or lazily create per-persona cognition state, GPU-budget-aware. The one
    /// place that owns the lazy-create policy — every caller (remaining legacy match
    /// arms and the migrated typed commands) routes through here.
    pub fn get_or_create_persona(
        &self,
        persona_uuid: Uuid,
    ) -> dashmap::mapref::one::RefMut<'_, Uuid, PersonaCognition> {
        self.personas.entry(persona_uuid).or_insert_with(|| {
            let budget = self.per_persona_budget_mb();
            PersonaCognition::with_budget(
                persona_uuid,
                String::new(),
                self.rag_engine.clone(),
                budget,
            )
        })
    }
}

pub struct CognitionModule {
    state: Arc<CognitionState>,
    executor: LateBound<crate::runtime::CommandExecutor>,
}

impl CognitionModule {
    pub fn new(state: Arc<CognitionState>) -> Self {
        Self {
            state,
            executor: LateBound::new("cognition::executor"),
        }
    }
}

#[async_trait]
impl ServiceModule for CognitionModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "cognition",
            priority: ModulePriority::High,
            command_prefixes: &["cognition/", "inbox/", "persona/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            // Persona response is event-fanout work: every active persona
            // builds prompt/context/should-respond in parallel (cheap), then
            // hits ai_provider (which serializes inference). Capping cognition
            // itself was a belt-and-suspenders waiting for a real broker —
            // codex's persona inbox fanout primitive (today) + the upcoming
            // PressureBroker singleton (#1299) make event fanout the
            // intended invariant. Inference is still gated downstream by
            // ai_provider::max_concurrency. 0 is the runtime contract for
            // "unlimited / module-managed"; usize::MAX overflows Tokio's
            // semaphore permit ceiling during registration.
            max_concurrency: 0,
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
            // cognition/create-engine migrated to the typed DynCommand registry (Slice 7)
            // — see commands/cognition/create_engine.rs (dep-holding on CognitionState,
            // access: Internal).

            // NOTE: `cognition/eval` (the test-graded coder gym) is now a typed,
            // registered, Privileged ActionCommand — see `cognition::eval::CognitionEval`.
            // It dispatches via the central registry (so `cu` and every client can
            // reach it, and it's discoverable + gated), no longer a match-arm here.
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

            // cognition/enqueue-message migrated to the typed DynCommand registry (Slice 7)
            // — see commands/cognition/enqueue_message.rs. The wire→domain conversion now
            // lives as InboxMessageRequest::to_inbox_message (ipc/protocol.rs).

            // cognition/get-state migrated to the typed DynCommand registry (Slice 8)
            // — see commands/cognition/get_state.rs (dep-holding, access: Internal,
            // camelCase GetStateResult projection of PersonaState + service_cadence_ms).

            // inbox/create + inbox/drain-frame migrated to the typed DynCommand registry
            // (Slice 7) — see commands/cognition/{inbox_create,inbox_drain_frame}.rs. The
            // frame-recording helper `record_drained_turn_frame` stays here (made
            // pub(crate)) because the still-legacy Lane D arms below (persona/drain-turn-frame,
            // persona/turn-execute) also call it; it moves out when they migrate.

            // ─── Lane D: PersonaTurnFrame wrap-in-Rust ──────────────
            //
            // Wraps the inbox/drain-frame output in a PersonaTurnFrame
            // and returns the full PersonaTurnFrameReplayRecord (raw
            // inbox + consolidated_inbox + rag_seed) in ONE Rust hop.
            //
            // Why this command exists: per Joel's "no TS wrapping
            // Rust outputs" rule + ALPHA-GAP Lane D, the substrate
            // shouldn't return a raw PersonaInboxFrame and rely on
            // TS to wrap it as a turn frame. The Rust core owns the
            // turn-frame contract end-to-end.
            //
            // Replay: returns None when the frame is empty (no
            // messages) — caller treats empty drain as no-op, not a
            // failure. When non-empty, the returned record IS the
            // replay-stable input contract for inference / RAG /
            // sentinel attribution downstream.
            "persona/drain-turn-frame" => {
                let _timer = TimingGuard::new("module", "persona_drain_turn_frame");
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

                // Drain the inbox into a raw frame.
                let raw_frame = persona.inbox.drain_frame(window_ms, max_items);
                record_drained_turn_frame(&raw_frame);

                // Wrap + populate derived outputs. None = empty
                // drain; returned as JSON null.
                let record = match raw_frame {
                    Some(inbox_frame) => {
                        let turn_frame =
                            crate::persona::turn_frame::PersonaTurnFrame::from_inbox_frame(
                                inbox_frame,
                            );
                        turn_frame.replay_record()
                    }
                    None => None,
                };

                // Persist the record to ~/.continuum/replay/ for
                // prod-replay (Joel's "FROM PROD not POC" rule).
                if let Some(ref rec) = record {
                    crate::persona::recorder::record_turn_frame_replay(rec);
                }

                Ok(CommandResult::Json(
                    serde_json::to_value(&record).map_err(|e| format!("Serialize error: {e}"))?,
                ))
            }

            // ─── Lane D: persona/turn-execute (alpha card #1409) ──
            //
            // Chains the full Rust persona turn in one IPC hop:
            //   drain inbox
            //     -> wrap in PersonaTurnFrame
            //     -> derive ResponsePrompt (lazy output)
            //     -> build InferenceRequest (prompt_text path)
            //     -> dispatch `inference/llm/request` via the Rust
            //        ModuleRegistry only
            //     -> bundle replay_record + inference response
            //
            // Why one command: the TS persona loop previously
            // executed each stage with its own IPC round-trip
            // (drain, then build prompt, then call inference) —
            // 3 round-trips per turn, prompt-building lived in
            // TS. Lane D pulls all three into the substrate so
            // (a) the prompt is built in Rust where the turn-frame
            // lives, (b) the production replay record carries the
            // exact prompt that fed inference, (c) the persona
            // turn becomes one observable unit on the bus.
            //
            // Empty drain returns `{ "replayRecord": null,
            // "inferenceResponse": null }` — no-op, not an error.
            // Persona not found returns typed Err per Joel's never-
            // swallow rule.
            //
            // The actual inference happens in InferenceLlmModule:
            // when wired with no adapter (PR-5 shape), it returns
            // the 3-token stub response; when wired with an
            // adapter (future), it runs the real engine. Either
            // way the turn-execute command's contract is the same.
            "persona/turn-execute" => {
                let _timer = TimingGuard::new("module", "persona_turn_execute");
                let persona_uuid = p.uuid("persona_id")?;
                let window_ms = p.u64_or("window_ms", 80);
                let max_items_u64 = p.u64_or("max_items", 16);
                let max_items = usize::try_from(max_items_u64)
                    .map_err(|_| format!("max_items too large: {max_items_u64}"))?;

                // Optional composition + sampling + budget params. Callers that
                // don't pass them get defaults; the substrate uses the canonical
                // SamplingParams::default + a conservative GenerationBudget so
                // a misconfigured caller doesn't run unbounded inference.
                let composition_artifact_id =
                    p.uuid_opt("composition_artifact_id").unwrap_or(Uuid::nil());
                let max_tokens = u32::try_from(p.u64_or("max_tokens", 512))
                    .map_err(|_| "max_tokens too large for u32".to_string())?;
                let max_duration_ms = u32::try_from(p.u64_or("max_duration_ms", 10_000))
                    .map_err(|_| "max_duration_ms too large for u32".to_string())?;

                let persona = self
                    .state
                    .personas
                    .get(&persona_uuid)
                    .ok_or_else(|| format!("No cognition for {persona_uuid}"))?;

                let raw_frame = persona.inbox.drain_frame(window_ms, max_items);
                record_drained_turn_frame(&raw_frame);

                // Empty drain: returned as null pair, NOT an Err.
                // Idle ticks are routine; a no-op is the correct
                // outcome, not a failure.
                let inbox_frame = match raw_frame {
                    Some(f) => f,
                    None => {
                        return Ok(CommandResult::Json(serde_json::json!({
                            "replayRecord": Value::Null,
                            "inferenceResponse": Value::Null,
                        })));
                    }
                };

                let turn_frame = PersonaTurnFrame::from_inbox_frame(inbox_frame);
                let replay_record = turn_frame.replay_record();
                if let Some(ref rec) = replay_record {
                    crate::persona::recorder::record_turn_frame_replay(rec);
                }

                let response_prompt = turn_frame
                    .response_prompt()
                    .ok_or_else(|| {
                        format!(
                            "persona/turn-execute: non-empty drain produced no ResponsePrompt for {persona_uuid}"
                        )
                    })?;

                // Build the substrate InferenceRequest. The
                // request_id is fresh per-turn; the persona +
                // composition come from the turn frame + caller.
                // prompt_text is the flattened ResponsePrompt;
                // prompt_tokens is empty (adapter-path).
                let inference_request = crate::inference::llm_module::InferenceRequest {
                    request_id: crate::inference::llm_module::InferenceRequestId::new(
                        Uuid::new_v4(),
                    ),
                    persona: crate::identity::PeerId::from_uuid(persona_uuid),
                    composition: crate::inference::llm_module::CompositionPlan(
                        crate::genome::working_set::ArtifactId::new(composition_artifact_id),
                    ),
                    prompt_tokens: vec![],
                    prompt_text: Some(response_prompt.to_prompt_text()),
                    budget: crate::inference::llm_module::GenerationBudget {
                        max_tokens,
                        max_duration_ms,
                    },
                    sampling: crate::inference::llm_module::SamplingParams::default(),
                    stop_sequences: vec![],
                };

                let inference_response = execute_rust_module_json(
                    self.state.module_registry.as_deref(),
                    crate::inference::llm_module_service::COMMAND_REQUEST,
                    serde_json::to_value(&inference_request)
                        .map_err(|e| format!("Serialize inference request: {e}"))?,
                )
                .await
                .map_err(|e| {
                    format!(
                        "persona/turn-execute: Rust inference dispatch failed for {persona_uuid}: {e}"
                    )
                })?;

                Ok(CommandResult::Json(serde_json::json!({
                    "replayRecord": replay_record,
                    "inferenceResponse": inference_response,
                })))
            }

            // cognition/admit-inbox-message + cognition/recall-engrams migrated to the
            // typed DynCommand registry (Slice 9) — see
            // commands/cognition/{admit_inbox_message,recall_engrams}.rs. The wire→domain
            // conversion for admit now reuses InboxMessageRequest::to_inbox_message
            // (ipc/protocol.rs), the one canonical seam.

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
                let executor = self.executor.require()?;
                let result =
                    crate::cognition::vision_describe::describe_image(request, executor).await?;
                Ok(CommandResult::Json(serde_json::to_value(result).map_err(
                    |e| format!("vision-describe serialize result: {e}"),
                )?))
            }

            // ================================================================
            // AI Gating + Draft Redundancy — MIGRATED to the typed registry.
            // ================================================================
            // `cognition/should-respond` and `cognition/check-redundancy` are now
            // stateless `ActionCommand`s in `crate::commands::cognition` (each calls
            // the same free fn — `evaluate_gating` / `evaluate_redundancy` — over its
            // typed request). `route_object` dispatches them via `command_registry()`,
            // so they reach the ACL, codegen, `cu`, and grid routing. Both are
            // `access: Internal`. No match arm here — a second registration would be
            // the only place they could collide, and there is none.

            // ================================================================
            // Response Generation + Tool Embedding + Validate-Decision
            // ================================================================
            // cognition/generate-response, cognition/embed-tools,
            // cognition/semantic-search-tools, and cognition/validate-response-decision
            // migrated to the typed DynCommand registry as stateless unit-struct
            // action_command!s — see commands/cognition/{generate_response,embed_tools,
            // semantic_search_tools,validate_response_decision}.rs (access: Internal, no
            // module state — they self-route via inventory).

            // ================================================================
            // Message Deduplication (single source of truth in Rust)
            // ================================================================
            // cognition/has-evaluated + cognition/mark-evaluated migrated to the typed
            // DynCommand registry — see commands/cognition/{has_evaluated,mark_evaluated}.rs
            // (dep-holding on CognitionState, access: Internal).

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

            // cognition/track-response migrated to the typed DynCommand registry — see
            // commands/cognition/track_response.rs (dep-holding on CognitionState,
            // access: Internal).

            // cognition/set-sleep-mode + cognition/configure-rate-limiter migrated to the
            // typed DynCommand registry (Slice 6) — see
            // commands/cognition/{set_sleep_mode,configure_rate_limiter}.rs (dep-holding on
            // CognitionState, access: Internal).

            // =================================================================
            // Model Selection
            // =================================================================
            // =================================================================
            // Model Selection + Adapter Sync
            // =================================================================
            // cognition/select-model and cognition/sync-adapters migrated to the typed
            // DynCommand registry as dep-holding action_command!s capturing CognitionState
            // — see commands/cognition/{select_model,sync_adapters}.rs (access: Internal),
            // exposed via commands/cognition/mod.rs::command_objects.

            // =================================================================
            // Genome Paging (LRU eviction + memory budget decisions)
            // =================================================================
            // Migrated to the typed DynCommand registry (Slice 4):
            //   cognition/genome-activate-skill      → commands/cognition/genome_activate_skill.rs
            //   cognition/genome-sync                → commands/cognition/genome_sync.rs
            //   cognition/genome-state               → commands/cognition/genome_state.rs
            //   cognition/genome-evict-under-pressure → commands/cognition/genome_evict_under_pressure.rs
            // Exposed via commands/cognition/mod.rs::command_objects (dep-holding on CognitionState).

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
            // Recipe generation (continuum#1295 PR-2)
            // =================================================================
            // AI-driven recipe generator. Wires the prompt+parser+validator
            // shipped in #1295 PR-1 to AIProviderRegistry::generate_text. The
            // TS shim in PR-3 collapses RecipeGenerateServerCommand.ts (371 LOC)
            // to a thin Commands.execute('cognition/generate-recipe', ...) that
            // gathers templates + existing recipe IDs from runtime state,
            // delegates to Rust, and does FS-collision check + save on success.
            //
            // Wire shape: caller sends a JSON object with { request:
            // RecipeGenerationRequest, provider?, model?, temperature? }.
            // Returns { recipe: RecipeDefinitionShape, validationErrors: [] }.
            //
            // Errors propagate as Err(String) for inference/parser failures.
            // Validation errors are returned in the response (not Err) so the
            // shim can render them via the JTAG envelope, matching TS behavior.
            "cognition/generate-recipe" => {
                let _timer = TimingGuard::new("module", "cognition_generate_recipe");

                let request: crate::cognition::generate_recipe::RecipeGenerationRequest =
                    p.json("request")?;
                let orchestrator_params =
                    crate::cognition::generate_recipe::GenerateRecipeOrchestratorParams {
                        request,
                        provider: p.str_opt("provider").map(String::from),
                        model: p.str_opt("model").map(String::from),
                        temperature: p.f32_opt("temperature"),
                    };

                let response =
                    crate::cognition::generate_recipe::generate_recipe_with_ai(orchestrator_params)
                        .await?;

                Ok(CommandResult::Json(
                    serde_json::to_value(&response).map_err(|e| format!("Serialize error: {e}"))?,
                ))
            }

            // =================================================================
            // Peer-review proposal rating (continuum#1289 PR-2)
            // =================================================================
            // AI-driven rater for response proposals. Wires the prompt+parser
            // shipped in #1289 PR-1 to AIProviderRegistry::generate_text. The
            // TS shim in PR-3 collapses ProposalRatingAdapter.ts (252 LOC) to
            // a thin Commands.execute('cognition/rate-proposals', ...) wrapper.
            //
            // Wire shape: caller sends a `RateProposalsRequest` (camelCase
            // ts-rs export). Returns `RateProposalsResponse` with `ratings: []`.
            // Errors propagate as typed Err(String) over IPC; the chat
            // substrate handles "no rater responded" by skipping peer-review
            // for that round, no degraded scoring (no fallback).
            "cognition/rate-proposals" => {
                let _timer = TimingGuard::new("module", "cognition_rate_proposals");
                let request: crate::cognition::rate_proposals::RateProposalsRequest =
                    serde_json::from_value(params.clone())
                        .map_err(|e| format!("Invalid RateProposalsRequest: {e}"))?;

                let response =
                    crate::cognition::rate_proposals::rate_proposals_with_ai(request).await?;

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
            // Migrated to the typed DynCommand registry (Slice 5):
            //   cognition/classify-domain          → commands/cognition/classify_domain.rs
            //   cognition/sync-domain-classifier   → commands/cognition/sync_domain_classifier.rs
            //   cognition/register-domain-keywords → commands/cognition/register_domain_keywords.rs
            // Exposed via commands/cognition/mod.rs::command_objects (dep-holding on CognitionState).

            // =================================================================
            // Domain Activity Tracking & Gap Detection
            // =================================================================
            // Migrated to the typed DynCommand registry (Slice 4):
            //   cognition/genome-record-activity  → commands/cognition/genome_record_activity.rs
            //   cognition/genome-coverage-report  → commands/cognition/genome_coverage_report.rs
            // Exposed via commands/cognition/mod.rs::command_objects (dep-holding on CognitionState).

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
            // ================================================================
            // Recent-message cache + content dedup — MIGRATED to the typed registry.
            // ================================================================
            // `cognition/cache-message`, `cognition/check-content-dedup`, and
            // `cognition/record-content` are now dep-holding `ActionCommand`s in
            // `crate::commands::cognition` (each captures this module's
            // `Arc<CognitionState>` and delegates to `get_or_create_persona` +
            // the per-persona `message_cache` / `content_dedup`). They reach the
            // registry via `CognitionModule::commands()`. All `access: Internal`.

            _ => Err(format!("Unknown cognition command: {command}")),
        }
    }

    fn commands(&self) -> Vec<Arc<dyn crate::sdk_codegen::DynCommand>> {
        crate::commands::cognition::command_objects(self.state.clone())
    }

    fn install_executor(&self, executor: Arc<crate::runtime::CommandExecutor>) {
        self.executor.install(executor);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Fire-and-forget: build the replay record for a drained frame and write it on a
/// blocking pool thread so the hot drain path never stalls on recorder I/O.
///
/// `pub(crate)` because it is the one shared recorder-write for drained frames —
/// consumed by the typed `inbox/drain-frame` command (`commands/cognition/inbox_drain_frame.rs`)
/// and the still-legacy Lane D arms (`persona/drain-turn-frame`, `persona/turn-execute`).
/// When those arms migrate, this can move to the command file with its sole consumer.
pub(crate) fn record_drained_turn_frame(frame: &Option<PersonaInboxFrame>) {
    if let Some(record) = turn_frame_replay_record(frame) {
        tokio::task::spawn_blocking(move || {
            crate::persona::recorder::record_turn_frame_replay(&record);
        });
    }
}

fn turn_frame_replay_record(
    frame: &Option<PersonaInboxFrame>,
) -> Option<PersonaTurnFrameReplayRecord> {
    frame
        .as_ref()
        .and_then(|frame| PersonaTurnFrame::from_inbox_frame(frame.clone()).replay_record())
}

async fn execute_rust_module_json(
    registry: Option<&ModuleRegistry>,
    command: &str,
    params: Value,
) -> Result<Value, String> {
    let registry = registry.ok_or_else(|| {
        format!("{command}: Rust module registry unavailable; refusing TypeScript fallback")
    })?;
    let (module, routed_command) = registry.route_command(command).ok_or_else(|| {
        format!("{command}: no Rust module route registered; refusing TypeScript fallback")
    })?;

    // Project the cell shape into a plain JSON Value. Handle returns
    // its HandleRef as JSON (the caller can hold it and pass back);
    // Stream/Lambda return their not-yet-wired protocol error.
    module
        .handle_command(&routed_command, params)
        .await?
        .to_json_value()
}

#[cfg(test)]
mod turn_frame_recording_tests {
    use super::*;
    use crate::persona::PersonaInboxFrameMetrics;

    fn frame_with_messages(messages: Vec<InboxMessage>) -> PersonaInboxFrame {
        let persona_id = Uuid::new_v4();
        let room_id = messages
            .first()
            .map(|message| message.room_id)
            .unwrap_or_else(Uuid::new_v4);
        let oldest_timestamp = messages
            .iter()
            .map(|message| message.timestamp)
            .min()
            .unwrap_or_default();
        let newest_timestamp = messages
            .iter()
            .map(|message| message.timestamp)
            .max()
            .unwrap_or_default();
        let frame_span_ms = newest_timestamp.saturating_sub(oldest_timestamp);
        PersonaInboxFrame {
            persona_id,
            room_id,
            metrics: PersonaInboxFrameMetrics {
                queue_depth_before: messages.len(),
                queue_depth_after: 0,
                messages_drained: messages.len(),
                oldest_timestamp,
                newest_timestamp,
                frame_span_ms,
                drain_duration_us: 3,
            },
            messages,
        }
    }

    fn message(content: &str, timestamp: u64) -> InboxMessage {
        let room_id = Uuid::new_v4();
        InboxMessage {
            id: Uuid::new_v4(),
            room_id,
            sender_id: Uuid::new_v4(),
            sender_name: "Joel".to_string(),
            sender_type: SenderType::Human,
            content: content.to_string(),
            timestamp,
            priority: 0.9,
            source_modality: Some(Modality::Chat),
            voice_session_id: None,
        }
    }

    #[test]
    fn drained_frame_builds_replay_record_for_background_write() {
        let frame = frame_with_messages(vec![message("record the frame", 20_000)]);
        let record =
            turn_frame_replay_record(&Some(frame)).expect("non-empty frame creates record");

        assert_eq!(
            record.consolidated_inbox.transcript,
            "Joel: record the frame"
        );
        assert_eq!(record.rag_seed.query_text, "Joel: record the frame");
        assert_eq!(record.inbox_frame.metrics.messages_drained, 1);
    }

    #[test]
    fn missing_or_empty_frame_does_not_build_replay_record() {
        let empty = frame_with_messages(vec![]);

        assert!(turn_frame_replay_record(&None).is_none());
        assert!(turn_frame_replay_record(&Some(empty)).is_none());
    }
}

#[cfg(test)]
mod turn_execute_tests {
    //! Lane D persona/turn-execute command surface tests.
    //!
    //! These tests pin the Rust-only shape: success routes through a
    //! `ModuleRegistry` with `InferenceLlmModule` registered; missing registry
    //! or missing route fails loudly instead of falling through to TypeScript.
    use super::*;
    use crate::inference::llm_module_service::InferenceLlmModule;
    use crate::rag::RagEngine;
    use std::sync::Arc;

    fn module_with_persona(persona_id: Uuid) -> CognitionModule {
        module_with_persona_and_registry(persona_id, None)
    }

    fn module_with_persona_and_registry(
        persona_id: Uuid,
        registry: Option<Arc<ModuleRegistry>>,
    ) -> CognitionModule {
        let rag_engine = Arc::new(RagEngine::new());
        let mut state = CognitionState::new(rag_engine.clone());
        if let Some(registry) = registry {
            state = state.with_module_registry(registry);
        }
        let state = Arc::new(state);
        state.personas.insert(
            persona_id,
            crate::persona::PersonaCognition::new(
                persona_id,
                "Test Persona".to_string(),
                rag_engine,
            ),
        );
        CognitionModule::new(state)
    }

    fn rust_inference_registry() -> Arc<ModuleRegistry> {
        let registry = Arc::new(ModuleRegistry::new());
        registry.register(Arc::new(InferenceLlmModule::new()));
        registry
    }

    fn enqueue_message(module: &CognitionModule, persona_id: Uuid, content: &str, timestamp: u64) {
        let room_id = Uuid::new_v4();
        let persona = module
            .state
            .personas
            .get(&persona_id)
            .expect("test persona exists");
        persona.inbox.enqueue(InboxMessage {
            id: Uuid::new_v4(),
            room_id,
            sender_id: Uuid::new_v4(),
            sender_name: "Joel".to_string(),
            sender_type: SenderType::Human,
            content: content.to_string(),
            timestamp,
            priority: 0.9,
            source_modality: Some(Modality::Chat),
            voice_session_id: None,
        });
    }

    #[tokio::test]
    async fn turn_execute_persona_not_found_returns_typed_error() {
        let rag_engine = Arc::new(RagEngine::new());
        let state = Arc::new(CognitionState::new(rag_engine));
        let module = CognitionModule::new(state);

        let missing_persona = Uuid::new_v4();
        let result = module
            .handle_command(
                "persona/turn-execute",
                serde_json::json!({
                    "persona_id": missing_persona.to_string(),
                }),
            )
            .await;

        match result {
            Err(msg) => {
                assert!(
                    msg.contains("No cognition for"),
                    "expected 'No cognition for' in error, got: {msg}"
                );
                assert!(msg.contains(&missing_persona.to_string()));
            }
            Ok(_) => panic!("missing persona must surface typed Err"),
        }
    }

    #[tokio::test]
    async fn turn_execute_empty_drain_returns_null_bundle() {
        // Persona exists but inbox is empty -> the command should
        // short-circuit BEFORE any inference dispatch, returning
        // the documented null pair.
        let persona_id = Uuid::new_v4();
        let module = module_with_persona(persona_id);

        let result = module
            .handle_command(
                "persona/turn-execute",
                serde_json::json!({
                    "persona_id": persona_id.to_string(),
                    "window_ms": 50,
                    "max_items": 8,
                }),
            )
            .await
            .expect("empty drain is a no-op, not an error");

        match result {
            CommandResult::Json(v) => {
                assert_eq!(
                    v.get("replayRecord"),
                    Some(&Value::Null),
                    "empty drain produces null replayRecord; got {v}"
                );
                assert_eq!(
                    v.get("inferenceResponse"),
                    Some(&Value::Null),
                    "empty drain produces null inferenceResponse; got {v}"
                );
            }
            other => panic!("expected CommandResult::Json, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn turn_execute_bad_max_items_returns_typed_error() {
        // Defensive: usize::try_from rejects > usize::MAX (always
        // succeeds on 64-bit but defends 32-bit builds). The
        // happy path validation comes via the empty-drain test
        // above; this one pins the param-parse error path.
        let persona_id = Uuid::new_v4();
        let module = module_with_persona(persona_id);

        let result = module
            .handle_command(
                "persona/turn-execute",
                serde_json::json!({
                    "persona_id": persona_id.to_string(),
                    "max_duration_ms": u64::MAX,
                }),
            )
            .await;
        match result {
            Err(msg) => {
                assert!(
                    msg.contains("max_duration_ms too large"),
                    "expected max_duration_ms overflow error, got: {msg}"
                );
            }
            Ok(_) => panic!("u64::MAX max_duration_ms must fail u32 conversion"),
        }
    }

    #[tokio::test]
    async fn turn_execute_success_routes_through_rust_inference_module() {
        let persona_id = Uuid::new_v4();
        let module = module_with_persona_and_registry(persona_id, Some(rust_inference_registry()));
        enqueue_message(&module, persona_id, "what changed?", 20_000);

        let result = module
            .handle_command(
                "persona/turn-execute",
                serde_json::json!({
                    "persona_id": persona_id.to_string(),
                    "max_tokens": 64,
                    "max_duration_ms": 1_000,
                }),
            )
            .await
            .expect("Rust inference module handles turn");

        let CommandResult::Json(value) = result else {
            panic!("expected Json");
        };
        assert_eq!(
            value["replayRecord"]["responsePrompt"]["messages"][0]["content"],
            "Joel: what changed?"
        );
        assert_eq!(
            value["inferenceResponse"]["complete"]["tokensGenerated"], 3,
            "registered InferenceLlmModule stub proves Rust-only dispatch reached inference"
        );
        assert!(
            module
                .state
                .personas
                .get(&persona_id)
                .expect("persona remains")
                .inbox
                .is_empty(),
            "turn-execute drains one consolidated frame"
        );
    }

    #[tokio::test]
    async fn turn_execute_missing_rust_registry_refuses_ts_fallback() {
        let persona_id = Uuid::new_v4();
        let module = module_with_persona(persona_id);
        enqueue_message(&module, persona_id, "do not fall back to ts", 30_000);

        let result = module
            .handle_command(
                "persona/turn-execute",
                serde_json::json!({
                    "persona_id": persona_id.to_string(),
                }),
            )
            .await;

        match result {
            Err(msg) => assert!(
                msg.contains("refusing TypeScript fallback"),
                "expected loud no-TS-fallback refusal, got: {msg}"
            ),
            Ok(_) => panic!("missing Rust registry must not fall through"),
        }
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
            originator: SignalOriginator::User {
                user_id: Uuid::new_v4(),
            },
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
