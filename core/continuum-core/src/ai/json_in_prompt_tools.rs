//! Prompt-based tool calling (`JsonInPrompt`) — model-agnostic tools for models
//! whose gateway does NOT implement native OpenAI function-calling.
//!
//! PROVEN 2026-06-21: unsloth + the qwen GGUF ignores the `tools`/`tool_choice`
//! params (returns `tool_calls: null`, the model just narrates). Many local models
//! are like this until fine-tuned. So instead of relying on native function
//! calling, we describe the tools IN THE PROMPT and ask the model to emit a strict
//! JSON tool call, then PARSE it back into the same [`ToolCall`] the agent loop
//! already executes — i.e. we make prompt-based tools look native to the rest of
//! cognition.
//!
//! This is one half of the adapter strategy: the adapter picks the tool protocol
//! by MODEL (native when the model+gateway truly do it, JsonInPrompt otherwise),
//! and either way returns `FinishReason::ToolUse` + `tool_calls`. Never hardcoded
//! to one model — the protocol is a per-adapter property.
//!
//! The two pure pieces (TDD'd here, called by the adapter):
//!   - [`render_tool_instructions`] — the prompt block injected when offering tools.
//!   - [`parse_tool_call`] — extract the model's JSON tool call from messy output.

use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

use super::types::{NativeToolSpec, ToolCall};

/// The strict JSON shape the model is asked to emit to call a tool:
/// `{"tool_call": {"name": "...", "arguments": { ... }}}`. The envelope key
/// (`tool_call`) makes the intent unambiguous to parse out of surrounding prose.
#[derive(Debug, Deserialize)]
struct ToolCallEnvelope {
    tool_call: ToolCallJson,
}

/// The inner call: which tool + its arguments. `arguments` defaults to `{}` so a
/// no-arg tool can be called as `{"tool_call": {"name": "ping"}}`.
#[derive(Debug, Deserialize)]
struct ToolCallJson {
    name: String,
    #[serde(default)]
    arguments: Value,
}

/// Format adaptation, Rust-idiomatic: the model's emitted shape → our canonical
/// [`ToolCall`]. A fresh id (the agent loop correlates results by it) + `input` =
/// the model's `arguments` (null → `{}`).
impl From<ToolCallJson> for ToolCall {
    fn from(j: ToolCallJson) -> Self {
        ToolCall {
            id: format!("jip-{}", Uuid::new_v4()),
            name: j.name.trim().to_string(),
            input: match j.arguments {
                Value::Null => serde_json::json!({}),
                other => other,
            },
        }
    }
}

/// How a given model exchanges tool calls — a protocol-driven interface the
/// adapter delegates to, selected PER MODEL (never hardcoded). `Native` = the
/// gateway/model does real OpenAI function-calling (offer via the API `tools`
/// param, read `tool_calls` back). `JsonInPrompt` = describe tools in the prompt
/// + parse a JSON call from the text (the universal floor for models that ignore
/// the `tools` param, like unsloth+GGUF today). Adding a model = data; swapping a
/// gateway = a new variant — not a rewrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ToolProtocol {
    /// The model+gateway implement OpenAI function-calling natively.
    #[default]
    Native,
    /// Tools are offered in the prompt; calls are parsed from the response text.
    JsonInPrompt,
}

impl ToolProtocol {
    /// The prompt block to inject when offering `tools`, or `None` when tools are
    /// offered via the API `tools` param (native). Empty `tools` → `None`.
    pub fn tool_prompt(self, tools: &[NativeToolSpec]) -> Option<String> {
        match self {
            ToolProtocol::Native => None,
            ToolProtocol::JsonInPrompt if tools.is_empty() => None,
            ToolProtocol::JsonInPrompt => Some(render_tool_instructions(tools)),
        }
    }

    /// Extract a tool call from the model's TEXT response. `None` for `Native`
    /// (the adapter reads structured `tool_calls` instead) and when the model
    /// answered normally (no call).
    pub fn parse_text_call(self, text: &str) -> Option<ToolCall> {
        match self {
            ToolProtocol::Native => None,
            ToolProtocol::JsonInPrompt => parse_tool_call(text),
        }
    }
}

/// Render the tool-offering block injected into the prompt (as a system message)
/// when a JsonInPrompt model is offered tools. Lists each tool + its description
/// and states the EXACT JSON contract to emit. Kept terse — small models follow a
/// short, explicit contract far better than a verbose schema.
pub fn render_tool_instructions(tools: &[NativeToolSpec]) -> String {
    let mut s = String::with_capacity(256 + tools.len() * 96);
    s.push_str(
        "You can use tools. To call ONE tool, reply with ONLY this JSON object and \
         nothing else:\n\
         {\"tool_call\": {\"name\": \"<tool-name>\", \"arguments\": { ... }}}\n\
         After the tool result comes back, answer normally. If no tool is needed, \
         just answer normally (no JSON).\n\n\
         Available tools:\n",
    );
    for t in tools {
        s.push_str("- ");
        s.push_str(&t.name);
        s.push_str(": ");
        s.push_str(&t.description);
        s.push('\n');
    }
    s
}

/// Parse a model response for a JsonInPrompt tool call. Robust to the mess real
/// models emit: surrounding prose, ```json fences, `<think>` blocks, trailing
/// commentary. Scans for balanced `{...}` candidates and returns the first that
/// deserializes to the `{"tool_call": {...}}` envelope. Returns `None` when the
/// model chose to answer normally (no tool call) — that's the common, valid case.
///
/// The synthesized [`ToolCall`] gets a fresh id (the agent loop correlates results
/// by it) and `input` = the model's `arguments` (defaulting to `{}`).
pub fn parse_tool_call(text: &str) -> Option<ToolCall> {
    parse_tool_calls(text).into_iter().next()
}

/// Extract ALL tool-call envelopes a model emitted in its text — robust to the
/// mess real base models produce: ```json fences, surrounding prose, `<think>`
/// blocks, MULTIPLE calls in one turn, and a MALFORMED sibling (e.g. a call with a
/// missing closing brace) next to a valid one. A malformed call is skipped; every
/// well-formed `{"tool_call": {...}}` is returned, in order, de-overlapped.
///
/// This is the flexible floor: the BASE MODEL picks the surface format, the adapter
/// adapts. (A LoRA can later make the model emit native calls; until then this
/// keeps a persona's hands working regardless of how the model phrases the call.)
pub fn parse_tool_calls(text: &str) -> Vec<ToolCall> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if let Some(end) = matching_brace_end(bytes, i) {
                if let Ok(env) = serde_json::from_str::<ToolCallEnvelope>(&text[i..=end]) {
                    if !env.tool_call.name.trim().is_empty() {
                        out.push(env.tool_call.into()); // From<ToolCallJson> for ToolCall
                        i = end + 1; // consume this envelope; don't re-scan its insides
                        continue;
                    }
                }
            }
        }
        i += 1;
    }
    out
}

/// Yield substrings of `text` that are balanced `{...}` objects, outermost-first
/// at each start position — so a `{"tool_call": {...}}` envelope is tried before
/// its inner `{...}`. Brace-depth scan that ignores braces inside JSON strings
/// (so `{"k":"}"}` doesn't fool it). Cheap; the candidate set is tiny in practice.
fn json_object_candidates(text: &str) -> Vec<&str> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if let Some(end) = matching_brace_end(bytes, i) {
                out.push(&text[i..=end]);
                // Continue scanning AFTER this object's open brace so nested/later
                // objects are still considered, but we tried the outermost first.
            }
        }
        i += 1;
    }
    out
}

/// Index of the `}` matching the `{` at `start`, respecting JSON string literals
/// and escapes. `None` if unbalanced (truncated output).
fn matching_brace_end(bytes: &[u8], start: usize) -> Option<usize> {
    let mut depth = 0i32;
    let mut in_str = false;
    let mut escaped = false;
    let mut i = start;
    while i < bytes.len() {
        let c = bytes[i];
        if in_str {
            if escaped {
                escaped = false;
            } else if c == b'\\' {
                escaped = true;
            } else if c == b'"' {
                in_str = false;
            }
        } else {
            match c {
                b'"' => in_str = true,
                b'{' => depth += 1,
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        return Some(i);
                    }
                }
                _ => {}
            }
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::types::ToolInputSchema;

    fn spec(name: &str, desc: &str) -> NativeToolSpec {
        NativeToolSpec {
            name: name.to_string(),
            description: desc.to_string(),
            input_schema: ToolInputSchema {
                schema_type: "object".to_string(),
                properties: serde_json::json!({}),
                required: None,
            },
        }
    }

    // what this catches: the instruction block names every tool + its description
    // and states the exact JSON contract — what a small model needs to call a tool.
    #[test]
    fn instructions_list_tools_and_the_contract() {
        let s = render_tool_instructions(&[spec("ping", "Health check.")]);
        assert!(s.contains("\"tool_call\""));
        assert!(s.contains("ping: Health check."));
    }

    // what this catches: a clean bare JSON tool call parses to a ToolCall with the
    // right name + args (the happy path).
    #[test]
    fn parses_bare_tool_call() {
        let tc = parse_tool_call(r#"{"tool_call": {"name": "data/list", "arguments": {"collection": "rooms"}}}"#)
            .expect("a tool call");
        assert_eq!(tc.name, "data/list");
        assert_eq!(tc.input["collection"], "rooms");
        assert!(tc.id.starts_with("jip-"));
    }

    // what this catches: THE real-model mess — prose + <think> + a ```json fence
    // around the call. Must still extract it (models never emit bare JSON).
    #[test]
    fn parses_through_prose_think_and_fences() {
        let text = "<think>\nI should check health.\n</think>\nSure! Here:\n```json\n{\"tool_call\": {\"name\": \"ping\"}}\n```\n";
        let tc = parse_tool_call(text).expect("tool call through the mess");
        assert_eq!(tc.name, "ping");
        // no-arg call → arguments default to {}
        assert_eq!(tc.input, serde_json::json!({}));
    }

    // what this catches: a normal answer (no tool call) returns None — the common
    // case; we must NOT fabricate a call from ordinary prose.
    #[test]
    fn no_tool_call_in_plain_answer() {
        assert!(parse_tool_call("The deploy looks healthy; nothing to do.").is_none());
        // an unrelated JSON object is not a tool_call envelope
        assert!(parse_tool_call(r#"Here's data: {"status": "ok", "count": 3}"#).is_none());
    }

    // what this catches: brace-in-string doesn't fool the balanced scan, and the
    // outer envelope is preferred over any inner object.
    #[test]
    fn handles_braces_in_strings() {
        let tc = parse_tool_call(r#"{"tool_call": {"name": "chat/send", "arguments": {"text": "use {curly} braces"}}}"#)
            .expect("call");
        assert_eq!(tc.name, "chat/send");
        assert_eq!(tc.input["text"], "use {curly} braces");
    }

    // what this catches: THE live failure (Asha, 2026-06-22, seen via the prompt
    // capture) — a base model emitted a MALFORMED first call (missing a closing
    // brace) followed by a VALID second, wrapped in ``` fences. The old
    // single-shot parser returned None and the calls never executed (her hands
    // went dead). parse_tool_calls must skip the malformed one and still extract
    // the valid one, so a persona's agency survives messy model output.
    #[test]
    fn recovers_valid_call_next_to_a_malformed_sibling_in_fences() {
        let messy = "{\"tool_call\": {\"name\": \"code/read\", \"arguments\": {\"path\": \"df11d4c4\"}}\n\
                     ```\n\
                     {\"tool_call\": {\"name\": \"code/write\", \"arguments\": {\"path\": \"df11d4c4\", \"content\": \"it's claimed\"}}}\n\
                     ```";
        let calls = parse_tool_calls(messy);
        assert!(
            calls.iter().any(|c| c.name == "code/write"),
            "the well-formed call must be recovered despite the malformed sibling: {calls:?}"
        );
        // The single-shot accessor also yields a call now (not None).
        assert!(parse_tool_call(messy).is_some(), "single-shot must no longer return None");
    }

    // what this catches: MULTIPLE well-formed calls in one turn are all returned,
    // in order — a base model that batches tool calls gets all of them executed.
    #[test]
    fn extracts_multiple_well_formed_calls() {
        let two = r#"{"tool_call": {"name": "code/read", "arguments": {"file_path": "a.rs"}}}
        and then
        {"tool_call": {"name": "code/search", "arguments": {"pattern": "fn main"}}}"#;
        let calls = parse_tool_calls(two);
        assert_eq!(calls.len(), 2, "both calls extracted: {calls:?}");
        assert_eq!(calls[0].name, "code/read");
        assert_eq!(calls[1].name, "code/search");
    }
}
