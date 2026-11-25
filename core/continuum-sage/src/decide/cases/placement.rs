//! Placement (scaffold): where the next unit of work runs. Here, a machine I own, or a leased peer on
//! another grid, each priced the same way: transfer, restore, queueing, lost warmth and price. A
//! Premiere export or a second Continuum can take this machine's headroom at any moment; when it
//! ends, this machine takes new work again while jobs already warm elsewhere finish there.

use std::time::{Duration, Instant};

use super::super::capacity::Capacity;
use super::super::measured::Measured;
use super::super::use_case::{NonEmpty, Envelope, UseCase, Violation};

/// A machine that could take the work: mine or a peer's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HolderId(pub u128);

#[derive(Debug, Clone, PartialEq)]
pub struct Holder {
    pub id: HolderId,
    pub capacity: Capacity,
    pub transfer: Measured<Duration>,
    pub queue: Measured<Duration>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Situation {
    pub now: Instant,
    /// Bytes the work needs moved to run elsewhere, and whether it is warm here.
    pub moves: u64,
    pub warm_here: bool,
    pub here: Capacity,
    pub holders: Vec<Holder>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Here,
    On(HolderId),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub completed: bool,
    pub took: Duration,
    /// What it cost at the agreed price (0 on a machine I own).
    pub spent: f64,
    pub deadline: Duration,
}

pub struct Placement;

impl UseCase for Placement {
    type Situation = Situation;
    type Action = Action;
    type Outcome = Outcome;
    const NAME: &'static str = "placement";

    /// Observed capacity is a candidate, never a plan: work goes only to a holder whose lease is in
    /// force at the decision. Here is always admissible.
    fn envelope(situation: &Situation) -> Envelope<Action> {
        let leased = situation.holders.iter().filter(|holder| holder.capacity.committed_at(situation.now) > 0.0).map(|holder| Action::On(holder.id));
        Envelope::Admissible(NonEmpty::new(Action::Here, leased))
    }

    fn constraint(outcome: &Outcome) -> Option<Violation> {
        (outcome.took > outcome.deadline).then_some(Violation { constraint: "the work's deadline (s)", measured: outcome.took.as_secs_f64(), limit: outcome.deadline.as_secs_f64() })
    }
}
