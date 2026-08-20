//! EngramGraph — the relational graph that algorithm 3 (activation
//! spreading) traverses.
//!
//! Per `docs/architecture/COGNITION-ALGORITHMS.md` §3:
//!
//! > Topical recall alone surfaces what's *similar*. Real memory
//! > surfaces what's *structurally adjacent* — "I remember Joel said X
//! > about Y last week" comes up *when you hit a related concept Z*,
//! > because Y and Z share entities, not because Y and Z are embedding-
//! > similar.
//!
//! The graph stores typed edges between engrams. Edges carry weights
//! tuned by algorithm 7 (substrate yield-learning) over time. Algorithm
//! 3's traversal (lands in L0-3a.5) starts from focus engrams and
//! spreads activation along these edges with per-hop decay; this
//! module ships the **storage substrate only** — no traversal logic
//! yet.
//!
//! ## Sidecar pattern
//!
//! This module is intentionally **separate** from
//! [`crate::persona::engram`], which ships the admission membrane
//! (provenance, trust, content references). The admission membrane is
//! about *where engrams come from*; this graph is about *how engrams
//! connect*. Keeping them separate means admission consumers don't
//! grow algorithm-3 dependencies, and algorithm-3 consumers don't
//! grow admission dependencies.
//!
//! ## Concurrency
//!
//! Edges are stored in a [`DashMap`], so `add_edge` from multiple
//! threads is wait-free in the common case and per-shard-locked in
//! the contended case. Hippocampus admission (when it runs in
//! parallel for multiple personas) can add edges concurrently
//! without coordination.

use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

// ─── EdgeKind ───────────────────────────────────────────────────────

/// Why two engrams are connected. Determines edge weight defaults and
/// algorithm-7 yield-learning behavior — different edge kinds have
/// different prior probabilities of producing consumed-by-handler
/// recall hits.
///
/// Per COGNITION-ALGORITHMS.md §3, the prior ordering is roughly:
/// `SharedEntity` > `SharedTopic` > `ConversationalReply` > `CitedIn`
/// > `RecallCoOccurrence` > `TaskOutcome`. Exact weights are tuned
/// empirically by algorithm 7 in L0-4c; this enum just declares the
/// variants the substrate supports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "kebab-case")]
#[ts(export, export_to = "../../../protocol/typescript/persona/EdgeKind.ts")]
pub enum EdgeKind {
    /// Both engrams reference the same named entity (person, place,
    /// project, file path, function name, etc.). Highest-prior signal
    /// for structural relevance — entity co-mention is rare and
    /// meaningful.
    SharedEntity,

    /// Both engrams cluster in the same topic per embedding similarity.
    /// Lower-prior than SharedEntity but broader recall surface.
    SharedTopic,

    /// Engram A's content cited / quoted / referenced in engram B's
    /// content. Asymmetric (A → B direction matters); add both
    /// directions if the recall should surface either way.
    CitedIn,

    /// Both engrams were retrieved together in past recall events.
    /// Self-reinforcing — engrams often retrieved together stay
    /// together. Algorithm 7's yield-learning amplifies the signal
    /// when the co-retrievals are consumed by handlers.
    RecallCoOccurrence,

    /// Chat-message → reply edge. Conversational thread structure.
    /// Per-channel; chat handler populates these.
    ConversationalReply,

    /// Task-start → task-completion edge. Outcomes the persona
    /// produced. Used by the outcome-linked salience boost in
    /// algorithm 4.
    TaskOutcome,

    /// Act-engram → the engram of what directly triggered it: the
    /// predecessor act in the same settle chain, the inbound message,
    /// or the work-card kickoff. The causal spine of the memory graph
    /// (docs/cognition/CAUSAL-MEMORY-GRAPH.md) — the `because` clause
    /// as structure instead of archived prose. Wired at the act write
    /// site, never inferred after the fact.
    CausedBy,

    /// Act-engram → engram/handle of what the act created: an artifact
    /// write, a posted message, a card state change. The forward half
    /// of the causal spine; TaskOutcome remains the start→done pairing.
    Produced,
}

// ─── EngramEdge ─────────────────────────────────────────────────────

/// One directed edge from a source engram to a target engram. Stored
/// in the source's outbound list; `EngramGraph::in_degree` does the
/// inverse lookup by scanning all sources.
///
/// Weight is in `[0.0, 1.0]` by convention. Algorithm 3's traversal
/// multiplies by `decay_per_hop` per step and prunes below a
/// threshold; algorithm 7's yield-learning updates the weight based
/// on whether spreading along this edge surfaces engrams that get
/// consumed by handlers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/EngramEdge.ts"
)]
pub struct EngramEdge {
    /// Target engram id. The source is the map key in `EngramGraph`,
    /// so it's not duplicated on the edge.
    #[ts(type = "string")]
    pub target: Uuid,

    pub kind: EdgeKind,

    /// Edge weight in `[0.0, 1.0]`. Used as the multiplier in
    /// algorithm 3's `propagated = score * edge.weight * decay_per_hop`.
    pub weight: f32,
}

// ─── EngramGraph ────────────────────────────────────────────────────

/// The per-persona engram relational graph.
///
/// ## What this is
///
/// A sharded `DashMap<source_id, Vec<EngramEdge>>` — each entry is one
/// source engram's outbound edge list. Lookup by source id (the
/// common case for forward traversal) is O(1) amortized. Inbound
/// lookup (`in_degree`) is O(N) over all sources but only used for
/// structural-centrality salience updates (algorithm 4), not on the
/// hot recall path.
///
/// ## What this is NOT
///
/// - **Not** the engram store. The actual `Engram` content lives in
///   the admission membrane (`crate::persona::engram`); the graph
///   only carries ids and connectivity.
/// - **Not** the spreading algorithm. Algorithm 3 (activation
///   spreading) traversal lands in L0-3a.5 — it reads this graph but
///   isn't implemented in this module.
/// - **Not** a recall-metadata sidecar. Salience / last_touched /
///   access_count for per-engram algorithm-4 state lands in
///   L0-3a.2b's `RecallMetadata` module.
///
/// ## Eviction
///
/// `evict_engram` removes both outbound edges (the source's entry)
/// and inbound edges (scans all sources and filters their lists). The
/// inbound scan is O(N) over engrams; acceptable because eviction
/// happens at sleep-policy cadence (L0-4d) or under storage pressure,
/// not on the hot path.
pub struct EngramGraph {
    edges: DashMap<Uuid, Vec<EngramEdge>>,
}

impl EngramGraph {
    pub fn new() -> Self {
        Self {
            edges: DashMap::new(),
        }
    }

    /// Pre-allocated shard capacity for use cases where the working
    /// set size is roughly known up-front (e.g., one entry per
    /// admitted engram).
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            edges: DashMap::with_capacity(capacity),
        }
    }

    /// Append an outbound edge from `from` → `to`. Edges to the same
    /// target with the same kind are NOT deduplicated here — algorithm
    /// 7 may want to count repeated edge events as a strengthening
    /// signal. Callers needing dedup do it themselves.
    pub fn add_edge(&self, from: Uuid, to: Uuid, kind: EdgeKind, weight: f32) {
        self.edges.entry(from).or_default().push(EngramEdge {
            target: to,
            kind,
            weight,
        });
    }

    /// Return all outbound edges from `id`, in insertion order. Empty
    /// vec if the source has no outbound edges (vs `Option<Vec>` —
    /// callers virtually always want to iterate, never branch on
    /// presence, so we elide the Option).
    pub fn neighbors(&self, id: &Uuid) -> Vec<EngramEdge> {
        self.edges.get(id).map(|e| e.clone()).unwrap_or_default()
    }

    /// Count inbound edges to `id` by scanning all sources. O(N) over
    /// the engram set. Used by algorithm 4 for the structural-centrality
    /// component of salience — engrams many others connect to are
    /// central, and central engrams decay slower. Called at
    /// consolidation cadence, not per-tick.
    pub fn in_degree(&self, id: &Uuid) -> usize {
        let mut count = 0;
        for entry in self.edges.iter() {
            count += entry.value().iter().filter(|e| &e.target == id).count();
        }
        count
    }

    /// Total edge count across all sources. Used by region telemetry
    /// + memory-pressure reporting.
    pub fn edge_count(&self) -> usize {
        self.edges.iter().map(|e| e.value().len()).sum()
    }

    /// Remove all edges involving this engram (both outbound and
    /// inbound). Called when an engram is pruned from the store
    /// under storage pressure or by sleep-policy consolidation.
    pub fn evict_engram(&self, id: &Uuid) {
        // Outbound — remove the source's whole entry.
        self.edges.remove(id);
        // Inbound — scan every other source's edge list and filter
        // out edges targeting this id. We rewrite the vec rather than
        // mutating in place because `DashMap::iter` doesn't permit
        // mutation through the iterator; using `iter_mut` would work
        // but we'd hold per-shard write locks longer. Acceptable
        // O(N) given the cold-path use case.
        let sources: Vec<Uuid> = self.edges.iter().map(|e| *e.key()).collect();
        for src in sources {
            if let Some(mut entry) = self.edges.get_mut(&src) {
                entry.retain(|edge| &edge.target != id);
            }
        }
    }

    /// Whether the graph has any edges. Cheap.
    pub fn is_empty(&self) -> bool {
        self.edges.is_empty()
    }
}

impl Default for EngramGraph {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    #[test]
    fn new_engram_graph_is_empty() {
        let g = EngramGraph::new();
        assert!(g.is_empty());
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn add_edge_increments_count() {
        let g = EngramGraph::new();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        g.add_edge(a, b, EdgeKind::SharedEntity, 0.8);
        assert!(!g.is_empty());
        assert_eq!(g.edge_count(), 1);
    }

    #[test]
    fn neighbors_returns_added_edges_in_insertion_order() {
        let g = EngramGraph::new();
        let src = Uuid::new_v4();
        let t1 = Uuid::new_v4();
        let t2 = Uuid::new_v4();
        let t3 = Uuid::new_v4();
        g.add_edge(src, t1, EdgeKind::SharedEntity, 0.9);
        g.add_edge(src, t2, EdgeKind::SharedTopic, 0.5);
        g.add_edge(src, t3, EdgeKind::ConversationalReply, 0.7);

        let neighbors = g.neighbors(&src);
        assert_eq!(neighbors.len(), 3);
        assert_eq!(neighbors[0].target, t1);
        assert_eq!(neighbors[1].target, t2);
        assert_eq!(neighbors[2].target, t3);
    }

    #[test]
    fn neighbors_of_unknown_source_is_empty() {
        let g = EngramGraph::new();
        assert!(g.neighbors(&Uuid::new_v4()).is_empty());
    }

    #[test]
    fn weights_preserved_through_neighbors() {
        let g = EngramGraph::new();
        let src = Uuid::new_v4();
        let tgt = Uuid::new_v4();
        g.add_edge(src, tgt, EdgeKind::TaskOutcome, 0.42);

        let edge = g
            .neighbors(&src)
            .into_iter()
            .next()
            .expect("edge should be present");
        assert!((edge.weight - 0.42).abs() < f32::EPSILON);
        assert_eq!(edge.kind, EdgeKind::TaskOutcome);
    }

    #[test]
    fn in_degree_counts_inbound_edges_across_sources() {
        let g = EngramGraph::new();
        let target = Uuid::new_v4();
        let s1 = Uuid::new_v4();
        let s2 = Uuid::new_v4();
        let s3 = Uuid::new_v4();
        let unrelated = Uuid::new_v4();

        g.add_edge(s1, target, EdgeKind::SharedEntity, 1.0);
        g.add_edge(s2, target, EdgeKind::SharedTopic, 0.6);
        g.add_edge(s3, target, EdgeKind::CitedIn, 0.4);
        g.add_edge(s1, unrelated, EdgeKind::SharedEntity, 1.0); // should NOT count

        assert_eq!(g.in_degree(&target), 3);
        assert_eq!(g.in_degree(&unrelated), 1);
        assert_eq!(g.in_degree(&Uuid::new_v4()), 0);
    }

    #[test]
    fn in_degree_counts_repeated_edges_from_same_source() {
        // Same (src, target, kind) pair added twice — both count for
        // in_degree because we don't dedup. Algorithm 7 may want the
        // strengthening signal of repeated co-occurrence.
        let g = EngramGraph::new();
        let src = Uuid::new_v4();
        let target = Uuid::new_v4();
        g.add_edge(src, target, EdgeKind::RecallCoOccurrence, 0.5);
        g.add_edge(src, target, EdgeKind::RecallCoOccurrence, 0.5);
        assert_eq!(g.in_degree(&target), 2);
    }

    #[test]
    fn evict_engram_removes_outbound_edges() {
        let g = EngramGraph::new();
        let evicted = Uuid::new_v4();
        let other = Uuid::new_v4();
        g.add_edge(evicted, other, EdgeKind::SharedEntity, 1.0);
        g.add_edge(evicted, Uuid::new_v4(), EdgeKind::SharedTopic, 0.5);

        g.evict_engram(&evicted);
        assert!(g.neighbors(&evicted).is_empty());
    }

    #[test]
    fn evict_engram_removes_inbound_edges_from_other_engrams() {
        let g = EngramGraph::new();
        let evicted = Uuid::new_v4();
        let survivor_src = Uuid::new_v4();
        let unrelated = Uuid::new_v4();

        g.add_edge(survivor_src, evicted, EdgeKind::SharedEntity, 1.0);
        g.add_edge(survivor_src, unrelated, EdgeKind::SharedTopic, 0.7);

        g.evict_engram(&evicted);

        // survivor's edge to evicted is gone, edge to unrelated survives.
        let remaining = g.neighbors(&survivor_src);
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].target, unrelated);
    }

    #[test]
    fn evict_engram_is_idempotent() {
        let g = EngramGraph::new();
        let id = Uuid::new_v4();
        g.evict_engram(&id); // no-op
        g.evict_engram(&id); // still no-op
        assert!(g.is_empty());
    }

    #[test]
    fn concurrent_add_edge_from_threads_is_safe() {
        let g = Arc::new(EngramGraph::new());
        let target = Uuid::new_v4();

        let mut handles = vec![];
        for _ in 0..8 {
            let g = Arc::clone(&g);
            handles.push(thread::spawn(move || {
                for _ in 0..100 {
                    let src = Uuid::new_v4();
                    g.add_edge(src, target, EdgeKind::SharedTopic, 0.5);
                }
            }));
        }
        for h in handles {
            h.join().expect("thread panic");
        }

        // 8 threads × 100 edges all targeting `target` = 800 in-degree.
        assert_eq!(g.in_degree(&target), 800);
        assert_eq!(g.edge_count(), 800);
    }

    #[test]
    fn default_constructor_matches_new() {
        let a = EngramGraph::new();
        let b: EngramGraph = Default::default();
        assert_eq!(a.is_empty(), b.is_empty());
        assert_eq!(a.edge_count(), b.edge_count());
    }

    #[test]
    fn with_capacity_constructor_works() {
        let g = EngramGraph::with_capacity(128);
        assert!(g.is_empty());
        let src = Uuid::new_v4();
        let tgt = Uuid::new_v4();
        g.add_edge(src, tgt, EdgeKind::CitedIn, 0.3);
        assert_eq!(g.edge_count(), 1);
    }

    #[test]
    fn edge_kind_round_trips_through_serde() {
        // Sanity: ts-rs / serde encode the variants we expect.
        for kind in [
            EdgeKind::SharedEntity,
            EdgeKind::SharedTopic,
            EdgeKind::CitedIn,
            EdgeKind::RecallCoOccurrence,
            EdgeKind::ConversationalReply,
            EdgeKind::TaskOutcome,
            EdgeKind::CausedBy,
            EdgeKind::Produced,
        ] {
            let json = serde_json::to_string(&kind).expect("serialize");
            let decoded: EdgeKind = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(decoded, kind);
        }
    }
}
