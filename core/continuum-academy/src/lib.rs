//! continuum-academy: the whole learning system, everything a mind learns through: exams
//! (eval), coursework, curriculum, held-out tests, dream consolidation, lived experience,
//! benchmarks as measurement and academy sections a mind enters (not a runner), and the Matrix
//! Dojo (histories replayed as perception). Its target shape is independent of the mind and the
//! genome: perception and histories in, lessons, exams, verdicts and gene candidates out,
//! through typed seams. This first cut is a pure move out of continuum-cognition, placed above
//! it; its remaining imports of cognition's internals are the seams still to cut. Cognition's
//! core names none of these modules; the verbs it needs arrive through `cognition::extensions`.
//! Everything cognition exports is re-exported here under its old path, so
//! `crate::cognition::<any>` resolves inside the moved files; continuum-mind re-exports this
//! crate's `cognition`.

pub use continuum_cognition::*;

pub mod cognition;
pub mod hf;
