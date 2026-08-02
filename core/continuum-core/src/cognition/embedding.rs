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

use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use dashmap::DashMap;

use crate::ai::adapter::AIProviderAdapter;
use crate::ai::types::{EmbeddingInput, EmbeddingRequest};

/// Soft bound on the global embedding cache so a long-lived grid node servicing a
/// busy room can't grow it without limit (the substrate's pressure doctrine — an
/// unbounded hot-path cache is a leak at scale). On overflow we evict one
/// arbitrary entry; a proper LRU / `PagedResourcePool` tie-in is the follow-up.
const EMBEDDING_CACHE_MAX: usize = 20_000;

/// A backend that turns text into a dense vector. **Async** because real backends
/// do IO: a neural embedder runs a model forward pass, a grid embedder makes a
/// cross-grid round-trip. The lexical bootstrap is sync-bodied (cheap) but still
/// exposes the async signature so all backends are interchangeable. The
/// content-addressed cache (CachingEmbeddingProvider) makes repeat embeds a sync
/// map hit, so the async cost is paid once per unique content.
#[async_trait]
pub trait EmbeddingProvider: Send + Sync {
    /// Stable identifier for the embedding space (so vectors from different
    /// providers are never silently mixed).
    fn id(&self) -> &str;
    /// The vector dimensionality.
    fn dim(&self) -> usize;
    /// Embed text into a (typically L2-normalized) vector of length `dim()`.
    async fn embed(&self, text: &str) -> Vec<f32>;
    /// The MEASURED null distribution of this embedder's cosine over UNRELATED
    /// text pairs: `(mean, std)`. Neural embedding spaces are anisotropic —
    /// unrelated texts do NOT score ~0 (Qwen3-Embedding baselines near 0.25–0.3)
    /// — so any absolute cosine threshold calibrated for one embedder silently
    /// breaks under another (glass-boxed live 2026-07-10: a 0.15 recall floor
    /// filtered nothing under the neural embedder and saturated-salience chatter
    /// surfaced identically on 11/11 unrelated coding tasks). Consumers gate by
    /// SIGNIFICANCE against this null — "does this pair score like an unrelated
    /// pair?" — pure geometry, embedder-agnostic and language-agnostic by
    /// construction (never strings/keywords: a German room must work identically).
    /// `None` = this provider has not calibrated; consumers fall back to their
    /// legacy behavior. Implementations MEASURE it (embed [`CALIBRATION_PAIRS`]
    /// once at init), never assume it.
    fn unrelated_null(&self) -> Option<(f32, f32)> {
        None
    }
}

/// Canned UNRELATED text pairs for measuring an embedder's cosine null
/// distribution. Calibration probes (measurement data), not matching logic —
/// deliberately diverse in topic, register, length AND LANGUAGE (an English-only
/// null would mis-measure the space a multilingual room actually queries in).
pub const CALIBRATION_PAIRS: &[(&str, &str)] = &[
    ("the invoice for March is overdue", "a heron stood motionless in the shallows"),
    ("fn main() { println!(\"hello\"); }", "she packed two sweaters for the trip north"),
    ("die Sitzung wurde auf Donnerstag verschoben", "el río bajaba turbio después de la tormenta"),
    ("our quarterly revenue grew eight percent", "the sonata's third movement is in A minor"),
    ("git rebase rewrites commit history", "la soupe manque de sel et d'une feuille de laurier"),
    ("降雨量は流域全体で予想を上回った", "the defendant waived the right to a jury"),
    ("the cache invalidation bug ships tomorrow", "auf dem Bergrücken blühten die Wildblumen früh"),
    ("please review the attached slide deck", "der Springer gabelte Dame und Turm"),
];

/// Measure an embedder's unrelated-cosine null distribution over
/// [`CALIBRATION_PAIRS`]: embed each pair, cosine each, return `(mean, std)`.
/// Population std — the pairs ARE the calibration population. ~16 embeds, paid
/// once at init (and cached by the content-addressed cache wrapper).
pub async fn measure_unrelated_null(provider: &dyn EmbeddingProvider) -> (f32, f32) {
    let mut cosines = Vec::with_capacity(CALIBRATION_PAIRS.len());
    for (a, b) in CALIBRATION_PAIRS {
        let (va, vb) = futures::join!(provider.embed(a), provider.embed(b));
        cosines.push(cosine_similarity(&va, &vb));
    }
    let n = cosines.len() as f32;
    let mean = cosines.iter().sum::<f32>() / n;
    let std = (cosines.iter().map(|c| (c - mean).powi(2)).sum::<f32>() / n).sqrt();
    (mean, std)
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
    // Reject zero AND non-finite norms. A future EmbeddingProvider (neural/grid)
    // could return a NaN/Inf component; without this the `== 0.0` guard passes,
    // cosine returns NaN, and a NaN salience poisons the arbiter's sort
    // (every partial_cmp → Equal = arbitrary order). Fail to "no signal", never NaN.
    if !na.is_finite() || !nb.is_finite() || na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    let sim = dot / (na.sqrt() * nb.sqrt());
    if sim.is_finite() {
        sim
    } else {
        0.0
    }
}

/// Bootstrap embedder: hashing-trick term-frequency vectors. Lowercase, split on
/// non-alphanumeric, hash each token into one of `DIM` buckets, count, then
/// L2-normalize so cosine is a dot product. Captures topical word overlap — a
/// query sharing vocabulary with a memory scores high regardless of recency.
pub struct LexicalEmbedder {
    dim: usize,
    /// Measured unrelated-cosine null (mean, std) over [`CALIBRATION_PAIRS`] —
    /// computed at construction (pure CPU, trivial). For a token-overlap space
    /// this is genuinely ≈ (0, 0): disjoint vocabularies share no buckets.
    null: (f32, f32),
}

impl Default for LexicalEmbedder {
    fn default() -> Self {
        // 512 buckets — enough to keep collisions low for short engram/burst text
        // without paying for a real model.
        let mut s = Self {
            dim: 512,
            null: (0.0, 0.0),
        };
        s.null = s.measure_null_sync();
        s
    }
}

impl LexicalEmbedder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sync calibration over [`CALIBRATION_PAIRS`] — the embed body is pure CPU,
    /// so the lexical space can measure its null at construction, no async needed.
    fn measure_null_sync(&self) -> (f32, f32) {
        let cosines: Vec<f32> = CALIBRATION_PAIRS
            .iter()
            .map(|(a, b)| cosine_similarity(&self.embed_sync(a), &self.embed_sync(b)))
            .collect();
        let n = cosines.len() as f32;
        let mean = cosines.iter().sum::<f32>() / n;
        let std = (cosines.iter().map(|c| (c - mean).powi(2)).sum::<f32>() / n).sqrt();
        (mean, std)
    }

    /// The pure embed body — shared by the async trait method and the sync
    /// construction-time calibration (one implementation, two entry points).
    fn embed_sync(&self, text: &str) -> Vec<f32> {
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

#[async_trait]
impl EmbeddingProvider for LexicalEmbedder {
    fn id(&self) -> &str {
        "lexical-fnv-tf"
    }

    fn dim(&self) -> usize {
        self.dim
    }

    async fn embed(&self, text: &str) -> Vec<f32> {
        self.embed_sync(text)
    }

    fn unrelated_null(&self) -> Option<(f32, f32)> {
        Some(self.null)
    }
}

/// Content-addressed embedding cache — compute ONCE per message content, share
/// across every persona. An embedding is a property of the CONTENT, not the
/// persona (exactly like a vision description or an STT transcript): 14 personas
/// in a room reuse ONE embedding per message, never 14. Keyed by (embedding
/// space, content) so vectors from different embedders never collide — cosine
/// across spaces is arithmetic nonsense. Global + shared; the hot path is a
/// `DashMap::get`. Optimization-first: the sharing is the seam, not a later pass.
pub struct EmbeddingCache {
    map: DashMap<u64, Vec<f32>>,
}

impl Default for EmbeddingCache {
    fn default() -> Self {
        Self { map: DashMap::new() }
    }
}

impl EmbeddingCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// FNV-1a-64 over `provider_id \0 text` — keys the vector to its embedding
    /// SPACE as well as its content. Deterministic (replay-safe); collision
    /// probability is negligible at realistic message volumes.
    fn key(provider_id: &str, text: &str) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for b in provider_id.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h ^= 0xff; // real separator byte so provider_id/text boundary can't alias
        h = h.wrapping_mul(0x100000001b3);
        for b in text.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }

    /// Number of vectors currently cached.
    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// Serialize the cache to a compact, dep-free binary snapshot at `path`
    /// (atomic: write a sibling temp file, then rename). This is what lets a
    /// content's vector survive a core restart — the difference between
    /// re-embedding 375 memories (and re-fighting for VRAM) on every boot and a
    /// warm cache that embeds each unique content ONCE, ever. Format, all
    /// little-endian: `[u64 count]` then per entry `[u64 key][u32 dim][dim × f32]`.
    /// Best-effort: the caller treats a failure as a warn, never fatal — a lost
    /// snapshot just means the cache rebuilds by re-embedding.
    pub fn snapshot_to(&self, path: &std::path::Path) -> std::io::Result<usize> {
        use std::io::Write;
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = path.with_extension("bin.tmp");
        let mut w = std::io::BufWriter::new(std::fs::File::create(&tmp)?);
        // Snapshot the keys first so the count matches the bytes even if the map
        // grows during the write (a concurrent insert simply lands in the next
        // snapshot). Iterating clones under DashMap's per-shard locks — brief.
        let entries: Vec<(u64, Vec<f32>)> =
            self.map.iter().map(|e| (*e.key(), e.value().clone())).collect();
        w.write_all(&(entries.len() as u64).to_le_bytes())?;
        for (key, vec) in &entries {
            w.write_all(&key.to_le_bytes())?;
            w.write_all(&(vec.len() as u32).to_le_bytes())?;
            for f in vec {
                w.write_all(&f.to_le_bytes())?;
            }
        }
        w.flush()?;
        drop(w);
        std::fs::rename(&tmp, path)?;
        Ok(entries.len())
    }

    /// Load a snapshot written by [`snapshot_to`] into the map (additive; honors
    /// the [`EMBEDDING_CACHE_MAX`] cap; skips a truncated tail rather than
    /// failing). A missing file is `Ok(0)` — first boot, nothing to warm from.
    /// Returns the count loaded.
    pub fn load_from(&self, path: &std::path::Path) -> std::io::Result<usize> {
        let bytes = match std::fs::read(path) {
            Ok(b) => b,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
            Err(e) => return Err(e),
        };
        let mut pos = 0usize;
        let take = |pos: &mut usize, n: usize| -> Option<&[u8]> {
            if *pos + n > bytes.len() {
                return None;
            }
            let s = &bytes[*pos..*pos + n];
            *pos += n;
            Some(s)
        };
        let count = match take(&mut pos, 8) {
            Some(b) => u64::from_le_bytes(b.try_into().unwrap()) as usize,
            None => return Ok(0),
        };
        let mut loaded = 0usize;
        for _ in 0..count {
            if self.map.len() >= EMBEDDING_CACHE_MAX {
                break;
            }
            let key = match take(&mut pos, 8) {
                Some(b) => u64::from_le_bytes(b.try_into().unwrap()),
                None => break, // truncated tail — keep what parsed
            };
            let dim = match take(&mut pos, 4) {
                Some(b) => u32::from_le_bytes(b.try_into().unwrap()) as usize,
                None => break,
            };
            let raw = match take(&mut pos, dim * 4) {
                Some(b) => b,
                None => break,
            };
            let vec: Vec<f32> = raw
                .chunks_exact(4)
                .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
                .collect();
            self.map.insert(key, vec);
            loaded += 1;
        }
        Ok(loaded)
    }
}

/// Cadence for the embedding-cache snapshot — a background consolidator, so the
/// slower end of the ladder (concurrency guide: 30s–5min). A crash loses at most
/// this window of newly-embedded vectors, which simply re-embed on next use.
const EMBEDDING_CACHE_FLUSH_SECS: u64 = 60;

/// Warm the process-global embedding cache from `path` NOW (boot warm-start), then
/// snapshot it every [`EMBEDDING_CACHE_FLUSH_SECS`] on its OWN tokio task — the
/// RTOS shape (own task + `tokio::time::interval`, off every hot path; the file
/// write itself is `spawn_blocking`). Best-effort throughout: a failed load or
/// flush is a warn, never fatal. Call once at boot, inside the tokio runtime.
pub fn spawn_embedding_cache_persistence(cache: Arc<EmbeddingCache>, path: std::path::PathBuf) {
    match cache.load_from(&path) {
        Ok(n) => crate::probe!(
            class = "embedding.cache.loaded",
            vectors = n,
            "warmed embedding cache from durable snapshot — no re-embed for cached content"
        ),
        Err(e) => tracing::warn!(
            target = "embedding_cache",
            "embedding cache warm-load failed ({e}) — starting cold, will re-embed on demand"
        ),
    }
    tokio::spawn(async move {
        let mut ticker =
            tokio::time::interval(std::time::Duration::from_secs(EMBEDDING_CACHE_FLUSH_SECS));
        ticker.tick().await; // consume the immediate first tick
        loop {
            ticker.tick().await;
            let cache = cache.clone();
            let path = path.clone();
            let result = tokio::task::spawn_blocking(move || cache.snapshot_to(&path)).await;
            match result {
                Ok(Ok(n)) => crate::probe!(
                    class = "embedding.cache.flushed",
                    vectors = n,
                    "snapshotted embedding cache to durable store"
                ),
                Ok(Err(e)) => tracing::warn!(
                    target = "embedding_cache",
                    "embedding cache flush failed ({e}) — cache stays in-memory, retries next tick"
                ),
                Err(join) => tracing::warn!(
                    target = "embedding_cache",
                    "embedding cache flush task panicked ({join}) — skipping this tick"
                ),
            }
        }
    });
}

/// Process-global content-addressed embedding cache — the one place a message's
/// vector lives, shared by every persona. Same global-singleton pattern as the
/// persona-workspace registry and the ai_provider registry.
pub fn global_embedding_cache() -> Arc<EmbeddingCache> {
    static G: OnceLock<Arc<EmbeddingCache>> = OnceLock::new();
    G.get_or_init(|| Arc::new(EmbeddingCache::new())).clone()
}

/// Wraps any [`EmbeddingProvider`] with the content-addressed cache. `embed` is a
/// `DashMap::get` on the hot path; the inner provider (lexical now; neural / grid
/// later) fires ONLY on a miss, and the result is shared with every other persona
/// that embeds the same content. This is how "compute once per element, reuse
/// across 14 personas" is enforced structurally.
pub struct CachingEmbeddingProvider {
    inner: Arc<dyn EmbeddingProvider>,
    cache: Arc<EmbeddingCache>,
}

impl CachingEmbeddingProvider {
    /// Wrap `inner`, sharing the process-global cache — what makes the
    /// compute-once-across-personas property hold in production.
    pub fn new(inner: Arc<dyn EmbeddingProvider>) -> Self {
        Self {
            inner,
            cache: global_embedding_cache(),
        }
    }

    /// Wrap `inner` against a specific cache (tests / isolated benches).
    pub fn with_cache(inner: Arc<dyn EmbeddingProvider>, cache: Arc<EmbeddingCache>) -> Self {
        Self { inner, cache }
    }
}

#[async_trait]
impl EmbeddingProvider for CachingEmbeddingProvider {
    fn id(&self) -> &str {
        self.inner.id()
    }

    fn dim(&self) -> usize {
        self.inner.dim()
    }

    fn unrelated_null(&self) -> Option<(f32, f32)> {
        // The null is a property of the embedding SPACE — the cache wrapper
        // changes nothing about the geometry, so it delegates.
        self.inner.unrelated_null()
    }

    async fn embed(&self, text: &str) -> Vec<f32> {
        let key = EmbeddingCache::key(self.inner.id(), text);
        if let Some(v) = self.cache.map.get(&key) {
            return v.clone(); // hot path: sync map hit, the async cost is paid once
        }
        let v = self.inner.embed(text).await;
        // NEVER cache a failed embed. The inner provider degrades a failure to an
        // empty vector (the "no signal" sentinel); caching that would POISON the
        // shared content-addressed cache — a transient embedder hiccup would become
        // a permanent zero-relevance verdict for this content across every persona,
        // never re-attempted. A real embedding is always dim()-length and non-empty,
        // so empty unambiguously means "failed this time": skip the cache, let the
        // next miss retry. (The failure itself is surfaced loud by the inner provider.)
        if v.is_empty() {
            return v;
        }
        // Bound memory: evict one arbitrary entry on overflow before inserting a
        // genuinely new key. Hot/recent content re-populates on its next miss.
        if !self.cache.map.contains_key(&key) && self.cache.map.len() >= EMBEDDING_CACHE_MAX {
            if let Some(victim) = self.cache.map.iter().next().map(|e| *e.key()) {
                self.cache.map.remove(&victim);
            }
        }
        self.cache.map.insert(key, v.clone());
        v
    }
}

/// Neural embedder behind the trait — semantic relevance, not just lexical
/// overlap. Wraps an `AIProviderAdapter` that has loaded a retrieval embedding
/// model (the grid's canonical Qwen3-Embedding-0.6B); `embed` calls
/// `create_embedding` (GPU forward pass via llama.cpp Metal/CUDA — the fast path).
///
/// `id()` returns the MODEL SLUG (not a transport prefix) so vectors share cache
/// space with the grid embedder serving the same model ("identity is the model,
/// transport is the policy"). Wrapped in `CachingEmbeddingProvider` in production
/// so the GPU embed runs once per unique content and every persona reuses it.
pub struct NeuralEmbeddingProvider {
    adapter: Arc<dyn AIProviderAdapter>,
    /// The canonical model slug = the embedding space identity + cache key.
    model_slug: String,
    /// Vector dimensionality (Qwen3-Embedding-0.6B = 1024).
    dim: usize,
    /// Measured unrelated-cosine null — set once by [`calibrate`](Self::calibrate)
    /// at resolve time (a neural space MUST measure its anisotropy; assuming ~0
    /// is the bug class that let a 0.15 floor filter nothing).
    null: std::sync::OnceLock<(f32, f32)>,
}

impl NeuralEmbeddingProvider {
    pub fn new(
        adapter: Arc<dyn AIProviderAdapter>,
        model_slug: impl Into<String>,
        dim: usize,
    ) -> Self {
        Self {
            adapter,
            model_slug: model_slug.into(),
            dim,
            null: std::sync::OnceLock::new(),
        }
    }

    /// Measure this space's unrelated-cosine null over [`CALIBRATION_PAIRS`]
    /// (~16 forward passes, paid once at resolve time). Idempotent.
    pub async fn calibrate(&self) -> (f32, f32) {
        if let Some(n) = self.null.get() {
            return *n;
        }
        let measured = measure_unrelated_null(self).await;
        *self.null.get_or_init(|| measured)
    }
}

#[async_trait]
impl EmbeddingProvider for NeuralEmbeddingProvider {
    fn id(&self) -> &str {
        &self.model_slug
    }

    fn dim(&self) -> usize {
        self.dim
    }

    async fn embed(&self, text: &str) -> Vec<f32> {
        let request = EmbeddingRequest {
            input: EmbeddingInput::Single(text.to_string()),
            model: Some(self.model_slug.clone()),
            provider: None,
        };
        match self.adapter.create_embedding(request).await {
            Ok(resp) => resp.embeddings.into_iter().next().unwrap_or_default(),
            Err(e) => {
                // Degrade to "no signal" (empty → cosine 0 by cosine_similarity's
                // contract), never junk vectors — but VISIBLY, never silently. A
                // tracing::warn! does not survive across the substrate's tokio tasks
                // (it gets filtered/lost — the recurring "swallowed error" trap); a
                // probe! is the surviving RTOS-style signal that a configured embedder
                // is actually DOWN, so a half-blind recall is diagnosable instead of
                // looking identical to a genuinely-irrelevant memory. The caller still
                // degrades (doesn't panic — faculties degrade, never panic), but the
                // failure is named and observable, not absorbed.
                crate::probe!(
                    class = "embedding.neural.failed",
                    model = %self.model_slug,
                    error = %e,
                    "neural embed failed — degrading this item to no-relevance (embedder may be down)"
                );
                Vec::new()
            }
        }
    }

    fn unrelated_null(&self) -> Option<(f32, f32)> {
        self.null.get().copied()
    }
}

// ─── Live recall embedder resolution ───────────────────────────────────────────

/// The grid's canonical retrieval embedding model + its dimensionality. One
/// embedding SPACE across the grid keeps vectors comparable everywhere
/// ([[embeddings-are-per-content-computed-once-shared]]). Overridable via
/// `UNSLOTH_EMBED_MODEL` for a grid that serves a different embed model.
pub const CANONICAL_EMBED_MODEL: &str = "qwen3-embedding-0.6b";
/// Qwen3-Embedding-0.6B vector dimensionality (advisory metadata; the real
/// length comes from the response).
pub const CANONICAL_EMBED_DIM: usize = 1024;

/// Does a probe vector indicate a *working* neural embedder? Usable = non-empty,
/// at least one non-zero component, and all-finite. An empty / all-zero / NaN
/// probe means the embed model isn't actually serving (model not loaded, endpoint
/// error → `NeuralEmbeddingProvider` degraded to an empty vec), so the caller must
/// fall back rather than embed every memory into "no signal". Pure → TDD'd.
fn probe_indicates_usable(probe: &[f32]) -> bool {
    !probe.is_empty() && probe.iter().any(|x| *x != 0.0) && probe.iter().all(|x| x.is_finite())
}

/// Build the dedicated **in-process** embedding adapter: a `LlamaCppAdapter`
/// bound to the registry's `llamacpp-local` embedding model (the canonical
/// Qwen3-Embedding-0.6B) whose GGUF is already pulled to disk.
///
/// This is the PREFERRED recall embedder and the reason recall doesn't flake
/// when the operator picks a different chat model: embeddings are pinned to this
/// dedicated embed model regardless of what Asha's brain is. It's a GPU forward
/// pass with NO HTTP hop — the chat adapter may be a remote gateway that doesn't
/// serve `/v1/embeddings` at all. `None` when no embedding GGUF is on disk (→ the
/// caller tries the chat adapter's embeddings, then the lexical floor).
///
/// Uses `try_global()`, never the panicking `global()`: the resolver's contract is
/// "always returns a usable embedder, never panics" (faculties degrade, never panic
/// — "solve for public users"). A box where the registry isn't up yet (early boot,
/// a focused test) is just another flavor of "no in-process embed model available
/// right now" → fall through to the chat adapter then the lexical floor. The
/// resolution is still observable via the `recall.embedder.resolved` probe, so this
/// is the resolver's explicit ladder, not a silent fallback.
fn local_embed_adapter() -> Option<(Arc<dyn AIProviderAdapter>, String)> {
    let reg = crate::model_registry::try_global()?;
    let model = reg
        .models_for_provider(crate::inference::llamacpp_adapter::LLAMACPP_PROVIDER_ID)
        .find(|m| {
            m.capabilities.contains(&crate::model_registry::Capability::Embedding)
                && m.gguf_local_path.as_ref().is_some_and(|p| p.exists())
        })?;
    let path = model.gguf_local_path.clone()?;
    let adapter = crate::inference::llamacpp_adapter::LlamaCppAdapter::with_model_id(
        path,
        model.id.clone(),
    );
    Some((Arc::new(adapter), model.id.clone()))
}

/// Probe a candidate adapter as a neural embedder; return the cache-wrapped
/// provider only when a one-shot embed proves the model actually serves usable
/// vectors. `None` → the caller tries the next source. `model` is the
/// embedding-SPACE identity (cache key) — the canonical slug, NOT the registry
/// id — so an in-process embed and a grid-gateway embed of the same model share
/// cache space and stay comparable.
async fn try_neural_embedder(
    adapter: Arc<dyn AIProviderAdapter>,
    model: &str,
) -> Option<Arc<dyn EmbeddingProvider>> {
    let neural = NeuralEmbeddingProvider::new(adapter, model.to_string(), CANONICAL_EMBED_DIM);
    let probe = neural.embed("probe").await;
    if !probe_indicates_usable(&probe) {
        return None;
    }
    // Calibrate the space's unrelated-cosine null NOW (~16 forward passes, once
    // per resolve). Neural spaces are anisotropic — consumers gate relevance by
    // significance against this MEASURED null, never an assumed-zero baseline
    // (the assumed-zero floor is the bug that let saturated chatter surface on
    // 11/11 unrelated coding tasks, 2026-07-10).
    let (mean, std) = neural.calibrate().await;
    crate::probe!(
        class = "recall.embedder.calibrated",
        model = %model,
        null_mean = mean,
        null_std = std,
        "measured unrelated-cosine null for the embedding space"
    );
    Some(Arc::new(CachingEmbeddingProvider::new(Arc::new(neural))) as Arc<dyn EmbeddingProvider>)
}

/// Process-global memo of rung 1 — the dedicated in-process embed model.
///
/// Glass-boxed live on BigMama's Windows node (2026-08-02): `resolve_recall_embedder`
/// runs once per persona REGISTRATION, and registration re-runs on every respawn
/// (node resilience). Before this memo, EVERY call built a fresh `LlamaCppAdapter`,
/// loaded the ~1.2 GB embed GGUF into a NEW llama context, probed it, and burned
/// ~16 calibration passes — nine personas meant nine resident embed contexts, and a
/// respawn-churning node became a context-creation storm (102 llama_context
/// creations × ~1200 MiB with ZERO IPC deltas) eating memory out from under the
/// ResourceGovernor. The resolver's own contract ("embedded ONCE and shared across
/// every persona") requires ONE shared provider; this state enforces it.
///
/// Semantics:
/// - SUCCESS memoizes permanently. The async lock makes resolution single-flight:
///   N concurrently-spawning personas produce exactly ONE model load; the rest
///   wait and share the Arc.
/// - FAILURE is never memoized permanently (the #71 lesson: a one-shot resolution
///   must not blind a box whose embed model arrives late) — but re-attempts are
///   rate-limited by [`RUNG1_RETRY_COOLDOWN`], so a box where the load or probe
///   fails cannot re-enter the load-probe-fail storm.
#[derive(Default)]
struct Rung1State {
    provider: Option<(Arc<dyn EmbeddingProvider>, String)>,
    last_attempt: Option<Instant>,
    /// Total real resolve attempts (loads) this process has made — the
    /// observable invariant the storm regression test pins.
    attempts: u32,
}

/// Cooldown between failed rung-1 resolve attempts. One knock per minute keeps
/// late-arriving embed models recoverable (a boot-race registry, a mid-session
/// model pull) while capping the worst case at ~1 model-load per minute instead
/// of one per persona respawn.
const RUNG1_RETRY_COOLDOWN: Duration = Duration::from_secs(60);

fn rung1_cell() -> &'static tokio::sync::Mutex<Rung1State> {
    static CELL: OnceLock<tokio::sync::Mutex<Rung1State>> = OnceLock::new();
    CELL.get_or_init(|| tokio::sync::Mutex::new(Rung1State::default()))
}

/// The ONE shared in-process embedder (rung 1), or `None` if it isn't available
/// right now. See [`Rung1State`] for the memo/single-flight/cooldown contract.
/// Returns the provider and the resolved GGUF id (for the caller's probe line).
async fn shared_in_process_embedder(model: &str) -> Option<(Arc<dyn EmbeddingProvider>, String)> {
    // Holding the async lock across the load IS the single-flight: concurrent
    // resolvers queue here and find `provider` populated when they wake.
    let mut state = rung1_cell().lock().await;
    if let Some((provider, gguf_id)) = &state.provider {
        return Some((provider.clone(), gguf_id.clone()));
    }
    if let Some(last) = state.last_attempt {
        if last.elapsed() < RUNG1_RETRY_COOLDOWN {
            return None;
        }
    }
    state.last_attempt = Some(Instant::now());
    state.attempts += 1;
    let (embed_adapter, gguf_id) = local_embed_adapter()?;
    let provider = try_neural_embedder(embed_adapter, model).await?;
    state.provider = Some((provider.clone(), gguf_id.clone()));
    Some((provider, gguf_id))
}

/// Resolve the embedder for the live recall path. Tries, in order:
///   1. the dedicated **in-process** embed model (GPU, no HTTP hop) — preferred;
///   2. the chat `adapter`'s `/v1/embeddings`, for a grid gateway that serves an
///      embedding lane alongside chat;
///   3. the **lexical** word-overlap floor (degrade-not-panic).
///
/// Each candidate is gated on a one-shot probe that proves the model actually
/// serves usable vectors. The choice is PROCESS-STABLE — decided once here, never
/// per-embed — because a query and the stored vectors must live in the SAME
/// embedding space (mixing neural and lexical per call makes cosine meaningless).
/// The result is wrapped in the content-addressed cache so each message is
/// embedded ONCE and shared across every persona.
///
/// EXACTLY ONE `recall.embedder.resolved` probe fires per resolution, naming the
/// resolved `kind` (neural|lexical) and `source`. That is the hardening of the
/// old silent fallback: a half-blind lexical recall can never again masquerade as
/// a healthy neural one — the resolved kind is a surviving, greppable signal.
///
/// Always returns a usable embedder — never errors, never panics: a persona on a
/// box with no embed model still gets real lexical relevance ("solve for public
/// users" / degrade-not-panic).
pub async fn resolve_recall_embedder(adapter: Arc<dyn AIProviderAdapter>) -> Arc<dyn EmbeddingProvider> {
    // The embedding-SPACE identity (cache key). Defaults to the canonical grid
    // embedder; an operator standardizing on a different embed model overrides it
    // so in-process and gateway vectors stay in one comparable space.
    let model = crate::config_env::read("UNSLOTH_EMBED_MODEL")
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| CANONICAL_EMBED_MODEL.to_string());

    // 1. Preferred: the dedicated in-process embed model. Pinned independently of
    //    the chat model, so changing Asha's brain never disturbs recall. ONE
    //    shared instance process-wide — see [`Rung1State`].
    if let Some((provider, gguf_id)) = shared_in_process_embedder(&model).await {
        crate::probe!(
            class = "recall.embedder.resolved",
            kind = "neural",
            source = "in-process-llamacpp",
            model = %model,
            gguf = %gguf_id,
            "recall embedder = NEURAL (dedicated in-process Qwen3-Embedding, shared)"
        );
        return provider;
    }

    // 2. Fallback: the chat adapter's embeddings endpoint, for a gateway that
    //    serves an embedding lane (e.g. unsloth/llama-server started --embeddings).
    if adapter
        .capabilities()
        .has(crate::model_registry::Capability::Embedding)
    {
        if let Some(provider) = try_neural_embedder(adapter, &model).await {
            crate::probe!(
                class = "recall.embedder.resolved",
                kind = "neural",
                source = "chat-adapter-embeddings",
                model = %model,
                "recall embedder = NEURAL (chat-adapter /v1/embeddings gateway)"
            );
            return provider;
        }
    }

    // 3. Floor: lexical word-overlap. Legitimate for a box with no embed model,
    //    but LOUD — a degraded semantic recall must be diagnosable, not silent.
    crate::probe!(
        class = "recall.embedder.resolved",
        kind = "lexical",
        source = "fallback",
        model = %model,
        "recall embedder = LEXICAL — no neural embed model serving; semantic recall DEGRADED to word-overlap"
    );
    Arc::new(CachingEmbeddingProvider::new(Arc::new(LexicalEmbedder::new())))
}

/// Resolve the recall embedder WITHOUT a chat adapter — for the GLOBAL memory
/// manager (agent-memory bridge + hydrated corpora), which has no per-persona
/// chat gateway to offer. Tries the dedicated in-process embed model (GPU, no
/// HTTP hop), then the lexical floor. Same process-stable, probe-gated,
/// LOUD-on-degrade contract as [`resolve_recall_embedder`]; it simply omits the
/// chat-adapter `/v1/embeddings` rung (rung 2) that only a persona's own adapter
/// can provide. Always returns a usable embedder — never errors, never panics.
pub async fn resolve_recall_embedder_local() -> Arc<dyn EmbeddingProvider> {
    let model = crate::config_env::read("UNSLOTH_EMBED_MODEL")
        .filter(|m| !m.trim().is_empty())
        .unwrap_or_else(|| CANONICAL_EMBED_MODEL.to_string());

    if let Some((provider, gguf_id)) = shared_in_process_embedder(&model).await {
        crate::probe!(
            class = "recall.embedder.resolved",
            kind = "neural",
            source = "in-process-llamacpp-global",
            model = %model,
            gguf = %gguf_id,
            "global recall embedder = NEURAL (dedicated in-process Qwen3-Embedding, shared)"
        );
        return provider;
    }

    crate::probe!(
        class = "recall.embedder.resolved",
        kind = "lexical",
        source = "fallback-global",
        model = %model,
        "global recall embedder = LEXICAL — no in-process embed model serving; semantic recall DEGRADED to word-overlap"
    );
    Arc::new(CachingEmbeddingProvider::new(Arc::new(LexicalEmbedder::new())))
}

/// A recall embedder that resolves its real backend LAZILY on first use, off the
/// boot critical path. The global memory manager is constructed during boot,
/// where NOTHING may gate the IPC socket bind on a GPU/gateway probe (the
/// concurrency guide's non-negotiable — a probe here previously wedged boot).
/// So the manager holds THIS: trivially cheap to construct, and on the first
/// `embed` it resolves the dedicated in-process neural embedder via
/// [`resolve_recall_embedder_local`] (probe-gated, LOUD-on-degrade), caches it
/// **process-stably** (one embedding space for the whole process — a query and
/// the stored vectors must live in the SAME space), and delegates. Every later
/// call reuses the resolved provider; the resolution's cost (probe + ~16
/// calibration embeds) is paid once, on the first real recall, never at boot.
pub struct LazyRecallEmbedder {
    resolved: tokio::sync::OnceCell<Arc<dyn EmbeddingProvider>>,
}

impl LazyRecallEmbedder {
    pub fn new() -> Self {
        Self {
            resolved: tokio::sync::OnceCell::new(),
        }
    }

    /// The resolved backend, resolving (once) on first call.
    async fn backend(&self) -> &Arc<dyn EmbeddingProvider> {
        self.resolved
            .get_or_init(resolve_recall_embedder_local)
            .await
    }
}

impl Default for LazyRecallEmbedder {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl EmbeddingProvider for LazyRecallEmbedder {
    /// The resolved space id once known; a stable placeholder until first embed.
    /// Only informational (degrade logging) — cache identity lives in the
    /// resolved provider's own `CachingEmbeddingProvider`/`NeuralEmbeddingProvider`.
    fn id(&self) -> &str {
        self.resolved
            .get()
            .map(|p| p.id())
            .unwrap_or("recall-embedder(resolving)")
    }

    fn dim(&self) -> usize {
        self.resolved.get().map(|p| p.dim()).unwrap_or(0)
    }

    async fn embed(&self, text: &str) -> Vec<f32> {
        self.backend().await.embed(text).await
    }

    fn unrelated_null(&self) -> Option<(f32, f32)> {
        self.resolved.get().and_then(|p| p.unrelated_null())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    // what this catches: the lazy recall embedder is safe to CONSTRUCT at boot —
    // no GPU/gateway probe fires until the first `embed` — and reports a stable
    // "resolving" identity + dim 0 until then, so nothing on the boot path can
    // wedge the IPC socket bind (the concurrency-guide non-negotiable this exists
    // to honor). Resolution itself needs the model registry, exercised live.
    #[test]
    fn lazy_recall_embedder_is_boot_safe_before_resolution() {
        let e = LazyRecallEmbedder::new();
        assert_eq!(e.id(), "recall-embedder(resolving)");
        assert_eq!(e.dim(), 0);
        assert!(e.unrelated_null().is_none());
    }

    // what this catches: the persistent snapshot survives a "restart" — vectors
    // written by snapshot_to load back byte-identical via load_from, so cached
    // content never re-embeds (nor re-fights the serving lane for VRAM) across a
    // core restart. A missing file warms as Ok(0), never an error (first boot).
    #[test]
    fn embedding_cache_snapshot_round_trips() {
        let src = EmbeddingCache::new();
        let ka = EmbeddingCache::key("qwen3-embedding-0.6b", "the deploy went red");
        let kb = EmbeddingCache::key("qwen3-embedding-0.6b", "a heron in the shallows");
        let va = vec![0.1f32, -0.2, 0.3, 0.4];
        let vb = vec![1.0f32, 2.0, 3.0];
        src.map.insert(ka, va.clone());
        src.map.insert(kb, vb.clone());

        let path = std::env::temp_dir().join("continuum-embcache-roundtrip.bin");
        let _ = std::fs::remove_file(&path);
        assert_eq!(src.snapshot_to(&path).unwrap(), 2);

        // A fresh cache (the "restarted" process) warms from the snapshot.
        let dst = EmbeddingCache::new();
        assert_eq!(dst.load_from(&path).unwrap(), 2);
        assert_eq!(dst.map.get(&ka).map(|e| e.clone()), Some(va));
        assert_eq!(dst.map.get(&kb).map(|e| e.clone()), Some(vb));

        // Missing file (first boot) → Ok(0), not an error.
        let _ = std::fs::remove_file(&path);
        assert_eq!(dst.load_from(&path).unwrap(), 0, "missing snapshot warms as Ok(0)");
    }

    // what this catches: the cosine of identical text is ~1; orthogonal
    // (no shared vocabulary) is ~0. The relevance primitive is sane.
    #[tokio::test]
    async fn identical_text_is_maximally_similar() {
        let e = LexicalEmbedder::new();
        let a = e.embed("the deploy pipeline went red after the migration").await;
        let same = e.embed("the deploy pipeline went red after the migration").await;
        assert!(cosine_similarity(&a, &same) > 0.999);
    }

    // what this catches: topical relevance — a query about the rollout plan is
    // MORE similar to the rollout memory than to an unrelated lunch memory, even
    // though both are equally "old". This is relevance, not recency.
    #[tokio::test]
    async fn relevant_text_outscores_unrelated_text() {
        let e = LexicalEmbedder::new();
        let query = e.embed("what was our rollout plan for the auth flow?").await;
        let relevant = e
            .embed("we will ship the auth flow behind a feature flag and ramp the rollout to 10%")
            .await;
        let unrelated = e.embed("lunch is at noon, someone booked the corner table").await;
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
        // Non-finite components → 0.0 ("no signal"), NEVER NaN — a NaN here
        // would poison the arbiter sort (every partial_cmp becomes Equal).
        assert_eq!(cosine_similarity(&[f32::NAN, 1.0], &[1.0, 1.0]), 0.0);
        assert_eq!(cosine_similarity(&[f32::INFINITY, 1.0], &[1.0, 1.0]), 0.0);
        assert!(cosine_similarity(&[f32::NAN, 1.0], &[1.0, 1.0]).is_finite());
    }

    // what this catches: embeddings are REPRODUCIBLE across calls (FNV, not the
    // randomized DefaultHasher) — required for replay/record-recreate to work.
    #[tokio::test]
    async fn embeddings_are_reproducible() {
        let e = LexicalEmbedder::new();
        assert_eq!(
            e.embed("reproducible vectors").await,
            e.embed("reproducible vectors").await
        );
    }

    /// An embedder that counts how many times it actually computed — to prove
    /// the cache prevents recomputation.
    struct CountingEmbedder {
        id: &'static str,
        calls: AtomicUsize,
    }
    impl CountingEmbedder {
        fn new(id: &'static str) -> Self {
            Self {
                id,
                calls: AtomicUsize::new(0),
            }
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
            // Deterministic per-content vector (content length spread over dims).
            let n = text.len() as f32;
            vec![n, n + 1.0, n + 2.0, n + 3.0]
        }
    }

    // what this catches: THE OPTIMIZATION — the same content is embedded ONCE;
    // the second call (any persona) is a cache hit, the inner embedder is NOT
    // re-invoked. This is the compute-once-per-content / 14-personas-reuse-one win.
    #[tokio::test]
    async fn cache_computes_once_and_reuses() {
        let inner = Arc::new(CountingEmbedder::new("counting"));
        let cache = Arc::new(EmbeddingCache::new());
        let cached = CachingEmbeddingProvider::with_cache(inner.clone(), cache);

        let a = cached.embed("the deploy went red").await;
        let b = cached.embed("the deploy went red").await; // any persona, same content
        assert_eq!(a, b, "cache returns the identical vector");
        assert_eq!(
            inner.calls.load(Ordering::SeqCst),
            1,
            "inner embedder computed ONCE despite two embed calls"
        );
    }

    // what this catches: distinct content is genuinely recomputed (the cache
    // isn't returning a stale vector for new text).
    #[tokio::test]
    async fn cache_recomputes_distinct_content() {
        let inner = Arc::new(CountingEmbedder::new("counting"));
        let cache = Arc::new(EmbeddingCache::new());
        let cached = CachingEmbeddingProvider::with_cache(inner.clone(), cache);
        let _ = cached.embed("first message").await;
        let _ = cached.embed("a different message").await;
        assert_eq!(inner.calls.load(Ordering::SeqCst), 2);
    }

    /// An embedder that fails the first time (empty = "no signal", the down-embedder
    /// sentinel) then succeeds — to prove a transient failure is never cached.
    struct FlakyEmbedder {
        calls: AtomicUsize,
    }
    #[async_trait]
    impl EmbeddingProvider for FlakyEmbedder {
        fn id(&self) -> &str {
            "flaky"
        }
        fn dim(&self) -> usize {
            4
        }
        async fn embed(&self, _text: &str) -> Vec<f32> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            if n == 0 {
                Vec::new() // first call: embedder "down" → no-signal sentinel
            } else {
                vec![1.0, 2.0, 3.0, 4.0] // recovered
            }
        }
    }

    // what this catches: CACHE POISONING. A failed embed (empty no-signal vector)
    // must NOT be cached — otherwise a transient embedder hiccup becomes a permanent
    // zero-relevance verdict for that content across every persona, never retried.
    // Regression here = one bad embed silently blinding recall to a memory forever.
    #[tokio::test]
    async fn failed_embed_is_not_cached_and_retries() {
        let inner = Arc::new(FlakyEmbedder {
            calls: AtomicUsize::new(0),
        });
        let cache = Arc::new(EmbeddingCache::new());
        let cached = CachingEmbeddingProvider::with_cache(inner.clone(), cache);

        let first = cached.embed("the embedder is down right now").await;
        assert!(first.is_empty(), "first embed failed → no-signal sentinel");

        // Same content again: because the failure was NOT cached, the inner embedder
        // is re-invoked (now recovered) instead of returning the poisoned empty.
        let second = cached.embed("the embedder is down right now").await;
        assert_eq!(
            second,
            vec![1.0, 2.0, 3.0, 4.0],
            "retry recomputed a real vector — the failure was not cached"
        );
        assert_eq!(
            inner.calls.load(Ordering::SeqCst),
            2,
            "inner embedder was retried, proving the empty was never cached"
        );
    }

    // what this catches: the probe gate — only a non-empty, non-zero, all-finite
    // vector counts as "neural is live". Empty / all-zero / NaN all mean "no
    // signal" → fall back to lexical. Regression here = embedding every memory
    // into a zero vector (recall returns nothing) instead of using lexical.
    #[test]
    fn probe_gate_distinguishes_usable_from_no_signal() {
        assert!(probe_indicates_usable(&[0.0, 0.1, 0.0]));
        assert!(!probe_indicates_usable(&[]), "empty = no signal");
        assert!(!probe_indicates_usable(&[0.0, 0.0]), "all-zero = no signal");
        assert!(!probe_indicates_usable(&[f32::NAN, 1.0]), "NaN = no signal");
        assert!(!probe_indicates_usable(&[f32::INFINITY, 1.0]), "Inf = no signal");
    }

    /// Minimal adapter whose embeddings are configurable, to drive the resolver
    /// without a network. `embeds` = the vector create_embedding returns (empty
    /// simulates "model not loaded"); `supports` = the capability flag.
    struct FakeEmbedAdapter {
        supports: bool,
        embeds: Vec<f32>,
    }
    #[async_trait]
    impl AIProviderAdapter for FakeEmbedAdapter {
        fn provider_id(&self) -> &str {
            "fake"
        }
        fn name(&self) -> &str {
            "fake"
        }
        fn default_model(&self) -> &str {
            "fake-model"
        }
        fn capabilities(&self) -> crate::ai::adapter::AdapterCapabilities {
            let mut capabilities = std::collections::BTreeSet::new();
            if self.supports {
                capabilities.insert(crate::model_registry::Capability::Embedding);
            }
            crate::ai::adapter::AdapterCapabilities {
                capabilities,
                ..Default::default()
            }
        }
        async fn generate_text(
            &self,
            _request: crate::ai::types::TextGenerationRequest,
        ) -> Result<crate::ai::types::TextGenerationResponse, String> {
            Err("not used".into())
        }
        async fn create_embedding(
            &self,
            _request: crate::ai::types::EmbeddingRequest,
        ) -> Result<crate::ai::types::EmbeddingResponse, String> {
            Ok(crate::ai::types::EmbeddingResponse {
                embeddings: vec![self.embeds.clone()],
                model: "fake-model".into(),
                provider: "fake".into(),
                usage: crate::ai::types::UsageMetrics {
                    input_tokens: 0,
                    output_tokens: 0,
                    total_tokens: 0,
                    estimated_cost: None,
                },
                response_time_ms: 0,
            })
        }
    }

    // what this catches: when the adapter serves real embeddings, the live path
    // resolves to the NEURAL embedder (semantic recall), not lexical. id() of a
    // neural-backed provider is the model slug.
    #[tokio::test]
    async fn resolver_picks_neural_when_embed_model_serves() {
        let adapter = Arc::new(FakeEmbedAdapter {
            supports: true,
            embeds: vec![0.1, 0.2, 0.3],
        });
        let embedder = resolve_recall_embedder(adapter).await;
        assert_eq!(
            embedder.id(),
            CANONICAL_EMBED_MODEL,
            "neural embedder's id is the model slug"
        );
    }

    // what this catches: THE NO-SIGNAL GUARD — an adapter that claims embedding
    // support but returns an empty vector (model not loaded) must fall back to
    // lexical, NOT embed everything into nothing.
    #[tokio::test]
    async fn resolver_falls_back_to_lexical_when_probe_empty() {
        let adapter = Arc::new(FakeEmbedAdapter {
            supports: true,
            embeds: vec![],
        });
        let embedder = resolve_recall_embedder(adapter).await;
        assert_eq!(embedder.id(), "lexical-fnv-tf", "must fall back to lexical");
    }

    // what this catches: an adapter with no embedding capability skips the probe
    // entirely and uses lexical — no wasted round-trip, always a working embedder.
    #[tokio::test]
    async fn resolver_uses_lexical_when_adapter_lacks_embeddings() {
        let adapter = Arc::new(FakeEmbedAdapter {
            supports: false,
            embeds: vec![0.1],
        });
        let embedder = resolve_recall_embedder(adapter).await;
        assert_eq!(embedder.id(), "lexical-fnv-tf");
    }

    // what this catches: vectors from DIFFERENT embedding spaces (provider ids)
    // never collide in the cache — keying includes provider_id, so a lexical
    // vector is never served to a neural query (cosine across spaces is nonsense).
    #[tokio::test]
    async fn cache_keys_by_embedding_space() {
        let cache = Arc::new(EmbeddingCache::new());
        let lexical = CachingEmbeddingProvider::with_cache(
            Arc::new(CountingEmbedder::new("space-a")),
            cache.clone(),
        );
        let neural = CachingEmbeddingProvider::with_cache(
            Arc::new(CountingEmbedder::new("space-b")),
            cache.clone(),
        );
        let _ = lexical.embed("same text").await;
        let _ = neural.embed("same text").await;
        assert_eq!(
            cache.len(),
            2,
            "same text in two embedding spaces = two distinct cache entries"
        );
    }

    // what this catches: TRANSPORT IS INVISIBLE TO THE CACHE — "identity is the
    // model, transport is the policy" (IntelMac/BigMama contract). Two DIFFERENT
    // provider impls (e.g. local NeuralEmbeddingProvider + cross-grid
    // GridEmbeddingProvider) that serve the SAME model return the SAME
    // provider_id, so a vector computed by one is reused by the other — same
    // model, same space, same cache key. The grid round-trip never fires for
    // content the local path already embedded.
    #[tokio::test]
    async fn cache_shared_across_transports_with_same_model_slug() {
        let cache = Arc::new(EmbeddingCache::new());
        // Two distinct impls, SAME model slug — stand-ins for Neural (local) and
        // Grid (cross-grid), both serving qwen3-embedding-0.6b.
        let local = Arc::new(CountingEmbedder::new("qwen3-embedding-0.6b"));
        let grid = Arc::new(CountingEmbedder::new("qwen3-embedding-0.6b"));
        let neural = CachingEmbeddingProvider::with_cache(local.clone(), cache.clone());
        let grid_provider = CachingEmbeddingProvider::with_cache(grid.clone(), cache.clone());

        let a = neural.embed("the deploy went red").await; // local path computes + caches
        let b = grid_provider.embed("the deploy went red").await; // grid path hits the SAME entry
        assert_eq!(a, b, "same model slug → same cache entry across transports");
        assert_eq!(local.calls.load(Ordering::SeqCst), 1, "local computed once");
        assert_eq!(
            grid.calls.load(Ordering::SeqCst),
            0,
            "grid round-trip never fired — the local-cached vector was reused"
        );
        assert_eq!(cache.len(), 1, "one entry: identity is the model, not the transport");
    }

    // what this catches: the in-process neural embedder actually PRODUCES semantic
    // vectors — runtime proof the resolve_recall_embedder neural path works end to
    // end (registry lookup → LlamaCppAdapter → backend.embed → cosine), not just
    // that it compiles. Similar text must rank above unrelated text; a lexical or
    // broken embedder fails this. Exercises the real production path:
    // local_embed_adapter() → try_neural_embedder().
    // #[ignore]: needs the Qwen3-Embedding-0.6B GGUF on disk; CI without it skips.
    // Run: cargo test -p continuum-core --features metal,accelerate,test-fixtures \
    //   --lib cognition::embedding::tests::in_process_embedder_ranks_semantic_similarity \
    //   -- --ignored --nocapture
    #[tokio::test]
    #[ignore]
    async fn in_process_embedder_ranks_semantic_similarity() {
        // local_embed_adapter() reads the process-wide registry; seed it (idempotent
        // — ignore "already initialized" when run alongside other tests).
        let _ = crate::model_registry::init_global();

        let (adapter, model_id) =
            local_embed_adapter().expect("embed GGUF must be on disk for this real-model test");
        eprintln!("[embed-test] loaded in-process embed adapter: {model_id}");

        let embedder = try_neural_embedder(adapter, CANONICAL_EMBED_MODEL)
            .await
            .expect("neural embedder probe must succeed with a real embed model on disk");

        let anchor = embedder.embed("the deployment failed with a compile error").await;
        let similar = embedder
            .embed("the build broke because the code did not compile")
            .await;
        let different = embedder.embed("she watered the tomato plants in the garden").await;

        assert!(!anchor.is_empty(), "real embed must return a non-empty vector");
        eprintln!(
            "[embed-test] dim={} (canonical={})",
            anchor.len(),
            CANONICAL_EMBED_DIM
        );

        let sim = cosine_similarity(&anchor, &similar);
        let dif = cosine_similarity(&anchor, &different);
        eprintln!("[embed-test] cosine(similar)={sim:.4}  cosine(different)={dif:.4}");

        assert!(
            sim > dif,
            "semantically similar text must rank above unrelated text: {sim} !> {dif}"
        );
        assert!(sim > 0.5, "similar pair should be clearly related: {sim}");
    }

    // what this catches: the rung-1 load-probe-fail storm (BigMama's Windows
    // node, 2026-08-02 — 102 llama_context creations × ~1200 MiB, zero IPC).
    // A failed shared resolve must consume exactly ONE attempt and every
    // subsequent resolve inside the cooldown must return fast WITHOUT
    // re-attempting a model load. Runs with no model registry installed, so
    // rung 1 fails at `local_embed_adapter` — the cheap flavor of the same
    // failure path the storm rode.
    #[tokio::test]
    async fn failed_rung1_resolve_does_not_reattempt_within_cooldown() {
        let attempts_before = rung1_cell().lock().await.attempts;
        let a = shared_in_process_embedder(CANONICAL_EMBED_MODEL).await;
        let b = shared_in_process_embedder(CANONICAL_EMBED_MODEL).await;
        let c = shared_in_process_embedder(CANONICAL_EMBED_MODEL).await;
        let attempts_after = rung1_cell().lock().await.attempts;
        // In a bare test process there is no registry → all resolves fail…
        assert!(a.is_none() && b.is_none() && c.is_none());
        // …and the cooldown means three resolves cost at most ONE attempt.
        // (≤ rather than == : another test in the parallel suite may have
        // burned the process-global attempt first — the invariant is "no
        // storm", not "this test owns the counter".)
        assert!(
            attempts_after - attempts_before <= 1,
            "three back-to-back failed resolves must not attempt more than one load \
             (got {} new attempts)",
            attempts_after - attempts_before
        );
    }
}
