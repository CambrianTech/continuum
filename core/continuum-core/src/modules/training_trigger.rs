//! `TrainingTriggerModule` — substrate-native batching coordinator
//! that sits between curriculum producers (the teacher persona's
//! synthesis, the hippocampus's noteworthy drain, operator submits)
//! and the [`super::genome::GenomeModule`]'s `genome/job-create`
//! command.
//!
//! ## Why this module exists
//!
//! Per `[[teacher-synthesizes-in-academy-like-dreaming]]` +
//! `[[noteworthy-flag-feeds-memory-AND-curriculum]]`, examples
//! arrive in dribs — one engram at a time, one synthesis call at a
//! time. Firing `genome/job-create` per example would be wasteful;
//! waiting for the producer to accumulate a "right-sized" batch
//! pushes batching policy out of the substrate and into N callers
//! that would each implement it slightly differently. The substrate
//! should own that policy ONCE, here.
//!
//! ## What it does
//!
//! Owns a per-`(persona_id, trait_kind, base_model)` bucket of
//! accumulating [`TrainingExample`]s. The three verbs —
//! `genome/training-trigger/submit`, `/flush`, `/status` — are
//! migrated to the typed [`DynCommand`](crate::sdk_codegen::DynCommand)
//! registry under `commands/training_trigger/` (task #62). This module
//! retains the shared [`TrainingTriggerState`] (durable submissions/intents,
//! bucket caches, the per-key submit gate, and the late-bound executor);
//! the verbs are dep-holding commands
//! built over that one `Arc<TrainingTriggerState>` so submit and flush
//! serialize on the SAME `PerKeyGate` and mutate the SAME buckets.
//! Its legacy `handle_command` arms now fail loud.
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
//! This module is the opposite: substrate-native command surface,
//! typed inputs validated synchronously, typed outcome on the
//! return path, batch state is NEVER lost on dispatch failure (per
//! `[[no-fallbacks-ever]]`).
//!
//! ## Doctrinal alignment
//!
//! - `[[commands-are-dumb-daemons-are-smart]]` — the submit command
//!   is the dumb door; the smart bits (batching, threshold logic,
//!   dispatch) live in [`TrainingTriggerState`].
//! - `[[no-fallbacks-ever]]` — every code path returns a typed
//!   outcome or a typed error. Explicit refusals can retry; uncertain
//!   provider creation requires exact JobBoard evidence before it resolves.
//! - Receipts follow the configured ORM commit, with that adapter's durability
//!   policy. Keys deduplicate at this destination owner; they do not supply
//!   cross-node consensus or exactly-once provider dispatch.
//! - `[[rust-is-the-core-node-is-the-shell]]` — entire path is
//!   substrate-side. The teacher persona (Rust) submits; the trigger
//!   (Rust) batches; the genome module (Rust) dispatches; the local
//!   trainer (Rust) produces the safetensors.

use std::any::Any;
use std::path::PathBuf;
use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use crate::genome::fine_tuning::types::{
    JobHandle, LoRAHyperparams, ScheduleParams, TrainingDataset, TrainingExample,
    TrainingJobRequest, TrainingSource,
};
use crate::runtime::{
    CommandExecutor, CommandResult, LateBound, ModuleConfig, ModuleContext, ModulePriority,
    PerKeyGate, ServiceModule,
};
use crate::sdk_codegen::DynCommand;

mod durable;
pub use durable::{AcceptanceReceipt, DispatchPhase};
pub(crate) use durable::{DispatchFailure, DispatchResult};

/// Default per-bucket fire threshold. 16 examples is a healthy
/// THE BOUND ON A HELD BUCKET. A bucket held by a job in flight, a trial open, or her
/// surprise below the floor keeps filling, in memory and durably, until the hold lifts
/// (Cormac on #4794: a trial with no verdict grew it without limit). Past this many
/// pending examples a submit into a held bucket is refused as `BucketHeldFull`; the
/// producer keeps its staged rows (a refused submit is "evidence retained") and settles
/// them on a later pass, so nothing is lost, only deferred. Sixteen fills' worth at the
/// default floor: more than the fill after the hold lifts can use at once.
pub const MAX_HELD_EXAMPLES: usize = 16 * DEFAULT_MIN_EXAMPLES as usize;

/// LoRA-training floor — large enough to give SGD signal,
/// small enough that latency-to-first-layer stays minutes not hours
/// on the substrate-native trainer. Override per-submit via
/// `SubmitParams::min_examples`.
pub const DEFAULT_MIN_EXAMPLES: u32 = 16;

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct PendingBatch {
    /// Durable submission identities; payloads are stored separately, once each.
    #[serde(skip)]
    pub(crate) submission_ids: Vec<Uuid>,
    pub(crate) persona_name: String,
    pub(crate) source: TrainingSource,
    pub(crate) examples: Vec<TrainingExample>,
    pub(crate) lora: Option<LoRAHyperparams>,
    pub(crate) schedule: Option<ScheduleParams>,
    pub(crate) local_artifact_dir: Option<PathBuf>,
    pub(crate) preferred_provider: Option<String>,
    pub(crate) min_examples: u32,
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
    pub(crate) buckets: Arc<DashMap<BucketKey, PendingBatch>>,
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
    /// The boot's orphaned jobs are resumed once, on the first tick with a live executor.
    resumed_orphans: std::sync::atomic::AtomicBool,
    /// Orphans whose run may STILL be training (re-attach answered uncertain or not at all),
    /// each with when to ask again. Re-asked on later ticks until the answer is positive
    /// evidence either way; never resumed while held (Cormac on #4537: asked once and dropped,
    /// a transiently silent engine held serving off it for its whole life).
    held_orphans: std::sync::Mutex<Vec<(crate::genome::fine_tuning::job_board::OrphanedJob, u64)>>,
    durable: LateBound<durable::DurableStore>,
    acceptance_gates: PerKeyGate<Uuid>,
    active_dispatches: DashMap<BucketKey, Arc<durable::ActiveDispatch>>,
    hydrating: DashMap<BucketKey, durable::BucketHydration>,
    hydrated: dashmap::DashSet<BucketKey>,
    data: LateBound<crate::modules::data::DataState>,
    initialization: tokio::sync::Mutex<()>,
    initial_adapter: std::sync::OnceLock<Arc<dyn crate::orm::StorageAdapter>>,
    recovery: tokio::sync::Mutex<()>,
    ready: std::sync::atomic::AtomicBool,
    stopping: std::sync::atomic::AtomicBool,
    next_sequence: std::sync::atomic::AtomicU64,
    pending_scan_sequence: std::sync::atomic::AtomicU64,
    active_scan_after: std::sync::Mutex<Option<String>>,
    cached_scan_after: std::sync::Mutex<Option<BucketKey>>,
    initial_scan: std::sync::atomic::AtomicU8,
    /// Finite admitted operations outlive a cancelled caller. Shutdown drains
    /// this set; dropping a caller never aborts a writer or releases its lease.
    operations: tokio::sync::Mutex<tokio::task::JoinSet<()>>,
    #[cfg(test)]
    operation_pause: std::sync::Mutex<Option<Arc<durable::OperationPause>>>,
    #[cfg(test)]
    pub(crate) test_job_board: Arc<crate::genome::fine_tuning::TrainingJobBoard>,
    /// Test-only: the trial file `ready_to_dispatch` reads, so a test can open a trial
    /// and watch the bucket hold.
    #[cfg(test)]
    pub(crate) test_trials: std::sync::Arc<crate::genome::gene_trial::GeneTrials>,
}

/// What the trigger does with one orphan, given what re-attach answered (step 3).
#[derive(Debug)]
enum OrphanStep {
    /// Its run is still going: watch it under the same id for the sentinel's chain.
    Register(Box<crate::genome::fine_tuning::WatchedJob>),
    /// Its run may still be training: hold it and ask again later. Never resumed.
    Hold,
    /// Nothing is resident: resume it from its input, as before step 3.
    Resume,
}

/// PURE: the step for an orphan from re-attach's answer. Only `Resume` may reach
/// `genome/job-create`, and only an answer that `permits_resume`; an answer that could not be
/// had at all holds, like an uncertain one.
fn orphan_step(
    orphan: &crate::genome::fine_tuning::job_board::OrphanedJob,
    answer: Result<crate::genome::fine_tuning::ReattachOutcome, String>,
) -> OrphanStep {
    use crate::genome::fine_tuning::ReattachOutcome;
    match answer {
        Ok(ReattachOutcome::Attached { handle }) => OrphanStep::Register(Box::new(crate::genome::fine_tuning::WatchedJob {
            trigger_dispatch_id: orphan.trigger_dispatch_id,
            handle,
            persona_id: orphan.persona_id,
            persona_name: orphan.persona_name.clone(),
            base_model: orphan.base_model.clone(),
            trait_kind: orphan.trait_kind.clone(),
            eval_set: orphan.eval_set.clone(),
            // the gene's signature was minted in the dead core and is not journaled; the gene
            // still adopts, routed by the fallback path
            signature: None,
                decision: None,
        })),
        Ok(outcome) if outcome.permits_resume() => OrphanStep::Resume,
        Ok(_) | Err(_) => OrphanStep::Hold,
    }
}

/// How long a held orphan waits before re-attach is asked again: long enough that an engine
/// that stays silent costs a probe a minute, short enough that serving is freed within a minute
/// of the engine answering.
const HELD_ORPHAN_REASK_MS: u64 = 60_000;

impl TrainingTriggerState {
    pub(crate) fn new() -> Self {
        Self {
            buckets: Arc::new(DashMap::new()),
            submit_gates: PerKeyGate::new(),
            executor: LateBound::new("training-trigger::executor"),
            resumed_orphans: std::sync::atomic::AtomicBool::new(false),
            held_orphans: std::sync::Mutex::new(Vec::new()),
            durable: LateBound::new("training-trigger::durable-store"),
            acceptance_gates: PerKeyGate::new(),
            active_dispatches: DashMap::new(),
            hydrating: DashMap::new(),
            hydrated: dashmap::DashSet::new(),
            data: LateBound::new("training-trigger::data"),
            initialization: tokio::sync::Mutex::new(()),
            initial_adapter: std::sync::OnceLock::new(),
            recovery: tokio::sync::Mutex::new(()),
            ready: std::sync::atomic::AtomicBool::new(false),
            stopping: std::sync::atomic::AtomicBool::new(false),
            next_sequence: std::sync::atomic::AtomicU64::new(1),
            pending_scan_sequence: std::sync::atomic::AtomicU64::new(0),
            active_scan_after: std::sync::Mutex::new(None),
            cached_scan_after: std::sync::Mutex::new(None),
            initial_scan: std::sync::atomic::AtomicU8::new(0),
            operations: tokio::sync::Mutex::new(tokio::task::JoinSet::new()),
            #[cfg(test)]
            operation_pause: std::sync::Mutex::new(None),
            #[cfg(test)]
            test_job_board: Arc::new(crate::genome::fine_tuning::TrainingJobBoard::default()),
            #[cfg(test)]
            test_trials: std::sync::Arc::new(crate::genome::gene_trial::GeneTrials::at(
                std::env::temp_dir().join(format!("training-trigger-trials-{}.json", Uuid::new_v4())),
            )),
        }
    }

    /// Test-only: count of currently-pending buckets. Useful for
    /// asserting "bucket cleared after dispatch" without exposing
    /// internal state to production callers.
    #[cfg(test)]
    pub(crate) fn pending_bucket_count(&self) -> usize {
        self.buckets.len() + self.active_dispatches.len()
    }

    /// Test-only: peek the example count for a specific bucket. None
    /// if the bucket doesn't exist (cleared or never created).
    #[cfg(test)]
    pub(crate) fn bucket_example_count(
        &self,
        persona_id: Uuid,
        trait_kind: &str,
        base_model: &str,
    ) -> Option<usize> {
        let key = BucketKey {
            persona_id,
            trait_kind: trait_kind.to_string(),
            base_model: base_model.to_string(),
        };
        let pending = self.buckets.get(&key).map(|b| b.examples.len());
        let active = self
            .active_dispatches
            .get(&key)
            .map(|b| b.batch.examples.len());
        match (pending, active) {
            (None, None) => None,
            (pending, active) => Some(pending.unwrap_or(0) + active.unwrap_or(0)),
        }
    }

    /// Build a `TrainingJobRequest` from a drained `PendingBatch`,
    /// dispatch `genome/job-create` via the late-bound executor, and
    /// return the (job_handle, selected_provider) pair on success.
    /// Shared by submit and flush so both fire through exactly one
    /// dispatch path.
    /// Resume the jobs a previous core left in flight (card 244757bc). Once per boot,
    /// on the first tick with a live executor: the board journals each as dead (it is),
    /// and for each whose job directory still holds its `request.json` — the whole
    /// dataset and spec, written before the trainer started — a fresh job is created
    /// from that input on the same provider, under a new dispatch id. Bounded by
    /// [`crate::genome::fine_tuning::job_board::MAX_RESUMES`] per lineage. Every
    /// outcome is a probe; a job that cannot be resumed is said, never silently dropped.
    ///
    /// Why here and not the board: the board owns the ledger, the trigger owns the
    /// executor that reaches `genome/job-create`, and the resume needs both.
    ///
    /// Every later tick re-asks the HELD orphans (re-attach uncertain or unanswered) once their
    /// wait is up, so an engine that answers later is re-attached or released then; a held
    /// orphan is never resumed (step 3, Cormac on #4537).
    pub(crate) async fn resume_orphans_once(&self) {
        use std::sync::atomic::Ordering;
        if self.resumed_orphans.load(Ordering::Acquire)
            && self.held_orphans.lock().map_or(true, |held| held.is_empty())
        {
            return; // boot's orphans handled and nothing held: the common tick costs one lock
        }
        let Ok(executor) = self.executor.require() else {
            return; // not installed yet: try again next tick, the orphans wait on the board
        };
        let board = crate::genome::fine_tuning::job_board::TrainingJobBoard::global();
        let now = crate::persona::trace::now_ms();
        // the held ones whose time to ask again has come, and (once) the boot's orphans
        let mut due = self.take_due_orphans(now);
        if !self.resumed_orphans.swap(true, Ordering::AcqRel) {
            due.extend(board.take_orphans());
        }
        for orphan in due {
            // STEP 3 (SHARED-RESIDENT-LIFECYCLE.md): the job's run may have OUTLIVED the core
            // (an in-engine run survives a core-only restart). Ask its adapter first; only an
            // answer that permits it resumes, since a resume is a second POST into an engine
            // the first run may still be training in. One rule: `permits_resume`.
            let answer = self.reattach(&executor, &orphan).await;
            if let Err(error) = &answer {
                crate::probe!(
                    class = "training.job.reattach_unanswered",
                    local_id = %orphan.local_id,
                    error = %error,
                    "whether this job's run outlived the core could not be asked, so it is NOT started again; asked again later"
                );
            }
            match orphan_step(&orphan, answer) {
                OrphanStep::Register(watched) => {
                    board.register(*watched);
                    continue;
                }
                OrphanStep::Hold => {
                    // held: asked again later, never resumed meanwhile
                    self.hold_orphan(orphan, now);
                    continue;
                }
                OrphanStep::Resume => {}
            }
            let origin = board.resume_origin(orphan.local_id);
            let attempt = board.resume_attempts(origin) + 1;
            if attempt > crate::genome::fine_tuning::job_board::MAX_RESUMES {
                crate::probe!(
                    class = "training.job.resume_refused",
                    origin = %origin,
                    local_id = %orphan.local_id,
                    attempts = attempt - 1,
                    "a job that dies at every boot is broken, not interrupted — no further resume"
                );
                continue;
            }
            let Some(dir) = default_job_dir(&orphan.persona_name, &orphan.trait_kind, orphan.local_id) else {
                crate::probe!(
                    class = "training.job.not_resumable",
                    local_id = %orphan.local_id,
                    error = "no home directory: the job directory cannot be found",
                    "the job's request cannot be read back, so there is nothing to resume from"
                );
                continue;
            };
            let dispatch_id = Uuid::new_v4();
            let params = match resumable_request(&dir, &orphan.provider_id, dispatch_id) {
                Ok(params) => params,
                Err(error) => {
                    crate::probe!(
                        class = "training.job.not_resumable",
                        local_id = %orphan.local_id,
                        dir = %dir.display(),
                        error = %error,
                        "the job's request cannot be read back, so there is nothing to resume from"
                    );
                    continue;
                }
            };
            match executor.execute_json("genome/job-create", params.clone()).await.map_err(|e| e.to_string()).and_then(|r| decode_job_create(r).map_err(|e| format!("{e:?}"))) {
                // A competence already training, on trial, or carried by an existing gene:
                // the orphan's examples go back to her bucket through the one submit verb
                // (the bucket waits while that job or trial is pending), and nothing is
                // re-created. The orphan is not resumed.
                Ok(Created::Held(took)) => {
                    // Her trial file could not be read: nothing is returned or journaled
                    // blind; the orphan stays for the next restart, which reads again.
                    if matches!(took, Took::TrialFileUnreadable) {
                        crate::probe!(
                            class = "training.job.resume_deferred",
                            origin = %origin,
                            from = %orphan.local_id,
                            "an orphan's fate waits on her trial file, which could not be read; the next restart tries again"
                        );
                        continue;
                    }
                    let resubmitted = match read_job_request(&dir).and_then(|request| {
                        serde_json::to_value(crate::commands::training_trigger::submit::SubmitParams::returning(request, orphan.local_id))
                            .map_err(|e| format!("the returned batch does not serialize: {e}"))
                    }) {
                        Ok(returned) => executor
                            .execute_json("genome/training-trigger/submit", returned)
                            .await
                            .map_err(|e| e.to_string())
                            .and_then(|out| {
                                // The call returning is not the batch being taken: a refused
                                // submit (InconsistentBucket, …) returns success=false.
                                let outcome: crate::commands::training_trigger::submit::SubmitOutcome =
                                    serde_json::from_value(out).map_err(|e| format!("submit's outcome is unreadable: {e}"))?;
                                if outcome.success {
                                    Ok(())
                                } else {
                                    Err(format!("{:?}: {:?}", outcome.error_kind, outcome.error))
                                }
                            }),
                        Err(error) => Err(error),
                    };
                    // JOURNALED AS RESOLVED (BigMama on #4791): the orphan's examples now
                    // live in her bucket, so this orphan is resumed-into the joined job and
                    // the next restart must not return the same examples again. The same
                    // `resumed` row the Job arm writes, with the joined job as the new id;
                    // a failed resubmit leaves the orphan for the next restart to try.
                    if resubmitted.is_ok() {
                        match &took {
                            // Its input continues in that job: lineage.
                            Took::Joined { job } => board.journal_resumed(origin, orphan.local_id, *job, attempt),
                            // Its input waits in her bucket; nothing continues it. Lineage ends.
                            Took::Awaited { .. } | Took::Reused { .. } | Took::Unsurprised { .. } => {
                                board.journal_returned(origin, orphan.local_id, serde_json::to_value(&took).unwrap_or(Value::Null), attempt) // unwrap_or: a Took always serializes; Null would only follow a serde bug
                            }
                            Took::TrialFileUnreadable => {} // guarded above: never reaches a resubmit
                        }
                    }
                    crate::probe!(
                        class = "training.job.resume_joined",
                        origin = %origin,
                        from = %orphan.local_id,
                        held_by = ?took,
                        examples_returned = resubmitted.is_ok(),
                        "an orphan whose competence is already pending (a job, a trial, an existing gene, or no surprise) was not re-created: its examples returned to her bucket"
                    );
                }
                Ok(Created::Job(handle, provider)) => {
                    board.journal_resumed(origin, orphan.local_id, handle.local_id, attempt);
                    crate::probe!(
                        class = "training.job.resumed",
                        origin = %origin,
                        from = %orphan.local_id,
                        to = %handle.local_id,
                        provider = %provider,
                        attempt,
                        "an in-flight training job survived the reboot: re-created from its own request.json"
                    );
                }
                Err(error) => {
                    crate::probe!(
                        class = "training.job.resume_failed",
                        origin = %origin,
                        local_id = %orphan.local_id,
                        error = %error,
                        "the resume dispatch was refused — the input still sits in the job directory"
                    );
                }
            }
        }
    }

    /// Hold an orphan whose run may still be training, to be asked again after
    /// [`HELD_ORPHAN_REASK_MS`].
    fn hold_orphan(&self, orphan: crate::genome::fine_tuning::job_board::OrphanedJob, now_ms: u64) {
        if let Ok(mut held) = self.held_orphans.lock() {
            held.push((orphan, now_ms + HELD_ORPHAN_REASK_MS));
        }
    }

    /// The held orphans whose wait is up at `now_ms`, taken out of the held set to be asked again.
    fn take_due_orphans(&self, now_ms: u64) -> Vec<crate::genome::fine_tuning::job_board::OrphanedJob> {
        let Ok(mut held) = self.held_orphans.lock() else {
            return Vec::new(); // a poisoned lock yields nothing to ask, never a panic in the tick
        };
        let (due, waiting): (Vec<_>, Vec<_>) = std::mem::take(&mut *held).into_iter().partition(|(_, next)| *next <= now_ms);
        *held = waiting;
        due.into_iter().map(|(orphan, _)| orphan).collect()
    }

    /// Ask the orphan's adapter, through `genome/job-reattach`, whether its run outlived the
    /// core. A job with no recorded provider was never an in-engine run: not resident.
    async fn reattach(
        &self,
        executor: &CommandExecutor,
        orphan: &crate::genome::fine_tuning::job_board::OrphanedJob,
    ) -> Result<crate::genome::fine_tuning::ReattachOutcome, String> {
        if orphan.provider_id.is_empty() {
            return Ok(crate::genome::fine_tuning::ReattachOutcome::NotResident);
        }
        let params = serde_json::json!({ "providerId": orphan.provider_id, "localId": orphan.local_id });
        let answer = executor.execute_json("genome/job-reattach", params).await.map_err(|e| e.to_string())?;
        serde_json::from_value(answer).map_err(|e| format!("genome/job-reattach answered an unreadable outcome: {e}"))
    }

    pub(crate) async fn dispatch_job_create(
        &self,
        persona_id: Uuid,
        trait_kind: &str,
        base_model: &str,
        batch: &PendingBatch,
        dispatch_id: Uuid,
    ) -> Result<Created, DispatchFailure> {
        let executor = self
            .executor
            .require()
            .map_err(DispatchFailure::Retryable)?;

        let request = TrainingJobRequest {
            persona_id,
            persona_name: batch.persona_name.clone(),
            base_model: base_model.to_string(),
            trait_kind: trait_kind.to_string(),
            resume_from: None,
            parent: None,
            dataset: TrainingDataset {
                examples: batch.examples.clone(),
                source: batch.source,
                validation_split: batch.validation_split,
            },
            eval_set: batch.eval_set.clone(),
            lora: batch.lora.clone(),
            schedule: batch.schedule.clone(),
            local_artifact_dir: batch.local_artifact_dir.clone(),
        };

        let mut params = serde_json::to_value(&request).map_err(|e| {
            DispatchFailure::Retryable(format!("serialize TrainingJobRequest: {e}"))
        })?;
        params["triggerDispatchId"] = Value::String(dispatch_id.to_string());
        if let Some(provider) = &batch.preferred_provider {
            if let Value::Object(ref mut map) = params {
                map.insert("preferredProvider".into(), Value::String(provider.clone()));
            }
        }

        let response = executor
            .execute_json("genome/job-create", params)
            .await
            .map_err(|e| DispatchFailure::Uncertain(format!("genome/job-create dispatch: {e}")))?;

        let result = decode_job_create(response)?;

        // L2→L3 retention (the board write) lives at the ONE birth-seam every
        // training job funnels through — the `genome/job-create` command body that
        // this helper dispatches to above — NOT here. That keeps a single
        // registration point for the trigger path, a direct `uu genome/job-create`,
        // and any future caller alike (compression principle), instead of one writer
        // per caller that silently misses jobs born off this path.
        // ([[dev-task-learning-loop-gap-map]] L3, docs/genome/DEV-TASK-LOOP-CLOSURE-PLAN.md)
        Ok(result)
    }
}

// ─── Module ──────────────────────────────────────────────────────────

/// Consume the existing typed command envelope. Only an explicit refusal is
/// evidence that retry is safe; malformed/transport responses may follow creation.
/// Where a native job's directory lives when the request named no `local_artifact_dir`
/// — the same rule as `native_jobs::job_dir_for`, from the ledger's fields.
pub(crate) fn default_job_dir(persona_name: &str, trait_kind: &str, local_id: Uuid) -> Option<std::path::PathBuf> {
    Some(job_dir_under(&genome_root()?, persona_name, trait_kind, local_id))
}

/// Where this node keeps its persona genomes (and their job directories).
pub(crate) fn genome_root() -> Option<std::path::PathBuf> {
    dirs::home_dir().map(|home| home.join(".continuum/genome"))
}

/// A job's directory under a genome root: THE one layout of persona/trait/job.
pub(crate) fn job_dir_under(root: &std::path::Path, persona_name: &str, trait_kind: &str, local_id: Uuid) -> std::path::PathBuf {
    root
        .join(persona_name.replace(['/', ' '], "_"))
        .join(trait_kind.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect::<String>())
        .join(local_id.to_string())
}

/// PURE over the job directory: the `genome/job-create` params that re-create a job from
/// its own `request.json` (the flattened `TrainingJobRequest` the trainer was handed,
/// with the adapter's admission fields alongside, which the request type ignores).
/// `None` when there is nothing on disk to resume from.
/// A job's own request, read from its directory as the trigger first sent it. THE one
/// reader of a job's `request.json` (the resume and `genome/training-trigger/return`).
pub(crate) fn read_job_request(dir: &std::path::Path) -> Result<TrainingJobRequest, String> {
    let path = dir.join("request.json");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let spec: Value = serde_json::from_str(&text).map_err(|e| format!("{}: not JSON: {e}", path.display()))?;
    let mut request: TrainingJobRequest =
        serde_json::from_value(spec.clone()).map_err(|e| format!("{}: not a training request: {e}", path.display()))?;
    // The spec on disk is the PLANNER's: the adapter replaced `baseModel` with the
    // trainable HF base (hf_source) and kept the registry id in `canonicalBase`.
    // job-create takes the registry id, so the resume must too. Measured on the 5090
    // 2026-10-05: all five killed-by-reboot jobs failed to resume with "cannot resolve
    // hf_source for 'Qwen/Qwen3.8-27B'": the HF id fed back where a registry id belongs.
    if let Some(canonical) = spec.get("canonicalBase").and_then(Value::as_str).filter(|s| !s.trim().is_empty()) {
        request.base_model = canonical.to_string();
    }
    Ok(request)
}

pub(crate) fn resumable_request(dir: &std::path::Path, provider: &str, dispatch_id: Uuid) -> Result<Value, String> {
    let mut request = read_job_request(dir)?;
    // The dead job's latest checkpoint, when it got that far: the re-created job
    // continues from it instead of from zero (Fable's readiness ask: a resume loses
    // minutes, not the run). `checkpoints/LATEST` names the directory the trainer
    // finished writing; a missing pointer is a job that never reached its first
    // checkpoint and starts over.
    if let Some(latest) = std::fs::read_to_string(dir.join("checkpoints").join("LATEST"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
    {
        let checkpoint = dir.join("checkpoints").join(latest);
        if checkpoint.join("adapter_config.json").is_file() {
            request.resume_from = Some(checkpoint);
        }
    }
    // The dead job's trainer is pinned ONLY when it left a checkpoint: a checkpoint is that
    // trainer's format (a PEFT directory cannot continue in-engine). With none, the job never
    // trained, and the coordinator chooses again. Measured on the 5090 2026-10-05: eight of
    // Kimi's jobs sat on cuda-local waiting for a second 29.9 GB copy beside the lane that
    // could have trained them in place; a pinned resume would have parked them there again.
    let pin = request.resume_from.is_some() && !provider.is_empty();
    let mut params = serde_json::to_value(&request).map_err(|e| format!("the request does not serialize: {e}"))?;
    params["triggerDispatchId"] = Value::String(dispatch_id.to_string());
    if pin {
        params["preferredProvider"] = Value::String(provider.to_string());
    }
    Ok(params)
}

use crate::commands::genome::job_create::Took;

/// What `genome/job-create` did with a fill: created a job, or HELD the fill without one
/// (`JobCreateOutcome::took`: joined a job of hers, awaited a trial, or adopted an
/// existing gene for trial). In every held case the examples belong back in her bucket.
#[derive(Debug, Clone)]
pub(crate) enum Created {
    Job(JobHandle, String),
    Held(Took),
}

fn decode_job_create(response: Value) -> Result<Created, DispatchFailure> {
    use crate::commands::genome::job_create::JobCreateOutcome;
    if response
        .get("errorKind")
        .is_some_and(|kind| !kind.is_string())
    {
        return Err(DispatchFailure::Uncertain(
            "genome/job-create has malformed errorKind".into(),
        ));
    }
    let response: JobCreateOutcome = serde_json::from_value(response).map_err(|error| {
        DispatchFailure::Uncertain(format!("genome/job-create response parse: {error}"))
    })?;
    if response.success {
        if let Some(took) = response.took {
            return Ok(Created::Held(took));
        }
        let result = response.result.ok_or_else(|| {
            DispatchFailure::Uncertain("genome/job-create returned success without result".into())
        })?;
        if result.handle.provider_id.is_empty()
            || result.handle.provider_job_id.is_empty()
            || result.selected_provider.is_empty()
            || result.selected_provider != result.handle.provider_id
        {
            return Err(DispatchFailure::Uncertain(
                "genome/job-create has inconsistent handle/provider".into(),
            ));
        }
        return Ok(Created::Job(result.handle, result.selected_provider));
    }
    if response.result.is_some() || response.error.as_ref().is_none_or(|error| error.is_empty()) {
        return Err(DispatchFailure::Uncertain(
            "genome/job-create has incomplete refusal evidence".into(),
        ));
    }
    let error = format!(
        "genome/job-create rejected: {}",
        response.error.unwrap_or_default()
    );
    use crate::commands::genome::job_create::{ERROR_KIND_REUSE_ADOPT_FAILED, ERROR_KIND_REUSE_PULL_FAILED, ERROR_KIND_TRIAL_FILE_UNREADABLE};
    Err(match response.error_kind.as_deref() {
        // No kind is the command's pre-provider validation/selection refusal.
        None | Some("InvalidRequest" | "MissingCredentials" | "ProviderRejected") => {
            DispatchFailure::Retryable(error)
        }
        // A decision that created nothing is certain, never recovery: a hub pull that
        // failed, a gene not adopted, a trial file not read. The next fill retries
        // (Cormac on #4794: Uncertain here wedged the bucket in RecoveryRequired).
        Some(kind) if kind == ERROR_KIND_REUSE_PULL_FAILED || kind == ERROR_KIND_REUSE_ADOPT_FAILED || kind == ERROR_KIND_TRIAL_FILE_UNREADABLE => {
            DispatchFailure::Retryable(error)
        }
        Some(_) => DispatchFailure::Uncertain(error),
    })
}

pub struct TrainingTriggerModule {
    pub(crate) state: Arc<TrainingTriggerState>,
}

impl TrainingTriggerModule {
    pub fn new() -> Self {
        Self {
            state: Arc::new(TrainingTriggerState::new()),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_test_board(
        board: Arc<crate::genome::fine_tuning::TrainingJobBoard>,
    ) -> Self {
        let mut state = TrainingTriggerState::new();
        state.test_job_board = board;
        Self {
            state: Arc::new(state),
        }
    }
}

impl Default for TrainingTriggerModule {
    fn default() -> Self {
        Self::new()
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
        let module = ctx
            .registry
            .get_by_name("data")
            .ok_or_else(|| "training-trigger requires the data module".to_string())?;
        let data = module
            .as_any()
            .downcast_ref::<crate::modules::data::DataModule>()
            .ok_or_else(|| "training-trigger data module type mismatch".to_string())?;
        self.state.data.install(data.state.clone());
        self.state.ensure_storage().await
    }

    async fn tick(&self) -> Result<(), String> {
        self.state.resume_orphans_once().await;
        self.state.recover_tick().await
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
            | "genome/training-trigger/flush"
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

    async fn shutdown(&self) -> Result<(), String> {
        self.state.drain_operations().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (Cormac on #4537): an orphan whose engine was silent at boot asked
    // once and dropped, holding serving off its run for the engine's life; or, worse, resumed
    // (a second POST into an engine still training it). Uncertain on the first tick holds it;
    // it is not due again before its wait; on the next ask an Attached answer registers the
    // SAME job. Neither tick is a Resume, the only step that reaches genome/job-create.
    #[test]
    fn an_uncertain_orphan_is_held_then_reattached_and_never_resumed() {
        use crate::genome::fine_tuning::{job_board::OrphanedJob, JobHandle, ReattachOutcome};
        let state = TrainingTriggerState::new();
        let job = Uuid::new_v4();
        let orphan = OrphanedJob {
            local_id: job,
            trigger_dispatch_id: None,
            provider_id: "engine-local".into(),
            persona_id: Uuid::new_v4(),
            persona_name: "Kimi".into(),
            base_model: "qwen3.8-27b".into(),
            trait_kind: "code".into(),
            eval_set: None,
        };
        let t0 = 1_000_000;
        let first = orphan_step(&orphan, Ok(ReattachOutcome::Uncertain { reason: "no answer".into() }));
        assert!(matches!(first, OrphanStep::Hold), "tick 1: uncertain holds");
        state.hold_orphan(orphan, t0);
        assert!(state.take_due_orphans(t0 + 1).is_empty(), "not asked again before its wait");
        let mut due = state.take_due_orphans(t0 + HELD_ORPHAN_REASK_MS);
        assert_eq!(due.len(), 1, "asked again once its wait is up");
        let again = due.remove(0);
        let handle = JobHandle { provider_id: "engine-local".into(), provider_job_id: job.to_string(), local_id: job };
        let OrphanStep::Register(watched) = orphan_step(&again, Ok(ReattachOutcome::Attached { handle })) else {
            panic!("tick 2: an attached run is registered, never resumed");
        };
        assert_eq!(watched.handle.local_id, job, "the same job, watched again");
        assert!(state.take_due_orphans(u64::MAX).is_empty(), "a registered job is no longer held");
        assert!(matches!(orphan_step(&again, Err("executor down".into())), OrphanStep::Hold), "no answer holds too");
        assert!(matches!(orphan_step(&again, Ok(ReattachOutcome::NotResident)), OrphanStep::Resume), "positive control");
    }

    // What this catches (278afa6c): malformed command output cannot turn an
    // uncertain provider creation into a safe retry. Exercise the actual decoder.
    #[test]
    fn job_create_requires_explicit_typed_refusal_evidence() {
        use serde_json::json;
        for response in [
            json!({}),
            json!({"success": "false"}),
            json!({"success": false}),
            json!({"success": false, "error": "bad", "errorKind": 7}),
            json!({"success": false, "error": "bad", "errorKind": null}),
            json!({"success": false, "error": "bad", "errorKind": "Transient"}),
            json!({"success": true}),
        ] {
            assert!(matches!(
                decode_job_create(response),
                Err(DispatchFailure::Uncertain(_))
            ));
        }
        // what this catches (Cormac on #4794): a refusal that created no job is retryable,
        // never Uncertain; Uncertain froze the bucket in RecoveryRequired with no journal
        // row to recover from.
        for kind in [
            None,
            Some("InvalidRequest"),
            Some("MissingCredentials"),
            Some("ProviderRejected"),
            Some(crate::commands::genome::job_create::ERROR_KIND_REUSE_PULL_FAILED),
            Some(crate::commands::genome::job_create::ERROR_KIND_REUSE_ADOPT_FAILED),
            Some(crate::commands::genome::job_create::ERROR_KIND_TRIAL_FILE_UNREADABLE),
        ] {
            let mut response = json!({"success": false, "error": "explicit refusal"});
            if let Some(kind) = kind {
                response["errorKind"] = json!(kind);
            }
            assert!(matches!(
                decode_job_create(response),
                Err(DispatchFailure::Retryable(_))
            ));
        }
    }

    // what this catches: every migrated verb now fails loud through the legacy
    // path, naming itself + pointing at the typed registry (no silent success that
    // would mask a routing regression). The behavioral tests live in the command
    // files (commands/training_trigger/*).
    #[tokio::test]
    async fn migrated_arms_fail_loud() {
        let module = TrainingTriggerModule::new();
        for command in [
            "genome/training-trigger/submit",
            "genome/training-trigger/flush",
            "genome/training-trigger/status",
        ] {
            let err = module
                .handle_command(command, Value::Null)
                .await
                .expect_err("migrated arm must fail loud");
            assert!(err.contains("migrated"), "for {command}: {err}");
            assert!(err.contains(command), "for {command}: {err}");
        }
    }

    // what this catches: the module contributes the three dep-holding verbs to the
    // typed object map (sharing its one state). A regression that drops the
    // `commands()` override — leaving them unroutable — is caught.
    #[test]
    fn contributes_the_typed_training_trigger_commands() {
        let module = TrainingTriggerModule::new();
        let names: Vec<&str> = module.commands().iter().map(|c| c.name()).collect();
        assert!(names.contains(&"genome/training-trigger/submit"));
        assert!(names.contains(&"genome/training-trigger/flush"));
        assert!(names.contains(&"genome/training-trigger/status"));
    }

    // what this catches: an unmigrated/unknown verb still errors (not a panic, not
    // a silent ok).
    #[tokio::test]
    async fn unknown_command_errors() {
        let module = TrainingTriggerModule::new();
        let result = module
            .handle_command("genome/training-trigger/nope", Value::Null)
            .await;
        assert!(result.is_err());
    }

    // regression for card 244757bc (BigMama, 2026-09-26): three jobs in the ledger ended
    // `killed-by-reboot` — the consumer deploys six times a day — and nothing resumed
    // them, while each job directory still held the full request.json.
    // what this catches: the resumable input is the job directory's own request.json,
    // re-created as job-create params under a fresh dispatch id on the same provider;
    // a directory with nothing in it resumes nothing; the directory rule matches
    // `job_dir_for` for the ledger's fields.
    #[test]
    fn a_dead_jobs_request_json_is_its_resume_and_an_empty_directory_is_none() {
        let dir = tempfile::tempdir().expect("test: tempdir");
        let dispatch = Uuid::new_v4();
        assert!(resumable_request(dir.path(), "cuda-local", dispatch).is_err(), "nothing on disk");
        // What the CUDA adapter writes: the flattened request plus its admission fields.
        let spec = serde_json::json!({
            "personaId": Uuid::from_u128(7).to_string(),
            "personaName": "Kimi",
            // What the adapter really writes: the TRAINABLE base here, the registry id
            // in canonicalBase. The fixture once wrote the same id in both, so it could
            // not see a resume that fed the HF id back to job-create.
            "baseModel": "Qwen/Qwen3.8-27B",
            "traitKind": "code",
            "dataset": {"examples": [{"prompt": "p", "completion": "c"}], "source": "teacher_synthesized", "validationSplit": 0.1},
            "canonicalBase": "ggml-org/Qwen3.8-27B-GGUF",
            "memoryBytes": 30_000_000_000u64,
            "availableBytes": 6_000_000_000u64,
            "microBatchSize": 1,
            "revision": null
        });
        std::fs::write(dir.path().join("request.json"), spec.to_string()).expect("test: write");
        let params = resumable_request(dir.path(), "cuda-local", dispatch).expect("test: resumable");
        assert_eq!(params["triggerDispatchId"], serde_json::json!(dispatch.to_string()));
        assert!(
            params.get("preferredProvider").is_none(),
            "a job that never checkpointed is re-chosen, not pinned to the trainer that never ran it"
        );
        assert_eq!(params["personaName"], serde_json::json!("Kimi"));
        assert_eq!(
            params["baseModel"],
            serde_json::json!("ggml-org/Qwen3.8-27B-GGUF"),
            "a resume re-creates the job against the registry id, never the HF base"
        );
        assert_eq!(params["dataset"]["examples"].as_array().map(|e| e.len()), Some(1));
        assert!(params.get("memoryBytes").is_none(), "admission fields are the adapter's, not the request's");
        assert!(params.get("resumeFrom").is_none(), "no checkpoint, no resume point");
        // A checkpoint the dead job finished writing becomes the resume point.
        let ck = dir.path().join("checkpoints").join("step-8");
        std::fs::create_dir_all(&ck).expect("test: checkpoint dir");
        std::fs::write(ck.join("adapter_config.json"), "{}").expect("test: adapter config");
        std::fs::write(dir.path().join("checkpoints").join("LATEST"), "step-8\n").expect("test: pointer");
        let params = resumable_request(dir.path(), "cuda-local", dispatch).expect("test: resumable");
        assert_eq!(params["resumeFrom"], serde_json::json!(ck.to_string_lossy()), "continues from the latest checkpoint");
        assert_eq!(params["preferredProvider"], serde_json::json!("cuda-local"), "a checkpoint is its trainer's format: pinned");
        let d = default_job_dir("Kimi", "code/owner", Uuid::from_u128(9)).expect("test: a home directory");
        assert!(d.ends_with(std::path::Path::new("Kimi").join("code_owner").join(Uuid::from_u128(9).to_string())), "{d:?}");
    }
}
