//! [`DemandFollowing`]: the one grant policy over a shared pool, and its service guarantee.

use std::collections::BTreeMap;

use super::{Demand, TurnsEnded, WindowPolicy};
use crate::inference::planning::window_allocator::{Floor, Tokens};

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
///   3. censored BOUNDS take only what is left, in the same order, clamped by `Observed::within`:
///      a bound never displaces a mind. On the M5, 2026-11-04 20:15-20:35Z (87eccb3a7), a 125,696
///      pool granted Aiko's bound 114,592 and nothing to Aris, Atlas and Benchy: 28 acts, all hers,
///      three residents dark;
/// - THE SERVICE GUARANTEE (plan goal 9: "no mind is ever starved"): whenever those stages leave any
///   mind without a grant, whatever her floor's state, the grant ROTATES. Minds are seated in order of
///   their claim: turns in custody first (a turn in flight on a live grant is never preempted or
///   shrunk), then every other mind by how long she has been owed service (a waiter since she was
///   left out; a mind seated but not yet served keeps the claim that seated her), then minds already
///   served, the longest-seated last. A seat is given at her measured floor, or her exact need when
///   her floor is unmeasured. The first mind that does not fit blocks everyone behind her, so room is
///   never handed past her; and while anyone waits, every grant LAPSES with the turn it serves
///   (`PoolGrants::lapses`): a mind whose ended-turn count passes it is refused at her next turn's
///   start, so she cannot re-enter between ticks ahead of the waiter. Turn ends are counted by the
///   turn ledger, never sampled on a tick. A seated mind that leaves her seat unused for one grant
///   interval is served too.
///
/// THE BOUND: the longest-owed waiter is seated within S of her predecessors' turn endings, S being
/// the minds seated ahead of her (each ends one turn, or leaves her seat unused one interval, then
/// queues behind her); the k-th waiter within (k+1)·S, so every waiter within (M − S)·S for M minds
/// and S seats.
pub struct DemandFollowing<K> {
    /// Grant calls so far: the clock [`Tenure`] is stamped with.
    tick: u64,
    tenure: BTreeMap<K, Tenure>,
    /// The last call's grants that end with the turn they serve: hers lapses once her ended-turn count
    /// passes it. Empty when nobody waits.
    lapses: BTreeMap<K, TurnsEnded>,
}

impl<K> Default for DemandFollowing<K> {
    fn default() -> Self {
        Self { tick: 0, tenure: BTreeMap::new(), lapses: BTreeMap::new() }
    }
}

/// Where a mind stands for rotation, kept by the policy across grants (stamped in grant calls).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tenure {
    /// Owed service since `since`: she has no grant.
    Waiting { since: u64 },
    /// Granted since `since`, when she had ended `at` turns, by the claim `claim` she waited with.
    Seated { since: u64, at: TurnsEnded, claim: u64 },
}

impl<K: Ord + Copy> WindowPolicy<K> for DemandFollowing<K> {
    fn grant(
        &mut self,
        demands: &[Demand<K>],
        held: &BTreeMap<K, u32>,
        pool_tokens: u32,
    ) -> BTreeMap<K, u32> {
        self.note(demands, held);
        let (grants, lapses) = self.allocate(demands, held, pool_tokens);
        self.lapses = lapses;
        self.tick += 1;
        grants
    }
}

impl<K: Ord + Copy> DemandFollowing<K> {
    /// The last call's lapses: published beside the grants, read at a turn's start.
    pub fn lapses(&self) -> &BTreeMap<K, TurnsEnded> {
        &self.lapses
    }

    /// Her grant lapsed: she has ended a turn past the one it served.
    fn lapsed(&self, d: &Demand<K>) -> bool {
        self.lapses.get(&d.mind).is_some_and(|&lapse| d.turns_ended > lapse)
    }

    /// Stamp every mind's tenure from what she holds going into this grant.
    fn note(&mut self, demands: &[Demand<K>], held: &BTreeMap<K, u32>) {
        let tick = self.tick;
        let before = std::mem::take(&mut self.tenure);
        self.tenure = demands
            .iter()
            .map(|d| {
                let seated = held.get(&d.mind).is_some_and(|&g| g > 0);
                let now = match (before.get(&d.mind).copied(), seated) {
                    (Some(seat @ Tenure::Seated { .. }), true) => seat,
                    (Some(Tenure::Waiting { since }), true) => Tenure::Seated { since: tick, at: d.turns_ended, claim: since },
                    (None, true) => Tenure::Seated { since: tick, at: d.turns_ended, claim: tick },
                    (Some(Tenure::Waiting { since }), false) => Tenure::Waiting { since },
                    (_, false) => Tenure::Waiting { since: tick },
                };
                (d.mind, now)
            })
            .collect();
    }

    fn allocate(
        &self,
        demands: &[Demand<K>],
        held: &BTreeMap<K, u32>,
        pool_tokens: u32,
    ) -> (BTreeMap<K, u32>, BTreeMap<K, TurnsEnded>) {
        let everyone = |grants: BTreeMap<K, u32>| (grants, BTreeMap::new());
        // Her need is never trimmed: a need above what one sequence can hold right now (a small pool)
        // waits for room. The model's own trained limit is applied by the caller before this.
        let holds = |d: &Demand<K>| held.get(&d.mind).copied().unwrap_or(0).min(d.ceiling); // unwrap_or: a mind with no grant yet holds nothing
        // A turn in custody: in flight on a grant that has not lapsed. A turn that started after her
        // grant lapsed is waiting for a new one, not running on it.
        let in_turn = |d: &Demand<K>| d.in_flight && !self.lapsed(d);
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
        let target = |d: &Demand<K>| match in_turn(d) {
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
            return everyone(demands.iter().map(|d| (d.mind, keep_or_grow(d))).collect());
        }
        if total(&target) <= pool {
            return everyone(demands.iter().map(|d| (d.mind, target(d))).collect());
        }
        if total(&want) <= pool {
            return everyone(demands.iter().map(|d| (d.mind, want(d))).collect());
        }
        // Surplus reclaimed: only a turn in flight keeps more than its need.
        let reclaimed = |d: &Demand<K>| match in_turn(d) {
            true => want(d),
            false => need(d),
        };
        if total(&reclaimed) <= pool {
            return everyone(demands.iter().map(|d| (d.mind, reclaimed(d))).collect());
        }
        // Not every need fits. What every holder is bound to is already spent: a holder's binding is
        // what she keeps (down to her need when she is not in a turn and holds more; a turn in flight
        // keeps what it holds), so a mind left out never adds to what the pool carries (Cormac on
        // #5745: A grown first while B kept her 25k bound 75k on a 65,536 pool).
        let kept = |d: &Demand<K>| match (in_turn(d), held.contains_key(&d.mind)) {
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
        let seat = |d: &Demand<K>| match (in_turn(d), d.floor, d.need.is_bound()) {
            (true, _, _) => holds(d).max(measured(d)),
            (false, Floor::Measured(_), _) => measured(d),
            (false, Floor::Unmeasured, false) => kept(d),
            (false, Floor::Unmeasured, true) => 0,
        };
        let rank = |d: &Demand<K>| -> (u8, u64) {
            match (in_turn(d), held.contains_key(&d.mind)) {
                (true, _) => (0, 0),
                (false, true) => (1, 0),
                (false, false) => (2, 0),
            }
        };
        if total(&seat) <= pool {
            let staged = self.grow(demands, &seat, &reclaimed, &rank, pool.saturating_sub(total(&seat)));
            if demands.iter().all(|d| staged.contains_key(&d.mind)) {
                return everyone(staged);
            }
        }
        // Custody: a turn running on a live grant (a mind waiting for a grant is a waiter). Seated at
        // her measured floor, a running turn at what it holds; an unmeasured floor at the binding she
        // keeps, else (a waiter) at her exact need, never left waiting because her floor is unknown.
        let custody = |d: &Demand<K>| in_turn(d) && holds(d) > 0;
        let admit = |d: &Demand<K>| match (custody(d), d.floor, kept(d)) {
            (true, _, _) => holds(d).max(measured(d)),
            (false, Floor::Measured(_), _) => measured(d),
            (false, Floor::Unmeasured, 0) => need(d),
            (false, Floor::Unmeasured, binding) => binding,
        };
        self.rotate(demands, pool, &custody, &admit, &reclaimed)
    }

    /// Stages 2 and 3 from seats `base`: exact needs grow whole in `order`, then bounds take what is
    /// left through `within`, never another mind's seat.
    fn grow(
        &self,
        demands: &[&Demand<K>],
        base: &dyn Fn(&Demand<K>) -> u32,
        reclaimed: &dyn Fn(&Demand<K>) -> u32,
        order: &dyn Fn(&Demand<K>) -> (u8, u64),
        mut left: u64,
    ) -> BTreeMap<K, u32> {
        let mut sorted: Vec<&Demand<K>> = demands.to_vec();
        sorted.sort_by_key(|d| (d.need.is_bound(), order(*d))); // stable: within a place, the caller's order
        let mut grants = BTreeMap::new();
        for d in sorted {
            let target = reclaimed(d).max(base(d));
            let growth = u64::from(target - base(d));
            let amount = match (growth <= left, d.need.is_bound()) {
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

    /// The service guarantee (see the type): seats by claim, head-of-line blocking, and lapses while
    /// anyone waits.
    fn rotate(
        &self,
        demands: &[&Demand<K>],
        pool: u64,
        custody: &dyn Fn(&Demand<K>) -> bool,
        admit: &dyn Fn(&Demand<K>) -> u32,
        reclaimed: &dyn Fn(&Demand<K>) -> u32,
    ) -> (BTreeMap<K, u32>, BTreeMap<K, TurnsEnded>) {
        // Her place: custody, then owed service by claim, then served (her seat ended a turn, or sat
        // unused through the grant interval after the one that granted it), the longest-seated last.
        let place = |d: &Demand<K>| -> (u8, u64) {
            match (custody(d), self.tenure.get(&d.mind).copied()) {
                (true, _) => (0, 0),
                (false, Some(Tenure::Seated { since, at, claim })) if d.turns_ended <= at && since == self.tick => (1, claim),
                (false, Some(Tenure::Seated { since, .. })) => (2, u64::MAX - since),
                (false, Some(Tenure::Waiting { since })) => (1, since),
                (false, None) => (1, self.tick), // `note` stamps every mind first; unstamped = just arrived
            }
        };
        let mut order: Vec<&Demand<K>> = demands.to_vec();
        order.sort_by_key(|d| place(*d)); // stable: within a place, the caller's order
        let mut left = pool;
        let mut waits = false;
        let mut seats: BTreeMap<K, u32> = BTreeMap::new();
        for d in order {
            let amount = match (custody(d), waits, u64::from(admit(d)) <= left) {
                (true, _, _) => admit(d), // custody: never preempted, never shrunk
                (false, true, _) => 0,
                (false, false, true) => admit(d),
                // An unmeasured bound seats nothing of her own: she takes what is left, if any.
                (false, false, false) => match (d.floor, d.need.is_bound(), u32::try_from(left).ok().and_then(Tokens::new)) {
                    (Floor::Unmeasured, true, Some(room)) => d.need.within(room.get()).get().min(room.get()),
                    _ => {
                        waits = true; // the head of the line: no room is handed past her
                        0
                    }
                },
            };
            left = left.saturating_sub(u64::from(amount));
            if amount > 0 {
                seats.insert(d.mind, amount);
            }
        }
        if !waits {
            // Everyone is seated: what the seats leave grows needs above them, exact first.
            let seat = |d: &Demand<K>| seats.get(&d.mind).copied().unwrap_or(0); // unwrap_or: every mind is seated here
            return (self.grow(demands, &seat, reclaimed, &place, left), BTreeMap::new());
        }
        // Someone waits: every grant ends with the turn it serves, so no holder re-enters ahead of her.
        let lapses = demands.iter().filter(|d| seats.contains_key(&d.mind)).map(|d| (d.mind, d.turns_ended)).collect();
        (seats, lapses)
    }
}
