//! The contention dyno: several minds acting on one engine, replayed under an admission policy.
//!
//! Each mind loops over her measured acts: an act prefills its fresh tokens and decodes its
//! output, then she spends her measured gap (tools, reading) before the next. The engine is one
//! GPU: prefilling requests share the server's prefill rate ([`ModelRates`]), decoding requests
//! decode as one batch at the per-stream rate for their count ([`ModelRates::decode_at`]), and
//! when both run the engine interleaves them half and half (the fork's time-fair interleave).
//! Pure and deterministic: the same minds, rates and policy give the same numbers, in
//! milliseconds, with no resident touched (Joel, 2026-10-25: "tune our car in the shop like a
//! dynamometer").
//!
//! Why: on the M5 (canary, 2026-10-25, away from restarts) an act took 146 s median at 72%
//! cached, only 8,162 fresh tokens, because 11-14 minds shared the GPU; the same act alone is
//! ~15-20 s. The policy question is how many minds act at once, and whether the mind a person is
//! working with goes first.

use super::service_rate::{ModelRates, Workload};

/// One mind's measured loop: her acts in order, and the median gap between the end of one act
/// and the start of her next.
#[derive(Debug, Clone)]
pub struct MindLoop {
    pub acts: Vec<Workload>,
    pub gap_secs: f64,
}

/// Who acts when.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// Every mind with an act ready runs at once (the M5 today).
    AllAtOnce,
    /// At most `active` acts run at once; the rest wait their turn, first come first served.
    Cap { active: usize },
    /// Mind `mind` (the one a person works with) is always admitted and prefills first;
    /// at most `background` other acts run beside her.
    Foreground { mind: usize, background: usize },
}

/// What a replay measured.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Replayed {
    /// Latency of each act of the foreground mind (mind 0 unless the policy names one), seconds.
    pub foreground: Vec<f64>,
    /// Latency of every other mind's acts, seconds.
    pub background: Vec<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    Gap(f64),
    Waiting { since: f64 },
    Prefill { left: f64, since: f64 },
    Decode { left: f64, since: f64 },
}

/// The step the replay advances by. A tenth of a second is far below an act (15-150 s) and far
/// above a decode step, so the shares it averages are the engine's, not an artifact of the step.
const STEP_SECS: f64 = 0.1;

/// PURE: replay `minds` on one engine with `rates` under `policy` for `horizon_secs`. `None`
/// when a rate is unmeasured: the dyno never runs on a guessed rate.
pub fn replay(minds: &[MindLoop], rates: &ModelRates, policy: Policy, horizon_secs: f64) -> Option<Replayed> {
    let prefill_tps = rates.prefill_tps.filter(|r| r.is_finite() && *r > 0.0)?;
    rates.decode_at(1)?;
    let foreground = match policy {
        Policy::Foreground { mind, .. } => mind,
        _ => 0,
    };
    let mut phase: Vec<Phase> = minds.iter().map(|_| Phase::Waiting { since: 0.0 }).collect();
    let mut next_act: Vec<usize> = vec![0; minds.len()];
    let mut out = Replayed::default();
    let mut now = 0.0;
    while now < horizon_secs {
        // Admission: who among the waiting starts an act now.
        let active = |phase: &[Phase]| phase.iter().filter(|p| matches!(p, Phase::Prefill { .. } | Phase::Decode { .. })).count();
        let mut waiting: Vec<(usize, f64)> = phase
            .iter()
            .enumerate()
            .filter_map(|(i, p)| match p {
                Phase::Waiting { since } if !minds[i].acts.is_empty() => Some((i, *since)),
                _ => None,
            })
            .collect();
        waiting.sort_by(|a, b| a.1.total_cmp(&b.1));
        for (i, since) in waiting {
            let admit = match policy {
                Policy::AllAtOnce => true,
                Policy::Cap { active: cap } => active(&phase) < cap,
                Policy::Foreground { mind, background } => {
                    let others = phase.iter().enumerate().filter(|(j, p)| *j != mind && matches!(p, Phase::Prefill { .. } | Phase::Decode { .. })).count();
                    i == mind || others < background
                }
            };
            if admit {
                phase[i] = Phase::Prefill { left: minds[i].acts[next_act[i] % minds[i].acts.len()].prefill_tokens, since };
            }
        }
        // The engine's step: prefill and decode share the step half and half when both run.
        let prefilling: Vec<usize> = (0..minds.len()).filter(|i| matches!(phase[*i], Phase::Prefill { .. })).collect();
        let decoding: Vec<usize> = (0..minds.len()).filter(|i| matches!(phase[*i], Phase::Decode { .. })).collect();
        let (prefill_time, decode_time) = match (prefilling.is_empty(), decoding.is_empty()) {
            (false, false) => (STEP_SECS / 2.0, STEP_SECS / 2.0),
            (false, true) => (STEP_SECS, 0.0),
            (true, false) => (0.0, STEP_SECS),
            (true, true) => (0.0, 0.0),
        };
        let first = matches!(policy, Policy::Foreground { .. }) && prefilling.contains(&foreground);
        for &i in &prefilling {
            let share = if first {
                if i == foreground { 1.0 } else { 0.0 }
            } else {
                1.0 / prefilling.len() as f64
            };
            if let Phase::Prefill { left, since } = phase[i] {
                let left = left - prefill_tps * prefill_time * share;
                phase[i] = if left > 0.0 {
                    Phase::Prefill { left, since }
                } else {
                    Phase::Decode { left: minds[i].acts[next_act[i] % minds[i].acts.len()].decode_tokens, since }
                };
            }
        }
        let per_stream = rates.decode_at(decoding.len().max(1) as u32).unwrap_or(0.0); // unwrap_or: checked measured above
        for &i in &decoding {
            if let Phase::Decode { left, since } = phase[i] {
                let left = left - per_stream * decode_time;
                if left > 0.0 {
                    phase[i] = Phase::Decode { left, since };
                } else {
                    let latency = now + STEP_SECS - since;
                    if i == foreground { out.foreground.push(latency) } else { out.background.push(latency) }
                    next_act[i] += 1;
                    phase[i] = Phase::Gap(minds[i].gap_secs);
                }
            }
        }
        for p in phase.iter_mut() {
            if let Phase::Gap(left) = *p {
                *p = if left - STEP_SECS > 0.0 { Phase::Gap(left - STEP_SECS) } else { Phase::Waiting { since: now + STEP_SECS } };
            }
        }
        now += STEP_SECS;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rates() -> ModelRates {
        ModelRates { prefill_tps: Some(1000.0), decode_per_stream: [(1, 50.0), (8, 12.0)].into_iter().collect() }
    }

    fn mind(acts: usize) -> MindLoop {
        MindLoop { acts: vec![Workload { prefill_tokens: 8000.0, decode_tokens: 300.0 }; acts], gap_secs: 5.0 }
    }

    fn median(mut v: Vec<f64>) -> f64 {
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    }

    // what this catches: a dyno that does not reproduce a lone mind's act: 8,000 tokens at 1,000
    // tok/s plus 300 at 50 tok/s is 14 s.
    #[test]
    fn a_lone_mind_acts_at_her_own_speed() {
        let r = replay(&[mind(5)], &rates(), Policy::AllAtOnce, 200.0).expect("measured rates");
        assert!((median(r.foreground.clone()) - 14.0).abs() < 0.5, "{:?}", r.foreground);
    }

    // what this catches: the M5's shape. Twelve minds at once stretch every act several-fold, and
    // putting the mind a person works with first brings hers back near her lone speed while the
    // background still advances.
    #[test]
    fn many_minds_stretch_every_act_and_foreground_restores_hers() {
        let minds: Vec<MindLoop> = (0..12).map(|_| mind(50)).collect();
        let all = replay(&minds, &rates(), Policy::AllAtOnce, 1800.0).expect("measured rates");
        let fg = replay(&minds, &rates(), Policy::Foreground { mind: 0, background: 3 }, 1800.0).expect("measured rates");
        assert!(median(all.foreground.clone()) > 3.0 * 14.0, "all at once: {}", median(all.foreground));
        assert!(median(fg.foreground.clone()) < 2.0 * 14.0, "foreground: {}", median(fg.foreground));
        assert!(!fg.background.is_empty(), "the background still acts");
    }

    // what this catches: a dyno run on a guessed rate.
    #[test]
    fn an_unmeasured_rate_gives_no_replay() {
        let none = ModelRates::default();
        assert!(replay(&[mind(1)], &none, Policy::AllAtOnce, 10.0).is_none());
    }
}
