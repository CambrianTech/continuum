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
//!
//! The answer it gave, with no number to configure (Joel: "not some arbitrary threshold or
//! you'd have to code literally every model"): PREEMPTIVE PRIORITY, WORK-CONSERVING. The mind a
//! person works with prefills first; every other mind is admitted and fills what she leaves. On
//! the M5's probes: her act roughly halves or better, no other mind starves, and the node's
//! total moves with the rates (holding background prefills shrinks the decode batch; -2% and
//! +27% on two M5 windows), so the dyno measures that cost per node rather than assuming it. Measured and rejected, so nobody rebuilds them:
//! caps of 2-4 (foreground 226-237 s: a cap without priority does not help her), a background
//! grant grown while it did not measurably stretch her (foreground 26 s, 119 acts/h), and one
//! climbed on measured throughput (25 s, 109 acts/h): throughput is flat once anything runs in
//! her gaps, so there is no gradient to settle on.

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
    /// Each mind's own act latencies, in the order of `minds`: the fairness view (no mind's
    /// acts may starve under a policy that looks good on the median).
    pub per_mind: Vec<Vec<f64>>,
}

impl Replayed {
    /// The minds that completed no act within the horizon, by index. A median over completed
    /// acts cannot see them, so a report shows them beside it (Codex on #5435). Zero completions
    /// over a finite horizon is not proof of starvation, but it is never hidden.
    pub fn without_completions(&self) -> Vec<usize> {
        self.per_mind.iter().enumerate().filter(|(_, acts)| acts.is_empty()).map(|(mind, _)| mind).collect()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    Gap(f64),
    Waiting { since: f64 },
    Prefill { left: f64, since: f64 },
    Decode { left: f64, since: f64 },
}

/// How many steps the replay spends on its median act: the step is the median act's predicted
/// solo time over this, so the shares it averages are the engine's, not an artifact of the
/// step, on any rates and any work (the median, not the shortest: a few-token act would make
/// the step microscopic and the replay slow for nothing).
// derived-or-floor: a RESOLUTION of the simulation (steps per act), not a substrate threshold;
// the step itself is derived from the measured work and rates.
const STEPS_PER_MEDIAN_ACT: f64 = 100.0;

/// The most steps one replay may take: bounds its work whatever the inputs (a tiny median act
/// over a long horizon would otherwise run unbounded), by coarsening the step instead.
// derived-or-floor: a FLOOR on the simulation's work (steps), not a substrate threshold; at a
// million steps the replay of an hour still resolves a 15 s act in ~4 steps or finer.
const MAX_STEPS: f64 = 1_000_000.0;

/// PURE: replay `minds` on one engine with `rates` under `policy` for `horizon_secs`. `None`
/// when a rate is unmeasured or an input is not a finite, non-negative quantity (a horizon that
/// is not positive, a negative gap, a NaN in a workload): the dyno never runs on a guessed rate
/// or a broken input (Codex on #5435).
pub fn replay(minds: &[MindLoop], rates: &ModelRates, policy: Policy, horizon_secs: f64) -> Option<Replayed> {
    let prefill_tps = rates.prefill_tps.filter(|r| r.is_finite() && *r > 0.0)?;
    rates.decode_at(1).filter(|d| d.is_finite() && *d > 0.0)?;
    let finite = |x: f64| x.is_finite() && x >= 0.0;
    let valid = horizon_secs.is_finite()
        && horizon_secs > 0.0
        && minds.iter().all(|m| finite(m.gap_secs) && m.acts.iter().all(|w| finite(w.prefill_tokens) && finite(w.decode_tokens)));
    if !valid {
        return None;
    }
    let solo_of = |w: &Workload| w.prefill_tokens / prefill_tps + w.decode_tokens / rates.decode_at(1).unwrap_or(f64::INFINITY); // unwrap_or: checked measured above
    let mut solos: Vec<f64> = minds.iter().flat_map(|m| m.acts.iter().map(solo_of)).filter(|t| t.is_finite() && *t > 0.0).collect();
    solos.sort_by(f64::total_cmp);
    let step = solos.get(solos.len() / 2).map_or(1.0, |median| median / STEPS_PER_MEDIAN_ACT) // map_or: no work to replay steps by the second
        .max(horizon_secs / MAX_STEPS);
    // A subnormal step would never advance `now` (Codex on #5435).
    if !(step.is_finite() && step > 0.0 && step.is_normal()) {
        return None;
    }
    let foreground = match policy {
        Policy::Foreground { mind, .. } => mind,
        _ => 0,
    };
    let mut phase: Vec<Phase> = minds.iter().map(|_| Phase::Waiting { since: 0.0 }).collect();
    let mut next_act: Vec<usize> = vec![0; minds.len()];
    let mut out = Replayed { per_mind: vec![Vec::new(); minds.len()], ..Replayed::default() };
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
            (false, false) => (step / 2.0, step / 2.0),
            (false, true) => (step, 0.0),
            (true, false) => (0.0, step),
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
                    let latency = now + step - since;
                    if i == foreground { out.foreground.push(latency) } else { out.background.push(latency) }
                    out.per_mind[i].push(latency);
                    next_act[i] += 1;
                    phase[i] = Phase::Gap(minds[i].gap_secs);
                }
            }
        }
        for p in phase.iter_mut() {
            if let Phase::Gap(left) = *p {
                *p = if left - step > 0.0 { Phase::Gap(left - step) } else { Phase::Waiting { since: now + step } };
            }
        }
        now += step;
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

    // what this catches: the M5's shape and its answer. Twelve minds at once stretch every act
    // several-fold; preemptive priority with every other mind admitted brings hers back near her
    // lone speed, and no other mind starves. What it costs the node's total is NOT asserted: it
    // depends on the rates (holding background prefills while she prefills shrinks the decode
    // batch, which on a chip whose aggregate decode grows with the batch loses acts: -9% on these
    // rates, -2% and +27% on two M5 windows), so the dyno measures it per node.
    #[test]
    fn preemptive_priority_restores_her_speed_and_starves_no_one() {
        let minds: Vec<MindLoop> = (0..12).map(|_| mind(50)).collect();
        let all = replay(&minds, &rates(), Policy::AllAtOnce, 1800.0).expect("measured rates");
        let fg = replay(&minds, &rates(), Policy::Foreground { mind: 0, background: usize::MAX }, 1800.0).expect("measured rates");
        assert!(median(all.foreground.clone()) > 3.0 * 14.0, "all at once: {}", median(all.foreground));
        assert!(median(fg.foreground.clone()) < 2.0 * 14.0, "foreground: {}", median(fg.foreground));
        assert!(fg.per_mind.iter().all(|acts| !acts.is_empty()), "every mind still acts: {:?}", fg.per_mind.iter().map(Vec::len).collect::<Vec<_>>());
    }

    // what this catches: a mind that completes nothing vanishing from the fairness report. One
    // background mind finishes its acts; another's single act is too large for the horizon, and
    // it must be named, not dropped from a median over completed acts.
    #[test]
    fn a_mind_with_no_completions_is_reported_not_dropped() {
        let stuck = MindLoop { acts: vec![Workload { prefill_tokens: 1.0e9, decode_tokens: 1.0 }], gap_secs: 0.0 };
        let r = replay(&[mind(5), mind(5), stuck], &rates(), Policy::AllAtOnce, 600.0).expect("measured rates");
        assert!(!r.per_mind[1].is_empty(), "the finishing mind completes");
        assert_eq!(r.without_completions(), [2], "the stuck mind is named");
    }

    // what this catches: a broken input running the replay (a NaN workload, a negative gap, a
    // horizon that is not positive) instead of refusing it.
    #[test]
    fn a_broken_input_gives_no_replay() {
        let mut bad = mind(1);
        bad.acts[0].prefill_tokens = f64::NAN;
        assert!(replay(&[bad], &rates(), Policy::AllAtOnce, 10.0).is_none());
        let mut gap = mind(1);
        gap.gap_secs = -1.0;
        assert!(replay(&[gap], &rates(), Policy::AllAtOnce, 10.0).is_none());
        assert!(replay(&[mind(1)], &rates(), Policy::AllAtOnce, 0.0).is_none());
    }

    // what this catches: a dyno run on a guessed rate.
    #[test]
    fn an_unmeasured_rate_gives_no_replay() {
        let none = ModelRates::default();
        assert!(replay(&[mind(1)], &none, Policy::AllAtOnce, 10.0).is_none());
    }
}
