//! The APIs agents speak, each an adapter holding only its diff.
//!
//! A format decodes the agent's body into one [`Call`] and encodes the engine's answer back,
//! whole or as a stream. Everything they share lives once: the checked field decoders in
//! [`decode`], selection, generation, the SSE transport and errors in `route` and `server`.

pub(crate) mod anthropic;
pub(crate) mod chat;
pub(crate) mod decode;
pub(crate) mod responses;

use continuum_engine::ai::adapter::GenerationChunk;
use continuum_engine::ai::{ChatMessage, ContentPart, FinishReason, MessageContent, TextGenerationRequest, TextGenerationResponse};
use serde_json::Value;

/// One decoded call: the engine's request, and whether the agent wants it streamed.
pub(crate) struct Call {
    pub request: TextGenerationRequest,
    pub stream: bool,
}

/// A body the format cannot read, with the reason the agent is told.
#[derive(Debug)]
pub(crate) struct BadRequest(pub String);

/// One API an agent speaks. `Self` is the encoder of one streamed answer, held in place by
/// the answer's body ([`crate::answer::Answer`]).
pub(crate) trait Wire: Send + Unpin + 'static {
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
    /// The call's capture cannot be opened: refused rather than served unrecorded.
    CaptureUnavailable,
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

/// Where system text may go. The engine's chat templates accept one system message, first
/// (Qwen's raises "System message must be at the beginning"), and agents send more: Claude
/// Code a `system` message after the user's turn (its hook and environment context), Codex
/// `developer` items, Hermes mid-run notices. System and developer messages before any
/// turn join the system prompt; a later one stays in order as a user turn marked as system
/// text. One rule for every format.
pub(crate) fn place_system(system_prompt: Option<String>, messages: Vec<ChatMessage>) -> (Option<String>, Vec<ChatMessage>) {
    let mut system = system_prompt.filter(|s| !s.is_empty());
    let mut out = Vec::with_capacity(messages.len());
    for message in messages {
        if message.role != "system" && message.role != "developer" {
            out.push(message);
            continue;
        }
        let text = plain_text(&message.content);
        if out.is_empty() {
            system = Some(match system {
                Some(head) => format!("{head}\n\n{text}"),
                None => text,
            });
        } else {
            out.push(ChatMessage { role: "user".into(), content: MessageContent::Text(format!("[system] {text}")), name: None });
        }
    }
    (system, out)
}

/// The text of a message's content, its text parts joined.
pub(crate) fn plain_text(content: &MessageContent) -> String {
    match content {
        MessageContent::Text(text) => text.clone(),
        MessageContent::Parts(parts) => parts
            .iter()
            .filter_map(|p| match p {
                ContentPart::Text { text } => Some(text.as_str()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join("\n"),
    }
}

/// Prompt tokens the engine reused from a warm page this call, when it measured them.
pub(crate) fn cached_tokens(response: &TextGenerationResponse) -> Option<u32> {
    response.timing.as_ref().map(|t| t.cached_tokens)
}

#[cfg(test)]
mod tests;
