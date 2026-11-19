//! A text turn through the model's own chat contract: the messages and tools serving sends a
//! `/v1` server (`request_body::wire_messages`, `request_body::openai_tools`), rendered by the
//! model's jinja template, and the reply read back with that template's own parser. The contract a
//! resident had on llama-server, in process: her tool calls come back structured, in the model's
//! native syntax, never guessed out of prose.

use super::*;
use crate::ai::types::ToolCall;
use llama::{ChatPrompt, ChatTemplates};

/// A rendered turn and the parser its reply is read with.
pub(super) struct NativeTurn<'b> {
    templates: &'b ChatTemplates,
    pub(super) prompt: ChatPrompt,
}

/// The reply as the model's own parser read it.
pub(super) struct NativeReply {
    pub(super) text: String,
    pub(super) reasoning: Option<String>,
    pub(super) tool_calls: Vec<ToolCall>,
}

/// Render `request` through the model's template with its tools. `enable_thinking` is the turn's
/// think term (off only when it states a zero budget).
pub(super) fn render<'b>(
    backend: &'b LlamaCppBackend,
    request: &TextGenerationRequest,
    enable_thinking: bool,
) -> Result<NativeTurn<'b>, String> {
    let templates = backend.chat_templates()?;
    let messages = crate::inference::request_body::wire_messages(
        &request.messages,
        request.system_prompt.as_deref(),
        false,
        false,
        LLAMACPP_PROVIDER_ID,
    );
    let tools = request
        .tools
        .as_deref()
        .map(crate::inference::request_body::openai_tools)
        .unwrap_or_default(); // unwrap_or_default: a turn offered no tools renders with none
    let messages_json = serde_json::to_string(&messages).map_err(|e| format!("native chat: messages: {e}"))?; // boundary: llama.cpp's OpenAI-compatible chat input
    let tools_json = serde_json::to_string(&tools).map_err(|e| format!("native chat: tools: {e}"))?; // boundary: llama.cpp's OpenAI-compatible tools input
    let prompt = templates
        .apply(&messages_json, &tools_json, enable_thinking)
        .map_err(|refusal| format!("native chat: the model's template refused this turn: {refusal}"))?;
    Ok(NativeTurn { templates, prompt })
}

/// The text of her prompt that her next act will hold unchanged: her view's messages
/// (`request.view_messages`, where cognition says her next act stops matching), rendered by the
/// model's own template with its generation prompt taken off. The engine checkpoints her there, so
/// a next act that keeps it prefills only what follows (CONTINUUM-ENGINE 4.15). `None` for a turn
/// with no view, or a render that does not end in its own generation prompt.
pub(super) fn stable_text(backend: &LlamaCppBackend, request: &TextGenerationRequest, enable_thinking: bool, full: &ChatPrompt) -> Option<String> {
    let held = usize::try_from(request.view_messages?).ok()?;
    let rendered = if held >= request.messages.len() {
        full.clone()
    } else {
        let view = TextGenerationRequest { messages: request.messages[..held].to_vec(), ..request.clone() };
        render(backend, &view, enable_thinking).ok()?.prompt
    };
    rendered.prompt.strip_suffix(rendered.generation_prompt.as_str()).map(str::to_owned)
}

impl NativeTurn<'_> {
    /// Read a finished generation with the template's own parser.
    pub(super) fn read(&self, output: &str) -> Result<NativeReply, String> {
        let parsed = self
            .templates
            .parse(output, &self.prompt)
            .map_err(|refusal| format!("native chat: the model's parser refused its own reply: {refusal}"))?;
        let mut tool_calls = Vec::with_capacity(parsed.tool_calls.len());
        for (index, call) in parsed.tool_calls.into_iter().enumerate() {
            let input: serde_json::Value = serde_json::from_str(&call.function.arguments).map_err(|e| { // boundary: the model's tool-call arguments, a JSON object's text
                format!("native chat: {}'s arguments are not JSON ({e}): {}", call.function.name, call.function.arguments)
            })?;
            tool_calls.push(ToolCall {
                id: call.id.filter(|id| !id.is_empty()).unwrap_or_else(|| format!("call_{index}")), // unwrap_or_else: a template that names no call id gets its position
                name: call.function.name,
                input,
            });
        }
        Ok(NativeReply {
            text: parsed.content.unwrap_or_default(), // unwrap_or_default: a reply that is only a call has no prose
            reasoning: parsed.reasoning_content.filter(|r| !r.is_empty()),
            tool_calls,
        })
    }
}
