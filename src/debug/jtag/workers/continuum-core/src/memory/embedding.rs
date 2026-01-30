//! Embedding provider — trait-based for pluggable models/backends.
//!
//! Default: fastembed AllMiniLML6V2 (384 dims, ~5ms per embed).
//! Loaded once in-process — no IPC hop, no socket call.
//!
//! Extension points (each a pluggable adapter):
//! - BGE models (768 dims, higher quality)
//! - Fine-tuned persona-specific embedding models
//! - Quantized models (faster, smaller footprint)
//! - Remote embedding APIs (OpenAI, Cohere)

use std::fmt;

// ─── Trait: EmbeddingProvider ──────────────────────────────────────────────────

/// Pluggable embedding provider.
///
/// Each implementation is a separate "adapter" — swap models without changing
/// any consuming code. Known future adapters: BGE-large, fine-tuned persona
/// embeddings, quantized variants.
pub trait EmbeddingProvider: Send + Sync {
    fn name(&self) -> &str;
    fn dimensions(&self) -> usize;
    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError>;
    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError>;
}

// ─── Error ─────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub struct EmbeddingError(pub String);

impl fmt::Display for EmbeddingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for EmbeddingError {}

// ─── FastEmbed Provider (default) ──────────────────────────────────────────────

/// Default embedding provider: fastembed AllMiniLML6V2.
/// - 384 dimensions (same as TS embedding worker)
/// - ~5ms per embed (in-process ONNX, no network)
/// - Model cached at ~/.cache/fastembed or $FASTEMBED_CACHE_PATH
pub struct FastEmbedProvider {
    model: fastembed::TextEmbedding,
}

impl FastEmbedProvider {
    pub fn new() -> Result<Self, EmbeddingError> {
        // InitOptions is #[non_exhaustive] — must use Default + field mutation
        let mut options = fastembed::InitOptions::default();
        options.model_name = fastembed::EmbeddingModel::AllMiniLML6V2;
        options.show_download_progress = true;

        let model = fastembed::TextEmbedding::try_new(options)
            .map_err(|e| EmbeddingError(format!("Failed to load AllMiniLML6V2: {e}")))?;

        Ok(Self { model })
    }
}

impl EmbeddingProvider for FastEmbedProvider {
    fn name(&self) -> &str {
        "fastembed-allminilml6v2"
    }

    fn dimensions(&self) -> usize {
        384
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        let results = self
            .model
            .embed(vec![text], None)
            .map_err(|e| EmbeddingError(format!("Embed failed: {e}")))?;
        results
            .into_iter()
            .next()
            .ok_or_else(|| EmbeddingError("No embedding returned".into()))
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        if texts.is_empty() {
            return Ok(vec![]);
        }
        self.model
            .embed(texts.to_vec(), None)
            .map_err(|e| EmbeddingError(format!("Batch embed failed: {e}")))
    }
}

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
/// This enables testing semantic recall without loading a 50MB ONNX model.
///
/// Properties:
/// - Identical texts → identical vectors → cosine similarity = 1.0
/// - Texts sharing words → partial overlap → 0.0 < similarity < 1.0
/// - Unrelated texts → no overlap → similarity ≈ 0.0
/// - Deterministic: same input always produces same output
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

impl EmbeddingProvider for DeterministicEmbeddingProvider {
    fn name(&self) -> &str {
        "deterministic-test"
    }

    fn dimensions(&self) -> usize {
        384
    }

    fn embed(&self, text: &str) -> Result<Vec<f32>, EmbeddingError> {
        Ok(Self::embed_deterministic(text))
    }

    fn embed_batch(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbeddingError> {
        Ok(texts.iter().map(|t| Self::embed_deterministic(t)).collect())
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
        assert!((sim - 1.0).abs() < 1e-6, "Identical vectors should have similarity 1.0");
    }

    #[test]
    fn test_cosine_similarity_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-6, "Orthogonal vectors should have similarity 0.0");
    }

    #[test]
    fn test_cosine_similarity_opposite() {
        let a = vec![1.0, 2.0, 3.0];
        let b = vec![-1.0, -2.0, -3.0];
        let sim = cosine_similarity(&a, &b);
        assert!((sim + 1.0).abs() < 1e-6, "Opposite vectors should have similarity -1.0");
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

    #[test]
    fn test_deterministic_identical_texts() {
        let provider = DeterministicEmbeddingProvider;
        let a = provider.embed("Rust borrow checker rules").unwrap();
        let b = provider.embed("Rust borrow checker rules").unwrap();
        let sim = cosine_similarity(&a, &b);
        assert!(
            (sim - 1.0).abs() < 1e-6,
            "Identical texts should produce similarity 1.0, got {sim}"
        );
    }

    #[test]
    fn test_deterministic_similar_texts() {
        let provider = DeterministicEmbeddingProvider;
        let a = provider.embed("Rust borrow checker rules").unwrap();
        let b = provider.embed("Rust ownership and borrow system").unwrap();
        let sim = cosine_similarity(&a, &b);
        assert!(
            sim > 0.2,
            "Texts sharing 'rust' and 'borrow' should have meaningful similarity, got {sim}"
        );
    }

    #[test]
    fn test_deterministic_unrelated_texts() {
        let provider = DeterministicEmbeddingProvider;
        let a = provider.embed("Rust borrow checker rules").unwrap();
        let b = provider.embed("Purple elephants dance at midnight").unwrap();
        let sim = cosine_similarity(&a, &b);
        assert!(
            sim < 0.15,
            "Unrelated texts should have low similarity, got {sim}"
        );
    }

    #[test]
    fn test_deterministic_dimension_count() {
        let provider = DeterministicEmbeddingProvider;
        let v = provider.embed("test text").unwrap();
        assert_eq!(v.len(), 384);
        assert_eq!(provider.dimensions(), 384);
    }

    #[test]
    fn test_deterministic_batch_consistency() {
        let provider = DeterministicEmbeddingProvider;
        let single = provider.embed("hello world").unwrap();
        let batch = provider
            .embed_batch(&["hello world".to_string()])
            .unwrap();
        assert_eq!(single, batch[0], "Single and batch embed should produce identical vectors");
    }

    #[test]
    fn test_deterministic_similarity_gradient() {
        // Verify similarity ordering: identical > similar > unrelated
        let provider = DeterministicEmbeddingProvider;
        let base = provider.embed("learning Rust memory management").unwrap();
        let identical = provider.embed("learning Rust memory management").unwrap();
        let similar = provider.embed("understanding Rust memory safety").unwrap();
        let different = provider.embed("cooking Italian pasta recipes").unwrap();

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
