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
pub mod airc_bridge_directive;
pub mod airc_bridge_dispatch;
// Disabled pending v5 owner-core fixture rewrite (continuum task #83).
// The whole `TestAircDaemon` was modeled on v4 wire shapes
// (Response::Event { event: Box<TranscriptEvent> }, ResolveWire,
// InboxResponse.events, PublishRequest.body) which no longer exist
// after the SHA bump in this PR. Rewriting the fixture requires
// adding airc-bus + airc-wire encode of synthetic envelopes — same
// substrate the daemon itself uses. Tracked separately so the
// production v5 migration can ship without that scope.
// #[cfg(test)]
// mod airc_runtime_e2e_tests;
pub mod auth;
pub mod avatar;
pub mod bevy_consumer;
pub mod channel;
pub mod chat;
pub mod code;
pub mod code_commands;
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
pub mod genome;
pub mod genome_fitness_sentinel;
pub mod gpu;
pub mod grant_issuance;
pub mod grid;
pub mod grid_capacity;
pub mod health;
pub mod hippocampus;
pub mod inference_coordinator_module;
pub mod launch_mode;
pub mod live;
pub mod live_session_consumer;
pub mod logger;
pub mod mcp;
pub mod mcp_protocol;
pub mod mcp_transport;
pub mod memory;
pub mod nav;
pub mod models;
pub mod perception_consumer;
pub mod persona_allocator;
pub mod persona_instance_manager;
pub mod persona_rag_inspect;
pub mod persona_rag_inspect_filesystem;
pub mod plasticity;
pub mod pressure_broker_module;
pub mod probe_stream;
pub mod python_adapter;
pub mod rag;
pub mod resource_broker;
pub mod resources_module;
pub mod runtime_control;
pub mod sentinel;
pub mod serving_consumer;
pub mod serving_daemon;
pub mod serving_tier_down;
pub mod system_resources;
pub mod tool_parsing;
pub mod training_completion_sentinel;
pub mod training_trigger;
pub mod vdd;
pub mod vision;
pub mod work;
