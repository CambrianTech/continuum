//! The window validator (FLOW-IMPLEMENTATION-PLAN §6e): a mind's window follows what she reads.
//!
//! A scenario is DATA: a timeline of each mind's demand events (her untrimmed required tokens
//! plus her completion reserve, published when it moves) over one shared KV pool. A
//! [`WindowPolicy`] grants every mind a window from the pool; [`run`] plays the timeline through
//! it and checks the contract the policy must keep:
//!
//! - the grants never exceed the pool;
//! - when every demand fits the pool, every mind is granted her demand;
//! - a grant held by an in-flight turn never shrinks under it;
//! - a SQUEEZE (a turn granted less than she demanded while another mind is granted past her own demand) is counted: it
//!   is what made the residents re-read what fell out of view (2026-11-02: 54-92% re-reads).
//!
//! The outliers prove the interface, the same shape as [`super::sim`]: the equal split the
//! engine runs today (one window per slot, a scratch slot beside the residents) must squeeze a
//! reading mind while others idle; a demand-following policy must not. Needs no engine change.

use std::collections::BTreeMap;

/// A mind on the pool (a resident, or the scratch traffic beside them).
pub type MindId = u32;

/// One mind's demand changed: what her next turn needs, and whether a turn is in flight now
/// (a grant held by an in-flight turn may not shrink under it).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DemandEvent {
    pub at_ms: u64,
    pub mind: MindId,
    pub required_tokens: u32,
    pub in_flight: bool,
}

/// A timeline of demand events over one pool.
#[derive(Debug, Clone)]
pub struct WindowScenario {
    pub name: &'static str,
    pub pool_tokens: u32,
    pub events: Vec<DemandEvent>,
}

/// The current demand of every mind on the pool, as a policy reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Demand {
    pub mind: MindId,
    pub required_tokens: u32,
    pub in_flight: bool,
}

/// Grants every mind a window from the pool, given every mind's current demand and the grants
/// they hold now.
pub trait WindowPolicy {
    fn grant(&mut self, demands: &[Demand], held: &BTreeMap<MindId, u32>, pool_tokens: u32) -> BTreeMap<MindId, u32>;
}

/// What a run measured, and every contract breach it saw.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct WindowRun {
    pub squeezes: u32,
    pub over_pool: u32,
    pub in_flight_shrinks: u32,
    pub unmet_when_fits: u32,
}

impl WindowRun {
    /// The policy kept the contract and squeezed no one the pool had room for.
    pub fn kept(&self) -> bool {
        self == &Self::default()
    }
}

/// Play `scenario` through `policy`, one regrant per demand event.
pub fn run(scenario: &WindowScenario, policy: &mut dyn WindowPolicy) -> WindowRun {
    let mut demands: BTreeMap<MindId, Demand> = BTreeMap::new();
    let mut held: BTreeMap<MindId, u32> = BTreeMap::new();
    let mut out = WindowRun::default();
    for event in &scenario.events {
        demands.insert(event.mind, Demand { mind: event.mind, required_tokens: event.required_tokens, in_flight: event.in_flight });
        let current: Vec<Demand> = demands.values().copied().collect();
        let grants = policy.grant(&current, &held, scenario.pool_tokens);
        let total: u64 = grants.values().map(|&g| u64::from(g)).sum();
        if total > u64::from(scenario.pool_tokens) {
            out.over_pool += 1;
        }
        let demanded: u64 = current.iter().map(|d| u64::from(d.required_tokens)).sum();
        // Granted past demand: the room an idle mind holds that a reading mind could have used.
        let slack: u64 = current
            .iter()
            .map(|d| u64::from(grants.get(&d.mind).copied().unwrap_or(0)).saturating_sub(u64::from(d.required_tokens))) // unwrap_or: a mind granted nothing holds no slack
            .sum();
        for demand in &current {
            let granted = grants.get(&demand.mind).copied().unwrap_or(0); // unwrap_or: a mind the policy granted nothing holds nothing
            if demand.in_flight && held.get(&demand.mind).is_some_and(|&before| granted < before) {
                out.in_flight_shrinks += 1;
            }
            if granted < demand.required_tokens {
                if demanded <= u64::from(scenario.pool_tokens) {
                    out.unmet_when_fits += 1;
                }
                if slack > 0 {
                    out.squeezes += 1;
                }
            }
        }
        held = grants;
    }
    out
}

/// TODAY'S ENGINE: one equal window per slot, the slots being the residents plus one scratch
/// slot (`demand.rs` "+1 SCRATCH LANE"): every mind gets `pool / slots`, whatever she reads.
pub struct EqualSplit {
    pub slots: u32,
}

impl WindowPolicy for EqualSplit {
    fn grant(&mut self, demands: &[Demand], _held: &BTreeMap<MindId, u32>, pool_tokens: u32) -> BTreeMap<MindId, u32> {
        let each = pool_tokens / self.slots.max(1);
        demands.iter().map(|d| (d.mind, each)).collect()
    }
}

/// Demand-following over one shared pool: every mind is granted her demand when the pool holds
/// them all; otherwise in-flight grants are kept whole and the rest share what remains in
/// proportion to their demand.
pub struct DemandFollowing;

impl WindowPolicy for DemandFollowing {
    fn grant(&mut self, demands: &[Demand], held: &BTreeMap<MindId, u32>, pool_tokens: u32) -> BTreeMap<MindId, u32> {
        let demanded: u64 = demands.iter().map(|d| u64::from(d.required_tokens)).sum();
        if demanded <= u64::from(pool_tokens) {
            return demands.iter().map(|d| (d.mind, d.required_tokens)).collect();
        }
        // An in-flight turn keeps what it holds: a grant never shrinks under a turn using it.
        let kept: BTreeMap<MindId, u32> = demands
            .iter()
            .filter(|d| d.in_flight)
            .filter_map(|d| held.get(&d.mind).map(|&h| (d.mind, h)))
            .collect();
        let kept_total: u64 = kept.values().map(|&g| u64::from(g)).sum();
        let rest = u64::from(pool_tokens).saturating_sub(kept_total);
        let rest_demand: u64 = demands.iter().filter(|d| !kept.contains_key(&d.mind)).map(|d| u64::from(d.required_tokens)).sum();
        let mut grants = kept;
        for d in demands.iter().filter(|d| !grants.contains_key(&d.mind)).copied().collect::<Vec<_>>() {
            let share = if rest_demand == 0 { 0 } else { u64::from(d.required_tokens) * rest / rest_demand };
            grants.insert(d.mind, u32::try_from(share).unwrap_or(u32::MAX).min(d.required_tokens)); // unwrap_or: a share past u32 is capped by her demand anyway
        }
        grants
    }
}

/// The M5 on 2026-11-02: 290,560 tokens of KV pool; Aiko reading large files (instantiate.rs
/// and skeleton.rs, ~70k of view), Benchy and Aris mid-work, Atlas quiet, and scratch traffic
/// (a 432-token prompt) beside them.
pub fn m5_uneven_reading() -> WindowScenario {
    const AIKO: MindId = 1;
    const ARIS: MindId = 2;
    const ATLAS: MindId = 3;
    const BENCHY: MindId = 4;
    const SCRATCH: MindId = 5;
    let e = |at_ms, mind, required_tokens, in_flight| DemandEvent { at_ms, mind, required_tokens, in_flight };
    WindowScenario {
        name: "m5-uneven-reading",
        pool_tokens: 290_560,
        events: vec![
            e(0, ATLAS, 12_000, false),
            e(1, ARIS, 40_000, true),
            e(2, BENCHY, 52_000, true),
            e(3, SCRATCH, 1_000, true),
            e(4, AIKO, 70_000, true),
            e(5, AIKO, 76_000, true),
            e(6, ATLAS, 9_000, false),
            e(7, BENCHY, 58_000, true),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the equal split the engine runs today passing as adequate. On the M5's
    // own shape it grants Aiko 58,112 of the 70-76k she reads while Atlas idles at 12k of his
    // 58k: squeezed with the pool half empty, the re-read loop.
    #[test]
    fn todays_equal_split_squeezes_a_reading_mind_while_others_idle() {
        let run = run(&m5_uneven_reading(), &mut EqualSplit { slots: 5 });
        assert!(run.squeezes > 0, "{run:?}");
        assert!(run.unmet_when_fits > 0, "every demand fit the pool and one was cut: {run:?}");
    }

    // what this catches: a demand-following grant breaking the contract: over the pool, an
    // in-flight shrink, or a mind cut while the pool holds every demand.
    #[test]
    fn demand_following_grants_every_mind_what_she_reads_within_the_pool() {
        let run = run(&m5_uneven_reading(), &mut DemandFollowing);
        assert!(run.kept(), "{run:?}");
    }

    // what this catches: an overloaded pool shrinking a turn in flight to seat another.
    #[test]
    fn an_overloaded_pool_never_shrinks_a_turn_in_flight() {
        let e = |at_ms, mind, required_tokens, in_flight| DemandEvent { at_ms, mind, required_tokens, in_flight };
        let scenario = WindowScenario {
            name: "overloaded",
            pool_tokens: 100_000,
            events: vec![e(0, 1, 60_000, true), e(1, 2, 60_000, false), e(2, 3, 30_000, false)],
        };
        let run = run(&scenario, &mut DemandFollowing);
        assert_eq!(run.in_flight_shrinks, 0, "{run:?}");
        assert_eq!(run.over_pool, 0, "{run:?}");
    }
}
