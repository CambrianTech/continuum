//! Embedding + similarity — the relevance primitive for recall.
//!
//! Recall must surface the memory RELEVANT to the current burst, not merely the
//! most recent (PERSONA-BRAIN-ARCHITECTURE.md §5 / §2.9: "always remembering" =
//! similarity recall into the long-term store). That needs a vector embedding of
//! text + a similarity measure. This module owns both, behind a trait so the
//! BACKEND is swappable:
//!
//! - [`LexicalEmbedder`] — the bootstrap: a hashing-trick term-frequency vector.
//!   Real, deterministic, zero-dependency topical-overlap relevance that works on
//!   ANY machine with no model loaded ("solve for public users"). It is NOT a
//!   stub — lexical overlap is a genuine (if shallow) relevance signal.
//! - A neural embedder (llama.cpp `--embedding` mode, or a dedicated bge/nomic
//!   model) slots in behind the SAME trait for semantic relevance, when an
//!   embedding backend exists on the grid. The recall re-ranker (§7) does not
//!   change when it does — only the vectors get smarter.
//!
//! This is the project's adapter discipline: build the simplest outlier first,
//! prove the interface, let the strong backend slot in unchanged.

/// A backend that turns text into a dense vector. Sync + cheap for the lexical
/// bootstrap; a neural backend pre-embeds engrams at admission (off the recall
/// hot path) and looks them up here.
pub trait EmbeddingProvider: Send + Sync {
    /// Stable identifier for the embedding space (so vectors from different
    /// providers are never silently mixed).
    fn id(&self) -> &str;
    /// The vector dimensionality.
    fn dim(&self) -> usize;
    /// Embed text into a (typically L2-normalized) vector of length `dim()`.
    fn embed(&self, text: &str) -> Vec<f32>;
}

/// Cosine similarity in `[-1, 1]` (≈`[0,1]` for non-negative vectors). Returns
/// 0.0 for a length mismatch or a zero vector — a relevance score of "no signal",
/// never a panic.
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let mut dot = 0.0f32;
    let mut na = 0.0f32;
    let mut nb = 0.0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// Bootstrap embedder: hashing-trick term-frequency vectors. Lowercase, split on
/// non-alphanumeric, hash each token into one of `DIM` buckets, count, then
/// L2-normalize so cosine is a dot product. Captures topical word overlap — a
/// query sharing vocabulary with a memory scores high regardless of recency.
pub struct LexicalEmbedder {
    dim: usize,
}

impl Default for LexicalEmbedder {
    fn default() -> Self {
        // 512 buckets — enough to keep collisions low for short engram/burst text
        // without paying for a real model.
        Self { dim: 512 }
    }
}

impl LexicalEmbedder {
    pub fn new() -> Self {
        Self::default()
    }

    /// FNV-1a — a small, fast, deterministic, dependency-free string hash. We do
    /// NOT use Rust's `DefaultHasher` (it is randomized per process, which would
    /// make embeddings non-reproducible across runs — replay would break).
    fn hash_token(token: &str) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for b in token.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }
}

impl EmbeddingProvider for LexicalEmbedder {
    fn id(&self) -> &str {
        "lexical-fnv-tf"
    }

    fn dim(&self) -> usize {
        self.dim
    }

    fn embed(&self, text: &str) -> Vec<f32> {
        let mut v = vec![0.0f32; self.dim];
        let mut token = String::new();
        let mut push = |tok: &mut String, v: &mut [f32]| {
            if !tok.is_empty() {
                let idx = (Self::hash_token(tok) as usize) % v.len();
                v[idx] += 1.0;
                tok.clear();
            }
        };
        for ch in text.chars() {
            if ch.is_alphanumeric() {
                token.extend(ch.to_lowercase());
            } else {
                push(&mut token, &mut v);
            }
        }
        push(&mut token, &mut v);
        // L2-normalize so cosine == dot product.
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in &mut v {
                *x /= norm;
            }
        }
        v
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the cosine of identical text is ~1; orthogonal
    // (no shared vocabulary) is ~0. The relevance primitive is sane.
    #[test]
    fn identical_text_is_maximally_similar() {
        let e = LexicalEmbedder::new();
        let a = e.embed("the deploy pipeline went red after the migration");
        let same = e.embed("the deploy pipeline went red after the migration");
        assert!(cosine_similarity(&a, &same) > 0.999);
    }

    // what this catches: topical relevance — a query about the rollout plan is
    // MORE similar to the rollout memory than to an unrelated lunch memory, even
    // though both are equally "old". This is relevance, not recency.
    #[test]
    fn relevant_text_outscores_unrelated_text() {
        let e = LexicalEmbedder::new();
        let query = e.embed("what was our rollout plan for the auth flow?");
        let relevant = e.embed("we will ship the auth flow behind a feature flag and ramp the rollout to 10%");
        let unrelated = e.embed("lunch is at noon, someone booked the corner table");
        let rel = cosine_similarity(&query, &relevant);
        let unrel = cosine_similarity(&query, &unrelated);
        assert!(
            rel > unrel,
            "relevant memory must score higher: relevant={rel:.3} unrelated={unrel:.3}"
        );
        assert!(rel > 0.0);
    }

    // what this catches: a zero/empty embedding or length mismatch yields 0.0,
    // never a panic or NaN — recall must degrade to "no signal", not crash.
    #[test]
    fn degrades_safely() {
        assert_eq!(cosine_similarity(&[], &[]), 0.0);
        assert_eq!(cosine_similarity(&[1.0, 2.0], &[1.0]), 0.0);
        assert_eq!(cosine_similarity(&[0.0, 0.0], &[1.0, 1.0]), 0.0);
    }

    // what this catches: embeddings are REPRODUCIBLE across calls (FNV, not the
    // randomized DefaultHasher) — required for replay/record-recreate to work.
    #[test]
    fn embeddings_are_reproducible() {
        let e = LexicalEmbedder::new();
        assert_eq!(e.embed("reproducible vectors"), e.embed("reproducible vectors"));
    }
}
