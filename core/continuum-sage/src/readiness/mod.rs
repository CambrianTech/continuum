//! Readiness: the parts that get the next mind ready for the Core (THE-LANE-ON-SAGE.md, "Readiness").
//!
//! A swap is a pointer now; what decides when the lane can switch to a mind is whether she is ready: her KV hot, her
//! genes attached, her new tail rendered, her graph built. Each is a preparation of its own, running in parallel with
//! the Core's step, as her parts run in parallel in her mind (THE-WHOLE-MIND.md §3).
//!
//! [`Readiness`] is the one parent: a SAGE [`Component`](crate::component::Component) that does what every preparation shares.
//! It hears the lane's [`Horizon`] (who may step soon, with her slack), keeps each mind's [`Ready`] state, works the
//! most pressed mind first one unit at a time, measures what a unit costs so it can say when each mind will be ready,
//! gives back what it holds for minds who left the horizon (or when it rests), and publishes one [`Table`] the lane
//! reads. A leaf implements [`Readies`]: what a mind needs from it, how one unit of that advances, how to let her go.

mod part;
pub use part::{Readiness, ReadinessIn};

#[cfg(test)]
mod tests;

use std::future::Future;
use std::time::Duration;

use crate::dataflow::TopicName;
use crate::decide::Measured;
use crate::Qos;

/// Where the lane publishes its horizon: the Core's one, under `Key::ONE`.
pub const HORIZON: TopicName = TopicName("core/horizon");

/// The minds that may step soon, most pressed first or in any order: each with her slack against her bound. The lane
/// publishes it at its decisions; a mind absent from it is let go by every readiness part.
#[derive(Debug, Clone, PartialEq)]
pub struct Horizon<M> {
    pub minds: Vec<Upcoming<M>>,
}

/// One mind the lane may step soon.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Upcoming<M> {
    pub mind: M,
    /// How long until stepping her later would carry her past her bound.
    pub slack: Duration,
}

/// What a mind needs from a part now.
#[derive(Debug)]
pub enum Needs<W> {
    /// She is ready as far as this part goes.
    Nothing,
    /// This work makes her ready.
    Work(W),
}

/// One unit of a mind's work, advanced.
#[derive(Debug)]
pub enum Advanced<W, Why> {
    /// She is ready.
    Done,
    /// What is left of it.
    More(W),
    /// The unit is in flight (a DMA on its fence): its end arrives as one of the leaf's acts ([`Readies::act`]). One
    /// unit of a part is in flight at a time, so nothing is started behind a move its device has not finished.
    Started,
    /// It cannot make her ready; asked again when the horizon next changes.
    Refused(Why),
}

/// A mind's readiness as one part publishes it.
#[derive(Debug, Clone, PartialEq)]
pub enum Ready<Why> {
    Ready,
    /// Being prepared: ready in about `eta` (her work and the work of every more pressed mind ahead of her, at this
    /// part's measured cost a unit), `Unmeasured` before its first unit.
    Preparing { eta: Measured<Duration> },
    Refused(Why),
}

/// What one part publishes on its topic: each mind of the horizon, and how ready she is.
#[derive(Debug, Clone, PartialEq)]
pub struct Table<M, Why> {
    pub minds: Vec<(M, Ready<Why>)>,
}

impl<M: PartialEq, Why> Table<M, Why> {
    pub fn of(&self, mind: &M) -> Option<&Ready<Why>> {
        self.minds.iter().find(|(held, _)| held == mind).map(|(_, ready)| ready)
    }
}

/// A leaf: one preparation, declaring only what is its own.
pub trait Readies: Send + 'static {
    /// Its name, the same wherever it runs.
    const NAME: &'static str;
    const QOS: Qos;
    /// Where its [`Table`] is published.
    const TOPIC: TopicName;
    /// Who it prepares.
    type Mind: Copy + Eq + std::fmt::Debug + Send + Sync + 'static;
    /// One mind's preparation, as much of it as is left.
    type Work: Send;
    /// Why it cannot make a mind ready.
    type Refusal: Clone + std::fmt::Debug + PartialEq + Send + Sync + 'static;
    /// What only this leaf is handed (a block to hold, a fence that fired), routed to [`Readies::act`].
    type Act: Send + 'static;

    /// What `mind` needs from it now. A cheap check: asked of every mind whenever the horizon changes, so a mind it
    /// had made ready is prepared again once what it prepared changed.
    fn needs(&mut self, mind: Self::Mind) -> Needs<Self::Work>;

    /// How much of `work` is left, in its own unit (bytes, tokens, nodes): what its cost is measured by.
    fn left(work: &Self::Work) -> u64;

    /// Advance her work by one unit: a bounded piece, so a newer horizon is heard between units.
    fn advance(&mut self, mind: Self::Mind, work: Self::Work) -> impl Future<Output = Advanced<Self::Work, Self::Refusal>> + Send;

    /// One of its own inputs. When it ends a mind's unit in flight, it says whose and how; after every act the parent
    /// asks again what each mind not being prepared needs, so whatever the act changed is prepared next.
    fn act(&mut self, act: Self::Act) -> Option<(Self::Mind, Advanced<Self::Work, Self::Refusal>)>;

    /// Give back what it holds for her: she left the horizon, or its owner rests. A leaf that keeps custody of what
    /// it prepared (the pager keeps her block) only stops putting her first.
    fn release(&mut self, mind: Self::Mind);
}
