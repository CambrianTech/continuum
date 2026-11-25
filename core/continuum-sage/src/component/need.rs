//! Need: how much a component wants to act, said by the component, with facts the engine adds.
//!
//! Object model 5.7 ("Need"). Most components never need a decision: one waiting on its intake costs
//! nothing and runs when its input arrives. Need matters where capacity is scarce (the Core, the GPU,
//! disk, network): a part asks that capacity's dispatcher with its need, and a tree of cheap decisions
//! honors it (a mind folds her parts' needs; the Core's dispatcher picks the next mind by need, then
//! waiting age, then switch cost). A component's `QOS` is its class floor; `Need` is the live signal.

use super::Key;
use crate::dataflow::TopicName;

/// How much it wants to act, in its own judgment. Ordered: `Content < Want < Urgent`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum Urgency {
    /// Nothing new: happy to rest.
    #[default]
    Content,
    /// It has a next step worth taking when nothing more urgent waits.
    Want,
    /// It really needs to act now (someone addressed it; a result it waited on arrived).
    Urgent,
}

impl Urgency {
    /// The longest a ready subject at this urgency may wait for a scarce capacity's next turn: the
    /// guarantee every scheduling policy is held to, never a tuning a policy trades (Joel, 2026-11-24:
    /// "Any over 2 s are wrong. Probably over 1 s"). A subject that wants or needs to act has it; a
    /// more urgent one is served sooner by preference, not by a looser bound for the others.
    ///
    /// `Content` work (dreaming, learning in the background) has no per-item bound: its guarantee is
    /// its class's queue staying in check, its served rate keeping up with its arrival rate, like a
    /// person who learns through the day and dreams at night (Joel, 2026-11-25). A 1 s bound on each
    /// of twenty dreams made the Core rotate through them every second (the M5, 21:24 to 21:34Z:
    /// 2,509 bound violations, all background).
    pub fn act_bound() -> std::time::Duration {
        std::time::Duration::from_secs(1)
    }

    /// Her per-item bound, at her urgency: the act bound, or none for background work.
    pub fn wait_bound(self) -> Option<std::time::Duration> {
        match self {
            Self::Content => None,
            Self::Want | Self::Urgent => Some(Self::act_bound()),
        }
    }
}

/// A component's need as published: its own urgency, and the engine's facts beside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Need {
    pub urgency: Urgency,
    /// Inputs waiting in its intake when this was published: a fact it cannot set.
    pub waiting: usize,
}

impl Need {
    /// The higher of two needs: a tree folds its children's needs this way. Urgency decides;
    /// among equals the one with more waiting.
    pub fn max(self, other: Need) -> Need {
        if (other.urgency, other.waiting) > (self.urgency, self.waiting) { other } else { self }
    }
}

/// The topic every component's need is published on, keyed like its presence (by its name).
pub const NEED: TopicName = TopicName("sage/need");

/// The key a component's need is published under: its name.
pub fn need_key(name: &'static str) -> Key {
    super::presence_key(name)
}
