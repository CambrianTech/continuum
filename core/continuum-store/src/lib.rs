//! Store (MODULE-ARCHITECTURE.md section 9.2): the data layer and its commands, cut out of
//! continuum-core (card 2709ec6d). The kernel is re-exported so the moved code's `crate::orm`,
//! `crate::runtime` and logging macros resolve as they did in core.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![deny(clippy::await_holding_lock, clippy::significant_drop_in_scrutinee, clippy::let_underscore_must_use)] // clean on 2026-11-08 (#5858 CI); CI's clippy step enforces it

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
