//! A test embedder that counts its work: the one fixture for asserting how often an
//! embedding was actually computed (the cache's reuse, a mint's single batch). Gated like
//! every fixture, so production binaries cannot link it.

use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;

use super::EmbeddingProvider;

/// Counts single embeds (`calls`) and batched embeds (`batches`); each text's vector is
/// derived from its length, so equal content always gets the same vector.
pub struct CountingEmbedder {
    id: &'static str,
    pub calls: AtomicUsize,
    pub batches: AtomicUsize,
}

impl CountingEmbedder {
    pub fn new(id: &'static str) -> Self {
        Self {
            id,
            calls: AtomicUsize::new(0),
            batches: AtomicUsize::new(0),
        }
    }

    fn vector_for(text: &str) -> Vec<f32> {
        let n = text.len() as f32;
        vec![n, n + 1.0, n + 2.0, n + 3.0]
    }
}

#[async_trait]
impl EmbeddingProvider for CountingEmbedder {
    fn id(&self) -> &str {
        self.id
    }
    fn dim(&self) -> usize {
        4
    }
    async fn embed(&self, text: &str) -> Vec<f32> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Self::vector_for(text)
    }
    async fn embed_many(&self, texts: &[String]) -> Vec<Vec<f32>> {
        self.batches.fetch_add(1, Ordering::SeqCst);
        texts.iter().map(|t| Self::vector_for(t)).collect()
    }
}
