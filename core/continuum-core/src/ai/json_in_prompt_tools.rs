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
    &[
        &EnvelopeFormat,
        &TaggedFormat,
        &BareFormat,
        &NarratedWriteFormat,
        &NarratedScriptFormat,
    ]
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
}
