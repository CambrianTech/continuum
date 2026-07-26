//! Hippocampus — Rust-native memory subsystem for AI personas.
//!
//! Rust is a pure compute engine. Data comes from the TS ORM via IPC.
//! No SQL, no filesystem access — all operations run on in-memory MemoryCorpus.
//!
//! Architecture:
//! ```text
//! PersonaMemoryManager (DashMap<persona_id, Arc<RwLock<MemoryCorpus>>>)
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
pub mod consolidation_adapter;
pub mod consolidation_pipeline;
pub mod consolidation_threshold;
pub mod consolidator;
pub mod conversation_summary;
pub mod corpus;
pub mod embedding;
pub mod raw_adapter;
pub mod recall;
pub mod timeline;
pub mod types;

pub use cache::MemoryCache;
pub use consciousness::build_consciousness_context;
pub use consolidation_adapter::{
    ConsolidatedMemory, ConsolidationAdapter, ConsolidationContext, ConsolidationResult,
    MemoryType as ConsolidatedMemoryType, Thought,
};
pub use consolidation_pipeline::{run_consolidation_pass, to_corpus_memory};
pub use consolidation_threshold::{AdaptiveConsolidationThreshold, ConsolidationThresholdStats};
pub use consolidator::{ConsolidationMetrics, Consolidator, ConsolidatorStats};
pub use conversation_summary::{ConversationSummary, RecallMode};
pub use corpus::MemoryCorpus;
pub use embedding::{cosine_similarity, DeterministicEmbeddingProvider, EmbeddingProvider};
pub use raw_adapter::RawMemoryAdapter;
pub use recall::{MultiLayerRecall, RecallLayer, RecallQuery, ScoredMemory};
pub use types::*;

use dashmap::DashMap;
use std::sync::{Arc, RwLock};
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

/// Max memories per persona corpus before trimming (keep highest importance).
const MAX_MEMORIES_PER_CORPUS: usize = 2000;
/// Max timeline events per persona corpus before trimming (keep most recent).
const MAX_EVENTS_PER_CORPUS: usize = 2000;
/// Stale corpus TTL — evict if not accessed in 30 minutes.
const CORPUS_STALE_TTL: Duration = Duration::from_secs(30 * 60);

/// Top-level manager for all persona memory operations.
///
/// - Holds per-persona MemoryCorpus in a DashMap (zero cross-persona contention)
/// - Shared embedding provider loaded once at startup (~100ms)
/// - 6-layer multi-recall runs in parallel via Rayon on in-memory data
/// - Consciousness context cached per-persona with 30s TTL
///
/// Thread safety: All 14 personas operate on independent DashMap entries.
/// RwLock per corpus: multiple concurrent readers, exclusive writer.
/// In-place mutation on append — zero full-corpus cloning.
pub struct PersonaMemoryManager {
    corpora: DashMap<String, Arc<RwLock<MemoryCorpus>>>,
    corpus_access_times: DashMap<String, Instant>,
    embedding: Arc<dyn EmbeddingProvider>,
    recall_engine: MultiLayerRecall,
    consciousness_cache: MemoryCache<ConsciousnessContextResponse>,
}

impl PersonaMemoryManager {
    pub fn new(embedding: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            corpora: DashMap::new(),
            corpus_access_times: DashMap::new(),
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

        let embedded_memory_count = corpus_memories
            .iter()
            .filter(|cm| cm.embedding.is_some())
            .count();
        let embedded_event_count = corpus_events
            .iter()
            .filter(|ce| ce.embedding.is_some())
            .count();
        let memory_count = corpus_memories.len();
        let timeline_event_count = corpus_events.len();

        let corpus = MemoryCorpus::from_corpus_data(corpus_memories, corpus_events);
        self.corpora
            .insert(persona_id.to_string(), Arc::new(RwLock::new(corpus)));
        self.corpus_access_times
            .insert(persona_id.to_string(), Instant::now());

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

    /// Whether a corpus is already cached for this persona — the hydrate-on-miss
    /// gate the `memory/*` commands check before loading from the durable store.
    pub fn has_corpus(&self, persona_id: &str) -> bool {
        self.corpora.contains_key(persona_id)
    }

    /// Get a persona's cached corpus (Arc<RwLock>). Caller acquires read/write lock as needed.
    fn get_corpus(&self, persona_id: &str) -> Result<Arc<RwLock<MemoryCorpus>>, MemoryError> {
        self.corpus_access_times
            .insert(persona_id.to_string(), Instant::now());
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
    ///
    /// Async because the query embedding is produced through the adapter-routed
    /// [`EmbeddingProvider`] (unsloth `/v1/embeddings`, task #40) BEFORE the
    /// synchronous Rayon recall layers run. The layers themselves never embed —
    /// they consume the pre-computed `query.query_embedding`, so the only async
    /// hop is this one round-trip (cached content-addressed; one embed per unique
    /// query). An empty vector means "no signal" (embedder down / no model) and
    /// the semantic/cross-context layers degrade to no-op, never panic.
    pub async fn multi_layer_recall(
        &self,
        persona_id: &str,
        req: &MultiLayerRecallRequest,
    ) -> Result<MemoryRecallResponse, MemoryError> {
        let corpus_lock = self.get_corpus(persona_id)?;

        // Pre-compute query embedding (adapter round-trip) OUTSIDE the read lock
        // and OUTSIDE the sync Rayon recall — never hold a lock across await.
        let query_embedding = match req.query_text.as_ref() {
            Some(text) => {
                let v = self.embedding.embed(text).await;
                // Empty = "no signal" (down embedder); treat as absent so the
                // semantic layer degrades rather than scoring against zeros.
                if v.is_empty() {
                    None
                } else {
                    Some(v)
                }
            }
            None => None,
        };

        let had_query_embedding = query_embedding.is_some();

        // Candidate-vector backfill (only when the query itself embedded — a real
        // signal to rank by). The semantic layer scores the query against STORED
        // memory vectors, but agent-authored memories are written WITHOUT one (the
        // embedding is computed here, not at write) and a cold hydrate carries
        // none — so `memories_with_embeddings()` is empty and SemanticRecallLayer
        // no-ops, leaving recall in importance/recency order that IGNORES the query.
        // That is the exact "same off-topic memories for every query" bug the
        // agent-memory bridge hit (2026-07). Embed the missing vectors NOW — async,
        // content-addressed-cached, OUTSIDE the sync recall and outside any lock —
        // so the semantic layer has vectors to rank against. First recall per
        // persona pays it; the cache makes repeats free.
        if had_query_embedding {
            let embedded = self.ensure_memory_embeddings(&corpus_lock).await;
            if embedded > 0 {
                crate::log_info!(
                    "module",
                    "memory_recall",
                    "backfilled {embedded} candidate memory embeddings for {persona_id} (embedder={}) — semantic layer can now rank",
                    self.embedding.id()
                );
            }
        }

        // Phase 1: recall with read lock (pure sync compute on in-memory corpus)
        let (response, memories_with_vectors) = {
            let corpus = corpus_lock.read().map_err(|e| {
                MemoryError(format!("Failed to acquire read lock for {persona_id}: {e}"))
            })?;

            let query = RecallQuery {
                query_text: req.query_text.clone(),
                query_embedding,
                room_id: req.room_id.clone(),
                max_results_per_layer: (req.max_results / 2).max(5),
            };

            let resp = self
                .recall_engine
                .recall_parallel(&corpus, &query, req.max_results);
            let vec_count = corpus.memories_with_embeddings().len();
            (resp, vec_count)
        }; // read lock dropped here

        // Never silent ([[reliability-is-it-works-not-that-it-reports-failure-well]]): if a
        // query was given but the semantic layer could not contribute — the query embedding
        // was absent/from a lexical provider, or no memory carries a vector — then recall
        // degraded to NON-semantic order (the layers ignore query text). Say so LOUD so it
        // cannot hide as "working recall" (2026-07-26: this is the exact trap the agent-memory
        // bridge hit — off-topic results looked like working recall).
        if req.query_text.is_some() && (!had_query_embedding || memories_with_vectors == 0) {
            crate::log_info!(
                "module",
                "memory_recall",
                "⚠ SEMANTIC RECALL DEGRADED → non-semantic for {persona_id}: embedder={}, memories_with_vectors={memories_with_vectors}, query_embedded={had_query_embedding}. Results are NOT relevance-ranked — wire a neural embedder + populate memory vectors.",
                self.embedding.id()
            );
        }

        // Phase 2: mark accessed with write lock (testing effect — retrieval strengthens memory)
        if !response.memories.is_empty() {
            if let Ok(mut corpus) = corpus_lock.write() {
                let ids: Vec<String> = response.memories.iter().map(|m| m.id.clone()).collect();
                corpus.mark_accessed(&ids);
            }
            // If write lock fails, skip — access tracking is best-effort
        }

        Ok(response)
    }

    /// Ensure candidate memories carry an embedding so [`recall::SemanticRecallLayer`]
    /// can rank them. Snapshots `(id, content)` for every memory MISSING a vector
    /// (read lock), embeds each via the manager's embedder (async, content-addressed
    /// cache — OUTSIDE any lock, never held across the await), then writes the vectors
    /// back (write lock). Returns the number newly embedded.
    ///
    /// In-memory only: the durable store round-trips `CorpusMemory.embedding`, but
    /// agent memories are currently written with `None`, so this is the on-demand
    /// backfill that gives the semantic layer something to score until embed-on-write
    /// lands. A no-op once every memory has a vector (a cheap read-lock scan), so it
    /// is safe to call on every recall. Content is immutable, so a computed vector
    /// never goes stale.
    async fn ensure_memory_embeddings(
        &self,
        corpus_lock: &Arc<RwLock<MemoryCorpus>>,
    ) -> usize {
        // 1. Snapshot the memories that lack a vector (read lock, dropped before await).
        let missing: Vec<(String, String)> = {
            let corpus = match corpus_lock.read() {
                Ok(c) => c,
                Err(_) => return 0,
            };
            corpus
                .memories
                .iter()
                .filter(|m| !corpus.memory_embeddings.contains_key(&m.id))
                .map(|m| (m.id.clone(), m.content.clone()))
                .collect()
        };
        if missing.is_empty() {
            return 0;
        }

        // 2. Embed each missing memory's content (async, cached) — no lock held.
        //    An empty vector = "no signal" (embedder down / degenerate); skip it so
        //    the memory is retried on a later recall rather than cached as zeros.
        let mut embedded: Vec<(String, Vec<f32>)> = Vec::with_capacity(missing.len());
        for (id, content) in missing {
            let v = self.embedding.embed(&content).await;
            if !v.is_empty() {
                embedded.push((id, v));
            }
        }

        // 3. Write the vectors back (write lock).
        let n = embedded.len();
        if n > 0 {
            if let Ok(mut corpus) = corpus_lock.write() {
                for (id, v) in embedded {
                    corpus.memory_embeddings.insert(id, v);
                }
            }
        }
        n
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

        let corpus_lock = self.get_corpus(persona_id)?;
        let corpus = corpus_lock.read().map_err(|e| {
            MemoryError(format!("Failed to acquire read lock for {persona_id}: {e}"))
        })?;
        let response = build_consciousness_context(&corpus, req);

        // Cache the result
        self.consciousness_cache.set(cache_key, response.clone());

        Ok(response)
    }

    // ─── Incremental Append (In-Place Mutation) ─────────────────────────────

    /// Append a single memory to the persona's cached corpus.
    /// In-place mutation via write lock — O(1) amortized, zero cloning.
    pub fn append_memory(&self, persona_id: &str, memory: CorpusMemory) -> Result<(), MemoryError> {
        let corpus_lock = self.get_corpus(persona_id)?;
        let mut corpus = corpus_lock.write().map_err(|e| {
            MemoryError(format!(
                "Failed to acquire write lock for {persona_id}: {e}"
            ))
        })?;
        corpus.append_memory_mut(memory);
        // Trim if over capacity
        if corpus.memories.len() > MAX_MEMORIES_PER_CORPUS {
            let evicted = corpus.trim_memories(MAX_MEMORIES_PER_CORPUS);
            if evicted > 0 {
                eprintln!(
                    "🧠 MemoryManager: Trimmed {evicted} low-importance memories for {persona_id} (cap: {MAX_MEMORIES_PER_CORPUS})"
                );
            }
        }
        drop(corpus); // Release write lock before invalidating cache
        self.consciousness_cache.invalidate(persona_id);
        Ok(())
    }

    /// Append a single timeline event to the persona's cached corpus.
    /// In-place mutation via write lock — O(1) amortized, zero cloning.
    pub fn append_event(
        &self,
        persona_id: &str,
        event: CorpusTimelineEvent,
    ) -> Result<(), MemoryError> {
        let corpus_lock = self.get_corpus(persona_id)?;
        let mut corpus = corpus_lock.write().map_err(|e| {
            MemoryError(format!(
                "Failed to acquire write lock for {persona_id}: {e}"
            ))
        })?;
        corpus.append_event_mut(event);
        // Trim if over capacity
        if corpus.timeline_events.len() > MAX_EVENTS_PER_CORPUS {
            let evicted = corpus.trim_events(MAX_EVENTS_PER_CORPUS);
            if evicted > 0 {
                eprintln!(
                    "🧠 MemoryManager: Trimmed {evicted} old timeline events for {persona_id} (cap: {MAX_EVENTS_PER_CORPUS})"
                );
            }
        }
        drop(corpus); // Release write lock before invalidating cache
        self.consciousness_cache.invalidate(persona_id);
        Ok(())
    }

    // ─── Maintenance ──────────────────────────────────────────────────────────

    /// Evict expired cache entries and stale corpora (call periodically).
    pub fn evict_caches(&self) {
        self.consciousness_cache.evict_expired();

        // Evict stale corpora not accessed within TTL
        let now = Instant::now();
        let stale_personas: Vec<String> = self
            .corpus_access_times
            .iter()
            .filter(|entry| now.duration_since(*entry.value()) > CORPUS_STALE_TTL)
            .map(|entry| entry.key().clone())
            .collect();

        for persona_id in &stale_personas {
            self.corpora.remove(persona_id);
            self.corpus_access_times.remove(persona_id);
            eprintln!("🧠 MemoryManager: Evicted stale corpus for {persona_id}");
        }
    }

    /// Get memory usage stats for debugging.
    pub fn memory_stats(&self) -> Vec<(String, usize, usize, usize)> {
        self.corpora
            .iter()
            .map(|entry| {
                let persona_id = entry.key().clone();
                if let Ok(corpus) = entry.value().read() {
                    (
                        persona_id,
                        corpus.memories.len(),
                        corpus.timeline_events.len(),
                        corpus.approx_size_bytes(),
                    )
                } else {
                    (persona_id, 0, 0, 0)
                }
            })
            .collect()
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    use async_trait::async_trait;

    /// Stub embedding provider for tests (avoids a real model / network).
    /// Implements the canonical async [`EmbeddingProvider`].
    struct StubEmbeddingProvider;

    #[async_trait]
    impl EmbeddingProvider for StubEmbeddingProvider {
        fn id(&self) -> &str {
            "stub"
        }
        fn dim(&self) -> usize {
            384
        }
        async fn embed(&self, _text: &str) -> Vec<f32> {
            vec![0.1; 384]
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
        let events = vec![make_corpus_event("e1", "room-1", "General")];

        let resp = manager.load_corpus("p1", memories, events);
        assert_eq!(resp.memory_count, 2);
        assert_eq!(resp.embedded_memory_count, 2);
        assert_eq!(resp.timeline_event_count, 1);
        assert_eq!(resp.embedded_event_count, 0);
        assert!(resp.load_time_ms >= 0.0);
    }

    // what this catches: memories written WITHOUT a vector (the agent-memory
    // bridge's `remember` path sets embedding: None) get embedded on demand so
    // SemanticRecallLayer has something to rank against, instead of no-op'ing into
    // non-semantic importance/recency order — the exact "same off-topic memories
    // for every query" bug the bridge hit (2026-07). Idempotent once populated.
    #[tokio::test]
    async fn ensure_memory_embeddings_backfills_missing_vectors() {
        let manager = test_manager();
        let mut m_none1 = make_corpus_memory("m1", "alpha lesson", 0.5);
        m_none1.embedding = None;
        let mut m_none2 = make_corpus_memory("m2", "beta lesson", 0.5);
        m_none2.embedding = None;
        let m_has = make_corpus_memory("m3", "gamma lesson", 0.5); // already Some(vec)
        manager.load_corpus("p1", vec![m_none1, m_none2, m_has], vec![]);

        let corpus_lock = manager.get_corpus("p1").unwrap();
        // Precondition: only the pre-embedded memory carries a vector.
        assert_eq!(
            corpus_lock.read().unwrap().memories_with_embeddings().len(),
            1
        );

        // Backfill embeds the two missing (StubEmbeddingProvider yields a real vec).
        let n = manager.ensure_memory_embeddings(&corpus_lock).await;
        assert_eq!(n, 2, "both unembedded memories are backfilled");
        assert_eq!(
            corpus_lock.read().unwrap().memories_with_embeddings().len(),
            3,
            "every memory now carries a vector the semantic layer can rank"
        );

        // Idempotent: a second pass finds nothing missing (cheap read-lock scan).
        let n2 = manager.ensure_memory_embeddings(&corpus_lock).await;
        assert_eq!(n2, 0, "no re-embedding once every memory has a vector");
    }

    #[tokio::test]
    async fn test_multi_layer_recall() {
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

        let resp = manager.multi_layer_recall("p1", &req).await.unwrap();
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
        assert_eq!(
            resp2.cross_context_event_count,
            resp1.cross_context_event_count
        );
    }

    #[tokio::test]
    async fn test_corpus_not_loaded() {
        let manager = test_manager();
        let req = MultiLayerRecallRequest {
            query_text: None,
            room_id: "room-1".into(),
            max_results: 10,
            layers: None,
        };
        let result = manager.multi_layer_recall("nonexistent", &req).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_load_corpus_replaces_previous() {
        let manager = test_manager();

        // Load initial corpus with 1 memory
        manager.load_corpus("p1", vec![make_corpus_memory("m1", "first", 0.9)], vec![]);

        // Load new corpus with 3 memories
        let resp = manager.load_corpus(
            "p1",
            vec![
                make_corpus_memory("m2", "second", 0.8),
                make_corpus_memory("m3", "third", 0.7),
                make_corpus_memory("m4", "fourth", 0.6),
            ],
            vec![],
        );

        assert_eq!(resp.memory_count, 3);

        // Recall should find new memories, not old ones
        let req = MultiLayerRecallRequest {
            query_text: None,
            room_id: "room-1".into(),
            max_results: 10,
            layers: None,
        };
        let recall_resp = manager.multi_layer_recall("p1", &req).await.unwrap();
        assert!(recall_resp.memories.iter().all(|m| m.id != "m1"));
    }

    #[tokio::test]
    async fn test_append_memory() {
        let manager = test_manager();

        // Load initial corpus
        manager.load_corpus(
            "p1",
            vec![make_corpus_memory("m1", "Initial memory", 0.9)],
            vec![],
        );

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
        let resp = manager.multi_layer_recall("p1", &req).await.unwrap();
        let ids: Vec<&str> = resp.memories.iter().map(|m| m.id.as_str()).collect();
        assert!(ids.contains(&"m1"), "Original memory should still exist");
        assert!(ids.contains(&"m2"), "Appended memory should exist");
    }

    #[test]
    fn test_append_event() {
        let manager = test_manager();

        // Load initial corpus with one event
        manager.load_corpus(
            "p1",
            vec![],
            vec![make_corpus_event("e1", "room-1", "General")],
        );

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

    #[tokio::test]
    async fn test_append_preserves_embeddings() {
        let manager = test_manager();

        // Load initial corpus with embedded memory
        manager.load_corpus(
            "p1",
            vec![
                make_corpus_memory("m1", "with embedding", 0.9), // has Some(vec![0.1; 384])
            ],
            vec![],
        );

        // Append another embedded memory
        manager
            .append_memory("p1", make_corpus_memory("m2", "also embedded", 0.8))
            .unwrap();

        // Both should be findable via semantic recall (which needs embeddings)
        let req = MultiLayerRecallRequest {
            query_text: Some("embedded".into()),
            room_id: "room-1".into(),
            max_results: 10,
            layers: None,
        };
        let resp = manager.multi_layer_recall("p1", &req).await.unwrap();
        assert!(
            resp.memories.len() >= 2,
            "Both embedded memories should be recalled"
        );
    }
}
