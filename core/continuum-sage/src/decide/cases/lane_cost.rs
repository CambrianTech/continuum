//! The lane's cost model: an algorithm over the lane's history. The running mind continues while her
//! next step (or her whole predicted remainder) fits inside the tightest waiter's slack: her bound,
//! less her wait, less what bringing her in is measured to cost, times a learned `margin`. When it no
//! longer fits, that waiter is brought in; a more urgent mind is brought in at once. So a deep mind
//! whose switch is costly is served before her bound, and a long turn runs while it can: run lengths
//! follow from measured costs, never a quantum.
//!
//! The margin is a control loop that moves both ways: every closed decision with a mind waiting is
//! one reading. A wait past its bound pushes the margin down (switch sooner); a window with every
//! waiter inside her bound pushes it up (run longer, switch less). The step is adaptive (Rprop): it
//! grows while the readings agree and halves when they flip, so the margin settles at the edge this
//! node's load allows and follows it when the load changes. Its state is saved through the owner's
//! saved state and resumed at the next start; absent, it starts from its starting value.
//! hot-path: the lane decides at every Core step.

use std::fmt::Debug;
use std::time::Duration;

use super::super::policy::{Choice, Param, Policy, PolicyId, Refused};
use super::super::record::Record;
use super::lane::{Action, Lane, Mind, Situation, Slot, SLOTS};

/// The cost model, with its learned state.
#[derive(Debug, Clone, Copy)]
pub struct CostModel {
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

/// The share of the slack a node with nothing learned starts from. The check already budgets the
/// waiter's measured switch cost and the running mind's measured step time, so the one risk it leaves
/// unbudgeted is a step running longer than her mean: the best guess spends nearly all the slack and
/// keeps a tenth for that overrun. An inference, not a measurement: the loop corrects it within a few
/// readings, and the saved state replaces it after the first stop.
pub const STARTING_MARGIN: f64 = 0.9;

/// The first step is the headroom the starting margin kept: one breach reading spends exactly what
/// was guessed for overrun, and agreement grows it from there.
const STARTING_STEP: f64 = 1.0 - STARTING_MARGIN;

impl Default for CostModel {
    /// The starting state when nothing was saved.
    fn default() -> Self {
        Self { margin: STARTING_MARGIN, step: STARTING_STEP, last: Direction::None } // cold: the starting state; learning and load move it
    }
}

/// A waiter's slack: her bound less her wait, less her measured switch cost (one not yet measured is a
/// cost this run has not seen, so none is subtracted).
fn slack<K>(mind: &Mind<K>) -> Duration {
    mind.deadline.saturating_sub(mind.waited).saturating_sub(mind.switch_in.value().unwrap_or(Duration::ZERO)) // unwrap_or: an unmeasured switch is a cost not yet seen
}

impl CostModel {
    /// The margin, refused outside (0, 1] (and NaN), never clamped.
    fn set_margin(&mut self, margin: f64) -> Result<(), Refused> {
        if margin > 0.0 && margin <= 1.0 {
            self.margin = margin;
            Ok(())
        } else {
            Err(Refused::Value { param: "margin", value: Param::Real(margin) })
        }
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
        PolicyId { name: "cost-model", parameters: self.margin.to_bits() }
    }

    fn choose(&mut self, situation: &Situation<K>, admissible: &[Action]) -> Choice {
        let ready = &situation.ready[..situation.ready.len().min(SLOTS)];
        let tightest = ready.iter().enumerate().min_by_key(|(_, mind)| slack(mind));
        // a more urgent waiter than the running mind (with none running, the most urgent of all)
        let urgent = ready.iter().enumerate().filter(|(_, mind)| situation.running.is_none_or(|running| mind.urgency > running.urgency)).max_by_key(|(_, mind)| mind.urgency);
        let wanted = match (&situation.running, urgent, tightest) {
            (_, Some((slot, _)), _) => Action::Switch(slot as Slot),
            (None, None, _) => Action::Idle,
            (Some(_), None, None) => Action::Continue,
            (Some(running), None, Some((slot, waiter))) => {
                let budget = slack(waiter).mul_f64(self.margin);
                let next_step = running.step.value();
                let remainder = running.remaining.value().zip(next_step).map(|(left, step)| step.mul_f64(left as f64));
                let fits = remainder.is_some_and(|rest| rest <= budget) || next_step.is_some_and(|step| step <= budget);
                if fits { Action::Continue } else { Action::Switch(slot as Slot) }
            }
        };
        let index = admissible.iter().position(|action| *action == wanted).unwrap_or(0); // unwrap_or: not offered means the envelope decided; its first is its choice
        Choice { index, expected: None }
    }

    /// A reading only when the decision was the margin's to make: a mind running and another waiting.
    /// A wait past its bound reads down; every waiter inside hers reads up.
    fn learn(&mut self, record: &Record<Lane<K>>) {
        let (Some(_), false, Some(outcome)) = (&record.situation.running, record.situation.ready.is_empty(), &record.outcome) else { return };
        self.adjust(if outcome.worst_wait > outcome.worst_deadline { Direction::Down } else { Direction::Up });
    }

    fn params(&self) -> &'static [&'static str] {
        &["margin"]
    }

    fn param(&self, name: &str) -> Option<Param> {
        (name == "margin").then_some(Param::Real(self.margin))
    }

    fn set_param(&mut self, name: &'static str, value: Param) -> Result<(), Refused> {
        match (name, value) {
            ("margin", Param::Real(margin)) => self.set_margin(margin),
            ("margin", value) => Err(Refused::Value { param: name, value }),
            _ => Err(Refused::Unknown(name)),
        }
    }

    fn save(&self, into: &mut Vec<u8>) {
        into.extend_from_slice(&self.margin.to_le_bytes());
        into.extend_from_slice(&self.step.to_le_bytes());
        into.push(self.last as u8);
    }

    /// Empty bytes (nothing saved yet) keep the starting state; anything else is its own saved state
    /// or refused whole.
    fn load(&mut self, from: &[u8]) -> Result<(), Refused> {
        if from.is_empty() {
            return Ok(());
        }
        let refused = Refused::State { algorithm: "cost-model", bytes: from.len() };
        let bytes: &[u8; STATE] = from.try_into().map_err(|_| refused)?;
        let real = |at: usize| bytes[at..at + 8].try_into().map(f64::from_le_bytes).map_err(|_| refused);
        let (margin, step) = (real(0)?, real(8)?);
        let last = match bytes[16] {
            0 => Direction::None,
            1 => Direction::Up,
            2 => Direction::Down,
            _ => return Err(refused),
        };
        if !(step.is_finite() && step > 0.0) {
            return Err(refused);
        }
        self.set_margin(margin)?;
        (self.step, self.last) = (step, last);
        Ok(())
    }
}
