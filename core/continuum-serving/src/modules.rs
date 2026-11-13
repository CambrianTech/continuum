//! The serving modules under the paths they had in continuum-core (`crate::modules::serving_*`),
//! so a move changes no import inside them.

pub use continuum_engine::{serving_pin_store, serving_tier_down};

pub mod serving_consumer;
pub mod serving_daemon;
