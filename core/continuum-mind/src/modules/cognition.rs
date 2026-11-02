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
//! - `cognition/full-evaluate`: Unified 6-gate evaluation (replaces 5 TS gates)
//! - `cognition/track-response`: Track response for rate limiting
//! - `cognition/set-sleep-mode`: Set voluntary sleep mode
//! - `cognition/configure-rate-limiter`: Configure rate limiter params
//! - `cognition/select-model`: 3-tier model priority chain
//! - `cognition/sync-adapters`: Sync adapter registry from TypeScript
//! - `cognition/check-adequacy`: Batch adequacy check
//!
//! The TS-era `cognition/genome-*` family and `sync-domain-classifier` are deleted: which genes
//! a turn runs is `genome::composer`, where they live is `genome::gene_residency`.
//! - `inbox/create`: Create persona inbox (alias for create-engine)
//!
//! Every command is a typed object whose params serde decodes at the boundary: a required field
//! cannot be missing past it.

use crate::gpu::GpuMemoryManager;
use continuum_text_analysis::LoopDetector;
use crate::persona::{PersonaCognition, PersonaInboxFrame, PersonaTurnFrame, PersonaTurnFrameReplayRecord};
use crate::rag::RagEngine;
use crate::runtime::{CommandResult, LateBound, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
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
        // No lock is held across anything but the insert (a `len()` inside `or_insert_with`
        // re-entered the shard lock and self-deadlocked the live core once).
        self.personas.entry(persona_uuid).or_insert_with(|| {
            PersonaCognition::new(persona_uuid, String::new(), self.rag_engine.clone())
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

/// One blocking checkpoint pass shared by periodic servicing and final save.
/// A timed-out caller does not cancel filesystem IO; the registry serializes
/// that work and closes periodic admission before the final snapshot.
enum CheckpointBoundary {
    Periodic,
    Save,
    Shutdown,
}

async fn checkpoint_residents(boundary: CheckpointBoundary) -> Result<(), String> {
    let outcomes = tokio::task::spawn_blocking(move || {
        let registry = crate::cognition::persona_workspace::global();
        match boundary {
            CheckpointBoundary::Periodic => registry.checkpoint_volatile_all(),
            CheckpointBoundary::Save => registry.flush_volatile_all(),
            CheckpointBoundary::Shutdown => registry.stop_volatile_checkpoints(),
        }
    })
    .await
    .map_err(|error| format!("volatile checkpoint task failed: {error}"))?;
    let total = outcomes.len();
    let failed: Vec<_> = outcomes
        .into_iter()
        .filter_map(|(id, result)| result.err().map(|error| format!("{id}: {error}")))
        .collect();
    if failed.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "volatile checkpoint failed for {} of {total} residents: {}",
            failed.len(),
            failed.join("; ")
        ))
    }
}

#[async_trait]
impl ServiceModule for CognitionModule {
    /// Reusable explicit snapshot; periodic persistence remains active.
    async fn save_state(&self) -> Result<(), String> {
        checkpoint_residents(CheckpointBoundary::Save).await
    }

    /// Sample the drain's COMPETITOR on a timer that already runs.
    ///
    /// `turn_ingress::in_flight()` decides whether a stop tears a citizen mid-thought, and
    /// until now it was read in exactly two places — both INSIDE `drain()`. So the depth was
    /// reported only at the moment the drain gave up on it, never before, and the question
    /// the receipts raise ("was that a busy moment, or a node that is never quiet?") had no
    /// instrument. Measured 2026-09-22: 89 deploys on the M5, 76 tearing, 0 clean drains,
    /// 432 turns cut — against one clean drain in 8 on a quieter node. Those two populations
    /// ask for a fix that is not the same in both cases, and nothing could tell them apart.
    ///
    /// This rides cognition's existing 15s tick: no new task, no new cadence, no new concern
    /// — a `probe!` on a timer that is already ticking, which is what the probe guide asks
    /// for at a load-bearing seam.
    ///
    /// SCOPE, so the number is not over-read: this is the SERVICE-LOOP depth. A
    /// `cognition/eval` fork calls the faculties directly and runs UNCOUNTED (see
    /// `turn_ingress`'s own contract), so a 0 here means "no citizen turn", not "nothing
    /// executing".
    async fn tick(&self) -> Result<(), String> {
        continuum_base::probe!(
            class = "cognition.turn_ingress.depth",
            in_flight = crate::cognition::turn_ingress::in_flight(),
            "citizens inside a turn right now — the drain budget's competitor, sampled before the drain instead of only at it"
        );
        checkpoint_residents(CheckpointBoundary::Periodic).await
    }

    async fn shutdown(&self) -> Result<(), String> {
        checkpoint_residents(CheckpointBoundary::Shutdown).await
    }

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
            tick_interval: Some(std::time::Duration::from_secs(15)),
        }
    }

    /// CLOSE THE DOOR AND WAIT FOR THE CITIZENS TO FINISH THEIR TURNS.
    ///
    /// This is the module the drain phase exists for. Everything else in a stopping node
    /// either holds no work or holds a buffer; cognition holds a citizen mid-thought, and
    /// a `save_state` taken across that boundary writes a working memory that is half of
    /// one turn and half of the next.
    ///
    /// Two steps, in this order:
    ///
    /// 1. `turn_ingress::close()` — no service loop admits another turn. NOT
    ///    `quiesce_all`, whose contract is the opposite: a quiesced citizen stops
    ///    wandering and deliberately STAYS reachable, so she would keep starting turns
    ///    while we tried to save her.
    /// 2. Wait for the turns already admitted to finish, bounded.
    ///
    /// The count comes from the permits the loops hold: a turn that has taken a message and
    /// is still composing context holds one before any serving lane, so the drain waits for
    /// exactly the turns whose loss would leave no trace.
    ///
    /// Returns the number still running when the budget expired — a citizen who is still
    /// thinking is a fact the receipt reports, not one it waits forever for.
    async fn drain(&self) -> Result<u32, String> {
        const POLL: std::time::Duration = std::time::Duration::from_millis(50);
        // Inside the runtime's 2s phase bound, so the deadline that fires is this one,
        // with a count, rather than the outer timeout, which produces no number.
        const BUDGET: std::time::Duration = std::time::Duration::from_millis(1_800);

        let first_to_close = crate::cognition::turn_ingress::close();
        let deadline = std::time::Instant::now() + BUDGET;
        while std::time::Instant::now() < deadline {
            if crate::cognition::turn_ingress::in_flight() == 0 {
                return Ok(0);
            }
            tokio::time::sleep(POLL).await;
        }
        let left = crate::cognition::turn_ingress::in_flight();
        continuum_base::probe!(
            class = "cognition.drain.incomplete",
            in_flight = left,
            first_to_close = first_to_close,
            "citizens were still inside turns when the drain budget expired — whatever is saved next is a snapshot taken mid-turn, and the receipt says so"
        );
        Ok(left.min(u32::MAX as u64) as u32)
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        // No init needed.
        Ok(())
    }

    /// Every cognition command is a typed object on the one registry (`commands()`), its params
    /// decoded by serde at the boundary. Reaching this legacy path means a command name no
    /// object registered.
    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        Err(format!("Unknown cognition command: {command}"))
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
    use crate::cognition::conversation::types::{InboxMessage, Modality, SenderType};
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
            sender_name: "Operator".to_string(),
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
            "Operator: record the frame"
        );
        assert_eq!(record.rag_seed.query_text, "Operator: record the frame");
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
