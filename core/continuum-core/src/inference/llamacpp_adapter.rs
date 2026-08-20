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
    EmbeddingInput, EmbeddingRequest, EmbeddingResponse, FinishReason, HealthState, HealthStatus,
    MessageContent, ModelInfo, ResponseFormat, TextGenerationRequest, TextGenerationResponse,
    UsageMetrics,
};
use crate::inference::backends::llamacpp::{LlamaCppBackend, LlamaCppConfig};
use crate::inference::backends::{SamplingConfig, JSON_GRAMMAR};
use crate::inference_capability::enforce_residency;
use crate::model_registry::Capability;
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

/// The embed lane's identity on the resource board — the `consumer_id` on both
/// its per-call VRAM leases and its standing reservation floor.
pub const EMBED_LANE_CONSUMER_ID: &str = "embed";

/// The embed lane's working VRAM: the bounded Metal embedding context (~1.2 GiB
/// compute buffer + ~224 MiB KV) plus headroom. ONE constant, two uses: the
/// per-call lease in [`create_embedding`], and the standing FLOOR the embedder
/// reserves on the board at resolve so serving's plan can never grow into it
/// (#225 — measured 2026-08-08: the grown window left 604 MiB governed-available
/// against this 1792 MiB need, and embedding went fully dead on the box).
pub const EMBED_LANE_VRAM_BYTES: u64 = 1792 * 1024 * 1024;

/// Overlay live runtime metadata (throughput) on top of the registry's
/// declared ModelInfo. Context-window still flows from `backend.n_ctx_train()`
/// because that's the GGUF's ground truth — the catalog value is the intent,
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
    /// Continuous-batching slot count. `None` = single-seq mode
    /// (the conservative qwen3.5 default — its recurrent /
    /// Gated-Delta-Net Metal graph aborts on multi-seq). When set,
    /// the in-backend scheduler multiplexes N concurrent
    /// generations through one shared model load via
    /// `llamacpp_scheduler.rs`'s driver loop.
    ///
    /// **Coordinator wiring:** `InferenceCoordinator::open_lane`
    /// admits up to `lane_budgets.max_concurrency` lanes against
    /// this adapter; the scheduler's `n_seq_max` MUST match or
    /// exceed that number, otherwise admission lets in lanes the
    /// scheduler can't actually serve in parallel. The realistic-
    /// floor coordinator config (4 concurrent lanes) pairs with
    /// `with_n_seq_max(4)` on the adapter.
    ///
    /// **Per-model safety:** qwen3.5 (and any model with a
    /// recurrent KV layer that the Metal graph can't multiplex)
    /// must keep this at None / 1. Standard Llama / Qwen-2.5 /
    /// Gemma-2 architectures multiplex cleanly.
    ///
    /// See [`docs/architecture/INFERENCE-LANES-REALISTIC.md`]
    /// (Step 4) for the rollout plan.
    n_seq_max_override: Option<u32>,
    /// Max ubatch size override — when set, the LlamaCppConfig built at
    /// `load()` time uses this instead of the hardcoded default. The
    /// compute graph is reserved for ubatches up to this size, so
    /// setting it correctly is what avoids the
    /// `decode: failed to find a memory slot for batch of size N`
    /// panic when N exceeds the reserved graph (observed #130
    /// 2026-06-01 with RAG-built persona prompts at 337 tokens).
    /// Profile-driven via `PersonaInferenceProfile.n_ubatch`.
    n_ubatch_override: Option<u32>,
    /// GPU offload depth override. `None` = honor whatever the load
    /// path decides from tier policy. Set explicitly when the persona
    /// profile already resolved the right value (e.g., 0 on Compat
    /// tier while #131's Metal hang fix is pending).
    n_gpu_layers_override: Option<i32>,
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
            n_seq_max_override: None,
            n_ubatch_override: None,
            n_gpu_layers_override: None,
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
    /// The `model_id` MUST match a row in the Rust catalog (catalog.rs) so the
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
            n_seq_max_override: None,
            n_ubatch_override: None,
            n_gpu_layers_override: None,
        }
    }

    /// **The intent-driven constructor** per
    /// [[intent-driven-api-not-hot-patches]] (Joel, 2026-06-01).
    /// Replaces the chain of `with_model_id().with_context_length()
    /// .with_n_seq_max()...` with one call that takes a substrate-
    /// resolved profile and derives every knob from declared intent.
    ///
    /// The profile is produced by `PersonaSpawnerModule` (#121) from
    /// the persona's (role_template, hw_tier_descriptor, model_meta).
    /// Callers — chat surface, RAG inspector, future inference command
    /// hot path — never touch n_ubatch, n_seq_max, n_gpu_layers, etc.
    /// directly; they're already resolved in the profile.
    ///
    /// Returns an error per [[no-fallbacks-ever]] if the profile says
    /// "local inference" but `gguf_local_path` is None (cloud-only
    /// profiles route through Anthropic/OpenAI adapters, not here).
    pub fn for_persona(
        profile: &crate::persona::inference_profile::PersonaInferenceProfile,
    ) -> Result<Self, crate::persona::inference_profile::InferenceProfileError> {
        let gguf_path = profile.gguf_local_path.clone().ok_or_else(|| {
            crate::persona::inference_profile::InferenceProfileError::NoLocalGguf {
                model_id: profile.model_id.clone(),
                gguf_hint: None,
            }
        })?;
        Ok(Self {
            backend: Arc::new(RwLock::new(None)),
            model_path: gguf_path,
            last_throughput_tok_s: Arc::new(RwLock::new(0.0)),
            default_model: profile.model_id.clone(),
            context_length_override: Some(profile.context_length),
            kv_quant_policy: crate::inference::kv_quant::KvQuantPolicy::default(),
            n_seq_max_override: Some(profile.n_seq_max),
            n_ubatch_override: Some(profile.n_ubatch),
            n_gpu_layers_override: Some(profile.n_gpu_layers),
        })
    }

    /// Override max ubatch size — typically not needed when using
    /// `for_persona`; kept for legacy call sites and tests that
    /// construct ad-hoc adapters without a full profile.
    pub fn with_n_ubatch(mut self, n: u32) -> Self {
        self.n_ubatch_override = Some(n);
        self
    }

    /// Override GPU offload depth. `-1` = all on GPU; `0` = CPU only;
    /// N = bottom N layers on GPU. As with `with_n_ubatch`, legacy
    /// path — production code paths go through `for_persona`.
    pub fn with_n_gpu_layers(mut self, n: i32) -> Self {
        self.n_gpu_layers_override = Some(n);
        self
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

    /// Enable multi-seq continuous batching at the in-backend
    /// scheduler. Sets `LlamaCppConfig::n_seq_max = n`, which sizes
    /// the shared `Context`'s seq pool. Coordinator wiring at
    /// `InferenceCoordinator` time SHOULD set this to match (or
    /// modestly exceed) the lane budget's `max_concurrency` so the
    /// scheduler can actually serve every admitted lane in parallel.
    ///
    /// **WARNING (model-specific):** qwen3.5 (and any model with a
    /// Gated-Delta-Net or recurrent KV layer that llama.cpp's Metal
    /// graph can't multiplex) MUST keep n_seq_max=1. Standard Llama,
    /// Qwen-2.5, Gemma-2, and similar transformer architectures
    /// multiplex cleanly. Caller verifies model compatibility — the
    /// adapter doesn't auto-detect today. (Q21 follow-up: probe the
    /// model architecture at load time and refuse n_seq_max>1 for
    /// known-incompatible families.)
    pub fn with_n_seq_max(mut self, n: u32) -> Self {
        self.n_seq_max_override = Some(n.max(1));
        self
    }

    /// Current n_seq_max setting (None = single-seq default).
    /// Coordinators use this to size their admission budgets — if
    /// the adapter reports None, max_concurrency is effectively 1
    /// regardless of what the lane budget says.
    pub fn n_seq_max(&self) -> Option<u32> {
        self.n_seq_max_override
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
        // Resolve the multimodal projector via the ONE registry resolver — the
        // single source of truth for "where is this model's mmproj" (declared
        // local path first, else the projector sitting beside the GGUF in the
        // HF cache snapshot, existence-checked). Reading the raw
        // `mmproj_local_path` field here would miss the self-provisioned sibling
        // and skip the existence check; `resolve_mmproj_for_model` is what
        // `llama-server` uses too, so both serving paths agree. When set, the
        // backend's generate_with_image route lazily loads the MtmdContext from
        // it; when None, generate_with_image returns a clear error rather than
        // silently bridging to text — a config issue, not a degraded experience.
        let mmproj_path = crate::model_registry::try_global().and_then(|reg| {
            reg.model(&self.default_model)
                .and_then(crate::model_registry::artifacts::resolve_mmproj_for_model)
        });
        // CONTINUUM_TIER is set by install.sh's hardware probe (commit
        // 7b3b8e086) — when the install detects a Mac Intel + discrete
        // AMD or integrated Intel UHD host, it exports
        // CONTINUUM_TIER=mac_intel_discrete because llama.cpp's
        // Metal-AMD shaders produce garbled tokens at 0.8 tok/s with
        // hundreds of nil tensor buffer errors (continuum 2026-05-30
        // evidence on MacBookPro15,1 / Radeon Pro 560X). CPU-only at
        // 1.1 tok/s + coherent output beats broken Metal every time
        // — n_gpu_layers=0 forces the CPU path. Follow-up: native
        // Rust probe at adapter construction so this doesn't depend
        // on the install-time env-var trust chain (see task tracker).
        // Profile-driven override wins per [[intent-driven-api-not-hot-
        // patches]] — the substrate already resolved the right value from
        // the persona's tier_descriptor. Env var stays as the legacy
        // operator escape hatch for ad-hoc ad-hoc construction (tests,
        // smoke binaries that don't carry a profile yet).
        let n_gpu_layers: i32 = self
            .n_gpu_layers_override
            .unwrap_or_else(|| match std::env::var("CONTINUUM_TIER").as_deref() {
                Ok("mac_intel_discrete") => 0,
                _ => -1,
            });
        // Defense-in-depth (task #110): the realistic-lane work lifted
        // n_seq_max to a caller-controlled knob, but the substrate
        // MUST NOT enable multi-seq batching on architectures that
        // llama.cpp's batched decode aborts on (qwen3 Gated-Delta-Net,
        // mamba / rwkv / jamba / griffin / recurrentgemma /
        // falcon_mamba). The probe reads the GGUF's general.architecture
        // and classifies. Unsafe architectures clamp n_seq_max → 1
        // regardless of what the caller configured. A `tracing::warn!`
        // surfaces the clamp so operators see the safety net firing
        // instead of silent quality loss.
        let requested_n_seq_max = self.n_seq_max_override.unwrap_or(1);
        let effective_n_seq_max = if requested_n_seq_max > 1 {
            match crate::inference::batching_probe::probe_gguf_batching_safety(&self.model_path) {
                Ok(verdict) => {
                    let clamped = verdict.clamp_n_seq_max(requested_n_seq_max);
                    if clamped < requested_n_seq_max {
                        tracing::warn!(
                            arch = %verdict.arch(),
                            requested = requested_n_seq_max,
                            effective = clamped,
                            "batching_probe: clamped n_seq_max — architecture is not safe for multi-seq batching; \
                             continuous batching disabled for this adapter. Coordinator lanes \
                             will queue at the in-backend scheduler instead of running in parallel."
                        );
                    }
                    clamped
                }
                Err(err) => {
                    // Probe failure shouldn't block adapter load — but
                    // we conservatively clamp to 1 since we can't
                    // verify safety. Logged so operators chase the
                    // root cause (malformed GGUF metadata).
                    tracing::warn!(
                        error = %err,
                        requested = requested_n_seq_max,
                        "batching_probe failed — conservatively clamping n_seq_max to 1"
                    );
                    1
                }
            }
        } else {
            requested_n_seq_max
        };

        let config = LlamaCppConfig {
            model_path: self.model_path.clone(),
            mmproj_path,
            n_gpu_layers,
            // None = honor model's n_ctx_train. Adapter caller can shrink
            // this via with_context_length() to bound the KV cache (24GB
            // at 262K → 500MB at 16K).
            context_length: self.context_length_override,
            // n_seq_max comes from the adapter's override clamped by
            // the model-arch probe above. Standard transformers
            // (Llama, Qwen-2.5, Gemma-2, ...) pass through; recurrent
            // / state-space / hybrid families (qwen3, mamba, rwkv,
            // jamba, ...) clamp to 1. See task #110.
            n_seq_max: effective_n_seq_max,
            // Profile-driven override wins per [[intent-driven-api-not-
            // hot-patches]]. Fallback to 512 (not the old 128 default)
            // because compute-graph reservation matches n_ubatch and a
            // RAG-built persona prompt arrives at 200-500 tokens — at
            // n_ubatch=128 the scheduler panicked with "decode: failed
            // to find a memory slot for batch of size 337" during #130
            // 2026-06-01 multi-persona LCD chat. 512 covers realistic
            // persona prompts without ballooning memory (graph nodes
            // scale with n_ubatch but at ~4 KiB per node × 942 nodes ×
            // 4 multiplier we're talking ~15 MiB per scheduler — trivial).
            // Future: derive from the Rust catalog (catalog.rs) row per [[orm-everything-
            // not-hand-edited-files]] so each model declares its own
            // realistic batch ceiling.
            n_ubatch: self.n_ubatch_override.unwrap_or(512),
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
        // llama.cpp does text + chat + streaming, native (prompt-driven) tool
        // calls, and embeddings (--embedding mode). Vision is handled by the
        // mmproj adapter when loaded, not declared at this text-LLM layer;
        // audio is bridged via STT (whisper) / TTS in the substrate.
        // Tools are prompt-driven (no native protocol); structured output via
        // GBNF grammar-constrained sampling, which IS native to llama.cpp.
        AdapterCapabilities::builder()
            .capabilities([
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Streaming,
                Capability::Embedding,
            ])
            .local()
            .context_window(max_ctx)
            .max_output_tokens(4096)
            .protocols(crate::ai::adapter::NativeProtocols::GrammarConstrained)
            .build()
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

    async fn warmup(&self) -> Result<(), String> {
        // Tiny throwaway decode against a minimal prompt. The model
        // file is already loaded by `initialize`; this call exercises
        // the KV-cache allocation path, the attention kernels, and
        // the sampler state — so when the first real `generate_text`
        // lands, it pays only the marginal per-token cost, not the
        // cold-cache JIT bill.
        //
        // Per [[init-once-handle-then-lease-zero-copy-refs]]: the
        // adapter's hot path is leased per turn; the substrate pays
        // the init cost ONCE here, at boot, never on a user's first
        // message. On Intel Mac with Qwen 0.5B this saves ~200-500ms
        // off the first turn; on M5 Metal with a larger model the
        // save is multiples of that.
        let warmup_request = TextGenerationRequest {
            messages: vec![crate::ai::types::ChatMessage::text("user", "Hi")],
            system_prompt: None,
            model: None,
            provider: None,
            temperature: Some(0.0),
            max_tokens: Some(1),
            top_p: None,
            top_k: None,
            repeat_penalty: None,
            frequency_penalty: None,
            repeat_last_n: None,
            stop_sequences: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            active_adapters: None,
            request_id: Some("warmup".to_string()),
            user_id: None,
            room_id: None,
            purpose: Some("warmup".to_string()),
            persona_id: None,
        };
        match self.generate_text(warmup_request).await {
            Ok(_) => Ok(()),
            Err(e) => Err(format!("LlamaCppAdapter warmup decode failed: {e}")),
        }
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
        //   2. the Rust catalog (catalog.rs) `chat_template` field (memento's registry —
        //      authoritative when GGUF is silent).
        // No in-code constant. Adding a new model = catalog row, never an
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

            // RTOS probe: inference entry seam. The persona service loop
            // talks through `adapter.generate_text`; this is where the
            // wall-clock cost actually accrues. Class taxonomy lives in
            // docs/architecture/RTOS-DEBUGGER-PROBES.md. Per
            // [[jtag-probes-are-rtos-debugger]]: name the surrounding
            // vars so the operator filtering on
            // `class=="inference.generate.enter"` has the request
            // fingerprint without grepping logs.
            crate::probe!(
                class = "inference.generate.enter",
                model = request.model.as_deref().unwrap_or("?"),
                persona_id = request.persona_id.as_deref().unwrap_or(""),
                msg_count = request.messages.len(),
                max_tokens = request.max_tokens.unwrap_or(0),
                has_system_prompt = request.system_prompt.is_some(),
                parts_image = parts_image,
                parts_audio = parts_audio,
                "generate_text entry"
            );
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
        // RTOS probe: chat-template rendering is synchronous + small,
        // but cumulative across thousands of turns it can shadow real
        // bottlenecks. Bracketing it lets the operator subtract it
        // from `inference.forward.*` cleanly.
        let prompt = crate::time_sync!("inference.render_chat", {
            llama::render_chat(template.as_deref(), &messages, true)
        })?;

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

        // Genome paging: the adapter contract carries the genes to apply as
        // `(name, path, scale)`. We page each in (idempotent) and hand the
        // resolved `(id, scale)` list to the scheduler, which applies it
        // context-level before decode. Empty in the common base case.
        let requested_genes: Vec<(String, std::path::PathBuf, f32)> = request
            .active_adapters
            .as_ref()
            .map(|v| {
                v.iter()
                    .map(|a| {
                        (
                            a.name.clone(),
                            std::path::PathBuf::from(&a.path),
                            a.scale as f32,
                        )
                    })
                    .collect()
            })
            .unwrap_or_default();

        let result: Result<(String, usize), String> = if collected_media.is_empty() {
            // Pure-text path: scheduler-managed continuous batching.
            // RTOS timing probe: this is the actual LLM forward pass —
            // by far the dominant cost (95%+ on LCD tier per
            // 2026-06-06 baseline). `time_probe!` wraps the JoinHandle
            // future cleanly across the `.await`.
            let genes_for_closure = requested_genes;
            crate::time_probe!(
                "inference.forward.text",
                tokio::task::spawn_blocking(move || {
                    let stop_refs: Vec<&str> =
                        stop_for_closure.iter().map(|s| s.as_str()).collect();
                    // Page in each requested gene (idempotent) before generation.
                    // A missing/unreadable adapter file is a hard error — never
                    // silently run the base model in its place (Rule 2).
                    for (id, path, _) in &genes_for_closure {
                        backend_for_blocking.ensure_adapter(id, path)?;
                    }
                    let active_loras: Vec<(String, f32)> = genes_for_closure
                        .iter()
                        .map(|(id, _, scale)| (id.clone(), *scale))
                        .collect();
                    backend_for_blocking.generate_for_persona(
                        persona_id,
                        &prompt_for_blocking,
                        max_tokens,
                        sampling_for_closure,
                        &stop_refs,
                        &active_loras,
                    )
                })
            )
            .map_err(|e| format!("generate task panicked: {e}"))?
        } else {
            // Multimodal path: bypass the scheduler — media tokens have
            // a fixed positional layout the scheduler can't interleave
            // with concurrent text seqs. Single-media-per-call scope for
            // v1; mtmd's C API supports multiple media in one prompt
            // (one marker each in order) but our backend signatures take
            // one bytes blob. Hard-error rather than silently dropping
            // extras — clearer signal upstream.
            // Genes on the multimodal bypass path are not yet supported (the
            // mtmd single-flight path doesn't route through the scheduler that
            // applies set_loras). Fail loud rather than silently dropping the
            // requested adapter (Rule 2).
            if !requested_genes.is_empty() {
                let names: Vec<&str> = requested_genes
                    .iter()
                    .map(|(id, _, _)| id.as_str())
                    .collect();
                return Err(format!(
                    "llamacpp_adapter: LoRA genes {names:?} requested with media — not supported \
                     on the multimodal bypass path (v1). Genes apply only on the text scheduler \
                     path; send the gene-bearing request without media."
                ));
            }
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
            // RTOS timing probe: mtmd path runs single-flight (no
            // scheduler batching for media) so the timing here is
            // direct end-to-end. Separate seam from the text path so
            // operators can `jq` text-only vs mtmd cost distinctly.
            crate::time_probe!(
                "inference.forward.multimodal",
                tokio::task::spawn_blocking(move || {
                    let stop_refs: Vec<&str> =
                        stop_for_closure.iter().map(|s| s.as_str()).collect();
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
            )
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

        // RTOS probe: inference exit seam. Pair with
        // `inference.generate.enter` via the same span ancestry. The
        // critical campaign metric is `tok_per_sec` — every probe
        // here carries it so a `jq` over `inference.generate.exit`
        // events is the latency dashboard. `text_len` lets the
        // operator catch the silent-truncation class of bug where
        // the model stops short of EOS.
        // Real tokens on the served lane = proof of life for the health heartbeat, which
        // otherwise probes for a slot it cannot get while this very work holds them all.
        if tokens > 0 {
            crate::inference::llama_server::note_real_decode();
        }
        crate::probe!(
            class = "inference.generate.exit",
            model = backend.model_id(),
            tokens_out = tokens,
            text_len = text.len(),
            duration_ms = elapsed.as_millis() as u64,
            tok_per_sec = tok_per_sec,
            "generate_text exit"
        );

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
            // TODO: if this in-process backend serves a reasoning model (qwen3 etc.)
            // it would emit inline `<think>` in `text` too — apply
            // `crate::ai::openai_adapter::extract_reasoning` here to separate it.
            // Not Asha's path today (she routes through the unsloth/openai adapter).
            reasoning: None,
            routing: None,
            error: None,
            timing: None,
        })
    }

    /// Embeddings via the backend's dedicated embedding-mode context. The loaded
    /// model determines the vector space — for grid-comparable vectors this
    /// adapter must have loaded the canonical Qwen3-Embedding-0.6B (the
    /// `NeuralEmbeddingProvider` is responsible for loading it). `backend.embed`
    /// is a blocking forward pass, so it runs on `spawn_blocking`, off the async
    /// executor (the same bridge as `generate_text`'s forward).
    async fn create_embedding(
        &self,
        request: EmbeddingRequest,
    ) -> Result<EmbeddingResponse, String> {
        let backend = self.ensure_loaded()?;
        let model_id = backend.model_id().to_string();
        let texts: Vec<String> = match request.input {
            EmbeddingInput::Single(s) => vec![s],
            EmbeddingInput::Multiple(v) => v,
        };
        let zero_usage = UsageMetrics {
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            estimated_cost: None,
        };
        if texts.is_empty() {
            return Ok(EmbeddingResponse {
                embeddings: Vec::new(),
                model: model_id,
                provider: LLAMACPP_PROVIDER_ID.to_string(),
                usage: zero_usage,
                response_time_ms: 0,
            });
        }
        let start = Instant::now();

        // ── Governed GPU admission (mirrors cognition/eval.rs::acquire_eval_lane_slot) ──
        // The embedding forward pass allocates a bounded Metal context (a 2048-token
        // embedding context: ~1.2 GiB compute buffer — quadratic in n_ubatch=2048 —
        // plus ~224 MiB KV). UNGOVERNED, it grabbed that context behind the governor's
        // back and competed with the serving lane for VRAM; under pressure the Metal
        // command buffer OOM'd and the decode returned an ALL-ZERO vector — the
        // "degenerate embedding" that silently broke semantic recall. Lease the VRAM
        // from the ResourceGovernor FIRST: granted ⇒ the bytes are reserved for the
        // life of the guard and the decode has room to succeed; refused ⇒ fail LOUD
        // here instead of allocating a doomed context that emits garbage. Embeddings
        // are GPU-only ([[gpu-is-non-negotiable-every-component-no-cpu-fallback]]), so
        // unlike the eval lane there is NO CPU spill — the honest degrade is a named
        // refusal the caller ([`NeuralEmbeddingProvider::embed`]) already surfaces as
        // "no signal", never a zero vector.
        //
        // Const, not env-tunable — substrate policy lives in code (concurrency guide).
        // Only the per-call CONTEXT is leased here; the embed lane's standing FLOOR on
        // the board (so serving can never grow into this slice — Joel 2026-08-08: "the
        // budgeter just has all its parts figure it out") is claimed at embedder
        // resolve via [`crate::resources::ResourceDaemon::reserve`], from the same
        // module-level constants below.
        const EMBED_LANE_LEASE_TTL_MS: u64 = 60_000; // SIGKILL backstop; the RAII guard frees on drop
        let _vram_lease = {
            use crate::resources::{
                LeaseError, LeaseRequest, ReclaimPolicy, ResourceDaemon, ResourceKind,
            };
            match ResourceDaemon::global() {
                Some(daemon) => {
                    let req = LeaseRequest {
                        consumer_id: EMBED_LANE_CONSUMER_ID.to_string(),
                        kind: ResourceKind::Vram,
                        bytes: EMBED_LANE_VRAM_BYTES,
                        // A bounded, sub-second forward pass is not yanked mid-embed;
                        // the guard returns the bytes the instant it finishes.
                        ttl_ms: EMBED_LANE_LEASE_TTL_MS,
                        reclaim_policy: ReclaimPolicy::Pinned,
                    };
                    // ONE bounded retry on a capacity refusal (never a loop —
                    // [[brittleness-is-the-highest-priority-work-there-is]]): with
                    // the standing floor reserved at resolve, a refusal here means
                    // a TRANSIENT over-commit (a lane mid-relaunch, a burst), and
                    // relief lands within a governor tick. The second refusal is
                    // the honest failure.
                    let mut refused: Option<u64> = None;
                    let mut granted = None;
                    for attempt in 0..2u8 {
                        match daemon.acquire_guarded(&req) {
                            Ok(guard) => {
                                crate::probe!(
                                    class = "embed.vram.leased",
                                    consumer = EMBED_LANE_CONSUMER_ID,
                                    bytes = EMBED_LANE_VRAM_BYTES,
                                    texts = texts.len(),
                                    retried = (attempt > 0),
                                    "embedding lane acquired a governed VRAM lease"
                                );
                                granted = Some(guard);
                                break;
                            }
                            Err(LeaseError::InsufficientCapacity { available, .. }) => {
                                crate::probe!(
                                    class = "embed.vram.refused",
                                    requested = EMBED_LANE_VRAM_BYTES,
                                    available = available,
                                    attempt = attempt,
                                    "embedding VRAM lease refused — failing loud, NOT emitting degenerate zeros"
                                );
                                refused = Some(available);
                                if attempt == 0 {
                                    tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                                }
                            }
                            Err(e) => {
                                return Err(format!(
                                    "embed: VRAM lease error ({e:?}) — refusing to embed ungoverned"
                                ));
                            }
                        }
                    }
                    match granted {
                        Some(guard) => Some(guard),
                        None => {
                            let available = refused.unwrap_or(0);
                            return Err(format!(
                                "embed: VRAM lease refused twice — {} MiB requested, {} MiB governed-available. \
                                 Refusing to allocate an OOM-doomed embedding context (it would decode to \
                                 degenerate zeros). Free VRAM (tier down serving) and retry.",
                                EMBED_LANE_VRAM_BYTES / (1024 * 1024),
                                available / (1024 * 1024),
                            ));
                        }
                    }
                }
                // Ungoverned node (no ResourceDaemon::global()): behavior unchanged —
                // the backend's own new_context allocation is the only gate, as before.
                None => None,
            }
        };

        let embeddings = tokio::task::spawn_blocking(move || backend.embed(&texts))
            .await
            .map_err(|e| format!("embedding task join failed: {e}"))??;
        // `_vram_lease` drops here → the VRAM is returned to the governor's board.
        Ok(EmbeddingResponse {
            embeddings,
            model: model_id,
            provider: LLAMACPP_PROVIDER_ID.to_string(),
            usage: zero_usage,
            response_time_ms: start.elapsed().as_millis() as u64,
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
        // Identity + capabilities come from the registry (the Rust catalog (catalog.rs)).
        // Runtime overlay (context_window from GGUF metadata, tokens/sec
        // from last measurement) only applies if the backend is loaded;
        // otherwise we return the catalog-declared view and let the first
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

    fn lcd_compat_profile() -> crate::persona::inference_profile::PersonaInferenceProfile {
        use crate::persona::hw_tier_descriptor::HwTierCategory;
        use crate::persona::inference_profile::{PersonaInferenceProfile, SamplingProfile};
        use uuid::Uuid;
        PersonaInferenceProfile {
            persona_id: Uuid::nil(),
            persona_name: "Paige".to_string(),
            model_id: "continuum-ai/qwen2.5-0.5b-instruct-GGUF".to_string(),
            gguf_local_path: Some(PathBuf::from("/tmp/test-qwen2.5-0.5b-instruct-q4_k_m.gguf")),
            tier_category: HwTierCategory::Compat,
            tier_id: "mac_intel_metal_discrete".to_string(),
            context_length: 2048,
            n_ubatch: 512,
            n_batch: 2048,
            n_seq_max: 1,
            n_gpu_layers: 0,
            sampling: SamplingProfile::chat_defaults(),
            chat_template: None,
            stop_sequences: vec!["<|im_end|>".to_string()],
        }
    }

    /// `for_persona` produces an adapter with every override field set
    /// from the profile. Without this, the substrate's intent-driven
    /// guarantee per [[intent-driven-api-not-hot-patches]] breaks:
    /// hardcoded defaults silently override what the spawner resolved.
    #[test]
    fn for_persona_populates_all_overrides_from_profile() {
        let profile = lcd_compat_profile();
        let adapter = LlamaCppAdapter::for_persona(&profile).expect("build adapter");
        assert_eq!(
            adapter.model_path,
            PathBuf::from("/tmp/test-qwen2.5-0.5b-instruct-q4_k_m.gguf")
        );
        assert_eq!(adapter.default_model, profile.model_id);
        assert_eq!(adapter.context_length_override, Some(2048));
        assert_eq!(adapter.n_seq_max_override, Some(1));
        assert_eq!(adapter.n_ubatch_override, Some(512));
        assert_eq!(adapter.n_gpu_layers_override, Some(0));
    }

    /// A profile with no `gguf_local_path` is invalid for local
    /// inference. `for_persona` rejects it loud per [[no-fallbacks-
    /// ever]] — better an error message naming the missing field than
    /// a silent fallback to a "default" model.
    #[test]
    fn for_persona_errors_when_gguf_local_path_missing() {
        let mut profile = lcd_compat_profile();
        profile.gguf_local_path = None;
        // `LlamaCppAdapter` doesn't derive Debug (Arc<RwLock<...>> isn't
        // straightforward to format), so `expect_err` won't compile.
        // Match on the result directly.
        match LlamaCppAdapter::for_persona(&profile) {
            Ok(_) => panic!("missing gguf_local_path must error per no-fallbacks doctrine"),
            Err(crate::persona::inference_profile::InferenceProfileError::NoLocalGguf {
                model_id,
                ..
            }) => {
                assert_eq!(model_id, "continuum-ai/qwen2.5-0.5b-instruct-GGUF");
            }
            Err(other) => panic!("unexpected error variant: {other:?}"),
        }
    }

    /// `with_n_ubatch` and `with_n_gpu_layers` setters work for legacy
    /// call sites + tests that build adapters without a full profile.
    /// They're the escape hatch; production paths use `for_persona`.
    #[test]
    fn with_n_ubatch_and_n_gpu_layers_setters() {
        let adapter =
            LlamaCppAdapter::with_model_id(PathBuf::from("/tmp/x.gguf"), "model".to_string())
                .with_n_ubatch(1024)
                .with_n_gpu_layers(20);
        assert_eq!(adapter.n_ubatch_override, Some(1024));
        assert_eq!(adapter.n_gpu_layers_override, Some(20));
    }

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
            frequency_penalty: None,
            repeat_last_n: None,
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
            weights_bytes: None,
            mmproj_bytes: None,
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
            hf_source: None,
            gguf_local_path: gguf_path,
            mmproj_local_path: None,
            chat_template: None,
            multi_party_strategy: MultiPartyChatStrategy::default(),
            stop_sequences: vec![],
            parameter_count: 0,
            sampling: crate::model_registry::types::ModelSampling::default(),
            persona_serving_eligible: true,
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

    // ── n_seq_max coordinator wiring (task #109, step 4) ────────

    #[test]
    fn n_seq_max_defaults_to_none_for_single_seq_backcompat() {
        // The adapter without a configured override stays in the
        // historical single-seq mode. Qwen3.5 + recurrent / GDN
        // models stay safe; older callers' behavior is unchanged.
        let adapter = LlamaCppAdapter::with_model_id(
            PathBuf::from("/tmp/no-such-file.gguf"),
            "test-model".to_string(),
        );
        assert_eq!(adapter.n_seq_max(), None);
    }

    #[test]
    fn n_seq_max_override_round_trips_through_builder() {
        // Coordinator wiring sets this from
        // CoordinatorConfig.lane_budgets.max_concurrency.
        let adapter = LlamaCppAdapter::with_model_id(
            PathBuf::from("/tmp/no-such-file.gguf"),
            "test-model".to_string(),
        )
        .with_n_seq_max(4);
        assert_eq!(adapter.n_seq_max(), Some(4));
    }

    #[test]
    fn n_seq_max_zero_clamps_to_one() {
        // Zero would be a config error — the backend's scheduler
        // can't serve any seq with n_seq_max=0. Clamping to 1
        // matches the back-compat default and avoids load-time
        // panics from inside llama.cpp.
        let adapter = LlamaCppAdapter::with_model_id(
            PathBuf::from("/tmp/no-such-file.gguf"),
            "test-model".to_string(),
        )
        .with_n_seq_max(0);
        assert_eq!(adapter.n_seq_max(), Some(1));
    }

    #[test]
    fn n_seq_max_builder_composes_with_other_overrides() {
        // Builders should chain — coordinator wiring sets context,
        // KV quant, AND n_seq_max in one builder pipeline.
        let adapter = LlamaCppAdapter::with_model_id(
            PathBuf::from("/tmp/no-such-file.gguf"),
            "test-model".to_string(),
        )
        .with_context_length(16_384)
        .with_n_seq_max(4)
        .with_kv_quant_policy(crate::inference::kv_quant::KvQuantPolicy::default());
        assert_eq!(adapter.n_seq_max(), Some(4));
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

    /// what this catches: that the OWNED in-process page-in (`ensure_adapter`
    /// → scheduler `set_loras` before decode, committed d763da1b7) actually
    /// LOADS a GGUF-lora produced by our own conversion path
    /// (`forge::lora_convert::mlx_to_gguf_lora`) against the real dense base
    /// and CHANGES the generated text vs the bare base. This is the last
    /// unproven link in the owned genome loop: train (mlx_lm) → transpose
    /// (Rust) → GGUF-lora (owned) → PAGE-IN (this). Greedy decode (temp 0) on
    /// both arms so any output difference is the gene, not sampling noise.
    ///
    /// `#[ignore]` — loads a ~5.8 GB base + runs two forward passes; needs the
    /// metal/accelerate build and the on-disk artifacts. Run explicitly:
    ///   cargo test -p continuum-core --features metal,accelerate \
    ///     --lib owned_gene_page_in_changes_output -- --ignored --nocapture
    /// Override paths via CONTINUUM_BASE_GGUF / CONTINUUM_GENE_GGUF.
    #[tokio::test]
    #[ignore]
    async fn owned_gene_page_in_changes_output() {
        use crate::ai::adapter::AIProviderAdapter;
        use crate::ai::types::ActiveAdapterRequest;

        let home = std::env::var("HOME").expect("HOME");
        let env_or = |k: &str, d: String| std::env::var(k).unwrap_or(d);
        let base = PathBuf::from(env_or(
            "CONTINUUM_BASE_GGUF",
            format!("{home}/.continuum/models/qwen2.5-coder-3b-instruct-f16.gguf"),
        ));
        let gene = env_or(
            "CONTINUUM_GENE_GGUF",
            format!("{home}/.continuum/forge/gguf-lora/coder-3b-dense.gguf"),
        );
        assert!(base.exists(), "base GGUF missing at {base:?}");
        assert!(
            std::path::Path::new(&gene).exists(),
            "gene GGUF-lora missing at {gene}"
        );

        // A code-write prompt from the gym's wheelhouse — the gene was trained
        // on the coder curriculum, so its influence should surface here.
        let prompt = "Write a Rust function `fn is_palindrome(s: &str) -> bool` \
            that ignores case and non-alphanumeric characters. Reply with only the code.";
        let make_req = |adapters: Option<Vec<ActiveAdapterRequest>>| TextGenerationRequest {
            messages: vec![ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text(prompt.to_string()),
                name: None,
            }],
            system_prompt: None,
            model: None,
            provider: None,
            temperature: Some(0.0), // greedy — isolate the gene from sampling
            max_tokens: Some(256),
            top_p: None,
            top_k: None,
            repeat_penalty: None,
            frequency_penalty: None,
            repeat_last_n: None,
            stop_sequences: None,
            tools: None,
            tool_choice: None,
            response_format: Some(ResponseFormat::Text),
            active_adapters: adapters,
            request_id: None,
            user_id: None,
            room_id: None,
            purpose: None,
            persona_id: Some(uuid::Uuid::nil().to_string()),
        };

        // context_length MUST be set explicitly — the scheduler refuses the
        // GGUF's n_ctx_train (262144) fallback (would crush Metal KV alloc).
        let adapter =
            LlamaCppAdapter::with_model_id(base.clone(), "qwen2.5-coder-3b-instruct".to_string())
                .with_context_length(2048);

        let out_base = adapter
            .generate_text(make_req(None))
            .await
            .expect("base generation failed")
            .text;
        let out_gene = adapter
            .generate_text(make_req(Some(vec![ActiveAdapterRequest {
                name: "coder-3b-dense".to_string(),
                path: gene.clone(),
                domain: "code".to_string(),
                scale: 1.0,
            }])))
            .await
            .expect("gene generation failed")
            .text;

        println!("=== BASE ===\n{out_base}\n=== GENE ===\n{out_gene}\n=== END ===");
        assert!(!out_base.trim().is_empty(), "base produced empty output");
        assert!(!out_gene.trim().is_empty(), "gene produced empty output");
        // The gene MUST change greedy output — identical text means set_loras
        // silently no-op'd (the exact failure the owned page-in must not have).
        assert_ne!(
            out_base, out_gene,
            "gene paged in but output is byte-identical to base — set_loras did not apply"
        );
    }
}
