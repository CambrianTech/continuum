//! The driver thread: a frame loop over one context (CONTINUUM-ENGINE 4.15). Each frame advances one
//! turn by one step, so no mind waits behind another's whole prompt; one mind decodes at a time.

use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use tokio::sync::mpsc::UnboundedReceiver as Receiver;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use continuum_persona_engine::engine::{DecodeError, HandoffError, PersonaEngine, RetireError, StageError};
use continuum_persona_engine::persona::{Epoch, Footprint, Held, MindId, PersonaId, PersonaParts, PersonaUnit};
use continuum_persona_engine::residency::TierRefusal;
use continuum_persona_llama::{Blocks, Genes, Step, UnifiedTier, STREAMS};
use llama::{ContextParams, FlashAttn, LoraAdapter, Model};
use uuid::Uuid;

use super::geometry::StopsOnExit;
use continuum_flow::{Outbox, StageInput};

use super::{emit, frame, preserve, record, reuse, sampling, EngineGeometry, EngineState, GenerationRequest, Incarnation, SchedulerConfig, Submission, Suspended, TokenEvent};
use crate::inference::footprint_registry::{self, FootprintKey, ResourceType};
use crate::inference::kv_quant::Residency;
use crate::runtime;

/// Why a turn did not complete. The mind stays in the engine's custody either way.
#[derive(Debug, thiserror::Error)]
pub(super) enum TurnError {
    /// Her turn's sampler could not be built as asked (a think bound llama.cpp refused).
    #[error("her sampler was refused: {0}")]
    Sampler(#[from] llama::ChatRefusal),
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
    #[error("the caller released this turn")]
    CallerReleased,
}

/// Why one mind was not preserved at a stop. She stays in the engine's custody, and the stop
/// reports her as retained.
#[derive(Debug, thiserror::Error)]
enum PreserveError {
    #[error(transparent)]
    Turn(#[from] TurnError),
    #[error(transparent)]
    Retire(#[from] HandoffError),
    #[error("she did not come back from her retirement")]
    NotReturned,
    #[error("the fork refused to write her blocks: {0}")]
    Write(#[from] llama::PersonaRefusal),
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

/// One mind this scheduler serves: the unit that travels (CONTINUUM-ENGINE 4.17). Her blocks are her
/// own part, so wherever her unit is (active, parked, preserved) her blocks are with her.
type Mind = PersonaUnit<Blocks, History, Basis, (), Tenure>;

/// Her parts, as the active mind's record is changed in place.
type Parts = PersonaParts<Blocks, History, Basis, (), Tenure>;

/// How long she stays: a resident is parked between turns and preserved at a stop; a one-off (no
/// persona) is dropped, blocks and all, when her turn ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tenure {
    Resident,
    OneOff,
}

mod history;
use history::{Boundary, History, Boundaries};

/// A bound mind's blocks, as the tier that holds them in the context reports them.
struct Bound<'t, 'c, 'm> {
    tier: &'t UnifiedTier<'c, 'm>,
    mind: MindId,
}

impl Footprint for Bound<'_, '_, '_> {
    fn held(&self) -> Held {
        self.tier.state_bytes_of(self.mind).map_or_else(Held::default, Held::device) // an unbound mind holds nothing in the context
    }
}

/// Where `prompt` under `basis` would start for `her`.
fn reuse_for(her: &Mind, prompt: &[i32], basis: &Basis) -> reuse::Decision {
    her.parts().tokens.reuse(&her.parts().genome, prompt, basis)
}

fn resident(her: &Mind) -> bool {
    her.parts().decode == Tenure::Resident
}

/// The driver's state: the model, the one engine over the one context, and every mind not active.
struct Driver<'c, 'm> {
    model: &'m Model,
    batch: NonZeroUsize,
    engine: PersonaEngine<Mind, UnifiedTier<'c, 'm>>,
    parked: HashMap<PersonaId, Mind>,
    /// Admitted anonymous turns that still own their native blocks, across all transfer states.
    live_one_offs: HashSet<PersonaId>,
    /// Where decoded tokens and answer ends go (4.16); the engine thread never does text work.
    emit: Outbox<emit::Out>,
    /// Where each GPU step's span and each finished turn's row go; measured and written off this
    /// thread.
    record: Outbox<record::Fact>,
    /// Answers opened by this engine, numbering each.
    answers: u64,
    /// Where the stop preserves minds and the start resumes them (`None`: this node has no home to
    /// write them to, and its minds are dropped at a stop, said once at start).
    store: Option<preserve::Preserved>,
    /// The model this engine serves: preserved minds resume only on it.
    model_id: String,
    /// The stop has suspended every mind: no new turn is taken.
    stopping: bool,
    bound_since: Instant,
    prefill_sample: Option<(usize, std::time::Duration)>,
}

/// The root resume: every mind the last stop published, held Warm (in RAM, bound when scheduled),
/// each verified first (her model through the manifest, her token history's length and checksum
/// here, her owner, epoch and memory layout in the fork). A mind that does not verify is named and
/// left on disk; she starts cold.
fn resume(tier: &mut UnifiedTier<'_, '_>, preserved: &mut preserve::Preserved, model: &str, log: &runtime::ModuleLogger) -> HashMap<PersonaId, Mind> {
    // The saved-state contract's verdict: anything but a matched (or upgraded) value resumes nobody,
    // and the store probes which verdict it was (`state.load.<outcome>`). Every outcome is also
    // logged: at boot the probe sink may not be up yet (none of these probes reached the probe file
    // on the M5's first restore, 2026-11-19 04:03Z), and a restore that fails silently is the defect.
    let minds = match preserved.store.load(&continuum_saved_state::Singleton, &preserve::ServedModel { model: model.to_owned() }) {
        continuum_saved_state::Loaded::Matched(minds) | continuum_saved_state::Loaded::Upgraded { value: minds, .. } => minds,
        continuum_saved_state::Loaded::Absent => return HashMap::new(),
        other => {
            log.warn(&format!("no preserved mind resumes: the saved minds load as {}", other.describe()));
            return HashMap::new();
        }
    };
    // the last complete generation: what a mind whose next write fails keeps (preserve.rs)
    preserved.remember(&minds);
    let mut parked = HashMap::new();
    for mind in minds {
        let id = PersonaId(mind.persona);
        let epoch = Epoch(mind.epoch);
        match tier.read_file(MindId { persona: id, epoch }, &preserved.path(&mind.blocks)) {
            Ok(blocks) => {
                continuum_base::probe!(class = "persona.engine.resumed", persona = %mind.persona, tokens = mind.held.len() as u64, "a mind came back from her preserved blocks");
                let parts = Parts { blocks, tokens: History::from(mind.held), genome: Basis { genes: mind.basis }, learning: (), decode: Tenure::Resident };
                parked.insert(id, Mind::new(id, epoch, parts));
            }
            Err(refused) => {
                log.warn(&format!("preserved mind {} did not verify and starts cold: {refused}", mind.persona));
                continuum_base::probe!(class = "persona.engine.resume_refused", persona = %mind.persona, why = %refused, "a preserved mind did not verify; she starts cold");
            }
        }
    }
    parked
}

/// What the driver thread starts with: the incarnation it serves as, where it publishes what the engine
/// is, and its one queue.
pub(super) struct Start {
    pub(super) incarnation: Incarnation,
    pub(super) state: tokio::sync::watch::Sender<EngineState>,
    pub(super) requests: Receiver<Submission>,
    pub(super) emit: StageInput<emit::Out>,
    pub(super) record: StageInput<record::Fact>,
    /// The runtime its stages run on, which a driver with nothing to advance awaits their space on.
    pub(super) stages_on: tokio::runtime::Handle,
}

/// Run until every handle is dropped.
pub(super) fn run(model: &Model, config: &SchedulerConfig, start: Start) {
    let log = runtime::logger("persona-scheduler");
    let Start { incarnation, state, mut requests, emit, record, stages_on } = start;
    // However this thread ends (its last handle dropped, an early return, or a panic unwinding it),
    // a Starting or Ready it leaves becomes Stopped: serving never grants turns to a gone engine.
    let _stops = StopsOnExit { state: &state, incarnation };
    let refuse = |refused: &StartError| {
        log.error(&refused.to_string());
        state.send_replace(EngineState::Refused { incarnation, reason: refused.to_string().into() });
    };
    // A start refused: every submission is answered with why (a suspend finds nothing to preserve).
    let mut refuse_all = |why: String| {
        while let Some(submission) = requests.blocking_recv() {
            match submission {
                Submission::Turn(req) => {
                    tell(&req.response_tx, TokenEvent::Error(why.clone()));
                }
                Submission::Suspend(reply) => {
                    report(reply, Suspended::default());
                }
            }
        }
    };
    let mut ctx = match context(model, config) {
        Ok(ctx) => ctx,
        Err(refused) => {
            refuse(&refused);
            refuse_all(refused.to_string());
            return;
        }
    };
    let Some(batch) = usize::try_from(ctx.n_batch()).ok().and_then(NonZeroUsize::new) else {
        refuse(&StartError::ZeroBatch);
        refuse_all(StartError::ZeroBatch.to_string());
        return;
    };
    let geometry = EngineGeometry::applied(incarnation, &ctx, config);
    // The root resume (CONTINUUM-ENGINE 4.16): the minds the last stop preserved come back Warm, in
    // RAM, and bind when they are next scheduled. A mind that does not verify stays on disk, named.
    let mut tier = UnifiedTier::new(&mut ctx);
    let mut store = preserve::Preserved::at_home();
    let parked = match &mut store {
        Ok(store) => resume(&mut tier, store, &config.model, &log),
        Err(why) => {
            log.warn(&format!("no preserved minds resume: {why}"));
            HashMap::new()
        }
    };
    log.info(&format!("persona scheduler ready: {geometry:?} (one mind decodes at a time; {} minds resumed)", parked.len()));
    state.send_replace(EngineState::Ready(geometry));
    let mut driver = Driver {
        model,
        batch,
        engine: PersonaEngine::new(tier),
        parked,
        live_one_offs: HashSet::new(),
        emit: Outbox::new(emit),
        record: Outbox::new(record),
        answers: 0,
        store: store.ok(),
        model_id: config.model.clone(),
        stopping: false,
        bound_since: Instant::now(),
        prefill_sample: None,
    };
    // The frame loop (CONTINUUM-ENGINE 4.15): submissions are drained without blocking, and each
    // frame advances ONE turn by ONE step (a prefill chunk or a token), chosen by `frame::pick`. What
    // it decodes goes to Emit (4.16) and the loop goes on. It blocks only when no turn is left, on
    // the queue itself: a wake, never a poll.
    let mut turns: Vec<Turn> = Vec::new();
    loop {
        driver.flush();
        // A stage full defers the producer: no step runs while what it decoded waits, so the backlog
        // stays within the stage's grant plus one step (Codex on f530b00670). With nothing to step, or
        // while deferred, the thread waits for EITHER a submission (a turn, or the stop's suspend) or
        // the stage's space: a wake on both, never a poll, and a stalled caller never blocks a stop.
        let holding = driver.holding();
        if holding || turns.is_empty() {
            let parked_at = Instant::now();
            let emit_pending = driver.emit.waiting();
            let record_pending = driver.record.waiting();
            let woke = stages_on.block_on(async {
                tokio::select! {
                    biased;
                    submission = requests.recv() => Some(submission),
                    () = driver.drain(), if holding => None,
                }
            });
            let parked_for = parked_at.elapsed();
            if parked_for > std::time::Duration::from_millis(500) {
                let wake = match &woke {
                    Some(Some(Submission::Turn(_))) => "turn",
                    Some(Some(Submission::Suspend(_))) => "suspend",
                    Some(None) => "closed",
                    None => "stage_space",
                };
                continuum_base::probe!(
                    class = "persona.engine.park",
                    parked_ms = super::record::whole_ms(parked_for),
                    active_turns = turns.len() as u64,
                    emit_pending = emit_pending as u64,
                    record_pending = record_pending as u64,
                    wake = wake,
                    "engine thread resumed after parking"
                );
            }
            match woke {
                Some(Some(submission)) => driver.take(submission, &mut turns),
                // Every handle dropped: Emit ends once it has drained what it holds.
                Some(None) => return,
                None => {} // the stages took what waited
            }
            continue;
        }
        while let Ok(submission) = requests.try_recv() {
            driver.take(submission, &mut turns);
        }
        let Some((index, ready)) = driver.pick(&turns) else { continue };
        match driver.step(&mut turns[index], ready) {
            Ok(Advanced::Continues) => {}
            Ok(Advanced::Done(done)) => {
                let turn = turns.remove(index);
                driver.finish_one_off(&turn);
                driver.emit.send(emit::Out::End { answer: turn.answer, event: done });
            }
            Err(refused) => {
                log.warn(&format!("turn refused: {refused}"));
                let turn = turns.remove(index);
                driver.finish_one_off(&turn);
                driver.emit.send(emit::Out::End { answer: turn.answer, event: TokenEvent::Error(refused.to_string()) });
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
    ready_since: Instant,
    spent: Spent,
    /// Where her view ends in `prompt` (CONTINUUM-ENGINE 4.15): a prefill chunk ends exactly there
    /// and she is checkpointed, so her next act resumes from it. `None` for a turn with no view.
    boundaries: Boundaries,
    /// Her answer, as Emit numbers it.
    answer: emit::AnswerSeq,
    /// Raised by Emit when her answer has ended on a stop sequence or her caller hung up; read at
    /// each step without a lock.
    cancel: Arc<AtomicBool>,
}

/// Where a turn stands.
enum Phase {
    /// Not bound yet: what her prefill starts from is decided against her blocks when she is.
    /// Estimate once when first eligible, after any earlier turn for this mind. Native resume checks the actual blocks again.
    Waiting(std::cell::OnceCell<usize>),
    /// Her prompt from `pos` is still to prefill.
    Prefilling { pos: usize },
    /// `token` is sampled and not yet emitted; it is decoded at `pos` if she goes on.
    Decoding { token: i32, pos: i32, generated: usize },
}

/// Where a turn's time went, for its one probe row.
#[derive(Default)]
struct Spent {
    reused: usize,
    shared_prefix_tokens: usize,
    /// How her prefill started: append, rewind, or the restart's reason.
    start: &'static str,
    checkpoint_ms: u64,
    first_step: Option<std::time::Duration>,
    longest_gap: std::time::Duration,
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
    /// Hand each stage what waited while it was full, before anything newer.
    fn flush(&mut self) {
        self.emit.flush();
        self.record.flush();
    }

    /// Each outbox wakes on its own consumer's space. Both retain undelivered items
    /// when an incoming submission interrupts the drain.
    async fn drain(&mut self) {
        tokio::join!(self.emit.drain(), self.record.drain());
    }

    /// Whether anything still waits for a stage that was full.
    fn holding(&self) -> bool {
        self.emit.waiting() + self.record.waiting() > 0
    }

    /// Route what arrived: a turn into the frame loop (refused once the stop has suspended every
    /// mind), or the stop's suspend.
    fn take(&mut self, submission: Submission, turns: &mut Vec<Turn>) {
        match submission {
            Submission::Turn(req) if self.stopping => {
                tell(&req.response_tx, TokenEvent::Error("the engine is stopping: her minds are preserved, and her turn starts after it restarts".into()));
            }
            Submission::Turn(req) => self.admit(req, turns),
            Submission::Suspend(reply) => {
                let suspended = self.suspend(turns);
                report(reply, suspended);
            }
        }
    }

    /// The root suspend (CONTINUUM-ENGINE 4.16): no new turn is taken, turns in flight end (what her
    /// blocks computed stays hers), and every mind is preserved: bound, rewound to her view's
    /// checkpoint (so a restart resumes exactly at her view), retired through the pipeline, then her
    /// blocks and token history written. The manifest naming every preserved mind is published last:
    /// it is the commit point. A mind not preserved is reported, never silently dropped.
    fn suspend(&mut self, turns: &mut Vec<Turn>) -> Suspended {
        self.stopping = true;
        for turn in turns.drain(..) {
            self.finish_one_off(&turn);
            self.emit.send(emit::Out::End { answer: turn.answer, event: TokenEvent::Error("the engine is stopping: her state is preserved".into()) });
        }
        let minds: Vec<_> = self.parked.values().chain(self.engine.units())
            .filter(|mind| resident(mind)).map(|mind| mind.id()).collect();
        let blocked = |cause: Uuid, why: String| {
            let retained = if minds.is_empty() {
                vec![(cause, why)]
            } else {
                minds.iter().map(|mind| (mind.0, why.clone())).collect()
            };
            Suspended { preserved: 0, retained }
        };
        if let Some(id) = self.engine.retiring() {
            if let Err(why) = self.retire_previous() {
                return blocked(id.0, format!("copy-out of {:?} blocked preservation: {why}", id.0));
            }
        }
        let Some(mut store) = self.store.take() else {
            return blocked(Uuid::nil(), "this node has no home to preserve minds in".into());
        };
        // This stop's generation: every mind's blocks go to a file no published manifest names, and the
        // manifest naming them is published only once every write has landed (preserve.rs).
        let generation = preserve::Generation::fresh();
        let mut entries = Vec::new();
        let mut retained = Vec::new();
        for id in minds {
            match self.preserve(id, &store, &generation) {
                Ok(entry) => entries.push(entry),
                Err(why) => retained.push((id.0, why.to_string())), // the stop's report: Display at the edge
            }
        }
        let preserved = entries.len();
        let manifest = preserve::next_manifest(store.last(), &entries);
        let served = preserve::ServedModel { model: self.model_id.clone() };
        match store.publish(&served, &manifest) {
            Ok(unremoved) => retained.extend(unremoved.into_iter().map(|(path, why)| (Uuid::nil(), format!("an unnamed block file {} was not removed: {why}", path.display())))),
            Err(why) => retained.push((Uuid::nil(), format!("this stop's manifest was not saved, so the last complete generation stands: {why}"))),
        }
        continuum_base::probe!(class = "persona.engine.suspended", preserved = preserved as u64, retained = retained.len() as u64, "the engine's stop preserved its minds");
        // logged too: a stop's probes can be lost as the process ends (none reached the probe file on
        // the M5's first suspend, 2026-11-19 03:57Z)
        runtime::logger("persona-scheduler").info(&format!("the stop preserved {preserved} minds; not preserved: {retained:?}"));
        self.store = Some(store);
        Suspended { preserved, retained }
    }

    /// One mind out to her file in `generation`: bound, rewound to her view, retired through the
    /// pipeline, written. Her entry names that file; it is published with the stop's manifest.
    fn preserve(&mut self, id: PersonaId, store: &preserve::Preserved, generation: &preserve::Generation) -> Result<preserve::PreservedMind, PreserveError> {
        self.switch_to(id, false)?;
        let end = self.engine.active_mut().and_then(|her| her.parts().tokens.deepest());
        if let Some(end) = end {
            self.rewind_to_checkpoint(end)?;
            self.record(|her| her.tokens.cut(end))?;
        }
        if self.engine.retire_active()?.is_some() {
            self.retire_previous()?;
        }
        let her = self.parked.get(&id).ok_or(PreserveError::NotReturned)?;
        let parts = her.parts();
        let file = generation.file(id.0);
        parts.blocks.write_file(&store.path(&file))?;
        Ok(preserve::PreservedMind { persona: id.0, epoch: her.epoch().0, basis: parts.genome.genes.clone(), held: parts.tokens.held.clone(), blocks: file })
    }

    /// Take a submission into the frame loop: its token ids and its sampler, built once. A prompt
    /// or sampler that cannot be built is refused at once, before it waits for a frame.
    fn admit(&mut self, mut req: GenerationRequest, turns: &mut Vec<Turn>) {
        let (id, one_off) = match req.persona_id {
            Some(persona) => (PersonaId(persona), false),
            None => (PersonaId(Uuid::new_v4()), true),
        };
        // Her prompt arrives as token ids (`prepare`, on her own thread): nothing is tokenized here.
        match sampling::sampler(self.model, &req.sampling).map_err(TurnError::from) {
            Ok(sampler) => {
                let prompt = std::mem::take(&mut req.prompt);
                let basis = Basis { genes: signature(&req.active_loras) };
                let genes: Genes = req.active_loras.iter().map(|(_, adapter, scale)| (Arc::clone(adapter), *scale)).collect::<Vec<_>>().into();
                let boundary = req.view_end;
                self.answers += 1;
                let answer = emit::AnswerSeq(self.answers);
                let cancel = Arc::new(AtomicBool::new(false));
                self.emit.send(emit::Out::Open { answer, sink: req.response_tx.clone(), stops: req.stop_sequences.clone().into(), cancel: Arc::clone(&cancel) });
                let submitted = Instant::now();
                if one_off { self.live_one_offs.insert(id); }
                turns.push(Turn { req, id, one_off, prompt, basis, genes, sampler, phase: Phase::Waiting(Default::default()), submitted, ready_since: submitted, spent: Spent::default(), boundaries: Boundaries { view: boundary, learned: None }, answer, cancel });
            }
            Err(refused) => {
                tell(&req.response_tx, TokenEvent::Error(refused.to_string()));
            }
        }
    }

    /// The turn the next frame advances: a mind is one serial queue, so only her oldest turn is a
    /// candidate; among candidates, `frame::pick` decides. A waiting turn's prefill is what she
    /// cannot reuse, never her whole prompt: a 1.2k tail on a 29k prompt she holds is a 1.2k turn
    /// (the M5, 2026-11-19, a6da599330: sized as 29k, her rewind waited 161 s behind cold prompts).
    fn pick(&mut self, turns: &[Turn]) -> Option<(usize, std::num::NonZeroUsize)> {
        let mut seen = std::collections::HashSet::new();
        let eligible: Vec<usize> = (0..turns.len()).filter(|&index| seen.insert(turns[index].id)).collect();
        let active = self.engine.active();
        let candidates: Vec<frame::Candidate> = eligible
            .iter()
            .map(|&index| {
                let turn = &turns[index];
                let (decoding, remaining) = match &turn.phase {
                    Phase::Waiting(estimate) => (false, *estimate.get_or_init(|| {
                        let her = if active == Some(turn.id) { self.engine.active_mut().map(|her| &*her) } else { self.parked.get(&turn.id) };
                        let reused = if let Some(her) = her { reuse_for(her, &turn.prompt, &turn.basis).action.reused() } else { 0 }; // a mind never seen reuses nothing
                        turn.prompt.len().saturating_sub(reused)
                    })),
                    Phase::Prefilling { pos } => (false, turn.prompt.len().saturating_sub(*pos)),
                    Phase::Decoding { .. } => (true, 0),
                };
                let waited_ms = record::whole_ms(turn.ready_since.elapsed());
                frame::Candidate { active: active == Some(turn.id), decoding, remaining, waited_ms }
            })
            .collect();
        let ready = std::num::NonZeroUsize::new(eligible.len())?;
        frame::pick(&candidates, record::whole_ms(self.bound_since.elapsed())).map(|chosen| (eligible[chosen], ready))
    }

    /// One frame for `turn`: bind her if another mind is bound, then one step (a prefill chunk, or
    /// one token).
    fn step(&mut self, turn: &mut Turn, ready: std::num::NonZeroUsize) -> Result<Advanced, TurnError> {
        if turn.req.response_tx.is_closed() {
            return Err(TurnError::CallerReleased);
        }
        let gap = turn.ready_since.elapsed();
        turn.spent.longest_gap = turn.spent.longest_gap.max(gap);
        if turn.spent.first_step.is_none() {
            turn.spent.first_step = Some(turn.submitted.elapsed());
        }
        if self.engine.active() != Some(turn.id) {
            let switching = Instant::now();
            self.switch_to(turn.id, turn.one_off)?;
            self.bound_since = switching;
            turn.spent.switch += switching.elapsed();
            turn.spent.switches += 1;
        }
        if matches!(turn.phase, Phase::Waiting(_)) {
            let from = self.resume(turn)?;
            if from >= turn.prompt.len() {
                return Err(TurnError::NothingDecoded);
            }
            turn.phase = Phase::Prefilling { pos: from };
        }
        let stepping = Instant::now();
        let (advanced, decoding) = match std::mem::replace(&mut turn.phase, Phase::Waiting(Default::default())) {
            Phase::Waiting(_) => (Err(TurnError::NothingDecoded), false),
            Phase::Prefilling { pos } => (self.prefill_chunk(turn, pos, ready).map(|()| Advanced::Continues), false),
            Phase::Decoding { token, pos, generated } => (self.decode_token(turn, token, pos, generated), true),
        };
        let stepped = Instant::now();
        turn.ready_since = stepped;
        let took = stepped.saturating_duration_since(stepping);
        if decoding {
            turn.spent.decode += took;
        } else {
            turn.spent.prefill += took;
        }
        self.record.send(record::Fact::Busy { started: stepping, ended: stepped });
        if let Ok(Advanced::Done(TokenEvent::Done { tokens_generated, .. })) = &advanced {
            self.report_footprint(turn.id, turn.one_off);
            self.record.send(record::Fact::Turn(Self::row(turn, *tokens_generated)));
        }
        advanced
    }

    /// Where her prefill starts, now that she is bound (`reuse::choose`): what she holds when the
    /// prompt extends it, her checkpoint when the prompt keeps her view and changes what follows, else
    /// from zero for the reason named in her probe row. A rewind the engine refuses restarts her.
    fn resume(&mut self, turn: &mut Turn) -> Result<usize, TurnError> {
        let basis = turn.basis.clone();
        let her = self.engine.active_mut().ok_or(TurnError::NoneActive)?;
        let reuse::Decision { shared, action: decided } = reuse_for(her, &turn.prompt, &basis);
        turn.spent.shared_prefix_tokens = shared;
        // Prefill will cross this matching prefix again. Take a native checkpoint there
        // so the next turn can use it if both token history and recurrent lineage match.
        let can_learn = match decided {
            reuse::Reuse::Rewind { from } => shared > from,
            reuse::Reuse::Restart(reuse::Miss::Diverged | reuse::Miss::NoCheckpoint) => shared > 0,
            _ => false,
        };
        if can_learn && her.parts().genome == basis && turn.boundaries.view.is_some_and(|view| shared < view) {
            turn.boundaries.learned = Some(shared);
        }
        let (from, start) = match decided {
            reuse::Reuse::Append { from } => (from, "append"),
            reuse::Reuse::Rewind { from } => match self.rewind_to_checkpoint(from) {
                Ok(()) => (from, "rewind"),
                Err(_) => (0, "rewind_refused"),
            },
            reuse::Reuse::Restart(miss) => (0, miss.reason()),
        };
        let her = self.engine.active_mut().ok_or(TurnError::NoneActive)?;
        her.parts_mut().tokens.cut(from);
        her.parts_mut().genome = basis;
        turn.spent.reused = from;
        turn.spent.start = start;
        Ok(from)
    }

    /// Rewind the active mind to her kept checkpoint (taken out and put back, so the engine can read
    /// it while it moves her).
    fn rewind_to_checkpoint(&mut self, end: usize) -> Result<(), TurnError> {
        let snapshot = self.engine.active_mut().ok_or(TurnError::NoneActive)?.parts_mut().tokens.take_at(end).ok_or(TurnError::NoneActive)?;
        let rewound = self.engine.rewind_active(snapshot.blocks());
        self.record(|her| her.tokens.put_back(snapshot))?;
        Ok(rewound?)
    }

    /// Checkpoint the active mind at her view's end (`end` tokens held), reusing her checkpoint's
    /// buffers. A refused take keeps none, so a later act restarts instead of rewinding to a stale one.
    fn keep_checkpoint(&mut self, turn: &mut Turn, kind: Boundary, end: usize) -> Result<(), TurnError> {
        let taking = Instant::now();
        let history = &mut self.engine.active_mut().ok_or(TurnError::NoneActive)?.parts_mut().tokens;
        let Some(mut blocks) = history.capture_buffer(kind, end) else { return Ok(()) };
        if self.engine.checkpoint_active(&mut blocks).is_ok_and(|pos| pos as usize + 1 == end) {
            self.record(|her| her.tokens.save(kind, blocks, end, &turn.basis))?;
        }
        turn.spent.checkpoint_ms += record::whole_ms(taking.elapsed());
        Ok(())
    }

    /// One chunk of her prompt from `pos`. The last chunk asks for logits and samples her first
    /// token; only it is sampled, since the sampler accepts what it samples (Codex on #6140).
    fn prefill_chunk(&mut self, turn: &mut Turn, pos: usize, ready: std::num::NonZeroUsize) -> Result<(), TurnError> {
        // A chunk never runs past her view's end, so she can be checkpointed exactly there.
        let limit = turn.boundaries.next_end(pos, turn.prompt.len());
        // A submission can arrive during this frame. Size the next native call from
        // observed prefill time so a long prompt yields at the service boundary.
        let started = Instant::now();
        let allowance = u128::from(frame::share_ms(ready)) * 1_000_000;
        let count = self.prefill_sample.and_then(|(tokens, elapsed)| {
            (tokens as u128 * allowance).checked_div(elapsed.as_nanos())
        }).map_or(1, |count| count.clamp(1, self.batch.get() as u128) as usize);
        let end = pos.saturating_add(count).min(limit);
        let chunk = turn.prompt[pos..end].to_vec();
        let last = end == turn.prompt.len();
        let from = i32::try_from(pos).map_err(|_| TurnError::TooLong)?;
        let len = chunk.len();
        self.engine.submit(Step { tokens: chunk.clone(), from, logits_at_end: last, genes: Arc::clone(&turn.genes) })?;
        if !last {
            self.engine.settle()?;
            self.record(|her| her.tokens.held.extend_from_slice(&chunk))?;
            if let Some(kind) = turn.boundaries.at(end) {
                self.keep_checkpoint(turn, kind, end)?;
            }
            self.prefill_sample = Some((len, started.elapsed()));
            turn.phase = Phase::Prefilling { pos: end };
            return Ok(());
        }
        let at = i32::try_from(len - 1).map_err(|_| TurnError::TooLong)?;
        let token = self.engine.tier().sample(&mut turn.sampler, at);
        self.record(|her| her.tokens.held.extend_from_slice(&chunk))?;
        self.report_footprint(turn.id, turn.one_off);
        let pos = i32::try_from(end).map_err(|_| TurnError::TooLong)?;
        self.prefill_sample = Some((len, started.elapsed()));
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
        self.emit.send(emit::Out::Token { answer: turn.answer, token });
        let generated = generated + 1;
        if generated >= turn.req.max_tokens {
            return Ok(done(turn, generated));
        }
        let forward = Instant::now();
        let reused_before = self.engine.tier().graphs_reused();
        self.engine.submit(Step { tokens: vec![token], from: pos, logits_at_end: true, genes: Arc::clone(&turn.genes) })?;
        turn.spent.forward += forward.elapsed();
        turn.spent.graphs_reused += self.engine.tier().graphs_reused().saturating_sub(reused_before);
        self.record(|her| her.tokens.held.push(token))?;
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
    /// chip's rate, reuse >= 80%, no turn waiting behind another's whole prompt) is read live. Built
    /// from her counters here; written by Record.
    fn row(turn: &Turn, generated: usize) -> record::TurnRow {
        let ms = record::whole_ms;
        record::TurnRow {
            persona: turn.id.0,
            one_off: turn.one_off,
            prompt_tokens: turn.prompt.len() as u64,
            reused_tokens: turn.spent.reused as u64,
            shared_prefix_tokens: turn.spent.shared_prefix_tokens as u64,
            start: turn.spent.start,
            boundary: turn.boundaries.view.map(|end| end as u64),
            learned_boundary: turn.boundaries.learned.map(|end| end as u64),
            checkpoint_ms: turn.spent.checkpoint_ms,
            wait_ms: turn.spent.first_step.map(ms),
            longest_service_gap_ms: ms(turn.spent.longest_gap),
            switches: turn.spent.switches,
            switch_ms: ms(turn.spent.switch),
            prefill_ms: ms(turn.spent.prefill),
            decode_ms: ms(turn.spent.decode),
            forward_ms: ms(turn.spent.forward),
            graphs_reused: turn.spent.graphs_reused,
            gpu_ms: ms(turn.spent.gpu),
            sample_ms: ms(turn.spent.sample),
            generated: generated as u64,
            total_ms: ms(turn.submitted.elapsed()),
        }
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
            None => {
                let first = MindId { persona: id, epoch: Epoch(1) };
                let blocks = self.engine.blocks_for(first)?;
                let tenure = if one_off { Tenure::OneOff } else { Tenure::Resident };
                Mind::new(id, first.epoch, Parts { blocks, tokens: History::default(), genome: Basis::default(), learning: (), decode: tenure })
            }
        };
        if let Err(refused) = self.engine.stage(mind) {
            self.parked.insert(refused.returned.id(), refused.returned);
            return Err(refused.reason.into());
        }
        self.engine.settle()?;
        match self.engine.handoff() {
            Ok(_) | Err(HandoffError::CopyOut { .. }) => {}
            Err(refused) => return Err(refused.into()),
        }
        self.retire_previous()
    }

    fn finish_one_off(&mut self, turn: &Turn) {
        if turn.one_off {
            self.parked.remove(&turn.id);
            self.live_one_offs.remove(&turn.id);
        }
    }

    /// A paused one-off keeps her blocks until she finishes, just like a paused resident.
    fn retire_previous(&mut self) -> Result<(), TurnError> {
        let Some(previous) = self.engine.finish_retirement()? else { return Ok(()) };
        if resident(&previous) || self.live_one_offs.contains(&previous.id()) {
            self.parked.insert(previous.id(), previous);
        }
        Ok(())
    }

    /// Update the active mind's own record (what her blocks now hold).
    fn record(&mut self, change: impl FnOnce(&mut Parts)) -> Result<(), TurnError> {
        change(self.engine.active_mut().ok_or(TurnError::NoneActive)?.parts_mut());
        Ok(())
    }

    /// Attribute what the active mind holds to her in the footprint registry: her own footprint
    /// (tokens, checkpoint) and her blocks in the context, each asked of its owner (one-offs are not
    /// attributed).
    fn report_footprint(&mut self, id: PersonaId, one_off: bool) {
        if one_off {
            return;
        }
        let Some(her) = self.engine.active_mut().filter(|her| her.id() == id) else { return };
        let (mind, own) = (MindId { persona: id, epoch: her.epoch() }, her.parts().tokens.held());
        let blocks = Bound { tier: self.engine.tier(), mind }.held();
        if blocks == Held::NOTHING {
            return;
        }
        footprint_registry::global().report_authoritative(FootprintKey::for_persona(id.0, ResourceType::KvCache, Residency::Active), (blocks + own).total());
    }
}

/// A gene set's identity: sorted `id:scale`, comma-joined; empty is the base model.
/// Tell a turn's caller `event`. A caller that hung up has gone, and the probe says so.
fn tell(caller: &tokio::sync::mpsc::UnboundedSender<TokenEvent>, event: TokenEvent) {
    if caller.send(event).is_err() {
        continuum_base::probe!(class = "persona.engine.caller_gone", "a turn's caller hung up before its answer");
    }
}

/// Report a suspend to the stop that asked. A stop that gave up has its custody report already
/// (its cap ran out first), and the probe says this one arrived after.
fn report(stop: tokio::sync::oneshot::Sender<Suspended>, suspended: Suspended) {
    if let Err(late) = stop.send(suspended) {
        continuum_base::probe!(class = "persona.engine.suspend_unheard", preserved = late.preserved as u64, retained = late.retained.len() as u64, "a suspend finished after its stop gave up");
    }
}

fn signature(genes: &[(String, Arc<LoraAdapter>, f32)]) -> String {
    let mut parts: Vec<String> = genes.iter().map(|(id, _, scale)| format!("{id}:{scale}")).collect();
    parts.sort_unstable();
    parts.join(",")
}
