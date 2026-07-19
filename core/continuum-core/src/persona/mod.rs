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
pub mod admission_persistence;
pub mod admission_state;
pub mod airc_admission;
pub mod airc_citizen;
pub mod airc_persona_conversation;
pub mod airc_runtime;
pub mod command_inbound_pump;
// `scripted_*` are SYSTEM-level test/replay primitives per
// [[test-fixtures-are-system-primitives]] — ubiquitous across every
// test in the substrate, never bespoke per module. They're gated to
// the SAME cfg as `HeuristicInferenceAdapter` they depend on, because
// Joel (2026-06-01): "You mix this fake shit in and it's going live
// ALL THE TIME. The fake shit is a CHOSEN model adapter no other
// form. Declaration." cfg gating IS the declaration.
#[cfg(any(test, feature = "test-fixtures"))]
pub mod scripted_adapter_factory;
#[cfg(any(test, feature = "test-fixtures"))]
pub mod scripted_conversation;
pub mod airc_runtime_registry;
pub mod airc_source;
pub mod allocator;
pub mod card;
pub mod channel_items;
pub mod channel_queue;
pub mod channel_registry;
pub mod channel_types;
pub mod channel_view;
pub mod cognition;
pub mod cognition_io;
pub mod decay_tick;
pub mod domain_classifier;
pub mod engram;
pub mod engram_graph;
pub mod engram_source;
pub mod evaluator;
pub mod focus;
pub mod genome_paging;
pub mod home;
pub mod host;
pub mod hw_tier_descriptor;
pub mod identity_provider;
pub mod persona_identity;
pub mod inbox;
pub mod inference_profile;
pub mod loop_dedup;
pub mod model_override;
pub mod portability;
pub mod profile_builder;
pub mod service_loop;
pub mod spawner;
pub mod spawner_module;
pub mod supervisor;
pub mod training_producer;
pub mod inbox_admission;
pub mod media_policy;
pub mod message_cache;
pub mod model_selection;
pub mod name_generator;
pub mod projection;
pub mod prompt_assembly;
pub mod rag_budget;
pub mod rag_capture;
pub mod rag_inspect;
pub mod rag_replay;
pub mod recall_metadata;
pub mod redaction;
pub mod active_work_source;
pub mod recorder;
pub mod resource_forecast;
pub mod response;
pub mod media_perception_source;
pub mod room_board_source;
pub mod room_doctrine_source;
pub mod room_roster_source;
pub mod resume_or_mint_provider;
pub mod role_template;
pub mod seed;
pub mod self_task_generator;
pub mod service_module;
pub mod text_analysis;
pub mod trace;
pub mod turn_context;
pub mod wall_source;
pub mod workspace_map_source;
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
pub use model_override::{PersonaModelOverride, PersonaModelOverrideError};
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
    analyze_burst, AdequacyResult, BurstEvaluateResult, BurstRespondContext, FullEvaluateRequest,
    FullEvaluateResult, GateDetails, RateLimiterState, RecentResponse, SleepMode, SleepState,
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

// ── Substrate ORM entity registration ────────────────────────────
//
// Rust-native authoring path per [[orm-everything-not-hand-edited-
// files]] and [[authored-data-vs-procedural-projection]] — substrate
// entities (hw tiers, role templates, identity pools, universes,
// future continuum config) get their schemas from this side; the
// TS-decorator pipeline stays for user-app entities.
//
// Headless requirement (Joel, 2026-06-01): substrate must work with
// no Node runtime present. Rust-native authoring is the only valid
// path for substrate data — TS-decorator pipeline isn't reachable in
// headless mode.
//
// Call this once during continuum-core boot, BEFORE the first
// `data/ensure-schema` for any of these collections fires. Boot wires
// it as `register_substrate_orm_entities(OrmEntityRegistry::global())`.
// The parameter is for testability — tests construct fresh registries
// to avoid singleton races under parallel cargo test runs.

/// Register the persona substrate's Rust-authored ORM entities into
/// the supplied registry. Idempotent — repeat calls with the same
/// schemas are no-ops. Conflicts with a previously registered
/// different shape return `Err`.
///
/// Production boot:
///   `register_substrate_orm_entities(OrmEntityRegistry::global())?;`
pub fn register_substrate_orm_entities(
    registry: &crate::orm::OrmEntityRegistry,
) -> Result<(), crate::orm::RegistrationError> {
    registry.register::<hw_tier_descriptor::HwTierDescriptor>()?;
    registry.register::<role_template::RoleTemplate>()?;
    Ok(())
}

#[cfg(test)]
mod orm_entity_registration_tests {
    use super::*;

    /// Boot-order proof: after `register_substrate_orm_entities`, both
    /// substrate collections resolve via the Rust path. This is the
    /// slice-1 acceptance test for #123.
    #[test]
    fn substrate_entities_register_and_resolve() {
        let registry = crate::orm::OrmEntityRegistry::new();
        register_substrate_orm_entities(&registry).expect("register substrate entities");

        let hw_tiers = registry
            .resolve("hw_tiers")
            .expect("hw_tiers resolves via Rust registry");
        assert_eq!(hw_tiers.collection, "hw_tiers");
        assert!(
            hw_tiers.fields.iter().any(|f| f.name == "id" && f.unique),
            "hw_tiers must have a unique `id` field"
        );

        let role_templates = registry
            .resolve("role_templates")
            .expect("role_templates resolves via Rust registry");
        assert_eq!(role_templates.collection, "role_templates");
        assert!(
            role_templates
                .fields
                .iter()
                .any(|f| f.name == "role" && f.unique),
            "role_templates must have a unique `role` field"
        );

        // BaseEntity contract — every Rust-authored entity carries id +
        // timestamps + version. This is the "adhering to some base"
        // requirement Joel called out 2026-06-01. If a future entity
        // forgets to call `base_entity_fields()`, this test catches it.
        for collection in [&hw_tiers, &role_templates] {
            let names: Vec<&str> = collection.fields.iter().map(|f| f.name.as_str()).collect();
            for base in ["id", "createdAt", "updatedAt", "version"] {
                assert!(
                    names.contains(&base),
                    "collection {} missing BaseEntity field '{}' — got {:?}",
                    collection.collection,
                    base,
                    names
                );
            }
        }
    }

    /// Idempotence: calling twice is safe. Load-bearing because boot
    /// order across modules can cause double-registration.
    #[test]
    fn registration_is_idempotent() {
        let registry = crate::orm::OrmEntityRegistry::new();
        register_substrate_orm_entities(&registry).expect("first call");
        register_substrate_orm_entities(&registry).expect("second call is no-op");
        register_substrate_orm_entities(&registry).expect("third call still no-op");

        assert!(registry.resolve("hw_tiers").is_some());
        assert!(registry.resolve("role_templates").is_some());
    }
}
