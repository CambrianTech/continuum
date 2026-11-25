//! A stock policy: a linear upper-confidence bandit with constraints, over any use case that states
//! its features and what its outcomes measured ([`Featured`]). Nothing in it knows a lane.
//! hot-path: the lane decides at every Core step.
//!
//! One [`Ridge`] predicts, for each admissible action, every physical target the use case measures:
//! target 0 is the value to maximize (the lane's tokens per second), the rest are loads to keep at or
//! under one (the lane's worst wait against its bound). An action is feasible when every load's
//! pessimistic prediction (mean plus one standard error) is at most one; among feasible actions the
//! one with the highest optimistic value (mean plus one standard error) is chosen, so what has been
//! tried least is tried until its standard error falls (optimism in the face of uncertainty). With no
//! feasible action the one with the lowest pessimistic load is chosen. No weight trades a load against
//! the value (Codex's alternative on #6280: predict physical outcomes, never a fixed penalty), and the
//! use case's envelope still forces whatever its bound requires.
//!
//! The confidence width is one standard error in the target's own measured units, so there is no
//! exploration constant: a target that varies little explores little.

use std::marker::PhantomData;
use std::num::NonZeroUsize;

use super::measured::Measured;
use super::policy::{Choice, Param, Policy, PolicyId, Refused};
use super::record::Record;
use super::ridge::Ridge;
use super::use_case::UseCase;

/// A use case a linear learner can read: `D` features of a candidate action in a situation, each
/// scaled to about one, and `M` physical targets of an outcome.
pub trait Featured<const D: usize, const M: usize>: UseCase {
    fn features(situation: &Self::Situation, action: Self::Action) -> [f64; D];

    /// What the outcome measured: target 0 maximized, the rest loads kept at or under one. `None`
    /// when it measured nothing to learn from.
    fn measured(outcome: &Self::Outcome) -> Option<[f64; M]>;
}

/// The linear UCB over a [`Featured`] use case.
pub struct LinearUcb<U, const D: usize, const M: usize> {
    model: Ridge<D, M>,
    revision: u64,
    use_case: PhantomData<fn() -> U>,
}

/// The name it registers under.
pub const NAME: &str = "linear-ucb";

impl<U, const D: usize, const M: usize> LinearUcb<U, D, M> {
    /// It remembers about `memory` outcomes (each older one weighs `1/e` less per `memory`).
    pub fn new(memory: NonZeroUsize) -> Self {
        Self { model: Ridge::new(memory), revision: 0, use_case: PhantomData }
    }

    fn scale(&self, target: usize) -> f64 {
        match self.model.scale(target) {
            Measured::Value(scale) => scale,
            // no error seen yet: no width to add, so the first choices are the envelope's order
            Measured::Unmeasured => 0.0,
        }
    }
}

impl<U: Featured<D, M>, const D: usize, const M: usize> Policy<U> for LinearUcb<U, D, M> {
    fn id(&self) -> PolicyId {
        PolicyId { name: NAME, parameters: self.revision }
    }

    fn choose(&mut self, situation: &U::Situation, admissible: &[U::Action]) -> Choice {
        // (index, optimistic value, mean value) of the best feasible; (index, pessimistic load, mean) of the safest
        let mut best: Option<(usize, f64, f64)> = None;
        let mut safest: Option<(usize, f64, f64)> = None;
        for (index, action) in admissible.iter().enumerate() {
            let predicted = self.model.predict(&U::features(situation, *action));
            let load = (1..M).map(|m| predicted.mean[m] + self.scale(m) * predicted.width).fold(f64::NEG_INFINITY, f64::max);
            let value = predicted.mean[0] + self.scale(0) * predicted.width;
            if load <= 1.0 && best.is_none_or(|(_, top, _)| value > top) {
                best = Some((index, value, predicted.mean[0]));
            }
            if safest.is_none_or(|(_, low, _)| load < low) {
                safest = Some((index, load, predicted.mean[0]));
            }
        }
        match best.or(safest) {
            Some((index, _, expected)) => Choice { index, expected: Some(expected) },
            None => Choice { index: 0, expected: None },
        }
    }

    fn learn(&mut self, record: &Record<U>) {
        let (Some(action), Some(outcome)) = (record.action(), &record.outcome) else { return };
        let Some(measured) = U::measured(outcome) else { return };
        match self.model.observe(&U::features(&record.situation, action), &measured) {
            Ok(()) => self.revision += 1,
            Err(refused) => continuum_base::probe!(class = "sage.decide.unlearned", use_case = U::NAME, refused = ?refused, "an outcome the linear learner could not take"),
        }
    }

    fn params(&self) -> &'static [&'static str] {
        &["memory"]
    }

    fn param(&self, name: &str) -> Option<Param> {
        (name == "memory").then(|| Param::Real(self.model.memory()))
    }

    fn save(&self, into: &mut Vec<u8>) {
        self.model.save(into);
    }

    /// Empty bytes leave it unlearned; anything else is its own shape or refused whole.
    fn load(&mut self, from: &[u8]) -> Result<(), Refused> {
        if from.is_empty() {
            return Ok(());
        }
        self.model.load(from, NAME)?;
        self.revision += 1;
        Ok(())
    }
}
