//! AI Provider Adapter Trait - The AI abstraction interface
//!
//! All AI providers implement this trait. The AIProviderModule works with
//! this trait, never with concrete implementations directly.
//!
//! Supported backends:
//! - OpenAI (GPT models)
//! - Anthropic (Claude models)
//! - DeepSeek
//! - Together AI
//! - Groq
//! - Fireworks
//! - XAI (Grok)
//! - Google (Gemini)
//! - Local (Candle, llama.cpp)

use crate::clog_warn;
use async_trait::async_trait;

use super::types::{
    EmbeddingRequest, EmbeddingResponse, HealthStatus, ModelCapability, ModelInfo,
    TextGenerationRequest, TextGenerationResponse,
};

/// Device preference for inference — same pattern as PyTorch device='cuda'
/// or Android's MediaCodec hardware acceleration flags. Callers declare
/// what they need; the registry picks the best match from what's available.
///
/// Default: Gpu (enforced now — no silent CPU fallback).
/// Auto (try GPU, explicit CPU fallback) is reserved for future opt-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InferenceDevice {
    /// GPU-accelerated inference only. Metal (Mac) / CUDA (Nvidia) /
    /// ROCm (AMD) / Vulkan (everyone else). If no GPU adapter can serve
    /// the model → hard error, never silent CPU.
    Gpu,
    /// CPU-only inference. Candle / future CPU adapter. Currently only
    /// reachable when caller EXPLICITLY requests it (training pipelines,
    /// or env CONTINUUM_ALLOW_CPU_INFERENCE=1). Never auto-selected.
    Cpu,
    /// Try GPU first; if unavailable, fall back to CPU WITH a visible
    /// log warning. NOT IMPLEMENTED YET — reserved for when we trust
    /// the CPU path enough to ship it as a degraded-but-acceptable
    /// experience. Until then, `Auto` behaves identically to `Gpu`.
    Auto,
}

impl Default for InferenceDevice {
    fn default() -> Self {
        InferenceDevice::Gpu
    }
}

/// AI provider adapter configuration
#[derive(Debug, Clone)]
pub struct AdapterConfig {
    /// Provider identifier (e.g., "openai", "anthropic", "deepseek")
    pub provider_id: String,
    /// Human-readable name
    pub name: String,
    /// Base URL for API calls
    pub base_url: String,
    /// Environment variable name for API key
    pub api_key_env: String,
    /// Default model to use
    pub default_model: String,
    /// Request timeout in milliseconds
    pub timeout_ms: u64,
    /// Maximum retries on failure
    pub max_retries: u32,
    /// Retry delay in milliseconds
    pub retry_delay_ms: u64,
}

impl Default for AdapterConfig {
    fn default() -> Self {
        Self {
            provider_id: String::new(),
            name: String::new(),
            base_url: String::new(),
            api_key_env: String::new(),
            default_model: String::new(),
            timeout_ms: 120_000,
            max_retries: 3,
            retry_delay_ms: 1000,
        }
    }
}

/// AI provider adapter capabilities
#[derive(Debug, Clone, Default)]
pub struct AdapterCapabilities {
    pub supports_text_generation: bool,
    pub supports_chat: bool,
    pub supports_tool_use: bool,
    pub supports_vision: bool,
    pub supports_streaming: bool,
    pub supports_embeddings: bool,
    pub supports_audio: bool,
    pub supports_image_generation: bool,
    pub is_local: bool,
    pub max_context_window: u32,
}

/// LoRA capabilities reported by adapters
#[derive(Debug, Clone, Default)]
pub enum LoRACapabilities {
    /// No LoRA support (most cloud APIs)
    #[default]
    None,
    /// Single adapter at a time (cloud fine-tuning APIs like Together, Fireworks)
    SingleAdapter,
    /// Full local control with multi-adapter paging
    MultiLayerPaging {
        max_loaded: usize,
        supports_hot_swap: bool,
    },
}


/// Information about a loaded LoRA adapter
#[derive(Debug, Clone)]
pub struct LoRAAdapterInfo {
    pub adapter_id: String,
    pub path: String,
    pub scale: f64,
    pub loaded: bool,
    pub active: bool,
}

/// API style for the provider
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiStyle {
    /// OpenAI-compatible API (most providers)
    /// POST /v1/chat/completions with Bearer auth
    OpenAI,
    /// Anthropic API (different format)
    /// POST /v1/messages with x-api-key header
    Anthropic,
    /// Google Gemini API
    /// POST /v1beta/models/{model}:generateContent
    Google,
    /// Local inference (Candle, llama.cpp)
    Local,
}

/// The universal AI provider adapter trait
///
/// All AI providers implement this trait. The AIProviderModule calls
/// these methods; adapters translate to native provider API calls.
#[async_trait]
pub trait AIProviderAdapter: Send + Sync {
    /// Get adapter provider ID (e.g., "openai", "anthropic")
    fn provider_id(&self) -> &str;

    /// Get adapter human-readable name
    fn name(&self) -> &str;

    /// Get adapter capabilities
    fn capabilities(&self) -> AdapterCapabilities;

    /// Get API style
    fn api_style(&self) -> ApiStyle;

    /// Get default model for this provider
    fn default_model(&self) -> &str;

    /// Initialize the adapter (verify API key, warm up if needed)
    async fn initialize(&mut self) -> Result<(), String>;

    /// Shutdown the adapter
    async fn shutdown(&mut self) -> Result<(), String>;

    // ─── Text Generation ────────────────────────────────────────────────────

    /// Generate text (main entry point)
    /// Handles both plain text generation AND tool calling
    async fn generate_text(
        &self,
        request: TextGenerationRequest,
    ) -> Result<TextGenerationResponse, String>;

    // ─── Embeddings (optional) ──────────────────────────────────────────────

    /// Create embeddings (optional - not all providers support this)
    async fn create_embedding(
        &self,
        _request: EmbeddingRequest,
    ) -> Result<EmbeddingResponse, String> {
        Err(format!("{} does not support embeddings", self.name()))
    }

    // ─── Health & Metadata ──────────────────────────────────────────────────

    /// Check provider health
    async fn health_check(&self) -> HealthStatus;

    /// Get available models from this provider
    async fn get_available_models(&self) -> Vec<ModelInfo>;

    /// Check if this adapter supports a specific capability
    fn supports(&self, capability: ModelCapability) -> bool {
        let caps = self.capabilities();
        match capability {
            ModelCapability::TextGeneration => caps.supports_text_generation,
            ModelCapability::Chat => caps.supports_chat,
            ModelCapability::ToolUse => caps.supports_tool_use,
            ModelCapability::ImageAnalysis | ModelCapability::Multimodal => caps.supports_vision,
            ModelCapability::Embeddings => caps.supports_embeddings,
            ModelCapability::AudioGeneration | ModelCapability::AudioTranscription => {
                caps.supports_audio
            }
            ModelCapability::ImageGeneration => caps.supports_image_generation,
            _ => false,
        }
    }

    // ─── LoRA Capabilities ─────────────────────────────────────────────────────
    // These methods enable fine-tuning/adapter support across providers.
    // Cloud providers may support single adapters (Together, Fireworks).
    // Local Candle supports full multi-layer paging.

    /// Get LoRA capabilities for this adapter
    fn lora_capabilities(&self) -> LoRACapabilities {
        LoRACapabilities::None
    }

    /// Apply a LoRA adapter (for adapters that support it)
    /// Cloud providers: Sets the active fine-tuned model
    /// Local Candle: Activates the adapter (may require model rebuild)
    async fn apply_lora(&self, _adapter_id: &str) -> Result<(), String> {
        Err(format!("{} does not support LoRA", self.name()))
    }

    /// Remove/deactivate a LoRA adapter
    async fn remove_lora(&self, _adapter_id: &str) -> Result<(), String> {
        Err(format!("{} does not support LoRA", self.name()))
    }

    /// List available LoRA adapters
    fn list_lora_adapters(&self) -> Vec<LoRAAdapterInfo> {
        vec![]
    }

    // ─── Device & Capability Routing ─────────────────────────────────────────
    // Adapters declare their device class (GPU/CPU/Cloud) and what model
    // prefixes they support. AdapterRegistry::select() uses both to pick
    // the best match for the caller's request.

    /// What device class does this adapter run on?
    ///
    /// - Gpu: Metal, CUDA, ROCm, Vulkan — hardware-accelerated inference.
    ///   Docker Model Runner, llama.cpp-metal, llama-vulkan all return Gpu.
    /// - Cpu: Candle CPU inference. Only selected when explicitly requested
    ///   (training pipelines) or when CONTINUUM_ALLOW_CPU_INFERENCE is set.
    /// - Cloud: API-based providers (Anthropic, OpenAI, etc.) — not local
    ///   compute at all. Always eligible regardless of device preference
    ///   because they don't consume local resources.
    ///
    /// Default: Gpu. Override in CPU-only adapters (Candle).
    fn device_type(&self) -> InferenceDevice {
        InferenceDevice::Gpu
    }

    /// Get model name prefixes this adapter supports.
    /// Used by AdapterRegistry to auto-route requests based on model name.
    fn supported_model_prefixes(&self) -> Vec<&'static str> {
        vec![] // Default: no auto-routing by model name
    }

    /// Check if this adapter can handle a specific model by name.
    /// Default implementation checks supported_model_prefixes().
    fn supports_model(&self, model_name: &str) -> bool {
        let model_lower = model_name.to_lowercase();
        self.supported_model_prefixes()
            .iter()
            .any(|prefix| model_lower.starts_with(prefix))
    }
}

/// Registry of AI provider adapters
/// Manages adapter lifecycle and selection
pub struct AdapterRegistry {
    adapters: std::collections::HashMap<String, Box<dyn AIProviderAdapter>>,
    priority_order: Vec<String>,
}

impl AdapterRegistry {
    pub fn new() -> Self {
        Self {
            adapters: std::collections::HashMap::new(),
            priority_order: Vec::new(),
        }
    }

    /// Register an adapter with a priority (lower = higher priority)
    pub fn register(&mut self, adapter: Box<dyn AIProviderAdapter>, priority: usize) {
        let id = adapter.provider_id().to_string();

        // Insert into priority order
        if priority >= self.priority_order.len() {
            self.priority_order.push(id.clone());
        } else {
            self.priority_order.insert(priority, id.clone());
        }

        self.adapters.insert(id, adapter);
    }

    /// Get adapter by provider ID
    pub fn get(&self, provider_id: &str) -> Option<&dyn AIProviderAdapter> {
        self.adapters.get(provider_id).map(|b| b.as_ref())
    }

    /// Get mutable adapter by provider ID
    pub fn get_mut(&mut self, provider_id: &str) -> Option<&mut Box<dyn AIProviderAdapter>> {
        self.adapters.get_mut(provider_id)
    }

    /// Get available adapters (those that initialized successfully)
    pub fn available(&self) -> Vec<&str> {
        self.priority_order
            .iter()
            .filter_map(|id| self.adapters.get(id).map(|_| id.as_str()))
            .collect()
    }

    /// Select best adapter based on request.
    ///
    /// Device-aware routing (like PyTorch device='cuda' / Android MediaCodec):
    /// - `device = Gpu`: only GPU-capable adapters (DMR, llama-metal, llama-vulkan).
    ///   Hard error if no GPU adapter supports the model. DEFAULT.
    /// - `device = Cpu`: only CPU-capable adapters (Candle). Explicit opt-in for
    ///   training/LoRA. Never auto-selected for chat.
    /// - `device = Auto`: try GPU first, CPU fallback WITH warning. RESERVED —
    ///   not implemented yet, behaves as Gpu until we trust the CPU path.
    ///
    /// Explicit `preferred_provider` always wins regardless of device.
    /// Cloud providers (Anthropic, OpenAI, etc.) are always eligible — they're
    /// not local compute, so device preference doesn't apply.
    pub fn select<'a>(
        &'a self,
        preferred_provider: Option<&str>,
        model: Option<&str>,
        device: InferenceDevice,
    ) -> Option<(&'a str, &'a dyn AIProviderAdapter)> {
        // 1. Explicit provider — bypass routing for NAMED adapters.
        //    Special case: "local" means "best available local GPU adapter"
        //    — NOT a specific adapter name. Drops through to device-filtered
        //    auto-selection (tier 3) with the requested model. This is how
        //    local personas get DMR when available, Vulkan when not, and
        //    hard-error when neither can serve the model.
        if let Some(pref) = preferred_provider {
            if pref != "local" {
                for (id, adapter) in self.adapters.iter() {
                    if id == pref {
                        return Some((id.as_str(), adapter.as_ref()));
                    }
                }
                clog_warn!(
                    "Provider '{}' explicitly requested but not available. Available: {:?}",
                    pref,
                    self.available()
                );
                return None;
            }
            // "local" — fall through to device-filtered auto-selection below
        }

        // 2. Cloud-provider prefix detection (always eligible regardless of device).
        // These are the well-known cloud API providers whose model names
        // unambiguously identify the provider.
        if let Some(model_name) = model {
            let model_lower = model_name.to_lowercase();
            let cloud_match: Option<&str> = if model_lower.starts_with("claude") {
                Some("anthropic")
            } else if model_lower.starts_with("gpt") || model_lower.starts_with("o1") || model_lower.starts_with("o3") {
                Some("openai")
            } else if model_lower.starts_with("deepseek") {
                Some("deepseek")
            } else if model_lower.starts_with("grok") {
                Some("xai")
            } else if model_lower.starts_with("gemini") {
                Some("google")
            } else {
                None
            };
            if let Some(provider_id) = cloud_match {
                if let Some(adapter) = self.adapters.get(provider_id) {
                    return Some((provider_id, adapter.as_ref()));
                }
            }
        }

        // 3. Device-filtered local adapter selection.
        // Walk priority order; only consider adapters whose device_type
        // matches the request. GPU adapter that honestly supports the model
        // wins. No silent cross-device fallback.
        let device_matches = |adapter_device: InferenceDevice| -> bool {
            match device {
                InferenceDevice::Gpu => adapter_device == InferenceDevice::Gpu,
                InferenceDevice::Cpu => adapter_device == InferenceDevice::Cpu,
                InferenceDevice::Auto => true, // future: GPU-first then CPU
            }
        };

        for id in &self.priority_order {
            if let Some(adapter) = self.adapters.get(id) {
                if !device_matches(adapter.device_type()) {
                    continue; // wrong device class — skip, don't fallback
                }
                // If model specified, adapter must honestly support it.
                // If no model specified, any adapter on the right device works.
                if model.map_or(true, |m| adapter.supports_model(m)) {
                    return Some((id.as_str(), adapter.as_ref()));
                }
            }
        }

        // No adapter matched. Fail loud.
        if let Some(model_name) = model {
            clog_warn!(
                "No {:?}-device adapter supports model '{}'. Registered: {:?}. Pull model into DMR: `docker model pull {}`, or install the right GPU backend.",
                device,
                model_name,
                self.available(),
                model_name
            );
        } else {
            clog_warn!(
                "No {:?}-device adapter available. Registered: {:?}.",
                device,
                self.available()
            );
        }
        None
    }

    /// Initialize all registered adapters
    pub async fn initialize_all(&mut self) -> Result<(), String> {
        let ids: Vec<_> = self.adapters.keys().cloned().collect();
        for id in ids {
            if let Some(adapter) = self.adapters.get_mut(&id) {
                if let Err(e) = adapter.initialize().await {
                    clog_warn!("Failed to initialize {} adapter: {}", id, e);
                    // Don't fail entirely - other adapters may work
                }
            }
        }
        Ok(())
    }

    /// Shutdown all adapters
    pub async fn shutdown_all(&mut self) -> Result<(), String> {
        for (id, adapter) in self.adapters.iter_mut() {
            if let Err(e) = adapter.shutdown().await {
                clog_warn!("Failed to shutdown {} adapter: {}", id, e);
            }
        }
        Ok(())
    }
}

impl Default for AdapterRegistry {
    fn default() -> Self {
        Self::new()
    }
}
