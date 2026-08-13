//! `ai/generate` — the inference seam. Every live persona turn's token generation
//! flows through here: select an adapter from the registry, run the request, stamp
//! routing observability, return the response.
//!
//! This is NOT a persona tool — it is the compute boundary the cognition layer calls
//! in-process (and the grid leases remotely). Gated `Privileged`: leased inference,
//! not a citizen-facing action.
//!
//! ## Faithful migration
//!
//! Ported verbatim off `AIProviderModule::handle_command`'s `ai/generate` arm. The
//! input is polymorphic (a `messages` array OR a `prompt` string, with snake_case
//! AND camelCase aliases across a dozen sampling fields), so `Params = Value` and
//! [`parse_request`] reproduces the legacy alias surface byte-for-byte — the canonical
//! typed input [`TextGenerationRequest`] is the eventual convergence target, not this
//! migration's job. The output [`AiGenerateResult`] reuses the canonical sub-types
//! ([`UsageMetrics`]/[`ContentPart`]/[`ToolCall`]/[`RoutingInfo`]) and mirrors the
//! exact camelCase wire shape the old hand-rolled `response_to_json` produced.

use std::sync::Arc;

use serde_json::Value;
use tokio::sync::RwLock;

use crate::ai::adapter::InferenceDevice;
use crate::ai::{
    AdapterRegistry, ChatMessage, ContentPart, MessageContent, RoutingInfo, TextGenerationRequest,
    TextGenerationResponse, ToolCall, UsageMetrics,
};
use crate::logging::TimingGuard;
use crate::modules::ai_provider::select_failure_message;
use crate::utils::params::Params;

/// Result of `ai/generate` — the exact camelCase shape the inference seam has always
/// returned. `success` is always `true` on the Ok path (errors return `Err`, which the
/// runtime renders as a failure result); the optional blocks are omitted when absent.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/AiGenerateResult.ts"
)]
pub struct AiGenerateResult {
    pub success: bool,
    pub text: String,
    /// `FinishReason` rendered as its wire string (`stop` | `length` | `tool_use` | `error`).
    pub finish_reason: String,
    pub model: String,
    pub provider: String,
    pub usage: UsageMetrics,
    #[ts(type = "number")]
    pub response_time_ms: u64,
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub content: Option<Vec<ContentPart>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub routing: Option<RoutingInfo>,
}

impl From<TextGenerationResponse> for AiGenerateResult {
    fn from(response: TextGenerationResponse) -> Self {
        AiGenerateResult {
            success: true,
            text: response.text,
            finish_reason: format!("{}", response.finish_reason),
            model: response.model,
            provider: response.provider,
            usage: response.usage,
            response_time_ms: response.response_time_ms,
            request_id: response.request_id,
            content: response.content,
            tool_calls: response.tool_calls,
            routing: response.routing,
        }
    }
}

/// Parse a [`TextGenerationRequest`] from the polymorphic JSON params — a `messages`
/// array or a single `prompt` string, with snake_case + camelCase aliases on every
/// sampling field. Lifted verbatim from the legacy `AIProviderModule::parse_request`.
fn parse_request(params: &Value) -> Result<TextGenerationRequest, String> {
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
        repeat_penalty: p
            .f32_opt("repeat_penalty")
            .or_else(|| p.f32_opt("repeatPenalty")),
        frequency_penalty: p
            .f32_opt("frequency_penalty")
            .or_else(|| p.f32_opt("frequencyPenalty")),
        repeat_last_n: p
            .u64_opt_alias("repeat_last_n", "repeatLastN")
            .map(|n| n as u32),
        stop_sequences: p
            .json_opt("stop_sequences")
            .or_else(|| p.json_opt("stopSequences")),
        tools: p.json_opt("tools"),
        tool_choice: p.json_opt("tool_choice"),
        response_format: None,
        active_adapters: p.json_opt("activeAdapters"),
        request_id: p.string_opt_alias("request_id", "requestId"),
        user_id: p.string_opt_alias("user_id", "userId"),
        room_id: p.string_opt_alias("room_id", "roomId"),
        purpose: p.str_opt("purpose").map(String::from),
        // Caller-provided persona attribution. TS sends `personaId` (camelCase)
        // per Continuum convention; snake_case alias accepted for symmetry.
        persona_id: p.string_opt_alias("persona_id", "personaId"),
    })
}

crate::action_command! {
    /// Generate text from an AI provider: select the best adapter for the requested
    /// provider/model (or the default), run the request, and return the completion
    /// with usage, finish reason, optional tool calls, and routing observability.
    /// The inference seam every persona turn flows through. Gated `Privileged`
    /// (leased inference — not a persona-facing tool).
    pub struct AiGenerate { registry: Arc<RwLock<AdapterRegistry>> }
    name: "ai/generate",
    access: Privileged,
    params: Value,
    output: AiGenerateResult,
    run(this, _ctx, p) => {
        let _timer = TimingGuard::new("module", "ai_generate");

        let request = parse_request(&p)?;

        let registry = this.registry.read().await;

        let (provider_id, adapter) = registry
            .select(
                request.provider.as_deref(),
                request.model.as_deref(),
                InferenceDevice::default(),
            )
            .ok_or_else(|| {
                select_failure_message(
                    &registry,
                    request.provider.as_deref(),
                    request.model.as_deref(),
                )
            })?;

        crate::runtime::logger("ai_provider").info(&format!(
            "Using {} adapter for model {:?}",
            provider_id, request.model
        ));

        let is_local = adapter.capabilities().is_local;
        let mut response = adapter.generate_text(request).await?;

        // Stamp routing info, preserving any adapters_applied / reason the adapter set.
        let prior_routing = response.routing.take();
        response.routing = Some(RoutingInfo {
            provider: provider_id.to_string(),
            is_local,
            routing_reason: prior_routing
                .as_ref()
                .map(|r| r.routing_reason.clone())
                .unwrap_or_else(|| "adapter_selected".to_string()),
            adapters_applied: prior_routing
                .as_ref()
                .map(|r| r.adapters_applied.clone())
                .unwrap_or_default(),
            model_mapped: None,
            model_requested: prior_routing.and_then(|r| r.model_requested),
        });

        Ok(AiGenerateResult::from(response))
    }
}
