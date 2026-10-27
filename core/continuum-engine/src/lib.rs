//! continuum-engine: cut out of continuum-core (MICROKERNEL-DECOMPOSITION.md). Depends only on the
//! kernel and crates below it; core re-exports these modules under their old names.

pub use continuum_kernel::*;

pub mod ai;
pub mod capacity;
pub mod inference;
pub mod inference_capability;
pub mod model_registry;
pub mod plasticity;
pub mod sensory;
pub mod serving_pin_store;
pub mod serving_tier_down;
