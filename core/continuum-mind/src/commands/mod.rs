//! The mind's commands (cut out of continuum-core, plan phase 2): `cognition/*` (the commands that
//! hold the persona module's state, beside the stateless ones continuum-cognition owns) and
//! `persona/*`. Core re-exports these under their old paths.

pub mod cognition;
pub mod persona;
