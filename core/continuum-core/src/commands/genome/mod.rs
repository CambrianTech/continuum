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
//! - `genome/job-status` — look the adapter back up by `handle.providerId`, poll.
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

pub mod curriculum;
pub mod job_cancel;
pub mod job_create;
pub mod job_status;
pub mod teach;

/// Wire shape for `genome/job-status` + `genome/job-cancel`. A single handle;
/// adapter lookup keys on `handle.providerId`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/genome/JobLookupParams.ts")]
#[serde(rename_all = "camelCase")]
pub struct JobLookupParams {
    /// The job handle returned by `genome/job-create`.
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
        FineTuningError::UnknownHandle(_) => "UnknownHandle",
    }
}

/// Build the dep-holding `genome/job-*` command objects. `job-create` drives the
/// coordinator; `job-status` + `job-cancel` look adapters up directly in the
/// registry. Called from `GenomeModule::commands`.
pub fn command_objects(
    registry: Arc<FineTuningRegistry>,
    coordinator: Arc<FineTuningCoordinator>,
) -> Vec<Arc<dyn DynCommand>> {
    vec![
        Arc::new(job_create::GenomeJobCreate { coordinator }),
        Arc::new(job_status::GenomeJobStatus {
            registry: registry.clone(),
        }),
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
            }
        }
        async fn create_job(
            &self,
            _r: TrainingJobRequest,
        ) -> Result<JobHandle, FineTuningError> {
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
            dataset: dataset(),
            eval_set: None,
            lora: None,
            schedule: None,
            local_artifact_dir: None,
        }
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

    // what this catches: command_objects assembles all three verbs — a dropped entry
    // would silently remove a genome command from the registry.
    #[test]
    fn command_objects_assembles_all_three() {
        let registry = Arc::new(FineTuningRegistry::new());
        let coordinator = Arc::new(FineTuningCoordinator::new(registry.clone()));
        let objs = command_objects(registry, coordinator);
        assert_eq!(objs.len(), 3);
    }
}
