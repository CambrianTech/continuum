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

use async_trait::async_trait;

use super::types::{
    EmbeddingRequest, EmbeddingResponse, HealthStatus, ModelCapability, ModelInfo,
    TextGenerationRequest, TextGenerationResponse,
};

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
#[derive(Debug, Clone)]
pub enum LoRACapabilities {
    /// No LoRA support (most cloud APIs)
    None,
    /// Single adapter at a time (cloud fine-tuning APIs like Together, Fireworks)
    SingleAdapter,
    /// Full local control with multi-adapter paging
    MultiLayerPaging {
        max_loaded: usize,
        supports_hot_swap: bool,
    },
}

impl Default for LoRACapabilities {
    fn default() -> Self {
        LoRACapabilities::None
    }
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

    // ─── Model Routing ────────────────────────────────────────────────────────
    // Adapters define what model prefixes they support for automatic routing.
    // This replaces hardcoded string matching in routing logic.

    /// Get model name prefixes this adapter supports.
    /// Used by AdapterRegistry to auto-route requests based on model name.
    /// Example: Anthropic returns ["claude"], OpenAI returns ["gpt"],
    /// Candle returns ["llama", "qwen", "phi", "mistral", ...].
    fn supported_model_prefixes(&self) -> Vec<&'static str> {
        vec![]  // Default: no auto-routing by model name
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

    /// Select best adapter based on request
    /// Returns (provider_id, adapter)
    pub fn select<'a>(
        &'a self,
        preferred_provider: Option<&str>,
        model: Option<&str>,
    ) -> Option<(&'a str, &'a dyn AIProviderAdapter)> {
        // 1. If preferred provider specified, use it
        if let Some(pref) = preferred_provider {
            // Find the static key that matches
            for (id, adapter) in self.adapters.iter() {
                if id == pref {
                    return Some((id.as_str(), adapter.as_ref()));
                }
            }
        }

        // 2. Detect provider from model name
        if let Some(model_name) = model {
            let model_lower = model_name.to_lowercase();

            // Claude -> Anthropic
            if model_lower.starts_with("claude") {
                if let Some(adapter) = self.adapters.get("anthropic") {
                    return Some(("anthropic", adapter.as_ref()));
                }
            }

            // GPT -> OpenAI
            if model_lower.starts_with("gpt") {
                if let Some(adapter) = self.adapters.get("openai") {
                    return Some(("openai", adapter.as_ref()));
                }
            }

            // DeepSeek models
            if model_lower.starts_with("deepseek") {
                if let Some(adapter) = self.adapters.get("deepseek") {
                    return Some(("deepseek", adapter.as_ref()));
                }
            }

            // Grok -> XAI
            if model_lower.starts_with("grok") {
                if let Some(adapter) = self.adapters.get("xai") {
                    return Some(("xai", adapter.as_ref()));
                }
            }

            // Gemini -> Google
            if model_lower.starts_with("gemini") {
                if let Some(adapter) = self.adapters.get("google") {
                    return Some(("google", adapter.as_ref()));
                }
            }

            // 2.5. Check if any adapter explicitly supports this model
            // Adapters define their supported prefixes via supported_model_prefixes()
            // This is the authoritative routing - adapter knows what it supports
            for id in &self.priority_order {
                if let Some(adapter) = self.adapters.get(id) {
                    if adapter.supports_model(model_name) {
                        return Some((id.as_str(), adapter.as_ref()));
                    }
                }
            }
        }

        // 3. Return highest priority available adapter
        for id in &self.priority_order {
            if let Some(adapter) = self.adapters.get(id) {
                return Some((id.as_str(), adapter.as_ref()));
            }
        }

        None
    }

    /// Initialize all registered adapters
    pub async fn initialize_all(&mut self) -> Result<(), String> {
        let ids: Vec<_> = self.adapters.keys().cloned().collect();
        for id in ids {
            if let Some(adapter) = self.adapters.get_mut(&id) {
                if let Err(e) = adapter.initialize().await {
                    eprintln!("⚠️ Failed to initialize {} adapter: {}", id, e);
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
                eprintln!("⚠️ Failed to shutdown {} adapter: {}", id, e);
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
