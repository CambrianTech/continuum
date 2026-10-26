//! Store (MODULE-ARCHITECTURE.md section 9.2): the data layer and its commands, cut out of
//! continuum-core (card 2709ec6d). The kernel is re-exported so the moved code's `crate::orm`,
//! `crate::runtime` and logging macros resolve as they did in core.

pub use continuum_kernel::*;

pub mod data;
pub mod module;

pub use module::{DataModule, SubstrateEntities};

/// The commands `DataModule` builds: data/*, vector/*, adapter/* and migration/*.
pub mod commands {
    pub mod adapter;
    pub mod data;
    pub mod migration;
    pub mod vector;
}
