//! The driver thread: a frame loop over one context (CONTINUUM-ENGINE 4.15). Each frame advances one
//! turn by one step, so no mind waits behind another's whole prompt; one mind decodes at a time.

use std::collections::{HashMap, VecDeque};
use std::num::NonZeroUsize;
use std::sync::mpsc::Receiver;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use continuum_persona_engine::engine::{DecodeError, HandoffError, PersonaEngine, RetireError, StageError};
use continuum_persona_engine::persona::{Epoch, Footprint, Held, PersonaId};
use continuum_persona_engine::pipeline::Identified;
use continuum_persona_engine::residency::TierRefusal;
use continuum_persona_llama::{Genes, Step, UnifiedTier, STREAMS};
use llama::{ContextParams, FlashAttn, LoraAdapter, Model};
use uuid::Uuid;

use super::geometry::StopsOnExit;
use continuum_flow::{PushError, StageInput};

use super::{emit, frame, reuse, sampling, EngineGeometry, EngineState, GenerationRequest, Incarnation, SchedulerConfig, TokenEvent};
use crate::inference::footprint_registry::{self, FootprintKey, ResourceType};
use crate::inference::kv_quant::Residency;
use crate::runtime;

/// Why a turn did not complete. The mind stays in the engine's custody either way.
#[derive(Debug, thiserror::Error)]
pub(super) enum TurnError {
    #[error("her prompt did not tokenize: {0}")]
    Tokenize(String),
    /// Her turn's sampler could not be built as asked (a think bound llama.cpp refused).
    #[error("her sampler was refused: {0}")]
    Sampler(String),
    #[error(transparent)]
    Stage(#[from] StageError),
    #[error(transparent)]
    Handoff(#[from] HandoffError),
    #[error(transparent)]
    Decode(#[from] DecodeError),
    #[error(transparent)]
    Tier(#[from] TierRefusal),
    #[error(transparent)]
    Retire(#[from] RetireError),
    #[error("no mind is active after the switch")]
    NoneActive,
    #[error("her history is longer than a context position holds")]
    TooLong,
    #[error("nothing was decoded for her turn")]
    NothingDecoded,
}

/// The context could not be made, so no turn can run.
#[derive(Debug, thiserror::Error)]
enum StartError {
    #[error("the persona scheduler's context was refused: {0}")]
    Context(String),
    #[error("the persona scheduler's batch size is zero")]
    ZeroBatch,
}

/// What her blocks were computed under beside the token ids. The model is this context's own, and no
/// native media reaches this path (the prompt is text), so her gene set (ids and scales) is the rest.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
struct Basis {
    genes: String,
}

/// One mind this scheduler serves: her id and epoch, the tokens her blocks hold, and their basis.
struct Mind {
    id: PersonaId,
    epoch: Epoch,
    held: Vec<i32>,
    basis: Basis,
    /// A one-off (no persona): her blocks are released when her turn ends.
    one_off: bool,
    /// Her checkpoint at her last view's end, while what she holds still runs through it.
    kept: Option<KeptCheckpoint>,
}

/// Her checkpoint and what makes it valid: she held exactly `held[..end]` under `basis` when it was
/// taken, and nothing she holds below `end` has changed since (a cut below it drops it).
struct KeptCheckpoint {
    blocks: llama::PersonaCheckpoint,
    end: usize,
    basis: Basis,
}

/// Her checkpoint holds her recurrent rows in host memory.
impl Footprint for KeptCheckpoint {
    fn held(&self) -> Held {
        Held::host(self.blocks.bytes() as u64)
    }
}

/// What a mind holds herself: her token history and her checkpoint. Her blocks in the context are
/// the tier's custody; [`Bound`] is that child's view.
impl Footprint for Mind {
    fn held(&self) -> Held {
        self.held.held() + self.kept.held()
    }
}

/// A bound mind's blocks, as the tier that holds them in the context reports them.
struct Bound<'t, 'c, 'm> {
    tier: &'t UnifiedTier<'c, 'm>,
    id: PersonaId,
}

impl Footprint for Bound<'_, '_, '_> {
    fn held(&self) -> Held {
        Held::device(self.tier.state_bytes_of(self.id).unwrap_or(0)) // unwrap_or: an unbound mind holds nothing in the context
    }
}

impl Mind {
    /// Keep only her first `len` tokens; a cut below her checkpoint's end drops the checkpoint.
    fn cut(&mut self, len: usize) {
        self.held.truncate(len);
        if self.kept.as_ref().is_some_and(|kept| kept.end > len) {
            self.kept = None;
        }
    }

    /// Where `prompt` under `basis` would start for her (`reuse::choose`), deciding nothing and
    /// moving nothing: the bind uses it to resume her, the frame to size her turn's prefill.
    fn reuse(&self, prompt: &[i32], basis: &Basis) -> reuse::Reuse {
        let kept = self.kept.as_ref().map(|kept| reuse::Kept { end: kept.end, basis: &kept.basis });
        reuse::choose(&self.held, &self.basis, kept, prompt, basis)
    }
}

impl Identified for Mind {
    fn persona(&self) -> PersonaId {
        self.id
    }
    fn epoch(&self) -> Epoch {
        self.epoch
    }
}

/// The driver's state: the model, the one engine over the one context, and every mind not active.
struct Driver<'c, 'm> {
    model: &'m Model,
    batch: NonZeroUsize,
    engine: PersonaEngine<Mind, UnifiedTier<'c, 'm>>,
    parked: HashMap<PersonaId, Mind>,
    /// Where decoded tokens and answer ends go (4.16); the engine thread never does text work.
    emit: StageInput<emit::Out>,
    /// What Emit turned back while full, handed over in order before anything newer.
    backlog: VecDeque<emit::Out>,
    /// Answers opened by this engine, numbering each.
    answers: u64,
}

/// How long the loop with no turn waits for a request while Emit is full, before handing the
/// backlog over again.
const EMIT_RETRY: std::time::Duration = std::time::Duration::from_millis(5);

/// What the driver thread starts with: the incarnation it serves as, where it publishes what the engine
/// is, and its one queue.
pub(super) struct Start {
    pub(super) incarnation: Incarnation,
    pub(super) state: tokio::sync::watch::Sender<EngineState>,
    pub(super) requests: Receiver<GenerationRequest>,
    pub(super) emit: StageInput<emit::Out>,
}

/// Run until every handle is dropped.
pub(super) fn run(model: &Model, config: &SchedulerConfig, start: Start) {
    let log = runtime::logger("persona-scheduler");
    let Start { incarnation, state, requests, emit } = start;
    // However this thread ends (its last handle dropped, an early return, or a panic unwinding it),
    // a Starting or Ready it leaves becomes Stopped: serving never grants turns to a gone engine.
    let _stops = StopsOnExit { state: &state, incarnation };
    let refuse = |refused: &StartError| {
        log.error(&refused.to_string());
        state.send_replace(EngineState::Refused { incarnation, reason: refused.to_string().into() });
    };
    let mut ctx = match context(model, config) {
        Ok(ctx) => ctx,
        Err(refused) => {
            refuse(&refused);
            for req in &requests {
                let _ = req.response_tx.send(TokenEvent::Error(refused.to_string()));
            }
            return;
        }
    };
    let Some(batch) = usize::try_from(ctx.n_batch()).ok().and_then(NonZeroUsize::new) else {
        refuse(&StartError::ZeroBatch);
        for req in &requests {
            let _ = req.response_tx.send(TokenEvent::Error(StartError::ZeroBatch.to_string()));
        }
        return;
    };
    let geometry = EngineGeometry::applied(incarnation, &ctx, config);
    log.info(&format!("persona scheduler ready: {geometry:?} (one mind decodes at a time)"));
    state.send_replace(EngineState::Ready(geometry));
    let mut driver = Driver {
        model,
        batch,
        engine: PersonaEngine::new(UnifiedTier::new(&mut ctx)),
        parked: HashMap::new(),
        emit,
        backlog: VecDeque::new(),
        answers: 0,
    };
    // The frame loop (CONTINUUM-ENGINE 4.15): submissions are drained without blocking, and each
    // frame advances ONE turn by ONE step (a prefill chunk or a token), chosen by `frame::pick`. What
    // it decodes goes to Emit (4.16) and the loop goes on. It blocks only when no turn is left, on
    // the queue itself: a wake, never a poll.
    let mut turns: Vec<Turn> = Vec::new();
    loop {
        driver.flush();
        if turns.is_empty() {
            // Emit full with no turn left: the only wait with a bound, until Emit drains (its depth
            // is thousands of tokens; this does not happen at the M5's decode rate).
            let next = if driver.backlog.is_empty() {
                requests.recv().map(Some).map_err(|_| ())
            } else {
                match requests.recv_timeout(EMIT_RETRY) {
                    Ok(req) => Ok(Some(req)),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => Ok(None),
                    Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => Err(()),
                }
            };
            match next {
                Ok(Some(req)) => driver.admit(req, &mut turns),
                Ok(None) => continue,
                // Every handle dropped: Emit ends once it has drained what it holds.
                Err(()) => return,
            }
        }
        while let Ok(req) = requests.try_recv() {
            driver.admit(req, &mut turns);
        }
        let Some(index) = driver.pick(&turns) else { continue };
        match driver.step(&mut turns[index]) {
            Ok(Advanced::Continues) => {}
            Ok(Advanced::Done(done)) => {
                let turn = turns.remove(index);
                driver.emit(emit::Out::End { answer: turn.answer, event: done });
            }
            Err(refused) => {
                log.warn(&format!("turn refused: {refused}"));
                let turn = turns.remove(index);
                driver.emit(emit::Out::End { answer: turn.answer, event: TokenEvent::Error(refused.to_string()) });
            }
        }
    }
}

/// A turn in flight: what she asked, its token ids, and how far it has come. The frame loop
/// advances it a step at a time; between steps another mind may be bound, and her blocks hold
/// exactly what her turn has computed so far.
struct Turn {
    req: GenerationRequest,
    id: PersonaId,
    one_off: bool,
    prompt: Vec<i32>,
    basis: Basis,
    genes: Genes,
    sampler: llama::Sampler,
    phase: Phase,
    submitted: Instant,
    spent: Spent,
    /// Where her view ends in `prompt` (CONTINUUM-ENGINE 4.15): a prefill chunk ends exactly there
    /// and she is checkpointed, so her next act resumes from it. `None` for a turn with no view.
    boundary: Option<usize>,
    /// Her answer, as Emit numbers it.
    answer: emit::AnswerSeq,
    /// Raised by Emit when her answer has ended on a stop sequence or her caller hung up; read at
    /// each step without a lock.
    cancel: Arc<AtomicBool>,
}

/// Where a turn stands.
enum Phase {
    /// Not bound yet: what her prefill starts from is decided against her blocks when she is.
    Waiting,
    /// Her prompt from `pos` is still to prefill.
    Prefilling { pos: usize },
    /// `token` is sampled and not yet emitted; it is decoded at `pos` if she goes on.
    Decoding { token: i32, pos: i32, generated: usize },
}

/// Where a turn's time went, for its one probe row.
#[derive(Default)]
struct Spent {
    reused: usize,
    /// How her prefill started: append, rewind, or the restart's reason.
    start: &'static str,
    checkpoint_ms: u64,
    first_step: Option<std::time::Duration>,
    switch: std::time::Duration,
    prefill: std::time::Duration,
    decode: std::time::Duration,
    /// Of `decode`: the forward pass to its landed result (decode and sync), and the sampler. What
    /// is left of `decode` is the frame's own work around them (CONTINUUM-ENGINE 4.16: the M5 at
    /// 7f67dc28d0 decoded ~10 ms/token slower than llama-bench on the same GGUF and KV type).
    forward: std::time::Duration,
    /// Of `decode`: waiting for the forward pass to land on the GPU.
    gpu: std::time::Duration,
    sample: std::time::Duration,
    /// Of her decoded tokens, how many reused a built graph instead of rebuilding it on the CPU.
    graphs_reused: u64,
    switches: u64,
}

/// What a step did to its turn.
enum Advanced {
    Continues,
    Done(TokenEvent),
}

/// The one context: `STREAMS` sequences (the active mind's and the next one's).
fn context<'m>(model: &'m Model, config: &SchedulerConfig) -> Result<llama::Context<'m>, StartError> {
    let streams = u32::try_from(STREAMS).map_err(|_| StartError::Context("stream count exceeds a sequence count".into()))?;
    model
        .new_context(ContextParams {
            n_ctx: config.n_ctx,
            n_batch: config.n_batch,
            n_ubatch: config.n_ubatch,
            n_seq_max: streams,
            // Required by the copy-out every switch runs: the fork refuses to move a mind out of a
            // transposed V cache (live on the M5: "backend status 3: transposed_v", 191 refused
            // deliberations, build 4aefe5131a, 2026-11-18 22:26Z). Flash attention keeps V
            // untransposed; a backend without it refuses this context by name (`Refused`).
            flash_attn: FlashAttn::Enabled,
            // The fused Gated Delta Net graphs, the fork's defaults (llama-context.cpp:3601-3602)
            // and what llama-bench measures: Qwen3.6 is mostly GDN layers. The launch had turned
            // them off ("their fused Metal kernels threw across FFI", no receipt); with them off the
            // M5 decoded ~31-38 ms/token (8 ms of it CPU in the forward call) where llama-bench
            // with them on decodes the same GGUF and Q4_0 KV at ~20.5 ms at 15k (a990358854 vs
            // fork a58c3b4cf, 2026-11-19 01:44-01:51Z). The fork probes support and falls back
            // where a backend lacks the op (llama-context.cpp:565-566).
            fused_gdn_ar: true,
            fused_gdn_ch: true,
            type_k: config.type_k,
            type_v: config.type_v,
            embeddings: false,
            pooling_type: llama::PoolingType::None,
            expert_observer: None,
        })
        .map_err(StartError::Context)
}

impl Driver<'_, '_> {
    /// Take a submission into the frame loop: its token ids and its sampler, built once. A prompt
    /// or sampler that cannot be built is refused at once, before it waits for a frame.
    /// Hand `out` to Emit without waiting. Emit full: it joins the backlog, which goes over in
    /// order, before anything newer, at the next frame.
    fn emit(&mut self, out: emit::Out) {
        if !self.backlog.is_empty() {
            self.backlog.push_back(out);
            return;
        }
        match self.emit.push(out) {
            Ok(_) => {}
            Err(PushError::Full(out)) => self.backlog.push_back(out),
            Err(PushError::Stopped(_)) => {} // Emit ended: its callers are gone with it
        }
    }

    /// Hand the backlog to Emit, in order, until it is empty or Emit is full again.
    fn flush(&mut self) {
        while let Some(out) = self.backlog.pop_front() {
            match self.emit.push(out) {
                Ok(_) => {}
                Err(PushError::Full(out)) => {
                    self.backlog.push_front(out);
                    return;
                }
                Err(PushError::Stopped(_)) => {
                    self.backlog.clear();
                    return;
                }
            }
        }
    }

    fn admit(&mut self, req: GenerationRequest, turns: &mut Vec<Turn>) {
        let (id, one_off) = match req.persona_id {
            Some(persona) => (PersonaId(persona), false),
            None => (PersonaId(Uuid::new_v4()), true),
        };
        // special=true so chat-template markers tokenize as the model's own special tokens
        let built = self
            .model
            .tokenize(&req.prompt, true, true)
            .map_err(TurnError::Tokenize)
            .and_then(|prompt| sampling::sampler(self.model, &req.sampling).map(|sampler| (prompt, sampler)).map_err(TurnError::Sampler));
        match built {
            Ok((prompt, sampler)) => {
                let basis = Basis { genes: signature(&req.active_loras) };
                let genes: Genes = req.active_loras.iter().map(|(_, adapter, scale)| (Arc::clone(adapter), *scale)).collect::<Vec<_>>().into();
                // Her view's end, in this prompt's own tokens: where the tokens of her view's text
                // stop agreeing with the prompt's (a token can straddle the edge). A boundary that
                // leaves nothing to prefill after it marks nothing.
                let boundary = req
                    .stable_prefix
                    .as_deref()
                    .and_then(|stable| self.model.tokenize(stable, true, true).ok())
                    .map(|stable| reuse::common_prefix(&stable, &prompt))
                    .filter(|end| *end > 0 && *end < prompt.len());
                self.answers += 1;
                let answer = emit::AnswerSeq(self.answers);
                let cancel = Arc::new(AtomicBool::new(false));
                self.emit(emit::Out::Open { answer, sink: req.response_tx.clone(), stops: req.stop_sequences.clone().into(), cancel: Arc::clone(&cancel) });
                turns.push(Turn { req, id, one_off, prompt, basis, genes, sampler, phase: Phase::Waiting, submitted: Instant::now(), spent: Spent::default(), boundary, answer, cancel });
            }
            Err(refused) => {
                let _ = req.response_tx.send(TokenEvent::Error(refused.to_string()));
            }
        }
    }

    /// The turn the next frame advances: a mind is one serial queue, so only her oldest turn is a
    /// candidate; among candidates, `frame::pick` decides. A waiting turn's prefill is what she
    /// cannot reuse, never her whole prompt: a 1.2k tail on a 29k prompt she holds is a 1.2k turn
    /// (the M5, 2026-11-19, a6da599330: sized as 29k, her rewind waited 161 s behind cold prompts).
    fn pick(&mut self, turns: &[Turn]) -> Option<usize> {
        let mut seen = std::collections::HashSet::new();
        let eligible: Vec<usize> = (0..turns.len()).filter(|&index| seen.insert(turns[index].id)).collect();
        let active = self.engine.active();
        let candidates: Vec<frame::Candidate> = eligible
            .iter()
            .map(|&index| {
                let turn = &turns[index];
                let (decoding, remaining) = match &turn.phase {
                    Phase::Waiting => {
                        let her = if active == Some(turn.id) { self.engine.active_mut().map(|her| &*her) } else { self.parked.get(&turn.id) };
                        let reused = her.map_or(0, |her| her.reuse(&turn.prompt, &turn.basis).reused()); // map_or: a mind never seen holds nothing
                        (false, turn.prompt.len().saturating_sub(reused))
                    }
                    Phase::Prefilling { pos } => (false, turn.prompt.len().saturating_sub(*pos)),
                    Phase::Decoding { .. } => (true, 0),
                };
                let waited_ms = u64::try_from(turn.submitted.elapsed().as_millis()).unwrap_or(u64::MAX); // unwrap_or: a wait past u64 ms is the oldest
                frame::Candidate { active: active == Some(turn.id), decoding, remaining, waited_ms }
            })
            .collect();
        frame::pick(&candidates).map(|chosen| eligible[chosen])
    }

    /// One frame for `turn`: bind her if another mind is bound, then one step (a prefill chunk, or
    /// one token).
    fn step(&mut self, turn: &mut Turn) -> Result<Advanced, TurnError> {
        if turn.spent.first_step.is_none() {
            turn.spent.first_step = Some(turn.submitted.elapsed());
        }
        if self.engine.active() != Some(turn.id) {
            let switching = Instant::now();
            self.switch_to(turn.id, turn.one_off)?;
            turn.spent.switch += switching.elapsed();
            turn.spent.switches += 1;
        }
        if matches!(turn.phase, Phase::Waiting) {
            let from = self.resume(turn)?;
            if from >= turn.prompt.len() {
                return Err(TurnError::NothingDecoded);
            }
            turn.phase = Phase::Prefilling { pos: from };
        }
        let stepping = Instant::now();
        let (advanced, decoding) = match std::mem::replace(&mut turn.phase, Phase::Waiting) {
            Phase::Waiting => (Err(TurnError::NothingDecoded), false),
            Phase::Prefilling { pos } => (self.prefill_chunk(turn, pos).map(|()| Advanced::Continues), false),
            Phase::Decoding { token, pos, generated } => (self.decode_token(turn, token, pos, generated), true),
        };
        let took = stepping.elapsed();
        if decoding {
            turn.spent.decode += took;
        } else {
            turn.spent.prefill += took;
        }
        if let Ok(Advanced::Done(TokenEvent::Done { tokens_generated, .. })) = &advanced {
            self.report_footprint(turn.id, turn.one_off);
            self.probe(turn, *tokens_generated);
        }
        advanced
    }

    /// Where her prefill starts, now that she is bound (`reuse::choose`): what she holds when the
    /// prompt extends it, her checkpoint when the prompt keeps her view and changes what follows, else
    /// from zero for the reason named in her probe row. A rewind the engine refuses restarts her.
    fn resume(&mut self, turn: &mut Turn) -> Result<usize, TurnError> {
        let basis = turn.basis.clone();
        let decided = self.engine.active_mut().ok_or(TurnError::NoneActive)?.reuse(&turn.prompt, &basis);
        let (from, start) = match decided {
            reuse::Reuse::Append { from } => (from, "append"),
            reuse::Reuse::Rewind { from } => match self.rewind_to_checkpoint() {
                Ok(()) => (from, "rewind"),
                Err(_) => (0, "rewind_refused"),
            },
            reuse::Reuse::Restart(miss) => (0, miss.reason()),
        };
        let her = self.engine.active_mut().ok_or(TurnError::NoneActive)?;
        her.cut(from);
        her.basis = basis;
        turn.spent.reused = from;
        turn.spent.start = start;
        Ok(from)
    }

    /// Rewind the active mind to her kept checkpoint (taken out and put back, so the engine can read
    /// it while it moves her).
    fn rewind_to_checkpoint(&mut self) -> Result<(), TurnError> {
        let kept = self.engine.active_mut().ok_or(TurnError::NoneActive)?.kept.take();
        let Some(kept) = kept else { return Err(TurnError::NoneActive) };
        let rewound = self.engine.rewind_active(&kept.blocks);
        self.record(|her| her.kept = Some(kept))?;
        Ok(rewound?)
    }

    /// Checkpoint the active mind at her view's end (`end` tokens held), reusing her checkpoint's
    /// buffers. A refused take keeps none, so a later act restarts instead of rewinding to a stale one.
    fn keep_checkpoint(&mut self, turn: &mut Turn, end: usize) -> Result<(), TurnError> {
        let taking = Instant::now();
        let previous = self.engine.active_mut().ok_or(TurnError::NoneActive)?.kept.take();
        let mut blocks = match previous.map(|kept| kept.blocks).or_else(llama::PersonaCheckpoint::new) {
            Some(blocks) => blocks,
            None => return Ok(()), // the host could not allocate a checkpoint: she restarts next act, named in its row
        };
        let taken = self.engine.checkpoint_active(&mut blocks);
        let basis = turn.basis.clone();
        self.record(|her| {
            her.kept = match taken {
                Ok(pos) if pos as usize + 1 == end => Some(KeptCheckpoint { blocks, end, basis }),
                _ => None,
            }
        })?;
        turn.spent.checkpoint_ms = u64::try_from(taking.elapsed().as_millis()).unwrap_or(u64::MAX); // unwrap_or: past u64 ms reads as the most
        Ok(())
    }

    /// One chunk of her prompt from `pos`. The last chunk asks for logits and samples her first
    /// token; only it is sampled, since the sampler accepts what it samples (Codex on #6140).
    fn prefill_chunk(&mut self, turn: &mut Turn, pos: usize) -> Result<(), TurnError> {
        // A chunk never runs past her view's end, so she can be checkpointed exactly there.
        let limit = turn.boundary.filter(|boundary| *boundary > pos).unwrap_or(turn.prompt.len()); // unwrap_or: no view end ahead, the prompt's end bounds the chunk
        let end = (pos + self.batch.get()).min(limit);
        let chunk = turn.prompt[pos..end].to_vec();
        let last = end == turn.prompt.len();
        let from = i32::try_from(pos).map_err(|_| TurnError::TooLong)?;
        let len = chunk.len();
        self.engine.submit(Step { tokens: chunk.clone(), from, logits_at_end: last, genes: Arc::clone(&turn.genes) })?;
        if !last {
            self.engine.settle()?;
            self.record(|her| her.held.extend_from_slice(&chunk))?;
            if turn.boundary == Some(end) {
                self.keep_checkpoint(turn, end)?;
            }
            turn.phase = Phase::Prefilling { pos: end };
            return Ok(());
        }
        let at = i32::try_from(len - 1).map_err(|_| TurnError::TooLong)?;
        let token = self.engine.tier().sample(&mut turn.sampler, at);
        self.record(|her| her.held.extend_from_slice(&chunk))?;
        self.report_footprint(turn.id, turn.one_off);
        let pos = i32::try_from(end).map_err(|_| TurnError::TooLong)?;
        turn.phase = Phase::Decoding { token, pos, generated: 0 };
        Ok(())
    }

    /// Emit her sampled token and, unless she stopped, decode it and sample the next.
    fn decode_token(&mut self, turn: &mut Turn, token: i32, pos: i32, generated: usize) -> Result<Advanced, TurnError> {
        let done = |turn: &Turn, generated: usize| {
            let took = turn.submitted.elapsed();
            Advanced::Done(TokenEvent::Done {
                tokens_generated: generated,
                elapsed_ms: took.as_secs().saturating_mul(1000).saturating_add(u64::from(took.subsec_millis())),
            })
        };
        // Emit raised her cancel flag: a stop sequence ended her answer, or her caller hung up. What
        // her blocks computed stays hers.
        if self.model.is_eog_token(token) || turn.cancel.load(Ordering::Acquire) {
            return Ok(done(turn, generated));
        }
        // The token goes to Emit (text, stop sequences, her caller) and the engine goes on.
        self.emit(emit::Out::Token { answer: turn.answer, token });
        let generated = generated + 1;
        if generated >= turn.req.max_tokens {
            return Ok(done(turn, generated));
        }
        let forward = Instant::now();
        let reused_before = self.engine.tier().graphs_reused();
        self.engine.submit(Step { tokens: vec![token], from: pos, logits_at_end: true, genes: Arc::clone(&turn.genes) })?;
        turn.spent.forward += forward.elapsed();
        turn.spent.graphs_reused += self.engine.tier().graphs_reused().saturating_sub(reused_before);
        self.record(|her| her.held.push(token))?;
        // Wait for the forward pass here, where reading her logits would wait anyway, so the GPU's
        // time and the sampler's are measured apart.
        let landing = Instant::now();
        self.engine.settle()?;
        turn.spent.gpu += landing.elapsed();
        let sampling = Instant::now();
        let next = self.engine.tier().sample(&mut turn.sampler, 0);
        turn.spent.sample += sampling.elapsed();
        turn.phase = Phase::Decoding { token: next, pos: pos + 1, generated };
        Ok(Advanced::Continues)
    }

    /// One row per turn: where her seconds went (waiting for her first frame, switches, prefill of
    /// what her blocks did not hold, decode), so the one-mind gate (switch in ms, decode near the
    /// chip's rate, reuse >= 80%, no turn waiting behind another's whole prompt) is read live.
    fn probe(&self, turn: &Turn, generated: usize) {
        let ms = |d: std::time::Duration| u64::try_from(d.as_millis()).unwrap_or(u64::MAX); // unwrap_or: past u64 ms reads as the most
        continuum_base::probe!(
            class = "persona.engine.turn",
            persona = %turn.id.0,
            one_off = turn.one_off,
            prompt_tokens = turn.prompt.len() as u64,
            reused_tokens = turn.spent.reused as u64,
            start = turn.spent.start,
            boundary = turn.boundary.map_or(0, |end| end as u64), // map_or: 0 = no view end in this prompt
            checkpoint_ms = turn.spent.checkpoint_ms,
            wait_ms = turn.spent.first_step.map_or(0, ms),
            switches = turn.spent.switches,
            switch_ms = ms(turn.spent.switch),
            prefill_ms = ms(turn.spent.prefill),
            decode_ms = ms(turn.spent.decode),
            forward_ms = ms(turn.spent.forward),
            graphs_reused = turn.spent.graphs_reused,
            gpu_ms = ms(turn.spent.gpu),
            sample_ms = ms(turn.spent.sample),
            generated = generated as u64,
            total_ms = ms(turn.submitted.elapsed()),
            "persona engine turn"
        );
    }

    /// Make `id` the active mind: her parked state comes back (or a new mind starts), the previous
    /// mind moves out to the RAM tier and is parked, or forgotten if she was a one-off.
    ///
    /// A retirement a refused copy-out left behind is finished first, so it is retried rather than
    /// skipped and the retiring mind is never staged while still in custody (Codex on #6140). A
    /// copy-out refused during this handoff leaves next bound; it is retried right after, and again
    /// at the start of the next turn if the tier still refuses.
    fn switch_to(&mut self, id: PersonaId, one_off: bool) -> Result<(), TurnError> {
        self.retire_previous()?;
        if self.engine.active() == Some(id) {
            return Ok(());
        }
        let mind = match self.parked.remove(&id) {
            Some(parked) => parked,
            None => Mind { id, epoch: Epoch(1), held: Vec::new(), basis: Basis::default(), one_off, kept: None },
        };
        if let Err(refused) = self.engine.stage(mind) {
            self.parked.insert(refused.returned.id, refused.returned);
            return Err(refused.reason.into());
        }
        self.engine.settle()?;
        match self.engine.handoff() {
            Ok(_) | Err(HandoffError::CopyOut { .. }) => {}
            Err(refused) => return Err(refused.into()),
        }
        self.retire_previous()
    }

    /// Park the mind whose retirement finishes (or forget her if she was a one-off).
    fn retire_previous(&mut self) -> Result<(), TurnError> {
        let Some(previous) = self.engine.finish_retirement()? else { return Ok(()) };
        if previous.one_off {
            self.engine.forget(previous.id)?;
        } else {
            self.parked.insert(previous.id, previous);
        }
        Ok(())
    }

    /// Update the active mind's own record (what her blocks now hold).
    fn record(&mut self, change: impl FnOnce(&mut Mind)) -> Result<(), TurnError> {
        change(self.engine.active_mut().ok_or(TurnError::NoneActive)?);
        Ok(())
    }

    /// Attribute what the active mind holds to her in the footprint registry: her own footprint
    /// (tokens, checkpoint) and her blocks in the context, each asked of its owner (one-offs are not
    /// attributed).
    fn report_footprint(&mut self, id: PersonaId, one_off: bool) {
        if one_off {
            return;
        }
        let blocks = Bound { tier: self.engine.tier(), id }.held();
        if blocks == Held::NOTHING {
            return;
        }
        let own = match self.engine.active_mut() {
            Some(her) if her.id == id => her.held(),
            _ => Held::NOTHING,
        };
        footprint_registry::global().report_authoritative(FootprintKey::for_persona(id.0, ResourceType::KvCache, Residency::Active), (blocks + own).total());
    }
}

/// A gene set's identity: sorted `id:scale`, comma-joined; empty is the base model.
fn signature(genes: &[(String, Arc<LoraAdapter>, f32)]) -> String {
    let mut parts: Vec<String> = genes.iter().map(|(id, _, scale)| format!("{id}:{scale}")).collect();
    parts.sort_unstable();
    parts.join(",")
}
