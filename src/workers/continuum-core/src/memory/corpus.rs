//! MemoryCorpus — in-memory data container for ORM-provided memory data.
//!
//! Rust is a pure compute engine. Data comes from the TS ORM via IPC.
//! The corpus is cached per-persona and refreshed on demand.
//!
//! Architecture: TS ORM queries data → sends to Rust via memory/load-corpus
//! → Rust caches MemoryCorpus per persona → recall layers operate on corpus.
//! Zero SQL. Zero filesystem access. Pure computation.

use crate::memory::types::*;
use std::collections::HashMap;
use std::time::Instant;

// ─── MemoryCorpus ─────────────────────────────────────────────────────────────

/// In-memory corpus of a persona's memories and timeline events.
/// Loaded from the TS ORM via IPC, cached per-persona.
///
/// All recall layers and consciousness builders operate on this data.
/// No SQL, no filesystem access — pure compute on in-memory data.
pub struct MemoryCorpus {
    pub memories: Vec<MemoryRecord>,
    pub memory_embeddings: HashMap<String, Vec<f32>>,
    pub timeline_events: Vec<TimelineEvent>,
    pub event_embeddings: HashMap<String, Vec<f32>>,
    pub loaded_at: Instant,
}

impl MemoryCorpus {
    /// Create a corpus from ORM-provided data.
    pub fn new(
        memories: Vec<MemoryRecord>,
        memory_embeddings: HashMap<String, Vec<f32>>,
        timeline_events: Vec<TimelineEvent>,
        event_embeddings: HashMap<String, Vec<f32>>,
    ) -> Self {
        Self {
            memories,
            memory_embeddings,
            timeline_events,
            event_embeddings,
            loaded_at: Instant::now(),
        }
    }

    /// Create from corpus load request (CorpusMemory/CorpusTimelineEvent wrappers).
    pub fn from_corpus_data(
        corpus_memories: Vec<CorpusMemory>,
        corpus_events: Vec<CorpusTimelineEvent>,
    ) -> Self {
        let mut memories = Vec::with_capacity(corpus_memories.len());
        let mut memory_embeddings = HashMap::with_capacity(corpus_memories.len());

        for cm in corpus_memories {
            if let Some(emb) = cm.embedding {
                memory_embeddings.insert(cm.record.id.clone(), emb);
            }
            memories.push(cm.record);
        }

        let mut timeline_events = Vec::with_capacity(corpus_events.len());
        let mut event_embeddings = HashMap::with_capacity(corpus_events.len());

        for ce in corpus_events {
            if let Some(emb) = ce.embedding {
                event_embeddings.insert(ce.event.id.clone(), emb);
            }
            timeline_events.push(ce.event);
        }

        Self {
            memories,
            memory_embeddings,
            timeline_events,
            event_embeddings,
            loaded_at: Instant::now(),
        }
    }

    /// Empty corpus (no data loaded yet).
    pub fn empty() -> Self {
        Self {
            memories: vec![],
            memory_embeddings: HashMap::new(),
            timeline_events: vec![],
            event_embeddings: HashMap::new(),
            loaded_at: Instant::now(),
        }
    }

    // ─── Memory Queries (used by recall layers) ─────────────────────────────

    /// All memories, sorted by importance DESC then timestamp DESC.
    pub fn all_memories_sorted(&self) -> Vec<&MemoryRecord> {
        let mut result: Vec<&MemoryRecord> = self.memories.iter().collect();
        result.sort_by(|a, b| {
            b.importance
                .partial_cmp(&a.importance)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.timestamp.cmp(&a.timestamp))
        });
        result
    }

    /// Memories with importance >= threshold, sorted by importance DESC.
    pub fn high_importance_memories(&self, threshold: f64, limit: usize) -> Vec<&MemoryRecord> {
        let mut result: Vec<&MemoryRecord> = self
            .memories
            .iter()
            .filter(|m| m.importance >= threshold)
            .collect();
        result.sort_by(|a, b| {
            b.importance
                .partial_cmp(&a.importance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        result.truncate(limit);
        result
    }

    /// Memories with their embeddings (for semantic search).
    pub fn memories_with_embeddings(&self) -> Vec<(&MemoryRecord, &[f32])> {
        self.memories
            .iter()
            .filter_map(|m| {
                self.memory_embeddings
                    .get(&m.id)
                    .map(|e| (m, e.as_slice()))
            })
            .collect()
    }

    /// Recent memories (timestamp >= since), sorted by timestamp DESC.
    pub fn recent_memories(&self, since: &str, limit: usize) -> Vec<&MemoryRecord> {
        let mut result: Vec<&MemoryRecord> = self
            .memories
            .iter()
            .filter(|m| m.timestamp.as_str() >= since)
            .collect();
        result.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        result.truncate(limit);
        result
    }

    /// Memories eligible for decay resurfacing (importance >= threshold).
    /// Returns (memory, days_since_access).
    pub fn decayable_memories(
        &self,
        min_importance: f64,
        limit: usize,
    ) -> Vec<(&MemoryRecord, f64)> {
        let now = chrono::Utc::now();
        let mut result: Vec<(&MemoryRecord, f64)> = self
            .memories
            .iter()
            .filter(|m| m.importance >= min_importance)
            .map(|m| {
                let access_time = m.last_accessed_at.as_deref().unwrap_or(&m.timestamp);
                let days = chrono::DateTime::parse_from_rfc3339(access_time)
                    .map(|t| (now - t.with_timezone(&chrono::Utc)).num_hours() as f64 / 24.0)
                    .unwrap_or(0.0);
                (m, days.max(0.0))
            })
            .collect();
        result.sort_by(|a, b| {
            b.0.importance
                .partial_cmp(&a.0.importance)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        result.truncate(limit);
        result
    }

    /// All memories up to a limit (sorted by importance DESC), for tag/content matching.
    pub fn all_memories_limited(&self, limit: usize) -> Vec<&MemoryRecord> {
        let mut result = self.all_memories_sorted();
        result.truncate(limit);
        result
    }

    // ─── Timeline Queries (used by consciousness/cross-context) ─────────────

    /// Timeline events NOT in the specified context, sorted by importance DESC.
    pub fn cross_context_events(
        &self,
        exclude_context_id: &str,
        since: &str,
        limit: usize,
    ) -> Vec<&TimelineEvent> {
        let mut result: Vec<&TimelineEvent> = self
            .timeline_events
            .iter()
            .filter(|e| e.context_id != exclude_context_id && e.timestamp.as_str() >= since)
            .collect();
        result.sort_by(|a, b| {
            b.importance
                .partial_cmp(&a.importance)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| b.timestamp.cmp(&a.timestamp))
        });
        result.truncate(limit);
        result
    }

    /// Cross-context timeline events with their embeddings.
    pub fn cross_context_events_with_embeddings(
        &self,
        exclude_context_id: &str,
        since: &str,
        limit: usize,
    ) -> Vec<(&TimelineEvent, &[f32])> {
        let mut result: Vec<(&TimelineEvent, &[f32])> = self
            .timeline_events
            .iter()
            .filter(|e| e.context_id != exclude_context_id && e.timestamp.as_str() >= since)
            .filter_map(|e| {
                self.event_embeddings
                    .get(&e.id)
                    .map(|emb| (e, emb.as_slice()))
            })
            .collect();
        result.sort_by(|a, b| b.0.timestamp.cmp(&a.0.timestamp));
        result.truncate(limit);
        result
    }

    /// Most recent event in a specific context.
    pub fn last_event_in_context(&self, context_id: &str) -> Option<&TimelineEvent> {
        self.timeline_events
            .iter()
            .filter(|e| e.context_id == context_id)
            .max_by(|a, b| a.timestamp.cmp(&b.timestamp))
    }

    // ─── Copy-on-Write Append ─────────────────────────────────────────────

    /// Create a new corpus with an additional memory appended.
    /// Copy-on-write: clones all data, pushes new memory, returns new corpus.
    /// O(n) but appends are rare (~1/min/persona). Readers on old Arc unaffected.
    pub fn with_appended_memory(&self, corpus_memory: CorpusMemory) -> Self {
        let mut memories = self.memories.clone();
        let mut memory_embeddings = self.memory_embeddings.clone();

        if let Some(emb) = corpus_memory.embedding {
            memory_embeddings.insert(corpus_memory.record.id.clone(), emb);
        }
        memories.push(corpus_memory.record);

        Self {
            memories,
            memory_embeddings,
            timeline_events: self.timeline_events.clone(),
            event_embeddings: self.event_embeddings.clone(),
            loaded_at: self.loaded_at,
        }
    }

    /// Create a new corpus with an additional timeline event appended.
    /// Copy-on-write: clones all data, pushes new event, returns new corpus.
    pub fn with_appended_event(&self, corpus_event: CorpusTimelineEvent) -> Self {
        let mut timeline_events = self.timeline_events.clone();
        let mut event_embeddings = self.event_embeddings.clone();

        if let Some(emb) = corpus_event.embedding {
            event_embeddings.insert(corpus_event.event.id.clone(), emb);
        }
        timeline_events.push(corpus_event.event);

        Self {
            memories: self.memories.clone(),
            memory_embeddings: self.memory_embeddings.clone(),
            timeline_events,
            event_embeddings,
            loaded_at: self.loaded_at,
        }
    }

    // ─── Timeline Queries (continued) ────────────────────────────────────

    /// All timeline events within a time range, sorted by timestamp DESC.
    pub fn events_since(&self, since: &str, limit: usize) -> Vec<&TimelineEvent> {
        let mut result: Vec<&TimelineEvent> = self
            .timeline_events
            .iter()
            .filter(|e| e.timestamp.as_str() >= since)
            .collect();
        result.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        result.truncate(limit);
        result
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn make_memory(id: &str, content: &str, importance: f64, timestamp: &str) -> MemoryRecord {
        MemoryRecord {
            id: id.into(),
            persona_id: "test".into(),
            memory_type: "observation".into(),
            content: content.into(),
            context: serde_json::json!({}),
            timestamp: timestamp.into(),
            importance,
            access_count: 0,
            tags: vec![],
            related_to: vec![],
            source: None,
            last_accessed_at: None,
            layer: None,
            relevance_score: None,
        }
    }

    fn make_event(
        id: &str,
        context_id: &str,
        context_name: &str,
        timestamp: &str,
        importance: f64,
    ) -> TimelineEvent {
        TimelineEvent {
            id: id.into(),
            persona_id: "test".into(),
            timestamp: timestamp.into(),
            context_type: "room".into(),
            context_id: context_id.into(),
            context_name: context_name.into(),
            event_type: "message".into(),
            actor_id: "user1".into(),
            actor_name: "User".into(),
            content: "test content".into(),
            importance,
            topics: vec![],
        }
    }

    #[test]
    fn test_empty_corpus() {
        let corpus = MemoryCorpus::empty();
        assert!(corpus.memories.is_empty());
        assert!(corpus.timeline_events.is_empty());
    }

    #[test]
    fn test_high_importance() {
        let corpus = MemoryCorpus::new(
            vec![
                make_memory("m1", "low", 0.3, "2025-01-01T00:00:00Z"),
                make_memory("m2", "high", 0.9, "2025-01-01T00:00:00Z"),
                make_memory("m3", "medium", 0.6, "2025-01-01T00:00:00Z"),
            ],
            HashMap::new(),
            vec![],
            HashMap::new(),
        );

        let high = corpus.high_importance_memories(0.8, 10);
        assert_eq!(high.len(), 1);
        assert_eq!(high[0].id, "m2");
    }

    #[test]
    fn test_cross_context_events() {
        let corpus = MemoryCorpus::new(
            vec![],
            HashMap::new(),
            vec![
                make_event("e1", "room-1", "General", "2025-01-01T12:00:00Z", 0.7),
                make_event("e2", "room-2", "Academy", "2025-01-01T13:00:00Z", 0.8),
                make_event("e3", "room-1", "General", "2025-01-01T14:00:00Z", 0.6),
            ],
            HashMap::new(),
        );

        let events = corpus.cross_context_events("room-1", "2025-01-01T00:00:00Z", 10);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].context_id, "room-2");
    }

    #[test]
    fn test_from_corpus_data() {
        let memories = vec![CorpusMemory {
            record: make_memory("m1", "test", 0.5, "2025-01-01T00:00:00Z"),
            embedding: Some(vec![0.1, 0.2, 0.3]),
        }];
        let events = vec![CorpusTimelineEvent {
            event: make_event("e1", "room-1", "General", "2025-01-01T00:00:00Z", 0.6),
            embedding: Some(vec![0.4, 0.5, 0.6]),
        }];

        let corpus = MemoryCorpus::from_corpus_data(memories, events);
        assert_eq!(corpus.memories.len(), 1);
        assert_eq!(corpus.memory_embeddings.len(), 1);
        assert_eq!(corpus.timeline_events.len(), 1);
        assert_eq!(corpus.event_embeddings.len(), 1);
    }

    #[test]
    fn test_last_event_in_context() {
        let corpus = MemoryCorpus::new(
            vec![],
            HashMap::new(),
            vec![
                make_event("e1", "room-1", "General", "2025-01-01T12:00:00Z", 0.7),
                make_event("e2", "room-1", "General", "2025-01-01T14:00:00Z", 0.6),
                make_event("e3", "room-2", "Academy", "2025-01-01T13:00:00Z", 0.8),
            ],
            HashMap::new(),
        );

        let last = corpus.last_event_in_context("room-1");
        assert!(last.is_some());
        assert_eq!(last.unwrap().id, "e2");

        let none = corpus.last_event_in_context("room-nonexistent");
        assert!(none.is_none());
    }
}
