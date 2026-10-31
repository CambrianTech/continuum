use super::*;

/// Params for `data/list` — read records from a collection.
///
/// Persona/UI-facing contract: name the `collection` and (optionally) an
/// intuitive plain-JSON `filter`, ordering, and paging. There is deliberately
/// no database handle to reason about — the shared "main" store is the default.
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/data/DataListParams.ts"
)]
pub struct DataListParams {
    /// The collection to read (e.g. "rooms", "users", "messages").
    pub collection: String,
    /// Optional field filter as plain JSON: `{"roomId": "general"}` for an
    /// equality match, `{"age": {"$gt": 18}}` for an operator match.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "Record<string, unknown>")]
    pub filter: Option<serde_json::Value>,
    /// Optional ordering clauses, applied in order.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub sort: Option<Vec<OrderByClause>>,
    /// Max records to return. Omit for all matching (bounded by the store).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub limit: Option<u32>,
    /// Records to skip before returning (paging alongside `limit`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub offset: Option<u32>,
    /// Storage handle. Defaults to "main" (the shared DB). Power callers may pass
    /// "@persona:<slug>" or "@metrics" to target a specific store.
    ///
    /// On the wire this is `dbPath`: the `handle` key is CLAIMED by the
    /// [`CommandRequest`](crate::runtime::CommandRequest) envelope (a kernel
    /// `HandleRef`), so a string under that name never reached these params and was
    /// refused (`expected struct HandleRef`). The prose knew; now the type says it, and
    /// the envelope's schema guard (card ea28d2f6) refuses the bare name at test time.
    #[serde(default, rename = "dbPath", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub handle: Option<String>,
}

/// Output of `adapter/info` — the storage adapter's identity plus its full
/// capability surface for a given handle. Subsumes the old `adapter/capabilities`
/// (capabilities are a field here, not a parallel command).
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/data/AdapterInfo.ts")]
pub struct AdapterInfo {
    /// The adapter implementation name (e.g. "sqlite").
    pub adapter: String,
    /// The storage handle this info describes.
    pub handle: String,
    /// What the adapter can do (transactions, joins, indexing, vector search, …).
    pub capabilities: crate::orm::adapter::AdapterCapabilities,
}

// ============================================================================
// vector/* typed outputs
//
// The vector handlers once returned ad-hoc `json!` blobs. As typed commands
// they carry real result structs — so the persona surface, codegen, and `uu`
// all see the shape, and a caller can deserialize without guessing field names.
// ============================================================================
/// A single hit from `vector/search` — a record id with its cosine `score`
/// and the `distance` (`1 - score`). `data` is the full record, populated only
/// when the caller passed `includeData`.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/vector/VectorHit.ts")]
pub struct VectorHit {
    pub id: String,
    pub score: f64,
    pub distance: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "Record<string, unknown>")]
    pub data: Option<serde_json::Value>,
}

/// Output of `vector/search` — the top-k nearest records by cosine similarity,
/// plus the `corpus_size` the search ran against (the recall context: a small
/// corpus means low confidence in the ranking).
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vector/VectorSearchResults.ts"
)]
pub struct VectorSearchResults {
    /// The ranked hits (highest score first), at most `k`.
    pub results: Vec<VectorHit>,
    /// Number of hits returned (`results.len()`).
    pub count: usize,
    /// Number of vectors the search scored against.
    pub corpus_size: usize,
}

/// Output of `vector/stats` — how many records in a collection carry an
/// embedding, the vector dimensionality, and the in-memory cache occupancy.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vector/VectorStats.ts"
)]
pub struct VectorStats {
    pub collection: String,
    /// Total records in the collection.
    pub total_records: usize,
    /// Records that carry a non-empty embedding.
    pub records_with_vectors: usize,
    /// Dimensionality of the embeddings (0 if none indexed).
    pub vector_dimensions: usize,
    /// Vectors currently held in the in-memory similarity cache.
    pub cached_vectors: usize,
    /// RFC-3339 timestamp this snapshot was computed.
    pub last_updated: String,
}

/// Output of `vector/invalidate-cache` — whether a cached vector set for the
/// collection existed and was dropped.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vector/VectorCacheInvalidation.ts"
)]
pub struct VectorCacheInvalidation {
    pub collection: String,
    /// True if a cache entry existed and was removed.
    pub cache_invalidated: bool,
}

/// Output of `vector/backfill` — the per-collection tally of an embedding
/// backfill pass: how many records were embedded, already had a vector, or
/// failed.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vector/VectorBackfillStats.ts"
)]
pub struct VectorBackfillStats {
    pub collection: String,
    /// Records examined.
    pub total: usize,
    /// Records freshly embedded this pass.
    pub processed: usize,
    /// Records skipped because they already had an embedding.
    pub skipped: usize,
    /// Records the embedder produced no signal for, or whose write failed.
    pub failed: usize,
    /// Wall-clock duration of the pass, in milliseconds.
    #[ts(type = "number")]
    pub elapsed_ms: u64,
}

/// Output of `migration/cutover` — the connection swap that redirected the active
/// backend to `active`, recording `previous` for a later rollback.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/migration/MigrationCutover.ts"
)]
pub struct MigrationCutover {
    /// The connection string that was swapped out (stored for rollback).
    pub previous: String,
    /// The connection string now active.
    pub active: String,
    /// The adapter backing the newly-active connection.
    pub adapter: String,
}

/// Output of `migration/rollback` — the reversion from `rolledBackFrom` to the
/// previously-active `rolledBackTo` connection.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/migration/MigrationRollback.ts"
)]
pub struct MigrationRollback {
    /// The connection string that was swapped out by the rollback.
    pub rolled_back_from: String,
    /// The connection string restored to active.
    pub rolled_back_to: String,
    /// The adapter backing the restored connection.
    pub adapter: String,
}
