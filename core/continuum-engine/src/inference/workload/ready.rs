//! Complete capture intervals folded back into what the scheduler faced: each engine's ready set at
//! the moment each step BEGAN, what it chose from it, and each mind's progress (her acts, their time,
//! and the time she was runnable and not running). The same fold reads production and a replayed
//! policy, so the two are compared on one definition.
//!
//! A [`Replay`] is fed one capture's intervals in order, starting at its first (index zero, from the
//! capture's origin), so the ready set is complete from the start; it keeps every sequence's state
//! across drains, so an act or a step that spans two intervals is one. Another capture, a gap, a
//! repeat, a change to a sequence that never arrived, or a step that ends without beginning is an
//! error, never silently dropped (Codex's reviews of #6052). Waiting is elapsed time on the capture's
//! clock, so a runnable mind on an engine that runs no step at all accrues it.

use std::collections::BTreeMap;
use std::time::Duration;

use continuum_base::capture::{At, CaptureId, CompleteInterval};
use continuum_flow::dataflow::MindId;

use super::{EngineIncarnation, SequenceChange, SequenceId, StepId, WorkloadEvent};

/// A sequence across relaunches: an id is only meaningful on its own engine.
pub type SequenceKey = (EngineIncarnation, SequenceId);

/// One step as its engine's scheduler saw it when it began.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepView {
    pub engine: EngineIncarnation,
    pub began: At,
    pub ended: At,
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
    /// The first interval fed is not the capture's first: what came before it is unknown.
    MissingStart { index: u64, from: At },
    /// An interval of a different capture.
    OtherCapture { expected: CaptureId, got: CaptureId },
    /// Not the next interval of the capture: one is missing or repeated.
    Gap { expected: u64, got: u64 },
    /// It does not begin where the previous one ended.
    Discontinuous { expected: At, got: At },
    /// A change, or a step, naming a sequence that has not arrived.
    UnknownSequence { sequence: SequenceKey },
    /// A sequence arriving while it is still live.
    ArrivedTwice { sequence: SequenceKey },
    /// A step ending that never began, or beginning twice.
    UnknownStep { engine: EngineIncarnation, step: StepId },
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for ReplayError {}

/// Where a live sequence stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Standing {
    /// Arrived, not yet runnable.
    Arrived,
    /// Runnable and not running since this moment.
    Waiting(At),
    /// Inside a step.
    Running,
    /// Not the scheduler's to serve (memory, a page-in).
    Blocked,
}

#[derive(Debug)]
struct Live {
    mind: MindId,
    arrived: At,
    standing: Standing,
}

#[derive(Debug)]
struct OpenStep {
    began: At,
    ready: Vec<SequenceKey>,
    ran: Vec<SequenceKey>,
}

/// The fold, fed one capture's complete intervals in order.
#[derive(Debug, Default)]
pub struct Replay {
    /// The capture and the next interval's index and start; none before the first.
    next: Option<(CaptureId, u64, At)>,
    live: BTreeMap<SequenceKey, Live>,
    steps: BTreeMap<(EngineIncarnation, StepId), OpenStep>,
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
        match self.next {
            None if interval.index() != 0 || interval.from() != At::ORIGIN => {
                return Err(ReplayError::MissingStart { index: interval.index(), from: interval.from() });
            }
            None => {}
            Some((capture, _, _)) if capture != interval.capture() => {
                return Err(ReplayError::OtherCapture { expected: capture, got: interval.capture() });
            }
            Some((_, expected, _)) if interval.index() != expected => {
                return Err(ReplayError::Gap { expected, got: interval.index() });
            }
            Some((_, _, starts)) if interval.from() != starts => {
                return Err(ReplayError::Discontinuous { expected: starts, got: interval.from() });
            }
            Some(_) => {}
        }
        // a CompleteInterval's events are in the order they happened, inside its span
        for stamped in interval.events() {
            let at = stamped.at;
            match &stamped.event {
                WorkloadEvent::Sequence { engine, sequence, mind, change } => self.change((*engine, *sequence), *mind, *change, at)?,
                WorkloadEvent::StepBegan { engine, step, chosen } => {
                    let ready: Vec<SequenceKey> = self
                        .live
                        .iter()
                        .filter(|(key, l)| key.0 == *engine && matches!(l.standing, Standing::Waiting(_)))
                        .map(|(key, _)| *key)
                        .collect();
                    let mut ran = Vec::with_capacity(chosen.len());
                    for sequence in chosen {
                        let key = (*engine, *sequence);
                        let Some(seq) = self.live.get_mut(&key) else { return Err(ReplayError::UnknownSequence { sequence: key }) };
                        if let Standing::Waiting(since) = seq.standing {
                            charge(&mut self.out, seq.mind, elapsed(since, at));
                        }
                        seq.standing = Standing::Running;
                        ran.push(key);
                    }
                    if self.steps.insert((*engine, *step), OpenStep { began: at, ready, ran }).is_some() {
                        return Err(ReplayError::UnknownStep { engine: *engine, step: *step });
                    }
                }
                WorkloadEvent::StepEnded { engine, step, .. } => {
                    let Some(open) = self.steps.remove(&(*engine, *step)) else {
                        return Err(ReplayError::UnknownStep { engine: *engine, step: *step });
                    };
                    for key in &open.ran {
                        // a sequence that completed, cancelled or blocked inside the step stays as it is
                        if let Some(seq) = self.live.get_mut(key).filter(|seq| seq.standing == Standing::Running) {
                            seq.standing = Standing::Waiting(at);
                        }
                    }
                    self.out.steps.push(StepView { engine: *engine, began: open.began, ended: at, ready: open.ready, ran: open.ran });
                }
            }
        }
        self.next = Some((interval.capture(), interval.index() + 1, interval.to()));
        Ok(())
    }

    fn change(&mut self, key: SequenceKey, mind: MindId, change: SequenceChange, at: At) -> Result<(), ReplayError> {
        if let SequenceChange::Arrived { .. } = change {
            if self.live.contains_key(&key) {
                return Err(ReplayError::ArrivedTwice { sequence: key });
            }
            self.live.insert(key, Live { mind, arrived: at, standing: Standing::Arrived });
            return Ok(());
        }
        let Some(seq) = self.live.get_mut(&key) else { return Err(ReplayError::UnknownSequence { sequence: key }) };
        let waited = match seq.standing {
            Standing::Waiting(since) => Some(elapsed(since, at)),
            _ => None,
        };
        match change {
            SequenceChange::Ready | SequenceChange::Resumed => {
                if matches!(seq.standing, Standing::Arrived | Standing::Blocked) {
                    seq.standing = Standing::Waiting(at);
                }
            }
            SequenceChange::Blocked { .. } => {
                // blocked is not the scheduler's to serve: an open wait closes here
                if let Some(span) = waited {
                    charge(&mut self.out, seq.mind, span);
                }
                seq.standing = Standing::Blocked;
            }
            SequenceChange::Completed { .. } | SequenceChange::Cancelled => {
                let (mind, arrived) = (seq.mind, seq.arrived);
                self.live.remove(&key);
                if let Some(span) = waited {
                    charge(&mut self.out, mind, span);
                }
                if let SequenceChange::Completed { .. } = change {
                    let progress = self.out.minds.entry(mind).or_default();
                    progress.acts += 1;
                    progress.act_time += elapsed(arrived, at);
                }
            }
            SequenceChange::PagedOut { .. } | SequenceChange::PagedIn { .. } | SequenceChange::Arrived { .. } => {}
        }
        Ok(())
    }

    /// What was fed, with every still-open runnable span counted up to the last interval's end.
    pub fn report(&self) -> Replayed {
        let mut out = self.out.clone();
        if let Some((_, _, end)) = self.next {
            for seq in self.live.values() {
                if let Standing::Waiting(since) = seq.standing {
                    charge(&mut out, seq.mind, elapsed(since, end));
                }
            }
        }
        out
    }
}

/// `span` of runnable-and-not-running time to `mind`.
fn charge(out: &mut Replayed, mind: MindId, span: Duration) {
    let progress = out.minds.entry(mind).or_default();
    progress.left_waiting += span;
    progress.longest_wait = progress.longest_wait.max(span);
}

#[cfg(test)]
mod tests;
