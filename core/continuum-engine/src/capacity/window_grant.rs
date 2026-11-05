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
//! what is left, so no bound ever displaces a mind. Whenever a mind is still left without a grant,
//! the grant rotates, each waiter served within a stated bound of turn endings (plan goal 9: "no
//! mind is ever starved"). The pool is the engine's; paging a cold mind out and making the next one
//! wait for admission are how the node accommodates past it. [`super::window_sim`] validates the contract against today's equal
//! split; ONE-BUDGET slice 3b's solver replaces this policy with `cambrian-flex` claims.

use std::collections::BTreeMap;

use crate::inference::planning::window_allocator::observed::Observed;
use crate::inference::planning::window_allocator::{Floor, Tokens};

mod demand_following;
pub use demand_following::DemandFollowing;
#[cfg(any(test, feature = "test-fixtures"))]
pub mod schedule;
#[cfg(test)]
mod tests;

/// How many turns a mind has ended this boot: the turn ledger's count (`TurnShape::turns`), bumped
/// as each turn's guard drops. A turn's end is read from it, never sampled on a grant tick, so a
/// turn that ends and restarts between two ticks is still seen to have ended.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct TurnsEnded(pub u64);

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
    /// Grants that end with the turn they serve, while a mind waits ([`DemandFollowing`]'s service
    /// guarantee): hers is refused once her ended-turn count passes this.
    pub lapses: BTreeMap<uuid::Uuid, TurnsEnded>,
}

impl PoolGrants {
    /// Her grant, when she is one of the minds on this pool and it has not lapsed: `ended` is her
    /// ended-turn count now, so a turn starting after the one her grant served is refused (she waits
    /// for the next grant, behind the mind owed service).
    pub fn grant_for(&self, persona: uuid::Uuid, ended: TurnsEnded) -> Option<u32> {
        match self.lapses.get(&persona) {
            Some(&lapse) if ended > lapse => None,
            _ => self.by_mind.get(&persona).copied(),
        }
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

/// Her current grant on this node's engine, given her ended-turn count now: `None` before the
/// daemon installs its grants, before the engine serves, while she has none yet, or once it lapsed.
/// A `watch` borrow, no I/O.
pub fn current_grant(persona: uuid::Uuid, ended: TurnsEnded) -> Option<u32> {
    GRANTS.get()?.borrow().as_ref()?.grant_for(persona, ended)
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
    /// Her ended-turn count now: how the policy knows a turn ended between two grants.
    pub turns_ended: TurnsEnded,
    /// The most she may be granted: what one sequence of the engine can hold.
    pub ceiling: u32,
}

/// Grants every mind a window from the pool, given every mind's current demand and the grants
/// they hold now.
pub trait WindowPolicy<K> {
    fn grant(&mut self, demands: &[Demand<K>], held: &BTreeMap<K, u32>, pool_tokens: u32) -> BTreeMap<K, u32>;
}
