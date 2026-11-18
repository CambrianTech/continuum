//! llama.cpp backend — wraps our owned `llama` crate.
//!
//! The `llama` crate vendors llama.cpp source and builds it via cmake with
//! platform-specific features (metal/cuda). This backend is the adapter
//! between Continuum's TextGenerationRequest pipeline and the safe Rust API.
//!
//! Architecture: ONE `Context` driven by the persona scheduler (see `persona_scheduler`): one mind
//! decodes at a time, each persona keeps her blocks between turns, and a turn prefills only what her
//! blocks do not hold. A switch is the persona engine's native bind. Measured on the M5: four minds
//! decoding at once ran 2 tok/s each; one at a time runs at the full rate.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use llama::{FlashAttn, KvCacheType, LoraAdapter, Model, ModelParams};

use super::persona_scheduler::{GenerationRequest, Scheduler, SchedulerConfig, TokenEvent};
use super::SamplingConfig;
use crate::runtime;

mod validate_embedding_model;
pub use validate_embedding_model::*;
mod llama_cpp_backend;
mod embedding_lane;
mod embedding_batch;
pub(crate) use embedding_batch::{resident_weights, ContextBytes, EmbedWindow, EmbeddingBatch, Placement, NO_TRAINED_CONTEXT};

#[cfg(test)]
mod tests;
