//! SAGE's `Component`: what everything that works on its own inherits (object model 5.7). A part
//! of a mind, a stage of the Core, the node's feed, a grid service: each declares what it hears,
//! what it says, and how it handles one input, and the engine gives it everything else, once:
//!
//! - its worker: its own task, a bounded intake whose producers wait for room, in order;
//! - its subscriptions, wired by topic on its owner's [`Bus`], so a component observes another's
//!   output by naming the topic, never by holding the other component;
//! - provenance: each publication names the component's run and the revision of the input it
//!   handled;
//! - timers: [`Ctx::after`] delivers an input at a time the component chose, replacing any earlier
//!   timer under the same key, so none holds a handle to itself and none polls;
//! - lifecycle: [`Running`] stops it when dropped, and its last publications stay readable.
//!
//! Components work on their own and signal what they have when they have it; whoever cares
//! subscribes, and whoever reads a state reads the newest without waiting (CBAR's frame pipeline).
//! What a kind of owner adds (a mind: her bus, her identity, starting her parts) is a thin layer
//! above this, never a copy of it.
//!
//! One concern per file: [`Component`] and [`Presence`] here (the contract); `delivered` (what
//! reaches a worker); `wiring` (what it hears); `worker` (the engine and [`start`]); `running`
//! (its handles); `ctx` (what its handler is given); `observe` (its measured numbers); `need`,
//! `known`, `bus`; [`Owner`] (the parts one node or mind runs) and `profile` (where its time goes).

mod bus;
mod ctx;
mod delivered;
mod known;
mod need;
mod observe;
mod owner;
mod profile;
mod running;
mod wiring;
mod worker;

pub use bus::{node, Bus, Key, Taken};
pub use ctx::Ctx;
pub use delivered::Delivered;
pub use known::{Known, LastSeen, Novelty, Seen};
pub use need::{need_key, Need, Urgency, NEED};
pub use observe::{Observe, Telemetry, OBSERVE, TELEMETRY};
pub use owner::Owner;
pub use profile::{node_profile, Profiler};
pub use running::{Running, Teller};
pub use wiring::Wiring;
pub use worker::start;

use std::future::Future;

use crate::dataflow::{StageId, TopicName};
use crate::Qos;

/// Something that works on its own. Implement this and nothing else: [`start`] gives it the mechanism.
pub trait Component: Send + Sized + 'static {
    /// Everything that wakes it: each subscription's events or states, its owner's acts, its timers.
    type Input: Send + 'static;
    /// Its name, the same wherever it runs (its stage id).
    const NAME: &'static str;
    /// Its class, for any capacity it submits to.
    const QOS: Qos;
    /// Declare what it hears.
    fn wire(&self, wiring: &mut Wiring<'_, Self>);
    /// Handle one input: arithmetic on its own state, publishing through what it said it says.
    fn handle(&mut self, input: Self::Input, ctx: &mut Ctx<Self>) -> impl Future<Output = ()> + Send;
    /// Rest: give back what it holds (drop or page its caches) because its owner rests or its
    /// memory is wanted elsewhere. Its next input wakes it. The default holds nothing and does
    /// nothing.
    fn rest(&mut self) -> impl Future<Output = ()> + Send {
        async {}
    }
}

/// Where a component is in its life, owned by the engine and published on its owner's bus under
/// [`PRESENCE`], keyed by the component's name: any component can observe another's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Presence {
    /// Handling an input.
    Acting,
    /// Idle on its intake: no CPU spent, its caches warm.
    Ready,
    /// It gave back what it holds; its next input wakes it.
    Resting,
}

/// The topic every component's presence is published on.
pub const PRESENCE: TopicName = TopicName("sage/presence");

/// The key a component's presence is published under: its name.
pub fn presence_key(name: &'static str) -> Key {
    Key::of(StageId::named(name).as_uuid())
}

#[cfg(test)]
mod tests;
