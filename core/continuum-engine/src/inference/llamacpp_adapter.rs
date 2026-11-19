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
mod provider;
pub use construction::LlamaCppLaunch;
mod engine_start;
pub use engine_start::{EngineKv, EngineLaunch, EngineLaunchRefusal, Offload};
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
    /// the in-backend scheduler admitted N concurrent generations. The
    /// persona scheduler now serves one mind at a time whatever this says;
    /// the field is removed with the lanes (serving integration S3).
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
    /// How the backend is configured at load: the registry knobs above, or the node's one engine
    /// exactly as serving launched it (then the knobs above are unused).
    plan: engine_start::LoadPlan,
}

