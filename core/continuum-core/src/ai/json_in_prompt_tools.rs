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
    /// `name` is canonical; `function`/`tool` are accepted as aliases — observed
    /// live 2026-07-10: Asha, after a day of narrating, finally emitted a
    /// STRUCTURED call — `{"function": "work/claim", "params": {…}}` — and the
    /// parser rejected it on key SPELLING alone ([[pass-must-be-trained-not-told]]:
    /// check the action parser before declaring a model gap; she DID emit it).
    #[serde(alias = "function", alias = "tool")]
    name: String,
    /// `arguments` is canonical; `parameters`/`params`/`input` are aliases so
    /// Llama/Mistral/OpenAI-ish bare calls all normalize cleanly.
    #[serde(default, alias = "parameters", alias = "params", alias = "input")]
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
    &[
        &MistralToolCallsFormat,
        &EnvelopeFormat,
        &TaggedFormat,
        &BareFormat,
        &BbcodeCallFormat,
        &BracketTagFormat,
        &FencedCallFormat,
        &CliFlagFormat,
        &NarratedWriteFormat,
        &NarratedScriptFormat,
        &NarratedBareArgsFormat,
    ]
}

/// Mistral-family native tool-call marker: the model prefixes its call with the
/// literal `[TOOL_CALLS]` token (Mistral/Devstral's trained format). llama-server
/// leaves it in the CONTENT with `finish=stop` (it isn't the OpenAI `tool_calls`
/// field), so ONLY text-parsing catches it — and without this, EVERY Devstral tool
/// call silently no-ops (glass-boxed 2026-07-14: cognition/eval 1/13 on tool-requiring
/// tasks, `acts:0`, answers full of `[TOOL_CALLS]code/search(...)` that never fired —
/// the persona rendered handless, the real "system tax" on tool tasks).
///
/// The payload after the marker is a call in a form the OTHER formats already handle:
/// the Mistral canonical JSON array `[{"name":"code/list","arguments":{…}}]`, a
/// paren-call `code/search({…})`, or a bare slash-token `code/list`. So: strip the
/// marker(s) and delegate the tail to the rest of the registry. A `[TOOL_CALLS]` that
/// precedes a NON-call (`[active-work]`, `[recall]` — reserved vocab, no slash) yields
/// nothing, exactly as it must. Runs FIRST: the marker is the strongest signal.
struct MistralToolCallsFormat;
impl ToolCallFormat for MistralToolCallsFormat {
    fn id(&self) -> &'static str {
        "mistral-tool-calls"
    }
    fn parse(&self, text: &str) -> Vec<ToolCall> {
        if !text.contains("[TOOL_CALLS]") {
            return Vec::new();
        }
        // Drop every marker so the tail is clean and this format can't re-trigger on
        // the delegated parse. What remains is the call in a supported sub-format.
        let cleaned = text.replace("[TOOL_CALLS]", " ");
        for fmt in tool_call_formats() {
            if fmt.id() == self.id() {
                continue; // never delegate back to ourselves
            }
            let calls = fmt.parse(&cleaned);
            if !calls.is_empty() {
                return calls;
            }
        }
        Vec::new()
    }
}

/// Detect a tool-call ATTEMPT that did NOT lift to a valid call — the Mistral/Devstral
/// native `[TOOL_CALLS]` marker is present, but the tail named no real (slash) tool,
/// only reserved receipt vocabulary (`[recall]`, `[action]`, `[active-work]`). The
/// parser correctly yields no call for those (they are not tools). The DANGER is
/// downstream: the verdict layer then treats the whole `[TOOL_CALLS][recall]…` emission
/// as ordinary SPEECH, so the mind gets ZERO feedback that its "call" was bogus, never
/// learns to reach for `code/search`, and rambles to the deadline — `acts:0`,
/// glass-boxed 2026-07-16 (#158 reserved-vocab mimicry, #159 unknown-tool must fail
/// loud). Returning the attempted NAME lets the verdict route it through the executor's
/// unknown-command TEACHER (`"…is not a tool you can call. Closest: …"`) so the failure
/// is LOUD and `drive_to_settle` gives her another generation to do it right.
///
/// Precision: fires ONLY on the explicit `[TOOL_CALLS]` marker (a token no persona
/// emits in ordinary prose) AND only when nothing valid parsed — a well-formed
/// `[TOOL_CALLS]code/search(…)` returns `None` here because it lifts on the normal path.
pub fn attempted_tool_name(text: &str) -> Option<String> {
    if !text.contains("[TOOL_CALLS]") {
        return None;
    }
    // A real call lifted — this is not a failed attempt, leave it to the normal path.
    if !parse_tool_calls(text).is_empty() {
        return None;
    }
    // Best-effort: the first token after the marker is what she tried to "call".
    let tail = text.split("[TOOL_CALLS]").nth(1).unwrap_or("").trim_start();
    let name = if let Some(rest) = tail.strip_prefix('[') {
        // Reserved bracket-token: `[recall]` → `recall`.
        rest.split(']').next().unwrap_or("").trim().to_string()
    } else {
        // Else the leading identifier-ish token (stop at whitespace / open-bracket).
        tail.chars()
            .take_while(|c| !c.is_whitespace() && !matches!(c, '(' | '{' | '[' | '\n'))
            .collect::<String>()
    };
    let name = name
        .trim()
        .trim_matches(|c: char| c == '"' || c == '`' || c == ',')
        .to_string();
    // Marker with no parseable name at all is still a loud-worthy attempt.
    Some(if name.is_empty() {
        "[TOOL_CALLS]".to_string()
    } else {
        name
    })
}

/// A model that wraps a function-style call in explicit BBCode tags —
/// `[tool_call]list_commands()[/tool_call]`. Observed live 2026-07-12 (idiom 6,
/// Casper): the most FORMAL invocation any persona has invented — explicit
/// open/close intent markers around a paren-call — and the highest-precision
/// lift of the whole family: the tags exist for no other reason than to call a
/// tool, so a well-formed pair lifts unconditionally. Name may be a slash-token
/// OR a bare identifier (the discovery pair `list_commands`/`help` are native
/// names without slashes); an unknown name fails LOUD at the executor — honest
/// feedback that teaches the real name (NarratedBareArgs precedent). Args:
/// empty parens → {}; `key="value"` / `key=value` pairs → object.
///
/// The tagless narrated paren-call (`"by calling list_commands() ..."`, idiom 5)
/// is deliberately NOT lifted: in a code-discussion room, prose is full of
/// `function()` mentions, and without a registry-aware existence check the
/// false-positive rate is unacceptable (precision-first). See task #153 for the
/// registry-guarded v2.
struct BbcodeCallFormat;
impl ToolCallFormat for BbcodeCallFormat {
    fn id(&self) -> &'static str {
        "bbcode-call"
    }
    fn parse(&self, text: &str) -> Vec<ToolCall> {
        let mut out = Vec::new();
        let lower = text.to_lowercase();
        let mut from = 0usize;
        while let Some(open_rel) = lower[from..].find("[tool_call]") {
            let body_start = from + open_rel + "[tool_call]".len();
            let Some(close_rel) = lower[body_start..].find("[/tool_call]") else {
                break;
            };
            let body = text[body_start..body_start + close_rel].trim();
            from = body_start + close_rel + "[/tool_call]".len();
            // name(args) — name is a slash-token or bare identifier.
            let Some(paren) = body.find('(') else { continue };
            let name = body[..paren].trim();
            let ok_name = !name.is_empty()
                && name.len() <= 64
                && !name.starts_with("http")
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '/' || c == '-');
            if !ok_name || !body.ends_with(')') {
                continue;
            }
            let args_src = &body[paren + 1..body.len() - 1];
            let Some(args) = paren_call_args(args_src) else {
                continue;
            };
            out.push(ToolCall {
                id: format!("jip-{}", Uuid::new_v4()),
                name: name.to_string(),
                input: serde_json::Value::Object(args),
            });
        }
        out
    }
}

/// `key="value"` / `key=value` pairs (comma-separated) and NOTHING else;
/// empty input → empty args. Leftover junk → `None` (whole call stays inert).
fn paren_call_args(s: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
    let mut map = serde_json::Map::new();
    let s = s.trim();
    if s.is_empty() {
        return Some(map);
    }
    // A JSON object passed whole — `name({"cmd": "..."})` (Casper live,
    // 2026-07-12): the args ARE the object.
    if s.starts_with('{') {
        return match serde_json::from_str::<serde_json::Value>(s) {
            Ok(serde_json::Value::Object(obj)) => Some(obj),
            _ => None,
        };
    }
    for part in s.split(',') {
        let part = part.trim();
        let eq = part.find('=')?;
        let key = part[..eq].trim();
        if key.is_empty() || key.contains(char::is_whitespace) {
            return None;
        }
        let raw = part[eq + 1..].trim();
        let value = raw
            .strip_prefix('"')
            .and_then(|r| r.strip_suffix('"'))
            .or_else(|| raw.strip_prefix('\'').and_then(|r| r.strip_suffix('\'')));
        let v = match value {
            Some(quoted) => serde_json::Value::String(quoted.to_string()),
            None => {
                if raw.is_empty() || raw.contains(char::is_whitespace) {
                    return None;
                }
                match raw {
                    "None" | "null" => serde_json::Value::Null,
                    "true" => serde_json::Value::Bool(true),
                    "false" => serde_json::Value::Bool(false),
                    _ => raw
                        .parse::<i64>()
                        .map(serde_json::Value::from)
                        .unwrap_or_else(|_| serde_json::Value::String(raw.to_string())),
                }
            }
        };
        map.insert(key.to_string(), v);
    }
    Some(map)
}

/// A model that writes the tool call as a BRACKET TAG inline in speech —
/// `[code/read path="conway_game_of_life/src/main.rs"]` /
/// `[code/shell cmd="cargo new --name wordstats"]`. Observed live 2026-07-12:
/// Asha coined the idiom mid-conversation and Atlas adopted it within minutes —
/// a socially-spreading syntax the room converged on, every use a silent
/// non-lift. Precision guards (precision-first, like every last-resort format):
/// - the tag closes on the SAME line and nothing but whitespace may follow it;
/// - the first token must be a tool slash-token (same shape rules as
///   [`backticked_tool_token`]): the '/' requirement keeps every provenance
///   marker — `[repetition]`, `[unfulfilled]`, `[action #1]`,
///   `[thought:historian]` — inert, since none contain a slash;
/// - the remainder must parse as ONE OR MORE `key="value"` pairs with nothing
///   left over — prose in brackets and bare path citations (`[docs/x.md]`)
///   stay speech.
/// A wrong param name (`cmd=` where the command takes `command=`) lifts and
/// fails LOUD with the executor's real error — honest feedback that teaches the
/// contract, strictly better than the silent non-lift. ACL unchanged.
struct BracketTagFormat;
impl ToolCallFormat for BracketTagFormat {
    fn id(&self) -> &'static str {
        "bracket-tag"
    }
    fn parse(&self, text: &str) -> Vec<ToolCall> {
        let mut out = Vec::new();
        for line in text.lines() {
            let line = line.trim_end();
            let Some(open) = line.rfind('[') else { continue };
            let Some(close_rel) = line[open..].find(']') else { continue };
            if !line[open + close_rel + 1..].trim().is_empty() {
                continue; // prose after the tag → not a call
            }
            let inner = &line[open + 1..open + close_rel];
            let mut parts = inner.splitn(2, char::is_whitespace);
            let tool = parts.next().unwrap_or("");
            if !(tool.contains('/')
                && !tool.contains(char::is_whitespace)
                && tool.len() <= 64
                && !tool.starts_with("http"))
            {
                continue;
            }
            let Some(args) = bracket_tag_args(parts.next().unwrap_or("")) else {
                continue;
            };
            if args.is_empty() {
                continue; // require ≥1 key="value" so path citations stay inert
            }
            out.push(ToolCall {
                id: format!("jip-{}", Uuid::new_v4()),
                name: tool.to_string(),
                input: serde_json::Value::Object(args),
            });
        }
        out
    }
}

/// Parse `key="value"` pairs and NOTHING else (leftover → `None`, the whole
/// tag stays inert). Values are plain strings; the command layer's schema does
/// the typing.
fn bracket_tag_args(mut s: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
    let mut map = serde_json::Map::new();
    loop {
        s = s.trim_start();
        if s.is_empty() {
            return Some(map);
        }
        let eq = s.find('=')?;
        let key = s[..eq].trim();
        if key.is_empty() || key.contains(char::is_whitespace) {
            return None;
        }
        let rest = s[eq + 1..].strip_prefix('"')?;
        let end = rest.find('"')?;
        map.insert(
            key.to_string(),
            serde_json::Value::String(rest[..end].to_string()),
        );
        s = &rest[end + 1..];
    }
}

/// A model that writes the tool call CLI-style on its own line — the dominant
/// live idiom of 2026-07-12 (all three personas converged on it within an hour):
///   `code/write --path "sample.txt" --content="The quick brown fox..."`   (Atlas)
///   `code/create-workspace --name word_freq_analysis`                     (Anwen, Casper)
///   `code/write --path "x.py" --content """\n...multiline...\n"""`        (Casper)
///   `code/write path=README.md content=f"{readme_content}"`               (Asha — MUST NOT lift)
///   `code/shell "echo -n 'continuum' | sha256sum"`                        (Anwen — bare positional)
/// Bare on a line OR inside a ```python fence — the personas write these inside
/// pseudo-scripts, so the parser reads lines, not fences. Precision guards:
/// - first token must be a slash-token (same shape rules as [`BracketTagFormat`]:
///   '/' required keeps prose words + provenance markers inert), no '.' (so file
///   citations `docs/x.md` on their own line stay speech), no trailing '/';
/// - at least one argument (a bare `system/memory-budget` line stays inert —
///   the BracketTag ≥1-arg precedent);
/// - UNRESOLVED-TEMPLATE guard: a value that is an f-string (`f"..."`) or
///   contains `{...}` is words, not a call — that CALL is rejected while sibling
///   calls on other lines still lift (per-call, not per-message, rejection);
/// - flags accept `--key "v"`, `--key="v"`, `key=v`, `key="v"`, and
///   triple-quoted multiline values (`--content """…"""` spanning lines);
///   a trailing `# comment` after the last value is tolerated (their fences
///   are commented pseudo-code);
/// - ONE bare quoted positional maps to the tool's live-observed default key
///   (`code/shell`→`command`, `code/read`→`file_path`) — other tools require
///   named flags.
/// An invented name (`root/health-check`) lifts and fails LOUD at the executor —
/// honest feedback that teaches the real surface (NarratedBareArgs precedent).
/// ACL unchanged: a lifted call is gated identically to a hand-written envelope.
struct CliFlagFormat;
impl ToolCallFormat for CliFlagFormat {
    fn id(&self) -> &'static str {
        "cli-flag"
    }
    fn parse(&self, text: &str) -> Vec<ToolCall> {
        let mut out = Vec::new();
        let lines: Vec<&str> = text.lines().collect();
        let mut i = 0usize;
        while i < lines.len() {
            let line = lines[i].trim();
            i += 1;
            // A whole line that IS one paren-call with a slash-token name —
            // `code/list()` beside other pseudo-code (Casper live 2026-07-12;
            // the fence-scoped sole-call check can't see it when the fence
            // holds more than the one call). Same guards as FencedCall. A
            // failed lift falls through to the flag grammar (a paren inside a
            // flag VALUE — `--path "x(y).txt"` — is not a call shape).
            if line.ends_with(')') {
                if let Some(call) = lift_sole_paren_call(line) {
                    out.push(call);
                    continue;
                }
            }
            let Some((name, rest)) = split_cli_head(line) else {
                continue;
            };
            // Triple-quoted value opening on this line without closing → consume
            // following lines until the closer (Casper's multiline --content).
            let mut logical = rest.to_string();
            if unclosed_triple_quote(&logical) {
                let mut closed = false;
                while i < lines.len() {
                    logical.push('\n');
                    logical.push_str(lines[i]);
                    i += 1;
                    if !unclosed_triple_quote(&logical) {
                        closed = true;
                        break;
                    }
                }
                if !closed {
                    continue; // half-written heredoc is not an executable intent
                }
            }
            let Some(args) = cli_flag_args(&logical, name) else {
                continue;
            };
            if args.is_empty() {
                continue; // bare tool-name line stays speech (≥1-arg precedent)
            }
            out.push(ToolCall {
                id: format!("jip-{}", Uuid::new_v4()),
                name: name.to_string(),
                input: serde_json::Value::Object(args),
            });
        }
        out
    }
}

/// Split a candidate CLI line into (tool-name, args-remainder). `None` unless the
/// first token is a plausible slash-token tool name (see [`CliFlagFormat`] guards).
fn split_cli_head(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_start();
    let head_end = line
        .find(char::is_whitespace)
        .unwrap_or(line.len());
    let (name, rest) = line.split_at(head_end);
    let ok = name.contains('/')
        && !name.contains('.')
        && !name.ends_with('/')
        && !name.starts_with('/')
        && name.len() <= 64
        && !name.starts_with("http")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '/' || c == '-');
    if !ok || rest.trim().is_empty() {
        return None; // bare name (no args) handled by caller via empty-args guard
    }
    Some((name, rest.trim_start()))
}

/// True while a `"""` heredoc opened on the accumulated text has no closer yet.
fn unclosed_triple_quote(s: &str) -> bool {
    s.matches("\"\"\"").count() % 2 == 1
}

/// Parse the CLI-flag argument grammar for [`CliFlagFormat`]. `None` = this is
/// words, not a call (unparseable remainder, or a template-marker value).
fn cli_flag_args(
    s: &str,
    tool: &str,
) -> Option<serde_json::Map<String, serde_json::Value>> {
    let mut map = serde_json::Map::new();
    let mut rest = s.trim();
    // ONE bare quoted positional → the tool's live-observed default key.
    if rest.starts_with('"') && !rest.starts_with("\"\"\"") {
        let inner = &rest[1..];
        let end = inner.find('"')?;
        let (value, tail) = (&inner[..end], inner[end + 1..].trim());
        if !(tail.is_empty() || tail.starts_with('#')) {
            return None;
        }
        let key = match tool {
            "code/shell" => "command",
            "code/read" => "file_path",
            _ => return None,
        };
        map.insert(key.into(), serde_json::Value::String(value.to_string()));
        return Some(map);
    }
    while !rest.is_empty() {
        if rest.starts_with('#') {
            break; // trailing comment after the last value
        }
        // key, optionally --prefixed; separator is '=' or whitespace — but a
        // whitespace separator REQUIRES the -- prefix. Without that rule any
        // prose line starting with a path-ish token ("analysis_modules/x is
        // for NLP") would parse as bare `key value` pairs; every live specimen
        // carries `--` or `=`, so flag-shape is the precision line.
        let dashed = rest.starts_with("--");
        let key_src = rest.trim_start_matches('-');
        let sep = key_src.find(|c: char| c == '=' || c.is_whitespace())?;
        if !dashed && !key_src[sep..].starts_with('=') {
            return None;
        }
        let key = &key_src[..sep];
        if key.is_empty()
            || !key
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return None;
        }
        let mut val_src = key_src[sep..].trim_start_matches('=').trim_start();
        // f-string prefix — Asha's `content=f"{var}"` — unresolved template.
        let is_fstring = val_src.starts_with("f\"") || val_src.starts_with("f'");
        if is_fstring {
            val_src = &val_src[1..];
        }
        let (value, tail) = if let Some(body) = val_src.strip_prefix("\"\"\"") {
            let end = body.find("\"\"\"")?;
            (
                body[..end].trim_start_matches('\\').trim().to_string(),
                &body[end + 3..],
            )
        } else if let Some(body) = val_src.strip_prefix('"') {
            let end = body.find('"')?;
            (body[..end].to_string(), &body[end + 1..])
        } else if let Some(body) = val_src.strip_prefix('\'') {
            let end = body.find('\'')?;
            (body[..end].to_string(), &body[end + 1..])
        } else {
            let end = val_src
                .find(char::is_whitespace)
                .unwrap_or(val_src.len());
            (val_src[..end].to_string(), &val_src[end..])
        };
        if is_fstring {
            // `content=f"{var}"` — an UNRESOLVED template referencing fence
            // state we can't evaluate; the whole call is words. (Plain `{...}`
            // braces stay legal: shell brace-expansion is real syntax.)
            return None;
        }
        map.insert(key.to_string(), serde_json::Value::String(value));
        rest = tail.trim_start();
    }
    Some(map)
}

/// A model that wraps ONE paren-call in a code fence or inline code span —
/// ```` ```code/shell(command="cargo test")``` ```` (idiom 8, Asha's verbatim
/// line, pinned in #153). The fence delimiters are the intent markers (the
/// BbcodeCall logic — the span exists to set the call apart from prose), and
/// the entire span content must BE the call: a slash-token name + `(args)` and
/// nothing else. Multi-line fences whose sole body is one paren-call also lift
/// (same emission, wrapped). Args via [`paren_call_args`] (JSON-object
/// passthrough included). Prose fences, code examples, and anything with
/// leftover text stay speech.
struct FencedCallFormat;
impl ToolCallFormat for FencedCallFormat {
    fn id(&self) -> &'static str {
        "fenced-call"
    }
    fn parse(&self, text: &str) -> Vec<ToolCall> {
        let mut out = Vec::new();
        let mut rest = text;
        while let Some(open) = rest.find("```") {
            let after = &rest[open + 3..];
            let Some(close) = after.find("```") else { break };
            let mut span = after[..close].trim();
            rest = &after[close + 3..];
            // Drop a leading language token line (```python\ncode/list```):
            // a single '/‑less word on its own first line is fence metadata,
            // not call content.
            if let Some((first, body)) = span.split_once('\n') {
                let first = first.trim();
                if !first.is_empty()
                    && !first.contains('/')
                    && !first.contains(char::is_whitespace)
                {
                    span = body.trim();
                }
            }
            let Some(call) = lift_sole_paren_call(span) else {
                continue;
            };
            out.push(call);
        }
        out
    }
}

/// `name(args)` — or a bare zero-arg `name` — and NOTHING else (modulo
/// whitespace) with a slash-token name → a lifted call. The '/' requirement
/// keeps ordinary `function()` mentions in fenced code inert; the no-dot rule
/// keeps fenced file citations (```src/main.rs```) speech. The bare form is
/// Atlas's live specimen (2026-07-12, first post-deploy attempt): "I'll use
/// the code/list command:" + a fence containing exactly `code/list` — real
/// tool, real intent, zero args; refusing that lift over missing parens would
/// be pedantry the model can't perceive.
fn lift_sole_paren_call(span: &str) -> Option<ToolCall> {
    // The documented DISCOVERY PAIR are the only bare identifiers that lift
    // without a slash (same exception BbcodeCall carries): the [unfulfilled]
    // fact and the wake briefing point lost personas at `list_commands`, so
    // the pointer must be followable in the fence idioms they actually use —
    // otherwise the cure for invented names dead-ends at its own door.
    let is_discovery = |name: &str| name == "list_commands" || name == "help";
    let span = span.trim();
    let Some(paren) = span.find('(') else {
        if is_discovery(span) {
            return Some(ToolCall {
                id: format!("jip-{}", Uuid::new_v4()),
                name: span.to_string(),
                input: serde_json::Value::Object(serde_json::Map::new()),
            });
        }
        // Bare zero-arg form: the entire span is one slash-token tool name.
        let ok = span.contains('/')
            && !span.contains('.')
            && !span.ends_with('/')
            && !span.starts_with('/')
            && span.len() <= 64
            && !span.starts_with("http")
            && !span.is_empty()
            && span
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '/' || c == '-');
        return ok.then(|| ToolCall {
            id: format!("jip-{}", Uuid::new_v4()),
            name: span.to_string(),
            input: serde_json::Value::Object(serde_json::Map::new()),
        });
    };
    let name = span[..paren].trim();
    let ok = (name.contains('/') || is_discovery(name))
        && !name.contains('.')
        && name.len() <= 64
        && !name.starts_with("http")
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '/' || c == '-');
    if !ok || !span.ends_with(')') {
        return None;
    }
    let args = paren_call_args(&span[paren + 1..span.len() - 1])?;
    Some(ToolCall {
        id: format!("jip-{}", Uuid::new_v4()),
        name: name.to_string(),
        input: serde_json::Value::Object(args),
    })
}

/// A model that NAMES the tool in prose and puts ONLY the bare arguments in a JSON
/// fence — `Let me call the `commands/list` tool directly:` + ```` ```json
/// {"filter": null}``` ````. Observed live 2026-07-11/12 (#143): Asha emitted this
/// idiom 4+ times across an hour, nothing lifted, and she iterated on the ARGUMENT
/// VALUE instead of the format while peers coached the same non-lifting shape back
/// at her. LAST-RESORT format, precision-first:
/// - the fence body parses as a plain JSON OBJECT that is NOT an envelope (no
///   `tool_call`/`name`/`arguments` keys — those belong to the precise formats);
/// - the trailing narration is FIRST-PERSON intent and not peer-addressed
///   (coaching that SHOWS the shape — "you can call…: {…}" — must never execute);
/// - the narration carries a backticked slash-token: the tool name.
/// A wrong tool name (models/list for ai/models/list) lifts and fails LOUD with the
/// executor's unknown-command error — honest feedback that teaches the real name,
/// strictly better than the silent non-lift she was stuck in. ACL unchanged: a
/// lifted call is gated identically to a hand-written envelope.
struct NarratedBareArgsFormat;
impl ToolCallFormat for NarratedBareArgsFormat {
    fn id(&self) -> &'static str {
        "narrated-bare-args"
    }
    fn parse(&self, text: &str) -> Vec<ToolCall> {
        let mut out = Vec::new();
        for f in fenced_blocks(text) {
            if !(f.lang.is_empty() || f.lang == "json") {
                continue;
            }
            let Ok(serde_json::Value::Object(map)) =
                serde_json::from_str::<serde_json::Value>(f.body.trim())
            else {
                continue;
            };
            if map.contains_key("tool_call")
                || map.contains_key("name")
                || map.contains_key("arguments")
            {
                continue; // an envelope-shaped fence belongs to the precise formats
            }
            let narration = trailing_narration(&text[..f.open]);
            if addressed_to_peer(&narration) || !first_person_intent(&narration) {
                continue;
            }
            let Some(tool) = backticked_tool_token(&narration) else {
                continue;
            };
            out.push(ToolCall {
                id: format!("jip-{}", Uuid::new_v4()),
                name: tool,
                input: serde_json::Value::Object(map),
            });
        }
        out
    }
}

/// The LAST backticked slash-token in the narration — the tool name a bare-args
/// fence belongs to. Slash required so `` `ai` `` (a filter value) never reads as
/// a tool; whitespace and absurd length rejected so backticked prose stays inert.
fn backticked_tool_token(narration: &str) -> Option<String> {
    let mut found = None;
    let mut rest = narration;
    while let Some(start) = rest.find('`') {
        let after = &rest[start + 1..];
        let Some(end) = after.find('`') else { break };
        let tok = &after[..end];
        if tok.contains('/')
            && !tok.contains(char::is_whitespace)
            && tok.len() <= 64
            && !tok.starts_with("http")
        {
            found = Some(tok.to_string());
        }
        rest = &after[end + 1..];
    }
    found
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
            let has_args = ["\"arguments\"", "\"parameters\"", "\"params\"", "\"input\""]
                .iter()
                .any(|k| obj.contains(k));
            let has_name =
                ["\"name\"", "\"function\"", "\"tool\""].iter().any(|k| obj.contains(k));
            if !has_args || !has_name {
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
        let Some(content) = extract_first_code_fence(text) else {
            return Vec::new();
        };
        if content.trim().is_empty() {
            return Vec::new();
        }
        // The fence is itself the rendered `code/write(...)` call — recover it AS the
        // call (real content + path) rather than writing the envelope verbatim into
        // the file (the sample.txt corruption, #122). Only when the WHOLE naive path
        // would otherwise mis-extract `file_path` FROM the envelope text.
        if let Some(call) = recover_call_from_fence_body(&content) {
            return vec![call];
        }
        let Some(path) = extract_file_path(text) else {
            return Vec::new();
        };
        vec![ToolCall {
            id: format!("jip-{}", Uuid::new_v4()),
            name: "code/write".to_string(),
            input: serde_json::json!({ "file_path": path, "content": content }),
        }]
    }
}

/// A model that NARRATES a whole working session — prose steps, each followed by a
/// fenced block — instead of emitting any machine-readable call. Observed live from
/// Devstral-Small-2507 (2026-07-09, the wordstats card): "I'll create a file called
/// `wordstats.rs` with the following content: ```rust …```", "Next, I'll compile:
/// ```bash rustc wordstats.rs```", "I'll paste the output here once it's ready" —
/// parsed as Speak, NOTHING executed, and two personas then hallucinated a shared
/// workspace neither had touched. The `[Acting]` prompt block already scolds exactly
/// this in prose; telling doesn't work — the parser meets the idiom (the same
/// differentiator as `<tools>`-in-content and the `[PASS]` bracket fix).
///
/// LAST-RESORT format: walks the message's fences IN ORDER and lifts each into a
/// real call — but ONLY when the narration immediately before the fence is
/// FIRST-PERSON INTENT ("I'll run…", "Let me try…", "Next, I'll compile…"):
/// - creation framing ("create/write a file called `X`") + any fence → `code/write`
/// - shell-shaped fence (```bash/sh/zsh, or first token a known command) → `code/shell`
///
/// The safety line (each arm test-pinned): second-person / request framing near the
/// fence ("you've written…", "please provide…", "could you run…") NEVER lifts —
/// reviewing a peer's code must not execute it; bare example fences with no intent
/// framing NEVER lift. No privilege expansion: the executor ACL gates every lifted
/// call identically to a hand-written envelope.
///
/// KNOWN LIMITATION (Joel 2026-07-09, accepted as a bridge): the intent/veto
/// predicates are English string tables — locked to English, brittle to phrasing
/// (#70/#124 string-matching smell). The durable exits, in preference order:
/// (1) the LoRA loop trains the narration reflex toward the canonical envelope,
///     making this format VESTIGIAL — its live hit-rate (telemetry id
///     "narrated-script") is the signal it can be retired;
/// (2) if narrated lifting must persist, intent classification moves to the
///     embedding/classifier layer (semantic, language-neutral), not string tables.
/// Until then her hands work when she describes the work —
/// [[local-first-tool-call-robustness-is-the-differentiator]].
struct NarratedScriptFormat;
impl ToolCallFormat for NarratedScriptFormat {
    fn id(&self) -> &'static str {
        "narrated-script"
    }
    fn parse(&self, text: &str) -> Vec<ToolCall> {
        let mut out = Vec::new();
        for fence in fenced_blocks(text) {
            let narration = trailing_narration(&text[..fence.open]);
            // Peer-addressed framing (a review, a request) vetoes ANY lift — never
            // execute quoted code on someone's behalf.
            if addressed_to_peer(&narration) {
                continue;
            }
            // The fence body is ITSELF a rendered tool call — the model illustrated
            // the CALL inside a fence rather than emitting it bare. Lift THAT, not the
            // envelope-as-content. Glass-boxed live 2026-07-09: Anwen narrated "I'll use
            // code/write to create sample.txt" then fenced `code/write({"content": …,
            // "file_path": …})`, and the whole envelope got written verbatim INTO
            // sample.txt as content. Recover the real intent instead of corrupting the
            // file. (Precise formats only — a fence of ordinary code never false-parses
            // as an envelope: it needs the `{"tool_call"|"name","arguments"}` shape or a
            // `code/write(` call form.)
            if let Some(call) = recover_call_from_fence_body(&fence.body) {
                out.push(call);
                continue;
            }
            if let Some(path) = extract_created_file_name(&narration) {
                if !fence.body.trim().is_empty() {
                    out.push(ToolCall {
                        id: format!("jip-{}", Uuid::new_v4()),
                        name: "code/write".to_string(),
                        input: serde_json::json!({ "file_path": path, "content": fence.body }),
                    });
                }
                continue;
            }
            // Shell execution needs EXPLICIT first-person intent ("I'll run…") — a
            // shell command is higher-stakes than writing a file she's presenting, so
            // it never lifts from a bare fence (file-authoring is gated by the
            // authoring framing inside extract_created_file_name above).
            if first_person_intent(&narration) && fence_is_shell(&fence) && !fence.body.trim().is_empty() {
                out.push(ToolCall {
                    id: format!("jip-{}", Uuid::new_v4()),
                    name: "code/shell".to_string(),
                    input: serde_json::json!({ "cmd": fence.body.trim() }),
                });
                continue;
            }
            // A rust-tagged fence with the same explicit first-person RUN intent and
            // no filename lifts into `code/run` — the standalone "does this little
            // program do what I think?" hand. Glass-boxed live 2026-07-10: Anwen's
            // "I'll compile and run this function" + ```rust fence fell BETWEEN
            // NarratedWriteFormat (no filename) and the shell arm (not a shell
            // fence), leaving three [unfulfilled] promises on the reverse-string
            // card. A snippet without `fn main` compile-fails HONESTLY — she sees
            // rustc's error next tick and corrects (errors as data, never silence).
            // rust/rs ONLY: code/run is the Rust organism's hand and fails loud on
            // other languages, so a ```python fence stays an [unfulfilled] promise
            // rather than a guaranteed-useless call.
            let post = leading_narration(&text[fence.end.min(text.len())..]);
            let run_signal = (first_person_intent(&narration)
                || first_person_intent(&post)
                || first_person_execution_claim(&post)
                || first_person_execution_claim(&narration))
                && !addressed_to_peer(&narration)
                && !addressed_to_peer(&post);
            if run_signal
                && matches!(fence.lang.as_str(), "rust" | "rs")
                && !fence.body.trim().is_empty()
            {
                out.push(ToolCall {
                    id: format!("jip-{}", Uuid::new_v4()),
                    name: "code/run".to_string(),
                    input: serde_json::json!({ "lang": "rust", "code": fence.body }),
                });
            }
        }
        out
    }
}

/// A fenced block whose CONTENT is a rendered tool call (the model illustrated the
/// call inside a fence). Recover the actual call via the precise formats + the
/// `code/write("json")` prose call-form. Returns `None` for ordinary code/text — so
/// a fence of real source is never mistaken for a call.
fn recover_call_from_fence_body(body: &str) -> Option<ToolCall> {
    // Precise object formats first (envelope / tagged / bare) — the machine-readable
    // shapes; ordinary code lacks the `{"tool_call"…}` / `{"name","arguments"}` shape.
    if let Some(c) = EnvelopeFormat
        .parse(body)
        .into_iter()
        .chain(TaggedFormat.parse(body))
        .chain(BareFormat.parse(body))
        .next()
    {
        return Some(c);
    }
    // The `<tool/name>({ … })` call-FORM: a JSON object wrapped in a
    // `category/verb( … )` invocation the model rendered as text. Recover it as a
    // call to THAT tool with the inner object as arguments — so code/write,
    // code/edit, code/shell, code/read, etc. ALL lift when narrated this way, not
    // just code/write (edit is crucial — a persona that can only rewrite whole
    // files can't do surgical changes). The name must look like a real tool path
    // (`a/b`, lowercase + slash) so prose like `foo(x)` never false-parses.
    let trimmed = body.trim();
    if let Some(open) = trimmed.find('(') {
        let name = trimmed[..open].trim();
        let looks_like_tool = name.contains('/')
            && name
                .chars()
                .all(|c| c.is_ascii_lowercase() || c == '/' || c == '_' || c == '-');
        if looks_like_tool {
            let after = &trimmed[open + 1..];
            let obj_end = after.rfind(')').unwrap_or(after.len());
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(after[..obj_end].trim()) {
                if v.is_object() {
                    return Some(ToolCall {
                        id: format!("jip-{}", Uuid::new_v4()),
                        name: name.to_string(),
                        input: v,
                    });
                }
            }
        }
    }
    None
}

/// Does this text NARRATE an action the speaker intends to take — a fence preceded
/// by first-person intent, not addressed to a peer — regardless of whether any
/// format could LIFT it? The act→observe Speak arm uses this as the backstop: a
/// Speak that narrates action which nothing executed gets an `[unfulfilled]`
/// proprioception line, so next tick she perceives her own unkept promise instead
/// of believing the work happened (the shared-hallucinated-workspace failure,
/// 2026-07-09). Broader than the lift conditions on purpose — e.g. a ```python
/// fence with "I'll run this" is narrated action we can't lift into `code/shell`.
pub fn narrates_fenced_action(text: &str) -> bool {
    fenced_blocks(text).iter().any(|f| {
        let narration = trailing_narration(&text[..f.open]);
        first_person_intent(&narration) && !addressed_to_peer(&narration)
    })
}

/// Does this text end its intent in a theatrical STAGE DIRECTION — a line that is
/// exactly a bracketed present-participle phrase, `[writing test files]` /
/// `[creating test files]`? The fence-less sibling of [`narrates_fenced_action`]:
/// glass-boxed live 2026-07-10, Atlas looped "I'll create three test files …
/// [writing test files]" for over an hour — no fence, so no format could lift it
/// and the fenced backstop never fired; the intent stayed unsatisfied and he
/// re-declared it every turn. The bracket-gerund shape is pure geometry: it never
/// collides with the substrate's own bracket tags (`[t=…]`, `[recall]`,
/// `[action #n]`, `[unfulfilled]` — none open with a gerund).
/// Does this text contain any fenced block at all? Cheap public predicate for
/// the unverified-artifact backstop (#134/Joel 2026-07-11: under verification
/// pressure a persona upgraded from stage directions to CONFABULATED file
/// contents — plausible fenced "artifacts" no tool ever produced. Prose can't
/// distinguish a draft from a claim; the turn-level evidence can).
pub fn has_fenced_block(text: &str) -> bool {
    !fenced_blocks(text).is_empty()
}

/// Does this text CLAIM a past tool execution in the first person — "I ran
/// `commands/list` and got…", "The tool returned this poem"? The PAST-TENSE
/// sibling of [`narrates_fenced_action`] (#144): glass-boxed live 2026-07-11/12
/// when a persona presented self-authored poems as the output of a `gpt-4` run
/// that never happened (log-verified: zero ai/inference/generate invocations),
/// and a peer adopted the fabricated result as room truth. The caller gates on
/// working memory: a claim WITH matching `[action #n]` receipts is honest
/// reporting; a claim with ZERO acts this concern is confabulated execution.
/// Precision-first: quoted lines, substrate tags, second-person coaching, and
/// hypothetical framing never fire; first-person forms additionally require a
/// tool-shaped token (backticked name or slash-path) so "I ran fast" is inert.
pub fn claims_past_tool_run(text: &str) -> bool {
    text.lines().any(|l| {
        let l = l.trim();
        // Quoted/relayed lines and substrate bracket-tags are never her claim.
        if l.starts_with('>') || l.starts_with('[') {
            return false;
        }
        let lower = l.to_lowercase();
        // Hypothetical / instructional framing is not a claim of execution.
        if lower.contains("if i ran")
            || lower.contains("if you run")
            || lower.contains("would return")
            || lower.contains("here's how")
            || lower.contains("you can use")
        {
            return false;
        }
        const RESULT_CLAIMS: &[&str] = &[
            "the tool returned",
            "the command returned",
            "it returned this",
            "and got this list",
            "and got this result",
            "here's the output i got",
            // Presenting file/command contents as an accomplished fact —
            // Casper live 2026-07-12: "I have initialized a new Rust project
            // ... Here are the contents of the `Cargo.toml` file:" + a fully
            // FABRICATED toml (the real crate was two days old and different).
            "here are the contents of",
        ];
        if RESULT_CLAIMS.iter().any(|p| lower.contains(p)) {
            return true;
        }
        const FIRST_PERSON_PAST: &[&str] = &[
            "i ran ",
            "i executed ",
            "i've run ",
            "i have run ",
            "i've executed ",
            "i just ran ",
            "i called ",
            // The creation/initialization family (same live incident): claims
            // of having scaffolded/created something. The tool-shaped-token
            // gate below keeps benign retrospectives ("I created the plan")
            // inert — they carry no backticked/slash token.
            "i have initialized ",
            "i initialized ",
            "i've initialized ",
            "i have created ",
            "i've created ",
            "i have set up ",
        ];
        let claims = FIRST_PERSON_PAST
            .iter()
            .any(|p| lower.starts_with(p) || lower.contains(&format!(". {p}")) || lower.contains(&format!("! {p}")));
        if !claims {
            return false;
        }
        // Require a tool-shaped token: backticked name or a slash-path word.
        l.contains('`')
            || l.split_whitespace()
                .any(|w| w.contains('/') && w.len() > 3 && !w.starts_with("http"))
    })
}

pub fn narrates_stage_direction(text: &str) -> bool {
    text.lines().any(|l| {
        let l = l.trim();
        let Some(inner) = l.strip_prefix('[').and_then(|r| r.strip_suffix(']')) else {
            return false;
        };
        // Stage directions are short standalone phrases, not prose in brackets.
        if inner.len() > 60 || inner.contains('[') || inner.contains(']') {
            return false;
        }
        let Some(head) = inner.split_whitespace().next() else {
            return false;
        };
        head.len() >= 4 && head.ends_with("ing") && head.chars().all(|c| c.is_ascii_alphabetic())
    })
}

/// One ```…``` fenced block: byte offset of the opening fence, the language token
/// (lowercased, may be empty), and the body with the language line stripped.
struct FencedBlock {
    open: usize,
    /// Byte offset just past the CLOSING fence — where any trailing narration
    /// ("I have already run this…") begins.
    end: usize,
    lang: String,
    body: String,
}

/// All fenced blocks in `text`, in order. An unterminated final fence is ignored
/// (half-written code is not an executable intent).
fn fenced_blocks(text: &str) -> Vec<FencedBlock> {
    let mut out = Vec::new();
    let mut rest = text;
    let mut base = 0usize;
    while let Some(open) = rest.find("```") {
        let after = &rest[open + 3..];
        let (lang, body_start) = match after.find('\n') {
            Some(nl) => (after[..nl].trim().to_lowercase(), nl + 1),
            None => break,
        };
        let body_zone = &after[body_start..];
        let Some(close) = body_zone.find("```") else {
            break;
        };
        let consumed = open + 3 + body_start + close + 3;
        out.push(FencedBlock {
            open: base + open,
            end: base + consumed,
            lang,
            body: body_zone[..close].to_string(),
        });
        base += consumed;
        rest = &rest[consumed..];
    }
    out
}

/// The narration IMMEDIATELY before a fence: the text after the previous fence (or
/// message start), bounded to the last few lines so early-message framing can't
/// leak intent onto a distant fence.
fn trailing_narration(before: &str) -> String {
    let after_prev_fence = match before.rfind("```") {
        Some(idx) => &before[idx + 3..],
        None => before,
    };
    let lines: Vec<&str> = after_prev_fence.lines().rev().take(4).collect();
    lines
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase()
}

/// The narration IMMEDIATELY after a fence: the first few lines before the next
/// fence (or message end). Where a model states what it claims it DID with the
/// code it just presented ("I have already run this program and received: …").
fn leading_narration(after: &str) -> String {
    let before_next_fence = match after.find("```") {
        Some(idx) => &after[..idx],
        None => after,
    };
    before_next_fence
        .lines()
        .take(4)
        .collect::<Vec<_>>()
        .join("\n")
        .to_lowercase()
}

/// Does this narration CLAIM the speaker already executed the thing? Past-tense
/// first-person execution ("I have already run…", "I ran it and got…"). Glass-
/// boxed live 2026-07-10 (card 34d8aff7): Anwen posted a complete program, wrote
/// "I have already run this program and received the following output:", and
/// fabricated the output — WRONG output ("muitnednoc" for reverse("continuum"),
/// letters not even in the input) — which her teammate then validated as correct.
/// Lifting the claim RUNS the code, so the real output reaches her before the
/// fabrication reaches the room: the substrate makes her words true or shows her
/// they weren't. Same veto as intent: peer-addressed narration never lifts.
fn first_person_execution_claim(narration: &str) -> bool {
    const MARKERS: &[&str] = &[
        "i have already run",
        "i have run ",
        "i've run ",
        "i've already run",
        "i ran ",
        "i have executed",
        "i've executed",
        "i executed ",
        "i compiled and ran",
        "i have compiled and run",
    ];
    MARKERS.iter().any(|m| narration.contains(m))
}

/// Is this narration the SPEAKER's own stated intent to do the thing? Markers are
/// first-person-future doing ("I'll…", "let me…", "next, i…"). This is the positive
/// gate; [`addressed_to_peer`] is the veto.
fn first_person_intent(narration: &str) -> bool {
    const MARKERS: &[&str] = &[
        "i'll ",
        "i will ",
        "i'm going to",
        "i am going to",
        "let me ",
        "let's ",
        "next, i",
        "now, i",
        "first, i",
        "then, i",
        "finally, ",
        "now, let",
        "next, let",
        "finally, let",
    ];
    MARKERS.iter().any(|m| narration.contains(m))
}

/// Is this narration aimed at SOMEONE ELSE doing/having done the thing? A request
/// or a review of a peer's work must never execute — the fence there is quotation.
fn addressed_to_peer(narration: &str) -> bool {
    const MARKERS: &[&str] = &[
        "you've ",
        "you have ",
        "you wrote",
        "you ran",
        "your code",
        "please ",
        "could you",
        "would you",
        "can you ",
        "here's how you",
        "here is how you",
        "you would",
        "you can ",
    ];
    MARKERS.iter().any(|m| narration.contains(m))
}

/// Extract the target file name from file-authoring narration. Two shapes:
///   1. explicit "create/write a file called|named `X`"
///   2. any save/write/create intent + a BACKTICK-QUOTED filename-with-extension
///      anywhere in the narration (`reverse.rs`, `lib.rs`), e.g. "Here's the code for
///      `reverse.rs`:", "I've saved this to `reverse.rs`", "the `wordstats.rs` program".
/// Shape 2 was added after glass-boxing Asha 2026-07-10: she narrated "Here's the code
/// for `reverse.rs`" + a fence and asserted "I've saved this to reverse.rs" — but the
/// old detector only matched "file called/named", so nothing lifted and the save was
/// confabulated. Meeting the idiom ([[local-first-tool-call-robustness-is-the-differentiator]]).
/// None → not a file authoring (the shell/other paths handle it).
fn extract_created_file_name(narration: &str) -> Option<String> {
    const AUTHOR_INTENT: &[&str] =
        &["create", "write", "save", "here's the", "here is the", "code for", "program"];
    if !AUTHOR_INTENT.iter().any(|k| narration.contains(k)) {
        return None;
    }
    // Shape 1: "file called|named `X`".
    if let Some(idx) = narration
        .find("file called")
        .map(|i| i + "file called".len())
        .or_else(|| narration.find("file named").map(|i| i + "file named".len()))
    {
        let after = narration[idx..].trim_start();
        let name: String = after
            .trim_start_matches(['`', '"', '\''])
            .chars()
            .take_while(|c| !c.is_whitespace() && !matches!(c, '`' | '"' | '\'' | ',' | ':' | ';'))
            .collect();
        if !name.is_empty() && name.contains('.') {
            return Some(name);
        }
    }
    // Shape 2: the first backtick-quoted `filename.ext` token in the narration. A
    // filename = has a dot with a short alphanumeric extension and no path separator
    // spaces, so prose in backticks (`the value`) is not mistaken for a file.
    for seg in narration.split('`').skip(1).step_by(2) {
        let tok = seg.trim();
        if let Some((stem, ext)) = tok.rsplit_once('.') {
            let ext_ok = !ext.is_empty()
                && ext.len() <= 5
                && ext.chars().all(|c| c.is_ascii_alphanumeric());
            let stem_ok = !stem.is_empty()
                && !tok.contains(' ')
                && tok.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '/'));
            if ext_ok && stem_ok {
                return Some(tok.to_string());
            }
        }
    }
    None
}

/// Is this fence a runnable shell block? Either shell-tagged, or untagged with a
/// first token that is unmistakably a command (a `./binary` or a well-known CLI).
fn fence_is_shell(fence: &FencedBlock) -> bool {
    if matches!(
        fence.lang.as_str(),
        "bash" | "sh" | "shell" | "zsh" | "console" | "terminal"
    ) {
        return true;
    }
    if !fence.lang.is_empty() {
        return false;
    }
    let first = fence
        .body
        .lines()
        .find(|l| !l.trim().is_empty())
        .and_then(|l| l.trim().split_whitespace().next())
        .unwrap_or("");
    first.starts_with("./")
        || matches!(
            first,
            "rustc"
                | "cargo"
                | "echo"
                | "ls"
                | "cat"
                | "python"
                | "python3"
                | "pip"
                | "git"
                | "make"
                | "mkdir"
                | "touch"
                | "grep"
                | "find"
                | "curl"
                | "sha256sum"
                | "shasum"
                | "npm"
                | "node"
        )
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

    // what this catches: under llama-server --jinja the 14B emits its call wrapped in
    // `<tools>…</tools>` (a template quirk — not the canonical `<tool_call>`), and
    // llama-server does NOT stamp finish=tool_calls for it. The universal salvage MUST
    // still extract it (via the inner {name,arguments} object) or the persona's hands go
    // dead on the exact path native tool-calling now takes. Regression for the --jinja fix.
    #[test]
    fn extracts_jinja_tools_wrapped_call() {
        let text = "<tools>\n{\"name\": \"list_dir\", \"arguments\": {\"path\": \".\"}}\n</tools>";
        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 1, "the tools-wrapped call must be recovered");
        assert_eq!(calls[0].name, "list_dir");
        assert_eq!(calls[0].input["path"], ".");
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

    // what this catches: the narrated-script gap (#122, glass-boxed live 2026-07-09).
    // Devstral narrates a whole session — first-person intent + fences — and it parsed
    // as Speak; nothing executed; two personas hallucinated a shared workspace. The
    // format must lift the ORDERED sequence: shell, file write (creation framing),
    // shell, shell — and the faculty's single-step path takes the first.
    #[test]
    fn narrated_session_lifts_ordered_shell_and_write_calls() {
        // Anwen's live wordstats message, verbatim shape (prompt-captures 2026-07-09).
        let text = "I'll start by creating a sample text file for testing purposes.\n\n\
```bash\necho 'the quick brown fox the lazy dog the end' > sample.txt\n```\n\n\
Now, let's write the Rust code for the word frequency program. I'll create a file called `wordstats.rs` with the following content:\n\n\
```rust\nuse std::collections::HashMap;\nfn main() { println!(\"hi\"); }\n```\n\n\
Next, I'll compile the Rust code using `rustc`:\n\n\
```bash\nrustc wordstats.rs\n```\n\n\
Finally, let's run the program with the sample text file:\n\n\
```bash\n./wordstats sample.txt\n```\n\n\
I'll paste the output here once it's ready.";
        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 4, "all four narrated steps lift, in order: {calls:?}");
        assert_eq!(calls[0].name, "code/shell");
        assert_eq!(
            calls[0].input["cmd"],
            "echo 'the quick brown fox the lazy dog the end' > sample.txt"
        );
        assert_eq!(calls[1].name, "code/write");
        assert_eq!(calls[1].input["file_path"], "wordstats.rs");
        assert!(calls[1].input["content"]
            .as_str()
            .unwrap()
            .contains("HashMap"));
        assert_eq!(calls[2].name, "code/shell");
        assert_eq!(calls[2].input["cmd"], "rustc wordstats.rs");
        assert_eq!(calls[3].name, "code/shell");
        assert_eq!(calls[3].input["cmd"], "./wordstats sample.txt");
        // The faculty's one-step-per-generation path takes the FIRST — the rest
        // re-emerge across the drive_to_settle loop as results land in memory.
        assert_eq!(parse_tool_call(text).unwrap().name, "code/shell");
    }

    // what this catches: a rust-tagged fence with first-person compile/run intent
    // and NO filename lifts into code/run — the gap between NarratedWriteFormat
    // (needs a filename) and the shell arm (needs a shell fence). Anwen's live
    // reverse-string message, verbatim shape (prompt-captures 2026-07-10): three
    // [unfulfilled] promises on card 34d8aff7 before this arm existed.
    #[test]
    fn narrated_rust_fence_with_run_intent_lifts_into_code_run() {
        let text = "Let me proceed with card 34d8aff7 and write the code to reverse a string in Rust.

```rust
fn reverse_string(s: &str) -> String {
    s.chars().rev().collect()
}
```

I'll compile and run this function to ensure it works correctly.";
        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 1, "the rust fence lifts once: {calls:?}");
        assert_eq!(calls[0].name, "code/run");
        assert_eq!(calls[0].input["lang"], "rust");
        assert!(calls[0].input["code"].as_str().unwrap().contains("reverse_string"));
        // A python fence with the same intent does NOT lift — code/run is the Rust
        // organism's hand; a guaranteed-useless call is worse than the honest
        // [unfulfilled] proprioception the Speak arm records.
        let py = "I'll run this now.

```python
print('hi')
```";
        assert!(parse_tool_calls(py).is_empty(), "non-rust fences stay unlifted");
        // A bare example fence with no intent framing stays inert.
        let example = "Here's how reverse looks in Rust:

```rust
fn r() {}
```";
        assert!(parse_tool_calls(example).is_empty(), "no intent, no lift");
    }

    // what this catches: key-spelling tolerance on structured calls. Asha's live
    // 2026-07-10 claim — her FIRST structured emission after a day of narration —
    // used {"function","params"} and the strict {"name","arguments"} parser
    // rejected it on spelling alone. All common spellings normalize; prose JSON
    // without a name+args shape still never false-parses.
    #[test]
    fn function_params_key_spellings_normalize() {
        let asha = "I'll claim a card for my Conway project.\n```json\n{\"function\": \"work/claim\", \"params\": {\"card_id\": \"33a0e899\"}}\n```";
        let calls = parse_tool_calls(asha);
        assert_eq!(calls.len(), 1, "her structured claim must lift: {calls:?}");
        assert_eq!(calls[0].name, "work/claim");
        assert_eq!(calls[0].input["card_id"], "33a0e899");
        // {"tool","input"} spelling too.
        let alt = "{\"tool\": \"code/read\", \"input\": {\"file_path\": \"a.rs\"}}";
        assert_eq!(parse_tool_call(alt).unwrap().name, "code/read");
        // Ordinary JSON data (no name/args shape) still never false-parses.
        assert!(parse_tool_calls("{\"count\": 3, \"items\": []}").is_empty());
    }

    // what this catches: a PAST-TENSE first-person execution claim after the fence
    // lifts too — Anwen's live fabrication (2026-07-10, card 34d8aff7): posted a
    // complete program, claimed "I have already run this program and received the
    // following output:", and invented WRONG output which her teammate validated.
    // Running the claim puts the REAL output in front of her before the
    // fabrication becomes shared room truth. Peer-addressed past tense stays inert.
    #[test]
    fn past_tense_execution_claim_after_fence_lifts_and_grounds_the_fabrication() {
        let text = "```rust\nfn reverse_string(s: &str) -> String {\n    s.chars().rev().collect()\n}\nfn main() {\n    println!(\"{}\", reverse_string(\"continuum\"));\n}\n```\nI have already run this program and received the following output:\n```\nmuitnednoc\n```";
        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 1, "the claimed-run fence lifts: {calls:?}");
        assert_eq!(calls[0].name, "code/run");
        assert!(calls[0].input["code"].as_str().unwrap().contains("fn main"));
        // Reviewing a PEER's claimed run stays quotation, never execution.
        let peer = "```rust\nfn main() {}\n```\nYou've run this already and it worked, right?";
        assert!(parse_tool_calls(peer).is_empty(), "peer-addressed past tense never lifts");
    }

    // what this catches: the safety line. A REVIEW of a peer's work quotes commands
    // in fences — executing them would run someone else's (possibly wrong) code on
    // her key. Second-person / request framing must never lift. (Anwen's live review
    // message, verbatim shape.)
    #[test]
    fn peer_review_and_request_fences_never_lift() {
        let review = "It looks like you've written and compiled the Rust code for the word \
frequency program. Now, let's run the program with the sample text file to see the output.\n\n\
```bash\n./wordstats sample.txt\n```\n\n\
Please provide the output so I can review it.";
        assert!(
            parse_tool_calls(review).is_empty(),
            "review-quoted fence must not execute"
        );
        let request = "Could you run this for me?\n```bash\nls -la\n```";
        assert!(parse_tool_calls(request).is_empty(), "a request is not my intent");
    }

    // what this catches: bare example fences with no intent framing are teaching
    // material, not action — and non-shell fences without creation framing stay inert.
    #[test]
    fn example_fences_without_intent_stay_inert() {
        let example = "Here's how you would count words in bash:\n```bash\nwc -w file.txt\n```";
        assert!(parse_tool_calls(example).is_empty());
        let plain = "The algorithm looks like:\n```rust\nfn main() {}\n```";
        assert!(parse_tool_calls(plain).is_empty());
        // A precise format present anywhere still wins outright (most-specific-first).
        let with_envelope = "I'll run it.\n```bash\nls\n```\n\
{\"tool_call\": {\"name\": \"ping\", \"arguments\": {}}}";
        let calls = parse_tool_calls(with_envelope);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "ping");
    }

    // what this catches: the sample.txt corruption (#122, glass-boxed live 2026-07-09).
    // Anwen narrated "I'll use code/write to create sample.txt" then fenced the RENDERED
    // call `code/write({"content": …, "file_path": …})` — and the whole envelope got
    // written verbatim INTO the file as content. A fence body that IS a tool call must
    // be recovered AS the call, not wrapped; a fence of ordinary code must still write.
    #[test]
    fn fenced_rendered_call_is_recovered_not_written_as_content() {
        let corrupting = "I'll create a file called `sample.txt`:\n\
```\ncode/write({\n  \"content\": \"hello world\",\n  \"file_path\": \"work-x/sample.txt\"\n})\n```";
        let calls = parse_tool_calls(corrupting);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "code/write");
        assert_eq!(calls[0].input["content"], "hello world", "content is the INNER text, not the envelope");
        assert_eq!(calls[0].input["file_path"], "work-x/sample.txt");
        // A fenced ENVELOPE with intent narration is likewise recovered as the call.
        let enveloped = "Let me write it:\n\
```json\n{\"tool_call\": {\"name\": \"code/write\", \"arguments\": {\"file_path\": \"a.rs\", \"content\": \"fn main(){}\"}}}\n```";
        let c2 = parse_tool_calls(enveloped);
        assert_eq!(c2.len(), 1);
        assert_eq!(c2[0].input["content"], "fn main(){}");
        // BUT a fence of ordinary code under creation framing still writes verbatim.
        let real_file = "I'll create a file called `lib.rs`:\n\
```rust\npub fn add(a: i32, b: i32) -> i32 { a + b }\n```";
        let c3 = parse_tool_calls(real_file);
        assert_eq!(c3.len(), 1);
        assert_eq!(c3[0].name, "code/write");
        assert_eq!(c3[0].input["file_path"], "lib.rs");
        assert!(c3[0].input["content"].as_str().unwrap().contains("pub fn add"));
    }

    // what this catches: the file-authoring idiom coverage gap (#122, glass-boxed
    // Asha 2026-07-10). "Here's the code for `reverse.rs`:" + fence, and "I've saved
    // this to `reverse.rs`", must lift to code/write — the old detector only matched
    // "file called/named X", so her save was confabulated (asserted, never executed).
    // Prose in backticks (`the value`) must NOT be mistaken for a filename.
    #[test]
    fn backtick_filename_under_author_intent_lifts_to_write() {
        let asha = "Sure thing! Here's the code for `reverse.rs`:\n\
```rust\nuse std::env;\nfn main() { let a: Vec<String> = env::args().collect(); println!(\"{}\", a[1].chars().rev().collect::<String>()); }\n```";
        let c = parse_tool_calls(asha);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].name, "code/write");
        assert_eq!(c[0].input["file_path"], "reverse.rs");
        assert!(c[0].input["content"].as_str().unwrap().contains("env::args"));
        // A subdirectory path filename lifts too.
        let sub = "I'll write the program. Here's `work-x/main.rs`:\n```rust\nfn main(){}\n```";
        assert_eq!(parse_tool_calls(sub)[0].input["file_path"], "work-x/main.rs");
        // Prose in backticks with intent words nearby must NOT become a file write.
        let prose = "I'll write a clear explanation of `the design` for you.\n```text\nsome notes\n```";
        assert!(parse_tool_calls(prose).is_empty(),
            "backtick prose with no filename-shaped token must not lift as a write");
    }

    // what this catches: EDIT is crucial (Joel 2026-07-10) — a persona that can only
    // rewrite whole files can't make surgical changes. The generalized call-form
    // recovery lifts ANY rendered `category/verb({json})` in a fence, so code/edit,
    // code/shell, code/read all work when narrated as a call — not just code/write.
    #[test]
    fn any_rendered_tool_call_in_a_fence_lifts_including_edit() {
        let edit = "I'll apply the fix with code/edit:\n\
```\ncode/edit({\"file_path\": \"wordstats.rs\", \"search\": \"replace(x)\", \"replace\": \"filter(y)\"})\n```";
        let c = parse_tool_calls(edit);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].name, "code/edit");
        assert_eq!(c[0].input["file_path"], "wordstats.rs");
        assert_eq!(c[0].input["search"], "replace(x)");
        assert_eq!(c[0].input["replace"], "filter(y)");
        // A rendered code/read call likewise lifts.
        let read = "Let me read it:\n```\ncode/read({\"file_path\": \"lib.rs\"})\n```";
        let r = parse_tool_calls(read);
        assert_eq!(r[0].name, "code/read");
        // Prose that merely mentions a slash isn't a call — `and/or(this)` never lifts.
        assert!(parse_tool_calls("I'll consider this and/or(that) approach.").is_empty());
    }

    // what this catches: the unfulfilled-promise backstop's predicate (#122). Narrated
    // action we can't LIFT (a python fence with run-intent — not shell, not file
    // creation) must still register as a promise so the Speak arm records the
    // [unfulfilled] proprioception; peer-addressed and example fences must not.
    #[test]
    fn narrated_action_predicate_is_broader_than_the_lift() {
        let unliftable = "I'll run this script to check:\n```python\nprint(2+2)\n```";
        assert!(parse_tool_calls(unliftable).is_empty(), "python isn't liftable");
        assert!(narrates_fenced_action(unliftable), "but it IS a narrated promise");
        assert!(!narrates_fenced_action(
            "Here's how you would do it:\n```python\nprint(2+2)\n```"
        ));
        assert!(!narrates_fenced_action(
            "Could you run this?\n```bash\nls\n```"
        ));
        assert!(!narrates_fenced_action("just prose, no fences at all"));
    }

    // what this catches: the FENCE-LESS unfulfilled-promise idiom (glass-boxed
    // live 2026-07-10) — Atlas looped "I'll create three test files …
    // [writing test files]" for over an hour: no fence, so narrates_fenced_action
    // never fired and he re-declared the intent every turn. A bracketed
    // present-participle line IS a stage direction; the substrate's own bracket
    // tags must never trip it.
    #[test]
    // what this catches: the #143 bare-args idiom — Asha's verbatim stuck shape
    // ("Let me call the `commands/list` tool directly:" + a bare-args json fence)
    // lifts into a real call, while Anwen's COACHING that shows the identical fence
    // ("you can call commands/list… For example: {…}") stays inert, envelope-shaped
    // fences stay owned by the precise formats, and a backticked VALUE (`ai`) never
    // reads as a tool name. A regression here either re-strands her for another
    // hour or executes a peer's example — both observed failure classes.
    #[test]
    // what this catches: the bracket-tag idiom the room INVENTED live
    // (2026-07-12: Asha coined [code/read path="..."], Atlas adopted
    // [code/shell cmd="..."] minutes later) lifts to a real call — and every
    // provenance marker the system itself writes into content/working memory
    // ([repetition], [unfulfilled], [action #n], [thought:historian]) stays
    // inert, because none carry a slash-token + key="value" args.
    #[test]
    // what this catches: the initialization-claim family — Casper's live
    // fabricated-completion message (2026-07-12: claimed `cargo new wordstats`
    // ran + posted invented Cargo.toml contents; the real crate was two days
    // old and different) must read as a past-tool-run claim, while benign
    // retrospectives without tool-shaped tokens stay inert.
    #[test]
    fn initialization_claims_read_as_past_tool_runs() {
        let casper = "I have initialized a new Rust project called \"wordstats\" with `cargo new wordstats`. Here are the contents of the `Cargo.toml` file:";
        assert!(claims_past_tool_run(casper), "fabricated completion must be claimed");
        // "here are the contents of" alone is a result claim:
        assert!(claims_past_tool_run(
            "Here are the contents of the `Cargo.toml` file:"
        ));
        // Benign retrospective without a tool-shaped token stays inert.
        assert!(!claims_past_tool_run(
            "I have created a plan for our collaboration going forward."
        ));
    }

    // what this catches: idiom 6 — Casper's live [tool_call]list_commands()
    // [/tool_call] (2026-07-12, probe-confirmed non-lift at the time) now lifts;
    // args parse; malformed bodies and prose mentions stay inert.
    #[test]
    // what this catches: Casper's live multiline BBCode with a JSON-object
    // arg — [tool_call]\ncode/shell({"cmd":"..."})\n[/tool_call] — lifts with
    // the object as the args verbatim (wrong param names fail loud downstream).
    #[test]
    fn bbcode_json_object_args_lift() {
        let live = "I'll run this command now:\n[tool_call]\ncode/shell({\"cmd\":\"printf %s continuum | shasum -a 256\"})\n[/tool_call]";
        let calls = parse_tool_calls(live);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "code/shell");
        assert_eq!(calls[0].input["cmd"], "printf %s continuum | shasum -a 256");
    }

    fn bbcode_call_lifts_and_prose_mentions_stay_inert() {
        // Casper's exact live line.
        let live = "Let me check what's accessible here by listing all of them first.\n[tool_call]list_commands()[/tool_call]";
        let calls = parse_tool_calls(live);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "list_commands");
        assert_eq!(calls[0].input, serde_json::json!({}));

        // Args in both quoted and bare forms.
        let with_args = "[tool_call]list_commands(filter=\"code\")[/tool_call]";
        let calls = parse_tool_calls(with_args);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].input["filter"], "code");

        let slash = "[tool_call]code/read(path=\"wordstats/Cargo.toml\")[/tool_call]";
        let calls = parse_tool_calls(slash);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "code/read");

        // Inert: unclosed tag, no parens, prose containing function() mentions
        // WITHOUT the tags (idiom 5 deliberately not lifted), junk args.
        for inert in [
            "[tool_call]list_commands()",
            "[tool_call]just words[/tool_call]",
            "I could call list_commands() to see what exists.",
            "[tool_call]help(some junk here)[/tool_call]",
        ] {
            assert!(parse_tool_calls(inert).is_empty(), "must stay inert: {inert}");
        }
    }

    fn bracket_tag_lifts_and_provenance_markers_stay_inert() {
        // Asha's exact live line.
        let asha = "For the Game of Life implementation - here's what we have so far:\n[code/read path=\"conway_game_of_life/src/main.rs\"]";
        let calls = parse_tool_calls(asha);
        assert_eq!(calls.len(), 1, "Asha's bracket tag lifts");
        assert_eq!(calls[0].name, "code/read");
        assert_eq!(calls[0].input["path"], "conway_game_of_life/src/main.rs");

        // Atlas's exact live line — wrong param name (cmd vs command) still
        // lifts; the executor's loud error is the honest feedback.
        let atlas = "let me create a new workspace:\n[code/shell cmd=\"cargo new --name wordstats\"]";
        let calls = parse_tool_calls(atlas);
        assert_eq!(calls.len(), 1, "Atlas's bracket tag lifts");
        assert_eq!(calls[0].name, "code/shell");
        assert_eq!(calls[0].input["cmd"], "cargo new --name wordstats");

        // Provenance markers + prose brackets: all inert.
        for inert in [
            "[repetition] you have said this nearly verbatim 3 times",
            "[unfulfilled] you said you would run commands, but no tool ran",
            "[action #1] I ran code/shell(command=ls) Result: ok",
            "[thought:historian] You have repeatedly asked teammates to run tools",
            "see the doc at [docs/architecture/CBAR.md]", // path citation, no args
            "[code/read path=\"x\"] and then we can review it", // prose after tag
        ] {
            assert!(
                parse_tool_calls(inert).is_empty(),
                "must stay inert: {inert}"
            );
        }
    }

    fn bare_args_fence_with_named_tool_lifts_and_coaching_stays_inert() {
        // Asha's live receipt:
        let stuck = "Let me call the `commands/list` tool directly:\n```json\n{\"filter\": null}\n```";
        let calls = parse_tool_calls(stuck);
        assert_eq!(calls.len(), 1, "{calls:?}");
        assert_eq!(calls[0].name, "commands/list");
        assert_eq!(calls[0].input, serde_json::json!({"filter": null}));

        // Wrong-but-named tool still lifts (fails loud downstream — teaches the real name):
        let wrong = "Let me run `models/list` to see what we have:\n```json\n{\"filter\": \"ai\"}\n```";
        let calls = parse_tool_calls(wrong);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "models/list");

        // Peer coaching showing the shape must NOT execute:
        let coaching = "If you want to dive deeper, you can call `commands/list` again. For example:\n```json\n{\"filter\": \"code\"}\n```";
        assert!(parse_tool_calls(coaching).is_empty(), "coaching executed!");

        // Backticked VALUE with no slash is not a tool name:
        let value_only = "Let me filter for `ai` tools now:\n```json\n{\"filter\": \"ai\"}\n```";
        assert!(parse_tool_calls(value_only).is_empty());

        // Envelope-shaped fences stay with the precise formats (no double-lift):
        let envelope = "Let me call the `commands/list` tool:\n```json\n{\"tool_call\": {\"name\": \"commands/list\", \"arguments\": {\"filter\": null}}}\n```";
        let calls = parse_tool_calls(envelope);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "commands/list");
    }

    // what this catches: the #144 fabricated-execution detector — verbatim live
    // receipts (2026-07-11/12) fire; peer coaching that SHOWS the format, quoted
    // lines, hypotheticals, and plain prose "I ran" stay inert. A false positive
    // here taxes honest speech; a false negative lets a fake result become room
    // truth (observed: a peer adopted a fabricated model list within two turns).
    #[test]
    fn past_tool_run_claims_fire_and_coaching_stays_inert() {
        // Anwen's live fabrications:
        assert!(claims_past_tool_run(
            "I ran `ai/inference/generate` with the following parameters:\n- model: \"gpt-4\""
        ));
        assert!(claims_past_tool_run(
            "I ran commands/list --filter ai to get the list of AI-related commands."
        ));
        assert!(claims_past_tool_run(
            "The tool returned this poem:\n\"In the silent sea of night...\""
        ));
        // Asha's live claim:
        assert!(claims_past_tool_run("I ran `models/list` but it seems there might be an issue"));

        // Peer coaching / second person — never a self-claim:
        assert!(!claims_past_tool_run(
            "You've just called `commands/list` — that's the full index."
        ));
        // Hypothetical / instructional:
        assert!(!claims_past_tool_run(
            "If you run commands/list with a filter, here's how it narrows the set."
        ));
        // Plain prose past tense with no tool token:
        assert!(!claims_past_tool_run("I ran fast to catch the bus."));
        // Quoted relay:
        assert!(!claims_past_tool_run("> I ran `code/run` earlier, said Atlas."));
        // Substrate act-admission tag lines are not her claim:
        assert!(!claims_past_tool_run("[action #3] I ran code/read(...) Result: ok"));
    }

    #[test]
    fn stage_direction_is_a_narrated_promise() {
        // Atlas's exact live message shape.
        let atlas = "Thank you, Anwen! I'll create the three test files now and then \
                     run your implementation against them. Let me start with the first \
                     file: a simple text file.\n[writing test files]";
        assert!(narrates_stage_direction(atlas));
        assert!(narrates_stage_direction("Understood!\n[creating test files]"));

        // Substrate bracket tags and ordinary bracket use never match.
        assert!(!narrates_stage_direction("[t=1783731774979] Anwen: hi"));
        assert!(!narrates_stage_direction("[recall]\n- (heard, 3h ago) a fact"));
        assert!(!narrates_stage_direction("[action #5] I ran code/run({...})"));
        assert!(!narrates_stage_direction(
            "[unfulfilled] I said I would run commands, but no tool ran"
        ));
        // A gerund bracket buried in prose (not a standalone line) doesn't match.
        assert!(!narrates_stage_direction(
            "we discussed [writing test files] as an option yesterday"
        ));
        // Long bracketed prose is not a stage direction.
        assert!(!narrates_stage_direction(
            "[writing a very long explanation of everything I might ever do with all these files in the workspace today]"
        ));
        assert!(!narrates_stage_direction("just prose, no brackets"));
    }

    // ── CliFlagFormat (idioms 9+10) + FencedCallFormat (idiom 8) — the
    // 2026-07-12 afternoon corpus: all three personas converged on CLI-flag
    // calls and none lifted (task #153 metadata carries each verbatim). ──

    // what this catches: Atlas's bare-line CLI-flag write (idiom 9). The
    // dominant live idiom must lift with both `--key "v"` and `--key="v"`.
    #[test]
    fn cli_flag_bare_line_write_lifts() {
        let text = "I'd rather create the sample text file now to get us started.\n\
             code/write --path \"sample.txt\" --content=\"The quick brown fox jumps over the lazy dog.\"";
        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "code/write");
        assert_eq!(calls[0].input["path"], "sample.txt");
        assert_eq!(
            calls[0].input["content"],
            "The quick brown fox jumps over the lazy dog."
        );
    }

    // what this catches: Anwen/Casper's workspace-create inside a ```python
    // fence with a space-separated bare-token flag value, plus a trailing
    // `# comment` line above it that must stay inert.
    #[test]
    fn cli_flag_inside_python_fence_lifts() {
        let text = "Let me set up our project properly:\n\
             ```python\n\
             # First, create a dedicated workspace to keep everything organized\n\
             code/create-workspace --name word_freq_analysis\n\
             ```";
        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "code/create-workspace");
        assert_eq!(calls[0].input["name"], "word_freq_analysis");
    }

    // what this catches: Casper's triple-quoted MULTILINE --content value plus
    // a second call in the same fence — both lift, in order.
    #[test]
    fn cli_flag_triple_quote_multiline_and_sibling_lift_in_order() {
        let text = "```python\n\
             code/write --path \"word_freq_analysis/text_cleaner.py\" --content \"\"\"\\\n\
             import re\n\
             class TextCleaner:\n\
                 pass\n\
             \"\"\"\n\
             code/list --path \"word_freq_analysis/\"\n\
             ```";
        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].name, "code/write");
        assert_eq!(
            calls[0].input["path"],
            "word_freq_analysis/text_cleaner.py"
        );
        let content = calls[0].input["content"].as_str().unwrap();
        assert!(content.contains("class TextCleaner"));
        assert_eq!(calls[1].name, "code/list");
        assert_eq!(calls[1].input["path"], "word_freq_analysis/");
    }

    // what this catches: Asha's mixed fence — an f-string templated call MUST
    // stay words (unresolved `content=f"{readme_content}"`) while the sibling
    // bare-positional `code/shell "…"` on another line still lifts, including
    // shell brace-expansion braces (NOT a template).
    #[test]
    fn cli_flag_fstring_call_stays_words_but_positional_sibling_lifts() {
        let text = "```python\n\
             readme_content = \"some docs\"\n\
             code/shell \"mkdir -p analysis_modules/{text_processing,data_analysis}\"\n\
             code/write path=README.md content=f\"{readme_content}\"\n\
             ```";
        let calls = parse_tool_calls(text);
        assert_eq!(calls.len(), 1, "the f-string call is words, not a call");
        assert_eq!(calls[0].name, "code/shell");
        assert_eq!(
            calls[0].input["command"],
            "mkdir -p analysis_modules/{text_processing,data_analysis}"
        );
    }

    // what this catches: idiom-10 bare positional args map to each tool's
    // live-observed default key, and an UNRESOLVED f-string positional
    // (`code/read f"{file}"`) stays inert.
    #[test]
    fn cli_flag_bare_positional_maps_default_key() {
        let calls =
            parse_tool_calls("code/shell \"echo -n 'continuum' | sha256sum\"");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "code/shell");
        assert_eq!(calls[0].input["command"], "echo -n 'continuum' | sha256sum");

        assert!(
            parse_tool_calls("code/read f\"{file}\"").is_empty(),
            "an f-string positional is an unresolved template — words"
        );
    }

    // what this catches: the precision line. Prose lines starting with a
    // path-ish token, dotted file citations, and bare tool names with no args
    // must all stay speech — only flag-shaped (`--` or `=`) remainders lift.
    #[test]
    fn cli_flag_prose_and_citations_stay_inert() {
        for text in [
            "analysis_modules/text_processing is for NLP components",
            "word_freq_analysis/text_cleaner.py handles normalization",
            "system/memory-budget",
            "the src/main entry point loads config",
        ] {
            assert!(
                parse_tool_calls(text).is_empty(),
                "must stay speech: {text}"
            );
        }
    }

    // what this catches: idiom 8 — a fence whose ENTIRE content is one
    // paren-call lifts (the fence delimiters are the intent markers), while a
    // fence containing a call plus other code does not match THIS format.
    #[test]
    fn fenced_sole_paren_call_lifts() {
        let calls = parse_tool_calls("```code/shell(command=\"cargo test\")```");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "code/shell");
        assert_eq!(calls[0].input["command"], "cargo test");

        // Block form with the call as the sole body also lifts.
        let calls = parse_tool_calls("```\ncode/shell(command=\"ls -la\")\n```");
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].input["command"], "ls -la");

        // An ordinary code fence full of python stays speech for THIS format
        // (function() mentions carry no slash-token).
        assert!(parse_tool_calls(
            "```python\ndef tokenize(text):\n    return text.split()\n```"
        )
        .is_empty());
    }

    // what this catches: Atlas's first post-deploy attempt (2026-07-12) — a
    // fence whose sole content is a bare zero-arg slash-token (`code/list`)
    // lifts as a no-args call, while fenced file citations (dots) and prose
    // fences stay speech.
    #[test]
    fn fenced_bare_zero_arg_tool_name_lifts() {
        let calls = parse_tool_calls(
            "I'll use the code/list command to see the current file structure:\n```python\ncode/list\n```",
        );
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "code/list");
        assert!(calls[0].input.as_object().unwrap().is_empty());

        // Fenced file citation (dot) and fenced prose stay speech.
        assert!(parse_tool_calls("```src/main.rs```").is_empty());
        assert!(parse_tool_calls("```just some words here```").is_empty());
    }

    // what this catches: Casper's live combo (2026-07-12) — a paren-call line
    // (`code/list()`) beside OTHER pseudo-code in the same fence lifts via the
    // per-line scan, while the invented non-slash name (`file_tree(...)`) on
    // the sibling line stays inert.
    #[test]
    fn paren_call_line_lifts_beside_pseudo_code() {
        let calls = parse_tool_calls(
            "I'll run both to get a comprehensive view:\n```python\nfile_tree(max_depth=2)\ncode/list()\n```",
        );
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "code/list");
        assert!(calls[0].input.as_object().unwrap().is_empty());
    }

    // what this catches: Mistral/Devstral's native `[TOOL_CALLS]` marker must lift —
    // without it EVERY Devstral tool call silently no-ops (glass-boxed 2026-07-14:
    // cognition/eval 1/13 on tool tasks, acts:0, the persona rendered handless). The
    // marker precedes a call in an already-supported sub-form; a marker before a
    // non-call reserved token (`[active-work]`) lifts nothing.
    #[test]
    fn mistral_tool_calls_marker_lifts_the_native_devstral_format() {
        // paren-call after the marker (the exact live shape)
        let c = parse_tool_calls("I'll search now.\n[TOOL_CALLS]code/search({\"pattern\": \"fn build\"})");
        assert_eq!(c.len(), 1, "paren-call after marker lifts");
        assert_eq!(c[0].name, "code/search");
        assert_eq!(c[0].input["pattern"], "fn build");

        // Mistral canonical JSON array after the marker
        let c = parse_tool_calls("[TOOL_CALLS][{\"name\": \"code/list\", \"arguments\": {\"path\": \"core\"}}]");
        assert_eq!(c.len(), 1, "canonical json-array after marker lifts");
        assert_eq!(c[0].name, "code/list");
        assert_eq!(c[0].input["path"], "core");

        // marker before a NON-call reserved token → nothing
        assert!(parse_tool_calls("[TOOL_CALLS][active-work] card 08ece9e8 claimed").is_empty(),
            "a marker before reserved vocab is not a tool call");
    }

    // what this catches: a [TOOL_CALLS] marker that names a NON-tool (reserved receipt
    // vocab [recall]/[action]) must be REPORTED as an attempted-but-unlifted call so the
    // verdict can fail loud through the executor's unknown-command teacher (#158/#159) —
    // never silently pass as speech (acts:0 spiral, glass-boxed 2026-07-16, Anwen's
    // exact live "[TOOL_CALLS][recall]" emission). A well-formed native call is NOT a
    // failed attempt (it lifts on the normal path).
    #[test]
    fn attempted_tool_name_flags_reserved_vocab_mimicry_not_real_calls() {
        // Anwen's exact live emission — the [recall] receipt token mimicked as a call.
        assert_eq!(
            attempted_tool_name("[TOOL_CALLS][recall]\nYou are Anwen. You were handed the silent hatch").as_deref(),
            Some("recall"),
            "reserved [recall] after the marker is a failed tool attempt"
        );
        assert_eq!(
            attempted_tool_name("[TOOL_CALLS][active-work] card claimed").as_deref(),
            Some("active-work")
        );
        // A well-formed native call LIFTS on the normal path → NOT a failed attempt.
        assert_eq!(
            attempted_tool_name("[TOOL_CALLS]code/search({\"pattern\": \"fn build\"})"),
            None,
            "a real call that lifts must not be flagged as a failed attempt"
        );
        // No marker → ordinary prose (incl. an eval question) is never a tool attempt.
        assert_eq!(
            attempted_tool_name("Which file defines the struct WorkspaceCycle?"),
            None
        );
    }

    // what this catches: Casper's EXACT live emission (glass-boxed 2026-07-17) —
    // a hallucinated `[room-roster]` tool followed by prose. #159 must flag it as a
    // failed attempt (Some("room-roster")) so it routes to the executor's teacher;
    // if it returns None the turn falls through to Speech and the bogus tag leaks,
    // exactly as observed. Also asserts parse_tool_calls does NOT spuriously lift a
    // call from the prose (the guard that would wrongly suppress the flag).
    #[test]
    fn attempted_tool_name_flags_hallucinated_room_roster_from_live_emission() {
        let live = "[TOOL_CALLS][room-roster] (no one else is present right now)\n\
                    The room is empty aside from you. The question is addressed to you alone.";
        assert!(
            parse_tool_calls(live).is_empty(),
            "no valid tool should lift from the hallucinated-tag prose"
        );
        assert_eq!(
            attempted_tool_name(live).as_deref(),
            Some("room-roster"),
            "the hallucinated tool must be flagged so #159 routes it to the teacher"
        );
    }
}
