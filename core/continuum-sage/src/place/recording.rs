//! The placer's field recording and its replay (Joel, 2026-11-26: "Need probing, replay, build into these
//! components. So you can take what you know in the field and diagnose or even simulate"). Every input of a
//! [`PlacerCore`] is data (an [`Act`]), so the core keeps the last of them in a ring, and with each commit what the
//! claim actually spent (a consumer spends straight from the claim's atomics, which no act carries). A recording
//! taken on a node (the 5090 losing its driver, a seat that never seats) replays into a fresh core on any machine
//! and reproduces the same tables, grants, refusals and shrink asks, step by step: the simulator is the component
//! itself.

use std::collections::VecDeque;

use super::claim::Need;
use super::component::{Act, AskId, Domain, Status};
use super::core::PlacerCore;

/// One thing the field did: an act, or what an ask's claim had spent when it was committed.
#[derive(Debug, Clone, PartialEq)]
pub enum Recorded<P, Q> {
    Act(Act<P, Q>),
    Spent { ask: AskId, spent: Vec<Need<P, Q>> },
}

/// The last `capacity` things a core did, oldest first.
#[derive(Debug, Clone, PartialEq)]
pub struct Recording<P, Q> {
    capacity: usize,
    /// How many rolled off the front.
    pub first: u64,
    pub entries: VecDeque<Recorded<P, Q>>,
}

impl<P, Q> Recording<P, Q> {
    /// A ring of `capacity` entries; a capacity of 0 records nothing.
    pub fn new(capacity: usize) -> Self {
        Self { capacity, first: 0, entries: VecDeque::with_capacity(capacity) }
    }

    pub(super) fn record(&mut self, entry: Recorded<P, Q>) {
        if self.capacity == 0 {
            return;
        }
        if self.entries.len() == self.capacity {
            self.entries.pop_front();
            self.first += 1;
        }
        self.entries.push_back(entry);
    }

    pub(super) fn on(&self) -> bool {
        self.capacity > 0
    }

    /// Whether it holds everything since the core began (nothing rolled off): only then is a replay the field's.
    pub fn whole(&self) -> bool {
        self.first == 0
    }
}

/// Replay `recording` into a fresh core: each act applied and settled as the field did, each claim spending what
/// the field's spent. Refused when entries rolled off the ring, since the state before them is not in it.
pub fn replay<D: Domain>(recording: &Recording<D::Place, D::Quantity>) -> Result<PlacerCore<D>, Partial> {
    if !recording.whole() {
        return Err(Partial { first: recording.first });
    }
    let mut core = PlacerCore::default();
    for entry in &recording.entries {
        match entry {
            Recorded::Act(act) => {
                core.apply(act.clone());
                core.settle();
            }
            Recorded::Spent { ask, spent } => {
                if let Some(Status::Granted(claim)) = core.status(*ask) {
                    for need in spent {
                        claim.consume(need.place, need.amount); // the field's spend, on the replayed claim
                    }
                }
            }
        }
    }
    Ok(core)
}

/// A recording whose oldest entries rolled off: `first` of them are missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Partial {
    pub first: u64,
}
