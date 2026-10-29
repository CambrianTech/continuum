//! OpenAI Chat Completions (`POST /v1/chat/completions`): Hermes, opencode, and most agents.

use super::decode::{self, present};
use super::{event, place_system, stopped_for_tools, text_of, tool_calls, BadRequest, Call, ErrorKind, Frame, Wire};
use continuum_engine::ai::adapter::GenerationChunk;
use continuum_engine::ai::types::ImageInput;
use continuum_engine::ai::{ChatMessage, ContentPart, MessageContent, NativeToolSpec, TextGenerationRequest, TextGenerationResponse, ToolChoice};
use serde_json::{json, Value};

/// OpenAI's documented ceiling for `temperature` (Chat Completions and Responses alike).
pub(super) const OPENAI_TEMPERATURE_MAX: f64 = 2.0;

/// One streamed answer.
pub(crate) struct Chat {
    model: String,
    id: String,
    created: u64,
}

impl Wire for Chat {
    const NAME: &'static str = "openai_chat";

    fn decode(body: Value) -> Result<Call, BadRequest> {
        let model = body.get("model").and_then(Value::as_str).ok_or_else(|| BadRequest("`model` is required".into()))?;
        let messages = body.get("messages").and_then(Value::as_array).ok_or_else(|| BadRequest("`messages` is required".into()))?;
        let (system_prompt, messages) = place_system(None, conversation(messages)?);
        let mut request = TextGenerationRequest {
            model: Some(model.to_owned()),
            max_tokens: decode::token_limit(&body, &["max_completion_tokens", "max_tokens"])?,
            system_prompt,
            messages,
            tools: decode::tools(&body, tool)?,
            tool_choice: tool_choice(&body)?,
            purpose: Some(continuum_engine::inference::request_body::DRIVER_PURPOSE.into()),
            ..Default::default()
        };
        decode::sampling(&body, &mut request, OPENAI_TEMPERATURE_MAX)?;
        Ok(Call { request, stream: decode::streamed(&body)? })
    }

    fn answer(response: &TextGenerationResponse, id: &str) -> Value {
        let mut message = json!({"role": "assistant", "content": if response.text.is_empty() { Value::Null } else { json!(response.text) }});
        if let Some(reasoning) = response.reasoning.as_deref().filter(|r| !r.is_empty()) {
            message["reasoning_content"] = json!(reasoning);
        }
        let calls: Vec<Value> = tool_calls(response).iter().map(function_call).collect();
        if !calls.is_empty() {
            message["tool_calls"] = json!(calls);
        }
        json!({
            "id": id, "object": "chat.completion", "created": continuum_base::time::EpochMs::now().secs(), "model": response.model,
            "choices": [{"index": 0, "message": message, "finish_reason": finish_reason(response)}],
            "usage": usage(response),
        })
    }

    fn error(kind: ErrorKind, message: &str) -> Value {
        let (kind, code) = match kind {
            ErrorKind::InvalidRequest => ("invalid_request_error", "invalid_request"),
            ErrorKind::NotFound => ("invalid_request_error", "model_not_found"),
            ErrorKind::Engine => ("server_error", "engine_error"),
            ErrorKind::CaptureUnavailable => ("server_error", "capture_unavailable"),
        };
        json!({"error": {"message": message, "type": kind, "code": code}})
    }

    fn streamer(model: &str, id: &str) -> Self {
        Self { model: model.to_owned(), id: id.to_owned(), created: continuum_base::time::EpochMs::now().secs() }
    }

    fn opened(&mut self) -> Vec<Frame> {
        vec![self.delta(json!({"role": "assistant", "content": ""}), Value::Null)]
    }

    fn chunk(&mut self, chunk: &GenerationChunk) -> Vec<Frame> {
        match chunk {
            GenerationChunk::Token(text) => vec![self.delta(json!({"content": text}), Value::Null)],
            GenerationChunk::Reasoning(text) => vec![self.delta(json!({"reasoning_content": text}), Value::Null)],
            _ => Vec::new(), // prefill progress has no Chat Completions form
        }
    }

    fn finished(&mut self, response: &TextGenerationResponse) -> Vec<Frame> {
        let mut events = Vec::new();
        let calls: Vec<Value> = tool_calls(response)
            .iter()
            .enumerate()
            .map(|(index, call)| {
                let mut call = function_call(call);
                call["index"] = json!(index);
                call
            })
            .collect();
        if !calls.is_empty() {
            events.push(self.delta(json!({"tool_calls": calls}), Value::Null));
        }
        events.push(self.delta(json!({}), json!(finish_reason(response))));
        events.push(event(None, &json!({"id": self.id, "object": "chat.completion.chunk", "created": self.created,
            "model": self.model, "choices": [], "usage": usage(response)})));
        events.push(Frame::done());
        events
    }

    fn failed(&mut self, message: &str) -> Vec<Frame> {
        vec![event(None, &Self::error(ErrorKind::Engine, message)), Frame::done()]
    }
}

impl Chat {
    fn delta(&self, delta: Value, finish_reason: Value) -> Frame {
        event(None, &json!({"id": self.id, "object": "chat.completion.chunk", "created": self.created, "model": self.model,
            "choices": [{"index": 0, "delta": delta, "finish_reason": finish_reason}]}))
    }
}

fn finish_reason(response: &TextGenerationResponse) -> &'static str {
    if stopped_for_tools(response) {
        "tool_calls"
    } else if response.finish_reason == continuum_engine::ai::FinishReason::Length {
        "length"
    } else {
        "stop"
    }
}

fn usage(response: &TextGenerationResponse) -> Value {
    let mut usage = json!({"prompt_tokens": response.usage.input_tokens, "completion_tokens": response.usage.output_tokens,
        "total_tokens": response.usage.input_tokens + response.usage.output_tokens});
    if let Some(cached) = super::cached_tokens(response) {
        usage["prompt_tokens_details"] = json!({"cached_tokens": cached});
    }
    usage
}

fn function_call((id, name, input): &(String, String, Value)) -> Value {
    json!({"id": id, "type": "function", "function": {"name": name, "arguments": input.to_string()}})
}

/// The messages as the engine reads them; `place_system` then decides where system text goes.
fn conversation(messages: &[Value]) -> Result<Vec<ChatMessage>, BadRequest> {
    let mut out = Vec::new();
    for m in messages {
        let role = m.get("role").and_then(Value::as_str).ok_or_else(|| BadRequest("a message has no `role`".into()))?;
        let content = &m["content"];
        match role {
            "assistant" => {
                let mut parts = Vec::new();
                let text = text_of(content);
                if !text.is_empty() {
                    parts.push(ContentPart::Text { text });
                }
                for call in m.get("tool_calls").and_then(Value::as_array).into_iter().flatten() {
                    let function = call.get("function").ok_or_else(|| BadRequest("a tool call has no `function`".into()))?;
                    let arguments = function.get("arguments").and_then(Value::as_str).unwrap_or("{}"); // unwrap_or: a call with no arguments
                    parts.push(ContentPart::ToolUse {
                        id: call.get("id").and_then(Value::as_str).unwrap_or_default().to_owned(), // unwrap_or_default: an unnamed call keeps an empty id
                        name: function.get("name").and_then(Value::as_str).ok_or_else(|| BadRequest("a tool call has no name".into()))?.to_owned(),
                        input: serde_json::from_str(arguments).unwrap_or_else(|_| Value::String(arguments.to_owned())), // unwrap_or_else: arguments that are not JSON pass through as written
                    });
                }
                out.push(ChatMessage { role: "assistant".into(), content: MessageContent::Parts(parts), name: None });
            }
            "tool" => out.push(ChatMessage {
                role: "tool".into(),
                content: MessageContent::Parts(vec![ContentPart::ToolResult {
                    tool_use_id: m.get("tool_call_id").and_then(Value::as_str).ok_or_else(|| BadRequest("a tool message has no `tool_call_id`".into()))?.to_owned(),
                    content: text_of(content),
                    is_error: None,
                }]),
                name: None,
            }),
            _ => out.push(ChatMessage { role: role.to_owned(), content: user_content(content), name: None }),
        }
    }
    Ok(out)
}

fn user_content(content: &Value) -> MessageContent {
    match content {
        Value::Array(parts) => MessageContent::Parts(
            parts
                .iter()
                .filter_map(|p| match p.get("type").and_then(Value::as_str)? {
                    "text" => Some(ContentPart::Text { text: p.get("text").and_then(Value::as_str)?.to_owned() }),
                    "image_url" => Some(ContentPart::Image { image: image(p.get("image_url")?.get("url")?.as_str()?) }),
                    _ => None,
                })
                .collect(),
        ),
        other => MessageContent::Text(text_of(other)),
    }
}

/// An image URL, or a `data:` URL carrying its bytes.
pub(super) fn image(url: &str) -> ImageInput {
    match url.strip_prefix("data:").and_then(|rest| rest.split_once(";base64,")) {
        Some((mime, data)) => ImageInput { url: None, base64: Some(data.to_owned()), mime_type: Some(mime.to_owned()) },
        None => ImageInput { url: Some(url.to_owned()), base64: None, mime_type: None },
    }
}

/// A Chat Completions tool: `{type: "function", function: {name, description, parameters}}`.
fn tool(entry: &Value) -> Result<NativeToolSpec, String> {
    match decode::tool_type(entry)? {
        Some("function") => {}
        Some(other) => return Err(decode::unserved(other, "`function`")),
        None => return Err("has no `type`".into()),
    }
    let function = entry.get("function").filter(|f| f.is_object()).ok_or("has no `function` object")?;
    decode::tool_spec(function.get("name"), function.get("description"), function.get("parameters"))
}

/// The OpenAI `tool_choice`: a mode (`required` is the engine's `any`), or one function by
/// name (`{type: "function", function: {name}}` here, `{type: "function", name}` in
/// Responses).
pub(super) fn tool_choice(body: &Value) -> Result<Option<ToolChoice>, BadRequest> {
    match present(body, "tool_choice") {
        None => Ok(None),
        Some(Value::String(mode)) => decode::choice_mode(mode, &[("none", "none"), ("auto", "auto"), ("required", "any")]).map(Some),
        Some(choice @ Value::Object(_)) => {
            decode::choice_named(choice.get("function").and_then(|f| f.get("name")).or_else(|| choice.get("name"))).map(Some)
        }
        Some(other) => Err(BadRequest(format!("`tool_choice` must be a mode or an object, got {}", decode::kind(other)))),
    }
}
