//! The placer: the one SAGE component that owns a [`Ledger`] (UNIFIED-PAGING.md, slice 3a). Nobody
//! holds it, awaits it or locks it; a consumer spends a granted claim straight from its atomics.
//!
//! - **In: acts, by its `Teller`.** Lossless, in order, from any number of consumers. An event topic
//!   has one writer and drops a lagging subscriber's oldest, so it suits neither many askers nor an ask
//!   that must be answered. A consumer names each ask (`AskId`: its owner and its own sequence) and
//!   refers to it after: commit, unwind, release, forget. The authority tells it room the same way.
//! - **Out: state, never events.** Each owner's asks are a state keyed by the owner on `Domain::ASKS`:
//!   every live ask and where it stands (waiting with its shortfall, granted with the claim, committed,
//!   retired, or a terminal answer). It is the whole table each time, so a consumer that wakes late or
//!   restarts reads where everything stands; nothing hangs on catching one event. `Domain::SHORT` is
//!   each place's shortfall to its waits: the law's demand, and how far each wait is from its grant.
//!
//! One placer per domain: a domain names its places, its quantity and its topics, so bytes on a node's
//! tiers and minds awake on it are two placers on the same code.

use std::collections::HashMap;
use std::fmt::Debug;

use super::claim::{Claim, Generation, Need, Owner};
use super::core::PlacerCore;
use super::ledger::Refusal;
use super::quantity::Quantity;
use crate::component::{Bus, Component, Ctx, Key, Taken, Wiring};
use crate::dataflow::{Publisher, TopicName};
use crate::Qos;

/// What one placer places: its places, the quantity they hold, and its topics.
pub trait Domain: Send + 'static {
    type Place: Copy + PartialEq + Debug + Send + Sync + 'static;
    type Quantity: Quantity;
    /// The placer's component name.
    const NAME: &'static str;
    /// Each owner's asks, a state keyed by the owner.
    const ASKS: TopicName;
    /// Each place's shortfall to its waits, a state.
    const SHORT: TopicName;
}

/// One ask, named by its consumer: its owner and the consumer's own sequence for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AskId {
    pub owner: Owner,
    pub seq: u64,
}

/// The key an owner's asks are published under.
pub fn key_of(owner: Owner) -> Key {
    Key(u128::from(owner.0))
}

/// What the placer is told.
#[derive(Debug, Clone, PartialEq)]
pub enum Act<P, Q> {
    Prepare { ask: AskId, generation: Generation, needs: Vec<Need<P, Q>> },
    /// The claim's spending is over: what it consumed is held.
    Commit(AskId),
    /// It will not be spent: everything it prepared returns.
    Unwind(AskId),
    /// Its holding was released by its allocator (the consumer's fence): free again.
    Release(AskId),
    /// A terminal answer has been read: drop it from the owner's table.
    Forget(AskId),
    /// A place's room, as the authority measures it.
    Room { place: P, capacity: Q, budget: Q },
    /// Its committed holding on `place` can run on `floor`, at `priority`: the law may ask it to shrink.
    Elastic { ask: AskId, place: P, floor: Q, priority: u8 },
    /// Its waiting ask's priority, as the law measures it (temperature, age): it may now ask lower
    /// holdings to shrink.
    Prioritize { ask: AskId, priority: u8 },
    /// Its holding on `place` released `by` after its own fence: free again, and its shrink answered.
    Shrunk { ask: AskId, place: P, by: Q },
}

/// Where one ask stands.
#[derive(Debug, Clone)]
pub enum Status<P, Q> {
    Waiting { short: Vec<Need<P, Q>> },
    Granted(Claim<P, Q>),
    Committed,
    /// Held by an earlier generation of its owner: charged until released.
    Retired,
    Never { place: P, need: Q, capacity: Q },
    Unknown { place: P },
    Stale,
    Refused(Refusal),
    /// Committed, and asked to shrink to `to` on `place` so a higher wait fits: it releases the rest
    /// after its own fence and says [`Act::Shrunk`].
    Shrink { place: P, to: Q },
}

/// One owner's asks, as the placer publishes them: (its sequence, where it stands).
#[derive(Debug, Clone)]
pub struct Asks<P, Q>(pub Vec<(u64, Status<P, Q>)>);

/// Each place's shortfall to its waits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortfall<P, Q>(pub Vec<Need<P, Q>>);

/// The component around one domain's [`PlacerCore`]: it hears acts by its teller, applies them, and says
/// each changed owner's table and the shortfall. Nothing else.
pub struct Placer<D: Domain> {
    core: PlacerCore<D>,
    bus: &'static Bus,
    says: HashMap<Owner, Publisher<Asks<D::Place, D::Quantity>>>,
    short: Publisher<Shortfall<D::Place, D::Quantity>>,
    /// Reused: the tables a settle changed.
    changed: Vec<(Owner, Asks<D::Place, D::Quantity>)>,
}

impl<D: Domain> Placer<D> {
    /// Claim the domain's shortfall topic on `bus` (an owner's asks are claimed at its first ask);
    /// refused if another placer holds it.
    pub fn new(bus: &'static Bus) -> Result<Self, Taken> {
        Ok(Self { core: PlacerCore::default(), bus, says: HashMap::new(), short: bus.publisher(D::SHORT, Key::ONE)?, changed: Vec::new() })
    }

    /// The owner's table topic first: an ask whose answer cannot be said is never prepared, so no claim is
    /// held for an owner who could not read it. Answers whether `act` may be applied.
    fn sayable(&mut self, act: &Act<D::Place, D::Quantity>) -> bool {
        let Act::Prepare { ask, .. } = act else { return true };
        if self.says.contains_key(&ask.owner) {
            return true;
        }
        match self.bus.publisher(D::ASKS, key_of(ask.owner)) {
            Ok(says) => {
                self.says.insert(ask.owner, says);
                true
            }
            Err(taken) => {
                continuum_base::probe!(class = "place.asks.taken", placer = D::NAME, owner = ask.owner.0, topic = taken.0 .0,
                    "an owner's asks key is held elsewhere: its ask is not prepared");
                false
            }
        }
    }
}

impl<D: Domain> Component for Placer<D> {
    type Input = Act<D::Place, D::Quantity>;
    const NAME: &'static str = D::NAME;
    const QOS: Qos = Qos::Interactive;

    /// It hears nothing on the bus: every ask and every room arrives by its teller.
    fn wire(&self, _: &mut Wiring<'_, Self>) {}

    async fn handle(&mut self, act: Self::Input, ctx: &mut Ctx<Self>) {
        if self.sayable(&act) {
            self.core.apply(act);
        }
        let short_changed = self.core.settle();
        self.core.changed(&mut self.changed);
        for (owner, asks) in self.changed.drain(..) {
            if let Some(says) = self.says.get_mut(&owner) {
                ctx.publish(says, asks);
            }
        }
        if short_changed {
            ctx.publish(&mut self.short, self.core.shortfall().clone());
        }
    }
}
