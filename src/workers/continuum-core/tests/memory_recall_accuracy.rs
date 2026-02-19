//! Memory Recall Accuracy Tests — Corpus-based verification.
//!
//! Like TTS->STT roundtrip tests, these verify that the RIGHT memories surface
//! for the right queries, and irrelevant ones DON'T.
//!
//! Architecture: Rust is a pure compute engine. Data comes from the TS ORM.
//! Tests build MemoryCorpus directly with pre-computed embeddings from
//! DeterministicEmbeddingProvider — no SQL, no filesystem, no temp directories.
//!
//! Test dataset: A persona's thought chain about learning Rust,
//! with explicit tags forming an associative graph.

use continuum_core::memory::{
    CorpusMemory, CorpusTimelineEvent, DeterministicEmbeddingProvider, EmbeddingProvider,
    MemoryCorpus, MemoryRecord, MultiLayerRecallRequest, PersonaMemoryManager, TimelineEvent,
};
use continuum_core::memory::recall::{
    AssociativeRecallLayer, CoreRecallLayer, CrossContextLayer, DecayResurfaceLayer,
    RecallLayer, RecallQuery, SemanticRecallLayer, TemporalRecallLayer,
};
use std::sync::Arc;

// ─── Test Harness ─────────────────────────────────────────────────────────────

const PERSONA_ID: &str = "test-persona-recall";

/// Create a test manager with deterministic embeddings.
fn test_manager() -> PersonaMemoryManager {
    PersonaMemoryManager::new(Arc::new(DeterministicEmbeddingProvider))
}

/// Create a CorpusMemory with a deterministic embedding.
fn make_memory(
    id: &str,
    content: &str,
    importance: f64,
    memory_type: &str,
    timestamp: &str,
    tags: Vec<String>,
    room_id: &str,
) -> CorpusMemory {
    let provider = DeterministicEmbeddingProvider;
    let embedding = provider.embed(content).ok();

    CorpusMemory {
        record: MemoryRecord {
            id: id.into(),
            persona_id: PERSONA_ID.into(),
            memory_type: memory_type.into(),
            content: content.into(),
            context: serde_json::json!({"roomId": room_id}),
            timestamp: timestamp.into(),
            importance,
            access_count: 0,
            tags,
            related_to: vec![],
            source: Some("chat".into()),
            last_accessed_at: None,
            layer: None,
            relevance_score: None,
        },
        embedding,
    }
}

/// Seed the standard thought-chain dataset.
/// Returns (memories, IDs) in order: [rust_basics, borrow_checker, lifetimes, concurrency, tokio, cooking]
fn build_thought_chain() -> Vec<CorpusMemory> {
    let now = chrono::Utc::now();

    vec![
        // 0: Foundation — high importance core memory
        make_memory(
            "m-rust-basics",
            "Rust memory safety is achieved through ownership and borrowing rules enforced at compile time",
            0.95,
            "insight",
            &(now - chrono::Duration::minutes(60)).to_rfc3339(),
            vec!["rust".into(), "memory-safety".into(), "ownership".into()],
            "room-academy",
        ),
        // 1: Builds on ownership -> borrow checker
        make_memory(
            "m-borrow-checker",
            "The borrow checker prevents data races by ensuring only one mutable reference exists at a time",
            0.85,
            "observation",
            &(now - chrono::Duration::minutes(55)).to_rfc3339(),
            vec!["rust".into(), "borrow-checker".into(), "concurrency".into()],
            "room-academy",
        ),
        // 2: Builds on borrowing -> lifetimes
        make_memory(
            "m-lifetimes",
            "Lifetimes are annotations that tell the compiler how long references are valid",
            0.7,
            "observation",
            &(now - chrono::Duration::minutes(50)).to_rfc3339(),
            vec!["rust".into(), "lifetimes".into(), "references".into()],
            "room-academy",
        ),
        // 3: Concurrency — connected to borrow checker
        make_memory(
            "m-concurrency",
            "Rust fearless concurrency means the type system prevents data races at compile time",
            0.8,
            "insight",
            &(now - chrono::Duration::minutes(30)).to_rfc3339(),
            vec!["rust".into(), "concurrency".into(), "type-system".into()],
            "room-general",
        ),
        // 4: Tokio — recent, related to concurrency
        make_memory(
            "m-tokio",
            "Tokio async runtime enables concurrent network IO without threads, using Rust futures and await",
            0.65,
            "observation",
            &(now - chrono::Duration::minutes(5)).to_rfc3339(),
            vec!["rust".into(), "tokio".into(), "async".into(), "concurrency".into()],
            "room-general",
        ),
        // 5: UNRELATED — cooking (should NOT surface for Rust queries)
        make_memory(
            "m-cooking",
            "The best pasta is cooked al dente in salted boiling water for eight minutes",
            0.4,
            "observation",
            &(now - chrono::Duration::hours(12)).to_rfc3339(),
            vec!["cooking".into(), "pasta".into(), "italian".into()],
            "room-kitchen",
        ),
    ]
}

/// Build timeline events for cross-context testing.
fn build_timeline_events() -> Vec<CorpusTimelineEvent> {
    let now = chrono::Utc::now();
    let provider = DeterministicEmbeddingProvider;

    vec![
        CorpusTimelineEvent {
            event: TimelineEvent {
                id: "ev-academy".into(),
                persona_id: PERSONA_ID.into(),
                timestamp: (now - chrono::Duration::minutes(20)).to_rfc3339(),
                context_type: "room".into(),
                context_id: "room-academy".into(),
                context_name: "Academy".into(),
                event_type: "discussion".into(),
                actor_id: "joel".into(),
                actor_name: "Joel".into(),
                content: "Deep discussion about Rust ownership patterns and memory management strategies".into(),
                importance: 0.8,
                topics: vec!["rust".into(), "ownership".into(), "architecture".into()],
            },
            embedding: provider
                .embed("Deep discussion about Rust ownership patterns and memory management strategies")
                .ok(),
        },
        CorpusTimelineEvent {
            event: TimelineEvent {
                id: "ev-kitchen".into(),
                persona_id: PERSONA_ID.into(),
                timestamp: (now - chrono::Duration::minutes(15)).to_rfc3339(),
                context_type: "room".into(),
                context_id: "room-kitchen".into(),
                context_name: "Kitchen".into(),
                event_type: "discussion".into(),
                actor_id: "chef-ai".into(),
                actor_name: "Chef AI".into(),
                content: "Discussing Mediterranean cooking techniques and recipes".into(),
                importance: 0.5,
                topics: vec!["cooking".into(), "mediterranean".into()],
            },
            embedding: provider
                .embed("Discussing Mediterranean cooking techniques and recipes")
                .ok(),
        },
    ]
}

/// Load the standard test corpus into a manager and return the manager.
fn load_standard_corpus(
    manager: &PersonaMemoryManager,
    memories: Vec<CorpusMemory>,
    events: Vec<CorpusTimelineEvent>,
) {
    manager.load_corpus(PERSONA_ID, memories, events);
}

/// Build a raw MemoryCorpus directly (for individual layer tests).
fn build_corpus(memories: Vec<CorpusMemory>, events: Vec<CorpusTimelineEvent>) -> MemoryCorpus {
    MemoryCorpus::from_corpus_data(memories, events)
}

// ─── Test 1: Basic Store/Recall Roundtrip ─────────────────────────────────────

#[test]
fn test_roundtrip_corpus_load_and_recall() {
    let manager = test_manager();
    let memories = build_thought_chain();
    load_standard_corpus(&manager, memories, vec![]);

    // Multi-layer recall with no query — should get core + temporal + decay layers
    let req = MultiLayerRecallRequest {
        query_text: None,
        room_id: "room-general".into(),
        max_results: 10,
        layers: None,
    };
    let resp = manager.multi_layer_recall(PERSONA_ID, &req).unwrap();

    // Should find memories (core layer alone finds 3 with importance >= 0.8)
    assert!(
        !resp.memories.is_empty(),
        "Multi-layer recall should find memories from corpus"
    );

    // Verify content roundtrip
    assert!(resp.memories.iter().any(|m| m.content.contains("ownership and borrowing")));
}

// ─── Test 2: Core Layer — Never-Forget Memories ──────────────────────────────

#[test]
fn test_core_layer_finds_high_importance() {
    let corpus = build_corpus(build_thought_chain(), vec![]);
    let provider = DeterministicEmbeddingProvider;

    let query = RecallQuery {
        query_text: None,
        query_embedding: None,
        room_id: "room-general".into(),
        max_results_per_layer: 10,
    };

    let results = CoreRecallLayer.recall(&corpus, &query, &provider);

    // Core layer: importance >= 0.8
    assert!(!results.is_empty(), "Core layer should find high-importance memories");
    assert_eq!(
        results.len(), 3,
        "Should find 3 core memories (0.95, 0.85, 0.8), got {}",
        results.len()
    );
    assert!(
        results.iter().all(|r| r.memory.importance >= 0.8),
        "All core results should have importance >= 0.8"
    );

    // The pasta memory (0.4) should NOT be here
    assert!(
        !results.iter().any(|r| r.memory.content.contains("pasta")),
        "Cooking memory should NOT appear in core recall"
    );
}

// ─── Test 3: Semantic Layer — Meaning-Based Similarity ───────────────────────

#[test]
fn test_semantic_layer_finds_related_by_meaning() {
    let corpus = build_corpus(build_thought_chain(), vec![]);
    let provider = DeterministicEmbeddingProvider;

    let query_emb = provider
        .embed("How does Rust handle memory safety and concurrency?")
        .unwrap();

    let query = RecallQuery {
        query_text: Some("How does Rust handle memory safety and concurrency?".into()),
        query_embedding: Some(query_emb),
        room_id: "room-general".into(),
        max_results_per_layer: 5,
    };

    let results = SemanticRecallLayer.recall(&corpus, &query, &provider);

    assert!(
        !results.is_empty(),
        "Semantic layer should find memories related to 'Rust memory safety concurrency'"
    );

    // The top results should be about Rust, not cooking
    let top = &results[0];
    assert!(
        top.memory.content.to_lowercase().contains("rust")
            || top.memory.content.to_lowercase().contains("concurrency")
            || top.memory.content.to_lowercase().contains("memory"),
        "Top semantic result should be about Rust, got: {}",
        top.memory.content
    );

    // Cooking memory should be ranked low (if present at all)
    if let Some(pasta_idx) = results.iter().position(|r| r.memory.content.contains("pasta")) {
        assert!(
            pasta_idx >= results.len() - 2,
            "Cooking memory should be near the bottom, found at position {pasta_idx}/{}",
            results.len()
        );
    }
}

#[test]
fn test_semantic_layer_cooking_query_finds_cooking() {
    let corpus = build_corpus(build_thought_chain(), vec![]);
    let provider = DeterministicEmbeddingProvider;

    let query_emb = provider.embed("What do I know about cooking pasta?").unwrap();

    let query = RecallQuery {
        query_text: Some("What do I know about cooking pasta?".into()),
        query_embedding: Some(query_emb),
        room_id: "room-kitchen".into(),
        max_results_per_layer: 5,
    };

    let results = SemanticRecallLayer.recall(&corpus, &query, &provider);

    assert!(
        !results.is_empty(),
        "Semantic layer should find cooking memory"
    );

    // The pasta memory should rank higher for a cooking query
    let has_pasta_in_top = results.iter().take(3).any(|r| r.memory.content.contains("pasta"));
    assert!(
        has_pasta_in_top,
        "Cooking memory should appear in top 3 for cooking query"
    );
}

// ─── Test 4: Temporal Layer — Recent Context ─────────────────────────────────

#[test]
fn test_temporal_layer_surfaces_recent() {
    let corpus = build_corpus(build_thought_chain(), vec![]);
    let provider = DeterministicEmbeddingProvider;

    let query = RecallQuery {
        query_text: None,
        query_embedding: None,
        room_id: "room-general".into(),
        max_results_per_layer: 3,
    };

    let results = TemporalRecallLayer.recall(&corpus, &query, &provider);

    // Should find recent memories (within 2 hours)
    // Memories 0-4 are within 60 minutes; memory 5 (cooking) is 12 hours old
    assert!(
        !results.is_empty(),
        "Temporal layer should find recent memories"
    );

    // The most recent memory (Tokio, 5 min ago) should be in results
    assert!(
        results.iter().any(|r| r.memory.content.contains("Tokio")),
        "Most recent memory (Tokio) should appear in temporal recall"
    );

    // Cooking memory (12 hours old) should NOT appear
    assert!(
        !results.iter().any(|r| r.memory.content.contains("pasta")),
        "Old cooking memory should NOT appear in temporal recall (12h > 2h window)"
    );
}

// ─── Test 5: Associative Layer — Tag/Keyword Graph ───────────────────────────

#[test]
fn test_associative_layer_finds_by_tags() {
    let corpus = build_corpus(build_thought_chain(), vec![]);
    let provider = DeterministicEmbeddingProvider;

    let query = RecallQuery {
        query_text: Some("Tell me about Rust concurrency".into()),
        query_embedding: None,
        room_id: "room-general".into(),
        max_results_per_layer: 10,
    };

    let results = AssociativeRecallLayer.recall(&corpus, &query, &provider);

    assert!(
        !results.is_empty(),
        "Associative layer should find memories tagged with 'rust' or 'concurrency'"
    );

    // Keywords extracted: "tell", "rust", "concurrency"
    // Should match memories tagged with "rust" and/or "concurrency"
    let rust_count = results
        .iter()
        .filter(|r| {
            r.memory.tags.iter().any(|t| t.contains("rust"))
                || r.memory.content.to_lowercase().contains("rust")
        })
        .count();

    assert!(
        rust_count >= 3,
        "At least 3 memories should match 'rust' by tag or content, got {rust_count}"
    );

    // Cooking should NOT appear (no tag overlap with "rust concurrency")
    assert!(
        !results.iter().any(|r| r.memory.content.contains("pasta")),
        "Cooking memory should NOT appear for 'Rust concurrency' query"
    );
}

// ─── Test 6: Decay Resurface — Spaced Repetition ─────────────────────────────

#[test]
fn test_decay_resurface_surfaces_unaccessed() {
    let corpus = build_corpus(build_thought_chain(), vec![]);
    let provider = DeterministicEmbeddingProvider;

    let query = RecallQuery {
        query_text: None,
        query_embedding: None,
        room_id: "room-general".into(),
        max_results_per_layer: 10,
    };

    let results = DecayResurfaceLayer.recall(&corpus, &query, &provider);

    // All memories have access_count=0 and were created recently,
    // so decay score ~ 0 for recent ones. But any memory with importance >= 0.5 is eligible.
    let eligible_count = results.iter().filter(|r| r.memory.importance >= 0.5).count();
    assert_eq!(
        eligible_count,
        results.len(),
        "All decay resurface results should have importance >= 0.5"
    );
}

// ─── Test 7: Cross-Context Layer ─────────────────────────────────────────────

#[test]
fn test_cross_context_finds_other_room_events() {
    let corpus = build_corpus(build_thought_chain(), build_timeline_events());
    let provider = DeterministicEmbeddingProvider;

    // Query from room-general — should find events from room-academy and room-kitchen
    let query = RecallQuery {
        query_text: Some("Rust ownership patterns".into()),
        query_embedding: None,
        room_id: "room-general".into(),
        max_results_per_layer: 10,
    };

    let results = CrossContextLayer.recall(&corpus, &query, &provider);

    // Should find timeline events from OTHER rooms (not room-general)
    assert!(
        !results.is_empty(),
        "Cross-context layer should find events from other rooms"
    );

    // Events are from room-academy and room-kitchen — both should be cross-context
    for r in &results {
        let ctx_id = r.memory.context.get("context_id").and_then(|v| v.as_str());
        assert_ne!(
            ctx_id,
            Some("room-general"),
            "Cross-context should exclude current room"
        );
    }
}

// ─── Test 8: Multi-Layer Merge — Convergence Boosting ────────────────────────

#[test]
fn test_multi_layer_convergence_boost() {
    let manager = test_manager();
    load_standard_corpus(&manager, build_thought_chain(), vec![]);

    // Multi-layer recall with Rust query — memories found by multiple layers
    // should rank higher than single-layer finds
    let req = MultiLayerRecallRequest {
        query_text: Some("Rust memory safety and concurrency".into()),
        room_id: "room-general".into(),
        max_results: 10,
        layers: None,
    };

    let resp = manager.multi_layer_recall(PERSONA_ID, &req).unwrap();

    assert!(
        !resp.memories.is_empty(),
        "Multi-layer recall should find memories"
    );

    // The ownership/safety memory (0.95 importance, tagged "rust", recent, semantic match)
    // should be found by MULTIPLE layers and thus score highest
    let top = &resp.memories[0];
    assert!(
        top.content.to_lowercase().contains("rust")
            || top.content.to_lowercase().contains("concurrency")
            || top.content.to_lowercase().contains("memory"),
        "Top multi-layer result should be about Rust, got: {}",
        top.content
    );

    // Cooking memory should be at the bottom (or absent) — found by 0-1 layers
    if let Some(pasta) = resp.memories.iter().find(|m| m.content.contains("pasta")) {
        let pasta_pos = resp.memories.iter().position(|m| m.id == pasta.id).unwrap();
        assert!(
            pasta_pos >= resp.memories.len() - 2,
            "Cooking memory should rank near last, got position {pasta_pos}/{}",
            resp.memories.len()
        );
    }

    // Verify layer timings are populated
    assert!(
        resp.layer_timings.len() >= 4,
        "Should have timings for at least 4 layers, got {}",
        resp.layer_timings.len()
    );

    // Verify performance target: < 100ms for entire multi-layer recall
    assert!(
        resp.recall_time_ms < 100.0,
        "Multi-layer recall should complete in <100ms, took {}ms",
        resp.recall_time_ms
    );
}

// ─── Test 9: Thought Chain — Associative Graph Traversal ─────────────────────

#[test]
fn test_thought_chain_association() {
    let manager = test_manager();
    load_standard_corpus(&manager, build_thought_chain(), vec![]);

    // Query about "borrow checker" — should find:
    // - Direct match: borrow_checker memory (tagged "borrow-checker")
    // - Associated: ownership memory (tagged "rust", content overlap)
    // - Associated: concurrency memory (tagged "concurrency", content mentions "data races")
    let req = MultiLayerRecallRequest {
        query_text: Some("borrow checker prevents data races".into()),
        room_id: "room-academy".into(),
        max_results: 6,
        layers: None,
    };

    let resp = manager.multi_layer_recall(PERSONA_ID, &req).unwrap();

    // Top result should be about Rust memory/ownership/borrow concepts.
    let top_content = resp.memories[0].content.to_lowercase();
    assert!(
        top_content.contains("borrow")
            || top_content.contains("data races")
            || top_content.contains("ownership")
            || top_content.contains("memory safety"),
        "Top result for 'borrow checker prevents data races' should be about Rust memory concepts, got: {}",
        resp.memories[0].content
    );

    // Multiple Rust memories should surface — the thought chain connects them
    let rust_count = resp
        .memories
        .iter()
        .filter(|m| {
            m.content.to_lowercase().contains("rust") || m.tags.iter().any(|t| t.contains("rust"))
        })
        .count();
    assert!(
        rust_count >= 3,
        "At least 3 Rust-related memories should surface via thought chain, got {rust_count}"
    );
}

// ─── Test 10: Negative — Unrelated Query Returns Unrelated ───────────────────

#[test]
fn test_unrelated_query_finds_correct_domain() {
    let manager = test_manager();
    load_standard_corpus(&manager, build_thought_chain(), vec![]);

    // Query about cooking — cooking memory should appear somewhere
    let req = MultiLayerRecallRequest {
        query_text: Some("How do you cook pasta al dente?".into()),
        room_id: "room-kitchen".into(),
        max_results: 10,
        layers: None,
    };

    let resp = manager.multi_layer_recall(PERSONA_ID, &req).unwrap();

    // Cooking memory should appear somewhere in results
    let has_pasta = resp.memories.iter().any(|m| m.content.contains("pasta"));
    assert!(
        has_pasta,
        "Cooking query should surface cooking memory. Got {} results: {:?}",
        resp.memories.len(),
        resp.memories.iter().map(|m| &m.content).collect::<Vec<_>>()
    );
}

// ─── Test 11: Performance — Multi-Layer Recall Under Load ─────────────────────

#[test]
fn test_recall_performance_with_many_memories() {
    let manager = test_manager();
    let now = chrono::Utc::now();
    let provider = DeterministicEmbeddingProvider;

    // Build 100 memories to stress-test
    let memories: Vec<CorpusMemory> = (0..100)
        .map(|i| {
            let topic = match i % 5 {
                0 => "Rust programming",
                1 => "database design",
                2 => "network protocols",
                3 => "machine learning",
                _ => "cooking recipes",
            };
            let content = format!(
                "Memory #{i}: Discussion about topic {topic} with importance level {}",
                (50 + (i % 50)) as f64 / 100.0
            );
            let embedding = provider.embed(&content).ok();

            CorpusMemory {
                record: MemoryRecord {
                    id: format!("perf-m-{i}"),
                    persona_id: PERSONA_ID.into(),
                    memory_type: if i % 3 == 0 { "insight" } else { "observation" }.into(),
                    content,
                    context: serde_json::json!({"roomId": format!("room-{}", i % 5)}),
                    timestamp: (now - chrono::Duration::minutes(i as i64)).to_rfc3339(),
                    importance: (50 + (i % 50)) as f64 / 100.0,
                    access_count: 0,
                    tags: vec![
                        format!("topic-{}", i % 5),
                        if i % 2 == 0 { "even" } else { "odd" }.into(),
                    ],
                    related_to: vec![],
                    source: Some("test".into()),
                    last_accessed_at: None,
                    layer: None,
                    relevance_score: None,
                },
                embedding,
            }
        })
        .collect();

    load_standard_corpus(&manager, memories, vec![]);

    // Time the multi-layer recall
    let start = std::time::Instant::now();
    let req = MultiLayerRecallRequest {
        query_text: Some("Rust programming language features".into()),
        room_id: "room-0".into(),
        max_results: 10,
        layers: None,
    };
    let resp = manager.multi_layer_recall(PERSONA_ID, &req).unwrap();
    let elapsed = start.elapsed();

    assert!(
        !resp.memories.is_empty(),
        "Should find memories in 100-memory corpus"
    );

    // Performance target: < 200ms for 100 memories with all 6 layers
    // (relaxed for CI; real target is <50ms)
    assert!(
        elapsed.as_millis() < 200,
        "Multi-layer recall on 100 memories should complete in <200ms, took {}ms",
        elapsed.as_millis()
    );

    println!(
        "Performance: {} memories, {} results, {}ms total, rust internal: {:.1}ms",
        100,
        resp.memories.len(),
        elapsed.as_millis(),
        resp.recall_time_ms
    );
    for lt in &resp.layer_timings {
        println!(
            "  {} layer: {} results in {:.1}ms",
            lt.layer, lt.results_found, lt.time_ms
        );
    }
}

// ─── Test 12: Corpus Loading Response ────────────────────────────────────────

#[test]
fn test_corpus_load_response_counts() {
    let manager = test_manager();
    let memories = build_thought_chain(); // 6 memories, all with embeddings
    let events = build_timeline_events(); // 2 events, both with embeddings

    let resp = manager.load_corpus(PERSONA_ID, memories, events);

    assert_eq!(resp.memory_count, 6);
    assert_eq!(resp.embedded_memory_count, 6);
    assert_eq!(resp.timeline_event_count, 2);
    assert_eq!(resp.embedded_event_count, 2);
    assert!(resp.load_time_ms >= 0.0);
}

// ─── Test 13: Corpus Not Loaded Error ────────────────────────────────────────

#[test]
fn test_recall_without_corpus_returns_error() {
    let manager = test_manager();

    let req = MultiLayerRecallRequest {
        query_text: None,
        room_id: "room-1".into(),
        max_results: 10,
        layers: None,
    };

    let result = manager.multi_layer_recall("nonexistent-persona", &req);
    assert!(result.is_err(), "Recall without loaded corpus should error");
}

// ─── Test 14: Consciousness Context ──────────────────────────────────────────

#[test]
fn test_consciousness_context_with_cross_context_events() {
    let manager = test_manager();
    load_standard_corpus(&manager, build_thought_chain(), build_timeline_events());

    let req = continuum_core::memory::ConsciousnessContextRequest {
        room_id: "room-general".into(),
        current_message: None,
        skip_semantic_search: false,
    };

    let resp = manager.consciousness_context(PERSONA_ID, &req).unwrap();

    // Should have cross-context events (from room-academy and room-kitchen)
    assert!(
        resp.cross_context_event_count > 0,
        "Should detect cross-context events from other rooms"
    );
    assert!(resp.build_time_ms >= 0.0);
}

// ─── Test 15: Corpus Replacement ─────────────────────────────────────────────

#[test]
fn test_corpus_replacement_clears_old_data() {
    let manager = test_manager();
    let provider = DeterministicEmbeddingProvider;

    // Load initial corpus
    let initial = vec![CorpusMemory {
        record: MemoryRecord {
            id: "old-memory".into(),
            persona_id: PERSONA_ID.into(),
            memory_type: "observation".into(),
            content: "This is the old memory that should disappear".into(),
            context: serde_json::json!({}),
            timestamp: chrono::Utc::now().to_rfc3339(),
            importance: 0.9,
            access_count: 0,
            tags: vec!["old".into()],
            related_to: vec![],
            source: None,
            last_accessed_at: None,
            layer: None,
            relevance_score: None,
        },
        embedding: provider.embed("This is the old memory that should disappear").ok(),
    }];
    manager.load_corpus(PERSONA_ID, initial, vec![]);

    // Replace with new corpus
    let replacement = vec![CorpusMemory {
        record: MemoryRecord {
            id: "new-memory".into(),
            persona_id: PERSONA_ID.into(),
            memory_type: "observation".into(),
            content: "This is the new replacement memory".into(),
            context: serde_json::json!({}),
            timestamp: chrono::Utc::now().to_rfc3339(),
            importance: 0.9,
            access_count: 0,
            tags: vec!["new".into()],
            related_to: vec![],
            source: None,
            last_accessed_at: None,
            layer: None,
            relevance_score: None,
        },
        embedding: provider.embed("This is the new replacement memory").ok(),
    }];
    manager.load_corpus(PERSONA_ID, replacement, vec![]);

    // Recall should find new memory, not old
    let req = MultiLayerRecallRequest {
        query_text: None,
        room_id: "room-1".into(),
        max_results: 10,
        layers: None,
    };
    let resp = manager.multi_layer_recall(PERSONA_ID, &req).unwrap();

    assert!(
        resp.memories.iter().all(|m| m.id != "old-memory"),
        "Old memory should not appear after corpus replacement"
    );
    assert!(
        resp.memories.iter().any(|m| m.id == "new-memory"),
        "New memory should appear after corpus replacement"
    );
}
