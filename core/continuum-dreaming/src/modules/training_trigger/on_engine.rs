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

use super::durable::{Arrival, Generation, Position};
use super::in_place::{bucket_stem, decide, start, Decided, Decision, Pending};
use super::seat_run::{Heard, NotTold, SeatRun};
use super::{BucketKey, TrainingTriggerState};
use crate::genome::learner::example::learn_messages;
use crate::genome::learner::seat::{lease_memory, Authority, LeasedMemory, SeatRefusal};
use crate::inference::learning::{LearnPort, Lesson, Measure, MemoryRow, Optimizer, PersonaId, Said, SeatId, ShadowSource};
use crate::inference::training_admission::TrainingWait;
use crate::resources::{Free, ResourceKind};
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
    /// Short of memory: seated again once `needed` bytes of `kind` are free on the authority's board.
    Memory { kind: ResourceKind, needed: u64 },
}

/// The seats the trainer holds on the node engine, and the buckets it holds off.
#[derive(Default)]
pub(crate) struct OnEngine {
    seats: parking_lot::Mutex<HashMap<SeatId, EngineSeat>>,
    held_off: parking_lot::Mutex<HashMap<BucketKey, HeldOff>>,
    /// Each bucket's minted competence signature, with the rows it was minted from: minting embeds
    /// every example, so it is minted again only when the bucket's rows change (Codex on the 5090
    /// replay, 2026-11-25: a bucket decide() answered NotNow was re-embedded on every wake).
    signatures: parking_lot::Mutex<HashMap<BucketKey, (RowsSeen, crate::genome::signature::GeneSignature)>>,
}

impl OnEngine {
    fn seated(&self, key: &BucketKey) -> bool {
        self.seats.lock().values().any(|seat| seat.key == *key)
    }

    fn key_of(&self, seat: SeatId) -> Option<BucketKey> {
        self.seats.lock().get(&seat).map(|held| held.key.clone())
    }

    /// Whether `key` is still held off on `port`, with `free` what the authority shows: a defect
    /// while the same engine serves, a memory wait while too little is free. A hold that no longer
    /// applies is dropped.
    fn holds(&self, key: &BucketKey, port: &Arc<dyn LearnPort>, free: &Free) -> bool {
        let mut held = self.held_off.lock();
        let still = match held.get(key) {
            None => return false,
            Some(HeldOff::Defect { engine }) => std::ptr::addr_eq(engine.as_ptr(), Arc::as_ptr(port)),
            Some(HeldOff::Memory { kind, needed }) => short(free, *kind, *needed),
        };
        if !still {
            held.remove(key);
        }
        still
    }

    /// Whether any bucket waits for memory.
    fn waits_for_memory(&self) -> bool {
        self.held_off.lock().values().any(|hold| matches!(hold, HeldOff::Memory { .. }))
    }

    /// The buckets held for memory that `free` now covers and that hold no seat, their holds
    /// dropped: what a release wakes. A bucket whose seat is still being let go is left to its
    /// Released ([`Self::covered`]).
    fn freed(&self, free: &Free) -> Vec<BucketKey> {
        let seats = self.seats.lock();
        let mut held = self.held_off.lock();
        let ready: Vec<BucketKey> = held
            .iter()
            .filter(|(key, hold)| matches!(hold, HeldOff::Memory { kind, needed } if !short(free, *kind, *needed)) && !seats.values().any(|seat| seat.key == **key))
            .map(|(key, _)| key.clone())
            .collect();
        for key in &ready {
            held.remove(key);
        }
        ready
    }

    /// Whether `key`, its seat just let go, is held for memory that `free` now covers (the hold
    /// dropped). A release said while that seat was still being let go woke nothing for it, so its
    /// Released seats it again.
    fn covered(&self, key: &BucketKey, free: &Free) -> bool {
        let mut held = self.held_off.lock();
        let covered = matches!(held.get(key), Some(HeldOff::Memory { kind, needed }) if !short(free, *kind, *needed));
        if covered {
            held.remove(key);
        }
        covered
    }

    /// Hold `key` off after the authority refused `needed` bytes of `kind` for `rows`.
    fn hold_off(&self, key: &BucketKey, port: &Arc<dyn LearnPort>, free: &Free, wait: Short, rows: &[MemoryRow]) {
        let Short { kind, needed } = wait;
        let capacity = free.kind(kind).map(|governed| governed.capacity);
        let held = match capacity {
            Some(capacity) if needed > capacity => {
                continuum_base::probe!(class = "training.on_engine.measure_defect", persona = %key.persona_id, kind = ?kind, needed, capacity, rows = ?rows,
                    "the engine measured more than the whole governed device holds: a measurement defect, not a wait; this bucket is not seated again on this engine");
                HeldOff::Defect { engine: Arc::downgrade(port) }
            }
            _ => {
                continuum_base::probe!(class = "training.on_engine.awaiting_memory", persona = %key.persona_id, kind = ?kind, needed, capacity = ?capacity,
                    "short of memory: this bucket is not measured again until the authority says that much is free");
                HeldOff::Memory { kind, needed }
            }
        };
        self.held_off.lock().insert(key.clone(), held);
    }
}

/// The rows a signature was minted from: each submission and how many examples it held. A new
/// submission or more examples in one makes it a different set.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RowsSeen(Vec<(Uuid, usize)>);

impl RowsSeen {
    fn of(rows: &[Arrival]) -> Self {
        Self(rows.iter().map(|row| (row.id, row.examples.len())).collect())
    }
}

/// What the authority refused: `needed` bytes of `kind`.
struct Short {
    kind: ResourceKind,
    needed: u64,
}

/// Whether `free` shows less than `needed` bytes of `kind` free (a kind it does not govern has
/// nothing free yet).
fn short(free: &Free, kind: ResourceKind, needed: u64) -> bool {
    free.kind(kind).is_none_or(|governed| governed.available < needed)
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
        },
        Said::Generation { outcome, .. } => match outcome {
            GenerationOutcome::Saved { .. } => "saved",
            GenerationOutcome::Refused { .. } => "generation_refused",
        },
    }
}

/// The seat a `said` is about.
fn seat_of(said: &Said) -> SeatId {
    match said {
        Said::Seat { seat, .. } | Said::Step { seat, .. } | Said::Generation { seat, .. } => *seat,
    }
}

/// PURE: the next lesson of `rows`, past what the journal, the newest generation (`checkpointed`)
/// and the seat's volatile `progress` hold.
fn next_lesson(rows: &[Arrival], checkpointed: &HashMap<Uuid, u32>, progress: &HashMap<Uuid, u32>) -> Option<(Position, Lesson)> {
    rows.iter().find_map(|row| {
        let learned = start(row.learned, checkpointed.get(&row.id).copied(), progress.get(&row.id).copied());
        let example = row.examples.get(usize::try_from(learned).ok()?)?;
        Some((Position { submission: row.id, learned }, learn_messages(example)))
    })
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
    /// A Said heard on the node bus, for the trainer's seats on the node engine.
    pub(crate) async fn heard_from_engine(self: &Arc<Self>, said: Said) {
        match port() {
            Some(port) => self.on_said(&port, said).await,
            None => continuum_base::probe!(class = "training.on_engine.no_port", said = ?said, "a Said arrived with no node engine installed: nothing is told for it"),
        }
    }

    /// What is free, said by the authority, for the buckets held for memory on the node engine.
    pub(crate) async fn heard_free(self: &Arc<Self>, free: &Free) {
        if !self.on_engine.waits_for_memory() {
            return; // nothing held: the engine is not asked what it serves
        }
        let (Some(port), Some(served)) = (port(), served_model().await) else { return };
        self.on_free(&port, free, &served).await;
    }

    /// Seat every bucket whose base `served_model` serves and that has a lesson waiting, through
    /// `port`. A bucket already seated learns on from what the engine says.
    pub(crate) async fn learn_on_engine(self: &Arc<Self>, port: &Arc<dyn LearnPort>, served_model: &str) -> Result<(), OnEngineError> {
        let store = self.durable.require().map_err(OnEngineError::Journal)?;
        for key in super::in_place::eligible_keys(&store, served_model).await.map_err(OnEngineError::Journal)? {
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
        let _gate = self.submit_gates.acquire(key).await;
        if self.on_engine.seated(key) || self.on_engine.holds(key, port, &free_now(&self.authority)) {
            return Ok(());
        }
        let rows = self.rows_of(key).await?;
        self.seat_on_engine(port, key, &rows).await
    }

    /// Decide and measure `key`'s seat, sized for her next lesson.
    async fn seat_on_engine(self: &Arc<Self>, port: &Arc<dyn LearnPort>, key: &BucketKey, rows: &[Arrival]) -> Result<(), OnEngineError> {
        let Some(first) = rows.first() else { return Ok(()) };
        let signature = self.signature_of(key, rows).await;
        let in_flight: Vec<(Uuid, crate::genome::signature::GeneSignature)> = {
            let seats = self.on_engine.seats.lock();
            super::in_place::in_flight(key, seats.iter().map(|(id, held)| (&held.key, id.0, held.signature.as_ref())))
        };
        let pending = Pending { row: first, examples: rows.iter().map(|row| row.examples.len()).sum(), signature };
        let Decision::Learn(Decided { policy, signature, generation }) = decide(self, key, pending, &in_flight).await.map_err(OnEngineError::Journal)? else {
            return Ok(());
        };
        let checkpointed: HashMap<Uuid, u32> = generation.iter().flat_map(|g| g.positions.iter()).map(|p| (p.submission, p.learned)).collect();
        let Some((_, sized_for)) = next_lesson(rows, &checkpointed, &HashMap::new()) else {
            return Ok(()); // everything pending is already in her newest generation
        };
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
        let run = SeatRun::measure(port.as_ref(), measure)?;
        continuum_base::probe!(class = "training.on_engine.measure_told", persona = %key.persona_id, trait_kind = %key.trait_kind, seat = %id, from_generation, lesson_messages,
            "a seat was measured on the node engine; its Measured (or Refused) is what follows");
        let seat = EngineSeat {
            key: key.clone(),
            run,
            signature,
            persona_name: first.persona_name.clone(),
            binding: Arc::from(format!("{}:{}:{}", key.persona_id, key.trait_kind, key.base_model)),
            checkpointed,
            names: VecDeque::new(),
            finishing: false,
        };
        self.on_engine.seats.lock().insert(SeatId(id), seat);
        Ok(())
    }

    /// What the engine said about one of the trainer's seats: its next act, under the bucket's gate.
    pub(crate) async fn on_said(self: &Arc<Self>, port: &Arc<dyn LearnPort>, said: Said) {
        let seat = seat_of(&said);
        let Some(key) = self.on_engine.key_of(seat) else {
            return; // a seat of another trainer, or one already let go
        };
        let gate = self.submit_gates.acquire(&key).await;
        let heard = {
            let mut seats = self.on_engine.seats.lock();
            let Some(held) = seats.get_mut(&seat) else { return };
            held.run.hear(port.as_ref(), &said)
        };
        continuum_base::probe!(class = "training.on_engine.said", persona = %key.persona_id, seat = %seat.0, said = ?said_kind(&said), acted = heard.is_some(),
            "the node engine answered one of the trainer's seats");
        let Some(heard) = heard else { return };
        let saved = matches!(heard, Heard::Saved { .. });
        let ended = matches!(heard, Heard::Released | Heard::NotSeated { .. });
        let acted = self.act(port, seat, &key, heard).await;
        drop(gate); // the candidate sweep and a seat again take this bucket's gate themselves
        match acted {
            Err(why) => {
                continuum_base::probe!(class = "training.on_engine.act_failed", persona = %key.persona_id, reason = %why,
                    "the trainer could not take its next act on a seat; the seat is let go and the journal keeps what is not committed");
                self.let_go(port, seat);
            }
            // a generation committed now is offered as a candidate now, not a pass later
            Ok(()) if saved => {
                if let Err(why) = self.sweep_bucket(&key, &key.base_model).await {
                    continuum_base::probe!(class = "training.in_place.candidate_sweep_failed", persona = %key.persona_id, reason = %why,
                        "a committed generation's candidate could not be swept now; the next pass takes it");
                }
            }
            // let go while its hold was being armed: what was freed meanwhile woke nothing for it
            Ok(()) if ended && self.on_engine.covered(&key, &free_now(&self.authority)) => self.seat_again(port, &key).await,
            Ok(()) => {}
        }
    }

    /// The authority said what is free: seat each bucket held for memory that it now covers, on
    /// `port` while it serves `served_model`. Only held buckets are read; with none held this is
    /// nothing.
    pub(crate) async fn on_free(self: &Arc<Self>, port: &Arc<dyn LearnPort>, free: &Free, served_model: &str) {
        for key in self.on_engine.freed(free) {
            if key.base_model == served_model {
                self.seat_again(port, &key).await;
            }
        }
    }

    /// Seat `key` again now that what it waited for is free; a failure is probed and the next wake
    /// tries again.
    async fn seat_again(self: &Arc<Self>, port: &Arc<dyn LearnPort>, key: &BucketKey) {
        if let Err(why) = self.seat_bucket(port, key).await {
            continuum_base::probe!(class = "training.on_engine.seat_failed", persona = %key.persona_id, reason = %why,
                "a bucket whose memory was freed was not seated; its examples stay pending and the next wake tries again");
        }
    }

    async fn act(self: &Arc<Self>, port: &Arc<dyn LearnPort>, seat: SeatId, key: &BucketKey, heard: Heard) -> Result<(), OnEngineError> {
        let consumer = format!("learning:{}:{}", key.persona_id, key.trait_kind);
        match heard {
            Heard::LeaseWeights(rows) => {
                let leased = self.lease(port, key, &consumer, rows)?;
                self.with_seat(seat, |held| held.run.load(port.as_ref(), leased));
                Ok(())
            }
            Heard::LeaseSeat { optimizer, workspace, .. } => {
                let optimizer = self.lease(port, key, &consumer, optimizer)?;
                let workspace = self.lease(port, key, &format!("{consumer}:workspace"), workspace)?;
                let view = optimizer.view().map_err(OnEngineError::Coverage)?;
                self.with_seat(seat, |held| held.run.seat(port.as_ref(), Box::new(view), optimizer, workspace));
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
                Ok(())
            }
        }
    }

    /// Tell the seat's next act: its next lesson, or (none waiting, or finishing) a checkpoint of
    /// what it learned, or its release.
    async fn advance(self: &Arc<Self>, port: &Arc<dyn LearnPort>, seat: SeatId, key: &BucketKey) -> Result<(), OnEngineError> {
        let rows = self.rows_of(key).await?;
        let mut seats = self.on_engine.seats.lock();
        let Some(held) = seats.get_mut(&seat) else { return Ok(()) };
        if !held.run.ready() {
            return Ok(());
        }
        if !held.finishing {
            if let Some((position, lesson)) = next_lesson(&rows, &held.checkpointed, held.run.progress()) {
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

    /// Lease `rows` for `consumer`. A refusal for memory holds `key` off: until that much is free, or
    /// (a need past the whole device) for as long as this engine serves.
    fn lease(&self, port: &Arc<dyn LearnPort>, key: &BucketKey, consumer: &str, rows: Vec<MemoryRow>) -> Result<LeasedMemory, OnEngineError> {
        let measured = rows.clone(); // what the probe names if the authority refuses it
        lease_memory(&self.authority, consumer, rows).map_err(|refusal| {
            if let SeatRefusal::AwaitingMemory(TrainingWait::Capacity { kind, needed, .. }) = &refusal {
                self.on_engine.hold_off(key, port, &free_now(&self.authority), Short { kind: *kind, needed: *needed }, &measured);
            }
            OnEngineError::Lease(refusal)
        })
    }

    /// `key`'s competence signature over `rows`: the one minted for these same rows if there is one,
    /// otherwise minted now (every example embedded) and kept for the next pass.
    async fn signature_of(&self, key: &BucketKey, rows: &[Arrival]) -> Option<crate::genome::signature::GeneSignature> {
        let seen = RowsSeen::of(rows);
        if let Some((minted_from, signature)) = self.on_engine.signatures.lock().get(key) {
            if *minted_from == seen {
                return Some(signature.clone());
            }
        }
        let minted = super::competence::mint_signature(key.persona_id, format!("inline:{}", key.trait_kind), &key.trait_kind, rows.iter().flat_map(|row| row.examples.iter())).await?;
        self.on_engine.signatures.lock().insert(key.clone(), (seen, minted.clone()));
        Some(minted)
    }

    /// `key`'s pending rows.
    async fn rows_of(&self, key: &BucketKey) -> Result<Vec<Arrival>, OnEngineError> {
        let store = self.durable.require().map_err(OnEngineError::Journal)?;
        store.arrivals_in(key).await.map_err(OnEngineError::Journal)
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

#[cfg(test)]
mod tests;
