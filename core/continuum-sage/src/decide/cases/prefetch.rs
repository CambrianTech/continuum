//! Prefetch: how far ahead of her need the Streamer starts bringing a parked mind's block back (Joel, 2026-11-26:
//! "If it's by a learning algorithm with feedback it'll figure it out"; "super important to use our extensible
//! algorithms"). Never a fixed lead: each forecast mind is a decision, its outcome measured both ways.
//!
//! - Late: her move had not landed when the lane needed her, and the lane waited. The lead was too short.
//! - Wasted: her move never served. It was abandoned in flight for a mind needed sooner (its link time lost), or she
//!   landed and was parked again unused (her room held for nothing). The lead was too long. A move that lands and
//!   then serves costs nothing however early it landed: room no one else wanted is no loss.
//!
//! A lead is a margin over her predicted copy, in units of how wrong the need prediction has measured (its spread):
//! the learner learns how many spreads are worth the room. The 5090's day in replay (slice/sim.rs, 2026-11-26) is
//! why: 129 s of 900 waiting on whole copies, 34 s with a prefetch started when the next mind was known, most of
//! the rest switches whose need nobody predicted.

use std::num::NonZeroU64;
use std::time::Duration;

use super::super::linear_ucb::Featured;
use super::super::measured::Measured;
use super::super::use_case::{Gate, UseCase};

/// One forecast mind, as the Streamer's owner sees her when it decides.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Situation {
    pub bytes: u64,
    /// Her whole copy at the link's measured rate.
    pub copy: Measured<Duration>,
    /// When she is predicted to be needed, from now (the lane's slice, or her measured time off the lane).
    pub until: Measured<Duration>,
    /// How far that prediction has measured wrong (a typical miss).
    pub spread: Measured<Duration>,
    /// Device room beside what is in use now: whether she fits at all.
    pub room: u64,
    /// The device's whole KV room: what her share of it is read against.
    pub capacity: NonZeroU64,
    /// The lane's wait bound: every time is read against it.
    pub bound: Duration,
}

/// When to start her move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Now, however early that is.
    Now,
    /// Her copy plus this many spreads before her predicted need.
    Margin(u8),
    /// Only when she is needed: no prefetch (the room is not worth it, or there is nothing to move).
    AtNeed,
}

/// The margins offered, in spreads.
pub const MARGINS: [u8; 4] = [0, 1, 2, 3];

/// What her move measured once the lane brought her in.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Outcome {
    /// The lane waited this long for her move to land.
    pub late: Duration,
    /// Link time or room spent on a move that never served her.
    pub wasted: Duration,
    pub bytes: u64,
    /// The device's whole KV room, which her share of the waste is read against.
    pub capacity: NonZeroU64,
    pub bound: Duration,
}

#[derive(Debug, Clone, Copy)]
pub struct Prefetch;

impl Situation {
    /// How long before her predicted need the move starts.
    pub fn lead(&self, action: Action) -> Duration {
        let (until, copy, spread) = (self.until.value(), self.copy.value(), self.spread.value());
        match action {
            Action::Now => until.unwrap_or(Duration::ZERO), // unwrap_or: an unpredicted need is now
            Action::AtNeed => Duration::ZERO,
            Action::Margin(k) => {
                let lead = copy.unwrap_or(Duration::ZERO) + spread.unwrap_or(Duration::ZERO) * u32::from(k); // unwrap_or: an unmeasured part adds nothing yet
                until.map_or(lead, |until| lead.min(until)) // map_or: an unpredicted need bounds nothing
            }
        }
    }
}

impl UseCase for Prefetch {
    type Situation = Situation;
    type Action = Action;
    type Outcome = Outcome;
    const NAME: &'static str = "prefetch";

    /// Nothing to move (unified memory: her copy measures zero) is forced at need; a copy never measured starts now,
    /// so its first move measures the link; no room for her is the placer's to make, never an eviction here.
    fn envelope(situation: &Situation, admissible: &mut Vec<Action>) -> Gate<Action> {
        match situation.copy {
            Measured::Value(copy) if copy.is_zero() => return Gate::Forced(Action::AtNeed),
            Measured::Unmeasured => return Gate::Forced(Action::Now),
            Measured::Value(_) => {}
        }
        if situation.room < situation.bytes {
            return Gate::Forced(Action::AtNeed);
        }
        admissible.push(Action::Now);
        admissible.extend(MARGINS.iter().map(|&k| Action::Margin(k)));
        admissible.push(Action::AtNeed);
        Gate::Choose
    }

    fn retain(slot: &mut Situation, from: &Situation) {
        *slot = *from;
    }

    fn empty() -> Situation {
        Situation { bytes: 0, copy: Measured::Unmeasured, until: Measured::Unmeasured, spread: Measured::Unmeasured, room: 0, capacity: NonZeroU64::MIN, bound: Duration::ZERO }
    }
}

/// The features: bias; the lead against the bound; the predicted lateness (copy past the lead) and the predicted
/// early hold (lead past the copy) weighed by her share of the device's room; the spread; and whether nothing is prefetched.
pub const FEATURES: usize = 6;
/// The targets: minus the lateness against the bound (maximized), and the waste against the bound weighed by
/// her share of the device's room (kept at or under one).
pub const TARGETS: usize = 2;

fn against(time: Duration, bound: Duration) -> f64 {
    time.as_secs_f64() / bound.as_secs_f64().max(f64::MIN_POSITIVE)
}

fn share(bytes: u64, capacity: NonZeroU64) -> f64 {
    bytes as f64 / capacity.get() as f64
}

impl Featured<FEATURES, TARGETS> for Prefetch {
    fn features(situation: &Situation, action: Action) -> [f64; FEATURES] {
        let lead = situation.lead(action);
        let copy = situation.copy.value().unwrap_or(Duration::ZERO); // unwrap_or: the envelope forces an unmeasured copy
        let bound = situation.bound;
        [
            1.0,
            against(lead, bound),
            against(copy.saturating_sub(lead), bound),
            against(lead.saturating_sub(copy), bound) * share(situation.bytes, situation.capacity),
            against(situation.spread.value().unwrap_or(Duration::ZERO), bound), // unwrap_or: an unmeasured spread is none seen
            f64::from(u8::from(action == Action::AtNeed)),
        ]
    }

    fn measured(outcome: &Outcome) -> Option<[f64; TARGETS]> {
        Some([-against(outcome.late, outcome.bound), against(outcome.wasted, outcome.bound) * share(outcome.bytes, outcome.capacity)])
    }
}

/// How many forecasts a learner remembers.
pub const DECISIONS: std::num::NonZeroUsize = match std::num::NonZeroUsize::new(1_024) {
    Some(decisions) => decisions,
    None => std::num::NonZeroUsize::MIN,
};

/// Prefetch's algorithms by name: the baseline (start as soon as she is parked), at-need (never prefetch), and the
/// stock linear UCB.
pub fn algorithms() -> Result<super::super::Algorithms<Prefetch>, super::super::Unknown> {
    super::super::Algorithms::default()
        .register("baseline", || Box::new(super::super::Baseline))? // cold: built once, when the Streamer's owner is
        .register("at-need", || Box::new(AtNeed))? // cold: built once
        .register(super::super::linear_ucb::NAME, || Box::new(super::super::LinearUcb::<Prefetch, FEATURES, TARGETS>::new(DECISIONS))) // cold: built once
}

/// Never prefetch: a parked mind's move starts when the lane brings her in (today's fork, a synchronous copy).
#[derive(Debug, Clone, Copy, Default)]
pub struct AtNeed;

impl super::super::Policy<Prefetch> for AtNeed {
    fn id(&self) -> super::super::PolicyId {
        super::super::PolicyId { name: "at-need", parameters: 0 }
    }

    fn choose(&mut self, _: &Situation, admissible: &[Action]) -> super::super::Choice {
        super::super::Choice::wanted_or_envelopes_first(admissible, &Action::AtNeed)
    }
}
