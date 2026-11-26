//! Placement (scaffold): where the next unit of work runs. Here, a machine I own, or a leased peer on
//! another grid, each priced the same way: transfer, restore, queueing, lost warmth and price. A
//! Premiere export or a second Continuum can take this machine's headroom at any moment; when it
//! ends, this machine takes new work again while jobs already warm elsewhere finish there.

use std::time::{Duration, Instant};

use super::super::capacity::{Capacity, Lease, Rate};
use super::super::measured::Measured;
use super::super::use_case::{Gate, UseCase, Violation};

/// A holder's index in [`Situation::holders`].
pub type Slot = u16;

/// A machine that could take the work: mine or a peer's.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Holder {
    pub id: u128,
    pub capacity: Capacity,
    pub transfer: Measured<Duration>,
    pub queue: Measured<Duration>,
}

/// The work being placed: how much of it, how long it is predicted to take, and what may be spent.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Work {
    pub units: f64,
    /// Its predicted run time where it would go; unmeasured work cannot be shown to fit a lease's term.
    pub predicted: Measured<Duration>,
    pub budget: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Situation {
    pub now: Instant,
    pub work: Work,
    /// Bytes the work needs moved to run elsewhere, and whether it is warm here.
    pub moves: u64,
    pub warm_here: bool,
    pub here: Capacity,
    pub holders: Vec<Holder>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Here,
    On(Slot),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outcome {
    pub completed: bool,
    pub took: Duration,
    /// What it cost at the agreed price (0 on a machine I own).
    pub spent: f64,
    pub deadline: Duration,
}

/// Whether `lease` carries `work` to its end from `now` within its budget: in force for the work's
/// whole predicted run (plus its transfer and queue), and priced within what may be spent. Observed
/// capacity is never enough; neither is a lease that ends mid-run or costs past the budget.
fn carries(lease: &Lease<Rate>, holder: &Holder, work: &Work, now: Instant) -> bool {
    let (Some(predicted), Some(transfer), Some(queue)) = (work.predicted.value(), holder.transfer.value(), holder.queue.value()) else {
        return false; // an unmeasured term cannot be shown to fit
    };
    // The lease's guaranteed rate bounds how long the work takes there, whatever was predicted: a
    // zero or non-finite rate carries nothing (Codex on #6278).
    let Ok(guaranteed) = Duration::try_from_secs_f64(work.units / lease.amount.0) else { return false };
    if !(lease.amount.0.is_finite() && lease.amount.0 > 0.0) {
        return false;
    }
    let run = predicted.max(guaranteed) + transfer + queue;
    now + run <= lease.until && lease.price.per_unit * work.units <= work.budget
}

pub struct Placement;

impl UseCase for Placement {
    type Situation = Situation;
    type Action = Action;
    type Outcome = Outcome;
    const NAME: &'static str = "placement";

    /// Here is always admissible; a peer only under a lease that carries the work to its end within
    /// budget.
    fn envelope(situation: &Situation, admissible: &mut Vec<Action>) -> Gate<Action> {
        admissible.push(Action::Here);
        let named = situation.holders.len().min(usize::from(Slot::MAX));
        for (slot, holder) in situation.holders[..named].iter().enumerate() {
            if let Capacity::Committed(lease) = &holder.capacity {
                if carries(lease, holder, &situation.work, situation.now) {
                    admissible.push(Action::On(slot as Slot));
                }
            }
        }
        Gate::Choose
    }

    fn constraint(outcome: &Outcome) -> Option<Violation> {
        (outcome.took > outcome.deadline).then_some(Violation { constraint: "the work's deadline (s)", measured: outcome.took.as_secs_f64(), limit: outcome.deadline.as_secs_f64() })
    }

    fn retain(slot: &mut Situation, from: &Situation) {
        slot.now = from.now;
        slot.work = from.work;
        slot.moves = from.moves;
        slot.warm_here = from.warm_here;
        slot.here = from.here;
        slot.holders.clone_from(&from.holders);
    }

    fn empty() -> Situation {
        let unmeasured = Capacity::Observed { rate: Measured::Unmeasured, as_of: Instant::now() };
        Situation { now: Instant::now(), work: Work { units: 0.0, predicted: Measured::Unmeasured, budget: 0.0 }, moves: 0, warm_here: false, here: unmeasured, holders: Vec::new() }
    }
}
