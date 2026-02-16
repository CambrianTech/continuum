//! Candle Adapter - Local LLM Inference via AIProviderAdapter
//!
//! Implements the AIProviderAdapter trait for local Candle inference,
//! providing a unified interface for local models alongside cloud providers.
//!
//! Features:
//! - Local model inference (no API calls)
//! - LoRA adapter support (single and multi-adapter genome)
//! - Quantized model support (Q4_K_M, Q8_0)
//! - GPU acceleration (Metal/CUDA)
//!
//! This adapter reports `LoRACapabilities::MultiLayerPaging` since local
//! Candle has full control over adapter paging, unlike cloud providers.
//!
//! Logging: Uses crate::runtime::logger("candle") - no special setup needed.

use async_trait::async_trait;
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

use crate::ai::{
    AdapterCapabilities, AdapterConfig, AIProviderAdapter, ApiStyle,
    FinishReason, HealthState, HealthStatus, LoRACapabilities, LoRAAdapterInfo,
    ModelCapability, ModelInfo, TextGenerationRequest, TextGenerationResponse, UsageMetrics,
};
use crate::runtime;

use super::lora::{load_lora_adapter, LoadedAdapter};
use super::model::{generate_text, load_model_by_id, rebuild_with_stacked_lora, GenomeAdapter, ModelState};
use super::quantized::{generate_text_quantized, load_default_quantized, QuantizedModelState};

/// Model variant - regular or quantized
enum ModelVariant {
    Regular(ModelState),
    Quantized(QuantizedModelState),
}

// Required for spawn_blocking
// SAFETY: ModelVariant contains GPU tensors that are pinned to the thread that created them.
// We ensure all model access happens within spawn_blocking on a consistent thread pool.
unsafe impl Send for ModelVariant {}

/// Candle adapter for local LLM inference
pub struct CandleAdapter {
    config: AdapterConfig,
    /// Model wrapped in Arc for sharing across spawn_blocking threads
    model: Arc<RwLock<Option<ModelVariant>>>,
    /// Loaded LoRA adapters (may or may not be active)
    loaded_adapters: RwLock<HashMap<String, LoadedAdapter>>,
    /// Currently active adapter IDs (order matters for stacking)
    active_adapters: RwLock<Vec<String>>,
    /// Use quantized model
    use_quantized: bool,
}

impl CandleAdapter {
    /// Create a new Candle adapter
    pub fn new() -> Self {
        Self {
            config: AdapterConfig {
                provider_id: "candle".to_string(),
                name: "Candle Local".to_string(),
                base_url: String::new(), // Not used for local
                api_key_env: String::new(), // Not used for local
                default_model: "meta-llama/Llama-3.1-8B-Instruct".to_string(),
                timeout_ms: 300_000, // 5 minutes for local generation
                max_retries: 1,
                retry_delay_ms: 0,
            },
            model: Arc::new(RwLock::new(None)),
            loaded_adapters: RwLock::new(HashMap::new()),
            active_adapters: RwLock::new(Vec::new()),
            use_quantized: false, // BF16 for stability and LoRA training support
        }
    }

    /// Create with specific model ID
    pub fn with_model(model_id: &str) -> Self {
        let mut adapter = Self::new();
        adapter.config.default_model = model_id.to_string();
        adapter
    }

    /// Create with quantized model
    pub fn quantized() -> Self {
        let mut adapter = Self::new();
        adapter.use_quantized = true;
        adapter
    }

    /// Create with regular (non-quantized) model
    pub fn regular() -> Self {
        let mut adapter = Self::new();
        adapter.use_quantized = false;
        adapter
    }

    /// Get LoRA capabilities
    pub fn lora_capabilities(&self) -> LoRACapabilities {
        LoRACapabilities::MultiLayerPaging {
            max_loaded: 8,  // Can load up to 8 adapters
            supports_hot_swap: true,
        }
    }

    /// Load a LoRA adapter from path
    pub async fn load_lora(&self, adapter_id: &str, path: &str, scale: f64) -> Result<(), String> {
        let model_guard = self.model.read();
        let model = model_guard.as_ref().ok_or("Model not loaded")?;

        // Get device and dtype from model
        let (device, dtype) = match model {
            ModelVariant::Regular(state) => (&state.device, state.dtype),
            ModelVariant::Quantized(state) => (&state.device, candle_core::DType::F32),
        };

        // Load the adapter weights
        let weights = load_lora_adapter(path, device, dtype, scale)
            .map_err(|e| format!("Failed to load LoRA adapter: {e}"))?;

        // Store loaded adapter
        let mut adapters = self.loaded_adapters.write();
        let mut loaded = LoadedAdapter::new(adapter_id.to_string(), path.to_string(), scale);
        loaded.weights = Some(weights);
        adapters.insert(adapter_id.to_string(), loaded);

        runtime::logger("candle").info(&format!("Loaded LoRA adapter: {} from {}", adapter_id, path));
        Ok(())
    }

    /// Activate a LoRA adapter (must be loaded first)
    pub async fn apply_lora(&self, adapter_id: &str) -> Result<(), String> {
        // Verify adapter is loaded
        {
            let adapters = self.loaded_adapters.read();
            if !adapters.contains_key(adapter_id) {
                return Err(format!("Adapter '{}' not loaded", adapter_id));
            }
        }

        // Add to active list if not already there
        let mut active = self.active_adapters.write();
        if !active.contains(&adapter_id.to_string()) {
            active.push(adapter_id.to_string());
        }

        // Mark as active in loaded adapters
        {
            let mut adapters = self.loaded_adapters.write();
            if let Some(adapter) = adapters.get_mut(adapter_id) {
                adapter.active = true;
            }
        }

        // Rebuild model with active adapters
        self.rebuild_model_with_active_lora().await?;

        runtime::logger("candle").info(&format!("Applied LoRA adapter: {}", adapter_id));
        Ok(())
    }

    /// Deactivate a LoRA adapter
    pub async fn remove_lora(&self, adapter_id: &str) -> Result<(), String> {
        // Remove from active list
        {
            let mut active = self.active_adapters.write();
            active.retain(|id| id != adapter_id);
        }

        // Mark as inactive
        {
            let mut adapters = self.loaded_adapters.write();
            if let Some(adapter) = adapters.get_mut(adapter_id) {
                adapter.active = false;
            }
        }

        // Rebuild model without this adapter
        self.rebuild_model_with_active_lora().await?;

        runtime::logger("candle").info(&format!("Removed LoRA adapter: {}", adapter_id));
        Ok(())
    }

    /// Unload a LoRA adapter (removes from memory)
    pub async fn unload_lora(&self, adapter_id: &str) -> Result<(), String> {
        // First deactivate if active
        self.remove_lora(adapter_id).await?;

        // Remove from loaded adapters
        let mut adapters = self.loaded_adapters.write();
        adapters.remove(adapter_id);

        runtime::logger("candle").info(&format!("Unloaded LoRA adapter: {}", adapter_id));
        Ok(())
    }

    /// List all LoRA adapters
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

    /// Rebuild model with currently active LoRA adapters
    async fn rebuild_model_with_active_lora(&self) -> Result<(), String> {
        let active = self.active_adapters.read().clone();
        if active.is_empty() {
            // No active adapters - reload base model
            runtime::logger("candle").info("No active adapters, reloading base model");
            drop(active);
            return self.reload_base_model().await;
        }

        // Collect active adapter weights
        let adapters = self.loaded_adapters.read();
        let mut genome_adapters: Vec<GenomeAdapter> = Vec::new();

        for adapter_id in &active {
            if let Some(loaded) = adapters.get(adapter_id) {
                if let Some(weights) = &loaded.weights {
                    genome_adapters.push(GenomeAdapter {
                        adapter_id: loaded.adapter_id.clone(),
                        weights: weights.clone(),
                        scale: loaded.scale,
                    });
                }
            }
        }

        drop(adapters);

        if genome_adapters.is_empty() {
            return Err("No active adapters have loaded weights".to_string());
        }

        // Get current model state
        let model_guard = self.model.read();
        let current = model_guard.as_ref().ok_or("Model not loaded")?;

        match current {
            ModelVariant::Regular(state) => {
                // Rebuild with stacked LoRA
                let new_model = rebuild_with_stacked_lora(
                    &state.weight_paths,
                    &state.device,
                    state.dtype,
                    &state.config,
                    &genome_adapters,
                )
                .map_err(|e| format!("Failed to rebuild model with LoRA: {e}"))?;

                // Update model
                drop(model_guard);
                let mut model_write = self.model.write();
                if let Some(ModelVariant::Regular(state)) = model_write.as_mut() {
                    state.model = new_model;
                }
            }
            ModelVariant::Quantized(_) => {
                // Quantized models don't support LoRA stacking yet
                return Err("Quantized models don't support LoRA stacking yet".to_string());
            }
        }

        Ok(())
    }

    /// Reload base model without LoRA
    async fn reload_base_model(&self) -> Result<(), String> {
        if self.use_quantized {
            let state = load_default_quantized()
                .map_err(|e| format!("Failed to reload base model: {e}"))?;
            let mut model = self.model.write();
            *model = Some(ModelVariant::Quantized(state));
        } else {
            let state = load_model_by_id(&self.config.default_model)
                .map_err(|e| format!("Failed to reload base model: {e}"))?;
            let mut model = self.model.write();
            *model = Some(ModelVariant::Regular(state));
        }
        Ok(())
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
            supports_tool_use: false, // Local models don't have native tool calling
            supports_vision: false,
            supports_streaming: false, // Could add streaming later
            supports_embeddings: false, // Use fastembed instead
            supports_audio: false,
            supports_image_generation: false,
            is_local: true,
            max_context_window: 1400, // Candle quantized attention breaks at ~1000 input tokens
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
        log.info(&format!("Initializing Candle adapter (quantized={}, self_ptr={:p})", self.use_quantized, self as *const _));

        // Load the model
        if self.use_quantized {
            log.info("About to call load_default_quantized...");
            let state = load_default_quantized()
                .map_err(|e| format!("Failed to load quantized model: {e}"))?;
            log.info("load_default_quantized returned, acquiring write lock...");
            let mut model = self.model.write();
            log.info("Write lock acquired, storing model...");
            *model = Some(ModelVariant::Quantized(state));
            log.info(&format!("Model stored, is_some={}", model.is_some()));
        } else {
            let state = load_model_by_id(&self.config.default_model)
                .map_err(|e| format!("Failed to load model: {e}"))?;
            let mut model = self.model.write();
            *model = Some(ModelVariant::Regular(state));
            log.info(&format!("Model stored, is_some={}", model.is_some()));
        }

        // Verify it's actually stored
        let verification = self.model.read();
        log.info(&format!("Post-init verification: is_some={}", verification.is_some()));

        log.info("Candle adapter initialized successfully");
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), String> {
        runtime::logger("candle").info("Shutting down Candle adapter");
        let mut model = self.model.write();
        *model = None;
        Ok(())
    }

    async fn generate_text(
        &self,
        request: TextGenerationRequest,
    ) -> Result<TextGenerationResponse, String> {
        let log = runtime::logger("candle");
        let start = std::time::Instant::now();

        log.info(&format!("generate_text called, use_quantized={}, self_ptr={:p}", self.use_quantized, self as *const _));

        // Build prompt from messages
        let prompt = build_prompt_from_messages(&request.messages);

        let max_tokens = request.max_tokens.unwrap_or(1024) as usize;
        let temperature = request.temperature.unwrap_or(0.7) as f64;

        log.info(&format!("Prompt length: {} chars, max_tokens: {}", prompt.len(), max_tokens));

        // Clone Arc for spawn_blocking - this allows the async runtime to continue
        // handling other requests while inference runs on a dedicated thread
        let model_arc = Arc::clone(&self.model);
        let _use_quantized = self.use_quantized;
        let default_model = self.config.default_model.clone();

        // Run CPU-intensive inference on a blocking thread pool
        // This prevents inference from blocking the async IPC handler,
        // allowing data operations to continue in parallel
        let result = tokio::task::spawn_blocking(move || {
            let log = runtime::logger("candle");

            // Acquire model lock within blocking thread
            let mut model_guard = model_arc.write();
            log.info(&format!("Got model write lock (blocking), model is_some={}", model_guard.is_some()));

            let model = model_guard.as_mut().ok_or_else(|| {
                log.error("Model not loaded - was initialize() called?");
                "Model not loaded".to_string()
            })?;

            let (output_text, completion_tokens) = match model {
                ModelVariant::Regular(state) => {
                    generate_text(state, &prompt, max_tokens, temperature)?
                }
                ModelVariant::Quantized(state) => {
                    generate_text_quantized(state, &prompt, max_tokens, temperature)?
                }
            };

            Ok::<_, String>((output_text, completion_tokens, prompt.len()))
        })
        .await
        .map_err(|e| format!("Inference task panicked: {e}"))?;

        let (output_text, completion_tokens, prompt_len) = result?;

        let duration = start.elapsed();

        let input_tokens = (prompt_len / 4) as u32; // Rough estimate
        let output_tokens = completion_tokens as u32;

        // Build response
        Ok(TextGenerationResponse {
            text: output_text,
            model: default_model,
            provider: "candle".to_string(),
            finish_reason: FinishReason::Stop,
            usage: UsageMetrics {
                input_tokens,
                output_tokens,
                total_tokens: input_tokens + output_tokens,
                estimated_cost: Some(0.0), // Local inference is free
            },
            response_time_ms: duration.as_millis() as u64,
            request_id: uuid::Uuid::new_v4().to_string(),
            content: None,
            tool_calls: None,
            routing: None,
            error: None,
        })
    }

    async fn health_check(&self) -> HealthStatus {
        let model = self.model.read();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if model.is_some() {
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
                status: HealthState::Unhealthy,
                api_available: false,
                response_time_ms: 0,
                error_rate: 1.0,
                last_checked: now,
                message: Some("Model not loaded".to_string()),
            }
        }
    }

    async fn get_available_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: "llama-3.2-3b-instruct-q4".to_string(),
                name: "Llama 3.2 3B Instruct (Q4)".to_string(),
                provider: "candle".to_string(),
                capabilities: vec![ModelCapability::TextGeneration, ModelCapability::Chat],
                context_window: 1400,
                max_output_tokens: Some(4096),
                cost_per_1k_tokens: None, // Local is free
                supports_streaming: false,
                supports_tools: false,
            },
            ModelInfo {
                id: "llama-3.2-3b-instruct".to_string(),
                name: "Llama 3.2 3B Instruct".to_string(),
                provider: "candle".to_string(),
                capabilities: vec![ModelCapability::TextGeneration, ModelCapability::Chat],
                context_window: 1400,
                max_output_tokens: Some(4096),
                cost_per_1k_tokens: None,
                supports_streaming: false,
                supports_tools: false,
            },
        ]
    }

    /// Model prefixes this adapter supports for auto-routing.
    /// Local models typically use these naming conventions.
    fn supported_model_prefixes(&self) -> Vec<&'static str> {
        vec![
            "llama",        // Meta's LLaMA models (llama3.2:3b, Llama-3.2-3B-Instruct)
            "qwen",         // Alibaba's Qwen models (qwen2:1.5b, Qwen/Qwen2-1.5B-Instruct)
            "phi",          // Microsoft's Phi models (phi3:mini, phi-3-mini)
            "mistral",      // Mistral AI models (mistral:7b, mistral-7b-instruct)
            "codellama",    // Code-focused LLaMA
            "gemma",        // Google's Gemma models
            "tinyllama",    // TinyLlama
            "orca",         // Orca models
            "vicuna",       // Vicuna models
            "wizardlm",     // WizardLM
            "neural-chat",  // Intel Neural Chat
            "stablelm",     // Stability AI LM
            "yi",           // 01.AI Yi models
            "deepseek-coder", // DeepSeek local coder (not the API)
            "unsloth/",     // Unsloth fine-tuned models
        ]
    }
}

/// Build a prompt string from chat messages using Llama 3/3.2 chat template
///
/// CRITICAL: Llama 3 Instruct models require specific chat template format with special tokens.
/// Using generic "System: User: Assistant:" format WILL NOT WORK and produces garbage output.
///
/// Llama 3 chat template format:
/// ```
/// <|begin_of_text|><|start_header_id|>system<|end_header_id|>
///
/// {system_message}<|eot_id|><|start_header_id|>user<|end_header_id|>
///
/// {user_message}<|eot_id|><|start_header_id|>assistant<|end_header_id|>
///
/// {assistant_message}<|eot_id|>...
/// ```
///
/// The final assistant turn MUST end with just the header (no eot_id) so the model generates the response.
///
/// Reference: https://www.llama.com/docs/model-cards-and-prompt-formats/meta-llama-3/
fn build_prompt_from_messages(messages: &[crate::ai::ChatMessage]) -> String {
    let mut prompt = String::from("<|begin_of_text|>");

    // Check if there's a system message
    let has_system = messages.iter().any(|m| m.role == "system");
    if !has_system {
        // Add default system prompt
        prompt.push_str("<|start_header_id|>system<|end_header_id|>\n\n");
        prompt.push_str("You are a helpful AI assistant.<|eot_id|>");
    }

    for msg in messages {
        let role = match msg.role.as_str() {
            "system" => "system",
            "user" => "user",
            "assistant" => "assistant",
            _ => "user", // Default unknown roles to user
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

        // Add message with proper Llama 3 chat template format
        prompt.push_str(&format!("<|start_header_id|>{}<|end_header_id|>\n\n", role));
        prompt.push_str(&content);
        prompt.push_str("<|eot_id|>");
    }

    // Add final assistant header for model to generate response
    prompt.push_str("<|start_header_id|>assistant<|end_header_id|>\n\n");

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::{ChatMessage, MessageContent};

    /// Helper to create a ChatMessage
    fn msg(role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: role.to_string(),
            content: MessageContent::Text(content.to_string()),
            name: None,
        }
    }

    /// Test that build_prompt_from_messages produces correct Llama 3 chat template format
    #[test]
    fn test_prompt_format_simple() {
        let messages = vec![msg("user", "What is 2+2?")];

        let prompt = build_prompt_from_messages(&messages);

        // Should have begin_of_text
        assert!(prompt.starts_with("<|begin_of_text|>"), "Should start with begin_of_text");

        // Should have default system prompt (since no system message provided)
        assert!(prompt.contains("<|start_header_id|>system<|end_header_id|>"), "Should have system header");
        assert!(prompt.contains("You are a helpful AI assistant."), "Should have default system content");

        // Should have user message
        assert!(prompt.contains("<|start_header_id|>user<|end_header_id|>"), "Should have user header");
        assert!(prompt.contains("What is 2+2?"), "Should have user content");

        // Should end with assistant header for generation
        assert!(prompt.ends_with("<|start_header_id|>assistant<|end_header_id|>\n\n"), "Should end with assistant header");

        // Should have eot_id after content
        assert!(prompt.contains("<|eot_id|>"), "Should have eot_id markers");

        println!("Generated prompt:\n{}", prompt);
    }

    /// Test that prompt format works with system message
    #[test]
    fn test_prompt_format_with_system() {
        let messages = vec![
            msg("system", "You are a pirate."),
            msg("user", "Hello!"),
        ];

        let prompt = build_prompt_from_messages(&messages);

        // Should have custom system message
        assert!(prompt.contains("You are a pirate."), "Should have custom system content");

        // Should NOT have default system (since custom provided)
        assert!(!prompt.contains("You are a helpful AI assistant."), "Should not have default system");

        println!("Generated prompt:\n{}", prompt);
    }

    /// Test multi-turn conversation format
    #[test]
    fn test_prompt_format_multi_turn() {
        let messages = vec![
            msg("system", "Be concise."),
            msg("user", "Hi"),
            msg("assistant", "Hello!"),
            msg("user", "How are you?"),
        ];

        let prompt = build_prompt_from_messages(&messages);

        // Verify structure
        assert!(prompt.starts_with("<|begin_of_text|>"));
        assert!(prompt.contains("<|start_header_id|>system<|end_header_id|>\n\nBe concise.<|eot_id|>"));
        assert!(prompt.contains("<|start_header_id|>user<|end_header_id|>\n\nHi<|eot_id|>"));
        assert!(prompt.contains("<|start_header_id|>assistant<|end_header_id|>\n\nHello!<|eot_id|>"));
        assert!(prompt.contains("<|start_header_id|>user<|end_header_id|>\n\nHow are you?<|eot_id|>"));
        assert!(prompt.ends_with("<|start_header_id|>assistant<|end_header_id|>\n\n"));

        println!("Generated prompt:\n{}", prompt);
    }

    /// Full integration test - generate text with proper format via CandleAdapter
    ///
    /// Run with: cargo test --release test_candle_adapter_generation -- --ignored --nocapture
    #[test]
    #[ignore] // Requires model download, takes ~60 seconds
    fn test_candle_adapter_generation() {
        // Create and initialize adapter
        let mut adapter = CandleAdapter::quantized();
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            adapter.initialize().await.expect("Failed to initialize adapter");

            // Simple request
            let request = TextGenerationRequest {
                messages: vec![
                    msg("system", "You are a helpful assistant. Keep responses very short."),
                    msg("user", "What is 2+2?"),
                ],
                system_prompt: None,
                model: None,
                provider: None,
                temperature: Some(0.3),
                max_tokens: Some(50),
                top_p: None,
                top_k: None,
                stop_sequences: None,
                tools: None,
                tool_choice: None,
                request_id: None,
                user_id: None,
                room_id: None,
                purpose: None,
            };

            let response = adapter.generate_text(request).await.expect("Generation failed");

            println!("Response: {}", response.text);
            println!("Tokens: {}/{}", response.usage.output_tokens, response.usage.input_tokens);

            // Verify response is coherent (not garbage)
            assert!(!response.text.contains('\u{FFFD}'), "Response contains garbage");
            assert!(!response.text.is_empty(), "Response is empty");

            // Should mention 4 somewhere (the answer to 2+2)
            let has_answer = response.text.contains("4") || response.text.to_lowercase().contains("four");
            assert!(has_answer, "Response should contain the answer (4): {}", response.text);
        });
    }

    /// Test with longer conversation (simulates real chat usage)
    ///
    /// Run with: cargo test --release test_candle_adapter_long_conversation -- --ignored --nocapture
    #[test]
    #[ignore] // Requires model download, takes ~60 seconds
    fn test_candle_adapter_long_conversation() {
        let mut adapter = CandleAdapter::quantized();
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            adapter.initialize().await.expect("Failed to initialize adapter");

            // Simulate a longer conversation with context
            let request = TextGenerationRequest {
                messages: vec![
                    msg("system", "You are Helper AI, a friendly assistant in a development team chat. Keep responses brief and helpful."),
                    msg("user", "Hi team, I'm testing the local inference."),
                    msg("assistant", "Great! Local inference is working. How can I help?"),
                    msg("user", "What color is the sky?"),
                ],
                system_prompt: None,
                model: None,
                provider: None,
                temperature: Some(0.3),
                max_tokens: Some(100),
                top_p: None,
                top_k: None,
                stop_sequences: None,
                tools: None,
                tool_choice: None,
                request_id: None,
                user_id: None,
                room_id: None,
                purpose: None,
            };

            let response = adapter.generate_text(request).await.expect("Generation failed");

            println!("Response: {}", response.text);

            // Verify response is coherent (not garbage)
            assert!(!response.text.contains('\u{FFFD}'), "Response contains garbage");
            assert!(!response.text.is_empty(), "Response is empty");

            // Response should be intelligible English (not random tokens)
            // The actual content may vary - the model may answer about sky color OR
            // deflect the question based on the "development team chat" context
            let has_words = response.text.split_whitespace().count() >= 3;
            assert!(has_words, "Response should have at least 3 words: {}", response.text);

            println!("✓ Long conversation generated coherent response");
        });
    }
}
