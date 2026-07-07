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
    /// `arguments` is the canonical key; `parameters` is accepted as an alias so
    /// Llama/Mistral-style bare calls (`{"name", "parameters"}`) normalize cleanly.
    #[serde(default, alias = "parameters")]
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

// The tool-exchange protocol enum is the ONE `model_registry::ToolProtocol`
// (#69) — it's catalog data a provider declares. The rendering/parsing
// BEHAVIOR lives here in `ai` (it depends on this module's `NativeToolSpec` /
// `ToolCall` / parser), hung off that type as an inherent impl. Same crate, so
// the impl is legal here even though the type is defined in `model_registry`.
use crate::model_registry::ToolProtocol;

impl ToolProtocol {
    /// The prompt block to inject when offering `tools`, or `None` when tools are
    /// offered via the API `tools` param (native) or not at all. Empty `tools`
    /// → `None`.
    pub fn tool_prompt(self, tools: &[NativeToolSpec]) -> Option<String> {
        match self {
            ToolProtocol::NativeFunctionCalling | ToolProtocol::None => None,
            ToolProtocol::JsonInPrompt if tools.is_empty() => None,
            ToolProtocol::JsonInPrompt => Some(render_tool_instructions(tools)),
        }
    }

    /// Extract a tool call from the model's TEXT response. `None` for
    /// `NativeFunctionCalling` (the adapter reads structured `tool_calls`
    /// instead), `None` for `None` (no tools), and when the model answered
    /// normally (no call).
    pub fn parse_text_call(self, text: &str) -> Option<ToolCall> {
        match self {
            ToolProtocol::NativeFunctionCalling | ToolProtocol::None => None,
            ToolProtocol::JsonInPrompt => parse_tool_call(text),
        }
    }
}

/// Render the tool-offering block injected into the prompt (as a system message)
/// when a JsonInPrompt model is offered tools. Lists each tool + its description
/// and states the EXACT JSON contract to emit. Kept terse — small models follow a
/// short, explicit contract far better than a verbose schema.
pub fn render_tool_instructions(tools: &[NativeToolSpec]) -> String {
    let mut s = String::with_capacity(256 + tools.len() * 160);
    s.push_str(
        "You can use tools. To call ONE tool, reply with ONLY this JSON object and \
         nothing else:\n\
         {\"tool_call\": {\"name\": \"<tool-name>\", \"arguments\": { <args> }}}\n\
         Use the EXACT argument field names listed for each tool — do NOT invent field \
         names (e.g. don't use \"command\"). After the tool result comes back, answer \
         normally. If no tool is needed, just answer normally (no JSON).\n\n\
         Available tools (name — description — arguments):\n",
    );
    for t in tools {
        s.push_str("- ");
        s.push_str(&t.name);
        s.push_str(": ");
        s.push_str(&t.description);
        // Render the argument FIELD NAMES + types so the model passes the right keys
        // instead of guessing — the deaf-hands fix. Without this the model only sees
        // an empty `arguments: { ... }` placeholder and invents fields.
        let req = t.input_schema.required.clone().unwrap_or_default();
        match t.input_schema.properties.as_object() {
            Some(props) if !props.is_empty() => {
                s.push_str("\n    arguments: ");
                let mut first = true;
                for (field, spec) in props {
                    if !first {
                        s.push_str(", ");
                    }
                    first = false;
                    let ty = spec.get("type").and_then(|v| v.as_str()).unwrap_or("any");
                    s.push_str(&format!("{field}: {ty}"));
                    if req.iter().any(|r| r == field) {
                        s.push_str(" (required)");
                    }
                }
            }
            _ => s.push_str("\n    arguments: {} (no arguments)"),
        }
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

/// Extract every tool call a base model emitted in its text, normalized to the one
/// canonical [`ToolCall`] — across the VARIETY of surface formats different base
/// models use. Tries each registered [`ToolCallFormat`] most-specific-first; the
/// first format that yields any calls wins (a model emits ONE format per turn, so
/// this avoids cross-format double-counting). Robust to ```json fences, prose,
/// `<think>` blocks, multiple calls, and a malformed sibling next to a valid one.
///
/// This is the flexible floor for continuum's dynamic model mix: the BASE MODEL
/// picks the format, the adapter adapts. Adding support for a new model's format is
/// ONE new `ToolCallFormat` impl — no change to the adapter or the agent loop
/// ([[polymorphism-pattern]]). A LoRA can later train a model toward our canonical
/// envelope, but until then a persona's hands work on any of these.
pub fn parse_tool_calls(text: &str) -> Vec<ToolCall> {
    for fmt in tool_call_formats() {
        let calls = fmt.parse(text);
        if !calls.is_empty() {
            return calls;
        }
    }
    Vec::new()
}

/// One way a base model surfaces a tool call in text. Pure parsing → canonical
/// [`ToolCall`]s; no model/gateway coupling. The registry ([`tool_call_formats`])
/// is the single place the supported variety lives.
trait ToolCallFormat: Send + Sync {
    /// Stable id (telemetry / "which format matched").
    fn id(&self) -> &'static str;
    /// Extract any calls in THIS format from `text` (empty = not this format).
    fn parse(&self, text: &str) -> Vec<ToolCall>;
}

/// The supported surface formats, most-specific first. Extend this list to support
/// a new base model — the only edit a new format requires.
fn tool_call_formats() -> &'static [&'static dyn ToolCallFormat] {
    &[&EnvelopeFormat, &TaggedFormat, &BareFormat, &NarratedWriteFormat]
}

/// `{"tool_call": {"name": "...", "arguments": {...}}}` — continuum's injected
/// JsonInPrompt contract (and what a LoRA will be trained to emit).
struct EnvelopeFormat;
impl ToolCallFormat for EnvelopeFormat {
    fn id(&self) -> &'static str {
        "envelope"
    }
    fn parse(&self, text: &str) -> Vec<ToolCall> {
        scan_objects(text, |obj| {
            serde_json::from_str::<ToolCallEnvelope>(obj)
                .ok()
                .map(|e| e.tool_call)
                .filter(|c| !c.name.trim().is_empty())
                .map(Into::into)
        })
    }
}

/// `<tool_call>{...}</tool_call>` — Qwen / Hermes / NousResearch style. Strips the
/// tags and parses the inner object as a bare or enveloped call.
struct TaggedFormat;
impl ToolCallFormat for TaggedFormat {
    fn id(&self) -> &'static str {
        "tagged"
    }
    fn parse(&self, text: &str) -> Vec<ToolCall> {
        let mut out = Vec::new();
        let mut rest = text;
        while let Some(open) = rest.find("<tool_call>") {
            let after = &rest[open + "<tool_call>".len()..];
            let inner = match after.find("</tool_call>") {
                Some(close) => &after[..close],
                None => after, // unterminated tag — take the remainder
            };
            // Inner is usually bare `{name,arguments}`; tolerate an envelope too.
            out.extend(EnvelopeFormat.parse(inner));
            if out.is_empty() {
                out.extend(BareFormat.parse(inner));
            }
            rest = match after.find("</tool_call>") {
                Some(close) => &after[close + "</tool_call>".len()..],
                None => "",
            };
        }
        out
    }
}

/// Bare `{"name": "...", "arguments"|"parameters": {...}}` — Llama / Mistral /
/// generic. Requires an `arguments`/`parameters` key present so prose containing a
/// stray `{"name": ...}` isn't mistaken for a call (tried LAST, so our envelope and
/// tagged models never reach it).
struct BareFormat;
impl ToolCallFormat for BareFormat {
    fn id(&self) -> &'static str {
        "bare"
    }
    fn parse(&self, text: &str) -> Vec<ToolCall> {
        scan_objects(text, |obj| {
            if !obj.contains("\"arguments\"") && !obj.contains("\"parameters\"") {
                return None; // not a tool call — avoid false positives
            }
            serde_json::from_str::<ToolCallJson>(obj)
                .ok()
                .filter(|c| !c.name.trim().is_empty())
                .map(Into::into)
        })
    }
}

/// A model that NARRATES a file write instead of emitting the JSON envelope — e.g.
/// `code/write with file_path="lru.rs" and content: ```rust\n<code>\n``` ` — observed
/// live from Qwen2.5-Coder-32B on a real task. The code IS there and the intent is
/// unambiguous; only the machine-readable envelope is missing, so the file never lands
/// and the turn scores `acts=0` (the "describe acting instead of acting" reflex). This
/// LAST-RESORT format recovers that attempt into a real `code/write`. It fires ONLY when
/// the text names `code/write` AND carries a fenced code block AND a `file_path`, so a
/// model that emits proper JSON never reaches it and stray prose can't false-trigger.
/// (A LoRA trains the reflex away; until then her hands work when she describes the call.)
struct NarratedWriteFormat;
impl ToolCallFormat for NarratedWriteFormat {
    fn id(&self) -> &'static str {
        "narrated-write"
    }
    fn parse(&self, text: &str) -> Vec<ToolCall> {
        if !text.contains("code/write") {
            return Vec::new();
        }
        let (Some(path), Some(content)) =
            (extract_file_path(text), extract_first_code_fence(text))
        else {
            return Vec::new();
        };
        if content.trim().is_empty() {
            return Vec::new();
        }
        vec![ToolCall {
            id: format!("jip-{}", Uuid::new_v4()),
            name: "code/write".to_string(),
            input: serde_json::json!({ "file_path": path, "content": content }),
        }]
    }
}

/// Pull `file_path`'s value out of prose: `file_path="x"`, `file_path=x`,
/// `file_path: x`, `file_path x`. Value ends at a quote, whitespace, or comma.
fn extract_file_path(text: &str) -> Option<String> {
    let idx = text.find("file_path")?;
    let after =
        text[idx + "file_path".len()..].trim_start_matches([' ', '=', ':', '"', '\'']);
    let end = after
        .find(|c: char| c == '"' || c == '\'' || c.is_whitespace() || c == ',')
        .unwrap_or(after.len());
    let path = after[..end].trim();
    (!path.is_empty()).then(|| path.to_string())
}

/// The body of the FIRST ```…``` fenced block, with the optional language line stripped.
fn extract_first_code_fence(text: &str) -> Option<String> {
    let open = text.find("```")?;
    let after = &text[open + 3..];
    // Skip the language token on the opening fence line (```rust\n → drop "rust\n").
    let body = match after.find('\n') {
        Some(nl) => &after[nl + 1..],
        None => after,
    };
    let close = body.find("```")?;
    Some(body[..close].to_string())
}

/// Scan `text` for balanced `{...}` objects (ignoring braces inside JSON strings),
/// applying `f` to each; collect the `Some` results, skipping past a matched object
/// so its insides aren't re-scanned. A malformed (unbalanced) object is skipped and
/// scanning resumes after it. Shared by the object-based formats.
fn scan_objects<F>(text: &str, f: F) -> Vec<ToolCall>
where
    F: Fn(&str) -> Option<ToolCall>,
{
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'{' {
            if let Some(end) = matching_brace_end(bytes, i) {
                if let Some(call) = f(&text[i..=end]) {
                    out.push(call);
                    i = end + 1; // consume this object; don't re-scan its insides
                    continue;
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
                definitions: None,
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

    // what this catches: the narrate-instead-of-act reflex — Qwen-Coder-32B on a real
    // task emitted `code/write with file_path="..." and content: ```rust\n<code>\n``` `
    // as PROSE (no JSON envelope), so acts=0 and the file never landed. The last-resort
    // NarratedWriteFormat must recover it into a real code/write with the fenced code.
    #[test]
    fn recovers_a_narrated_code_write_with_fenced_code() {
        let text = "Sure — I'll use code/write with file_path=\"lru.rs\" and content of \
                    the implementation:\n\n```rust\npub struct LruCache;\nimpl LruCache { }\n```";
        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 1, "narrated write should parse to one call");
        assert_eq!(calls[0].name, "code/write");
        assert_eq!(calls[0].input["file_path"], "lru.rs");
        assert!(
            calls[0].input["content"].as_str().unwrap().contains("pub struct LruCache"),
            "the fenced code must become the write content"
        );
    }

    // what this catches: the last-resort format must NOT false-trigger on prose that
    // merely mentions code/write without an actual file + fenced code to write.
    #[test]
    fn narrated_write_ignores_prose_without_a_fence() {
        assert!(parse_tool_calls("I could use code/write to save file_path=x.rs later.").is_empty());
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

    // what this catches: the DYNAMIC model variety — Qwen/Hermes <tool_call> tags
    // around a bare {name,arguments}. The registry's TaggedFormat normalizes it to
    // the same canonical ToolCall, so a different base model "just works".
    #[test]
    fn parses_qwen_style_tagged_call() {
        let q = "<tool_call>\n{\"name\": \"code/read\", \"arguments\": {\"file_path\": \"x.rs\"}}\n</tool_call>";
        let tc = parse_tool_call(q).expect("tagged call");
        assert_eq!(tc.name, "code/read");
        assert_eq!(tc.input["file_path"], "x.rs");
    }

    // what this catches: Llama/Mistral-style BARE call with `parameters` (not
    // `arguments`). BareFormat + the `parameters` alias normalize it.
    #[test]
    fn parses_llama_style_bare_parameters_call() {
        let l = r#"sure, calling: {"name": "code/search", "parameters": {"pattern": "todo"}}"#;
        let tc = parse_tool_call(l).expect("bare call");
        assert_eq!(tc.name, "code/search");
        assert_eq!(tc.input["pattern"], "todo");
    }

    // what this catches: NO false positive — prose mentioning a JSON object with a
    // `name` but no arguments/parameters is NOT treated as a tool call (the bare
    // format's guard). A persona musing "my name is Asha" must not fire a tool.
    #[test]
    fn prose_with_a_name_field_is_not_a_tool_call() {
        assert!(parse_tool_calls(r#"I think {"name": "Asha"} is a nice handle."#).is_empty());
        assert!(parse_tool_call("just answering normally, no tools today").is_none());
    }
}
