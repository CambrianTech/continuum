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

use crate::model_registry::{AuthKind, Capability};
use crate::secrets::get_secret;
use crate::{clog_info, clog_warn};

use super::adapter::{AIProviderAdapter, AdapterCapabilities, ApiStyle};
use super::registry_bridge::models_for_provider_via_registry;
use super::types::{
    ChatMessage, ContentPart, EmbeddingInput, EmbeddingRequest, EmbeddingResponse, FinishReason,
    HealthState, HealthStatus, MessageContent, ModelInfo, TextGenerationRequest,
    TextGenerationResponse, ToolCall, ToolChoice, UsageMetrics,
};

/// Runtime-resolved config carried by each `OpenAICompatibleAdapter`
/// instance. Populated exclusively by `OpenAICompatibleAdapter::from_registry`
/// — no hand-written literals. Fields that the registry doesn't know
/// about (HTTP concerns — auth shape, Authorization header requirement)
/// are derived from `Provider.auth`, not separately configured.
#[derive(Debug, Clone)]
pub struct OpenAICompatibleConfig {
    pub provider_id: String,
    pub name: String,
    pub base_url: String,
    pub api_key_env: Option<String>,
    pub default_model: String,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub models: Vec<ModelInfo>,
    pub model_prefixes: Vec<String>,
    /// Whether this provider requires an Authorization header. Derived
    /// from `Provider.auth`: Bearer → true, ApiKey → true, None → false.
    pub requires_auth: bool,
    /// How this provider exchanges tool calls. Native = real OpenAI
    /// function-calling; JsonInPrompt = describe tools in the prompt + parse the
    /// JSON call from the response (for gateways/models that ignore the `tools`
    /// param, e.g. unsloth+GGUF — proven 2026-06-21). Gateway-level for now;
    /// per-model refinement (from the gateway's model data) is the follow-up.
    pub tool_protocol: super::json_in_prompt_tools::ToolProtocol,
}

/// OpenAI-compatible adapter implementation
pub struct OpenAICompatibleAdapter {
    config: OpenAICompatibleConfig,
    api_key: Option<String>,
    /// Runtime base URL set via `with_runtime_base_url` — overrides
    /// `config.base_url` without mutating the registry-sourced config.
    /// Used when DMR reaches us at `model-runner.docker.internal` instead
    /// of `localhost:12434` (detected by `probe_dmr`).
    runtime_base_url: Option<String>,
    client: reqwest::Client,
    initialized: bool,
    /// Live model catalog, populated from the server's /v1/models endpoint
    /// at init and on-demand refresh. Lets `supports_model()` be HONEST —
    /// for DMR this reflects whatever the user has `docker model pull`ed,
    /// so the registry can route to DMR only when the model is actually
    /// available. Without this, supports_model falls back to static
    /// `supported_model_prefixes()` which for docker-model-runner returned
    /// `[]` → DMR never won routing → every user silently landed on Candle.
    runtime_models: std::sync::Arc<std::sync::RwLock<Option<std::collections::HashSet<String>>>>,
    /// Throttle for concurrent POSTs to this provider's endpoint.
    /// llama.cpp-backed providers (DMR) are single-slot in practice:
    /// one prompt at a time gets the full GPU. Letting N personas
    /// fan-out into N simultaneous POSTs causes each to serialize on
    /// DMR's side while reqwest's 120s client timeout burns. This
    /// semaphore does the same serialization CLIENT-side so requests
    /// wait in an observable queue instead of inside reqwest's
    /// opaque "no response yet" state, and so the adapter's 120s
    /// timeout is measured from "actually reached the server," not
    /// "joined the queue."
    ///
    /// DMR → 1 slot (single-slot llama.cpp backend).
    /// Cloud providers (OpenAI / Groq / etc.) → high slot count (no throttle).
    concurrency: std::sync::Arc<tokio::sync::Semaphore>,
}

impl OpenAICompatibleAdapter {
    pub fn new(config: OpenAICompatibleConfig) -> Self {
        // 120s total timeout bounds long generations (qwen3.5 reasoning
        // can take ~60s to emit a full response). Connect timeout bounds
        // the local-loopback DMR case specifically: when Docker Desktop
        // restarts or DMR isn't listening, we want the fast explicit
        // "connect refused" instead of a 120s stall. Idle timeout keeps
        // the reqwest pool from holding onto dead sockets across DMR
        // restarts — a stale pooled connection to a killed server was
        // the reproducing cause of 120s "error sending request" stalls.
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(120))
            .connect_timeout(std::time::Duration::from_secs(3))
            .pool_idle_timeout(std::time::Duration::from_secs(30))
            .build()
            .expect("Failed to create HTTP client");

        // Per-provider concurrency gate. DMR = 1 slot (single-slot
        // llama.cpp). Everyone else = effectively unbounded. When N
        // personas fan-out into concurrent DMR POSTs, the excess
        // queue in this semaphore INSTEAD of stalling inside reqwest
        // past its 120s client timeout — which is the specific
        // failure mode where personas emitted "error sending request
        // for url -> operation timed out" with connect=false (the
        // request reached DMR, but DMR was busy on the prior
        // persona's forward pass when its 120s budget expired).
        let slots = if config.provider_id == "docker-model-runner" {
            1
        } else {
            64
        };
        let concurrency = std::sync::Arc::new(tokio::sync::Semaphore::new(slots));

        Self {
            config,
            api_key: None,
            runtime_base_url: None,
            client,
            initialized: false,
            runtime_models: std::sync::Arc::new(std::sync::RwLock::new(None)),
            concurrency,
        }
    }

    /// Override the base URL at runtime (e.g. when running inside a Docker
    /// container on Windows/Linux where DMR is at model-runner.docker.internal
    /// instead of localhost:12434). Called post-construction, before init.
    pub fn with_runtime_base_url(mut self, url: String) -> Self {
        self.runtime_base_url = Some(url);
        self
    }

    /// The host root that request URLs append `/v1/...` to — the runtime override
    /// if set, else the configured base. ONE resolution point for all request
    /// sites (was copy-pasted 4×). The value is already canonical (bare host): the
    /// catalog stores bare hosts, and the unsloth gateway is registered with the
    /// single [`unsloth_base_url`](crate::inference::unsloth_control::unsloth_base_url)
    /// accessor — so no per-site normalization is needed or wanted here.
    fn api_base(&self) -> &str {
        self.runtime_base_url
            .as_deref()
            .unwrap_or(self.config.base_url.as_str())
    }

    /// Fetch the live model list from the provider's /v1/models endpoint.
    /// Used by adapters that have dynamic catalogs (DMR above all — the list
    /// changes every time the user runs `docker model pull`). Populates
    /// `runtime_models` on success; leaves it unchanged on failure so stale
    /// data is preferred over empty data. Never silently succeeds with an
    /// empty set — returns Err if the endpoint responds with nothing.
    async fn refresh_runtime_models(&self) -> Result<(), String> {
        let base_url = self.api_base();
        let url = format!("{}/v1/models", base_url);

        let mut req = self.client.get(&url);
        if let Some(ref key) = self.api_key {
            req = req.bearer_auth(key);
        }
        let resp = req
            .send()
            .await
            .map_err(|e| format!("GET {} failed: {}", url, e))?;
        if !resp.status().is_success() {
            return Err(format!("GET {} returned {}", url, resp.status()));
        }
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("Parse {} body: {}", url, e))?;
        let ids: std::collections::HashSet<String> = body
            .get("data")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|m| m.get("id").and_then(|i| i.as_str()).map(String::from))
                    .collect()
            })
            .unwrap_or_default();
        if ids.is_empty() {
            return Err(format!("{} returned no models", url));
        }
        *self.runtime_models.write().unwrap() = Some(ids);
        Ok(())
    }

    /// Resolve a logical model name to the actual DMR model ID stored in
    /// the runtime catalog. Returns the owned resolved ID on match, or an
    /// Err describing what the caller asked for vs what DMR actually has
    /// — no fallback to the raw name (DMR would just 404 on it).
    ///
    /// On cache miss (either an empty cache or a populated cache that
    /// doesn't contain the needle) this forces a single
    /// `refresh_runtime_models` and retries the lookup once. That covers
    /// the common case: the user ran `docker model pull` after the
    /// adapter initialized, so the forged model exists in DMR but not in
    /// our stale in-memory set.
    async fn resolve_dmr_model_name(&self, model_name: &str) -> Result<String, String> {
        if let Some(hit) = self.lookup_runtime_model(model_name) {
            return Ok(hit);
        }
        // Cache miss — refresh once, then retry. If refresh itself fails
        // we surface that error; if the needle still isn't there we
        // hard-error with the full available set so the log makes the
        // mismatch obvious (e.g. persona asked for "-GGUF" but DMR stores
        // "...-gguf:latest").
        self.refresh_runtime_models().await?;
        if let Some(hit) = self.lookup_runtime_model(model_name) {
            return Ok(hit);
        }
        let available: Vec<String> = self
            .runtime_models
            .read()
            .unwrap()
            .as_ref()
            .map(|ids| ids.iter().cloned().collect())
            .ok_or_else(|| "DMR runtime_models still empty after refresh".to_string())?;
        Err(format!(
            "DMR does not have model '{}'. Available: {:?}. Pull it with: docker model pull <id>",
            model_name, available
        ))
    }

    /// Pure lookup against the cached runtime_models set. Same matching
    /// rules as `runtime_models_contain`: case-insensitive exact or
    /// trivial contains in either direction. No I/O, no refresh — callers
    /// own the refresh decision.
    fn lookup_runtime_model(&self, model_name: &str) -> Option<String> {
        let guard = self.runtime_models.read().unwrap();
        let ids = guard.as_ref()?;
        let needle = model_name.to_lowercase();
        ids.iter()
            .find(|id| {
                let hay = id.to_lowercase();
                hay == needle || hay.contains(&needle) || needle.contains(&hay)
            })
            .cloned()
    }

    /// Returns true if model_name matches any live runtime model.
    /// Match is exact OR a trivial contains in either direction to
    /// handle the common "persona says short name, DMR stores full
    /// hf.co/…-GGUF ID" pattern. No fuzzy magic beyond that — if neither
    /// contains the other, the adapter honestly does not have the model.
    fn runtime_models_contain(&self, model_name: &str) -> bool {
        let guard = self.runtime_models.read().unwrap();
        match guard.as_ref() {
            None => false, // not populated — can't lie, return false
            Some(ids) => {
                let needle = model_name.to_lowercase();
                ids.iter().any(|id| {
                    let hay = id.to_lowercase();
                    hay == needle || hay.contains(&needle) || needle.contains(&hay)
                })
            }
        }
    }

    /// Build an adapter for `provider_id` by reading everything from the
    /// model_registry. Replaces eight hand-rolled factories whose combined
    /// bulk was ~280 LOC of `ModelInfo { ... }` literals that drifted
    /// whenever a new model shipped. Now the TOML is the only place a
    /// new model's context_window / capabilities / pricing lives.
    ///
    /// Panics if the provider isn't in the registry — that's a boot-time
    /// config bug, not a runtime condition (per the no-fallback rule).
    ///
    /// Capability flags (`supports_tools`, `supports_vision`) are derived
    /// from whether ANY model under this provider advertises the relevant
    /// Capability. A new Vision-capable model showing up in TOML flips
    /// the adapter's vision flag automatically on next boot — no code
    /// change.
    pub fn from_registry(provider_id: &str) -> Self {
        let reg = crate::model_registry::global();
        let provider = reg.provider(provider_id).unwrap_or_else(|| {
            panic!(
                "provider `{}` not in config/providers.toml — can't build \
                 OpenAICompatibleAdapter",
                provider_id
            )
        });

        let models = models_for_provider_via_registry(provider_id);
        let supports_tools = reg
            .models_for_provider(provider_id)
            .any(|m| m.has(Capability::ToolUse));
        let supports_vision = reg
            .models_for_provider(provider_id)
            .any(|m| m.has(Capability::Vision));
        let requires_auth = !matches!(provider.auth, AuthKind::None);

        // `default_model` is non-optional in the adapter trait
        // (`fn default_model(&self) -> &str`) — callers always get a
        // concrete id back. Providers with genuinely dynamic catalogs
        // (DMR) still declare a default id the user is most likely to
        // want; operator overrides flow through explicit request.model.
        // Panic if missing: the registry row is incomplete, not a runtime
        // condition.
        let default_model = provider.default_model.clone().unwrap_or_else(|| {
            panic!(
                "provider `{}` has no `default_model` in config/providers.toml — \
                 every OpenAI-compatible adapter needs one because the trait \
                 returns &str, not Option<&str>",
                provider_id
            )
        });

        Self::new(OpenAICompatibleConfig {
            provider_id: provider.id.clone(),
            name: provider.display_name().to_string(),
            base_url: provider.base_url.clone(),
            api_key_env: provider.api_key_env.clone(),
            default_model,
            supports_tools,
            supports_vision,
            models,
            model_prefixes: provider.model_prefixes.clone(),
            requires_auth,
            // unsloth's GGUF path ignores the OpenAI `tools` param (proven), so it
            // uses prompt-based tools; cloud OpenAI-compatible providers do native
            // function-calling. Keyed by GATEWAY, not a specific model — refine to
            // per-model from the gateway's model capabilities later.
            tool_protocol: if provider.id == "unsloth" {
                super::json_in_prompt_tools::ToolProtocol::JsonInPrompt
            } else {
                super::json_in_prompt_tools::ToolProtocol::Native
            },
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
                    let has_tool_use = parts
                        .iter()
                        .any(|p| matches!(p, ContentPart::ToolUse { .. }));
                    let has_tool_result = parts
                        .iter()
                        .any(|p| matches!(p, ContentPart::ToolResult { .. }));

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
                            if let ContentPart::ToolResult {
                                tool_use_id,
                                content,
                                ..
                            } = part
                            {
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
                                    } else {
                                        image.base64.as_ref().map(|b64| json!({
                                            "type": "image_url",
                                            "image_url": {
                                                "url": format!("data:{};base64,{}",
                                                    image.mime_type.as_deref().unwrap_or("image/png"), b64)
                                            }
                                        }))
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
        &self.config.provider_id
    }

    fn name(&self) -> &str {
        &self.config.name
    }

    fn capabilities(&self) -> AdapterCapabilities {
        let supports_tools = self.config.supports_tools;
        let supports_vision = self.config.supports_vision;
        AdapterCapabilities {
            supports_text_generation: true,
            supports_chat: true,
            supports_tool_use: supports_tools,
            supports_vision,
            supports_streaming: true,
            // Embeddings: OpenAI itself, plus unsloth — the universal model
            // gateway exposes OpenAI-compatible `/v1/embeddings`, so continuum's
            // neural recall path embeds through it (UNSLOTH-INTEGRATION.md). This
            // is what lets us delete the local fastembed/ONNX embedder.
            supports_embeddings: matches!(self.config.provider_id.as_str(), "openai" | "unsloth"),
            supports_audio: false,
            supports_image_generation: self.config.provider_id == "openai",
            is_local: false,
            max_context_window: self
                .config
                .models
                .first()
                .map(|m| m.context_window)
                .unwrap_or(128000),

            // Arc 1: OpenAI-compatible providers (OpenAI, DeepSeek, Together,
            // Fireworks, Groq, xAI, Mistral) support native function calling
            // when supports_tools is set, and JSON Schema for structured output.
            // Vision-in when supports_vision is set; rest goes through bridges.
            tool_call_protocol: if supports_tools {
                crate::ai::adapter::ToolCallProtocol::NativeFunctionCalling
            } else {
                crate::ai::adapter::ToolCallProtocol::None
            },
            structured_output_protocol: if supports_tools {
                crate::ai::adapter::StructuredOutputProtocol::JsonSchema
            } else {
                crate::ai::adapter::StructuredOutputProtocol::PromptOnly
            },
            modalities: crate::ai::adapter::ModalitySet {
                text_in: true,
                text_out: true,
                vision_in: supports_vision,
                audio_in: false,
                audio_out: false,
            },
            max_output_tokens: 16_384,
        }
    }

    fn api_style(&self) -> ApiStyle {
        ApiStyle::OpenAI
    }

    fn default_model(&self) -> &str {
        &self.config.default_model
    }

    async fn initialize(&mut self) -> Result<(), String> {
        // Only require API key if provider needs auth. Providers without
        // an `api_key_env` in TOML (localhost DMR, llamacpp-local) skip
        // this entirely — their `requires_auth` is false.
        if self.config.requires_auth {
            let key_env = self.config.api_key_env.as_deref().unwrap_or_else(|| {
                panic!(
                    "provider `{}` requires auth but has no api_key_env in TOML",
                    self.config.provider_id
                )
            });
            self.api_key = get_secret(key_env).map(|s| s.to_string());
            if self.api_key.is_none() {
                return Err(format!(
                    "{} API key not configured ({})",
                    self.config.name, key_env
                ));
            }
        }

        self.initialized = true;

        // Populate runtime_models for adapters with dynamic catalogs (DMR).
        // Best-effort: if the endpoint isn't reachable right now, init still
        // succeeds — runtime_models stays None → supports_model returns false
        // → registry hard-errors instead of silently routing to this adapter.
        // That's the correct failure mode: don't falsely claim availability.
        if self.config.provider_id == "docker-model-runner" {
            if let Err(e) = self.refresh_runtime_models().await {
                clog_warn!(
                    "DMR model catalog fetch failed at init: {}. DMR will report no models available until a successful refresh.",
                    e
                );
            } else {
                let count = self
                    .runtime_models
                    .read()
                    .unwrap()
                    .as_ref()
                    .map(|s| s.len())
                    .unwrap_or(0);
                clog_info!("DMR live model catalog: {} model(s) available", count);
            }
        }

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
        // Only require API key for providers that need auth
        if self.config.requires_auth && self.api_key.is_none() {
            return Err(format!("{} not initialized", self.config.name));
        }

        let start = Instant::now();
        let request_id = request
            .request_id
            .clone()
            .unwrap_or_else(|| format!("req-{}", chrono::Utc::now().timestamp_millis()));
        let raw_model = request
            .model
            .as_deref()
            .unwrap_or(self.config.default_model.as_str());

        // For DMR: resolve the logical model name to the actual model ID
        // stored in Docker Model Runner (which may have hf.co/ prefix and
        // different casing). Persona says "continuum-ai/qwen3.5-4b-code-forged-GGUF",
        // DMR has "huggingface.co/continuum-ai/qwen3.5-4b-code-forged-gguf:latest".
        // If DMR doesn't have the model, resolve returns Err — we propagate
        // it as a fast, explicit failure instead of POSTing an unresolved
        // name and stalling on the 120s request timeout.
        let resolved_model: String = if self.config.provider_id == "docker-model-runner" {
            self.resolve_dmr_model_name(raw_model).await?
        } else {
            raw_model.to_string()
        };
        let model: &str = &resolved_model;

        // Build request body
        let mut messages = self.format_messages(&request.messages, request.system_prompt.as_deref());

        // JsonInPrompt tool offering: for gateways/models that ignore the OpenAI
        // `tools` param (unsloth+GGUF), describe the tools IN the prompt and ask
        // for a strict JSON call. Appended as a system message; the matching parse
        // happens on the response below. Native providers skip this (tool_prompt →
        // None) and use the `tools` param instead.
        if let Some(tools) = request.tools.as_ref() {
            if let Some(block) = self.config.tool_protocol.tool_prompt(tools) {
                messages.push(json!({ "role": "system", "content": block }));
            }
        }

        let mut body = json!({
            "model": model,
            "messages": messages,
            "temperature": request.temperature.unwrap_or(0.7),
            "max_tokens": request.max_tokens.unwrap_or(2048),
            "stream": false
        });

        // DMR-specific: llama.cpp's OpenAI-compatible server accepts the
        // llama.cpp-native `repeat_penalty` field as an extension. Until
        // this patch the POST body shipped ONLY the 5 fields above, so
        // DMR inference ran with repeat_penalty=1.0 (llama.cpp default,
        // disabled) and produced runaway repetition — empirically verified
        // 2026-04-24 on Linux/CUDA Carl stack: qwen3.5-4b-code-forged
        // reprinted the same <think> paragraph 10-40 times then burned
        // max_tokens without emitting a real reply. Meanwhile the
        // in-process llamacpp_adapter path defaults
        // `sampling.repeat_penalty = 1.1` (backends/mod.rs:195,205) and
        // does NOT exhibit this failure mode on Mac Metal. Classic RULE 1
        // divergence (integration test path ≠ production path).
        //
        // Scoped to docker-model-runner ONLY because cloud OpenAI-compat
        // providers (openai, groq, xai, fireworks, together) do NOT accept
        // `repeat_penalty` (non-standard field); some ignore it silently,
        // others reject. Behavior parity with pre-patch for those
        // providers is preserved by gating on provider_id.
        if self.config.provider_id == "docker-model-runner" {
            let rp = request.repeat_penalty.unwrap_or(1.1);
            if let Some(obj) = body.as_object_mut() {
                obj.insert("repeat_penalty".to_string(), json!(rp));
            }
        }

        // Forward response_format when set. Llama.cpp/DMR DO grammar-constrain
        // JSON output, but for qwen3.5 reasoning models the model still
        // emits its <think> reasoning BEFORE the constrained JSON region,
        // which is no help to a JSON parser. Verified empirically 2026-04-19:
        // `response_format=json_object` alone returns "<think>\nThinking
        // Process:..." with no JSON.
        if let Some(format) = &request.response_format {
            if let Ok(value) = serde_json::to_value(format) {
                body["response_format"] = value;

                // qwen3-family-specific kicker: when caller asks for JSON,
                // ALSO disable thinking via the chat_template_kwargs
                // hatch. Verified the same model returns
                // "<think></think>\n\n{...JSON...}" in 434ms with this
                // flag set — empty think block, clean JSON, parser-friendly.
                // Cloud providers ignore unknown fields, so this is safe to
                // set unconditionally when we want JSON.
                // Insert chat_template_kwargs.enable_thinking=false in two
                // sequential mutable borrows so each Map ref is short-lived.
                if let Some(obj) = body.as_object_mut() {
                    obj.insert(
                        "chat_template_kwargs".to_string(),
                        json!({ "enable_thinking": false }),
                    );
                }
            }
            // Diagnostic — print the request body exactly as serialized so we
            // can see which fields actually reach DMR. Helps catch silent
            // serialization drops (caught one 2026-04-19 — entry chain wasn't
            // mutating body in place).
            tracing::info!(
                target: "openai_adapter",
                "request body to {}: {}",
                self.config.name,
                serde_json::to_string(&body).unwrap_or_default()
            );
        }

        // Add tools via the native OpenAI `tools` param — ONLY for Native
        // providers. JsonInPrompt providers already had the tools described in the
        // prompt above (sending the param too would be ignored or confuse them).
        if let Some(tools) = &request.tools {
            if !tools.is_empty()
                && self.config.supports_tools
                && self.config.tool_protocol
                    == super::json_in_prompt_tools::ToolProtocol::Native
            {
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

        // Make request - use runtime base URL if set, otherwise config base URL
        let base_url = self.api_base();
        let url = format!("{}/v1/chat/completions", base_url);

        let mut request_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

        // Only add Authorization header if provider requires auth
        if self.config.requires_auth {
            if let Some(api_key) = &self.api_key {
                request_builder =
                    request_builder.header("Authorization", format!("Bearer {}", api_key));
            }
        }

        // Log the body size + model so post-mortem can reconstruct why a
        // stall happened (oversized prompt, wrong model, etc.). Kept at
        // info! because this is the one log line every failing-persona
        // investigation needs to see.
        let body_bytes = serde_json::to_vec(&body).unwrap_or_default();
        clog_info!(
            "POST {} model={} body_bytes={} has_tools={} stream={}",
            url,
            model,
            body_bytes.len(),
            body.get("tools")
                .and_then(|v| v.as_array())
                .map(|a| a.len())
                .unwrap_or(0)
                > 0,
            body.get("stream")
                .and_then(|v| v.as_bool())
                .unwrap_or(false)
        );

        // Acquire concurrency slot. For DMR (1 slot) this serializes
        // requests so the 120s client timeout measures actual request
        // time, not "time waiting for the previous persona's forward
        // pass." For non-DMR providers (64 slots) this is effectively
        // a no-op. Acquire can't fail here — the semaphore is never
        // closed over the adapter's lifetime.
        let queue_start = Instant::now();
        let _permit = self
            .concurrency
            .clone()
            .acquire_owned()
            .await
            .expect("adapter semaphore never closed");
        let queued_ms = queue_start.elapsed().as_millis();
        if queued_ms > 100 {
            clog_info!(
                "concurrency gate waited {}ms before POST to {}",
                queued_ms,
                self.config.provider_id
            );
        }

        let send_start = Instant::now();
        let response = request_builder.json(&body).send().await.map_err(|e| {
            // reqwest::Error's top-level Display often collapses the
            // real cause (timeout vs connect vs body-write) into a
            // generic "error sending request" string. Walk the error
            // source chain so the log shows the actual terminal
            // reason — critical for debugging stalls where the
            // outer message alone is useless.
            let mut chain: Vec<String> = vec![e.to_string()];
            let mut cur: &dyn std::error::Error = &e;
            while let Some(src) = cur.source() {
                chain.push(src.to_string());
                cur = src;
            }
            format!(
                "{} POST failed after {}ms: {} (kind: timeout={}, connect={}, request={}, body={})",
                self.config.name,
                send_start.elapsed().as_millis(),
                chain.join(" -> "),
                e.is_timeout(),
                e.is_connect(),
                e.is_request(),
                e.is_body()
            )
        })?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(format!(
                "{} returned {}: {}",
                self.config.name, status, body
            ));
        }

        let response_json: OpenAIResponse = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse {} response: {}", self.config.name, e))?;

        let response_time_ms = start.elapsed().as_millis() as u64;

        // Parse response
        let choice = response_json
            .choices
            .first()
            .ok_or_else(|| "No completion in response".to_string())?;

        let text = choice.message.content.clone().unwrap_or_default();
        let mut finish_reason = choice
            .finish_reason
            .as_deref()
            .map(|r| self.map_finish_reason(r))
            .unwrap_or(FinishReason::Stop);

        // Parse native tool calls
        let mut tool_calls: Option<Vec<ToolCall>> = choice.message.tool_calls.as_ref().map(|tcs| {
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

        // JsonInPrompt synthesis: when the gateway/model doesn't do native function
        // calling, the model answers as TEXT containing the JSON tool call from the
        // injected contract. Lift it into the canonical ToolUse shape so the agent
        // loop executes it EXACTLY like a native call (one seam, model-agnostic).
        if tool_calls.as_ref().map_or(true, |t| t.is_empty()) {
            if let Some(call) = self.config.tool_protocol.parse_text_call(&text) {
                finish_reason = FinishReason::ToolUse;
                tool_calls = Some(vec![call]);
            }
        }

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

        let usage = response_json
            .usage
            .map(|u| UsageMetrics {
                input_tokens: u.prompt_tokens,
                output_tokens: u.completion_tokens,
                total_tokens: u
                    .total_tokens
                    .unwrap_or(u.prompt_tokens + u.completion_tokens),
                estimated_cost: None, // TODO: Calculate from model pricing
            })
            .unwrap_or_default();

        Ok(TextGenerationResponse {
            text,
            finish_reason,
            model: response_json.model,
            provider: self.config.provider_id.to_string(),
            usage,
            response_time_ms,
            request_id,
            content: if content_blocks.is_empty() {
                None
            } else {
                Some(content_blocks)
            },
            tool_calls,
            routing: None,
            error: None,
        })
    }

    /// Create embeddings over the OpenAI-compatible `/v1/embeddings` endpoint.
    /// This is the path continuum's neural recall ([`NeuralEmbeddingProvider`])
    /// takes through the unsloth gateway — it replaces the in-process
    /// fastembed/ONNX embedder. Degrades to an `Err` (never panics) when the
    /// endpoint is unreachable or the model isn't an embedding model; the caller
    /// falls back to the lexical embedder.
    ///
    /// [`NeuralEmbeddingProvider`]: crate::cognition::embedding::NeuralEmbeddingProvider
    async fn create_embedding(
        &self,
        request: EmbeddingRequest,
    ) -> Result<EmbeddingResponse, String> {
        let start = Instant::now();

        // The model is the embedding SPACE identity — no silent default among
        // chat models ([[no-fallbacks-ever]]). NeuralEmbeddingProvider always
        // pins the canonical embedding slug; a None here is a config error.
        let model = request.model.clone().ok_or_else(|| {
            format!(
                "{} embeddings require an explicit model (the embedding-space identity)",
                self.config.name
            )
        })?;

        let body = build_embedding_body(&request.input, &model);

        let base_url = self.api_base();
        let url = format!("{base_url}/v1/embeddings");

        let mut request_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");
        if self.config.requires_auth {
            if let Some(api_key) = &self.api_key {
                request_builder =
                    request_builder.header("Authorization", format!("Bearer {api_key}"));
            }
        }

        let response = request_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("{} embeddings POST failed: {e}", self.config.name))?;

        let status = response.status();
        if !status.is_success() {
            let err_body = response.text().await.unwrap_or_default();
            return Err(format!(
                "{} /v1/embeddings {status}: {err_body}",
                self.config.name
            ));
        }

        let json: Value = response
            .json()
            .await
            .map_err(|e| format!("{} embeddings response parse failed: {e}", self.config.name))?;

        let embeddings = parse_embedding_response(&json)?;
        let usage = parse_embedding_usage(&json);

        Ok(EmbeddingResponse {
            embeddings,
            model,
            provider: self.config.provider_id.clone(),
            usage,
            response_time_ms: start.elapsed().as_millis() as u64,
        })
    }

    async fn health_check(&self) -> HealthStatus {
        // Only require API key if provider needs auth
        if self.config.requires_auth && self.api_key.is_none() {
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
        let base_url = self.api_base();
        let url = format!("{}/v1/models", base_url);

        let mut request_builder = self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs(5));

        // Only add Authorization header if provider requires auth
        if self.config.requires_auth {
            if let Some(api_key) = &self.api_key {
                request_builder =
                    request_builder.header("Authorization", format!("Bearer {}", api_key));
            }
        }

        let result = request_builder.send().await;

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
        // Intentionally empty: prefixes live in the registry's
        // `Provider.model_prefixes` and are consulted directly by
        // `supports_model` below. The trait's Vec<&'static str> return
        // can't carry the registry's dynamic Vec<String> without leaking,
        // so we bypass it rather than faking a static slice.
        Vec::new()
    }

    /// Dynamic catalog for DMR, registry-declared prefix match for
    /// everyone else.
    ///
    /// The default trait impl uses `starts_with` against
    /// `supported_model_prefixes`. We override because prefixes now live
    /// in `config/providers.toml` (Provider.model_prefixes), not as
    /// `&'static str` embedded in code. DMR is special-cased because its
    /// catalog is dynamic — what's available depends on `docker model
    /// pull` history — so we check the live runtime_models set populated
    /// at init.
    ///
    /// Returning false when DMR's live set is empty/missing is the right
    /// behavior: AdapterRegistry::select hard-errors when no adapter
    /// supports a model, which surfaces the real problem ("user never
    /// pulled X") instead of silently routing to some other provider.
    fn supports_model(&self, model_name: &str) -> bool {
        if self.config.provider_id == "docker-model-runner" {
            return self.runtime_models_contain(model_name);
        }
        let lower = model_name.to_lowercase();
        // Exact id match against the registry's declared models.
        if self
            .config
            .models
            .iter()
            .any(|m| m.id.to_lowercase() == lower)
        {
            return true;
        }
        // Family prefix match for "id we haven't listed yet but this
        // provider clearly owns" (e.g. gpt-5-preview → openai).
        self.config
            .model_prefixes
            .iter()
            .any(|prefix| lower.starts_with(&prefix.to_lowercase()))
    }
}

// ─── /v1/embeddings helpers (pure — TDD'd apart from the HTTP I/O) ──────────────

/// Build the OpenAI-compatible `/v1/embeddings` request body. A single input is
/// sent as a string and a batch as an array — both shapes the spec accepts —
/// and `model` is the embedding-space identity (already resolved by the caller).
fn build_embedding_body(input: &EmbeddingInput, model: &str) -> Value {
    let input = match input {
        EmbeddingInput::Single(s) => json!(s),
        EmbeddingInput::Multiple(v) => json!(v),
    };
    json!({ "input": input, "model": model })
}

/// Parse an OpenAI-compatible `/v1/embeddings` response into vectors ordered by
/// the response's `index` field. The spec does NOT guarantee `data` comes back
/// in input order, so we sort by `index` — getting this wrong silently
/// misaligns every vector with its source text (a corruption, not a crash).
/// Errors (rather than fabricating a vector) when `data` is missing or an entry
/// has no `embedding`, so a misconfigured endpoint degrades to the lexical
/// fallback instead of poisoning recall with junk.
fn parse_embedding_response(body: &Value) -> Result<Vec<Vec<f32>>, String> {
    let data = body
        .get("data")
        .and_then(|d| d.as_array())
        .ok_or_else(|| format!("embeddings response missing `data` array: {body}"))?;

    let mut indexed: Vec<(usize, Vec<f32>)> = Vec::with_capacity(data.len());
    for (i, item) in data.iter().enumerate() {
        let idx = item
            .get("index")
            .and_then(|v| v.as_u64())
            .map(|v| v as usize)
            .unwrap_or(i);
        let emb = item
            .get("embedding")
            .and_then(|v| v.as_array())
            .ok_or_else(|| format!("embeddings response item {i} missing `embedding`"))?;
        let vec: Vec<f32> = emb.iter().map(|n| n.as_f64().unwrap_or(0.0) as f32).collect();
        indexed.push((idx, vec));
    }
    indexed.sort_by_key(|(i, _)| *i);
    Ok(indexed.into_iter().map(|(_, v)| v).collect())
}

/// Extract token usage from an embeddings response, defaulting missing fields to
/// 0 — usage is observability, never load-bearing, so a provider that omits it
/// must not fail the embed.
fn parse_embedding_usage(body: &Value) -> UsageMetrics {
    let usage = body.get("usage");
    let prompt = usage
        .and_then(|u| u.get("prompt_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(0) as u32;
    let total = usage
        .and_then(|u| u.get("total_tokens"))
        .and_then(|v| v.as_u64())
        .map(|v| v as u32)
        .unwrap_or(prompt);
    UsageMetrics {
        input_tokens: prompt,
        output_tokens: 0,
        total_tokens: total,
        estimated_cost: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a single string input serializes as a JSON string (not
    // a 1-element array) and the resolved model is carried through — the request
    // shape unsloth/OpenAI actually expects.
    #[test]
    fn build_body_single_input() {
        let body = build_embedding_body(&EmbeddingInput::Single("hello".into()), "qwen3-embed");
        assert_eq!(body["input"], json!("hello"));
        assert_eq!(body["model"], json!("qwen3-embed"));
    }

    // what this catches: a batch input serializes as a JSON array, so batched
    // embeds (the recall hot path) go out in one request.
    #[test]
    fn build_body_batch_input() {
        let body = build_embedding_body(
            &EmbeddingInput::Multiple(vec!["a".into(), "b".into()]),
            "qwen3-embed",
        );
        assert_eq!(body["input"], json!(["a", "b"]));
    }

    // what this catches: THE CORRUPTION GUARD — `data` returned out of order is
    // re-sorted by `index`, so vector[k] always corresponds to input[k]. A
    // regression here silently pairs every memory with the wrong vector.
    #[test]
    fn parse_response_orders_by_index() {
        let body = json!({
            "data": [
                { "index": 1, "embedding": [0.4, 0.5] },
                { "index": 0, "embedding": [0.1, 0.2] },
            ],
            "model": "qwen3-embed",
            "usage": { "prompt_tokens": 3, "total_tokens": 3 }
        });
        let vecs = parse_embedding_response(&body).unwrap();
        assert_eq!(vecs, vec![vec![0.1, 0.2], vec![0.4, 0.5]]);
    }

    // what this catches: a malformed response (no `data`) is an Err, NOT a
    // panic and NOT an empty success — so recall degrades to the lexical
    // fallback instead of treating "no signal" as a real (empty) embedding.
    #[test]
    fn parse_response_missing_data_errors() {
        let body = json!({ "object": "error", "message": "no model loaded" });
        assert!(parse_embedding_response(&body).is_err());
    }

    // what this catches: usage is optional — a provider that omits it yields
    // zeroed metrics, never an error (usage is observability, not load-bearing).
    #[test]
    fn parse_usage_defaults_to_zero_when_absent() {
        let usage = parse_embedding_usage(&json!({ "data": [] }));
        assert_eq!(usage.input_tokens, 0);
        assert_eq!(usage.total_tokens, 0);
    }
}
