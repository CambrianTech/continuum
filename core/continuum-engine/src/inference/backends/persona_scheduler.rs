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
mod reuse;
mod geometry;
mod admission;
pub(crate) use admission::Capacity;
mod transition;
pub(crate) mod lesson;
pub(crate) use driver::{LearningJob, LearningLesson, LearningMeasuring};

pub use geometry::{EngineGeometry, EngineState, Incarnation, Requested};

/// A submitted generation request's identity (the adapter request's `request_id`), the one id that
/// crosses from her deliberation into the engine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestId(Arc<str>);

impl RequestId {
    #[must_use]
    pub fn new(id: &str) -> Self {
        Self(Arc::from(id))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}


use std::sync::Arc;

use llama::{KvCacheType, LoraAdapter, Model};
use uuid::Uuid;

/// Token event streamed from the scheduler to the requester.
#[derive(Debug)]
pub enum TokenEvent {
    /// One generated token piece (UTF-8 fragment from the tokenizer).
    Token(String),
    /// Generation finished cleanly (EOG / max_tokens / stop sequence), with what the Core measured of it.
    Done(TurnFacts),
    /// Refused at the door: her prompt and the reply she asked for do not fit one stream. Her caller
    /// refits to `capacity`; resending the same request cannot succeed.
    Exceeded(Exceeded),
    /// Generation failed: a refused move, a decode error.
    Error(String),
}

/// A turn too large for one stream, as its caller refits: her prompt's exact count and what one
/// stream holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exceeded {
    pub prompt_tokens: usize,
    pub capacity: usize,
}

/// What the Core measured of a turn it answered: her caller reports these, never estimates them
/// (2026-11-21: the response said `input_tokens: 0` and no timing, so cognition priced her prompt from
/// an estimate and saw no reuse while the Core had counted both).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TurnFacts {
    /// Her prompt's tokens, counted exactly.
    pub prompt_tokens: usize,
    /// Of those, how many her held state already covered (no prefill).
    pub reused_tokens: usize,
    /// Tokens she wrote.
    pub generated: usize,
    pub prefill: std::time::Duration,
    pub decode: std::time::Duration,
    /// From submission to her last token.
    pub elapsed: std::time::Duration,
    /// Why the Core ended it.
    pub end: TurnEnd,
}

/// Why the Core ended a turn it answered: the Core saw it happen, so nothing downstream infers it
/// from counts (2026-11-21: the provider compared `tokens >= max_tokens` to guess a cut).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnEnd {
    /// The model ended her generation.
    Finished,
    /// A stop sequence ended it, or her caller stopped reading.
    Stopped,
    /// She wrote the `max_tokens` she asked for.
    AtCap,
    /// The Core's context is full: everything she wrote is kept, and her next view collapses oldest
    /// first to fit.
    CapacityReached,
}

impl TurnEnd {
    /// The end as a call's fate reads it: a limit cut her, or she finished.
    pub fn generation_end(self) -> continuum_tool_protocol::GenerationEnd {
        match self {
            Self::Finished | Self::Stopped => continuum_tool_protocol::GenerationEnd::Finished,
            Self::AtCap | Self::CapacityReached => continuum_tool_protocol::GenerationEnd::OutputLimit,
        }
    }
}

impl TurnFacts {
    /// Her turn's token counts as a response reports them.
    pub fn usage(&self) -> crate::ai::types::UsageMetrics {
        crate::ai::types::UsageMetrics {
            input_tokens: wire_count(self.prompt_tokens),
            output_tokens: wire_count(self.generated),
            total_tokens: wire_count(self.prompt_tokens.saturating_add(self.generated)),
            estimated_cost: None,
        }
    }

    /// Her turn's phases as a response reports them: what was reused, what was prefilled and how
    /// fast, what was decoded and how fast.
    pub fn timing(&self) -> crate::ai::types::GenerationTiming {
        let prefilled = self.prompt_tokens.saturating_sub(self.reused_tokens);
        crate::ai::types::GenerationTiming {
            cached_tokens: wire_count(self.reused_tokens),
            prefill_tokens: wire_count(prefilled),
            prefill_ms: self.prefill.as_secs_f64() * 1000.0,
            prefill_tokens_per_second: rate(prefilled, self.prefill),
            decode_tokens: wire_count(self.generated),
            decode_ms: self.decode.as_secs_f64() * 1000.0,
            decode_tokens_per_second: rate(self.generated, self.decode),
        }
    }
}

/// A token count as the response's `u32` fields carry it; a count past `u32` is past every context,
/// so the clamp never changes a real count.
fn wire_count(tokens: usize) -> u32 {
    tokens.min(u32::MAX as usize) as u32
}

/// Tokens per second over a phase; a phase that processed no tokens took no rate.
fn rate(tokens: usize, took: std::time::Duration) -> f64 {
    if tokens == 0 || took.is_zero() {
        return 0.0;
    }
    tokens as f64 / took.as_secs_f64()
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
    /// Her turn's sampler, built on her own thread by the render that produced her prompt
    /// (`LlamaCppBackend::turn_sampler`): the engine thread only samples with it.
    pub sampler: llama::TurnSampler,
    pub stop_sequences: Vec<String>,
    /// Genes for this turn: `(id, handle, scale)`, resolved by the backend before enqueue. Empty is
    /// the base model.
    pub active_loras: Vec<(String, Arc<LoraAdapter>, f32)>,
    /// Tokens stream back through this.
    pub response_tx: tokio::sync::mpsc::UnboundedSender<TokenEvent>,
    /// How much her mind needs this turn: the SAGE slice weighs her share by it (`driver::lane`).
    pub urgency: continuum_sage::component::Urgency,
    /// The persona whose mind this turn continues. `None` is a one-off (a probe, a test rig): it
    /// runs as a mind of its own and its blocks are released when the turn ends.
    pub persona_id: Option<Uuid>,
    /// The submitted request's identity, carried through every transition this turn reports, so a
    /// stall says how far it got. `None` when the caller sent none.
    pub request: Option<RequestId>,
    /// When the turn was handed to the scheduler, so the driver's admission reports how long it waited.
    pub submitted: std::time::Instant,
}

/// The context the scheduler creates: the Core's whole context, one stream ([`continuum_persona_llama::STREAMS`]),
/// whole to the mind swapped in; no sequence count is configurable, and no
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
    /// What the trainer told (IN-PROCESS-LEARNING.md, L2): worked between her turns.
    Learning(driver::LearningJob),
}

/// What a stop can say about the records of the turns it ended. The record stage confirms it
/// processed them, in order, into the log; whether each line reached disk is the log sink's, so a
/// settle reports the sink's failed writes in its window instead of claiming them written (Codex on
/// #6208 at 162fbe5fa7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Records {
    /// Every record sent before the stop was processed into the log; `write_failures` counts the
    /// probe-log writes that failed since the previous settle (0: none reported).
    Processed { write_failures: u64 },
    /// The record stage ended before confirming them: they may be lost.
    Unconfirmed,
}

impl Default for Records {
    /// A stop that ended no turns: nothing to lose.
    fn default() -> Self {
        Self::Processed { write_failures: 0 }
    }
}

/// What a suspend preserved (CONTINUUM-ENGINE 4.16): how many minds it published, and every mind it
/// could not preserve, with why. A mind it could not preserve is held only by the live process.
#[derive(Debug, Clone, Default)]
pub struct Suspended {
    /// Whether the records of the turns this stop ended were confirmed written: telemetry, so it
    /// never changes custody, and a stop that preserved every mind says so even when it is not.
    pub records: Records,
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

/// What a stop's report adds about its turns' records: nothing when processed with no failed write.
fn records_note(records: Records) -> String {
    match records {
        Records::Processed { write_failures: 0 } => String::new(),
        Records::Processed { write_failures } => format!("; {write_failures} probe-log writes failed since the last stop, so some turn records may not be on disk"),
        Records::Unconfirmed => "; the records of the turns it ended were not confirmed processed".into(),
    }
}


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
                // Records are telemetry: unconfirmed ones never change custody, and the report says so.
                Ok(Ok(suspended)) if suspended.retained.is_empty() => {
                    let endpoint = format!("persona_engine: {} minds preserved{}", suspended.preserved, records_note(suspended.records));
                    vec![crate::runtime::HeldCustody { endpoint, process: None, state: crate::runtime::CustodyState::Saved }]
                }
                Ok(Ok(suspended)) => held(&format!("{} minds not preserved: {:?}{}", suspended.retained.len(), suspended.retained, records_note(suspended.records))),
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
        let capacity = continuum_sage::Capacity::granted(window);
        let (emit, emit_worker) = continuum_sage::spawn_stage(emit::Emit::new(Arc::clone(&model)), capacity);
        let (snapshot_tx, snapshot) = tokio::sync::watch::channel(record::Record::empty(incarnation.get()));
        let (record, record_worker) = continuum_sage::spawn_stage(record::Record::new(incarnation.get(), snapshot_tx), capacity);
        let (request_tx, request_rx) = tokio::sync::mpsc::unbounded_channel();
        let (state_tx, state) = tokio::sync::watch::channel(EngineState::Starting(incarnation));
        let runtime = tokio::runtime::Handle::current();
        std::thread::Builder::new()
            .name("persona-scheduler".to_string())
            .spawn(move || {
                driver::run(&model, &config, driver::Start { incarnation, state: state_tx, requests: request_rx, emit, record, stages_on: runtime.clone() });
                // No stop: the stages end once their last producer is gone and they have drained, so
                // answer ends a closing driver handed to its drain owner still reach their callers.
                let (emit, record) = runtime.block_on(driver::finish(emit_worker, record_worker));
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
    /// Queue what the trainer told beside her turns. An engine whose thread is gone says the job
    /// refused on learning/said: the trainer hears every outcome there.
    pub(crate) fn learn(&self, job: LearningJob) {
        if let Err(tokio::sync::mpsc::error::SendError(Submission::Learning(job))) = self.request_tx.send(Submission::Learning(job)) {
            job.refuse("the engine stopped");
        }
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    // what this catches: a call the output limit or the full context cut read as finished (a
    // truncated call run as if complete), or a finished one read as cut (a retry that re-cuts it).
    // The Core says why it ended; the call's fate follows from that, never from counting tokens.
    #[test]
    fn a_turn_the_core_cut_is_cut_and_one_that_finished_is_finished() {
        use continuum_tool_protocol::GenerationEnd;
        assert_eq!(TurnEnd::AtCap.generation_end(), GenerationEnd::OutputLimit);
        assert_eq!(TurnEnd::CapacityReached.generation_end(), GenerationEnd::OutputLimit);
        assert_eq!(TurnEnd::Finished.generation_end(), GenerationEnd::Finished);
        assert_eq!(TurnEnd::Stopped.generation_end(), GenerationEnd::Finished);
    }

    // what this catches: the in-process response saying `input_tokens: 0` and no timing while the
    // Core had counted her prompt and its reuse exactly (2026-11-21), so cognition priced her prompt
    // from an estimate and saw no reuse. The response carries what the Core measured.
    #[test]
    fn a_turns_response_carries_what_the_core_measured() {
        let facts = TurnFacts {
            prompt_tokens: 24_000,
            reused_tokens: 22_000,
            generated: 300,
            prefill: Duration::from_secs(10),
            decode: Duration::from_secs(15),
            elapsed: Duration::from_secs(26),
            end: TurnEnd::Finished,
        };
        let usage = facts.usage();
        assert_eq!((usage.input_tokens, usage.output_tokens, usage.total_tokens), (24_000, 300, 24_300));
        let timing = facts.timing();
        assert_eq!((timing.cached_tokens, timing.prefill_tokens, timing.decode_tokens), (22_000, 2_000, 300));
        assert_eq!((timing.prefill_tokens_per_second, timing.decode_tokens_per_second), (200.0, 20.0));
        let all_reused = TurnFacts { reused_tokens: 24_000, prefill: Duration::ZERO, ..facts };
        assert_eq!((all_reused.timing().prefill_tokens, all_reused.timing().prefill_tokens_per_second), (0, 0.0), "nothing prefilled, no rate");
    }
}
