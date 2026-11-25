//! The lane as a [`Featured`] use case, so any stock linear learner can choose its steps: what each
//! candidate action looks like, and what its outcome physically measured.
//! hot-path: the lane decides at every Core step.
//!
//! Every feature is a time against a wait bound (so about one, whatever the chip), or an indicator.
//! An unmeasured time is a cost the lane has not seen yet, so it reads as none; the learner's
//! uncertainty about the action carries what is unknown.

use std::fmt::Debug;
use std::time::Duration;

use super::super::linear_ucb::Featured;
use super::lane::{Action, Lane, Mind, Outcome, Phase, Situation, SLOTS};

/// The features: bias; continuing, with her step, the most pressed waiter's pressure, and whether she
/// prefills; switching, with the target's switch cost, wait, step and prefill, and the pressure left
/// on the others; batching.
pub const FEATURES: usize = 12;

/// The targets: the share of the Core's time that served a mind rather than a transition (maximized;
/// tokens per second would not do, since a prefill token costs a small fraction of a decode step, so
/// it would reward switching to whoever prefills), and the worst wait against its bound (kept at or
/// under one).
pub const TARGETS: usize = 2;

fn against(time: Duration, bound: Duration) -> f64 {
    time.as_secs_f64() / bound.as_secs_f64()
}

fn measured(time: crate::decide::Measured<Duration>) -> Duration {
    time.value().unwrap_or(Duration::ZERO) // unwrap_or: an unmeasured cost is one not yet seen
}

fn pressure<K>(mind: &Mind<K>) -> f64 {
    against(mind.waited, mind.deadline)
}

fn prefilling<K>(mind: &Mind<K>) -> f64 {
    f64::from(u8::from(matches!(mind.phase, Phase::Prefill { .. })))
}

impl<K: Copy + Debug + PartialEq + Send + 'static> Featured<FEATURES, TARGETS> for Lane<K> {
    fn features(situation: &Situation<K>, action: Action) -> [f64; FEATURES] {
        let ready = &situation.ready[..situation.ready.len().min(SLOTS)];
        let pressed = ready.iter().max_by(|a, b| pressure(a).total_cmp(&pressure(b)));
        let mut x = [0.0; FEATURES];
        x[0] = 1.0;
        match action {
            Action::Continue => {
                x[1] = 1.0;
                if let (Some(running), Some(waiter)) = (&situation.running, pressed) {
                    x[2] = against(measured(running.step), waiter.deadline);
                    x[3] = pressure(waiter);
                    x[4] = prefilling(running);
                }
            }
            Action::Switch(slot) => {
                if let Some(target) = ready.get(usize::from(slot)) {
                    x[5] = 1.0;
                    x[6] = against(measured(target.switch_in), target.deadline);
                    x[7] = pressure(target);
                    x[8] = against(measured(target.step), target.deadline);
                    x[9] = prefilling(target);
                    x[10] = ready.iter().enumerate().filter(|(other, _)| *other != usize::from(slot)).map(|(_, mind)| pressure(mind)).fold(0.0, f64::max);
                }
            }
            Action::Batch(_) => x[11] = 1.0,
            Action::Idle => {}
        }
        x
    }

    fn measured(outcome: &Outcome) -> Option<[f64; TARGETS]> {
        let engine = outcome.engine.as_secs_f64();
        (engine > 0.0).then(|| [1.0 - outcome.switching.as_secs_f64().min(engine) / engine, against(outcome.worst_wait, outcome.worst_deadline)])
    }
}
