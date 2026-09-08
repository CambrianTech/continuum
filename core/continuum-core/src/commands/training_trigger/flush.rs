//! `genome/training-trigger/flush` — force-dispatch a single
//! `(persona_id, trait_kind, base_model)` bucket regardless of its threshold,
//! draining whatever examples it holds into a `genome/job-create`.
//!
//! Dep-holding: shares the owning module's
//! [`TrainingTriggerState`](crate::modules::training_trigger::TrainingTriggerState),
//! so flush serializes on the SAME per-key gate as [`submit`](super::submit) — a
//! flush can never race a concurrent submit's drain/restore on the same key.
//!
//! ## Gating
//!
//! `Privileged` — flushing dispatches a training job (spends compute + may touch
//! provider credentials), same surface as submit.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::genome::fine_tuning::types::JobHandle;
use crate::modules::training_trigger::{BucketKey, DispatchResult, TrainingTriggerState};
use crate::sdk_codegen::CommandError;

/// `genome/training-trigger/flush` input. `base_model` is required so the flush
/// targets exactly one bucket — a persona may have multiple `(trait_kind,
/// base_model)` buckets pending and flush picks a single one (per the
/// BucketKey-includes-base_model contract).
#[derive(Debug, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/training_trigger/FlushParams.ts"
)]
pub struct FlushParams {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub trait_kind: String,
    pub base_model: String,
}

/// `genome/training-trigger/flush` typed outcome — mirrors submit's outcome-as-data
/// shape: job dispatched, nothing to flush (idempotent no-op), or dispatch failed all
/// come back as `success` + a discriminator, NOT a transport error.
#[derive(Debug, Clone, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/training_trigger/FlushOutcome.ts"
)]
pub struct FlushOutcome {
    pub success: bool,
    /// Discriminator: `"JobDispatched"` | `"NothingToFlush"` (absent on failure).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub outcome: Option<String>,
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
    /// Dispatch/persistence refusal or unresolved provider outcome.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
    /// DispatchFailed, PersistenceFailed, or RecoveryRequired discriminator.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error_kind: Option<String>,
}

impl FlushOutcome {
    fn base(success: bool) -> Self {
        Self {
            success,
            outcome: None,
            examples_used: None,
            selected_provider: None,
            job_handle: None,
            error: None,
            error_kind: None,
        }
    }

    fn nothing_to_flush() -> Self {
        Self {
            outcome: Some("NothingToFlush".into()),
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

    fn dispatch_failed(error: String) -> Self {
        Self {
            error: Some(error),
            error_kind: Some("DispatchFailed".into()),
            ..Self::base(false)
        }
    }
}

crate::action_command! {
    /// Force-dispatch a persona's pending training bucket for a `(traitKind,
    /// baseModel)` regardless of how many examples it holds — drains it into a
    /// `genome/job-create` immediately. Returns `JobDispatched` with the handle +
    /// selected provider, or `NothingToFlush` if the bucket is empty/absent (flush is
    /// idempotent). Explicit provider refusals remain retryable; uncertain results
    /// return `RecoveryRequired` and only inspect correlated JobBoard evidence.
    /// Accepted examples remain persisted in either case.
    pub struct TrainingTriggerFlush {
        state: Arc<TrainingTriggerState>,
    }
    name: "genome/training-trigger/flush",
    access: Privileged,
    params: FlushParams,
    output: FlushOutcome,
    run(this, _ctx, p) => {
        let state = &this.state;
        let key = BucketKey {
            persona_id: p.persona_id,
            trait_kind: p.trait_kind.clone(),
            base_model: p.base_model.clone(),
        };

        state.run_owned(key, |state, key| async move {
            match state.dispatch_pending(&key).await {
                DispatchResult::Empty => FlushOutcome::nothing_to_flush(),
                DispatchResult::Dispatched { examples, handle, provider } =>
                    FlushOutcome::job_dispatched(examples as u32, provider, handle),
                DispatchResult::Failed { kind, error } => {
                    let mut outcome = FlushOutcome::dispatch_failed(error);
                    outcome.error_kind = Some(kind.into());
                    outcome
                }
            }
        }).await.map_err(CommandError::Internal)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::training_trigger::test_support::{
        build_runtime_with_trigger_and_genome, ex, submit_params,
    };
    use crate::sdk_codegen::{AccessLevel, ActionCommand};
    use serde_json::json;
    use uuid::Uuid;

    // what this catches: name/access wiring — flush dispatches a training job, so it
    // lives on the Privileged surface, not AiSafe.
    #[test]
    fn name_and_access_wired() {
        assert_eq!(TrainingTriggerFlush::NAME, "genome/training-trigger/flush");
        assert!(matches!(
            TrainingTriggerFlush::ACCESS,
            AccessLevel::Privileged
        ));
    }

    // what this catches: flush dispatches a bucket that has NOT crossed its threshold.
    // The local trainer needs at least one full batch; 5 examples gives one full batch
    // and exercises flush's dispatch-on-partial-bucket contract.
    #[tokio::test]
    async fn flush_dispatches_partial_bucket() {
        let (trigger, executor, _dir) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();

        let examples = (0..5)
            .map(|i| ex(&format!("p-{i}"), &format!("c-{i}")))
            .collect();
        let _ = executor
            .execute_json(
                "genome/training-trigger/submit",
                submit_params(persona, "test-trait", examples, Some(100)),
            )
            .await
            .unwrap();
        assert_eq!(
            trigger
                .state
                .bucket_example_count(persona, "test-trait", "synthetic"),
            Some(5)
        );

        let json = executor
            .execute_json(
                "genome/training-trigger/flush",
                json!({
                    "personaId": persona,
                    "traitKind": "test-trait",
                    "baseModel": "synthetic",
                }),
            )
            .await
            .unwrap();
        assert_eq!(json["success"], true, "got: {json}");
        assert_eq!(json["outcome"], "JobDispatched");
        assert_eq!(json["examplesUsed"], 5);
        assert_eq!(
            trigger
                .state
                .bucket_example_count(persona, "test-trait", "synthetic"),
            None
        );

        // Same structural-eviction pin as the submit lease site — flush is the other
        // lease site, so it gets its own production-shaped regression test.
        assert_eq!(
            trigger.state.submit_gates.len(),
            0,
            "submit_gates MUST be empty after successful flush — \
             a non-zero count indicates the structural-eviction-on-drop \
             invariant is broken"
        );
    }

    // what this catches: flush on an empty/absent bucket is a no-op success, not an
    // error. Idempotent flush lets callers retry safely without checking state first.
    #[tokio::test]
    async fn flush_empty_bucket_is_noop() {
        let (_trigger, executor, _dir) = build_runtime_with_trigger_and_genome().await;
        let persona = Uuid::new_v4();

        let json = executor
            .execute_json(
                "genome/training-trigger/flush",
                json!({"personaId": persona, "traitKind": "nope", "baseModel": "synthetic"}),
            )
            .await
            .unwrap();
        assert_eq!(json["success"], true);
        assert_eq!(json["outcome"], "NothingToFlush");
    }
}
