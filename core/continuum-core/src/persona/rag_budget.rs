//! RagBudgetManager — flexbox-style token allocation across RAG
//! sources, with the no-clipping doctrine baked in.
//!
//! ### What this module solves
//!
//! Every LLM has a different context window — local Qwen 1.7B at 4k,
//! Qwen 3-30B at 128k, Claude Sonnet at 200k, future models at 1M+.
//! Plus per-channel constraints (video real-time is bandwidth-bound,
//! coding sessions can afford bigger working sets) and per-LoRA-stack
//! overhead. The L1 RAG working memory has to share that budget
//! across multiple content sources (recent conversation, salience-
//! scored engrams, code context, tool descriptions, …) WITHOUT
//! truncating anyone mid-content. Clipping breaks HTML, code, JSON,
//! mid-sentence semantics — it's never acceptable.
//!
//! Per `RAGBudgetManager.ts` (the production TS prior art) +
//! `docs/architecture/COGNITION-CACHE-HIERARCHY.md` (the L1 budget
//! math + recent-universal floor doctrine), this module implements
//! a CSS-flexbox-inspired allocator that gives each source a token
//! budget; sources are responsible for delivering COMPLETE atomic
//! units within that budget.
//!
//! ### Doctrine — no clipping
//!
//! When budget is tight, sources are dropped WHOLE in priority
//! order (required=false first). A source that can't satisfy its
//! `floor_tokens` (the unconditional minimum) returns
//! `AllocationState::UnderProvisioned` and the caller escalates —
//! the substrate never silently clips content mid-unit.
//!
//! The source-owned-unit model means each source decides what
//! counts as "complete":
//! - `ConversationSource`: one message
//! - `EngramSource`: one engram
//! - `CodeSource`: one function / one snippet
//! - `ToolSource`: one tool description
//! The allocator never knows what a "complete unit" looks like —
//! it only deals in token counts.
//!
//! ### Doctrine — sources own state
//!
//! Joel, 2026-05-31: "And to maintain state if necessary."
//!
//! Implementations use interior mutability (DashMap, Mutex, atomics)
//! to hold per-source state — cursor positions, recently-served
//! sets, computation caches, telemetry. The `RagSource::deliver`
//! method takes `&self`; state lives inside via the same pattern
//! `PersonaAircRuntimeRegistry`, `RecallMetadataRegistry`, etc.
//! already use across the substrate.
//!
//! ### Variability is intrinsic
//!
//! Context window sizes vary by 250×. Allocation must scale
//! continuously (no `if context > 32k` branches inside the
//! algorithm). The `RagBudgetAdapter` trait + per-profile presets
//! handle the variability cleanly; the math doesn't care.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

//=============================================================================
// CONTEXT — Android-style first-parameter pattern
//=============================================================================

mod substrate_context;
pub use substrate_context::*;
mod flex_allocate;
pub use flex_allocate::*;

//=============================================================================
// TESTS
//=============================================================================
#[cfg(test)]
mod tests;
