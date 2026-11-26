//! Who a stage is, which instance of it is running, and which run produced a result.

use uuid::Uuid;

/// A brain function's identity, stable across restarts and nodes: derived from its name, which
/// is kept beside it as an attribute, never parsed from it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StageId(Uuid);

impl StageId {
    /// The one derivation: the same name yields the same id everywhere.
    pub fn named(name: &str) -> Self {
        Self(Uuid::new_v5(
            &Uuid::NAMESPACE_OID,
            format!("continuum.stage.{name}").as_bytes(),
        ))
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

/// The mind a stage serves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MindId(pub Uuid);

/// One running instance of a stage for one mind. A new incarnation (restart, migration,
/// re-host) adopts the previous one's open obligations with a durable receipt and fences its
/// mutations; `epoch` orders incarnations of the same (stage, mind).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StageIncarnation {
    pub stage: StageId,
    pub mind: MindId,
    pub epoch: u64,
}

impl StageIncarnation {
    /// Whether `other` is a later incarnation of the same stage for the same mind: the one rule
    /// that fences a stale incarnation's mutations.
    pub fn is_superseded_by(&self, other: &StageIncarnation) -> bool {
        self.stage == other.stage && self.mind == other.mind && other.epoch > self.epoch
    }
}

/// One run within an incarnation: what a dispatch, a stop receipt and an output are tied to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RunId {
    pub incarnation: StageIncarnation,
    pub seq: u64,
}
