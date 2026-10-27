//! The APIs agents speak, each an adapter holding only its diff.
//!
//! A format decodes the agent's body into one [`Call`] and encodes the engine's answer back,
//! whole or as a stream. Everything they share (selection, generation, the SSE transport,
//! errors) lives once in `route` and `server`.

pub(crate) mod anthropic;
pub(crate) mod chat;
pub(crate) mod responses;

use continuum_engine::ai::adapter::GenerationChunk;
use continuum_engine::ai::{ContentPart, FinishReason, TextGenerationRequest, TextGenerationResponse};
use serde_json::Value;

/// One decoded call: the engine's request, and whether the agent wants it streamed.
pub(crate) struct Call {
    pub request: TextGenerationRequest,
    pub stream: bool,
}

/// A body the format cannot read, with the reason the agent is told.
#[derive(Debug)]
pub(crate) struct BadRequest(pub String);

/// One API an agent speaks. `Self` is the encoder of one streamed answer.
pub(crate) trait Wire: Send + 'static {
    /// The format's name, for probes.
    const NAME: &'static str;
    fn decode(body: Value) -> Result<Call, BadRequest>;
    /// The whole answer, for a call that did not ask to stream.
    fn answer(response: &TextGenerationResponse, id: &str) -> Value;
    /// The body of a refusal, in the format's own error shape.
    fn error(kind: ErrorKind, message: &str) -> Value;
    /// A new encoder for one streamed answer.
    fn streamer(model: &str, id: &str) -> Self;
    /// What the stream opens with, before any token.
    fn opened(&mut self) -> Vec<Frame>;
    fn chunk(&mut self, chunk: &GenerationChunk) -> Vec<Frame>;
    /// What closes a completed answer: its tool calls, stop reason and usage.
    fn finished(&mut self, response: &TextGenerationResponse) -> Vec<Frame>;
    /// What closes an answer the engine failed mid-stream.
    fn failed(&mut self, message: &str) -> Vec<Frame>;
}

/// The refusals every format can express.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ErrorKind {
    InvalidRequest,
    NotFound,
    Engine,
}

/// One server-sent event of a streamed answer: its name, if the format names events, and data.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Frame {
    pub name: Option<String>,
    pub data: String,
}

impl Frame {
    /// The OpenAI terminator.
    pub(crate) fn done() -> Self {
        Self { name: None, data: "[DONE]".into() }
    }

    pub(crate) fn into_event(self) -> axum::response::sse::Event {
        let event = axum::response::sse::Event::default().data(self.data);
        match self.name {
            Some(name) => event.event(name),
            None => event,
        }
    }

    #[cfg(test)]
    pub(crate) fn json(&self) -> Value {
        serde_json::from_str(&self.data).expect("test: a JSON frame")
    }
}

/// A JSON frame, optionally named.
pub(crate) fn event(name: Option<&str>, data: &Value) -> Frame {
    Frame { name: name.map(str::to_owned), data: data.to_string() }
}

/// The tool calls of an answer as (id, name, input), from either place the engine puts them.
pub(crate) fn tool_calls(response: &TextGenerationResponse) -> Vec<(String, String, Value)> {
    if let Some(calls) = &response.tool_calls {
        return calls.iter().map(|c| (c.id.clone(), c.name.clone(), c.input.clone())).collect();
    }
    response
        .content
        .iter()
        .flatten()
        .filter_map(|part| match part {
            ContentPart::ToolUse { id, name, input } => Some((id.clone(), name.clone(), input.clone())),
            _ => None,
        })
        .collect()
}

/// Whether the answer stopped to call tools.
pub(crate) fn stopped_for_tools(response: &TextGenerationResponse) -> bool {
    response.finish_reason == FinishReason::ToolUse || !tool_calls(response).is_empty()
}

/// A text field that is a string, or an array of `{type: "text", text}` blocks.
pub(crate) fn text_of(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(blocks) => blocks.iter().filter_map(|b| b.get("text").and_then(Value::as_str)).collect::<Vec<_>>().join("\n"),
        _ => String::new(),
    }
}

/// A JSON-schema object as the engine's tool input schema.
pub(crate) fn input_schema(schema: &Value) -> continuum_engine::ai::ToolInputSchema {
    continuum_engine::ai::ToolInputSchema {
        schema_type: "object".into(),
        properties: schema.get("properties").cloned().unwrap_or_else(|| Value::Object(Default::default())), // unwrap_or_else: a tool with no parameters has empty properties
        required: schema
            .get("required")
            .and_then(Value::as_array)
            .map(|r| r.iter().filter_map(Value::as_str).map(str::to_owned).collect()),
        definitions: schema.get("$defs").or_else(|| schema.get("definitions")).cloned(),
    }
}

/// The sampling fields every format names the same way: temperature, top_p, stop.
pub(crate) fn sampling(body: &Value, request: &mut TextGenerationRequest) {
    request.temperature = body.get("temperature").and_then(Value::as_f64).map(|t| t as f32);
    request.top_p = body.get("top_p").and_then(Value::as_f64).map(|t| t as f32);
    request.stop_sequences = match body.get("stop").or_else(|| body.get("stop_sequences")) {
        Some(Value::String(s)) => Some(vec![s.clone()]),
        Some(Value::Array(a)) => Some(a.iter().filter_map(Value::as_str).map(str::to_owned).collect()),
        _ => None,
    };
}

#[cfg(test)]
mod tests;
