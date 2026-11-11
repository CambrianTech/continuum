//! The knee scenarios: the derivation, the model's calibration on the recorded runs (green), today's
//! admission against the policy invariant and the outcome gates (known-red), and the hour.
#![cfg(test)]

use super::*;
use crate::capacity::dyno::expected::MissingSeam;
use crate::inference::resource_admission::LanePriority;

fn evidence() -> Evidence {
    Evidence::load().expect("test: the recorded knee evidence parses")
}

fn intelmac(e: &Evidence) -> Engine {
    Engine { prefill: e.intelmac_prefill(), decode: e.intelmac_decode(), ubatch: e.intelmac_prefill_only.value.ubatch }
}

/// The bench posted straight to the engine's slots: admitted while a slot is free.
struct EngineSlots {
    slots: usize,
    held: std::collections::BTreeSet<usize>,
}

impl Admission for EngineSlots {
    fn try_admit(&mut self, index: usize) -> bool {
        if self.held.contains(&index) || self.held.len() < self.slots {
            self.held.insert(index);
            return true;
        }
        false
    }
    fn release(&mut self, index: usize) {
        self.held.remove(&index);
    }
}

fn slots(n: usize) -> EngineSlots {
    EngineSlots { slots: n, held: Default::default() }
}

fn cold(prompt_tokens: u32) -> Request {
    Request { arrival_s: 0.0, prompt_tokens, decode_tokens: 1, warm: false }
}

/// The largest |predicted - recorded| of sorted completions, as a fraction of the trial's wall.
fn residual(predicted: &[f64], recorded: &[f64], wall_s: f64) -> f64 {
    let mut p = predicted.to_vec();
    p.sort_by(f64::total_cmp);
    let mut r = recorded.to_vec();
    r.sort_by(f64::total_cmp);
    p.iter().zip(&r).map(|(a, b)| (a - b).abs() / wall_s).fold(0.0, f64::max)
}

/// The IntelMac trace Codex specified: one warm decoder and `n` cold arrivals, warm first or last
/// in slot order (all submitted at once, as the bench did).
fn trace(e: &Evidence, n: usize, warm_first: bool) -> Vec<Request> {
    let m = &e.intelmac_mixed.value;
    let warm = Request { arrival_s: 0.0, prompt_tokens: m.warm_prompt_tokens, decode_tokens: m.decode_tokens, warm: true };
    let colds = std::iter::repeat_n(cold(m.cold_prompt_tokens), n);
    match warm_first {
        true => std::iter::once(warm).chain(colds).collect(),
        false => colds.chain(std::iter::once(warm)).collect(),
    }
}

const COLD_ARRIVALS: [usize; 4] = [1, 2, 4, 9];

mod derivation {
    use super::*;

    // what this catches: a knee that is a number. The IntelMac's aggregate prefill falls from
    // 68.30 to 68.02 tok/s at 2 streams (63.18 at 4, 61.31 at 9), so the knee is one whatever the
    // noise; its decode aggregate rises 19.9 -> 32.8, and without a measured repeat spread that
    // rise is a typed unknown, never assumed significant.
    #[test]
    fn the_intelmac_knee_is_derived_from_its_curve_and_an_unmeasured_band_is_typed() {
        let e = evidence();
        assert_eq!(derive_knee(&e.intelmac_prefill(), NoiseBand::Unmeasured), Knee::Derived(1));
        assert_eq!(derive_knee(&e.intelmac_prefill(), NoiseBand::Measured(0.0)), Knee::Derived(1));
        assert!(matches!(derive_knee(&e.intelmac_decode(), NoiseBand::Unmeasured), Knee::NeedsNoiseBand { below: 1, above: 2, .. }));
        assert_eq!(derive_knee(&e.intelmac_decode(), NoiseBand::Measured(1.0)), Knee::Derived(9), "a curve still rising at its last point stops there");
    }
}

mod calibration {
    use super::*;

    // what this catches: a service-curve model believed before it reproduces the runs it is built
    // from. Prefill-only, 1/2/4/9 requests of the 4,641-token prompt: every completion within 5%
    // of the trial's wall (stated error; residual printed). Discriminating: processor sharing
    // would finish all nine together at ~681 s, 85% of the wall off for the first.
    #[test]
    fn the_model_reproduces_the_intelmac_prefill_only_bench() {
        let e = evidence();
        let engine = intelmac(&e);
        let bench = &e.intelmac_prefill_only.value;
        for trial in &bench.trials {
            let reqs: Vec<Request> = (0..trial.concurrency).map(|_| cold(bench.prompt_tokens)).collect();
            let run = engine.run(&reqs, &mut slots(bench.served_slots));
            let predicted: Vec<f64> = run.outcomes.iter().map(|o| o.completion_s).collect();
            let err = residual(&predicted, &trial.completions_s, trial.wall_s);
            eprintln!("prefill-only n={}: predicted {predicted:.1?} recorded {:?} residual {:.1}% of {:.1} s", trial.concurrency, trial.completions_s, err * 100.0, trial.wall_s);
            assert!(err <= 0.05, "n={}: residual {:.1}% > 5%", trial.concurrency, err * 100.0);
        }
    }

    // what this catches: the model missing 5d's recorded starvation. Mixed, cold-first: every
    // warm request's 19-token tail waits behind the cold prompts in slot order, so its first token
    // comes when the last cold finishes (recorded: 81.3 -> 88.4 s, 162.2 -> 171.2 s, 319.3 ->
    // 335.6 s). Stated error 20% of the trial wall: the model's prefill is the prefill-only curve
    // and the waiting decoders slow the recorded prefill by up to ~16% (residual printed).
    #[test]
    fn the_model_reproduces_the_intelmac_mixed_trials_cold_first() {
        let e = evidence();
        let engine = intelmac(&e);
        let m = &e.intelmac_mixed.value;
        for trial in &m.trials {
            let warm = Request { arrival_s: 0.0, prompt_tokens: m.warm_prompt_tokens, decode_tokens: m.decode_tokens, warm: true };
            let reqs: Vec<Request> = std::iter::repeat_n(cold(m.cold_prompt_tokens), trial.cold as usize)
                .chain(std::iter::repeat_n(warm, trial.warm as usize))
                .collect();
            let run = engine.run(&reqs, &mut slots(e.intelmac_prefill_only.value.served_slots));
            let (c, w): (Vec<_>, Vec<_>) = reqs.iter().zip(&run.outcomes).partition(|(r, _)| !r.warm);
            let cold_p: Vec<f64> = c.iter().map(|(_, o)| o.completion_s).collect();
            let warm_p: Vec<f64> = w.iter().map(|(_, o)| o.completion_s).collect();
            let last_cold = cold_p.iter().copied().fold(0.0, f64::max);
            assert!(w.iter().all(|(_, o)| o.first_token_s >= last_cold), "cold-first: no warm token before the last cold finishes");
            let err = residual(&cold_p, &trial.cold_completions_s, trial.wall_s).max(residual(&warm_p, &trial.warm_completions_s, trial.wall_s));
            eprintln!("mixed {}+{}: cold {cold_p:.1?} vs {:?}, warm {warm_p:.1?} vs {:?}, residual {:.1}%", trial.cold, trial.warm, trial.cold_completions_s, trial.warm_completions_s, err * 100.0);
            assert!(err <= 0.20, "mixed {}+{}: residual {:.1}% > 20%", trial.cold, trial.warm, err * 100.0);
        }
    }

    // what this catches: a chunk budget that is not the chunk. On the M5 sweep (warm decoding, a
    // cold 12,288-token prompt 0.5 s later) the longest warm gap is one ubatch at the solo prefill
    // rate plus one decode step: within 10% of the recorded max at 512/1024/2048; the cold wall
    // within 15% (residual ~10%: the solo rate is measured on a 4,096-token prompt); and both move
    // the way the sweep moved (gap up, cold wall down with the ubatch).
    #[test]
    fn the_model_reproduces_the_m5_chunk_sweep() {
        let sweep = evidence().m5_chunk_sweep.value;
        let mut last: Option<(f64, f64)> = None;
        for p in &sweep.points {
            let engine = Engine {
                prefill: ServiceCurve::new(vec![(1, mean(&p.solo_prefill_tok_per_s))]),
                decode: ServiceCurve::new(vec![(1, mean(&p.solo_decode_tok_per_s))]),
                ubatch: p.ubatch,
            };
            let reqs = [
                Request { arrival_s: 0.0, prompt_tokens: sweep.warm_prompt_tokens - sweep.warm_cached_tokens, decode_tokens: sweep.decode_tokens, warm: true },
                Request { arrival_s: sweep.cold_starts_after_s, prompt_tokens: sweep.cold_prompt_tokens, decode_tokens: sweep.decode_tokens, warm: false },
            ];
            let run = engine.run(&reqs, &mut slots(2));
            let gap = run.outcomes[0].longest_gap_s;
            let recorded_gap = p.warm_max_gap_s.iter().copied().fold(0.0, f64::max);
            let wall = run.outcomes[1].completion_s - sweep.cold_starts_after_s;
            let recorded_wall = mean(&p.cold_wall_s);
            let (gap_err, wall_err) = ((gap - recorded_gap) / recorded_gap, (wall - recorded_wall) / recorded_wall);
            eprintln!("m5 ubatch {}: gap {gap:.3} vs {recorded_gap:.3} ({:+.1}%), cold wall {wall:.1} vs {recorded_wall:.1} ({:+.1}%); warm decode {:.1} tok/s recorded, not modeled (MTP drafting)",
                p.ubatch, gap_err * 100.0, wall_err * 100.0, mean(&p.warm_decode_tok_per_s));
            assert!(gap_err.abs() <= 0.10 && wall_err.abs() <= 0.15, "ubatch {}", p.ubatch);
            if let Some((g, w)) = last {
                assert!(gap > g && wall < w, "the tradeoff: a larger chunk lengthens the gap and shortens the cold wall");
            }
            last = Some((gap, wall));
        }
    }

    // what this catches: the hour's admission misread. Today's lane gate at the recorded 9 served
    // lanes admits all eight residents at their recorded grant times (05:41:30-57Z), none queued:
    // eight concurrent cold prefills on a node whose knee is one.
    #[test]
    fn todays_admission_admits_the_recorded_hours_eight_cold_arrivals_at_once() {
        let hour = evidence().intelmac_live_hour.value;
        let mut lanes = TodaysLanes::new(hour.served_lanes, LanePriority::Directed);
        let admitted = (0..hour.lane_grants_s.len()).filter(|i| lanes.try_admit(*i)).count();
        assert_eq!(admitted, hour.lane_grants_s.len());
        let f = hour.first_completion;
        eprintln!("hour: Eitan prefill {} tokens in {} ms ({:.1} tok/s), decode {:.2} tok/s; 7 unserved for {} s",
            f.prefill_tokens, f.prefill_ms, f64::from(f.prefill_tokens) * 1000.0 / f.prefill_ms as f64,
            f64::from(f.decode_tokens) * 1000.0 / f.decode_ms as f64, hour.regrant_s);
        assert!(f.cached_tokens > 0);
    }

    #[test]
    #[ignore = "ONE-ACT 5c: the IntelMac hour's starvation reproduced by a service model"]
    fn the_hours_unserved_residents_are_reproduced() {
        panic!("{}", MissingSeam::LiveHourPrompts.describe());
    }
}

mod todays_admission {
    use super::*;

    fn run_today(e: &Evidence, n: usize, warm_first: bool) -> Run {
        let lanes = e.intelmac_live_hour.value.served_lanes;
        intelmac(e).run(&trace(e, n, warm_first), &mut TodaysLanes::new(lanes, LanePriority::Directed))
    }

    // what this catches: nothing by itself; it prints Codex's per-request table and holds the
    // half of 5d today already meets: every cold prefill completes.
    #[test]
    fn every_request_of_the_trace_completes_under_todays_admission() {
        let e = evidence();
        for n in COLD_ARRIVALS {
            for warm_first in [true, false] {
                let run = run_today(&e, n, warm_first);
                eprintln!("n={n} warm_first={warm_first}: makespan {:.1} s, aggregate {:.1} tok/s, oldest waiter {:.1} s, active cold {}",
                    run.makespan_s, run.aggregate_tok_per_s, run.oldest_waiter_s, run.max_active_cold);
                for (r, o) in trace(&e, n, warm_first).iter().zip(&run.outcomes) {
                    eprintln!("   {} queue {:.1} first {:.1} gap {:.1} done {:.1}", if r.warm { "warm" } else { "cold" },
                        o.admitted_s - r.arrival_s, o.first_token_s, o.longest_gap_s, o.completion_s);
                }
                assert!(run.outcomes.iter().all(|o| o.completion_s.is_finite()));
            }
        }
    }

    // what this catches (the IntelMac, 2026-11-04/05): admission that runs cold prefills past the
    // node's measured knee. Today's gate admits as many as it has lanes (9 served; the plan's own
    // clamp floors at `MIN_KNEE_LANES` = 2), against a derived knee of one.
    #[test]
    #[ignore = "ONE-ACT 5c: active cold prefills never exceed the knee derived from the node's curve"]
    fn todays_admission_keeps_active_cold_prefills_within_the_knee() {
        let e = evidence();
        let Knee::Derived(knee) = derive_knee(&e.intelmac_prefill(), NoiseBand::Unmeasured) else { panic!("test: the IntelMac knee is decided") };
        let floor = crate::inference::decode_knee::knee_lanes(9, crate::inference::prefill_knee::tighter_knee(None, Some(knee)));
        for n in COLD_ARRIVALS {
            for warm_first in [true, false] {
                let run = run_today(&e, n, warm_first);
                assert!(run.max_active_cold <= knee, "n={n} warm_first={warm_first}: {} cold prefills active against a knee of {knee} (the plan's clamped floor is {floor})", run.max_active_cold);
            }
        }
    }

    /// The measured chunk budget: one ubatch at the knee's aggregate, plus one decode step.
    fn chunk_budget_s(engine: &Engine, knee: u32) -> f64 {
        f64::from(engine.ubatch) / engine.prefill.at(knee) + 1.0 / engine.decode.at(1)
    }

    // what this catches (ONE-ACT 5d): a warm decoder that is not eligible at the next scheduling
    // boundary. Its first token after its tail, and every later gap, within one measured chunk at
    // the knee. Cold-first, today's engine order puts her tail behind every cold prompt.
    #[test]
    #[ignore = "ONE-ACT 5d: a warm decoder is eligible at the next boundary (gap within the measured chunk budget)"]
    fn a_warm_decoder_is_eligible_at_the_next_boundary() {
        let e = evidence();
        let engine = intelmac(&e);
        let budget = chunk_budget_s(&engine, 1);
        for n in COLD_ARRIVALS {
            for warm_first in [true, false] {
                let run = run_today(&e, n, warm_first);
                let (r, o) = trace(&e, n, warm_first).into_iter().zip(run.outcomes).find(|(r, _)| r.warm).expect("test: one warm request");
                let tail = f64::from(r.prompt_tokens) / engine.prefill.at(1);
                assert!(o.first_token_s - r.arrival_s <= budget + tail + 1e-9 && o.longest_gap_s <= budget + 1e-9,
                    "n={n} warm_first={warm_first}: first token {:.1} s, longest gap {:.2} s, budget {budget:.2} s", o.first_token_s, o.longest_gap_s);
            }
        }
    }

    // what this catches: cold work that ages past FIFO service at the knee, or a makespan worse
    // than running the same work serialized at the measured single-stream rate. Each cold's bound
    // is the cold work admitted at or before it at the knee's aggregate, plus the warm decoder's
    // steps; the makespan bound is all work at the knee.
    #[test]
    #[ignore = "ONE-ACT 5c: cold aging within FIFO service at the knee, makespan within the measured serialized makespan"]
    fn cold_aging_and_makespan_stay_within_service_at_the_knee() {
        let e = evidence();
        let engine = intelmac(&e);
        let warm_s = f64::from(e.intelmac_mixed.value.decode_tokens) / engine.decode.at(1);
        for n in COLD_ARRIVALS {
            for warm_first in [true, false] {
                let reqs = trace(&e, n, warm_first);
                let run = run_today(&e, n, warm_first);
                let mut ahead = 0.0;
                for (r, o) in reqs.iter().zip(&run.outcomes) {
                    ahead += f64::from(r.prompt_tokens) / engine.prefill.at(1);
                    if !r.warm {
                        assert!(o.completion_s <= ahead + warm_s + 1e-9, "n={n} warm_first={warm_first}: a cold done at {:.1} s past its bound {:.1} s", o.completion_s, ahead + warm_s);
                    }
                }
                assert!(run.makespan_s <= ahead + warm_s + 1e-9, "n={n}: makespan {:.1} s past serialized {:.1} s", run.makespan_s, ahead + warm_s);
            }
        }
        // The recorded bench itself: nine at once took 681.2 s against 9 x 67.9 s serialized.
        let bench = &e.intelmac_prefill_only.value;
        let one = bench.trials.iter().find(|t| t.concurrency == 1).expect("test: a solo trial").wall_s;
        for t in &bench.trials {
            assert!(t.wall_s <= f64::from(t.concurrency) * one, "recorded n={}: {:.1} s > serialized {:.1} s", t.concurrency, t.wall_s, f64::from(t.concurrency) * one);
        }
    }
}
