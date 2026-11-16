#![cfg(test)]
//! How many generating sequences decode per step, chosen from the measured cost curve
//! (CONTINUUM-ENGINE 4.10 P5): the baseline every learned policy must beat on the replay.
//!
//! The fork today decodes every generating slot on every step. On the M5 (2026-11-16 02:41Z) four
//! streams decoded together at 1.1-4.3 tok/s each against 57-80 alone: the step's cost grows with the
//! streams and their summed depth, so past some count every stream waits longer than it gains. This
//! policy asks the fitted decode plane, at the depths being served now, which count minimizes the
//! mean time for the generating sequences to finish, shortest remaining first, and decodes that many.
//! No count is written down: a memory-bound engine at long context and a compute-bound one at short
//! context land on different answers from the same rule.

use super::{BatchScheduler, Costs, Fork, Plan, View};

/// The prompt fill stays the fork's; only the decode set is chosen.
pub(crate) struct MeasuredConcurrency {
    costs: Costs,
    fill: Fork,
}

impl MeasuredConcurrency {
    pub(crate) fn new(costs: Costs) -> Self {
        Self { costs, fill: Fork::default() }
    }
}

/// One generating sequence: its slot, the tokens it has left, its depth now.
#[derive(Clone, Copy)]
struct Running {
    slot: usize,
    left: f64,
    depth: u64,
}

/// Mean finishing time of `running` (sorted shortest first) when at most `width` decode per step,
/// under `costs`: each step every active sequence advances one token at the plane's step time for
/// that many sequences and their summed depth; a finished one hands its place to the next.
fn mean_finish(costs: &Costs, running: &[Running], width: usize) -> f64 {
    let mut left: Vec<f64> = running.iter().map(|r| r.left).collect();
    let (mut now, mut finished) = (0.0, 0.0);
    // shortest first and advancing together, the first unfinished is always the next to finish
    for done in 0..left.len() {
        let active = done..left.len().min(done + width);
        let summed: u64 = running[active.clone()].iter().map(|r| r.depth).sum();
        let steps = left[done].max(1.0);
        now += steps * costs.decode_s(active.len(), summed);
        for i in active {
            left[i] -= steps;
        }
        finished += now;
    }
    finished / running.len() as f64
}

impl BatchScheduler for MeasuredConcurrency {
    fn name(&self) -> &'static str {
        "measured concurrency"
    }

    fn plan(&mut self, view: &View<'_>) -> Plan {
        let mut plan = self.fill.plan(view);
        let mut running: Vec<Running> = view
            .slots
            .iter()
            .filter(|s| s.generating)
            .map(|s| Running { slot: s.slot, left: s.decode_left, depth: s.depth })
            .collect();
        if running.len() > 1 {
            running.sort_by(|a, b| a.left.total_cmp(&b.left));
            let width = (1..=running.len())
                .min_by(|a, b| mean_finish(&self.costs, &running, *a).total_cmp(&mean_finish(&self.costs, &running, *b)))
                .unwrap_or(running.len()); // unwrap_or: a non-empty range always has a least
            plan.decode = Some(running.iter().take(width).map(|r| r.slot).collect());
        }
        plan
    }
}
