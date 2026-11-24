//! One seat's life on the persona engine, as the trainer drives it through [`LearnPort`]
//! (IN-PROCESS-LEARNING.md L3, card 28aac827).
//!
//! The trainer TELLS and never awaits: each call here queues one [`Learning`] and returns. What the
//! engine made of it arrives later on [`LEARNING_SAID`], and [`SeatRun::hear`] turns each [`Said`]
//! about this seat into what the trainer must do next ([`Heard`]). The order is the engine's: Measure,
//! Load once the trainer leased the measured weights, Seat once it leased the loaded shadow's
//! optimizer and workspace, then Learn and Checkpoint at the trainer's cadence, then Release.
//!
//! The trainer owns every lease guard and the commands carry only non-owning views of them. The seat
//! holds its guards until the engine proves its memory released ([`Heard::Released`]) or refuses with
//! nothing held; a quarantined seat keeps them (Codex on #6254).
//!
//! What it learned is volatile until a generation holding it is saved: [`SeatRun::checkpoint`]
//! snapshots the positions learned so far, and the [`Heard::Saved`] that answers it hands them back,
//! so the journal's cursor moves only with a saved generation. A checkpoint the engine refused hands
//! them back as [`Heard::Unsaved`], still volatile.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::Arc;

use uuid::Uuid;

use super::durable::Position;
use crate::inference::learning::{
    GenerationOutcome, Learn, LearnPort, Learning, Lesson, Measure, MemoryRow, OptimizerLease, OptimizerStart, ReplyAdmission, Said,
    Seat, SeatId, SeatState, StepId, StepOutcome, Unavailable,
};

/// Where the seat stands, as the engine last said it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    /// Measure was told; its weights have not come back.
    Measuring,
    /// Its weights were measured: the trainer leases them, then loads.
    Measured,
    /// Load was told under the weights lease.
    Loading,
    /// Loaded: the trainer leases its optimizer and workspace, then seats.
    Loaded,
    /// Seat was told under those leases.
    Seating,
    Seated,
    /// Release was told.
    Releasing,
    /// The engine holds it until it proves its memory released: its guards stay held.
    Quarantined,
    /// Released, or refused with nothing held: its guards are dropped and nothing more is told.
    Gone,
}

/// What the trainer does after the engine said something about this seat.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Heard {
    /// Lease the shadow's `weights` off the engine thread, then [`SeatRun::load`] under it.
    LeaseWeights(Vec<MemoryRow>),
    /// Lease its `optimizer` and a `workspace` for lessons up to `reply_rows` tokens, then
    /// [`SeatRun::seat`] under them.
    LeaseSeat { optimizer: Vec<MemoryRow>, workspace: Vec<MemoryRow>, reply_rows: std::num::NonZeroU32 },
    /// Seated: lessons may be learned. `optimizer` says whether its moments came back.
    Seated { optimizer: OptimizerStart },
    /// `position`'s lesson committed `steps`: it is learned, volatile until a generation holds it.
    Learned { position: Position, steps: u64, loss: Option<f64> },
    /// `position`'s lesson committed nothing: it may be told again.
    Again { position: Position },
    /// A lesson was refused (the shadow is as it was): the bucket defers.
    StepRefused { position: Position, why: Arc<str> },
    /// The shadow's effects are uncertain: Release was told and its volatile progress is dropped.
    Uncertain,
    /// A generation was written into `into` holding `positions` and `steps` optimizer steps: the
    /// trainer writes its manifest and commits it, and the journal's cursor moves to them.
    Saved { into: PathBuf, steps: u64, positions: Vec<Position> },
    /// A checkpoint was refused: `positions` stay volatile and a later checkpoint publishes them.
    Unsaved { why: Arc<str>, positions: Vec<Position> },
    /// Refused: the engine holds nothing for it, and its guards are dropped.
    NotSeated { why: Arc<str> },
    /// Quarantined: the engine holds it until it proves its memory released, so every guard stays
    /// held until it says [`Heard::Released`] (Codex on #6254).
    Quarantined { why: Arc<str> },
    /// Released: native code freed its memory and its guards are dropped.
    Released,
}

/// Why the trainer could not tell this seat something now.
#[derive(Debug)]
pub(crate) enum NotTold {
    /// No engine took it: the command came back and its guards dropped with it.
    NoEngine,
    /// The seat is not where this command belongs (not seated, or a lesson is already in flight).
    OutOfOrder(&'static str),
}

impl From<Unavailable> for NotTold {
    fn from(_returned: Unavailable) -> Self {
        NotTold::NoEngine // the returned command, and every guard in it, drops here
    }
}

/// One seat the trainer drives.
pub(crate) struct SeatRun {
    seat: SeatId,
    phase: Phase,
    /// The lesson told and not yet answered: one at a time per seat.
    in_flight: Option<(StepId, Position)>,
    /// What each submission has learned since the newest saved generation: volatile.
    progress: HashMap<Uuid, u32>,
    /// The positions each told Checkpoint holds, oldest first: a Generation answers the oldest.
    checkpoints: VecDeque<Vec<Position>>,
    /// The leases the engine's views draw on: the weights' from Load, the optimizer's and the
    /// workspace's from Seat. Dropped only when the engine proves nothing of the seat remains.
    guards: Vec<Box<dyn Send>>,
}

impl SeatRun {
    /// Measure the seat `measure` names: the first thing told for it.
    pub(crate) fn measure(port: &dyn LearnPort, measure: Measure) -> Result<Self, NotTold> {
        let seat = measure.seat;
        port.tell(Learning::Measure(measure))?;
        Ok(Self { seat, phase: Phase::Measuring, in_flight: None, progress: HashMap::new(), checkpoints: VecDeque::new(), guards: Vec::new() })
    }

    pub(crate) fn id(&self) -> SeatId {
        self.seat
    }

    /// Whether a lesson can be told now: seated, none in flight.
    pub(crate) fn ready(&self) -> bool {
        self.phase == Phase::Seated && self.in_flight.is_none()
    }

    /// What each submission has learned since the newest saved generation.
    pub(crate) fn progress(&self) -> &HashMap<Uuid, u32> {
        &self.progress
    }

    /// Load the measured shadow under `weights`, the lease the trainer took for its measured weights.
    /// Not told, the guard drops here: nothing was allocated under it.
    pub(crate) fn load(&mut self, port: &dyn LearnPort, weights: Box<dyn Send>) -> Result<(), NotTold> {
        if self.phase != Phase::Measured {
            return Err(NotTold::OutOfOrder("load before its weights were measured"));
        }
        port.tell(Learning::Load { seat: self.seat })?;
        self.guards.push(weights);
        self.phase = Phase::Loading;
        Ok(())
    }

    /// Seat the loaded shadow under `guards` (its optimizer's and workspace's leases), with `lease`
    /// the engine's non-owning view of the optimizer's.
    pub(crate) fn seat(&mut self, port: &dyn LearnPort, lease: Box<dyn OptimizerLease>, guards: impl IntoIterator<Item = Box<dyn Send>>) -> Result<(), NotTold> {
        if self.phase != Phase::Loaded {
            return Err(NotTold::OutOfOrder("seat before its shadow was loaded"));
        }
        port.tell(Learning::Seat(Seat { seat: self.seat, lease }))?;
        self.guards.extend(guards);
        self.phase = Phase::Seating;
        Ok(())
    }

    /// Learn the lesson at `position` (the example `position.learned` of its submission).
    pub(crate) fn learn(&mut self, port: &dyn LearnPort, position: Position, lesson: Lesson, admission: Box<dyn ReplyAdmission>) -> Result<StepId, NotTold> {
        if !self.ready() {
            return Err(NotTold::OutOfOrder("a lesson while not seated or another is in flight"));
        }
        let step = StepId(Uuid::new_v4());
        port.tell(Learning::Learn(Learn { seat: self.seat, step, lesson, admission }))?;
        self.in_flight = Some((step, position));
        Ok(step)
    }

    /// Save what was learned since the newest generation into the directory `into` (the trainer's:
    /// the engine writes the adapter and its optimizer state there, bound to `binding`). `false` when
    /// there is nothing to save.
    pub(crate) fn checkpoint(&mut self, port: &dyn LearnPort, into: PathBuf, binding: Arc<str>) -> Result<bool, NotTold> {
        if self.phase != Phase::Seated {
            return Err(NotTold::OutOfOrder("a checkpoint while not seated"));
        }
        if self.progress.is_empty() {
            return Ok(false);
        }
        port.tell(Learning::Checkpoint { seat: self.seat, into, binding })?;
        self.checkpoints.push_back(self.positions());
        Ok(true)
    }

    /// Let the seat go.
    pub(crate) fn release(&mut self, port: &dyn LearnPort) -> Result<(), NotTold> {
        if matches!(self.phase, Phase::Releasing | Phase::Gone) {
            return Ok(());
        }
        port.tell(Learning::Release { seat: self.seat })?;
        self.phase = Phase::Releasing;
        Ok(())
    }

    fn positions(&self) -> Vec<Position> {
        let mut positions: Vec<Position> = self.progress.iter().map(|(submission, learned)| Position { submission: *submission, learned: *learned }).collect();
        positions.sort_by_key(|position| position.submission);
        positions
    }

    /// What the trainer does now that the engine said `said`. `None` when it is about another seat,
    /// or answers nothing this seat told (a step it does not hold).
    pub(crate) fn hear(&mut self, port: &dyn LearnPort, said: &Said) -> Option<Heard> {
        match said {
            Said::Seat { seat, state } if *seat == self.seat => Some(self.seat_said(state)),
            Said::Step { seat, step, outcome } if *seat == self.seat => self.step_said(port, *step, outcome),
            Said::Generation { seat, outcome } if *seat == self.seat => self.generation_said(outcome),
            _ => None,
        }
    }

    fn seat_said(&mut self, state: &SeatState) -> Heard {
        match state {
            SeatState::Measured { weights } => {
                self.phase = Phase::Measured;
                Heard::LeaseWeights(weights.clone())
            }
            SeatState::Loaded { optimizer, workspace, reply_rows } => {
                self.phase = Phase::Loaded;
                Heard::LeaseSeat { optimizer: optimizer.clone(), workspace: workspace.clone(), reply_rows: *reply_rows }
            }
            SeatState::Seated { optimizer } => {
                self.phase = Phase::Seated;
                Heard::Seated { optimizer: *optimizer }
            }
            // the engine holds nothing for it: its guards go
            SeatState::Refused { why } => {
                self.gone();
                Heard::NotSeated { why: Arc::clone(why) }
            }
            // the engine has not proved its memory released: every guard stays until it says Released
            SeatState::Quarantined { why } => {
                self.phase = Phase::Quarantined;
                self.in_flight = None;
                Heard::Quarantined { why: Arc::clone(why) }
            }
            SeatState::Released => {
                self.gone();
                Heard::Released
            }
        }
    }

    fn step_said(&mut self, port: &dyn LearnPort, step: StepId, outcome: &StepOutcome) -> Option<Heard> {
        let (told, position) = self.in_flight?;
        if told != step {
            return None;
        }
        self.in_flight = None;
        Some(match outcome {
            StepOutcome::Learned { steps, loss } => {
                // a non-finite loss is a poisoned update: never counted, never saved (Codex on 5f26871bc)
                if loss.is_some_and(|loss| !loss.is_finite()) {
                    return Some(self.uncertain(port));
                }
                let learned = position.learned.saturating_add(1);
                let held = self.progress.entry(position.submission).or_default();
                *held = (*held).max(learned);
                Heard::Learned { position, steps: steps.get(), loss: *loss }
            }
            StepOutcome::Unchanged => Heard::Again { position },
            StepOutcome::Uncertain { .. } => self.uncertain(port),
            StepOutcome::Refused { why } => Heard::StepRefused { position, why: Arc::clone(why) },
        })
    }

    /// Effects uncertain: nothing it learned is kept or saved, and it is let go.
    fn uncertain(&mut self, port: &dyn LearnPort) -> Heard {
        self.progress.clear();
        self.checkpoints.clear();
        // no engine to take the Release is no engine holding the shadow: it is gone either way
        if self.release(port).is_err() {
            self.gone();
        }
        Heard::Uncertain
    }

    fn generation_said(&mut self, outcome: &GenerationOutcome) -> Option<Heard> {
        let positions = self.checkpoints.pop_front()?;
        Some(match outcome {
            GenerationOutcome::Saved { into, steps } => {
                // what this generation holds is no longer volatile; anything learned after it stays
                for saved in &positions {
                    if self.progress.get(&saved.submission).is_some_and(|learned| *learned <= saved.learned) {
                        self.progress.remove(&saved.submission);
                    }
                }
                Heard::Saved { into: into.clone(), steps: *steps, positions }
            }
            GenerationOutcome::Refused { why } => Heard::Unsaved { why: Arc::clone(why), positions },
        })
    }

    /// Nothing of the seat remains on the engine: its guards drop.
    fn gone(&mut self) {
        self.phase = Phase::Gone;
        self.in_flight = None;
        self.guards.clear();
    }

    /// How many lease guards the seat holds.
    #[cfg(test)]
    fn held(&self) -> usize {
        self.guards.len()
    }
}

#[cfg(test)]
mod tests;
