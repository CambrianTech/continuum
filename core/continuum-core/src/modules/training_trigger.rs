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
    /// The gym that MEASURES this bucket's trait — the `cognition/eval` `eval_set`
    /// JSONL path, carried verbatim onto the dispatched
    /// [`TrainingJobRequest::eval_set`] so it rides the board to the L3 sentinel. A
    /// first-arrival-wins bucket policy field like `lora`/`schedule`: a later submit
    /// with a divergent gym is rejected `InconsistentBucket`, never silently merged.
    /// `None` means the recipe declared no gym — the sentinel then REFUSES to adopt
    /// rather than measuring against a default ([[fallbacks-are-illegal-fail-loud]]).
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
}

impl TrainingTriggerState {
    pub(crate) fn new() -> Self {
        Self {
            buckets: Arc::new(DashMap::new()),
            submit_gates: PerKeyGate::new(),
            executor: LateBound::new("training-trigger::executor"),
            resumed_orphans: std::sync::atomic::AtomicBool::new(false),
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
    pub(crate) async fn resume_orphans_once(&self) {
        use std::sync::atomic::Ordering;
        if self.resumed_orphans.load(Ordering::Acquire) {
            return;
        }
        let Ok(executor) = self.executor.require() else {
            return; // not installed yet: try again next tick, the orphans wait on the board
        };
        self.resumed_orphans.store(true, Ordering::Release);
        let board = crate::genome::fine_tuning::job_board::TrainingJobBoard::global();
        for orphan in board.take_orphans() {
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
            let dir = default_job_dir(&orphan.persona_name, &orphan.trait_kind, orphan.local_id);
            let dispatch_id = Uuid::new_v4();
            let Some(params) = resumable_request(&dir, &orphan.provider_id, dispatch_id) else {
                crate::probe!(
                    class = "training.job.not_resumable",
                    local_id = %orphan.local_id,
                    dir = %dir.display(),
                    "the job directory holds no request.json — nothing to resume from"
                );
                continue;
            };
            match executor.execute_json("genome/job-create", params).await.map_err(|e| e.to_string()).and_then(|r| decode_job_create(r).map_err(|e| format!("{e:?}"))) {
                Ok((handle, provider)) => {
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

    pub(crate) async fn dispatch_job_create(
        &self,
        persona_id: Uuid,
        trait_kind: &str,
        base_model: &str,
        batch: &PendingBatch,
        dispatch_id: Uuid,
    ) -> Result<(JobHandle, String), DispatchFailure> {
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
pub(crate) fn default_job_dir(persona_name: &str, trait_kind: &str, local_id: Uuid) -> std::path::PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from(".")); // unwrap_or_else: mirrors job_dir_for — no home resolves the same relative root
    home.join(".continuum/genome")
        .join(persona_name.replace(['/', ' '], "_"))
        .join(trait_kind.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect::<String>())
        .join(local_id.to_string())
}

/// PURE over the job directory: the `genome/job-create` params that re-create a job from
/// its own `request.json` (the flattened `TrainingJobRequest` the trainer was handed,
/// with the adapter's admission fields alongside, which the request type ignores).
/// `None` when there is nothing on disk to resume from.
pub(crate) fn resumable_request(dir: &std::path::Path, provider: &str, dispatch_id: Uuid) -> Option<Value> {
    let text = std::fs::read_to_string(dir.join("request.json")).ok()?;
    let mut request: TrainingJobRequest = serde_json::from_str(&text).ok()?;
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
    let mut params = serde_json::to_value(&request).ok()?;
    params["triggerDispatchId"] = Value::String(dispatch_id.to_string());
    if !provider.is_empty() {
        params["preferredProvider"] = Value::String(provider.to_string());
    }
    Some(params)
}

fn decode_job_create(response: Value) -> Result<(JobHandle, String), DispatchFailure> {
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
        return Ok((result.handle, result.selected_provider));
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
    Err(match response.error_kind.as_deref() {
        // No kind is the command's pre-provider validation/selection refusal.
        None | Some("InvalidRequest" | "MissingCredentials" | "ProviderRejected") => {
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
        for kind in [
            None,
            Some("InvalidRequest"),
            Some("MissingCredentials"),
            Some("ProviderRejected"),
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
        assert_eq!(resumable_request(dir.path(), "cuda-local", dispatch), None, "nothing on disk");
        // What the CUDA adapter writes: the flattened request plus its admission fields.
        let spec = serde_json::json!({
            "personaId": Uuid::from_u128(7).to_string(),
            "personaName": "Kimi",
            "baseModel": "ggml-org/Qwen3.8-27B-GGUF",
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
        assert_eq!(params["preferredProvider"], serde_json::json!("cuda-local"));
        assert_eq!(params["personaName"], serde_json::json!("Kimi"));
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
        let d = default_job_dir("Kimi", "code/owner", Uuid::from_u128(9));
        assert!(d.ends_with(std::path::Path::new("Kimi").join("code_owner").join(Uuid::from_u128(9).to_string())), "{d:?}");
    }
}
