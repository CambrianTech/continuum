//! The one seam between minds and the engine (CONTINUUM-ENGINE 4.16): typed commands in, typed
//! events out, one published snapshot. Nothing crosses it but these types, and nothing on either
//! side waits on the other: a command is sent and the sender goes on; its outcome is an event the
//! interested stage reacts to (Joel, 2026-11-19: "Never wait on anything. It's all threads and
//! handles"; "Events and data flow").
//!
//! A mind is a multi-activity process. Each [`Activity`] is one stream of her thinking (a card, a
//! conversation, a review, her own reflection); her identity and memory are the prefix her
//! activities share. She never blocks: a tool's result is an [`Command::Extend`] into the activity
//! that called it, whenever it arrives, while she goes on thinking there or elsewhere.
//!
//! Ordering is per activity: an activity's commands apply in the order sent, whichever stage or
//! clone of her handle sent them (Codex on 4.16). Different activities, of one mind or of many,
//! interleave as the scheduler's policy decides.

use std::sync::Arc;

use crate::persona::{Epoch, PersonaId};

/// One stream of a mind's thinking.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ActivityId(pub uuid::Uuid);

/// An activity, with the mind and the epoch of her state it belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Activity {
    pub mind: PersonaId,
    pub epoch: Epoch,
    pub id: ActivityId,
}

/// A named point in an activity's token history (an epoch's start, a view's stable end). Its
/// checkpoint is the engine's; a rewind to it never re-reads what precedes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MarkId(pub uuid::Uuid);

/// One answer being generated in an activity. Its events carry this id; dropping the handle that
/// holds it is a [`Command::Cancel`]. A UUIDv7 minted by the mind: never reused, across activities
/// and engine restarts, so a delayed credit or cancel can only ever reach its own answer (Codex on
/// 42ca70c64b).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct AnswerId(pub uuid::Uuid);

/// The kind of a media span.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MediaKind {
    Image,
    Audio,
}

/// One piece of an activity's appended input, in order.
#[derive(Debug, Clone)]
pub enum Span<M> {
    /// Token ids, tokenized on her side.
    Text(Arc<[i32]>),
    /// A media item already encoded by the projector. `chunk` is the engine adapter's owning
    /// handle to the immutable native chunk (never a raw pointer or encoded bytes); it is held
    /// through the activity's marks and rewinds and released with them. `basis` names the
    /// projector revision it was encoded under, so a stale chunk is refused, not decoded.
    /// `positions` is what the engine reports the chunk occupies, which is not its token count.
    Media { chunk: M, kind: MediaKind, basis: u64, positions: u32 },
}

/// What a mind asks of the engine. `S` is the engine's sampler specification and `M` its media
/// chunk handle: the engine owns both; the seam only carries them.
#[derive(Debug, Clone)]
pub enum Command<S, M> {
    /// Append spans to the activity, in order. The engine prefills them as soon as it schedules
    /// the activity, whether or not she has asked to think yet.
    Extend { activity: Activity, spans: Vec<Span<M>> },
    /// Think on: generate from the activity's current end. `credit` is how many tokens may be
    /// in flight to the consumer unacknowledged (see [`Command::Credit`]); the engine never
    /// holds a frame for a consumer, it stops producing for that answer until credit returns.
    Think { activity: Activity, answer: AnswerId, sampling: S, max_tokens: u32, credit: u32 },
    /// Return credit for `tokens` more of an answer's tokens.
    Credit { answer: AnswerId, tokens: u32 },
    /// Stop an answer. What was decoded stays in her history; the answer ends `Cancelled`.
    Cancel { answer: AnswerId },
    /// Name the activity's current end.
    Mark { activity: Activity, mark: MarkId },
    /// Put the activity back at a mark: what follows it is dropped from her history.
    Rewind { activity: Activity, to: MarkId },
    /// The activity is over: its stream and checkpoints are released.
    Release { activity: Activity },
}

/// Why an answer ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ended {
    /// The model ended it (an end-of-generation token).
    Finished,
    /// It reached `max_tokens`.
    Budget,
    /// A stop sequence or a tool call ended it (detected by the consumer stage, which sent Cancel).
    Cancelled,
}

/// The command a refusal refuses, by identity, so a failed mark and a failed rewind are told apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Extend,
    Think(AnswerId),
    Credit(AnswerId),
    Cancel(AnswerId),
    Mark(MarkId),
    Rewind(MarkId),
    Release,
}

/// Why a command was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    /// The activity is not one the engine holds (released, or never extended).
    NoSuchActivity,
    /// The command names another epoch of her state than the one held.
    StaleEpoch,
    /// The answer named is not open.
    NoSuchAnswer,
    /// The mark named was never taken in this activity, or was released.
    NoSuchMark,
    /// The mark lies past what she holds; a rewind never moves forward.
    MarkAhead,
    /// A media span was encoded under another projector revision.
    StaleBasis,
    /// The engine could not hold more of her (memory).
    OutOfMemory,
    /// The native engine refused, in its words.
    Native(Arc<str>),
}

/// What the engine reports. Every event names what it is about; none is an answer to a caller
/// waiting for it. An answer ends exactly once, with [`Event::Done`] or with a [`Event::Refused`]
/// naming it, whatever its credit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event {
    /// The next token of an answer.
    Token { answer: AnswerId, token: i32 },
    /// An answer ended, with how many tokens it produced.
    Done { answer: AnswerId, generated: u32, ended: Ended },
    /// A mark's checkpoint exists at `position` (tokens held through it).
    Marked { activity: Activity, mark: MarkId, position: u32 },
    /// A command could not be applied; the activity is as it was before it. `answer` is set when
    /// the command concerned one.
    Refused { activity: Activity, operation: Operation, reason: Refusal },
}

/// Where a mind's state is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Residency {
    /// In the context: an activity of hers is the one decoding, or is staged next.
    Bound,
    /// Native blocks in RAM; binds back in milliseconds.
    Warm,
    /// On disk; restored to RAM before she binds.
    Cold,
}

/// One mind's row in the snapshot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MindRow {
    pub mind: PersonaId,
    pub residency: Residency,
    /// Tokens delivered to her answers since this engine incarnation started.
    pub delivered_tokens: u64,
    /// Activities she holds open.
    pub activities: u32,
}

/// What the engine publishes once per window: the only thing serving and the scheduler read. It is
/// observational: a resource grant still commits through its owner (Codex on 4.16).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineSnapshot {
    /// Which engine this is; a restart is a new incarnation, so a missed or repeated snapshot is
    /// told apart from a restart.
    pub incarnation: u64,
    /// The engine's frame count when it was taken (monotonic within an incarnation).
    pub frame: u64,
    /// Unix milliseconds when it was taken.
    pub observed_at_ms: u64,
    /// The share of the window the GPU was busy, in thousandths (the gate: at least 800).
    pub gpu_busy_permille: u16,
    /// The longest stretch in the window with no GPU work (the gate: at most 2,000 ms).
    pub longest_idle_ms: u32,
    pub minds: Vec<MindRow>,
}
