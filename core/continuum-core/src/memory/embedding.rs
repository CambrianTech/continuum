//! Memory-side embedding helpers.
//!
//! There is exactly ONE embedding trait in the substrate:
//! [`crate::cognition::embedding::EmbeddingProvider`] — async, adapter-routed
//! (unsloth / llama-server `/v1/embeddings` via `AIProviderAdapter`), with the
//! lexical bootstrap and the content-addressed cache behind the same interface
//! (task #40). The old synchronous fastembed/ONNX providers that used to live
//! here (`FastEmbedProvider`, `ModuleBackedEmbeddingProvider`) are deleted —
//! embedding is async and goes through the adapter, never an in-process ONNX
//! model.
//!
//! This module now owns only:
//! - [`cosine_similarity`] — the pure relevance math the Rayon recall layers run.
//! - [`DeterministicEmbeddingProvider`] — a deterministic, word-overlap-sensitive
//!   test embedder that implements the canonical async trait (so semantic recall
//!   can be exercised without a model or a network round-trip).
//!
//! Re-export the canonical trait so existing `memory::embedding::EmbeddingProvider`
//! references resolve to the one true trait.

pub use crate::cognition::embedding::EmbeddingProvider;

use async_trait::async_trait;

// ─── Vector Math ───────────────────────────────────────────────────────────────

/// Cosine similarity between two embedding vectors.
/// Returns 0.0 for zero-length or mismatched vectors.
/// Auto-vectorized by rustc in release mode (SIMD).
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }

    let mut dot = 0.0f32;
    let mut norm_a = 0.0f32;
    let mut norm_b = 0.0f32;

    for i in 0..a.len() {
        dot += a[i] * b[i];
        norm_a += a[i] * a[i];
        norm_b += b[i] * b[i];
    }

    let denom = norm_a.sqrt() * norm_b.sqrt();
    if denom == 0.0 {
        0.0
    } else {
        dot / denom
    }
}

// ─── Deterministic Embedding Provider (for testing) ───────────────────────────

/// Test embedding provider that produces deterministic, word-overlap-sensitive vectors.
///
/// How it works: each word in the input text is hashed to a position in a 384-dim vector.
/// Texts sharing words produce overlapping vectors → higher cosine similarity.
/// This enables testing semantic recall without loading a model or calling the
/// embedding endpoint.
///
/// Properties:
/// - Identical texts → identical vectors → cosine similarity = 1.0
/// - Texts sharing words → partial overlap → 0.0 < similarity < 1.0
/// - Unrelated texts → no overlap → similarity ≈ 0.0
/// - Deterministic: same input always produces same output
///
/// Implements the canonical async [`EmbeddingProvider`]; on failure (never, for
/// this pure provider) the contract is an empty vector = "no signal".
pub struct DeterministicEmbeddingProvider;

impl DeterministicEmbeddingProvider {
    /// Simple hash: FNV-1a for deterministic word → dimension mapping.
    fn fnv1a_hash(word: &str) -> usize {
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in word.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash as usize
    }

    /// Embed text into a 384-dim vector by hashing words to positions.
    fn embed_deterministic(text: &str) -> Vec<f32> {
        let dims = 384;
        let mut vec = vec![0.0f32; dims];

        // Normalize: lowercase, split by whitespace and punctuation
        let words: Vec<String> = text
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|w| w.len() >= 2)
            .map(|w| w.to_string())
            .collect();

        if words.is_empty() {
            return vec;
        }

        // Each word contributes to 3 dimensions (spreading reduces collision)
        for word in &words {
            let base = Self::fnv1a_hash(word);
            for offset in 0..3 {
                let dim = (base.wrapping_add(offset * 7919)) % dims;
                vec[dim] += 1.0;
            }
        }

        // L2-normalize so cosine similarity works correctly
        let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut vec {
                *v /= norm;
            }
        }

        vec
    }
}

#[async_trait]
impl EmbeddingProvider for DeterministicEmbeddingProvider {
    fn id(&self) -> &str {
        "deterministic-test"
    }

    fn dim(&self) -> usize {
        384
    }

    async fn embed(&self, text: &str) -> Vec<f32> {
        Self::embed_deterministic(text)
    }
}

// ─── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cosine_similarity_identical() {
        let v = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&v, &v);
        assert!(
            (sim - 1.0).abs() < 1e-6,
            "Identical vectors should have similarity 1.0"
        );
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(
            sim.abs() < 1e-6,
            "Orthogonal vectors should have similarity 0.0"
        );
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![-1.0, -2.0, -3.0];
        let sim = cosine_similarity(&a, &b);
        assert!(
            (sim + 1.0).abs() < 1e-6,
            "Opposite vectors should have similarity -1.0"
        );
    }

    #[test]
    fn test_cosine_similarity_empty() {
        let sim = cosine_similarity(&[], &[]);
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_cosine_similarity_mismatched() {
        let a = vec![1.0, 2.0];
        let b = vec![1.0, 2.0, 3.0];
        let sim = cosine_similarity(&a, &b);
        assert_eq!(sim, 0.0);
    }

    // ─── DeterministicEmbeddingProvider Tests ─────────────────────────────────

    #[tokio::test]
    async fn test_deterministic_identical_texts() {
        let provider = DeterministicEmbeddingProvider;
        let a = provider.embed("Rust borrow checker rules").await;
        let b = provider.embed("Rust borrow checker rules").await;
        let sim = cosine_similarity(&a, &b);
        assert!(
            (sim - 1.0).abs() < 1e-6,
            "Identical texts should produce similarity 1.0, got {sim}"
        );
    }

    #[tokio::test]
    async fn test_deterministic_similar_texts() {
        let provider = DeterministicEmbeddingProvider;
        let a = provider.embed("Rust borrow checker rules").await;
        let b = provider.embed("Rust ownership and borrow system").await;
        let sim = cosine_similarity(&a, &b);
        assert!(
            sim > 0.2,
            "Texts sharing 'rust' and 'borrow' should have meaningful similarity, got {sim}"
        );
    }

    #[tokio::test]
    async fn test_deterministic_unrelated_texts() {
        let provider = DeterministicEmbeddingProvider;
        let a = provider.embed("Rust borrow checker rules").await;
        let b = provider.embed("Purple elephants dance at midnight").await;
        let sim = cosine_similarity(&a, &b);
        assert!(
            sim < 0.15,
            "Unrelated texts should have low similarity, got {sim}"
        );
    }

    #[tokio::test]
    async fn test_deterministic_dimension_count() {
        let provider = DeterministicEmbeddingProvider;
        let v = provider.embed("test text").await;
        assert_eq!(v.len(), 384);
        assert_eq!(provider.dim(), 384);
    }

    #[tokio::test]
    async fn test_deterministic_similarity_gradient() {
        // Verify similarity ordering: identical > similar > unrelated
        let provider = DeterministicEmbeddingProvider;
        let base = provider.embed("learning Rust memory management").await;
        let identical = provider.embed("learning Rust memory management").await;
        let similar = provider.embed("understanding Rust memory safety").await;
        let different = provider.embed("cooking Italian pasta recipes").await;

        let sim_identical = cosine_similarity(&base, &identical);
        let sim_similar = cosine_similarity(&base, &similar);
        let sim_different = cosine_similarity(&base, &different);

        assert!(
            sim_identical > sim_similar,
            "identical({sim_identical}) should be > similar({sim_similar})"
        );
        assert!(
            sim_similar > sim_different,
            "similar({sim_similar}) should be > different({sim_different})"
        );
    }
}
