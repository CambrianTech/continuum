//! continuum-cognition: a mind's thinking, cut out of continuum-mind below persona
//! (MODULE-ARCHITECTURE.md section 9.2). It depends on the live crate (which carries the engine
//! and the kernel), the hands, and the crates cut out of the mind before it: perception (and
//! rag), genome, memory and experience (and recipe), re-exported here under their old paths so
//! every `crate::` path inside cognition resolves. continuum-mind re-exports `cognition`.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub use continuum_hands::code;
pub use continuum_live::*;
pub use continuum_genome::genome;
pub use continuum_memory::memory;
pub use continuum_experience::{experience, recipe};
pub use continuum_perception::perception;

pub mod cognition;
pub mod commands;
