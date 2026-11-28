//! The driver thread: a frame loop over one context (CONTINUUM-ENGINE 4.15). Each frame advances one
//! turn by one step, so no mind waits behind another's whole prompt; one mind decodes at a time.

use std::collections::{HashMap, HashSet};
use std::num::NonZeroUsize;
use tokio::sync::mpsc::UnboundedReceiver as Receiver;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use continuum_persona_engine::engine::{BatchError, DecodeError, HandoffError, PersonaEngine, RetireError, StageError};
use continuum_persona_engine::persona::{Epoch, Footprint, Held, MindId, PersonaId, PersonaParts, PersonaUnit};
use continuum_persona_engine::residency::TierRefusal;
use continuum_persona_llama::{Blocks, Genes, KvHome, Step, Tokens, UnifiedTier};
use llama::{ContextParams, FlashAttn, Model, Pos};
use uuid::Uuid;

use super::geometry::StopsOnExit;
use continuum_sage::decide::cases::lane_history::Observation;
use continuum_sage::decide::Measured;
use continuum_sage::{Outbox, StageInput};

use super::{admission, emit, preserve, record, reuse, transition, EngineGeometry, EngineState, GenerationRequest, Incarnation, SchedulerConfig, Submission, Suspended, TokenEvent};
use crate::inference::footprint_registry::{self, FootprintKey, ResourceType};
use crate::inference::kv_quant::Residency;
use crate::runtime;

/// Why a turn did not complete. The mind stays in the engine's custody either way.
#[derive(Debug, thiserror::Error)]
pub(super) enum TurnError {
    /// Her sampler refused a step (a token its grammar could not accept, named by llama.cpp).
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
    #[error(transparent)]
    Batch(#[from] BatchError),
    #[error("no mind is active after the switch")]
    NoneActive,
    #[error("only her genes' identity was kept (she was resumed from disk): her prefix cannot be prefilled under them")]
    GenesNotHeld,
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
    #[error(transparent)]
    HandOver(#[from] preserve::HandOverError),
}

/// The context could not be made, so no turn can run.
#[derive(Debug, thiserror::Error)]
enum StartError {
    #[error("the persona scheduler's context was refused: {0}")]
    Context(String),
    #[error("the persona scheduler's batch size is zero")]
    ZeroBatch,
    #[error("no home for minds' KV and preserved state: {0}")]
    Home(#[from] preserve::StoreError),
    #[error("the lane has no algorithm named {name}", name = .0.name)]
    Lane(continuum_sage::decide::Unknown),
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

mod ending;
mod history;
mod lane;
mod learning;
mod basis;
mod divergence;
use basis::Basis;
mod prefix;
mod on_board;
mod prefill_rate;
mod intake;
mod step;
mod facts;
mod switch;
mod turn;
mod batch;
use step::{Before, Kind, Moved};
use turn::{Advanced, Phase, Spent, Turn};
pub(crate) use learning::{Job as LearningJob, Lesson as LearningLesson, Measuring as LearningMeasuring};

use ending::{Ending, Stop, Suspension};
pub(super) use ending::finish;
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
    /// The most one mind's stream holds (the applied `n_ctx_seq`): a prompt it cannot fit is refused at the door.
    capacity: admission::Capacity,
    engine: PersonaEngine<Mind, UnifiedTier<'c, 'm>>,
    parked: HashMap<PersonaId, Mind>,
    /// Admitted anonymous turns that still own their native blocks, across all transfer states.
    live_one_offs: HashSet<PersonaId>,
    /// Where decoded tokens and answer ends go (4.16); the engine thread never does text work.
    emit: Outbox<emit::Out>,
    /// Where each GPU step's span and each finished turn's row go; measured and written off this
    /// thread.
    record: Outbox<record::Fact>,
    /// The probe-log write failures counted at the last settle: a stop reports those since.
    log_failures: continuum_base::probe_writes::Reading,
    /// Answers opened by this engine, numbering each.
    answers: u64,
    /// Where the stop preserves minds and the start resumes them (`None`: this node has no home to
    /// write them to, and its minds are dropped at a stop, said once at start).
    store: Option<preserve::Preserved>,
    /// The model this engine serves: preserved minds resume only on it.
    model_id: String,
    /// Which turn takes the next step: the SAGE lane (driver/lane.rs).
    lane: lane::Lanes,
    /// What the trainer told, worked between her turns.
    learning: learning::Learning,
    /// What the Core holds, on the resource board (driver/on_board.rs).
    on_board: on_board::CoreHeld,
    /// The stop has suspended every mind: no new turn is taken.
    stopping: bool,
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
        match preserved.restore(tier, &mind) {
            Ok(preserve::Restored { blocks, held, checkpoint }) => {
                continuum_base::probe!(class = "persona.engine.resumed", persona = %mind.persona, tokens = held.len() as u64, in_place = mind.kv.is_some(), "a mind came back from her preserved files");
                // a mind resumed from disk keeps her genes and their files, never the adapters themselves
                let genome = Basis::resumed(mind.basis, mind.genes); let parts = Parts { blocks, tokens: History::from(held).with_view(checkpoint, &genome), genome, learning: (), decode: Tenure::Resident };
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
                Submission::Learning(job) => job.refuse(&why),
                Submission::Suspend(reply) => {
                    report(reply, Suspended::default());
                }
            }
        }
    };
    let lane = match lane::Lanes::new(config.lane_algorithm) {
        Ok(lane) => lane,
        Err(unknown) => {
            let refused = StartError::Lane(unknown);
            refuse(&refused);
            refuse_all(refused.to_string());
            return;
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
    let geometry = EngineGeometry::applied(incarnation, &mut ctx, config);
    let capacity = admission::Capacity::new(geometry.n_ctx_seq as usize);
    // The root resume (CONTINUUM-ENGINE 4.16): the minds the last stop preserved come back Warm, in
    // RAM, and bind when they are next scheduled. A mind that does not verify stays on disk, named.
    // Each mind's KV is a block over her own mapped file in this node's home. With no home, or no
    // instance in it, the engine refuses to start: never the fork's row-copy memory (a 3 ms CPU copy
    // per switch on the 35B) and never minds that a stop cannot preserve.
    let (mut store, instance) = match preserve::Preserved::at_home().and_then(|store| store.live_kv().map(|kv| (store, kv))) {
        Ok(found) => found,
        Err(why) => {
            let refused = StartError::Home(why);
            refuse(&refused);
            refuse_all(refused.to_string());
            return;
        }
    };
    let mut lane = lane;
    lane.resume(store.dir(), &config.model);
    lane.batch_up_to(config.streams);
    let mut tier = UnifiedTier::new(&mut ctx, KvHome::Mapped { instance });
    let parked = resume(&mut tier, &mut store, &config.model, &log);
    log.info(&format!("persona scheduler ready: {geometry:?} (one mind decodes at a time; {} minds resumed)", parked.len()));
    state.send_replace(EngineState::Ready(geometry));
    let mut driver = Driver {
        model,
        batch,
        capacity,
        engine: PersonaEngine::new(tier),
        parked,
        live_one_offs: HashSet::new(),
        emit: Outbox::new(emit),
        record: Outbox::new(record),
        log_failures: continuum_base::probe_writes::Reading::now(),
        answers: 0,
        store: Some(store),
        model_id: config.model.clone(),
        stopping: false,
        learning: learning::Learning::default(),
        on_board: on_board::CoreHeld::new(),
        lane,
    };
    driver.on_board.read(driver.engine.tier());
    // The frame loop (CONTINUUM-ENGINE 4.15): submissions are drained without blocking, and each
    // frame advances ONE turn by ONE step (a prefill chunk or a token), chosen by the SAGE lane
    // (driver/lane.rs): any mind past her wait bound steps next, an addressed mind takes the next step. What
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
        // Learning between her turns, and when the lane owes the background class a step (her learning
        // keeps its queue in check, never waiting for an empty Core), asking between chunks whether a turn waits.
        if !holding && driver.learning.waiting() && (turns.is_empty() || driver.lane.learning_owed()) {
            driver.learn_next(&mut || requests.is_empty());
            continue;
        }
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
                    Some(Some(Submission::Learning(_))) => "learning",
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
                Some(Some(submission)) => {
                    if let Some(stop) = driver.take(submission, &mut turns) {
                        driver.acknowledge(stop, &stages_on);
                    }
                }
                // Every handle dropped: every admitted turn ends and its caller is told.
                Some(None) => return driver.close(&mut turns, &stages_on),
                None => {} // the stages took what waited
            }
            continue;
        }
        while let Ok(submission) = requests.try_recv() {
            if let Some(stop) = driver.take(submission, &mut turns) {
                driver.acknowledge(stop, &stages_on);
            }
        }
        let Some(picked) = driver.pick(&turns) else { continue };
        let stepped = match picked {
            lane::Picked::One(index) => vec![(index, driver.step(&mut turns[index]))],
            lane::Picked::Batch => driver.step_batch(&mut turns),
        };
        // ended turns leave by descending index, so removing one never moves another's
        let mut ended: Vec<(usize, Ending)> = Vec::new();
        for (index, advanced) in stepped {
            match advanced {
                Ok(Advanced::Continues) => { if let Some(r) = &turns[index].req.request { crate::inference::llama_server::engine_step(crate::inference::llama_server::GenerationKey::of(r.as_str())); } } // completed progress the act deadline reads
                Ok(Advanced::Done { tokens_generated, end }) => ended.push((index, Ending::Answered { tokens_generated, end })),
                Err(refused) => {
                    log.warn(&format!("turn refused: {refused}"));
                    ended.push((index, Ending::Refused(refused)));
                }
            }
        }
        ended.sort_by_key(|&(index, _)| std::cmp::Reverse(index));
        for (index, ending) in ended {
            driver.end(turns.remove(index), ending);
        }
    }
}

/// The Core's one context: a stream per mind it binds at once (`config.streams`), each the whole window.
fn context<'m>(model: &'m Model, config: &SchedulerConfig) -> Result<llama::Context<'m>, StartError> {
    let streams = config.streams.get();
    // each stream holds a whole window (a window never shrinks to seat more): the context holds them all
    let n_ctx = config.n_ctx.checked_mul(streams).ok_or_else(|| StartError::Context("the streams' windows exceed what a context addresses".into()))?;
    model
        .new_context(ContextParams {
            n_ctx,
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
            type_k: config.type_k,
            type_v: config.type_v,
            embeddings: false,
            pooling_type: llama::PoolingType::None,
            expert_observer: None,
        })
        .map_err(StartError::Context)
}

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
