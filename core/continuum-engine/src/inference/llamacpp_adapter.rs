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
use crate::inference::backends::llamacpp::{GenerationAllocation, LlamaCppBackend, LlamaCppConfig};
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

pub use super::embed_lane_memory::EMBED_LANE_CONSUMER_ID;

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

mod chat_turn;
mod construction;
mod generate;
pub use construction::LlamaCppLaunch;
mod payload;
#[cfg(test)]
mod tests;

use payload::{decode_audio_bytes, decode_image_bytes, sampling_config_from_request};

#[derive(Clone, Copy)]
enum GenerationSupport {
    EmbeddingOnly,
    Enabled { context: u32 },
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
    generation: GenerationSupport,
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
            .unwrap_or(0); // unwrap_or: no backend loaded yet; 0 is the capabilities contract's "ask the model" (model_metadata)
        let capabilities = match self.generation {
            GenerationSupport::EmbeddingOnly => vec![Capability::Embedding],
            GenerationSupport::Enabled { .. } => vec![
                Capability::TextGeneration, Capability::Chat, Capability::ToolUse,
                Capability::Streaming, Capability::Embedding,
            ],
        };
        let output_window = match self.generation {
            GenerationSupport::EmbeddingOnly => 0,
            GenerationSupport::Enabled { context } => context,
        };
        AdapterCapabilities::builder()
            .capabilities(capabilities)
            .local()
            .context_window(max_ctx)
            .max_output_tokens(output_window)
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
            tail_tools: None,
            view_messages: None,
            tool_choice: None,
            response_format: None,
            active_adapters: None,
            request_id: Some("warmup".to_string()),
            user_id: None,
            room_id: None,
            purpose: Some("warmup".to_string()),
            persona_id: None,
            turn_bound: None,
        };
        // A warm-up decode states no think term: the model's own default, as it always ran.
        let terms = crate::ai::types::DispatchTerms::thinking(crate::ai::types::Think::ModelDefault);
        match crate::ai::stream_sinks::drain_generation(|sink| crate::ai::adapter::DispatchTermsPolicy::generate_admitted(self, warmup_request, terms, sink)).await {
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
        _dispatched: crate::ai::adapter::Dispatched,
    ) -> Result<TextGenerationResponse, String> {
        self.generate(request).await
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

        // ── Governed GPU admission (mirrors inference::single_purpose_lane::acquire_lane_slot) ──
        // Wait without a memory claim. The admitted worker owns both through context teardown,
        // even when the request future is cancelled after spawn_blocking starts.
        let embedding = backend.admit_embedding().await?;
        // Tokenized first, so the context's memory is known before it exists: the lease is what THIS
        // request allocates, in the pool the context resides in, within the lane's need (its weights
        // and its window's context, `embed_lane_memory`). Ungoverned, a context grabbed memory behind
        // the governor's back and, under pressure, decoded to an all-zero vector that silently broke
        // recall; a fixed 1,792 MiB bound refused 91% of short texts on the M5 (26960c09d, 758 of 834
        // in 3 h).
        let residency = backend.embed_residency().ok_or(super::backends::llamacpp::NO_TRAINED_CONTEXT)?;
        let batch = embedding.tokenize(&texts)?;
        let lease = residency
            .lease(batch.context_bytes(backend.placement()), texts.len())
            .await
            .map_err(|e| e.to_string())?;
        let embeddings = embedding.run(batch, lease).await?;
        Ok(EmbeddingResponse {
            embeddings,
            model: model_id,
            provider: LLAMACPP_PROVIDER_ID.to_string(),
            usage: zero_usage,
            response_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn health_check(&self) -> HealthStatus {
        let (healthy, message) = if self.backend.read().is_some() {
            (true, "in-process llama.cpp backend loaded".to_string())
        } else {
            match std::fs::metadata(&self.model_path) {
                Ok(metadata) if metadata.is_file() =>
                    (true, "model file available; backend not loaded".to_string()),
                Ok(_) => (false, "model path is not a regular file".to_string()),
                Err(error) => (false, format!("model file metadata: {error}")),
            }
        };
        HealthStatus {
            status: if healthy {
                HealthState::Healthy
            } else {
                HealthState::Unhealthy
            },
            api_available: healthy,
            response_time_ms: 0,
            error_rate: 0.0,
            last_checked: continuum_base::time::EpochMs::now().millis(),
            message: Some(message),
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
