//! The trainer on the node's in-process engine (IN-PROCESS-LEARNING.md L3, card 28aac827).
//!
//! Every bucket with pending examples learns through the engine's [`LearnPort`], one seat per bucket.
//! A pass starts a seat: the competence decision ([`super::in_place::decide`]) and a Measure sized for
//! her next lesson. From there the engine's own word drives it. Each [`Said`] on `learning/said`
//! reaches [`TrainingTriggerState::on_said`] through the trainer component, under the bucket's gate,
//! and the seat's next act follows from it: lease what was measured, tell the next lesson,
//! checkpoint, commit a saved generation, release. Nothing waits on the engine.
//!
//! The trainer owns every lease (genome `seat::lease_memory`), and the engine gets only
//! `LearningMemory` views over them. The journal's cursor moves only with a committed generation, so
//! a lesson too long for the seat, or a seat let go, never loses or replays what was learned.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Weak};

use uuid::Uuid;

use super::durable::{Arrival, Counted, Generation, Position};
use super::in_place::{bucket_stem, decide, start, Decided, Decision, Pending};
pub(crate) use follow_up::FollowUp;
use phases::{Phase, Phases};
use super::seat_run::{Heard, NotTold, SeatRun};
use super::{BucketKey, TrainingTriggerState};
use crate::genome::learner::example::learn_messages;
use crate::genome::learner::seat::{lease_memory_within, Authority, LeasedMemory, SeatRefusal};
use crate::inference::learning::{leased, Fit, LearnPort, Lesson, Measure, MemoryRow, Optimizer, PersonaId, Said, SeatId, ShadowSource};
use crate::inference::training_admission::TrainingWait;
use crate::resources::{Ask, AskTicket, Free, LeaseGuard, ResourceKind};
use crate::inference::learning::FreshShadow;
use continuum_kernel::system_resources::disk_reporters::ClassWriter;

/// The disk class the engine writes generations into and the trainer writes their manifests into.
const ENGINE_TRAIN: &str = "engine-train";

/// Why the trainer could not take a seat's next act.
#[derive(Debug, thiserror::Error)]
pub(crate) enum OnEngineError {
    /// The training journal refused (the durable store's own failures are text).
    #[error("the training journal: {0}")]
    Journal(String),
    /// A counted read of her examples: rows.iter().map(|row| row.examples as usize).sum(), signature }; // as: a row's count fits a usize on every 64-bit target this builds fors failed (card d1e5675c).
    #[error("her rows: {0}")]
    Rows(#[from] super::durable::JournalRead),
    #[error("its memory was not leased: {0}")]
    Lease(SeatRefusal),
    /// The leases do not cover what the engine measured.
    #[error("its leases do not cover the measured memory: {0}")]
    Coverage(&'static str),
    #[error("the engine was not told: {0}")]
    Told(NotTold),
    #[error("the engine train dir: {0}")]
    TrainDir(#[source] std::io::Error),
    #[error("the generation's manifest: {0}")]
    Manifest(#[source] serde_json::Error),
    #[error("the manifest write ended without an answer: {0}")]
    Write(#[source] tokio::task::JoinError),
    /// The engine said what the seat never told: a fact the trainer cannot act on.
    #[error("{0}")]
    Unanswered(&'static str),
    /// A lease could not be made elastic: it was gone before the seat held it.
    #[error("its lease could not give back: {0:?}")]
    Elastic(crate::resources::LeaseError),
}

impl From<NotTold> for OnEngineError {
    fn from(not: NotTold) -> Self {
        OnEngineError::Told(not)
    }
}

/// One bucket's seat on the node engine.
struct EngineSeat {
    key: BucketKey,
    run: SeatRun<LeasedMemory>,
    signature: Option<crate::genome::signature::GeneSignature>,
    persona_name: String,
    binding: Arc<str>,
    /// The positions the bucket's newest committed generation holds: the floor every lesson starts at.
    checkpointed: HashMap<Uuid, u32>,
    /// The generation name of each Checkpoint told, oldest first: a Saved answers the oldest.
    names: VecDeque<String>,
    /// No more lessons for this seat (one was too long or refused): what it learned is saved and it
    /// is let go, and the next pass seats the bucket again sized for what waits.
    finishing: bool,
    /// The weight rows the engine measured for this seat: part of the whole seat a hold for memory
    /// waits for, since seating it again leases them again.
    weights: Vec<MemoryRow>,
    /// Whether a higher wait asked it to give back: it then finishes as `finishing` does.
    asked_back: give_back::AskedBack,
    /// The examples its bucket still owes past her newest generation and its own progress (refreshed
    /// whenever its rows are read), its next lesson's rows as the engine measured them, and when it
    /// began owing: what `learning/owed` says of it.
    examples_owed: usize,
    next: Option<crate::inference::learning::Rows>,
    since: std::time::Instant,
}

/// A generation the engine saved: where, how many optimizer steps it holds, and the positions it
/// commits.
struct Saved {
    into: PathBuf,
    steps: u64,
    positions: Vec<Position>,
}

/// Why a bucket is not seated again on the next pass.
enum HeldOff {
    /// The engine measured more than the whole governed device holds: a defect in the measurement,
    /// never memory to wait for. Seated again only on another node engine (the 5090, 2026-11-24: a
    /// fresh shadow measured at 428,581,700,665,600 VRAM bytes was measured again on every wake).
    Defect { engine: Weak<dyn LearnPort> },
    /// Short of memory: the whole seat was asked of the authority, and the bucket is held until the
    /// ask is granted. The grant becomes the bucket's reserve, and its seat's stages are carved from it
    /// with no second ask (Codex's contract, 2026-11-26). Dropping the hold withdraws the ask.
    Asked { ticket: AskTicket },
}

/// The seats the trainer holds on the node engine, and the buckets it holds off.
#[derive(Default)]
pub(crate) struct OnEngine {
    seats: parking_lot::Mutex<HashMap<SeatId, EngineSeat>>,
    held_off: parking_lot::Mutex<HashMap<BucketKey, HeldOff>>,
    /// Each bucket's granted reserve: what its ask was granted, carved stage by stage as its seat
    /// leases, and released (what is left of it) once the seat is seated or let go.
    reserves: parking_lot::Mutex<HashMap<BucketKey, Vec<LeaseGuard>>>,
    /// Each bucket's minted competence signature, with the rows it was minted from: minting embeds
    /// every example, so it is minted again only when the bucket's rows change (Codex on the 5090
    /// replay, 2026-11-25: a bucket decide() answered NotNow was re-embedded on every wake).
    signatures: parking_lot::Mutex<HashMap<BucketKey, (RowsSeen, crate::genome::signature::GeneSignature)>>,
    /// The pending buckets on a base this engine does not serve, as last said, with the base it
    /// served: said again only when either changes, so a node whose lessons were taken on another
    /// base says so once instead of seating nothing in silence (the M5, 2026-11-26: 1,454 examples
    /// keyed to Ornith-1.5-35B, Qwen3.6-35B served, no probe at all).
    unserved: parking_lot::Mutex<Option<(String, Vec<BucketKey>)>>,
}

impl OnEngine {
    fn seated(&self, key: &BucketKey) -> bool {
        self.seats.lock().values().any(|seat| seat.key == *key)
    }

    fn key_of(&self, seat: SeatId) -> Option<BucketKey> {
        self.seats.lock().get(&seat).map(|held| held.key.clone())
    }

    /// Whether `key` is still held off on `port`: a defect while the same engine serves, an ask until it
    /// is granted. A granted ask becomes the bucket's reserve and the hold is dropped.
    fn holds(&self, key: &BucketKey, port: &Arc<dyn LearnPort>) -> bool {
        let mut held = self.held_off.lock();
        let still = match held.get_mut(key) {
            None => return false,
            Some(HeldOff::Defect { engine }) => std::ptr::addr_eq(engine.as_ptr(), Arc::as_ptr(port)),
            Some(HeldOff::Asked { ticket }) => !self.reserve_granted(key, ticket),
        };
        if !still {
            held.remove(key);
        }
        still
    }

    /// Take `ticket`'s grant into `key`'s reserve, if it was granted.
    fn reserve_granted(&self, key: &BucketKey, ticket: &mut AskTicket) -> bool {
        let Some(granted) = ticket.granted() else { return false };
        let key = key.clone();
        self.reserves.lock().entry(key).or_default().push(granted);
        true
    }

    /// Say the pending buckets this engine does not serve the base of, when they or the served base
    /// changed since last said.
    fn say_unserved(&self, served: &str, elsewhere: Vec<BucketKey>) {
        let mut said = self.unserved.lock();
        if said.as_ref().is_some_and(|(base, keys)| base == served && *keys == elsewhere) {
            return;
        }
        if !elsewhere.is_empty() {
            let mut bases: Vec<&str> = elsewhere.iter().map(|key| key.base_model.as_str()).collect();
            bases.sort_unstable();
            bases.dedup();
            continuum_base::probe!(class = "training.on_engine.unserved_base", served = %served, buckets = elsewhere.len(), bases = ?bases,
                "pending lessons wait on a base this engine does not serve: nothing here learns them");
        }
        *said = Some((served.to_owned(), elsewhere));
    }

    /// Whether any bucket waits for memory.
    fn waits_for_memory(&self) -> bool {
        self.held_off.lock().values().any(|hold| matches!(hold, HeldOff::Asked { .. }))
    }

    /// The buckets whose asks were granted and that hold no seat, their grants taken into their reserves
    /// and their holds dropped: what a release wakes. A bucket whose seat is still being let go is left
    /// to its Released ([`Self::covered`]).
    fn freed(&self) -> Vec<BucketKey> {
        let seats = self.seats.lock();
        let mut held = self.held_off.lock();
        let granted = held.extract_if(|key, hold| {
            if seats.values().any(|seat| seat.key == *key) {
                return false;
            }
            let HeldOff::Asked { ticket } = hold else { return false };
            self.reserve_granted(key, ticket)
        });
        granted.map(|(key, _)| key).collect()
    }

    /// Whether `key`, its seat just let go, had its ask granted (the grant taken into its reserve and
    /// the hold dropped). A release said while that seat was still being let go woke nothing for it, so
    /// its Released seats it again.
    fn covered(&self, key: &BucketKey) -> bool {
        let mut held = self.held_off.lock();
        let covered = match held.get_mut(key) {
            Some(HeldOff::Asked { ticket }) => self.reserve_granted(key, ticket),
            _ => false,
        };
        if covered {
            held.remove(key);
        }
        covered
    }

    /// `key`'s reserve is spent or no longer wanted (its seat is seated, or let go): what is left of it
    /// goes back to the authority.
    fn release_reserve(&self, key: &BucketKey) {
        self.reserves.lock().remove(key);
    }

    /// Hold `key` off after the authority refused `needed` bytes of `kind` for `rows`: a defect past the
    /// whole governed device, otherwise an ask for that much, which waits until it is granted (granted at
    /// once, it is the bucket's reserve and nothing is held).
    fn hold_off(&self, key: &BucketKey, port: &Arc<dyn LearnPort>, wait: Short, ask: impl FnOnce(ResourceKind, u64) -> Result<Ask, crate::resources::LeaseError>) {
        let Short { kind, needed, capacity } = wait;
        let defect = |why: &str| {
            continuum_base::probe!(class = "training.on_engine.measure_defect", persona = %key.persona_id, kind = ?kind, needed, capacity = ?capacity, why = why,
                "the engine measured more than the whole governed device holds: a measurement defect, not a wait; this bucket is not seated again on this engine");
            HeldOff::Defect { engine: Arc::downgrade(port) }
        };
        let held = match capacity {
            Some(capacity) if needed > capacity => defect("past the governed capacity"),
            _ => match ask(kind, needed) {
                Ok(Ask::Granted(granted)) => {
                    let key = key.clone();
                    self.reserves.lock().entry(key).or_default().push(granted);
                    return;
                }
                Ok(Ask::Waiting { ticket, short }) => {
                    continuum_base::probe!(class = "training.on_engine.awaiting_memory", persona = %key.persona_id, kind = ?kind, needed, short, capacity = ?capacity,
                        "short of memory: the whole seat is asked of the authority, and this bucket waits for the grant");
                    HeldOff::Asked { ticket }
                }
                Err(refused) => defect(&format!("the authority refused the ask: {refused:?}")),
            },
        };
        self.held_off.lock().insert(key.clone(), held);
    }
}

/// The rows a signature was minted from: each submission and how many examples it held. A new
/// submission or more examples in one makes it a different set.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RowsSeen(Vec<(Uuid, u64)>);

impl RowsSeen {
    fn of(rows: &[Counted]) -> Self {
        Self(rows.iter().map(|row| (row.id, row.examples)).collect())
    }
}

/// One lease the trainer asks the authority for: `rows` for `consumer`, as part of the whole seat
/// `whole` (every row seating it again leases), which a refusal for memory waits for.
struct Lease<'a> {
    consumer: &'a str,
    rows: &'a [MemoryRow],
    whole: &'a [MemoryRow],
}

/// What the authority refused: `needed` bytes of `kind`, of the `capacity` it governs of that kind
/// (`None` when it governs none).
struct Short {
    kind: ResourceKind,
    needed: u64,
    capacity: Option<u64>,
}

/// What a seat may lease of each kind beside its `weights`: the kind's ceiling (capacity less what
/// nothing can demote and what others hold now) less them. Never capacity: on the 5090 a lesson sized
/// within the whole card asked for 29.5 GB serving held 30.7 GB of, and waited forever (Cormac,
/// 2026-11-26). A kind the authority does not govern leaves none, and weights past a byte count leave none.
fn fit_beside(free: &Free, weights: &[MemoryRow]) -> Fit {
    let room = |kind: ResourceKind| match (free.kind(kind), leased(weights, kind)) {
        (Some(governed), Some(held)) => governed.ceiling.saturating_sub(held),
        (None, _) | (_, None) => 0,
    };
    Fit { vram: room(ResourceKind::Vram), ram: room(ResourceKind::Ram) }
}

/// What `authority` shows free now.
fn free_now(authority: &Authority) -> Free {
    Free::of(&authority.resources.board())
}

/// What a `said` reports, by kind alone (no rows or lesson text), for its probe.
fn said_kind(said: &Said) -> &'static str {
    use crate::inference::learning::{GenerationOutcome, SeatState, StepOutcome};
    match said {
        Said::Seat { state, .. } => match state {
            SeatState::Measured { .. } => "measured",
            SeatState::Loaded { .. } => "loaded",
            SeatState::Seated { .. } => "seated",
            SeatState::Refused { .. } => "seat_refused",
            SeatState::Kept { .. } => "kept",
            SeatState::Released => "released",
            SeatState::Quarantined { .. } => "quarantined",
        },
        Said::Step { outcome, .. } => match outcome {
            StepOutcome::Learned { .. } => "learned",
            StepOutcome::Unchanged => "unchanged",
            StepOutcome::Uncertain { .. } => "uncertain",
            StepOutcome::TooLong { .. } => "too_long",
            StepOutcome::Refused { .. } => "step_refused",
            StepOutcome::WouldCutHerContext { .. } => "would_cut_her_context",
        },
        Said::Generation { outcome, .. } => match outcome {
            GenerationOutcome::Saved { .. } => "saved",
            GenerationOutcome::Refused { .. } => "generation_refused",
        },
    }
}

/// Why the engine refused, kept or quarantined, as it said it; empty for an answer that carries no
/// reason. A refusal said with no reason left the M5's step_refused seats unexplained (2026-11-26).
fn why_of(said: &Said) -> &str {
    use crate::inference::learning::{GenerationOutcome, SeatState, StepOutcome};
    match said {
        Said::Seat { state: SeatState::Refused { why } | SeatState::Kept { why } | SeatState::Quarantined { why }, .. }
        | Said::Step { outcome: StepOutcome::Refused { why }, .. }
        | Said::Generation { outcome: GenerationOutcome::Refused { why }, .. } => why,
        _ => "",
    }
}

/// What a learned step or a saved generation counted: the optimizer steps, and a step's loss when
/// native code read one. Nothing for any other answer.
fn progress_of(said: &Said) -> (Option<u64>, Option<f64>) {
    use crate::inference::learning::{GenerationOutcome, StepOutcome};
    match said {
        Said::Step { outcome: StepOutcome::Learned { steps, loss }, .. } => (Some(steps.get()), *loss),
        Said::Generation { outcome: GenerationOutcome::Saved { steps, .. }, .. } => (Some(*steps), None),
        _ => (None, None),
    }
}

/// The seat a `said` is about.
fn seat_of(said: &Said) -> SeatId {
    match said {
        Said::Seat { seat, .. } | Said::Step { seat, .. } | Said::Generation { seat, .. } => *seat,
    }
}

/// PURE: where the next lesson of `rows` is, past what the journal, the newest generation
/// (`checkpointed`) and the seat's volatile `progress` hold: counted, no batch read (card d1e5675c).
fn next_lesson(rows: &[Counted], checkpointed: &HashMap<Uuid, u32>, progress: &HashMap<Uuid, u32>) -> Option<Position> {
    rows.iter().find_map(|row| {
        let learned = start(row.learned, checkpointed.get(&row.id).copied(), progress.get(&row.id).copied());
        (u64::from(learned) < row.examples).then_some(Position { submission: row.id, learned })
    })
}

/// The lesson at `position`: the one example it names, read from its own row.
async fn lesson_at(store: &super::durable::DurableStore, position: Position) -> Result<Lesson, OnEngineError> {
    let row = store.arrival_of(position.submission).await?;
    let example = usize::try_from(position.learned).ok().and_then(|index| row.examples.get(index));
    example.map(learn_messages).ok_or(OnEngineError::Unanswered("a counted lesson is not in its row"))
}

/// Tell `held` to save what it learned as a new generation in the engine train dir.
fn checkpoint(port: &dyn LearnPort, held: &mut EngineSeat) -> Result<(), OnEngineError> {
    let train = ClassWriter::of(ENGINE_TRAIN).map_err(OnEngineError::TrainDir)?;
    let name = format!("{}-{}.gguf", bucket_stem(&held.key), Uuid::new_v4().simple());
    let into = train.dir().join(format!("{name}.checkpoint"));
    held.run.checkpoint(port, into, Arc::clone(&held.binding))?;
    held.names.push_back(name);
    Ok(())
}

/// The model the node's in-process engine serves now: `None` with no engine or nothing loaded. Asked
/// of the engine itself (the 5090, 2026-11-24: the trainer read the serving snapshot's model, said
/// no learning engine, and never seated).
pub(crate) async fn served_model() -> Option<String> {
    use crate::inference::llama_server::LlamaServerControl;
    let engine = crate::inference::in_process_engine::node_engine()?;
    match engine.active_model().await {
        Ok(model) => model,
        Err(why) => {
            continuum_base::probe!(class = "training.on_engine.model_unread", reason = %why, "the node engine could not say what it serves; nothing is seated this pass");
            None
        }
    }
}

/// The trainer's port: the node's in-process engine, once serving installed one.
pub(crate) fn port() -> Option<Arc<dyn LearnPort>> {
    crate::inference::in_process_engine::node_engine().map(|engine| engine as Arc<dyn LearnPort>)
}

impl TrainingTriggerState {
    /// A Said heard on the node bus, for the trainer's seats on the node engine: the seat's next act,
    /// and what it leaves for the trainer's worker.
    pub(crate) async fn heard_from_engine(self: &Arc<Self>, said: Said) -> Option<FollowUp> {
        match port() {
            Some(port) => self.on_said(&port, said).await,
            None => {
                continuum_base::probe!(class = "training.on_engine.no_port", said = ?said, "a Said arrived with no node engine installed: nothing is told for it");
                None
            }
        }
    }

    /// The authority said its board changed: a release may have granted the asks of buckets held for
    /// memory (what is free is the authority's to grant, not the trainer's to read).
    pub(crate) async fn heard_free(self: &Arc<Self>, _changed: &Free) -> Vec<FollowUp> {
        if !self.on_engine.waits_for_memory() {
            return Vec::new(); // nothing held: the engine is not asked what it serves
        }
        let Some(served) = served_model().await else { return Vec::new() };
        self.on_free(&served)
    }

    /// Seat every bucket whose base `served_model` serves and that has a lesson waiting, through
    /// `port`. A bucket already seated learns on from what the engine says.
    pub(crate) async fn learn_on_engine(self: &Arc<Self>, port: &Arc<dyn LearnPort>, served_model: &str) -> Result<(), OnEngineError> {
        let store = self.durable.require().map_err(OnEngineError::Journal)?;
        let waiting = super::in_place::waiting_by_base(&store, served_model).await.map_err(OnEngineError::Journal)?;
        self.on_engine.say_unserved(served_model, waiting.elsewhere);
        for key in waiting.here {
            if let Err(why) = self.seat_bucket(port, &key).await {
                continuum_base::probe!(class = "training.on_engine.seat_failed", persona = %key.persona_id, reason = %why,
                    "a bucket was not seated on the engine; its examples stay pending and the next pass tries again");
            }
        }
        // every bucket's newest generation is candidate work of its own, retried until taken
        self.sweep_all(served_model).await.map_err(OnEngineError::Journal)
    }

    /// Seat bucket `key` if it is not seated and has a lesson waiting, under its gate. Its rows are
    /// read again under the gate: a re-key that moved them after the pass listed the bucket leaves
    /// nothing here to seat (Codex on #6226), and the moved rows learn under their new key.
    pub(crate) async fn seat_bucket(self: &Arc<Self>, port: &Arc<dyn LearnPort>, key: &BucketKey) -> Result<(), OnEngineError> {
        let began = std::time::Instant::now();
        let _gate = self.submit_gates.acquire(key).await;
        let gate_wait_ms = began.elapsed().as_millis() as u64;
        if self.on_engine.seated(key) || self.on_engine.holds(key, port) {
            return Ok(());
        }
        let read = std::time::Instant::now();
        let rows = self.counts_of(key).await;
        let rows_ms = read.elapsed().as_millis() as u64;
        let rows_read = rows.as_ref().ok().map(Vec::len); // none: the journal could not be read
        let seating = std::time::Instant::now();
        let seated = match rows {
            Ok(rows) => self.seat_on_engine(port, key, &rows).await,
            Err(why) => Err(why),
        };
        // each phase timed, so where a pass spends its time is read from one probe, never from gaps
        // between others (the 5090, card d1e5675c: 442 s between two buckets' decisions, unattributed)
        continuum_base::probe!(class = "training.on_engine.seat_bucket", persona = %key.persona_id, trait_kind = %key.trait_kind, gate_wait_ms, rows_ms, rows_read = ?rows_read,
            seat_ms = seating.elapsed().as_millis() as u64, ok = seated.is_ok(), "one bucket's seating, by phase");
        // a reserve only ever serves a seat: a pass that seated nothing (nothing pending, not now, a
        // refusal) gives it back, or it is held from everyone with nothing to spend it on
        if !self.on_engine.seated(key) {
            self.on_engine.release_reserve(key);
        }
        seated
    }

    /// Decide and measure `key`'s seat, sized for her next lesson.
    async fn seat_on_engine(self: &Arc<Self>, port: &Arc<dyn LearnPort>, key: &BucketKey, rows: &[Counted]) -> Result<(), OnEngineError> {
        let Some(first) = rows.first() else { return Ok(()) };
        let mut phases = Phases::start();
        let store = self.durable.require().map_err(OnEngineError::Journal)?;
        // her policy and name from her oldest pending row alone, one row decoded (card d1e5675c)
        let first = store.arrival_of(first.id).await?;
        phases.mark(Phase::Head);
        let signature = self.signature_of(key, rows).await;
        phases.mark(Phase::Signature);
        let in_flight: Vec<(Uuid, crate::genome::signature::GeneSignature)> = {
            let seats = self.on_engine.seats.lock();
            super::in_place::in_flight(key, seats.iter().map(|(id, held)| (&held.key, id.0, held.signature.as_ref())))
        };
        let pending = Pending { row: &first, examples: rows.iter().map(|row| row.examples as usize).sum(), signature }; // as: a u32 fits a usize on every target this builds for
        let Decision::Learn(Decided { policy, signature, generation }) = decide(self, key, pending, &in_flight).await.map_err(OnEngineError::Journal)? else {
            return Ok(());
        };
        phases.mark(Phase::Decide);
        let checkpointed: HashMap<Uuid, u32> = generation.iter().flat_map(|g| g.positions.iter()).map(|p| (p.submission, p.learned)).collect();
        let Some(position) = next_lesson(rows, &checkpointed, &HashMap::new()) else {
            return Ok(()); // everything pending is already in her newest generation
        };
        let sized_for = lesson_at(&store, position).await?;
        phases.mark(Phase::Size);
        let id = Uuid::new_v4();
        let shadow = match &generation {
            Some(generation) => ShadowSource::Generation(PathBuf::from(&generation.path)),
            None => ShadowSource::Fresh(FreshShadow {
                name: format!("{}-{}.gguf", bucket_stem(key), id.simple()),
                rank: policy.rank,
                alpha: policy.alpha,
                targets: policy.targets,
                top_layers: policy.top_layers,
                seed: id.as_u128() as u32, // a seed per shadow; the truncation keeps 32 of its random bits
            }),
        };
        let from_generation = matches!(shadow, ShadowSource::Generation(_));
        let lesson_messages = sized_for.messages.len();
        let measure = Measure { seat: SeatId(id), mind: PersonaId(key.persona_id), shadow, optimizer: Optimizer::adamw(policy.learning_rate), sized_for };
        let examples_owed = owed::examples_owed(rows, &checkpointed, &HashMap::new()); // nothing progressed yet: a new seat
        let seat = EngineSeat {
            key: key.clone(),
            run: SeatRun::measuring(SeatId(id)),
            signature,
            persona_name: first.persona_name.clone(),
            binding: Arc::from(format!("{}:{}:{}", key.persona_id, key.trait_kind, key.base_model)),
            checkpointed,
            names: VecDeque::new(),
            finishing: false,
            weights: Vec::new(),
            asked_back: give_back::AskedBack::default(),
            examples_owed,
            next: None,
            since: std::time::Instant::now(),
        };
        phases.mark(Phase::Build);
        // registered before it is told: the learner hears on its own worker, and a Measured for a
        // seat nobody holds would be dropped (Codex on #6292). A refused tell takes it back out.
        self.on_engine.seats.lock().insert(SeatId(id), seat);
        phases.mark(Phase::Register);
        if let Err(not_told) = SeatRun::<LeasedMemory>::tell_measure(port.as_ref(), measure) {
            self.on_engine.seats.lock().remove(&SeatId(id));
            return Err(not_told.into());
        }
        phases.mark(Phase::Tell);
        self.say_owed();
        phases.mark(Phase::Owe);
        continuum_base::probe!(class = "training.on_engine.measure_told", persona = %key.persona_id, trait_kind = %key.trait_kind, seat = %id, from_generation, lesson_messages, phases_ms = ?phases.ms(),
            "a seat was measured on the node engine, its seating timed by phase; its Measured (or Refused) is what follows");
        Ok(())
    }

    /// What the engine said about one of the trainer's seats: its next act, under the bucket's gate.
    /// What the act leaves (a candidate to offer, a bucket to seat again) is returned for the trainer's
    /// worker, never taken here: it is long, and this runs on the learner's (card d1e5675c).
    pub(crate) async fn on_said(self: &Arc<Self>, port: &Arc<dyn LearnPort>, said: Said) -> Option<FollowUp> {
        let seat = seat_of(&said);
        let key = self.on_engine.key_of(seat)?; // none: a seat of another trainer, or one already let go
        let waited = std::time::Instant::now();
        let gate = self.submit_gates.acquire(&key).await;
        let gate_wait_ms = waited.elapsed().as_millis() as u64;
        let heard = {
            let mut seats = self.on_engine.seats.lock();
            let held = seats.get_mut(&seat)?;
            held.run.hear(port.as_ref(), &said)
        };
        let (steps, loss) = progress_of(&said);
        continuum_base::probe!(class = "training.on_engine.said", persona = %key.persona_id, seat = %seat.0, said = ?said_kind(&said), why = why_of(&said), steps = ?steps, loss = ?loss, acted = heard.is_some(), gate_wait_ms,
            "the node engine answered one of the trainer's seats");
        let heard = heard?;
        let saved = matches!(heard, Heard::Saved { .. });
        let ended = matches!(heard, Heard::Released | Heard::NotSeated { .. });
        let acted = self.act(port, seat, &key, heard).await;
        self.say_owed(); // what it owes changed with its act (a lesson, a commit, a release)
        drop(gate); // the candidate sweep and a seat again take this bucket's gate themselves
        match acted {
            Err(why) => {
                continuum_base::probe!(class = "training.on_engine.act_failed", persona = %key.persona_id, reason = %why,
                    "the trainer could not take its next act on a seat; the seat is let go and the journal keeps what is not committed");
                self.let_go(port, seat);
                None
            }
            // a generation committed now is offered as a candidate now, not a pass later
            Ok(()) if saved => Some(FollowUp::Sweep(key)),
            // let go while its hold was being armed: what was freed meanwhile woke nothing for it
            Ok(()) if ended && self.on_engine.covered(&key) => Some(FollowUp::Seat(key)),
            Ok(()) => None,
        }
    }

    /// The authority's board changed: each bucket held for memory whose ask it granted, while the
    /// engine serves `served_model`, is seated again by the trainer. Only held buckets are read; with
    /// none held this is nothing.
    pub(crate) fn on_free(&self, served_model: &str) -> Vec<FollowUp> {
        self.on_engine.freed().into_iter().filter(|key| key.base_model == served_model).map(FollowUp::Seat).collect()
    }

    async fn act(self: &Arc<Self>, port: &Arc<dyn LearnPort>, seat: SeatId, key: &BucketKey, heard: Heard) -> Result<(), OnEngineError> {
        let consumer = format!("learning:{}:{}", key.persona_id, key.trait_kind);
        match heard {
            Heard::LeaseWeights(rows) => {
                let weights = self.lease(port, key, Lease { consumer: &consumer, rows: &rows, whole: &rows })?;
                self.yield_when_asked(seat, &weights)?;
                // the engine sizes the lesson a seat learns within what the device governs beside them
                let fit = fit_beside(&free_now(&self.authority), &rows);
                self.with_seat(seat, |held| {
                    held.weights = rows; // seating it again leases them again
                    held.run.load(port.as_ref(), weights, fit)
                });
                Ok(())
            }
            Heard::LeaseSeat { optimizer, workspace, rows } => {
                self.with_seat(seat, |held| {
                    held.next = rows; // her next lesson as the engine sized it
                    Ok(())
                });
                // The whole seat: what seating it again takes, its weights included.
                let whole = self.whole_seat(seat, &optimizer, &workspace);
                let optimizer = self.lease(port, key, Lease { consumer: &consumer, rows: &optimizer, whole: &whole })?;
                self.yield_when_asked(seat, &optimizer)?;
                let workspace = self.lease(port, key, Lease { consumer: &format!("{consumer}:workspace"), rows: &workspace, whole: &whole })?;
                self.yield_when_asked(seat, &workspace)?;
                let view = optimizer.view().map_err(OnEngineError::Coverage)?;
                self.with_seat(seat, |held| held.run.seat(port.as_ref(), Box::new(view), optimizer, workspace));
                self.on_engine.release_reserve(key); // every stage is leased: what is left of the reserve goes back
                Ok(())
            }
            Heard::Seated { .. } | Heard::Learned { .. } | Heard::Again { .. } => self.advance(port, seat, key).await,
            Heard::TooLong { .. } | Heard::StepRefused { .. } => {
                self.with_seat(seat, |held| {
                    held.finishing = true;
                    Ok(())
                });
                self.advance(port, seat, key).await
            }
            Heard::Saved { into, steps, positions } => {
                self.commit(seat, key, Saved { into, steps, positions }).await?;
                self.advance(port, seat, key).await
            }
            Heard::Unsaved { why, .. } => {
                // nothing advances: the next seat learns again from the newest committed generation
                continuum_base::probe!(class = "training.on_engine.unsaved", persona = %key.persona_id, reason = %why,
                    "a checkpoint was refused; the seat is let go and its lessons are learned again from the last committed generation");
                self.let_go(port, seat);
                Ok(())
            }
            Heard::Uncertain => {
                continuum_base::probe!(class = "training.on_engine.uncertain", persona = %key.persona_id,
                    "a lesson's effects are uncertain: nothing it learned is kept, and the seat is let go");
                Ok(())
            }
            Heard::Quarantined { why } | Heard::Kept { why } => {
                continuum_base::probe!(class = "training.on_engine.held", persona = %key.persona_id, reason = %why,
                    "the engine holds the seat; every lease stays held until it says Released");
                Ok(())
            }
            Heard::NotSeated { .. } | Heard::Released => {
                self.on_engine.seats.lock().remove(&seat); // its guards drop here: nothing of it remains on the engine
                self.on_engine.release_reserve(key); // and what is left of its reserve
                Ok(())
            }
        }
    }

    /// Tell the seat's next act: its next lesson, or (none waiting, or finishing) a checkpoint of
    /// what it learned, or its release.
    async fn advance(self: &Arc<Self>, port: &Arc<dyn LearnPort>, seat: SeatId, key: &BucketKey) -> Result<(), OnEngineError> {
        // counted, never decoded: on the 5090 each lesson read and freed her whole history (card d1e5675c)
        let rows = self.counts_of(key).await?;
        let next = {
            let mut seats = self.on_engine.seats.lock();
            let Some(held) = seats.get_mut(&seat) else { return Ok(()) };
            held.examples_owed = owed::examples_owed(&rows, &held.checkpointed, held.run.progress());
            if !held.run.ready() {
                return Ok(());
            }
            match held.finishing || held.asked_back.asked() {
                true => None,
                false => next_lesson(&rows, &held.checkpointed, held.run.progress()),
            }
        };
        // the one example it names, read outside the seats lock. The bucket's gate is held across
        // both (its caller takes it), so no other answer of this bucket moves the seat in between.
        let lesson = match next {
            Some(position) => {
                let store = self.durable.require().map_err(OnEngineError::Journal)?;
                Some((position, lesson_at(&store, position).await?))
            }
            None => None,
        };
        let mut seats = self.on_engine.seats.lock();
        let Some(held) = seats.get_mut(&seat) else { return Ok(()) };
        // asked to give back while the lesson was read: it finishes instead, as if asked before
        if !held.finishing && !held.asked_back.asked() {
            if let Some((position, lesson)) = lesson {
                let workspace = held.run.workspace().ok_or(OnEngineError::Unanswered("a ready seat holds no workspace lease"))?;
                let admission = workspace.view().map_err(OnEngineError::Coverage)?;
                held.run.learn(port.as_ref(), position, lesson, Box::new(admission))?;
                return Ok(());
            }
        }
        if held.run.progress().is_empty() {
            return Ok(held.run.release(port.as_ref())?);
        }
        checkpoint(port.as_ref(), held)
    }

    /// At save: checkpoint every ready seat that holds progress (its Saved commits it, as any other).
    /// A seat with a lesson in flight is left to it: its progress is learned again from the journal
    /// if the core stops before the next checkpoint. The count told.
    pub(crate) fn checkpoint_seats(&self) -> Result<u32, OnEngineError> {
        let Some(port) = port() else { return Ok(0) };
        let mut told = 0u32;
        for held in self.on_engine.seats.lock().values_mut() {
            if held.run.ready() && !held.run.progress().is_empty() {
                checkpoint(port.as_ref(), held)?;
                told = told.saturating_add(1);
            }
        }
        Ok(told)
    }

    /// At shutdown: let every seat go. The in-process engine stops with the core; what a seat learned
    /// since its newest committed generation is learned again from the journal. The count.
    pub(crate) fn let_go_seats(&self) -> usize {
        let Some(port) = port() else { return 0 };
        let seats: Vec<SeatId> = self.on_engine.seats.lock().keys().copied().collect();
        for seat in &seats {
            self.let_go(&port, *seat);
        }
        seats.len()
    }

    /// Whether a seat on the node engine holds bucket `key`: rekey refuses to move a seated source.
    pub(crate) fn seated_on_engine(&self, key: &BucketKey) -> bool {
        self.on_engine.seated(key)
    }

    /// Commit a saved generation: its manifest, then the durable checkpoint that moves the journal's
    /// cursor to its positions.
    async fn commit(self: &Arc<Self>, seat: SeatId, key: &BucketKey, saved: Saved) -> Result<(), OnEngineError> {
        let Saved { into, steps, positions } = saved;
        let (name, binding, persona_name, signature, mut consumed) = {
            let mut seats = self.on_engine.seats.lock();
            let held = seats.get_mut(&seat).ok_or(OnEngineError::Unanswered("the seat left before its generation was committed"))?;
            let name = held.names.pop_front().ok_or(OnEngineError::Unanswered("a Saved answers no Checkpoint told"))?;
            (name, Arc::clone(&held.binding), held.persona_name.clone(), held.signature.clone(), held.checkpointed.clone())
        };
        // the manifest records what the generation consumed (ONE-RESIDENT 9.2): each submission and how
        // many of its examples, merged over the generations before it
        for position in &positions {
            let held = consumed.entry(position.submission).or_default();
            *held = (*held).max(position.learned);
        }
        let consumed: Vec<Position> = consumed.into_iter().map(|(submission, learned)| Position { submission, learned }).collect();
        let provenance = serde_json::json!({
            "personaId": key.persona_id, "traitKind": key.trait_kind, "baseModel": key.base_model,
            "consumed": consumed, "signature": signature.as_ref().map(|sig| sig.corpus.content_hash.clone()),
        });
        let manifest = serde_json::json!({ "binding": &*binding, "steps": steps, "provenance": provenance });
        let bytes = serde_json::to_vec_pretty(&manifest).map_err(OnEngineError::Manifest)?;
        let relative = PathBuf::from(format!("{name}.checkpoint")).join("manifest.json");
        let train = ClassWriter::of(ENGINE_TRAIN).map_err(OnEngineError::TrainDir)?;
        tokio::task::spawn_blocking(move || train.publish_new(&relative, &bytes)).await.map_err(OnEngineError::Write)?.map_err(OnEngineError::TrainDir)?;
        let generation = Generation {
            name,
            path: into.to_string_lossy().into_owned(), // a path the trainer built under the continuum home
            binding: binding.to_string(),
            steps: i64::try_from(steps).map_err(|_| OnEngineError::Unanswered("a generation's step count past i64"))?,
            provenance,
            persona_name,
            signature,
        };
        let store = self.durable.require().map_err(OnEngineError::Journal)?;
        store.commit_checkpoint(key, generation, &positions).await.map_err(OnEngineError::Journal)?;
        if let Some(held) = self.on_engine.seats.lock().get_mut(&seat) {
            for position in &positions {
                let floor = held.checkpointed.entry(position.submission).or_default();
                *floor = (*floor).max(position.learned);
            }
        }
        Ok(())
    }

    /// Lease `rows` for `consumer`, carved from `key`'s reserve where it holds one. A refusal for memory
    /// asks the authority for the WHOLE seat (`whole`, every row seating it again leases) of the short
    /// kind and holds `key` until it is granted: a hold for the refused lease alone was covered by the
    /// seat's own weights freed by its release, and it was measured, loaded and refused again in a loop
    /// (the 5090, 2026-11-26: 97 Loaded, 0 Seated in four minutes); a hold that only watched what is free
    /// was never woken while serving held the device.
    fn lease(&self, port: &Arc<dyn LearnPort>, key: &BucketKey, ask: Lease<'_>) -> Result<LeasedMemory, OnEngineError> {
        let Lease { consumer, rows, whole } = ask;
        let leased_now = {
            let reserves = self.on_engine.reserves.lock();
            let reserve = reserves.get(key).map_or(&[][..], Vec::as_slice); // map_or: no reserve is an empty one
            lease_memory_within(&self.authority, reserve, consumer, rows.iter().cloned())
        };
        leased_now.map_err(|refusal| {
            if let SeatRefusal::AwaitingMemory(TrainingWait::Capacity { kind, needed, .. }) = &refusal {
                // a kind the rows do not name, or rows past a byte count, ask for the refused lease alone
                let seat = leased(whole, *kind).map_or(*needed, |seat| seat.max(*needed));
                let whole_seat = |kind, bytes| crate::inference::training_admission::ask_training_memory(&self.authority.resources, consumer, kind, bytes);
                let capacity = free_now(&self.authority).kind(*kind).map(|governed| governed.capacity);
                self.on_engine.hold_off(key, port, Short { kind: *kind, needed: seat, capacity }, whole_seat);
            }
            OnEngineError::Lease(refusal)
        })
    }

    /// `key`'s competence signature over the rows `counted`: the one minted for these same rows if there
    /// is one, otherwise minted now (every example embedded) and kept for the next pass. Minting is the
    /// one read of her whole pending history left (card d1e5675c), and only when her rows changed; the
    /// rows are freed on a blocking thread, so no runtime worker waits on the free (the 5090: 340.4 s for
    /// Kimi's 784). A signature built from per-submission parts replaces it (step 2 of that card).
    async fn signature_of(&self, key: &BucketKey, counted: &[Counted]) -> Option<crate::genome::signature::GeneSignature> {
        let seen = RowsSeen::of(counted);
        if let Some((minted_from, signature)) = self.on_engine.signatures.lock().get(key) {
            if *minted_from == seen {
                return Some(signature.clone());
            }
        }
        let rows = match self.durable.require() {
            Ok(store) => store.arrivals_in(key).await,
            Err(why) => Err(why),
        };
        let rows = match rows {
            Ok(rows) => rows,
            Err(why) => {
                continuum_base::probe!(class = "training.on_engine.signature_unread", persona = %key.persona_id, reason = %why, "her rows could not be read to mint a signature; the competence routes without one this pass");
                return None;
            }
        };
        let minted = super::competence::mint_signature(key.persona_id, format!("inline:{}", key.trait_kind), &key.trait_kind, rows.iter().flat_map(|row| row.examples.iter())).await;
        drop(tokio::task::spawn_blocking(move || drop(rows))); // freed off the runtime's workers; nothing waits on it
        let minted = minted?;
        self.on_engine.signatures.lock().insert(key.clone(), (seen, minted.clone()));
        Some(minted)
    }

    /// `key`'s pending rows as counting reads them: no batch decoded (card d1e5675c).
    async fn counts_of(&self, key: &BucketKey) -> Result<Vec<Counted>, OnEngineError> {
        let store = self.durable.require().map_err(OnEngineError::Journal)?;
        Ok(store.pending_counts(key).await?)
    }

    /// The whole seat `seat` asks for: the weights the engine measured for it, its `optimizer` and its
    /// `workspace`, every row seating it again leases.
    fn whole_seat(&self, seat: SeatId, optimizer: &[MemoryRow], workspace: &[MemoryRow]) -> Vec<MemoryRow> {
        let seats = self.on_engine.seats.lock();
        let weights: &[MemoryRow] = match seats.get(&seat) {
            Some(held) => &held.weights,
            None => &[], // a seat already let go has no weights left to lease again
        };
        weights.iter().chain(optimizer).chain(workspace).cloned().collect()
    }

    /// Make `leased` give back when a higher wait asks, answered by `seat`: it then finishes and is released.
    fn yield_when_asked(&self, seat: SeatId, leased: &LeasedMemory) -> Result<(), OnEngineError> {
        let asked = self.on_engine.seats.lock().get(&seat).map(|held| &held.asked_back).cloned();
        let Some(asked) = asked else { return Ok(()) }; // let go already: its guards drop with `leased`
        give_back::yield_when_asked(&self.authority, leased, &asked).map_err(OnEngineError::Elastic)
    }

    /// Run `act` on `seat` if the trainer still holds it; a command no engine took is probed.
    fn with_seat(&self, seat: SeatId, act: impl FnOnce(&mut EngineSeat) -> Result<(), NotTold>) {
        let mut seats = self.on_engine.seats.lock();
        let Some(held) = seats.get_mut(&seat) else { return };
        if let Err(not) = act(held) {
            continuum_base::probe!(class = "training.on_engine.not_told", persona = %held.key.persona_id, not = %not,
                "a seat command was not told; the seat keeps what it holds and the next pass or Said tries again");
        }
    }

    /// Release `seat`: its leases go only on the Released that answers.
    fn let_go(&self, port: &Arc<dyn LearnPort>, seat: SeatId) {
        self.with_seat(seat, |held| held.run.release(port.as_ref()));
    }
}

mod follow_up;
mod give_back;
mod owed;
mod phases;

#[cfg(test)]
mod tests;
