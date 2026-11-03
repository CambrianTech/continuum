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

static GRANTS: std::sync::OnceLock<tokio::sync::watch::Receiver<Option<PoolGrants>>> = std::sync::OnceLock::new();

/// Install the serving daemon's grants as the process-wide readable seam, beside
/// `install_serving_state`. Set once; a second call is ignored. `true` iff this call installed it.
pub fn install_grants(rx: tokio::sync::watch::Receiver<Option<PoolGrants>>) -> bool {
    GRANTS.set(rx).is_ok()
}

/// Her current grant on this node's engine: `None` before the daemon installs its grants, before
/// the engine serves, or while she has none yet. A `watch` borrow, no I/O.
pub fn current_grant(persona: uuid::Uuid) -> Option<u32> {
    GRANTS.get()?.borrow().as_ref()?.grant_for(persona)
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

/// Demand-following over one shared pool. A grant is never below a mind's need: below it she cannot
/// hold her turn, so a sub-need window is the hard cut Joel ruled out ("System accommodates it doesn't
/// do stupid division"), and on the M5 on 2026-11-03 proportional shares of a 2,048-token launch pool
/// bound every resident to 409-819 tokens and rebound them every few seconds. So:
/// - her want is her need, capped at what one sequence holds; a mind already holding at least her need
///   keeps what she holds, so a drifting estimate never rebinds her (each rebind resizes her view and
///   costs her prompt cache: the M5 rebound all four every 2-3 minutes);
/// - when the wants do not all fit, surplus is reclaimed first: a mind not in a turn gives back what
///   she holds above her need; a turn in flight keeps what it holds;
/// - when the needs still do not all fit, minds are admitted in order (turns in flight, then minds
///   already holding a grant, then the rest as given), each with her whole need or nothing. A mind
///   left out keeps her current binding and is admitted when the pool has room (the engine's
///   admission queues her turn); she is never granted a fraction of what she needs.
pub struct DemandFollowing;

impl<K: Ord + Copy> WindowPolicy<K> for DemandFollowing {
    fn grant(
        &mut self,
        demands: &[Demand<K>],
        held: &BTreeMap<K, u32>,
        pool_tokens: u32,
    ) -> BTreeMap<K, u32> {
        // Her need is never trimmed: a need above what one sequence can hold right now (a small pool)
        // waits for room. The model's own trained limit is applied by the caller before this.
        let need = |d: &Demand<K>| d.required_tokens;
        // A turn in flight stays whatever its need now says: its window cannot change mid-turn.
        let demands: Vec<&Demand<K>> = demands.iter().filter(|d| d.in_flight || need(d) <= d.ceiling).collect();
        let demands = demands.as_slice();
        let holds = |d: &Demand<K>| held.get(&d.mind).copied().unwrap_or(0).min(d.ceiling); // unwrap_or: a mind with no grant yet holds nothing
        // What each mind keeps when there is room for everyone: at least her need, and what she already
        // holds when that is more (no shrink for drift).
        let want = |d: &Demand<K>| match need(d) > d.ceiling {
            true => holds(d), // a turn in flight whose need outgrew one sequence keeps what it holds
            false => need(d).max(holds(d)),
        };
        let total = |f: &dyn Fn(&Demand<K>) -> u32| demands.iter().map(|d| u64::from(f(d))).sum::<u64>();
        let pool = u64::from(pool_tokens);
        if total(&want) <= pool {
            return demands.iter().map(|d| (d.mind, want(d))).collect();
        }
        // Surplus reclaimed: only a turn in flight keeps more than its need.
        let reclaimed = |d: &Demand<K>| match d.in_flight {
            true => want(d),
            false => need(d),
        };
        if total(&reclaimed) <= pool {
            return demands.iter().map(|d| (d.mind, reclaimed(d))).collect();
        }
        // Not every need fits: admit whole needs in order, never a fraction.
        let rank = |d: &Demand<K>| match (d.in_flight, held.contains_key(&d.mind)) {
            (true, _) => 0,
            (false, true) => 1,
            (false, false) => 2,
        };
        let mut order: Vec<&Demand<K>> = demands.to_vec();
        order.sort_by_key(|d| rank(d)); // stable: within a rank, the caller's order
        let mut left = pool;
        let mut grants = BTreeMap::new();
        for d in order {
            // A turn in flight is never cut: it takes its want when that fits, else what it holds,
            // which already fit the pool when it was granted.
            let amount = match (d.in_flight, reclaimed(d)) {
                (true, want) if u64::from(want) > left => holds(d),
                (_, amount) => amount,
            };
            if u64::from(amount) <= left {
                left -= u64::from(amount);
                grants.insert(d.mind, amount);
            }
        }
        grants
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
        let too_big = DemandFollowing.grant(&[demand(1, 40_000, false, 32_768)], &BTreeMap::new(), pool);
        assert!(too_big.is_empty(), "a need above what one sequence holds now waits, never trimmed: {too_big:?}");
    }

    // what this catches (the M5, 2026-11-03, 2bbfa56c9): a grant below need. The 2,048-token launch
    // pool was shared in proportion, binding every resident to 409-819 tokens. A mind gets her whole
    // need or no grant (she keeps her binding and waits for room), never a fraction.
    #[test]
    fn a_grant_is_never_below_need_a_mind_that_does_not_fit_waits() {
        let grants = DemandFollowing.grant(
            &[demand(1, 20_000, false, 2_048), demand(2, 30_000, false, 2_048)],
            &BTreeMap::new(),
            2_048,
        );
        assert!(grants.is_empty(), "a 2,048 pool holds no one's need: nobody is bound to a sliver: {grants:?}");
        let pool = 65_536;
        let grants = DemandFollowing.grant(
            &[demand(1, 40_000, false, pool), demand(2, 20_000, false, pool), demand(3, 30_000, false, pool)],
            &BTreeMap::new(),
            pool,
        );
        assert!(grants.iter().all(|(m, g)| *g == [0, 40_000, 20_000, 30_000][*m as usize]), "whole needs only: {grants:?}");
        assert!(grants.values().map(|&g| u64::from(g)).sum::<u64>() <= u64::from(pool));
        assert_eq!(grants.len(), 2, "the third need does not fit and waits: {grants:?}");
    }

    // what this catches (the M5, 2026-11-03): every resident rebound every 2-3 minutes as her estimate
    // drifted, each rebind resizing her view and dropping her prompt cache. Holding at least her need,
    // she keeps her grant; a turn in flight keeps what it holds even when room must be made.
    #[test]
    fn a_held_grant_survives_drift_and_a_running_turn_keeps_its_grant() {
        let pool = 257_536;
        let held = BTreeMap::from([(1, 50_000), (2, 30_000)]);
        let drift = DemandFollowing.grant(&[demand(1, 44_000, false, pool), demand(2, 31_000, false, pool)], &held, pool);
        assert_eq!(drift[&1], 50_000, "her need drifted down: she keeps what she holds");
        assert_eq!(drift[&2], 31_000, "her need rose above what she holds: she grows to it");
        let pool = 65_536;
        let held = BTreeMap::from([(1, 40_000), (2, 20_000)]);
        let tight = DemandFollowing.grant(
            &[demand(1, 20_000, true, pool), demand(2, 10_000, false, pool), demand(3, 30_000, false, pool)],
            &held,
            pool,
        );
        assert_eq!(tight[&1], 40_000, "a turn in flight keeps what it holds");
        assert_eq!(tight[&2], 10_000, "a mind not in a turn gives back her surplus to make room");
        assert!(!tight.contains_key(&3) || tight[&3] == 30_000, "a newcomer gets her whole need or waits");
    }

}
