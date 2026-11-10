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
    // Non-causal embedding needs each piece in one batch. Size the graph from the actual request,
    // never a private token ceiling or head clip.
    let sequences = texts
        .iter()
        .map(|text| model.tokenize(text, true, false))
        .collect::<Result<Vec<_>, _>>()?;
    if sequences.iter().any(Vec::is_empty) {
        return Err("embedding tokenization produced an empty sequence".into());
    }
    let model_limit = usize::try_from(model.n_ctx_train())
        .map_err(|_| "embedding model context exceeds native representation")?;
    // The prefix (BOS) is read only when a text needs windows: a batch of texts that fit never
    // depends on how a tokenizer treats an empty string (Codex on #5901).
    let prefix = if sequences.iter().any(|tokens| tokens.len() > model_limit) {
        model.tokenize("", true, false)?
    } else {
        Vec::new()
    };
    let plans = plan(&sequences, &prefix, model_limit)?;
    let widest = plans
        .iter()
        .zip(&sequences)
        .map(|(plan, tokens)| match plan {
            Plan::Direct => tokens.len(),
            Plan::Windows(windows) => windows.iter().map(|w| prefix.len() + w.len()).max().unwrap_or(0), // unwrap_or: plan() never returns an empty window list
        })
        .max()
        .ok_or("empty embedding request")?;
    let widest = u32::try_from(widest).map_err(|_| "embedding sequence length exceeds native representation")?;
    let mut ctx = model.new_context(llama::ContextParams {
        embeddings: true,
        pooling_type: llama::PoolingType::Last,
        n_ctx: widest,
        n_batch: widest,
        n_ubatch: widest,
        ..Default::default()
    })?;
    embed_plans(&sequences, &plans, &prefix, |tokens| {
        ctx.memory_clear(true);
        ctx.embed(tokens)
    })
}

/// How one tokenized text is embedded.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Plan {
    /// It fits the context: its own tokens, embedded exactly as they are.
    Direct,
    /// It does not (card 57ef2a06: BigMama's 38,603-token example at a 32,768 context errored the
    /// whole batch): every window of it, each carrying the model's prefix, never its head alone.
    Windows(Vec<std::ops::Range<usize>>),
}

/// Each text's plan. Only a text over `limit` is windowed, and it must begin with `prefix`.
fn plan(sequences: &[Vec<i32>], prefix: &[i32], limit: usize) -> Result<Vec<Plan>, String> {
    sequences
        .iter()
        .map(|tokens| {
            if tokens.len() <= limit {
                return Ok(Plan::Direct);
            }
            if !tokens.starts_with(prefix) {
                return Err(format!(
                    "a {}-token embedding sequence does not begin with the model's {}-token prefix: its windows would be built from the wrong tokens",
                    tokens.len(),
                    prefix.len()
                ));
            }
            windows(tokens.len(), prefix.len(), limit).map(Plan::Windows)
        })
        .collect()
}

/// One vector per text, in order: a direct text is `embed` of its own tokens, untouched; a windowed
/// text is its windows combined by `weighted_mean`.
fn embed_plans(
    sequences: &[Vec<i32>],
    plans: &[Plan],
    prefix: &[i32],
    mut embed: impl FnMut(&[i32]) -> Result<Vec<f32>, String>,
) -> Result<Vec<Vec<f32>>, String> {
    let mut out = Vec::with_capacity(sequences.len());
    for (tokens, plan) in sequences.iter().zip(plans) {
        out.push(match plan {
            Plan::Direct => embed(tokens)?,
            Plan::Windows(windows) => {
                let mut pieces = Vec::with_capacity(windows.len());
                for window in windows {
                    let mut piece = Vec::with_capacity(prefix.len() + window.len());
                    piece.extend_from_slice(prefix);
                    piece.extend_from_slice(&tokens[window.clone()]);
                    pieces.push((embed(&piece)?, window.len()));
                }
                weighted_mean(&pieces)?
            }
        });
    }
    Ok(out)
}

/// The text-token windows of a `len`-token sequence whose first `prefix` tokens are the model's
/// prefix, each fitting `limit` with the prefix carried: one window when it fits, else contiguous
/// windows covering every text token once.
fn windows(len: usize, prefix: usize, limit: usize) -> Result<Vec<std::ops::Range<usize>>, String> {
    let span = limit.checked_sub(prefix).filter(|span| *span > 0).ok_or_else(|| {
        format!("embedding model context {limit} cannot hold its own {prefix}-token prefix and any text")
    })?;
    let text = prefix..len;
    if text.is_empty() {
        return Err("embedding sequence holds no text after its prefix".into());
    }
    Ok(text.clone().step_by(span).map(|start| start..(start + span).min(len)).collect())
}

/// The vector a text's windows stand for together: their mean weighted by the text tokens each
/// holds, L2-normalized like every window (`ctx.embed`), so a one-window text is its window.
fn weighted_mean(pieces: &[(Vec<f32>, usize)]) -> Result<Vec<f32>, String> {
    let (first, _) = pieces.first().ok_or("a text with no embedded window")?;
    let mut sum = vec![0.0f32; first.len()];
    for (vector, weight) in pieces {
        if vector.len() != sum.len() {
            return Err(format!("embedding windows disagree on dimension: {} vs {}", vector.len(), sum.len()));
        }
        for (s, x) in sum.iter_mut().zip(vector) {
            *s += x * *weight as f32;
        }
    }
    let norm = sum.iter().map(|x| x * x).sum::<f32>().sqrt();
    if !norm.is_finite() || norm == 0.0 {
        return Err("embedding windows cancelled to a degenerate vector".into());
    }
    Ok(sum.into_iter().map(|x| x / norm).collect())
}

/// A caller-issued nonzero generation window; model metadata cannot create one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GenerationAllocation(std::num::NonZeroU32);

impl GenerationAllocation {
    pub fn new(tokens: std::num::NonZeroU32) -> Self {
        Self(tokens)
    }
    pub fn tokens(self) -> u32 {
        self.0.get()
    }
}

/// Configuration for loading a model.
#[derive(Debug, Clone)]
pub struct LlamaCppConfig {
    /// Path to the GGUF model file
    pub model_path: PathBuf,
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
    /// One temporary embedding context is admitted per loaded model.
    pub(super) embedding_lane: super::embedding_lane::EmbeddingLane,
    pub(super) model: Arc<Model>,
    pub(super) config: LlamaCppConfig,
    pub(super) model_id: String,
    /// Real model capabilities, read once at load. The authority for the
    /// context window — never a per-tier guess.
    pub(super) caps: ModelCapabilities,
    /// Lazy-spawned scheduler. Lives behind OnceLock because spawning
    /// touches the Model Arc and we want a single instance per backend.
    pub(super) scheduler: OnceLock<(GenerationAllocation, Scheduler)>,
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

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (card 57ef2a06): a text longer than the model's context clipped to its
    // head, or rejected so the whole batch embeds nothing. Every text token lands in exactly one
    // window, each window fits the context with the prefix carried, and a text that fits is one
    // window.
    #[test]
    fn windows_cover_every_text_token_once_within_the_context() {
        assert_eq!(windows(100, 1, 32_768).expect("test: fits"), vec![1..100]);
        let long = windows(38_603, 1, 32_768).expect("test: plans");
        assert_eq!(long, vec![1..32_768, 32_768..38_603], "the BigMama example: two windows, nothing dropped");
        assert!(long.iter().all(|w| 1 + w.len() <= 32_768), "each window fits with its prefix");
        assert_eq!(long.iter().map(|w| w.len()).sum::<usize>(), 38_603 - 1, "every text token once");
        assert!(windows(10, 4, 4).is_err(), "a context that cannot hold its prefix and text refuses");
        assert!(windows(1, 1, 8).is_err(), "a sequence of only its prefix holds no text");
    }

    // what this catches: windows combined unweighted (a short tail outweighs most of the text) or
    // left unnormalized (cosine stops being a dot product); one window must be itself.
    #[test]
    fn windows_combine_by_their_token_share_into_a_unit_vector() {
        let one = weighted_mean(&[(vec![0.6, 0.8], 7)]).expect("test: one window");
        assert!((one[0] - 0.6).abs() < 1e-6 && (one[1] - 0.8).abs() < 1e-6, "{one:?}");
        let two = weighted_mean(&[(vec![1.0, 0.0], 3), (vec![0.0, 1.0], 1)]).expect("test: two windows");
        assert!(two[0] > two[1] * 2.9 && two[0] < two[1] * 3.1, "3:1 by tokens: {two:?}");
        assert!(((two[0] * two[0] + two[1] * two[1]).sqrt() - 1.0).abs() < 1e-6, "unit length");
        assert!(weighted_mean(&[(vec![1.0], 1), (vec![1.0, 0.0], 1)]).is_err(), "dimensions must agree");
        assert!(weighted_mean(&[(vec![1.0, 0.0], 2), (vec![-1.0, 0.0], 2)]).is_err(), "windows that cancel are refused, never normalized to NaN");
        assert!(weighted_mean(&[]).is_err(), "a text with no window has no vector");
    }

    // what this catches (Codex on #5901): a text that fits re-encoded through the window combiner
    // (multiplied by its token count and renormalized, so its f32 vector drifts from the model's),
    // a prefix-only sequence refused, or outputs out of order in a mixed batch. A text that fits is
    // EXACTLY embed(its tokens); a long one is planned into windows; order is kept.
    #[test]
    fn a_text_that_fits_is_embedded_exactly_and_a_mixed_batch_keeps_its_order() {
        // A deterministic stand-in for the model: a non-normalized vector from the tokens, so any
        // re-normalization of a direct text would show as a difference.
        let fake = |tokens: &[i32]| -> Result<Vec<f32>, String> {
            Ok(vec![tokens.len() as f32, tokens.iter().map(|t| *t as f32).sum::<f32>() + 0.1])
        };
        let short = vec![1, 5, 9];
        let prefix_only = vec![1];
        let long: Vec<i32> = std::iter::once(1).chain(2..12).collect(); // 11 tokens, prefix [1]
        let sequences = vec![short.clone(), long.clone(), prefix_only.clone()];
        let plans = plan(&sequences, &[1], 8).expect("test: plans");
        assert_eq!(plans[0], Plan::Direct);
        assert_eq!(plans[1], Plan::Windows(vec![1..8, 8..11]), "only the over-limit text is windowed");
        assert_eq!(plans[2], Plan::Direct, "a prefix-only sequence that fits is embedded as before, never refused");
        let out = embed_plans(&sequences, &plans, &[1], fake).expect("test: embeds");
        assert_eq!(out.len(), 3, "one output per input");
        assert_eq!(out[0], fake(&short).expect("test: fake"), "a fitting text is exactly embed(its tokens)");
        assert_eq!(out[2], fake(&prefix_only).expect("test: fake"));
        assert!(((out[1][0] * out[1][0] + out[1][1] * out[1][1]).sqrt() - 1.0).abs() < 1e-6, "the windowed text is a unit vector");
        assert!(plan(&[vec![7; 20]], &[1], 8).is_err(), "a long text not beginning with the prefix refuses");
    }

    // what this catches (Codex on 57ef2a06): the last, short window of a long text weighted like a
    // full one, so a few hundred tail tokens pull the vector as hard as 32k head tokens.
    #[test]
    fn the_short_final_window_weighs_only_its_own_tokens() {
        let plan = windows(38_603, 1, 32_768).expect("test: plans");
        let weights: Vec<usize> = plan.iter().map(|w| w.len()).collect();
        assert_eq!(weights, vec![32_767, 5_835], "each window weighs the text tokens it holds");
        let mean = weighted_mean(&[(vec![1.0, 0.0], weights[0]), (vec![0.0, 1.0], weights[1])]).expect("test: combine");
        assert!(mean[0] / mean[1] > 5.6 && mean[0] / mean[1] < 5.62, "the head dominates by its token share: {mean:?}");
    }
}
