//! continuum-grid: cut out of continuum-core (MODULE-ARCHITECTURE.md section 9.2, "Grid:
//! discovery, trust, routing, on every node"): the grid module (nodes, transports, router,
//! trust policy, interceptor, placement), the grid allocator daemon and the grid commands.
//! Depends on the engine, the kernel and the mind; core re-exports these modules under
//! their old names.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![deny(clippy::await_holding_lock, clippy::significant_drop_in_scrutinee)] // clean on 2026-11-08 (#5858 CI); CI's clippy step enforces it

pub use continuum_engine::*;
pub use continuum_mind::{cognition, experience, persona};

pub mod forge;
pub mod grid;
pub mod grid_allocator;
