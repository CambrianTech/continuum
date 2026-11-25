//! The lane: who takes the Core's next step (THE-LANE-ON-SAGE.md, "When to switch"). How long a mind
//! keeps the Core is an outcome of these choices, never a parameter: a deep mind whose pages went cold
//! costs more to bring in, and a policy that sees her features finds that out.
//! hot-path: the lane decides at every Core step (THE-LANE-ON-SAGE.md, "The types").
//!
//! It decides at every step, so it allocates nothing past its buffers' high-water mark: actions name ready minds by
//! slot (their index in `ready`) and a batch by a bitmask of slots; a mind's key is `Copy`.

use std::fmt::Debug;
use std::marker::PhantomData;
use std::time::Duration;

use crate::component::Urgency;

use super::super::measured::Measured;
use super::super::policy::{Choice, Policy, PolicyId};
use super::super::use_case::{Gate, UseCase, Violation};

/// A ready mind's index in [`Situation::ready`]. Only the first [`SLOTS`] ready minds are offered.
pub type Slot = u8;

/// A batch's members: bit `i` is ready slot `i`.
pub type Mask = u64;

/// How many ready minds a decision can name: one per bit of a [`Mask`], a fact of the type.
pub const SLOTS: usize = Mask::BITS as usize;

/// Where her turn stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Tokens of her prompt still to prefill.
    Prefill { remaining: u64 },
    Decode,
}

/// One mind as a lane policy sees her: what the algorithm might need, each from its owner, each
/// explicit when not yet measured. A learned policy may read more; these are the start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Mind<K> {
    pub mind: K,
    pub urgency: Urgency,
    /// Ready and waiting for the Core this long.
    pub waited: Duration,
    /// Her wait bound at this urgency: past it, the envelope steps her next.
    pub deadline: Duration,
    pub phase: Phase,
    /// Her KV's depth in bytes, and how much of it is not resident (the block layer knows exactly).
    pub kv_bytes: Measured<u64>,
    pub cold_bytes: Measured<u64>,
    /// Tokens left in her turn, predicted from her own history in this activity.
    pub remaining: Measured<u32>,
    /// Her step time, and the cost of bringing her in from where she is now.
    pub step: Measured<Duration>,
    pub switch_in: Measured<Duration>,
    /// Her prefix cache's hit rate: how much of her work reuses what is warm.
    pub prefix_hit: Measured<f64>,
}

/// The Core at a step boundary: who holds it, and who is ready.
#[derive(Debug, Clone, PartialEq)]
pub struct Situation<K> {
    pub running: Option<Mind<K>>,
    pub ready: Vec<Mind<K>>,
    /// The widest batch the Core can step now (1: no batching).
    pub batch_width: u32,
}

/// The lane's actions: slots and masks, never owned keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Continue,
    Switch(Slot),
    /// One step for the running mind and the ready minds whose bits are set.
    Batch(Mask),
    Idle,
}

/// What a decision window measured.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outcome {
    pub tokens: Measured<u64>,
    pub engine: Duration,
    /// The longest any ready mind waited in the window, against her deadline.
    pub worst_wait: Duration,
    pub worst_deadline: Duration,
}

/// The lane as a use case, over any `Copy` mind key.
#[derive(Debug, Clone, Copy)]
pub struct Lane<K>(PhantomData<K>);

impl<K: Copy + Debug + PartialEq + Send + 'static> UseCase for Lane<K> {
    type Situation = Situation<K>;
    type Action = Action;
    type Outcome = Outcome;
    const NAME: &'static str = "lane";

    fn envelope(situation: &Situation<K>, admissible: &mut Vec<Action>) -> Gate<Action> {
        let ready = &situation.ready[..situation.ready.len().min(SLOTS)];
        // A mind past her deadline goes next: the most overdue, whatever any policy prefers.
        let overdue = ready.iter().enumerate().filter(|(_, mind)| mind.waited >= mind.deadline).max_by_key(|(_, mind)| mind.waited.saturating_sub(mind.deadline));
        if let Some((slot, _)) = overdue {
            return Gate::Forced(Action::Switch(slot as Slot));
        }
        let Some(running) = &situation.running else {
            if ready.is_empty() {
                return Gate::Forced(Action::Idle);
            }
            admissible.extend((0..ready.len()).map(|slot| Action::Switch(slot as Slot)));
            return Gate::Choose;
        };
        // Finish first: her measured remainder costs less than bringing anyone in.
        let cheapest_switch = ready.iter().filter_map(|mind| mind.switch_in.value()).min();
        let finishing = matches!((running.remaining.value(), running.step.value(), cheapest_switch), (Some(left), Some(step), Some(switch)) if step * left < switch);
        if finishing || ready.is_empty() {
            return Gate::Forced(Action::Continue);
        }
        admissible.push(Action::Continue);
        admissible.extend((0..ready.len()).map(|slot| Action::Switch(slot as Slot)));
        // the running mind and the first `others` ready minds: the lowest `others` bits (1..=SLOTS)
        let others = (situation.batch_width as usize).saturating_sub(1).min(ready.len());
        if others > 0 {
            admissible.push(Action::Batch(!0 >> (SLOTS - others)));
        }
        Gate::Choose
    }

    fn constraint(outcome: &Outcome) -> Option<Violation> {
        (outcome.worst_wait > outcome.worst_deadline).then_some(Violation {
            constraint: "a ready mind's wait bound (s)",
            measured: outcome.worst_wait.as_secs_f64(),
            limit: outcome.worst_deadline.as_secs_f64(),
        })
    }

    fn retain(slot: &mut Situation<K>, from: &Situation<K>) {
        slot.running = from.running;
        slot.ready.clone_from(&from.ready);
        slot.batch_width = from.batch_width;
    }

    fn empty() -> Situation<K> {
        Situation { running: None, ready: Vec::new(), batch_width: 1 } // cold: an empty slot is built once, at the decider's construction
    }
}

/// Need first: the most urgent ready mind is switched in when she needs the Core more than the running
/// one (an addressed mind takes the next step); otherwise the running one continues; with nobody
/// running, the most urgent, longest-waiting mind starts. The envelope keeps every wait inside its
/// bound whatever this prefers. The baseline the cost model is measured against.
#[derive(Debug, Clone, Copy, Default)]
pub struct NeedFirst;

impl<K: Copy + Debug + PartialEq + Send + 'static> Policy<Lane<K>> for NeedFirst {
    fn id(&self) -> PolicyId {
        PolicyId { name: "need-first", parameters: 0 }
    }

    fn choose(&mut self, situation: &Situation<K>, admissible: &[Action]) -> Choice {
        let best = situation.ready.iter().take(SLOTS).enumerate().max_by(|(_, a), (_, b)| a.urgency.cmp(&b.urgency).then(a.waited.cmp(&b.waited)));
        let wanted = match (&situation.running, best) {
            (Some(running), Some((slot, mind))) if mind.urgency > running.urgency => Action::Switch(slot as Slot),
            (Some(_), _) => Action::Continue,
            (None, Some((slot, _))) => Action::Switch(slot as Slot),
            (None, None) => Action::Idle,
        };
        // The envelope offered what it allows; a wanted action it did not offer yields its first.
        let index = admissible.iter().position(|action| *action == wanted).unwrap_or(0); // unwrap_or: not offered means the envelope decided; its first is its choice
        Choice { index, expected: None }
    }
}
