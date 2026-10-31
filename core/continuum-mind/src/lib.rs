//! continuum-mind: cut out of continuum-core (MICROKERNEL-DECOMPOSITION.md): persona,
//! context and vdd, over cognition (its own crate, re-exported here under its old path). Depends on the live crate (which carries the engine and the
//! kernel), the hands, and the crates cut out of the mind: perception (perception and rag),
//! genome, memory and experience (experience and recipe), re-exported here under their old
//! paths; core re-exports these modules under their old names.

pub use continuum_hands::code;
pub use continuum_live::*;
pub use continuum_genome::genome;
pub use continuum_memory::memory;
pub use continuum_experience::{experience, recipe};
pub use continuum_perception::{perception, rag};

pub use continuum_academy::cognition;
pub mod activity;
pub mod chat;
pub mod commands;
pub mod context;
pub mod modules;
pub mod persona;
pub mod room;
pub mod vdd;
pub mod work;
