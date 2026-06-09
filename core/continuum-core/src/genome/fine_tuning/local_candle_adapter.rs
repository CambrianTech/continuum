//! [`LocalCandleFineTuner`] — substrate-native in-process LoRA
//! trainer. Implements the [`FineTuningAdapter`] contract on top of
//! the [`super::job_actor`] lifecycle actor + the
//! [`super::training_loop::LoRATrainer`] + the
//! [`super::lora_module::LoRAModule`].
//!
//! This is the matrix-dojo doctrine
//! ([[matrix-dojo-layer-loading-as-substrate-primitive]] +
//! [[teacher-synthesizes-in-academy-like-dreaming]]) closed
//! loop-locally: noteworthy engrams → teacher synthesis → THIS
//! adapter → safetensors → genome paging, all inside one continuum,
//! no cloud hop required.
//!
//! ## What this adapter does
//!
//! - **`create_job`** validates the request, spawns a
//!   [`super::job_actor::JobActor`] via
//!   [`super::job_actor::spawn_job`], stores the resulting
//!   [`super::job_actor::JobController`] in a per-adapter
//!   `DashMap<Uuid, JobController>`, returns a
//!   [`super::types::JobHandle`].
//! - **`poll`** looks up the controller by `local_id`, reads the
//!   actor's watch channel for the current
//!   [`super::types::TrainingStatus`].
//! - **`cancel`** flips the controller's cancel flag; the actor
//!   honors it at the next epoch boundary.
//!
//! ## What this adapter is honest about
//!
//! The training math runs against a *synthetic* base weight, not a
//! real loaded model. The LoRA gradient path is exercised end-to-end
//! (forward + cross-entropy + backward + AdamW + safetensors) but
//! the `request.base_model` field is not yet validated against an
//! actual on-disk model. The next slice wires real base-model
//! loading + a model-specific tokenizer. Until then the adapter
//! produces real safetensors files whose A/B contents were trained
//! against a substrate-side stand-in base — useful for end-to-end
//! pipeline validation, but not for production layer publishing.
//!
//! The adapter's capabilities advertise this honestly via
//! `supported_base_model_prefixes: vec![]` (wildcard — caller
//! responsibility to know the base exists).

use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use uuid::Uuid;

use super::adapter::{FineTuningAdapter, FineTuningCapabilities, FineTuningError};
use super::job_actor::{spawn_job, JobActorError, JobController, SpawnJobRequest};
use super::types::{JobHandle, TrainingJobRequest, TrainingStatus};

const PROVIDER_ID: &str = "local-candle";

/// In-process LoRA trainer. Holds a concurrent table of in-flight +
/// terminal job controllers keyed by substrate-side `local_id`.
///
/// Terminal entries remain in the map so repeated `poll`s after
/// completion return the same `Completed { artifact }` payload (the
/// substrate's reputation / lineage subsystem reads metrics from the
/// terminal status; idempotent reads are part of the contract).
pub struct LocalCandleFineTuner {
    jobs: Arc<DashMap<Uuid, JobController>>,
}

impl LocalCandleFineTuner {
    pub fn new() -> Self {
        Self {
            jobs: Arc::new(DashMap::new()),
        }
    }

    /// Number of jobs tracked (in-flight + terminal). Test-only
    /// observability; production callers should `poll` by handle.
    #[cfg(test)]
    pub(super) fn tracked_job_count(&self) -> usize {
        self.jobs.len()
    }
}

impl Default for LocalCandleFineTuner {
    fn default() -> Self {
        Self::new()
    }
}

fn map_actor_error(e: JobActorError) -> FineTuningError {
    match e {
        JobActorError::EmptyDataset
        | JobActorError::InvalidEpochs
        | JobActorError::MissingOutputPath => FineTuningError::InvalidRequest(e.to_string()),
        // Module construction / training / safetensors / candle
        // failures are all "the local trainer couldn't honor the
        // request" — typed as LocalTrainerFailed so the coordinator
        // distinguishes them from transient cloud-provider errors.
        other => FineTuningError::LocalTrainerFailed(other.to_string()),
    }
}

#[async_trait]
impl FineTuningAdapter for LocalCandleFineTuner {
    fn capabilities(&self) -> FineTuningCapabilities {
        FineTuningCapabilities {
            provider_id: PROVIDER_ID.to_string(),
            supports_lora: true,
            supports_validation: true,
            // produces_local_artifact stays true — locality is the
            // coordinator's tie-break signal that makes the substrate
            // pick this adapter ahead of cloud when both are viable.
            produces_local_artifact: true,
            // Wildcard: caller responsibility. Base-model validation
            // (does the local model cache have it?) lands with the
            // real-model-loading slice.
            supported_base_model_prefixes: vec![],
        }
    }

    async fn create_job(
        &self,
        request: TrainingJobRequest,
    ) -> Result<JobHandle, FineTuningError> {
        // Resolve output path. The substrate's convention is
        // `~/.continuum/genome/<persona>/<trait>/<uuid>.safetensors`
        // — when the caller doesn't pin one, default to that under
        // the user's home dir. The full directory tree is created
        // lazily by the safetensors writeout.
        let local_id = Uuid::new_v4();
        let output_path = match request.local_artifact_dir {
            Some(dir) => dir.join(format!("{local_id}.safetensors")),
            None => default_output_path(&request.persona_name, &request.trait_kind, local_id)?,
        };

        let spawn_req = SpawnJobRequest {
            persona_name: request.persona_name,
            base_model: request.base_model,
            trait_kind: request.trait_kind,
            dataset: request.dataset,
            schedule: request.schedule,
            lora: request.lora,
            output_path,
        };

        let controller = spawn_job(spawn_req).map_err(map_actor_error)?;
        self.jobs.insert(local_id, controller);

        Ok(JobHandle {
            provider_id: PROVIDER_ID.to_string(),
            // For in-process adapters, provider_job_id echoes
            // local_id as a string — there's no separate provider
            // side to correlate against.
            provider_job_id: local_id.to_string(),
            local_id,
        })
    }

    async fn poll(&self, handle: &JobHandle) -> Result<TrainingStatus, FineTuningError> {
        if handle.provider_id != PROVIDER_ID {
            return Err(FineTuningError::UnknownHandle(handle.clone()));
        }
        let entry = self
            .jobs
            .get(&handle.local_id)
            .ok_or_else(|| FineTuningError::UnknownHandle(handle.clone()))?;
        Ok(entry.current_status())
    }

    async fn cancel(&self, handle: &JobHandle) -> Result<(), FineTuningError> {
        if handle.provider_id != PROVIDER_ID {
            return Err(FineTuningError::UnknownHandle(handle.clone()));
        }
        let entry = self
            .jobs
            .get(&handle.local_id)
            .ok_or_else(|| FineTuningError::UnknownHandle(handle.clone()))?;
        entry.cancel();
        Ok(())
    }
}

/// Default output path when the caller doesn't pin one. Resolves to
/// `~/.continuum/genome/<persona>/<trait>/<uuid>.safetensors`. Per
/// `[[use-continuum-dir-not-tmp]]`, scratch + artifact paths live
/// under `~/.continuum`, not `/tmp`.
fn default_output_path(
    persona_name: &str,
    trait_kind: &str,
    local_id: Uuid,
) -> Result<std::path::PathBuf, FineTuningError> {
    let home = std::env::var_os("HOME").ok_or_else(|| {
        FineTuningError::InvalidRequest(
            "HOME env var missing; cannot pick default output dir".into(),
        )
    })?;
    let mut path = std::path::PathBuf::from(home);
    path.push(".continuum");
    path.push("genome");
    path.push(sanitize_segment(persona_name));
    path.push(sanitize_segment(trait_kind));
    path.push(format!("{local_id}.safetensors"));
    Ok(path)
}

/// Sanitize a path segment — replace path separators + nul with `_`.
/// Persona / trait names come from caller input; a malicious caller
/// shouldn't be able to escape the genome dir via `../`.
fn sanitize_segment(s: &str) -> String {
    s.chars()
        .map(|c| if matches!(c, '/' | '\\' | '\0') { '_' } else { c })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::fine_tuning::types::{
        LoRAHyperparams, ScheduleParams, TrainingDataset, TrainingExample, TrainingSource,
    };
    use tempfile::tempdir;

    fn small_dataset() -> TrainingDataset {
        TrainingDataset {
            examples: vec![
                TrainingExample {
                    prompt: "hi".into(),
                    completion: "ok".into(),
                    metadata: None,
                },
                TrainingExample {
                    prompt: "yo".into(),
                    completion: "hey".into(),
                    metadata: None,
                },
                TrainingExample {
                    prompt: "foo".into(),
                    completion: "bar".into(),
                    metadata: None,
                },
                TrainingExample {
                    prompt: "ping".into(),
                    completion: "pong".into(),
                    metadata: None,
                },
            ],
            source: TrainingSource::OperatorCurated,
            validation_split: 0.0,
        }
    }

    fn req_with_dir(dir: std::path::PathBuf) -> TrainingJobRequest {
        TrainingJobRequest {
            persona_id: Uuid::nil(),
            persona_name: "test-p".into(),
            base_model: "synthetic".into(),
            trait_kind: "stand-in".into(),
            dataset: small_dataset(),
            lora: Some(LoRAHyperparams {
                rank: 2,
                alpha: 4,
                dropout: 0.0,
                target_modules: vec![],
            }),
            schedule: Some(ScheduleParams {
                epochs: 2,
                batch_size: 2,
                sequence_length: 8,
                learning_rate: 1e-3,
            }),
            local_artifact_dir: Some(dir),
        }
    }

    async fn poll_until_terminal(
        adapter: &LocalCandleFineTuner,
        handle: &JobHandle,
    ) -> TrainingStatus {
        for _ in 0..200 {
            let status = adapter.poll(handle).await.expect("poll");
            if matches!(
                status,
                TrainingStatus::Completed { .. }
                    | TrainingStatus::Failed { .. }
                    | TrainingStatus::Cancelled
            ) {
                return status;
            }
            tokio::time::sleep(std::time::Duration::from_millis(25)).await;
        }
        panic!("adapter never reached terminal status within timeout")
    }

    // what this catches: capabilities are stable + locality marker
    // intact. The coordinator's tie-break rank prefers
    // produces_local_artifact=true; flipping it would silently
    // demote the substrate-native slot below cloud providers.
    #[test]
    fn capabilities_advertise_locality_and_wildcard_base() {
        let caps = LocalCandleFineTuner::new().capabilities();
        assert_eq!(caps.provider_id, "local-candle");
        assert!(caps.produces_local_artifact);
        assert!(caps.supports_lora);
        assert!(caps.supports_validation);
        assert!(caps.supported_base_model_prefixes.is_empty());
    }

    // what this catches: create_job spawns an actor + returns a
    // handle; poll progresses through Running to Completed; the
    // safetensors file lives at the requested path. End-to-end
    // smoke against the matrix-dojo doctrine — substrate hosts
    // training itself.
    #[tokio::test]
    async fn create_job_reaches_completed_and_writes_safetensors() {
        let dir = tempdir().expect("tempdir");
        let adapter = LocalCandleFineTuner::new();
        let handle = adapter
            .create_job(req_with_dir(dir.path().to_path_buf()))
            .await
            .expect("create_job");

        assert_eq!(handle.provider_id, "local-candle");
        assert_eq!(handle.provider_job_id, handle.local_id.to_string());

        let terminal = poll_until_terminal(&adapter, &handle).await;
        match terminal {
            TrainingStatus::Completed { artifact } => {
                let path = artifact.local_path.as_ref().expect("local path");
                assert!(path.starts_with(dir.path()), "artifact under requested dir");
                assert!(path.exists(), "safetensors file must exist");
                assert!(artifact.model_id.contains("local-candle"));
                assert!(artifact.metrics.trained_tokens > 0);
            }
            other => panic!("expected Completed, got {other:?}"),
        }
    }

    // what this catches: empty dataset goes through map_actor_error
    // as InvalidRequest — caller gets a typed boundary failure, not
    // a LocalTrainerFailed (which the substrate treats as
    // potentially-retryable). Distinguishing categories is load-bearing
    // for the coordinator's retry policy.
    #[tokio::test]
    async fn empty_dataset_maps_to_invalid_request() {
        let dir = tempdir().unwrap();
        let mut req = req_with_dir(dir.path().to_path_buf());
        req.dataset.examples.clear();

        let adapter = LocalCandleFineTuner::new();
        let err = adapter.create_job(req).await.expect_err("must reject");
        assert!(
            matches!(err, FineTuningError::InvalidRequest(_)),
            "expected InvalidRequest, got {err:?}"
        );
    }

    // what this catches: poll/cancel with a foreign provider_id
    // returns UnknownHandle. Coordinator stores adapters by
    // provider_id; mis-routing a handle to the wrong adapter is a
    // different failure class than "this adapter has no state for
    // this job."
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

        let err = adapter.cancel(&foreign).await.expect_err("must reject");
        assert!(matches!(err, FineTuningError::UnknownHandle(_)));
    }

    // what this catches: terminal entries stay in the job table so
    // repeated polls work. Without this guarantee, the substrate's
    // reputation / lineage subsystem couldn't reliably read final
    // metrics — it would race the actor's last publish.
    #[tokio::test]
    async fn terminal_entry_stays_tracked() {
        let dir = tempdir().unwrap();
        let adapter = LocalCandleFineTuner::new();
        let handle = adapter
            .create_job(req_with_dir(dir.path().to_path_buf()))
            .await
            .expect("create_job");

        let _ = poll_until_terminal(&adapter, &handle).await;
        assert_eq!(adapter.tracked_job_count(), 1);
        // Repeated poll still works.
        let status = adapter.poll(&handle).await.expect("poll after terminal");
        assert!(matches!(status, TrainingStatus::Completed { .. }));
    }

    // what this catches: path sanitization — a malicious persona /
    // trait name with embedded "../" or "/" cannot escape the
    // genome dir. The default path joins these segments under the
    // user's HOME; without sanitization an attacker controlling
    // persona_name could write anywhere on disk.
    #[test]
    fn sanitize_segment_neutralizes_traversal() {
        assert_eq!(sanitize_segment("../etc/passwd"), ".._etc_passwd");
        assert_eq!(sanitize_segment("a/b\\c"), "a_b_c");
        assert_eq!(sanitize_segment("normal-name"), "normal-name");
    }
}
