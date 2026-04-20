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
use crate::ai::types::{
    CostPer1kTokens, FinishReason, HealthState, HealthStatus, MessageContent,
    ModelCapability, ModelInfo, TextGenerationRequest, TextGenerationResponse, UsageMetrics,
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

/// Build the ModelInfo for our forge model. Context-window and
/// max-output-tokens come from the LOADED model — its GGUF metadata is
/// the source of truth for "what can this model handle." No hardcoded
/// caps. If callers want a smaller window they pass it explicitly; the
/// adapter never invents its own MAX. Throughput is the last measured
/// value, refreshed on every inference.
fn forge_qwen35_4b_model_info(backend: &LlamaCppBackend, last_tok_per_s: f64) -> ModelInfo {
    let n_ctx = backend.n_ctx_train();
    ModelInfo {
        id: "continuum-ai/qwen3.5-4b-code-forged-GGUF".to_string(),
        name: "Qwen3.5 4B Code Forged (in-process llama.cpp Metal)".to_string(),
        provider: LLAMACPP_PROVIDER_ID.to_string(),
        capabilities: vec![
            ModelCapability::TextGeneration,
            ModelCapability::Chat,
            ModelCapability::ToolUse,
        ],
        context_window: n_ctx,
        // The model can decode up to its full context window. If a caller
        // has reason to limit output (UX latency, RAG reservations) they
        // declare it on the request — never as a baked-in adapter cap.
        max_output_tokens: n_ctx,
        cost_per_1k_tokens: CostPer1kTokens { input: 0.0, output: 0.0 },
        tokens_per_second: last_tok_per_s as f32,
        supports_streaming: true,
        supports_tools: true,
    }
}

/// The default GGUF path layout DMR uses on Mac. We piggyback on its
/// download cache rather than pulling our own copy — same model file,
/// no duplication. If DMR isn't installed, this path won't exist and
/// initialization fails loud (per the no-fallback rule).
fn default_qwen35_gguf_path() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    PathBuf::from(format!(
        "{home}/.docker/models/bundles/sha256/\
         18055fe8ee379b95f4af3cf420588c5daa28f2a1ce1da335112a2d1ea188d3e6/model/model.gguf"
    ))
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
}

impl LlamaCppAdapter {
    /// Construct with the default qwen3.5-4b path (DMR's download cache).
    /// To use a different model, use `with_model_path`.
    pub fn new() -> Self {
        Self {
            backend: Arc::new(RwLock::new(None)),
            model_path: default_qwen35_gguf_path(),
            last_throughput_tok_s: Arc::new(RwLock::new(0.0)),
        }
    }

    /// Override the model path. Useful for tests + when the model isn't
    /// at DMR's standard location.
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
                "model GGUF not found at {:?} — pull via DMR \
                 (`docker model pull huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf`) \
                 or override the path via with_model_path()",
                self.model_path
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
        "continuum-ai/qwen3.5-4b-code-forged-GGUF"
    }

    async fn initialize(&mut self) -> Result<(), String> {
        // Don't load the model here — keep registration cheap. The first
        // `generate_text` call triggers `ensure_loaded`. This avoids
        // paying the load cost when the adapter is registered but never
        // exercised (e.g., user only uses cloud providers).
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

        // Flatten the structured request into the single prompt string
        // the in-process backend expects. Apply the system prompt + each
        // message in role-prefixed form. Future: replace with the proper
        // chat template applier (llama.cpp has built-in templates per
        // model arch — using them directly avoids the role-prefix hack
        // and matches what the model was trained on).
        let mut prompt = String::new();
        if let Some(sys) = request.system_prompt.as_ref() {
            if !sys.is_empty() {
                prompt.push_str("<|im_start|>system\n");
                prompt.push_str(sys);
                prompt.push_str("<|im_end|>\n");
            }
        }
        for msg in &request.messages {
            prompt.push_str("<|im_start|>");
            prompt.push_str(&msg.role);
            prompt.push('\n');
            match &msg.content {
                MessageContent::Text(t) => prompt.push_str(t),
                MessageContent::Parts(parts) => {
                    for p in parts {
                        if let crate::ai::types::ContentPart::Text { text } = p {
                            prompt.push_str(text);
                        }
                    }
                }
            }
            prompt.push_str("<|im_end|>\n");
        }
        prompt.push_str("<|im_start|>assistant\n");

        // No hardcoded cap. If the caller didn't specify, the model can
        // decode up to its trained context. Capping silently at 2048 was
        // the source of clipped JSON/XML output — the model would stop
        // mid-structure and downstream JSON.parse / XML parsers blew up.
        let max_tokens = request
            .max_tokens
            .map(|n| n as usize)
            .unwrap_or_else(|| backend.n_ctx_train() as usize);
        let temperature = request.temperature.unwrap_or(0.7);
        // Owned strings so the closure can move them and the post-generation
        // loop below can still strip them off the response tail.
        let stop_owned: Vec<String> = vec!["<|im_end|>".to_string(), "<|im_start|>".to_string()];

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

        // Strip stop sequences from the tail (the backend may include
        // them depending on tokenizer behavior).
        let mut clean = text;
        for s in &stop_owned {
            if let Some(idx) = clean.rfind(s.as_str()) {
                clean.truncate(idx);
            }
        }
        let clean = clean.trim_end().to_string();

        Ok(TextGenerationResponse {
            text: clean,
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
        // Loading the model is the only honest way to answer "what's its
        // context window / max output." Pay the load cost once; subsequent
        // calls use the cached backend.
        match self.ensure_loaded() {
            Ok(b) => vec![forge_qwen35_4b_model_info(&b, *self.last_throughput_tok_s.read())],
            Err(_) => vec![],
        }
    }

    fn model_metadata(&self, model_id: &str) -> Option<ModelInfo> {
        let want = model_id.to_lowercase();
        // Only answer when the backend is loaded — that's the only way to
        // know the real ceiling. If not loaded yet, return None and let
        // the caller fall back to the async get_available_models which
        // can pay the load cost.
        let backend_guard = self.backend.read();
        let backend = backend_guard.as_ref()?;
        let info = forge_qwen35_4b_model_info(backend, *self.last_throughput_tok_s.read());
        if info.id.to_lowercase() == want || want.contains("qwen3.5-4b-code-forged") {
            Some(info)
        } else {
            None
        }
    }

    fn device_type(&self) -> InferenceDevice {
        // Bundled llama.cpp is built with Metal (Mac) / CUDA (Linux) per
        // continuum's build flags. Either way: GPU-class device.
        InferenceDevice::Gpu
    }

    fn supported_model_prefixes(&self) -> Vec<&'static str> {
        // Match the forge family. Add more entries as the forge ships
        // additional models.
        vec!["continuum-ai/qwen3.5", "qwen3.5-4b-code-forged"]
    }

    fn supports_model(&self, model_name: &str) -> bool {
        let lower = model_name.to_lowercase();
        self.supported_model_prefixes()
            .iter()
            .any(|p| lower.starts_with(&p.to_lowercase()) || lower.contains(&p.to_lowercase()))
    }
}
