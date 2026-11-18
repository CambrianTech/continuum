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
mod geometry;
mod sampling;

pub use geometry::{EngineGeometry, EngineState, Incarnation};

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
    state: tokio::sync::watch::Receiver<EngineState>,
}

impl Scheduler {
    /// Spawn the driver thread. It runs until the last handle is dropped and its last turn ended.
    pub fn spawn(model: Arc<Model>, config: SchedulerConfig) -> Result<Self, SchedulerError> {
        let (request_tx, request_rx) = std::sync::mpsc::channel();
        let incarnation = Incarnation::next();
        let (state_tx, state) = tokio::sync::watch::channel(EngineState::Starting(incarnation));
        std::thread::Builder::new()
            .name("persona-scheduler".to_string())
            .spawn(move || driver::run(&model, &config, driver::Start { incarnation, state: state_tx, requests: request_rx }))
            .map_err(|spawn| SchedulerError::Spawn(Arc::new(spawn)))?;
        Ok(Self { request_tx, state })
    }

    /// The engine as it is: starting, the geometry its context applied, or why it was refused. The
    /// one place serving reads the window, the batch and the KV layout from.
    pub fn state(&self) -> tokio::sync::watch::Receiver<EngineState> {
        self.state.clone()
    }

    /// Enqueue a turn. Refused once the driver thread has exited.
    pub fn enqueue(&self, req: GenerationRequest) -> Result<(), SchedulerError> {
        self.request_tx.send(req).map_err(|_| SchedulerError::Closed)
    }
}

/// Why the scheduler could not take a turn.
#[derive(Debug, Clone, thiserror::Error)]
pub enum SchedulerError {
    #[error("the persona scheduler thread could not be spawned: {0}")]
    Spawn(Arc<std::io::Error>),
    #[error("the persona scheduler has stopped")]
    Closed,
}
