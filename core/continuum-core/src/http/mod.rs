//! HTTP server for Anthropic-compatible local inference endpoint.
//!
//! Claude Code sends `POST /v1/messages` in Anthropic format when
//! `ANTHROPIC_BASE_URL` is set. This module serves that endpoint,
//! routing through the SAME `AdapterRegistry::select()` path as the
//! default persona chat — DMR / Vulkan GPU adapters preferred,
//! hard-fails when no GPU adapter can serve the model.
//!
//! GPU-always contract (matches Joel's "no silent CPU fallback" rule):
//! - This endpoint NEVER silently falls back to CandleAdapter for
//!   inference. Candle's role is LoRA training on GPU, not chat.
//! - If select() returns None, the response is 503 with a remediation
//!   hint (pull model into DMR, or install the right GPU backend).
//! - Historic note: an earlier version had explicit
//!   `or_else(|| select(Some(PROVIDER_CANDLE_QUANTIZED), ...))` chained
//!   onto select(), which bypassed the GPU device check by hitting
//!   tier-1 explicit-provider match. Removed because Candle inference
//!   is currently slower than CPU+DMR and shipping it as a silent
//!   fallback gave users a "Continuum feels broken" first-chat tier
//!   surface even when the right GPU backend was available.
//!
//! Architecture:
//! - Axum HTTP server on dynamic port (127.0.0.1:0)
//! - Shared GLOBAL_REGISTRY from AIProviderModule (same adapters)
//! - Translates Anthropic request → TextGenerationRequest → response
//! - Supports both streaming (SSE) and non-streaming responses
//!
//! Lifecycle:
//! - Started on first local-claude-code request via `start_if_needed()`
//! - Runs as tokio task, shared across all sentinels
//! - Port stored in SERVER_PORT for IPC query

pub mod desktop;

pub mod anthropic_compat;

use anthropic_compat::{
    build_sse_events, AnthropicContent, ContentBlock, MessagesRequest, MessagesResponse,
    ResponseContentBlock, Usage,
};

use crate::ai::{
    adapter::InferenceDevice, ActiveAdapterRequest, ChatMessage, MessageContent,
    TextGenerationRequest,
};

use axum::{
    http::StatusCode,
    response::{IntoResponse, Json},
    routing::{get, post},
    Router,
};
use once_cell::sync::Lazy;
use serde_json::{json, Value};
use std::sync::Arc;
use tokio::sync::{OnceCell, RwLock};
use tower_http::cors::CorsLayer;

/// Global server port — set after bind, read by IPC command
static SERVER_PORT: Lazy<Arc<RwLock<Option<u16>>>> = Lazy::new(|| Arc::new(RwLock::new(None)));

/// Ensure the server is started exactly once
static SERVER_INIT: OnceCell<()> = OnceCell::const_new();

/// Get the current server port (None if not started)
pub async fn port() -> Option<u16> {
    *SERVER_PORT.read().await
}

/// Start the HTTP server if not already running.
///
/// Binds to 127.0.0.1:0 (dynamic port), stores the assigned port.
/// Returns the port number.
pub async fn start_if_needed() -> Result<u16, String> {
    SERVER_INIT
        .get_or_try_init(|| async { start_server().await })
        .await
        .map_err(|e| format!("HTTP server failed to start: {}", e))?;

    port()
        .await
        .ok_or_else(|| "HTTP server started but port not set".to_string())
}

/// Internal: start the axum server
async fn start_server() -> Result<(), String> {
    let app = Router::new()
        .route("/v1/messages", post(messages_handler))
        .route("/health", get(health_handler))
        .layer(CorsLayer::permissive())
        .with_state(());

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| format!("Failed to bind HTTP server: {}", e))?;

    let addr = listener
        .local_addr()
        .map_err(|e| format!("Failed to get local addr: {}", e))?;

    let port = addr.port();
    *SERVER_PORT.write().await = Some(port);

    eprintln!(
        "[http] Anthropic-compat inference server started on http://127.0.0.1:{}",
        port
    );

    // Spawn the server as a background task
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app).await {
            eprintln!("[http] Server error: {}", e);
        }
    });

    Ok(())
}

/// Health check endpoint
async fn health_handler() -> impl IntoResponse {
    Json(json!({
        "status": "ok",
        "type": "continuum-local-inference",
        "api": "anthropic-compat"
    }))
}

/// POST /v1/messages — Anthropic Messages API handler
///
/// Pass-through proxy: converts Anthropic format → internal format, routes to
/// the selected adapter. The adapter/backend validates context length and returns
/// errors if input exceeds the model's capacity. No artificial truncation here —
/// the model's own definition is the single source of truth for its limits.
async fn messages_handler(
    Json(req): Json<MessagesRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    let start = std::time::Instant::now();

    // Resolve model spec → provider + model + optional LoRA adapter
    let spec = parse_model_spec(&req.model);

    // Resolve the adapter
    let registry = crate::modules::ai_provider::global_registry();
    let registry_guard = registry.read().await;

    // GPU-always contract: route through select() with the requested
    // provider/model, accept what select() returns, fail loud if
    // nothing matches. NO silent fallback to Candle (or anything CPU)
    // — Candle is for LoRA training on GPU, not for inference. See the
    // module-level docstring above for the historic note on why the
    // explicit candle fallback chain was removed.
    let (provider_id, adapter) = registry_guard
        .select(Some(&spec.provider), spec.model.as_deref(), InferenceDevice::default())
        .ok_or_else(|| {
            let available = registry_guard.available();
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "type": "error",
                    "error": {
                        "type": "overloaded_error",
                        "message": format!(
                            "No GPU-capable adapter supports model '{}' (provider='{}'). \
                             Available adapters: {:?}. \
                             Pull the model into Docker Model Runner (`docker model pull <model-id>`) \
                             or install the right GPU backend (Metal on Mac via Docker Desktop AI \
                             toggle, CUDA on Linux/WSL2 via nvidia-container-toolkit + Docker \
                             Desktop AI toggle). Falling back to Candle (CPU-tier speed) was \
                             intentionally disabled — see docs/SETUP.md for per-OS install steps.",
                            spec.model.as_deref().unwrap_or("(unspecified)"),
                            spec.provider,
                            available
                        )
                    }
                })),
            )
        })?;

    // Log request sizes for debugging
    let context_window = adapter
        .capabilities()
        .max_context_window
        .map_or_else(|| "undeclared".to_string(), |w| w.to_string());
    let system_chars = req.system.as_ref().map(|s| s.as_text().len()).unwrap_or(0);
    let msg_chars: usize = req.messages.iter().map(|m| m.content.as_text().len()).sum();
    let tools_count = req.tools.as_ref().map(|t| t.len()).unwrap_or(0);
    eprintln!(
        "[http] Request: model={}, context_window={}, system={}chars, messages={}chars ({}msgs), tools={}, max_tokens={}",
        req.model,
        context_window,
        system_chars,
        msg_chars,
        req.messages.len(),
        tools_count,
        req.max_tokens
    );

    // Convert Anthropic messages → internal format (no truncation — pass through faithfully)
    let system_prompt = req.system.as_ref().map(|s| s.as_text());
    let messages = convert_messages(&req.messages);

    // Build LoRA adapter request if persona was specified
    let active_adapters = spec.adapter.as_ref().map(|name| {
        vec![ActiveAdapterRequest {
            name: name.clone(),
            path: String::new(), // CandleAdapter resolves from AdapterStore
            domain: "coding".to_string(),
            scale: 1.0,
        }]
    });

    let gen_request = TextGenerationRequest {
        messages,
        system_prompt,
        model: spec.model.clone(),
        provider: Some(spec.provider.clone()),
        temperature: req.temperature,
        max_tokens: Some(req.max_tokens),
        top_p: req.top_p,
        top_k: req.top_k,
        repeat_penalty: req.repeat_penalty,
        frequency_penalty: None,
        repeat_last_n: None,
        stop_sequences: req.stop_sequences.clone(),
        tools: None, // Tool calls handled by Claude Code, not the local model
        tool_choice: None,
        response_format: None,
        active_adapters,
        request_id: Some(format!(
            "msg_{}",
            uuid::Uuid::new_v4().to_string().replace('-', "")
        )),
        user_id: None,
        room_id: None,
        purpose: Some("local-coding-agent".to_string()),
        // External coding-agent caller (not a persona-owned conversation).
        persona_id: None,
    };

    let response = adapter.generate_text(gen_request).await.map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(json!({
                "type": "error",
                "error": {
                    "type": "api_error",
                    "message": format!("Inference error: {}", e)
                }
            })),
        )
    })?;

    let response_time_ms = start.elapsed().as_millis() as u64;
    let request_id = response.request_id.clone();

    // Build Anthropic-format response
    let anthropic_response = MessagesResponse {
        id: request_id,
        response_type: "message".to_string(),
        role: "assistant".to_string(),
        content: vec![ResponseContentBlock {
            block_type: "text".to_string(),
            text: response.text.clone(),
        }],
        model: response.model.clone(),
        stop_reason: Some(format!("{}", response.finish_reason)),
        stop_sequence: None,
        usage: Usage {
            input_tokens: response.usage.input_tokens as u64,
            output_tokens: response.usage.output_tokens as u64,
        },
    };

    eprintln!(
        "[http] {} → {} ({}ms, {} in / {} out tokens)",
        req.model,
        provider_id,
        response_time_ms,
        response.usage.input_tokens,
        response.usage.output_tokens
    );

    if req.stream {
        // SSE streaming response (single burst for now — full text in one event sequence)
        let events = build_sse_events(&anthropic_response);
        let body = events.iter().map(|e| e.to_sse_string()).collect::<String>();

        Ok(axum::response::Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/event-stream")
            .header("cache-control", "no-cache")
            .body(axum::body::Body::from(body))
            .unwrap()
            .into_response())
    } else {
        // Non-streaming JSON response
        Ok(Json(serde_json::to_value(&anthropic_response).unwrap()).into_response())
    }
}

/// Parsed model specification from the Anthropic API request.
///
/// Model strings map as follows:
///   "local/default"     → provider=candle, model=None (base model), adapter=None
///   "local/helper"      → provider=candle, model=None (base model), adapter=Some("helper")
///   "candle"            → provider=candle, model=None (base model), adapter=None
///   "unsloth/Llama-3.2" → provider=candle, model=Some("unsloth/Llama-3.2"), adapter=None
///
/// The "local/" prefix is the convention for routing through Candle.
/// Anything after "local/" that isn't "default" is treated as a persona
/// name whose LoRA adapter should be activated.
struct ModelSpec {
    provider: String,
    /// Explicit model name (None = use adapter's default_model)
    model: Option<String>,
    /// LoRA adapter to activate (persona name)
    adapter: Option<String>,
}

const LOCAL_PREFIX: &str = "local/";
const DEFAULT_PERSONA: &str = "default";
/// Provider ID for quantized GGUF backend (large context, no LoRA).
const PROVIDER_CANDLE_QUANTIZED: &str = "candle-q";
/// Provider ID for safetensors BF16 backend (LoRA support, smaller context).
const PROVIDER_CANDLE_SAFETENSORS: &str = "candle";

fn parse_model_spec(raw: &str) -> ModelSpec {
    if let Some(persona) = raw.strip_prefix(LOCAL_PREFIX) {
        if persona == DEFAULT_PERSONA || persona.is_empty() {
            // No LoRA needed → use quantized (larger context window)
            ModelSpec {
                provider: PROVIDER_CANDLE_QUANTIZED.to_string(),
                model: None,
                adapter: None,
            }
        } else {
            // Persona specified → use safetensors (LoRA support)
            ModelSpec {
                provider: PROVIDER_CANDLE_SAFETENSORS.to_string(),
                model: None,
                adapter: Some(persona.to_string()),
            }
        }
    } else if raw == "candle" || raw == "candle-q" {
        ModelSpec {
            provider: raw.to_string(),
            model: None,
            adapter: None,
        }
    } else {
        // Treat as explicit model name (e.g. "unsloth/Llama-3.2-3B-Instruct")
        // Default to quantized for large context
        ModelSpec {
            provider: PROVIDER_CANDLE_QUANTIZED.to_string(),
            model: Some(raw.to_string()),
            adapter: None,
        }
    }
}

/// Convert Anthropic messages to our ChatMessage format
fn convert_messages(messages: &[anthropic_compat::AnthropicMessage]) -> Vec<ChatMessage> {
    messages
        .iter()
        .map(|msg| {
            let content = match &msg.content {
                AnthropicContent::Text(s) => MessageContent::Text(s.clone()),
                AnthropicContent::Blocks(blocks) => {
                    // If all blocks are text, flatten to single text
                    let all_text = blocks
                        .iter()
                        .all(|b| matches!(b, ContentBlock::Text { .. }));
                    if all_text {
                        let text = blocks
                            .iter()
                            .filter_map(|b| {
                                if let ContentBlock::Text { text } = b {
                                    Some(text.as_str())
                                } else {
                                    None
                                }
                            })
                            .collect::<Vec<_>>()
                            .join("\n");
                        MessageContent::Text(text)
                    } else {
                        // Mixed content — convert to Parts
                        let parts = blocks
                            .iter()
                            .filter_map(|b| match b {
                                ContentBlock::Text { text } => {
                                    Some(crate::ai::ContentPart::Text { text: text.clone() })
                                }
                                ContentBlock::ToolUse { id, name, input } => {
                                    Some(crate::ai::ContentPart::ToolUse {
                                        id: id.clone(),
                                        name: name.clone(),
                                        input: input.clone(),
                                    })
                                }
                                ContentBlock::ToolResult {
                                    tool_use_id,
                                    content,
                                    is_error,
                                } => {
                                    let content_str = match content {
                                        Value::String(s) => s.clone(),
                                        other => serde_json::to_string(other).unwrap_or_default(),
                                    };
                                    Some(crate::ai::ContentPart::ToolResult {
                                        tool_use_id: tool_use_id.clone(),
                                        content: content_str,
                                        is_error: *is_error,
                                    })
                                }
                                _ => None,
                            })
                            .collect();
                        MessageContent::Parts(parts)
                    }
                }
            };

            ChatMessage {
                role: msg.role.clone(),
                content,
                name: None,
            }
        })
        .collect()
}
