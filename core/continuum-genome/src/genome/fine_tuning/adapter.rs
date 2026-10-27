//! [`FineTuningAdapter`] — the trait that abstracts cloud-provider
//! HTTP-API trainers, local Candle in-process training, and
//! cross-grid airc-routed training behind one substrate seam.
//!
//! See `super`'s module doc for the architectural rationale +
//! continuous-learning-loop placement.

use async_trait::async_trait;
use std::sync::Arc;

use super::types::{JobHandle, TrainingJobRequest, TrainingStatus};
use crate::system_resources::hardware_profile::HardwareProfile;

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

    /// The trainer refused BEFORE creating anything: its interpreter is missing, or this
    /// host cannot run it. Unlike [`Self::LocalTrainerFailed`], which can follow a trainer
    /// that started work, this is certain: nothing exists to recover, so the trigger
    /// retries the batch on a later fill and never parks it awaiting evidence. Only
    /// returned from a path that has created no job, process or file (card a9bc56ea:
    /// IntelMac, a missing MLX interpreter wedged Sigurd's bucket in RecoveryRequired).
    #[error("trainer unavailable: {0}")]
    TrainerUnavailable(String),

    /// The memory the trainer is governed to has no room for the smallest window that trains
    /// one of her examples, by measurement: the job is not created, and its examples wait,
    /// owned by the trigger, until [`super::types::CapacityWait::wakes`] (card de9e1342).
    #[error("waiting for capacity: window {} needs {} bytes, {} free", .0.window, .0.need_bytes, .0.governed_free)]
    CapacityParked(super::types::CapacityWait),

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
/// This is an **enum matched against the host's probed profile**: CUDA and
/// Vulkan read the `has_cuda` / `has_vulkan` flags, and Apple Silicon asks
/// the shared hardware classifier (governor's `classify_silicon`, which
/// reads `has_metal` plus the architecture in `HardwareProfile::platform`).
/// No adapter parses the profile itself. One rich struct describes the
/// host; this enum states the adapter's demand. Adding "the same for CUDA or
/// other varieties" is a new variant + a new adapter — the coordinator
/// never changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainerHardware {
    /// Runs on any host — cloud HTTP trainers and accelerator-agnostic
    /// in-process trainers (Candle selects Metal/CUDA/CPU itself).
    Any,
    /// Requires Apple Silicon (Apple's `mlx_lm`). Not merely Metal: an Intel Mac's
    /// discrete GPU reports Metal too, and `mlx_lm` cannot run there.
    AppleSilicon,
    /// Requires an NVIDIA CUDA device.
    Cuda,
    /// Requires a Vulkan device (AMD / non-CUDA GPUs).
    Vulkan,
}

impl TrainerHardware {
    /// Can `host` actually run this trainer? Deterministic: the device
    /// flags, or the shared classifier for Apple Silicon — never a parse
    /// of its own.
    pub fn satisfied_by(&self, host: &HardwareProfile) -> bool {
        match self {
            TrainerHardware::Any => true,
            // Apple Silicon, not merely Metal: an Intel Mac's discrete GPU reports Metal too, and
            // `mlx_lm` cannot run there (IntelMac 2026-10-12: Delia's training dispatch wedged on
            // "mlx python interpreter not found"). One classifier owns the distinction.
            TrainerHardware::AppleSilicon => {
                crate::governor::types::classify_silicon(host) == crate::governor::types::TargetSilicon::AppleM
            }
            TrainerHardware::Cuda => host.has_cuda,
            TrainerHardware::Vulkan => host.has_vulkan,
        }
    }

    /// True when this names a specific host accelerator (not `Any`).
    /// The coordinator ranks a host-native accelerator trainer above
    /// the accelerator-agnostic ones, so on Apple Silicon the MLX trainer
    /// beats the generic Candle trainer.
    pub fn is_specific_accelerator(&self) -> bool {
        !matches!(self, TrainerHardware::Any)
    }
}

/// WHERE a trainer fits the adapter, in the order the coordinator prefers it: the state
/// walk of ONE-RESIDENT-MODEL §S3 (Joel, 2026-09-26: the GPU here, on the resident weights >
/// a GPU on the grid > the CPU here at background priority). Declared by each trainer; the
/// coordinator ranks by it and nothing else decides the order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TrainingPlace {
    /// On the resident weights of a lane that serves the base on this node: no second copy
    /// of the model, no trainer process (Joel, 2026-10-05: "inference and training are
    /// exactly the same model with training switched on").
    ResidentHere,
    /// On the lane of a grid peer that serves the base.
    GridPeer,
    /// On a lane of our own engine stood up for the run, for a base no live lane here serves:
    /// a second copy of the weights, but trained in the engine that serves them, so it ranks
    /// before a separate trainer process (card d67fd801).
    EngineLaneHere,
    /// A trainer holding its own copy of the base on this host.
    SeparateCopyHere,
    /// This host's CPU, at background priority: the last local resort.
    CpuHere,
    /// A provider's service: the artifact lives provider-side.
    Cloud,
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
    /// Where this trainer fits the adapter ([`TrainingPlace`]): the coordinator's ranking.
    pub place: TrainingPlace,
}

/// Where a training job runs (card 975d27fa): on this node, or leased to one grid peer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Destination {
    Local,
    Peer(uuid::Uuid),
}

impl Destination {
    /// The attempt ledger's stored form: `local`, or `peer:<uuid>`.
    pub fn to_ledger(&self) -> String {
        match self {
            Self::Local => "local".into(),
            Self::Peer(peer) => format!("peer:{peer}"),
        }
    }

    /// The ledger's stored form read back; `None` for a form this build does not know.
    pub fn from_ledger(stored: &str) -> Option<Self> {
        match stored.strip_prefix("peer:") {
            Some(peer) => uuid::Uuid::parse_str(peer).ok().map(Self::Peer),
            None if stored == "local" => Some(Self::Local),
            None => None,
        }
    }
}

/// What `genome/job-create` bound for a job before its trainer runs: the attempt and dispatch
/// it runs under, and where. Recorded in the attempt ledger first, so a retry finds the same
/// destination and never chooses again.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobBinding {
    pub attempt: Option<uuid::Uuid>,
    pub dispatch: Option<uuid::Uuid>,
    pub destination: Destination,
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

    /// Can this trainer take `base_model` on THIS node right now? The default is the
    /// static answer (`capabilities`); a trainer bound to live state answers from it: the
    /// in-engine trainer says yes only while a live lane serves that base, so the
    /// coordinator never routes to it a job it would refuse at `create_job`.
    fn serves_base(&self, _base_model: &str) -> bool {
        true
    }

    /// Submit a training job. Returns a [`JobHandle`] the substrate
    /// persists alongside the originating request. Idempotency is
    /// the caller's job (assign a substrate-side correlation id in
    /// `local_id`, pass it on retries — see
    /// [`super::types::JobHandle::local_id`]). Most cloud providers
    /// don't have idempotency keys; the adapter eats duplicate
    /// submissions and the substrate dedupes via its own bookkeeping.
    async fn create_job(&self, request: TrainingJobRequest) -> Result<JobHandle, FineTuningError>;

    /// Where this trainer would run `request`, chosen ONCE before anything is sent and
    /// recorded with the attempt. A trainer on this host runs it here; a grid-peer trainer
    /// names the one peer it binds to.
    fn destination(&self, _request: &TrainingJobRequest) -> Result<Destination, FineTuningError> {
        Ok(Destination::Local)
    }

    /// Submit `request` under the `binding` recorded for it. A trainer on this host needs
    /// nothing from the binding; a grid-peer trainer sends to exactly its destination, under
    /// its attempt, and never chooses again.
    async fn create_bound(&self, request: TrainingJobRequest, _binding: &JobBinding) -> Result<JobHandle, FineTuningError> {
        self.create_job(request).await
    }

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

    /// Re-attach to `local_id`, a job a previous core started, when its run can outlive the
    /// core (an in-engine run survives a core-only restart). The default is for adapters whose
    /// runs die with the core: nothing is resident, so the caller may resume from the input.
    async fn reattach(&self, _local_id: uuid::Uuid) -> Result<super::types::ReattachOutcome, FineTuningError> {
        Ok(super::types::ReattachOutcome::NotResident)
    }
}

/// Convenience alias for the shared-pointer adapter shape that
/// callers store. Matches the inference adapter convention in
/// `ai/adapter.rs`.
pub type ArcFineTuningAdapter = Arc<dyn FineTuningAdapter>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::genome::fine_tuning::types::{
        ArtifactFormat, JobMetrics, TrainingArtifact, TrainingDataset, TrainingSource,
        TrainingStatus,
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
                place: TrainingPlace::SeparateCopyHere,
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
                    format: ArtifactFormat::ProviderHosted,
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
            resume_from: None,
            parent: None,
            dataset: TrainingDataset {
                examples: vec![],
                source: TrainingSource::TeacherSynthesized,
                validation_split: 0.1,
            },
            eval_set: None,
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
