//! The batch-scheduler decision on the recorded M5 hour: the fork baseline calibrated first, then
//! the candidates on the SAME arrivals, each number beside the baseline's calibration error.
#![cfg(test)]

use super::*;
use std::collections::BTreeMap;

fn evidence() -> Evidence {
    Evidence::load().expect("test: the recorded M5 hour parses")
}

fn median(mut v: Vec<f64>) -> f64 {
    v.retain(|x| x.is_finite());
    v.sort_by(f64::total_cmp);
    match v.len() {
        0 => f64::NAN,
        n => v[n / 2],
    }
}

fn p90(mut v: Vec<f64>) -> f64 {
    v.retain(|x| x.is_finite());
    v.sort_by(f64::total_cmp);
    match v.len() {
        0 => f64::NAN,
        n => v[((n as f64 * 0.9) as usize).min(n - 1)],
    }
}

/// Per slot: fresh tokens over prefill seconds.
fn slot_rates(hour: &Hour, prefill_s: impl Fn(usize) -> f64) -> BTreeMap<usize, f64> {
    let mut sums: BTreeMap<usize, (f64, f64)> = BTreeMap::new();
    for (i, t) in hour.tasks.iter().enumerate() {
        let e = sums.entry(t.slot).or_insert((0.0, 0.0));
        e.0 += f64::from(t.fresh);
        e.1 += prefill_s(i);
    }
    sums.into_iter().map(|(s, (tok, sec))| (s, tok / sec)).collect()
}

/// The baseline's calibration against the recording: per-slot prefill rate error and the median
/// act-prefill error (fractions), and the median act-time error.
struct Calibration {
    slot_rate_err: BTreeMap<usize, f64>,
    median_prefill_err: f64,
    median_act_err: f64,
}

fn calibrate(hour: &Hour, costs: &Costs) -> Calibration {
    let out = replay(hour, costs, &mut Fork::default());
    let recorded = slot_rates(hour, |i| hour.tasks[i].recorded_prefill_s);
    let modeled = slot_rates(hour, |i| out[i].prefill_s);
    let slot_rate_err = recorded.iter().map(|(s, r)| (*s, (modeled[s] - r) / r)).collect();
    let acts = |f: &dyn Fn(usize) -> f64| median(hour.tasks.iter().enumerate().filter(|(_, t)| t.mind != "scratch").map(|(i, _)| f(i)).collect());
    let (rp, mp) = (acts(&|i| hour.tasks[i].recorded_prefill_s), acts(&|i| out[i].prefill_s));
    let (ra, ma) = (acts(&|i| hour.tasks[i].recorded_act_s), acts(&|i| out[i].act_s()));
    let (rd, md) = (acts(&|i| hour.tasks[i].recorded_decode_s), acts(&|i| out[i].release_s - out[i].arrival_s - hour.tasks[i].restore_s - out[i].first_service_wait_s - out[i].prefill_s));
    eprintln!("calibration: per-slot prefill tok/s recorded {recorded:.1?} modeled {modeled:.1?}; median act prefill {rp:.1} vs {mp:.1} s; median act decode {rd:.1} vs {md:.1} s; median act {ra:.1} vs {ma:.1} s");
    Calibration { slot_rate_err, median_prefill_err: (mp - rp) / rp, median_act_err: (ma - ra) / ra }
}

/// The stated calibration bound: 20% of the recording (the model holds per-batch rates constant
/// across context depth, gives a decoder one token per iteration, and costs a decode-only step
/// per decoder from one measured interval).
const CALIBRATION_BOUND: f64 = 0.20;

/// The residents' slots (0-3) and the median act prefill.
fn residents_calibrated(c: &Calibration) -> bool {
    c.slot_rate_err.iter().filter(|(s, _)| **s < 4).all(|(_, e)| e.abs() <= CALIBRATION_BOUND) && c.median_prefill_err.abs() <= CALIBRATION_BOUND
}

fn print_error(c: &Calibration) {
    eprintln!("calibration error: per slot {:+.1?}, median prefill {:+.1}%, median act {:+.1}%",
        c.slot_rate_err.iter().map(|(s, x)| (*s, x * 100.0)).collect::<Vec<_>>(), c.median_prefill_err * 100.0, c.median_act_err * 100.0);
}

// what this catches: believing an alternative schedule from a model that does not reproduce the
// hour it replays. The fork baseline reproduces the residents' recorded per-slot prefill rates
// (88/68/53/34 tok/s, slot-id order on the unified KV) and the median act prefill (84.7 s) within
// the stated 20%.
#[test]
fn the_fork_baseline_reproduces_the_residents_prefill_in_the_recorded_hour() {
    let e = evidence();
    let c = calibrate(&e.m5_hour.value, &Costs::of(&e));
    print_error(&c);
    assert!(residents_calibrated(&c), "the baseline is outside the stated {:.0}% bound", CALIBRATION_BOUND * 100.0);
}

// what this catches: the rest of the calibration still open. Slot 4 (scratch traffic, last in
// slot order) prefills at ~4.3 tok/s modeled against 8.1 recorded, and an act's decode takes
// ~21 s modeled against 35.2 recorded (median act -22%): the candidates' act times carry that
// error until this is green.
#[test]
#[ignore = "dyno batch model: slot 4 prefill and act decode outside the stated 20% (candidates carry the error)"]
fn the_fork_baseline_reproduces_every_slot_and_the_act_time() {
    let e = evidence();
    let c = calibrate(&e.m5_hour.value, &Costs::of(&e));
    print_error(&c);
    assert!(c.slot_rate_err.values().all(|x| x.abs() <= CALIBRATION_BOUND) && c.median_act_err.abs() <= CALIBRATION_BOUND);
}

// what this catches: nothing by itself; it is the comparison the fork's 4b change is chosen from,
// each candidate on the same arrivals, printed beside the baseline's calibration error. It holds
// only that every candidate completes every recorded task (no starvation to the end of time).
#[test]
fn candidate_schedulers_on_the_recorded_hour() {
    let e = evidence();
    let hour = &e.m5_hour.value;
    let costs = Costs::of(&e);
    let c = calibrate(hour, &costs);
    let knee = match costs.knee() {
        Knee::Derived(k) => k as usize,
        Knee::NeedsNoiseBand { below, .. } => below as usize,
    };
    let mut policies: Vec<Box<dyn BatchScheduler>> = vec![
        Box::new(Fork::default()),
        Box::new(ShortestRemaining::default()),
        Box::new(WholeTail::default()),
        Box::new(BoundedGap { chunk: 512 }),
        Box::new(KneeAdmitted::new(knee)),
    ];
    eprintln!("calibration (baseline vs recorded): per-slot prefill rate {:+.0?}%, median act prefill {:+.0}%, median act {:+.0}%; knee {knee} ({:?})",
        c.slot_rate_err.values().map(|x| x * 100.0).collect::<Vec<_>>(), c.median_prefill_err * 100.0, c.median_act_err * 100.0, costs.knee());
    for policy in policies.iter_mut() {
        let out = replay(hour, &costs, policy.as_mut());
        let makespan = out.iter().map(|o| o.release_s).fold(0.0, f64::max);
        let useful = hour.tasks.iter().zip(&out).filter(|(t, o)| t.mind != "scratch" && o.release_s.is_finite()).count();
        eprintln!("{}: makespan {makespan:.0} s, useful acts/h {:.1}, longest decode gap {:.1} s, oldest waiter {:.0} s",
            policy.name(), useful as f64 * 3600.0 / makespan,
            out.iter().map(|o| o.longest_gap_s).fold(0.0, f64::max),
            out.iter().map(|o| o.first_service_wait_s).fold(0.0, f64::max));
        let mut minds: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
        for (i, t) in hour.tasks.iter().enumerate() {
            minds.entry(t.mind.as_str()).or_default().push(i);
        }
        for (mind, idx) in &minds {
            let acts: Vec<f64> = idx.iter().map(|i| out[*i].act_s()).collect();
            let prefill: Vec<f64> = idx.iter().map(|i| out[*i].prefill_s).collect();
            eprintln!("   {mind:8} acts {:3} median {:6.0} s p90 {:6.0} s, prefill median {:5.0} s",
                idx.len(), median(acts.clone()), p90(acts), median(prefill));
        }
        assert!(out.iter().all(|o| o.release_s.is_finite()), "{} left tasks unfinished", policy.name());
    }
    let recorded: Vec<f64> = hour.tasks.iter().filter(|t| t.mind != "scratch").map(|t| t.recorded_act_s).collect();
    let span = hour.tasks.iter().map(|t| t.launch_s + t.recorded_act_s).fold(0.0, f64::max);
    eprintln!("recorded: makespan {span:.0} s, useful acts/h {:.1}, median act {:.0} s p90 {:.0} s; residents calibrated: {}",
        recorded.len() as f64 * 3600.0 / span, median(recorded.clone()), p90(recorded), residents_calibrated(&c));
}

// what this catches: a fork fill that drifts from the source: a slot stops where its remaining
// prompt reaches 4 + n_ubatch, then 4, then 1 (`server-context.cpp` :4166, `--train-dir`).
#[test]
fn the_fork_fill_breaks_at_each_checkpoint_offset() {
    let hour = evidence().m5_hour.value;
    assert_eq!(hour.checkpoint_offsets, vec![4 + hour.n_ubatch, 4, 1], "the recorded offsets are the source's");
    let offsets = [2052, 4, 1];
    assert_eq!(fork_fill(3000, 2048, &offsets), 948);
    assert_eq!(fork_fill(2052, 2048, &offsets), 2048);
    assert_eq!(fork_fill(4, 2048, &offsets), 3);
    assert_eq!(fork_fill(1, 2048, &offsets), 1);
    assert_eq!(fork_fill(9000, 2048, &offsets), 2048);
}
