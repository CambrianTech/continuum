//! Hippocampus — Rust-native memory subsystem for AI personas.
//!
//! Rust is a pure compute engine. Data comes from the TS ORM via IPC.
//! No SQL, no filesystem access — all operations run on in-memory MemoryCorpus.
//!
//! Architecture:
//! ```text
//! PersonaMemoryManager (DashMap<persona_id, Arc<MemoryCorpus>>)
//!   ├── embedding_provider: Arc<dyn EmbeddingProvider>  (shared, loaded once)
//!   ├── recall_engine: MultiLayerRecall                 (6 pluggable layers)
//!   └── per-persona cached MemoryCorpus                 (loaded from TS ORM via IPC)
//! ```
//!
//! Data flow: TS ORM queries data → sends to Rust via memory/load-corpus IPC
//! → Rust caches MemoryCorpus per persona → recall layers operate on corpus.
//!
//! Extension points (trait-based, pluggable):
//! - EmbeddingProvider: swap embedding models (fastembed, BGE, fine-tuned)
//! - RecallLayer: add new retrieval strategies (neural, graph, attention-based)
//! - Each layer is an independent "PhD paper" — develop/test/replace independently

pub mod cache;
pub mod consciousness;
pub mod corpus;
pub mod embedding;
pub mod recall;
pub mod timeline;
pub mod types;

pub use cache::MemoryCache;
pub use consciousness::build_consciousness_context;
pub use corpus::MemoryCorpus;
pub use embedding::{cosine_similarity, DeterministicEmbeddingProvider, EmbeddingProvider, FastEmbedProvider};
pub use recall::{MultiLayerRecall, RecallLayer, RecallQuery, ScoredMemory};
pub use types::*;

use dashmap::DashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

// ─── Error ────────────────────────────────────────────────────────────────────

/// Memory subsystem error — no SQL, just logic errors.
#[derive(Debug)]
pub struct MemoryError(pub String);

impl std::fmt::Display for MemoryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for MemoryError {}

// ─── PersonaMemoryManager ─────────────────────────────────────────────────────

/// Top-level manager for all persona memory operations.
///
/// - Holds per-persona MemoryCorpus in a DashMap (zero cross-persona contention)
/// - Shared embedding provider loaded once at startup (~100ms)
/// - 6-layer multi-recall runs in parallel via Rayon on in-memory data
/// - Consciousness context cached per-persona with 30s TTL
///
/// Thread safety: All 14 personas operate on independent DashMap entries.
/// Within a persona, recall layers operate on shared &MemoryCorpus (read-only).
pub struct PersonaMemoryManager {
    corpora: DashMap<String, Arc<MemoryCorpus>>,
    embedding: Arc<dyn EmbeddingProvider>,
    recall_engine: MultiLayerRecall,
    consciousness_cache: MemoryCache<ConsciousnessContextResponse>,
}

impl PersonaMemoryManager {
    pub fn new(embedding: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            corpora: DashMap::new(),
            embedding,
            recall_engine: MultiLayerRecall::new(),
            consciousness_cache: MemoryCache::new(Duration::from_secs(30)),
        }
    }

    // ─── Corpus Lifecycle ─────────────────────────────────────────────────────

    /// Load a persona's memory corpus (called from TS ORM via IPC).
    /// Replaces any previously cached corpus for this persona.
    pub fn load_corpus(
        &self,
        persona_id: &str,
        corpus_memories: Vec<CorpusMemory>,
        corpus_events: Vec<CorpusTimelineEvent>,
    ) -> LoadCorpusResponse {
        let start = Instant::now();

        let embedded_memory_count = corpus_memories.iter().filter(|cm| cm.embedding.is_some()).count();
        let embedded_event_count = corpus_events.iter().filter(|ce| ce.embedding.is_some()).count();
        let memory_count = corpus_memories.len();
        let timeline_event_count = corpus_events.len();

        let corpus = MemoryCorpus::from_corpus_data(corpus_memories, corpus_events);
        self.corpora.insert(persona_id.to_string(), Arc::new(corpus));

        // Invalidate consciousness cache (new data affects context)
        self.consciousness_cache.invalidate(persona_id);

        let load_time_ms = start.elapsed().as_secs_f64() * 1000.0;

        LoadCorpusResponse {
            memory_count,
            embedded_memory_count,
            timeline_event_count,
            embedded_event_count,
            load_time_ms,
        }
    }

    /// Get a persona's cached corpus.
    fn get_corpus(&self, persona_id: &str) -> Result<Arc<MemoryCorpus>, MemoryError> {
        self.corpora
            .get(persona_id)
            .map(|c| c.value().clone())
            .ok_or_else(|| {
                MemoryError(format!(
                    "No memory corpus for persona {persona_id}. Call memory/load-corpus first."
                ))
            })
    }

    // ─── Recall Operations ────────────────────────────────────────────────────

    /// 6-layer parallel multi-recall — the improved recall algorithm.
    /// Operates on in-memory MemoryCorpus data. Zero SQL.
    pub fn multi_layer_recall(
        &self,
        persona_id: &str,
        req: &MultiLayerRecallRequest,
    ) -> Result<MemoryRecallResponse, MemoryError> {
        let corpus = self.get_corpus(persona_id)?;

        // Pre-compute query embedding if text provided
        let query_embedding = req.query_text.as_ref().and_then(|text| {
            self.embedding.embed(text).ok()
        });

        let query = RecallQuery {
            query_text: req.query_text.clone(),
            query_embedding,
            room_id: req.room_id.clone(),
            max_results_per_layer: (req.max_results / 2).max(5),
        };

        Ok(self.recall_engine.recall_parallel(
            &corpus,
            &query,
            self.embedding.as_ref(),
            req.max_results,
        ))
    }

    // ─── Consciousness Context ────────────────────────────────────────────────

    /// Build consciousness context (temporal + cross-context + intentions).
    /// Cached per-persona with 30s TTL.
    pub fn consciousness_context(
        &self,
        persona_id: &str,
        req: &ConsciousnessContextRequest,
    ) -> Result<ConsciousnessContextResponse, MemoryError> {
        // Check cache
        let cache_key = format!("{}:{}", persona_id, req.room_id);
        if let Some(cached) = self.consciousness_cache.get(&cache_key) {
            return Ok(cached);
        }

        let corpus = self.get_corpus(persona_id)?;
        let response = build_consciousness_context(&corpus, req);

        // Cache the result
        self.consciousness_cache.set(cache_key, response.clone());

        Ok(response)
    }

    // ─── Incremental Append (Cache Coherence) ───────────────────────────────

    /// Append a single memory to the persona's cached corpus.
    /// Copy-on-write: clones corpus, appends memory, swaps Arc in DashMap.
    /// Readers holding old Arc are unaffected (snapshot isolation).
    /// O(n) per append, but appends are rare (~1/min/persona).
    pub fn append_memory(
        &self,
        persona_id: &str,
        memory: CorpusMemory,
    ) -> Result<(), MemoryError> {
        let old_corpus = self.get_corpus(persona_id)?;
        let new_corpus = old_corpus.with_appended_memory(memory);
        self.corpora.insert(persona_id.to_string(), Arc::new(new_corpus));
        self.consciousness_cache.invalidate(persona_id);
        Ok(())
    }

    /// Append a single timeline event to the persona's cached corpus.
    /// Copy-on-write: clones corpus, appends event, swaps Arc in DashMap.
    pub fn append_event(
        &self,
        persona_id: &str,
        event: CorpusTimelineEvent,
    ) -> Result<(), MemoryError> {
        let old_corpus = self.get_corpus(persona_id)?;
        let new_corpus = old_corpus.with_appended_event(event);
        self.corpora.insert(persona_id.to_string(), Arc::new(new_corpus));
        self.consciousness_cache.invalidate(persona_id);
        Ok(())
    }

    // ─── Maintenance ──────────────────────────────────────────────────────────

    /// Evict expired cache entries (call periodically).
    pub fn evict_caches(&self) {
        self.consciousness_cache.evict_expired();
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Stub embedding provider for tests (avoids loading real model).
    struct StubEmbeddingProvider;

    impl EmbeddingProvider for StubEmbeddingProvider {
        fn name(&self) -> &str {
            "stub"
        }
        fn dimensions(&self) -> usize {
            384
        }
        fn embed(&self, _text: &str) -> Result<Vec<f32>, embedding::EmbeddingError> {
            Ok(vec![0.1; 384])
        }
        fn embed_batch(
            &self,
            texts: &[String],
        ) -> Result<Vec<Vec<f32>>, embedding::EmbeddingError> {
            Ok(texts.iter().map(|_| vec![0.1; 384]).collect())
        }
    }

    fn test_manager() -> PersonaMemoryManager {
        PersonaMemoryManager::new(Arc::new(StubEmbeddingProvider))
    }

    fn make_corpus_memory(id: &str, content: &str, importance: f64) -> CorpusMemory {
        CorpusMemory {
            record: MemoryRecord {
                id: id.into(),
                persona_id: "test".into(),
                memory_type: "observation".into(),
                content: content.into(),
                context: serde_json::json!({}),
                timestamp: chrono::Utc::now().to_rfc3339(),
                importance,
                access_count: 0,
                tags: vec![],
                related_to: vec![],
                source: Some("test".into()),
                last_accessed_at: None,
                layer: None,
                relevance_score: None,
            },
            embedding: Some(vec![0.1; 384]),
        }
    }

    fn make_corpus_event(id: &str, context_id: &str, context_name: &str) -> CorpusTimelineEvent {
        CorpusTimelineEvent {
            event: TimelineEvent {
                id: id.into(),
                persona_id: "test".into(),
                timestamp: chrono::Utc::now().to_rfc3339(),
                context_type: "room".into(),
                context_id: context_id.into(),
                context_name: context_name.into(),
                event_type: "message".into(),
                actor_id: "user1".into(),
                actor_name: "User".into(),
                content: "test content".into(),
                importance: 0.6,
                topics: vec![],
            },
            embedding: None,
        }
    }

    #[test]
    fn test_load_corpus() {
        let manager = test_manager();

        let memories = vec![
            make_corpus_memory("m1", "Purple elephants dance", 0.9),
            make_corpus_memory("m2", "Blue sky observation", 0.5),
        ];
        let events = vec![
            make_corpus_event("e1", "room-1", "General"),
        ];

        let resp = manager.load_corpus("p1", memories, events);
        assert_eq!(resp.memory_count, 2);
        assert_eq!(resp.embedded_memory_count, 2);
        assert_eq!(resp.timeline_event_count, 1);
        assert_eq!(resp.embedded_event_count, 0);
        assert!(resp.load_time_ms >= 0.0);
    }

    #[test]
    fn test_multi_layer_recall() {
        let manager = test_manager();

        let memories = vec![
            make_corpus_memory("m1", "Memory number 0", 0.9),
            make_corpus_memory("m2", "Memory number 1", 0.7),
            make_corpus_memory("m3", "Memory number 2", 0.5),
        ];

        manager.load_corpus("p1", memories, vec![]);

        let req = MultiLayerRecallRequest {
            query_text: Some("memory test".into()),
            room_id: "room-1".into(),
            max_results: 10,
            layers: None,
        };

        let resp = manager.multi_layer_recall("p1", &req).unwrap();
        assert!(!resp.memories.is_empty());
        assert!(resp.recall_time_ms > 0.0);
        assert!(!resp.layer_timings.is_empty());
    }

    #[test]
    fn test_consciousness_context_caching() {
        let manager = test_manager();

        let events = vec![
            make_corpus_event("e1", "room-1", "General"),
            make_corpus_event("e2", "room-2", "Academy"),
        ];

        manager.load_corpus("p1", vec![], events);

        let req = ConsciousnessContextRequest {
            room_id: "room-1".into(),
            current_message: None,
            skip_semantic_search: false,
        };

        // First call: cache miss
        let resp1 = manager.consciousness_context("p1", &req).unwrap();

        // Second call: cache hit
        let resp2 = manager.consciousness_context("p1", &req).unwrap();
        assert_eq!(resp2.cross_context_event_count, resp1.cross_context_event_count);
    }

    #[test]
    fn test_corpus_not_loaded() {
        let manager = test_manager();
        let req = MultiLayerRecallRequest {
            query_text: None,
            room_id: "room-1".into(),
            max_results: 10,
            layers: None,
        };
        let result = manager.multi_layer_recall("nonexistent", &req);
        assert!(result.is_err());
    }

    #[test]
    fn test_load_corpus_replaces_previous() {
        let manager = test_manager();

        // Load initial corpus with 1 memory
        manager.load_corpus("p1", vec![make_corpus_memory("m1", "first", 0.9)], vec![]);

        // Load new corpus with 3 memories
        let resp = manager.load_corpus("p1", vec![
            make_corpus_memory("m2", "second", 0.8),
            make_corpus_memory("m3", "third", 0.7),
            make_corpus_memory("m4", "fourth", 0.6),
        ], vec![]);

        assert_eq!(resp.memory_count, 3);

        // Recall should find new memories, not old ones
        let req = MultiLayerRecallRequest {
            query_text: None,
            room_id: "room-1".into(),
            max_results: 10,
            layers: None,
        };
        let recall_resp = manager.multi_layer_recall("p1", &req).unwrap();
        assert!(recall_resp.memories.iter().all(|m| m.id != "m1"));
    }

    #[test]
    fn test_append_memory() {
        let manager = test_manager();

        // Load initial corpus
        manager.load_corpus("p1", vec![
            make_corpus_memory("m1", "Initial memory", 0.9),
        ], vec![]);

        // Append a new memory
        let new_memory = make_corpus_memory("m2", "Appended memory", 0.7);
        manager.append_memory("p1", new_memory).unwrap();

        // Verify both memories exist in recall
        let req = MultiLayerRecallRequest {
            query_text: None,
            room_id: "room-1".into(),
            max_results: 10,
            layers: None,
        };
        let resp = manager.multi_layer_recall("p1", &req).unwrap();
        let ids: Vec<&str> = resp.memories.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"m1"), "Original memory should still exist");
        assert!(ids.contains(&"m2"), "Appended memory should exist");
    }

    #[test]
    fn test_append_event() {
        let manager = test_manager();

        // Load initial corpus with one event
        manager.load_corpus("p1", vec![], vec![
            make_corpus_event("e1", "room-1", "General"),
        ]);

        // Append a new event
        let new_event = make_corpus_event("e2", "room-2", "Academy");
        manager.append_event("p1", new_event).unwrap();

        // Verify consciousness context sees both events
        let req = crate::memory::ConsciousnessContextRequest {
            room_id: "room-1".into(),
            current_message: None,
            skip_semantic_search: false,
        };
        let resp = manager.consciousness_context("p1", &req).unwrap();
        // room-2 event should appear as cross-context (not in room-1)
        assert!(resp.cross_context_event_count >= 1);
    }

    #[test]
    fn test_append_to_nonexistent_corpus_fails() {
        let manager = test_manager();

        let memory = make_corpus_memory("m1", "orphan", 0.5);
        let result = manager.append_memory("nonexistent", memory);
        assert!(result.is_err(), "Append to nonexistent corpus should fail");
    }

    #[test]
    fn test_append_preserves_embeddings() {
        let manager = test_manager();

        // Load initial corpus with embedded memory
        manager.load_corpus("p1", vec![
            make_corpus_memory("m1", "with embedding", 0.9), // has Some(vec![0.1; 384])
        ], vec![]);

        // Append another embedded memory
        manager.append_memory("p1", make_corpus_memory("m2", "also embedded", 0.8)).unwrap();

        // Both should be findable via semantic recall (which needs embeddings)
        let req = MultiLayerRecallRequest {
            query_text: Some("embedded".into()),
            room_id: "room-1".into(),
            max_results: 10,
            layers: None,
        };
        let resp = manager.multi_layer_recall("p1", &req).unwrap();
        assert!(resp.memories.len() >= 2, "Both embedded memories should be recalled");
    }
}
