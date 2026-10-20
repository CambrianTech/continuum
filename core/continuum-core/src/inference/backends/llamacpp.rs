//! llama.cpp backend — wraps our owned `llama` crate.
//!
//! The `llama` crate vendors llama.cpp source and builds it via cmake with
//! platform-specific features (metal/cuda). This backend is the adapter
//! between Continuum's TextGenerationRequest pipeline and the safe Rust API.
//!
//! Architecture: ONE shared `Context` driven by a continuous-batching
//! scheduler (see `llamacpp_scheduler`). Each `generate()` call enqueues a
//! request that becomes one sequence (`seq_id`) in the shared batch.
//! Multiple sequences advance per decode step — weights are read once,
//! used for N sequences at once. This replaces the prior per-call-context
//! design where N concurrent generations created N independent contexts
//! that fought for memory bandwidth (each got ~1/N throughput).
//!
//! Measured 67.8 tok/s on M5 Metal with forged Qwen3.5 Q4_K_M (single seq
//! bench). With continuous batching across 3 seqs the per-stream cost
//! drops only ~10% rather than dividing by 3.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use llama::{FlashAttn, KvCacheType, LoraAdapter, Model, ModelParams};

use super::llamacpp_scheduler::{GenerationRequest, Scheduler, SchedulerConfig, TokenEvent};
use super::SamplingConfig;
use crate::runtime;

mod validate_embedding_model;
pub use validate_embedding_model::*;
mod llama_cpp_backend;
pub use llama_cpp_backend::*;

#[cfg(test)]
mod tests;
