//! Multi-layer recall algorithm — 6 pluggable layers running in parallel.
//!
//! Each layer is an independent retrieval strategy operating on in-memory data.
//! The orchestrator runs all layers in parallel via Rayon and merges results.
//!
//! Data comes from MemoryCorpus (loaded from TS ORM). Zero SQL access.
//!
//! Layers:
//! 1. Core — high-importance never-forget memories
//! 2. Semantic — embedding cosine similarity search
//! 3. Temporal — recent context with room bonus
//! 4. Associative — tag/keyword graph traversal
//! 5. DecayResurface — spaced repetition (surface fading memories)
//! 6. CrossContext — knowledge from other rooms/contexts

use crate::memory::corpus::MemoryCorpus;
use crate::memory::embedding::{cosine_similarity, EmbeddingProvider};
use crate::memory::types::*;
use std::collections::HashMap;
use std::time::Instant;

// ─── Trait: RecallLayer ──────────────────────────────────────────────────────

/// Pluggable recall layer — each implementation is an independent retrieval strategy.
///
/// All layers operate on MemoryCorpus (in-memory data from TS ORM).
/// No SQL, no filesystem access — pure computation.
pub trait RecallLayer: Send + Sync {
    /// Unique name for this layer (used in timing reports and convergence scoring).
    fn name(&self) -> &str;

    /// Execute this layer's recall strategy against the in-memory corpus.
    fn recall(
        &self,
        corpus: &MemoryCorpus,
        query: &RecallQuery,
        embedding_provider: &dyn EmbeddingProvider,
    ) -> Vec<ScoredMemory>;
}

// ─── Query Context ───────────────────────────────────────────────────────────

/// Context for a recall query — shared across all layers.
pub struct RecallQuery {
    pub query_text: Option<String>,
    pub query_embedding: Option<Vec<f32>>,
    pub room_id: String,
    pub max_results_per_layer: usize,
}

/// A memory candidate with a relevance score and source layer.
#[derive(Clone)]
pub struct ScoredMemory {
    pub memory: MemoryRecord,
    pub score: f64,
    pub layer: String,
}

// ─── Layer 1: Core Recall ────────────────────────────────────────────────────

/// High-importance memories that should never be forgotten.
/// Simple filter: importance >= 0.8, ordered by importance.
pub struct CoreRecallLayer;

impl RecallLayer for CoreRecallLayer {
    fn name(&self) -> &str {
        "core"
    }

    fn recall(
        &self,
        corpus: &MemoryCorpus,
        query: &RecallQuery,
        _embedding_provider: &dyn EmbeddingProvider,
    ) -> Vec<ScoredMemory> {
        corpus
            .high_importance_memories(0.8, query.max_results_per_layer)
            .into_iter()
            .map(|m| {
                let mut record = m.clone();
                record.layer = Some("core".into());
                ScoredMemory {
                    score: record.importance,
                    memory: record,
                    layer: "core".into(),
                }
            })
            .collect()
    }
}

// ─── Layer 2: Semantic Recall ────────────────────────────────────────────────

/// Embedding-based cosine similarity search.
/// Compares query embedding against all stored memory embeddings.
pub struct SemanticRecallLayer;

impl RecallLayer for SemanticRecallLayer {
    fn name(&self) -> &str {
        "semantic"
    }

    fn recall(
        &self,
        corpus: &MemoryCorpus,
        query: &RecallQuery,
        embedding_provider: &dyn EmbeddingProvider,
    ) -> Vec<ScoredMemory> {
        // Need query text or pre-computed embedding
        let query_embedding = match &query.query_embedding {
            Some(e) => e.clone(),
            None => match &query.query_text {
                Some(text) => match embedding_provider.embed(text) {
                    Ok(e) => e,
                    Err(_) => return vec![],
                },
                None => return vec![],
            },
        };

        let memories_with_embeddings = corpus.memories_with_embeddings();

        // Compute cosine similarity for each memory
        let mut scored: Vec<ScoredMemory> = memories_with_embeddings
            .into_iter()
            .map(|(record, embedding)| {
                let similarity = cosine_similarity(&query_embedding, embedding);
                let mut record = record.clone();
                record.layer = Some("semantic".into());
                record.relevance_score = Some(similarity as f64);
                ScoredMemory {
                    score: similarity as f64,
                    memory: record,
                    layer: "semantic".into(),
                }
            })
            .collect();

        // Sort by similarity descending and take top N
        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(query.max_results_per_layer);
        scored
    }
}

// ─── Layer 3: Temporal Recall ────────────────────────────────────────────────

/// Recent memories — "what was I just thinking about?"
/// Fetches memories from the last 2 hours, with a room-context bonus.
pub struct TemporalRecallLayer;

impl RecallLayer for TemporalRecallLayer {
    fn name(&self) -> &str {
        "temporal"
    }

    fn recall(
        &self,
        corpus: &MemoryCorpus,
        query: &RecallQuery,
        _embedding_provider: &dyn EmbeddingProvider,
    ) -> Vec<ScoredMemory> {
        // Look back 2 hours
        let since = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::hours(2))
            .map(|t| t.to_rfc3339())
            .unwrap_or_default();

        let recent = corpus.recent_memories(&since, query.max_results_per_layer * 2);

        recent
            .into_iter()
            .enumerate()
            .map(|(i, m)| {
                // Recency score: most recent = highest score
                let recency_score =
                    1.0 - (i as f64 / (query.max_results_per_layer * 2) as f64);

                // Room bonus: memories from the same room get a 20% boost
                let room_bonus = if m
                    .context
                    .get("roomId")
                    .and_then(|v| v.as_str())
                    .map(|r| r == query.room_id)
                    .unwrap_or(false)
                {
                    0.2
                } else {
                    0.0
                };

                let score = recency_score * 0.7 + m.importance * 0.3 + room_bonus;

                let mut record = m.clone();
                record.layer = Some("temporal".into());
                ScoredMemory {
                    score,
                    memory: record,
                    layer: "temporal".into(),
                }
            })
            .take(query.max_results_per_layer)
            .collect()
    }
}

// ─── Layer 4: Associative Recall ─────────────────────────────────────────────

/// Tag-based and relatedTo graph traversal.
/// Extracts keywords from query text, matches against memory tags,
/// then follows relatedTo links for one hop.
pub struct AssociativeRecallLayer;

impl AssociativeRecallLayer {
    /// Simple keyword extraction: split by whitespace, filter stopwords + short words.
    pub fn extract_keywords(text: &str) -> Vec<String> {
        const STOPWORDS: &[&str] = &[
            "the", "a", "an", "is", "are", "was", "were", "be", "been", "being",
            "have", "has", "had", "do", "does", "did", "will", "would", "could",
            "should", "may", "might", "can", "shall", "to", "of", "in", "for", "on",
            "with", "at", "by", "from", "as", "into", "about", "like", "through",
            "after", "over", "between", "out", "up", "down", "this", "that", "these",
            "those", "it", "its", "i", "me", "my", "we", "our", "you", "your", "he",
            "she", "they", "them", "what", "which", "who", "when", "where", "how",
            "not", "no", "nor", "but", "and", "or", "if", "then", "so", "too",
            "very", "just", "don", "now", "here", "there",
        ];

        text.to_lowercase()
            .split_whitespace()
            .filter(|w| w.len() >= 3 && !STOPWORDS.contains(w))
            .map(|w| {
                w.trim_matches(|c: char| !c.is_alphanumeric())
                    .to_string()
            })
            .filter(|w| !w.is_empty())
            .collect()
    }
}

impl RecallLayer for AssociativeRecallLayer {
    fn name(&self) -> &str {
        "associative"
    }

    fn recall(
        &self,
        corpus: &MemoryCorpus,
        query: &RecallQuery,
        _embedding_provider: &dyn EmbeddingProvider,
    ) -> Vec<ScoredMemory> {
        let query_text = match &query.query_text {
            Some(t) => t.clone(),
            None => return vec![],
        };

        let keywords = Self::extract_keywords(&query_text);
        if keywords.is_empty() {
            return vec![];
        }

        let memories = corpus.all_memories_limited(200);

        // Score each memory by keyword-tag overlap
        let mut scored: Vec<ScoredMemory> = memories
            .into_iter()
            .filter_map(|m| {
                let tag_matches = keywords
                    .iter()
                    .filter(|kw| {
                        m.tags
                            .iter()
                            .any(|tag| tag.to_lowercase().contains(kw.as_str()))
                            || m.content.to_lowercase().contains(kw.as_str())
                    })
                    .count();

                if tag_matches == 0 {
                    return None;
                }

                let score =
                    (tag_matches as f64 / keywords.len() as f64) * 0.7 + m.importance * 0.3;

                let mut record = m.clone();
                record.layer = Some("associative".into());
                Some(ScoredMemory {
                    score,
                    memory: record,
                    layer: "associative".into(),
                })
            })
            .collect();

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(query.max_results_per_layer);

        // Follow relatedTo links (1 hop) for top results
        let related_ids: Vec<String> = scored
            .iter()
            .flat_map(|s| s.memory.related_to.clone())
            .collect();

        if !related_ids.is_empty() {
            for m in &corpus.memories {
                if related_ids.contains(&m.id) {
                    let already_has = scored.iter().any(|s| s.memory.id == m.id);
                    if !already_has {
                        let mut record = m.clone();
                        record.layer = Some("associative".into());
                        scored.push(ScoredMemory {
                            score: m.importance * 0.5, // Related memories get dampened score
                            memory: record,
                            layer: "associative".into(),
                        });
                    }
                }
            }
        }

        scored.truncate(query.max_results_per_layer);
        scored
    }
}

// ─── Layer 5: Decay Resurface ────────────────────────────────────────────────

/// Spaced repetition — surface important memories that are fading.
/// Higher score = more in need of resurfacing.
/// decay_score = days_since_access / (access_count + 1)
/// Only considers memories with importance >= 0.5.
pub struct DecayResurfaceLayer;

impl RecallLayer for DecayResurfaceLayer {
    fn name(&self) -> &str {
        "decay_resurface"
    }

    fn recall(
        &self,
        corpus: &MemoryCorpus,
        query: &RecallQuery,
        _embedding_provider: &dyn EmbeddingProvider,
    ) -> Vec<ScoredMemory> {
        let decayable = corpus.decayable_memories(0.5, 100);

        let mut scored: Vec<ScoredMemory> = decayable
            .into_iter()
            .map(|(m, days_since_access)| {
                // Decay score: high = needs resurfacing
                let decay = days_since_access / (m.access_count as f64 + 1.0);
                let score = decay.min(1.0) * m.importance;

                let mut record = m.clone();
                record.layer = Some("decay_resurface".into());
                ScoredMemory {
                    score,
                    memory: record,
                    layer: "decay_resurface".into(),
                }
            })
            .collect();

        scored.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        scored.truncate(query.max_results_per_layer);
        scored
    }
}

// ─── Layer 6: Cross-Context Recall ───────────────────────────────────────────

/// Knowledge from other rooms/contexts — cross-pollination.
/// Uses timeline events from other contexts, optionally with semantic relevance.
pub struct CrossContextLayer;

impl RecallLayer for CrossContextLayer {
    fn name(&self) -> &str {
        "cross_context"
    }

    fn recall(
        &self,
        corpus: &MemoryCorpus,
        query: &RecallQuery,
        embedding_provider: &dyn EmbeddingProvider,
    ) -> Vec<ScoredMemory> {
        // Look back 24 hours for cross-context events
        let since = chrono::Utc::now()
            .checked_sub_signed(chrono::Duration::hours(24))
            .map(|t| t.to_rfc3339())
            .unwrap_or_default();

        // If we have query text, do semantic cross-context search
        if let Some(ref query_text) = query.query_text {
            if let Ok(query_emb) = embedding_provider.embed(query_text) {
                let events_with_emb = corpus.cross_context_events_with_embeddings(
                    &query.room_id,
                    &since,
                    50,
                );

                let mut scored: Vec<ScoredMemory> = events_with_emb
                    .into_iter()
                    .map(|(event, embedding)| {
                        let similarity = cosine_similarity(&query_emb, embedding);
                        let record = timeline_event_to_memory_record(
                            event,
                            "cross_context",
                            Some(similarity as f64),
                        );
                        ScoredMemory {
                            score: similarity as f64 * 0.7 + event.importance * 0.3,
                            memory: record,
                            layer: "cross_context".into(),
                        }
                    })
                    .collect();

                scored.sort_by(|a, b| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                });
                scored.truncate(query.max_results_per_layer);
                return scored;
            }
        }

        // Fallback: importance-based cross-context (no semantic search)
        let events =
            corpus.cross_context_events(&query.room_id, &since, query.max_results_per_layer);

        events
            .into_iter()
            .map(|event| {
                let record = timeline_event_to_memory_record(event, "cross_context", None);
                ScoredMemory {
                    score: event.importance,
                    memory: record,
                    layer: "cross_context".into(),
                }
            })
            .collect()
    }
}

// ─── Helper: Timeline → Memory ───────────────────────────────────────────────

/// Convert a TimelineEvent to a MemoryRecord for uniform recall output.
fn timeline_event_to_memory_record(
    event: &TimelineEvent,
    layer: &str,
    relevance_score: Option<f64>,
) -> MemoryRecord {
    MemoryRecord {
        id: event.id.clone(),
        persona_id: event.persona_id.clone(),
        memory_type: format!("timeline:{}", event.event_type),
        content: event.content.clone(),
        context: serde_json::json!({
            "context_type": event.context_type,
            "context_id": event.context_id,
            "context_name": event.context_name,
            "actor_id": event.actor_id,
            "actor_name": event.actor_name,
        }),
        timestamp: event.timestamp.clone(),
        importance: event.importance,
        access_count: 0,
        tags: event.topics.clone(),
        related_to: vec![],
        source: Some("timeline".into()),
        last_accessed_at: None,
        layer: Some(layer.into()),
        relevance_score,
    }
}

// ─── MultiLayerRecall Orchestrator ───────────────────────────────────────────

/// Orchestrates all recall layers in parallel, merges and deduplicates results.
///
/// All layers run on Rayon threads, operating on in-memory MemoryCorpus data.
/// No SQL, no filesystem — pure parallel computation.
pub struct MultiLayerRecall {
    layers: Vec<Box<dyn RecallLayer>>,
}

impl MultiLayerRecall {
    /// Create with all 6 default layers.
    pub fn new() -> Self {
        Self {
            layers: vec![
                Box::new(CoreRecallLayer),
                Box::new(SemanticRecallLayer),
                Box::new(TemporalRecallLayer),
                Box::new(AssociativeRecallLayer),
                Box::new(DecayResurfaceLayer),
                Box::new(CrossContextLayer),
            ],
        }
    }

    /// Run all layers in parallel and return merged, deduplicated results.
    pub fn recall_parallel(
        &self,
        corpus: &MemoryCorpus,
        query: &RecallQuery,
        embedding_provider: &dyn EmbeddingProvider,
        max_results: usize,
    ) -> MemoryRecallResponse {
        let start = Instant::now();

        // Determine which layers to run
        let active_layers: Vec<&Box<dyn RecallLayer>> = match &query.query_text {
            Some(_) => self.layers.iter().collect(), // All layers when text available
            None => self
                .layers
                .iter()
                .filter(|l| l.name() != "semantic" && l.name() != "associative")
                .collect(),
        };

        // Run all active layers sequentially to avoid Rayon thread starvation
        // (IPC dispatch uses Rayon threads that block waiting for these results)
        let layer_results: Vec<(String, Vec<ScoredMemory>, f64)> = active_layers
            .iter()
            .map(|layer| {
                let layer_start = Instant::now();
                let results = layer.recall(corpus, query, embedding_provider);
                let time_ms = layer_start.elapsed().as_secs_f64() * 1000.0;
                (layer.name().to_string(), results, time_ms)
            })
            .collect();

        // Collect layer timings
        let layer_timings: Vec<LayerTiming> = layer_results
            .iter()
            .map(|(name, results, time_ms)| LayerTiming {
                layer: name.clone(),
                time_ms: *time_ms,
                results_found: results.len(),
            })
            .collect();

        let total_candidates: usize = layer_results.iter().map(|(_, r, _)| r.len()).sum();

        // Merge and deduplicate by memory ID
        // Memories found by multiple layers get a convergence boost
        let mut merged: HashMap<String, (ScoredMemory, usize)> = HashMap::new();

        for (_, results, _) in &layer_results {
            for scored in results {
                let entry = merged
                    .entry(scored.memory.id.clone())
                    .or_insert_with(|| (scored.clone(), 0));
                entry.0.score = entry.0.score.max(scored.score);
                entry.1 += 1; // Count layers that found this memory
            }
        }

        // Apply convergence boost: memories from multiple layers get bonus.
        // No score cap — scores > 1.0 are valid for ranking (these are relative
        // rankings, not probabilities). Capping destroys the convergence signal
        // when high-scoring memories from different layers all hit 1.0.
        let mut final_results: Vec<ScoredMemory> = merged
            .into_values()
            .map(|(mut scored, layer_count)| {
                if layer_count > 1 {
                    // 15% boost per additional layer
                    scored.score *= 1.0 + 0.15 * (layer_count - 1) as f64;
                }
                scored
            })
            .collect();

        // Sort by final score descending, then importance as tiebreaker
        final_results.sort_by(|a, b| {
            b.score
                .partial_cmp(&a.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    b.memory
                        .importance
                        .partial_cmp(&a.memory.importance)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        final_results.truncate(max_results);

        let recall_time_ms = start.elapsed().as_secs_f64() * 1000.0;

        // Convert to MemoryRecallResponse
        let memories: Vec<MemoryRecord> = final_results
            .into_iter()
            .map(|s| {
                let mut m = s.memory;
                m.relevance_score = Some(s.score);
                m
            })
            .collect();

        MemoryRecallResponse {
            memories,
            recall_time_ms,
            layer_timings,
            total_candidates,
        }
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_keyword_extraction() {
        let keywords = AssociativeRecallLayer::extract_keywords(
            "What did we discuss about the blue sky yesterday?",
        );
        assert!(keywords.contains(&"discuss".to_string()));
        assert!(keywords.contains(&"blue".to_string()));
        assert!(keywords.contains(&"sky".to_string()));
        assert!(keywords.contains(&"yesterday".to_string()));
        // Stopwords should be filtered
        assert!(!keywords.contains(&"the".to_string()));
        assert!(!keywords.contains(&"did".to_string()));
        assert!(!keywords.contains(&"about".to_string()));
    }

    #[test]
    fn test_keyword_extraction_empty() {
        let keywords = AssociativeRecallLayer::extract_keywords("the a is");
        assert!(keywords.is_empty());
    }

    #[test]
    fn test_timeline_event_to_memory_record() {
        let event = TimelineEvent {
            id: "ev-1".into(),
            persona_id: "p-1".into(),
            timestamp: "2025-01-01T00:00:00Z".into(),
            context_type: "room".into(),
            context_id: "room-2".into(),
            context_name: "Academy".into(),
            event_type: "message_sent".into(),
            actor_id: "user-1".into(),
            actor_name: "Joel".into(),
            content: "Teaching something".into(),
            importance: 0.8,
            topics: vec!["teaching".into()],
        };

        let record = timeline_event_to_memory_record(&event, "cross_context", Some(0.95));
        assert_eq!(record.id, "ev-1");
        assert_eq!(record.memory_type, "timeline:message_sent");
        assert_eq!(record.layer, Some("cross_context".into()));
        assert_eq!(record.relevance_score, Some(0.95));
        assert_eq!(record.tags, vec!["teaching"]);
    }

    #[test]
    fn test_multi_layer_recall_creation() {
        let recall = MultiLayerRecall::new();
        assert_eq!(recall.layers.len(), 6);
        assert_eq!(recall.layers[0].name(), "core");
        assert_eq!(recall.layers[1].name(), "semantic");
        assert_eq!(recall.layers[2].name(), "temporal");
        assert_eq!(recall.layers[3].name(), "associative");
        assert_eq!(recall.layers[4].name(), "decay_resurface");
        assert_eq!(recall.layers[5].name(), "cross_context");
    }
}
