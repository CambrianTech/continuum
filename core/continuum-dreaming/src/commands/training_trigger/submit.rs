//! `genome/training-trigger/submit` — append curated training examples to a
//! per-`(persona_id, trait_kind, base_model)` bucket; dispatch a `genome/job-create`
//! once the bucket crosses its threshold.
//!
//! Dep-holding: captures the module's shared
//! [`TrainingTriggerState`](crate::modules::training_trigger::TrainingTriggerState)
//! so it serializes on the same per-key gate + mutates the same buckets as
//! [`flush`](super::flush).
//!
//! ## Gating
//!
//! `Privileged` — submitting a batch can spend training compute + touch provider
//! credentials (the threshold-crossing dispatch), so it is NOT on the AiSafe surface.

use std::path::PathBuf;
use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::genome::training_outcome::SubmitOutcome;

use crate::genome::fine_tuning::types::{
    LoRAHyperparams, ScheduleParams, TrainingExample, TrainingSource,
};
use crate::modules::training_trigger::{
    BucketKey, DispatchResult, PendingBatch, TrainingTriggerState,
    DEFAULT_MIN_EXAMPLES, DEFAULT_VALIDATION_SPLIT,
};
use crate::sdk_codegen::CommandError;

/// `genome/training-trigger/submit` input — one batch of curated examples plus the
/// bucket discriminator + optional per-bucket policy (threshold, LoRA, schedule,
/// provider preference). All `Option` fields default; first-arrival pins the bucket's
/// policy and later submits to the same bucket must agree (else `InconsistentBucket`).
#[derive(Debug, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/training_trigger/SubmitParams.ts"
)]
pub struct SubmitParams {
    /// Stable immutable batch identity, reused on retries at this destination.
    /// Omitted: every invocation gets a fresh ID; blind retries are new batches.
    #[serde(default)]
    #[ts(optional, type = "string")]
    pub submission_id: Option<Uuid>,
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub persona_name: String,
    pub base_model: String,
    pub trait_kind: String,
    pub examples: Vec<TrainingExample>,
    pub source: TrainingSource,
    /// The trait's `cognition/eval` `eval_set` JSONL path, a manual spot-check gym.
    /// First-arrival pins it for the bucket; a later submit with a divergent gym is
    /// rejected `InconsistentBucket`. It rides onto the dispatched `TrainingJobRequest`;
    /// adoption does not read it (the gene's verdict is its in-room trial).
    #[serde(default)]
    #[ts(optional)]
    pub eval_set: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub lora: Option<LoRAHyperparams>,
    #[serde(default)]
    #[ts(optional)]
    pub schedule: Option<ScheduleParams>,
    #[serde(default)]
    #[ts(optional)]
    pub local_artifact_dir: Option<PathBuf>,
    #[serde(default)]
    #[ts(optional)]
    pub preferred_provider: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub min_examples: Option<u32>,
    #[serde(default)]
    #[ts(optional)]
    pub validation_split: Option<f32>,
}

/// The submission identity of a returned job's examples: derived from the job, so a second
/// return is a replay, and never the job's own id. A job the trigger dispatched is registered
/// under the id of the submission that filled it, so a return keyed by the job's id WAS that
/// submission: the store answered AlreadyAccepted (nothing appended) or SubmissionConflict
/// (the returned batch carries the bucket's adopted policy). On the 5090, 2026-10-07, 34 of
/// Kimi's examples stayed stranded that way, and the same batches had re-failed for two weeks.
pub(crate) fn returned_id(job: Uuid) -> Uuid {
    Uuid::new_v5(&job, b"genome/training-trigger/return")
}

/// The identity of the `index`-th chunk of a returned job's examples: chunk 0 is
/// [`returned_id`] (a single-chunk return is unchanged), every later chunk derives from the job
/// and its index, so re-running the same return replays each chunk instead of copying it.
/// Chunk 0 sharing the whole-return id is deliberate (Fable on #4936): a job an older core
/// returned WHOLE, returned again chunked, has chunk 0 refused as a changed replay, loudly,
/// where fresh ids for every chunk would silently put its examples in her bucket twice.
pub(crate) fn returned_chunk_id(job: Uuid, index: usize) -> Uuid {
    if index == 0 {
        returned_id(job)
    } else {
        Uuid::new_v5(&job, format!("genome/training-trigger/return/{index}").as_bytes())
    }
}

/// WHAT one return hands back, and so the identity of each chunk of it.
#[derive(Debug, Clone, Copy)]
pub(crate) enum ReturnScope<'a> {
    /// A job's whole input: an ended job that never trained ([`returned_chunk_id`]).
    Whole,
    /// Only the examples a job that TRAINED parked, by their request indices, ascending. Its
    /// own namespace, keyed by the exact index set: it never collides with a whole return of
    /// the same job, and the same remainder returned again is a replay (card 8a2334d9).
    Parked(&'a [usize]),
}

impl ReturnScope<'_> {
    /// The submission identity of the `index`-th chunk of this return of `job`.
    pub(crate) fn chunk_id(self, job: Uuid, index: usize) -> Uuid {
        match self {
            Self::Whole => returned_chunk_id(job, index),
            Self::Parked(indices) => {
                let set = indices.iter().map(usize::to_string).collect::<Vec<_>>().join(",");
                Uuid::new_v5(&job, format!("genome/training-trigger/return-parked/{set}/{index}").as_bytes())
            }
        }
    }
}

impl SubmitParams {
    /// A job's examples handed back to her bucket: THE one conversion from a job's request
    /// to a submit, carrying the job's own policy (which [`return_request`] replaces with
    /// the bucket's when the key already holds one). The batch identity derives from the
    /// job: a second return of the same job is a replay the bucket recognises, never
    /// a copy, and the return never wears the identity of the submission it came from.
    /// `id` is [`returned_chunk_id`] of the job: a return larger than the bucket's threshold
    /// arrives as several chunks, each its own identity.
    pub(crate) fn returning(request: crate::genome::fine_tuning::types::TrainingJobRequest, id: Uuid) -> Self {
        Self {
            submission_id: Some(id),
            persona_id: request.persona_id,
            persona_name: request.persona_name,
            base_model: request.base_model,
            trait_kind: request.trait_kind,
            examples: request.dataset.examples,
            source: request.dataset.source,
            eval_set: request.eval_set,
            lora: request.lora,
            schedule: request.schedule,
            local_artifact_dir: request.local_artifact_dir,
            preferred_provider: None,
            min_examples: None,
            validation_split: Some(request.dataset.validation_split),
        }
    }

    /// Take `policy` as this batch's: a returned job's examples JOIN the bucket they go
    /// back to, whatever the job's request.json says (the trainer's adapter writes its own
    /// defaults into it, so a returned `lora: Some(default)` would never equal a
    /// producer's `None`).
    fn adopting(mut self, policy: crate::modules::training_trigger::BucketPolicy) -> Self {
        self.source = policy.source;
        self.lora = policy.lora;
        self.schedule = policy.schedule;
        self.validation_split = Some(policy.validation_split);
        self.local_artifact_dir = policy.local_artifact_dir;
        self.preferred_provider = policy.preferred_provider;
        self.eval_set = policy.eval_set;
        self
    }
}

/// A job's examples back into her bucket: THE one return (a held orphan at boot,
/// `genome/training-trigger/return`, a trained job's parked remainder). Into a key that already
/// holds a batch they adopt its policy; into an empty key they pin the job's own. `scope` says
/// what `request` carries, and so names each chunk.
pub(crate) async fn return_request(
    state: &Arc<TrainingTriggerState>,
    request: crate::genome::fine_tuning::types::TrainingJobRequest,
    job: Uuid,
    scope: ReturnScope<'_>,
) -> Result<SubmitOutcome, CommandError> {
    // RETURNED IN CHUNKS of the bucket's own threshold, each its own submission: a run's
    // time budget splits a bucket only at submission boundaries, so a job's examples returned
    // as ONE submission dispatched whole (the 5090, 2026-10-11 02:15Z: 311 lived conversations
    // in one submission, a ~100 h run). A refused chunk stops the return; re-running it replays
    // the accepted chunks by their derived identities and submits the rest.
    let size = (crate::modules::training_trigger::DEFAULT_MIN_EXAMPLES as usize).max(1);
    if request.dataset.examples.len() <= size {
        return return_chunk(state, request, job, scope.chunk_id(job, 0)).await;
    }
    let mut reported: Option<SubmitOutcome> = None;
    let chunks = return_chunks(request.dataset.examples.clone(), size);
    let total = chunks.len();
    for (index, part) in chunks.into_iter().enumerate() {
        let mut chunk = request.clone();
        chunk.dataset.examples = part;
        let mut outcome = return_chunk(state, chunk, job, scope.chunk_id(job, index)).await?;
        if !outcome.success {
            // say how far the return got: the first `index` chunks are in her bucket, and
            // re-running the return replays them and resumes at this one
            outcome.error = Some(format!(
                "{index} of {total} chunks returned before chunk {index} was refused: {}",
                outcome.error.as_deref().unwrap_or("no reason given") // unwrap_or: a refusal without a message still reports the count
            ));
            return Ok(outcome);
        }
        // report the chunk that dispatched, if one did; else the last
        let dispatched = |o: &SubmitOutcome| o.job_handle.is_some();
        if !reported.as_ref().is_some_and(dispatched) {
            reported = Some(outcome);
        }
    }
    Ok(reported.unwrap_or_else(|| SubmitOutcome::refused("Empty", "the job carried no examples".into())))
}

/// A return's examples in arrival order, cut into chunks of `size` (the last may be shorter).
fn return_chunks<T>(mut examples: Vec<T>, size: usize) -> Vec<Vec<T>> {
    let mut chunks = Vec::new();
    while !examples.is_empty() {
        let rest = examples.split_off(examples.len().min(size));
        chunks.push(std::mem::replace(&mut examples, rest));
    }
    chunks
}

async fn return_chunk(
    state: &Arc<TrainingTriggerState>,
    request: crate::genome::fine_tuning::types::TrainingJobRequest,
    job: Uuid,
    id: Uuid,
) -> Result<SubmitOutcome, CommandError> {
    let params = SubmitParams::returning(request, id);
    let key = BucketKey {
        persona_id: params.persona_id,
        trait_kind: params.trait_kind.clone(),
        base_model: params.base_model.clone(),
    };
    let params = match state.held_policy(&key) {
        Some(policy) => params.adopting(policy),
        None => params,
    };
    submit_batch_inner(state, params, Some(job)).await
}

crate::action_command! {
    /// Submit curated training examples for a persona's trait. They accumulate in a
    /// per-`(persona, trait, base_model)` bucket; when the bucket reaches its
    /// threshold (`minExamples`, default 16) a `genome/job-create` fires and the
    /// bucket clears. Below threshold returns `BatchAppended`; at threshold returns
    /// `JobDispatched` with the handle + selected provider. A submit whose policy
    /// (source/lora/schedule/validationSplit/artifactDir/provider) disagrees with the
    /// bucket's first-arrival policy is rejected `InconsistentBucket` (never silently
    /// overridden). An explicit refusal returns `DispatchFailed` and can retry;
    /// uncertain dispatch returns `RecoveryRequired` until exact handle evidence
    /// resolves it. Both preserve accepted ownership. Supply submissionId for retry
    /// dedupe; an omitted ID deliberately creates a new submission each invocation.
    pub struct TrainingTriggerSubmit {
        state: Arc<TrainingTriggerState>,
    }
    name: "genome/training-trigger/submit",
    access: Privileged,
    params: SubmitParams,
    output: SubmitOutcome,
    run(this, _ctx, p) => {
        submit_batch(&this.state, p).await
    }
}

/// One batch into a bucket: THE acceptance path, shared by `submit` and `return` so a
/// returned job's examples are accepted, deduped and dispatched exactly as any batch is.
pub(crate) async fn submit_batch(state: &Arc<TrainingTriggerState>, p: SubmitParams) -> Result<SubmitOutcome, CommandError> {    
    submit_batch_inner(state, p, None).await
}

async fn submit_batch_inner(state: &Arc<TrainingTriggerState>, p: SubmitParams, returned_job: Option<Uuid>) -> Result<SubmitOutcome, CommandError> {
    if let Err(error) = state.require_ready() {
        return Ok(SubmitOutcome::refused("PersistenceUnavailable", error));
    }

    // Validation that fails synchronously — caller mistake, not worth a typed
    // outcome (these are programmer-facing → transport Err).
    if p.persona_name.trim().is_empty() {
        return Err(CommandError::Invalid("persona_name must be non-empty".into()));
    }
    if p.base_model.trim().is_empty() {
        return Err(CommandError::Invalid("base_model must be non-empty".into()));
    }
    if p.trait_kind.trim().is_empty() {
        return Err(CommandError::Invalid("trait_kind must be non-empty".into()));
    }
    if p.examples.is_empty() {
        return Err(CommandError::Invalid("examples must be non-empty".into()));
    }
    let min_examples = p.min_examples.unwrap_or(DEFAULT_MIN_EXAMPLES).max(1);
    let validation_split = p.validation_split.unwrap_or(DEFAULT_VALIDATION_SPLIT);

    let key = BucketKey {
        persona_id: p.persona_id,
        trait_kind: p.trait_kind.clone(),
        base_model: p.base_model.clone(),
    };

    let batch = PendingBatch {
        submission_ids: Vec::new(),
        submission_examples: Vec::new(),
        submission_returned: Vec::new(),
        returned: returned_job.is_some(),
        persona_name: p.persona_name,
        source: p.source,
        examples: p.examples,
        lora: p.lora,
        schedule: p.schedule,
        local_artifact_dir: p.local_artifact_dir,
        preferred_provider: p.preferred_provider,
        min_examples,
        validation_split,
        eval_set: p.eval_set,
    };
    let submission_id = p.submission_id;
    state.run_owned(key, move |state, key| async move {
        if let Some(job) = returned_job {
            if let Err((kind, error)) = state.verify_return_ownership(&key, job, returned_id(job)).await {
                return SubmitOutcome::refused(kind, error);
            }
        }
        // A held bucket is bounded: past MAX_HELD_EXAMPLES a NEW submit is refused and
        // the producer keeps its evidence for a later pass (never silently dropped). A
        // replay of a submission the bucket already holds is recognised first: it is
        // AlreadyAccepted, never refused as full (BigMama on #4794).
        let replay = submission_id.is_some_and(|id| state.contains_submission(&key, id));
        if let Some(held_by) = state.held_for(&key).filter(|_| !replay) {
            let pending = state.buckets.get(&key).map(|b| b.examples.len()).unwrap_or(0); // unwrap_or: no bucket yet = nothing pending
            if pending >= crate::modules::training_trigger::MAX_HELD_EXAMPLES {
                continuum_base::probe!(
                    class = "training.trigger.held_full",
                    persona = %key.persona_id,
                    trait_kind = %key.trait_kind,
                    pending = pending as u64,
                    held_by = ?held_by,
                    "a held bucket is at its bound: this submit is refused, its evidence stays staged with the producer"
                );
                return SubmitOutcome::refused(
                    "BucketHeldFull",
                    format!("the bucket for this competence holds {pending} examples waiting on {held_by:?}; at its bound, this submit is refused and retried after the hold lifts"),
                );
            }
        }
        let acceptance = match state.accept(&key, submission_id, batch).await {
            Ok(receipt) => receipt,
            Err((kind, error)) => return SubmitOutcome::refused(kind, error),
        };
        // A replay never appends again. It can still drive an already accepted,
        // retryable batch forward using the same persisted dispatch intent.
        let mut outcome = if state.ready_to_dispatch(&key) {
            match state.dispatch_pending(&key).await {
                DispatchResult::Dispatched { examples, handle, provider } =>
                    SubmitOutcome::job_dispatched(examples as u32, provider, handle),
                DispatchResult::Failed { kind, error } => SubmitOutcome::refused(kind, error),
                DispatchResult::Empty => SubmitOutcome::batch_appended(0, min_examples),
                DispatchResult::Held { examples, took } => SubmitOutcome::batch_held(examples as u32, min_examples, took),
            }
        } else {
            let (count, threshold) = state.buckets.get(&key)
                .map(|b| (b.examples.len() as u32, b.min_examples))
                .unwrap_or((0, min_examples)); // Acceptance holds the bucket gate; an absent bucket is an already-dispatched replay with no pending examples.
            if acceptance.replayed && count == 0 {
                SubmitOutcome { outcome: Some("AlreadyAccepted".into()), ..SubmitOutcome::base(true) }
            } else if let Some(took) = state.held_for(&key) {
                // Appended, and the bucket is held: the outcome names the job of hers
                // already training this competence, or the trial judging a gene for
                // it, that these examples wait for.
                SubmitOutcome::batch_held(count, threshold, took)
            } else {
                SubmitOutcome::batch_appended(count, threshold)
            }
        };
        outcome.acceptance = Some(acceptance);
        outcome
    }).await.map_err(CommandError::Internal)
}

#[cfg(test)]
mod tests;
