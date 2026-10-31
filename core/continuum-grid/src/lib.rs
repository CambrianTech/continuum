//! continuum-grid: cut out of continuum-core (MODULE-ARCHITECTURE.md section 9.2, "Grid:
//! discovery, trust, routing, on every node"): the grid module (nodes, transports, router,
//! trust policy, interceptor, placement), the grid allocator daemon and the grid commands.
//! Depends on the engine, the kernel and the mind; core re-exports these modules under
//! their old names.

pub use continuum_engine::*;
pub use continuum_mind::{cognition, experience, persona};

pub mod forge;
pub mod grid;
pub mod grid_allocator;
