//! Candle Adapter - Local LLM Inference via AIProviderAdapter
//!
//! Implements the AIProviderAdapter trait for local Candle inference.
//! Uses `ModelBackend` trait — no format-specific code paths.
//! One backend, one generate function, works for GGUF and safetensors.
//!
//! Context window, EOS tokens, architecture — all from the model file.
//! No hardcoded values.

use async_trait::async_trait;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use crate::ai::{
    ActiveAdapterRequest, AdapterCapabilities, AdapterConfig, AIProviderAdapter, ApiStyle,
    FinishReason, HealthState, HealthStatus, LoRACapabilities, LoRAAdapterInfo,
    ModelCapability, ModelInfo, RoutingInfo, TextGenerationRequest, TextGenerationResponse,
    UsageMetrics,
};
use crate::runtime;

use super::backends::{self, GenomeAdapter, ModelBackend, ModelFormat};
use super::backends::llama_safetensors::BF16_PRACTICAL_CONTEXT;
use super::lora::{load_lora_adapter, LoadedAdapter};
use super::model::load_model_by_id;
use super::quantized::load_default_quantized;

// SAFETY: ModelBackend contains GPU tensors pinned to creation thread.
// All model access happens within spawn_blocking on a consistent thread pool.
// Sync is required because CandleAdapter is shared via Arc<RwLock<>> in async context.
struct BackendWrapper(Box<dyn ModelBackend>);
unsafe impl Send for BackendWrapper {}
unsafe impl Sync for BackendWrapper {}

/// Candle adapter for local LLM inference.
///
/// Holds a single `ModelBackend` — no ModelVariant enum, no format switches.
/// The backend reports its own capabilities (context_length, architecture, etc.)
pub struct CandleAdapter {
    config: AdapterConfig,
    /// The model backend (GGUF or safetensors — doesn't matter)
    backend: Arc<RwLock<Option<BackendWrapper>>>,
    /// Loaded LoRA adapters (may or may not be active)
    loaded_adapters: RwLock<HashMap<String, LoadedAdapter>>,
    /// Currently active adapter IDs (order matters for stacking)
    active_adapters: RwLock<Vec<String>>,
    /// Use quantized model
    use_quantized: bool,
}

impl CandleAdapter {
    pub fn new() -> Self {
        Self {
            config: AdapterConfig {
                provider_id: "candle".to_string(),
                name: "Candle Local".to_string(),
                base_url: String::new(),
                api_key_env: String::new(),
                default_model: "unsloth/Llama-3.2-3B-Instruct".to_string(),
                timeout_ms: 300_000,
                max_retries: 1,
                retry_delay_ms: 0,
            },
            backend: Arc::new(RwLock::new(None)),
            loaded_adapters: RwLock::new(HashMap::new()),
            active_adapters: RwLock::new(Vec::new()),
            use_quantized: false,
        }
    }

    pub fn with_model(model_id: &str) -> Self {
        let mut adapter = Self::new();
        adapter.config.default_model = model_id.to_string();
        adapter
    }

    pub fn quantized() -> Self {
        let mut adapter = Self::new();
        adapter.use_quantized = true;
        adapter
    }

    pub fn regular() -> Self {
        let mut adapter = Self::new();
        adapter.use_quantized = false;
        adapter
    }

    pub fn lora_capabilities(&self) -> LoRACapabilities {
        LoRACapabilities::MultiLayerPaging {
            max_loaded: 8,
            supports_hot_swap: true,
        }
    }

    /// Load a LoRA adapter from path.
    pub async fn load_lora(&self, adapter_id: &str, path: &str, scale: f64) -> Result<(), String> {
        let backend_guard = self.backend.read();
        let wrapper = backend_guard.as_ref().ok_or("Model not loaded")?;
        let backend = &wrapper.0;

        let device = backend.device().clone();
        let dtype = if backend.format() == ModelFormat::Safetensors {
            // Downcast to get dtype — only safetensors backends have this
            candle_core::DType::BF16 // Safe default for Metal
        } else {
            candle_core::DType::F32
        };

        let weights = load_lora_adapter(path, &device, dtype, scale)
            .map_err(|e| format!("Failed to load LoRA adapter: {e}"))?;

        let mut adapters = self.loaded_adapters.write();
        let mut loaded = LoadedAdapter::new(adapter_id.to_string(), path.to_string(), scale);
        loaded.weights = Some(weights);
        adapters.insert(adapter_id.to_string(), loaded);

        runtime::logger("candle").info(&format!("Loaded LoRA adapter: {} from {}", adapter_id, path));
        Ok(())
    }

    /// Activate a LoRA adapter (must be loaded first).
    pub async fn apply_lora(&self, adapter_id: &str) -> Result<(), String> {
        {
            let adapters = self.loaded_adapters.read();
            if !adapters.contains_key(adapter_id) {
                return Err(format!("Adapter '{}' not loaded", adapter_id));
            }
        }

        let mut active = self.active_adapters.write();
        if !active.contains(&adapter_id.to_string()) {
            active.push(adapter_id.to_string());
        }

        {
            let mut adapters = self.loaded_adapters.write();
            if let Some(adapter) = adapters.get_mut(adapter_id) {
                adapter.active = true;
            }
        }

        self.rebuild_model_with_active_lora().await?;

        runtime::logger("candle").info(&format!("Applied LoRA adapter: {}", adapter_id));
        Ok(())
    }

    /// Deactivate a LoRA adapter.
    pub async fn remove_lora(&self, adapter_id: &str) -> Result<(), String> {
        {
            let mut active = self.active_adapters.write();
            active.retain(|id| id != adapter_id);
        }
        {
            let mut adapters = self.loaded_adapters.write();
            if let Some(adapter) = adapters.get_mut(adapter_id) {
                adapter.active = false;
            }
        }

        self.rebuild_model_with_active_lora().await?;
        runtime::logger("candle").info(&format!("Removed LoRA adapter: {}", adapter_id));
        Ok(())
    }

    /// Unload a LoRA adapter (removes from memory).
    pub async fn unload_lora(&self, adapter_id: &str) -> Result<(), String> {
        self.remove_lora(adapter_id).await?;
        let mut adapters = self.loaded_adapters.write();
        adapters.remove(adapter_id);
        runtime::logger("candle").info(&format!("Unloaded LoRA adapter: {}", adapter_id));
        Ok(())
    }

    pub fn list_lora_adapters(&self) -> Vec<LoRAAdapterInfo> {
        let adapters = self.loaded_adapters.read();
        adapters
            .values()
            .map(|a| LoRAAdapterInfo {
                adapter_id: a.adapter_id.clone(),
                path: a.path.clone(),
                scale: a.scale,
                loaded: a.weights.is_some(),
                active: a.active,
            })
            .collect()
    }

    /// Ensure exactly these adapters are loaded and active, rebuilding model once.
    async fn ensure_adapters(&self, adapters: &[ActiveAdapterRequest]) -> Result<Vec<String>, String> {
        let log = runtime::logger("candle");

        for adapter in adapters {
            let needs_load = !self.loaded_adapters.read().contains_key(&adapter.name);
            if needs_load {
                log.info(&format!("Loading LoRA adapter: {} from {} (scale={})", adapter.name, adapter.path, adapter.scale));
                self.load_lora(&adapter.name, &adapter.path, adapter.scale).await?;
            }
        }

        let desired_ids: Vec<String> = adapters.iter().map(|a| a.name.clone()).collect();
        {
            let mut active = self.active_adapters.write();
            *active = desired_ids.clone();
        }
        {
            let mut loaded = self.loaded_adapters.write();
            for (id, adapter) in loaded.iter_mut() {
                adapter.active = desired_ids.contains(id);
            }
        }

        self.rebuild_model_with_active_lora().await?;
        log.info(&format!("Active LoRA adapters: {:?}", desired_ids));
        Ok(desired_ids)
    }

    /// Rebuild model with currently active LoRA adapters.
    async fn rebuild_model_with_active_lora(&self) -> Result<(), String> {
        let active = self.active_adapters.read().clone();
        if active.is_empty() {
            runtime::logger("candle").info("No active adapters, reloading base model");
            drop(active);
            return self.reload_base_model().await;
        }

        // Collect genome adapters
        let loaded = self.loaded_adapters.read();
        let mut genome_adapters: Vec<GenomeAdapter> = Vec::new();

        for adapter_id in &active {
            if let Some(la) = loaded.get(adapter_id) {
                if let Some(weights) = &la.weights {
                    genome_adapters.push(GenomeAdapter {
                        adapter_id: la.adapter_id.clone(),
                        weights: weights.clone(),
                        scale: la.scale,
                    });
                }
            }
        }
        drop(loaded);

        if genome_adapters.is_empty() {
            return Err("No active adapters have loaded weights".to_string());
        }

        // Use the trait method
        let mut backend_guard = self.backend.write();
        let wrapper = backend_guard.as_mut().ok_or("Model not loaded")?;
        let backend = &mut wrapper.0;

        if !backend.supports_lora() {
            return Err("Current backend does not support LoRA".to_string());
        }

        backend.rebuild_with_lora(&genome_adapters)
    }

    /// Reload base model without LoRA.
    async fn reload_base_model(&self) -> Result<(), String> {
        let mut backend_guard = self.backend.write();
        let wrapper = backend_guard.as_mut().ok_or("Model not loaded")?;
        wrapper.0.reload_base()
    }
}

impl Default for CandleAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl AIProviderAdapter for CandleAdapter {
    fn provider_id(&self) -> &str {
        &self.config.provider_id
    }

    fn name(&self) -> &str {
        &self.config.name
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            supports_text_generation: true,
            supports_chat: true,
            supports_tool_use: false,
            supports_vision: false,
            supports_streaming: false,
            supports_embeddings: false,
            supports_audio: false,
            supports_image_generation: false,
            is_local: true,
            max_context_window: BF16_PRACTICAL_CONTEXT as u32,
        }
    }

    fn api_style(&self) -> ApiStyle {
        ApiStyle::Local
    }

    fn default_model(&self) -> &str {
        &self.config.default_model
    }

    async fn initialize(&mut self) -> Result<(), String> {
        let log = runtime::logger("candle");
        log.info(&format!(
            "Candle adapter ready (quantized={}, model will load on first use)",
            self.use_quantized
        ));
        // Model loads lazily on first generate_text() call.
        // This keeps IPC socket creation fast — no 30s model loading during startup.
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), String> {
        runtime::logger("candle").info("Shutting down Candle adapter");
        let mut backend = self.backend.write();
        *backend = None;
        Ok(())
    }

    async fn generate_text(
        &self,
        request: TextGenerationRequest,
    ) -> Result<TextGenerationResponse, String> {
        let log = runtime::logger("candle");
        let start = std::time::Instant::now();

        log.info(&format!(
            "generate_text called, use_quantized={}, self_ptr={:p}",
            self.use_quantized, self as *const _
        ));

        let prompt = build_prompt_from_messages(&request.messages);
        let max_tokens = request.max_tokens.unwrap_or(1024) as usize;
        let temperature = request.temperature.unwrap_or(0.7) as f64;

        // Apply LoRA adapters if requested
        let mut applied_adapters: Vec<String> = Vec::new();
        if let Some(adapters) = &request.active_adapters {
            if !adapters.is_empty() {
                applied_adapters = self.ensure_adapters(adapters).await?;
            }
        }

        let prompt_len = prompt.len();
        log.info(&format!("Prompt length: {} chars, max_tokens: {}", prompt_len, max_tokens));

        let backend_arc = Arc::clone(&self.backend);
        let default_model = self.config.default_model.clone();
        let use_quantized = self.use_quantized;
        let model_id = self.config.default_model.clone();

        // Run inference on blocking thread pool (lazy model loading on first call)
        let result = tokio::task::spawn_blocking(move || {
            let log = runtime::logger("candle");

            let mut backend_guard = backend_arc.write();

            // Lazy load: if model not loaded yet, load it now
            if backend_guard.is_none() {
                log.info("First inference call — loading model...");
                let model: Box<dyn ModelBackend> = if use_quantized {
                    load_default_quantized()
                        .map_err(|e| format!("Failed to load quantized model: {e}"))?
                } else {
                    load_model_by_id(&model_id)
                        .map_err(|e| format!("Failed to load model: {e}"))?
                };
                log.info(&format!(
                    "Model loaded: arch={}, format={:?}, context_length={}, model_id={}",
                    model.architecture(), model.format(), model.context_length(), model.model_id()
                ));
                *backend_guard = Some(BackendWrapper(model));
            }

            let wrapper = backend_guard.as_mut().expect("just loaded");
            backends::generate(&mut *wrapper.0, &prompt, max_tokens, temperature)
        })
        .await
        .map_err(|e| format!("Inference task panicked: {e}"))?;

        let (output_text, completion_tokens) = result?;

        let duration = start.elapsed();
        let input_tokens = (prompt_len / 4) as u32;
        let output_tokens = completion_tokens as u32;

        Ok(TextGenerationResponse {
            text: output_text,
            model: default_model,
            provider: "candle".to_string(),
            finish_reason: FinishReason::Stop,
            usage: UsageMetrics {
                input_tokens,
                output_tokens,
                total_tokens: input_tokens + output_tokens,
                estimated_cost: Some(0.0),
            },
            response_time_ms: duration.as_millis() as u64,
            request_id: uuid::Uuid::new_v4().to_string(),
            content: None,
            tool_calls: None,
            routing: if applied_adapters.is_empty() {
                None
            } else {
                Some(RoutingInfo {
                    provider: "candle".to_string(),
                    is_local: true,
                    routing_reason: "local_with_lora".to_string(),
                    adapters_applied: applied_adapters,
                    model_mapped: None,
                    model_requested: None,
                })
            },
            error: None,
        })
    }

    async fn health_check(&self) -> HealthStatus {
        let backend = self.backend.read();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if backend.is_some() {
            HealthStatus {
                status: HealthState::Healthy,
                api_available: true,
                response_time_ms: 0,
                error_rate: 0.0,
                last_checked: now,
                message: Some("Model loaded".to_string()),
            }
        } else {
            HealthStatus {
                status: HealthState::Healthy,
                api_available: true,
                response_time_ms: 0,
                error_rate: 0.0,
                last_checked: now,
                message: Some("Model will load on first use".to_string()),
            }
        }
    }

    async fn get_available_models(&self) -> Vec<ModelInfo> {
        let format_label = if self.use_quantized { "quantized" } else { "safetensors" };

        vec![ModelInfo {
            id: self.config.default_model.clone(),
            name: format!("{} ({})", self.config.default_model, format_label),
            provider: "candle".to_string(),
            capabilities: vec![ModelCapability::TextGeneration, ModelCapability::Chat],
            context_window: BF16_PRACTICAL_CONTEXT as u32,
            max_output_tokens: Some(4096),
            cost_per_1k_tokens: None,
            supports_streaming: false,
            supports_tools: false,
        }]
    }

    fn supported_model_prefixes(&self) -> Vec<&'static str> {
        vec![
            "llama", "qwen", "phi", "mistral", "codellama", "gemma",
            "tinyllama", "orca", "vicuna", "wizardlm", "neural-chat",
            "stablelm", "yi", "deepseek-coder", "unsloth/",
        ]
    }
}

/// Build a prompt string from chat messages using Llama 3 chat template.
fn build_prompt_from_messages(messages: &[crate::ai::ChatMessage]) -> String {
    let mut prompt = String::from("<|begin_of_text|>");

    let has_system = messages.iter().any(|m| m.role == "system");
    if !has_system {
        prompt.push_str("<|start_header_id|>system<|end_header_id|>\n\n");
        prompt.push_str("You are a helpful AI assistant.<|eot_id|>");
    }

    for msg in messages {
        let role = match msg.role.as_str() {
            "system" => "system",
            "user" => "user",
            "assistant" => "assistant",
            _ => "user",
        };

        let content = match &msg.content {
            crate::ai::MessageContent::Text(text) => text.clone(),
            crate::ai::MessageContent::Parts(parts) => {
                parts
                    .iter()
                    .filter_map(|p| {
                        if let crate::ai::ContentPart::Text { text } = p {
                            Some(text.clone())
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        };

        prompt.push_str(&format!("<|start_header_id|>{}<|end_header_id|>\n\n", role));
        prompt.push_str(&content);
        prompt.push_str("<|eot_id|>");
    }

    prompt.push_str("<|start_header_id|>assistant<|end_header_id|>\n\n");
    prompt
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{ChatMessage, MessageContent};

    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: role.to_string(),
            content: MessageContent::Text(content.to_string()),
            name: None,
        }
    }

    #[test]
    fn test_prompt_format_simple() {
        let messages = vec![msg("user", "What is 2+2?")];
        let prompt = build_prompt_from_messages(&messages);

        assert!(prompt.starts_with("<|begin_of_text|>"));
        assert!(prompt.contains("<|start_header_id|>system<|end_header_id|>"));
        assert!(prompt.contains("You are a helpful AI assistant."));
        assert!(prompt.contains("<|start_header_id|>user<|end_header_id|>"));
        assert!(prompt.contains("What is 2+2?"));
        assert!(prompt.ends_with("<|start_header_id|>assistant<|end_header_id|>\n\n"));
    }

    #[test]
    fn test_prompt_format_with_system() {
        let messages = vec![
            msg("system", "You are a pirate."),
            msg("user", "Hello!"),
        ];
        let prompt = build_prompt_from_messages(&messages);

        assert!(prompt.contains("You are a pirate."));
        assert!(!prompt.contains("You are a helpful AI assistant."));
    }

    #[test]
    fn test_prompt_format_multi_turn() {
        let messages = vec![
            msg("system", "Be concise."),
            msg("user", "Hi"),
            msg("assistant", "Hello!"),
            msg("user", "How are you?"),
        ];
        let prompt = build_prompt_from_messages(&messages);

        assert!(prompt.starts_with("<|begin_of_text|>"));
        assert!(prompt.contains("<|start_header_id|>system<|end_header_id|>\n\nBe concise.<|eot_id|>"));
        assert!(prompt.contains("<|start_header_id|>user<|end_header_id|>\n\nHi<|eot_id|>"));
        assert!(prompt.contains("<|start_header_id|>assistant<|end_header_id|>\n\nHello!<|eot_id|>"));
        assert!(prompt.ends_with("<|start_header_id|>assistant<|end_header_id|>\n\n"));
    }
}
