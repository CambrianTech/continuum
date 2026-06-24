//! [`FineTuningAdapter`] — the trait that abstracts cloud-provider
//! HTTP-API trainers, local Candle in-process training, and
//! cross-grid airc-routed training behind one substrate seam.
//!
//! See `super`'s module doc for the architectural rationale +
//! continuous-learning-loop placement.

use async_trait::async_trait;
use std::sync::Arc;

use super::types::{JobHandle, TrainingJobRequest, TrainingStatus};
use crate::inference_capability::HardwareProfile;

// ─── Errors ──────────────────────────────────────────────────────────

/// Typed adapter-side failure modes. Callers branch on these to
/// decide retry-vs-surface-to-operator. No stringly-typed errors
/// crossing the trait boundary; per
/// [[every-error-is-an-opportunity-to-battle-harden]] the variant
/// set captures the failure space, not its messages.
#[derive(Debug, thiserror::Error)]
pub enum FineTuningError {
    /// Caller-side validation failure — bad hyperparams, empty
    /// dataset, mismatched base model. Never retriable as-is; the
    /// caller has to fix the request.
    #[error("invalid request: {0}")]
    InvalidRequest(String),

    /// API key missing for a cloud provider. The substrate doesn't
    /// retry; it shells out to the operator (per the
    /// [[no-fallbacks-ever]] rule — silent skip is worse than loud
    /// failure).
    #[error("missing credentials for provider {0}")]
    MissingCredentials(String),

    /// Provider rejected the request (4xx). Body carries the
    /// provider's reason; the substrate logs it and surfaces to the
    /// operator. Not retriable without changing the request.
    #[error("provider rejected job: {0}")]
    ProviderRejected(String),

    /// Provider had a transient failure (5xx, timeout, network
    /// error). The substrate's adapter pool can retry with backoff.
    #[error("transient failure: {0}")]
    Transient(String),

    /// Unknown / unexpected provider response. Substrate logs and
    /// surfaces; same handling as `ProviderRejected` but distinct
    /// signal for telemetry.
    #[error("malformed provider response: {0}")]
    MalformedResponse(String),

    /// Local-trainer-specific failure (Candle device init, file I/O,
    /// optimizer divergence). Carries the underlying error string;
    /// not retriable without changing the request OR the environment.
    #[error("local trainer failed: {0}")]
    LocalTrainerFailed(String),

    /// The job handle doesn't correspond to anything the adapter
    /// knows about. Common cause: substrate restart mid-job, the
    /// adapter's in-memory poll state was lost. Cloud adapters can
    /// still poll the provider's job ID directly; local adapters
    /// can't, so they return this.
    #[error("unknown job handle: {0:?}")]
    UnknownHandle(JobHandle),
}

// ─── Hardware requirement ────────────────────────────────────────────

/// The accelerator an adapter REQUIRES to run, matched against the
/// host's probed [`HardwareProfile`] so the coordinator never routes a
/// job to a trainer the host can't execute (Apple's `mlx_lm` on a Linux
/// box, a CUDA trainer on a Mac).
///
/// This is an **enum matched against boolean device flags** — never a
/// parse of `HardwareProfile::platform` (that field is a free-form
/// human label; routing decisions read `has_metal` / `has_cuda` /
/// `has_vulkan`, never the string). One rich struct describes the host;
/// this enum states the adapter's demand. Adding "the same for CUDA or
/// other varieties" is a new variant + a new adapter — the coordinator
/// never changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainerHardware {
    /// Runs on any host — cloud HTTP trainers and accelerator-agnostic
    /// in-process trainers (Candle selects Metal/CUDA/CPU itself).
    Any,
    /// Requires an Apple-Silicon Metal device (Apple's `mlx_lm`).
    Metal,
    /// Requires an NVIDIA CUDA device.
    Cuda,
    /// Requires a Vulkan device (AMD / non-CUDA GPUs).
    Vulkan,
}

impl TrainerHardware {
    /// Can `host` actually run this trainer? Deterministic match on the
    /// host's device flags — no string parsing.
    pub fn satisfied_by(&self, host: &HardwareProfile) -> bool {
        match self {
            TrainerHardware::Any => true,
            TrainerHardware::Metal => host.has_metal,
            TrainerHardware::Cuda => host.has_cuda,
            TrainerHardware::Vulkan => host.has_vulkan,
        }
    }

    /// True when this names a specific host accelerator (not `Any`).
    /// The coordinator ranks a host-native accelerator trainer above
    /// the accelerator-agnostic ones, so on a Mac the Metal trainer
    /// beats the generic Candle trainer.
    pub fn is_specific_accelerator(&self) -> bool {
        !matches!(self, TrainerHardware::Any)
    }
}

// ─── Capabilities ────────────────────────────────────────────────────

/// Static declaration of what an adapter can do. Lets the dispatcher
/// pick a provider that supports the requested trait kind without
/// actually attempting the job and failing.
#[derive(Debug, Clone)]
pub struct FineTuningCapabilities {
    /// Stable identifier — must match the model_registry provider id
    /// (`openai`, `mistral`, `local-candle`, etc).
    pub provider_id: String,

    /// Whether the adapter accepts LoRA-style training (rank, alpha,
    /// target modules). All current cloud trainers do. Future
    /// full-finetune-only providers would set this `false`.
    pub supports_lora: bool,

    /// Whether the adapter accepts validation splits in the dataset.
    /// Some providers force their own validation policy and ignore
    /// `validation_split`; callers can read this to decide whether
    /// to bother computing one.
    pub supports_validation: bool,

    /// Whether the adapter ships the trained weights as a downloadable
    /// artifact (true for local Candle; varies for cloud). When
    /// false, the artifact lives provider-side and the inference
    /// adapter pulls it on demand by `model_id`.
    pub produces_local_artifact: bool,

    /// Base model id prefixes this adapter can train on. The
    /// dispatcher uses this to pick a compatible adapter given the
    /// request's `base_model`. Empty `Vec` = wildcard; the adapter
    /// will validate the actual base on `create_job`.
    pub supported_base_model_prefixes: Vec<String>,

    /// The accelerator this adapter requires. The coordinator matches
    /// it against the host's probed [`HardwareProfile`] device flags and
    /// never routes a job to a trainer the host can't run. `Any` for
    /// cloud HTTP trainers and accelerator-agnostic in-process trainers.
    pub requires: TrainerHardware,
}

// ─── The trait ───────────────────────────────────────────────────────

/// One adapter per (cloud-provider OR in-process-trainer). Same
/// shape as `AIProviderAdapter` in `ai/adapter.rs`:
/// [`crate::ai::adapter::AIProviderAdapter`] is the inference seam,
/// `FineTuningAdapter` is the training seam. They compose — a
/// cognitive cycle can call inference now, fine-tune later, and
/// the substrate routes both through trait-object dispatch.
///
/// ## Lifetime + concurrency
///
/// Adapters are `Arc<dyn FineTuningAdapter>` — stored in
/// [`super::FineTuningRegistry`] and cloned cheaply per call. The
/// trait is `Send + Sync`; impls own a shared `reqwest::Client` (for
/// cloud) or a shared GPU context (for local) and serialize at the
/// concurrency layer they need.
///
/// ## Error contract
///
/// Every method returns [`FineTuningError`]. The variant taxonomy is
/// the contract: `InvalidRequest` / `MissingCredentials` /
/// `ProviderRejected` / `Transient` / `MalformedResponse` /
/// `LocalTrainerFailed` / `UnknownHandle`. Callers branch on these
/// at the kind level; the inner string is for telemetry, not control
/// flow. Per [[no-fallbacks-ever]] no impl returns `Ok(...)` for
/// a job it didn't actually start.
#[async_trait]
pub trait FineTuningAdapter: Send + Sync {
    /// Static identity + what this adapter accepts. Called once at
    /// registration time + occasionally for dispatcher inspection.
    fn capabilities(&self) -> FineTuningCapabilities;

    /// Submit a training job. Returns a [`JobHandle`] the substrate
    /// persists alongside the originating request. Idempotency is
    /// the caller's job (assign a substrate-side correlation id in
    /// `local_id`, pass it on retries — see
    /// [`super::types::JobHandle::local_id`]). Most cloud providers
    /// don't have idempotency keys; the adapter eats duplicate
    /// submissions and the substrate dedupes via its own bookkeeping.
    async fn create_job(&self, request: TrainingJobRequest) -> Result<JobHandle, FineTuningError>;

    /// Poll the provider for current job state. Cheap — must not
    /// download weights or perform heavy work. Returns a
    /// [`TrainingStatus`] including the [`super::types::TrainingArtifact`]
    /// on terminal `Completed` (downloading the actual safetensors
    /// is a separate step, owned by the caller, NOT poll).
    async fn poll(&self, handle: &JobHandle) -> Result<TrainingStatus, FineTuningError>;

    /// Operator-initiated stop. Adapter sends the provider's cancel
    /// API call (or aborts local training). After this, `poll`
    /// SHOULD return [`TrainingStatus::Cancelled`] for some bounded
    /// window — providers vary on how quickly they reflect the
    /// cancel in their status endpoint.
    async fn cancel(&self, handle: &JobHandle) -> Result<(), FineTuningError>;
}

/// Convenience alias for the shared-pointer adapter shape that
/// callers store. Matches the inference adapter convention in
/// `ai/adapter.rs`.
pub type ArcFineTuningAdapter = Arc<dyn FineTuningAdapter>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::fine_tuning::types::{
        JobMetrics, TrainingArtifact, TrainingDataset, TrainingSource, TrainingStatus,
    };
    use uuid::Uuid;

    /// what this catches: a deliberate stub impl to prove the trait
    /// compiles + is object-safe (dispatched via `Arc<dyn ...>`).
    /// Subsequent PRs add the OpenAI / Mistral / local Candle impls.
    struct StubAdapter;

    #[async_trait]
    impl FineTuningAdapter for StubAdapter {
        fn capabilities(&self) -> FineTuningCapabilities {
            FineTuningCapabilities {
                provider_id: "stub".into(),
                supports_lora: true,
                supports_validation: true,
                produces_local_artifact: true,
                supported_base_model_prefixes: vec![],
                requires: TrainerHardware::Any,
            }
        }

        async fn create_job(
            &self,
            _request: TrainingJobRequest,
        ) -> Result<JobHandle, FineTuningError> {
            Ok(JobHandle {
                provider_id: "stub".into(),
                provider_job_id: "stub-job-1".into(),
                local_id: Uuid::nil(),
            })
        }

        async fn poll(&self, _handle: &JobHandle) -> Result<TrainingStatus, FineTuningError> {
            Ok(TrainingStatus::Completed {
                artifact: TrainingArtifact {
                    model_id: "stub:trained-model".into(),
                    local_path: None,
                    metrics: JobMetrics::default(),
                },
            })
        }

        async fn cancel(&self, _handle: &JobHandle) -> Result<(), FineTuningError> {
            Ok(())
        }
    }

    // what this catches: the trait is dyn-compatible (object-safe),
    // so the registry can store `Arc<dyn FineTuningAdapter>`. A future
    // edit that adds a generic method would break this and we want
    // to know at compile time.
    #[tokio::test]
    async fn trait_is_object_safe() {
        let adapter: ArcFineTuningAdapter = Arc::new(StubAdapter);
        let caps = adapter.capabilities();
        assert_eq!(caps.provider_id, "stub");
        assert!(caps.supports_lora);
    }

    // what this catches: end-to-end happy-path through the trait —
    // create, poll, terminal Completed. A future refactor that
    // moves work between create/poll (e.g. uploading the dataset
    // inside poll instead of create) without updating callers would
    // fail this.
    #[tokio::test]
    async fn stub_roundtrip_completes_with_artifact() {
        let adapter: ArcFineTuningAdapter = Arc::new(StubAdapter);
        let req = TrainingJobRequest {
            persona_id: Uuid::nil(),
            persona_name: "test-persona".into(),
            base_model: "gpt-4o-mini".into(),
            trait_kind: "test-trait".into(),
            dataset: TrainingDataset {
                examples: vec![],
                source: TrainingSource::TeacherSynthesized,
                validation_split: 0.1,
            },
            lora: None,
            schedule: None,
            local_artifact_dir: None,
        };

        let handle = adapter.create_job(req).await.unwrap();
        assert_eq!(handle.provider_job_id, "stub-job-1");

        let status = adapter.poll(&handle).await.unwrap();
        match status {
            TrainingStatus::Completed { artifact } => {
                assert_eq!(artifact.model_id, "stub:trained-model");
            }
            other => panic!("expected Completed, got {other:?}"),
        }

        adapter.cancel(&handle).await.unwrap();
    }
}
