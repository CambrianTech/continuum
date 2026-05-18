//! Rust-owned tool-embedding types + pure cosine-similarity scoring.
//!
//! Oxidizer for `ToolRegistry.generateToolEmbeddings` +
//! `ToolRegistry.semanticSearchTools` (TS, see
//! `src/system/tools/server/ToolRegistry.ts:421-511`). Sibling to
//! `check_redundancy.rs` (#1375) + `generate_response.rs` (#1385) +
//! `should_respond.rs` — all part of the #1248 "TS-as-thin-glue" arc.
//!
//! ## Scope of this PR (PR-1 — pure types + cosine + threshold)
//!
//! - IPC request/response shapes (ts-rs):
//!   - `ToolDescription`, `ToolEmbedding`, `EmbedToolsRequest`,
//!     `EmbedToolsResponse`, `SemanticSearchToolsRequest`,
//!     `SemanticSearchResult`
//! - `cosine_similarity(a, b) -> f32` — pure, mirrors TS impl
//! - `extract_category(tool_name) -> &str` — pure (first slash segment or "root")
//! - `SIMILARITY_THRESHOLD: f32 = 0.3` — matches TS literal
//! - `TOOL_EMBEDDING_MODEL: &str = "nomic-embed-text"` — matches TS literal
//!
//! ## NOT in this PR
//!
//! - **PR-2**: cache (`LazyLock<Mutex<ToolEmbeddingCache>>`) + async
//!   `embed_tools` + `semantic_search_tools` + IPC handlers
//!   `tools/embed` + `tools/semantic-search`.
//! - **PR-3**: TS shim — `ToolRegistry` calls `client.toolsEmbed` /
//!   `client.toolsSemanticSearch`.
//! - **PR-4**: Delete dead TS (inline `cosineSimilarity` helper,
//!   `toolEmbeddings` Map, `AIProviderDaemon.createEmbedding` calls).
//!
//! ## Failure-mode discipline
//!
//! - Mismatched vector lengths → `0.0` (matches TS `if (a.length !== b.length) return 0`).
//! - Zero-magnitude vector(s) → `0.0` (matches TS guard).
//! - No silent default-on-error elsewhere — caller in PR-2 surfaces
//!   typed errors.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Default similarity threshold for `semantic_search_tools` — results
/// below this are filtered out. Matches TS literal `0.3`.
pub const SIMILARITY_THRESHOLD: f32 = 0.3;

/// Default embedding model — matches TS literal. Local fastembed via
/// the existing adapter registry handles routing in PR-2.
pub const TOOL_EMBEDDING_MODEL: &str = "nomic-embed-text";

/// Default `limit` for semantic search results — matches TS default.
pub const DEFAULT_SEARCH_LIMIT: u32 = 10;

// ─── Tool description input ───────────────────────────────────────────

/// One tool surface the registry exposes — name + description.
/// PR-2's `embed_tools` consumes these to build the embedding payload.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/ToolDescription.ts"
)]
pub struct ToolDescription {
    pub name: String,
    pub description: String,
}

/// One embedded tool — name plus vector. Returned by PR-2's
/// `embed_tools` IPC for downstream caching / introspection.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/ToolEmbedding.ts"
)]
pub struct ToolEmbedding {
    pub tool_name: String,
    pub vector: Vec<f32>,
}

// ─── IPC request + response shapes ────────────────────────────────────

/// IPC request: embed a batch of tool descriptions.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/EmbedToolsRequest.ts"
)]
pub struct EmbedToolsRequest {
    pub tools: Vec<ToolDescription>,
    /// Optional model override. PR-2 defaults to
    /// [`TOOL_EMBEDDING_MODEL`] when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
}

/// IPC response from `tools/embed`: per-tool embeddings + provenance.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/EmbedToolsResponse.ts"
)]
pub struct EmbedToolsResponse {
    pub embeddings: Vec<ToolEmbedding>,
    pub model: String,
    #[ts(type = "number")]
    pub generated_at_ms: u64,
}

/// IPC request: rank cached tool embeddings against a query vector.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/SemanticSearchToolsRequest.ts"
)]
pub struct SemanticSearchToolsRequest {
    pub query: String,
    /// Optional model override (must match the model used for
    /// `tools/embed` — mixing models within one similarity space
    /// is meaningless). PR-2 defaults to [`TOOL_EMBEDDING_MODEL`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    /// Max results to return. PR-2 defaults to
    /// [`DEFAULT_SEARCH_LIMIT`] when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub limit: Option<u32>,
    /// Minimum cosine similarity to include in results. PR-2 defaults
    /// to [`SIMILARITY_THRESHOLD`] when unset. Caller may pass `0.0`
    /// to disable filtering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub threshold: Option<f32>,
}

/// One semantic-search hit — tool surface + computed similarity score.
/// Similarity is rounded to 3 decimal places (matches TS
/// `Math.round(similarity * 1000) / 1000`).
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/SemanticSearchResult.ts"
)]
pub struct SemanticSearchResult {
    pub name: String,
    pub description: String,
    pub category: String,
    pub similarity: f32,
}

// ─── Pure scoring ─────────────────────────────────────────────────────

/// Cosine similarity between two equal-length vectors. Pure.
///
/// Returns `0.0` when:
/// - lengths differ (mirrors TS `if (a.length !== b.length) return 0`),
/// - either magnitude is `0.0` (mirrors TS `magnitude === 0 ? 0 : ...`).
///
/// Result is `f32` to match the wire shape consumed by
/// `SemanticSearchResult.similarity`. The TS implementation accumulated
/// in `f64` then truncated; we accumulate in `f64` here too to avoid
/// the well-known float-error compounding on long vectors, then cast
/// the final ratio to `f32`.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() {
        return 0.0;
    }
    let mut dot: f64 = 0.0;
    let mut mag_a: f64 = 0.0;
    let mut mag_b: f64 = 0.0;
    for (x, y) in a.iter().zip(b.iter()) {
        let xf = *x as f64;
        let yf = *y as f64;
        dot += xf * yf;
        mag_a += xf * xf;
        mag_b += yf * yf;
    }
    let magnitude = mag_a.sqrt() * mag_b.sqrt();
    if magnitude == 0.0 {
        0.0
    } else {
        (dot / magnitude) as f32
    }
}

/// Extract the category for display from a tool name. Mirrors TS
/// `tool.name.includes('/') ? tool.name.split('/')[0] : 'root'`.
///
/// Examples:
/// - `"interface/screenshot"` → `"interface"`
/// - `"data/users/list"` → `"data"` (first segment only)
/// - `"plain"` → `"root"`
pub fn extract_category(tool_name: &str) -> &str {
    match tool_name.find('/') {
        Some(idx) => &tool_name[..idx],
        None => "root",
    }
}

/// Round a similarity score to 3 decimal places for wire output.
/// Mirrors TS `Math.round(similarity * 1000) / 1000`.
pub fn round_similarity(similarity: f32) -> f32 {
    (similarity * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── cosine_similarity ────────────────────────────────────────────

    /// What this catches: identical unit vectors return ~1.0. The
    /// canonical sanity check.
    #[test]
    fn identical_vectors_return_one() {
        let v = vec![1.0_f32, 0.0, 0.0];
        let sim = cosine_similarity(&v, &v);
        assert!((sim - 1.0).abs() < 1e-6, "expected ~1.0, got {sim}");
    }

    /// What this catches: orthogonal vectors return 0.0. Bedrock
    /// property of cosine similarity.
    #[test]
    fn orthogonal_vectors_return_zero() {
        let a = vec![1.0_f32, 0.0, 0.0];
        let b = vec![0.0_f32, 1.0, 0.0];
        assert!(cosine_similarity(&a, &b).abs() < 1e-6);
    }

    /// What this catches: opposite-direction vectors return ~-1.0.
    /// Anti-similarity is well-defined; downstream filters can include
    /// or exclude negatives based on threshold (default 0.3 cuts them).
    #[test]
    fn opposite_vectors_return_minus_one() {
        let a = vec![1.0_f32, 0.0, 0.0];
        let b = vec![-1.0_f32, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-6, "expected ~-1.0, got {sim}");
    }

    /// What this catches: mismatched vector lengths return 0.0 (TS
    /// parity). Without this guard, the dot loop would panic on
    /// index access — the typed Rust version is safer than TS but
    /// the SHAPED behavior (return 0) is what callers expect.
    #[test]
    fn mismatched_lengths_return_zero() {
        let a = vec![1.0_f32, 2.0, 3.0];
        let b = vec![1.0_f32, 2.0];
        assert_eq!(cosine_similarity(&a, &b), 0.0);
    }

    /// What this catches: zero-magnitude vector → 0.0 (avoids NaN
    /// from divide-by-zero). TS check: `magnitude === 0 ? 0 : ratio`.
    #[test]
    fn zero_magnitude_returns_zero() {
        let zero = vec![0.0_f32, 0.0, 0.0];
        let v = vec![1.0_f32, 2.0, 3.0];
        assert_eq!(cosine_similarity(&zero, &v), 0.0);
        assert_eq!(cosine_similarity(&v, &zero), 0.0);
        assert_eq!(cosine_similarity(&zero, &zero), 0.0);
    }

    /// What this catches: empty vectors return 0.0 (length match but
    /// magnitude=0). Pins behavior at the length=0 boundary.
    #[test]
    fn empty_vectors_return_zero() {
        let empty: Vec<f32> = vec![];
        assert_eq!(cosine_similarity(&empty, &empty), 0.0);
    }

    /// What this catches: non-trivial similarity for a known case.
    /// vec a = (3,4), vec b = (4,3) → dot=24, |a|=5, |b|=5, sim=0.96.
    #[test]
    fn known_case_pythagorean() {
        let a = vec![3.0_f32, 4.0];
        let b = vec![4.0_f32, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 0.96).abs() < 1e-4, "expected ~0.96, got {sim}");
    }

    /// What this catches: f64 accumulation prevents catastrophic
    /// cancellation on long vectors. 1000-dim vector with tiny values
    /// should still give meaningful similarity.
    #[test]
    fn long_vector_no_precision_loss() {
        let a: Vec<f32> = (0..1000).map(|i| (i as f32) * 0.001).collect();
        let b = a.clone();
        let sim = cosine_similarity(&a, &b);
        assert!((sim - 1.0).abs() < 1e-4, "expected ~1.0, got {sim}");
    }

    // ─── extract_category ─────────────────────────────────────────────

    /// What this catches: single-segment name (no slash) returns
    /// `"root"`. Matches TS fallback for built-in tools like
    /// `search_tools` that don't have a category prefix.
    #[test]
    fn category_no_slash_returns_root() {
        assert_eq!(extract_category("search_tools"), "root");
        assert_eq!(extract_category("list_tools"), "root");
        assert_eq!(extract_category(""), "root");
    }

    /// What this catches: standard `category/tool` name returns the
    /// first segment. Most tools follow this convention.
    #[test]
    fn category_standard_two_segments() {
        assert_eq!(extract_category("interface/screenshot"), "interface");
        assert_eq!(extract_category("collaboration/chat/send"), "collaboration");
        assert_eq!(extract_category("ai/report"), "ai");
    }

    /// What this catches: leading slash (degenerate input) returns
    /// empty string for the category, not panic. Pins behavior at
    /// the boundary so a malformed registration doesn't crash.
    #[test]
    fn category_leading_slash_returns_empty() {
        assert_eq!(extract_category("/foo"), "");
    }

    // ─── round_similarity ─────────────────────────────────────────────

    /// What this catches: rounding to 3 decimals for wire output.
    /// Mirrors TS `Math.round(similarity * 1000) / 1000`.
    #[test]
    fn round_three_decimal_places() {
        assert_eq!(round_similarity(0.123456_f32), 0.123_f32);
        assert_eq!(round_similarity(0.1235_f32), 0.124_f32);
        assert_eq!(round_similarity(1.0_f32), 1.0_f32);
        assert_eq!(round_similarity(0.0_f32), 0.0_f32);
    }

    /// What this catches: negative scores round correctly (TS
    /// `Math.round` rounds toward +∞ on .5 ties; Rust `f32::round`
    /// rounds away from zero — they agree on the magnitudes we
    /// actually emit but the boundary is worth pinning).
    #[test]
    fn round_negative_similarity() {
        assert_eq!(round_similarity(-0.12345_f32), -0.123_f32);
    }

    // ─── constants ────────────────────────────────────────────────────

    /// What this catches: SIMILARITY_THRESHOLD matches the TS literal
    /// 0.3 — recipe-relevant for downstream filtering behavior.
    #[test]
    fn threshold_matches_ts_literal() {
        assert_eq!(SIMILARITY_THRESHOLD, 0.3_f32);
    }

    /// What this catches: TOOL_EMBEDDING_MODEL matches the TS literal
    /// "nomic-embed-text" — same model so embedding space is identical
    /// to legacy cached vectors.
    #[test]
    fn model_matches_ts_literal() {
        assert_eq!(TOOL_EMBEDDING_MODEL, "nomic-embed-text");
    }

    /// What this catches: DEFAULT_SEARCH_LIMIT matches the TS default
    /// limit=10.
    #[test]
    fn default_limit_matches_ts_literal() {
        assert_eq!(DEFAULT_SEARCH_LIMIT, 10);
    }
}
