//! The lane: who takes the Core's next step (THE-LANE-ON-SAGE.md, "When to switch"). How long a mind
//! keeps the Core is an outcome of these choices, never a parameter: a deep mind whose pages went cold
//! costs more to bring in, and a policy that sees her features finds that out.

use std::fmt::Debug;
use std::hash::Hash;
use std::marker::PhantomData;
use std::time::Duration;

use crate::component::Urgency;

use super::super::measured::Measured;
use super::super::use_case::{NonEmpty, Envelope, UseCase, Violation};

/// Where her turn stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Tokens of her prompt still to prefill.
    Prefill { remaining: u32 },
    Decode,
}

/// One mind as a lane policy sees her: what the algorithm might need, each from its owner, each
/// explicit when not yet measured. A learned policy may read more; these are the start.
#[derive(Debug, Clone, PartialEq)]
pub struct Mind<K> {
    pub mind: K,
    pub urgency: Urgency,
    /// Ready and waiting for the Core this long.
    pub waited: Duration,
    /// Her wait bound at this urgency: past it, the envelope steps her next.
    pub deadline: Duration,
    pub phase: Phase,
    /// Her KV's depth in bytes, and how much of it is not resident (the block layer knows exactly).
    pub kv_bytes: u64,
    pub cold_bytes: u64,
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

/// The lane's actions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action<K> {
    Continue,
    Switch(K),
    /// One step for several minds in their own streams.
    Batch(Vec<K>),
    Idle,
}

/// What a decision window measured.
#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub tokens: u64,
    pub engine: Duration,
    /// The longest any ready mind waited in the window, against her deadline.
    pub worst_wait: Duration,
    pub worst_deadline: Duration,
}

/// The lane as a use case, over any mind key.
pub struct Lane<K>(PhantomData<K>);

impl<K: Clone + Debug + PartialEq + Eq + Hash + Send + 'static> UseCase for Lane<K> {
    type Situation = Situation<K>;
    type Action = Action<K>;
    type Outcome = Outcome;
    const NAME: &'static str = "lane";

    fn envelope(situation: &Situation<K>) -> Envelope<Action<K>> {
        // A mind past her deadline goes next: the most overdue, whatever any policy prefers.
        let overdue = situation.ready.iter().filter(|mind| mind.waited >= mind.deadline).max_by_key(|mind| mind.waited.saturating_sub(mind.deadline));
        if let Some(mind) = overdue {
            return Envelope::Forced(Action::Switch(mind.mind.clone()));
        }
        let Some(running) = &situation.running else {
            return match situation.ready.first() {
                Some(first) => Envelope::Admissible(NonEmpty::new(Action::Switch(first.mind.clone()), situation.ready.iter().skip(1).map(|mind| Action::Switch(mind.mind.clone())))),
                None => Envelope::Forced(Action::Idle),
            };
        };
        // Finish first: her measured remainder costs less than bringing anyone in.
        let finishing = matches!((running.remaining.value(), running.step.value()), (Some(left), Some(step)) if {
            let cheapest_switch = situation.ready.iter().filter_map(|mind| mind.switch_in.value()).min();
            cheapest_switch.is_some_and(|switch| step * left < switch)
        });
        if finishing || situation.ready.is_empty() {
            return Envelope::Forced(Action::Continue);
        }
        let switches = situation.ready.iter().map(|mind| Action::Switch(mind.mind.clone()));
        let batch = (situation.batch_width > 1).then(|| {
            let width = situation.batch_width as usize;
            Action::Batch(std::iter::once(&running.mind).chain(situation.ready.iter().map(|mind| &mind.mind)).take(width).cloned().collect())
        });
        Envelope::Admissible(NonEmpty::new(Action::Continue, switches.chain(batch)))
    }

    fn constraint(outcome: &Outcome) -> Option<Violation> {
        (outcome.worst_wait > outcome.worst_deadline).then_some(Violation {
            constraint: "a ready mind's wait bound (s)",
            measured: outcome.worst_wait.as_secs_f64(),
            limit: outcome.worst_deadline.as_secs_f64(),
        })
    }
}
