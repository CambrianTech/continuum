//! The lane's cost model: an algorithm over the lane's history. The running mind continues while her
//! next step (or her whole predicted remainder) fits inside the tightest waiter's slack: her bound,
//! less her wait, less what bringing her in is measured to cost, times a learned `margin`. When it no
//! longer fits, that waiter is brought in; a more urgent mind is brought in at once. So a deep mind
//! whose switch is costly is served before her bound, and a long turn runs while it can: run lengths
//! follow from measured costs, never a quantum.
//!
//! The margin is never a constant. It comes from this node's saved state when one exists (the owner
//! keys it by algorithm and served model, so it is learned per model class); otherwise it is seeded
//! by a procedure at the first decision that needs it, from what was just measured: keep exactly one
//! more of the running mind's measured steps as headroom, `(slack - step) / slack`, so the one risk the
//! check leaves unbudgeted (a step overrunning her mean) is covered once. Until a step is measured the
//! model cannot seed, and switches, which keeps the bound.
//!
//! From there the margin is a control loop both ways: every closed decision with a mind waiting is
//! one reading. A wait past its bound pushes it down (switch sooner); every waiter inside her bound
//! pushes it up (run longer). The step is adaptive (Rprop): it starts as the seeded headroom, grows
//! while readings agree and halves when they flip, so the margin settles at the edge this node's
//! load allows and follows it when the load changes.
//! hot-path: the lane decides at every Core step.

use std::fmt::Debug;
use std::time::Duration;

use super::super::policy::{Choice, Param, Policy, PolicyId, Refused};
use super::super::record::Record;
use super::lane::{Action, Lane, Situation, Slot, SLOTS};

/// The cost model: unseeded until its first contended decision with a measured step, or its saved
/// state, gives it a margin.
#[derive(Debug, Clone, Copy, Default)]
pub struct CostModel {
    /// Advanced by every change to what it has learned (seeded, a reading, set, loaded): what a record
    /// names as the parameters that made its choice.
    revision: u64,
    learned: Option<Learned>,
}

/// What the model has learned.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Learned {
    /// The share of a waiter's slack the running mind may use before the waiter is brought in, in
    /// (0, 1]: lower switches sooner.
    margin: f64,
    /// The margin's current step, as a fraction of the margin, and the last reading's direction.
    step: f64,
    last: Direction,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
enum Direction {
    None = 0,
    Up = 1,
    Down = 2,
}

/// How the step adapts: grown while readings agree, halved when they flip (Rprop's published
/// factors). It needs no bounds of its own: it grows only while the margin can still move, so a
/// margin pinned at the whole slack never inflates it, and a halved step regrows on agreement.
const GROW: f64 = 1.2;
const SHRINK: f64 = 0.5;

/// The learned state's bytes: margin, step, last direction.
const STATE: usize = 17;

/// A margin in (0, 1], refused otherwise (and NaN), never clamped.
fn valid(margin: f64) -> Result<f64, Refused> {
    if margin > 0.0 && margin <= 1.0 { Ok(margin) } else { Err(Refused::Value { param: "margin", value: Param::Real(margin) }) }
}

impl Learned {
    /// The procedure: keep one more of her measured steps as headroom in this slack. `None` when the
    /// slack does not hold even one step (nothing to seed from; the waiter is brought in).
    fn seeded(slack: Duration, step: Duration) -> Option<Self> {
        let margin = slack.checked_sub(step)?.as_secs_f64() / slack.as_secs_f64();
        valid(margin).ok().map(|margin| Self { margin, step: 1.0 - margin, last: Direction::None })
    }

    /// One reading of the loop: move the margin by its step in `direction`, multiplicatively so it
    /// never reaches 0, and never past 1 (the whole slack).
    fn adjust(&mut self, direction: Direction) {
        let pinned = direction == Direction::Up && self.margin >= 1.0;
        self.step = match self.last {
            Direction::None => self.step,
            _ if pinned => self.step,
            last if last == direction => self.step * GROW,
            _ => self.step * SHRINK,
        };
        self.last = direction;
        self.margin = match direction {
            Direction::Up => (self.margin * (1.0 + self.step)).min(1.0),
            Direction::Down => self.margin / (1.0 + self.step),
            Direction::None => self.margin,
        };
    }
}

impl<K: Copy + Debug + PartialEq + Send + 'static> Policy<Lane<K>> for CostModel {
    fn id(&self) -> PolicyId {
        PolicyId { name: "cost-model", parameters: self.revision }
    }

    fn choose(&mut self, situation: &Situation<K>, admissible: &[Action]) -> Choice {
        let ready = &situation.ready[..situation.ready.len().min(SLOTS)];
        // the tightest bounded waiter (background work has no bound to fit)
        let tightest = ready.iter().enumerate().filter_map(|(slot, mind)| mind.slack().map(|room| (slot, room))).min_by_key(|&(_, room)| room);
        // a more urgent waiter than the running mind (with none running, the most urgent of all)
        let urgent = ready.iter().enumerate().filter(|(_, mind)| situation.running.is_none_or(|running| mind.urgency > running.urgency)).max_by_key(|(_, mind)| mind.urgency);
        let wanted = match (&situation.running, urgent, tightest) {
            (_, Some((slot, _)), _) => Action::Switch(slot as Slot),
            (None, None, _) => Action::Idle,
            (Some(_), None, None) => Action::Continue,
            (Some(running), None, Some((slot, room))) => {
                let next_step = running.step.value();
                if self.learned.is_none() {
                    self.learned = next_step.and_then(|step| Learned::seeded(room, step));
                    self.revision += u64::from(self.learned.is_some());
                }
                let fits = self.learned.is_some_and(|learned| {
                    let budget = room.mul_f64(learned.margin);
                    let remainder = running.remaining.value().zip(next_step).map(|(left, step)| step.mul_f64(left as f64));
                    remainder.is_some_and(|rest| rest <= budget) || next_step.is_some_and(|step| step <= budget)
                });
                if fits { Action::Continue } else { Action::Switch(slot as Slot) }
            }
        };
        Choice::wanted_or_envelopes_first(admissible, &wanted)
    }

    /// A reading only when the decision was the margin's to make: a mind running and another waiting,
    /// with a margin to move. A wait past its bound reads down; every waiter inside hers reads up.
    fn learn(&mut self, record: &Record<Lane<K>>) {
        let (Some(learned), Some(_), false, Some(outcome)) = (&mut self.learned, &record.situation.running, record.situation.ready.is_empty(), &record.outcome) else { return };
        learned.adjust(if outcome.worst_wait > outcome.worst_deadline { Direction::Down } else { Direction::Up });
        self.revision += 1;
    }

    fn params(&self) -> &'static [&'static str] {
        &["margin"]
    }

    /// The margin once seeded or loaded; `None` before.
    fn param(&self, name: &str) -> Option<Param> {
        self.learned.filter(|_| name == "margin").map(|learned| Param::Real(learned.margin))
    }

    /// Setting the margin seeds it, with its headroom as the first step.
    fn set_param(&mut self, name: &'static str, value: Param) -> Result<(), Refused> {
        match (name, value) {
            ("margin", Param::Real(margin)) => {
                let margin = valid(margin)?;
                self.learned = Some(Learned { margin, step: 1.0 - margin, last: Direction::None });
                self.revision += 1;
                Ok(())
            }
            ("margin", value) => Err(Refused::Value { param: name, value }),
            _ => Err(Refused::Unknown(name)),
        }
    }

    /// Nothing learned saves nothing, so the next start seeds again.
    fn save(&self, into: &mut Vec<u8>) {
        let Some(learned) = self.learned else { return };
        into.extend_from_slice(&learned.margin.to_le_bytes());
        into.extend_from_slice(&learned.step.to_le_bytes());
        into.push(learned.last as u8);
    }

    /// Empty bytes (nothing saved) leave it unseeded; anything else is its own saved state or refused
    /// whole.
    fn load(&mut self, from: &[u8]) -> Result<(), Refused> {
        if from.is_empty() {
            return Ok(());
        }
        let refused = Refused::State { algorithm: "cost-model", bytes: from.len() };
        let bytes: &[u8; STATE] = from.try_into().map_err(|_| refused)?;
        let real = |at: usize| bytes[at..at + 8].try_into().map(f64::from_le_bytes).map_err(|_| refused);
        let (margin, step) = (valid(real(0)?)?, real(8)?);
        let last = match bytes[16] {
            0 => Direction::None,
            1 => Direction::Up,
            2 => Direction::Down,
            _ => return Err(refused),
        };
        if !(step.is_finite() && step > 0.0) {
            return Err(refused);
        }
        self.learned = Some(Learned { margin, step, last });
        self.revision += 1;
        Ok(())
    }
}
