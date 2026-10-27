use super::*;

/// Explicit prebuilt validation, with no scheduler, persona, network or persistence.
/// CPU-only loading deliberately exercises the instructions required by every host.
pub fn validate_embedding_model(
    path: &Path,
) -> Result<continuum_cli_lifecycle::prebuilt_validation::EmbeddingReport, String> {
    let started = Instant::now();
    let model = Model::load(
        path,
        ModelParams {
            n_gpu_layers: 0,
            use_mmap: true,
        },
    )?;
    let load_ms = started.elapsed().as_millis() as u64;
    let started = Instant::now();
    let vectors = embed_model(
        &model,
        &["Continuum prebuilt embedding validation.".to_string()],
    )?;
    let dimensions = vectors
        .first()
        .ok_or("embedding validation produced no vector")?
        .len();
    let report = continuum_cli_lifecycle::prebuilt_validation::EmbeddingReport {
        dimensions,
        load_ms,
        embed_ms: started.elapsed().as_millis() as u64,
    };
    report.validate()?;
    Ok(report)
}

// Both the production backend and the explicit validation process use this owner.
pub(super) fn embed_model(model: &Model, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }
    // Embedding is single-batch and non-causal: ALL of a text's tokens are
    // decoded together in one `llama_decode`, so `n_tokens <= n_batch` is a
    // hard llama.cpp invariant — violating it is `GGML_ASSERT(... <=
    // cparams.n_batch)` → `ggml_abort` → SIGABRT, which kills the whole
    // process (C abort, uncatchable by Rust unwinding). A persona admitting
    // a long engram / doctrine chunk at spawn was exactly long enough to
    // trip it. Two-part fix: (1) size the embedding context's batch to a
    // fixed ceiling so it's never the default, and (2) TRUNCATE any input
    // past that ceiling — retrieval embedders cap their context anyway, so
    // clamping a pathologically long input is the correct degrade, never a
    // crash. Ceiling stays modest: this context is rebuilt per call, and
    // `n_ubatch` drives the compute-buffer size (~quadratic in attention).
    // context-budget-exempt: the EMBEDDING model's own architectural input limit (embeddings truncate by design); a property of that model, not a policy we chose
    const EMBED_MAX_TOKENS: usize = 2048;
    let mut ctx = model.new_context(llama::ContextParams {
        embeddings: true,
        // Qwen3-Embedding family is last-token pooled. Thread from config
        // when other embedders join the grid.
        pooling_type: llama::PoolingType::Last,
        // Explicit, not the default: KV + batch + ubatch all sized to the
        // ceiling so any input up to EMBED_MAX_TOKENS decodes in one batch.
        n_ctx: EMBED_MAX_TOKENS as u32,
        n_batch: EMBED_MAX_TOKENS as u32,
        n_ubatch: EMBED_MAX_TOKENS as u32,
        ..Default::default()
    })?;
    let mut out = Vec::with_capacity(texts.len());
    for text in texts {
        // Independent embedding per text — clear the KV so text N's pooled
        // vector doesn't include text N-1's sequence.
        ctx.memory_clear(true);
        let mut tokens = model.tokenize(text, true, false)?;
        if tokens.is_empty() {
            return Err(format!("embed: input tokenized to empty: {text:?}"));
        }
        // Clamp to the batch ceiling. Last-token pooling reads the final
        // kept token, so we keep the head (the standard truncation for an
        // over-length document) rather than crash on the full sequence.
        if tokens.len() > EMBED_MAX_TOKENS {
            tokens.truncate(EMBED_MAX_TOKENS);
        }
        out.push(ctx.embed(&tokens)?);
    }
    Ok(out)
}

/// Configuration for loading a model.
#[derive(Debug, Clone)]
pub struct LlamaCppConfig {
    /// Path to the GGUF model file
    pub model_path: PathBuf,
    /// Per-sequence context budget (tokens). `None` = use the model's
    /// trained `n_ctx_train` from GGUF metadata (the model's own ceiling).
    /// Override only when memory pressure forces a smaller window than the
    /// model natively supports — and pass it explicitly so the choice is
    /// visible. Hardcoded defaults like 8192 cap a 262144-context model
    /// at 3% of its real capability.
    ///
    /// The actual `n_ctx` passed to llama.cpp is `context_length * n_seq_max`
    /// because llama.cpp's KV cache is a single shared pool across sequences
    /// — if N seqs each hold P tokens, total KV needed is N*P.
    pub context_length: Option<u32>,
    /// Batch size for prefill / per-decode token cap. Larger = faster
    /// prefill but more Metal compute buffer.
    pub n_batch: u32,
    /// Physical backend ubatch. On llama.cpp this controls the largest graph
    /// reserved for prompt processing. Keeping it configurable lets Rust avoid
    /// known-bad fused Metal graph shapes without changing model/provider.
    pub n_ubatch: u32,
    /// GPU layers to offload (-1 = all)
    pub n_gpu_layers: i32,
    /// Maximum concurrent sequences in the shared context. Each persona
    /// inflight occupies one seq_id (0..n_seq_max). Scaled by RAM in the
    /// caller (CandleAdapter) and matched by the TS InferenceCoordinator.
    pub n_seq_max: u32,
    /// Flash attention. `Auto` lets llama.cpp pick per-backend.
    pub flash_attn: FlashAttn,
    /// Fused Gated Delta Net graph toggles. Defaults match upstream; callers
    /// can disable for model/backend combinations whose fused Metal kernels
    /// throw across FFI while preserving GPU residency.
    pub fused_gdn_ar: bool,
    pub fused_gdn_ch: bool,
    /// KV cache K element type. F16 = lossless. Q8_0 halves K memory.
    pub type_k: KvCacheType,
    /// KV cache V element type. V is more sensitive than K — keep F16
    /// unless RAM is tight enough to need Q8_0.
    pub type_v: KvCacheType,
    /// Optional path to the multimodal projector GGUF (mmproj). When
    /// present, the backend lazily loads an `MtmdContext` and exposes
    /// `generate_with_image()` so vision-capable models can receive raw
    /// image bytes natively. None = text-only model (the common case);
    /// `generate_with_image()` returns an error.
    pub mmproj_path: Option<PathBuf>,
}

impl Default for LlamaCppConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::new(),
            // None = derive from the model's GGUF metadata at load time
            // via `Model::n_ctx_train()`. The model is the source of truth
            // for its own context. Setting Some(N) here overrides only when
            // a hardware tier can't allocate KV for the model's native
            // window (rare on M5+/RTX class).
            context_length: None,
            n_batch: 512,
            n_ubatch: 512,
            n_gpu_layers: -1,
            // 3 = M5 Pro tier (48GB+). CandleAdapter overrides per-RAM.
            n_seq_max: 3,
            flash_attn: FlashAttn::Auto,
            fused_gdn_ar: true,
            fused_gdn_ch: true,
            // F16/F16 measured fastest for single-token decode on M5 Pro.
            // K=Q8_0 was slower (44 vs 47.5 tok/s) due to per-token dequant
            // overhead. Q8_0 only pays off when KV memory pressure is the
            // bottleneck (very long contexts or many parallel sequences).
            type_k: KvCacheType::F16,
            type_v: KvCacheType::F16,
            mmproj_path: None,
        }
    }
}

/// Currently-available system memory in bytes, read live via sysinfo. Used at
/// model-load / scheduler-spawn time (not the hot path) to size the KV cache
/// against the REAL machine rather than a guessed per-tier budget. On unified-
/// memory Macs this is the same pool the GPU allocates from; on discrete-VRAM
/// boxes it's a conservative proxy (the real fix there is a VRAM probe — see
/// task #46 follow-up). Returns 0 on read failure, which floors the KV budget
/// to MIN_CTX rather than over-allocating.
pub(super) fn available_memory_bytes() -> u64 {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_memory();
    // NOT `available_memory()`: it returns 0 on macOS while total/used are correct
    // (`system_resources::memory_pressure::available_from`, which exists to be the one
    // derivation every reader shares). The doc above says a 0 here "floors the KV budget
    // to MIN_CTX rather than over-allocating" — on a unified-memory Mac, which is the
    // platform this comment is about, that floor was unconditional rather than a
    // read-failure fallback.
    crate::system_resources::memory_pressure::available_from(&sys)
}

/// Rich, model-derived capability facts — read ONCE from the loaded GGUF at
/// `load()` and carried intact on the backend. Context windows and KV memory
/// cost (later: image/audio characteristics) are too model-specific to
/// hardcode — guessing a per-tier integer silently throttles a 32K model to
/// 6% of its window or OOMs a 262K one — and too expensive to re-derive per
/// call. So we read the real numbers once and forward the struct. This is the
/// single source of truth for "what can THIS model actually do," replacing the
/// per-tier `compat_context_length()` guess that used to live in the profile
/// builder. See [[no-hardcoded-heuristics-to-steer-cognition]].
#[derive(Debug, Clone)]
pub struct ModelCapabilities {
    /// The model's OWN trained context ceiling (GGUF `<arch>.context_length`).
    pub n_ctx_train: u32,
    /// Transformer blocks — KV is allocated per layer, so a multiplier.
    pub n_layer: u32,
    /// Attention (query) heads — divides `n_embd` to get head dim.
    pub n_head: u32,
    /// Key/value heads (GQA ≤ `n_head`) — the SCALAR summary. Display/telemetry
    /// only: hybrid models vary this per layer, so KV memory must come from
    /// [`Self::kv_layers`], never `n_head_kv * n_layer`.
    pub n_head_kv: u32,
    /// Hidden-state width.
    pub n_embd: u32,
    /// Per-layer KV geometry — the HONEST KV-memory driver (#238, BigMama's
    /// registered 5090 issue 2). Ordinary GQA models are uniform here and the
    /// per-layer sum reproduces the old scalar math byte-for-byte. Hybrids are
    /// not: kimi-k3 carries 69 recurrent KDA layers with ZERO per-token KV and
    /// 24 MLA layers caching ONE compressed head of width kv_lora+rope (the
    /// GGUF's own `attention.head_count_kv = [0,0,0,1,…]` / `key_length = 576`)
    /// — the scalar formula overestimated its KV ~85× and strangled the
    /// derived context window accordingly.
    pub kv_layers: Vec<KvLayer>,
}

/// One layer's KV cache geometry, in elements (see [`ModelCapabilities::kv_layers`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KvLayer {
    /// KV heads this layer caches — `0` marks a recurrent (SSM/KDA) layer
    /// with no per-token KV at all.
    pub n_head_kv: u32,
    /// K vector width per head (MLA: the COMPRESSED width, e.g. 576).
    pub k_width: u32,
    /// V vector width per head.
    pub v_width: u32,
}

impl ModelCapabilities {
    pub(super) fn from_model(model: &Model) -> Self {
        let n_layer = model.n_layer();
        // The fork's per-layer accessors (llama_model_*_il) are the same
        // hparams the model graph itself computes with — one source of truth.
        let kv_layers = (0..n_layer)
            .map(|il| KvLayer {
                n_head_kv: model.n_head_kv_il(il),
                k_width: model.n_embd_head_k_il(il),
                v_width: model.n_embd_head_v_il(il),
            })
            .collect();
        Self {
            n_ctx_train: model.n_ctx_train(),
            n_layer,
            n_head: model.n_head(),
            n_head_kv: model.n_head_kv(),
            n_embd: model.n_embd().max(0) as u32,
            kv_layers,
        }
    }

    /// Test/uniform constructor: every layer shares the classic GQA geometry
    /// (`head_dim = n_embd / n_head`, K and V both `head_dim` wide). This IS
    /// the old scalar formula, expressed per-layer — ordinary models price
    /// identically through it.
    pub fn uniform(
        n_ctx_train: u32,
        n_layer: u32,
        n_head: u32,
        n_head_kv: u32,
        n_embd: u32,
    ) -> Self {
        let head_dim = if n_head == 0 { 0 } else { n_embd / n_head };
        Self {
            n_ctx_train,
            n_layer,
            n_head,
            n_head_kv,
            n_embd,
            kv_layers: (0..n_layer)
                .map(|_| KvLayer {
                    n_head_kv,
                    k_width: head_dim,
                    v_width: head_dim,
                })
                .collect(),
        }
    }

    /// Bytes of KV cache consumed by one token, for the given K/V element
    /// types — the PER-LAYER sum over [`Self::kv_layers`]: each layer costs
    /// `n_head_kv * (k_width * k_bytes + v_width * v_bytes)`, so recurrent
    /// layers (0 heads) cost nothing and MLA layers cost their compressed
    /// width. Derived entirely from the model's real per-layer dimensions —
    /// no hardcoded "typical" cost, no scalar×n_layer overestimate.
    pub fn kv_bytes_per_token(&self, type_k: KvCacheType, type_v: KvCacheType) -> u64 {
        let centibytes: u64 = self
            .kv_layers
            .iter()
            .map(|l| {
                let heads = l.n_head_kv as u64;
                heads * l.k_width as u64 * type_k.bytes_per_elem_x100()
                    + heads * l.v_width as u64 * type_v.bytes_per_elem_x100()
            })
            .sum();
        centibytes / 100
    }
}

/// The backend: owns a `Model` plus a continuous-batching `Scheduler`.
///
/// Models are Send+Sync (read-only after load). The scheduler owns the
/// shared Context (n_seq_max sequences, single OS-thread driver loop)
/// and is the only place llama.cpp decode happens. Generate calls
/// enqueue requests and stream tokens back over channels — there's no
/// per-call context creation, no Rust-side semaphore.
///
/// LoRAs are loaded into `loras` for genome paging, but per-request LoRA
/// activation is a v2 follow-up — the scheduler currently ignores
/// `active_loras` (see `llamacpp_scheduler` module docs).
pub struct LlamaCppBackend {
    pub(super) model: Arc<Model>,
    pub(super) config: LlamaCppConfig,
    pub(super) model_id: String,
    /// Real model capabilities, read once at load. The authority for the
    /// context window — never a per-tier guess.
    pub(super) caps: ModelCapabilities,
    /// Lazy-spawned scheduler. Lives behind OnceLock because spawning
    /// touches the Model Arc and we want a single instance per backend.
    pub(super) scheduler: OnceLock<Scheduler>,
    /// Lazy-loaded multimodal projector. Built on first `generate_with_image`
    /// call from `config.mmproj_path` (so text-only backends pay zero cost).
    /// Sits behind a Mutex<Option<...>> so concurrent first-call requests
    /// don't double-load. None until first use OR if `mmproj_path` is unset.
    pub(super) mtmd: Mutex<Option<Arc<llama::MtmdContext>>>,
    /// Loaded LoRA adapters, behind `Arc` so a resolved handle can ride a
    /// `GenerationRequest` onto the scheduler thread and outlive a cache
    /// eviction while an in-flight seq is still decoding under it. Field
    /// order matters: `model` is declared BEFORE `loras` and drops AFTER it
    /// (Rust drops fields in declaration order, top-down; therefore `loras`
    /// drops first), upholding the "adapter must not outlive model"
    /// invariant — and the scheduler holds its own `Arc<Model>` clone, so
    /// the model outlives every handed-out adapter handle regardless.
    pub(super) loras: Mutex<HashMap<String, Arc<LoraAdapter>>>,
}

// SAFETY: Model is Send+Sync (llama.cpp models are immutable after load).
// LoraAdapter is Send+Sync per the llama crate's impl. The Mutex handles
// concurrent modification to the map.
unsafe impl Send for LlamaCppBackend {}

unsafe impl Sync for LlamaCppBackend {}
