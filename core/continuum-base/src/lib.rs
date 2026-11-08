//! The floor of the core: leaf utilities that depend on nothing else in Continuum, so every
//! crate above can depend on them without a cycle. Split out of `continuum-core` (card
//! a31492cb) so the monolith can be cut along downward layers.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod audio_constants;
pub mod config_env;
pub mod population;
pub mod home;
pub mod id_resolve;
pub mod probe;
pub mod repo_id;
pub mod secrets;
pub mod shell_portable;
pub mod time;
pub mod token_estimate;
pub mod tool_disclosure;
pub mod tool_effect;
