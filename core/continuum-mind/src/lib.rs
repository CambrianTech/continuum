//! continuum-mind: cut out of continuum-core (MICROKERNEL-DECOMPOSITION.md): cognition,
//! persona, genome, memory, rag, perception, experience, recipe, context and vdd. Depends on
//! the live crate (which carries the engine and the kernel) and the hands; core re-exports
//! these modules under their old names.

pub use continuum_hands::code;
pub use continuum_live::*;

pub mod cognition;
pub mod context;
pub mod experience;
pub mod genome;
pub mod memory;
pub mod perception;
pub mod persona;
pub mod rag;
pub mod recipe;
pub mod vdd;
