//! Complete capture intervals folded back into what the scheduler faced: the ready set of each
//! engine at every step, what it chose from it, and each mind's progress (her acts, their time, and
//! the time she was runnable and not run). The same fold reads production and a replayed policy, so
//! the two are compared on one definition.
//!
//! A [`Replay`] is fed consecutive intervals of ONE ring and keeps every sequence's state across
//! them: an act that arrives in one interval and completes in the next is one act. A gap, an interval
//! out of order, an event before the one it follows, or a change to a sequence that never arrived is
//! an error, never silently dropped (Codex's review of #6052). Waiting is elapsed time on the ring's
//! clock, so a runnable mind on an engine that runs no step at all accrues it.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use continuum_base::capture::{At, CompleteInterval};
use continuum_kernel::runtime::stage::MindId;

use super::{EngineIncarnation, SequenceChange, SequenceId, WorkloadEvent};

/// A sequence across relaunches: an id is only meaningful on its own engine.
pub type SequenceKey = (EngineIncarnation, SequenceId);

/// One step as its engine's scheduler saw it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepView {
    pub engine: EngineIncarnation,
    pub at: At,
    pub took: Duration,
    /// This engine's runnable sequences when the step began.
    pub ready: Vec<SequenceKey>,
    pub ran: Vec<SequenceKey>,
}

/// One mind's share of what was fed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MindProgress {
    pub acts: u32,
    /// Arrival to completion, summed over her completed acts.
    pub act_time: Duration,
    /// Elapsed time she was runnable and not running, open spans included up to the last interval's end.
    pub left_waiting: Duration,
    /// Her longest single runnable-and-not-running span: starvation.
    pub longest_wait: Duration,
}

/// What the fed intervals come to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Replayed {
    pub steps: Vec<StepView>,
    pub minds: BTreeMap<MindId, MindProgress>,
}

/// Why intervals cannot be replayed as one workload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReplayError {
    /// Not the next interval of the ring: one is missing or repeated.
    Gap { expected: u64, got: u64 },
    /// It does not begin where the previous one ended.
    Discontinuous { expected: At, got: At },
    /// An event earlier than the one before it, or outside its interval.
    OutOfOrder { at: At },
    /// A change, or a step, naming a sequence that has not arrived.
    UnknownSequence { sequence: SequenceKey },
    /// A sequence arriving while it is still live.
    ArrivedTwice { sequence: SequenceKey },
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ReplayError {}

#[derive(Debug)]
struct Live {
    mind: MindId,
    arrived: At,
    /// When her current runnable-and-not-running span began; none while blocked.
    unserved_since: Option<At>,
}

/// The fold, fed one complete interval after another.
#[derive(Debug, Default)]
pub struct Replay {
    /// The next interval's index and start; none before the first.
    next: Option<(u64, At)>,
    last_at: Option<At>,
    live: BTreeMap<SequenceKey, Live>,
    out: Replayed,
}

fn elapsed(from: At, to: At) -> Duration {
    to.since_origin().saturating_sub(from.since_origin())
}

impl Replay {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn feed(&mut self, interval: &CompleteInterval<WorkloadEvent>) -> Result<(), ReplayError> {
        if let Some((expected, starts)) = self.next {
            if interval.index() != expected {
                return Err(ReplayError::Gap { expected, got: interval.index() });
            }
            if interval.from() != starts {
                return Err(ReplayError::Discontinuous { expected: starts, got: interval.from() });
            }
        }
        for stamped in interval.events() {
            let at = stamped.at;
            if at < interval.from() || at > interval.to() || self.last_at.is_some_and(|last| at < last) {
                return Err(ReplayError::OutOfOrder { at });
            }
            self.last_at = Some(at);
            match &stamped.event {
                WorkloadEvent::Sequence { engine, sequence, mind, change } => self.change((*engine, *sequence), *mind, *change, at)?,
                WorkloadEvent::Step { engine, step } => {
                    let started = At::from_origin(at.since_origin().saturating_sub(step.took));
                    let ran: BTreeSet<SequenceKey> = step.ran.iter().map(|work| (*engine, work.sequence)).collect();
                    let ready: Vec<SequenceKey> =
                        self.live.iter().filter(|(key, l)| key.0 == *engine && l.unserved_since.is_some()).map(|(key, _)| *key).collect();
                    for key in &ran {
                        let Some(seq) = self.live.get_mut(key) else { return Err(ReplayError::UnknownSequence { sequence: *key }) };
                        if let Some(since) = seq.unserved_since {
                            let span = elapsed(since, started);
                            let progress = self.out.minds.entry(seq.mind).or_default();
                            progress.left_waiting += span;
                            progress.longest_wait = progress.longest_wait.max(span);
                            seq.unserved_since = Some(at); // runnable again from the step's end
                        }
                    }
                    self.out.steps.push(StepView { engine: *engine, at, took: step.took, ready, ran: ran.into_iter().collect() });
                }
            }
        }
        self.next = Some((interval.index() + 1, interval.to()));
        Ok(())
    }

    fn change(&mut self, key: SequenceKey, mind: MindId, change: SequenceChange, at: At) -> Result<(), ReplayError> {
        if let SequenceChange::Arrived { .. } = change {
            if self.live.contains_key(&key) {
                return Err(ReplayError::ArrivedTwice { sequence: key });
            }
            self.live.insert(key, Live { mind, arrived: at, unserved_since: None });
            return Ok(());
        }
        let Some(seq) = self.live.get_mut(&key) else { return Err(ReplayError::UnknownSequence { sequence: key }) };
        match change {
            SequenceChange::Ready | SequenceChange::Resumed => {
                seq.unserved_since.get_or_insert(at);
            }
            SequenceChange::Blocked { .. } | SequenceChange::Completed { .. } | SequenceChange::Cancelled => {
                // blocked is not the scheduler's to serve: the open span closes here
                if let Some(since) = seq.unserved_since.take() {
                    let span = elapsed(since, at);
                    let progress = self.out.minds.entry(seq.mind).or_default();
                    progress.left_waiting += span;
                    progress.longest_wait = progress.longest_wait.max(span);
                }
                let (mind, arrived) = (seq.mind, seq.arrived);
                match change {
                    SequenceChange::Completed { .. } => {
                        self.live.remove(&key);
                        let progress = self.out.minds.entry(mind).or_default();
                        progress.acts += 1;
                        progress.act_time += elapsed(arrived, at);
                    }
                    SequenceChange::Cancelled => {
                        self.live.remove(&key);
                    }
                    _ => {}
                }
            }
            SequenceChange::PagedOut { .. } | SequenceChange::PagedIn { .. } => {}
            SequenceChange::Arrived { .. } => {}
        }
        Ok(())
    }

    /// What was fed, with every still-open runnable span counted up to the last interval's end.
    pub fn report(&self) -> Replayed {
        let mut out = self.out.clone();
        if let Some((_, end)) = self.next {
            for seq in self.live.values() {
                if let Some(since) = seq.unserved_since {
                    let span = elapsed(since, end);
                    let progress = out.minds.entry(seq.mind).or_default();
                    progress.left_waiting += span;
                    progress.longest_wait = progress.longest_wait.max(span);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests;
