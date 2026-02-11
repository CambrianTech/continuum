//! OpenAI-Compatible Adapter - Handles providers using OpenAI's API format
//!
//! Many providers use OpenAI's API format, so we can share 95% of the code:
//! ✅ OpenAI (official)
//! ✅ DeepSeek
//! ✅ Together AI
//! ✅ Groq
//! ✅ Fireworks AI
//! ✅ XAI (Grok)
//! ✅ Google (Gemini via OpenAI-compatible endpoint)
//!
//! Only differences:
//! - API base URL
//! - API key
//! - Available models
//! - Pricing

use async_trait::async_trait;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::Instant;

use crate::secrets::get_secret;

use super::adapter::{AdapterCapabilities, AIProviderAdapter, ApiStyle};
use super::types::{
    ChatMessage, ContentPart, FinishReason, HealthState, HealthStatus, MessageContent,
    ModelCapability, ModelInfo, TextGenerationRequest, TextGenerationResponse, ToolCall,
    ToolChoice, UsageMetrics, CostPer1kTokens,
};

/// OpenAI-compatible adapter configuration
#[derive(Debug, Clone)]
pub struct OpenAICompatibleConfig {
    pub provider_id: &'static str,
    pub name: &'static str,
    pub base_url: &'static str,
    pub api_key_env: &'static str,
    pub default_model: &'static str,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub models: Vec<ModelInfo>,
}

/// OpenAI-compatible adapter implementation
pub struct OpenAICompatibleAdapter {
    config: OpenAICompatibleConfig,
    api_key: Option<String>,
    client: reqwest::Client,
    initialized: bool,
}

impl OpenAICompatibleAdapter {
    pub fn new(config: OpenAICompatibleConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            config,
            api_key: None,
            client,
            initialized: false,
        }
    }

    /// Create adapter for DeepSeek
    pub fn deepseek() -> Self {
        Self::new(OpenAICompatibleConfig {
            provider_id: "deepseek",
            name: "DeepSeek",
            base_url: "https://api.deepseek.com",
            api_key_env: "DEEPSEEK_API_KEY",
            default_model: "deepseek-chat",
            supports_tools: true,
            supports_vision: false,
            models: vec![
                ModelInfo {
                    id: "deepseek-chat".to_string(),
                    name: "DeepSeek Chat".to_string(),
                    provider: "deepseek".to_string(),
                    capabilities: vec![
                        ModelCapability::TextGeneration,
                        ModelCapability::Chat,
                        ModelCapability::ToolUse,
                    ],
                    context_window: 128000,
                    max_output_tokens: Some(8192),
                    cost_per_1k_tokens: Some(CostPer1kTokens {
                        input: 0.00014,
                        output: 0.00028,
                    }),
                    supports_streaming: true,
                    supports_tools: true,
                },
                ModelInfo {
                    id: "deepseek-reasoner".to_string(),
                    name: "DeepSeek Reasoner".to_string(),
                    provider: "deepseek".to_string(),
                    capabilities: vec![
                        ModelCapability::TextGeneration,
                        ModelCapability::Chat,
                        ModelCapability::ToolUse,
                    ],
                    context_window: 128000,
                    max_output_tokens: Some(8192),
                    cost_per_1k_tokens: Some(CostPer1kTokens {
                        input: 0.00055,
                        output: 0.00219,
                    }),
                    supports_streaming: true,
                    supports_tools: true,
                },
            ],
        })
    }

    /// Create adapter for OpenAI
    pub fn openai() -> Self {
        Self::new(OpenAICompatibleConfig {
            provider_id: "openai",
            name: "OpenAI",
            base_url: "https://api.openai.com",
            api_key_env: "OPENAI_API_KEY",
            default_model: "gpt-4-turbo-preview",
            supports_tools: true,
            supports_vision: true,
            models: vec![
                ModelInfo {
                    id: "gpt-4-turbo-preview".to_string(),
                    name: "GPT-4 Turbo".to_string(),
                    provider: "openai".to_string(),
                    capabilities: vec![
                        ModelCapability::TextGeneration,
                        ModelCapability::Chat,
                        ModelCapability::ToolUse,
                        ModelCapability::ImageAnalysis,
                    ],
                    context_window: 128000,
                    max_output_tokens: Some(4096),
                    cost_per_1k_tokens: Some(CostPer1kTokens {
                        input: 0.01,
                        output: 0.03,
                    }),
                    supports_streaming: true,
                    supports_tools: true,
                },
                ModelInfo {
                    id: "gpt-4o".to_string(),
                    name: "GPT-4o".to_string(),
                    provider: "openai".to_string(),
                    capabilities: vec![
                        ModelCapability::TextGeneration,
                        ModelCapability::Chat,
                        ModelCapability::ToolUse,
                        ModelCapability::ImageAnalysis,
                        ModelCapability::Multimodal,
                    ],
                    context_window: 128000,
                    max_output_tokens: Some(4096),
                    cost_per_1k_tokens: Some(CostPer1kTokens {
                        input: 0.005,
                        output: 0.015,
                    }),
                    supports_streaming: true,
                    supports_tools: true,
                },
            ],
        })
    }

    /// Create adapter for Together AI
    pub fn together() -> Self {
        Self::new(OpenAICompatibleConfig {
            provider_id: "together",
            name: "Together AI",
            base_url: "https://api.together.xyz",
            api_key_env: "TOGETHER_API_KEY",
            default_model: "meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo",
            supports_tools: true,
            supports_vision: false,
            models: vec![ModelInfo {
                id: "meta-llama/Meta-Llama-3.1-70B-Instruct-Turbo".to_string(),
                name: "Llama 3.1 70B Instruct".to_string(),
                provider: "together".to_string(),
                capabilities: vec![
                    ModelCapability::TextGeneration,
                    ModelCapability::Chat,
                    ModelCapability::ToolUse,
                ],
                context_window: 131072,
                max_output_tokens: Some(4096),
                cost_per_1k_tokens: Some(CostPer1kTokens {
                    input: 0.00088,
                    output: 0.00088,
                }),
                supports_streaming: true,
                supports_tools: true,
            }],
        })
    }

    /// Create adapter for Groq
    pub fn groq() -> Self {
        Self::new(OpenAICompatibleConfig {
            provider_id: "groq",
            name: "Groq",
            base_url: "https://api.groq.com/openai",
            api_key_env: "GROQ_API_KEY",
            default_model: "llama-3.1-8b-instant",
            supports_tools: true,
            supports_vision: false,
            models: vec![ModelInfo {
                id: "llama-3.1-8b-instant".to_string(),
                name: "Llama 3.1 8B Instant".to_string(),
                provider: "groq".to_string(),
                capabilities: vec![
                    ModelCapability::TextGeneration,
                    ModelCapability::Chat,
                    ModelCapability::ToolUse,
                ],
                context_window: 131072,
                max_output_tokens: Some(8192),
                cost_per_1k_tokens: Some(CostPer1kTokens {
                    input: 0.00005,
                    output: 0.00008,
                }),
                supports_streaming: true,
                supports_tools: true,
            }],
        })
    }

    /// Create adapter for Fireworks AI
    pub fn fireworks() -> Self {
        Self::new(OpenAICompatibleConfig {
            provider_id: "fireworks",
            name: "Fireworks AI",
            base_url: "https://api.fireworks.ai/inference",
            api_key_env: "FIREWORKS_API_KEY",
            default_model: "accounts/fireworks/models/deepseek-v3",
            supports_tools: true,
            supports_vision: false,
            models: vec![ModelInfo {
                id: "accounts/fireworks/models/deepseek-v3".to_string(),
                name: "DeepSeek V3 (Fireworks)".to_string(),
                provider: "fireworks".to_string(),
                capabilities: vec![
                    ModelCapability::TextGeneration,
                    ModelCapability::Chat,
                    ModelCapability::ToolUse,
                ],
                context_window: 128000,
                max_output_tokens: Some(8192),
                cost_per_1k_tokens: Some(CostPer1kTokens {
                    input: 0.0009,
                    output: 0.0009,
                }),
                supports_streaming: true,
                supports_tools: true,
            }],
        })
    }

    /// Create adapter for XAI (Grok)
    pub fn xai() -> Self {
        Self::new(OpenAICompatibleConfig {
            provider_id: "xai",
            name: "xAI",
            base_url: "https://api.x.ai",
            api_key_env: "XAI_API_KEY",
            default_model: "grok-3",
            supports_tools: true,
            supports_vision: false,
            models: vec![
                ModelInfo {
                    id: "grok-3".to_string(),
                    name: "Grok 3".to_string(),
                    provider: "xai".to_string(),
                    capabilities: vec![
                        ModelCapability::TextGeneration,
                        ModelCapability::Chat,
                        ModelCapability::ToolUse,
                    ],
                    context_window: 131072,
                    max_output_tokens: Some(8192),
                    cost_per_1k_tokens: Some(CostPer1kTokens {
                        input: 0.003,
                        output: 0.015,
                    }),
                    supports_streaming: true,
                    supports_tools: true,
                },
            ],
        })
    }

    /// Create adapter for Google (Gemini via OpenAI-compatible endpoint)
    pub fn google() -> Self {
        Self::new(OpenAICompatibleConfig {
            provider_id: "google",
            name: "Google",
            base_url: "https://generativelanguage.googleapis.com/v1beta/openai",
            api_key_env: "GOOGLE_API_KEY",
            default_model: "gemini-2.0-flash",
            supports_tools: true,
            supports_vision: true,
            models: vec![ModelInfo {
                id: "gemini-2.0-flash".to_string(),
                name: "Gemini 2.0 Flash".to_string(),
                provider: "google".to_string(),
                capabilities: vec![
                    ModelCapability::TextGeneration,
                    ModelCapability::Chat,
                    ModelCapability::ToolUse,
                    ModelCapability::ImageAnalysis,
                ],
                context_window: 1000000,
                max_output_tokens: Some(8192),
                cost_per_1k_tokens: Some(CostPer1kTokens {
                    input: 0.000075,
                    output: 0.0003,
                }),
                supports_streaming: true,
                supports_tools: true,
            }],
        })
    }

    /// Convert ChatMessage to OpenAI format
    fn format_messages(&self, messages: &[ChatMessage], system_prompt: Option<&str>) -> Vec<Value> {
        let mut result = Vec::new();

        // Add system prompt if provided
        if let Some(sys) = system_prompt {
            result.push(json!({
                "role": "system",
                "content": sys
            }));
        }

        for msg in messages {
            match &msg.content {
                MessageContent::Text(text) => {
                    result.push(json!({
                        "role": msg.role,
                        "content": text
                    }));
                }
                MessageContent::Parts(parts) => {
                    // Check for tool protocol blocks
                    let has_tool_use = parts.iter().any(|p| matches!(p, ContentPart::ToolUse { .. }));
                    let has_tool_result = parts.iter().any(|p| matches!(p, ContentPart::ToolResult { .. }));

                    if has_tool_use {
                        // Assistant message with tool_calls
                        let text_content: String = parts
                            .iter()
                            .filter_map(|p| match p {
                                ContentPart::Text { text } => Some(text.as_str()),
                                _ => None,
                            })
                            .collect::<Vec<_>>()
                            .join("");

                        let tool_calls: Vec<Value> = parts
                            .iter()
                            .filter_map(|p| match p {
                                ContentPart::ToolUse { id, name, input } => Some(json!({
                                    "id": id,
                                    "type": "function",
                                    "function": {
                                        "name": name,
                                        "arguments": serde_json::to_string(input).unwrap_or_default()
                                    }
                                })),
                                _ => None,
                            })
                            .collect();

                        result.push(json!({
                            "role": "assistant",
                            "content": if text_content.is_empty() { Value::Null } else { Value::String(text_content) },
                            "tool_calls": tool_calls
                        }));
                    } else if has_tool_result {
                        // Tool results as separate messages
                        for part in parts {
                            if let ContentPart::ToolResult { tool_use_id, content, .. } = part {
                                result.push(json!({
                                    "role": "tool",
                                    "tool_call_id": tool_use_id,
                                    "content": content
                                }));
                            }
                        }
                    } else {
                        // Standard multimodal content
                        let content: Vec<Value> = parts
                            .iter()
                            .filter_map(|p| match p {
                                ContentPart::Text { text } => Some(json!({
                                    "type": "text",
                                    "text": text
                                })),
                                ContentPart::Image { image } => {
                                    if let Some(url) = &image.url {
                                        Some(json!({
                                            "type": "image_url",
                                            "image_url": { "url": url }
                                        }))
                                    } else if let Some(b64) = &image.base64 {
                                        Some(json!({
                                            "type": "image_url",
                                            "image_url": {
                                                "url": format!("data:{};base64,{}",
                                                    image.mime_type.as_deref().unwrap_or("image/png"), b64)
                                            }
                                        }))
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            })
                            .collect();

                        result.push(json!({
                            "role": msg.role,
                            "content": content
                        }));
                    }
                }
            }
        }

        result
    }

    /// Map OpenAI finish reason to our enum
    fn map_finish_reason(&self, reason: &str) -> FinishReason {
        match reason {
            "stop" => FinishReason::Stop,
            "length" => FinishReason::Length,
            "tool_calls" => FinishReason::ToolUse,
            _ => FinishReason::Error,
        }
    }
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct OpenAIResponse {
    id: String,
    choices: Vec<OpenAIChoice>,
    usage: Option<OpenAIUsage>,
    model: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIChoice {
    message: OpenAIMessage,
    finish_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
struct OpenAIMessage {
    content: Option<String>,
    tool_calls: Option<Vec<OpenAIToolCall>>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct OpenAIToolCall {
    id: String,
    #[serde(rename = "type")]
    call_type: String,
    function: OpenAIFunction,
}

#[derive(Debug, Deserialize)]
struct OpenAIFunction {
    name: String,
    arguments: String,
}

#[derive(Debug, Deserialize)]
struct OpenAIUsage {
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: Option<u32>,
}

#[async_trait]
impl AIProviderAdapter for OpenAICompatibleAdapter {
    fn provider_id(&self) -> &str {
        self.config.provider_id
    }

    fn name(&self) -> &str {
        self.config.name
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            supports_text_generation: true,
            supports_chat: true,
            supports_tool_use: self.config.supports_tools,
            supports_vision: self.config.supports_vision,
            supports_streaming: true,
            supports_embeddings: self.config.provider_id == "openai",
            supports_audio: false,
            supports_image_generation: self.config.provider_id == "openai",
            is_local: false,
            max_context_window: self.config.models.first()
                .map(|m| m.context_window)
                .unwrap_or(128000),
        }
    }

    fn api_style(&self) -> ApiStyle {
        ApiStyle::OpenAI
    }

    fn default_model(&self) -> &str {
        self.config.default_model
    }

    async fn initialize(&mut self) -> Result<(), String> {
        // Load API key
        self.api_key = get_secret(self.config.api_key_env).map(|s| s.to_string());

        if self.api_key.is_none() {
            return Err(format!(
                "{} API key not configured ({})",
                self.config.name, self.config.api_key_env
            ));
        }

        self.initialized = true;
        Ok(())
    }

    async fn shutdown(&mut self) -> Result<(), String> {
        self.initialized = false;
        Ok(())
    }

    async fn generate_text(
        &self,
        request: TextGenerationRequest,
    ) -> Result<TextGenerationResponse, String> {
        let api_key = self.api_key.as_ref()
            .ok_or_else(|| format!("{} not initialized", self.config.name))?;

        let start = Instant::now();
        let request_id = request.request_id.clone()
            .unwrap_or_else(|| format!("req-{}", chrono::Utc::now().timestamp_millis()));
        let model = request.model.as_deref().unwrap_or(self.config.default_model);

        // Build request body
        let messages = self.format_messages(&request.messages, request.system_prompt.as_deref());

        let mut body = json!({
            "model": model,
            "messages": messages,
            "temperature": request.temperature.unwrap_or(0.7),
            "max_tokens": request.max_tokens.unwrap_or(2048),
            "stream": false
        });

        // Add tools if provided
        if let Some(tools) = &request.tools {
            if !tools.is_empty() && self.config.supports_tools {
                let openai_tools: Vec<Value> = tools
                    .iter()
                    .map(|tool| {
                        json!({
                            "type": "function",
                            "function": {
                                "name": tool.name,
                                "description": tool.description,
                                "parameters": tool.input_schema
                            }
                        })
                    })
                    .collect();
                body["tools"] = json!(openai_tools);

                // Add tool_choice if specified
                if let Some(choice) = &request.tool_choice {
                    match choice {
                        ToolChoice::Mode(mode) => {
                            body["tool_choice"] = json!(mode);
                        }
                        ToolChoice::Specific { name } => {
                            body["tool_choice"] = json!({
                                "type": "function",
                                "function": { "name": name }
                            });
                        }
                    }
                }
            }
        }

        // Make request
        let url = format!("{}/v1/chat/completions", self.config.base_url);
        let response = self.client
            .post(&url)
            .header("Authorization", format!("Bearer {}", api_key))
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("{} request failed: {}", self.config.name, e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("{} returned {}: {}", self.config.name, status, body));
        }

        let response_json: OpenAIResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse {} response: {}", self.config.name, e))?;

        let response_time_ms = start.elapsed().as_millis() as u64;

        // Parse response
        let choice = response_json.choices.first()
            .ok_or_else(|| "No completion in response".to_string())?;

        let text = choice.message.content.clone().unwrap_or_default();
        let finish_reason = choice.finish_reason.as_deref()
            .map(|r| self.map_finish_reason(r))
            .unwrap_or(FinishReason::Stop);

        // Parse tool calls
        let tool_calls: Option<Vec<ToolCall>> = choice.message.tool_calls.as_ref().map(|tcs| {
            tcs.iter()
                .map(|tc| {
                    let input: Value = serde_json::from_str(&tc.function.arguments)
                        .unwrap_or_else(|_| json!({ "_raw": tc.function.arguments }));
                    ToolCall {
                        id: tc.id.clone(),
                        name: tc.function.name.clone(),
                        input,
                    }
                })
                .collect()
        });

        // Build content blocks
        let mut content_blocks = Vec::new();
        if !text.is_empty() {
            content_blocks.push(ContentPart::Text { text: text.clone() });
        }
        if let Some(ref tcs) = tool_calls {
            for tc in tcs {
                content_blocks.push(ContentPart::ToolUse {
                    id: tc.id.clone(),
                    name: tc.name.clone(),
                    input: tc.input.clone(),
                });
            }
        }

        let usage = response_json.usage.map(|u| UsageMetrics {
            input_tokens: u.prompt_tokens,
            output_tokens: u.completion_tokens,
            total_tokens: u.total_tokens.unwrap_or(u.prompt_tokens + u.completion_tokens),
            estimated_cost: None, // TODO: Calculate from model pricing
        }).unwrap_or_default();

        Ok(TextGenerationResponse {
            text,
            finish_reason,
            model: response_json.model,
            provider: self.config.provider_id.to_string(),
            usage,
            response_time_ms,
            request_id,
            content: if content_blocks.is_empty() { None } else { Some(content_blocks) },
            tool_calls,
            routing: None,
            error: None,
        })
    }

    async fn health_check(&self) -> HealthStatus {
        if self.api_key.is_none() {
            return HealthStatus {
                status: HealthState::Unhealthy,
                api_available: false,
                response_time_ms: 0,
                error_rate: 1.0,
                last_checked: chrono::Utc::now().timestamp_millis() as u64,
                message: Some(format!("{} API key not configured", self.config.name)),
            };
        }

        let start = Instant::now();

        // Try to list models as health check
        let url = format!("{}/v1/models", self.config.base_url);
        let result = self.client
            .get(&url)
            .header("Authorization", format!("Bearer {}", self.api_key.as_ref().unwrap()))
            .timeout(std::time::Duration::from_secs(5))
            .send()
            .await;

        let response_time_ms = start.elapsed().as_millis() as u64;

        match result {
            Ok(resp) if resp.status().is_success() => HealthStatus {
                status: HealthState::Healthy,
                api_available: true,
                response_time_ms,
                error_rate: 0.0,
                last_checked: chrono::Utc::now().timestamp_millis() as u64,
                message: Some(format!("{} API is accessible", self.config.name)),
            },
            Ok(resp) => HealthStatus {
                status: HealthState::Unhealthy,
                api_available: false,
                response_time_ms,
                error_rate: 1.0,
                last_checked: chrono::Utc::now().timestamp_millis() as u64,
                message: Some(format!("{} returned {}", self.config.name, resp.status())),
            },
            Err(e) => HealthStatus {
                status: HealthState::Unhealthy,
                api_available: false,
                response_time_ms,
                error_rate: 1.0,
                last_checked: chrono::Utc::now().timestamp_millis() as u64,
                message: Some(format!("{} error: {}", self.config.name, e)),
            },
        }
    }

    async fn get_available_models(&self) -> Vec<ModelInfo> {
        self.config.models.clone()
    }

    fn supported_model_prefixes(&self) -> Vec<&'static str> {
        // Return prefixes based on provider
        match self.config.provider_id.as_ref() {
            "openai" => vec!["gpt", "o1", "o3"],
            "deepseek" => vec!["deepseek"],
            "groq" => vec!["llama-3", "mixtral", "gemma2"], // Groq's hosted models
            "together" => vec!["togethercomputer/"], // Together's namespace
            "fireworks" => vec!["accounts/fireworks/"], // Fireworks namespace
            "xai" => vec!["grok"],
            "google" => vec!["gemini"],
            _ => vec![], // No auto-routing for unknown providers
        }
    }
}
