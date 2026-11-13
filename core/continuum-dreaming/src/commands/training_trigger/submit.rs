//! `genome/training-trigger/submit` — accept curated training examples into the
//! per-`(persona_id, trait_kind, base_model)` journal, where the engine serving that base
//! learns them in place (`modules::training_trigger::in_place`).
//!
//! Dep-holding: captures the module's shared
//! [`TrainingTriggerState`](crate::modules::training_trigger::TrainingTriggerState)
//! so it serializes on the same per-key gate as learning and retirement.
//!
//! ## Gating
//!
//! `Privileged` — an accepted batch spends learning compute on her engine, so it is NOT on
//! the AiSafe surface.

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
    BucketKey, PendingBatch, TrainingTriggerState,
    DEFAULT_VALIDATION_SPLIT,
};
use crate::sdk_codegen::CommandError;

/// `genome/training-trigger/submit` input — one batch of curated examples plus the
/// bucket discriminator + optional per-bucket policy (LoRA, schedule, provider
/// preference). All `Option` fields default; first-arrival pins the bucket's
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
    pub validation_split: Option<f32>,
}

crate::action_command! {
    /// Submit curated training examples for a persona's trait. They are accepted into the
    /// per-`(persona, trait, base_model)` journal, where the engine serving that base learns
    /// them in place on the trigger's next pass (`BatchAppended`, or `BatchHeld` when a gene of
    /// hers on trial for the competence holds it). A submit whose policy
    /// (source/lora/schedule/validationSplit/artifactDir/provider) disagrees with the bucket's
    /// first-arrival policy is rejected `InconsistentBucket` (never silently overridden).
    /// Supply submissionId for retry dedupe; an omitted ID deliberately creates a new
    /// submission each invocation.
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

/// One batch into the journal: THE acceptance path.
pub(crate) async fn submit_batch(state: &Arc<TrainingTriggerState>, p: SubmitParams) -> Result<SubmitOutcome, CommandError> {
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
    let validation_split = p.validation_split.unwrap_or(DEFAULT_VALIDATION_SPLIT);

    let key = BucketKey {
        persona_id: p.persona_id,
        trait_kind: p.trait_kind.clone(),
        base_model: p.base_model.clone(),
    };

    let batch = PendingBatch {
        returned: false,
        persona_name: p.persona_name,
        source: p.source,
        examples: p.examples,
        lora: p.lora,
        schedule: p.schedule,
        local_artifact_dir: p.local_artifact_dir,
        preferred_provider: p.preferred_provider,
        validation_split,
        eval_set: p.eval_set,
    };
    let submission_id = p.submission_id;
    state.run_owned(key, move |state, key| async move {
        let acceptance = match state.accept(&key, submission_id, batch).await {
            Ok(receipt) => receipt,
            Err((kind, error)) => return SubmitOutcome::refused(kind, error),
        };
        // Accepted into the journal, where in-place learning reads it on the trigger's next pass (or
        // her next completed turn): nothing is dispatched from here.
        let count = match state.unlearned_in(&key).await {
            Ok(count) => count,
            Err(error) => return SubmitOutcome::refused("PersistenceFailed", error),
        };
        let mut outcome = if acceptance.replayed && count == 0 {
            SubmitOutcome { outcome: Some("AlreadyAccepted".into()), ..SubmitOutcome::base(true) }
        } else if let Some(took) = state.held_for(&key) {
            SubmitOutcome::batch_held(count, took)
        } else {
            SubmitOutcome::batch_appended(count)
        };
        outcome.acceptance = Some(acceptance);
        outcome
    }).await.map_err(CommandError::Internal)
}

