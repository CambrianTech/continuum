//! Persona Cognition Module
//!
//! Core persona intelligence in Rust:
//! - PersonaInbox: Priority queue for messages/tasks (flat, legacy)
//! - PersonaCognitionEngine: Fast decision making
//! - PersonaState: Energy, mood, attention tracking
//! - Evaluator: Unified pre-response gate (replaces 5 sequential TS gates)
//! - Channel system: Multi-channel queue with item polymorphism (replaces flat inbox)
//!   - channel_types: ActivityDomain enum + QueueItemBehavior trait
//!   - channel_items: Voice, Chat, Task concrete item structs
//!   - channel_queue: Generic per-domain queue container
//!   - channel_registry: Domain-to-queue routing + service_cycle()

pub mod admission;
pub mod admission_state;
pub mod airc_admission;
pub mod airc_runtime;
pub mod airc_runtime_registry;
pub mod allocator;
pub mod channel_items;
pub mod channel_queue;
pub mod channel_registry;
pub mod channel_types;
pub mod cognition;
pub mod cognition_io;
pub mod domain_classifier;
pub mod engram;
pub mod engram_graph;
pub mod evaluator;
pub mod genome_paging;
pub mod identity_provider;
pub mod inbox;
pub mod inbox_admission;
pub mod media_policy;
pub mod message_cache;
pub mod model_selection;
pub mod name_generator;
pub mod prompt_assembly;
pub mod recall_metadata;
pub mod recorder;
pub mod resource_forecast;
pub mod response;
pub mod resume_or_mint_provider;
pub mod seed;
pub mod self_task_generator;
pub mod service_module;
pub mod text_analysis;
pub mod trace;
pub mod turn_context;
pub mod turn_frame;
pub mod types;
pub mod unified;

pub use admission::{
    build_engram_from_candidate, AdmissionCandidate, AdmissionConfig, AdmissionContext,
    AdmissionGate, HeuristicIsMemorable, IsMemorable, SeenContentLookup, SeenEventLookup,
};
pub use admission_state::{AdmissionState, EngramOriginKind};
pub use airc_admission::{
    airc_envelope_to_candidate, airc_envelope_to_ref, AircAdmissionConversionError,
    AircAdmissionEnvelope,
};
pub use airc_runtime::{PersonaAircRuntime, PersonaAircRuntimeError};
pub use airc_runtime_registry::PersonaAircRuntimeRegistry;
pub use allocator::{
    allocate as allocate_personas, load_catalog, select_local_model, AllocationResult,
    PersonaAllocation, PersonaCatalogEntry,
};
pub use channel_items::{ChannelEnqueueRequest, MediaItemRequest};
pub use channel_registry::ChannelRegistry;
pub use channel_types::{ActivityDomain, ChannelRegistryStatus, ChannelStatus, ServiceCycleResult};
pub use cognition::{CognitionDecision, PersonaCognitionEngine, PriorityFactors, PriorityScore};
pub use domain_classifier::{DomainClassification, DomainClassifier, QualityFactors, QualityScore};
pub use engram::{
    AdmissionDecision, AdmissionDropReason, AdmissionError, AircMessageRef, ChatMessageRef, Engram,
    EngramKind, EngramOrigin, ToolInvocationRef, TrustState,
};
pub use evaluator::{
    AdequacyResult, FullEvaluateRequest, FullEvaluateResult, GateDetails, RateLimiterState,
    RecentResponse, SleepMode, SleepState,
};
pub use genome_paging::{
    ActivateSkillResult, CoverageReport, DomainActivity, GenomeAdapterInfo, GenomePagingEngine,
    GenomePagingState,
};
pub use inbox::{PersonaInbox, PersonaInboxFrame, PersonaInboxFrameMetrics};
pub use inbox_admission::{
    content_hash_sha256, inbox_message_to_candidate, inbox_message_to_origin, InboxAdmissionRunner,
    TrustMapping,
};
pub use message_cache::{
    CachedMessage, ContentDedupResult, ContentDeduplicator, EchoChamberResult, RecentMessageCache,
    SenderCategory,
};
pub use model_selection::{
    AdapterInfo, AdapterRegistry, ModelSelectionError, ModelSelectionRequest, ModelSelectionResult,
};
pub use name_generator::agent_name_from_identity;
pub use turn_context::TurnContext;
pub use turn_frame::{
    ConsolidatedInboxChunk, PersonaTurnFrame, PersonaTurnFrameReplayRecord, RagAssemblySeed,
    PERSONA_TURN_FRAME_REPLAY_SCHEMA_VERSION,
};
pub use types::*;
pub use unified::PersonaCognition;
