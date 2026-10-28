//! A discrete-time simulator: the pure scheduler driving a fake engine calibrated from the M5's
//! measurements (Qwen3.6-35B-A3B at ~64k context, 2026-10-28):
//! - decode: all streams together make `30.5 * n^0.865` tok/s (30.5 alone, 101 at four streams);
//! - prefill: each prefilling job gets `304 / sqrt(n)` tok/s with `n` jobs on the engine (152 at
//!   four, measured 146-163);
//! - a yield costs a page-out (`0.1 s`, fork #70).
//! Minds act (prefill, decode, then tools off the engine), directed calls arrive, dreams and
//! training run in the gaps. Every case in FLOW-IMPLEMENTATION-PLAN §6 runs here in milliseconds
//! and reports what a live read reports.

use crate::{Aging, Decision, Qos, Scheduler, Ticket};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// The fake engine's measured shape.
#[derive(Clone, Copy, Debug)]
pub struct Engine {
    pub slots: usize,
    pub decode_alone: f64,
    pub decode_exponent: f64,
    pub prefill_alone: f64,
    pub page_out_s: f64,
}

impl Engine {
    pub fn m5() -> Self {
        Self { slots: 4, decode_alone: 30.5, decode_exponent: 0.865, prefill_alone: 304.0, page_out_s: 0.1 }
    }
}

/// One unit of work: what it costs and how it repeats.
#[derive(Clone, Debug)]
pub struct Job {
    pub owner: &'static str,
    pub qos: Qos,
    pub prefill: f64,
    pub decode: f64,
    /// Off-engine time before the owner's next unit (tools, settle); `None` = runs once.
    pub then_off_engine_s: Option<f64>,
    /// First submission time.
    pub at_s: f64,
}

/// What a run reports: the numbers a live read reads.
#[derive(Debug, Default)]
pub struct Report {
    /// Per owner: units finished, total wait before start, worst wait, sum of unit times.
    pub owners: HashMap<&'static str, OwnerStats>,
    pub tokens_decoded: f64,
    pub seconds: f64,
}

#[derive(Debug, Default, Clone)]
pub struct OwnerStats {
    pub done: u32,
    pub waits: Vec<f64>,
    pub unit_s: Vec<f64>,
    /// Submission to first decoded token, per unit.
    pub first_token_s: Vec<f64>,
}

impl OwnerStats {
    pub fn median_unit_s(&self) -> f64 { median(&self.unit_s) }
    pub fn worst_wait_s(&self) -> f64 { self.waits.iter().cloned().fold(0.0, f64::max) }
    pub fn median_first_token_s(&self) -> f64 { median(&self.first_token_s) }
}

fn median(v: &[f64]) -> f64 {
    if v.is_empty() { return f64::NAN; }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    s[s.len() / 2]
}

struct Live {
    job: Job,
    submitted_s: f64,
    started_s: Option<f64>,
    first_token_s: Option<f64>,
    prefill_left: f64,
    decode_left: f64,
    yielding_until: Option<f64>,
}

/// Run `jobs` for `seconds` of virtual time in steps of `dt`. `prefill_cap` models a prefill throttle
/// (the gate deleted in 4579b4711) so the simulator can be checked against the live before/after.
pub fn run(engine: Engine, aging: Aging, jobs: &[Job], seconds: f64, dt: f64, prefill_cap: Option<usize>) -> Report {
    let t0 = Instant::now();
    let at = |s: f64| t0 + Duration::from_secs_f64(s);
    let mut sched: Scheduler<&'static str> = Scheduler::new(engine.slots, aging);
    let mut live: HashMap<Ticket, Live> = HashMap::new();
    let mut running: Vec<Ticket> = Vec::new();
    let mut next_ticket = 0u64;
    let mut timers: Vec<(f64, Job)> = jobs.iter().map(|j| (j.at_s, j.clone())).collect();
    let mut report = Report { seconds, ..Default::default() };
    let mut now = 0.0;
    while now < seconds {
        // Submissions due now.
        let (due, later): (Vec<_>, Vec<_>) = timers.into_iter().partition(|(t, _)| *t <= now);
        timers = later;
        for (_, job) in due {
            let ticket = Ticket(next_ticket);
            next_ticket += 1;
            for (gone, _) in sched.submit(ticket, job.owner, job.qos, at(now)) {
                live.remove(&gone);
            }
            live.insert(ticket, Live { prefill_left: job.prefill, decode_left: job.decode, job, submitted_s: now, started_s: None, first_token_s: None, yielding_until: None });
        }
        for decision in sched.decide(at(now)) {
            match decision {
                Decision::Run(t) => {
                    if let Some(l) = live.get_mut(&t) {
                        l.started_s.get_or_insert(now);
                        running.push(t);
                    }
                }
                Decision::Yield(t) => {
                    if let Some(l) = live.get_mut(&t) {
                        l.yielding_until = Some(now + engine.page_out_s);
                    }
                }
            }
        }
        // The engine advances every running job by one step.
        let active: Vec<Ticket> = running.iter().copied().filter(|t| live.get(t).is_some_and(|l| l.yielding_until.is_none())).collect();
        let n = active.len().max(1) as f64;
        let prefilling: Vec<Ticket> = active.iter().copied().filter(|t| live[t].prefill_left > 0.0).collect();
        let allowed_prefill = prefill_cap.map_or(prefilling.len(), |c| c.min(prefilling.len()));
        let decoding = active.iter().filter(|t| live[t].prefill_left <= 0.0).count().max(1) as f64;
        let per_stream_decode = engine.decode_alone * decoding.powf(engine.decode_exponent) / decoding;
        let mut finished = Vec::new();
        for t in active.iter() {
            let l = live.get_mut(t).expect("active tickets are live"); // expect: `active` was filtered from `live`
            if l.prefill_left > 0.0 {
                if prefilling.iter().position(|p| p == t).is_some_and(|k| k < allowed_prefill) {
                    l.prefill_left -= engine.prefill_alone / n.sqrt() * dt;
                }
            } else {
                l.first_token_s.get_or_insert(now);
                l.decode_left -= per_stream_decode * dt;
                report.tokens_decoded += per_stream_decode * dt;
                if l.decode_left <= 0.0 { finished.push(*t); }
            }
        }
        // Yields complete: the job leaves its slot and waits again with its work left.
        let yielded: Vec<Ticket> = running.iter().copied().filter(|t| live.get(t).and_then(|l| l.yielding_until).is_some_and(|u| u <= now)).collect();
        for t in yielded {
            running.retain(|r| *r != t);
            sched.release(t);
            if let Some(mut l) = live.remove(&t) {
                l.yielding_until = None;
                let ticket = Ticket(next_ticket);
                next_ticket += 1;
                let _ = sched.submit(ticket, l.job.owner, l.job.qos, at(l.submitted_s));
                live.insert(ticket, l);
            }
        }
        for t in finished {
            running.retain(|r| *r != t);
            sched.release(t);
            if let Some(l) = live.remove(&t) {
                let s = report.owners.entry(l.job.owner).or_default();
                s.done += 1;
                let started = l.started_s.unwrap_or(now); // started is set before any decode
                s.waits.push(started - l.submitted_s);
                s.unit_s.push(now - l.submitted_s);
                if let Some(ft) = l.first_token_s { s.first_token_s.push(ft - l.submitted_s); }
                if let Some(off) = l.job.then_off_engine_s {
                    timers.push((now + off, Job { at_s: now + off, ..l.job.clone() }));
                }
            }
        }
        now += dt;
    }
    report
}
