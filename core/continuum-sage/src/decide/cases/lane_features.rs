//! The lane as a [`Featured`] use case, so any stock linear learner can choose its steps: what each
//! candidate action looks like, and what its outcome physically measured.
//! hot-path: the lane decides at every Core step.
//!
//! What the lane serves is turns finished soon, under every mind's wait bound. Finishing the mind
//! with the least work left first (shortest remaining processing time) is what minimizes the mean
//! time to finish, so the value is the share of the stepped mind's remaining phase finished per
//! second of the Core, and each action's leading feature is that rate as her history predicts it:
//! the learner learns how far the prediction holds, and what a switch costs against it. The M5 on
//! 2026-11-25 (19:24Z, build 82750a4e27) showed why: valued only by the Core's transition time, which
//! a pointer swap makes about 1 ms, switching looked free and lowered the worst wait, so the learner
//! rotated minds every step (25 to 378 switches a turn) and turn p50 rose to 117 s.
//!
//! Every other feature is a time against a wait bound (so about one, whatever the chip), or an
//! indicator. An unmeasured time is a cost the lane has not seen yet, so it reads as none; the
//! learner's uncertainty about the action carries what is unknown.

use std::fmt::Debug;
use std::time::Duration;

use super::super::linear_ucb::Featured;
use super::lane::{Action, Lane, Mind, Outcome, Phase, Situation, SLOTS};

/// The features: bias; continuing, with her step, the most pressed waiter's pressure, and whether she
/// prefills; switching, with the target's switch cost, wait, step and prefill, and the pressure left
/// on the others; batching; then the predicted finishing rate of the mind continuing, and of the mind
/// switched in.
pub const FEATURES: usize = 14;

/// The targets: the share of the stepped mind's remaining phase finished per second of the Core
/// (maximized), and the worst wait against its bound (kept at or under one).
pub const TARGETS: usize = 2;

/// Her remaining phase finished per second by this step, as her history predicts it: the chunk this
/// step would move (`lane::step_tokens`, a decode step one token) as a share of what she has left,
/// over its time (her fixed part a step, when measured, plus the chunk at her time a token). Without the
/// fixed part and the chunk the prediction was a per-token rate that a chunk's fixed cost made wrong
/// (relative error 1.6 to 2.1 in the simulation once a chunk paid 12 ms). 0 when unpredicted.
fn finishing<K: Copy + PartialEq>(situation: &Situation<K>, mind: &Mind<K>) -> f64 {
    let (Some(left), Some(per_token)) = (mind.remaining.value(), mind.step.value()) else { return 0.0 };
    let chunk = match mind.phase {
        Phase::Prefill { .. } => super::lane::step_tokens(situation, mind, left.max(1)),
        Phase::Decode => 1,
    };
    let fixed = mind.overhead.value().unwrap_or(Duration::ZERO); // unwrap_or: an unmeasured fixed part is a cost not yet seen
    let time = fixed + per_token.mul_f64(chunk as f64);
    if time.is_zero() { 0.0 } else { (chunk as f64 / left.max(1) as f64) / time.as_secs_f64() }
}

fn against(time: Duration, bound: Duration) -> f64 {
    time.as_secs_f64() / bound.as_secs_f64()
}

fn measured(time: crate::decide::Measured<Duration>) -> Duration {
    time.value().unwrap_or(Duration::ZERO) // unwrap_or: an unmeasured cost is one not yet seen
}

fn pressure<K>(mind: &Mind<K>) -> f64 {
    against(mind.waited, mind.scale())
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
                    x[2] = against(measured(running.step), waiter.scale());
                    x[3] = pressure(waiter);
                    x[4] = prefilling(running);
                    x[12] = finishing(situation, running);
                }
            }
            Action::Switch(slot) => {
                if let Some(target) = ready.get(usize::from(slot)) {
                    x[5] = 1.0;
                    x[6] = against(measured(target.switch_in), target.scale());
                    x[7] = pressure(target);
                    x[8] = against(measured(target.step), target.scale());
                    x[9] = prefilling(target);
                    x[13] = finishing(situation, target);
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
        let served = outcome.served.value()?;
        (engine > 0.0).then(|| [served / engine, against(outcome.worst_wait, outcome.worst_deadline)])
    }
}
