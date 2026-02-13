//! Anthropic Adapter - Claude models (different API format from OpenAI)
//!
//! Anthropic uses a proprietary API format:
//! - POST /v1/messages with x-api-key header (not Bearer)
//! - System prompt is a separate field (not in messages array)
//! - Content is array of blocks (text, tool_use, tool_result)
//! - Tool calling uses native tool_use blocks
//!
//! Supports:
//! ✅ Claude Sonnet 4.5 (best reasoning)
//! ✅ Claude Opus 4 (most capable)
//! ✅ Claude 3.5 Haiku (fast and cheap)
//! ✅ Multimodal (vision)
//! ✅ Native tool calling
//! ✅ 200k context window

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

/// Anthropic adapter implementation
pub struct AnthropicAdapter {
    api_key: Option<String>,
    client: reqwest::Client,
    initialized: bool,
}

impl AnthropicAdapter {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("Failed to create HTTP client");

        Self {
            api_key: None,
            client,
            initialized: false,
        }
    }

    /// Convert ChatMessage to Anthropic format
    fn format_messages(&self, messages: &[ChatMessage]) -> (Vec<Value>, Option<String>) {
        let mut result = Vec::new();
        let mut system_prompt = None;

        for msg in messages {
            // Extract system prompt from messages
            if msg.role == "system" {
                system_prompt = Some(msg.content_text());
                continue;
            }

            let role = if msg.role == "assistant" { "assistant" } else { "user" };

            match &msg.content {
                MessageContent::Text(text) => {
                    result.push(json!({
                        "role": role,
                        "content": text
                    }));
                }
                MessageContent::Parts(parts) => {
                    // Check for tool protocol blocks
                    let has_tool_use = parts.iter().any(|p| matches!(p, ContentPart::ToolUse { .. }));
                    let has_tool_result = parts.iter().any(|p| matches!(p, ContentPart::ToolResult { .. }));

                    if has_tool_use || has_tool_result {
                        // Anthropic native tool format
                        let content: Vec<Value> = parts
                            .iter()
                            .filter_map(|p| match p {
                                ContentPart::Text { text } => Some(json!({
                                    "type": "text",
                                    "text": text
                                })),
                                ContentPart::ToolUse { id, name, input } => Some(json!({
                                    "type": "tool_use",
                                    "id": id,
                                    "name": name,
                                    "input": input
                                })),
                                ContentPart::ToolResult { tool_use_id, content, is_error } => {
                                    let mut obj = json!({
                                        "type": "tool_result",
                                        "tool_use_id": tool_use_id,
                                        "content": content
                                    });
                                    if is_error.unwrap_or(false) {
                                        obj["is_error"] = json!(true);
                                    }
                                    Some(obj)
                                }
                                _ => None,
                            })
                            .collect();

                        result.push(json!({
                            "role": role,
                            "content": content
                        }));
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
                                    if let Some(b64) = &image.base64 {
                                        Some(json!({
                                            "type": "image",
                                            "source": {
                                                "type": "base64",
                                                "media_type": image.mime_type.as_deref().unwrap_or("image/png"),
                                                "data": b64
                                            }
                                        }))
                                    } else if let Some(url) = &image.url {
                                        // Anthropic prefers base64, but supports URLs
                                        Some(json!({
                                            "type": "image",
                                            "source": {
                                                "type": "url",
                                                "url": url
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
                            "role": role,
                            "content": content
                        }));
                    }
                }
            }
        }

        (result, system_prompt)
    }

    /// Map Anthropic stop reason to our enum
    fn map_finish_reason(&self, reason: &str) -> FinishReason {
        match reason {
            "end_turn" => FinishReason::Stop,
            "max_tokens" => FinishReason::Length,
            "tool_use" => FinishReason::ToolUse,
            _ => FinishReason::Error,
        }
    }
}

impl Default for AnthropicAdapter {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct AnthropicResponse {
    id: String,
    content: Vec<AnthropicContentBlock>,
    model: String,
    stop_reason: Option<String>,
    usage: Option<AnthropicUsage>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum AnthropicContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse { id: String, name: String, input: Value },
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

// Model IDs
const CLAUDE_SONNET_4_5: &str = "claude-sonnet-4-5-20250929";
const CLAUDE_OPUS_4: &str = "claude-opus-4-20250514";
const CLAUDE_HAIKU_3_5: &str = "claude-3-5-haiku-20250107";

#[async_trait]
impl AIProviderAdapter for AnthropicAdapter {
    fn provider_id(&self) -> &str {
        "anthropic"
    }

    fn name(&self) -> &str {
        "Anthropic"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        AdapterCapabilities {
            supports_text_generation: true,
            supports_chat: true,
            supports_tool_use: true,
            supports_vision: true,
            supports_streaming: true,
            supports_embeddings: false,
            supports_audio: false,
            supports_image_generation: false,
            is_local: false,
            max_context_window: 200000,
        }
    }

    fn api_style(&self) -> ApiStyle {
        ApiStyle::Anthropic
    }

    fn default_model(&self) -> &str {
        CLAUDE_SONNET_4_5
    }

    async fn initialize(&mut self) -> Result<(), String> {
        self.api_key = get_secret("ANTHROPIC_API_KEY").map(|s| s.to_string());

        if self.api_key.is_none() {
            return Err("Anthropic API key not configured (ANTHROPIC_API_KEY)".to_string());
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
            .ok_or_else(|| "Anthropic not initialized".to_string())?;

        let start = Instant::now();
        let request_id = request.request_id.clone()
            .unwrap_or_else(|| format!("req-{}", chrono::Utc::now().timestamp_millis()));
        let model = request.model.as_deref().unwrap_or(CLAUDE_SONNET_4_5);

        // Build messages and extract system prompt
        let (messages, msg_system) = self.format_messages(&request.messages);
        let system_prompt = request.system_prompt.as_deref().or(msg_system.as_deref());

        // Build request body
        let mut body = json!({
            "model": model,
            "messages": messages,
            "max_tokens": request.max_tokens.unwrap_or(1024),
            "temperature": request.temperature.unwrap_or(0.7)
        });

        // Add system prompt if present
        if let Some(sys) = system_prompt {
            body["system"] = json!(sys);
        }

        // Add top_p if specified
        if let Some(top_p) = request.top_p {
            body["top_p"] = json!(top_p);
        }

        // Add stop sequences if specified
        if let Some(stop) = &request.stop_sequences {
            body["stop_sequences"] = json!(stop);
        }

        // Add tools if provided
        if let Some(tools) = &request.tools {
            if !tools.is_empty() {
                let anthropic_tools: Vec<Value> = tools
                    .iter()
                    .map(|tool| {
                        json!({
                            "name": tool.name,
                            "description": tool.description,
                            "input_schema": tool.input_schema
                        })
                    })
                    .collect();
                body["tools"] = json!(anthropic_tools);

                // Add tool_choice if specified
                if let Some(choice) = &request.tool_choice {
                    match choice {
                        ToolChoice::Mode(mode) => {
                            // Anthropic uses { type: "auto" | "any" | "none" }
                            body["tool_choice"] = json!({ "type": mode });
                        }
                        ToolChoice::Specific { name } => {
                            body["tool_choice"] = json!({
                                "type": "tool",
                                "name": name
                            });
                        }
                    }
                }
            }
        }

        // Make request
        let response = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("Anthropic request failed: {}", e))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!("Anthropic returned {}: {}", status, body));
        }

        let response_json: AnthropicResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse Anthropic response: {}", e))?;

        let response_time_ms = start.elapsed().as_millis() as u64;

        // Parse response content blocks
        let mut text = String::new();
        let mut tool_calls = Vec::new();
        let mut content_blocks = Vec::new();

        for block in &response_json.content {
            match block {
                AnthropicContentBlock::Text { text: t } => {
                    text.push_str(t);
                    content_blocks.push(ContentPart::Text { text: t.clone() });
                }
                AnthropicContentBlock::ToolUse { id, name, input } => {
                    tool_calls.push(ToolCall {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    });
                    content_blocks.push(ContentPart::ToolUse {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    });
                }
            }
        }

        let finish_reason = response_json.stop_reason.as_deref()
            .map(|r| self.map_finish_reason(r))
            .unwrap_or(FinishReason::Stop);

        let usage = response_json.usage.map(|u| UsageMetrics {
            input_tokens: u.input_tokens,
            output_tokens: u.output_tokens,
            total_tokens: u.input_tokens + u.output_tokens,
            estimated_cost: Some(self.calculate_cost(u.input_tokens, u.output_tokens, model)),
        }).unwrap_or_default();

        Ok(TextGenerationResponse {
            text,
            finish_reason,
            model: response_json.model,
            provider: "anthropic".to_string(),
            usage,
            response_time_ms,
            request_id,
            content: if content_blocks.is_empty() { None } else { Some(content_blocks) },
            tool_calls: if tool_calls.is_empty() { None } else { Some(tool_calls) },
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
                message: Some("Anthropic API key not configured".to_string()),
            };
        }

        let start = Instant::now();

        // Anthropic doesn't have a health endpoint, so we do a minimal API call
        let result = self.client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", self.api_key.as_ref().unwrap())
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&json!({
                "model": CLAUDE_HAIKU_3_5,
                "messages": [{ "role": "user", "content": "hi" }],
                "max_tokens": 1
            }))
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
                message: Some("Anthropic API is accessible".to_string()),
            },
            Ok(resp) => {
                let status = resp.status();
                let is_billing = status.as_u16() == 402 || status.as_u16() == 429;
                HealthStatus {
                    status: if is_billing { HealthState::InsufficientFunds } else { HealthState::Unhealthy },
                    api_available: false,
                    response_time_ms,
                    error_rate: 1.0,
                    last_checked: chrono::Utc::now().timestamp_millis() as u64,
                    message: Some(format!("Anthropic returned {}", status)),
                }
            }
            Err(e) => HealthStatus {
                status: HealthState::Unhealthy,
                api_available: false,
                response_time_ms,
                error_rate: 1.0,
                last_checked: chrono::Utc::now().timestamp_millis() as u64,
                message: Some(format!("Anthropic error: {}", e)),
            },
        }
    }

    async fn get_available_models(&self) -> Vec<ModelInfo> {
        vec![
            ModelInfo {
                id: CLAUDE_SONNET_4_5.to_string(),
                name: "Claude Sonnet 4.5".to_string(),
                provider: "anthropic".to_string(),
                capabilities: vec![
                    ModelCapability::TextGeneration,
                    ModelCapability::Chat,
                    ModelCapability::ToolUse,
                    ModelCapability::ImageAnalysis,
                    ModelCapability::Multimodal,
                ],
                context_window: 200000,
                max_output_tokens: Some(8192),
                cost_per_1k_tokens: Some(CostPer1kTokens {
                    input: 0.003,
                    output: 0.015,
                }),
                supports_streaming: true,
                supports_tools: true,
            },
            ModelInfo {
                id: CLAUDE_OPUS_4.to_string(),
                name: "Claude Opus 4".to_string(),
                provider: "anthropic".to_string(),
                capabilities: vec![
                    ModelCapability::TextGeneration,
                    ModelCapability::Chat,
                    ModelCapability::ToolUse,
                    ModelCapability::ImageAnalysis,
                    ModelCapability::Multimodal,
                ],
                context_window: 200000,
                max_output_tokens: Some(4096),
                cost_per_1k_tokens: Some(CostPer1kTokens {
                    input: 0.015,
                    output: 0.075,
                }),
                supports_streaming: true,
                supports_tools: true,
            },
            ModelInfo {
                id: CLAUDE_HAIKU_3_5.to_string(),
                name: "Claude 3.5 Haiku".to_string(),
                provider: "anthropic".to_string(),
                capabilities: vec![
                    ModelCapability::TextGeneration,
                    ModelCapability::Chat,
                    ModelCapability::ToolUse,
                    ModelCapability::ImageAnalysis,
                ],
                context_window: 200000,
                max_output_tokens: Some(4096),
                cost_per_1k_tokens: Some(CostPer1kTokens {
                    input: 0.00025,
                    output: 0.00125,
                }),
                supports_streaming: true,
                supports_tools: true,
            },
        ]
    }

    fn supported_model_prefixes(&self) -> Vec<&'static str> {
        vec!["claude"]
    }
}

impl AnthropicAdapter {
    fn calculate_cost(&self, input_tokens: u32, output_tokens: u32, model: &str) -> f64 {
        let (input_cost, output_cost) = match model {
            m if m.contains("sonnet") => (0.003, 0.015),
            m if m.contains("opus") => (0.015, 0.075),
            m if m.contains("haiku") => (0.00025, 0.00125),
            _ => (0.003, 0.015), // Default to Sonnet pricing
        };

        (input_tokens as f64 / 1000.0) * input_cost + (output_tokens as f64 / 1000.0) * output_cost
    }
}
