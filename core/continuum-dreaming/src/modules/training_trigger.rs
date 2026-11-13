//! `TrainingTriggerModule` — the journal curriculum producers submit to (the teacher persona's
//! synthesis, the hippocampus's noteworthy drain, settled cards, operator submits), learned in
//! place inside the serving engine.
//!
//! ## Why this module exists
//!
//! Per `[[teacher-synthesizes-in-academy-like-dreaming]]` +
//! `[[noteworthy-flag-feeds-memory-AND-curriculum]]`, examples arrive one at a time. Each is
//! journaled durably per `(persona_id, trait_kind, base_model)` bucket the moment it is
//! accepted, and every pass (the tick and serving's wake) learns the pending examples of each
//! bucket in place on the live engine that serves her base (`in_place`): one optimizer step at
//! a time on her own shadow adapter, checkpointed to a durable generation, which becomes a
//! candidate gene once a competence's worth is learned (`in_place::snapshot`). There is no
//! batch, no threshold and no separate trainer.
//!
//! ## What it does
//!
//! The verbs (`genome/training-trigger/submit`, `/retire`, `/status`) are typed
//! [`DynCommand`](crate::sdk_codegen::DynCommand)s under `commands/training_trigger/`, built
//! over the one shared [`TrainingTriggerState`] (the durable journal and checkpoints, the
//! per-key gate, the in-place learners), so a submit and a learning pass on the same bucket
//! serialize on the same [`PerKeyGate`](crate::runtime::PerKeyGate). Its legacy
//! `handle_command` arms fail loud.
//!
//! ## Replaces what was deleted in #1572
//!
//! The TS-side `channel.rs` trigger this replaces was the
//! cautionary-tale shape:
//!
//! - Fired on raw chat events (wrong signal — chat isn't curated
//!   experience).
//! - Sent minimal params; the validator silently rejected.
//! - Fire-and-forget call site swallowed the rejection.
//!
//! This module is the opposite: substrate-native command surface, typed inputs validated
//! synchronously, a typed outcome on the return path, and an accepted example journaled before
//! the receipt, so it is never lost (per `[[no-fallbacks-ever]]`).
//!
//! ## Doctrinal alignment
//!
//! - `[[commands-are-dumb-daemons-are-smart]]` — the submit command is the dumb door; learning
//!   lives in [`TrainingTriggerState`]'s in-place pass.
//! - `[[no-fallbacks-ever]]` — every code path returns a typed outcome or a typed error.
//! - Receipts follow the configured ORM commit, with that adapter's durability policy. Keys
//!   deduplicate at this destination owner; they do not supply cross-node consensus.
//! - `[[rust-is-the-core-node-is-the-shell]]` — the entire path is substrate-side: producers
//!   submit, the trigger journals, the serving engine learns.

use std::any::Any;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::genome::fine_tuning::types::{LoRAHyperparams, ScheduleParams, TrainingExample, TrainingSource};
use crate::runtime::{CommandExecutor, CommandResult, LateBound, ModuleConfig, ModuleContext, ModulePriority, PerKeyGate, ServiceModule};
use crate::sdk_codegen::DynCommand;

mod durable;
mod serving_wake;
pub(crate) mod competence;
mod in_place;
pub(crate) use durable::{RetireReport, RetireSelection};


/// Default `validation_split` when the submit doesn't pin one. 0.0
/// is conservative — substrate-native datasets are often small
/// enough that a held-out split hurts more than helps; producers
/// who know their data should override.
pub const DEFAULT_VALIDATION_SPLIT: f32 = 0.0;

// ─── Bucket key + state ──────────────────────────────────────────────

/// One bucket per `(persona_id, trait_kind, base_model)` triple.
///
/// Per Reviewer 2's BLOCK A4: keying on `(persona_id, trait_kind)`
/// only and rejecting the second submit with a different `base_model`
/// is the silent-data-loss class the trigger took pains to prevent
/// everywhere else. A persona legitimately trains the same trait
/// against multiple bases (local + cloud) for routing flexibility;
/// each base gets its own bucket and its own dispatched job. The key
/// IS the coherence guarantee — no runtime check needed.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct BucketKey {
    pub(crate) persona_id: Uuid,
    pub(crate) trait_kind: String,
    pub(crate) base_model: String,
}

/// What a bucket pins at its first arrival ([`PendingBatch::policy`]); a later batch
/// that disagrees is refused `InconsistentBucket`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct BucketPolicy {
    pub(crate) source: TrainingSource,
    pub(crate) lora: Option<LoRAHyperparams>,
    pub(crate) schedule: Option<ScheduleParams>,
    pub(crate) validation_split: f32,
    pub(crate) local_artifact_dir: Option<PathBuf>,
    pub(crate) preferred_provider: Option<String>,
    pub(crate) eval_set: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PendingBatch {
    /// This batch holds a RETURN: examples a job handed back, which met a threshold when they
    /// were first dispatched. Persisted with the submission that accepts it, so the recovery
    /// pass dispatches its bucket below the threshold after any restart, exactly once
    /// (`ready_to_recover`; the windows 5090 2026-10-28 20:16Z: a return held at 11 of 16).
    /// Rows accepted before this field read `false`: historical buckets keep their threshold.
    #[serde(default)]
    pub(crate) returned: bool,
    pub(crate) persona_name: String,
    pub(crate) source: TrainingSource,
    pub(crate) examples: Vec<TrainingExample>,
    pub(crate) lora: Option<LoRAHyperparams>,
    pub(crate) schedule: Option<ScheduleParams>,
    pub(crate) local_artifact_dir: Option<PathBuf>,
    pub(crate) preferred_provider: Option<String>,
    pub(crate) validation_split: f32,
    /// The bucket's `cognition/eval` `eval_set` JSONL path (a manual spot-check gym),
    /// carried verbatim onto the dispatched [`TrainingJobRequest::eval_set`]. A
    /// first-arrival-wins bucket policy field like `lora`/`schedule`: a later submit
    /// with a divergent gym is rejected `InconsistentBucket`, never silently merged.
    /// Adoption does not read it: the sentinel opens an in-room trial whether or not
    /// a gym is declared, and the room's card outcomes decide the gene.
    pub(crate) eval_set: Option<String>,
}

/// The shared state the three `genome/training-trigger/*` commands
/// operate over. Held by [`TrainingTriggerModule`] as one
/// `Arc<TrainingTriggerState>` and cloned into each dep-holding
/// command object via [`commands`](TrainingTriggerModule::commands),
/// so submit and flush serialize on the SAME [`PerKeyGate`] and touch
/// the SAME buckets. The late-bound executor is installed once
/// (`install_executor`) and reaches every command through the shared
/// `Arc`.
pub struct TrainingTriggerState {
    /// Per-`(persona_id, trait_kind, base_model)` submit
    /// serialization, backed by the substrate-canonical
    /// [`PerKeyGate`] primitive (`runtime/per_key_gate.rs`). Per
    /// Reviewer 3's BLOCK C1+C2 on PR #1580: concurrent submits
    /// to the same key MUST serialize so the drain→dispatch→remove
    /// path doesn't race a contending submit's append. The gate
    /// auto-evicts via `try_evict` on success — cold keys do not
    /// accumulate (closes the [[auto-clean-is-structural-not-operational]]
    /// concern).
    pub(crate) submit_gates: PerKeyGate<BucketKey>,
    pub(crate) executor: LateBound<CommandExecutor>,
    durable: LateBound<durable::DurableStore>,
    acceptance_gates: PerKeyGate<Uuid>,
    data: LateBound<dyn crate::runtime::StorageProvider>,
    initialization: tokio::sync::Mutex<()>,
    initial_adapter: std::sync::OnceLock<Arc<dyn crate::orm::StorageAdapter>>,
    ready: std::sync::atomic::AtomicBool,
    stopping: std::sync::atomic::AtomicBool,
    next_sequence: std::sync::atomic::AtomicU64,
    /// The learners seated in place and the engines' workspaces (card 4114a2c6).
    in_place: in_place::InPlace,
    /// Finite admitted operations outlive a cancelled caller. Shutdown drains
    /// this set; dropping a caller never aborts a writer or releases its lease.
    operations: tokio::sync::Mutex<tokio::task::JoinSet<()>>,
    #[cfg(test)]
    operation_pause: std::sync::Mutex<Option<Arc<durable::OperationPause>>>,
    /// Test-only: the trial file `ready_to_dispatch` reads, so a test can open a trial
    /// and watch the bucket hold.
    #[cfg(test)]
    pub(crate) test_trials: std::sync::Arc<crate::genome::gene_trial::GeneTrials>,
}

impl TrainingTriggerState {
    /// Test-only: how many examples the bucket holds not yet learned, from the journal; `None`
    /// when it holds none.
    #[cfg(test)]
    pub(crate) async fn bucket_example_count(&self, persona_id: Uuid, trait_kind: &str, base_model: &str) -> Option<usize> {
        let key = BucketKey { persona_id, trait_kind: trait_kind.to_string(), base_model: base_model.to_string() };
        let unlearned = self.unlearned_in(&key).await.ok()?;
        (unlearned > 0).then_some(unlearned as usize)
    }

    /// Test-only: record submission `id`'s first `learned` examples as learned in place.
    #[cfg(test)]
    pub(crate) async fn mark_learned(&self, id: Uuid, learned: u32) -> Result<(), String> {
        self.durable.require()?.record_learned(id, learned).await
    }

    pub(crate) fn new(authority: crate::genome::learner::seat::Authority) -> Self {
        Self {
            submit_gates: PerKeyGate::new(),
            executor: LateBound::new("training-trigger::executor"),
            durable: LateBound::new("training-trigger::durable-store"),
            acceptance_gates: PerKeyGate::new(),
            data: LateBound::new("training-trigger::data"),
            initialization: tokio::sync::Mutex::new(()),
            initial_adapter: std::sync::OnceLock::new(),
            ready: std::sync::atomic::AtomicBool::new(false),
            stopping: std::sync::atomic::AtomicBool::new(false),
            next_sequence: std::sync::atomic::AtomicU64::new(1),
            in_place: in_place::InPlace::new(authority),
            operations: tokio::sync::Mutex::new(tokio::task::JoinSet::new()),
            #[cfg(test)]
            operation_pause: std::sync::Mutex::new(None),
            #[cfg(test)]
            test_trials: std::sync::Arc::new(crate::genome::gene_trial::GeneTrials::at(
                std::env::temp_dir().join(format!("training-trigger-trials-{}.json", Uuid::new_v4())),
            )),
        }
    }




    /// Every recovery pass, in order: what the tick runs AND what a serving change wakes, one
    /// list, so a pass added for one is never missing from the other (Cormac on #5562: the wake
    /// ran two of four, and missed the remainder pass a grown window unblocks).
    /// The trigger's pass: learn every accepted example the live engine's base can learn, in place
    /// (`in_place`). A node serving nothing learns nothing until it serves.
    pub(crate) async fn recover_pass(self: &Arc<Self>) -> Result<(), String> {
        self.ensure_storage().await?; // an interrupted schema setup is retried here, serving or not
        let Some(lane) = crate::inference::lane_registry::live_lane() else {
            return Ok(());
        };
        self.learn_in_place(lane.incarnation(), &lane.model).await
    }

}

// ─── Module ──────────────────────────────────────────────────────────

// The job directory layout and its one reader live with the jobs (genome::fine_tuning::job_dir,
// card 32d2abbb); the trigger reads through them like every other caller.
pub struct TrainingTriggerModule {
    pub(crate) state: Arc<TrainingTriggerState>,
    serving_wake: serving_wake::ServingWake,
}

impl TrainingTriggerModule {
    /// The trigger, learning in place under `authority` (the core's resource authority, serving
    /// lifecycle gate and lease custody).
    pub fn new(authority: crate::genome::learner::seat::Authority) -> Self {
        Self {
            state: Arc::new(TrainingTriggerState::new(authority)),
            serving_wake: Default::default(),
        }
    }
}

#[async_trait]
impl ServiceModule for TrainingTriggerModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "training-trigger",
            priority: ModulePriority::Normal,
            command_prefixes: &["genome/training-trigger/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 0,
            tick_interval: Some(std::time::Duration::from_secs(30)),
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        self.state.data.install(ctx.storage("training-trigger")?);
        self.state.ensure_storage().await?;
        self.serving_wake.start(self.state.clone()).await;
        Ok(())
    }

    async fn tick(&self) -> Result<(), String> {
        // The tick and serving changes have one recovery owner: concurrent
        // passes must not drain each other's ended-job list before requeue.
        if self.serving_wake.request_recovery() {
            Ok(())
        } else {
            self.state.recover_pass().await
        }
    }

    /// Expose the three dep-holding training-trigger verbs over this
    /// module's shared [`TrainingTriggerState`] — submit / flush /
    /// status all bind the SAME buckets + per-key gate.
    fn commands(&self) -> Vec<Arc<dyn DynCommand>> {
        crate::commands::training_trigger::command_objects(self.state.clone())
    }

    async fn handle_command(&self, command: &str, _params: Value) -> Result<CommandResult, String> {
        // All three verbs are migrated to the typed registry
        // (commands/training_trigger/). They route via `route_object`
        // against THIS module's shared state (contributed by
        // `commands()`); the legacy path must fail loud.
        match command {
            "genome/training-trigger/submit"
            | "genome/training-trigger/retire"
            | "genome/training-trigger/status" => Err(format!(
                "'{command}' is migrated to the typed registry \
                 (commands/training_trigger/) — it must route via route_object, \
                 not the legacy handle_command path"
            )),
            other => Err(format!("unknown training-trigger command: {other}")),
        }
    }

    fn install_executor(&self, executor: Arc<CommandExecutor>) {
        self.state.executor.install(executor);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    /// Stop admitting passes and report the learning operations still running: a step in flight
    /// is counted, never reported as drained.
    async fn drain(&self) -> Result<u32, String> {
        self.serving_wake.stop().await;
        let in_flight = self.state.stop_admitting().await;
        u32::try_from(in_flight).map_err(|_| format!("{in_flight} learning operations in flight do not fit the drain count"))
    }

    /// Publish and page out every learner whose engine is idle; a learner mid-step is left to her
    /// step (her progress since her newest generation is learned again from the journal if the
    /// join does not reach it). Fails, naming the count, when a learner's progress was not published.
    async fn save_state(&self) -> Result<(), String> {
        let quiesced = self.state.in_place.quiesce(&self.state, in_place::Busy::Skip).await;
        continuum_base::probe!(class = "training.in_place.saved", stopped = quiesced.stopped, unpublished = quiesced.unpublished,
            unconfirmed = quiesced.unconfirmed, busy = quiesced.busy, "at save, every idle engine's learners published and paged out; a busy engine is left to its step");
        match quiesced.unpublished {
            0 => Ok(()),
            unpublished => Err(format!("{unpublished} learners' progress since their newest generation was not published; the journal still holds it")),
        }
    }

    async fn shutdown(&self) -> Result<(), String> {
        self.serving_wake.stop().await;
        self.state.drain_operations().await
    }
}

