//! [`LocalCandleFineTuner`] — substrate-native in-process LoRA
//! trainer skeleton.
//!
//! This is the architectural slot for the matrix-dojo doctrine
//! ([[matrix-dojo-layer-loading-as-substrate-primitive]] +
//! [[teacher-synthesizes-in-academy-like-dreaming]]). When the
//! optimizer loop lands, training closes loop-locally: noteworthy
//! engrams → teacher synthesis → THIS trainer → safetensors →
//! genome paging, all inside one continuum, no cloud hop required.
//!
//! ## Why a skeleton, not a stub
//!
//! Per [[no-fallbacks-ever]] + the cautionary tale of the dead
//! `genome/job-create` trigger we deleted in PR #1572: an adapter
//! that LIES about completing a job is worse than one that
//! explicitly returns [`FineTuningError::LocalTrainerFailed`].
//!
//! The skeleton:
//!   - **Exists as a real registered adapter**, so the substrate's
//!     [`super::FineTuningCoordinator`] sees it and CAN prefer it
//!     (locality beats cloud per the coordinator's rank function).
//!   - **Returns `LocalTrainerFailed` on `create_job`** with a
//!     pointer to the follow-up task, so the operator sees a typed
//!     "real adapter, math not implemented" signal rather than the
//!     coordinator silently routing past this slot to a cloud
//!     provider.
//!   - **The lifecycle plumbing is real** — `poll` / `cancel`
//!     correctly reject unknown handles with `UnknownHandle`
//!     instead of fabricating a [`super::TrainingStatus`].
//!
//! When the real training loop lands (task #231, see follow-ups in
//! this PR's commit message), this skeleton is replaced in place —
//! same provider id, same capabilities, same trait. No call sites
//! change; the substrate just starts picking local-candle for jobs
//! the coordinator already routes here.
//!
//! ## What's needed to make this real (the follow-up)
//!
//! 1. **LoRA module construction** — frozen base model + trainable
//!    A/B matrices. The math infrastructure already exists in
//!    `inference/lora.rs` for loading + merging; this adapter's
//!    skeleton mirrors that side.
//! 2. **Forward + backward** — through Candle's autograd.
//! 3. **Optimizer** — AdamW with the per-PR-3 LR scheduler the
//!    `ScheduleParams` field implies.
//! 4. **Data loader** — `TrainingExample` → tokenized batches with
//!    the right sequence length + padding.
//! 5. **Training loop** — epoch-major loop with gradient
//!    accumulation, validation pass, loss tracking.
//! 6. **Checkpoint + safetensors output** — writes to
//!    `request.local_artifact_dir` and surfaces the path in the
//!    [`super::TrainingArtifact`].
//! 7. **Job lifecycle state machine** — owned by an in-process
//!    actor; `create_job` spawns it, `poll` reads its
//!    state-snapshot watch channel, `cancel` sends a stop signal.
//!    This part is RTOS-shaped — same pattern as the existing
//!    `MemoryPressureMonitor` etc.

use async_trait::async_trait;

use super::adapter::{FineTuningAdapter, FineTuningCapabilities, FineTuningError};
use super::types::{JobHandle, TrainingJobRequest, TrainingStatus};

const PROVIDER_ID: &str = "local-candle";

/// In-process LoRA trainer skeleton. Holds no state today; the
/// follow-up that implements the optimizer loop adds a
/// `JoinHandle` table + a `watch::Sender<TrainingStatus>` per
/// active job, both behind an `Arc<DashMap<Uuid, ...>>` so `poll`
/// + `cancel` can find a running job's state-snapshot without
/// locking across `await`.
pub struct LocalCandleFineTuner {
    // Intentionally empty in the skeleton — the real impl adds
    // the job table here. Stays a struct (not a unit) so adding
    // fields doesn't change the public construction surface.
}

impl LocalCandleFineTuner {
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for LocalCandleFineTuner {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl FineTuningAdapter for LocalCandleFineTuner {
    fn capabilities(&self) -> FineTuningCapabilities {
        FineTuningCapabilities {
            provider_id: PROVIDER_ID.to_string(),
            // The skeleton declares the eventual real capability
            // set. The coordinator can already PREFER this adapter
            // by locality; create_job is honest about not being
            // ready, so the operator sees the gap loudly when they
            // hit it.
            supports_lora: true,
            supports_validation: true,
            produces_local_artifact: true,
            // Wildcard: validates the actual base on create_job
            // (does the local model cache have it loaded?).
            supported_base_model_prefixes: vec![],
        }
    }

    async fn create_job(
        &self,
        _request: TrainingJobRequest,
    ) -> Result<JobHandle, FineTuningError> {
        Err(FineTuningError::LocalTrainerFailed(
            "LocalCandleFineTuner skeleton: optimizer loop not yet implemented. \
             Track follow-up in tasks #231 (LoRA module + autograd), \
             #232 (data loader + training loop), #233 (checkpoint + safetensors). \
             For now the coordinator picks a registered cloud adapter; explicitly \
             setting preferred_provider=local-candle returns this error so the gap \
             is visible, not silent."
                .into(),
        ))
    }

    async fn poll(&self, handle: &JobHandle) -> Result<TrainingStatus, FineTuningError> {
        // Skeleton has no jobs to poll. Every handle is unknown.
        // The real impl looks up the job in its DashMap; absent
        // handle (post-restart, after the in-process actor lost
        // state) gets the same UnknownHandle response.
        if handle.provider_id != PROVIDER_ID {
            return Err(FineTuningError::UnknownHandle(handle.clone()));
        }
        Err(FineTuningError::UnknownHandle(handle.clone()))
    }

    async fn cancel(&self, handle: &JobHandle) -> Result<(), FineTuningError> {
        if handle.provider_id != PROVIDER_ID {
            return Err(FineTuningError::UnknownHandle(handle.clone()));
        }
        Err(FineTuningError::UnknownHandle(handle.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::fine_tuning::types::{TrainingDataset, TrainingSource};
    use uuid::Uuid;

    fn req() -> TrainingJobRequest {
        TrainingJobRequest {
            persona_id: Uuid::nil(),
            persona_name: "t".into(),
            base_model: "any".into(),
            trait_kind: "t".into(),
            dataset: TrainingDataset {
                examples: vec![],
                source: TrainingSource::OperatorCurated,
                validation_split: 0.0,
            },
            lora: None,
            schedule: None,
            local_artifact_dir: None,
        }
    }

    // what this catches: capabilities are stable. The coordinator
    // RANKS by produces_local_artifact; a future refactor that flips
    // this to false would silently make the substrate-native slot
    // tie with cloud providers (alphabetical fallback would then put
    // it last). This test pins the locality marker.
    #[test]
    fn capabilities_advertise_locality_and_wildcard_base() {
        let caps = LocalCandleFineTuner::new().capabilities();
        assert_eq!(caps.provider_id, "local-candle");
        assert!(caps.produces_local_artifact);
        assert!(caps.supports_lora);
        assert!(caps.supports_validation);
        assert!(caps.supported_base_model_prefixes.is_empty());
    }

    // what this catches: create_job returns LocalTrainerFailed with
    // a discoverable pointer to the follow-up tasks. A future
    // refactor that returns Ok with a fake handle would be the
    // exact "lying about success" pattern the doctrine forbids.
    #[tokio::test]
    async fn create_job_returns_typed_not_yet_implemented() {
        let adapter = LocalCandleFineTuner::new();
        let err = adapter.create_job(req()).await.expect_err("must reject");
        match err {
            FineTuningError::LocalTrainerFailed(msg) => {
                assert!(
                    msg.contains("skeleton") || msg.contains("not yet"),
                    "error message must signal skeleton state: got {msg}"
                );
            }
            other => panic!("expected LocalTrainerFailed, got {other:?}"),
        }
    }

    // what this catches: poll/cancel with a foreign provider_id
    // returns UnknownHandle, not LocalTrainerFailed. The
    // coordinator stores adapters by provider_id; mis-routing a
    // handle to the wrong adapter is a different failure class
    // than "this adapter has no state for this job."
    #[tokio::test]
    async fn poll_rejects_foreign_provider_id_as_unknown_handle() {
        let adapter = LocalCandleFineTuner::new();
        let foreign = JobHandle {
            provider_id: "openai".into(),
            provider_job_id: "ftjob-x".into(),
            local_id: Uuid::nil(),
        };
        let err = adapter.poll(&foreign).await.expect_err("must reject");
        assert!(matches!(err, FineTuningError::UnknownHandle(_)));
    }
}
