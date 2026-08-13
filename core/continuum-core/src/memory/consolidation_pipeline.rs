//! Wire-through: threshold + adapter + corpus in one call.
//!
//! Fifth 0.5.5 Hippocampus piece. Ties the additive-only substrate
//! pieces (AdaptiveConsolidationThreshold, Consolidator,
//! ConsolidationAdapter trait + impls) to the existing in-process
//! PersonaMemoryManager. One function that a snoop loop can call
//! when `consolidator.tick()` reports the cadence fired:
//!
//!   run_consolidation_pass(consolidator, thoughts, ctx, adapter, manager)
//!     → adapter.consolidate(thoughts, ctx)          (adapter strategy)
//!     → for each emitted memory: manager.append_memory (in-process write)
//!     → consolidator.record_success(N)              (reset decay + metrics)
//!
//! All in-process Rust. No reverse-IPC needed because
//! `PersonaMemoryManager.append_memory` is already Rust-native and
//! operates on the cached corpus per persona. The persona's TS-side
//! `longterm.db` picks up new memories via the ORM on the standard
//! corpus-sync path (outside this pipeline's scope; the TS layer
//! already subscribes to memory changes).
//!
//! What this does NOT own:
//! - WorkingMemory — the SOURCE of `thoughts`. The caller (future
//!   snoop loop) provides this vec from whatever its thought-stream
//!   abstraction is. Rust WorkingMemory primitive is still absent.
//! - Embedding generation — adapters that produce embeddings return
//!   them on the ConsolidatedMemory; the `to_corpus_memory` helper
//!   here propagates them to CorpusMemory.embedding. When the
//!   embedding-adapter lands it slots in transparently.

use chrono::DateTime;

use crate::memory::consolidation_adapter::{
    ConsolidatedMemory, ConsolidationAdapter, ConsolidationContext, ConsolidationResult,
};
use crate::memory::consolidator::Consolidator;
use crate::memory::types::{CorpusMemory, MemoryRecord};
use crate::memory::PersonaMemoryManager;

/// Convert a ConsolidatedMemory (adapter output) into a CorpusMemory
/// (PersonaMemoryManager input). Preserves every field the corpus
/// stores; drops adapter-only metadata (synthesis_from is tracked
/// via the caller to evict from working memory, not stored in the
/// corpus row itself).
pub fn to_corpus_memory(memory: &ConsolidatedMemory) -> CorpusMemory {
    CorpusMemory {
        record: MemoryRecord {
            id: memory.id.to_string(),
            persona_id: memory.persona_id.to_string(),
            memory_type: match memory.memory_type {
                crate::memory::consolidation_adapter::MemoryType::Observation => "observation",
                crate::memory::consolidation_adapter::MemoryType::Decision => "decision",
                crate::memory::consolidation_adapter::MemoryType::Insight => "insight",
                crate::memory::consolidation_adapter::MemoryType::Reflection => "reflection",
            }
            .to_string(),
            content: memory.content.clone(),
            // Tag the memory with its ROOM (contextId) — what recall scores on
            // (recall.rs reads "roomId"). Previously this emitted only
            // "sessionId", so the room bonus NEVER matched and memory lost its
            // room affinity on reconnect. Emit the canonical "contextId" plus the
            // "roomId" key recall reads today; keep sessionId as labelled session
            // metadata (NOT a context substitute). Omit the room keys entirely
            // when there's no context rather than writing nil.
            context: {
                let mut ctx = serde_json::json!({
                    "sessionId": memory.session_id.to_string(),
                    "synthesizedFrom": memory.synthesized_from.iter()
                        .map(|u| u.to_string())
                        .collect::<Vec<_>>(),
                });
                if let Some(context_id) = memory.context_id {
                    let id = context_id.to_string();
                    ctx["contextId"] = serde_json::json!(id);
                    ctx["roomId"] = serde_json::json!(id);
                }
                ctx
            },
            timestamp: ms_to_rfc3339(memory.timestamp_ms),
            importance: memory.importance,
            access_count: 0,
            tags: memory.tags.clone(),
            related_to: Vec::new(),
            source: Some("consolidation".to_string()),
            last_accessed_at: None,
            layer: None,
            relevance_score: None,
            origin_node: None,
            origin_seq: None,        },
        embedding: None,
    }
}

/// Run a full consolidation pass end-to-end.
///
/// Caller (a snoop loop) typically invokes this ONLY when
/// `consolidator.tick(messages_per_min)` returned `true`, but this
/// function doesn't enforce that — it's a legitimate use case to run
/// a pass unconditionally for tests or manual ops.
///
/// Returns the adapter's full result so the caller can inspect which
/// source-thought IDs were synthesized from (for working-memory
/// eviction) and telemetry.
pub async fn run_consolidation_pass(
    consolidator: &mut Consolidator,
    thoughts: &[crate::memory::consolidation_adapter::Thought],
    context: &ConsolidationContext,
    adapter: &dyn ConsolidationAdapter,
    manager: &PersonaMemoryManager,
) -> Result<ConsolidationResult, String> {
    let result = adapter.consolidate(thoughts, context).await?;

    for memory in &result.memories {
        let corpus_memory = to_corpus_memory(memory);
        manager
            .append_memory(&memory.persona_id.to_string().into(), corpus_memory)
            .map_err(|e| format!("append_memory failed for {}: {}", memory.id, e.0))?;
    }

    // Only record_success if we actually promoted something — an empty
    // pass shouldn't reset the time-decay clock, otherwise a persona
    // whose adapter produces nothing keeps getting "fresh" thresholds
    // and the time-based safety-net never fires.
    if !result.memories.is_empty() {
        consolidator.record_success(result.memories.len() as u64);
    }

    Ok(result)
}

fn ms_to_rfc3339(ms: u64) -> String {
    DateTime::from_timestamp_millis(ms as i64)
        .unwrap_or_else(|| DateTime::from_timestamp_millis(0).unwrap())
        .to_rfc3339()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::memory::consolidation_adapter::{MemoryType, Thought};
    use crate::memory::embedding::EmbeddingProvider;
    use crate::memory::raw_adapter::RawMemoryAdapter;
    use async_trait::async_trait;
    use std::collections::HashMap;
    use std::sync::Arc;
    use uuid::Uuid;

    /// Minimal embedding provider for tests — returns zero vectors.
    /// The consolidation pipeline never asks for embeddings (the raw
    /// adapter emits none, and PersonaMemoryManager only calls embed
    /// when a caller explicitly requests semantic recall), so this
    /// stub is enough to satisfy the type constraint.
    struct StubEmbedder;
    #[async_trait]
    impl EmbeddingProvider for StubEmbedder {
        fn id(&self) -> &str {
            "stub"
        }
        fn dim(&self) -> usize {
            8
        }
        async fn embed(&self, _text: &str) -> Vec<f32> {
            vec![0.0; 8]
        }
    }

    fn make_manager_with_empty_corpus(persona_id: &str) -> PersonaMemoryManager {
        let manager = PersonaMemoryManager::new(Arc::new(StubEmbedder));
        // Load an empty corpus so append_memory has a corpus to write into.
        // load_corpus returns LoadCorpusResponse (not Result) — it either
        // succeeds or records the failure in-band.
        let _ = manager.load_corpus(&persona_id.into(), Vec::new(), Vec::new());
        manager
    }

    fn make_thought(id: u8, content: &str) -> Thought {
        Thought {
            id: Uuid::from_u128(id as u128),
            thought_type: "observation".to_string(),
            content: content.to_string(),
            domain: Some("chat".to_string()),
            context_id: None,
            importance: 0.7,
            created_at_ms: 1_000 + id as u64,
            shareable: true,
        }
    }

    fn make_context(persona_id: Uuid) -> ConsolidationContext {
        ConsolidationContext {
            persona_id,
            persona_name: "TestPersona".to_string(),
            session_id: Uuid::from_u128(7),
            timestamp_ms: 9_000,
        }
    }

    #[tokio::test]
    async fn pass_writes_adapter_output_to_corpus() {
        // What this catches: the actual write-through. Adapter
        // produces N memories → manager.append_memory gets called N
        // times → corpus grows by N. A mutation that dropped the
        // write loop would silently lose every consolidated memory;
        // adapter metrics would claim success but the corpus stays
        // empty. The regression would only surface hours later when
        // a persona's recall returns "you have no memories."
        //
        // Validated 2026-04-21: mutation = replace the `for memory in
        // &result.memories { manager.append_memory(...) }` loop with
        // `{ }` (no-op) → the corpus-size assertion (== 3) fails
        // (stays 0). Reverted.
        let persona_id = Uuid::from_u128(42);
        let persona_key = persona_id.to_string();
        let manager = make_manager_with_empty_corpus(&persona_key);
        let mut consolidator = Consolidator::new();
        let adapter = RawMemoryAdapter;
        let ctx = make_context(persona_id);

        let thoughts = vec![
            make_thought(1, "first"),
            make_thought(2, "second"),
            make_thought(3, "third"),
        ];

        let result = run_consolidation_pass(&mut consolidator, &thoughts, &ctx, &adapter, &manager)
            .await
            .expect("pass should succeed");

        assert_eq!(result.memories.len(), 3);
        // Corpus now has 3 memories.
        let stats = manager.memory_stats();
        let (_, memories, _, _) = stats
            .iter()
            .find(|(id, _, _, _)| id == &persona_key)
            .expect("persona corpus loaded");
        assert_eq!(
            *memories, 3,
            "corpus should contain the 3 consolidated memories"
        );
        // Metrics updated.
        assert_eq!(consolidator.metrics().consolidation_count, 3);
    }

    #[tokio::test]
    async fn empty_thoughts_no_decay_clock_reset() {
        // What this catches: the guard against empty-pass clock reset.
        // A pass that promoted 0 memories MUST NOT reset the time-
        // decay clock — if it did, a persona whose adapter produces
        // nothing (below-threshold batch, failing synthesis, etc.)
        // would forever get "fresh" thresholds, and the time-decay
        // safety net that guarantees minimum consolidation frequency
        // never fires.
        //
        // Validated 2026-04-21: mutation = remove the `if
        // !result.memories.is_empty()` guard so `record_success(0)`
        // always runs → the assertion that seconds_since_consolidation
        // INCREASES after an empty pass fails (it resets to ~0
        // instead). Reverted.
        let persona_id = Uuid::from_u128(42);
        let persona_key = persona_id.to_string();
        let manager = make_manager_with_empty_corpus(&persona_key);
        let mut consolidator = Consolidator::new();

        // Age the decay clock.
        std::thread::sleep(std::time::Duration::from_millis(20));
        let before = consolidator.stats().threshold.seconds_since_consolidation;
        assert!(before > 0.0, "expected elapsed>0 before pass, got {before}");

        // Empty thoughts → adapter produces empty result → pipeline
        // MUST skip the record_success call.
        let adapter = RawMemoryAdapter;
        let ctx = make_context(persona_id);
        let result = run_consolidation_pass(&mut consolidator, &[], &ctx, &adapter, &manager)
            .await
            .expect("empty pass should succeed");

        assert!(result.memories.is_empty());
        assert_eq!(consolidator.metrics().consolidation_count, 0);

        let after = consolidator.stats().threshold.seconds_since_consolidation;
        assert!(
            after >= before,
            "empty pass reset the decay clock (before={before}, after={after}) — the safety-net timer would never fire"
        );
    }

    #[tokio::test]
    async fn to_corpus_memory_preserves_type_and_provenance() {
        // What this catches: the MemoryType → string conversion
        // vocabulary and the `synthesizedFrom` provenance carried
        // through the context blob. Downstream recall code filters
        // by memory_type string; a mutation that swapped Decision
        // and Insight in the match would silently miscategorize.
        // The synthesizedFrom trail is the only way a later editor
        // can audit which raw thoughts became a given memory.
        //
        // Validated 2026-04-21: mutation = swap
        // `MemoryType::Decision => "decision"` with
        // `MemoryType::Decision => "observation"` → memory_type
        // assertion fails. Reverted.
        let room = Uuid::from_u128(0xC0);
        let m = ConsolidatedMemory {
            id: Uuid::from_u128(100),
            persona_id: Uuid::from_u128(42),
            session_id: Uuid::from_u128(7),
            context_id: Some(room),
            memory_type: MemoryType::Decision,
            content: "chose path A".to_string(),
            importance: 0.9,
            created_at_ms: 1_000,
            timestamp_ms: 1_000,
            consolidated_at_ms: 2_000,
            tags: vec!["code".to_string()],
            synthesized_from: vec![Uuid::from_u128(11), Uuid::from_u128(12)],
        };
        let cm = to_corpus_memory(&m);
        assert_eq!(cm.record.memory_type, "decision");
        assert_eq!(cm.record.content, "chose path A");
        // what this catches: the memory carries its ROOM under the key recall
        // scores on ("roomId") + the canonical "contextId". Regression here =
        // recall's room bonus silently never matches and memory loses room
        // affinity on reconnect (it used to emit only "sessionId").
        assert_eq!(
            cm.record.context["roomId"].as_str(),
            Some(room.to_string().as_str()),
            "memory must carry its room under the key recall reads"
        );
        assert_eq!(
            cm.record.context["contextId"].as_str(),
            Some(room.to_string().as_str())
        );
        assert_eq!(cm.record.importance, 0.9);
        assert_eq!(cm.record.tags, vec!["code".to_string()]);
        assert_eq!(cm.record.source.as_deref(), Some("consolidation"));
        let synth: Vec<String> = cm.record.context["synthesizedFrom"]
            .as_array()
            .expect("synthesizedFrom present")
            .iter()
            .map(|v| v.as_str().unwrap().to_string())
            .collect();
        assert_eq!(
            synth,
            vec![
                Uuid::from_u128(11).to_string(),
                Uuid::from_u128(12).to_string()
            ]
        );
    }

    // silence unused-warning under cfg(test) for an import we want
    // available but don't reference in every test body.
    #[allow(dead_code)]
    fn _unused() {
        let _: HashMap<String, ()> = HashMap::new();
    }
}
