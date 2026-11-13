//! `/v1/embeddings` request/response helpers (pure, tested apart from the HTTP I/O).

use super::*;

// ─── /v1/embeddings helpers (pure — TDD'd apart from the HTTP I/O) ──────────────

/// Build the OpenAI-compatible `/v1/embeddings` request body. A single input is
/// sent as a string and a batch as an array — both shapes the spec accepts —
/// and `model` is the embedding-space identity (already resolved by the caller).
pub(super) fn build_embedding_body(input: &EmbeddingInput, model: &str) -> Value {
    let input = match input {
        EmbeddingInput::Single(s) => json!(s),
        EmbeddingInput::Multiple(v) => json!(v),
    };
    json!({ "input": input, "model": model })
}

/// Parse an OpenAI-compatible `/v1/embeddings` response into vectors ordered by
/// the response's `index` field. The spec does NOT guarantee `data` comes back
/// in input order, so we sort by `index` — getting this wrong silently
/// misaligns every vector with its source text (a corruption, not a crash).
/// Errors (rather than fabricating a vector) when `data` is missing or an entry
/// has no `embedding`, so a misconfigured endpoint degrades to the lexical
/// fallback instead of poisoning recall with junk.
pub(super) fn parse_embedding_response(body: &Value) -> Result<Vec<Vec<f32>>, String> {
    let data = body
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| format!("embeddings response missing `data` array: {body}"))?;

    let mut indexed: Vec<(usize, Vec<f32>)> = Vec::with_capacity(data.len());
    for (i, item) in data.iter().enumerate() {
        let idx = item
            .get("index")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(i);
        let emb = item
            .get("embedding")
            .and_then(|v| v.as_array())
            .ok_or_else(|| format!("embeddings response item {i} missing `embedding`"))?;
        let vec: Vec<f32> = emb
            .iter()
            .map(|n| n.as_f64().unwrap_or(0.0) as f32)
            .collect();
        indexed.push((idx, vec));
    }
    indexed.sort_by_key(|(i, _)| *i);
    Ok(indexed.into_iter().map(|(_, v)| v).collect())
}

/// Extract token usage from an embeddings response, defaulting missing fields to
/// 0 — usage is observability, never load-bearing, so a provider that omits it
/// must not fail the embed.
pub(super) fn parse_embedding_usage(body: &Value) -> UsageMetrics {
    let usage = body.get("usage");
    let prompt = usage
        .and_then(|u| u.get("prompt_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let total = usage
        .and_then(|u| u.get("total_tokens"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(prompt);
    UsageMetrics {
        input_tokens: prompt,
        output_tokens: 0,
        total_tokens: total,
        estimated_cost: None,
    }
}
