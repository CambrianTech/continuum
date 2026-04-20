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

use crate::ai::adapter::{AdapterCapabilities, AIProviderAdapter, ApiStyle, InferenceDevice};
use crate::ai::registry_bridge::models_for_provider_via_registry;
use crate::ai::types::{
    FinishReason, HealthState, HealthStatus, MessageContent,
    ModelInfo, TextGenerationRequest, TextGenerationResponse, UsageMetrics,
};
use crate::inference::backends::llamacpp::{LlamaCppBackend, LlamaCppConfig};
use async_trait::async_trait;
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
}

impl LlamaCppAdapter {
    /// Construct from the model_registry. Looks up the first model under
    /// provider `llamacpp-local` that has a non-None `gguf_local_path`
    /// and uses its id + path. If the registry has no such row, panics
    /// — that's a config bug, not a runtime failure mode (per the
    /// no-fallback rule).
    pub fn new() -> Self {
        let reg = crate::model_registry::global();
        let model = reg
            .models_for_provider(LLAMACPP_PROVIDER_ID)
            .find(|m| m.gguf_local_path.is_some())
            .expect(
                "no llamacpp-local model with gguf_local_path in config/models.toml — \
                 the in-process adapter has nothing to load",
            );
        let model_path = model
            .gguf_local_path
            .clone()
            .expect("gguf_local_path present — filtered by find()");
        Self {
            backend: Arc::new(RwLock::new(None)),
            model_path,
            last_throughput_tok_s: Arc::new(RwLock::new(0.0)),
            default_model: model.id.clone(),
        }
    }

    /// Override the model path. Useful for tests + when the model isn't
    /// at the registry's declared location.
    pub fn with_model_path(mut self, path: PathBuf) -> Self {
        self.model_path = path;
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
                 either pull the artifact to that path (it's the \
                 `gguf_local_path` declared in config/models.toml) or \
                 override via with_model_path()",
                self.model_path, self.default_model,
            ));
        }

        let config = LlamaCppConfig {
            model_path: self.model_path.clone(),
            n_gpu_layers: -1, // All layers to GPU
            ..Default::default()
        };
        let backend = LlamaCppBackend::load(config)
            .map_err(|e| format!("LlamaCppBackend::load failed: {e}"))?;
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
        // Model's own template if embedded; otherwise None → llama.cpp
        // falls back to its built-in chatml default. Our forge model
        // (qwen3.5-4b-code-forged) currently doesn't embed a template
        // in GGUF metadata; the chatml default is correct for the qwen3.5
        // family. TODO: forge recipe should embed an explicit template.
        let template = backend.model_chat_template();
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
                MessageContent::Parts(parts) => parts
                    .iter()
                    .filter_map(|p| match p {
                        crate::ai::types::ContentPart::Text { text } => Some(text.as_str()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(""),
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
        let temperature = request.temperature.unwrap_or(0.7);
        // Stop sequences come from caller; the model's own EOS tokens are
        // handled inside the scheduler via `is_eog_token` so we don't need
        // to manually pass `<|im_end|>` etc here. Caller-supplied stops
        // (e.g. JSON-mode end markers) still propagate.
        let stop_owned: Vec<String> = request.stop_sequences.clone().unwrap_or_default();

        let gen_start = Instant::now();
        let backend_for_blocking = backend.clone();
        let prompt_for_blocking = prompt.clone();
        let stop_for_closure = stop_owned.clone();
        let result: Result<(String, usize), String> = tokio::task::spawn_blocking(move || {
            let stop_refs: Vec<&str> = stop_for_closure.iter().map(|s| s.as_str()).collect();
            backend_for_blocking.generate(
                &prompt_for_blocking,
                max_tokens,
                temperature,
                &stop_refs,
                &[],
            )
        })
        .await
        .map_err(|e| format!("generate task panicked: {e}"))?;
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
            status: if healthy { HealthState::Healthy } else { HealthState::Unhealthy },
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
        let want = model_name.to_lowercase();
        models_for_provider_via_registry(LLAMACPP_PROVIDER_ID)
            .iter()
            .any(|m| m.id.to_lowercase() == want)
    }
}
