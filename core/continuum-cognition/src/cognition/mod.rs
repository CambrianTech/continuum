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
pub mod training_sources;
pub mod admission;
pub mod admission_persistence;
pub mod admission_state;
pub mod attention;
pub mod engram;
pub mod engram_graph;
pub mod home;
pub mod inbox_admission;
pub mod mind_room;
pub mod recall_metadata;
pub mod trace;
pub mod decay_tick;
pub mod cached_source;
pub mod card_facts;
pub mod card_holder;
pub mod grounding_invalidation;
pub mod room_substrates;
pub mod framing_echo;
pub mod not_speech;
pub mod act_observe;
pub mod directed_pending;
pub mod bench_round;
pub mod held_out;
pub mod grid_allocation;
pub mod measurement_staging;
pub mod bench_task;
pub mod activity;
pub mod benchmark;
pub mod benchmark_algotune;
pub mod benchmark_ds1000;
pub mod benchmark_mirrorcode;
pub mod benchmark_super;
pub mod benchmark_terminalbench;
pub mod benchmark_humaneval;
pub mod channel_digest;
pub mod channel_digest_region;
pub mod channel_element;
pub mod channel_substrate;
pub mod check_redundancy;
pub mod context_budget;
pub mod coursework;
pub mod deferred_faculty;
pub mod deliberation_budget;
pub mod deliberation_parse;
pub mod deliberation_prompt;
pub mod dispatch_listener;
pub mod activity_gate;
pub mod turn_ingress;
pub mod dream_consolidation;
pub mod curriculum;
pub mod eval;
pub mod exam_serving;
pub mod experience;
pub mod faculty_pulse;
pub mod vitals_pulse;
pub mod focus_policy;
pub mod generate_recipe;
pub mod generate_response;
pub(crate) mod generation_drop;
pub mod gym;
pub mod gym_rng;
pub mod inference_session;
pub mod introspect_commands;
pub mod learning_policy;
pub mod llm_deliberation_faculty;
pub(crate) mod context_streams;
pub mod tool_surface;
pub mod tool_head;
pub mod model_binding;
pub mod model_resolver;
/// Paging cursors over large data she is working through, kept in working memory.
pub mod open_views;
pub mod parroted_perception;
pub mod pass_intent;
pub mod perception_facts;
pub mod persona_tools;
pub mod persona_workspace;
/// The handoff record: her state at the seam, written before a stop, read first on wake
/// (card 49b5e806).
pub mod handoff;
/// Bounded per-room diagnostics state, with the eviction decision CLAUDE.md
/// requires of anything that accumulates (#3903 review).
pub mod bounded_room_ledger;
pub mod prompt_capture;
pub(crate) mod prompt_prefix;
pub mod rag_source_faculty;
pub mod rate_proposals;
pub mod recall_faculty;
pub mod recall_ranker;
pub mod audience;
pub mod replay;
pub mod room_roster;
pub mod turn_ledger;
pub mod response_orchestrator;
pub mod response_validator;
pub mod self_repeat;
pub mod shared_analysis;
pub mod should_respond;
pub mod should_respond_module;
pub mod swe_bench;
pub mod swe_verdict_sweep;
pub mod token_budget;
pub mod tool_embedding;
pub mod tool_executor;
pub mod turn_batch;
pub mod types;
pub mod validate_response;
pub mod vision_describe;
pub mod vision_gym;
pub mod working_memory;
pub mod working_set;
pub mod workspace;
pub mod workspace_capture;
// Leaf modules peeled down from persona (they name no persona module): the capture
// fixture policy, the faculty ids persona's sources deliver under, the one budget
// allocator, text analysis, prompt assembly, redaction, the health ledger, presence
// glyphs, mind events, the room directory and the focus registry.
pub mod capture;
pub mod faculty_ids;
pub mod focus;
pub mod health_ledger;
pub mod mind_events;
pub mod presence_glyph;
pub mod prompt_assembly;
pub mod rag_budget;
pub mod redaction;
pub mod room_directory;
pub mod text_analysis;

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
