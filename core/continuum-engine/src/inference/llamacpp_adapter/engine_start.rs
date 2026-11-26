//! Starting the one engine on demand: serving brings the model and the persona scheduler up when it
//! decides to serve, and reads the geometry the scheduler measured, instead of the first resident's
//! turn spawning it lazily with nobody watching.
//!
//! The launch is a contract, not a set of overrides: every field is required, and each comes from
//! the owner that decides it for serving ([`EngineLaunch`]). Nothing is left to a backend default
//! or an environment variable; a value serving cannot name is a refusal at the caller.

use std::num::NonZeroU32;

use super::*;
use crate::inference::backends::persona_scheduler::EngineState;
use llama::KvCacheType;

/// How an adapter's backend is configured when it loads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum LoadPlan {
    /// The registry-resolved adapters (embedding, tests, one-off tools): the knobs below, as before.
    Registry,
    /// The node's one engine: exactly this launch.
    Engine(EngineLaunch),
}

/// Where the engine's layers live. Always explicit: Metal offloads every layer by default and CUDA
/// none, so a placement left to the backend is a different engine on every OS.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offload {
    /// Every layer on the device (the living engine's placement).
    AllLayers,
    /// Every layer in RAM (a node whose device cannot serve this model).
    CpuOnly,
}

impl Offload {
    fn n_gpu_layers(self) -> i32 {
        match self {
            Self::AllLayers => -1,
            Self::CpuOnly => 0,
        }
    }
}

/// The KV element types the engine's context holds, as serving's KV plan decided them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EngineKv {
    pub k: KvCacheType,
    pub v: KvCacheType,
}

/// What the node's one engine loads with. Every field is required; the caller (serving) fills each
/// from its owner: the model and one mind's window from the serving target, the batch from the
/// measured prefill micro-batch, the KV types from the KV plan, the offload from the placement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineLaunch {
    pub model_id: String,
    pub gguf_path: std::path::PathBuf,
    /// The Core's whole context: the mind swapped in has all of it (one stream, `STREAMS`).
    pub context: NonZeroU32,
    /// Tokens per decode call. A whole number of ubatches (the learner's chunk walk needs it).
    pub n_batch: NonZeroU32,
    /// The physical micro-batch a prefill runs in.
    pub n_ubatch: NonZeroU32,
    pub offload: Offload,
    pub kv: EngineKv,
}

/// A launch the engine cannot run as named: refused at construction, never adjusted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EngineLaunchRefusal {
    #[error("the batch ({n_batch}) is not a whole number of ubatches ({n_ubatch})")]
    BatchNotWholeUbatches { n_batch: u32, n_ubatch: u32 },
}

impl EngineLaunch {
    /// Check the launch's own invariants (the parts no field type can carry).
    pub fn validated(self) -> Result<Self, EngineLaunchRefusal> {
        if self.n_batch.get() % self.n_ubatch.get() != 0 {
            return Err(EngineLaunchRefusal::BatchNotWholeUbatches {
                n_batch: self.n_batch.get(),
                n_ubatch: self.n_ubatch.get(),
            });
        }
        Ok(self)
    }

    /// The backend config, field by field: no defaults behind it.
    pub(super) fn config(&self, mmproj_path: Option<std::path::PathBuf>) -> LlamaCppConfig {
        LlamaCppConfig {
            model_path: self.gguf_path.clone(),
            n_batch: self.n_batch.get(),
            n_ubatch: self.n_ubatch.get(),
            n_gpu_layers: self.offload.n_gpu_layers(),
            // The one-off (non-scheduler) context only: one sequence. The scheduler's context holds
            // `STREAMS` and does not read this.
            n_seq_max: 1,
            // The one-off context only; the scheduler's context sets its own (driver.rs).
            flash_attn: FlashAttn::Disabled,
            type_k: self.kv.k,
            type_v: self.kv.v,
            mmproj_path,
            lane_algorithm: continuum_sage::decide::cases::lane::SERVING,
        }
    }
}

impl LlamaCppAdapter {
    /// The node's one engine, exactly as `launch` names it.
    pub fn for_engine(launch: EngineLaunch) -> Self {
        Self {
            backend: Arc::new(RwLock::new(None)),
            model_path: launch.gguf_path.clone(),
            last_throughput_tok_s: Arc::new(RwLock::new(0.0)),
            default_model: launch.model_id.clone(),
            generation: GenerationSupport::Enabled { context: launch.context.get() },
            kv_quant_policy: crate::inference::kv_quant::KvQuantPolicy::default().with_active(launch.kv.k, launch.kv.v),
            n_seq_max_override: None,
            n_ubatch_override: None,
            n_gpu_layers_override: None,
            plan: LoadPlan::Engine(launch),
        }
    }

    /// The scheduler's allocation: one mind's window. The generate path and [`Self::start_engine`]
    /// size it from this one place, so the engine serving started is the engine a turn reaches.
    pub(super) fn allocation(&self) -> Result<GenerationAllocation, String> {
        match self.generation {
            GenerationSupport::EmbeddingOnly => Err("embedding-only adapter does not support text generation".into()),
            GenerationSupport::Enabled { context } => Ok(GenerationAllocation::new(
                std::num::NonZeroU32::new(context).ok_or("invalid zero generation allocation")?,
            )),
        }
    }

    /// Load the model and start the persona scheduler now. The receiver carries the engine's
    /// lifecycle: `Ready` holds the geometry its context measured (per-mind window, sequences),
    /// which is what serving publishes; a refusal names why.
    pub fn start_engine(&self) -> Result<tokio::sync::watch::Receiver<EngineState>, String> {
        let allocation = self.allocation()?;
        self.ensure_loaded()?.start_scheduler(allocation)
    }
}
