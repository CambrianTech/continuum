//! continuum-dreaming: how a mind learns (eval, coursework, curriculum, dream consolidation,
//! experience) and the benchmark harness, cut out of continuum-cognition and placed ABOVE it
//! (plan line 1). Cognition's core names none of these modules; the verbs it needs from them
//! arrive through `cognition::extensions`, which this crate's boot fills. Everything cognition
//! exports is re-exported here under its old path, so `crate::cognition::<any>` resolves
//! inside the moved files; continuum-mind re-exports this crate's `cognition`.

pub use continuum_cognition::*;

pub mod cognition;
