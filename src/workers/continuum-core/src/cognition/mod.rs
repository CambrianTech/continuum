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
//!                    `shared/generated/cognition/` (TS gets the schema
//!                    for free; nobody hand-writes TS types for these)
//!   - `shared_analysis.rs`     — analysis pipeline (the verb that
//!                                produces `SharedAnalysis`)
//!   - `response_orchestrator.rs` — per-persona relevance scoring +
//!                                  decision (the verb that produces
//!                                  `ResponderDecision`)

pub mod response_orchestrator;
pub mod response_validator;
pub mod shared_analysis;
pub mod types;

pub use response_orchestrator::{orchestrate, score_persona, PersonaSlot, DEFAULT_RELEVANCE_THRESHOLD};
pub use response_validator::{clean_and_validate, is_hard_failure, ValidationOutcome};
pub use shared_analysis::{analyze, AnalysisInput, RecentMessage};
pub use types::*;
