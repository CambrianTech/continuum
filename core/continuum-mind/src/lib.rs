//! continuum-mind: cut out of continuum-core (MICROKERNEL-DECOMPOSITION.md): cognition,
//! persona, genome, memory, experience, recipe, context and vdd. Depends on the live crate
//! (which carries the engine and the kernel), the hands and the perception crate (perception
//! and rag, re-exported here under their old paths); core re-exports these modules under
//! their old names.

pub use continuum_hands::code;
pub use continuum_live::*;
pub use continuum_perception::{perception, rag};

pub mod cognition;
pub mod context;
pub mod experience;
pub mod genome;
pub mod memory;
pub mod persona;
pub mod recipe;
pub mod vdd;
