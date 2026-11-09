use super::*;

/// Probe a candidate adapter as a neural embedder; return the cache-wrapped
/// provider only when a one-shot embed proves the model actually serves usable
/// vectors. `None` → the caller tries the next source. `model` is the
/// embedding-SPACE identity (cache key) — the canonical slug, NOT the registry
/// id — so an in-process embed and a grid-gateway embed of the same model share
/// cache space and stay comparable.
pub(super) async fn try_neural_embedder(
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
    continuum_base::probe!(
        class = "recall.embedder.calibrated",
        model = %model,
        null_mean = mean,
        null_std = std,
        "measured unrelated-cosine null for the embedding space"
    );
    // This node embeds — claim the embed lane's standing FLOOR on the resource
    // board (#225, Joel 2026-08-08: "the budgeter just has all its parts figure
    // it out"). The floor makes the ledger's `available_for` math hold the lane's
    // working VRAM against every OTHER consumer, so the serving plan can no
    // longer grow its window into the slice cognition needs every turn (measured
    // the day this landed: 604 MiB governed-available vs the 1792 MiB need —
    // embedding fully dead, every recall degraded to no-relevance). Reserved
    // HERE, at successful resolve, so a node with no embed model wastes nothing.
    // Idempotent: re-resolve re-states the same floor.
    if let Some(daemon) = crate::resources::ResourceDaemon::global() {
        use crate::inference::llamacpp_adapter::{EMBED_LANE_CONSUMER_ID, EMBED_LANE_VRAM_BYTES};
        daemon.reserve(
            EMBED_LANE_CONSUMER_ID,
            crate::resources::ResourceKind::Vram,
            EMBED_LANE_VRAM_BYTES,
        );
        continuum_base::probe!(
            class = "embed.vram.reserved",
            consumer = EMBED_LANE_CONSUMER_ID,
            bytes = EMBED_LANE_VRAM_BYTES,
            "embed lane reserved its standing VRAM floor — serving's budget now plans around it"
        );
    }
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
pub(super) struct Rung1State {
    pub(super) provider: Option<(Arc<dyn EmbeddingProvider>, String)>,
    pub(super) last_attempt: Option<Instant>,
    /// Total real resolve attempts (loads) this process has made — the
    /// observable invariant the storm regression test pins.
    pub(super) attempts: u32,
}

/// Cooldown between failed rung-1 resolve attempts. One knock per minute keeps
/// late-arriving embed models recoverable (a boot-race registry, a mid-session
/// model pull) while capping the worst case at ~1 model-load per minute instead
/// of one per persona respawn.
pub(super) const RUNG1_RETRY_COOLDOWN: Duration = Duration::from_secs(60);

pub(super) fn rung1_cell() -> &'static tokio::sync::Mutex<Rung1State> {
    static CELL: OnceLock<tokio::sync::Mutex<Rung1State>> = OnceLock::new();
    CELL.get_or_init(|| tokio::sync::Mutex::new(Rung1State::default()))
}

/// The ONE shared in-process embedder (rung 1), or `None` if it isn't available
/// right now. See [`Rung1State`] for the memo/single-flight/cooldown contract.
/// Returns the provider and the resolved GGUF id (for the caller's probe line).
pub(super) async fn shared_in_process_embedder(model: &str) -> Option<(Arc<dyn EmbeddingProvider>, String)> {
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
pub async fn resolve_recall_embedder(
    adapter: Arc<dyn AIProviderAdapter>,
) -> Arc<dyn EmbeddingProvider> {
    // The embedding-SPACE identity (cache key): the canonical grid embedder, so
    // in-process and gateway vectors stay in one comparable space.
    let model = CANONICAL_EMBED_MODEL.to_string();

    // 1. Preferred: the dedicated in-process embed model. Pinned independently of
    //    the chat model, so changing Asha's brain never disturbs recall. ONE
    //    shared instance process-wide — see [`Rung1State`].
    if let Some((provider, gguf_id)) = shared_in_process_embedder(&model).await {
        continuum_base::probe!(
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
    //    serves an embedding lane (e.g. llama-server started --embeddings).
    if adapter
        .capabilities()
        .has(crate::model_registry::Capability::Embedding)
    {
        if let Some(provider) = try_neural_embedder(adapter, &model).await {
            continuum_base::probe!(
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
    continuum_base::probe!(
        class = "recall.embedder.resolved",
        kind = "lexical",
        source = "fallback",
        model = %model,
        "recall embedder = LEXICAL — no neural embed model serving; semantic recall DEGRADED to word-overlap"
    );
    Arc::new(CachingEmbeddingProvider::new(Arc::new(
        LexicalEmbedder::new(),
    )))
}

/// Resolve the recall embedder WITHOUT a chat adapter — for the GLOBAL memory
/// manager (hydrated corpora), which has no per-persona
/// chat gateway to offer. Tries the dedicated in-process embed model (GPU, no
/// HTTP hop), then the lexical floor. Same process-stable, probe-gated,
/// LOUD-on-degrade contract as [`resolve_recall_embedder`]; it simply omits the
/// chat-adapter `/v1/embeddings` rung (rung 2) that only a persona's own adapter
/// can provide. Always returns a usable embedder — never errors, never panics.
pub async fn resolve_recall_embedder_local() -> Arc<dyn EmbeddingProvider> {
    let model = CANONICAL_EMBED_MODEL.to_string();

    if let Some((provider, gguf_id)) = shared_in_process_embedder(&model).await {
        continuum_base::probe!(
            class = "recall.embedder.resolved",
            kind = "neural",
            source = "in-process-llamacpp-global",
            model = %model,
            gguf = %gguf_id,
            "global recall embedder = NEURAL (dedicated in-process Qwen3-Embedding, shared)"
        );
        return provider;
    }

    continuum_base::probe!(
        class = "recall.embedder.resolved",
        kind = "lexical",
        source = "fallback-global",
        model = %model,
        "global recall embedder = LEXICAL — no in-process embed model serving; semantic recall DEGRADED to word-overlap"
    );
    Arc::new(CachingEmbeddingProvider::new(Arc::new(
        LexicalEmbedder::new(),
    )))
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
    pub(super) resolved: tokio::sync::OnceCell<Arc<dyn EmbeddingProvider>>,
}

impl LazyRecallEmbedder {
    pub fn new() -> Self {
        Self {
            resolved: tokio::sync::OnceCell::new(),
        }
    }

    /// The resolved backend, resolving (once) on first call.
    pub(super) async fn backend(&self) -> &Arc<dyn EmbeddingProvider> {
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

    async fn embed_many(&self, texts: &[String]) -> Vec<Vec<f32>> {
        self.backend().await.embed_many(texts).await
    }

    fn unrelated_null(&self) -> Option<(f32, f32)> {
        self.resolved.get().and_then(|p| p.unrelated_null())
    }
    fn is_semantic(&self) -> bool {
        self.resolved.get().is_some_and(|p| p.is_semantic())
    }
}
