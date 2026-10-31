//! Shared Cognition — the objective-analysis + specialty-render split.
//!
//! Native-truth Rust core for the shared-cognition pipeline. The
//! TypeScript layer is a thin wrapper (IPC mixin + generated command
//! scaffolds + auto-generated types via ts-rs). All logic — analysis
//! pipeline, response orchestration, lever evaluation — lives here.
//!
//! Architecture: see `docs/architecture/SHARED-COGNITION.md`. Thesis:
//! today each persona independently rebuilds the objective picture
//! (what the message means, what RAG matters) before contributing
//! their specialty slice. Splitting into one shared analysis (cheap,
//! once per message) + N short specialty renders (one per persona)
//! drops the duplicate work without losing the distinct perspectives.
//!
//! Why Rust: SIMD scoring, true concurrency for parallel responder
//! evaluation, kernel-level memory rules for the cache. None of this
//! is expressible in TS without hand-waving.
//!
//! Same module-shape pattern as `rag/`:
//!   - `mod.rs`     — module surface
//!   - `types.rs`   — Rust source-of-truth types, ts-rs auto-emit to
//!                    `protocol/typescript/cognition/` (TS gets the schema
//!                    for free; nobody hand-writes TS types for these)
//!   - `shared_analysis.rs`     — analysis pipeline (the verb that
//!                                produces `SharedAnalysis`)
//!   - `response_orchestrator.rs` — per-persona relevance scoring +
//!                                  decision (the verb that produces
//!                                  `ResponderDecision`)

pub mod conversation;
pub mod extensions;
pub mod admission;
pub mod admission_persistence;
pub mod admission_state;
pub mod attention;
pub use continuum_engram::engram;
pub use continuum_engram::engram_graph;
pub use continuum_engram::home;
pub mod inbox_admission;
pub use continuum_rooms::mind_room;
pub use continuum_grounding::cached_source;
pub use continuum_engram::recall_metadata;
pub use continuum_engram::trace;
pub use continuum_rooms::card_facts;
pub use continuum_rooms::card_holder;
pub use continuum_grounding::grounding_invalidation;
pub use continuum_rooms::room_substrates;
pub use continuum_prompt::framing_echo;
pub use continuum_prompt::not_speech;
pub mod act_observe;
pub use continuum_vitals::directed_pending;
pub use continuum_vitals::grid_allocation;
pub mod channel_digest;
pub mod channel_digest_region;
pub mod channel_element;
pub mod channel_substrate;
pub mod check_redundancy;
pub use continuum_prompt::context_budget;
pub mod deferred_faculty;
pub mod deliberation_budget;
pub mod deliberation_parse;
pub mod deliberation_prompt;
pub mod dispatch_listener;
pub use continuum_vitals::activity_gate;
pub use continuum_vitals::turn_ingress;
pub mod faculty_pulse;
pub use continuum_vitals::vitals_pulse;
pub use continuum_engram::focus_policy;
pub mod generate_recipe;
pub mod generate_response;
pub(crate) mod generation_drop;
pub mod inference_session;
pub mod introspect_commands;
pub mod llm_deliberation_faculty;
pub(crate) mod context_streams;
pub mod tool_surface;
pub mod tool_head;
pub mod model_binding;
pub mod model_resolver;
/// Paging cursors over large data she is working through, kept in working memory.
pub mod open_views;
pub mod parroted_perception;
pub use continuum_vitals::pass_intent;
pub mod perception_facts;
pub mod persona_tools;
pub mod persona_workspace;
pub use continuum_rooms::handoff;
pub use continuum_rooms::bounded_room_ledger;
pub use continuum_prompt::prompt_prefix;
pub use continuum_capture::prompt_capture;
pub mod rag_source_faculty;
pub mod rate_proposals;
pub mod recall_faculty;
pub mod render_receipt;
pub use continuum_prompt::audience;
pub use continuum_engram::recall_ranker;
pub mod replay;
pub use continuum_rooms::room_roster;
pub use continuum_vitals::turn_ledger;
pub mod response_orchestrator;
pub use continuum_prompt::response_validator;
pub mod self_repeat;
pub mod shared_analysis;
pub mod should_respond;
pub mod should_respond_module;
pub use continuum_prompt::token_budget;
pub mod tool_embedding;
pub mod tool_executor;
pub mod turn_batch;
pub mod types;
pub mod validate_response;
pub mod vision_describe;
pub mod working_memory;
pub mod working_set;
pub mod workspace;
pub mod workspace_capture;
// Leaf modules peeled down from persona (they name no persona module): the capture
// fixture policy, the faculty ids persona's sources deliver under, the one budget
// allocator, text analysis, prompt assembly, redaction, the health ledger, presence
// glyphs, mind events, the room directory and the focus registry.
pub use continuum_capture::capture;
pub mod faculty_ids;
pub use continuum_vitals::health_ledger;
pub use continuum_vitals::mind_events;
pub use continuum_engram::focus;
pub use continuum_rooms::presence_glyph;
pub use continuum_prompt::prompt_assembly;
pub use continuum_grounding::rag_budget;
pub use continuum_engram::redaction;
pub mod resident_service;
pub use continuum_rooms::room_directory;

pub use model_resolver::*;
pub use response_orchestrator::{
    orchestrate, score_persona, PersonaSlot, DEFAULT_RELEVANCE_THRESHOLD,
};
pub use response_validator::{clean_and_validate, is_hard_failure, ValidationOutcome};
pub use shared_analysis::{analyze, AnalysisInput, RecentMessage};
pub use should_respond::*;
pub use tool_executor::{
    MediaItemLite, NativeBatchOutcome, ParsedToolBatch, PersonaMediaConfigLite,
    ToolExecutionContext, ToolExecutor, ToolInvocation, ToolOutcome,
};
pub use turn_batch::*;
pub use types::*;
