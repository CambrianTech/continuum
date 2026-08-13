//! llama.cpp backend — wraps our owned `llama` crate.
//!
//! The `llama` crate vendors llama.cpp source and builds it via cmake with
//! platform-specific features (metal/cuda). This backend is the adapter
//! between Continuum's TextGenerationRequest pipeline and the safe Rust API.
//!
//! Architecture: ONE shared `Context` driven by a continuous-batching
//! scheduler (see `llamacpp_scheduler`). Each `generate()` call enqueues a
//! request that becomes one sequence (`seq_id`) in the shared batch.
//! Multiple sequences advance per decode step — weights are read once,
//! used for N sequences at once. This replaces the prior per-call-context
//! design where N concurrent generations created N independent contexts
//! that fought for memory bandwidth (each got ~1/N throughput).
//!
//! Measured 67.8 tok/s on M5 Metal with forged Qwen3.5 Q4_K_M (single seq
//! bench). With continuous batching across 3 seqs the per-stream cost
//! drops only ~10% rather than dividing by 3.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use llama::{FlashAttn, KvCacheType, LoraAdapter, Model, ModelParams};

use super::llamacpp_scheduler::{GenerationRequest, Scheduler, SchedulerConfig, TokenEvent};
use super::SamplingConfig;
use crate::runtime;

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
fn available_memory_bytes() -> u64 {
    use sysinfo::System;
    let mut sys = System::new();
    sys.refresh_memory();
    sys.available_memory()
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
    fn from_model(model: &Model) -> Self {
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
    model: Arc<Model>,
    config: LlamaCppConfig,
    model_id: String,
    /// Real model capabilities, read once at load. The authority for the
    /// context window — never a per-tier guess.
    caps: ModelCapabilities,
    /// Lazy-spawned scheduler. Lives behind OnceLock because spawning
    /// touches the Model Arc and we want a single instance per backend.
    scheduler: OnceLock<Scheduler>,
    /// Lazy-loaded multimodal projector. Built on first `generate_with_image`
    /// call from `config.mmproj_path` (so text-only backends pay zero cost).
    /// Sits behind a Mutex<Option<...>> so concurrent first-call requests
    /// don't double-load. None until first use OR if `mmproj_path` is unset.
    mtmd: Mutex<Option<Arc<llama::MtmdContext>>>,
    /// Loaded LoRA adapters, behind `Arc` so a resolved handle can ride a
    /// `GenerationRequest` onto the scheduler thread and outlive a cache
    /// eviction while an in-flight seq is still decoding under it. Field
    /// order matters: `model` is declared BEFORE `loras` and drops AFTER it
    /// (Rust drops fields in declaration order, top-down; therefore `loras`
    /// drops first), upholding the "adapter must not outlive model"
    /// invariant — and the scheduler holds its own `Arc<Model>` clone, so
    /// the model outlives every handed-out adapter handle regardless.
    loras: Mutex<HashMap<String, Arc<LoraAdapter>>>,
}

// SAFETY: Model is Send+Sync (llama.cpp models are immutable after load).
// LoraAdapter is Send+Sync per the llama crate's impl. The Mutex handles
// concurrent modification to the map.
unsafe impl Send for LlamaCppBackend {}
unsafe impl Sync for LlamaCppBackend {}

impl LlamaCppBackend {
    /// Load a GGUF model.
    pub fn load(config: LlamaCppConfig) -> Result<Self, String> {
        let log = runtime::logger("llamacpp");
        if !config.model_path.exists() {
            return Err(format!(
                "Model file not found: {}",
                config.model_path.display()
            ));
        }
        let model_id = config
            .model_path
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "unknown".into());

        let load_start = Instant::now();
        let model = Model::load(
            &config.model_path,
            ModelParams {
                n_gpu_layers: config.n_gpu_layers,
                use_mmap: true,
            },
        )?;
        let caps = ModelCapabilities::from_model(&model);
        log.info(&format!(
            "Loaded {} in {:.2}s (vocab={}, n_ctx_train={}, n_layer={}, \
             n_head={}, n_head_kv={}, kv={}B/tok)",
            model_id,
            load_start.elapsed().as_secs_f64(),
            model.n_vocab(),
            caps.n_ctx_train,
            caps.n_layer,
            caps.n_head,
            caps.n_head_kv,
            caps.kv_bytes_per_token(config.type_k, config.type_v),
        ));

        Ok(Self {
            model: Arc::new(model),
            config,
            model_id,
            caps,
            scheduler: OnceLock::new(),
            mtmd: Mutex::new(None),
            loras: Mutex::new(HashMap::new()),
        })
    }

    /// The model's real capability facts (read once at load).
    pub fn capabilities(&self) -> &ModelCapabilities {
        &self.caps
    }

    /// The per-sequence context window to actually allocate, derived from the
    /// model's REAL ceiling bounded by REAL available memory — never a
    /// hardcoded per-tier integer, and never a silent fall to the full
    /// `n_ctx_train` (which OOMed Metal at 262144 → the 12-tok/s bug, 2026-04).
    ///
    /// Policy:
    /// - Ceiling = `caps.n_ctx_train` (the model's own limit).
    /// - `config.context_length`, when set, is an OPTIONAL further cap — an
    ///   operator/recipe asking for a smaller window. It can only shrink, never
    ///   exceed what the model + memory allow.
    /// - Memory bound = `KV_BUDGET_FRACTION` of currently-available RAM (read
    ///   live; weights are already resident at this point) divided across
    ///   `n_seq_max` sequences and the model's real KV-bytes-per-token. This is
    ///   a headroom RESERVATION that scales with the real machine, NOT a
    ///   context value — it cannot, by construction, allocate more KV than the
    ///   budget it was computed from, so it can't reintroduce the OOM.
    /// - Floored at `MIN_CTX` so a model always has a usable working window.
    pub fn effective_context_length(&self) -> u32 {
        /// Fraction of currently-available memory to reserve for the KV cache.
        /// The rest is left for compute graphs, other backends, and the OS.
        /// Scales with the real machine — not a fixed token count.
        const KV_BUDGET_FRACTION: f64 = 0.6;
        /// A model needs at least this many tokens to hold a RAG-built prompt
        /// and reply. Below this it can't function; if even this doesn't fit,
        /// we still allocate it (it eats headroom, not nonexistent memory) and
        /// log loudly rather than silently degrade.
        // context-budget-exempt: a FLOOR below which a lane cannot usefully run — it only ever raises
        const MIN_CTX: u32 = 1024;

        let log = runtime::logger("llamacpp");
        let ceiling = if self.caps.n_ctx_train > 0 {
            self.caps.n_ctx_train
        } else {
            // GGUF lacked context_length metadata — unusual. Fall to the floor
            // and name it, rather than guessing a large window.
            log.warn(&format!(
                "{}: GGUF reports n_ctx_train=0 — using MIN_CTX={MIN_CTX}",
                self.model_id
            ));
            MIN_CTX
        };

        let kv_per_token = self
            .caps
            .kv_bytes_per_token(self.config.type_k, self.config.type_v);
        let n_seq = self.config.n_seq_max.max(1) as u64;

        // Memory-bounded ceiling.
        let mem_bound = if kv_per_token == 0 {
            ceiling // can't size KV from dims (shouldn't happen) — trust ceiling
        } else {
            let available = available_memory_bytes();
            let budget = (available as f64 * KV_BUDGET_FRACTION) as u64;
            let tokens = budget / (n_seq * kv_per_token);
            (tokens.min(ceiling as u64) as u32).max(MIN_CTX)
        };

        // Operator/recipe hint can only shrink the window further.
        let chosen = match self.config.context_length {
            Some(hint) => hint.min(mem_bound),
            None => mem_bound,
        };
        let chosen = chosen.max(MIN_CTX);

        log.info(&format!(
            "{}: context={} (ceiling={}, mem_bound={}, hint={:?}, n_seq={}, kv={}B/tok)",
            self.model_id,
            chosen,
            ceiling,
            mem_bound,
            self.config.context_length,
            n_seq,
            kv_per_token,
        ));
        chosen
    }

    /// Compute pooled, L2-normalized embeddings for `texts` using a dedicated
    /// EMBEDDING-mode context. The generation scheduler's context is
    /// `embeddings: false` and physically cannot embed, so we build a separate
    /// embedding context here (one per call covers the whole batch; the KV is
    /// cleared between texts so each embedding is independent of the last).
    ///
    /// Model-agnostic plumbing: the LOADED model should be a retrieval embedder
    /// (the grid's canonical Qwen3-Embedding-0.6B — last-token pooled) for the
    /// vectors to be comparable grid-wide. `NeuralEmbeddingProvider` loads that
    /// model; this just runs it. Runtime-validated by an `#[ignore]` real-model
    /// test once the embedding GGUF is on disk; cargo-checked without it.
    pub fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
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
        let mut ctx = self.model.new_context(llama::ContextParams {
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
            let mut tokens = self.model.tokenize(text, true, false)?;
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

    /// Lazily load the multimodal projector. Returns Err when
    /// `config.mmproj_path` is None (text-only backend) or when the
    /// mmproj file fails to load. Idempotent — caches the loaded
    /// MtmdContext under the mutex.
    fn ensure_mtmd(&self) -> Result<Arc<llama::MtmdContext>, String> {
        let mut guard = self
            .mtmd
            .lock()
            .map_err(|e| format!("mtmd lock poisoned: {e}"))?;
        if let Some(existing) = guard.as_ref() {
            return Ok(existing.clone());
        }
        let mmproj = self.config.mmproj_path.as_ref().ok_or_else(|| {
            format!(
                "model {} has no mmproj configured — text-only backend can't process images. \
                 Set `mmproj_local_path` in the Rust catalog (catalog.rs) AND declare Capability::Vision.",
                self.model_id
            )
        })?;
        if !mmproj.exists() {
            return Err(format!(
                "mmproj file declared but missing on disk: {} (model: {})",
                mmproj.display(),
                self.model_id
            ));
        }
        let ctx = llama::MtmdContext::from_file(mmproj, &self.model).map_err(|e| {
            format!(
                "MtmdContext::from_file failed for {}: {e}",
                mmproj.display()
            )
        })?;
        let arc = Arc::new(ctx);
        *guard = Some(arc.clone());
        Ok(arc)
    }

    /// Single-shot multimodal generation: text prompt + one image →
    /// generated text. Bypasses the continuous-batching scheduler
    /// because image encoding produces tokens that aren't trivially
    /// batchable with concurrent text seqs (image tokens have a
    /// fixed positional layout dictated by the projector). Opens a
    /// fresh per-call llama_context, evaluates the image+text via
    /// `MtmdContext::eval_image`, then samples until EOG / max_tokens
    /// / stop sequence. Concurrent multimodal calls each get their
    /// own context — slower than batched but isolated and correct.
    ///
    /// `prompt_with_marker` MUST contain the model's media marker
    /// (see `llama::MtmdContext::default_marker()`, typically
    /// `<__media__>`) — that's where the image tokens splice in. If
    /// the caller's text doesn't include it, `mtmd_tokenize` returns
    /// an error and we surface it.
    pub fn generate_with_image(
        &self,
        prompt_with_marker: &str,
        image_bytes: &[u8],
        max_tokens: usize,
        sampling: SamplingConfig,
        stop_sequences: &[&str],
    ) -> Result<(String, usize), String> {
        self.generate_with_media(
            prompt_with_marker,
            image_bytes,
            max_tokens,
            sampling,
            stop_sequences,
            llama::MediaKind::Image,
        )
    }

    /// Audio analogue of `generate_with_image`. Same single-shot
    /// per-call-context pattern; the mtmd projector path inside auto-
    /// detects audio vs image from the bytes' magic numbers but the
    /// caller's `MediaKind::Audio` selects the capability check
    /// (`supports_audio` instead of `supports_vision`) and shapes error
    /// messages so a mistakenly-routed audio call doesn't surface as a
    /// confusing "vision unsupported" error.
    ///
    /// Supported audio container formats are whatever miniaudio
    /// understands inside the vendored llama.cpp build (wav, mp3, flac
    /// per upstream `tools/mtmd/mtmd-helper.h`). The caller is expected
    /// to deliver one of those — re-encoding from other formats is a
    /// sensory-bridge concern, not the backend's.
    pub fn generate_with_audio(
        &self,
        prompt_with_marker: &str,
        audio_bytes: &[u8],
        max_tokens: usize,
        sampling: SamplingConfig,
        stop_sequences: &[&str],
    ) -> Result<(String, usize), String> {
        self.generate_with_media(
            prompt_with_marker,
            audio_bytes,
            max_tokens,
            sampling,
            stop_sequences,
            llama::MediaKind::Audio,
        )
    }

    /// Internal workhorse for single-shot multimodal generation. Mirrors
    /// the eval+sample loop the public methods need; the only thing that
    /// differs per modality is the capability check (vision vs audio
    /// projector support) and which `MtmdContext::eval_*` method runs.
    /// Centralizing here avoids the 150-LOC duplication that would land
    /// if image and audio paths were copy-pasted.
    fn generate_with_media(
        &self,
        prompt_with_marker: &str,
        media_bytes: &[u8],
        max_tokens: usize,
        sampling: SamplingConfig,
        stop_sequences: &[&str],
        kind: llama::MediaKind,
    ) -> Result<(String, usize), String> {
        let log = runtime::logger("llamacpp");
        let start = Instant::now();
        let mtmd = self.ensure_mtmd()?;
        match kind {
            llama::MediaKind::Image => {
                if !mtmd.supports_vision() {
                    return Err(format!(
                        "model {}'s mmproj does not declare vision support — \
                         caller passed an image but the projector is text-only or audio-only",
                        self.model_id
                    ));
                }
            }
            llama::MediaKind::Audio => {
                if !mtmd.supports_audio() {
                    return Err(format!(
                        "model {}'s mmproj does not declare audio support — \
                         caller passed audio but the projector is text-only or vision-only",
                        self.model_id
                    ));
                }
            }
        }

        // Per-call context — see method-level docstring on why we don't
        // share the scheduler's context.
        //
        // The window comes from the model's REAL ceiling bounded by REAL
        // available memory (`effective_context_length`), NOT a hand-set
        // the Rust catalog (catalog.rs) value and NOT a blind fall to n_ctx_train. The old
        // blind fallback allocated a 262144-token KV cache for qwen3.5
        // (~38GB/seq) and crushed Metal to 12 tok/s (2026-04); the memory
        // bound makes that impossible by construction while still honoring
        // models that genuinely fit a large window. Task #46.
        let per_seq = self.effective_context_length();
        let mut ctx = self
            .model
            .new_context(llama::ContextParams {
                n_ctx: per_seq,
                n_batch: self.config.n_batch,
                n_ubatch: self.config.n_ubatch,
                n_seq_max: 1,
                flash_attn: self.config.flash_attn,
                fused_gdn_ar: self.config.fused_gdn_ar,
                fused_gdn_ch: self.config.fused_gdn_ch,
                type_k: self.config.type_k,
                type_v: self.config.type_v,
                embeddings: false,
                pooling_type: llama::PoolingType::None,
                // MoE expert-selection observer — None for now; the K3 serving path
                // sets a LiveExpertObserver here to feed the residency PGO tally.
                expert_observer: None,
            })
            .map_err(|e| format!("new_context failed: {e}"))?;

        // Eval text + media into the context, advancing n_past.
        let eval_result = match kind {
            llama::MediaKind::Image => mtmd.eval_image(
                &mut ctx,
                prompt_with_marker,
                media_bytes,
                0,
                self.config.n_batch as i32,
                0,
                true,
            ),
            llama::MediaKind::Audio => mtmd.eval_audio(
                &mut ctx,
                prompt_with_marker,
                media_bytes,
                0,
                self.config.n_batch as i32,
                0,
                true,
            ),
        };
        let n_past = eval_result.map_err(|e| format!("mtmd eval ({:?}) failed: {e}", kind))?;
        log.info(&format!(
            "mtmd eval done ({:?}): prompt+media consumed {} positions in {}ms",
            kind,
            n_past,
            start.elapsed().as_millis()
        ));

        // Sample-until-done loop. Mirrors LlamaCppBackend::generate but
        // single-seq, no scheduler. EOG / max_tokens / stop-sequence are
        // the three exit conditions, same shape.
        let mut sampler = if sampling.temperature <= 0.0 && sampling.grammar.is_none() {
            llama::Sampler::greedy()
        } else {
            let mut chain = llama::Sampler::chain();
            if let Some(g) = sampling.grammar.as_ref() {
                chain = chain.grammar(&self.model, g, "root");
            }
            if sampling.top_k > 0 {
                chain = chain.top_k(sampling.top_k as i32);
            }
            if sampling.top_p > 0.0 && sampling.top_p < 1.0 {
                chain = chain.top_p(sampling.top_p as f32, 1);
            }
            chain = chain.penalties(64, sampling.repeat_penalty, 0.0, 0.0);
            let temp = if sampling.temperature > 0.0 {
                sampling.temperature as f32
            } else {
                0.01
            };
            chain.temp(temp).dist(42).build()
        };

        // Diagnostic: dump top-10 logits at the post-image position when
        // MTMD_DEBUG_LOGITS is set. Used during the 2026-04-21 hunt for
        // why our logits diverged from brew's mtmd-cli on the same
        // model+image+prompt; kept env-gated so future bug hunts have a
        // ready-to-fire probe instead of needing to re-derive it.
        if std::env::var_os("MTMD_DEBUG_LOGITS").is_some() {
            let logits = ctx.logits_ith(-1);
            if logits.is_empty() {
                eprintln!("[gen-with-img] WARN: logits_ith(-1) returned empty");
            } else {
                let mut indexed: Vec<(usize, f32)> = logits.iter().copied().enumerate().collect();
                indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
                eprintln!("[gen-with-img] top-10 logits at post-image position:");
                for (id, score) in indexed.iter().take(10) {
                    let piece = self.model.token_to_piece(*id as i32);
                    eprintln!("  id={:>6} score={:.4} piece={:?}", id, score, piece);
                }
            }
        }

        let mut output = String::new();
        let mut pos = n_past;
        let mut tokens_generated = 0usize;
        // Sample at -1 = "last logits in last batch" — same convention
        // brew's mtmd-cli uses (mtmd-cli.cpp:186 calls
        // common_sampler_sample(smpl, lctx, -1) right after eval). After
        // mtmd_helper_eval_chunks with logits_last=true, the final
        // text-batch's last token has logits set and llama_get_logits_ith
        // honors -1 as that position.
        loop {
            let token = sampler.sample(&ctx, -1);
            if self.model.is_eog_token(token) {
                break;
            }
            let piece = self.model.token_to_piece(token);
            output.push_str(&piece);
            tokens_generated += 1;
            // Stop sequence early-exit — same end-of-output trim shape
            // as the scheduler path.
            if stop_sequences.iter().any(|s| output.ends_with(s)) {
                break;
            }
            if tokens_generated >= max_tokens {
                break;
            }
            // Push the sampled token back so the next decode can advance.
            let mut batch = llama::Batch::allocated(1, 1);
            batch.push(token, pos, &[0], true);
            if let Err(e) = ctx.decode(&batch) {
                log.warn(&format!("decode failed mid-generation: {e}"));
                break;
            }
            pos += 1;
        }

        log.info(&format!(
            "generate_with_image done: {} tokens in {}ms ({:.1} tok/s)",
            tokens_generated,
            start.elapsed().as_millis(),
            tokens_generated as f64 / start.elapsed().as_secs_f64().max(0.001)
        ));
        Ok((output, tokens_generated))
    }

    pub fn model_id(&self) -> &str {
        &self.model_id
    }

    /// Model's trained context length, straight from the GGUF metadata.
    /// Single source of truth — never hardcode a context window in
    /// adapters or RAG budgeters; ask this.
    pub fn n_ctx_train(&self) -> u32 {
        self.model.n_ctx_train()
    }

    /// Model's embedded chat template (Jinja-style string). Used by
    /// adapters to render messages through `llama::render_chat`. None
    /// means the model carries no template — caller decides what to do
    /// (error, default, etc.) instead of a silent fallback.
    pub fn model_chat_template(&self) -> Option<String> {
        self.model.chat_template()
    }

    /// Ensure a LoRA adapter is loaded (idempotent). Used by genome paging.
    pub fn ensure_adapter(&self, id: &str, path: &Path) -> Result<(), String> {
        let mut guard = self
            .loras
            .lock()
            .map_err(|e| format!("LoRA lock poisoned: {e}"))?;
        if guard.contains_key(id) {
            return Ok(());
        }
        let adapter = Arc::new(self.model.load_lora(path)?);
        guard.insert(id.to_string(), adapter);
        Ok(())
    }

    /// Remove a LoRA adapter from the cache.
    pub fn remove_adapter(&self, id: &str) -> Result<(), String> {
        let mut guard = self
            .loras
            .lock()
            .map_err(|e| format!("LoRA lock poisoned: {e}"))?;
        guard.remove(id);
        Ok(())
    }

    /// Lazily spawn (or get) the scheduler. Single instance per backend —
    /// owns the shared Context and the OS-thread driver loop.
    fn scheduler(&self) -> &Scheduler {
        self.scheduler.get_or_init(|| {
            // Per-sequence window from the model's REAL ceiling bounded by
            // REAL available memory (task #46), not a hand-set the Rust catalog (catalog.rs)
            // value. `effective_context_length` already divides the memory
            // budget by n_seq_max, so the total below stays within budget —
            // this is what makes the 2026-04 262144-token Metal OOM
            // impossible by construction rather than by a panic.
            let per_seq = self.effective_context_length();
            // n_ctx is the SHARED KV pool across all sequences. Scale by
            // n_seq_max so each seq has `per_seq` tokens of KV headroom
            // even when all slots are occupied with RAG-heavy prompts.
            // saturating_mul because 262144 × 3 overflows u32 (would be
            // 786432, fine, but n_seq_max could grow).
            let total_n_ctx = per_seq.saturating_mul(self.config.n_seq_max.max(1));
            Scheduler::spawn(
                self.model.clone(),
                SchedulerConfig {
                    n_ctx: total_n_ctx,
                    n_batch: self.config.n_batch,
                    n_ubatch: self.config.n_ubatch,
                    n_seq_max: self.config.n_seq_max,
                    flash_attn: self.config.flash_attn,
                    fused_gdn_ar: self.config.fused_gdn_ar,
                    fused_gdn_ch: self.config.fused_gdn_ch,
                    type_k: self.config.type_k,
                    type_v: self.config.type_v,
                },
            )
        })
    }

    /// Generate text. Routes through the continuous-batching scheduler:
    /// the request becomes one sequence in the shared batch and tokens
    /// stream back as they're sampled. Multiple concurrent generate()
    /// calls share the single Context — weights are read once per decode
    /// step and used to advance every active sequence in parallel.
    ///
    /// `active_loras` is `(id, scale)` pairs naming genes previously paged
    /// in via [`ensure_adapter`]. They are resolved to live adapter handles
    /// and applied context-level by the scheduler before decode (genome
    /// paging). A requested id that was never `ensure_adapter`'d is a hard
    /// error — never a silent base-model run (Rule 2: fail loud).
    pub fn generate(
        &self,
        prompt: &str,
        max_tokens: usize,
        sampling: SamplingConfig,
        stop_sequences: &[&str],
        active_loras: &[(String, f32)],
    ) -> Result<(String, usize), String> {
        // Forwards to the persona-aware variant with persona_id=None so
        // test rigs and ad-hoc probes don't need to change. Production
        // adapter calls go through generate_for_persona() so the registry
        // can attribute KV bytes per-persona.
        self.generate_for_persona(
            None,
            prompt,
            max_tokens,
            sampling,
            stop_sequences,
            active_loras,
        )
    }

    /// Same as `generate` but threads a `persona_id` through to the
    /// scheduler so the registry can attribute the seq slot's KV bytes
    /// to the right persona. Pass `None` for test/ad-hoc paths that
    /// shouldn't appear in per-persona accounting.
    ///
    /// `persona_id` is forwarded as-is into `ActiveSeq::persona_id`. The
    /// actual registry reporting (Piece 2 of the substrate work) hooks
    /// into seq alloc / Done events inside the scheduler — this method's
    /// only job here is to deliver the value.
    pub fn generate_for_persona(
        &self,
        persona_id: Option<uuid::Uuid>,
        prompt: &str,
        max_tokens: usize,
        sampling: SamplingConfig,
        stop_sequences: &[&str],
        active_loras: &[(String, f32)],
    ) -> Result<(String, usize), String> {
        let log = runtime::logger("llamacpp");
        let gen_start = Instant::now();
        let prompt_len_chars = prompt.len();

        // Channel for streaming tokens back from the scheduler.
        let (response_tx, mut response_rx) = tokio::sync::mpsc::unbounded_channel::<TokenEvent>();

        // Resolve requested genes (`id`, scale) to live adapter handles from
        // the cache BEFORE crossing to the scheduler thread (which can't lock
        // our cache). A requested id that was never paged in via
        // `ensure_adapter` is a hard error — never a silent base-model run
        // (Rule 2: fail loud, name the cause). Empty in the common no-gene
        // case, so the base path pays nothing.
        let resolved_loras: Vec<(String, Arc<LoraAdapter>, f32)> = if active_loras.is_empty() {
            Vec::new()
        } else {
            let guard = self
                .loras
                .lock()
                .map_err(|e| format!("LoRA cache lock poisoned: {e}"))?;
            active_loras
                .iter()
                .map(|(id, scale)| {
                    guard
                        .get(id)
                        .map(|h| (id.clone(), Arc::clone(h), *scale))
                        .ok_or_else(|| {
                            format!(
                                "LoRA gene '{id}' requested but not loaded — call \
                                 ensure_adapter(id, path) before generate (Rule 2: \
                                 refuse to silently run the base model in its place)"
                            )
                        })
                })
                .collect::<Result<Vec<_>, String>>()?
        };

        // Caller passes the full SamplingConfig (the value-object pattern
        // — adding fields like `grammar` doesn't require changing this
        // signature). Previously this path silently overwrote the caller's
        // top_k/top_p/repeat_penalty fields with no-op defaults.
        let req = GenerationRequest {
            prompt: prompt.to_string(),
            max_tokens,
            sampling,
            stop_sequences: stop_sequences.iter().map(|s| s.to_string()).collect(),
            active_loras: resolved_loras,
            response_tx,
            persona_id,
        };

        self.scheduler().enqueue(req)?;

        // Collect tokens from the channel until Done/Error. We're called
        // synchronously from spawn_blocking by CandleAdapter — block_in_place
        // a tokio runtime handle to await the channel.
        //
        // Stop-sequence trimming happens here (at the boundary): the
        // scheduler emits the stop sequence's tokens before signaling Done,
        // so we strip them from the collected output.
        let mut output = String::new();
        let mut n_decoded = 0usize;
        let runtime_handle = tokio::runtime::Handle::try_current().ok();

        loop {
            let event = if let Some(ref h) = runtime_handle {
                h.block_on(response_rx.recv())
            } else {
                // Fallback for non-tokio callers (e.g. tests). Spin briefly.
                let mut tries = 0u32;
                loop {
                    match response_rx.try_recv() {
                        Ok(e) => break Some(e),
                        Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                            break None;
                        }
                        Err(tokio::sync::mpsc::error::TryRecvError::Empty) => {
                            std::thread::sleep(std::time::Duration::from_millis(2));
                            tries += 1;
                            if tries > 60_000 {
                                // 2 minutes without a token — treat as fatal.
                                break None;
                            }
                        }
                    }
                }
            };
            match event {
                Some(TokenEvent::Token(piece)) => {
                    output.push_str(&piece);
                    n_decoded += 1;
                }
                Some(TokenEvent::Done {
                    tokens_generated,
                    elapsed_ms,
                }) => {
                    n_decoded = tokens_generated;
                    let elapsed = gen_start.elapsed();
                    log.info(&format!(
                        "Generated {} tokens in {:.3}s ({:.1} tok/s, scheduler={}ms, prompt={}chars)",
                        n_decoded,
                        elapsed.as_secs_f64(),
                        n_decoded as f64 / elapsed.as_secs_f64().max(0.001),
                        elapsed_ms,
                        prompt_len_chars
                    ));
                    // Trim trailing stop sequence(s) — scheduler emits them
                    // before signaling Done.
                    for s in stop_sequences {
                        if output.ends_with(s) {
                            output.truncate(output.len() - s.len());
                        }
                    }
                    return Ok((output, n_decoded));
                }
                Some(TokenEvent::Error(e)) => {
                    return Err(format!("scheduler error: {e}"));
                }
                None => {
                    return Err("scheduler closed without Done event".to_string());
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the KV-per-token cost is derived from the model's
    // REAL dimensions (the rule behind task #46), not a hardcoded "typical"
    // number. A GQA model (n_head_kv << n_head) must cost far less than its
    // query-head count implies — get the formula wrong and the memory budget
    // either OOMs or throttles. Worked example: n_embd=2048, n_head=16 →
    // head_dim=128; n_head_kv=2 → kv_width=256; F16=2B → 256*2 per K and per
    // V = 1024B/layer/token; ×36 layers = 36864 B/token.
    #[test]
    fn kv_bytes_per_token_from_real_dims() {
        let caps = ModelCapabilities::uniform(32768, 36, 16, 2, 2048);
        assert_eq!(
            caps.kv_bytes_per_token(KvCacheType::F16, KvCacheType::F16),
            36864
        );
        // Q8_0 K (1.0625 B/elem) + F16 V (2 B/elem): per layer/token =
        // 256*1.0625 + 256*2 = 272 + 512 = 784 → ×36 = 28224 B/token.
        // (Integer centibyte math: 256*106 + 256*200 = 27136+51200 = 78336
        // centibytes/layer ×36 /100 = 28200 — the .0625 rounds down in the
        // ×100 fixed point, which is the SAFE direction for a budget.)
        assert_eq!(
            caps.kv_bytes_per_token(KvCacheType::Q8_0, KvCacheType::F16),
            28200
        );
        // Degenerate guard: a model that reports zero heads can't be sized;
        // return 0 (caller falls back to the trained ceiling) rather than
        // dividing by zero.
        let bad = ModelCapabilities::uniform(0, 0, 0, 0, 0);
        assert_eq!(
            bad.kv_bytes_per_token(KvCacheType::F16, KvCacheType::F16),
            0
        );
    }

    // what this catches: BigMama's registered 5090 issue 2 (#238) — hybrid
    // KDA/MLA models must be KV-priced from their PER-LAYER geometry, never
    // scalar n_head_kv × n_layer. Fixture: the real Kimi-K3 UD-IQ2 GGUF
    // (scratchpad k3-ud-iq2-tensor-fixture.txt, gist 931517a3): 93 layers,
    // head_count_kv = [0,0,0,1]×23 + [1] (69 recurrent KDA layers with ZERO
    // per-token KV; 24 MLA layers caching ONE compressed head), key_length
    // 576 (kv_lora 512 + rope 64), value_length 74. Honest cost @F16 =
    // 24 × 1 × (576+74) × 2 = 31,200 B/token. The old scalar formula priced
    // this either at 0 (layer-0 KDA scalar) or ~2.6 MB/token (uniform-96-head
    // assumption) — both wrong by orders of magnitude, strangling or OOMing
    // the derived context window.
    #[test]
    fn kv_bytes_per_token_hybrid_kda_mla_per_layer() {
        let kv_layers: Vec<KvLayer> = (0..93u32)
            .map(|il| KvLayer {
                // The fixture's array: every 4th layer (3,7,…,91) plus the
                // final layer 92 is MLA (1 compressed KV head); rest are KDA.
                n_head_kv: if il % 4 == 3 || il == 92 { 1 } else { 0 },
                k_width: 576,
                v_width: 74,
            })
            .collect();
        assert_eq!(
            kv_layers.iter().filter(|l| l.n_head_kv > 0).count(),
            24,
            "fixture sanity: 24 MLA layers"
        );
        let caps = ModelCapabilities {
            n_ctx_train: 1_048_576,
            n_layer: 93,
            n_head: 96,
            n_head_kv: 0, // scalar summary (layer 0 is KDA) — display only
            n_embd: 7168,
            kv_layers,
        };
        assert_eq!(
            caps.kv_bytes_per_token(KvCacheType::F16, KvCacheType::F16),
            31_200
        );
    }

    // what this catches: with NO hand-set context_length, the backend must
    // derive a usable window from the model's real n_ctx_train bounded by real
    // memory — never panic (the old `.expect()`), never blindly allocate the
    // full ceiling (the 262144-token Metal OOM, 2026-04). Proves task #46
    // end-to-end against a real GGUF. Run:
    //   cargo test -p continuum-core --features metal,accelerate,test-fixtures \
    //     --lib inference::backends::llamacpp::tests::effective_context -- --ignored
    #[test]
    #[ignore = "requires the dense base GGUF on disk; run explicitly"]
    fn effective_context_derives_from_real_limits_without_override() {
        let path = dirs::home_dir()
            .unwrap()
            .join(".continuum/models/qwen2.5-coder-3b-instruct-f16.gguf");
        if !path.exists() {
            eprintln!("skip: {} not present", path.display());
            return;
        }
        // No context_length override — force the real-limit derivation path.
        let config = LlamaCppConfig {
            model_path: path,
            context_length: None,
            ..Default::default()
        };
        let backend = LlamaCppBackend::load(config).expect("load dense base");

        let caps = backend.capabilities();
        assert!(caps.n_ctx_train > 0, "GGUF must report a trained ceiling");
        assert!(
            caps.n_layer > 0 && caps.n_head_kv > 0,
            "real dims populated"
        );

        let ctx = backend.effective_context_length();
        // Derived, not panicked; within the model's real ceiling; usable.
        assert!(ctx >= 1024, "must give a usable window, got {ctx}");
        assert!(
            ctx <= caps.n_ctx_train,
            "must never exceed the model's real ceiling ({} > {})",
            ctx,
            caps.n_ctx_train
        );
        eprintln!(
            "effective_context_length={} (n_ctx_train={}, kv={}B/tok)",
            ctx,
            caps.n_ctx_train,
            caps.kv_bytes_per_token(KvCacheType::F16, KvCacheType::F16),
        );
    }
}
