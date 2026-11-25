//! The lane's cost model: an algorithm over the lane's history. The running mind continues while her
//! next step (or her whole predicted remainder) fits inside every waiter's slack: her bound, less her
//! wait, less what bringing her in is measured to cost. When it no longer fits, the waiter with the
//! least slack is brought in. A more urgent mind is brought in at once. So a deep mind whose switch is
//! costly keeps the Core longer, and a waiter near her bound is served before she reaches it: run
//! lengths follow from her measured costs, never a quantum.
//! hot-path: the lane decides at every Core step.

use std::fmt::Debug;
use std::time::Duration;

use super::super::policy::{Choice, Param, Policy, PolicyId, Refused};
use super::lane::{Action, Lane, Mind, Situation, Slot, SLOTS};

/// The cost model, with its one parameter.
#[derive(Debug, Clone, Copy)]
pub struct CostModel {
    /// The share of a waiter's slack the running mind may use before the waiter is brought in, in
    /// (0, 1]: lower switches sooner. A parameter to tune and learn, saved with the algorithm.
    margin: f64,
}

impl Default for CostModel {
    fn default() -> Self {
        Self { margin: 0.5 } // cold: the starting margin; set_param and load move it
    }
}

/// A waiter's slack: her bound less her wait, less her measured switch cost (one not yet measured is a
/// cost this run has not seen, so none is subtracted).
fn slack<K>(mind: &Mind<K>) -> Duration {
    mind.deadline.saturating_sub(mind.waited).saturating_sub(mind.switch_in.value().unwrap_or(Duration::ZERO)) // unwrap_or: an unmeasured switch is a cost not yet seen
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
    }

    fn load(&mut self, from: &[u8]) -> Result<(), Refused> {
        let bytes: [u8; 8] = from.try_into().map_err(|_| Refused::State { algorithm: "cost-model", bytes: from.len() })?;
        self.set_margin(f64::from_le_bytes(bytes))
    }
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
}
