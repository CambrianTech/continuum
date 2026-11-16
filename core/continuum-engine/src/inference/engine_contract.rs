//! The core's edge of the core<->engine contract (CONTINUUM-ENGINE 4.11; the definition is the fork's
//! `server-continuum.h`, bound in `llama::contract`). Ids are their native 16 bytes in distinct types; a frame's
//! kind and tags are validated here exactly as the engine validates them (`validate`), and the cross-language test
//! (`tests.rs`) runs both validators over the same bytes so the two edges cannot drift.

mod ids;
mod validate;
#[cfg(test)]
mod tests;

pub use ids::{ActId, EngineInstanceId, LayerId};
pub use validate::{validate, FrameError, FrameKind};
