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

use crate::model_registry::Capability;
use crate::secrets::get_secret;

use super::adapter::{AIProviderAdapter, AdapterCapabilities, ApiStyle};
use super::types::{
    ChatMessage, ContentPart, FinishReason, HealthState, HealthStatus, MessageContent, ModelInfo,
    TextGenerationRequest, TextGenerationResponse, ToolCall, ToolChoice, UsageMetrics,
};

/// Anthropic adapter implementation
pub struct AnthropicAdapter {
    api_key: Option<String>,
    client: reqwest::Client,
    initialized: bool,
    /// Resolved from registry at construction. Held as `String` so
    /// `default_model()` can return `&str`. No hardcoded CLAUDE_* const
    /// — the ID lives in the Rust catalog (catalog.rs), this is the cached view.
    default_model: String,
    /// Cheapest Anthropic model by `cost_input_per_1k`, used for the
    /// auth-probe health check. Picked at construction rather than
    /// hardcoded so a catalog edit that adds a cheaper model
    /// (Claude 4.0 Haiku?) takes effect without code changes.
    health_check_model: String,
}

impl AnthropicAdapter {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .build()
            .expect("Failed to create HTTP client");

        // Both model ids come from the registry. Panics (loudly) if the
        // registry wasn't initialized before adapter construction —
        // that's a boot-order bug, not a runtime failure mode.
        let reg = crate::model_registry::global();
        let default_model = reg
            .provider("anthropic")
            .and_then(|p| p.default_model.clone())
            .expect("anthropic provider has no default_model in the Rust catalog (catalog.rs)");
        let health_check_model = reg
            .models_for_provider("anthropic")
            .min_by(|a, b| {
                a.cost_input_per_1k
                    .partial_cmp(&b.cost_input_per_1k)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|m| m.id.clone())
            .expect("anthropic has no models registered");

        Self {
            api_key: None,
            client,
            initialized: false,
            default_model,
            health_check_model,
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

            let role = if msg.role == "assistant" {
                "assistant"
            } else {
                "user"
            };

            match &msg.content {
                MessageContent::Text(text) => {
                    result.push(json!({
                        "role": role,
                        "content": text
                    }));
                }
                MessageContent::Parts(parts) => {
                    // Check for tool protocol blocks
                    let has_tool_use = parts
                        .iter()
                        .any(|p| matches!(p, ContentPart::ToolUse { .. }));
                    let has_tool_result = parts
                        .iter()
                        .any(|p| matches!(p, ContentPart::ToolResult { .. }));

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
                                ContentPart::ToolResult {
                                    tool_use_id,
                                    content,
                                    is_error,
                                } => {
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
                                    } else {
                                        image.url.as_ref().map(|url| json!({
                                            "type": "image",
                                            "source": {
                                                "type": "url",
                                                "url": url
                                            }
                                        }))
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
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
}

#[derive(Debug, Deserialize)]
struct AnthropicUsage {
    input_tokens: u32,
    output_tokens: u32,
}

// Model IDs
// Model identity lives in the Rust catalog (catalog.rs).
// Adapter caches resolved ids in `self.default_model` + `self.health_check_model`
// at construction. Any code that needs a Claude id reads it via the
// registry, not via a constant here.

#[async_trait]
impl AIProviderAdapter for AnthropicAdapter {
    fn provider_id(&self) -> &str {
        "anthropic"
    }

    fn name(&self) -> &str {
        "Anthropic"
    }

    fn capabilities(&self) -> AdapterCapabilities {
        // Anthropic: native function calling (tool_use blocks) + native JSON
        // Schema enforcement, streaming, and vision-in. Audio is bridged
        // (STT/TTS) since it's absent from the set. Embeddings/image-gen not
        // offered by this API.
        AdapterCapabilities::builder()
            .capabilities([
                Capability::TextGeneration,
                Capability::Chat,
                Capability::ToolUse,
                Capability::Vision,
                Capability::Streaming,
            ])
            .remote()
            .context_window(200_000)
            .max_output_tokens(8_192)
            .protocols(crate::ai::adapter::NativeProtocols::FunctionCalling)
            .build()
    }

    fn api_style(&self) -> ApiStyle {
        ApiStyle::Anthropic
    }

    fn default_model(&self) -> &str {
        &self.default_model
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
        let api_key = self
            .api_key
            .as_ref()
            .ok_or_else(|| "Anthropic not initialized".to_string())?;

        let start = Instant::now();
        let request_id = request
            .request_id
            .clone()
            .unwrap_or_else(|| format!("req-{}", chrono::Utc::now().timestamp_millis()));
        let model = request.model.as_deref().unwrap_or(&self.default_model);

        // Build messages and extract system prompt
        let (messages, msg_system) = self.format_messages(&request.messages);
        let system_prompt = request.system_prompt.as_deref().or(msg_system.as_deref());

        // Anthropic's Messages API REQUIRES max_tokens — it cannot be omitted. When
        // the caller leaves it unset (`None` = "the model owns its length"), derive
        // the ceiling from the model's reported capability rather than inventing a
        // magic inline number. The capability is the single authority on this model's
        // real output limit; the adapter just reads it.
        let Some(max_tokens) = request
            .max_tokens
            .or_else(|| self.capabilities().max_output_tokens)
        else {
            // Undeclared is now representable (it used to silently inherit a 2048 floor), so
            // handle it honestly: this provider's API cannot proceed without the number, and
            // inventing one is the exact defect that floor was. Fail loud.
            return Err("Anthropic requires max_tokens and this adapter declares no \
                        max_output_tokens capability — declare it via \
                        AdapterCapabilities::builder().max_output_tokens(n)"
                .to_string());
        };

        // Build request body
        let mut body = json!({
            "model": model,
            "messages": messages,
            "max_tokens": max_tokens,
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
        let response = self
            .client
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

        let finish_reason = response_json
            .stop_reason
            .as_deref()
            .map(|r| self.map_finish_reason(r))
            .unwrap_or(FinishReason::Stop);

        let usage = response_json
            .usage
            .map(|u| UsageMetrics {
                input_tokens: u.input_tokens,
                output_tokens: u.output_tokens,
                total_tokens: u.input_tokens + u.output_tokens,
                estimated_cost: Some(self.calculate_cost(u.input_tokens, u.output_tokens, model)),
            })
            .unwrap_or_default();

        Ok(TextGenerationResponse {
            text,
            finish_reason,
            model: response_json.model,
            provider: "anthropic".to_string(),
            usage,
            response_time_ms,
            request_id,
            content: if content_blocks.is_empty() {
                None
            } else {
                Some(content_blocks)
            },
            tool_calls: if tool_calls.is_empty() {
                None
            } else {
                Some(tool_calls)
            },
            // TODO: Anthropic extended-thinking blocks could populate this; until
            // that's wired, Claude reasoning isn't separated here (it doesn't leak —
            // Claude doesn't emit inline <think> in text).
            reasoning: None,
            routing: None,
            error: None,
            timing: None,
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
        let result = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", self.api_key.as_deref().unwrap_or_default())
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&json!({
                "model": self.health_check_model,
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
                    status: if is_billing {
                        HealthState::InsufficientFunds
                    } else {
                        HealthState::Unhealthy
                    },
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
        // Source of truth lives in the Rust catalog (catalog.rs). Registry projects
        // each model_registry::Model to the legacy ai::ModelInfo shape
        // via the From impl in registry_bridge.
        super::registry_bridge::models_for_provider_via_registry("anthropic")
    }

    fn supported_model_prefixes(&self) -> Vec<&'static str> {
        vec!["claude"]
    }
}

impl AnthropicAdapter {
    fn calculate_cost(&self, input_tokens: u32, output_tokens: u32, model: &str) -> f64 {
        // Per-model cost is a registry FACT (#70) — read it, never re-guess it
        // from the name, and never silently default an unknown model to Sonnet
        // pricing (that was a fallback masking a misconfig). An unmodeled id
        // genuinely has no cost estimate; report 0 and name it, loudly.
        let (input_cost, output_cost) = match crate::model_registry::global().model(model) {
            Some(m) => (m.cost_input_per_1k as f64, m.cost_output_per_1k as f64),
            None => {
                crate::clog_warn!(
                    "calculate_cost: model '{model}' not in registry — \
                     no cost fields to read; reporting 0 (telemetry only, not dispatch)"
                );
                (0.0, 0.0)
            }
        };

        (input_tokens as f64 / 1000.0) * input_cost + (output_tokens as f64 / 1000.0) * output_cost
    }
}
