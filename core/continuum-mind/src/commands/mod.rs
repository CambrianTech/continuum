//! The mind's commands (cut out of continuum-core, plan phase 2): `cognition/*` (the commands that
//! hold the persona module's state, beside the stateless ones continuum-cognition owns),
//! `persona/*`, and `focus/*` and `gym/*`. Core re-exports these under their old paths.

pub mod cognition;
pub mod focus;
pub mod gym;
pub mod persona;
