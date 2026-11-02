//! Anthropic Messages (`POST /v1/messages`): Claude Code, via `ANTHROPIC_BASE_URL`.

use super::{decode, event, place_system, stopped_for_tools, text_of, tool_calls, BadRequest, Call, ErrorKind, Frame, Wire};
use continuum_engine::ai::adapter::GenerationChunk;
use continuum_engine::ai::types::ImageInput;
use continuum_engine::ai::{ChatMessage, ContentPart, MessageContent, NativeToolSpec, TextGenerationRequest, TextGenerationResponse, ToolChoice};
use serde_json::{json, Value};

/// Anthropic's documented ceiling for `temperature`.
const ANTHROPIC_TEMPERATURE_MAX: f64 = 1.0;

/// One streamed answer: the open content block with its index, and the next index.
pub(crate) struct Anthropic {
    model: String,
    id: String,
    open: Option<(Block, u32)>,
    next: u32,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Block {
    Thinking,
    Text,
}

impl Wire for Anthropic {
    const NAME: &'static str = "anthropic_messages";

    fn decode(body: Value) -> Result<Call, BadRequest> {
        let model = body.get("model").and_then(Value::as_str).ok_or_else(|| BadRequest("`model` is required".into()))?;
        let max_tokens = decode::token_limit(&body, &["max_tokens"])?.ok_or_else(|| BadRequest("`max_tokens` is required".into()))?;
        let messages = body.get("messages").and_then(Value::as_array).ok_or_else(|| BadRequest("`messages` is required".into()))?;
        let mut request = TextGenerationRequest {
            model: Some(model.to_owned()),
            max_tokens: Some(max_tokens),
            top_k: decode::count(&body, "top_k", 0)?,
            system_prompt: body.get("system").map(text_of).filter(|s| !s.is_empty()),
            messages: messages.iter().map(message).collect::<Result<_, _>>()?,
            tools: decode::tools(&body, tool)?,
            tool_choice: tool_choice(&body)?,
            purpose: Some(continuum_engine::inference::request_body::DRIVER_PURPOSE.into()),
            ..Default::default()
        };
        decode::sampling(&body, &mut request, ANTHROPIC_TEMPERATURE_MAX)?;
        (request.system_prompt, request.messages) = place_system(request.system_prompt.take(), std::mem::take(&mut request.messages));
        Ok(Call { request, stream: decode::streamed(&body)? })
    }

    fn answer(response: &TextGenerationResponse, id: &str) -> Value {
        let mut content = Vec::new();
        if let Some(reasoning) = response.reasoning.as_deref().filter(|r| !r.is_empty()) {
            content.push(json!({"type": "thinking", "thinking": reasoning, "signature": ""}));
        }
        if !response.text.is_empty() {
            content.push(json!({"type": "text", "text": response.text}));
        }
        for (call_id, name, input) in tool_calls(response) {
            content.push(json!({"type": "tool_use", "id": call_id, "name": name, "input": input}));
        }
        json!({
            "id": id, "type": "message", "role": "assistant", "model": response.model,
            "content": content,
            "stop_reason": stop_reason(response), "stop_sequence": null,
            "usage": usage(response),
        })
    }

    fn error(kind: ErrorKind, message: &str) -> Value {
        let kind = match kind {
            ErrorKind::InvalidRequest => "invalid_request_error",
            ErrorKind::NotFound => "not_found_error",
            ErrorKind::Engine | ErrorKind::CaptureUnavailable => "api_error",
        };
        json!({"type": "error", "error": {"type": kind, "message": message}})
    }

    fn streamer(model: &str, id: &str) -> Self {
        Self { model: model.to_owned(), id: id.to_owned(), open: None, next: 0 }
    }

    fn opened(&mut self) -> Vec<Frame> {
        vec![event(
            Some("message_start"),
            &json!({"type": "message_start", "message": {
                "id": self.id, "type": "message", "role": "assistant", "model": self.model, "content": [],
                "stop_reason": null, "stop_sequence": null, "usage": {"input_tokens": 0, "output_tokens": 0},
            }}),
        )]
    }

    fn chunk(&mut self, chunk: &GenerationChunk) -> Vec<Frame> {
        let (block, delta) = match chunk {
            GenerationChunk::Token(text) => (Block::Text, json!({"type": "text_delta", "text": text})),
            GenerationChunk::Reasoning(text) => (Block::Thinking, json!({"type": "thinking_delta", "thinking": text})),
            _ => return Vec::new(), // prefill progress has no Anthropic form
        };
        let (mut events, index) = self.switch_to(block);
        events.push(event(Some("content_block_delta"), &json!({"type": "content_block_delta", "index": index, "delta": delta})));
        events
    }

    fn finished(&mut self, response: &TextGenerationResponse) -> Vec<Frame> {
        let mut events = self.close();
        for (call_id, name, input) in tool_calls(response) {
            let index = self.next_index();
            events.push(event(Some("content_block_start"), &json!({"type": "content_block_start", "index": index,
                "content_block": {"type": "tool_use", "id": call_id, "name": name, "input": {}}})));
            events.push(event(Some("content_block_delta"), &json!({"type": "content_block_delta", "index": index,
                "delta": {"type": "input_json_delta", "partial_json": input.to_string()}})));
            events.push(event(Some("content_block_stop"), &json!({"type": "content_block_stop", "index": index})));
        }
        events.push(event(Some("message_delta"), &json!({"type": "message_delta",
            "delta": {"stop_reason": stop_reason(response), "stop_sequence": null},
            "usage": usage(response)})));
        events.push(event(Some("message_stop"), &json!({"type": "message_stop"})));
        events
    }

    fn failed(&mut self, message: &str) -> Vec<Frame> {
        let mut events = self.close();
        events.push(event(Some("error"), &Self::error(ErrorKind::Engine, message)));
        events
    }
}

impl Anthropic {
    /// Close the open block if it is another kind, open `block`, and return its index.
    fn switch_to(&mut self, block: Block) -> (Vec<Frame>, u32) {
        if let Some((open, index)) = self.open {
            if open == block {
                return (Vec::new(), index);
            }
        }
        let mut events = self.close();
        let index = self.next_index();
        let content_block = match block {
            Block::Text => json!({"type": "text", "text": ""}),
            Block::Thinking => json!({"type": "thinking", "thinking": ""}),
        };
        events.push(event(Some("content_block_start"), &json!({"type": "content_block_start", "index": index, "content_block": content_block})));
        self.open = Some((block, index));
        (events, index)
    }

    fn close(&mut self) -> Vec<Frame> {
        match self.open.take() {
            Some((_, index)) => vec![event(Some("content_block_stop"), &json!({"type": "content_block_stop", "index": index}))],
            None => Vec::new(),
        }
    }

    fn next_index(&mut self) -> u32 {
        let index = self.next;
        self.next += 1;
        index
    }
}

/// Anthropic counts cache reads apart from fresh input: `input_tokens` is what was prefilled.
fn usage(response: &TextGenerationResponse) -> Value {
    let cached = super::cached_tokens(response).unwrap_or(0); // unwrap_or: no measured reuse reads as none
    json!({"input_tokens": response.usage.input_tokens.saturating_sub(cached), "output_tokens": response.usage.output_tokens,
        "cache_read_input_tokens": cached})
}

fn stop_reason(response: &TextGenerationResponse) -> &'static str {
    if stopped_for_tools(response) {
        "tool_use"
    } else if response.finish_reason == continuum_engine::ai::FinishReason::Length {
        "max_tokens"
    } else {
        "end_turn"
    }
}

fn message(value: &Value) -> Result<ChatMessage, BadRequest> {
    let role = value.get("role").and_then(Value::as_str).ok_or_else(|| BadRequest("a message has no `role`".into()))?;
    let content = match value.get("content") {
        Some(Value::String(text)) => MessageContent::Text(text.clone()),
        Some(Value::Array(blocks)) => MessageContent::Parts(blocks.iter().filter_map(part).collect()),
        _ => return Err(BadRequest("a message has no `content`".into())),
    };
    Ok(ChatMessage { role: role.to_owned(), content, name: None })
}

/// One content block. A prior thinking block is not sent back to the engine.
fn part(block: &Value) -> Option<ContentPart> {
    match block.get("type").and_then(Value::as_str)? {
        "text" => Some(ContentPart::Text { text: block.get("text").and_then(Value::as_str)?.to_owned() }),
        "image" => {
            let source = block.get("source")?;
            Some(ContentPart::Image { image: ImageInput {
                url: source.get("url").and_then(Value::as_str).map(str::to_owned),
                base64: source.get("data").and_then(Value::as_str).map(str::to_owned),
                mime_type: source.get("media_type").and_then(Value::as_str).map(str::to_owned),
            }})
        }
        "tool_use" => Some(ContentPart::ToolUse {
            id: block.get("id").and_then(Value::as_str)?.to_owned(),
            name: block.get("name").and_then(Value::as_str)?.to_owned(),
            input: block.get("input").cloned().unwrap_or_else(|| json!({})), // unwrap_or_else: a call with no input has an empty one
        }),
        "tool_result" => Some(ContentPart::ToolResult {
            tool_use_id: block.get("tool_use_id").and_then(Value::as_str)?.to_owned(),
            content: block.get("content").map(text_of).unwrap_or_default(), // unwrap_or_default: an empty result
            is_error: block.get("is_error").and_then(Value::as_bool),
        }),
        _ => None,
    }
}

/// An Anthropic tool: `{name, description, input_schema}`, untyped or `type: "custom"`. A
/// server tool (`web_search_*`, `bash_*`, `text_editor_*`) is refused, never dropped: Claude
/// Code would expect a tool the model was never shown.
fn tool(entry: &Value) -> Result<NativeToolSpec, String> {
    match decode::tool_type(entry)? {
        None | Some("custom") => {
            let schema = entry.get("input_schema").filter(|s| !s.is_null()).ok_or("has no `input_schema`")?;
            decode::tool_spec(entry.get("name"), entry.get("description"), Some(schema))
        }
        Some(other) => Err(decode::unserved(other, "custom")),
    }
}

/// The Anthropic `tool_choice`: `{type: auto | any | none}`, or `{type: "tool", name}`.
fn tool_choice(body: &Value) -> Result<Option<ToolChoice>, BadRequest> {
    let Some(choice) = decode::present(body, "tool_choice") else {
        return Ok(None);
    };
    match choice.get("type").and_then(Value::as_str) {
        Some("tool") => decode::choice_named(choice.get("name")).map(Some),
        Some(mode) => decode::choice_mode(mode, &[("auto", "auto"), ("any", "any"), ("none", "none")]).map(Some),
        None => Err(BadRequest("`tool_choice` has no `type`".into())),
    }
}
