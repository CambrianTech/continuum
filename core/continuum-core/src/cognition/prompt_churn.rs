//! Where a prompt block sits is decided by how often it CHANGED, measured, not by a table.
//!
//! The prompt cache reuses only a common prefix, so blocks must sit most-stable first
//! (`deliberation_prompt::stable_prefix_order` states the law). The order used to come
//! from hand-assigned churn classes plus fixed phases (grounding, then the working-memory
//! trail, then perception facts, then the framing). Measured on the M5 over 79 consecutive
//! act pairs (2026-10-18, prompt captures of six residents): the working-memory trail
//! changed on 100% of acts, while the facts rendered AFTER it changed far less (the card
//! ledger 7%, own time 11%, the work turn 20%). Every act re-prefilled those stable bytes
//! because the phase order put the churniest block ahead of them.
//!
//! [`ChurnMeter`] counts, per block, how many of the acts it appeared in changed its bytes.
//! [`ChurnMeter::order`] sorts by that rate. Two rules keep the order itself from churning:
//! - Before a block has two observations, its rate is unknown and it keeps the position the
//!   caller gave it (the phase order: cold start is exactly the old layout).
//! - Rates are compared in steps of the sampling error at the smaller count (`1/sqrt(n)`),
//!   so two blocks swap only when their measured rates differ by more than the
//!   measurement can resolve. The step is derived from the counts, never chosen.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};

use crate::cognition::workspace::CycleId;

/// One block's identity across acts: the source or fact id it renders under.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockKey(u64);

impl BlockKey {
    pub fn of(id: &str) -> Self {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        id.hash(&mut h);
        Self(h.finish())
    }
}

#[derive(Debug, Default)]
struct Meter {
    last_cycle: Option<CycleId>,
    last_bytes: u64,
    observed: u32,
    changed: u32,
}

impl Meter {
    /// The measured change rate, once two observations exist.
    fn rate(&self) -> Option<f64> {
        (self.observed >= 2).then(|| f64::from(self.changed) / f64::from(self.observed - 1))
    }
}

/// Per-resident change counts for the blocks after the results ring.
#[derive(Debug, Default)]
pub struct ChurnMeter {
    blocks: HashMap<BlockKey, Meter>,
}

/// One block to place: its key, its prior position (the phase order), and its messages,
/// which stay together and in their own order.
pub struct Block {
    pub key: BlockKey,
    pub messages: Vec<String>,
}

impl ChurnMeter {
    /// Record this act's bytes for each block. A cycle is counted once per block however
    /// many times the prompt is rebuilt within it (refits re-render the same bytes).
    pub fn observe(&mut self, cycle: CycleId, blocks: &[Block]) {
        for block in blocks {
            let mut h = std::collections::hash_map::DefaultHasher::new();
            block.messages.hash(&mut h);
            let bytes = h.finish();
            let meter = self.blocks.entry(block.key).or_default();
            if meter.last_cycle == Some(cycle) {
                continue;
            }
            if meter.observed > 0 && meter.last_bytes != bytes {
                meter.changed += 1;
            }
            meter.observed += 1;
            meter.last_cycle = Some(cycle);
            meter.last_bytes = bytes;
        }
    }

    /// Sort `blocks` most-stable first by measured rate, in steps of the sampling error.
    /// A block without a rate yet, or any tie, keeps its prior (phase) position: the sort
    /// is stable.
    pub fn order(&self, blocks: &mut [Block]) {
        let min_n = blocks
            .iter()
            .filter_map(|b| self.blocks.get(&b.key).filter(|m| m.rate().is_some()).map(|m| m.observed))
            .min();
        let Some(min_n) = min_n else { return };
        let step = 1.0 / f64::from(min_n - 1).sqrt();
        // A block with no rate sorts with the rate of the block before it, so it never
        // jumps: walking in prior order, carry the last known bucket forward.
        let mut carried = 0u32;
        let buckets: Vec<u32> = blocks
            .iter()
            .map(|b| {
                if let Some(rate) = self.blocks.get(&b.key).and_then(Meter::rate) {
                    carried = (rate / step).floor() as u32;
                }
                carried
            })
            .collect();
        let mut indexed: Vec<(u32, usize)> = buckets.into_iter().zip(0..).collect();
        indexed.sort_by_key(|&(bucket, prior)| (bucket, prior));
        let order: Vec<usize> = indexed.into_iter().map(|(_, prior)| prior).collect();
        apply_permutation(blocks, &order);
    }
}

/// Reorder `items` so that position i holds the item formerly at `order[i]`.
fn apply_permutation<T>(items: &mut [T], order: &[usize]) {
    let mut placed = vec![false; items.len()];
    for start in 0..items.len() {
        if placed[start] {
            continue;
        }
        // Follow the cycle starting here, swapping each slot into place.
        let mut current = start;
        loop {
            placed[current] = true;
            let next = order[current];
            if placed[next] {
                break;
            }
            items.swap(current, next);
            current = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(id: &str, text: &str) -> Block {
        Block { key: BlockKey::of(id), messages: vec![text.to_string()] }
    }

    fn names(blocks: &[Block]) -> Vec<String> {
        blocks.iter().map(|b| b.messages[0].split(':').next().unwrap_or("").to_string()).collect() // unwrap_or: split yields at least one piece
    }

    // what this catches: a block that changes every act (the working-memory trail, 100% on
    // the M5) rendered ahead of blocks that rarely change (card ledger 7%, own time 11%),
    // so each act re-prefilled the stable bytes behind it. Measured order puts the churny
    // block last; with no data, the prior order stands untouched.
    #[test]
    fn the_block_that_changes_every_act_moves_behind_the_ones_that_do_not() {
        let mut meter = ChurnMeter::default();
        let prior = |act: u64| {
            vec![
                block("trail", &format!("trail:{act}")),
                block("ledger", "ledger:same"),
                block("own-time", if act % 10 == 0 { "own-time:b" } else { "own-time:a" }),
            ]
        };
        let mut cold = prior(0);
        meter.order(&mut cold);
        assert_eq!(names(&cold), ["trail", "ledger", "own-time"], "no data: the phase order stands");
        for act in 0..20 {
            let blocks = prior(act);
            meter.observe(CycleId(act), &blocks);
            // A refit inside the same cycle counts once.
            meter.observe(CycleId(act), &blocks);
        }
        let mut measured = prior(20);
        meter.order(&mut measured);
        assert_eq!(names(&measured), ["ledger", "own-time", "trail"]);
        // The order is stable across acts once measured: the next act lays out the same.
        let mut again = prior(21);
        meter.order(&mut again);
        assert_eq!(names(&again), names(&measured));
    }

    // what this catches: rates that differ by less than the sampling error reordering the
    // blocks act to act; each swap costs a re-prefill of everything behind it.
    #[test]
    fn rates_within_the_sampling_error_keep_their_prior_order() {
        let mut meter = ChurnMeter::default();
        for act in 0..5 {
            let blocks = vec![
                block("a", &format!("a:{}", act % 2)),
                block("b", &format!("b:{}", u64::from(act >= 1))),
            ];
            meter.observe(CycleId(act), &blocks);
        }
        // a changed 4 of 4, b changed 1 of 4: step = 1/sqrt(4) = 0.5, buckets 2 and 0.
        let mut blocks = vec![block("a", "a:x"), block("b", "b:x")];
        meter.order(&mut blocks);
        assert_eq!(names(&blocks), ["b", "a"]);
        // d changed 2 of 5 (0.4) and c 1 of 5 (0.2); the step at n=6 is 1/sqrt(5) = 0.45, so
        // both sit in bucket 0: the measurement cannot tell them apart, and the given order
        // (d first) stands rather than flipping on noise.
        let mut meter = ChurnMeter::default();
        for act in 0..6u64 {
            let blocks = vec![
                block("d", &format!("d:{}", u64::from(act == 2 || act == 3))),
                block("c", &format!("c:{}", u64::from(act >= 5))),
            ];
            meter.observe(CycleId(act), &blocks);
        }
        let mut blocks = vec![block("d", "d:x"), block("c", "c:x")];
        meter.order(&mut blocks);
        assert_eq!(names(&blocks), ["d", "c"]);
    }

    #[test]
    fn permutation_places_each_item_where_the_order_says() {
        let mut v = vec!['a', 'b', 'c', 'd'];
        apply_permutation(&mut v, &[2, 0, 3, 1]);
        assert_eq!(v, ['c', 'a', 'd', 'b']);
    }
}
