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
    /// Tokens of her prompt still to prefill, from position `at` (how deep her next chunk starts).
    Prefill { remaining: u64, at: u64 },
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
    /// Her wait bound at this urgency (`Urgency::wait_bound`): before the next step would carry her past
    /// it, the envelope steps her. `None` for background work, whose guarantee is its class's queue
    /// staying in check (`Situation::background_owed`).
    pub deadline: Option<Duration>,
    pub phase: Phase,
    /// Her KV's depth in bytes, and how much of it is not resident (the block layer knows exactly).
    pub kv_bytes: Measured<u64>,
    pub cold_bytes: Measured<u64>,
    /// Tokens left in her turn, predicted from her own history in this activity.
    pub remaining: Measured<u64>,
    /// Her step time, and the cost of bringing her in from where she is now.
    pub step: Measured<Duration>,
    pub switch_in: Measured<Duration>,
    /// The fixed time a step of hers costs whatever its tokens (a prefill chunk's submit and its pass
    /// over her context), apart from `step`, her time a token; `Unmeasured` until her steps vary in size.
    pub overhead: Measured<Duration>,
    /// What each position of depth adds to a prefill token's time, in seconds (`ChunkCost::depth`): her chunk's
    /// tokens cost more the deeper they land. `step` is already her part a token at the chunk's start.
    pub depth: Measured<f64>,
    /// Which genes she wears, as an identity: minds decode in one batch only under the same genes (adapters
    /// apply to a whole step), so a mind beside the running one with others is never a batch member.
    pub genes: u64,
    /// Her prefix cache's hit rate: how much of her work reuses what is warm.
    pub prefix_hit: Measured<f64>,
}

impl<K> Mind<K> {
    /// What is left of her bound once her wait and her measured switch cost are spent (an unmeasured
    /// switch is a cost not yet seen); `None` when she has no per-item bound (background work).
    pub fn slack(&self) -> Option<Duration> {
        self.deadline.map(|bound| bound.saturating_sub(self.waited).saturating_sub(self.switch_in.value().unwrap_or(Duration::ZERO))) // unwrap_or: an unmeasured switch is a cost not yet seen
    }

    /// The time her features are measured against: her bound, or for background work the bound acts
    /// are held to, so every mind's times are on one scale.
    pub fn scale(&self) -> Duration {
        self.deadline.unwrap_or_else(Urgency::act_bound) // unwrap_or_else: background work has no bound of its own, so it is measured on the act bound's scale by design
    }
}

/// The Core at a step boundary: who holds it, and who is ready.
#[derive(Debug, Clone, PartialEq)]
pub struct Situation<K> {
    pub running: Option<Mind<K>>,
    pub ready: Vec<Mind<K>>,
    /// The widest batch the Core can step now (1: no batching).
    pub batch_width: u32,
    /// The background class (minds with no per-item bound) is owed a step: its served rate has fallen
    /// behind its arrival rate, so its queue would grow (`lane_history::LaneHistory::background`).
    pub background_owed: bool,
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
    /// Of `engine`, the time spent bringing a mind in (zero when no one was): the Core's transition
    /// time, which serves no mind.
    pub switching: Duration,
    /// The share of the stepped mind's remaining phase this decision finished ([`served`]):
    /// `Unmeasured` when no one mind stepped or her remainder was not predicted.
    pub served: Measured<f64>,
}

/// How long the next step may take and still leave every bounded waiter (other than `stepping`) her
/// turn inside her bound: waiters are reached one a step, so a waiter with `k` ahead of her (less slack,
/// or as little and earlier) is reached after `k` turns, each as long as the longest waiter's measured
/// step and switch. Her room is her slack less those turns; the queue's room is the least of them.
/// `None` when no bounded mind waits. Counting only one step ahead, or the running mind's time a token
/// for a step, let a crowd overrun by a step apiece (the M5, 2026-11-25 21:45 to 21:55Z: 1,005 act
/// violations at p50 1.02 of the bound).
pub fn room<K: PartialEq>(situation: &Situation<K>, stepping: Option<&K>) -> Option<Duration> {
    let waiting = || situation.running.iter().chain(situation.ready.iter().take(SLOTS)).filter(move |mind| stepping != Some(&mind.mind)).filter_map(|mind| mind.slack().map(|left| (mind, left)));
    let turn = waiting().map(|(mind, _)| mind.step.value().unwrap_or(Duration::ZERO) + mind.switch_in.value().unwrap_or(Duration::ZERO)).max()?; // unwrap_or: an unmeasured step or switch is a cost not yet seen
    waiting()
        .enumerate()
        .map(|(at, (_, left))| {
            let ahead = waiting().enumerate().filter(|&(other_at, (_, other))| (other, other_at) < (left, at)).count() as u32;
            left.saturating_sub(turn * ahead)
        })
        .min()
}

/// A batch for the running mind and the ready mind in `slot` with the other decoding ready minds, the most pressed
/// first, as many as the streams beside her hold; `None` when she or the running mind does not decode, or the Core
/// binds one stream. A batch is one decode step: a mind still prefilling is never in one.
fn batch_with<K>(situation: &Situation<K>, slot: usize) -> Option<Mask> {
    let decoding = |mind: &Mind<K>| matches!(mind.phase, Phase::Decode);
    let width = (situation.batch_width as usize).saturating_sub(1);
    let ready = &situation.ready[..situation.ready.len().min(SLOTS)];
    let running = situation.running.as_ref()?;
    let decoding = |mind: &Mind<K>| decoding(mind) && mind.genes == running.genes; // a batch is one step under one gene set
    if width == 0 || !decoding(running) || !ready.get(slot).is_some_and(decoding) {
        return None;
    }
    let mut mask: Mask = 1 << slot;
    for _ in 1..width {
        let slack = |mind: &Mind<K>| mind.slack().unwrap_or(Duration::MAX); // unwrap_or: background work has no bound, so it is the least pressed
        let next = ready.iter().enumerate().filter(|&(at, mind)| mask & (1 << at) == 0 && decoding(mind)).min_by_key(|&(_, mind)| slack(mind));
        match next {
            Some((at, _)) => mask |= 1 << at,
            None => break,
        }
    }
    Some(mask)
}

/// The least a step of hers is worth taking: a decode step, or a prefill chunk long enough that its
/// tokens take at least as long as its fixed part (twice that part), so the envelope serves the queue
/// before her room shrinks to chunks that pay a full step for a token. A step or part not yet measured
/// is a cost not yet seen.
fn worthwhile_step<K>(mind: &Mind<K>) -> Duration {
    match (mind.overhead.value(), mind.step.value()) {
        (Some(fixed), _) => fixed * 2,
        (None, Some(step)) => step,
        (None, None) => Duration::ZERO,
    }
}

/// How many tokens one step of `stepped` may move, at most `most` (the engine's batch). A step cannot
/// be preempted, so the bound every other mind is promised is kept only if the step ends inside the
/// queue's room (`room`: every waiter's slack less the turns ahead of her) at the
/// stepped mind's measured time a token. With no one else ready, or her time a token not yet measured
/// (her first step, which measures it), the whole batch. At least one token, so she always advances.
/// The M5, 2026-11-25 19:40 to 19:50Z: prefill chunks a whole batch long left other minds 7 to 9 s
/// without a step under a 1 s bound.
pub fn step_tokens<K: PartialEq>(situation: &Situation<K>, stepped: &Mind<K>, most: std::num::NonZeroU64) -> u64 {
    let room = room(situation, Some(&stepped.mind));
    // the chunk's fixed part comes off the room first; what is left buys tokens at her part a token, each
    // costing `depth` more per position past the chunk's start: n·step + depth·n²/2 within the room
    let fixed = stepped.overhead.value().unwrap_or(Duration::ZERO); // unwrap_or: an unmeasured fixed part is a cost not yet seen
    let depth = stepped.depth.value().unwrap_or(0.0); // unwrap_or: an unmeasured depth part is a cost not yet seen
    match (room, stepped.step.value()) {
        (Some(room), Some(per_token)) if per_token > Duration::ZERO => {
            let (budget, step) = (room.saturating_sub(fixed).as_secs_f64(), per_token.as_secs_f64());
            let tokens = if depth > 0.0 { ((step * step + 2.0 * depth * budget).sqrt() - step) / depth } else { budget / step };
            (tokens as u64).clamp(1, most.get())
        }
        _ => most.get(),
    }
}

/// The share of her remaining phase a decision finished: `moved` of the `left` tokens she had when
/// it was made (at most all of it; a remainder predicted short is finished, not exceeded).
pub fn served(moved: u64, left: Measured<u64>) -> Measured<f64> {
    match left {
        Measured::Value(left) if left > 0 => Measured::Value(moved.min(left) as f64 / left as f64),
        _ => Measured::Unmeasured,
    }
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
        // A mind the next step would carry past her deadline goes next (the most pressed), whatever
        // any policy prefers: a step cannot be preempted, so forcing her only once she is past it
        // misses the bound by a step every time (the M5, 2026-11-25 19:56 to 20:06Z: 1,037 violations,
        // p50 1.014 of the bound). The next step is the running mind's measured one; a step not yet
        // measured is a cost not yet seen.
        //
        // Waiters are served one a step, so a waiter with `k` more pressed ahead of her is reached only
        // after `k + 1` steps: the most pressed goes next as soon as any waiter's slack is within her
        // rank's steps (earliest deadline first, counting the queue). Checked one at a time, a crowd
        // nearing the bound together each looked safe and overran by a step apiece (the M5, 2026-11-25
        // 20:54 to 21:04Z: 929 violations, p50 1.02 of the bound, twelve resident turns at once).
        let next = situation.running.as_ref().map_or(Duration::ZERO, worthwhile_step); // map_or: no one running takes no step
        // Only minds with a per-item bound crowd it (`room`); background work has none.
        let crowded = room(situation, situation.running.as_ref().map(|running| &running.mind)).is_some_and(|room| room <= next);
        let overdue = ready.iter().enumerate().filter(|_| crowded).filter_map(|(slot, mind)| mind.slack().map(|left| (slot, left))).min_by_key(|&(_, left)| left);
        if let Some((slot, _)) = overdue {
            // a waiter at her bound who decodes is served by a batch with the running mind: one step, both advance
            if let Some(mask) = batch_with(situation, slot) {
                return Gate::Forced(Action::Batch(mask));
            }
            return Gate::Forced(Action::Switch(slot as Slot));
        }
        // Background work's guarantee: when its class is owed (served behind arrival, its queue would
        // grow), it takes this step, the one running it continuing or its longest waiter brought in.
        if situation.background_owed {
            if situation.running.as_ref().is_some_and(|running| running.deadline.is_none()) {
                return Gate::Forced(Action::Continue);
            }
            if let Some((slot, _)) = ready.iter().enumerate().filter(|(_, mind)| mind.deadline.is_none()).max_by_key(|(_, mind)| mind.waited) {
                return Gate::Forced(Action::Switch(slot as Slot));
            }
        }
        // Priority classes, the bound as aging (CLAUDE.md goal 9): a policy chooses only among the most
        // urgent minds present; a less urgent one steps when none is ready, or when her bound forces
        // her above. A policy's objective (finish the least work left first) would otherwise serve
        // short background work ahead of a mind's act (the M5, 2026-11-25 21:09 to 21:19Z: dream
        // consolidation at Content took 62% of the Core from residents at Want).
        let top = ready.iter().map(|mind| mind.urgency).chain(situation.running.iter().map(|running| running.urgency)).max();
        let in_class = |mind: &Mind<K>| Some(mind.urgency) == top;
        let switches = ready.iter().enumerate().filter(|(_, mind)| in_class(mind)).map(|(slot, _)| Action::Switch(slot as Slot));
        let Some(running) = &situation.running else {
            if ready.is_empty() {
                return Gate::Forced(Action::Idle);
            }
            admissible.extend(switches);
            return Gate::Choose;
        };
        // Finish first: her measured remainder costs less than bringing anyone in.
        let cheapest_switch = ready.iter().filter_map(|mind| mind.switch_in.value()).min();
        let finishing = matches!((running.remaining.value(), running.step.value(), cheapest_switch), (Some(left), Some(step), Some(switch)) if step.mul_f64(left as f64) < switch);
        let outranked = !in_class(running);
        if (finishing && !outranked) || ready.is_empty() || !ready.iter().any(in_class) {
            return Gate::Forced(Action::Continue);
        }
        if !outranked {
            admissible.push(Action::Continue);
        }
        admissible.extend(switches);
        // the running mind and the ready minds of the top class that decode, as many as the streams beside her hold:
        // a batch is one decode step, so a mind still prefilling is never in one
        let decoding = |mind: &Mind<K>| matches!(mind.phase, Phase::Decode);
        let width = (situation.batch_width as usize).saturating_sub(1);
        if width > 0 && !outranked && decoding(running) {
            let mask = ready.iter().enumerate().filter(|(_, mind)| in_class(mind) && decoding(mind) && mind.genes == running.genes).take(width).fold(0 as Mask, |mask, (slot, _)| mask | (1 << slot));
            if mask != 0 {
                // One batch step serves the running mind and every member at about one step's cost (the M5, 2026-11-28:
                // four minds 67.1 -> 141.6 tok/s aggregate; the 5090: 160.0 -> 435.8), so continuing her alone, or
                // switching to a member and leaving her waiting, is dominated by it and not offered. A switch to a
                // mind outside the batch (one still prefilling) stays the policy's to weigh against it.
                admissible.retain(|action| match action {
                    Action::Continue => false,
                    Action::Switch(slot) => mask & (1 << slot) == 0,
                    Action::Batch(_) | Action::Idle => true,
                });
                admissible.push(Action::Batch(mask));
            }
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
        slot.background_owed = from.background_owed;
    }

    fn empty() -> Situation<K> {
        Situation { running: None, ready: Vec::new(), batch_width: 1, background_owed: false } // cold: an empty slot is built once, at the decider's construction
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
        Choice::wanted_or_envelopes_first(admissible, &wanted)
    }
}

/// The algorithm serving runs unless its configuration names another: the stock linear UCB over the
/// lane's features (on trial live against the cost model, which it matched in simulation).
pub const SERVING: &str = super::super::linear_ucb::NAME;

/// How many decisions the lane keeps for replay, and how many a learner remembers: one per step, so
/// at 26 ms a step about three and a half minutes, what replay can check a learner against.
pub const DECISIONS: std::num::NonZeroUsize = match std::num::NonZeroUsize::new(8_192) {
    Some(decisions) => decisions,
    None => std::num::NonZeroUsize::MIN,
};

/// The lane's algorithms by name: the baseline, need-first, the cost model over the lane's history,
/// and the stock linear UCB over the lane's features. A caller (the Core's driver) creates the one
/// its configuration names.
pub fn algorithms<K: Copy + Debug + PartialEq + Send + 'static>() -> Result<super::super::Algorithms<Lane<K>>, super::super::Unknown> {
    use super::lane_features::{FEATURES, TARGETS};
    super::super::Algorithms::default()
        .register("baseline", || Box::new(super::super::Baseline))? // cold: the registry is built once, when the lane is
        .register("need-first", || Box::new(NeedFirst))? // cold: the registry is built once, when the lane is
        .register("cost-model", || Box::new(super::lane_cost::CostModel::default()))? // cold: the registry is built once, when the lane is
        .register(super::super::linear_ucb::NAME, || Box::new(super::super::LinearUcb::<Lane<K>, FEATURES, TARGETS>::new(DECISIONS))) // cold: the registry is built once, when the lane is
}
