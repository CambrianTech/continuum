//! OpenAI Responses (`POST /v1/responses`): Grok Build (`--xai-api-base-url`) and Codex CLI.
//!
//! `previous_response_id` names server-side conversation state. That state is a driver
//! mind's memory, which arrives with session identity (DRIVER-GATEWAY.md G2); until then a
//! call that names one is refused rather than answered without its history.

use super::{chat, event, input_schema, sampling, stopped_for_tools, text_of, tool_calls, BadRequest, Call, ErrorKind, Frame, Wire};
use continuum_engine::ai::adapter::GenerationChunk;
use continuum_engine::ai::{ChatMessage, ContentPart, MessageContent, NativeToolSpec, TextGenerationRequest, TextGenerationResponse};
use serde_json::{json, Value};

/// One streamed answer: the text item, once opened, and the event sequence.
pub(crate) struct Responses {
    model: String,
    id: String,
    created: u64,
    sequence: u64,
    text: Option<String>,
}

impl Wire for Responses {
    const NAME: &'static str = "openai_responses";

    fn decode(body: Value) -> Result<Call, BadRequest> {
        let model = body.get("model").and_then(Value::as_str).ok_or_else(|| BadRequest("`model` is required".into()))?;
        if body.get("previous_response_id").is_some_and(|v| !v.is_null()) {
            return Err(BadRequest(
                "`previous_response_id` needs server-side session memory, which this gateway does not keep yet; send the full `input` with `store: false`".into(),
            ));
        }
        let messages = match body.get("input") {
            Some(Value::String(text)) => vec![ChatMessage { role: "user".into(), content: MessageContent::Text(text.clone()), name: None }],
            Some(Value::Array(items)) => items_to_messages(items)?,
            _ => return Err(BadRequest("`input` is required".into())),
        };
        let mut request = TextGenerationRequest {
            model: Some(model.to_owned()),
            max_tokens: body.get("max_output_tokens").and_then(Value::as_u64).map(|t| t as u32),
            system_prompt: body.get("instructions").and_then(Value::as_str).map(str::to_owned),
            messages,
            tools: body.get("tools").and_then(Value::as_array).map(|tools| tools.iter().filter_map(tool).collect()),
            tool_choice: body.get("tool_choice").and_then(chat::tool_choice),
            purpose: Some("driver".into()),
            ..Default::default()
        };
        sampling(&body, &mut request);
        Ok(Call { request, stream: body.get("stream").and_then(Value::as_bool).unwrap_or(false) }) // unwrap_or: the API's default is not streamed
    }

    fn answer(response: &TextGenerationResponse, id: &str) -> Value {
        envelope(id, &response.model, now_s(), "completed", output(response, id), Some(response))
    }

    fn error(kind: ErrorKind, message: &str) -> Value {
        let code = match kind {
            ErrorKind::InvalidRequest => "invalid_request",
            ErrorKind::NotFound => "model_not_found",
            ErrorKind::Engine => "server_error",
        };
        json!({"error": {"message": message, "type": "invalid_request_error", "code": code}})
    }

    fn streamer(model: &str, id: &str) -> Self {
        Self { model: model.to_owned(), id: id.to_owned(), created: now_s(), sequence: 0, text: None }
    }

    fn opened(&mut self) -> Vec<Frame> {
        let response = envelope(&self.id, &self.model, self.created, "in_progress", Vec::new(), None);
        vec![self.emit("response.created", json!({"response": response}))]
    }

    fn chunk(&mut self, chunk: &GenerationChunk) -> Vec<Frame> {
        let GenerationChunk::Token(delta) = chunk else {
            return Vec::new(); // reasoning and prefill progress are not streamed in this format
        };
        let mut events = Vec::new();
        if self.text.is_none() {
            self.text = Some(String::new());
            let item = json!({"type": "message", "id": self.message_id(), "status": "in_progress", "role": "assistant", "content": []});
            events.push(self.emit("response.output_item.added", json!({"output_index": 0, "item": item})));
            events.push(self.emit("response.content_part.added", json!({"item_id": self.message_id(), "output_index": 0,
                "content_index": 0, "part": {"type": "output_text", "text": "", "annotations": []}})));
        }
        if let Some(text) = self.text.as_mut() {
            text.push_str(delta);
        }
        events.push(self.emit("response.output_text.delta", json!({"item_id": self.message_id(), "output_index": 0, "content_index": 0, "delta": delta})));
        events
    }

    fn finished(&mut self, response: &TextGenerationResponse) -> Vec<Frame> {
        let mut events = Vec::new();
        let mut index = 0;
        if let Some(text) = self.text.take() {
            let part = json!({"type": "output_text", "text": text, "annotations": []});
            events.push(self.emit("response.output_text.done", json!({"item_id": self.message_id(), "output_index": 0, "content_index": 0, "text": text})));
            events.push(self.emit("response.content_part.done", json!({"item_id": self.message_id(), "output_index": 0, "content_index": 0, "part": part})));
            let item = json!({"type": "message", "id": self.message_id(), "status": "completed", "role": "assistant", "content": [part]});
            events.push(self.emit("response.output_item.done", json!({"output_index": 0, "item": item})));
            index = 1;
        }
        for call in tool_calls(response) {
            let item = function_call(&call);
            let arguments = item["arguments"].clone();
            let mut added = item.clone();
            added["arguments"] = json!("");
            added["status"] = json!("in_progress");
            events.push(self.emit("response.output_item.added", json!({"output_index": index, "item": added})));
            events.push(self.emit("response.function_call_arguments.delta", json!({"item_id": item["id"], "output_index": index, "delta": arguments})));
            events.push(self.emit("response.function_call_arguments.done", json!({"item_id": item["id"], "output_index": index, "arguments": arguments})));
            events.push(self.emit("response.output_item.done", json!({"output_index": index, "item": item})));
            index += 1;
        }
        let response = envelope(&self.id, &self.model, self.created, "completed", output(response, &self.id), Some(response));
        events.push(self.emit("response.completed", json!({"response": response})));
        events
    }

    fn failed(&mut self, message: &str) -> Vec<Frame> {
        let mut response = envelope(&self.id, &self.model, self.created, "failed", Vec::new(), None);
        response["error"] = json!({"code": "server_error", "message": message});
        vec![self.emit("response.failed", json!({"response": response}))]
    }
}

impl Responses {
    fn emit(&mut self, kind: &str, mut data: Value) -> Frame {
        data["type"] = json!(kind);
        data["sequence_number"] = json!(self.sequence);
        self.sequence += 1;
        event(Some(kind), &data)
    }

    fn message_id(&self) -> String {
        message_id(&self.id)
    }
}

fn message_id(response_id: &str) -> String {
    format!("msg_{}", response_id.trim_start_matches("resp_"))
}

fn now_s() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs()) // map_or: a clock before 1970 reads 0
}

fn envelope(id: &str, model: &str, created: u64, status: &str, output: Vec<Value>, response: Option<&TextGenerationResponse>) -> Value {
    json!({
        "id": id, "object": "response", "created_at": created, "status": status, "model": model,
        "output": output,
        "usage": response.map(|r| json!({"input_tokens": r.usage.input_tokens, "output_tokens": r.usage.output_tokens,
            "total_tokens": r.usage.input_tokens + r.usage.output_tokens})),
        "incomplete_details": response.filter(|r| r.finish_reason == continuum_engine::ai::FinishReason::Length && !stopped_for_tools(r))
            .map(|_| json!({"reason": "max_output_tokens"})),
    })
}

fn output(response: &TextGenerationResponse, id: &str) -> Vec<Value> {
    let mut output = Vec::new();
    if !response.text.is_empty() {
        output.push(json!({"type": "message", "id": message_id(id), "status": "completed", "role": "assistant",
            "content": [{"type": "output_text", "text": response.text, "annotations": []}]}));
    }
    output.extend(tool_calls(response).iter().map(function_call));
    output
}

fn function_call((call_id, name, input): &(String, String, Value)) -> Value {
    json!({"type": "function_call", "id": format!("fc_{call_id}"), "call_id": call_id, "name": name,
        "arguments": input.to_string(), "status": "completed"})
}

/// Input items to messages: consecutive function calls make one assistant turn, and each
/// call's output is a tool reply. Reasoning items are not sent back to the engine.
fn items_to_messages(items: &[Value]) -> Result<Vec<ChatMessage>, BadRequest> {
    let mut out: Vec<ChatMessage> = Vec::new();
    for item in items {
        let kind = item.get("type").and_then(Value::as_str).unwrap_or("message"); // unwrap_or: an item with only a role is a message
        match kind {
            "message" => {
                let role = item.get("role").and_then(Value::as_str).ok_or_else(|| BadRequest("a message item has no `role`".into()))?;
                let role = if role == "developer" { "system" } else { role };
                out.push(ChatMessage { role: role.to_owned(), content: content(item.get("content").unwrap_or(&Value::Null)), name: None });
            }
            "function_call" => {
                let arguments = item.get("arguments").and_then(Value::as_str).unwrap_or("{}"); // unwrap_or: a call with no arguments
                let call = ContentPart::ToolUse {
                    id: item.get("call_id").and_then(Value::as_str).ok_or_else(|| BadRequest("a function_call has no `call_id`".into()))?.to_owned(),
                    name: item.get("name").and_then(Value::as_str).ok_or_else(|| BadRequest("a function_call has no `name`".into()))?.to_owned(),
                    input: serde_json::from_str(arguments).unwrap_or_else(|_| Value::String(arguments.to_owned())), // unwrap_or_else: arguments that are not JSON pass through as written
                };
                match out.last_mut() {
                    Some(ChatMessage { role, content: MessageContent::Parts(parts), .. })
                        if role == "assistant" && parts.iter().any(|p| matches!(p, ContentPart::ToolUse { .. })) => parts.push(call),
                    _ => out.push(ChatMessage { role: "assistant".into(), content: MessageContent::Parts(vec![call]), name: None }),
                }
            }
            "function_call_output" => out.push(ChatMessage {
                role: "tool".into(),
                content: MessageContent::Parts(vec![ContentPart::ToolResult {
                    tool_use_id: item.get("call_id").and_then(Value::as_str).ok_or_else(|| BadRequest("a function_call_output has no `call_id`".into()))?.to_owned(),
                    content: item.get("output").map(text_of).unwrap_or_default(), // unwrap_or_default: an empty output
                    is_error: None,
                }]),
                name: None,
            }),
            _ => {} // reasoning and other items carry nothing the engine renders
        }
    }
    // The engine's templates take one system message, at the start (as in Chat Completions).
    let mut kept = Vec::with_capacity(out.len());
    for (i, mut m) in out.into_iter().enumerate() {
        if m.role == "system" && i > 0 {
            m.role = "user".into();
            if let MessageContent::Text(text) = &m.content {
                m.content = MessageContent::Text(format!("[system] {text}"));
            }
        }
        kept.push(m);
    }
    Ok(kept)
}

fn content(value: &Value) -> MessageContent {
    match value {
        Value::Array(parts) => MessageContent::Parts(
            parts
                .iter()
                .filter_map(|p| match p.get("type").and_then(Value::as_str)? {
                    "input_text" | "output_text" | "text" => Some(ContentPart::Text { text: p.get("text").and_then(Value::as_str)?.to_owned() }),
                    "input_image" => Some(ContentPart::Image { image: chat::image(p.get("image_url").and_then(Value::as_str)?) }),
                    _ => None,
                })
                .collect(),
        ),
        other => MessageContent::Text(text_of(other)),
    }
}

fn tool(value: &Value) -> Option<NativeToolSpec> {
    if value.get("type").and_then(Value::as_str) != Some("function") {
        return None; // built-in tools (web search, file search) are the provider's, not ours
    }
    Some(NativeToolSpec {
        name: value.get("name").and_then(Value::as_str)?.to_owned(),
        description: value.get("description").and_then(Value::as_str).unwrap_or_default().to_owned(), // unwrap_or_default: a tool may have no description
        input_schema: input_schema(value.get("parameters").unwrap_or(&json!({}))),
    })
}
