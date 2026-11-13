//! The floor of the core: leaf utilities that depend on nothing else in Continuum, so every
//! crate above can depend on them without a cycle. Split out of `continuum-core` (card
//! a31492cb) so the monolith can be cut along downward layers.
#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]
#![deny(clippy::await_holding_lock, clippy::significant_drop_in_scrutinee, clippy::let_underscore_must_use)] // clean on 2026-11-08 (#5858 CI); CI's clippy step enforces it

pub mod audio_constants;
pub mod config_env;
pub mod population;
pub mod home;
pub mod id_resolve;
pub mod non_empty_text;
pub mod probe;
pub mod probed;
pub mod repo_id;
pub mod reserved_rooms;
pub mod secrets;
pub mod shell_portable;
pub mod shown_text;
pub mod time;
pub mod token_estimate;
pub mod tool_disclosure;
pub mod tool_effect;
