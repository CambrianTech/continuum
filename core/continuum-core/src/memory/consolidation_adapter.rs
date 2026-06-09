//! STM→LTM consolidation strategy trait.
//!
//! Third 0.5.5 Hippocampus piece. Port of TS `MemoryConsolidationAdapter`
//! — the abstract "turn N working-memory thoughts into M long-term
//! memories" interface. Concrete strategies (pass-through raw,
//! LLM-based semantic compression, embedding generation) each
//! implement this trait independently.
//!
//! This file owns the TRAIT + input/output value types. Concrete impls
//! land in sibling files:
//!   - `raw_adapter.rs`         — one-to-one pass-through (Phase 1 baseline)
//!   - `semantic_adapter.rs`    — LLM synthesis of related thoughts (later)
//!   - `embedding_adapter.rs`   — vector embeddings alongside storage (later)
//!
//! Kept orthogonal to the `Consolidator` state container (threshold +
//! metrics + cadence gate) — the consolidator calls `adapter.consolidate
//! (thoughts, ctx)` when the cadence gate fires; which adapter is plugged
//! in is a persona configuration decision.

use async_trait::async_trait;
use uuid::Uuid;

/// Per-call context passed to every adapter invocation.
#[derive(Debug, Clone)]
pub struct ConsolidationContext {
    pub persona_id: Uuid,
    pub persona_name: String,
    pub session_id: Uuid,
    /// Unix milliseconds — "when this consolidation pass ran." Used
    /// for the `consolidated_at` field on emitted memories.
    pub timestamp_ms: u64,
}

/// A single raw working-memory entry considered for promotion. Minimal
/// shape — adapters that need more context either look it up on their
/// own or get it through the `context.persona_id`. Kept in-module
/// rather than pulling from corpus::* because the input type for this
/// boundary is orthogonal to the stored-memory shape.
#[derive(Debug, Clone)]
pub struct Thought {
    pub id: Uuid,
    /// "reflection" | "decision" | "pattern" | "observation" — free-form
    /// string rather than enum because the domains are discovered at
    /// runtime from the persona's thought stream; locking this to an
    /// enum would force the adapter to either fail on unknown types or
    /// add a catch-all, both of which are worse than passing through.
    pub thought_type: String,
    pub content: String,
    /// Domain tag — "chat" | "code" | "ui" | etc. Also free-form.
    pub domain: Option<String>,
    pub context_id: Option<Uuid>,
    /// 0.0–1.0 score. The consolidator's threshold filters this
    /// before the adapter sees the thought, but adapters can still
    /// use it (e.g. semantic adapter weights high-importance thoughts
    /// more in synthesis).
    pub importance: f64,
    pub created_at_ms: u64,
    /// `true` when this thought is private to the persona (not
    /// broadcastable). Adapters respect this when deciding whether
    /// to emit it as shareable long-term memory.
    pub shareable: bool,
}

/// One consolidated memory — the adapter's output row, ready for LTM
/// persistence by the caller.
#[derive(Debug, Clone)]
pub struct ConsolidatedMemory {
    pub id: Uuid,
    pub persona_id: Uuid,
    pub session_id: Uuid,
    /// Coerced down to a finite vocabulary at the corpus boundary;
    /// the adapter picks the best-fit type for each emitted memory.
    pub memory_type: MemoryType,
    pub content: String,
    pub importance: f64,
    pub created_at_ms: u64,
    pub timestamp_ms: u64,
    pub consolidated_at_ms: u64,
    pub tags: Vec<String>,
    /// IDs of the source thoughts this memory was synthesized from.
    /// For raw (pass-through) adapters: single-element vec (1:1). For
    /// synthesis adapters: multi-element (N thoughts → 1 memory). The
    /// consolidator uses this to know WHICH working-memory rows to
    /// clear after successful promotion.
    pub synthesized_from: Vec<Uuid>,
}

/// Closed vocabulary for the memory-type tag. Mirrors the TS `MemoryType`
/// enum; adapters that don't map cleanly to one of these pick the
/// closest match (usually `Observation`) rather than inventing a new
/// tag — keeping the vocabulary closed at the Rust boundary means
/// downstream recall code doesn't have to handle unknown types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryType {
    Observation,
    Decision,
    Insight,
    Reflection,
}

impl MemoryType {
    /// Best-fit mapping from the free-form thought_type string to
    /// the closed enum. Default to `Observation` for unknown values.
    pub fn from_thought_type(s: &str) -> Self {
        match s {
            "decision" => Self::Decision,
            "pattern" | "insight" => Self::Insight,
            "reflection" => Self::Reflection,
            // "observation" and anything unknown → Observation.
            _ => Self::Observation,
        }
    }
}

/// Adapter output + pass metadata.
#[derive(Debug, Clone, Default)]
pub struct ConsolidationResult {
    pub memories: Vec<ConsolidatedMemory>,
    pub synthesis_count: u64,
    pub groups_created: u64,
    pub embeddings_generated: u64,
}

/// Strategy trait. Each impl decides how N working-memory thoughts
/// become M long-term memories. The `async` comes from the synthesis
/// adapters that make LLM calls; raw adapters that don't need it are
/// still async-compatible trivially.
#[async_trait]
pub trait ConsolidationAdapter: Send + Sync {
    /// Consolidate a batch of thoughts into long-term memories. The
    /// caller (Consolidator's snoop loop) will write the returned
    /// memories to LTM and then clear the source thought IDs from
    /// working memory on success.
    async fn consolidate(
        &self,
        thoughts: &[Thought],
        context: &ConsolidationContext,
    ) -> Result<ConsolidationResult, String>;

    /// Adapter name for logs / metrics. Short, stable string.
    fn name(&self) -> &'static str;

    /// `true` when this adapter produces embeddings alongside the
    /// memory rows. Caller uses this hint to decide whether to write
    /// embeddings out to the vector store.
    fn supports_embeddings(&self) -> bool {
        false
    }

    /// `true` when this adapter does LLM synthesis (N thoughts → M<N
    /// memories). `false` for pass-through adapters. Caller uses this
    /// to set expectations on per-call latency (synthesis = seconds,
    /// pass-through = microseconds).
    fn does_synthesis(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_type_from_thought_type_known_and_unknown() {
        // What this catches: the mapping table + the default branch.
        // A mutation that, say, reordered the match arms or dropped
        // the default would either miscategorize known types or panic
        // on unknown ones. The consolidated vocabulary is what
        // downstream recall filtering uses; drift here creates silent
        // categorization bugs that only show up when a persona's
        // recall-by-type query starts missing results.
        //
        // Validated 2026-04-21: mutation = delete the "decision" arm
        // (so it falls through to Observation) → the decision
        // assertion fails. Reverted.
        assert_eq!(
            MemoryType::from_thought_type("decision"),
            MemoryType::Decision
        );
        assert_eq!(
            MemoryType::from_thought_type("pattern"),
            MemoryType::Insight
        );
        assert_eq!(
            MemoryType::from_thought_type("insight"),
            MemoryType::Insight
        );
        assert_eq!(
            MemoryType::from_thought_type("reflection"),
            MemoryType::Reflection
        );
        assert_eq!(
            MemoryType::from_thought_type("observation"),
            MemoryType::Observation
        );
        // Unknown → Observation (default fallthrough).
        assert_eq!(
            MemoryType::from_thought_type("some-new-type"),
            MemoryType::Observation
        );
        assert_eq!(MemoryType::from_thought_type(""), MemoryType::Observation);
    }

    /// Minimal NoOp adapter — proves the trait is implementable and
    /// the default methods (`supports_embeddings`, `does_synthesis`)
    /// return the baseline `false`. Not part of the production
    /// adapter set; lives in tests.
    struct NoOpAdapter;

    #[async_trait]
    impl ConsolidationAdapter for NoOpAdapter {
        async fn consolidate(
            &self,
            _thoughts: &[Thought],
            _context: &ConsolidationContext,
        ) -> Result<ConsolidationResult, String> {
            Ok(ConsolidationResult::default())
        }
        fn name(&self) -> &'static str {
            "NoOpAdapter"
        }
    }

    #[tokio::test]
    async fn noop_adapter_returns_empty_result() {
        // What this catches: the trait's default-method semantics
        // (`supports_embeddings=false`, `does_synthesis=false`) and
        // that `ConsolidationResult::default()` produces an empty,
        // zero-counter result. A mutation that flipped the defaults
        // to `true` would silently advertise nonexistent capabilities
        // to callers deciding whether to run embedding writes or
        // warn about slow synthesis — visible-later bugs that the
        // NoOp test catches at the trait surface.
        //
        // Validated 2026-04-21: mutation = flip
        // `supports_embeddings` default to `true` → the
        // `!adapter.supports_embeddings()` assertion fails. Reverted.
        let adapter = NoOpAdapter;
        assert_eq!(adapter.name(), "NoOpAdapter");
        assert!(
            !adapter.supports_embeddings(),
            "NoOp shouldn't claim embedding support"
        );
        assert!(!adapter.does_synthesis(), "NoOp shouldn't claim synthesis");

        let ctx = ConsolidationContext {
            persona_id: Uuid::nil(),
            persona_name: "Test".to_string(),
            session_id: Uuid::nil(),
            timestamp_ms: 0,
        };
        let result = adapter.consolidate(&[], &ctx).await.expect("noop ok");
        assert!(result.memories.is_empty());
        assert_eq!(result.synthesis_count, 0);
        assert_eq!(result.groups_created, 0);
        assert_eq!(result.embeddings_generated, 0);
    }
}
