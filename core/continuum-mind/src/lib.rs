//! continuum-mind: cut out of continuum-core (MICROKERNEL-DECOMPOSITION.md): cognition,
//! persona, experience, recipe, context and vdd. Depends on the live crate (which carries the
//! engine and the kernel), the hands, and the crates cut out of the mind: perception
//! (perception and rag), genome and memory, re-exported here under their old paths; core
//! re-exports these modules under their old names.

pub use continuum_hands::code;
pub use continuum_live::*;
pub use continuum_genome::genome;
pub use continuum_memory::memory;
pub use continuum_perception::{perception, rag};

pub mod cognition;
pub mod context;
pub mod experience;
pub mod persona;
pub mod recipe;
pub mod vdd;
