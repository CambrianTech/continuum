//! `genome/job-*` — the LoRA fine-tuning job lifecycle as typed
//! [`ActionCommand`](crate::sdk_codegen::ActionCommand)s, one verb per file.
//!
//! Three verbs drive one job through the [`FineTuningCoordinator`] +
//! [`FineTuningRegistry`] the [`GenomeModule`](crate::modules::genome::GenomeModule)
//! owns:
//!
//! - `genome/job-create` — coordinator picks a capable adapter (honoring an
//!   optional `preferredProvider`), the adapter creates the job, the
//!   [`JobHandle`] + selected provider come back.
//! - `genome/job-status` — look the adapter back up by `jobHandle.providerId`, poll.
//! - `genome/job-cancel` — same lookup, then cancel.
//!
//! Per [[commands-are-dumb-daemons-are-smart]] the verbs are narrow: validate →
//! look up adapter → dispatch. All the smart bits (capability filtering, locality
//! preference, the training work) live in the coordinator + adapters.
//!
//! ## Outcome-as-data, not error
//!
//! Expected domain failures — no capable adapter, an unsatisfiable provider
//! preference, an unknown handle — come back as a typed outcome with
//! `success=false` + an `errorKind` slug, NOT as a transport `Err`. Callers branch
//! on `errorKind` to decide retry-vs-surface without parsing free-form text. This
//! mirrors the legacy `handle_command` envelope 1:1 so every existing caller keeps
//! working; `Err` is reserved for genuine substrate faults (bad params).
//!
//! Access: all three are `Privileged` — creating, polling, and cancelling training
//! jobs spend real compute and touch provider credentials; they are not the
//! `AiSafe` read surface.

use std::sync::Arc;

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::genome::fine_tuning::{
    coordinator::FineTuningCoordinator, FineTuningError, FineTuningRegistry, JobHandle,
};
use crate::sdk_codegen::DynCommand;
use job_create::{ERROR_KIND_CAPACITY_PARKED, ERROR_KIND_TRAINER_UNAVAILABLE};

pub(crate) mod attempt_ledger;
pub mod grid_peer_adapter;
pub mod job_attempt_reconcile;
pub mod lease;
pub mod lease_artifact;
pub mod lease_pull;
pub mod job_cancel;
pub mod job_create;
pub mod job_pause;
pub mod job_reattach;
pub mod job_status;
pub mod teach;

/// Wire shape for `genome/job-cancel`. Status adds its own history continuation.
/// The adapter is selected by `jobHandle.providerId`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/genome/JobLookupParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct JobLookupParams {
    /// The job handle returned by `genome/job-create`.
    #[serde(rename = "jobHandle")]
    pub handle: JobHandle,
}

/// Stable string slug per [`FineTuningError`] variant. Callers branch on this to
/// decide retry-vs-surface without parsing free-form messages. Mirrors the variant
/// taxonomy 1:1; a future error variant must add a slug here too — caught at compile
/// time by the exhaustive match.
pub(crate) fn fine_tuning_error_kind(e: &FineTuningError) -> &'static str {
    match e {
        FineTuningError::InvalidRequest(_) => "InvalidRequest",
        FineTuningError::MissingCredentials(_) => "MissingCredentials",
        FineTuningError::ProviderRejected(_) => "ProviderRejected",
        FineTuningError::Transient(_) => "Transient",
        FineTuningError::MalformedResponse(_) => "MalformedResponse",
        FineTuningError::LocalTrainerFailed(_) => "LocalTrainerFailed",
        FineTuningError::TrainerUnavailable(_) => ERROR_KIND_TRAINER_UNAVAILABLE,
        FineTuningError::CapacityParked(_) => ERROR_KIND_CAPACITY_PARKED,
        FineTuningError::UnknownHandle(_) => "UnknownHandle",
    }
}

/// Build the dep-holding `genome/job-*` command objects. `job-create` drives the
/// coordinator; `job-status` + `job-cancel` look adapters up directly in the
/// registry. Called from `GenomeModule::commands`.
pub(crate) fn command_objects(
    registry: Arc<FineTuningRegistry>,
    coordinator: Arc<FineTuningCoordinator>,
    attempts: Arc<attempt_ledger::AttemptLedger>,
    lease_transport: Arc<dyn grid_peer_adapter::LeaseTransport>,
    #[cfg(test)] test_job_board: Arc<crate::genome::fine_tuning::TrainingJobBoard>,
    #[cfg(test)] test_artifacts: Arc<tempfile::TempDir>,
) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(job_attempt_reconcile::GenomeJobAttemptReconcile {
            attempts: Arc::clone(&attempts),
            transport: lease_transport,
        }),
        Arc::new(lease::GenomeLeaseTrain {
            coordinator: Arc::clone(&coordinator),
            attempts: Arc::clone(&attempts),
        }),
        Arc::new(lease::GenomeLeaseStatus { attempts: Arc::clone(&attempts) }),
        Arc::new(lease::GenomeLeasePoll { attempts: Arc::clone(&attempts), registry: registry.clone() }),
        Arc::new(lease::GenomeLeaseCancel { attempts: Arc::clone(&attempts), registry: registry.clone() }),
        Arc::new(lease::GenomeLeaseArtifact { attempts: Arc::clone(&attempts) }),
        Arc::new(job_create::GenomeJobCreate {
            coordinator,
            attempts,
            #[cfg(test)]
            test_job_board: test_job_board.clone(),
            #[cfg(test)]
            test_artifacts,
        }),
        Arc::new(job_status::GenomeJobStatus {
            registry: registry.clone(),
            #[cfg(test)]
            test_job_board,
        }),
        Arc::new(job_pause::GenomeJobPause { registry: registry.clone() }),
        Arc::new(job_reattach::GenomeJobReattach { registry: registry.clone() }),
        Arc::new(job_cancel::GenomeJobCancel { registry }),
    ]
}

/// Shared fine-tuning test fixtures for the `genome/job-*` verb files. One stub
/// adapter + builders live here so each verb's `mod tests` reuses them instead of
/// re-deriving a parallel mock (CLAUDE.md: one fixture per concern).
#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::Arc;

    use async_trait::async_trait;
    use uuid::Uuid;

    use crate::genome::fine_tuning::adapter::{
        FineTuningAdapter, FineTuningCapabilities, TrainerHardware,
    };
    use crate::genome::fine_tuning::types::{
        ArtifactFormat, JobMetrics, TrainingArtifact, TrainingDataset, TrainingSource,
        TrainingStatus,
    };
    use crate::genome::fine_tuning::{
        FineTuningError, FineTuningRegistry, JobHandle, TrainingJobRequest,
    };

    /// Adapter that returns predictable Ok values — exercises the verbs' wire shape,
    /// not the adapters themselves.
    pub(crate) struct OkStubAdapter(pub &'static str);

    #[async_trait]
    impl FineTuningAdapter for OkStubAdapter {
        fn capabilities(&self) -> FineTuningCapabilities {
            FineTuningCapabilities {
                provider_id: self.0.to_string(),
                supports_lora: true,
                supports_validation: true,
                produces_local_artifact: false,
                supported_base_model_prefixes: vec![],
                requires: TrainerHardware::Any,
                place: crate::genome::fine_tuning::adapter::TrainingPlace::Cloud,
            }
        }
        async fn create_job(&self, _r: TrainingJobRequest) -> Result<JobHandle, FineTuningError> {
            Ok(JobHandle {
                provider_id: self.0.to_string(),
                provider_job_id: format!("{}-job-1", self.0),
                local_id: Uuid::nil(),
            })
        }
        async fn poll(&self, _h: &JobHandle) -> Result<TrainingStatus, FineTuningError> {
            Ok(TrainingStatus::Completed {
                artifact: TrainingArtifact {
                    model_id: format!("{}:trained", self.0),
                    local_path: None,
                    format: ArtifactFormat::ProviderHosted,
                    metrics: JobMetrics::default(),
                },
            })
        }
        async fn cancel(&self, _h: &JobHandle) -> Result<(), FineTuningError> {
            Ok(())
        }
    }

    pub(crate) fn dataset() -> TrainingDataset {
        TrainingDataset {
            // One real pair: an empty dataset is now REJECTED by job-create's
            // resolution gate before adapter selection (it would burn a job on
            // nothing), so the shared fixture must be trainable-shaped.
            examples: vec![crate::genome::fine_tuning::TrainingExample {
                prompt: "ctx".into(),
                completion: "act".into(),
                metadata: None,
                lived: None,
            }],
            source: TrainingSource::OperatorCurated,
            validation_split: 0.0,
        }
    }

    /// A minimal valid request for `base`, matching the OkStubAdapter's any-base
    /// capability.
    pub(crate) fn request_for(base: &str) -> TrainingJobRequest {
        TrainingJobRequest {
            persona_id: Uuid::nil(),
            persona_name: "test".into(),
            base_model: base.into(),
            trait_kind: "test-trait".into(),
            resume_from: None,
            parent: None,
            dataset: dataset(),
            eval_set: None,
            lora: None,
            schedule: None,
            local_artifact_dir: None,
        }
    }

    /// A base GGUF with one tensor filled with `weight`: the same header for every `weight`
    /// (architecture, tokenizer, shapes), other weights, so a fine-tuned sibling is one call.
    pub(crate) fn write_base_gguf(path: &std::path::Path, weight: f32) {
        use candle_core::quantized::{gguf_file, GgmlDType, QTensor};
        let md = [
            ("general.architecture", gguf_file::Value::String("qwen2".into())),
            ("tokenizer.ggml.tokens", gguf_file::Value::Array(vec![gguf_file::Value::String("a".into())])),
        ];
        let refs: Vec<(&str, &gguf_file::Value)> = md.iter().map(|(k, v)| (*k, v)).collect();
        let weights = candle_core::Tensor::full(weight, (4, 32), &candle_core::Device::Cpu).expect("tensor"); // JUSTIFIED: test fixture
        let q = QTensor::quantize(&weights, GgmlDType::F32).expect("quantize"); // JUSTIFIED: test fixture
        let mut file = std::fs::File::create(path).expect("create"); // JUSTIFIED: test fixture
        gguf_file::write(&mut file, &refs, &[("blk.0.attn_q.weight", &q)]).expect("write"); // JUSTIFIED: test fixture
    }

    /// A registry seeded with one stub adapter per id.
    pub(crate) fn registry_with(ids: &[&'static str]) -> Arc<FineTuningRegistry> {
        let reg = Arc::new(FineTuningRegistry::new());
        for id in ids {
            reg.register(Arc::new(OkStubAdapter(id)));
        }
        reg
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk_codegen::ActionCommand;
    use job_cancel::GenomeJobCancel;
    use job_create::GenomeJobCreate;
    use job_status::GenomeJobStatus;

    // what this catches: the three genome verbs carry their `genome/job-<verb>` wire
    // names — the routing keys cu, the persona tool surface, and the grid bind to. The
    // name mirrors the file path; drift silently breaks the "file tree IS the namespace"
    // contract.
    #[test]
    fn genome_command_names_mirror_their_path() {
        assert_eq!(GenomeJobCreate::NAME, "genome/job-create");
        assert_eq!(GenomeJobStatus::NAME, "genome/job-status");
        assert_eq!(GenomeJobCancel::NAME, "genome/job-cancel");
    }

    // what this catches: command_objects assembles every genome/job-* verb (reconcile included) and the two lease verbs — a dropped entry
    // would silently remove a genome command from the registry.
    #[test]
    fn command_objects_assembles_all_three() {
        let registry = Arc::new(FineTuningRegistry::new());
        let coordinator = Arc::new(FineTuningCoordinator::new(registry.clone()));
        let objs = command_objects(
            registry,
            coordinator,
            Arc::new(attempt_ledger::AttemptLedger::new()),
            Arc::new(grid_peer_adapter::ExecutorLeaseTransport(Arc::new(crate::runtime::LateBound::new("test lease executor")))),
            Arc::new(crate::genome::fine_tuning::TrainingJobBoard::default()),
            Arc::new(tempfile::tempdir().unwrap()),
        );
        assert_eq!(objs.len(), 11);
        assert!(objs.iter().any(|c| c.name() == "genome/lease/artifact"), "the lease's artifact ranges are served");
    }
}
