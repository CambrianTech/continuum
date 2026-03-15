//! HTTP server for Anthropic-compatible local inference endpoint.
//!
//! Claude Code sends `POST /v1/messages` in Anthropic format when
//! `ANTHROPIC_BASE_URL` is set. This module serves that endpoint,
//! routing to CandleAdapter (with LoRA support) for inference.
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

pub mod anthropic_compat;

use anthropic_compat::{
    build_sse_events, AnthropicContent, ContentBlock, MessagesRequest, MessagesResponse,
    ResponseContentBlock, Usage,
};

use crate::ai::{
    ActiveAdapterRequest, ChatMessage, MessageContent, TextGenerationRequest,
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
        .get_or_try_init(|| async {
            start_server().await
        })
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
async fn messages_handler(
    Json(req): Json<MessagesRequest>,
) -> Result<impl IntoResponse, (StatusCode, Json<Value>)> {
    let start = std::time::Instant::now();

    // Resolve model → adapter selection
    // Format: "local/{persona}" or just a model name
    let (provider, model) = parse_model_spec(&req.model);

    // Convert Anthropic messages → our ChatMessage format
    let messages = convert_messages(&req.messages);

    // Extract system prompt
    let system_prompt = req.system.as_ref().map(|s| s.as_text());

    // Parse LoRA adapter from model spec if present
    let active_adapters = if provider == "candle" || provider == "local" {
        // "local/helper" → activate adapter for "helper" persona
        let adapter_name = req.model.strip_prefix("local/").unwrap_or(&req.model);
        if adapter_name != req.model {
            // Check if an adapter path is available — we let the adapter resolve it
            Some(vec![ActiveAdapterRequest {
                name: adapter_name.to_string(),
                path: String::new(), // CandleAdapter resolves from AdapterStore
                domain: "coding".to_string(),
                scale: 1.0,
            }])
        } else {
            None
        }
    } else {
        None
    };

    let gen_request = TextGenerationRequest {
        messages,
        system_prompt,
        model: Some(model.clone()),
        provider: Some(provider.clone()),
        temperature: req.temperature,
        max_tokens: Some(req.max_tokens),
        top_p: req.top_p,
        top_k: req.top_k,
        stop_sequences: req.stop_sequences.clone(),
        tools: None,       // Tool calls handled by Claude Code, not the local model
        tool_choice: None,
        active_adapters,
        request_id: Some(format!("msg_{}", uuid::Uuid::new_v4().to_string().replace('-', ""))),
        user_id: None,
        room_id: None,
        purpose: Some("local-coding-agent".to_string()),
    };

    // Route through GLOBAL_REGISTRY (same as AIProviderModule)
    let registry = crate::modules::ai_provider::global_registry();
    let registry_guard = registry.read().await;

    let (provider_id, adapter) = registry_guard
        .select(Some(&provider), Some(&model))
        .or_else(|| {
            // Fall back to candle if specific provider not found
            registry_guard.select(Some("candle"), None)
        })
        .ok_or_else(|| {
            (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({
                    "type": "error",
                    "error": {
                        "type": "overloaded_error",
                        "message": "No local inference provider available. Set INFERENCE_MODE=local in config.env"
                    }
                })),
            )
        })?;

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
        let body = events
            .iter()
            .map(|e| e.to_sse_string())
            .collect::<String>();

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

/// Parse model spec: "local/helper" → ("candle", "helper"), "candle" → ("candle", default)
fn parse_model_spec(model: &str) -> (String, String) {
    if let Some(name) = model.strip_prefix("local/") {
        ("candle".to_string(), name.to_string())
    } else if model.contains('/') {
        let parts: Vec<&str> = model.splitn(2, '/').collect();
        (parts[0].to_string(), parts[1].to_string())
    } else {
        ("candle".to_string(), model.to_string())
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
                    let all_text = blocks.iter().all(|b| matches!(b, ContentBlock::Text { .. }));
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
                                    Some(crate::ai::ContentPart::Text {
                                        text: text.clone(),
                                    })
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
