//! The core <-> engine training seam, typed once.
//!
//! The engine is our llama.cpp fork's server (`tools/server/server-train.cpp`,
//! `server-context.cpp`). Its requests and replies were serde types private to
//! `continuum-genome` plus two hand-built `json!` bodies, so the fork had nothing to check its
//! side against. These types are the one definition: the core sends and parses them, and
//! [`schema`] writes the JSON Schema the fork's tests validate against (Joel, 2026-11-01:
//! "better APIs and schemas across boundaries"). When serving drives libllama in-process
//! (SERVING-ENGINE.md), the same types become the function signatures.

mod error;
mod learn;
mod measure;
mod train;

pub use error::{EngineError, EngineErrorBody, ErrorType};
pub use learn::{LearnOutcome, LearnReply, LearnRequest};
pub use measure::{ExampleFit, MeasureReply, MeasureRequest, MeasuredExample};
pub use train::{EngineExample, EpochReport, Fit, JobRef, TrainRequest, TrainState, TrainStatus};

pub mod schema;
