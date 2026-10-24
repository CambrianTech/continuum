//! continuum-cognition-verbs: cognition's stateless verbs (analysis, validation, response
//! orchestration, generation helpers) and prompt assembly, cut out of continuum-cognition so they
//! build alone. They name no other cognition module; continuum-cognition re-exports each under its
//! old `cognition::<name>` path. The tree keeps the `cognition/` directory so audited paths and
//! every `crate::cognition::` path inside the moved files stay unchanged, and the engine crate (with
//! the kernel) is re-exported at the root so every other `crate::` path resolves too. It never
//! links the live crate: vision_describe, the one verb that feeds media, stays in cognition.

pub use continuum_engine::*;

pub mod cognition;
