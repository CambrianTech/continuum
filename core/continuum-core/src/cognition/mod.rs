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

pub mod act_observe;
pub mod adaptive_throughput;
pub mod audit;
pub mod bench_round;
pub mod bench_staging;
pub mod bench_task;
pub mod round_readiness;
pub mod activity;
pub mod benchmark;
pub mod benchmark_humaneval;
pub mod channel_digest;
pub mod channel_digest_region;
pub mod channel_element;
pub mod channel_substrate;
pub mod check_redundancy;
pub mod competitor;
pub mod context_budget;
pub mod deferred_faculty;
pub mod deliberation_budget;
pub mod deliberation_parse;
pub mod deliberation_prompt;
pub mod dispatch_listener;
pub mod dream_consolidation;
pub mod embedding;
pub mod eval;
pub mod exam_serving;
pub mod experience;
pub mod faculty_pulse;
pub mod focus_policy;
pub mod generate_recipe;
pub mod generate_response;
pub mod gym;
pub mod gym_grader;
pub mod host_capability_probe;
pub mod inference_session;
pub mod introspect_commands;
pub mod learning_policy;
pub mod llm_deliberation_faculty;
pub mod memory_consolidation_region;
pub mod model_resolver;
pub mod parroted_perception;
pub mod perception_facts;
pub mod persona_tools;
pub mod persona_workspace;
pub mod prefill_throttle;
pub mod prompt_capture;
pub mod rag_source_faculty;
pub mod rate_proposals;
pub mod recall_faculty;
pub mod recall_ranker;
pub mod replay;
pub mod resolution;
pub mod resolution_bench;
pub mod resolution_compute;
pub mod resource_admission;
pub mod response_orchestrator;
pub mod response_validator;
pub mod self_repeat;
pub mod serving_plan;
pub mod shared_analysis;
pub mod should_respond;
pub mod should_respond_module;
pub mod swe_bench;
pub mod swe_verdict_sweep;
pub mod threat_detector;
pub mod throughput_lease;
pub mod token_budget;
pub mod tool_dialect;
pub mod tool_embedding;
pub mod tool_executor;
pub mod tool_relevance;
pub mod tool_usage;
pub mod turn_batch;
pub mod types;
pub mod validate_response;
pub mod vision_describe;
pub mod will;
pub mod working_memory;
pub mod working_set;
pub mod workspace;
pub mod workspace_capture;
pub mod workspace_dashboard;

pub use adaptive_throughput::*;
pub use model_resolver::*;
pub use resource_admission::*;
pub use response_orchestrator::{
    orchestrate, score_persona, PersonaSlot, DEFAULT_RELEVANCE_THRESHOLD,
};
pub use response_validator::{clean_and_validate, is_hard_failure, ValidationOutcome};
pub use shared_analysis::{analyze, AnalysisInput, RecentMessage};
pub use should_respond::*;
pub use threat_detector::*;
pub use throughput_lease::*;
pub use tool_executor::{
    MediaItemLite, NativeBatchOutcome, ParsedToolBatch, PersonaMediaConfigLite,
    ToolExecutionContext, ToolExecutor, ToolInvocation, ToolOutcome,
};
pub use turn_batch::*;
pub use types::*;
