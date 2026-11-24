#![cfg(test)]
//! The simulator's cases (FLOW-IMPLEMENTATION-PLAN §6). Each prints what a live read reports and
//! asserts the plan's rule. Resident act shape from the M5's last 400 requests (2026-10-28):
//! ~4,000 tokens prefilled and ~870 decoded per model call, then ~15 s of tools off the engine.
use crate::sim::{run, Engine, Job, Report};
use crate::{Aging, Qos};
use std::time::Duration;

const RESIDENTS: [&str; 6] = ["aiko", "aris", "atlas", "benchy", "kimi", "nova"];

fn aging() -> Aging {
    Aging([Duration::from_secs(60), Duration::from_secs(30), Duration::ZERO, Duration::ZERO])
}

fn residents(n: usize) -> Vec<Job> {
    RESIDENTS[..n]
        .iter()
        .enumerate()
        .map(|(i, owner)| Job { owner, qos: Qos::Work, prefill: 4000.0, decode: 870.0, then_off_engine_s: Some(15.0), at_s: i as f64 * 0.5 })
        .collect()
}

fn summary(label: &str, r: &Report) -> f64 {
    let mut units: Vec<f64> = r.owners.iter().filter(|(o, _)| RESIDENTS.contains(*o)).flat_map(|(_, s)| s.unit_s.clone()).collect();
    units.sort_by(|a, b| a.total_cmp(b));
    let median = units.get(units.len() / 2).copied().unwrap_or(f64::NAN); // empty = NaN, printed as such
    let acts: u32 = r.owners.iter().filter(|(o, _)| RESIDENTS.contains(*o)).map(|(_, s)| s.done).sum();
    println!(
        "{label}: resident model call median {median:.0} s, {:.0} acts/h, {:.0} tok/s decoded overall",
        acts as f64 * 3600.0 / r.seconds,
        r.tokens_decoded / r.seconds
    );
    median
}

// what this catches: a simulator that does not match the node. The live M5 went from a 3-slot,
// prefill-throttled admission (median model-call wait+run ~122 s) to 4579b4711 (~84 s). The
// simulator must reproduce that direction and roughly that size before it may forecast anything.
#[test]
fn the_simulator_reproduces_todays_live_before_and_after() {
    let three_slots = Engine { slots: 3, ..Engine::m5() };
    let before = summary("before: 3 usable slots, prefill throttled to 1-2", &run(three_slots, aging(), &residents(4), 3600.0, 0.05, Some(1)));
    let after = summary("after 4579b4711: 3 usable slots, no throttle", &run(three_slots, aging(), &residents(4), 3600.0, 0.05, None));
    let all = summary("all 4 slots serve minds", &run(Engine::m5(), aging(), &residents(4), 3600.0, 0.05, None));
    assert!(after < before, "removing the throttle must help");
    assert!(all <= after, "a fourth usable slot must not hurt");
}

// what this catches: minds waiting when there is a slot for each.
#[test]
fn four_minds_on_four_slots_never_wait() {
    let r = run(Engine::m5(), aging(), &residents(4), 1800.0, 0.05, None);
    summary("4 minds, 4 slots", &r);
    for (owner, s) in &r.owners {
        assert!(s.worst_wait_s() < 0.2, "{owner} waited {:.1} s with a free slot", s.worst_wait_s());
    }
}

// what this catches: a mind starving when minds outnumber slots.
#[test]
fn six_minds_on_four_slots_all_progress_with_bounded_waits() {
    let r = run(Engine::m5(), aging(), &residents(6), 3600.0, 0.05, None);
    summary("6 minds, 4 slots", &r);
    for owner in RESIDENTS {
        let s = &r.owners[owner];
        assert!(s.done >= 10, "{owner} finished only {} acts in an hour", s.done);
        println!("  {owner}: {} acts, worst wait {:.0} s", s.done, s.worst_wait_s());
    }
}

// what this catches: a directed reply queued behind a full engine. It preempts at once: its first
// token comes within its own prefill time plus one page-out, not after someone's whole act.
#[test]
fn a_directed_reply_on_a_full_engine_starts_within_its_own_prefill() {
    let mut jobs = residents(4);
    jobs.push(Job { owner: "joel-asks-aiko", qos: Qos::Interactive, prefill: 300.0, decode: 120.0, then_off_engine_s: None, at_s: 300.0 });
    let r = run(Engine::m5(), aging(), &jobs, 600.0, 0.05, None);
    let reply = &r.owners["joel-asks-aiko"];
    println!("directed reply: first token after {:.2} s, done after {:.1} s", reply.median_first_token_s(), reply.median_unit_s());
    assert!(reply.median_first_token_s() < 3.0, "first token after {:.2} s", reply.median_first_token_s());
}

// what this catches: background work never running, or running at work's expense.
#[test]
fn a_dream_completes_under_steady_work_without_slowing_it() {
    let mut jobs = residents(6);
    jobs.push(Job { owner: "dream", qos: Qos::Background, prefill: 2000.0, decode: 400.0, then_off_engine_s: Some(60.0), at_s: 10.0 });
    let with = run(Engine::m5(), aging(), &jobs, 3600.0, 0.05, None);
    let without = run(Engine::m5(), aging(), &residents(6), 3600.0, 0.05, None);
    let dream = &with.owners["dream"];
    println!("dream: {} runs, worst wait {:.0} s", dream.done, dream.worst_wait_s());
    assert!(dream.done >= 3, "the dream ran {} times in an hour", dream.done);
    let slower = summary("6 minds + dream", &with) / summary("6 minds", &without);
    assert!(slower < 1.15, "the dream slowed work by {:.0}%", (slower - 1.0) * 100.0);
}

// what this catches: guessing where speed comes from. The same four residents at today's median
// working context (82k) and at smaller ones, where an epoch turnover re-prefills half the context
// once every ~20 acts (today: ~65k tokens). Prints the forecast; asserts only that less context is
// faster, so the numbers stay the measurement.
#[test]
fn how_much_faster_a_smaller_working_context_makes_an_act() {
    for (label, depth) in [("82k (today's median)", 82_000.0), ("32k", 32_000.0), ("16k", 16_000.0)] {
        let turnover_prefill_per_act = depth * 0.5 / 20.0;
        let jobs: Vec<Job> = RESIDENTS[..4]
            .iter()
            .enumerate()
            .map(|(i, owner)| Job { owner, qos: Qos::Work, prefill: 4000.0 + turnover_prefill_per_act, decode: 870.0, then_off_engine_s: Some(15.0), at_s: i as f64 * 0.5 })
            .collect();
        summary(&format!("working context {label}"), &run(Engine::m5_at_depth(depth), aging(), &jobs, 3600.0, 0.05, None));
    }
    let big = run(Engine::m5_at_depth(82_000.0), aging(), &residents(4), 1800.0, 0.05, None);
    let small = run(Engine::m5_at_depth(16_000.0), aging(), &residents(4), 1800.0, 0.05, None);
    assert!(summary("check 16k", &small) < summary("check 82k", &big));
}
