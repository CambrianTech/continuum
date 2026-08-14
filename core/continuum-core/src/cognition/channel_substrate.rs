//! Process-global shared channel substrate — the ONE place the shared element
//! cache, digest builder, and pre-staged digest buffer live.
//!
//! NOT the read cursor. That is durable airc state (`runtime_cursor`) reached
//! through `AircTranscriptReader::read_cursor` / `advance_read_cursor`; a
//! process-global copy of it was a second source of truth that also died on
//! restart.
//!
//! These are singletons because the whole point of the consolidation design
//! (CONCURRENT-MIND §3.3) is that an element's artifacts are computed ONCE and
//! shared across every persona. A per-persona cache would defeat that — 14 personas
//! must reuse one `ChannelElement` per message ([[consolidate-before-concern-shared-elements-via-cache]]).
//! Same global-singleton pattern as `global_embedding_cache()`.
//!
//! - [`global_channel_digest_builder`] is what a RAG source calls to build a
//!   digest on demand (the lazy-compute-once path).
//! - [`global_channel_digest_buffer`] is what [`ChannelDigestRegion`] pre-stages
//!   into and a consumer peeks — the SAME buffer, so pre-staged and on-demand share
//!   one substrate (not a fallback: identical output, one builder).

use std::sync::{Arc, OnceLock};

use crate::cognition::channel_digest::ChannelDigestBuilder;
use crate::cognition::channel_digest_region::DigestBuffer;
use crate::cognition::channel_element::ChannelElementCache;
use crate::cognition::embedding::{CachingEmbeddingProvider, LexicalEmbedder};

/// The shared element cache. Its embedder is the content-addressed lexical
/// bootstrap for now — element embeddings are LAZY and currently uninvoked by the
/// digest-to-prompt path (digests carry text, not vectors), so the choice is inert
/// until recall relevance reads `ChannelElement::embedding`. When that lands it MUST
/// unify with `resolve_recall_embedder` so channel-element and recall vectors share
/// one embedding space ([[embeddings-are-per-content-computed-once-shared]]).
pub fn global_channel_element_cache() -> Arc<ChannelElementCache> {
    static G: OnceLock<Arc<ChannelElementCache>> = OnceLock::new();
    G.get_or_init(|| {
        let embedder = Arc::new(CachingEmbeddingProvider::new(Arc::new(
            LexicalEmbedder::new(),
        )));
        Arc::new(ChannelElementCache::new(embedder))
    })
    .clone()
}

/// The shared digest builder over the global element cache. Both the region
/// (pre-staging) and any on-demand consumer build through THIS, so a pre-staged
/// digest and an on-demand one come from the same cache — one shape. The cursor is
/// NOT here: it is read per-build from airc, so the builder holds no reader state.
pub fn global_channel_digest_builder() -> Arc<ChannelDigestBuilder> {
    static G: OnceLock<Arc<ChannelDigestBuilder>> = OnceLock::new();
    G.get_or_init(|| Arc::new(ChannelDigestBuilder::new(global_channel_element_cache())))
        .clone()
}

/// The shared pre-staged-digest ready-buffer. `ChannelDigestRegion` publishes here;
/// the RAG source peeks here. One buffer = pre-staged and consumer agree.
pub fn global_channel_digest_buffer() -> Arc<DigestBuffer> {
    static G: OnceLock<Arc<DigestBuffer>> = OnceLock::new();
    G.get_or_init(|| Arc::new(DigestBuffer::new())).clone()
}
