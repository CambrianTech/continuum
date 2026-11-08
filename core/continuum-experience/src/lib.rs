//! continuum-experience: a room's typed manifest (experience) and the recipe it projects from,
//! with the executor that runs a recipe's pipeline, cut out of continuum-mind
//! (MODULE-ARCHITECTURE.md section 9.2). experience and recipe name each other, so they are one
//! crate. Depends only on the kernel and below; continuum-mind re-exports both under their old
//! paths.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub use continuum_kernel::*;

pub mod experience;
pub mod recipe;
