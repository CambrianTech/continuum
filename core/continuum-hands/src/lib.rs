//! continuum-hands: cut out of continuum-core (MICROKERNEL-DECOMPOSITION.md). Depends only on the
//! engine, the kernel and crates below them; core re-exports these modules under their old names.

pub use continuum_engine::*;

pub mod code;
pub mod commands;
pub mod modules;

#[cfg(test)]
mod test_acting;
