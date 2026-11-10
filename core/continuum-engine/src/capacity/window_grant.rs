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

use crate::inference::planning::window_allocator::Tokens;

mod lease;
pub(crate) use lease::{record_reclaimed_grant, record_refused_grant};
pub use lease::is_grant_retiring;
pub use lease::{publish_fair_grants, FairGrantState, begin_grant_handoff, GrantHandoff, HandoffRefused, grant_with_reservations, honoring_reservations, honoring_reservations_and_receipts, reserve_attempt, settle, GrantReservation, ReserveRefused, TerminalReceipt};

/// One publication of one mind's grant: monotonic and never reused in this process. A grant that
/// moves, or a new engine incarnation, takes a new revision, so a lease priced at an old one fails
/// even when the size came back (shrink then regrow to the same size is the ABA case a size compare
/// would miss).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GrantRevision(u64);

impl GrantRevision {
    /// A test's revision literal (production revisions are only issued at a publish).
    #[cfg(test)]
    pub(crate) const fn test(n: u64) -> Self {
        Self(n)
    }

    /// The next revision this process has never issued.
    fn issue() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        Self(NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

/// The grant owner that issued a revision: one per process (the serving daemon's grant table), so
/// a revision from another owner (another core on the grid, a restarted one) is never this one's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GrantOwner(uuid::Uuid);

impl GrantOwner {
    /// The owner's id on a wire (a residency or grant registered with an engine names it).
    pub fn id(&self) -> uuid::Uuid {
        self.0
    }

    /// This process's grant owner.
    pub fn this_process() -> Self {
        static OWNER: std::sync::OnceLock<GrantOwner> = std::sync::OnceLock::new();
        *OWNER.get_or_init(|| GrantOwner(uuid::Uuid::new_v4()))
    }
}

/// What a request is priced against: her grant's size and its IDENTITY, the revision that
/// published it on which engine incarnation by which owner. A reserve compares the identity,
/// never the size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GrantLease {
    pub tokens: u32,
    pub revision: GrantRevision,
    /// The engine incarnation the grant is on (pid, OS start time, port: a restarted pid is another).
    pub engine: Option<crate::inference::engine_residency::EngineIncarnation>,
    pub owner: GrantOwner,
}

impl GrantLease {
    /// The same publication of the same grant: revision, incarnation and owner.
    pub fn same_identity(&self, other: &GrantLease) -> bool {
        (self.revision, self.engine, self.owner) == (other.revision, other.engine, other.owner)
    }
}

/// Stamp `next`'s revisions against `current` (what is published now): a mind whose grant is
/// unchanged on the same engine incarnation keeps her revision; any other takes a new one.
pub fn stamp_revisions(current: Option<&PoolGrants>, next: &mut PoolGrants) {
    let same_engine = current.filter(|c| c.engine == next.engine);
    let revisions = next
        .by_mind
        .iter()
        .map(|(mind, tokens)| {
            let kept = same_engine
                .filter(|c| c.grant_for(*mind) == Some(*tokens))
                .and_then(|c| c.revisions.get(mind).copied());
            (*mind, kept.unwrap_or_else(GrantRevision::issue)) // unwrap_or_else: a moved grant (or a new engine) takes a new revision
        })
        .collect();
    next.revisions = revisions;
}

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
    /// The engine incarnation these grants are on, as the serving layer reads it
    /// (`LlamaServerControl::served_incarnation`); a new one re-issues every revision.
    pub engine: Option<crate::inference::engine_residency::EngineIncarnation>,
    /// Each mind's grant revision ([`stamp_revisions`]).
    pub revisions: BTreeMap<uuid::Uuid, GrantRevision>,
}

impl PoolGrants {
    /// Her grant, when she is one of the minds on this pool.
    pub fn grant_for(&self, persona: uuid::Uuid) -> Option<u32> {
        self.by_mind.get(&persona).copied()
    }

    /// Her grant with the revision that published it.
    pub fn lease_for(&self, persona: uuid::Uuid) -> Option<GrantLease> {
        Some(GrantLease {
            tokens: self.grant_for(persona)?,
            revision: *self.revisions.get(&persona)?,
            engine: self.engine,
            owner: GrantOwner::this_process(),
        })
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

/// Her current grant and its revision: what a request is priced against and reserved by.
pub fn current_lease(persona: uuid::Uuid) -> Option<GrantLease> {
    GRANTS.get()?.borrow().as_ref()?.lease_for(persona)
}

/// Why one grants evaluation granted nobody: every way `grant_inputs` has nothing to grant from,
/// typed (card 5e79e1c1; Codex: "type those truthfully").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoGrants {
    /// The engine does not serve yet (relaunching, faulted, not adopted).
    NotReady,
    /// The engine serves but reported a 0-token per-sequence window.
    ServedWindowZero,
    /// The engine reported no pool (`context_geometry.allocated_tokens` = 0).
    PoolZero,
    /// The served model is not in the registry, so its trained window is unknown.
    ModelUnresolved,
    /// The engine's pool kind cannot be read: no lane pidfile, or no recorded launch.
    PoolKindUnknown,
}

/// Why a measured mind's requirement entered no demand on one evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Excluded {
    /// Her requirement has no observed need yet.
    NeedUnobserved,
    /// Her requirement has no wish.
    NoWish,
    /// Her target rounds to no tokens.
    NoTarget,
}

/// One run of the grants publisher, published EVERY time it runs, changed or not: the grants it
/// published with, and, when it granted nobody, why; and each requirement it left out, and why. One
/// value, so a mind reads her grant and its reason from the same evaluation, never two watches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GrantEvaluation {
    /// Rises by one per evaluation: a waiter waits for one newer than what it saw.
    pub seq: u64,
    /// When this evaluation began reading its inputs: a mind measured before it is in them.
    pub started: std::time::Instant,
    /// The publisher's own interval between evaluations: a waiter that sees none begin within two
    /// of them knows the publisher stalled (`mind_window::WindowRefused::PublisherSilent`).
    pub cadence: std::time::Duration,
    pub engine: Option<crate::inference::engine_residency::EngineIncarnation>,
    /// The grants current after this evaluation (`None` when none are published).
    pub grants: Option<PoolGrants>,
    /// Why nobody could be granted; `None` when the inputs were there.
    pub nobody: Option<NoGrants>,
    /// Each requirement this evaluation could not turn into a demand.
    pub excluded: BTreeMap<uuid::Uuid, Excluded>,
    /// Each demand that reached the publisher and holds no grant after it, and why (Codex on #5890:
    /// a demand waiting for room is not one that never arrived).
    pub waiting: BTreeMap<uuid::Uuid, Waiting>,
}

/// Why a demand the publisher received holds no grant after its evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Waiting {
    /// Her need is above what one sequence holds: no grant can seat her.
    NeedAboveCeiling { need: u32, ceiling: u32 },
    /// Her need is above the whole pool.
    NeedAbovePool { need: u32, pool: u32 },
    /// Her own grant is being reclaimed: she is regranted after its receipt.
    Reclaiming,
    /// Queued for room behind others (1-based `position` of `of`): a holder must yield first.
    Queued { position: usize, of: usize },
    /// Seatable, not reclaiming, and in no queue: the fair publisher left her out, which it should
    /// never do. Named, so a live read shows the defect instead of a reason it does not have.
    Unplaced,
}

impl GrantEvaluation {
    /// Why `persona` holds no grant after this evaluation; `None` when she holds one.
    pub fn ungranted(&self, persona: uuid::Uuid) -> Option<Ungranted> {
        if self.grants.as_ref().and_then(|g| g.grant_for(persona)).is_some() {
            return None;
        }
        Some(match (self.nobody, self.excluded.get(&persona), self.waiting.get(&persona)) {
            (Some(nobody), _, _) => Ungranted::Nobody(nobody),
            (None, Some(excluded), _) => Ungranted::Excluded(*excluded),
            (None, None, Some(waiting)) => Ungranted::Waiting(*waiting),
            (None, None, None) => Ungranted::NotADemand,
        })
    }
}

/// Why one mind holds no grant after an evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ungranted {
    /// The publisher could grant nobody.
    Nobody(NoGrants),
    /// Her requirement was there and left out.
    Excluded(Excluded),
    /// Her demand reached the publisher and waits, for the named reason.
    Waiting(Waiting),
    /// The publisher had inputs, but no requirement of hers reached it.
    NotADemand,
}

impl Ungranted {
    /// Her turn comes once room opens: she is queued behind others, or the engine is relaunching.
    /// She waits for a later evaluation, never fails her turn (Joel, 2026-11-09: "get what it needs
    /// and wait its turn"). A grant being RECLAIMED is not waited out inside her turn: the reclaim
    /// takes her mind's turn lock (`MindTurn::reclaim`), so a turn waiting on it waits on itself
    /// (the M5, d0abbddd1, 2026-11-10: Aris parked to the 1,500 s tick deadline, five times, while
    /// her seat never came back). Her turn ends, the reclaim proceeds, and she is regranted.
    /// Every other reason names a defect.
    pub fn awaits_room(&self) -> bool {
        matches!(self, Self::Waiting(Waiting::Queued { .. }) | Self::Nobody(NoGrants::NotReady))
    }
}

impl std::fmt::Display for Ungranted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Nobody(why) => write!(f, "the grants publisher could grant nobody: {why:?}"),
            Self::Excluded(why) => write!(f, "her requirement was left out of the demands: {why:?}"),
            Self::Waiting(why) => write!(f, "her demand reached the grants publisher and waits: {why:?}"),
            Self::NotADemand => f.write_str("no requirement of hers reached the grants publisher"),
        }
    }
}

static EVALUATIONS: std::sync::OnceLock<tokio::sync::watch::Receiver<Option<GrantEvaluation>>> = std::sync::OnceLock::new();

/// Install the grants publisher's evaluations beside its grants ([`install_grants`]). Set once.
pub fn install_evaluations(rx: tokio::sync::watch::Receiver<Option<GrantEvaluation>>) -> bool {
    EVALUATIONS.set(rx).is_ok()
}

/// A receiver of the publisher's evaluations: what an ungranted mind waits on. `None` before the
/// serving daemon installs them.
pub fn evaluations_receiver() -> Option<tokio::sync::watch::Receiver<Option<GrantEvaluation>>> {
    EVALUATIONS.get().cloned()
}

/// One measured mind's current demand on the pool: her floor and target, and whether a turn of hers
/// is in flight now. A mind not measured yet has no `Demand`: there is no number to grant her
/// (`Measure::Unmeasured`), and her first view measures her.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Demand<K> {
    pub mind: K,
    /// The least her turn holds: below it she cannot think (the median of her held turns, else her
    /// wish). Never trimmed here; a need above what one sequence holds waits.
    pub need: Tokens,
    /// What she would read whole (her wish with headroom), granted when the pool holds every target.
    pub target: Tokens,
    /// Her wish without headroom: a grant that covers it is kept; one that does not grows to her
    /// target, so headroom is room to grow into (about one regrant per headroom of growth).
    pub wish: Tokens,
    pub in_flight: bool,
    /// She has completed a turn since her grant last changed (the fair publisher reads it off her
    /// receipts): only then may her grant shrink toward her share, never under a view she began on it.
    pub settled: bool,
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
/// - when the wants fit but the targets do not, the surplus above every need is shared toward the
///   targets in proportion to each mind's remaining growth, never leaving room idle; a grant shrinks
///   toward her share only once she has served on it, and drift within one headroom never rebinds;
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
        // What she holds, as the policy reads it. A settled mind with no turn in flight holds no more
        // than her target: what she holds above it is released at her next view start (the M5,
        // 2026-11-07, 19426f454: grants grown to 197k toward untrimmed demand made every act slower,
        // 71 acts and a 104 s median against 158 and 50 s the hour before; Fable on 7b7a696a). A mind
        // that has not served on her grant, or whose turn is in flight, keeps all of it.
        let holds = |d: &Demand<K>| {
            let held = held.get(&d.mind).copied().unwrap_or(0).min(d.ceiling); // unwrap_or: a mind with no grant yet holds nothing
            match d.settled && !d.in_flight {
                true => held.min(d.target.get().min(d.ceiling)),
                false => held,
            }
        };
        // Only a mind who holds nothing is dropped for a need above what one sequence holds now (she
        // waits). A holder whose need outgrew it keeps what she holds as her need: her binding stays
        // counted, so it can never be bound beside the pool uncounted (Cormac on #5745: A holding 40k,
        // need 70k, dropped while B was granted 60k bound 100k on a 65,536 pool).
        let need = |d: &Demand<K>| match d.need.get() > d.ceiling {
            true => holds(d),
            false => d.need.get(),
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
        // Every want fits but not every target: the surplus above the needs is shared toward the
        // targets in proportion to each mind's remaining growth (flexbox's grow, the principle the act
        // allocation searches by), so the largest demand gets the largest share. #5813 grew the smallest
        // growth first, and the largest demand was reached last on every publish (the M5, 2026-11-07
        // 08:46-09:46Z, a947e4ed1: Atlas granted 14,353 against a 111,165-token view, 18 acts at 42%
        // reuse, while the pool stayed with whoever was filled first; Fable and Cormac on 20b830fb).
        // A grant moves only where it may: a mind whose turn is in flight, or who has not served since
        // her grant last changed, never shrinks; a settled mind within one headroom of her share keeps
        // what she holds (no rebind for drift, Cormac on #5758); anyone else moves toward her share.
        // Every mind gets her low end, and what the low ends leave is filled in the same proportion.
        if total(&want) <= pool {
            let surplus = pool - total(&need);
            let growth = |d: &Demand<K>| u64::from(target(d) - need(d));
            // Positive here: the targets overflow the pool, which the wants fit, so some target is above its need.
            let growths: u64 = demands.iter().map(|d| growth(d)).sum();
            let share = |d: &Demand<K>| match u32::try_from(surplus * growth(d) / growths) {
                Ok(portion) => need(d) + portion,
                Err(_) => target(d), // a portion past u32 is past any target
            };
            // Her low end and her high end. Only a settled mind holding more than one headroom above
            // her share gives anything back, and only down to her share; every other mind keeps her
            // want and grows toward her share when the room is there (a mind below her share is never
            // shrunk to feed another).
            let bounds = |d: &Demand<K>| -> (u32, u32) {
                let share = share(d);
                let held = holds(d);
                let above_band = f64::from(held) > f64::from(share) * crate::inference::planning::serving_plan::SENT_HEADROOM;
                match d.settled && !d.in_flight && above_band {
                    true => (share, share),
                    false => (want(d), want(d).max(share)),
                }
            };
            let bounds: Vec<(u32, u32)> = demands.iter().map(|d| bounds(d)).collect();
            // Every low end is at most her want, and the wants fit: the low ends fit.
            let left = pool - bounds.iter().map(|(low, _)| u64::from(*low)).sum::<u64>();
            let room: u64 = bounds.iter().map(|(low, high)| u64::from(high - low)).sum();
            return demands
                .iter()
                .zip(&bounds)
                .map(|(d, &(low, high))| {
                    let range = u64::from(high - low);
                    let add = match room <= left {
                        true => range,
                        false => left * range / room,
                    };
                    // `add` is at most `range`, a u32 difference.
                    (d.mind, low + u32::try_from(add).map_or(high - low, |add| add))
                })
                .collect();
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
        // her floor (down to her need when she is not in a turn and holds more; a turn in flight keeps
        // what it holds), so a mind left out never adds to what the pool carries (Cormac on #5745: A
        // grown first while B kept her 25k bound 75k on a 65,536 pool). Growth and newcomers are
        // admitted whole, in order (turns in flight, holders, the rest), from what the floors leave.
        let floor = |d: &Demand<K>| match (d.in_flight, held.contains_key(&d.mind)) {
            (true, _) => holds(d),
            (false, true) => holds(d).min(need(d)),
            (false, false) => 0,
        };
        let mut left = pool.saturating_sub(total(&floor));
        let rank = |d: &Demand<K>| match (d.in_flight, held.contains_key(&d.mind)) {
            (true, _) => 0,
            (false, true) => 1,
            (false, false) => 2,
        };
        let mut order: Vec<&Demand<K>> = demands.to_vec();
        order.sort_by_key(|d| rank(d)); // stable: within a rank, the caller's order
        let mut grants = BTreeMap::new();
        for d in order {
            let target = reclaimed(d).max(floor(d));
            let growth = u64::from(target - floor(d));
            let amount = match growth <= left {
                true => {
                    left -= growth;
                    target
                }
                false => floor(d), // no room to grow: she keeps her floor (0 = a newcomer waits)
            };
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

    use crate::inference::planning::window_allocator::observed::Observed;
    use crate::inference::planning::window_allocator::test_tokens as t;

    /// A mind whose need is her target (a wish with no held turn yet).
    fn demand(mind: u32, need: u32, in_flight: bool, ceiling: u32) -> Demand<u32> {
        Demand { mind, need: t(need), target: t(need), wish: t(need), in_flight, settled: true, ceiling }
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
        // Targets as production builds them: her held need with the sent headroom (x1.25). A drift
        // inside the headroom keeps her grant; that headroom is what makes drift free (7b7a696a).
        let real = |m, need: u32| Demand { mind: m, need: t(need), target: t(need + need / 4), wish: t(need), in_flight: false, settled: true, ceiling: pool };
        let drift = DemandFollowing.grant(&[real(1, 44_000), real(2, 31_000)], &held, pool);
        assert_eq!(drift[&1], 50_000, "her need drifted down inside her headroom: she keeps what she holds");
        assert_eq!(drift[&2], 38_750, "her need rose above what she holds: she grows to her target");
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
    // Aris's 88,380, and Atlas got nothing. Every need is granted before any growth; the surplus then
    // goes to every mind in proportion to her remaining growth.
    #[test]
    fn needs_are_granted_before_any_target_and_targets_only_when_all_fit() {
        let pool = 524_288;
        let mind = |m, need, target| Demand { mind: m, need: t(need), target: t(target), wish: t(target), in_flight: false, settled: true, ceiling: 262_144 };
        let wishes = [mind(1, 70_000, 262_144), mind(2, 60_000, 104_015), mind(3, 50_000, 88_380), mind(4, 40_000, 90_000)];
        let grants = DemandFollowing.grant(&wishes, &BTreeMap::new(), pool);
        // 220,000 of needs leave 304,288, shared in proportion to growths 192,144 / 44,015 / 38,380 / 50,000.
        assert_eq!(grants, BTreeMap::from([(1, 250_154), (2, 101_268), (3, 85_985), (4, 86_880)]), "every need first, then the surplus: {grants:?}");
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
        let mind = |m, need, target| Demand { mind: m, need: t(need), target: t(target), wish: t(target), in_flight: false, settled: true, ceiling: 262_144 };
        let demands = [mind(1, 19_384, 78_000), mind(2, 14_499, 35_000), mind(3, 16_004, 51_000), mind(4, 14_477, 42_000)];
        let held = BTreeMap::from([(1, 262_144), (2, 14_445), (3, 18_213), (4, 14_242)]);
        let grants = DemandFollowing.grant(&demands, &held, pool);
        assert_eq!(grants, BTreeMap::from([(1, 78_000), (2, 35_000), (3, 51_000), (4, 42_000)]), "every target: {grants:?}");
        let roomy = DemandFollowing.grant(&demands, &BTreeMap::from([(1, 100_000)]), pool);
        // A settled holder above her target is released to it even when the pool has room: every held
        // token slows every act (the M5, 2026-11-07: grants grown to 197k, 71 acts vs 158; 7b7a696a).
        assert_eq!(roomy[&1], 78_000, "a settled holder above her target is released to it");
        assert_eq!(DemandFollowing.grant(&demands, &grants, pool), grants, "a steady demand is a steady grant");
    }

    // what this catches (the M5, 2026-11-07 06:50Z, fe91c2b55): surplus left idle when the wants fit
    // and the targets do not. Benchy 24,012 / Aiko 30,195 / Aris 29,589 / Atlas 34,018 held needs on a
    // 152,832 pool, targets 150-557k: 35,018 was granted to no one. The surplus is shared in proportion
    // to each mind's growth (targets capped at one sequence), no grant falls below a need, the pool is
    // spent but for rounding, and the next tick on those grants moves nothing.
    #[test]
    fn surplus_above_every_need_grows_minds_toward_their_targets() {
        let pool = 152_832;
        let mind = |m, need, target| Demand { mind: m, need: t(need), target: t(target), wish: t(target), in_flight: false, settled: true, ceiling: pool };
        let demands = [mind(1, 24_012, 150_000), mind(2, 30_195, 557_000), mind(3, 29_589, 200_000), mind(4, 34_018, 180_000)];
        // Negative control: the needs alone leave the surplus this spends.
        let needs: u32 = demands.iter().map(|d| d.need.get()).sum();
        assert_eq!(pool - needs, 35_018, "precondition: the 06:50Z ungranted surplus");
        let grants = DemandFollowing.grant(&demands, &BTreeMap::new(), pool);
        assert_eq!(grants, BTreeMap::from([(1, 33_003), (2, 38_947), (3, 38_384), (4, 42_497)]), "{grants:?}");
        assert_eq!(DemandFollowing.grant(&demands, &grants, pool), grants, "a steady demand is a steady grant");
    }

    // what this catches (the M5, 2026-11-07 08:46-09:46Z, a947e4ed1): a slow mind starved by the
    // surplus order. #5813 grew the smallest growth first, so Atlas, with the largest view, was
    // reached last on every publish and sat at a 14,353 grant against a 111,165-token view (18 acts at
    // 42% reuse) while Aris held far above her share. Shared in proportion, the largest demand gets
    // the largest share, and a settled holder gives back what she holds above hers. One who has not
    // served on her grant yet is never shrunk: the others grow only from what she leaves.
    #[test]
    fn the_largest_demand_gets_the_largest_share_and_only_a_settled_holder_gives_back() {
        let pool = 152_832;
        let mind = |m, need, target, settled| Demand { mind: m, need: t(need), target: t(target), wish: t(target), in_flight: false, settled, ceiling: pool };
        let held = BTreeMap::from([(1, 80_000), (2, 30_000), (3, 14_353), (4, 28_479)]);
        let demands = [mind(1, 30_000, 60_000, true), mind(2, 30_000, 70_000, true), mind(3, 14_353, 140_000, true), mind(4, 20_000, 120_000, true)];
        let grants = DemandFollowing.grant(&demands, &held, pool);
        assert_eq!(grants, BTreeMap::from([(1, 35_934), (2, 37_912), (3, 39_205), (4, 39_780)]), "{grants:?}");
        assert!(grants[&3] > 2 * held[&3], "Atlas grows from 14,353: {grants:?}");
        assert_eq!(DemandFollowing.grant(&demands, &grants, pool), grants, "a steady demand is a steady grant");
        // Aris has not served on her 80,000 yet: she keeps it. The pool is spent, so the others keep
        // what they hold (none below her share is shrunk to feed another) and Atlas grows once Aris
        // has served (above).
        let unserved = [mind(1, 30_000, 60_000, false), demands[1], demands[2], demands[3]];
        let grants = DemandFollowing.grant(&unserved, &held, pool);
        assert_eq!(grants, held, "nothing moves until the holder above her share has served: {grants:?}");
    }

    // what this catches (the M5, 2026-11-07, 19426f454; card 7b7a696a): grants grown toward
    // untrimmed demand kept. After the pool regrew to 755,456, minds held up to 197k, prompts grew to
    // 35-75k and every act slowed (71 acts at a 104 s median against 158 at 50 s). A settled mind with
    // no turn in flight is released down to her target (her held need with headroom) at her next
    // start; one that has not served on her grant keeps it until she has.
    #[test]
    fn a_settled_holder_above_her_target_is_released_to_it() {
        let pool = 755_456;
        let mind = |m, settled| Demand { mind: m, need: t(30_000), target: t(37_500), wish: t(400_000), in_flight: false, settled, ceiling: pool };
        let held = BTreeMap::from([(1, 197_000), (2, 30_000)]);
        let grants = DemandFollowing.grant(&[mind(1, true), mind(2, true)], &held, pool);
        assert_eq!(grants[&1], 37_500, "released to her target: {grants:?}");
        let grants = DemandFollowing.grant(&[mind(1, false), mind(2, true)], &held, pool);
        assert_eq!(grants[&1], 197_000, "not yet served on it: kept until she has: {grants:?}");
        let in_flight = Demand { in_flight: true, ..mind(1, true) };
        assert_eq!(DemandFollowing.grant(&[in_flight, mind(2, true)], &held, pool)[&1], 197_000, "never under a turn in flight");
    }

    // what this catches (Cormac on #5758): growing on every upward move of a target recomputed every
    // act, which regranted most acts as an append-only view grew. Her wish rises 300 tokens a call
    // and her target is the wish x 1.25: the grant holds while it covers the wish and regrants only
    // when the wish passes it, about once per 25% of growth.
    #[test]
    fn a_rising_wish_regrants_only_when_it_passes_the_grant() {
        let pool = 323_840;
        let at = |wish: u32| Demand { mind: 1u32, need: t(10_000), target: t(wish + wish / 4), wish: t(wish), in_flight: false, settled: true, ceiling: 262_144 };
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
        let mind = |m, need, target| Demand { mind: m, need: t(need), target: t(target), wish: t(target), in_flight: false, settled: true, ceiling: pool };
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
        let at = |need: Tokens| Demand { mind: 1u32, need, target: need, wish: need, in_flight: false, settled: true, ceiling };
        let bound = Observed::AtLeast { at_least: t(65_672) };
        let grants = DemandFollowing.grant(&[at(bound.within(ceiling))], &BTreeMap::new(), pool);
        assert_eq!(grants, BTreeMap::from([(1, ceiling)]), "a bound past one sequence is granted one sequence, never dropped");
        let exact = DemandFollowing.grant(&[at(Observed::Exact(t(65_672)).within(ceiling))], &BTreeMap::new(), pool);
        assert!(exact.is_empty(), "an exact need past the ceiling still waits for room: {exact:?}");
        // Negative control, the pre-fix read: the bound's value taken as her need drops her.
        let unclamped = DemandFollowing.grant(&[at(bound.value())], &BTreeMap::new(), pool);
        assert!(unclamped.is_empty(), "precondition: the unclamped bound is the drop this fixes: {unclamped:?}");
        assert_eq!(Observed::AtLeast { at_least: t(4_096) }.within(ceiling), t(4_096), "a bound within the ceiling is untouched");
    }

}
