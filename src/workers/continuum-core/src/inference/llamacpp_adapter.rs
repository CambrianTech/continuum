//! `LlamaCppAdapter` — implements `AIProviderAdapter` by wrapping our
//! in-process `LlamaCppBackend` (the bundled `llama` crate, statically
//! linked against the vendored llama.cpp Metal/CUDA build).
//!
//! Why this exists:
//!
//! Docker Model Runner (DMR) ships a containerized llama-server. On Mac
//! the container's Metal toolchain has been failing to compile the
//! tensor-API source on M5/Apple10 hardware (verified 2026-04-19, log:
//! `ggml_metal_library_init_from_source: error compiling source` →
//! `has tensor = false`). Result: M5 inference at 22 tok/s — slower
//! than M1 at 27 tok/s on the same model. The cripple is in DMR's
//! container build, not in llama.cpp itself.
//!
//! This adapter bypasses DMR entirely — loads the GGUF in-process via
//! our newer vendored llama.cpp build, which compiles Metal correctly
//! against the host toolchain. Empirical win: 33 tok/s vs DMR's 22 tok/s
//! on the same hardware (50% improvement, smoke test in
//! `tests/llamacpp_metal_throughput.rs`).
//!
//! Other wins from owning the inference call directly:
//! - No HTTP hop (in-process call vs localhost roundtrip)
//! - Full control of `n_gpu_layers`, batch sizes, sampling
//! - Direct access to LoRA hot-swap via `LlamaCppBackend::ensure_adapter`
//! - Metal command-buffer timing available for real GPU-utilization
//!   metrics (planned follow-up — addresses "we can't even see what
//!   percent GPU was used" observability gap)
//!
//! Coexistence with DMR adapter:
//! - Both registered. This adapter gets HIGHER priority (lower number)
//!   so local Mac inference flows here first.
//! - DMR remains the fallback for: cases where in-process load fails,
//!   non-Mac platforms, or operators who prefer the container path.

use crate::ai::adapter::{AIProviderAdapter, AdapterCapabilities, ApiStyle, InferenceDevice};
use crate::ai::registry_bridge::models_for_provider_via_registry;
use crate::ai::types::{
    FinishReason, HealthState, HealthStatus, MessageContent, ModelInfo, ResponseFormat,
    TextGenerationRequest, TextGenerationResponse, UsageMetrics,
};
use crate::inference::backends::llamacpp::{LlamaCppBackend, LlamaCppConfig};
use crate::inference::backends::{SamplingConfig, JSON_GRAMMAR};
use crate::inference_capability::enforce_residency;
use crate::runtime;
use async_trait::async_trait;
use llama::FlashAttn;
use parking_lot::RwLock;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

/// Provider ID for this adapter. Routing checks for this when the caller
/// asks for `provider="local"` (per `AdapterRegistry::select`'s
/// "local" → device-filtered local-GPU selection logic).
pub const LLAMACPP_PROVIDER_ID: &str = "llamacpp-local";

/// Overlay live runtime metadata (throughput) on top of the registry's
/// declared ModelInfo. Context-window still flows from `backend.n_ctx_train()`
/// because that's the GGUF's ground truth — the TOML value is the intent,
/// the GGUF metadata is what the runtime actually loaded. If they drift,
/// we trust the model, not the config.
fn model_info_with_runtime(
    mut info: ModelInfo,
    backend: &LlamaCppBackend,
    last_tok_per_s: f64,
) -> ModelInfo {
    let n_ctx = backend.n_ctx_train();
    info.context_window = n_ctx;
    // Same reasoning as elsewhere: the model can decode up to its full
    // context. Callers that want a smaller window declare it per-request;
    // the adapter never invents its own MAX.
    info.max_output_tokens = n_ctx;
    info.tokens_per_second = last_tok_per_s as f32;
    info
}

fn sampling_config_from_request(request: &TextGenerationRequest) -> SamplingConfig {
    let mut sampling = SamplingConfig::chat();
    if let Some(t) = request.temperature {
        sampling.temperature = t as f64;
    }
    if let Some(k) = request.top_k {
        sampling.top_k = k as usize;
    }
    if let Some(p) = request.top_p {
        sampling.top_p = p as f64;
    }
    if let Some(rp) = request.repeat_penalty {
        sampling.repeat_penalty = rp;
    }
    if matches!(request.response_format, Some(ResponseFormat::JsonObject)) {
        sampling.grammar = Some(JSON_GRAMMAR.to_string());
    }
    sampling
}

/// Decode an `ImageInput` to raw bytes the multimodal projector can
/// consume. Prefers `base64` (already in-process); URL fetching is
/// deliberately not supported here — that's a sensory-bridge upstream
/// concern (the bridge fetches once + caches; doing it again at adapter
/// time would silently re-fetch on every request). If the bridge handed
/// us a URL-only image, that's a configuration bug worth surfacing.
fn decode_image_bytes(image: &crate::ai::types::ImageInput) -> Result<Vec<u8>, String> {
    decode_data_url_or_base64(image.base64.as_deref(), image.url.as_deref(), "ImageInput")
}

/// Audio analogue of `decode_image_bytes`. Same base64-or-data-URL
/// shape (sensory-bridge upstream encodes captured PCM/WAV/MP3/FLAC
/// to base64 before passing through the persona pipeline), same
/// no-URL-fetching policy.
fn decode_audio_bytes(audio: &crate::ai::types::AudioInput) -> Result<Vec<u8>, String> {
    decode_data_url_or_base64(audio.base64.as_deref(), audio.url.as_deref(), "AudioInput")
}

/// Common base64 / data-URL decode for the modality-typed wrappers.
/// Splits on the first comma to tolerate `data:image/jpeg;base64,...`
/// or `data:audio/wav;base64,...` prefixes the caller may have included
/// upstream. Errors point at the modality so the diagnosis is specific.
fn decode_data_url_or_base64(
    b64: Option<&str>,
    url: Option<&str>,
    modality_label: &str,
) -> Result<Vec<u8>, String> {
    use base64::{engine::general_purpose, Engine};
    if let Some(b64) = b64 {
        let payload = b64.split_once(',').map(|(_, rest)| rest).unwrap_or(b64);
        general_purpose::STANDARD
            .decode(payload.as_bytes())
            .map_err(|e| format!("{modality_label}.base64 not valid base64: {e}"))
    } else if url.is_some() {
        Err(format!(
            "llamacpp_adapter received an URL-only {modality_label}; the sensory \
             bridge should resolve URLs to base64 before reaching the local \
             adapter (avoids per-request refetches and lets the adapter run \
             without network access)"
        ))
    } else {
        Err(format!(
            "{modality_label} has neither base64 nor url — nothing to decode"
        ))
    }
}

/// Typed failure for [`LlamaCppAdapter::try_new`] when the model
/// registry has no `llamacpp-local` row with a resolved
/// `gguf_local_path`. Surfaces install-time-no-Qwen state as observable
/// runtime health rather than a process panic. Operators see this in
/// install/health output and know exactly what's missing.
///
/// 2026-05-11: continuum-8e97 RTX 5090 finding showed cuda stack ready,
/// VRAM available, zero personas replying — root cause was no Qwen
/// GGUF seeded by carl install. Without this typed error the silent
/// state was indistinguishable from "personas just slow."
#[derive(Debug, thiserror::Error)]
#[error(
    "no `{provider_id}` model with `gguf_local_path` resolved on disk \
     ({rows_in_registry} provider rows, {rows_with_gguf_local_path} with \
     a path on disk). Install seeded no local Qwen GGUF — run model-init \
     downloader or seed manually."
)]
pub struct NoLocalModelLoadable {
    pub provider_id: String,
    pub rows_in_registry: usize,
    pub rows_with_gguf_local_path: usize,
}

/// In-process llama.cpp adapter. Lazy-loads the model on first
/// `generate_text` call (so adapter registration doesn't pay the
/// 5-10s model-load cost up front). After load, the backend lives for
/// the process lifetime in an `Arc` for concurrent generations across
/// personas.
pub struct LlamaCppAdapter {
    backend: Arc<RwLock<Option<Arc<LlamaCppBackend>>>>,
    model_path: PathBuf,
    last_throughput_tok_s: Arc<RwLock<f64>>,
    /// The model id this adapter serves. Resolved from the registry at
    /// construction — whichever llamacpp-local model row has a
    /// `gguf_local_path` pointing at an on-disk file, we claim that id.
    /// Held as `String` so `default_model()` can return `&str`.
    default_model: String,
    /// Per-sequence context budget override. None = honor the model's
    /// declared `n_ctx_train` (e.g. qwen3.5-4b's 262144). Set this
    /// explicitly when memory pressure / hardware tier forces a smaller
    /// window — the KV cache scales linearly with context_length, and a
    /// 262K alloc on qwen3.5-4b is ~24GB even at Q4. Tests use 16K;
    /// production tier-aware sizing is a follow-up (M5 Pro = 64K? or
    /// per-persona declaration).
    context_length_override: Option<u32>,
    /// Per-residency KV quant policy. Controls type_k / type_v at each
    /// lifecycle stage (Active hot in GPU, CpuResident warm in unified
    /// memory, Idle spilled to NVMe). Default = `KvQuantPolicy::default()`
    /// (F16/F16 active, Q8_0/F16 resident, Q8_0/Q8_0 spilled). Caller
    /// overrides via `with_kv_quant_policy()` per recipe / hardware tier.
    /// Currently only `active` is consumed at backend load time;
    /// CpuResident and Idle land with the paging substrate (Phase 3.x).
    /// See docs/architecture/PERSONA-CONTEXT-PAGING.md §16.
    kv_quant_policy: crate::inference::kv_quant::KvQuantPolicy,
}

impl LlamaCppAdapter {
    /// Construct from the model_registry. Looks up the first model under
    /// provider `llamacpp-local` whose GGUF artifact resolved locally
    /// and uses its id + path. If the registry has no such row, panics
    /// — that's a config bug, not a runtime failure mode (per the
    /// no-fallback rule).
    ///
    /// Prefer [`Self::try_new`] when calling from a path that should
    /// surface the missing-Qwen state as observable runtime health
    /// rather than crashing the process. Boot-time health checks
    /// (continuum status, ai/status, install-time validators) MUST use
    /// `try_new` so an install with no Qwen seeded reports
    /// `NoLocalModelLoadable` cleanly instead of crash-looping.
    pub fn new() -> Self {
        Self::try_new().unwrap_or_else(|err| panic!("{err}"))
    }

    /// Result-returning variant of [`Self::new`]. Returns
    /// [`NoLocalModelLoadable`] when the registry has no `llamacpp-local`
    /// row with a resolved `gguf_local_path` — the typed failure mode
    /// for "install seeded no local Qwen GGUF" which surfaces at
    /// install-time on hosts where the model-init container did not
    /// download a chat-capable model (RTX 5090 finding, 2026-05-11). The
    /// caller decides whether to crash (legacy `new()` behavior),
    /// degrade, or report the error to operators.
    pub fn try_new() -> Result<Self, NoLocalModelLoadable> {
        let reg = crate::model_registry::global();
        Self::try_new_from(reg.models_for_provider(LLAMACPP_PROVIDER_ID))
    }

    /// Pure variant of [`Self::try_new`] taking a model iterator
    /// directly — lets tests assemble synthetic registries without going
    /// through the global singleton. Production code uses
    /// [`Self::try_new`] which calls this with `global().models_for_provider(...)`.
    pub fn try_new_from<'a, I>(models: I) -> Result<Self, NoLocalModelLoadable>
    where
        I: IntoIterator<Item = &'a crate::model_registry::Model>,
    {
        let candidates: Vec<&crate::model_registry::Model> = models.into_iter().collect();
        let with_path: Vec<&crate::model_registry::Model> = candidates
            .iter()
            .copied()
            .filter(|m| m.gguf_local_path.is_some())
            .collect();
        let model = with_path.first().ok_or_else(|| NoLocalModelLoadable {
            provider_id: LLAMACPP_PROVIDER_ID.to_string(),
            rows_in_registry: candidates.len(),
            rows_with_gguf_local_path: 0,
        })?;
        let model_path = model
            .gguf_local_path
            .clone()
            .expect("gguf_local_path present — filtered above");
        Ok(Self {
            backend: Arc::new(RwLock::new(None)),
            model_path,
            last_throughput_tok_s: Arc::new(RwLock::new(0.0)),
            default_model: model.id.clone(),
            context_length_override: None,
            kv_quant_policy: crate::inference::kv_quant::KvQuantPolicy::default(),
        })
    }

    /// Override the model path. Useful for tests + when the model isn't
    /// at the registry's declared location.
    pub fn with_model_path(mut self, path: PathBuf) -> Self {
        self.model_path = path;
        self
    }

    /// Construct an adapter bound to a SPECIFIC `(model_path, model_id)`
    /// pair. `new()` picks "first llamacpp-local with a gguf path" which
    /// is fine for the default text model but a registry that holds
    /// multiple llamacpp-local entries (text + vision) needs a way to
    /// say which one this adapter instance serves.
    ///
    /// The `model_id` MUST match a row in `config/models.toml` so the
    /// adapter can look up that model's chat_template, mmproj_path,
    /// stop_sequences, and capabilities. A mismatch produces silently
    /// wrong output (wrong chat template → garbled response).
    pub fn with_model_id(model_path: PathBuf, model_id: String) -> Self {
        Self {
            backend: Arc::new(RwLock::new(None)),
            model_path,
            last_throughput_tok_s: Arc::new(RwLock::new(0.0)),
            default_model: model_id,
            context_length_override: None,
            kv_quant_policy: crate::inference::kv_quant::KvQuantPolicy::default(),
        }
    }

    /// Override the per-sequence context budget. Pass smaller-than-trained
    /// to bound the KV cache allocation (qwen3.5-4b @ 262K = 24GB; @ 16K
    /// = 500MB). Tests should always set this to keep the suite cheap and
    /// avoid leaving 24GB processes lingering when llama.cpp's Metal
    /// cleanup SIGABRTs prevent clean exit (see PR #17869).
    pub fn with_context_length(mut self, n: u32) -> Self {
        self.context_length_override = Some(n);
        self
    }

    /// Override the per-residency KV quant policy. Default is
    /// `KvQuantPolicy::default()` — F16/F16 active for max decode speed,
    /// Q8_0/F16 cpu-resident for compression with quality, Q8_0/Q8_0
    /// spilled for minimum file size. Override per recipe / hardware
    /// tier. See docs/architecture/PERSONA-CONTEXT-PAGING.md §16.
    pub fn with_kv_quant_policy(
        mut self,
        policy: crate::inference::kv_quant::KvQuantPolicy,
    ) -> Self {
        self.kv_quant_policy = policy;
        self
    }

    /// Size the backend's KV by a recipe's persona budgets. The adapter
    /// computes `sum(persona seeds)` bounded by the model's
    /// `n_ctx_train` ceiling, then sets `context_length` accordingly.
    /// Replaces the bandaid `with_context_length(magic_number)` calls
    /// in test rigs and recipe loaders — declare WHO is in the recipe
    /// and what they're DOING, the adapter computes the budget.
    ///
    /// See docs/architecture/PERSONA-CONTEXT-PAGING.md §14 for the
    /// task-default seed table this consumes.
    pub fn with_recipe_budget(
        mut self,
        budget: &crate::inference::recipe_budget::RecipeBudget,
    ) -> Self {
        let seed_sum = budget.sum_of_seed_tokens();
        // Floor of 1024 — even an empty recipe needs SOME context for
        // ad-hoc inference. The budget is a sizing hint; the policy
        // grows it later from observed demand. Above the floor,
        // honor the recipe sum.
        let computed = seed_sum.max(1024);
        self.context_length_override = Some(computed);
        self
    }

    /// Lazy-load the backend on first use. Cheap if already loaded.
    fn ensure_loaded(&self) -> Result<Arc<LlamaCppBackend>, String> {
        // Fast path — already loaded.
        if let Some(b) = self.backend.read().as_ref() {
            return Ok(b.clone());
        }

        // Slow path — load. Take write lock; another thread may have raced
        // here, so check again before constructing.
        let mut guard = self.backend.write();
        if let Some(b) = guard.as_ref() {
            return Ok(b.clone());
        }

        if !self.model_path.exists() {
            return Err(format!(
                "model GGUF not found at {:?} for model `{}` — \
                 either pull the artifact identified by the registry \
                 `gguf_hint` or \
                 override via with_model_path()",
                self.model_path, self.default_model,
            ));
        }

        enforce_residency(&self.model_path).map_err(|block| {
            format!(
                "refusing to load local llama.cpp model `{}` because residency gate failed: {block}",
                self.default_model
            )
        })?;

        // KV quant for the Active tier (the tier the backend is loaded
        // into). CpuResident and Idle quants apply later when the paging
        // substrate transitions sequences out of Active. Single source of
        // truth: the policy on this adapter, declared by the caller.
        let active_kv = self
            .kv_quant_policy
            .for_residency(crate::inference::kv_quant::Residency::Active);
        // Pull the multimodal projector path from the registry if this
        // model declares one. The registry is the source of truth for
        // per-model configuration (mmproj alongside chat_template,
        // stop_sequences, capabilities). When set, the backend's
        // generate_with_image route lazily loads the MtmdContext from it.
        // When absent, generate_with_image returns a clear error rather
        // than silently bridging to text — vision-capable callers should
        // surface that as a config issue, not a degraded experience.
        let mmproj_path = crate::model_registry::try_global()
            .and_then(|reg| reg.model(&self.default_model))
            .and_then(|m| m.mmproj_local_path.clone());
        let config = LlamaCppConfig {
            model_path: self.model_path.clone(),
            mmproj_path,
            n_gpu_layers: -1, // All layers to GPU
            // None = honor model's n_ctx_train. Adapter caller can shrink
            // this via with_context_length() to bound the KV cache (24GB
            // at 262K → 500MB at 16K).
            context_length: self.context_length_override,
            // qwen3.5's recurrent/Gated-Delta-Net Metal graph aborts inside
            // llama.cpp on the default aggressive graph shape. Keep this path
            // GPU-only but choose a conservative graph explicitly: single seq,
            // no FlashAttention auto-upgrade, smaller ubatch. That preserves
            // Rust-owned local inference while avoiding the known abort path.
            n_seq_max: 1,
            n_ubatch: 128,
            flash_attn: FlashAttn::Disabled,
            fused_gdn_ar: false,
            fused_gdn_ch: false,
            type_k: active_kv.k,
            type_v: active_kv.v,
            ..Default::default()
        };
        let backend = LlamaCppBackend::load(config)
            .map_err(|e| format!("LlamaCppBackend::load failed: {e}"))?;

        // Report model_weights bytes to the global FootprintRegistry so
        // the policy can see the on-disk size charged against this process
        // (mmap'd, so file size ≈ resident bytes for the model itself).
        // Backend-scoped key: two adapters loading two different GGUFs
        // produce two distinct entries instead of overwriting each other.
        // The size source is fs::metadata, not a backend method, because
        // llama.cpp doesn't expose a "bytes loaded" counter and the file
        // size is the most honest first-cut number.
        if let Ok(meta) = std::fs::metadata(&self.model_path) {
            use crate::inference::footprint_registry::{global, FootprintKey, ResourceType};
            use crate::inference::kv_quant::Residency;
            global().report_authoritative(
                FootprintKey::for_backend(
                    backend.model_id(),
                    ResourceType::ModelWeights,
                    Residency::Active,
                ),
                meta.len(),
            );
        }

        let arc = Arc::new(backend);
        *guard = Some(arc.clone());
        Ok(arc)
    }

    /// The most recent measured decode throughput in tokens/sec.
    /// Used for the GPU-observability hook — surface this in
    /// `TextGenerationResponse.routing` so chat can see whether the
    /// last inference looked GPU-fast or CPU-slow.
    pub fn last_throughput(&self) -> f64 {
        *self.last_throughput_tok_s.read()
    }
}

impl Default for LlamaCppAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AIProviderAdapter for LlamaCppAdapter {
    fn provider_id(&self) -> &str {
        LLAMACPP_PROVIDER_ID
    }

    fn name(&self) -> &str {
        "Llama.cpp (in-process Metal/CUDA)"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        // max_context_window: if the backend has been loaded, use the
        // model's actual training ceiling; otherwise leave 0 to signal
        // "ask the model" via model_metadata. Never invent a number.
        let max_ctx = self
            .backend
            .read()
            .as_ref()
            .map(|b| b.n_ctx_train())
            .unwrap_or(0);
        AdapterCapabilities {
            supports_text_generation: true,
            supports_chat: true,
            supports_tool_use: true,
            supports_vision: false,
            supports_streaming: true,
            supports_embeddings: false,
            supports_audio: false,
            supports_image_generation: false,
            is_local: true,
            max_context_window: max_ctx,
        }
    }

    fn api_style(&self) -> ApiStyle {
        ApiStyle::Local
    }

    fn default_model(&self) -> &str {
        &self.default_model
    }

    async fn initialize(&mut self) -> Result<(), String> {
        // Eagerly load the model at initialize time. The previous lazy-load
        // scheme meant `model_metadata()` returned None until the first
        // `generate_text` call, which in turn made TS-side callers of
        // `ai/model-info` get back nothing → they fell through to a
        // hardcoded 8192 context-window fallback, ignoring the model's
        // actual 262144. Eager-load pays the 5-10s cost once at boot and
        // guarantees every downstream consumer sees the model's real
        // capabilities from the first query on.
        //
        // If the GGUF isn't on disk we return Ok without loading —
        // `register_adapters` has already gated registration on
        // `health_check().api_available`, so we only get called when the
        // file exists. If something changed between those two checks
        // (e.g. the file was deleted), the first `generate_text` still
        // falls back to the ensure_loaded path and surfaces a clean
        // model-not-found error then.
        if self.model_path.exists() {
            let _ = self.ensure_loaded()?;
        }
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), String> {
        // Drop the backend — releases GPU memory.
        *self.backend.write() = None;
        Ok(())
    }

    async fn generate_text(
        &self,
        request: TextGenerationRequest,
    ) -> Result<TextGenerationResponse, String> {
        let backend = self.ensure_loaded()?;

        // Use the model's OWN chat template (from GGUF metadata) via
        // llama.cpp's template engine. The previous hand-rolled
        // `<|im_start|>role\n ...<|im_end|>\n` prefix was wrong for
        // qwen3.5 — it caused `<|im_end<|>` special-token leakage in
        // Teacher AI output (2026-04-20). Different models use different
        // boundary tokens; the model is the source of truth.
        // Use the model's own template if embedded in GGUF metadata;
        // Resolution order, no fallback:
        //   1. GGUF metadata `tokenizer.chat_template` (forge bake should
        //      put it here).
        //   2. models.toml `chat_template` field (memento's registry —
        //      authoritative when GGUF is silent).
        // No in-code constant. Adding a new model = TOML row, never an
        // adapter edit. If both sources are absent, render_chat passes
        // None to llama.cpp which is its own loud failure (chatml default
        // doesn't match qwen3.5's special tokens — output corruption).
        let registry_template: Option<String> = crate::model_registry::try_global()
            .and_then(|reg| reg.model(backend.model_id()))
            .and_then(|m| m.chat_template.clone());
        let template_string = backend.model_chat_template().or(registry_template);
        let template = template_string.as_deref();
        // Walk the request to find any image / audio content. If present,
        // the model MUST natively accept that modality (else the bridge
        // is wrong upstream — sensory-bridge converts to text BEFORE
        // reaching here for non-multimodal models). For vision-capable /
        // audio-capable local models with a loaded mmproj, media items
        // splice in as `<__media__>` markers inside the rendered text
        // and the call routes to `backend.generate_with_image()` /
        // `generate_with_audio()` instead of the scheduler.
        //
        // Single-media-per-call scope for v1: libmtmd's C API supports
        // multiple bitmaps per tokenize call (one marker each, in
        // order), but our backend signatures take one bytes blob. The
        // collected_media vector preserves order; if there's >1 item
        // OR a mix of image+audio, we hard-error rather than silently
        // dropping the rest. Multi-media is a follow-up once a real
        // caller needs it (mtmd_tokenize already does the work).
        // Diagnostic: prove what the adapter receives from the caller —
        // counts user message shapes (Text vs Parts) and ContentPart
        // variants. When vision routing breaks, this tells us whether
        // the image got dropped upstream (count=0, request had no
        // ContentPart::Image) vs in our walk (count>0 but
        // generate_with_image still doesn't fire). 2026-04-21: Vision AI
        // was producing wrong answers; this is the probe to localize.
        {
            let mut text_msgs = 0;
            let mut parts_msgs = 0;
            let mut parts_text = 0;
            let mut parts_image = 0;
            let mut parts_audio = 0;
            let mut parts_other = 0;
            for msg in &request.messages {
                match &msg.content {
                    MessageContent::Text(_) => text_msgs += 1,
                    MessageContent::Parts(parts) => {
                        parts_msgs += 1;
                        for p in parts {
                            match p {
                                crate::ai::types::ContentPart::Text { .. } => parts_text += 1,
                                crate::ai::types::ContentPart::Image { .. } => parts_image += 1,
                                crate::ai::types::ContentPart::Audio { .. } => parts_audio += 1,
                                _ => parts_other += 1,
                            }
                        }
                    }
                }
            }
            let log = runtime::logger("llamacpp");
            log.info(&format!(
                "generate_text request: model={} messages={} (text={} parts={}; parts contain text={} image={} audio={} other={})",
                request.model.as_deref().unwrap_or("?"),
                request.messages.len(),
                text_msgs,
                parts_msgs,
                parts_text,
                parts_image,
                parts_audio,
                parts_other,
            ));
        }

        let mut collected_media: Vec<(llama::MediaKind, Vec<u8>)> = Vec::new();
        let mut messages: Vec<llama::ChatMsg> = Vec::new();
        if let Some(sys) = request.system_prompt.as_ref() {
            if !sys.is_empty() {
                messages.push(llama::ChatMsg {
                    role: "system".to_string(),
                    content: sys.clone(),
                });
            }
        }
        for msg in &request.messages {
            let content = match &msg.content {
                MessageContent::Text(t) => t.clone(),
                MessageContent::Parts(parts) => {
                    let mut out = String::new();
                    for p in parts {
                        match p {
                            crate::ai::types::ContentPart::Text { text } => {
                                out.push_str(text);
                            }
                            crate::ai::types::ContentPart::Image { image } => {
                                // Splice the marker at this exact spot —
                                // mtmd_tokenize replaces it with the
                                // image-token chunk. Position matters
                                // (text-before-image vs after changes
                                // what the model sees).
                                out.push_str(llama::MtmdContext::default_marker());
                                let bytes = decode_image_bytes(image)?;
                                collected_media.push((llama::MediaKind::Image, bytes));
                            }
                            crate::ai::types::ContentPart::Audio { audio } => {
                                // Same shape as image — splice marker,
                                // collect bytes. mtmd's bitmap helper
                                // auto-detects audio from magic bytes;
                                // the modality tag here drives backend
                                // capability checks (supports_audio
                                // instead of supports_vision) and
                                // routing to generate_with_audio.
                                out.push_str(llama::MtmdContext::default_marker());
                                let bytes = decode_audio_bytes(audio)?;
                                collected_media.push((llama::MediaKind::Audio, bytes));
                            }
                            _ => {} // tool_use / tool_result handled by tool path, not here
                        }
                    }
                    out
                }
            };
            messages.push(llama::ChatMsg {
                role: msg.role.clone(),
                content,
            });
        }
        let prompt = llama::render_chat(template.as_deref(), &messages, true)?;

        // No hardcoded cap. If the caller didn't specify, the model can
        // decode up to its trained context. Capping silently at 2048 was
        // the source of clipped JSON/XML output — the model would stop
        // mid-structure and downstream JSON.parse / XML parsers blew up.
        let max_tokens = request
            .max_tokens
            .map(|n| n as usize)
            .unwrap_or_else(|| backend.n_ctx_train() as usize);
        let sampling = sampling_config_from_request(&request);
        // Stop sequences = caller-supplied + model's registry-declared
        // text-form stops. Some GGUFs (the forged qwen3.5 included) carry
        // the wrong tokenizer.ggml.eos_token_id, so is_eog_token never
        // fires for the chat-template terminator and the model loops the
        // same answer until max_tokens. The registry's stop_sequences
        // field carries the correct strings (e.g. `<|im_end|>`) that the
        // scheduler matches against streamed output.
        let mut stop_owned: Vec<String> = request.stop_sequences.clone().unwrap_or_default();
        if let Some(model_meta) =
            crate::model_registry::try_global().and_then(|reg| reg.model(backend.model_id()))
        {
            for s in &model_meta.stop_sequences {
                if !stop_owned.contains(s) {
                    stop_owned.push(s.clone());
                }
            }
        }

        let gen_start = Instant::now();
        let backend_for_blocking = backend.clone();
        let prompt_for_blocking = prompt.clone();
        let stop_for_closure = stop_owned.clone();
        let sampling_for_closure = sampling.clone();
        // Parse the wire-format persona_id (Option<String> on the public
        // request type) to Option<Uuid> for the typed scheduler API. A
        // malformed UUID drops to None rather than failing the request —
        // the request itself is still valid, we just can't attribute its
        // KV bytes per-persona. The registry's drift-detection sanity
        // check will surface this if it becomes systemic.
        let persona_id: Option<uuid::Uuid> = request
            .persona_id
            .as_deref()
            .and_then(|s| uuid::Uuid::parse_str(s).ok());
        let result: Result<(String, usize), String> = if collected_media.is_empty() {
            // Pure-text path: scheduler-managed continuous batching.
            tokio::task::spawn_blocking(move || {
                let stop_refs: Vec<&str> = stop_for_closure.iter().map(|s| s.as_str()).collect();
                backend_for_blocking.generate_for_persona(
                    persona_id,
                    &prompt_for_blocking,
                    max_tokens,
                    sampling_for_closure,
                    &stop_refs,
                    &[],
                )
            })
            .await
            .map_err(|e| format!("generate task panicked: {e}"))?
        } else {
            // Multimodal path: bypass the scheduler — media tokens have
            // a fixed positional layout the scheduler can't interleave
            // with concurrent text seqs. Single-media-per-call scope for
            // v1; mtmd's C API supports multiple media in one prompt
            // (one marker each in order) but our backend signatures take
            // one bytes blob. Hard-error rather than silently dropping
            // extras — clearer signal upstream.
            if collected_media.len() > 1 {
                let kinds: Vec<String> = collected_media
                    .iter()
                    .map(|(k, _)| format!("{:?}", k))
                    .collect();
                return Err(format!(
                    "llamacpp_adapter: multi-media not yet supported in this adapter \
                     ({} items: {}); send one media item per request until backend.\
                     generate_with_media accepts &[(MediaKind, Vec<u8>)]",
                    collected_media.len(),
                    kinds.join(", ")
                ));
            }
            let (kind, media_bytes) = collected_media.into_iter().next().unwrap();
            tokio::task::spawn_blocking(move || {
                let stop_refs: Vec<&str> = stop_for_closure.iter().map(|s| s.as_str()).collect();
                match kind {
                    llama::MediaKind::Image => backend_for_blocking.generate_with_image(
                        &prompt_for_blocking,
                        &media_bytes,
                        max_tokens,
                        sampling_for_closure,
                        &stop_refs,
                    ),
                    llama::MediaKind::Audio => backend_for_blocking.generate_with_audio(
                        &prompt_for_blocking,
                        &media_bytes,
                        max_tokens,
                        sampling_for_closure,
                        &stop_refs,
                    ),
                }
            })
            .await
            .map_err(|e| format!("generate_with_media task panicked: {e}"))?
        };
        let (text, tokens) = result?;

        let elapsed = gen_start.elapsed();
        let tok_per_sec = if elapsed.as_secs_f64() > 0.0 {
            tokens as f64 / elapsed.as_secs_f64()
        } else {
            0.0
        };
        *self.last_throughput_tok_s.write() = tok_per_sec;

        // No tail-strip. Previously this hand-rolled `text.rfind(stop)` and
        // truncated — only existed to clean up the special tokens that
        // leaked from the OLD hand-rolled chat-template prefixes. Now that
        // we use the model's real chat template via `render_chat`, the
        // model's actual EOS tokens stop generation (handled inside the
        // scheduler via `is_eog_token`) and don't leak as text.

        Ok(TextGenerationResponse {
            text,
            finish_reason: FinishReason::Stop,
            model: backend.model_id().to_string(),
            provider: LLAMACPP_PROVIDER_ID.to_string(),
            usage: UsageMetrics {
                input_tokens: 0, // backend doesn't return this currently; future enhancement
                output_tokens: tokens as u32,
                total_tokens: tokens as u32,
                estimated_cost: None,
            },
            response_time_ms: elapsed.as_millis() as u64,
            request_id: format!("llamacpp-{}", chrono::Utc::now().timestamp_millis()),
            content: None,
            tool_calls: None,
            routing: None,
            error: None,
        })
    }

    async fn health_check(&self) -> HealthStatus {
        let healthy = self.backend.read().is_some() || self.model_path.exists();
        HealthStatus {
            status: if healthy {
                HealthState::Healthy
            } else {
                HealthState::Unhealthy
            },
            api_available: healthy,
            response_time_ms: 0,
            error_rate: 0.0,
            last_checked: chrono::Utc::now().timestamp_millis() as u64,
            message: Some(if healthy {
                "in-process llama.cpp backend ready".to_string()
            } else {
                format!("model GGUF missing at {:?}", self.model_path)
            }),
        }
    }

    async fn get_available_models(&self) -> Vec<ModelInfo> {
        // Identity + capabilities come from the registry (config/models.toml).
        // Runtime overlay (context_window from GGUF metadata, tokens/sec
        // from last measurement) only applies if the backend is loaded;
        // otherwise we return the TOML-declared view and let the first
        // generate_text call refresh the numbers.
        let base = models_for_provider_via_registry(LLAMACPP_PROVIDER_ID);
        let backend_guard = self.backend.read();
        let last_tok_s = *self.last_throughput_tok_s.read();
        base.into_iter()
            .map(|info| match backend_guard.as_ref() {
                Some(b) if info.id == self.default_model => {
                    model_info_with_runtime(info, b, last_tok_s)
                }
                _ => info,
            })
            .collect()
    }

    fn model_metadata(&self, model_id: &str) -> Option<ModelInfo> {
        // Match against the registry (provider's declared models), then
        // overlay runtime fields if the backend happens to be loaded.
        // Matching is case-insensitive on the declared id; no substring
        // special-casing — the id is the contract.
        let want = model_id.to_lowercase();
        let info = models_for_provider_via_registry(LLAMACPP_PROVIDER_ID)
            .into_iter()
            .find(|m| m.id.to_lowercase() == want)?;
        let backend_guard = self.backend.read();
        match backend_guard.as_ref() {
            Some(b) if info.id == self.default_model => Some(model_info_with_runtime(
                info,
                b,
                *self.last_throughput_tok_s.read(),
            )),
            _ => Some(info),
        }
    }

    fn device_type(&self) -> InferenceDevice {
        // Bundled llama.cpp is built with Metal (Mac) / CUDA (Linux) per
        // continuum's build flags. Either way: GPU-class device.
        InferenceDevice::Gpu
    }

    fn supported_model_prefixes(&self) -> Vec<&'static str> {
        // Intentionally empty — this adapter lists its models explicitly
        // in the registry, and `supports_model` below matches against the
        // declared ids directly. The old hardcoded prefixes (qwen3.5-…)
        // would silently match a Qwen3.5 row under a *different* provider
        // (DMR) and mis-route it here. Exact-id match is the contract.
        Vec::new()
    }

    fn supports_model(&self, model_name: &str) -> bool {
        self.default_model.eq_ignore_ascii_case(model_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{ChatMessage, MessageContent};
    use crate::model_registry::types::{Arch, MultiPartyChatStrategy};
    use crate::model_registry::Model;
    use std::collections::BTreeSet;

    fn text_request(response_format: Option<ResponseFormat>) -> TextGenerationRequest {
        TextGenerationRequest {
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text("Return JSON.".to_string()),
                name: None,
            }],
            system_prompt: None,
            model: None,
            provider: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            top_k: None,
            repeat_penalty: None,
            stop_sequences: None,
            tools: None,
            tool_choice: None,
            response_format,
            active_adapters: None,
            request_id: None,
            user_id: None,
            room_id: None,
            purpose: None,
            persona_id: None,
        }
    }

    fn synthetic_llamacpp_local_model(id: &str, gguf_path: Option<PathBuf>) -> Model {
        Model {
            id: id.into(),
            name: None,
            provider: LLAMACPP_PROVIDER_ID.into(),
            arch: Arch::Qwen35,
            context_window: 32_768,
            max_output_tokens: 4096,
            tokens_per_second: 33.0,
            capabilities: BTreeSet::new(),
            cost_input_per_1k: 0.0,
            cost_output_per_1k: 0.0,
            gguf_hint: None,
            gguf_local_path: gguf_path,
            mmproj_local_path: None,
            chat_template: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            stop_sequences: vec![],
        }
    }

    #[test]
    fn try_new_from_errors_when_no_llamacpp_local_rows() {
        // Empty iterator — no llamacpp-local rows at all (the worst-case
        // install state continuum-8e97 saw on RTX 5090: install seeded
        // only voice-models, registry has no llamacpp-local Qwen row).
        let models: Vec<Model> = vec![];
        match LlamaCppAdapter::try_new_from(models.iter()) {
            Err(err) => {
                assert_eq!(err.provider_id, LLAMACPP_PROVIDER_ID);
                assert_eq!(err.rows_in_registry, 0);
                assert_eq!(err.rows_with_gguf_local_path, 0);
                // Error message must name the actionable next step so
                // operators see what to do (run model-init / seed manually).
                let msg = format!("{err}");
                assert!(
                    msg.contains("model-init"),
                    "error must name the actionable remediation: {msg}"
                );
            }
            Ok(_) => panic!("expected NoLocalModelLoadable on empty registry"),
        }
    }

    #[test]
    fn json_object_response_format_enables_json_grammar() {
        let sampling =
            sampling_config_from_request(&text_request(Some(ResponseFormat::JsonObject)));
        assert_eq!(sampling.grammar.as_deref(), Some(JSON_GRAMMAR));
    }

    #[test]
    fn text_response_format_leaves_grammar_unconstrained() {
        let sampling = sampling_config_from_request(&text_request(Some(ResponseFormat::Text)));
        assert!(sampling.grammar.is_none());
    }

    #[test]
    fn try_new_from_errors_when_llamacpp_rows_exist_but_none_have_gguf_path() {
        // Registry has llamacpp-local rows but artifact resolver couldn't
        // find the GGUF on disk for any of them — `gguf_local_path` is
        // None for every row. This is the SAME observable state as
        // "registry empty" from the adapter's perspective: nothing to
        // load. Operator-actionable signal must distinguish "registry is
        // wrong" (zero rows) from "files aren't seeded" (rows exist,
        // paths unresolved).
        let models = vec![
            synthetic_llamacpp_local_model("qwen3.5-4b-code-forged-GGUF", None),
            synthetic_llamacpp_local_model("qwen2-vl-7b-instruct", None),
        ];
        match LlamaCppAdapter::try_new_from(models.iter()) {
            Err(err) => {
                assert_eq!(err.provider_id, LLAMACPP_PROVIDER_ID);
                assert_eq!(err.rows_in_registry, 2);
                assert_eq!(err.rows_with_gguf_local_path, 0);
            }
            Ok(_) => panic!("expected NoLocalModelLoadable when no row has gguf_local_path"),
        }
    }

    #[test]
    fn try_new_from_succeeds_with_at_least_one_resolved_path() {
        // Mixed registry: one row has the path resolved, one doesn't.
        // Adapter should pick the resolved row (matches the existing
        // production behavior of legacy `new()`).
        let resolved_path = PathBuf::from("/tmp/synthetic-test-only.gguf");
        let models = vec![
            synthetic_llamacpp_local_model("qwen3.5-4b-code-forged-GGUF", None),
            synthetic_llamacpp_local_model("qwen2-vl-7b-instruct", Some(resolved_path.clone())),
        ];
        match LlamaCppAdapter::try_new_from(models.iter()) {
            Ok(adapter) => {
                assert_eq!(adapter.model_path, resolved_path);
                assert_eq!(adapter.default_model, "qwen2-vl-7b-instruct");
            }
            Err(err) => panic!("expected Ok with resolved path; got {err:?}"),
        }
    }
}
