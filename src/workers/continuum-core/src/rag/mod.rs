//! RAG (Retrieval-Augmented Generation) Engine
//!
//! Parallel source loading, budget allocation, context composition.
//! This is the brain's memory retrieval system.
//!
//! Design: All sources load in PARALLEL via rayon, not serial.
//! Target: <500ms total composition time (currently 20+ seconds in TypeScript)

pub mod engine;
pub mod types;
pub mod sources;
pub mod budget;

pub use engine::RagEngine;
pub use types::*;
