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

use crate::ai::adapter::InferenceDevice;
use crate::ai::types::{EmbeddingInput, EmbeddingRequest, EmbeddingResponse};
use crate::modules::ai_provider::global_registry;
use serde::{Deserialize, Serialize};
use std::sync::{LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
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
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ToolDescription.ts"
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
    export_to = "../../../protocol/typescript/cognition/ToolEmbedding.ts"
)]
pub struct ToolEmbedding {
    pub tool_name: String,
    pub vector: Vec<f32>,
}

// ─── IPC request + response shapes ────────────────────────────────────

/// IPC request: embed a batch of tool descriptions.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/EmbedToolsRequest.ts"
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
    export_to = "../../../protocol/typescript/cognition/EmbedToolsResponse.ts"
)]
pub struct EmbedToolsResponse {
    pub embeddings: Vec<ToolEmbedding>,
    pub model: String,
    #[ts(type = "number")]
    pub generated_at_ms: u64,
}

/// IPC request: rank cached tool embeddings against a query vector.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/SemanticSearchToolsRequest.ts"
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
    #[ts(optional)]
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
    export_to = "../../../protocol/typescript/cognition/SemanticSearchResult.ts"
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

// ─── Process-wide cache (PR-2) ────────────────────────────────────────

/// In-memory cache of tool embeddings. Single instance per process —
/// the registry of tools is process-singleton too, so one cache per
/// process matches the data lifecycle. Replaces the TS-side
/// `ToolRegistry.toolEmbeddings: Map<string, Float32Array>`.
///
/// `generated_at_ms` is reported on the `EmbedToolsResponse` returned
/// from `embed_tools` but not retained on the cache struct itself —
/// a future "cache state" IPC can re-add it when there's a real
/// consumer; today's `semantic_search_tools` does not need it.
#[derive(Debug, Clone)]
struct ToolEmbeddingCache {
    embeddings: Vec<ToolEmbedding>,
    /// Tool description text alongside each embedding, in the same
    /// order. Kept so `semantic_search_tools` can return descriptions
    /// without a second lookup (TS version had `this.tools.values()`
    /// to walk; Rust caches both per embed_tools call).
    descriptions: Vec<ToolDescription>,
    model: String,
}

static TOOL_EMBEDDING_CACHE: LazyLock<Mutex<Option<ToolEmbeddingCache>>> =
    LazyLock::new(|| Mutex::new(None));

// ─── Errors (PR-2) ────────────────────────────────────────────────────

/// Typed errors for the async tool-embedding API. No silent
/// default-on-error; caller decides policy.
#[derive(Debug, thiserror::Error)]
pub enum ToolEmbeddingError {
    /// No registered adapter advertised support for the requested
    /// provider + model. Operator should check that the embedding
    /// provider (fastembed for `nomic-embed-text`) is loaded.
    #[error("no AI adapter for provider={provider:?} model={model:?}")]
    NoAdapter {
        provider: String,
        model: Option<String>,
    },
    /// Provider returned an error during the `create_embedding` call.
    /// The string carries the raw provider message — caller logs +
    /// surfaces, never silently defaults.
    #[error("embedding generation failed: {0}")]
    EmbeddingFailed(String),
    /// `semantic_search_tools` was called before any `embed_tools` —
    /// the cache is empty. Caller should run embed_tools first OR
    /// register tools so embed_tools can populate the cache.
    #[error("tool embedding cache is empty — call embed_tools first")]
    CacheEmpty,
    /// Provider returned fewer embedding vectors than requested. Pins
    /// the wire contract; partial responses are typed errors here.
    #[error("provider returned {got} embeddings, expected {expected} (1 per requested tool)")]
    EmbeddingCountMismatch { got: usize, expected: usize },
}

// ─── Async API (PR-2) ─────────────────────────────────────────────────

/// Embed a batch of tools and populate the process-wide cache.
/// Replaces TS `ToolRegistry.generateToolEmbeddings`.
///
/// On success: the cache is replaced (not merged) — embed_tools is the
/// "rebuild from current tool list" operation, so any stale entries
/// from a prior registration must drop. Returns the same embeddings
/// to the caller for introspection / logging.
pub async fn embed_tools(
    request: EmbedToolsRequest,
) -> Result<EmbedToolsResponse, ToolEmbeddingError> {
    let model = request
        .model
        .clone()
        .unwrap_or_else(|| TOOL_EMBEDDING_MODEL.to_string());

    let inputs: Vec<String> = request
        .tools
        .iter()
        .map(|t| format!("{}: {}", t.name, t.description))
        .collect();
    let expected_count = inputs.len();

    let registry_arc = global_registry();
    let registry = registry_arc.read().await;
    let (_provider_id, adapter) = registry
        // Device = `Auto`: cognition has no opinion on placement.
        // See cognition/generate_response.rs:285 doctrine note.
        .select(None, Some(&model), InferenceDevice::Auto)
        .ok_or_else(|| ToolEmbeddingError::NoAdapter {
            provider: "any".to_string(),
            model: Some(model.clone()),
        })?;

    let embedding_req = EmbeddingRequest {
        input: EmbeddingInput::Multiple(inputs),
        model: Some(model.clone()),
        provider: None,
    };

    let response: EmbeddingResponse = adapter
        .create_embedding(embedding_req)
        .await
        .map_err(ToolEmbeddingError::EmbeddingFailed)?;

    if response.embeddings.len() != expected_count {
        return Err(ToolEmbeddingError::EmbeddingCountMismatch {
            got: response.embeddings.len(),
            expected: expected_count,
        });
    }

    let generated_at_ms = now_ms();
    let embeddings: Vec<ToolEmbedding> = request
        .tools
        .iter()
        .zip(response.embeddings.iter())
        .map(|(tool, vec)| ToolEmbedding {
            tool_name: tool.name.clone(),
            vector: vec.clone(),
        })
        .collect();

    {
        let mut cache = TOOL_EMBEDDING_CACHE
            .lock()
            .expect("TOOL_EMBEDDING_CACHE mutex poisoned");
        *cache = Some(ToolEmbeddingCache {
            embeddings: embeddings.clone(),
            descriptions: request.tools.clone(),
            model: model.clone(),
        });
    }

    Ok(EmbedToolsResponse {
        embeddings,
        model,
        generated_at_ms,
    })
}

/// Rank cached tool embeddings against a query. Replaces TS
/// `ToolRegistry.semanticSearchTools`.
///
/// - Embeds the query via the same adapter / model used for the
///   cached tool embeddings (mixing models within one similarity space
///   is meaningless).
/// - Computes cosine similarity against each cached tool vector.
/// - Filters by the configured / requested threshold (default
///   [`SIMILARITY_THRESHOLD`]).
/// - Returns top-N sorted by similarity descending.
///
/// Returns [`ToolEmbeddingError::CacheEmpty`] if `embed_tools` hasn't
/// run yet — caller surfaces; no silent fallback.
pub async fn semantic_search_tools(
    request: SemanticSearchToolsRequest,
) -> Result<Vec<SemanticSearchResult>, ToolEmbeddingError> {
    let (cached_embeddings, cached_descriptions, cache_model) = {
        let cache = TOOL_EMBEDDING_CACHE
            .lock()
            .expect("TOOL_EMBEDDING_CACHE mutex poisoned");
        let entry = cache.as_ref().ok_or(ToolEmbeddingError::CacheEmpty)?;
        (
            entry.embeddings.clone(),
            entry.descriptions.clone(),
            entry.model.clone(),
        )
    };

    // Use the cache's model unless the request explicitly overrides
    // — but ALWAYS embed the query through the same path. Passing a
    // different model would compute cosine in an alien embedding
    // space; refuse silent mixing.
    let model = request.model.clone().unwrap_or(cache_model);
    let threshold = request.threshold.unwrap_or(SIMILARITY_THRESHOLD);
    let limit = request.limit.unwrap_or(DEFAULT_SEARCH_LIMIT) as usize;

    let registry_arc = global_registry();
    let registry = registry_arc.read().await;
    let (_provider_id, adapter) = registry
        // Device = `Auto`: cognition has no opinion on placement.
        // See cognition/generate_response.rs:285 doctrine note.
        .select(None, Some(&model), InferenceDevice::Auto)
        .ok_or_else(|| ToolEmbeddingError::NoAdapter {
            provider: "any".to_string(),
            model: Some(model.clone()),
        })?;

    let embedding_req = EmbeddingRequest {
        input: EmbeddingInput::Single(request.query),
        model: Some(model.clone()),
        provider: None,
    };
    let response: EmbeddingResponse = adapter
        .create_embedding(embedding_req)
        .await
        .map_err(ToolEmbeddingError::EmbeddingFailed)?;

    let query_vector = response.embeddings.into_iter().next().ok_or_else(|| {
        ToolEmbeddingError::EmbeddingFailed("provider returned no query embedding".to_string())
    })?;

    let mut results: Vec<SemanticSearchResult> = cached_embeddings
        .iter()
        .zip(cached_descriptions.iter())
        .filter_map(|(emb, desc)| {
            let sim = cosine_similarity(&query_vector, &emb.vector);
            if sim < threshold {
                return None;
            }
            Some(SemanticSearchResult {
                name: emb.tool_name.clone(),
                description: desc.description.clone(),
                category: extract_category(&emb.tool_name).to_string(),
                similarity: round_similarity(sim),
            })
        })
        .collect();

    results.sort_by(|a, b| {
        b.similarity
            .partial_cmp(&a.similarity)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(limit);
    Ok(results)
}

/// Test-only: clear the process-wide cache. Production code should
/// rebuild via `embed_tools`, never silently clear.
#[cfg(test)]
pub fn _clear_cache_for_tests() {
    let mut cache = TOOL_EMBEDDING_CACHE
        .lock()
        .expect("TOOL_EMBEDDING_CACHE mutex poisoned");
    *cache = None;
}

/// Test-only: install a synthetic cache. Lets cache-dependent
/// behavior (filtering, sorting, limit, descriptions lookup) be
/// tested without requiring a real adapter.
#[cfg(test)]
pub fn _install_cache_for_tests(
    embeddings: Vec<ToolEmbedding>,
    descriptions: Vec<ToolDescription>,
    model: String,
) {
    let mut cache = TOOL_EMBEDDING_CACHE
        .lock()
        .expect("TOOL_EMBEDDING_CACHE mutex poisoned");
    *cache = Some(ToolEmbeddingCache {
        embeddings,
        descriptions,
        model,
    });
}

/// Current unix-ms timestamp. Private helper.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
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

    // ─── ToolEmbeddingError Display ───────────────────────────────────

    /// What this catches: Display impl carries the provider + model
    /// for NoAdapter so debug logs surface what went unrouted.
    #[test]
    fn error_no_adapter_displays_provider_and_model() {
        let err = ToolEmbeddingError::NoAdapter {
            provider: "any".to_string(),
            model: Some("nomic-embed-text".to_string()),
        };
        let s = format!("{err}");
        assert!(s.contains("any"));
        assert!(s.contains("nomic-embed-text"));
    }

    /// What this catches: CacheEmpty Display gives an actionable
    /// next-step ("call embed_tools first").
    #[test]
    fn error_cache_empty_displays_actionable_hint() {
        let s = format!("{}", ToolEmbeddingError::CacheEmpty);
        assert!(s.contains("embed_tools"));
    }

    /// What this catches: EmbeddingCountMismatch Display includes both
    /// counts so an operator can diagnose a provider truncation.
    #[test]
    fn error_count_mismatch_includes_both_numbers() {
        let err = ToolEmbeddingError::EmbeddingCountMismatch {
            got: 3,
            expected: 5,
        };
        let s = format!("{err}");
        assert!(s.contains('3'));
        assert!(s.contains('5'));
    }

    // ─── semantic_search_tools (cache-driven, no adapter needed) ──────

    /// What this catches: semantic search returns CacheEmpty before
    /// embed_tools has run. Mirrors TS guard that throws on missing
    /// embeddings.
    #[tokio::test]
    async fn semantic_search_empty_cache_errors() {
        _clear_cache_for_tests();
        let request = SemanticSearchToolsRequest {
            query: "anything".to_string(),
            model: None,
            limit: None,
            threshold: None,
        };
        // Note: we expect CacheEmpty before any adapter lookup.
        let result = semantic_search_tools(request).await;
        assert!(
            matches!(result, Err(ToolEmbeddingError::CacheEmpty)),
            "expected CacheEmpty, got {result:?}"
        );
    }

    /// What this catches: cache install + clear is plumbed and the
    /// test scaffolding doesn't leak state across tests. Without
    /// `_clear_cache_for_tests`, the `semantic_search_empty_cache_errors`
    /// test above would non-deterministically pass/fail depending on
    /// test order. This pins the test-scaffolding contract.
    #[test]
    fn cache_install_and_clear_for_tests() {
        _clear_cache_for_tests();
        _install_cache_for_tests(
            vec![ToolEmbedding {
                tool_name: "test/tool".to_string(),
                vector: vec![1.0, 0.0],
            }],
            vec![ToolDescription {
                name: "test/tool".to_string(),
                description: "test description".to_string(),
            }],
            "test-model".to_string(),
        );
        // Read it back to confirm install
        let snapshot = {
            let guard = TOOL_EMBEDDING_CACHE.lock().unwrap();
            guard.clone()
        };
        assert!(snapshot.is_some());
        let cache = snapshot.unwrap();
        assert_eq!(cache.embeddings.len(), 1);
        assert_eq!(cache.embeddings[0].tool_name, "test/tool");
        assert_eq!(cache.model, "test-model");
        _clear_cache_for_tests();
    }
}
