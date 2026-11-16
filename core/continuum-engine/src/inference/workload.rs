//! The engine's workload as a typed capture (CONTINUUM-ENGINE 4.10 P5): what the minds asked of the
//! engine and what it chose to run, so the dyno replays production and an alternative scheduling
//! policy is scored on the same arrivals.
//!
//! The chosen steps alone are not enough (Codex's review of 4.10): they hide the runnable work the
//! live policy left waiting. So every sequence's life is recorded too, arrival to completion,
//! including the time it was blocked and where its memory went, and the replay rebuilds the ready
//! set at every instant. Every record names the engine incarnation it happened on, so a relaunch is
//! another engine by construction; the time is the capture interval's monotonic clock
//! ([`continuum_base::capture::At`]).
//!
//! Records flow through [`continuum_base::capture::CaptureSink`]: `Noop` unless someone is
//! recording, and a replay or gate takes only a `CompleteInterval`.

use std::time::Duration;

use continuum_kernel::runtime::stage::MindId;

use super::engine_residency::EngineIncarnation;

/// One sequence on one engine: the engine's own task id, meaningful only with its incarnation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SequenceId(pub u64);

/// One record of the workload.
#[derive(Debug, Clone, PartialEq)]
pub enum WorkloadEvent {
    /// Something happened to one mind's sequence.
    Sequence { engine: EngineIncarnation, sequence: SequenceId, mind: MindId, change: SequenceChange },
    /// The engine ran one step.
    Step { engine: EngineIncarnation, step: EngineStep },
}

/// A moment in a sequence's life.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceChange {
    /// Her act reached the engine: `resident` tokens of her context were already in memory and
    /// `fresh` must be prefilled.
    Arrived { resident: u32, fresh: u32 },
    /// It can run on the next step.
    Ready,
    /// It cannot run until `on` is satisfied.
    Blocked { on: BlockedOn },
    /// What it was blocked on was satisfied.
    Resumed,
    /// She has her answer: `decoded` tokens.
    Completed { decoded: u32 },
    /// The act ended without an answer.
    Cancelled,
    /// `tokens` of her context left engine memory.
    PagedOut { tokens: u32 },
    /// `tokens` of her context came back into engine memory.
    PagedIn { tokens: u32 },
}

/// What a sequence was waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockedOn {
    /// Engine memory for her context.
    Memory,
    /// Her context paging back in.
    PageIn,
}

/// One engine step: which sequences ran and what each did, and how long it took.
#[derive(Debug, Clone, PartialEq)]
pub struct EngineStep {
    pub took: Duration,
    pub ran: Vec<StepWork>,
}

/// One sequence's share of a step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepWork {
    pub sequence: SequenceId,
    /// Tokens of her context in memory when the step began.
    pub depth: u32,
    pub prefilled: u32,
    pub decoded: u32,
    /// Of the decoded tokens, those a draft proposed and the model accepted.
    pub accepted_draft: u32,
}
