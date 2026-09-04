//! Fine-tuning subsystem — the conveyor belt from noteworthy
//! experience to deployed LoRA layer.
//!
//! ## Where this sits in the continuous-learning loop
//!
//! The substrate's continuous-learning doctrine
//! ([[teacher-synthesizes-in-academy-like-dreaming]],
//! [[noteworthy-flag-feeds-memory-AND-curriculum]],
//! [[matrix-dojo-layer-loading-as-substrate-primitive]],
//! [[mesh-of-lessons-cross-persona-curricula]],
//! [[exponential-compounding-via-inherited-layers]]) compresses to:
//!
//! 1. **Noteworthy engrams** accumulate per persona during normal
//!    operation. The hippocampus consolidation flag and the curriculum
//!    selector look at the SAME importance signal — one drain into
//!    long-term memory, one drain into a training queue.
//! 2. **Teacher persona** in academy synthesizes curated training
//!    examples from those engrams. It's the substrate's dream phase —
//!    not raw experience, but distilled experience.
//! 3. **Fine-tuning subsystem** (this module) takes the curated
//!    dataset, hyperparams, and provider preference, and runs a job.
//!    The job produces a LoRA artifact.
//! 4. **Forge / alloy** signs and addresses the artifact for trust +
//!    provenance ([[forge-alloy-secures-commodity-zero-trust-plus-reputation]]).
//! 5. **Genome paging** (`persona/genome_paging.rs`) loads the layer
//!    into a persona's working set when the corresponding skill is
//!    activated; LRU evicts under pressure.
//! 6. **Mesh of lessons** lets any persona on the grid receive
//!    another persona's layer ([[mesh-of-lessons-cross-persona-curricula]]).
//!
//! This module owns stage 3. Stages 4–6 plug in at clearly-marked
//! seams (the `TrainingArtifact` is the alloy input; the
//! `JobMetrics` is the reputation signal; the persona id on
//! `TrainingJobRequest` is the genome paging target).
//!
//! ## Adapter pattern
//!
//! [`FineTuningAdapter`] is the inversion-of-control seam. The same
//! trait abstracts:
//!
//! - **Cloud providers** (OpenAI, Mistral, Anthropic, Fireworks,
//!   DeepSeek, Together) — HTTP-API clients. The job runs on the
//!   provider's GPU; we poll until done.
//! - **Local Candle** — in-process LoRA training using the existing
//!   Candle infrastructure (`inference/lora.rs` already loads +
//!   merges; this module's local impl writes the optimizer loop).
//!   The job runs on our own GPU; we own the artifact directly.
//! - **AircRemoteFineTuner** (future) — cross-grid training: the
//!   crap-Mac persona ships its curriculum to the 5090 sibling
//!   continuum via airc and gets a layer back. Same shape as
//!   `AircRemoteInferenceAdapter` from task #108, applied to
//!   training instead of inference.
//!
//! All three look the same to the caller: `create_job` returns a
//! [`JobHandle`]; `poll` returns [`TrainingStatus`]; terminal
//! [`TrainingArtifact`] flows into forge/alloy regardless of source.
//!
//! ## Typed all the way down
//!
//! No `serde_json::Value` pass-through. Hyperparams are typed
//! ([`LoRAHyperparams`] + [`ScheduleParams`]). Dataset provenance is
//! typed ([`TrainingSource`]) so the academy → teacher → fine-tune
//! flow can carry "this came from teacher synthesis" vs "this came
//! from operator-supplied corpus" as a load-bearing fact, not a
//! convention. Errors are typed ([`FineTuningError`]) so the caller
//! can branch on transient-vs-terminal without parsing strings.
//!
//! Per [[commands-are-dumb-daemons-are-smart]]: the `genome/job-create`
//! command stays thin — it validates + dispatches. The smart bits
//! (provider selection, retry, alloy emission) live in this module
//! and the cloud-vs-local adapter implementations.
//!
//! ## Doctrinal alignment
//!
//! - `[[no-fallbacks-ever]]` — every adapter declares failure
//!   modes. No silent rejection. The dead channel.rs trigger that
//!   #227 deleted is the cautionary tale.
//! - `[[rust-is-the-core-node-is-the-shell]]` — fine-tuning is
//!   substrate work. The 6 TS LoRA trainer adapters in
//!   `src/daemons/ai-provider-daemon/adapters/*/server/*FineTuningAdapter.ts`
//!   become deletable once each provider has a Rust adapter here.
//! - `[[inference-is-an-adapter-always-in-the-loop]]` extended:
//!   fine-tuning is also an adapter, always in the loop. Local,
//!   cloud, and cross-grid impls are peers, not a hierarchy.

pub mod adapter;
pub mod byte_tokenizer;
pub mod coordinator;
pub mod job_actor;
pub mod job_board;
pub mod local_candle_adapter;
pub mod lora_module;
pub mod mlx_lora_adapter;
pub mod openai_adapter;
#[cfg(any(test, feature = "test-fixtures"))]
pub mod recording_adapter;
pub mod registry;
pub mod safetensors_io;
pub mod training_loop;
pub mod types;

pub use adapter::{
    ArcFineTuningAdapter, FineTuningAdapter, FineTuningCapabilities, FineTuningError,
    TrainerHardware,
};
pub use byte_tokenizer::{ByteTokenizer, BYTE_PAD_ID, BYTE_VOCAB};
pub use coordinator::{CoordinatorError, FineTuningCoordinator};
pub use job_actor::{spawn_job, JobActorError, JobController, SpawnJobRequest};
pub use job_board::{TrainingJobBoard, WatchedJob};
pub use local_candle_adapter::{LocalCandleFineTuner, SYNTHETIC_BASE_PREFIX};
pub use lora_module::{LoRAError, LoRAModule};
pub use mlx_lora_adapter::MlxLoraFineTuner;
pub use openai_adapter::OpenAIFineTuningAdapter;
#[cfg(any(test, feature = "test-fixtures"))]
pub use recording_adapter::{
    RecordingFineTuningAdapter, RECORDING_BASE_PREFIX, RECORDING_PROVIDER_ID,
};
pub use registry::FineTuningRegistry;
pub use safetensors_io::{write_lora_safetensors, SafetensorsIoError, LORA_A_KEY, LORA_B_KEY};
pub use training_loop::{
    DataLoader, LoRATrainer, TokenizedBatch, TokenizedExample, Tokenizer, TrainingError,
    TrainingMetrics,
};
pub use types::{
    ArtifactFormat, JobHandle, JobMetrics, LoRAHyperparams, ScheduleParams, TrainingArtifact,
    TrainingDataset, TrainingExample, TrainingJobRequest, TrainingSource, TrainingStatus,
};
