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
    pub(crate) fn new(costs: &Costs) -> Self {
        Self { costs: costs.clone(), fill: Fork::default() }
    }
}

/// One generating sequence: its slot, the tokens it has left, its depth now.
#[derive(Clone, Copy)]
struct Running {
    slot: usize,
    left: f64,
    depth: u64,
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
            let plane = self.costs.plane();
            let left: Vec<f64> = running.iter().map(|r| r.left).collect();
            let depths: Vec<u64> = running.iter().map(|r| r.depth).collect();
            let width = (1..=running.len())
                .filter_map(std::num::NonZeroUsize::new)
                .min_by(|a, b| plane.mean_finish(&left, &depths, *a).total_cmp(&plane.mean_finish(&left, &depths, *b)));
            if let Some(width) = width {
                plan.decode = Some(running.iter().take(width.get()).map(|r| r.slot).collect());
            }
        }
        plan
    }
}

/// What runs live (4.10 P5 slice 2): the core publishes only a width, from the plane at the depths
/// being served with remaining work unknown (`DecodePlane::width`), and the engine fills it with the
/// generating slots that have waited longest since they last decoded (llama.cpp#120). The prompt
/// fill stays the fork's.
pub(crate) struct PublishedWidth {
    costs: Costs,
    fill: Fork,
    /// The plan count at which each slot last decoded: the engine's `t_last_batched_us`.
    last_decoded: std::collections::BTreeMap<usize, u64>,
    plans: u64,
}

impl PublishedWidth {
    pub(crate) fn new(costs: &Costs) -> Self {
        Self { costs: costs.clone(), fill: Fork::default(), last_decoded: Default::default(), plans: 0 }
    }
}

impl BatchScheduler for PublishedWidth {
    fn name(&self) -> &'static str {
        "published width"
    }

    fn plan(&mut self, view: &View<'_>) -> Plan {
        self.plans += 1;
        let mut plan = self.fill.plan(view);
        let mut generating: Vec<(usize, u64)> = view.slots.iter().filter(|s| s.generating).map(|s| (s.slot, s.depth)).collect();
        let depths: Vec<u64> = generating.iter().map(|(_, depth)| *depth).collect();
        if let Some(width) = self.costs.plane().width(&depths) {
            // longest waiting first: never decoded sorts first, then the oldest last decode
            generating.sort_by_key(|(slot, _)| self.last_decoded.get(slot).copied());
            let chosen: Vec<usize> = generating.iter().take(width.get()).map(|(slot, _)| *slot).collect();
            for slot in &chosen {
                self.last_decoded.insert(*slot, self.plans);
            }
            plan.decode = Some(chosen);
        }
        plan
    }
}
