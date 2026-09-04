//! Memory type definitions — shared between Rust engine and TypeScript via ts-rs.
//!
//! These types are the lingua franca of the Hippocampus IPC protocol.
//! Rust is a pure compute engine — data comes from the TS ORM via IPC.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

// ─── Memory Record ───────────────────────────────────────────────────────────

/// A single memory record — comes from the TS ORM, not SQL.
/// Used as both input (corpus loading) and output (recall results).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/memory/MemoryRecord.ts")]
pub struct MemoryRecord {
    pub id: String,
    pub persona_id: String,
    pub memory_type: String,
    pub content: String,
    #[ts(type = "Record<string, any>")]
    pub context: serde_json::Value,
    pub timestamp: String,
    pub importance: f64,
    pub access_count: u32,
    pub tags: Vec<String>,
    pub related_to: Vec<String>,
    pub source: Option<String>,
    pub last_accessed_at: Option<String>,
    /// Set by recall layers — indicates which layer found this memory
    pub layer: Option<String>,
    /// Set by semantic recall — cosine similarity score
    pub relevance_score: Option<f64>,
    /// Persona-RAID replication provenance (#2056): the node that ORIGINALLY admitted
    /// this record. `None` = admitted locally by the persona herself on this node (lived,
    /// no migration). ORTHOGONAL to `memory_type` (the lived-vs-`shared-by` EXPERIENCE
    /// axis): a REPLICATED record keeps its experience kind — a replayed lived memory
    /// stays lived (same being, restored), a `shared-by` lesson stays taught — and gains
    /// `(origin_node, origin_seq)` purely as auditable replay metadata, so recall needs
    /// zero changes and audit gets everything. Co-designed with BigMama on the seam.
    #[serde(default)]
    #[ts(optional)]
    pub origin_node: Option<String>,
    /// Monotonic per-`(persona, origin_node)` admit sequence — the newest-wins key for
    /// replicated replay (#2056). `None` = a pre-replication record. `u64 → number`
    /// (not bigint) per the ts-rs contract rule ([[sdk-ts-rs-u64-bigint-drift]]); a
    /// per-persona admit counter stays well under 2^53.
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub origin_seq: Option<u64>,
}

// ─── Corpus Loading (ORM → Rust) ─────────────────────────────────────────────

/// A memory with its optional embedding vector — sent from TS ORM to Rust.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/memory/CorpusMemory.ts")]
pub struct CorpusMemory {
    pub record: MemoryRecord,
    pub embedding: Option<Vec<f32>>,
}

/// A timeline event with its optional embedding vector — sent from TS ORM to Rust.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/memory/CorpusTimelineEvent.ts")]
pub struct CorpusTimelineEvent {
    pub event: TimelineEvent,
    pub embedding: Option<Vec<f32>>,
}

/// Response from corpus loading.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/memory/LoadCorpusResponse.ts")]
pub struct LoadCorpusResponse {
    pub memory_count: usize,
    pub embedded_memory_count: usize,
    pub timeline_event_count: usize,
    pub embedded_event_count: usize,
    pub load_time_ms: f64,
}

// ─── Multi-Layer Recall ───────────────────────────────────────────────────────

/// Multi-layer recall request — the primary recall API.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/memory/MultiLayerRecallRequest.ts")]
pub struct MultiLayerRecallRequest {
    pub query_text: Option<String>,
    pub room_id: String,
    pub max_results: usize,
    /// Which layers to run (empty = all layers)
    pub layers: Option<Vec<String>>,
}

/// Response from any recall operation.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/memory/MemoryRecallResponse.ts")]
pub struct MemoryRecallResponse {
    pub memories: Vec<MemoryRecord>,
    pub recall_time_ms: f64,
    pub layer_timings: Vec<LayerTiming>,
    pub total_candidates: usize,
}

/// Timing for a single recall layer.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/memory/LayerTiming.ts")]
pub struct LayerTiming {
    pub layer: String,
    pub time_ms: f64,
    pub results_found: usize,
}

// ─── Consciousness Context ───────────────────────────────────────────────────

/// Request to build consciousness context (replaces TS UnifiedConsciousness.getContext).
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/memory/ConsciousnessContextRequest.ts")]
pub struct ConsciousnessContextRequest {
    pub room_id: String,
    pub current_message: Option<String>,
    pub skip_semantic_search: bool,
}

/// Response with formatted consciousness context for RAG injection.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/memory/ConsciousnessContextResponse.ts")]
pub struct ConsciousnessContextResponse {
    pub formatted_prompt: Option<String>,
    pub build_time_ms: f64,
    pub temporal: TemporalInfo,
    pub cross_context_event_count: usize,
    pub active_intention_count: usize,
    pub has_peripheral_activity: bool,
}

/// Temporal continuity information — "what was I doing before?"
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/memory/TemporalInfo.ts")]
pub struct TemporalInfo {
    pub last_active_context: Option<String>,
    pub last_active_context_name: Option<String>,
    pub time_away_ms: i64,
    pub was_interrupted: bool,
    pub interrupted_task: Option<String>,
}

// ─── Timeline Events ─────────────────────────────────────────────────────────

/// A timeline event — records cross-context activity for consciousness.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/memory/TimelineEvent.ts")]
pub struct TimelineEvent {
    pub id: String,
    pub persona_id: String,
    pub timestamp: String,
    pub context_type: String,
    pub context_id: String,
    pub context_name: String,
    pub event_type: String,
    pub actor_id: String,
    pub actor_name: String,
    pub content: String,
    pub importance: f64,
    pub topics: Vec<String>,
}
