//! A mind's window in the shared pool is what she needs, granted from her measured requirement;
//! never the pool divided by a count (Joel, 2026-11-03: "It's all about what they need. System
//! accommodates it doesn't do stupid division"; "Stupid divisions or constants are always gonna
//! fail").
//!
//! [`DemandFollowing`] grants every mind her required tokens when the pool holds them all, and no
//! more: what is left stays unallocated in the shared pool, where any sequence may grow into it and
//! a mind whose need rises is granted it on the next tick. A slack share granted as a window would
//! be pinned by a turn in flight while another mind's real need went unmet (Cormac on #5733). When
//! the pool cannot hold every need, a turn in flight keeps at least what it holds (a grant never shrinks under a running
//! turn) and the rest share what remains in proportion to what each still needs, so every prompt
//! compresses to its grant instead of one mind being cut while another holds slack. The pool is
//! the engine's; paging a cold mind out and making the next one wait for admission are how the
//! node accommodates past it. [`super::window_sim`] validates the contract against today's equal
//! split; ONE-BUDGET slice 3b's solver replaces this policy with `cambrian-flex` claims.

use std::collections::BTreeMap;

/// What the serving daemon grants the local minds on its engine's pool, each by her persona id.
/// Published on every tick that moves a grant; a mind's binding follows hers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PoolGrants {
    /// The engine's pool: every token its KV holds.
    pub pool_tokens: u32,
    /// The most one sequence may hold: the pool itself on a shared pool (capped at the trained
    /// window), a slot's window on fixed slots. No grant exceeds it.
    pub per_sequence_cap: u32,
    pub by_mind: BTreeMap<uuid::Uuid, u32>,
    /// The minds whose requirement is not their own measurement yet but the population's (the
    /// median measured requirement on the seat), marked so no reader takes it for hers.
    pub from_population: std::collections::BTreeSet<uuid::Uuid>,
}

impl PoolGrants {
    /// Her grant, when she is one of the minds on this pool.
    pub fn grant_for(&self, persona: uuid::Uuid) -> Option<u32> {
        self.by_mind.get(&persona).copied()
    }
}

/// One mind's current demand on the pool: what her next turn requires, and whether a turn of hers
/// is in flight now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Demand<K> {
    pub mind: K,
    pub required_tokens: u32,
    pub in_flight: bool,
    /// The most she may be granted: what one sequence of the engine can hold.
    pub ceiling: u32,
}

/// Grants every mind a window from the pool, given every mind's current demand and the grants
/// they hold now.
pub trait WindowPolicy<K> {
    fn grant(&mut self, demands: &[Demand<K>], held: &BTreeMap<K, u32>, pool_tokens: u32) -> BTreeMap<K, u32>;
}

/// Demand-following over one shared pool: every mind is granted her demand, up to her ceiling,
/// when the pool holds them all; otherwise in-flight grants are kept whole and the rest share what
/// remains in proportion to their demand. Nothing beyond a demand is granted.
pub struct DemandFollowing;

impl<K: Ord + Copy> WindowPolicy<K> for DemandFollowing {
    fn grant(
        &mut self,
        demands: &[Demand<K>],
        held: &BTreeMap<K, u32>,
        pool_tokens: u32,
    ) -> BTreeMap<K, u32> {
        // A turn in flight keeps at least what it holds: its grant is a floor whether or not
        // every demand fits (Cormac on #5718: the fits branch granted the new, smaller demand
        // and shrank a turn mid-flight). The floors fit the pool because they were granted
        // inside it; what they leave is shared over the rest of each mind's demand, whole when
        // it all fits and in proportion when it does not.
        let floor = |d: &Demand<K>| match d.in_flight {
            true => held.get(&d.mind).copied().unwrap_or(0), // unwrap_or: a turn that holds nothing yet has no floor
            false => 0,
        };
        let floors: u64 = demands.iter().map(|d| u64::from(floor(d))).sum();
        let rest = u64::from(pool_tokens).saturating_sub(floors);
        let needed: u64 = demands
            .iter()
            .map(|d| u64::from(d.required_tokens.saturating_sub(floor(d))))
            .sum();
        demands
            .iter()
            .map(|d| {
                let need = u64::from(d.required_tokens.saturating_sub(floor(d)));
                let more = match needed <= rest {
                    true => need,
                    false => (need * rest).checked_div(needed).unwrap_or(0), // unwrap_or: nothing needed means nothing more
                };
                let granted = u64::from(floor(d)).saturating_add(more).min(u64::from(d.ceiling.max(floor(d))));
                (d.mind, u32::try_from(granted).unwrap_or(u32::MAX)) // unwrap_or: a grant is at most her ceiling, a u32
            })
            .collect()
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn demand(mind: u32, required_tokens: u32, in_flight: bool, ceiling: u32) -> Demand<u32> {
        Demand { mind, required_tokens, in_flight, ceiling }
    }

    // what this catches: a window divided by a count (Joel, 2026-11-03: "What a mind needs not what
    // you dictate"). Minds that fit get exactly what they need, never an equal split, never past one
    // sequence's ceiling; what is left stays in the pool, granted to no one.
    #[test]
    fn every_mind_gets_what_she_needs_and_the_rest_stays_in_the_pool() {
        let pool = 257_536;
        let demands = [demand(1, 70_000, false, pool), demand(2, 20_000, false, pool), demand(3, 10_000, false, pool)];
        let grants = DemandFollowing.grant(&demands, &BTreeMap::new(), pool);
        assert_eq!(grants, BTreeMap::from([(1, 70_000), (2, 20_000), (3, 10_000)]));
        let capped = DemandFollowing.grant(&[demand(1, 40_000, false, 32_768)], &BTreeMap::new(), pool);
        assert_eq!(capped[&1], 32_768, "no grant past what one sequence can hold");
    }

    // what this catches: slack granted as windows and then pinned by a turn in flight while another
    // mind's real need goes unmet (Cormac on #5733: pool 65,536, A and B each granted 32,768 for a
    // 10,000 need; A in flight; B rose to 40,000 and got 32,768 while A held 22,768 unused).
    #[test]
    fn a_running_turn_pins_only_what_it_needed_and_a_rising_need_is_met() {
        let pool = 65_536;
        let first = DemandFollowing.grant(&[demand(1, 10_000, false, pool), demand(2, 10_000, false, pool)], &BTreeMap::new(), pool);
        assert_eq!(first, BTreeMap::from([(1, 10_000), (2, 10_000)]));
        let next = DemandFollowing.grant(&[demand(1, 10_000, true, pool), demand(2, 40_000, false, pool)], &first, pool);
        assert_eq!(next, BTreeMap::from([(1, 10_000), (2, 40_000)]), "B's need is met beside A's running turn");
    }

    // what this catches: needs past the pool cutting a running turn, or anyone's grant decided by a
    // count. A turn in flight keeps what it holds; the others share the rest in proportion to need.
    #[test]
    fn past_the_pool_a_running_turn_keeps_its_grant_and_the_rest_share_by_need() {
        let pool = 65_536;
        let held = BTreeMap::from([(1, 30_000)]);
        let demands = [demand(1, 20_000, true, pool), demand(2, 40_000, false, pool), demand(3, 20_000, false, pool)];
        let grants = DemandFollowing.grant(&demands, &held, pool);
        assert_eq!(grants[&1], 30_000, "a running turn's grant never shrinks under it");
        assert!(grants.values().map(|&g| u64::from(g)).sum::<u64>() <= u64::from(pool));
        assert_eq!(grants[&2], 2 * grants[&3], "the rest share by need: {grants:?}");
    }
}
