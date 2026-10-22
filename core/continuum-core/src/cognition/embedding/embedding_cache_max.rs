use super::*;

/// Soft bound on the global embedding cache so a long-lived grid node servicing a
/// busy room can't grow it without limit (the substrate's pressure doctrine — an
/// unbounded hot-path cache is a leak at scale). On overflow we evict one
/// arbitrary entry; a proper LRU / `PagedResourcePool` tie-in is the follow-up.
pub(super) const EMBEDDING_CACHE_MAX: usize = 20_000;

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
    /// Embed several texts, in order, one vector per text (an empty vector = that text
    /// failed, as for [`Self::embed`]). The default embeds them one by one; a provider whose
    /// backend takes a batch overrides it, so N texts cost one call, one lease and one queue
    /// wait instead of N. Measured on the 5090 2026-10-10: a gene signature minted text by
    /// text spent ~75 min of a 113-example dispatch in `genome/job-create`, ~1.5 embeds/min,
    /// each taking its own embed lease behind serving (card d157d382).
    async fn embed_many(&self, texts: &[String]) -> Vec<Vec<f32>> {
        let mut out = Vec::with_capacity(texts.len());
        for text in texts {
            out.push(self.embed(text).await);
        }
        out
    }
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
    /// Does cosine in this space stand for MEANING? A neural embedder says yes. A
    /// token-overlap space says no: two distinct cards that share "the / test /
    /// fails / on" score 0.70 in `lexical-fnv-tf` — a fine ranking fallback for
    /// recall, never a judgment that two things are the same. Consumers that turn
    /// similarity into an ACT (grouping duplicates, closing a card) must refuse on a
    /// non-semantic space (Cormac, #4140). Default `false` by cost asymmetry: a
    /// wrong "no" refuses (recoverable); a wrong "yes" merges distinct items.
    fn is_semantic(&self) -> bool {
        false
    }
}

/// Canned UNRELATED text pairs for measuring an embedder's cosine null
/// distribution. Calibration probes (measurement data), not matching logic —
/// deliberately diverse in topic, register, length AND LANGUAGE (an English-only
/// null would mis-measure the space a multilingual room actually queries in).
pub const CALIBRATION_PAIRS: &[(&str, &str)] = &[
    (
        "the invoice for March is overdue",
        "a heron stood motionless in the shallows",
    ),
    (
        "fn main() { println!(\"hello\"); }",
        "she packed two sweaters for the trip north",
    ),
    (
        "die Sitzung wurde auf Donnerstag verschoben",
        "el río bajaba turbio después de la tormenta",
    ),
    (
        "our quarterly revenue grew eight percent",
        "the sonata's third movement is in A minor",
    ),
    (
        "git rebase rewrites commit history",
        "la soupe manque de sel et d'une feuille de laurier",
    ),
    (
        "降雨量は流域全体で予想を上回った",
        "the defendant waived the right to a jury",
    ),
    (
        "the cache invalidation bug ships tomorrow",
        "auf dem Bergrücken blühten die Wildblumen früh",
    ),
    (
        "please review the attached slide deck",
        "der Springer gabelte Dame und Turm",
    ),
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

/// Bootstrap embedder: hashing-trick term-frequency vectors. Lowercase, split on
/// non-alphanumeric, hash each token into one of `DIM` buckets, count, then
/// L2-normalize so cosine is a dot product. Captures topical word overlap — a
/// query sharing vocabulary with a memory scores high regardless of recency.
pub struct LexicalEmbedder {
    pub(super) dim: usize,
    /// Measured unrelated-cosine null (mean, std) over [`CALIBRATION_PAIRS`] —
    /// computed at construction (pure CPU, trivial). For a token-overlap space
    /// this is genuinely ≈ (0, 0): disjoint vocabularies share no buckets.
    pub(super) null: (f32, f32),
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
    pub(super) fn measure_null_sync(&self) -> (f32, f32) {
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
    pub(super) fn embed_sync(&self, text: &str) -> Vec<f32> {
        let mut v = vec![0.0f32; self.dim];
        let mut token = String::new();
        let push = |tok: &mut String, v: &mut [f32]| {
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
    pub(super) fn hash_token(token: &str) -> u64 {
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
    pub(super) map: DashMap<u64, Vec<f32>>,
}

impl Default for EmbeddingCache {
    fn default() -> Self {
        Self {
            map: DashMap::new(),
        }
    }
}

impl EmbeddingCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// FNV-1a-64 over `provider_id \0 text` — keys the vector to its embedding
    /// SPACE as well as its content. Deterministic (replay-safe); collision
    /// probability is negligible at realistic message volumes.
    pub(super) fn key(provider_id: &str, text: &str) -> u64 {
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
        let entries: Vec<(u64, Vec<f32>)> = self
            .map
            .iter()
            .map(|e| (*e.key(), e.value().clone()))
            .collect();
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
pub(super) const EMBEDDING_CACHE_FLUSH_SECS: u64 = 60;

/// Warm the process-global embedding cache from `path` NOW (boot warm-start), then
/// snapshot it every [`EMBEDDING_CACHE_FLUSH_SECS`] on its OWN tokio task — the
/// RTOS shape (own task + `tokio::time::interval`, off every hot path; the file
/// write itself is `spawn_blocking`). Best-effort throughout: a failed load or
/// flush is a warn, never fatal. Call once at boot, inside the tokio runtime.
pub fn spawn_embedding_cache_persistence(cache: Arc<EmbeddingCache>, path: std::path::PathBuf) {
    match cache.load_from(&path) {
        Ok(n) => continuum_base::probe!(
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
                Ok(Ok(n)) => continuum_base::probe!(
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
    pub(super) inner: Arc<dyn EmbeddingProvider>,
    pub(super) cache: Arc<EmbeddingCache>,
}

impl CachingEmbeddingProvider {
    /// Wrap `inner`, sharing the process-global cache — what makes the
    /// compute-once-across-personas property hold in production.
    pub fn new(inner: Arc<dyn EmbeddingProvider>) -> Self {
        Self::with_cache(inner, global_embedding_cache())
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
    fn is_semantic(&self) -> bool {
        self.inner.is_semantic()
    }

    async fn embed(&self, text: &str) -> Vec<f32> {
        let key = EmbeddingCache::key(self.inner.id(), text);
        if let Some(v) = self.cache.map.get(&key) {
            return v.clone(); // hot path: sync map hit, the async cost is paid once
        }
        let v = self.inner.embed(text).await;
        self.remember(key, &v);
        v
    }

    async fn embed_many(&self, texts: &[String]) -> Vec<Vec<f32>> {
        let keys: Vec<_> = texts.iter().map(|t| EmbeddingCache::key(self.inner.id(), t)).collect();
        let mut out: Vec<Option<Vec<f32>>> = keys.iter().map(|k| self.cache.map.get(k).map(|v| v.clone())).collect();
        let misses: Vec<usize> = (0..texts.len()).filter(|&i| out[i].is_none()).collect();
        if !misses.is_empty() {
            let batch: Vec<String> = misses.iter().map(|&i| texts[i].clone()).collect();
            let computed = self.inner.embed_many(&batch).await;
            for (&i, v) in misses.iter().zip(computed) {
                self.remember(keys[i], &v);
                out[i] = Some(v);
            }
        }
        out.into_iter().map(Option::unwrap_or_default).collect()
    }
}

impl CachingEmbeddingProvider {
    /// Store one computed vector under its content key: the ONE insert, shared by `embed`
    /// and `embed_many`, so the failure rule and the eviction lock discipline below live once.
    pub(super) fn remember(&self, key: u64, v: &[f32]) {
        // NEVER cache a failed embed. The inner provider degrades a failure to an
        // empty vector (the "no signal" sentinel); caching that would POISON the
        // shared content-addressed cache — a transient embedder hiccup would become
        // a permanent zero-relevance verdict for this content across every persona,
        // never re-attempted. A real embedding is always dim()-length and non-empty,
        // so empty unambiguously means "failed this time": skip the cache, let the
        // next miss retry. (The failure itself is surfaced loud by the inner provider.)
        if v.is_empty() {
            return;
        }
        // Bound memory: evict one arbitrary entry on overflow before inserting a
        // genuinely new key. Hot/recent content re-populates on its next miss.
        //
        // LOCK DISCIPLINE (#368-family stall, 2026-08-08): the victim key MUST be
        // bound by its own `let` so the `iter()` guard (a shard READ lock) is
        // dropped at that statement's end. The previous form put the iterator
        // temporary inside an `if let` scrutinee, where it lives for the WHOLE
        // block — so `remove` waited forever on the same shard's WRITE lock.
        // Self-deadlock, reachable only when the cache is FULL, which is why it
        // slept until the cache first hit EMBEDDING_CACHE_MAX in production:
        // from then on every novel embed parked its task permanently, and since
        // recall embeds inside every turn, the entire citizenry went silent one
        // turn at a time (sampled live: 100% parked in lock_exclusive_slow under
        // DashMap::remove). dashmap's own docs: never call `remove` while
        // holding any reference into the map.
        if !self.cache.map.contains_key(&key) && self.cache.map.len() >= EMBEDDING_CACHE_MAX {
            let victim = self.cache.map.iter().next().map(|e| *e.key());
            if let Some(victim) = victim {
                self.cache.map.remove(&victim);
            }
        }
        self.cache.map.insert(key, v.to_vec());
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
/// The most texts one neural embed request carries. derived-or-floor: a floor that bounds one
/// request's body and the lane's batch, far above the 1 the per-text path paid per call.
pub(super) const NEURAL_EMBED_BATCH: usize = 32;

pub struct NeuralEmbeddingProvider {
    pub(super) adapter: Arc<dyn AIProviderAdapter>,
    /// The canonical model slug = the embedding space identity + cache key.
    pub(super) model_slug: String,
    /// Vector dimensionality (Qwen3-Embedding-0.6B = 1024).
    pub(super) dim: usize,
    /// Measured unrelated-cosine null — set once by [`calibrate`](Self::calibrate)
    /// at resolve time (a neural space MUST measure its anisotropy; assuming ~0
    /// is the bug class that let a 0.15 floor filter nothing).
    pub(super) null: std::sync::OnceLock<(f32, f32)>,
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
                continuum_base::probe!(
                    class = "embedding.neural.failed",
                    model = %self.model_slug,
                    error = %e,
                    "neural embed failed — degrading this item to no-relevance (embedder may be down)"
                );
                Vec::new()
            }
        }
    }

    async fn embed_many(&self, texts: &[String]) -> Vec<Vec<f32>> {
        let mut out = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(NEURAL_EMBED_BATCH) {
            let request = EmbeddingRequest {
                input: EmbeddingInput::Multiple(chunk.to_vec()),
                model: Some(self.model_slug.clone()),
                provider: None,
            };
            match self.adapter.create_embedding(request).await {
                Ok(resp) if resp.embeddings.len() == chunk.len() => out.extend(resp.embeddings),
                Ok(resp) => {
                    continuum_base::probe!(
                        class = "embedding.neural.failed",
                        model = %self.model_slug,
                        error = %format!("asked for {} embeddings, got {}", chunk.len(), resp.embeddings.len()),
                        "neural batch embed returned the wrong count; degrading this chunk to no-relevance"
                    );
                    out.extend(std::iter::repeat_with(Vec::new).take(chunk.len()));
                }
                Err(e) => {
                    continuum_base::probe!(
                        class = "embedding.neural.failed",
                        model = %self.model_slug,
                        error = %e,
                        "neural batch embed failed; degrading this chunk to no-relevance (embedder may be down)"
                    );
                    out.extend(std::iter::repeat_with(Vec::new).take(chunk.len()));
                }
            }
        }
        out
    }

    fn unrelated_null(&self) -> Option<(f32, f32)> {
        self.null.get().copied()
    }
    fn is_semantic(&self) -> bool {
        true
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
pub(super) fn probe_indicates_usable(probe: &[f32]) -> bool {
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
pub(super) fn local_embed_adapter() -> Option<(Arc<dyn AIProviderAdapter>, String)> {
    let reg = crate::model_registry::try_global()?;
    let model = reg
        .models_for_provider(crate::inference::llamacpp_adapter::LLAMACPP_PROVIDER_ID)
        .find(|m| {
            m.capabilities
                .contains(&crate::model_registry::Capability::Embedding)
                && m.gguf_local_path.as_ref().is_some_and(|p| p.exists())
        })?;
    let path = model.gguf_local_path.clone()?;
    let adapter =
        crate::inference::llamacpp_adapter::LlamaCppAdapter::with_model_id(path, model.id.clone());
    Some((Arc::new(adapter), model.id.clone()))
}
