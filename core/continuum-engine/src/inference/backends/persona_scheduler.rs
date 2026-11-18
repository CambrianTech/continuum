//! The in-process scheduler for our minds (CONTINUUM-ENGINE 4.14): one libllama context driven by
//! the persona engine, ONE mind decoding at a time. It replaces the continuous-batching scheduler
//! that advanced N sequences per decode (4 at once = 2 tok/s each; one at a time = the full rate).
//!
//! Each persona keeps her blocks between turns, so a turn prefills only the tokens her blocks do not
//! hold ([`continuum_persona_engine::turn::plan`]); the old scheduler dropped every sequence when its
//! turn ended and re-prefilled the whole prompt next time. A switch is the engine's native bind (her
//! blocks stay in RAM as she left them), never a serialize to disk.
//!
//! - [`Scheduler::enqueue`] sends a [`GenerationRequest`] to the driver thread;
//! - the driver serves turns one after another: switches to her (stage, settle, handoff, release the
//!   previous mind to the RAM tier), prefills her new tail, decodes until EOG / a stop sequence /
//!   max_tokens, and streams [`TokenEvent`]s back;
//! - genes apply when she is bound, for her decodes only, so her history is never decoded under
//!   another gene set; the gene set is part of her basis, so a turn under another set starts her over.
//!
//! Not yet: overlapping the next mind's copy-in with the active decode (the tier's moves are
//! synchronous today), and paging cold minds out of RAM.

mod driver;
mod sampling;

use std::sync::Arc;

use llama::{FlashAttn, KvCacheType, LoraAdapter, Model};
use uuid::Uuid;

use super::SamplingConfig;

/// Token event streamed from the scheduler to the requester.
#[derive(Debug)]
pub enum TokenEvent {
    /// One generated token piece (UTF-8 fragment from the tokenizer).
    Token(String),
    /// Generation finished cleanly (EOG / max_tokens / stop sequence).
    Done { tokens_generated: usize, elapsed_ms: u64 },
    /// Generation failed: a refused move, a decode error.
    Error(String),
}

/// What a caller submits to the scheduler.
pub struct GenerationRequest {
    pub prompt: String,
    pub max_tokens: usize,
    pub sampling: SamplingConfig,
    pub stop_sequences: Vec<String>,
    /// Genes for this turn: `(id, handle, scale)`, resolved by the backend before enqueue. Empty is
    /// the base model.
    pub active_loras: Vec<(String, Arc<LoraAdapter>, f32)>,
    /// Tokens stream back through this.
    pub response_tx: tokio::sync::mpsc::UnboundedSender<TokenEvent>,
    /// The persona whose mind this turn continues. `None` is a one-off (a probe, a test rig): it
    /// runs as a mind of its own and its blocks are released when the turn ends.
    pub persona_id: Option<Uuid>,
}

/// The context the scheduler creates. Its window holds [`continuum_persona_llama::STREAMS`] minds'
/// windows (the active mind's and the next one's); no sequence count is configurable.
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    pub n_ctx: u32,
    pub n_batch: u32,
    pub n_ubatch: u32,
    pub flash_attn: FlashAttn,
    pub fused_gdn_ar: bool,
    pub fused_gdn_ch: bool,
    pub type_k: KvCacheType,
    pub type_v: KvCacheType,
}

/// Public handle. Cloneable; clones share the one driver thread and context.
#[derive(Clone)]
pub struct Scheduler {
    request_tx: std::sync::mpsc::Sender<GenerationRequest>,
}

impl Scheduler {
    /// Spawn the driver thread. It runs until the last handle is dropped and its last turn ended.
    pub fn spawn(model: Arc<Model>, config: SchedulerConfig) -> Result<Self, String> {
        let (request_tx, request_rx) = std::sync::mpsc::channel();
        std::thread::Builder::new()
            .name("persona-scheduler".to_string())
            .spawn(move || driver::run(&model, &config, &request_rx))
            .map_err(|e| format!("failed to spawn the persona scheduler thread: {e}"))?;
        Ok(Self { request_tx })
    }

    /// Enqueue a turn. Refused once the driver thread has exited.
    pub fn enqueue(&self, req: GenerationRequest) -> Result<(), String> {
        self.request_tx.send(req).map_err(|e| format!("scheduler closed: {e}"))
    }
}
