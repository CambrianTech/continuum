//! LearnPort: how the trainer reaches the persona engine to learn in place (IN-PROCESS-LEARNING.md,
//! "How the trainer reaches it"; agreed on airc with Codex and Cormac, 2026-11-24).
//!
//! The trainer and the engine are disconnected parts. The trainer TELLS the engine a [`Learning`]
//! on the engine's one queue, beside her turns, and never awaits an answer. The engine PUBLISHES what
//! became of each seat, step and checkpoint on one bus stream, [`LEARNING_SAID`], every [`Said`]
//! naming its seat; the trainer hears it as a SAGE input and routes by seat, and an observer or a
//! replay hears the same feed. The engine owns no cadence: the trainer decides when to learn,
//! checkpoint and release.
//!
//! Memory is carried as guards. A seat's optimizer lease and a step's workspace admission ride in
//! the command, the engine holds each while native code does, and dropping it releases its bytes,
//! so nothing is handed back across the seam.
//!
//! No engine is a typed [`Unavailable`] that returns the command (its guards drop at the caller),
//! never a fallback to another path.

use std::num::NonZeroU64;
use std::path::PathBuf;
use std::sync::Arc;

use continuum_persona_engine::persona::PersonaId;
use continuum_sage::dataflow::TopicName;
use crate::inference::llama_server::FreshShadow;
use llama::{Optimizer, OptimizerLease, ReplyAdmission};

/// The engine's one learning stream: every [`Said`], from its one producer (Cormac on #6254).
pub const LEARNING_SAID: TopicName = TopicName("learning/said");

/// One shadow seated for one mind: the adapter she learns into while it is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SeatId(pub uuid::Uuid);

/// One learning step the trainer asked for, so its outcome is told apart from every other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StepId(pub uuid::Uuid);

/// What the trainer tells the engine.
pub enum Learning {
    /// Seat `shadow` for `mind`.
    Seat(Seat),
    /// Learn one lesson into a seated shadow.
    Learn(Learn),
    /// Save the seat's shadow as its next generation: its weights and its optimizer state.
    Checkpoint { seat: SeatId },
    /// Let the seat go: its optimizer and its lease are released once native code frees them.
    Release { seat: SeatId },
}

/// A shadow to seat, with the lease that pays for its optimizer.
pub struct Seat {
    pub seat: SeatId,
    pub mind: PersonaId,
    pub shadow: ShadowSource,
    pub optimizer: Optimizer,
    /// Admits and releases the optimizer's bytes as native code allocates and frees them.
    pub lease: Box<dyn OptimizerLease>,
}

/// Where a seat's shadow comes from (Cormac on #6254). Either way the engine makes it on her resident
/// weights.
pub enum ShadowSource {
    /// A new shadow the engine creates from its geometry.
    Fresh(FreshShadow),
    /// A generation a checkpoint saved: its weights, and its optimizer state when the generation
    /// carries one. The seat's [`SeatState::Seated`] says which came back.
    Generation(PathBuf),
}

/// One lesson for a seated shadow, admitted by its workspace admission.
pub struct Learn {
    pub seat: SeatId,
    pub step: StepId,
    pub lesson: Lesson,
    /// Checks the shared workspace's growth against what the memory owner holds for this step.
    pub admission: Box<dyn ReplyAdmission>,
}

/// One lesson as the engine learns it: the conversation she was served and the reply to learn. The
/// engine renders it with her chat template. Assistant messages marked `"train": false` are
/// context; the last assistant message is the reply.
#[derive(Debug, Clone, PartialEq)]
pub struct Lesson {
    pub messages: Vec<serde_json::Value>,
    /// The tools she was offered, as served (none for a conversation without tools).
    pub tools: Option<serde_json::Value>,
}

/// What the engine says on [`LEARNING_SAID`], in the order it happened to each seat. The stream is
/// bounded by what the trainer told: the engine says one state per Seat and per Release, one outcome
/// per Learn and one per Checkpoint, and nothing on its own, so the trainer that tells at its own
/// cadence also sets how much can wait in it. A Generation answers the Checkpoints of its seat in
/// the order they were told.
#[derive(Debug, Clone, PartialEq)]
pub enum Said {
    Seat { seat: SeatId, state: SeatState },
    Step { seat: SeatId, step: StepId, outcome: StepOutcome },
    Generation { seat: SeatId, outcome: GenerationOutcome },
}

/// Where a seat stands.
#[derive(Debug, Clone, PartialEq)]
pub enum SeatState {
    /// Loaded and retained: lessons may be learned into it. `bytes` is the shadow's weights.
    Seated { bytes: u64, optimizer: OptimizerStart },
    /// Not seated; nothing is held.
    Refused { why: Arc<str> },
    /// Released: native code freed its optimizer and the lease dropped.
    Released,
    /// A step's restoration was not confirmed: the engine holds the seat until it proves the
    /// optimizer and workspace released.
    Quarantined { why: Arc<str> },
}

/// How a seated shadow's optimizer starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptimizerStart {
    /// Its moments came back from the generation it was seated from.
    Restored,
    /// It starts cold: a fresh shadow, or a generation saved without optimizer state.
    Cold,
}

/// What one step did to the seat's shadow.
#[derive(Debug, Clone, PartialEq)]
pub enum StepOutcome {
    /// Optimizer steps committed. A turn that drained it early keeps the steps already committed.
    Learned { steps: NonZeroU64, loss: Option<f64> },
    /// Nothing committed (drained before a gradient step landed): the trainer may tell it again.
    Unchanged,
    /// Native code marks the shadow's effects uncertain: it must not be kept or published.
    Uncertain { steps: u64 },
    /// Not learned; the shadow is as it was.
    Refused { why: Arc<str> },
}

/// What a checkpoint made.
#[derive(Debug, Clone, PartialEq)]
pub enum GenerationOutcome {
    Saved { path: PathBuf, generation: u64 },
    Refused { why: Arc<str> },
}

/// No engine took the command: it comes back, and its guards drop with it at the caller.
pub struct Unavailable {
    pub returned: Learning,
}

/// The one way the trainer reaches the engine that serves her.
pub trait LearnPort: Send + Sync {
    /// Queue `learning` beside her turns. Its outcome arrives on the seat's stream.
    fn tell(&self, learning: Learning) -> Result<(), Unavailable>;
}
