//! Who changes the engine's gene catalog (CONTINUUM-ENGINE 4.10 P4a).
//!
//! A gene entering or leaving the serving engine is a claim on the one memory budget, so it goes
//! through the genome's owner of loaded genes, never a bare HTTP call: a load is planned, leased into
//! custody, registered as a residency and loaded inside it; an unload is bound to the engine
//! incarnation that holds the adapter. That owner lives above this crate, so the engine declares the
//! seam and the genome installs itself at boot. The engine still decides WHAT changes (the target's
//! set against what it serves) and stages the files; the owner performs every mutation.
//!
//! The engine's identity is read BEFORE its catalog and carried unchanged into every step, so an
//! engine replaced between the read and a mutation (or an adapter id reused by a relaunch) refuses
//! instead of changing the wrong engine (Codex, the 03:27 adoption failure).

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use super::{LearningEngine, LedgerEngine};

/// One change the engine asks of the owner, every step bound to `ledger`.
pub struct CatalogChange {
    pub engine: LearningEngine,
    /// The engine's identity as read before its catalog.
    pub ledger: LedgerEngine,
    /// Genes the target dropped that the engine lists, by its adapter id.
    pub retire: Vec<Retire>,
    /// Genes the target added, already staged under the engine's train dir.
    pub add: Vec<Add>,
}

pub struct Retire {
    pub gene: String,
    pub id: i64,
}

pub struct Add {
    pub gene: String,
    /// The bare file name it is staged under (the engine loads only inside its train dir).
    pub name: String,
    pub staged: PathBuf,
}

/// One step the engine accepted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogStep {
    Retired(String),
    Added(String),
}

/// Why the owner stopped short of the target set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogRefusal {
    /// No owner is installed in this process: nothing may change the catalog.
    NoOwner,
    /// The engine serving now is not the one whose catalog was read.
    EngineReplaced,
    /// The memory a gene needs was not granted on this attempt; nothing is held for it.
    AwaitingMemory(String),
    /// The engine or the resource authority refused, in its own words.
    Refused(String),
}

impl std::fmt::Display for CatalogRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CatalogRefusal::NoOwner => write!(f, "no owner of the gene catalog is installed"),
            CatalogRefusal::EngineReplaced => write!(f, "the engine was replaced after its catalog was read"),
            CatalogRefusal::AwaitingMemory(why) => write!(f, "waiting for memory: {why}"),
            CatalogRefusal::Refused(why) => write!(f, "{why}"),
        }
    }
}

/// How a change ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CatalogEnd {
    /// The engine holds the target set.
    Done,
    /// A retiring gene is active in a turn in flight: stopped there, the rest waits a tick.
    Busy,
    Refused(CatalogRefusal),
}

/// The steps applied, in order, and how the change ended: the engine records every applied step
/// whatever the end, so its served set always names what the engine holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogOutcome {
    pub applied: Vec<CatalogStep>,
    pub end: CatalogEnd,
}

/// The one owner of gene catalog changes.
#[async_trait::async_trait]
pub trait GenomeCatalog: Send + Sync {
    async fn converge(&self, change: CatalogChange) -> CatalogOutcome;
}

static OWNER: OnceLock<Arc<dyn GenomeCatalog>> = OnceLock::new();

/// A second owner was offered; the first stays.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OwnerAlreadyInstalled;

/// Install the process's owner (the genome, at boot).
pub fn install(owner: Arc<dyn GenomeCatalog>) -> Result<(), OwnerAlreadyInstalled> {
    OWNER.set(owner).map_err(|_| OwnerAlreadyInstalled)
}

pub(super) fn owner() -> Option<&'static Arc<dyn GenomeCatalog>> {
    OWNER.get()
}
