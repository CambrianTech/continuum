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

use super::adapter::{AIProviderAdapter, AdapterCapabilities, ApiStyle, GenerationChunk};
use super::stream_sinks::GenerationSink;
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
            .connect_timeout(std::time::Duration::from_secs(120))
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

    /// Encode every part or return an explicit transport error; never discard media.
    fn format_messages(messages: &[ChatMessage]) -> Result<(Vec<Value>, Option<String>), String> {
        let mut result = Vec::new();
        let mut system_prompt = None;
        for msg in messages {
            if msg.role == "system" {
                if let MessageContent::Parts(parts) = &msg.content {
                    if parts
                        .iter()
                        .any(|part| !matches!(part, ContentPart::Text { .. }))
                    {
                        return Err("Anthropic system transport supports text only; non-text content was not sent".into());
                    }
                }
                system_prompt = Some(msg.content_text());
                continue;
            }
            let role = if msg.role == "assistant" {
                "assistant"
            } else {
                "user"
            };
            let content = match &msg.content {
                MessageContent::Text(text) => json!(text),
                MessageContent::Parts(parts) => {
                    let mut content = Vec::with_capacity(parts.len());
                    for part in parts {
                        content.push(match part {
                            ContentPart::Text { text } => json!({"type":"text", "text":text}),
                            ContentPart::Image { image } => {
                                let source = if let Some(data) = &image.base64 {
                                    if data.is_empty() {
                                        return Err("Anthropic image base64 is empty".into());
                                    }
                                    let mime = image.mime_type.as_deref().filter(|mime| !mime.is_empty())
                                        .ok_or("Anthropic base64 image requires an explicit MIME type")?;
                                    json!({"type":"base64", "media_type":mime, "data":data})
                                } else {
                                    let url = image.url.as_deref().filter(|url| !url.is_empty())
                                        .ok_or("Anthropic image requires a nonempty source")?;
                                    json!({"type":"url", "url":url})
                                };
                                json!({"type":"image", "source":source})
                            }
                            ContentPart::Audio { .. } => return Err("Anthropic adapter has no native audio input transport; audio was not sent".into()),
                            ContentPart::Video { .. } => return Err("Anthropic adapter has no native video input transport; video was not sent".into()),
                            ContentPart::ToolUse { id, name, input } => json!({"type":"tool_use", "id":id, "name":name, "input":input}),
                            ContentPart::ToolResult { tool_use_id, content, is_error } => {
                                let mut block = json!({"type":"tool_result", "tool_use_id":tool_use_id, "content":content});
                                if let Some(is_error) = is_error {
                                    block["is_error"] = json!(is_error);
                                }
                                block
                            }
                        });
                    }
                    json!(content)
                }
            };
            result.push(json!({"role":role, "content":content}));
        }
        Ok((result, system_prompt))
    }
    /// Map Anthropic stop reason to our enum
    fn map_finish_reason(&self, reason: &str) -> FinishReason {
        match reason {
            "end_turn" | "stop_sequence" | "pause_turn" => FinishReason::Stop,
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
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
    #[serde(rename = "redacted_thinking")]
    RedactedThinking,
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

/// Messages SSE owns indexed content assembly; deltas reach the existing sink
/// before message_stop. Tools remain structured and reasoning never becomes text.
#[derive(Default)]
struct AnthropicStream {
    frame: Vec<u8>,
    message: Option<Value>,
    active: Option<usize>,
    tool_json: String,
    stopped: bool,
}

impl AnthropicStream {
    fn feed(&mut self, bytes: &[u8], sink: &GenerationSink) -> Result<(), String> {
        for &byte in bytes {
            if byte == b'\r' {
                continue;
            }
            self.frame.push(byte);
            if self.frame.len() > super::stream_sinks::MAX_GENERATION_CHUNK_BYTES * 2 {
                return Err("Anthropic SSE event exceeds shared stream byte budget".into());
            }
            if !self.frame.ends_with(b"\n\n") {
                continue;
            }
            let frame = std::mem::take(&mut self.frame);
            let frame =
                std::str::from_utf8(&frame).map_err(|e| format!("Anthropic SSE UTF-8: {e}"))?;
            let data = frame
                .lines()
                .filter_map(|line| line.strip_prefix("data:").map(str::trim_start))
                .collect::<Vec<_>>()
                .join("\n");
            if data.is_empty() {
                continue;
            }
            let event: Value =
                serde_json::from_str(&data).map_err(|e| format!("Anthropic SSE JSON: {e}"))?;
            self.event(event, sink)?;
            if self.stopped {
                break;
            }
        }
        Ok(())
    }

    fn event(&mut self, event: Value, sink: &GenerationSink) -> Result<(), String> {
        let kind = event["type"]
            .as_str()
            .ok_or("Anthropic SSE missing event type")?;
        if kind == "error" {
            return Err(format!("Anthropic stream error: {}", event["error"]));
        }
        if kind == "message_start" {
            if self.message.is_some() {
                return Err("Anthropic duplicate message_start".into());
            }
            let mut message = event["message"].clone();
            if message["model"]
                .as_str()
                .filter(|m| !m.is_empty())
                .is_none()
            {
                return Err("Anthropic message_start missing model identity".into());
            }
            message["content"] = json!([]);
            self.message = Some(message);
            return Ok(());
        }
        if kind == "ping" {
            return Ok(());
        }
        let message = self
            .message
            .as_mut()
            .ok_or("Anthropic event before message_start")?;
        match kind {
            "content_block_start" => {
                let index = event["index"]
                    .as_u64()
                    .ok_or("Anthropic content index missing")? as usize;
                let blocks = message["content"]
                    .as_array_mut()
                    .ok_or("Anthropic content is not an array")?;
                if self.active.is_some() || index != blocks.len() {
                    return Err("Anthropic out-of-order content block".into());
                }
                let block = event["content_block"].clone();
                match block["type"].as_str() {
                    Some("text") => {
                        let text = block["text"]
                            .as_str()
                            .ok_or("Anthropic text block missing text")?;
                        if !text.is_empty() {
                            sink.send(GenerationChunk::Token(text.into()))?;
                        }
                    }
                    Some("thinking") => {
                        let text = block["thinking"].as_str().unwrap_or_default();
                        if !text.is_empty() {
                            sink.send(GenerationChunk::Reasoning(text.into()))?;
                        }
                    }
                    Some("tool_use" | "redacted_thinking") => {}
                    _ => {
                        return Err(
                            "Anthropic unsupported content block; refusing to discard output"
                                .into(),
                        )
                    }
                }
                blocks.push(block);
                self.active = Some(index);
                self.tool_json.clear();
            }
            "content_block_delta" | "content_block_stop" => {
                let index = event["index"]
                    .as_u64()
                    .ok_or("Anthropic content index missing")? as usize;
                if self.active != Some(index) {
                    return Err("Anthropic delta/stop has no matching content block".into());
                }
                let block = &mut message["content"][index];
                if kind == "content_block_stop" {
                    if !self.tool_json.is_empty() {
                        block["input"] = serde_json::from_str(&self.tool_json)
                            .map_err(|e| format!("Anthropic incomplete tool JSON: {e}"))?;
                    }
                    self.active = None;
                    self.tool_json.clear();
                } else {
                    let delta = &event["delta"];
                    let (field, chunk) = match delta["type"].as_str() {
                        Some("text_delta") if block["type"] == "text" => ("text", Some(false)),
                        Some("thinking_delta") if block["type"] == "thinking" => {
                            ("thinking", Some(true))
                        }
                        Some("signature_delta") if block["type"] == "thinking" => {
                            ("signature", None)
                        }
                        Some("input_json_delta") if block["type"] == "tool_use" => {
                            self.tool_json.push_str(
                                delta["partial_json"]
                                    .as_str()
                                    .ok_or("Anthropic tool delta missing JSON")?,
                            );
                            return Ok(());
                        }
                        // Citation metadata does not change the text content.
                        Some("citations_delta") if block["type"] == "text" => return Ok(()),
                        _ => return Err("Anthropic unsupported or mismatched content delta".into()),
                    };
                    let text = delta[field]
                        .as_str()
                        .ok_or("Anthropic delta missing content")?;
                    if let Some(reasoning) = chunk {
                        if !text.is_empty() {
                            sink.send(if reasoning {
                                GenerationChunk::Reasoning(text.into())
                            } else {
                                GenerationChunk::Token(text.into())
                            })?;
                        }
                    }
                    let mut accumulated = block[field].as_str().unwrap_or_default().to_owned();
                    accumulated.push_str(text);
                    block[field] = json!(accumulated);
                }
            }
            "message_delta" => {
                if let Some(reason) = event["delta"]["stop_reason"].as_str() {
                    message["stop_reason"] = json!(reason);
                }
                if let Some(usage) = event["usage"].as_object() {
                    for (key, value) in usage {
                        message["usage"][key] = value.clone();
                    }
                }
            }
            "message_stop" => {
                if self.active.is_some() || message["stop_reason"].as_str().is_none() {
                    return Err(
                        "Anthropic stopped with unfinished content or missing stop reason".into(),
                    );
                }
                self.stopped = true;
            }
            // Forward-compatible non-content events do not replace a terminal receipt.
            _ => {}
        }
        Ok(())
    }

    fn finish(self) -> Result<AnthropicResponse, String> {
        if !self.stopped {
            return Err("Anthropic stream ended before message_stop".into());
        }
        serde_json::from_value(self.message.ok_or("Anthropic stream has no message")?)
            .map_err(|e| format!("Anthropic streamed message invalid: {e}"))
    }
}

async fn receive_anthropic_stream(
    mut response: reqwest::Response,
    sink: &GenerationSink,
) -> Result<AnthropicResponse, String> {
    let mut stream = AnthropicStream::default();
    loop {
        let next = tokio::select! {
            biased;
            _ = sink.closed() => return Err("Anthropic stream consumer cancelled".into()),
            next = tokio::time::timeout(
                std::time::Duration::from_secs(crate::inference::sse_stream::STREAM_IDLE_TIMEOUT_SECS),
                response.chunk()) => next.map_err(|_| "Anthropic stream stalled".to_string())?
                    .map_err(|e| format!("Anthropic stream read failed: {e}"))?,
        };
        let Some(bytes) = next else {
            break;
        };
        stream.feed(&bytes, sink)?;
        if stream.stopped {
            break;
        }
    }
    stream.finish()
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
        self.generate_stream(request, GenerationSink::discard())
            .await
    }

    async fn generate_stream(
        &self,
        request: TextGenerationRequest,
        sink: GenerationSink,
    ) -> Result<TextGenerationResponse, String> {
        request.require_text_output_transport(self.provider_id())?;
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
        let (messages, msg_system) = Self::format_messages(&request.messages)?;
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
            return Err(
                "Anthropic requires max_tokens and this adapter declares no \
                        max_output_tokens capability — declare it via \
                        AdapterCapabilities::builder().max_output_tokens(n)"
                    .to_string(),
            );
        };

        // Build request body
        let mut body = json!({
            "model": model,
            "stream": true,
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
        let send = self
            .client
            .post("https://api.anthropic.com/v1/messages")
            .header("x-api-key", api_key)
            .header("anthropic-version", "2023-06-01")
            .header("Content-Type", "application/json")
            .json(&body)
            .send();
        let response = tokio::select! {
            biased;
            _ = sink.closed() => return Err("Anthropic request consumer cancelled".into()),
            response = tokio::time::timeout(std::time::Duration::from_secs(120), send) =>
                response.map_err(|_| "Anthropic response headers timed out".to_string())?
                    .map_err(|e| format!("Anthropic request failed: {e}"))?,
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = tokio::time::timeout(std::time::Duration::from_secs(5), response.text())
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or_default();
            return Err(format!("Anthropic returned {}: {}", status, body));
        }

        let response_json = receive_anthropic_stream(response, &sink).await?;

        let response_time_ms = start.elapsed().as_millis() as u64;

        // Parse response content blocks
        let mut text = String::new();
        let mut tool_calls = Vec::new();
        let mut content_blocks = Vec::new();
        let mut reasoning = String::new();

        for block in &response_json.content {
            match block {
                AnthropicContentBlock::Thinking { thinking } => reasoning.push_str(thinking),
                AnthropicContentBlock::RedactedThinking => {}
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

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches (native-stream review): text arrives before terminal receipt,
    // UTF-8 survives packet splits, tool JSON is reassembled, and thinking stays private.
    #[test]
    fn messages_stream_delivers_incrementally_and_preserves_structured_output() {
        let (sink, mut receiver) = super::super::stream_sinks::channel();
        let mut stream = AnthropicStream::default();
        let events = [
            json!({"type":"message_start", "message":{"id":"m1","model":"claude-test","usage":{"input_tokens":7,"output_tokens":0}}}),
            json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}),
            json!({"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"h\u{e9}llo"}}),
        ];
        for event in events {
            let frame = format!(
                "event: {}\r\ndata: {event}\r\n\r\n",
                event["type"].as_str().unwrap()
            );
            for byte in frame.as_bytes() {
                stream.feed(&[*byte], &sink).unwrap();
            }
        }
        assert_eq!(
            receiver.try_recv().unwrap(),
            GenerationChunk::Token("h\u{e9}llo".into())
        );
        assert!(!stream.stopped);
        for event in [
            json!({"type":"content_block_stop","index":0}),
            json!({"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"t1","name":"inspect","input":{}}}),
            json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"path\":"}}),
            json!({"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"\"src\"}"}}),
            json!({"type":"content_block_stop","index":1}),
            json!({"type":"content_block_start","index":2,"content_block":{"type":"thinking","thinking":""}}),
            json!({"type":"content_block_delta","index":2,"delta":{"type":"thinking_delta","thinking":"fixture-private"}}),
            json!({"type":"content_block_delta","index":2,"delta":{"type":"signature_delta","signature":"signed"}}),
            json!({"type":"content_block_stop","index":2}),
            json!({"type":"message_delta","delta":{"stop_reason":"tool_use"},"usage":{"output_tokens":9}}),
            json!({"type":"message_stop"}),
        ] {
            stream
                .feed(format!("data: {event}\n\n").as_bytes(), &sink)
                .unwrap();
        }
        assert_eq!(
            receiver.try_recv().unwrap(),
            GenerationChunk::Reasoning("fixture-private".into())
        );
        assert!(receiver.try_recv().is_err());
        let response = stream.finish().unwrap();
        assert_eq!(response.model, "claude-test");
        assert_eq!(response.stop_reason.as_deref(), Some("tool_use"));
        let usage = response.usage.unwrap();
        assert_eq!((usage.input_tokens, usage.output_tokens), (7, 9));
        match &response.content[1] {
            AnthropicContentBlock::ToolUse { id, name, input } => {
                assert_eq!((id.as_str(), name.as_str()), ("t1", "inspect"));
                assert_eq!(input, &json!({"path":"src"}));
            }
            _ => panic!("structured tool lost"),
        }
    }

    // what this catches: truncated/error streams and cancelled consumers cannot report a complete answer.
    #[test]
    fn messages_stream_rejects_incomplete_error_and_cancelled_delivery() {
        let sink = GenerationSink::discard();
        assert!(AnthropicStream::default()
            .finish()
            .unwrap_err()
            .contains("message_stop"));
        assert!(AnthropicStream::default()
            .feed(
                b"data: {\"type\":\"error\",\"error\":{\"type\":\"overloaded_error\"}}\n\n",
                &sink
            )
            .unwrap_err()
            .contains("overloaded_error"));
        let mut stream = AnthropicStream::default();
        stream
            .event(
                json!({"type":"message_start","message":{"id":"m","model":"test"}}),
                &sink,
            )
            .unwrap();
        stream.event(json!({"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"t","name":"f","input":{}}}), &sink).unwrap();
        stream.event(json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{"}}), &sink).unwrap();
        assert!(stream
            .event(json!({"type":"content_block_stop","index":0}), &sink)
            .is_err());
        let (sink, receiver) = super::super::stream_sinks::channel();
        drop(receiver);
        let mut stream = AnthropicStream::default();
        stream
            .event(
                json!({"type":"message_start","message":{"id":"m","model":"test"}}),
                &sink,
            )
            .unwrap();
        assert!(stream.event(json!({"type":"content_block_start","index":0,"content_block":{"type":"text","text":"late"}}), &sink).is_err());
    }

    // Every part must survive in order or fail explicitly before network dispatch,
    // including media beside tool blocks and non-text system content.
    #[test]
    fn native_media_is_preserved_or_rejected_never_dropped() {
        let message = |role: &str, parts: Value| -> ChatMessage {
            serde_json::from_value(json!({"role":role,"content":parts})).unwrap()
        };
        let image = json!({"type":"image","image":{"base64":"aW1hZ2U=","mimeType":"image/jpeg"}});
        let tool = json!({"type":"tool_result","tool_use_id":"t1","content":"done"});
        let input = message(
            "user",
            json!([tool, image, {"type":"text","text":"inspect"}]),
        );
        let (wire, _) = AnthropicAdapter::format_messages(&[input]).unwrap();
        assert_eq!(wire[0]["content"][0]["type"], "tool_result");
        assert_eq!(wire[0]["content"][1]["source"]["data"], "aW1hZ2U=");
        assert_eq!(wire[0]["content"][1]["source"]["media_type"], "image/jpeg");
        assert_eq!(wire[0]["content"][2]["text"], "inspect");
        assert_eq!(wire[0]["content"].as_array().unwrap().len(), 3);

        for part in [
            json!({"type":"audio","audio":{"base64":"YXVkaW8=","mimeType":"audio/wav"}}),
            json!({"type":"video","video":{"url":"https://example.invalid/video"}}),
            json!({"type":"image","image":{}}),
            json!({"type":"image","image":{"base64":"","mimeType":"image/jpeg"}}),
            json!({"type":"image","image":{"base64":"aW1hZ2U="}}),
        ] {
            assert!(AnthropicAdapter::format_messages(&[message("user", json!([part]))]).is_err());
        }
        assert!(AnthropicAdapter::format_messages(&[message("system", json!([image]))]).is_err());
        let (wire, _) = AnthropicAdapter::format_messages(&[message(
            "user",
            json!([
                {"type":"image","image":{"url":"https://example.invalid/image.jpg"}}
            ]),
        )])
        .unwrap();
        assert_eq!(
            wire[0]["content"][0]["source"]["url"],
            "https://example.invalid/image.jpg"
        );
    }
}
