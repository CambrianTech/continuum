//! Pass-through consolidation — one thought, one memory, no synthesis.
//!
//! Fourth 0.5.5 Hippocampus piece. Port of `RawMemoryAdapter.ts`. The
//! baseline Phase 1 adapter: consolidation without LLM synthesis.
//! Every candidate thought becomes exactly one long-term memory, fields
//! copied over with `synthesized_from = [thought.id]`.
//!
//! Use cases (from the TS doc):
//! - Debugging / analysis (preserve raw thought stream in LTM).
//! - Low-resource personas that can't afford LLM synthesis latency.
//! - Baseline for A/B comparisons against synthesis adapters.
//!
//! No state, no config — a single unit struct is enough.

use async_trait::async_trait;
use uuid::Uuid;

use crate::memory::consolidation_adapter::{
    ConsolidatedMemory, ConsolidationAdapter, ConsolidationContext, ConsolidationResult,
    MemoryType, Thought,
};

/// Pass-through: one thought in → one memory out.
pub struct RawMemoryAdapter;

#[async_trait]
impl ConsolidationAdapter for RawMemoryAdapter {
    async fn consolidate(
        &self,
        thoughts: &[Thought],
        context: &ConsolidationContext,
    ) -> Result<ConsolidationResult, String> {
        let memories: Vec<ConsolidatedMemory> = thoughts
            .iter()
            .map(|t| ConsolidatedMemory {
                id: Uuid::new_v4(),
                persona_id: context.persona_id,
                session_id: context.session_id,
                memory_type: MemoryType::from_thought_type(&t.thought_type),
                content: t.content.clone(),
                importance: t.importance,
                created_at_ms: t.created_at_ms,
                timestamp_ms: t.created_at_ms,
                consolidated_at_ms: context.timestamp_ms,
                tags: t.domain.clone().map(|d| vec![d]).unwrap_or_default(),
                synthesized_from: vec![t.id],
            })
            .collect();

        let count = memories.len() as u64;
        Ok(ConsolidationResult {
            memories,
            synthesis_count: 0,
            groups_created: count,
            embeddings_generated: 0,
        })
    }

    fn name(&self) -> &'static str {
        "RawMemoryAdapter"
    }

    // does_synthesis defaults to false — explicit override for readers:
    // this adapter is deliberately the non-synthesis baseline.
    fn does_synthesis(&self) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_thought(id: u8, content: &str, thought_type: &str, domain: Option<&str>) -> Thought {
        Thought {
            id: Uuid::from_u128(id as u128),
            thought_type: thought_type.to_string(),
            content: content.to_string(),
            domain: domain.map(String::from),
            context_id: None,
            importance: 0.7,
            created_at_ms: 1_000 + id as u64,
            shareable: true,
        }
    }

    fn make_context() -> ConsolidationContext {
        ConsolidationContext {
            persona_id: Uuid::from_u128(42),
            persona_name: "Test".to_string(),
            session_id: Uuid::from_u128(7),
            timestamp_ms: 9_000,
        }
    }

    #[tokio::test]
    async fn one_thought_one_memory_preserves_fields() {
        // What this catches: the 1:1 pass-through contract — this
        // adapter must produce exactly as many memories as input
        // thoughts, with content preserved verbatim, source-id
        // tracked via synthesized_from, and context's timestamp
        // landing in consolidated_at (NOT thought timestamp — two
        // different concepts). A mutation that emitted N-1 memories
        // (say, skipping the first) would silently drop data during
        // consolidation; one that used `context.timestamp_ms` for
        // thought `timestamp_ms` would confuse "when the thought
        // happened" with "when it got promoted," breaking downstream
        // temporal recall.
        //
        // Validated 2026-04-21: mutation = replace
        // `t.created_at_ms` with `context.timestamp_ms` for the
        // `timestamp_ms` field → the `memories[0].timestamp_ms ==
        // 1000` assertion fails (gets 9000 instead). Reverted.
        let thoughts = vec![
            make_thought(0, "first", "reflection", Some("chat")),
            make_thought(1, "second", "decision", None),
        ];
        let ctx = make_context();
        let result = RawMemoryAdapter.consolidate(&thoughts, &ctx).await.unwrap();

        assert_eq!(result.memories.len(), 2);
        assert_eq!(result.groups_created, 2);
        assert_eq!(result.synthesis_count, 0);
        assert_eq!(result.embeddings_generated, 0);

        let m0 = &result.memories[0];
        assert_eq!(m0.content, "first");
        assert_eq!(m0.memory_type, MemoryType::Reflection);
        assert_eq!(m0.synthesized_from, vec![thoughts[0].id]);
        assert_eq!(m0.persona_id, ctx.persona_id);
        assert_eq!(m0.session_id, ctx.session_id);
        assert_eq!(
            m0.timestamp_ms, 1000,
            "timestamp_ms should reflect when the THOUGHT happened, not the consolidation pass"
        );
        assert_eq!(
            m0.consolidated_at_ms, ctx.timestamp_ms,
            "consolidated_at_ms should reflect the consolidation pass timestamp"
        );
        assert_eq!(m0.tags, vec!["chat".to_string()]);

        let m1 = &result.memories[1];
        assert_eq!(m1.content, "second");
        assert_eq!(m1.memory_type, MemoryType::Decision);
        assert_eq!(m1.synthesized_from, vec![thoughts[1].id]);
        assert!(m1.tags.is_empty(), "thought without domain → no tags");
    }

    #[tokio::test]
    async fn empty_input_empty_output() {
        // What this catches: the no-op path. An adapter that
        // accidentally emitted a sentinel "empty batch" memory on
        // zero input (say, an off-by-one loop bound that ran once
        // over an empty vec) would poison LTM with blank rows.
        // Assertion: zero in → zero out, all counters zero.
        //
        // Validated 2026-04-21: mutation = replace
        // `thoughts.iter().map(...)` with code that pushes one
        // default ConsolidatedMemory regardless → memories.len() == 1
        // makes the assertion fail. Reverted.
        let result = RawMemoryAdapter
            .consolidate(&[], &make_context())
            .await
            .unwrap();
        assert!(result.memories.is_empty());
        assert_eq!(result.groups_created, 0);
        assert_eq!(result.synthesis_count, 0);
    }

    #[tokio::test]
    async fn adapter_advertises_non_synthesis() {
        // What this catches: the explicit `does_synthesis → false`
        // override. If a refactor dropped the override (accidentally
        // inheriting a future default-true from the trait), callers
        // key on this for latency expectations — they'd wait seconds
        // for a microsecond operation. Pins the contract.
        //
        // Validated 2026-04-21: mutation = return `true` from
        // `does_synthesis` → assertion fails. Reverted.
        assert!(!RawMemoryAdapter.does_synthesis());
        assert!(!RawMemoryAdapter.supports_embeddings());
        assert_eq!(RawMemoryAdapter.name(), "RawMemoryAdapter");
    }
}
