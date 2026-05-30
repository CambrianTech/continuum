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
pub mod airc;
#[cfg(test)]
mod airc_runtime_e2e_tests;
pub mod auth;
pub mod avatar;
pub mod channel;
pub mod chat;
pub mod code;
pub mod cognition;
pub mod data;
pub mod dataset;
pub mod docker_tier;
pub mod docker_tier_pool;
pub mod embedding;
pub mod entity_schemas;
pub mod events;
pub mod forge;
pub mod generator;
pub mod gpu;
pub mod grid;
pub mod health;
pub mod hippocampus;
pub mod inference;
pub mod live;
pub mod logger;
pub mod mcp;
pub mod memory;
pub mod models;
pub mod persona_allocator;
pub mod plasticity;
pub mod pressure_broker_module;
pub mod python_adapter;
pub mod rag;
pub mod resource_broker;
pub mod runtime_control;
pub mod search;
pub mod sentinel;
pub mod system_resources;
pub mod tool_parsing;
pub mod vdd;
pub mod vision;
