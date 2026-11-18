//! Starting the one engine on demand: serving brings the model and the persona scheduler up when it
//! decides to serve, and reads the geometry the scheduler measured, instead of the first resident's
//! turn spawning it lazily with nobody watching.

use super::*;
use crate::inference::backends::persona_scheduler::EngineState;

impl LlamaCppAdapter {
    /// The node's one engine for `model_id`: one mind's window of `context`, and every load knob
    /// (batch, offload, KV types) left to the backend's own measured defaults rather than a number
    /// the caller guessed. No sequence count: one mind decodes at a time.
    pub fn for_engine(model_id: &str, gguf_path: &std::path::Path, context: u32) -> Self {
        Self {
            backend: Arc::new(RwLock::new(None)),
            model_path: gguf_path.to_path_buf(),
            last_throughput_tok_s: Arc::new(RwLock::new(0.0)),
            default_model: model_id.to_string(),
            generation: GenerationSupport::Enabled { context },
            kv_quant_policy: crate::inference::kv_quant::KvQuantPolicy::default(),
            n_seq_max_override: None,
            n_ubatch_override: None,
            n_gpu_layers_override: None,
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
