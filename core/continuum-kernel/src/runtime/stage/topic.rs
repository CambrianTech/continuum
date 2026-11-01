//! What a stage reads, and the provenance of what it writes.

use super::identity::RunId;

/// A topic's routing name. Routing is by this header, never by reading a payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TopicName(pub &'static str);

/// How a topic is read: every item of an event stream matters and is read through a cursor; only
/// the latest of a snapshot matters and is read by revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopicKind {
    ReplayableEvents,
    ReplaceableSnapshot,
}

/// The exact input a run saw: a topic and the cursor or revision it read through, captured at
/// dispatch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct InputRevision {
    pub topic: TopicName,
    pub through: u64,
}

/// Which run produced an output, from which inputs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Provenance {
    pub by: RunId,
    pub from: Vec<InputRevision>,
}
