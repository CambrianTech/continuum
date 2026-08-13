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
use crate::logging::TimingGuard;
use crate::persona::text_analysis::LoopDetector;
use crate::persona::{
    InboxMessage, Modality, PersonaCognition, PersonaInboxFrame, PersonaTurnFrame,
    PersonaTurnFrameReplayRecord, SenderType,
};
use crate::rag::RagEngine;
use crate::runtime;
use crate::runtime::{
    CommandResult, LateBound, ModuleConfig, ModuleContext, ModulePriority, ServiceModule,
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

    /// Get or lazily create per-persona cognition state, GPU-budget-aware. The one
    /// place that owns the lazy-create policy — every caller (remaining legacy match
    /// arms and the migrated typed commands) routes through here.
    pub fn get_or_create_persona(
        &self,
        persona_uuid: Uuid,
    ) -> dashmap::mapref::one::RefMut<'_, Uuid, PersonaCognition> {
        // Compute the budget BEFORE acquiring the entry shard-lock.
        // `per_persona_budget_mb()` reads `self.personas.len()`, which read-locks
        // EVERY shard. `DashMap::entry()` holds a WRITE lock on the target key's
        // shard; calling `len()` inside `or_insert_with` re-enters that same shard
        // lock, and parking_lot's RwLock is not reentrant → self-deadlock. This is
        // silent in tests (no GpuMemoryManager → the `None` arm skips `len()`) and
        // only bites the live core where `gpu_manager` is `Some`. Hoisting the read
        // out means no lock is held when `len()` runs. The budget is computed even
        // on the cache-hit path (a cheap shard-count sum) but only consumed on
        // insert; correctness over shaving one `len()` off the hit path.
        let budget = self.per_persona_budget_mb();
        self.personas.entry(persona_uuid).or_insert_with(|| {
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
    /// Shared late-bound executor slot. `Arc`-wrapped so the `cognition/vision-describe`
    /// command object can hold the same slot and re-enter the bus for `ai/generate`
    /// (same pattern as the `chat/*` family).
    executor: Arc<LateBound<crate::runtime::CommandExecutor>>,
}

impl CognitionModule {
    pub fn new(state: Arc<CognitionState>) -> Self {
        Self {
            state,
            executor: Arc::new(LateBound::new("cognition::executor")),
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
            // It dispatches via the central registry (so `uu` and every client can
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
            // pub(crate)): it is now called by two migrated commands
            // (commands/cognition/inbox_drain_frame.rs + commands/persona/turn_frame/drain.rs).

            // persona/drain-turn-frame + persona/turn-execute migrated to the typed
            // DynCommand registry (Slices 20 + 21) — see
            // commands/persona/turn_frame/{drain,execute}.rs (dep-holding on CognitionState,
            // access: Internal). The Lane D turn-frame types (PersonaTurnFrameReplayRecord +
            // its subtree) derive TS so the typed Output crosses the wire; execute.rs also
            // carries `execute_rust_module_json` (moved with its sole consumer) — the
            // in-process seam to the Rust inference module, fail-loud on a missing route.

            // cognition/admit-inbox-message + cognition/recall-engrams migrated to the
            // typed DynCommand registry (Slice 9) — see
            // commands/cognition/{admit_inbox_message,recall_engrams}.rs. The wire→domain
            // conversion for admit now reuses InboxMessageRequest::to_inbox_message
            // (ipc/protocol.rs), the one canonical seam.

            // ================================================================
            // Vision Describe (continuum#1276) — MIGRATED to the typed registry.
            // ================================================================
            // `cognition/vision-describe` is now a dep-holding `ActionCommand` in
            // `crate::commands::cognition::vision_describe` (captures this module's
            // shared `Arc<LateBound<CommandExecutor>>` and delegates to
            // `describe_image`). It reaches the registry via
            // `CognitionModule::commands()`. `access: Internal`. No match arm here.

            // ================================================================
            // AI Gating + Draft Redundancy — MIGRATED to the typed registry.
            // ================================================================
            // `cognition/should-respond` and `cognition/check-redundancy` are now
            // stateless `ActionCommand`s in `crate::commands::cognition` (each calls
            // the same free fn — `evaluate_gating` / `evaluate_redundancy` — over its
            // typed request). `route_object` dispatches them via `command_registry()`,
            // so they reach the ACL, codegen, `uu`, and grid routing. Both are
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
            // cognition/full-evaluate migrated to the typed DynCommand registry as a
            // dep-holding action_command! (captures this module's Arc<CognitionState>,
            // takes the persona's rate_limiter + sleep_state + engine + message_cache under
            // one DashMap read lock) — see commands/cognition/full_evaluate.rs (access:
            // Internal), exposed via CognitionModule::commands(). The typed
            // FullEvaluateRequest params deserialize the whole payload in one step
            // (SenderType's lowercase serde matches the old parse_sender_type; the three
            // legacy-defaulted fields carry #[serde(default)]); a request for a persona with
            // no live cognition engine fails loud as CommandError::NotFound.

            // cognition/track-response migrated to the typed DynCommand registry — see
            // commands/cognition/track_response.rs (dep-holding on CognitionState,
            // access: Internal).

            // cognition/set-sleep-mode + cognition/configure-rate-limiter migrated to the
            // typed DynCommand registry (Slice 6) — see
            // commands/cognition/{set_sleep_mode,configure_rate_limiter}.rs (dep-holding on
            // CognitionState, access: Internal).

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

            // MIGRATED to typed ActionCommand (task #62):
            //   cognition/respond → commands/cognition/respond.rs
            // The persona-response pipeline entry point. Dep-holding on
            // CognitionState (admission gate + recall read live per-persona
            // state through it); exposed via commands/cognition/mod.rs::
            // command_objects. See that file's doc comment + SHARED-COGNITION.md.

            // =================================================================
            // Recipe generation (continuum#1295 PR-2)
            // =================================================================
            // cognition/generate-recipe migrated to the typed DynCommand registry as a
            // stateless async action_command! unit struct (free fn generate_recipe_with_ai,
            // no CognitionState) — see commands/cognition/generate_recipe.rs (access:
            // Internal). Params ARE a typed GenerateRecipeOrchestratorParams (the whole
            // { request, provider?, model?, temperature? } payload deserializes in one
            // step, same wire shape as the p.json("request") + p.str_opt/p.f32_opt reads
            // here); inference/parser failures fail loud as CommandError::Internal while
            // structural-validation findings ride back in the response.

            // =================================================================
            // Peer-review proposal rating (continuum#1289 PR-2)
            // =================================================================
            // cognition/rate-proposals migrated to the typed DynCommand registry as a
            // stateless async action_command! unit struct (free fn rate_proposals_with_ai,
            // no CognitionState) — see commands/cognition/rate_proposals.rs (access:
            // Internal). Params ARE a typed RateProposalsRequest (the whole payload
            // deserializes 1:1, as the legacy arm did via from_value(params)); a rater
            // that produces no usable judgment fails loud as CommandError::Internal, no
            // fabricated degraded score.

            // =================================================================
            // Recipe/RAG turn batching boundary
            // =================================================================
            // cognition/plan-turn-batch migrated to the typed DynCommand registry as a
            // stateless action_command! unit struct (pure sync free fn, no CognitionState)
            // — see commands/cognition/plan_turn_batch.rs (access: Internal). The params
            // ARE a typed RecipeTurnBatchRequest now, flattening the legacy
            // `{ request: {...} }` envelope, and deserialize fails loud on a bad payload.

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
            // cognition/gpu-budget migrated to the typed DynCommand registry as a
            // dep-holding action_command! (captures this module's Arc<CognitionState>,
            // reads its optional GpuMemoryManager) — see commands/cognition/gpu_budget.rs
            // (access: Internal), exposed via CognitionModule::commands(). The typed
            // GpuBudgetInfo output replaces the hand-built serde_json::json! object; the
            // GPU-present / CPU-only branches are honest runtime states (no-GPU reports a
            // zeroed device + the CPU per-persona floor), not a happy-path + fallback.

            // =================================================================
            // Interaction Quality Scoring + Post-Inference Adequacy Check
            // =================================================================
            // cognition/score-interaction and cognition/check-adequacy migrated to the
            // typed DynCommand registry as stateless action_command! unit structs (they
            // wrap pure sync free fns — no CognitionState) — see
            // commands/cognition/{score_interaction,check_adequacy}.rs (access: Internal).
            // The typed Vec<RecentResponse> params deserialize fails loud on a malformed
            // batch, replacing the legacy filter_map(..ok()) silent-drop.

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
        let mut objects =
            crate::commands::cognition::command_objects(self.state.clone(), self.executor.clone());
        // Lane D `persona/*` turn-frame verbs carry the persona/ wire prefix but are
        // owned here (they act on the module's per-persona CognitionState). They live
        // under commands/persona/turn_frame per the rag_inspect precedent (path mirrors
        // wire name) yet are contributed from this module's commands(), not the shared
        // persona command_objects.
        objects.extend(crate::commands::persona::turn_frame::command_objects(
            self.state.clone(),
        ));
        objects
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
/// consumed by three migrated commands (`commands/cognition/inbox_drain_frame.rs` +
/// `commands/persona/turn_frame/{drain,execute}.rs`). All Lane D drain paths are now on
/// the typed registry, so this stays a shared `pub(crate)` helper rather than moving into
/// any single command file.
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

    // Nested theme (one test mod per file): the persona lazy-create locking contract.
    mod get_or_create_persona_locking {
        use super::*;
        use crate::gpu::GpuMemoryManager;
        use crate::rag::RagEngine;
        use std::sync::mpsc;
        use std::time::Duration;

        // what this catches: get_or_create_persona must never re-enter its own
        // DashMap shard lock. `entry()` holds a WRITE lock on the target key's
        // shard; the lazy-create closure used to call `per_persona_budget_mb()` →
        // `personas.len()`, which READ-locks every shard including the held one.
        // parking_lot's RwLock is not reentrant, so a live core with a
        // GpuMemoryManager (the `Some` arm that reads `len()`) SELF-DEADLOCKED on
        // the first persona create — the deadlock that hung `cognition/enqueue-message`
        // and every persona turn. Unit tests missed it because the `None` arm (no
        // GPU manager) short-circuits before `len()`. This test wires a real GPU
        // manager so the `Some` arm runs, and guards with a watchdog thread so a
        // regression fails loud in ~10s instead of hanging CI forever.
        // regression for the enqueue-message deadlock; fix = hoist the budget read
        // out of or_insert_with in CognitionState::get_or_create_persona.
        #[test]
        fn create_with_gpu_manager_does_not_self_deadlock() {
            let (done_tx, done_rx) = mpsc::channel();
            std::thread::spawn(move || {
                let rag = Arc::new(RagEngine::new());
                let (ptx, prx) = tokio::sync::watch::channel(0.0f32);
                let gpu = Arc::new(GpuMemoryManager::new_for_test(
                    24 * 1024 * 1024 * 1024, // total VRAM
                    "test-gpu".to_string(),
                    8 * 1024 * 1024 * 1024, // inference budget
                    2 * 1024 * 1024 * 1024, // tts budget
                    2 * 1024 * 1024 * 1024, // rendering budget
                    1024 * 1024 * 1024,     // reserve
                    ptx,
                    prx,
                ));
                let state = CognitionState::new(rag).with_gpu_manager(gpu);
                let id = Uuid::new_v4();
                // First call runs the lazy-create closure (the budget read); the
                // guard drops at the end of each statement, so the second call
                // exercises the cache-hit path without a same-key re-entry.
                let _ = state.get_or_create_persona(id);
                let _ = state.get_or_create_persona(id);
                let _ = done_tx.send(());
            });
            assert!(
                done_rx.recv_timeout(Duration::from_secs(10)).is_ok(),
                "get_or_create_persona self-deadlocked: the budget read re-entered \
                 the entry() shard lock"
            );
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
