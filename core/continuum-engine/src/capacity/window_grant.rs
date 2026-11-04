//! A mind's window in the shared pool is what she needs, granted from her measured requirement;
//! never the pool divided by a count (Joel, 2026-11-03: "It's all about what they need. System
//! accommodates it doesn't do stupid division"; "Stupid divisions or constants are always gonna
//! fail").
//!
//! [`DemandFollowing`] grants every mind her required tokens when the pool holds them all, and no
//! more: what is left stays unallocated in the shared pool, where any sequence may grow into it and
//! a mind whose need rises is granted it on the next tick. A slack share granted as a window would
//! be pinned by a turn in flight while another mind's real need went unmet (Cormac on #5733). When
//! the pool cannot hold every need, every mind is first seated at her measured floor (the window
//! she cannot act below; a turn in flight keeps at least what it holds, a grant never shrinks under
//! a running turn); then exact needs are admitted whole, and only then does a censored bound take
//! what is left, so no bound ever displaces a mind (plan goal 9). The pool is the engine's; paging a cold mind out and making the next one wait for admission are how the
//! node accommodates past it. [`super::window_sim`] validates the contract against today's equal
//! split; ONE-BUDGET slice 3b's solver replaces this policy with `cambrian-flex` claims.

use std::collections::BTreeMap;

use crate::inference::planning::window_allocator::observed::Observed;
use crate::inference::planning::window_allocator::{Floor, Tokens};

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

/// A receiver of the published grants, for a waiter that must wake when they change
/// ([`super::mind_window::grants_waiter`]). `None` before the serving daemon installs them.
pub fn grants_receiver() -> Option<tokio::sync::watch::Receiver<Option<PoolGrants>>> {
    GRANTS.get().cloned()
}

/// Her current grant on this node's engine: `None` before the daemon installs its grants, before
/// the engine serves, or while she has none yet. A `watch` borrow, no I/O.
pub fn current_grant(persona: uuid::Uuid) -> Option<u32> {
    GRANTS.get()?.borrow().as_ref()?.grant_for(persona)
}

/// One measured mind's current demand on the pool: her floor and target, and whether a turn of hers
/// is in flight now. A mind not measured yet has no `Demand`: there is no number to grant her
/// (`Measure::Unmeasured`), and her first view measures her.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Demand<K> {
    pub mind: K,
    /// What her turns hold (the median of her held turns, else her wish), exact or a censored lower
    /// bound. Read here only through [`Observed::within`]: an exact need is never trimmed (above what
    /// one sequence holds she waits); a bound is clamped to what the reader can give.
    pub need: Observed<Tokens>,
    /// Her newest turn's irreducible window ([`Floor`]): below it her turn cannot be sent at all.
    /// When not every need fits, every mind is seated at a measured one before any need above it.
    pub floor: Floor,
    /// What she would read whole (her wish with headroom), granted when the pool holds every target.
    pub target: Tokens,
    /// Her wish without headroom: a grant that covers it is kept; one that does not grows to her
    /// target, so headroom is room to grow into (about one regrant per headroom of growth).
    pub wish: Tokens,
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
/// - when the needs still do not all fit, in three stages:
///   1. every mind is SEATED: her measured floor (the window she cannot act below), a turn in flight at
///      least what it holds. Unmeasured, an exact need's seat is what she keeps, and a bound's is
///      nothing: a bound's binding is surplus, never a seat (a mind with no grant waits for one,
///      `mind_window::MindWindow::Ungranted`, so a returned binding is never used uncounted);
///   2. EXACT needs grow from what the seats leave, in order (turns in flight, then minds already
///      holding a grant, then the rest as given), each whole or not at all: never a fraction of her
///      need (her floor is not a fraction, it is what she needs to act). An unmeasured floor is
///      admitted here, by her exact need, ahead of every bound;
///   3. censored BOUNDS take only what is left, in the same order, clamped by [`Observed::within`]:
///      a bound never displaces a mind. On the M5, 2026-11-04 20:15-20:35Z (87eccb3a7), a 125,696
///      pool granted Aiko's bound 114,592 and nothing to Aris, Atlas and Benchy: 28 acts, all hers,
///      three residents dark;
/// - when even the seats do not fit (more minds than the pool can seat), admission is as before, in
///   one pass: holders keep their binding and minds are admitted whole in order; one left out keeps
///   her binding and waits for room (the engine's admission queues her turn).
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
        let holds = |d: &Demand<K>| held.get(&d.mind).copied().unwrap_or(0).min(d.ceiling); // unwrap_or: a mind with no grant yet holds nothing
        // Her need against what one sequence holds: a bound is clamped to it, an exact need is not.
        let within_ceiling = |d: &Demand<K>| d.need.within(d.ceiling).get();
        // Only a mind who holds nothing is dropped for a need above what one sequence holds now (she
        // waits). A holder whose need outgrew it keeps what she holds as her need: her binding stays
        // counted, so it can never be bound beside the pool uncounted (Cormac on #5745: A holding 40k,
        // need 70k, dropped while B was granted 60k bound 100k on a 65,536 pool).
        let need = |d: &Demand<K>| match within_ceiling(d) > d.ceiling {
            true => holds(d),
            false => within_ceiling(d),
        };
        let demands: Vec<&Demand<K>> = demands.iter().filter(|d| need(d) > 0).collect();
        let demands = demands.as_slice();
        // What each mind keeps when there is room for everyone: at least her need, and what she already
        // holds when that is more (no shrink for drift).
        let want = |d: &Demand<K>| need(d).max(holds(d));
        let total = |f: &dyn Fn(&Demand<K>) -> u32| demands.iter().map(|d| u64::from(f(d))).sum::<u64>();
        let pool = u64::from(pool_tokens);
        // Every mind's target, never below what she needs. When every target fits, every mind is
        // sized to it, settled or not: a mind settled at a small grant stayed there with the pool
        // three-quarters free (the M5, 2026-11-04 07:40Z: 14,445 / 14,242 / 18,213 beside one
        // 262,144, so three views kept 81-88 tokens of history and turned over on every act). A mind
        // holding more keeps it while everything still fits with it (no rebind for drift), and gives
        // back only what the others' targets need.
        // A grant only grows while everything fits, so no regime flip ever rebinds her (each flip rebound
        // all four residents: 22 rebinds in 15 minutes on the M5, d47c5c2a6); a turn in flight keeps
        // what it holds even when the others' targets need it back.
        let target = |d: &Demand<K>| match d.in_flight {
            true => d.target.get().min(d.ceiling).max(need(d)).max(holds(d)),
            false => d.target.get().min(d.ceiling).max(need(d)),
        };
        // A grant that still covers her wish is kept; only a wish that outgrew it grows her, to her
        // target. Growing on every upward move of a target that is recomputed every act regranted
        // most acts (Cormac on #5758).
        let keep_or_grow = |d: &Demand<K>| match holds(d) >= d.wish.get().min(d.ceiling) {
            true => holds(d).max(need(d)),
            false => target(d).max(holds(d)),
        };
        if total(&keep_or_grow) <= pool {
            return demands.iter().map(|d| (d.mind, keep_or_grow(d))).collect();
        }
        if total(&target) <= pool {
            return demands.iter().map(|d| (d.mind, target(d))).collect();
        }
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
        // Not every need fits. What every holder is bound to is already spent: a holder's binding is
        // what she keeps (down to her need when she is not in a turn and holds more; a turn in flight
        // keeps what it holds), so a mind left out never adds to what the pool carries (Cormac on
        // #5745: A grown first while B kept her 25k bound 75k on a 65,536 pool).
        let kept = |d: &Demand<K>| match (d.in_flight, held.contains_key(&d.mind)) {
            (true, _) => holds(d),
            (false, true) => holds(d).min(need(d)),
            (false, false) => 0,
        };
        // Stage 1, her seat: the measured window she cannot act below (never above her need), and for
        // a turn in flight at least what it holds. A mind not in a turn gives back what she holds above
        // her seat and regrows in stages 2-3. Unmeasured, an exact need keeps her binding as her seat;
        // a bound's binding is surplus, so it seats nothing and she takes only what remains.
        let measured = |d: &Demand<K>| match d.floor {
            Floor::Measured(floor) => floor.get().min(need(d)),
            Floor::Unmeasured => 0,
        };
        let seat = |d: &Demand<K>| match (d.in_flight, d.floor, d.need.is_bound()) {
            (true, _, _) => holds(d).max(measured(d)),
            (false, Floor::Measured(_), _) => measured(d),
            (false, Floor::Unmeasured, false) => kept(d),
            (false, Floor::Unmeasured, true) => 0,
        };
        // More minds than the pool can seat: no seat is taken from another, so admission is as before
        // (holders keep their binding, the rest are admitted whole in order or wait).
        let seated = total(&seat) <= pool;
        let base = |d: &Demand<K>| match seated {
            true => seat(d),
            false => kept(d),
        };
        let mut left = pool.saturating_sub(total(&base));
        let rank = |d: &Demand<K>| match (d.in_flight, held.contains_key(&d.mind)) {
            (true, _) => 0,
            (false, true) => 1,
            (false, false) => 2,
        };
        // Stage 2 (exact needs) before stage 3 (bounds) when everyone is seated; one pass otherwise.
        let stage = |d: &Demand<K>| match seated && d.need.is_bound() {
            true => 3,
            false => 2,
        };
        let mut order: Vec<&Demand<K>> = demands.to_vec();
        order.sort_by_key(|d| (stage(d), rank(d))); // stable: within a stage and rank, the caller's order
        let mut grants = BTreeMap::new();
        for d in order {
            let target = reclaimed(d).max(base(d));
            let growth = u64::from(target - base(d));
            let amount = match (growth <= left, seated && d.need.is_bound()) {
                (true, _) => target,
                // A censored bound: she needs at least it, so she takes what the seats leave, read
                // through the one rule (`within`), never another mind's seat. The room is below her
                // target (`left < growth`), so it always fits a u32; `min(room)` keeps a zero room (an
                // unseated mind with nothing left) at zero, where `within` holds nothing to clamp to.
                (false, true) => match u32::try_from(u64::from(base(d)) + left) {
                    Ok(room) => d.need.within(room).get().min(room).max(base(d)),
                    Err(_) => base(d),
                },
                // An exact need grows whole or not at all: she keeps her seat (0 = she waits).
                (false, false) => base(d),
            };
            left -= u64::from(amount - base(d));
            if amount > 0 {
                grants.insert(d.mind, amount);
            }
        }
        grants
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    use crate::inference::planning::window_allocator::test_tokens as t;

    /// A mind whose need is her target (a wish with no held turn yet).
    fn demand(mind: u32, need: u32, in_flight: bool, ceiling: u32) -> Demand<u32> {
        Demand { mind, need: Observed::Exact(t(need)), floor: Floor::Unmeasured, target: t(need), wish: t(need), in_flight, ceiling }
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


    // what this catches (Cormac on #5745): ordered admission overspending the pool. A mind left out
    // keeps her binding, so what is bound (grants, plus the bindings of minds without one) never
    // exceeds the pool. A holds 40k rising to 50k, B holds 25k, pool 65,536: B's 25k is a floor, A
    // grows only into what is left.
    #[test]
    fn what_is_bound_never_exceeds_the_pool() {
        let pool = 65_536;
        let held = BTreeMap::from([(1, 40_000), (2, 25_000)]);
        let demands = [demand(1, 50_000, false, pool), demand(2, 25_000, false, pool), demand(3, 20_000, false, pool)];
        let grants = DemandFollowing.grant(&demands, &held, pool);
        let bound: u64 = demands
            .iter()
            .map(|d| u64::from(grants.get(&d.mind).copied().or_else(|| held.get(&d.mind).copied()).unwrap_or(0)))
            .sum();
        assert!(bound <= u64::from(pool), "bound {bound} on a {pool} pool: {grants:?}");
        assert_eq!(grants.get(&2), Some(&25_000), "B keeps her binding as a floor");
        assert!(grants.get(&1).is_some_and(|&g| g >= 40_000), "A keeps at least what she holds: {grants:?}");
        assert!(!grants.contains_key(&3), "the newcomer waits: no room for her whole need");
    }


    // what this catches (Cormac on #5745): a holder whose need outgrew one sequence dropped from the
    // grants while keeping her binding, so a newcomer's grant bound past the pool. She keeps what she
    // holds, counted; the newcomer is admitted only into what is left.
    #[test]
    fn a_holder_whose_need_outgrew_the_sequence_keeps_her_counted_binding() {
        let pool = 65_536;
        let held = BTreeMap::from([(1, 40_000)]);
        let demands = [demand(1, 70_000, false, pool), demand(2, 60_000, false, pool)];
        let grants = DemandFollowing.grant(&demands, &held, pool);
        assert_eq!(grants.get(&1), Some(&40_000), "she keeps what she holds: {grants:?}");
        let bound: u64 = demands
            .iter()
            .map(|d| u64::from(grants.get(&d.mind).copied().or_else(|| held.get(&d.mind).copied()).unwrap_or(0)))
            .sum();
        assert!(bound <= u64::from(pool), "bound {bound} on a {pool} pool: {grants:?}");
    }


    // what this catches (the M5, 2026-11-03, b7c815484): grants sized from her wish instead of her
    // need. Benchy's wish took the model's whole 262,144 of a 524,288 pool, Aiko's 104,015 and
    // Aris's 88,380, and Atlas got nothing. Needs are granted first; targets only when every target
    // fits.
    #[test]
    fn needs_are_granted_before_any_target_and_targets_only_when_all_fit() {
        let pool = 524_288;
        let mind = |m, need, target| Demand { mind: m, need: Observed::Exact(t(need)), floor: Floor::Unmeasured, target: t(target), wish: t(target), in_flight: false, ceiling: 262_144 };
        let wishes = [mind(1, 70_000, 262_144), mind(2, 60_000, 104_015), mind(3, 50_000, 88_380), mind(4, 40_000, 90_000)];
        let grants = DemandFollowing.grant(&wishes, &BTreeMap::new(), pool);
        assert_eq!(grants, BTreeMap::from([(1, 70_000), (2, 60_000), (3, 50_000), (4, 40_000)]), "every need, no target: the targets sum past the pool");
        let modest = [mind(1, 70_000, 100_000), mind(2, 60_000, 104_015), mind(3, 50_000, 88_380), mind(4, 40_000, 90_000)];
        let grants = DemandFollowing.grant(&modest, &BTreeMap::new(), pool);
        assert_eq!(grants, BTreeMap::from([(1, 100_000), (2, 104_015), (3, 88_380), (4, 90_000)]), "every target fits: each reads whole");
    }


    // what this catches (the M5, 2026-11-04 07:40Z): three minds settled at their needs beside one
    // holding most of the pool, so their views kept 81-88 tokens of history and turned over on every
    // act (55 turnovers in 66 acts, prefix reuse 0 on 46 of 69). Every target fits once the large
    // holder gives back what she holds above hers: everyone is sized to her target. When everything
    // fits as held, nobody shrinks.
    #[test]
    fn every_mind_grows_to_her_target_when_every_target_fits() {
        let pool = 323_840;
        let mind = |m, need, target| Demand { mind: m, need: Observed::Exact(t(need)), floor: Floor::Unmeasured, target: t(target), wish: t(target), in_flight: false, ceiling: 262_144 };
        let demands = [mind(1, 19_384, 78_000), mind(2, 14_499, 35_000), mind(3, 16_004, 51_000), mind(4, 14_477, 42_000)];
        let held = BTreeMap::from([(1, 262_144), (2, 14_445), (3, 18_213), (4, 14_242)]);
        let grants = DemandFollowing.grant(&demands, &held, pool);
        assert_eq!(grants, BTreeMap::from([(1, 78_000), (2, 35_000), (3, 51_000), (4, 42_000)]), "every target: {grants:?}");
        let roomy = DemandFollowing.grant(&demands, &BTreeMap::from([(1, 100_000)]), pool);
        assert_eq!(roomy[&1], 100_000, "everything fits as held: she keeps her surplus, no rebind");
        assert_eq!(DemandFollowing.grant(&demands, &grants, pool), grants, "a steady demand is a steady grant");
    }

    // what this catches (Cormac on #5758): growing on every upward move of a target recomputed every
    // act, which regranted most acts as an append-only view grew. Her wish rises 300 tokens a call
    // and her target is the wish x 1.25: the grant holds while it covers the wish and regrants only
    // when the wish passes it, about once per 25% of growth.
    #[test]
    fn a_rising_wish_regrants_only_when_it_passes_the_grant() {
        let pool = 323_840;
        let at = |wish: u32| Demand { mind: 1u32, need: Observed::Exact(t(10_000)), floor: Floor::Unmeasured, target: t(wish + wish / 4), wish: t(wish), in_flight: false, ceiling: 262_144 };
        let mut held = BTreeMap::new();
        let mut regrants = 0;
        for call in 0..100u32 {
            let wish = 20_000 + 300 * call;
            let grants = DemandFollowing.grant(&[at(wish)], &held, pool);
            if held.get(&1) != grants.get(&1) {
                regrants += 1;
            }
            assert!(grants[&1] >= wish, "the grant always covers her wish: {} < {wish}", grants[&1]);
            held = grants;
        }
        // 20,000 -> 49,700 is about 2.5x: a grant regrows by 25% each time, so ~5 regrants, not ~100.
        assert!((3..=8).contains(&regrants), "{regrants} regrants over 100 calls of a rising wish");
    }

    // what this catches (the M5, 2026-11-03, d47c5c2a6: 22 rebinds in 15 minutes): a grant that moves
    // when the regime flips. Every target fits, so each mind is granted her target; then one mind's
    // need grows past what the targets leave. She is granted her need from the room, and every other
    // mind, still holding at least her need, keeps exactly her grant: no rebind. What is bound stays
    // within the pool.
    #[test]
    fn a_regime_flip_never_rebinds_a_mind_whose_grant_still_fits() {
        let pool = 200_000;
        let mind = |m, need, target| Demand { mind: m, need: Observed::Exact(t(need)), floor: Floor::Unmeasured, target: t(target), wish: t(target), in_flight: false, ceiling: pool };
        let first = [mind(1, 14_000, 47_000), mind(2, 20_000, 53_000), mind(3, 20_000, 51_000), mind(4, 14_000, 45_000)];
        let held = DemandFollowing.grant(&first, &BTreeMap::new(), pool);
        assert_eq!(held, BTreeMap::from([(1, 47_000), (2, 53_000), (3, 51_000), (4, 45_000)]), "every target fits");
        // Mind 4's need grows to 49,000: the targets no longer all fit (47+53+51+60 > 200), but her need
        // fits in what the others hold above theirs (47+53+51+49 = 200).
        let flipped = [mind(1, 14_000, 47_000), mind(2, 20_000, 53_000), mind(3, 20_000, 51_000), mind(4, 49_000, 60_000)];
        let next = DemandFollowing.grant(&flipped, &held, pool);
        for m in [1, 2, 3] {
            assert_eq!(next.get(&m), held.get(&m), "mind {m} still holds at least her need: no rebind ({next:?})");
        }
        assert_eq!(next.get(&4), Some(&49_000), "the grower gets her need from what the others leave");
        assert!(next.values().map(|&g| u64::from(g)).sum::<u64>() <= u64::from(pool), "bound within the pool: {next:?}");
        // And the same grants again: nothing moves.
        assert_eq!(DemandFollowing.grant(&flipped, &next, pool), next, "a steady demand is a steady grant");
    }

    // what this catches (Cormac on #5773): a censored need above the ceiling dropped a mind holding
    // nothing (`need > ceiling => holds = 0`). On fixed slots or a small pool she was then never
    // granted, never took a turn, and her `AtLeast` bound never aged out of her ring: starvation
    // forever. A bound is clamped to the ceiling, so she is granted all one sequence holds; an exact
    // need above it still waits for room, as before.
    #[test]
    fn a_bound_above_the_ceiling_is_granted_the_ceiling_and_an_exact_need_waits() {
        let (pool, ceiling) = (654_336, 8_192);
        let at = |need: Observed<Tokens>| Demand { mind: 1u32, need, floor: Floor::Unmeasured, target: need.value(), wish: need.value(), in_flight: false, ceiling };
        let bound = Observed::AtLeast { at_least: t(65_672) };
        let grants = DemandFollowing.grant(&[at(bound)], &BTreeMap::new(), pool);
        assert_eq!(grants, BTreeMap::from([(1, ceiling)]), "a bound past one sequence is granted one sequence, never dropped");
        let exact = DemandFollowing.grant(&[at(Observed::Exact(t(65_672)))], &BTreeMap::new(), pool);
        assert!(exact.is_empty(), "an exact need past the ceiling still waits for room: {exact:?}");
        // Negative control, the pre-fix read: the bound's value taken as her need drops her.
        let unclamped = DemandFollowing.grant(&[at(Observed::Exact(bound.value()))], &BTreeMap::new(), pool);
        assert!(unclamped.is_empty(), "precondition: the unclamped bound is the drop this fixes: {unclamped:?}");
        assert_eq!(Observed::AtLeast { at_least: t(4_096) }.within(ceiling), t(4_096), "a bound within the ceiling is untouched");
    }


    /// The M5 at 2026-11-04 20:15-20:35Z (87eccb3a7): a 125,696-token pool, Aiko's need a censored
    /// bound of 114,592 and held, Aris, Atlas and Benchy each needing ~40k. Floors are this test's
    /// inputs (each below her need), not a receipt. `floors: false` is the same minds unmeasured.
    const RECEIPT_POOL: u32 = 125_696;
    fn receipt_minds(floors: bool) -> ([Demand<u32>; 4], BTreeMap<u32, u32>) {
        let mind = |m, need: Observed<Tokens>, floor| Demand {
            mind: m,
            need,
            floor: match floors {
                true => Floor::Measured(t(floor)),
                false => Floor::Unmeasured,
            },
            target: need.value(),
            wish: need.value(),
            in_flight: false,
            ceiling: RECEIPT_POOL,
        };
        let minds = [
            mind(1, Observed::AtLeast { at_least: t(114_592) }, 15_000),
            mind(2, Observed::Exact(t(41_000)), 18_000),
            mind(3, Observed::Exact(t(38_000)), 16_000),
            mind(4, Observed::Exact(t(42_000)), 20_000),
        ];
        (minds, BTreeMap::from([(1, 114_592)]))
    }

    // what this catches (the M5, 2026-11-04 20:15-20:35Z, 87eccb3a7): starvation. `serving.grants`
    // granted Aiko's censored bound 114,592 of a 125,696 pool and nothing to Aris, Atlas and Benchy;
    // in 20 minutes only Aiko acted (28 acts), three residents dark. Every mind is seated at her floor
    // (69,000), exact needs grow whole from the 56,696 left (Aris +23,000, Atlas +22,000; Benchy's
    // +22,000 no longer fits, she keeps her floor), and the bound takes the last 11,696 above hers.
    #[test]
    fn every_mind_holds_her_floor_and_a_bound_takes_only_what_the_floors_leave() {
        let (minds, held) = receipt_minds(true);
        let grants = DemandFollowing.grant(&minds, &held, RECEIPT_POOL);
        for d in &minds {
            let floor = match d.floor {
            Floor::Measured(floor) => floor.get(),
            Floor::Unmeasured => 0,
        };
            let granted = grants.get(&d.mind).copied();
            assert!(granted.is_some_and(|g| g >= floor), "mind {} starved below her floor {floor}: {grants:?}", d.mind);
        }
        for d in &minds[1..] {
            let g = grants[&d.mind];
            assert!(d.floor == Floor::Measured(t(g)) || g == d.need.value().get(), "an exact need is her floor or whole: {g}");
        }
        let others: u32 = minds[1..].iter().map(|d| grants[&d.mind]).sum();
        assert_eq!(grants[&1], RECEIPT_POOL - others, "the bound takes exactly what the others leave: {grants:?}");
        assert_eq!(grants, BTreeMap::from([(1, 26_696), (2, 41_000), (3, 38_000), (4, 20_000)]));
        assert_eq!(DemandFollowing.grant(&minds, &grants, RECEIPT_POOL), grants, "a steady demand is a steady grant");
        // Her next turn starts: a running turn keeps what it holds and the others keep their seats.
        let mut running = minds;
        running[0].in_flight = true;
        assert_eq!(DemandFollowing.grant(&running, &grants, RECEIPT_POOL), grants, "a running turn shrinks no one");
    }

    // what this catches (the M5, 2026-11-04 20:15-20:35Z, 87eccb3a7): a bound displacing minds whose
    // floors are not measured yet. Unknown is a type: an unmeasured exact need is admitted by her need,
    // ahead of every bound. The three exact needs are admitted whole (41,000 + 38,000 + 42,000) and
    // Aiko's bound, whose held 114,592 is surplus and no seat, takes the 4,696 left: below her floor
    // her turn is refused, which measures it, and the next grant seats it (the test above).
    // Contrast, documented: the receipt's grant for these same minds was Aiko alone.
    #[test]
    fn without_measured_floors_exact_needs_are_admitted_before_a_bound() {
        const RECEIPT_GRANT: [(u32, u32); 1] = [(1, 114_592)];
        let (minds, held) = receipt_minds(false);
        let grants = DemandFollowing.grant(&minds, &held, RECEIPT_POOL);
        assert_eq!(grants, BTreeMap::from([(1, 4_696), (2, 41_000), (3, 38_000), (4, 42_000)]), "no bound starves a mind");
        assert_ne!(grants, BTreeMap::from(RECEIPT_GRANT), "the 20:15Z grant, Aiko alone, is gone");
        assert_eq!(DemandFollowing.grant(&minds, &grants, RECEIPT_POOL), grants, "a steady demand is a steady grant");
    }

    // what this catches: seating floors leaking into the case where everything fits. On a 654,336 pool
    // (the size the 8,192-ceiling test uses) every mind's target fits, so each is granted her target,
    // exactly as without floors: the floor pass runs only when the needs do not all fit.
    #[test]
    fn when_everything_fits_measured_floors_change_no_grant() {
        let pool = 654_336;
        let (minds, _) = receipt_minds(true);
        let fits = minds.map(|d| Demand { ceiling: 262_144, target: d.need.value().saturating_add(d.need.value().get() / 4), ..d });
        let unmeasured = fits.map(|d| Demand { floor: Floor::Unmeasured, ..d });
        let grants = DemandFollowing.grant(&fits, &BTreeMap::new(), pool);
        assert_eq!(grants, BTreeMap::from([(1, 143_240), (2, 51_250), (3, 47_500), (4, 52_500)]), "every target: {grants:?}");
        assert_eq!(grants, DemandFollowing.grant(&unmeasured, &BTreeMap::new(), pool), "floors move nothing when all fits");
    }
}
