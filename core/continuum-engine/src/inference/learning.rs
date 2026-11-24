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
//! Memory is leased by the trainer and only checked by the engine (Codex on #6254). The trainer owns
//! every lease guard: dropping one releases through the resource authority, which takes its governor
//! lock, so it must never happen on the serving thread. A command carries non-owning, fail-closed
//! views of those leases (the optimizer's [`OptimizerLease`], a lesson's [`ReplyAdmission`]); native
//! code consults them before it allocates. The trainer releases its guards when it hears the step or
//! the seat end on [`LEARNING_SAID`], whatever ended it: an outcome, a refusal, a dropped command or a
//! quarantine's release.
//!
//! No engine is a typed [`Unavailable`] that returns the command to the trainer, which still owns its
//! guards; never a fallback to another path.

use std::num::NonZeroU64;
use std::path::PathBuf;
use std::sync::Arc;

use continuum_persona_engine::persona::PersonaId;
use continuum_sage::dataflow::TopicName;
use crate::inference::llama_server::FreshShadow;
use llama::{MemoryRow, Optimizer, OptimizerLease, ReplyAdmission};

/// The engine's one learning stream: every [`Said`], from its one producer (Cormac on #6254).
pub const LEARNING_SAID: TopicName = TopicName("learning/said");

/// One shadow seated for one mind: the adapter she learns into while it is held.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SeatId(pub uuid::Uuid);

/// One learning step the trainer asked for, so its outcome is told apart from every other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StepId(pub uuid::Uuid);

/// What the trainer tells the engine, in this order for a seat: Measure, Load, Seat, each after the
/// trainer leased what the step before measured; then Learn and Checkpoint as it chooses; then
/// Release.
///
/// Native measurement needs the serving context, which only the engine thread holds, and leasing
/// takes the resource authority's lock, which the engine thread must never wait on (Codex on
/// #6254): so the engine measures and the trainer leases. And native code measures a shadow's
/// optimizer only once the shadow is loaded and attached (`opt_untrainable`, the check every
/// training entry makes): so its weights are measured and leased first, then loaded, then the
/// optimizer and workspace are measured and leased.
pub enum Learning {
    /// Measure what loading `shadow` for `mind` would hold, allocating nothing. Says
    /// [`SeatState::Measured`].
    Measure(Measure),
    /// Load the measured shadow, under the weights lease the trainer took, and measure what its
    /// optimizer and the lesson it was sized for would hold. Says [`SeatState::Loaded`].
    Load { seat: SeatId },
    /// Seat the loaded shadow under the optimizer and workspace leases the trainer took for it.
    Seat(Seat),
    /// Learn one lesson into a seated shadow.
    Learn(Learn),
    /// Save the seat's shadow as a generation in the directory `into`: its weights (`adapter.gguf`)
    /// and its optimizer state (`optimizer.gguf`), the state bound to `binding`, which the trainer
    /// also records in the generation's manifest, as llama-server's checkpoint did. The trainer owns
    /// the directory, its manifest and its durable commit.
    Checkpoint { seat: SeatId, into: PathBuf, binding: Arc<str> },
    /// Let the seat go: its optimizer and its lease are released once native code frees them.
    Release { seat: SeatId },
}

/// A seat to measure: whose, which shadow, how it learns, and the lesson it is sized for.
pub struct Measure {
    pub seat: SeatId,
    pub mind: PersonaId,
    pub shadow: ShadowSource,
    pub optimizer: Optimizer,
    /// The lesson the seat is sized for (Cormac on #6255): the port renders it with her template
    /// and sizes the workspace at the whole rendered lesson, the most a reply on it can be. A later,
    /// longer lesson is [`StepOutcome::TooLong`].
    pub sized_for: Lesson,
}

/// A loaded seat, under the optimizer and workspace leases the trainer took for it.
pub struct Seat {
    pub seat: SeatId,
    /// A non-owning view of the optimizer's lease: native code asks it before each allocation.
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
    /// A non-owning view of this step's workspace lease: native code asks it before the workspace grows.
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
/// bounded by what the trainer told: the engine says one state per Measure, Load, Seat and Release, one outcome
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
    /// Measured: lease `weights`, then tell Load. Nothing is allocated yet.
    Measured { weights: Vec<MemoryRow> },
    /// Loaded (its weights held) and measured: lease `optimizer` and `workspace`, then tell Seat.
    /// `workspace` is sized for a lesson of up to `reply_rows` tokens; a longer one is
    /// [`StepOutcome::TooLong`].
    Loaded { optimizer: Vec<MemoryRow>, workspace: Vec<MemoryRow>, reply_rows: std::num::NonZeroU32 },
    /// Retained: lessons may be learned into it.
    Seated { optimizer: OptimizerStart },
    /// Ended with nothing held: the trainer may release every guard for the seat. Said only once
    /// the engine dropped its last native reference to the shadow.
    Refused { why: Arc<str> },
    /// The command was not applied and the seat is as it was, still holding what it held (an
    /// out-of-order or repeated command, or a retain native code refused): keep every guard (Codex
    /// on #6254).
    Kept { why: Arc<str> },
    /// Released: native code freed its optimizer and the engine dropped its last reference to the
    /// shadow before saying so; the trainer may release every guard.
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
    /// It starts cold: a fresh shadow, or a generation whose weights were reloaded with cold moments
    /// (one saved without optimizer state, or any generation until the optimizer restore lands).
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
    /// Longer than the seat was measured for: release it and measure again for `need` tokens.
    TooLong { need: std::num::NonZeroU32 },
    /// Not learned; the shadow is as it was.
    Refused { why: Arc<str> },
}

/// What a checkpoint made.
#[derive(Debug, Clone, PartialEq)]
pub enum GenerationOutcome {
    /// Written to `into`, with the optimizer steps the shadow had committed when it was saved.
    Saved { into: PathBuf, steps: u64 },
    Refused { why: Arc<str> },
}

/// No engine took the command: it comes back to the trainer.
pub struct Unavailable {
    pub returned: Learning,
}

/// The one way the trainer reaches the engine that serves her.
pub trait LearnPort: Send + Sync {
    /// Queue `learning` beside her turns. Its outcome arrives on the seat's stream.
    fn tell(&self, learning: Learning) -> Result<(), Unavailable>;
}

/// The node bus's one emitter for [`LEARNING_SAID`], claimed here once per process (a scheduler may
/// be spawned again; this is the stream's producer). `None` when another producer holds it.
fn said_emitter() -> Option<&'static parking_lot::Mutex<continuum_sage::dataflow::Emitter<Said>>> {
    static SAID: std::sync::OnceLock<Option<parking_lot::Mutex<continuum_sage::dataflow::Emitter<Said>>>> = std::sync::OnceLock::new();
    SAID.get_or_init(|| continuum_sage::component::node().emitter::<Said>(LEARNING_SAID).ok().map(parking_lot::Mutex::new)).as_ref()
}

/// Whether this process can say what the engine learned. A port that cannot is [`Unavailable`]
/// before it takes any command, so no outcome (above all a Released) is ever told into silence
/// (Codex on L2).
pub(crate) fn speaks() -> bool {
    said_emitter().is_some()
}

/// Say `said` on [`LEARNING_SAID`]; an emit never waits. Reached only through a port that
/// [`speaks`].
pub(crate) fn say(said: Said) {
    match said_emitter() {
        Some(emitter) => {
            emitter.lock().emit(said);
        }
        None => continuum_base::probe!(class = "learning.said.unclaimed", said = ?said, "another producer holds learning/said: what the engine learned is not said"),
    }
}
