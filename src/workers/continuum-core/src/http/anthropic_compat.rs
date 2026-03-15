//! Anthropic Messages API compatible request/response types.
//!
//! Claude Code uses `@anthropic-ai/sdk` which sends `POST /v1/messages`
//! in Anthropic's native format. We translate to/from our TextGenerationRequest.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Anthropic Messages API request
#[derive(Debug, Clone, Deserialize)]
pub struct MessagesRequest {
    pub model: String,
    pub messages: Vec<AnthropicMessage>,
    pub max_tokens: u32,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub system: Option<SystemContent>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub top_k: Option<u32>,
    #[serde(default)]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(default)]
    pub tools: Option<Vec<Value>>,
    #[serde(default)]
    pub tool_choice: Option<Value>,
    #[serde(default)]
    pub metadata: Option<Value>,
}

/// System content — can be a string or array of content blocks
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum SystemContent {
    Text(String),
    Blocks(Vec<SystemBlock>),
}

#[derive(Debug, Clone, Deserialize)]
pub struct SystemBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: String,
}

impl SystemContent {
    pub fn as_text(&self) -> String {
        match self {
            SystemContent::Text(s) => s.clone(),
            SystemContent::Blocks(blocks) => blocks
                .iter()
                .map(|b| b.text.as_str())
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

/// Anthropic message (role + content)
#[derive(Debug, Clone, Deserialize)]
pub struct AnthropicMessage {
    pub role: String,
    pub content: AnthropicContent,
}

/// Content can be a plain string or array of content blocks
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum AnthropicContent {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

impl AnthropicContent {
    /// Extract plain text from content (ignoring tool blocks)
    pub fn as_text(&self) -> String {
        match self {
            AnthropicContent::Text(s) => s.clone(),
            AnthropicContent::Blocks(blocks) => blocks
                .iter()
                .filter_map(|b| {
                    if let ContentBlock::Text { text } = b {
                        Some(text.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

/// Content block variants
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        content: Value,
        #[serde(default)]
        is_error: Option<bool>,
    },
    #[serde(rename = "image")]
    Image { source: Value },
}

// ─── Response Types ──────────────────────────────────────────────────────────

/// Non-streaming response
#[derive(Debug, Clone, Serialize)]
pub struct MessagesResponse {
    pub id: String,
    #[serde(rename = "type")]
    pub response_type: String,
    pub role: String,
    pub content: Vec<ResponseContentBlock>,
    pub model: String,
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<String>,
    pub usage: Usage,
}

#[derive(Debug, Clone, Serialize)]
pub struct ResponseContentBlock {
    #[serde(rename = "type")]
    pub block_type: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
}

// ─── SSE Event Types ─────────────────────────────────────────────────────────

/// Build SSE events for a complete response (single burst, not token-by-token).
///
/// Phase 1: Return full text as one SSE burst.
/// Phase 2 (future): Token-by-token streaming from inference loop callback.
pub fn build_sse_events(response: &MessagesResponse) -> Vec<SseEvent> {
    let mut events = Vec::new();

    // message_start
    events.push(SseEvent {
        event: "message_start".to_string(),
        data: serde_json::json!({
            "type": "message_start",
            "message": {
                "id": response.id,
                "type": "message",
                "role": "assistant",
                "content": [],
                "model": response.model,
                "stop_reason": null,
                "stop_sequence": null,
                "usage": {
                    "input_tokens": response.usage.input_tokens,
                    "output_tokens": 0
                }
            }
        }),
    });

    // For each content block
    for (idx, block) in response.content.iter().enumerate() {
        // content_block_start
        events.push(SseEvent {
            event: "content_block_start".to_string(),
            data: serde_json::json!({
                "type": "content_block_start",
                "index": idx,
                "content_block": {
                    "type": "text",
                    "text": ""
                }
            }),
        });

        // content_block_delta (full text in one delta)
        events.push(SseEvent {
            event: "content_block_delta".to_string(),
            data: serde_json::json!({
                "type": "content_block_delta",
                "index": idx,
                "delta": {
                    "type": "text_delta",
                    "text": block.text
                }
            }),
        });

        // content_block_stop
        events.push(SseEvent {
            event: "content_block_stop".to_string(),
            data: serde_json::json!({
                "type": "content_block_stop",
                "index": idx
            }),
        });
    }

    // message_delta
    events.push(SseEvent {
        event: "message_delta".to_string(),
        data: serde_json::json!({
            "type": "message_delta",
            "delta": {
                "stop_reason": response.stop_reason,
                "stop_sequence": response.stop_sequence
            },
            "usage": {
                "output_tokens": response.usage.output_tokens
            }
        }),
    });

    // message_stop
    events.push(SseEvent {
        event: "message_stop".to_string(),
        data: serde_json::json!({
            "type": "message_stop"
        }),
    });

    events
}

/// SSE event for serialization
#[derive(Debug, Clone)]
pub struct SseEvent {
    pub event: String,
    pub data: Value,
}

impl SseEvent {
    /// Format as SSE wire format
    pub fn to_sse_string(&self) -> String {
        format!(
            "event: {}\ndata: {}\n\n",
            self.event,
            serde_json::to_string(&self.data).unwrap_or_default()
        )
    }
}
