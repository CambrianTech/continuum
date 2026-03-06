//! Service Modules — ServiceModule implementations for each domain.
//!
//! Each module wraps existing domain logic behind the ServiceModule trait.
//! The runtime routes commands and events to the correct module automatically.
//!
//! Phase 1: health (trivial outlier — validates interface)
//! Phase 2: cognition, channel (per-persona DashMap — most different outlier)
//! Phase 3: voice, code, memory, models (remaining core domains)
//! Phase 4: data, embedding, inference, search, training, logger (absorb external workers)

pub mod agent;
pub mod ai_provider;
pub mod avatar;
pub mod channel;
pub mod code;
pub mod cognition;
pub mod data;
pub mod dataset;
pub mod embedding;
pub mod gpu;
pub mod health;
pub mod live;
pub mod logger;
pub mod mcp;
pub mod memory;
pub mod models;
pub mod rag;
pub mod runtime_control;
pub mod search;
pub mod sentinel;
pub mod system_resources;
pub mod tool_parsing;
