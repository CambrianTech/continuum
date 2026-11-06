//! The engine's BATCH SCHEDULER as the decision under test (ONE-ACT 4b/5d/5e), replayed on the
//! M5's recorded hour (2026-11-06 16:13-19:17Z, `dyno/2026-11-06-m5-batches.json`): "like load
//! balance testing, but really decision making" (Joel).
//!
//! One trait, [`BatchScheduler`], holds what the fork's `update_slots` decides each iteration:
//! which slots' prompt tokens enter the next batch, how many of each, and whether the iteration
//! is decode-only. Everything else is the engine as measured: the per-batch prompt rate by
//! sequences in the batch and decoders present, the decode step, the recorded restore time of
//! each task, and each mind's closed loop (her next task arrives the recorded gap after her
//! previous one is released, so a faster schedule brings her next act sooner).
//!
//! [`Fork`] is today's behaviour, reproduced from `tools/server/server-context.cpp` @0d7dda6ae:
//! prompt slots filled in slot-id order on the unified KV (`server_prefill_order::order`, :3657-3666),
//! each slot's fill broken where its remaining prompt equals a checkpoint offset (4 + n_ubatch, 4,
//! 1 with `--train-dir`, :4166), and the time-fair interleave (:3299-3300, :3445, :3650-3655): a
//! step that carried prompt tokens adds its duration to the decode time owed, and while time is
//! owed and a slot is generating the next step is decode-only. Generating slots ride every batch.
//! It is CALIBRATED against the recorded hour before any alternative is believed (`batch/tests.rs`).
//!
//! Classification: a calibrated prediction (the engine) under a pluggable policy (the decision);
//! no production Rust policy exists for this decision yet (it lives in the fork's C++), so the
//! baseline is a source reproduction, not a call into the owner.
#![cfg(test)]

use serde::Deserialize;

use super::knee::Knee;
use super::recorded::Receipted;

const EVIDENCE_JSON: &str = include_str!("2026-11-06-m5-batches.json");

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Evidence {
    pub m5_hour: Receipted<Hour>,
    pub m5_batch_rates: Receipted<Rates>,
    /// `[seconds, tokens, depth]` per clean solo prefill chunk.
    pub m5_solo_chunks: Receipted<Vec<(f64, u32, u32)>>,
    /// `[seconds, tokens, decoders, summed decoder depth, mean accepted tokens per step]`.
    pub m5_decode_windows: Receipted<Vec<(f64, u32, u32, u64, f64)>>,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Hour {
    pub n_batch: u32,
    pub n_ubatch: u32,
    pub checkpoint_offsets: Vec<u32>,
    pub tasks: Vec<HourTask>,
}

/// Who sent a task: a resident's act (closed loop: her next act follows this one), or dream
/// traffic (its own recorded arrivals, uncapped, full re-prefill).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum TaskClass {
    Resident,
    Dream,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct HourTask {
    pub slot: usize,
    pub mind: String,
    pub class: TaskClass,
    /// Mean accepted tokens per decode step (the task's MTP draft acceptance).
    pub mean_len: f64,
    pub launch_s: f64,
    pub restore_s: f64,
    pub cached: u32,
    pub fresh: u32,
    pub decode: u32,
    pub recorded_prefill_s: f64,
    pub recorded_act_s: f64,
    pub recorded_decode_s: f64,
}

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct Rates {
    pub no_decoder: Vec<(u32, f64)>,
    pub one_decoder: Vec<(u32, f64)>,
    pub two_or_more_decoders: Vec<(u32, f64)>,
}

impl Evidence {
    pub(crate) fn load() -> Result<Self, serde_json::Error> {
        serde_json::from_str(EVIDENCE_JSON) // file format: the recorded M5 hour, 2026-11-06-m5-batches.json
    }
}

mod costs;
pub(crate) use costs::Costs;

/// One slot as a scheduler sees it at an iteration boundary.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct SlotView {
    pub slot: usize,
    /// Prompt tokens still to prefill (0: not prefilling).
    pub prompt_left: u32,
    /// Tokens resident for the slot's task (its restored prefix plus what it has processed).
    pub depth: u64,
    /// The task's prompt was not restored from a checkpoint (a cold prefill).
    pub cold: bool,
    pub generating: bool,
    /// When this slot's prompt became ready to prefill.
    pub ready_s: f64,
}

/// What an iteration is given to decide.
pub(crate) struct View<'a> {
    pub slots: &'a [SlotView],
    pub n_batch: u32,
    pub offsets: &'a [u32],
}

impl View<'_> {
    pub fn generating(&self) -> usize {
        self.slots.iter().filter(|s| s.generating).count()
    }
}

/// One iteration's decision: prompt tokens by slot (in batch order). Generating slots ride every
/// iteration; an empty prompt list with a slot generating is a decode-only step.
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Plan {
    pub prompt: Vec<(usize, u32)>,
}

/// The decision under test.
pub(crate) trait BatchScheduler {
    fn name(&self) -> &'static str;
    fn plan(&mut self, view: &View<'_>) -> Plan;
    /// The iteration ran for `step_s`.
    fn served(&mut self, _plan: &Plan, _step_s: f64) {}
}

/// The fork's per-slot fill: tokens until the batch is full or the remaining prompt reaches a
/// checkpoint offset below where it is now.
pub(crate) fn fork_fill(left: u32, budget: u32, offsets: &[u32]) -> u32 {
    let stop = offsets.iter().copied().filter(|o| *o < left).max().unwrap_or(0); // unwrap_or: no offset below: fill to the end
    (left - stop).min(budget)
}

/// Fill in `order`, with `fill` deciding each slot's take, at most `max_seqs` slots per batch.
fn fill_in_order(view: &View<'_>, order: &[usize], max_seqs: usize, fill: impl Fn(u32, u32) -> u32) -> Plan {
    let mut budget = view.n_batch.saturating_sub(view.generating() as u32);
    let mut plan = Plan::default();
    for &i in order {
        if budget == 0 || plan.prompt.len() >= max_seqs {
            break;
        }
        let s = view.slots[i];
        if s.prompt_left == 0 {
            continue;
        }
        let take = fill(s.prompt_left, budget);
        if take > 0 {
            plan.prompt.push((s.slot, take));
            budget -= take;
        }
    }
    plan
}

fn slot_order(view: &View<'_>) -> Vec<usize> {
    (0..view.slots.len()).collect()
}

/// The time-fair interleave the fork runs: decode is owed the time prompt steps took.
#[derive(Debug, Default)]
pub(crate) struct Owed {
    owed_s: f64,
}

impl Owed {
    fn decode_owed(&mut self, view: &View<'_>) -> bool {
        if view.generating() == 0 {
            self.owed_s = 0.0;
        }
        self.owed_s > 0.0 && view.generating() > 0
    }

    fn served(&mut self, plan: &Plan, step_s: f64) {
        self.owed_s = match plan.prompt.is_empty() {
            false => self.owed_s + step_s,
            true => (self.owed_s - step_s).max(0.0),
        };
    }
}

/// Today's fork (baseline).
#[derive(Debug, Default)]
pub(crate) struct Fork {
    owed: Owed,
}

impl BatchScheduler for Fork {
    fn name(&self) -> &'static str {
        "fork (slot order, checkpoint splits, time-fair)"
    }
    fn plan(&mut self, view: &View<'_>) -> Plan {
        if self.owed.decode_owed(view) {
            return Plan::default();
        }
        fill_in_order(view, &slot_order(view), usize::MAX, |left, budget| fork_fill(left, budget, view.offsets))
    }
    fn served(&mut self, plan: &Plan, step_s: f64) {
        self.owed.served(plan, step_s);
    }
}

/// (a) Prompt work ordered by measured remaining need (shortest remaining first).
#[derive(Debug, Default)]
pub(crate) struct ShortestRemaining {
    owed: Owed,
}

impl BatchScheduler for ShortestRemaining {
    fn name(&self) -> &'static str {
        "(a) shortest remaining prompt first"
    }
    fn plan(&mut self, view: &View<'_>) -> Plan {
        if self.owed.decode_owed(view) {
            return Plan::default();
        }
        let mut order = slot_order(view);
        order.sort_by_key(|i| (view.slots[*i].prompt_left == 0, view.slots[*i].prompt_left, *i));
        fill_in_order(view, &order, usize::MAX, |left, budget| fork_fill(left, budget, view.offsets))
    }
    fn served(&mut self, plan: &Plan, step_s: f64) {
        self.owed.served(plan, step_s);
    }
}

/// (b) A warm tail that fits the batch is finished in one iteration (its checkpoints are taken
/// at their offsets inside the batch instead of splitting it).
#[derive(Debug, Default)]
pub(crate) struct WholeTail {
    owed: Owed,
}

impl BatchScheduler for WholeTail {
    fn name(&self) -> &'static str {
        "(b) finish a fitting tail in one iteration"
    }
    fn plan(&mut self, view: &View<'_>) -> Plan {
        if self.owed.decode_owed(view) {
            return Plan::default();
        }
        fill_in_order(view, &slot_order(view), usize::MAX, |left, budget| match left <= budget {
            true => left,
            false => fork_fill(left, budget, view.offsets),
        })
    }
    fn served(&mut self, plan: &Plan, step_s: f64) {
        self.owed.served(plan, step_s);
    }
}

/// (c) Decode eligible at every boundary (5d) instead of equal time: no owed decode-only steps;
/// a prompt step is capped at `chunk` tokens so the gap a decoder sees is one bounded chunk. With
/// `shortest_first`, prompt work is ordered as (a) orders it: the combination (a)+(c).
#[derive(Debug)]
pub(crate) struct BoundedGap {
    pub chunk: u32,
    pub shortest_first: bool,
}

impl BatchScheduler for BoundedGap {
    fn name(&self) -> &'static str {
        match self.shortest_first {
            false => "(c) decode every boundary, chunk-bounded gap",
            true => "(a)+(c) shortest first, decode every boundary",
        }
    }
    fn plan(&mut self, view: &View<'_>) -> Plan {
        let cap = match view.generating() {
            0 => view.n_batch,
            _ => self.chunk,
        };
        let bounded = View { n_batch: cap.min(view.n_batch), ..*view };
        let mut order = slot_order(view);
        if self.shortest_first {
            order.sort_by_key(|i| (view.slots[*i].prompt_left == 0, view.slots[*i].prompt_left, *i));
        }
        fill_in_order(&bounded, &order, usize::MAX, |left, budget| fork_fill(left, budget, view.offsets))
    }
}

/// (d) Concurrent prompt sequences per batch admitted from the measured curve (the knee).
#[derive(Debug)]
pub(crate) struct KneeAdmitted {
    pub knee: usize,
    owed: Owed,
}

impl KneeAdmitted {
    pub(crate) fn new(knee: usize) -> Self {
        Self { knee, owed: Owed::default() }
    }
}

impl BatchScheduler for KneeAdmitted {
    fn name(&self) -> &'static str {
        "(d) prompt sequences at the measured knee"
    }
    fn plan(&mut self, view: &View<'_>) -> Plan {
        if self.owed.decode_owed(view) {
            return Plan::default();
        }
        fill_in_order(view, &slot_order(view), self.knee, |left, budget| fork_fill(left, budget, view.offsets))
    }
    fn served(&mut self, plan: &Plan, step_s: f64) {
        self.owed.served(plan, step_s);
    }
}

/// One task's outcome.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct TaskOutcome {
    pub arrival_s: f64,
    pub prefill_s: f64,
    pub first_service_wait_s: f64,
    pub decode_s: f64,
    pub longest_gap_s: f64,
    pub release_s: f64,
}

impl TaskOutcome {
    pub fn act_s(&self) -> f64 {
        self.release_s - self.arrival_s
    }

    fn unfinished() -> Self {
        Self { arrival_s: f64::NAN, prefill_s: f64::NAN, first_service_wait_s: f64::NAN, decode_s: f64::NAN, longest_gap_s: f64::NAN, release_s: f64::NAN }
    }
}

#[derive(Debug, Clone, Copy)]
enum Phase {
    Waiting,
    Restoring { until_s: f64 },
    Prefill { left: u32, depth: u64, ready_s: f64, served: bool },
    Generating { left: f64, depth: u64, since_s: f64, last_s: f64 },
}

/// When a slot's next task arrives: a resident's after her recorded think gap from the release of
/// her previous one (closed loop); dream traffic at its recorded launch, never before the slot frees.
fn next_arrival(hour: &Hour, queue: &[usize], k: usize, released_s: f64) -> f64 {
    let t = &hour.tasks[queue[k]];
    match (t.class, k) {
        (TaskClass::Dream, _) | (_, 0) => t.launch_s.max(released_s),
        (TaskClass::Resident, _) => {
            let p = &hour.tasks[queue[k - 1]];
            released_s + (t.launch_s - (p.launch_s + p.recorded_act_s)).max(0.0)
        }
    }
}

/// Replay the hour's tasks, slot by slot, under `scheduler`.
pub(crate) fn replay(hour: &Hour, costs: &Costs, scheduler: &mut dyn BatchScheduler) -> Vec<TaskOutcome> {
    let slots = hour.tasks.iter().map(|t| t.slot).max().map_or(0, |m| m + 1);
    let queues: Vec<Vec<usize>> = (0..slots).map(|s| (0..hour.tasks.len()).filter(|i| hour.tasks[*i].slot == s).collect()).collect();
    let mut next = vec![0usize; slots];
    let mut phase: Vec<Phase> = vec![Phase::Waiting; slots];
    let mut arrival: Vec<f64> = (0..slots).map(|s| if queues[s].is_empty() { f64::INFINITY } else { next_arrival(hour, &queues[s], 0, 0.0) }).collect();
    let mut outcomes: Vec<Option<TaskOutcome>> = vec![None; hour.tasks.len()];
    let mut current: Vec<Option<(usize, TaskOutcome)>> = vec![None; slots];
    let mut now = 0.0f64;
    let mut done = 0usize;
    while done < hour.tasks.len() {
        for s in 0..slots {
            if let Phase::Waiting = phase[s] {
                if next[s] < queues[s].len() && arrival[s] <= now {
                    let i = queues[s][next[s]];
                    let o = TaskOutcome { arrival_s: arrival[s], prefill_s: 0.0, first_service_wait_s: 0.0, decode_s: 0.0, longest_gap_s: 0.0, release_s: f64::NAN };
                    current[s] = Some((i, o));
                    phase[s] = Phase::Restoring { until_s: arrival[s] + hour.tasks[i].restore_s };
                }
            }
            if let Phase::Restoring { until_s } = phase[s] {
                if until_s <= now {
                    let (i, _) = current[s].expect("model: a restoring slot has a task");
                    let t = &hour.tasks[i];
                    phase[s] = Phase::Prefill { left: t.fresh.max(1), depth: u64::from(t.cached), ready_s: until_s, served: false };
                }
            }
        }
        let views: Vec<SlotView> = (0..slots).map(|s| {
            let (prompt_left, depth, ready_s) = match phase[s] {
                Phase::Prefill { left, depth, ready_s, .. } => (left, depth, ready_s),
                Phase::Generating { depth, .. } => (0, depth, 0.0),
                _ => (0, 0, 0.0),
            };
            let cold = current[s].is_some_and(|(i, _)| hour.tasks[i].cached == 0);
            SlotView { slot: s, prompt_left, depth, cold, generating: matches!(phase[s], Phase::Generating { .. }), ready_s }
        }).collect();
        if !views.iter().any(|v| v.prompt_left > 0 || v.generating) {
            let wake = (0..slots).map(|s| match phase[s] {
                Phase::Waiting if next[s] < queues[s].len() => arrival[s],
                Phase::Restoring { until_s } => until_s,
                _ => f64::INFINITY,
            }).fold(f64::INFINITY, f64::min);
            if !wake.is_finite() {
                break;
            }
            now = wake.max(now);
            continue;
        }
        let view = View { slots: &views, n_batch: hour.n_batch, offsets: &hour.checkpoint_offsets };
        let plan = scheduler.plan(&view);
        let decoders: Vec<&SlotView> = views.iter().filter(|v| v.generating).collect();
        let decode_depth: u64 = decoders.iter().map(|v| v.depth).sum();
        let prompt_s: f64 = plan.prompt.iter().map(|(s, n)| costs.prompt_s(*n, views[*s].depth)).sum();
        let step = prompt_s + costs.decode_s(decoders.len(), decode_depth);
        if step <= 0.0 {
            break; // a policy that serves nothing while work waits stalls the engine (reported as unfinished)
        }
        now += step;
        scheduler.served(&plan, step);
        for &(s, n) in &plan.prompt {
            if let Phase::Prefill { left, depth, ready_s, served } = phase[s] {
                let (i, o) = current[s].as_mut().expect("model: a prefilling slot has a task");
                if !served {
                    o.first_service_wait_s = now - step - ready_s;
                }
                let (left, depth) = (left.saturating_sub(n), depth + u64::from(n));
                phase[s] = match left {
                    0 => {
                        o.prefill_s = now - ready_s;
                        Phase::Generating { left: f64::from(hour.tasks[*i].decode.max(1)), depth, since_s: now, last_s: now }
                    }
                    _ => Phase::Prefill { left, depth, ready_s, served: true },
                };
            }
        }
        for s in 0..slots {
            if !views[s].generating {
                continue;
            }
            if let Phase::Generating { left, depth, since_s, last_s } = phase[s] {
                let (i, mut o) = current[s].expect("model: a generating slot has a task");
                let accepted = hour.tasks[i].mean_len;
                o.longest_gap_s = o.longest_gap_s.max(now - last_s);
                if left <= accepted {
                    o.release_s = now;
                    o.decode_s = now - since_s;
                    outcomes[i] = Some(o);
                    done += 1;
                    current[s] = None;
                    phase[s] = Phase::Waiting;
                    next[s] += 1;
                    if next[s] < queues[s].len() {
                        arrival[s] = next_arrival(hour, &queues[s], next[s], now);
                    }
                } else {
                    current[s] = Some((i, o));
                    phase[s] = Phase::Generating { left: left - accepted, depth: depth + accepted as u64, since_s, last_s: now };
                }
            }
        }
    }
    outcomes.into_iter().map(|o| o.unwrap_or_else(TaskOutcome::unfinished)).collect() // unwrap_or_else: a stalled policy leaves tasks unfinished, reported as NaN
}

#[cfg(test)]
mod tests;
