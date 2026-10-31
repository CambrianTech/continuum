//! Service Modules — ServiceModule implementations for each domain.
//!
//! Each module wraps existing domain logic behind the ServiceModule trait.
//! The runtime routes commands and events to the correct module automatically.
//!
//! Phase 1: health (trivial outlier — validates interface)
//! Phase 2: cognition, channel (per-persona DashMap — most different outlier)
//! Phase 3: voice, code, memory, models (remaining core domains)
//! Phase 4: data, embedding, inference, search, training, logger (absorb external workers)

pub mod acting_provider;
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
pub mod ext_commands;
pub mod auth;
pub mod benchmark_grade;
pub mod citizen_health;
pub(crate) mod deploy_actuator;
pub mod deploy_tracker;
pub mod deploy_candidate;
pub mod floor_fetch;
pub mod card_carry;
pub mod channel;
pub use continuum_mind::chat;
pub use continuum_hands::modules::{code, code_commands, python_adapter};
pub use continuum_mind::modules::cognition;
/// The data layer (continuum-store, card 2709ec6d), at its old path: the store's state and
/// helpers, and the module shell that the store keeps above them.
pub mod data {
    pub use continuum_store::data::*;
    pub use continuum_store::module::{DataModule, SubstrateEntities};
}
/// Core's half of the store's boot seam: the substrate entities the layers above the store own.
pub mod data_wiring;
pub use continuum_dreaming::modules::dataset;
pub use continuum_embedding::module as embedding;
pub mod events;
pub mod forge;
pub mod generator;
pub use continuum_dreaming::modules::gene_examiner;
pub use continuum_dreaming::modules::genome;
pub use continuum_dreaming::modules::genome_fitness_sentinel;
pub use continuum_kernel::gpu::module as gpu;
pub mod grant_issuance;
pub use continuum_grid::grid;
pub mod grid_capacity;
pub use continuum_grid::grid_allocator;
pub mod health;
pub mod hippocampus;
pub mod inference_coordinator_module;
pub mod launch_mode;
pub mod live;
pub mod voice_reply_speaker;
pub mod logger;
pub mod logger_clog_sink;
pub mod mcp;
pub mod mcp_protocol;
pub mod mcp_transport;
pub mod memory;
pub use continuum_models::module as models;
pub mod nav;
pub mod persona_allocator;
pub mod persona_instance_manager;
pub use continuum_mind::modules::persona_rag_inspect;
pub mod persona_rag_inspect_filesystem;
pub mod ports;
pub mod pressure_broker_module;
pub mod rag;
pub mod recipe;
pub mod resources_module;
pub use continuum_mind::room;
pub mod runtime_control;
pub use continuum_engine::{plasticity, serving_pin_store, serving_tier_down};
pub use continuum_serving::modules::{serving_consumer, serving_daemon};
pub mod system_resources;
pub mod training_completion_sentinel;
pub use continuum_dreaming::modules::training_trigger;
pub use continuum_mind::activity;
pub use continuum_mind::vdd::module as vdd;
pub mod vision;
pub mod verdict_board;
pub use continuum_mind::work;
pub use continuum_mind::work::card_staging;
pub mod work_seams;

/// Realistic 50-persona load/profiling harness (real CodeModule, real payloads, through a
/// mind's tool executor). Gated `stress-tests` per the test doctrine; profiling runs only.
#[cfg(all(test, feature = "stress-tests"))]
mod code_load_harness;
