//! Quality-of-service classes, highest first (GCD's model: a few classes, any number of queues).

/// What a unit of work is, which decides when it runs. A higher class never waits behind a
/// lower one; a lower one still runs, in the gaps, and ages upward so it is never starved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Qos {
    /// Work nobody waits on now: dreams, training, consolidation.
    Background = 0,
    /// Work that fills what the classes above leave: ambient thought, perception, exams.
    Utility = 1,
    /// An owner's own work in progress: a resident's act on her card.
    Work = 2,
    /// Someone is waiting on the answer: a directed reply.
    Interactive = 3,
}

impl Qos {
    /// Every class, highest first.
    pub const ALL: [Qos; 4] = [Qos::Interactive, Qos::Work, Qos::Utility, Qos::Background];

    pub(crate) fn rank(self) -> u32 {
        self as u32
    }
}
