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
mod emit;
mod preserve;
mod record;
mod frame;
mod reuse;
mod geometry;
mod sampling;

pub use geometry::{EngineGeometry, EngineState, Incarnation, Requested};

use std::sync::Arc;

use llama::{KvCacheType, LoraAdapter, Model};
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

/// A turn's prompt as token ids and where her view ends in them, computed by the submitter on its
/// own thread (each act runs on its own), so many acts tokenize at once and the engine thread never
/// does (CONTINUUM-ENGINE 4.16). The view's end is where the tokens of her view's text stop
/// agreeing with the prompt's (a token can straddle the edge); an end that leaves nothing after it
/// marks nothing.
pub fn prepare(model: &Model, prompt: &str, view: Option<&str>) -> Result<(Vec<i32>, Option<usize>), String> {
    // special=true so chat-template markers tokenize as the model's own special tokens
    let tokens = model.tokenize(prompt, true, true)?;
    let view_end = view
        .and_then(|view| model.tokenize(view, true, true).ok())
        .map(|view| reuse::common_prefix(&view, &tokens))
        .filter(|end| *end > 0 && *end < tokens.len());
    Ok((tokens, view_end))
}

/// What a caller submits to the scheduler.
pub struct GenerationRequest {
    /// Her prompt as token ids, tokenized by the submitter on its own thread ([`prepare`]): the
    /// engine thread never tokenizes.
    pub prompt: Vec<i32>,
    /// Where her view ends in `prompt` (what her next act will hold unchanged), or `None` for a
    /// turn with no view. The engine checkpoints her there, so her next act prefills only what
    /// follows.
    pub view_end: Option<usize>,
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
/// windows (the active mind's and the next one's); no sequence count is configurable, and no
/// attention kernel either: the scheduler moves minds through per-sequence copy-out, which needs an
/// untransposed V, so its context is always flash attention ([`driver`] owns that, not its caller).
#[derive(Debug, Clone)]
pub struct SchedulerConfig {
    /// The model this engine serves, as its owner names it: preserved minds resume only on the
    /// model they ran on (CONTINUUM-ENGINE 4.16).
    pub model: String,
    pub n_ctx: u32,
    pub n_batch: u32,
    pub n_ubatch: u32,
    pub type_k: KvCacheType,
    pub type_v: KvCacheType,
}

/// What reaches the driver thread, in order: a turn, or the stop's suspend.
pub(super) enum Submission {
    Turn(GenerationRequest),
    Suspend(tokio::sync::oneshot::Sender<Suspended>),
}

/// What a suspend preserved (CONTINUUM-ENGINE 4.16): how many minds it published, and every mind it
/// could not preserve, with why. A mind it could not preserve is held only by the live process.
#[derive(Debug, Clone, Default)]
pub struct Suspended {
    pub preserved: usize,
    pub retained: Vec<(Uuid, String)>,
}

/// Public handle. Cloneable; clones share the one driver thread and context.
#[derive(Clone)]
pub struct Scheduler {
    request_tx: tokio::sync::mpsc::UnboundedSender<Submission>,
    state: tokio::sync::watch::Receiver<EngineState>,
    snapshot: tokio::sync::watch::Receiver<continuum_persona_engine::protocol::EngineSnapshot>,
}

/// The engine the stop suspends: the last one spawned in this process.
static SUSPENDS: std::sync::Mutex<Option<tokio::sync::mpsc::UnboundedSender<Submission>>> = std::sync::Mutex::new(None);

/// The engine's part in a stop (CONTINUUM-ENGINE 4.16, Codex on 2990b44831): its root suspend,
/// through the kernel's `SettleBeforeStop`, before native contexts are released. Each mind leaves the
/// stream through the pipeline and is written; the manifest is the commit point. A mind not preserved
/// is reported held, so the stop knows ending the process loses her.
struct EngineSettle;

impl crate::runtime::extensions::SettleBeforeStop for EngineSettle {
    fn name(&self) -> &'static str {
        "persona_engine"
    }

    fn settle(&self, cap: std::time::Duration) -> std::pin::Pin<Box<dyn std::future::Future<Output = Vec<crate::runtime::HeldCustody>> + Send + 'static>> {
        let sender = SUSPENDS.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone(); // a poisoned slot still holds its sender
        Box::pin(async move {
            let held = |why: &str| {
                vec![crate::runtime::HeldCustody { endpoint: format!("persona_engine: {why}"), process: None, state: crate::runtime::CustodyState::Retained }]
            };
            let Some(sender) = sender else { return Vec::new() }; // no engine ever ran here: nothing held
            let (reply, suspended) = tokio::sync::oneshot::channel();
            if sender.send(Submission::Suspend(reply)).is_err() {
                return Vec::new(); // the engine already ended, and with it every mind it held
            }
            match tokio::time::timeout(cap, suspended).await {
                Ok(Ok(suspended)) if suspended.retained.is_empty() => {
                    vec![crate::runtime::HeldCustody { endpoint: format!("persona_engine: {} minds preserved", suspended.preserved), process: None, state: crate::runtime::CustodyState::Saved }]
                }
                Ok(Ok(suspended)) => held(&format!("{} minds not preserved: {:?}", suspended.retained.len(), suspended.retained)),
                Ok(Err(_)) => held("the engine ended before it reported its suspend"),
                Err(_) => held("the suspend did not finish within the stop's cap"),
            }
        })
    }
}

impl Scheduler {
    /// Spawn the driver thread and its stages. It runs until the last handle is dropped and its
    /// last turn ended.
    pub fn spawn(model: Arc<Model>, config: SchedulerConfig) -> Result<Self, SchedulerError> {
        // Emit and Record run on the runtime's workers beside the driver thread: the driver hands
        // them what it decoded and what it did, and never waits on text, callers or measurement.
        if tokio::runtime::Handle::try_current().is_err() {
            return Err(SchedulerError::NoRuntime);
        }
        let incarnation = Incarnation::next();
        // Each stage's capacity is the engine's admitted window in items: one decoded token (Emit) or
        // one step (Record) per item, and a full stage holds the driver's push until it drains, so
        // nothing waiting outgrows what serving admitted (Codex, 2026-11-19: the owner supplies it).
        let window = usize::try_from(config.n_ctx).ok().and_then(std::num::NonZeroUsize::new).ok_or(SchedulerError::NoWindow)?;
        let capacity = continuum_flow::Capacity::granted(window);
        let (emit, emit_worker) = continuum_flow::spawn_stage(emit::Emit::new(Arc::clone(&model)), capacity);
        let (snapshot_tx, snapshot) = tokio::sync::watch::channel(record::Record::empty(incarnation.get()));
        let (record, record_worker) = continuum_flow::spawn_stage(record::Record::new(incarnation.get(), snapshot_tx), capacity);
        let (request_tx, request_rx) = tokio::sync::mpsc::unbounded_channel();
        let (state_tx, state) = tokio::sync::watch::channel(EngineState::Starting(incarnation));
        let runtime = tokio::runtime::Handle::current();
        std::thread::Builder::new()
            .name("persona-scheduler".to_string())
            .spawn(move || {
                driver::run(&model, &config, driver::Start { incarnation, state: state_tx, requests: request_rx, emit, record, stages_on: runtime.clone() });
                emit_worker.stop();
                record_worker.stop();
                let (emit, record) = runtime.block_on(async { tokio::join!(emit_worker.join(), record_worker.join()) });
                for failure in [emit, record].into_iter().filter_map(Result::err) {
                    tracing::error!(%failure, "persona stage failed at shutdown");
                }
            })
            .map_err(|spawn| SchedulerError::Spawn(Arc::new(spawn)))?;
        *SUSPENDS.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(request_tx.clone()); // a poisoned slot is replaced whole
        crate::runtime::extensions::install_settle_before_stop(Arc::new(EngineSettle));
        Ok(Self { request_tx, state, snapshot })
    }

    /// The engine's published snapshot (CONTINUUM-ENGINE 4.16): its busy share and longest idle gap
    /// per window. Observational: grants commit through their owners.
    pub fn snapshot(&self) -> tokio::sync::watch::Receiver<continuum_persona_engine::protocol::EngineSnapshot> {
        self.snapshot.clone()
    }

    /// The engine as it is: starting, the geometry its context applied, or why it was refused. The
    /// one place serving reads the window, the batch and the KV layout from.
    pub fn state(&self) -> tokio::sync::watch::Receiver<EngineState> {
        self.state.clone()
    }

    /// Enqueue a turn. Refused once the driver thread has exited.
    pub fn enqueue(&self, req: GenerationRequest) -> Result<(), SchedulerError> {
        self.request_tx.send(Submission::Turn(req)).map_err(|_| SchedulerError::Closed)
    }
}

/// Why the scheduler could not take a turn.
#[derive(Debug, Clone, thiserror::Error)]
pub enum SchedulerError {
    #[error("the persona scheduler thread could not be spawned: {0}")]
    Spawn(Arc<std::io::Error>),
    #[error("the persona scheduler has stopped")]
    Closed,
    #[error("the persona scheduler was started outside a tokio runtime, so its Emit stage has nowhere to run")]
    NoRuntime,
    #[error("the persona scheduler was given no window, so its stages have no capacity")]
    NoWindow,
}
