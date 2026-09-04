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

use crate::genome::fine_tuning::types::{
    JobHandle, LoRAHyperparams, ScheduleParams, TrainingExample, TrainingSource,
};
use crate::modules::training_trigger::{
    BucketKey, PendingBatch, TrainingTriggerState, DEFAULT_MIN_EXAMPLES, DEFAULT_VALIDATION_SPLIT,
};
use crate::sdk_codegen::CommandError;

/// `genome/training-trigger/submit` input — one batch of curated examples plus the
/// bucket discriminator + optional per-bucket policy (threshold, LoRA, schedule,
/// provider preference). All `Option` fields default; first-arrival pins the bucket's
/// policy and later submits to the same bucket must agree (else `InconsistentBucket`).
#[derive(Debug, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/training_trigger/SubmitParams.ts"
)]
pub struct SubmitParams {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub persona_name: String,
    pub base_model: String,
    pub trait_kind: String,
    pub examples: Vec<TrainingExample>,
    pub source: TrainingSource,
    /// The gym that MEASURES this trait — the `cognition/eval` `eval_set` JSONL path.
    /// First-arrival pins it for the bucket; a later submit with a divergent gym is
    /// rejected `InconsistentBucket`. Rides onto the dispatched `TrainingJobRequest`
    /// so the L3 sentinel measures the gene on its OWN declared gym; `None` means no
    /// gym → the sentinel refuses to adopt ([[fallbacks-are-illegal-fail-loud]]).
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

/// `genome/training-trigger/submit` typed outcome — mirrors the legacy JSON 1:1
/// (outcome-as-data per the genome family doctrine): batch appended, job dispatched,
/// inconsistent bucket, or dispatch failed all return `success` + a discriminator,
/// NOT a transport error. The discriminating fields are populated per-outcome and
/// omitted otherwise.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/training_trigger/SubmitOutcome.ts"
)]
pub struct SubmitOutcome {
    pub success: bool,
    /// Discriminator: `"BatchAppended"` | `"JobDispatched"` (absent on rejections).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub outcome: Option<String>,
    /// BatchAppended: examples now pending in the bucket.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub current_count: Option<u32>,
    /// BatchAppended: the bucket's fire threshold.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub threshold: Option<u32>,
    /// JobDispatched: examples carried into the dispatched job.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub examples_used: Option<u32>,
    /// JobDispatched: the provider the coordinator selected.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub selected_provider: Option<String>,
    /// JobDispatched: the created job's handle.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub job_handle: Option<JobHandle>,
    /// Rejections: the diagnostic message.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
    /// Rejections: `"InconsistentBucket"` | `"DispatchFailed"`.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error_kind: Option<String>,
}

impl SubmitOutcome {
    fn base(success: bool) -> Self {
        Self {
            success,
            outcome: None,
            current_count: None,
            threshold: None,
            examples_used: None,
            selected_provider: None,
            job_handle: None,
            error: None,
            error_kind: None,
        }
    }

    fn batch_appended(current_count: u32, threshold: u32) -> Self {
        Self {
            outcome: Some("BatchAppended".into()),
            current_count: Some(current_count),
            threshold: Some(threshold),
            ..Self::base(true)
        }
    }

    fn job_dispatched(
        examples_used: u32,
        selected_provider: String,
        job_handle: JobHandle,
    ) -> Self {
        Self {
            outcome: Some("JobDispatched".into()),
            examples_used: Some(examples_used),
            selected_provider: Some(selected_provider),
            job_handle: Some(job_handle),
            ..Self::base(true)
        }
    }

    fn inconsistent(error: String) -> Self {
        Self {
            error: Some(error),
            error_kind: Some("InconsistentBucket".into()),
            ..Self::base(false)
        }
    }

    fn dispatch_failed(error: String) -> Self {
        Self {
            error: Some(error),
            error_kind: Some("DispatchFailed".into()),
            ..Self::base(false)
        }
    }
}

crate::action_command! {
    /// Submit curated training examples for a persona's trait. They accumulate in a
    /// per-`(persona, trait, base_model)` bucket; when the bucket reaches its
    /// threshold (`minExamples`, default 16) a `genome/job-create` fires and the
    /// bucket clears. Below threshold returns `BatchAppended`; at threshold returns
    /// `JobDispatched` with the handle + selected provider. A submit whose policy
    /// (source/lora/schedule/validationSplit/artifactDir/provider) disagrees with the
    /// bucket's first-arrival policy is rejected `InconsistentBucket` (never silently
    /// overridden); a dispatch fault returns `DispatchFailed` with the bucket
    /// preserved so the next submit can re-trigger.
    pub struct TrainingTriggerSubmit {
        state: Arc<TrainingTriggerState>,
    }
    name: "genome/training-trigger/submit",
    access: Privileged,
    params: SubmitParams,
    output: SubmitOutcome,
    run(this, _ctx, p) => {
        let state = &this.state;

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

        // Serialize per-key. Concurrent submits to different keys proceed in
        // parallel; concurrent submits to the same key queue here. This eliminates
        // the lost-update + restore-commingle races. Holding the lease across the
        // .await is intentional — PerKeyGate uses tokio::sync::Mutex internally. The
        // lease is RAII: on Drop it releases the lock AND attempts structural eviction
        // of the gate.
        let _submit_lease = state.submit_gates.acquire(&key).await;

        // Append phase. Hold the entry mutably for the minimum window: append + read
        // the new len + decide whether to fire. If we fire, take the examples out
        // (clear the bucket atomically) and drop the entry guard BEFORE awaiting the
        // dispatch.
        let snapshot_to_dispatch = {
            let mut entry = state.buckets.entry(key.clone()).or_insert_with(|| PendingBatch {
                persona_name: p.persona_name.clone(),
                source: p.source,
                examples: Vec::new(),
                lora: p.lora.clone(),
                schedule: p.schedule.clone(),
                local_artifact_dir: p.local_artifact_dir.clone(),
                preferred_provider: p.preferred_provider.clone(),
                min_examples,
                validation_split,
                eval_set: p.eval_set.clone(),
            });

            // Coherence checks for hyperparam fields NOT in the BucketKey. `base_model`
            // is in the key (independent buckets); the remaining policy fields are
            // first-arrival-wins via or_insert_with, so a divergent later submit must
            // be rejected rather than silently overriding the bucket's pinned policy.
            if entry.source != p.source {
                return Ok(SubmitOutcome::inconsistent(format!(
                    "bucket has source={:?}; submit gave source={:?}",
                    entry.source, p.source
                )));
            }
            if entry.lora != p.lora {
                return Ok(SubmitOutcome::inconsistent(format!(
                    "bucket has lora={:?}; submit gave lora={:?}",
                    entry.lora, p.lora
                )));
            }
            if entry.schedule != p.schedule {
                return Ok(SubmitOutcome::inconsistent(format!(
                    "bucket has schedule={:?}; submit gave schedule={:?}",
                    entry.schedule, p.schedule
                )));
            }
            if (entry.validation_split - validation_split).abs() > f32::EPSILON {
                return Ok(SubmitOutcome::inconsistent(format!(
                    "bucket has validation_split={}; submit gave validation_split={}",
                    entry.validation_split, validation_split
                )));
            }
            if entry.local_artifact_dir != p.local_artifact_dir {
                return Ok(SubmitOutcome::inconsistent(format!(
                    "bucket has local_artifact_dir={:?}; submit gave local_artifact_dir={:?}",
                    entry.local_artifact_dir, p.local_artifact_dir
                )));
            }
            if entry.preferred_provider != p.preferred_provider {
                return Ok(SubmitOutcome::inconsistent(format!(
                    "bucket has preferred_provider={:?}; submit gave preferred_provider={:?}",
                    entry.preferred_provider, p.preferred_provider
                )));
            }
            if entry.eval_set != p.eval_set {
                return Ok(SubmitOutcome::inconsistent(format!(
                    "bucket has eval_set={:?}; submit gave eval_set={:?}",
                    entry.eval_set, p.eval_set
                )));
            }

            entry.examples.extend(p.examples.into_iter());

            // Allow per-submit threshold downgrade — operator tooling pumping in
            // known-final batches can force-fire a smaller bucket by passing
            // min_examples = 1.
            if min_examples < entry.min_examples {
                entry.min_examples = min_examples;
            }

            let current_count = entry.examples.len() as u32;
            if current_count < entry.min_examples {
                return Ok(SubmitOutcome::batch_appended(current_count, entry.min_examples));
            }

            // Threshold reached — drain the bucket into a snapshot we can dispatch
            // outside the entry guard. Insert remains in the map (empty) until we know
            // whether dispatch succeeded; on success we remove it.
            let drained_examples = std::mem::take(&mut entry.examples);
            PendingBatch {
                persona_name: entry.persona_name.clone(),
                source: entry.source,
                examples: drained_examples,
                lora: entry.lora.clone(),
                schedule: entry.schedule.clone(),
                local_artifact_dir: entry.local_artifact_dir.clone(),
                preferred_provider: entry.preferred_provider.clone(),
                min_examples: entry.min_examples,
                validation_split: entry.validation_split,
                eval_set: entry.eval_set.clone(),
            }
        };

        // Dispatch under the per-key gate — no other submit to this key can race the
        // success-clear or failure-restore paths.
        let dispatch_result = state
            .dispatch_job_create(p.persona_id, &key.trait_kind, &key.base_model, &snapshot_to_dispatch)
            .await;

        match dispatch_result {
            Ok((job_handle, selected_provider)) => {
                // Remove the now-empty bucket so status() doesn't report a phantom
                // zero-count entry.
                state.buckets.remove(&key);
                Ok(SubmitOutcome::job_dispatched(
                    snapshot_to_dispatch.examples.len() as u32,
                    selected_provider,
                    job_handle,
                ))
            }
            Err(err) => {
                // Restore drained examples. Reinsert at the FRONT so submit order is
                // preserved across retries.
                if let Some(mut entry) = state.buckets.get_mut(&key) {
                    let mut restored = snapshot_to_dispatch.examples.clone();
                    restored.extend(std::mem::take(&mut entry.examples).into_iter());
                    entry.examples = restored;
                }
                Ok(SubmitOutcome::dispatch_failed(err))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::training_trigger::test_support::{
        build_runtime_trigger_only, build_runtime_with_trigger_and_genome, ex, submit_params,
    };
    use crate::sdk_codegen::{AccessLevel, ActionCommand};
    use serde_json::Value;
    use uuid::Uuid;

    // what this catches: name/access wiring — submitting a batch can spend training
    // compute (threshold-crossing dispatch), so it lives on the Privileged surface,
    // not AiSafe.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(
            TrainingTriggerSubmit::NAME,
            "genome/training-trigger/submit"
        );
        assert!(matches!(
            TrainingTriggerSubmit::ACCESS,
            AccessLevel::Privileged
        ));
    }

    // what this catches: under the threshold, submit returns BatchAppended and the
    // examples accumulate in the bucket. A regression that fired job-create on every
    // submit would surface as JobDispatched on the very first call.
    #[tokio::test]
    async fn submit_below_threshold_appends_and_does_not_fire() {
        let (trigger, executor) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();

        let params = submit_params(persona, "test-trait", vec![ex("a", "b")], Some(5));
        let json = executor
            .execute_json("genome/training-trigger/submit", params)
            .await
            .expect("submit ok");

        assert_eq!(json["success"], true);
        assert_eq!(json["outcome"], "BatchAppended");
        assert_eq!(json["currentCount"], 1);
        assert_eq!(json["threshold"], 5);
        assert_eq!(
            trigger
                .state
                .bucket_example_count(persona, "test-trait", "synthetic"),
            Some(1)
        );
    }

    // what this catches: hitting threshold dispatches and clears the bucket. Without
    // this, examples would pile up forever and the substrate's training loop would
    // never close.
    #[tokio::test]
    async fn submit_at_threshold_dispatches_and_clears() {
        let (trigger, executor) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();

        // First submit: 4 examples, threshold 5 → BatchAppended.
        let _ = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(
                    persona,
                    "test-trait",
                    vec![ex("a", "b"), ex("c", "d"), ex("e", "f"), ex("g", "h")],
                    Some(5),
                ),
            )
            .await
            .unwrap();
        assert_eq!(
            trigger
                .state
                .bucket_example_count(persona, "test-trait", "synthetic"),
            Some(4)
        );

        // Second submit: 1 more example → 5 → fires.
        let json = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(persona, "test-trait", vec![ex("i", "j")], Some(5)),
            )
            .await
            .unwrap();
        assert_eq!(json["success"], true, "submit must succeed, got: {json}");
        assert_eq!(json["outcome"], "JobDispatched");
        assert_eq!(json["examplesUsed"], 5);
        assert_eq!(json["selectedProvider"], "local-candle");
        assert!(json["jobHandle"]["localId"].is_string());

        // Bucket must be cleared.
        assert_eq!(
            trigger
                .state
                .bucket_example_count(persona, "test-trait", "synthetic"),
            None
        );
        assert_eq!(trigger.state.pending_bucket_count(), 0);

        // The per-key gate map MUST also be empty after a successful dispatch — the
        // RAII Lease evicts structurally on drop. This pins the structural invariant
        // in a production-shaped test (full submit → dispatch → success path).
        assert_eq!(
            trigger.state.submit_gates.len(),
            0,
            "submit_gates MUST be empty after successful dispatch — \
             a non-zero count indicates the structural-eviction-on-drop \
             invariant is broken"
        );
    }

    // what this catches: same persona + same trait_kind submitted with DIFFERENT
    // base_model values gets DIFFERENT buckets — a persona legitimately trains the
    // same trait against multiple bases for routing flexibility. The pre-fix behavior
    // rejected the second submit with InconsistentBucket and silently dropped its
    // data; base_model is now in the bucket key so the submits accumulate
    // independently.
    #[tokio::test]
    async fn different_base_models_create_separate_buckets() {
        let (trigger, executor) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();

        // First submit: base_model = "synthetic".
        let _ = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(persona, "test-trait", vec![ex("a", "b")], Some(100)),
            )
            .await
            .unwrap();

        // Second submit: SAME persona, SAME trait, DIFFERENT base.
        let mut other_base = submit_params(persona, "test-trait", vec![ex("c", "d")], Some(100));
        other_base
            .as_object_mut()
            .unwrap()
            .insert("baseModel".into(), Value::String("synthetic-tiny".into()));
        let json = executor
            .execute_json("genome/training-trigger/submit", other_base)
            .await
            .unwrap();
        // No more InconsistentBucket — the second submit succeeds because it lives in
        // its own bucket.
        assert_eq!(
            json["success"], true,
            "second-base submit must succeed: {json}"
        );
        assert_eq!(json["outcome"], "BatchAppended");

        // Two distinct buckets pending, one example each.
        assert_eq!(
            trigger
                .state
                .bucket_example_count(persona, "test-trait", "synthetic"),
            Some(1)
        );
        assert_eq!(
            trigger
                .state
                .bucket_example_count(persona, "test-trait", "synthetic-tiny"),
            Some(1)
        );
        assert_eq!(trigger.state.pending_bucket_count(), 2);
    }

    // what this catches: coherence check covers `lora` hyperparameters. A second
    // submit with different lora params gets InconsistentBucket so the caller learns
    // their config was rejected instead of silently overridden.
    #[tokio::test]
    async fn inconsistent_lora_in_same_bucket_is_rejected() {
        use crate::genome::fine_tuning::types::LoRAHyperparams;
        let (trigger, executor) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();

        let mut first = submit_params(persona, "test-trait", vec![ex("a", "b")], Some(100));
        first.as_object_mut().unwrap().insert(
            "lora".into(),
            serde_json::to_value(LoRAHyperparams {
                rank: 8,
                alpha: 16,
                dropout: 0.0,
                target_modules: vec![],
            })
            .unwrap(),
        );
        let _ = executor
            .execute_json("genome/training-trigger/submit", first)
            .await
            .unwrap();

        let mut wrong_lora = submit_params(persona, "test-trait", vec![ex("c", "d")], Some(100));
        wrong_lora.as_object_mut().unwrap().insert(
            "lora".into(),
            serde_json::to_value(LoRAHyperparams {
                rank: 16, // different
                alpha: 32,
                dropout: 0.0,
                target_modules: vec![],
            })
            .unwrap(),
        );
        let json = executor
            .execute_json("genome/training-trigger/submit", wrong_lora)
            .await
            .unwrap();
        assert_eq!(json["success"], false, "got: {json}");
        assert_eq!(json["errorKind"], "InconsistentBucket");
        // First-arrival's bucket survives intact.
        assert_eq!(
            trigger
                .state
                .bucket_example_count(persona, "test-trait", "synthetic"),
            Some(1)
        );
    }

    // what this catches: same coherence guarantee for ScheduleParams. A submit with
    // different `epochs` or `learning_rate` is rejected with InconsistentBucket rather
    // than silently using the first-arrival's schedule.
    #[tokio::test]
    async fn inconsistent_schedule_in_same_bucket_is_rejected() {
        use crate::genome::fine_tuning::types::ScheduleParams;
        let (_trigger, executor) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();

        let mut first = submit_params(persona, "test-trait", vec![ex("a", "b")], Some(100));
        first.as_object_mut().unwrap().insert(
            "schedule".into(),
            serde_json::to_value(ScheduleParams {
                epochs: 3,
                batch_size: 4,
                sequence_length: 32,
                learning_rate: 1e-4,
            })
            .unwrap(),
        );
        let _ = executor
            .execute_json("genome/training-trigger/submit", first)
            .await
            .unwrap();

        let mut wrong_schedule =
            submit_params(persona, "test-trait", vec![ex("c", "d")], Some(100));
        wrong_schedule.as_object_mut().unwrap().insert(
            "schedule".into(),
            serde_json::to_value(ScheduleParams {
                epochs: 5, // different
                batch_size: 4,
                sequence_length: 32,
                learning_rate: 1e-4,
            })
            .unwrap(),
        );
        let json = executor
            .execute_json("genome/training-trigger/submit", wrong_schedule)
            .await
            .unwrap();
        assert_eq!(json["success"], false);
        assert_eq!(json["errorKind"], "InconsistentBucket");
    }

    // what this catches: source coherence is still enforced even after base_model
    // moved into the key. A submit with `source: "operator_curated"` to a bucket that
    // already exists from a `source: "teacher_synthesized"` submit must be rejected —
    // the alloy provenance contract distinguishes those origins.
    #[tokio::test]
    async fn inconsistent_source_in_same_bucket_is_rejected() {
        let (trigger, executor) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();

        let _ = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(persona, "test-trait", vec![ex("a", "b")], Some(100)),
            )
            .await
            .unwrap();

        let mut wrong_source = submit_params(persona, "test-trait", vec![ex("c", "d")], Some(100));
        wrong_source
            .as_object_mut()
            .unwrap()
            .insert("source".into(), Value::String("teacher_synthesized".into()));
        let json = executor
            .execute_json("genome/training-trigger/submit", wrong_source)
            .await
            .unwrap();
        assert_eq!(json["success"], false);
        assert_eq!(json["errorKind"], "InconsistentBucket");
        // First-arrival's source survives intact.
        assert_eq!(
            trigger
                .state
                .bucket_example_count(persona, "test-trait", "synthetic"),
            Some(1)
        );
    }

    // what this catches: dispatch failure preserves the bucket contents — the worst
    // regression in this module would be a failed dispatch silently dropping curated
    // examples. A runtime with NO genome module → genome/job-create unregistered →
    // dispatch fails loud at the executor; the bucket must survive intact so the next
    // submit (with the dependency wired) can re-trigger.
    #[tokio::test]
    async fn dispatch_failure_preserves_bucket_contents() {
        let (trigger, executor) = build_runtime_trigger_only().await;
        let persona = Uuid::new_v4();

        let json = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(
                    persona,
                    "test-trait",
                    vec![ex("a", "b"), ex("c", "d")],
                    Some(2),
                ),
            )
            .await
            .unwrap();
        assert_eq!(json["success"], false);
        assert_eq!(json["errorKind"], "DispatchFailed");
        // The two examples must STILL be in the bucket.
        assert_eq!(
            trigger
                .state
                .bucket_example_count(persona, "test-trait", "synthetic"),
            Some(2)
        );
    }

    // what this catches: different personas have isolated buckets. A regression that
    // keyed on trait_kind alone would mix multiple personas' curricula into one
    // training run.
    #[tokio::test]
    async fn different_personas_have_isolated_buckets() {
        let (trigger, executor) = build_runtime_with_trigger_and_genome().await;
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();

        let _ = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(a, "shared-trait", vec![ex("a1", "b1")], Some(5)),
            )
            .await
            .unwrap();
        let _ = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(
                    b,
                    "shared-trait",
                    vec![ex("a2", "b2"), ex("c2", "d2")],
                    Some(5),
                ),
            )
            .await
            .unwrap();

        assert_eq!(
            trigger
                .state
                .bucket_example_count(a, "shared-trait", "synthetic"),
            Some(1)
        );
        assert_eq!(
            trigger
                .state
                .bucket_example_count(b, "shared-trait", "synthetic"),
            Some(2)
        );
        assert_eq!(trigger.state.pending_bucket_count(), 2);
    }

    /// VDD — validation-driven tests verifying the conservation invariant: every
    /// example a producer submits ends up in the dispatched training job EXACTLY once,
    /// in original order, with no duplicates or drops across the accumulate-fire
    /// boundary. TDD pins lifecycle; VDD pins example conservation.
    mod vdd {
        use super::*;
        use crate::genome::fine_tuning::types::TrainingExample;
        use crate::genome::fine_tuning::{
            FineTuningRegistry, RecordingFineTuningAdapter, RECORDING_BASE_PREFIX,
        };
        use crate::modules::genome::GenomeModule;
        use crate::modules::training_trigger::TrainingTriggerModule;
        use crate::runtime::{CommandExecutor, ModuleRegistry};

        /// Build a runtime where genome/job-create routes to a
        /// RecordingFineTuningAdapter (the substrate's canonical fixture for capturing
        /// dispatched TrainingJobRequests). Returns the trigger + executor + the
        /// shared captures handle so the test body can compare dispatched-vs-submitted
        /// at exact example granularity.
        async fn build_recording_runtime() -> (
            Arc<TrainingTriggerModule>,
            Arc<CommandExecutor>,
            Arc<RecordingFineTuningAdapter>,
        ) {
            let registry = Arc::new(ModuleRegistry::new());
            let trigger = Arc::new(TrainingTriggerModule::new());
            registry.register(trigger.clone());

            let ft_registry = Arc::new(FineTuningRegistry::new());
            let recorder = Arc::new(RecordingFineTuningAdapter::new());
            ft_registry.register(recorder.clone());
            registry.register(Arc::new(GenomeModule::new(ft_registry)));

            let executor = Arc::new(CommandExecutor::new(registry.clone()));
            registry.install_executor_on_all(executor.clone());
            (trigger, executor, recorder)
        }

        // what this VDD catches: every example submitted across N submits appears
        // EXACTLY ONCE in a dispatched job — no duplicates, no drops, in submission
        // order. The RecordingFineTuningAdapter captures the dispatched TrainingDataset
        // so we compare prompt-by-prompt against what was submitted.
        #[tokio::test]
        async fn submitted_examples_flow_through_dispatch_intact() {
            let (trigger, executor, recorder) = build_recording_runtime().await;
            let persona = Uuid::new_v4();
            let n = 8;

            let examples: Vec<TrainingExample> = (0..n)
                .map(|i| ex(&format!("prompt-{i}"), &format!("completion-{i}")))
                .collect();
            let mut params = submit_params(persona, "vdd-trait", examples.clone(), Some(n as u32));
            // Route deterministically to the recorder (matches base_model prefix).
            params.as_object_mut().unwrap().insert(
                "baseModel".into(),
                Value::String(format!("{RECORDING_BASE_PREFIX}-vdd")),
            );

            let json = executor
                .execute_json("genome/training-trigger/submit", params)
                .await
                .unwrap();
            assert_eq!(
                json["success"], true,
                "VDD: submit must succeed; got {json}"
            );
            assert_eq!(
                json["examplesUsed"], n,
                "VDD: every submitted example must be in the dispatched job"
            );

            // Bucket cleared exactly once — zero leftover.
            assert_eq!(
                trigger.state.bucket_example_count(
                    persona,
                    "vdd-trait",
                    &format!("{RECORDING_BASE_PREFIX}-vdd")
                ),
                None,
                "VDD: bucket must be fully drained, no leftover"
            );
            assert_eq!(trigger.state.pending_bucket_count(), 0);

            // CONSERVATION CHECK — exactly one job dispatched, exactly n examples
            // captured, examples match submitted set in ORDER.
            assert_eq!(
                recorder.captured_job_count(),
                1,
                "VDD: exactly one job dispatched"
            );
            assert_eq!(
                recorder.captured_example_count(),
                n,
                "VDD: dispatched dataset must carry exactly the {n} examples submitted"
            );

            let captures = recorder.captures();
            let guard = captures.lock().unwrap();
            let dispatched_examples = &guard[0].dataset.examples;
            assert_eq!(dispatched_examples.len(), examples.len());
            for (i, (sub, disp)) in examples.iter().zip(dispatched_examples.iter()).enumerate() {
                assert_eq!(
                    sub.prompt, disp.prompt,
                    "VDD: example {i} prompt mismatch — submission order violated or example replaced"
                );
                assert_eq!(sub.completion, disp.completion);
            }
        }

        // what this VDD catches: conservation across MULTIPLE submits to the same
        // bucket. Accumulated submits, when finally drained-and-dispatched, carry every
        // example in INSERTION ORDER.
        #[tokio::test]
        async fn multi_submit_accumulation_preserves_order_through_dispatch() {
            let (_trigger, executor, recorder) = build_recording_runtime().await;
            let persona = Uuid::new_v4();
            let trait_kind = "multi-submit-vdd";
            let base_model = format!("{RECORDING_BASE_PREFIX}-multi");

            // 4 submits × 3 examples = 12. Threshold 12 → fires only at the last submit.
            let mut all_submitted: Vec<TrainingExample> = Vec::new();
            for batch in 0..4 {
                let exs: Vec<TrainingExample> = (0..3)
                    .map(|i| ex(&format!("b{batch}-p{i}"), &format!("b{batch}-c{i}")))
                    .collect();
                all_submitted.extend(exs.clone());
                let mut params = submit_params(persona, trait_kind, exs, Some(12));
                params
                    .as_object_mut()
                    .unwrap()
                    .insert("baseModel".into(), Value::String(base_model.clone()));
                let _ = executor
                    .execute_json("genome/training-trigger/submit", params)
                    .await
                    .unwrap();
            }

            // Exactly one job dispatched (only the 4th submit crossed threshold).
            assert_eq!(recorder.captured_job_count(), 1);

            let captures = recorder.captures();
            let guard = captures.lock().unwrap();
            let dispatched = &guard[0].dataset.examples;
            assert_eq!(
                dispatched.len(),
                12,
                "VDD: dispatched dataset must carry all 12 accumulated examples"
            );
            for (i, (sub, disp)) in all_submitted.iter().zip(dispatched.iter()).enumerate() {
                assert_eq!(
                    sub.prompt, disp.prompt,
                    "VDD: position {i} reordering — accumulator violated submission order"
                );
            }
        }
    }

    /// Stress / concurrency tests — gated behind the `stress-tests` feature per
    /// CLAUDE.md's test-discipline doctrine. Default `cargo test` does NOT compile
    /// these. Multi-thread tokio runtime + a yielding stub adapter exercise the race
    /// windows the gate protects against.
    #[cfg(feature = "stress-tests")]
    mod stress {
        use super::*;
        use crate::genome::fine_tuning::adapter::{
            FineTuningAdapter, FineTuningCapabilities, FineTuningError, TrainerHardware,
        };
        use crate::genome::fine_tuning::types::{
            ArtifactFormat, JobHandle, JobMetrics, TrainingArtifact, TrainingExample,
            TrainingJobRequest, TrainingStatus,
        };
        use crate::genome::fine_tuning::FineTuningRegistry;
        use crate::modules::genome::GenomeModule;
        use crate::modules::training_trigger::TrainingTriggerModule;
        use crate::runtime::{CommandExecutor, ModuleRegistry};
        use async_trait::async_trait;
        use serde_json::json;
        use std::sync::Mutex as StdMutex;
        use std::time::Duration;

        /// Test-only adapter that captures dispatched `TrainingJobRequest`s and yields
        /// cooperatively (+ a short sleep) before returning, so concurrent submit tasks
        /// WILL interleave at the gate's `.await` boundary under a multi-thread runtime.
        struct YieldingRecordingAdapter {
            captured_requests: Arc<StdMutex<Vec<TrainingJobRequest>>>,
        }

        impl YieldingRecordingAdapter {
            fn new() -> Self {
                Self {
                    captured_requests: Arc::new(StdMutex::new(Vec::new())),
                }
            }
            fn captures(&self) -> Arc<StdMutex<Vec<TrainingJobRequest>>> {
                self.captured_requests.clone()
            }
        }

        #[async_trait]
        impl FineTuningAdapter for YieldingRecordingAdapter {
            fn capabilities(&self) -> FineTuningCapabilities {
                FineTuningCapabilities {
                    provider_id: "stress-yielding-recorder".to_string(),
                    supports_lora: true,
                    supports_validation: false,
                    produces_local_artifact: true,
                    supported_base_model_prefixes: vec!["stress".to_string()],
                    requires: TrainerHardware::Any,
                }
            }

            async fn create_job(
                &self,
                request: TrainingJobRequest,
            ) -> Result<JobHandle, FineTuningError> {
                // WIDE race window: multiple yields + a long-enough sleep open the
                // window for contending submits to interleave reliably across runs.
                for _ in 0..8 {
                    tokio::task::yield_now().await;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
                self.captured_requests.lock().unwrap().push(request.clone());
                Ok(JobHandle {
                    provider_id: "stress-yielding-recorder".into(),
                    provider_job_id: format!("stress-{}", uuid::Uuid::new_v4()),
                    local_id: uuid::Uuid::new_v4(),
                })
            }

            async fn poll(&self, handle: &JobHandle) -> Result<TrainingStatus, FineTuningError> {
                Ok(TrainingStatus::Completed {
                    artifact: TrainingArtifact {
                        model_id: handle.provider_job_id.clone(),
                        local_path: None,
                        format: ArtifactFormat::ProviderHosted,
                        metrics: JobMetrics::default(),
                    },
                })
            }

            async fn cancel(&self, _handle: &JobHandle) -> Result<(), FineTuningError> {
                Ok(())
            }
        }

        async fn build_stress_runtime() -> (
            Arc<TrainingTriggerModule>,
            Arc<CommandExecutor>,
            Arc<StdMutex<Vec<TrainingJobRequest>>>,
        ) {
            let registry = Arc::new(ModuleRegistry::new());
            let trigger = Arc::new(TrainingTriggerModule::new());
            registry.register(trigger.clone());

            let ft_registry = Arc::new(FineTuningRegistry::new());
            let recorder = Arc::new(YieldingRecordingAdapter::new());
            let captures = recorder.captures();
            ft_registry.register(recorder);
            registry.register(Arc::new(GenomeModule::new(ft_registry)));

            let executor = Arc::new(CommandExecutor::new(registry.clone()));
            registry.install_executor_on_all(executor.clone());
            (trigger, executor, captures)
        }

        fn stress_submit_params(
            persona_id: Uuid,
            trait_kind: &str,
            examples: Vec<TrainingExample>,
            min_examples: u32,
        ) -> Value {
            json!({
                "personaId": persona_id,
                "personaName": "stress-p",
                "baseModel": "stress-test",
                "traitKind": trait_kind,
                "examples": examples,
                "source": "operator_curated",
                "minExamples": min_examples,
            })
        }

        // what this catches: a SMOKE-LEVEL conservation check under multi-thread
        // concurrent submits. The YieldingRecordingAdapter captures dispatched datasets
        // so we sum-vs-submitted (true conservation). HONESTY NOTE: exercises the
        // multi-thread dispatch path with yields, but does not deterministically force
        // the C1/C2 race window (a Notify-barrier test is the deterministic exercise).
        #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
        async fn concurrent_submits_to_same_key_serialize_without_loss_stress() {
            let (trigger, executor, captures) = build_stress_runtime().await;
            let persona = Uuid::new_v4();

            // Mix FIRE-LOAD submits (threshold=5, immediately fires) with ACCUMULATOR
            // submits (threshold=100, accumulates 1 example at a time). WITHOUT the gate
            // the fire-load's remove-on-success deletes the accumulator's appended
            // examples; WITH the gate accumulators block behind the fire-load.
            const N_FIRE: usize = 10;
            const FIRE_EXAMPLES: usize = 5;
            const N_ACCUM: usize = 50;
            let total_examples = N_FIRE * FIRE_EXAMPLES + N_ACCUM;

            let mut handles = Vec::with_capacity(N_FIRE + N_ACCUM);
            for accum in 0..N_ACCUM {
                let exec = executor.clone();
                handles.push(tokio::spawn(async move {
                    exec.execute_json(
                        "genome/training-trigger/submit",
                        stress_submit_params(
                            persona,
                            "race-trait",
                            vec![ex(&format!("acc{accum}-p"), &format!("acc{accum}-c"))],
                            100, // never fires alone
                        ),
                    )
                    .await
                }));
            }
            for fire in 0..N_FIRE {
                let exec = executor.clone();
                handles.push(tokio::spawn(async move {
                    exec.execute_json(
                        "genome/training-trigger/submit",
                        stress_submit_params(
                            persona,
                            "race-trait",
                            (0..FIRE_EXAMPLES)
                                .map(|i| {
                                    ex(&format!("fire{fire}-p{i}"), &format!("fire{fire}-c{i}"))
                                })
                                .collect(),
                            5,
                        ),
                    )
                    .await
                }));
            }
            for h in handles {
                let json = h.await.unwrap().expect("submit");
                assert_eq!(
                    json["success"], true,
                    "stress: every submit must succeed; got {json}"
                );
            }

            // Sum examples in dispatched datasets.
            let dispatched: usize = captures
                .lock()
                .unwrap()
                .iter()
                .map(|req| req.dataset.examples.len())
                .sum();
            let pending = trigger
                .state
                .bucket_example_count(persona, "race-trait", "stress-test")
                .unwrap_or(0);
            let total = dispatched + pending;
            assert_eq!(
                total, total_examples,
                "STRESS conservation: dispatched={dispatched} + pending={pending} = {total}, \
                 expected {total_examples}. A drop or duplicate means the gate failed under concurrency."
            );

            // No duplicate prompts in the dispatched datasets.
            let mut seen_prompts = std::collections::HashSet::new();
            for req in captures.lock().unwrap().iter() {
                for ex in &req.dataset.examples {
                    assert!(
                        seen_prompts.insert(ex.prompt.clone()),
                        "STRESS: duplicate prompt {:?} in dispatched datasets — \
                         gate failed to prevent double-drain",
                        ex.prompt
                    );
                }
            }
        }
    }
}
