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
    /// Fresh tokens that are her previous reply and the tool results appended after it.
    pub stream: u32,
    /// Fresh tokens changed inside the previous prompt, view turnover, checkpoint granularity.
    pub unstable: u32,
    /// Fresh tokens of a recorded view turnover (the new epoch's re-prefill), 0 otherwise.
    pub turnover: u32,
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
    /// The prompt tokens are her gap's streamed append-only output (ahead of the go).
    pub streamed: bool,
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

/// (e) Frame contents by sort key (ONE-ACT 5e): streamed append-only tails first, then warm
/// tails (whole when they fit, (b): a checkpoint only where a resume uses it), then cold prompts,
/// at most `knee` of them per frame; decode rides every frame and is owed prompt time, as in (b).
#[derive(Debug)]
pub(crate) struct SortKey {
    pub knee: usize,
    owed: Owed,
}

impl SortKey {
    pub(crate) fn new(knee: usize) -> Self {
        Self { knee, owed: Owed::default() }
    }
}

impl BatchScheduler for SortKey {
    fn name(&self) -> &'static str {
        "(e) sort key: streamed tails, warm tails, cold at the knee"
    }
    fn plan(&mut self, view: &View<'_>) -> Plan {
        if self.owed.decode_owed(view) {
            return Plan::default();
        }
        let mut order = slot_order(view);
        order.sort_by_key(|i| {
            let v = view.slots[*i];
            (v.prompt_left == 0, if v.streamed { 0 } else if v.cold { 2 } else { 1 }, *i)
        });
        let mut budget = view.n_batch.saturating_sub(view.generating() as u32);
        let (mut plan, mut colds) = (Plan::default(), 0usize);
        for i in order {
            let v = view.slots[i];
            if budget == 0 || v.prompt_left == 0 || (v.cold && colds >= self.knee) {
                continue;
            }
            let take = match v.prompt_left <= budget {
                true => v.prompt_left,
                false => fork_fill(v.prompt_left, budget, view.offsets),
            };
            colds += usize::from(v.cold);
            plan.prompt.push((v.slot, take));
            budget -= take;
        }
        plan
    }
    fn served(&mut self, plan: &Plan, step_s: f64) {
        self.owed.served(plan, step_s);
    }
}

mod replay;
pub(crate) use replay::{replay, run, Pipeline, Scope, Shadow, TaskOutcome};

#[cfg(test)]
mod tests;
