//! AIProviderModule — Adapter-based AI provider system
//!
//! Uses the adapter pattern (like ORM) for pluggable AI providers.
//! Single entry point for all text generation with tool calling support.
//!
//! Supported providers (via adapters):
//! - DeepSeek (deepseek-chat, deepseek-reasoner)
//! - Anthropic (claude-sonnet-4-5, claude-opus-4, claude-3-5-haiku)
//! - OpenAI (gpt-4, gpt-4o)
//! - Together AI (llama-3.1-70b)
//! - Groq (llama-3.1-8b-instant)
//! - Fireworks (deepseek-v3)
//! - XAI (grok-3)
//! - Google (gemini-2.0-flash)
//!
//! Commands:
//! - ai/generate: Generate text with optional tool calling
//! - ai/providers/list: List available providers
//! - ai/providers/health: Check provider health

use crate::ai::{
    AdapterRegistry, AnthropicAdapter, CandleAdapter, OpenAICompatibleAdapter,
    TextGenerationRequest, TextGenerationResponse, RoutingInfo, ChatMessage, MessageContent,
};
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule, ModuleLogger};
use crate::logging::TimingGuard;
use crate::utils::params::Params;
use crate::secrets::get_secret;
use async_trait::async_trait;
use once_cell::sync::Lazy;
use serde_json::{json, Value};
use std::any::Any;
use std::sync::Arc;
use tokio::sync::{RwLock, OnceCell};

/// Global singleton registry - survives module recreation on server restart
static GLOBAL_REGISTRY: Lazy<Arc<RwLock<AdapterRegistry>>> = Lazy::new(|| {
    Arc::new(RwLock::new(AdapterRegistry::new()))
});

/// Track if we've done first-time initialization
static INITIALIZED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// AIProviderModule - ServiceModule implementation for AI inference
pub struct AIProviderModule {
    registry: Arc<RwLock<AdapterRegistry>>,
    log: OnceCell<Arc<ModuleLogger>>,
}

impl AIProviderModule {
    pub fn new() -> Self {
        Self {
            registry: GLOBAL_REGISTRY.clone(),  // Use global singleton
            log: OnceCell::new(),
        }
    }

    /// Get logger (panics if called before initialize)
    fn log(&self) -> &ModuleLogger {
        self.log.get().expect("AIProviderModule not initialized").as_ref()
    }

    /// Register all available adapters
    async fn register_adapters(&self) -> Result<(), String> {
        // Check global flag to prevent re-initialization (survives module recreation)
        if INITIALIZED.swap(true, std::sync::atomic::Ordering::SeqCst) {
            self.log().info("Adapters already initialized (global), skipping re-registration");
            return Ok(());
        }

        let mut registry = self.registry.write().await;

        // Priority order (lower = higher priority):
        // 0: DeepSeek (best price/performance)
        // 1: Anthropic (best reasoning)
        // 2: OpenAI
        // 3: Groq (fast)
        // 4: Together
        // 5: Fireworks
        // 6: XAI
        // 7: Google

        // Only register adapters that have API keys configured
        if get_secret("DEEPSEEK_API_KEY").is_some() {
            self.log().info("Registering DeepSeek adapter");
            registry.register(Box::new(OpenAICompatibleAdapter::deepseek()), 0);
        }

        if get_secret("ANTHROPIC_API_KEY").is_some() {
            self.log().info("Registering Anthropic adapter");
            registry.register(Box::new(AnthropicAdapter::new()), 1);
        }

        if get_secret("OPENAI_API_KEY").is_some() {
            self.log().info("Registering OpenAI adapter");
            registry.register(Box::new(OpenAICompatibleAdapter::openai()), 2);
        }

        if get_secret("GROQ_API_KEY").is_some() {
            self.log().info("Registering Groq adapter");
            registry.register(Box::new(OpenAICompatibleAdapter::groq()), 3);
        }

        if get_secret("TOGETHER_API_KEY").is_some() {
            self.log().info("Registering Together adapter");
            registry.register(Box::new(OpenAICompatibleAdapter::together()), 4);
        }

        if get_secret("FIREWORKS_API_KEY").is_some() {
            self.log().info("Registering Fireworks adapter");
            registry.register(Box::new(OpenAICompatibleAdapter::fireworks()), 5);
        }

        if get_secret("XAI_API_KEY").is_some() {
            self.log().info("Registering XAI adapter");
            registry.register(Box::new(OpenAICompatibleAdapter::xai()), 6);
        }

        if get_secret("GOOGLE_API_KEY").is_some() {
            self.log().info("Registering Google adapter");
            registry.register(Box::new(OpenAICompatibleAdapter::google()), 7);
        }

        // Check if Candle local inference is enabled (INFERENCE_MODE=local, candle, or hybrid)
        let inference_mode = get_secret("INFERENCE_MODE").unwrap_or_default();
        let enable_candle = inference_mode.eq_ignore_ascii_case("local")
            || inference_mode.eq_ignore_ascii_case("candle")
            || inference_mode.eq_ignore_ascii_case("hybrid");

        if enable_candle {
            self.log().info("Registering Candle adapter (local inference)");
            // Priority 8: Local inference is fallback when cloud fails or for LoRA
            // If INFERENCE_MODE=local or candle, make it priority 0 (highest)
            let priority = if inference_mode.eq_ignore_ascii_case("local") || inference_mode.eq_ignore_ascii_case("candle") { 0 } else { 8 };
            registry.register(Box::new(CandleAdapter::new()), priority);
        }

        // Initialize all registered adapters
        registry.initialize_all().await?;

        let available = registry.available();
        self.log().info(&format!("AIProviderModule initialized with {} providers: {:?}",
            available.len(), available));

        if available.is_empty() {
            self.log().warn("No providers available! Add API keys to ~/.continuum/config.env");
        }

        Ok(())
    }

    /// Parse TextGenerationRequest from JSON params
    fn parse_request(&self, params: &Value) -> Result<TextGenerationRequest, String> {
        let p = Params::new(params);

        // Parse messages (array) or simple prompt (string)
        let messages: Vec<ChatMessage> = if let Some(msgs) = p.value("messages") {
            serde_json::from_value(msgs.clone())
                .map_err(|e| format!("Failed to parse messages: {}", e))?
        } else if let Some(prompt) = p.str_opt("prompt") {
            vec![ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text(prompt.to_string()),
                name: None,
            }]
        } else {
            return Err("Missing messages or prompt".to_string());
        };

        if messages.is_empty() {
            return Err("Messages cannot be empty".to_string());
        }

        Ok(TextGenerationRequest {
            messages,
            system_prompt: p.string_opt_alias("system_prompt", "systemPrompt"),
            model: p.str_opt("model").map(String::from),
            provider: p.str_opt("provider").map(String::from),
            temperature: p.f32_opt("temperature"),
            max_tokens: p.u64_opt_alias("max_tokens", "maxTokens").map(|t| t as u32),
            top_p: p.f64_opt_alias("top_p", "topP").map(|t| t as f32),
            top_k: p.u64_opt_alias("top_k", "topK").map(|t| t as u32),
            stop_sequences: p.json_opt("stop_sequences")
                .or_else(|| p.json_opt("stopSequences")),
            tools: p.json_opt("tools"),
            tool_choice: p.json_opt("tool_choice"),
            active_adapters: p.json_opt("activeAdapters"),
            request_id: p.string_opt_alias("request_id", "requestId"),
            user_id: p.string_opt_alias("user_id", "userId"),
            room_id: p.string_opt_alias("room_id", "roomId"),
            purpose: p.str_opt("purpose").map(String::from),
        })
    }

    /// Convert response to JSON Value
    fn response_to_json(&self, response: &TextGenerationResponse) -> Value {
        let mut result = json!({
            "success": true,
            "text": response.text,
            "finishReason": format!("{}", response.finish_reason),
            "model": response.model,
            "provider": response.provider,
            "usage": {
                "inputTokens": response.usage.input_tokens,
                "outputTokens": response.usage.output_tokens,
                "totalTokens": response.usage.total_tokens,
                "estimatedCost": response.usage.estimated_cost
            },
            "responseTimeMs": response.response_time_ms,
            "requestId": response.request_id
        });

        // Add content blocks if present
        if let Some(content) = &response.content {
            result["content"] = serde_json::to_value(content).unwrap_or(json!([]));
        }

        // Add tool calls if present
        if let Some(tool_calls) = &response.tool_calls {
            result["toolCalls"] = serde_json::to_value(tool_calls).unwrap_or(json!([]));
        }

        // Add routing info if present
        if let Some(routing) = &response.routing {
            result["routing"] = serde_json::to_value(routing).unwrap_or(json!({}));
        }

        result
    }
}

impl Default for AIProviderModule {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl ServiceModule for AIProviderModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "ai_provider",
            priority: ModulePriority::Normal,
            command_prefixes: &["ai/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            max_concurrency: 10, // Allow parallel inference requests
            tick_interval: None,
        }
    }

    async fn initialize(&self, ctx: &ModuleContext) -> Result<(), String> {
        // Store logger for this module
        let _ = self.log.set(ctx.logger("ai_provider"));
        self.register_adapters().await
    }

    async fn handle_command(
        &self,
        command: &str,
        params: Value,
    ) -> Result<CommandResult, String> {
        match command {
            "ai/generate" => {
                let _timer = TimingGuard::new("module", "ai_generate");

                // Parse request
                let request = self.parse_request(&params)?;

                // Get registry
                let registry = self.registry.read().await;

                // Select adapter
                let (provider_id, adapter) = registry
                    .select(request.provider.as_deref(), request.model.as_deref())
                    .ok_or_else(|| {
                        let available = registry.available();
                        if available.is_empty() {
                            "No AI providers configured. Add API keys to ~/.continuum/config.env".to_string()
                        } else {
                            format!("Requested provider/model not available. Available: {:?}", available)
                        }
                    })?;

                self.log().info(&format!("Using {} adapter for model {:?}",
                    provider_id, request.model));

                // Generate text
                let mut response = adapter.generate_text(request).await?;

                // Add routing info (preserve adapters_applied from adapter response)
                let prior_routing = response.routing.take();
                response.routing = Some(RoutingInfo {
                    provider: provider_id.to_string(),
                    is_local: adapter.capabilities().is_local,
                    routing_reason: prior_routing.as_ref()
                        .map(|r| r.routing_reason.clone())
                        .unwrap_or_else(|| "adapter_selected".to_string()),
                    adapters_applied: prior_routing.as_ref()
                        .map(|r| r.adapters_applied.clone())
                        .unwrap_or_default(),
                    model_mapped: None,
                    model_requested: prior_routing
                        .and_then(|r| r.model_requested),
                });

                Ok(CommandResult::Json(self.response_to_json(&response)))
            }

            "ai/providers/list" => {
                let registry = self.registry.read().await;
                let available = registry.available();

                // Get all provider info
                let mut providers_info = Vec::new();
                for id in &available {
                    if let Some(adapter) = registry.get(id) {
                        let caps = adapter.capabilities();
                        providers_info.push(json!({
                            "id": id,
                            "name": adapter.name(),
                            "defaultModel": adapter.default_model(),
                            "capabilities": {
                                "textGeneration": caps.supports_text_generation,
                                "chat": caps.supports_chat,
                                "toolUse": caps.supports_tool_use,
                                "vision": caps.supports_vision,
                                "streaming": caps.supports_streaming,
                                "embeddings": caps.supports_embeddings,
                                "isLocal": caps.is_local,
                                "maxContextWindow": caps.max_context_window
                            }
                        }));
                    }
                }

                Ok(CommandResult::Json(json!({
                    "success": true,
                    "available": available,
                    "providers": providers_info,
                    "count": available.len()
                })))
            }

            "ai/providers/health" => {
                let registry = self.registry.read().await;
                let available = registry.available();

                let mut health_results = Vec::new();
                for id in &available {
                    if let Some(adapter) = registry.get(id) {
                        let health = adapter.health_check().await;
                        health_results.push(json!({
                            "provider": id,
                            "name": adapter.name(),
                            "status": format!("{:?}", health.status).to_lowercase(),
                            "apiAvailable": health.api_available,
                            "responseTimeMs": health.response_time_ms,
                            "message": health.message
                        }));
                    }
                }

                Ok(CommandResult::Json(json!({
                    "success": true,
                    "providers": health_results
                })))
            }

            "ai/models/list" => {
                let registry = self.registry.read().await;
                let available = registry.available();

                let mut all_models = Vec::new();
                for id in &available {
                    if let Some(adapter) = registry.get(id) {
                        let models = adapter.get_available_models().await;
                        for model in models {
                            all_models.push(serde_json::to_value(&model).unwrap_or(json!({})));
                        }
                    }
                }

                Ok(CommandResult::Json(json!({
                    "success": true,
                    "models": all_models,
                    "count": all_models.len()
                })))
            }

            "ai/lora/list" => {
                let registry = self.registry.read().await;
                let available = registry.available();

                let mut all_adapters = Vec::new();
                for id in &available {
                    if let Some(adapter) = registry.get(id) {
                        let lora_adapters = adapter.list_lora_adapters();
                        for lora in lora_adapters {
                            all_adapters.push(json!({
                                "provider": id,
                                "adapterId": lora.adapter_id,
                                "path": lora.path,
                                "scale": lora.scale,
                                "loaded": lora.loaded,
                                "active": lora.active
                            }));
                        }
                    }
                }

                Ok(CommandResult::Json(json!({
                    "success": true,
                    "adapters": all_adapters,
                    "count": all_adapters.len()
                })))
            }

            "ai/lora/capabilities" => {
                let registry = self.registry.read().await;
                let available = registry.available();

                let mut capabilities = Vec::new();
                for id in &available {
                    if let Some(adapter) = registry.get(id) {
                        let caps = adapter.lora_capabilities();
                        capabilities.push(json!({
                            "provider": id,
                            "capabilities": format!("{:?}", caps)
                        }));
                    }
                }

                Ok(CommandResult::Json(json!({
                    "success": true,
                    "providers": capabilities
                })))
            }

            _ => {
                // Forward unknown ai/* commands directly to TypeScript via Unix socket.
                // MUST use execute_ts (not execute) to bypass Rust registry — otherwise
                // the registry matches "ai/" prefix back to this module → infinite recursion.
                use crate::runtime::command_executor;
                let log = crate::runtime::logger("ai_provider");
                log.info(&format!("Forwarding '{}' to TypeScript via Unix socket (bypassing registry)", command));
                command_executor::execute_ts(command, params).await
            }
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// ============================================================================
// STANDALONE GENERATE FUNCTION (for internal use by other modules)
// ============================================================================

/// Generate text using the best available provider
/// This is a convenience function for internal use (e.g., AgentModule)
pub async fn generate_text(
    registry: &AdapterRegistry,
    request: TextGenerationRequest,
) -> Result<TextGenerationResponse, String> {
    let (provider_id, adapter) = registry
        .select(request.provider.as_deref(), request.model.as_deref())
        .ok_or_else(|| {
            let available = registry.available();
            if available.is_empty() {
                "No AI providers configured. Add API keys to ~/.continuum/config.env".to_string()
            } else {
                format!("Requested provider/model not available. Available: {:?}", available)
            }
        })?;

    let mut response = adapter.generate_text(request).await?;

    // Add routing info
    response.routing = Some(RoutingInfo {
        provider: provider_id.to_string(),
        is_local: adapter.capabilities().is_local,
        routing_reason: "generate_text_call".to_string(),
        adapters_applied: vec![],
        model_mapped: None,
        model_requested: response.routing
            .as_ref()
            .and_then(|r| r.model_requested.clone()),
    });

    Ok(response)
}
