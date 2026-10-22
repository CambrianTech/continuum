use super::*;

pub type ResolveName = fn(&str) -> String;

/// The strict JSON shape the model is asked to emit to call a tool:
/// `{"tool_call": {"name": "...", "arguments": { ... }}}`. The envelope key
/// (`tool_call`) makes the intent unambiguous to parse out of surrounding prose.
#[derive(Debug, Deserialize)]
pub(super) struct ToolCallEnvelope {
    pub(super) tool_call: ToolCallJson,
}

/// The inner call: which tool + its arguments. `arguments` defaults to `{}` so a
/// no-arg tool can be called as `{"tool_call": {"name": "ping"}}`.
#[derive(Debug)]
pub(super) struct ToolCallJson {
    /// `name` is canonical; `function`/`tool` are accepted as aliases — observed
    /// live 2026-07-10: Asha, after a day of narrating, finally emitted a
    /// STRUCTURED call — `{"function": "work/claim", "params": {…}}` — and the
    /// parser rejected it on key SPELLING alone ([[pass-must-be-trained-not-told]]:
    /// check the action parser before declaring a model gap; she DID emit it).
    pub(super) name: String,
    /// `arguments` is canonical; `parameters`/`params`/`input` are aliases so
    /// Llama/Mistral/OpenAI-ish bare calls all normalize cleanly. When NO
    /// args-family key is present, sibling keys beside the name become the
    /// arguments (see the manual `Deserialize` impl below).
    pub(super) arguments: Value,
}

/// The name-key family a model may use for "which tool". First present wins.
///
/// `command` joined the family 2026-08-05 after the SAME failure this struct's
/// doc already describes, a second time: Asha (Devstral-Small-2507) emitted
/// `{"command": "file_tree", "params": {"path": "."}}` — an OFFERED tool name
/// (`file_tree` is in her tool surface and aliases to `code/tree`), correct
/// params, correct shape — and it was dropped on the name-key SPELLING alone,
/// so she narrated exploration for hours without a single act. `params` was
/// already an accepted args key, which is what makes the miss so sharp: half of
/// her envelope was understood and the half naming the tool was not.
///
/// It is last in the list on purpose — `command` is also `code/shell`'s ARGUMENT
/// name, so `{"command": "cargo test"}` must never forge a tool called
/// "cargo test". [`BareFormat`] enforces that by requiring a command-keyed name
/// to be TOOL-SHAPED (slash, or a wire name the alias registry resolves to one).
pub(super) const NAME_KEYS: &[&str] = &["name", "function", "tool", "command"];

/// The args-key family. First present wins; when ABSENT, the remaining sibling
/// keys become the arguments object.
pub(super) const ARG_KEYS: &[&str] = &["arguments", "parameters", "params", "input"];

/// Manual deserialize replacing the old serde aliases so SIBLING-ARGS emissions
/// lift too (#293, glass-boxed live 2026-07-31): Asha/Atlas/Anwen/Benchy emit
/// `{"function": "code/list", "path": "."}` — the args TOP-LEVEL beside the
/// name, no `arguments`/`params` key at all — and the alias-only shape rejected
/// it, starving every call for hours. Rules:
///   - name: first present of `name`/`function`/`tool`; must be a string
///     (an OpenAI-wire `"function": {…}` object is NOT this shape — refuse so
///     other formats can try).
///   - arguments: first present of `arguments`/`parameters`/`params`/`input`
///     (extra siblings are then ignored, matching the old derive). When none is
///     present, the remaining siblings ARE the arguments — minus call-frame
///     metadata that is never a tool argument (`"type": "function"`, and an
///     `id` in the `call_`/`toolu_`/`jip-` wire shapes).
impl<'de> Deserialize<'de> for ToolCallJson {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::Error;
        let mut map = serde_json::Map::deserialize(deserializer)?;
        let mut name: Option<String> = None;
        for k in NAME_KEYS {
            match map.remove(*k) {
                Some(Value::String(s)) => {
                    if name.is_none() {
                        name = Some(s);
                    }
                }
                Some(_) => return Err(D::Error::custom("tool name is not a string")),
                None => {}
            }
        }
        let name = name.ok_or_else(|| D::Error::custom("missing tool name"))?;
        let mut arguments: Option<Value> = None;
        for k in ARG_KEYS {
            if let Some(v) = map.remove(*k) {
                if arguments.is_none() {
                    arguments = Some(v);
                }
            }
        }
        let arguments = arguments.unwrap_or_else(|| {
            // Sibling-args path: strip call-frame metadata, keep real keys.
            if map.get("type").and_then(Value::as_str) == Some("function") {
                map.remove("type");
            }
            if map.get("id").and_then(Value::as_str).is_some_and(|id| {
                id.starts_with("call_") || id.starts_with("toolu_") || id.starts_with("jip-")
            }) {
                map.remove("id");
            }
            if map.is_empty() {
                Value::Null
            } else {
                Value::Object(std::mem::take(&mut map))
            }
        });
        Ok(ToolCallJson { name, arguments })
    }
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
pub fn parse_tool_call(text: &str, resolve_name: ResolveName) -> Option<ToolCall> {
    parse_tool_calls(text, resolve_name).into_iter().next()
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
pub fn parse_tool_calls(text: &str, resolve_name: ResolveName) -> Vec<ToolCall> {
    for fmt in tool_call_formats() {
        let calls = fmt.parse(text, resolve_name);
        if !calls.is_empty() {
            return calls;
        }
    }
    Vec::new()
}

/// One way a base model surfaces a tool call in text. Pure parsing → canonical
/// [`ToolCall`]s; no model/gateway coupling. The registry ([`tool_call_formats`])
/// is the single place the supported variety lives.
pub(super) trait ToolCallFormat: Send + Sync {
    /// Stable id (telemetry / "which format matched").
    fn id(&self) -> &'static str;
    /// Extract any calls in THIS format from `text` (empty = not this format).
    fn parse(&self, text: &str, resolve_name: ResolveName) -> Vec<ToolCall>;
}

/// The supported surface formats, most-specific first. Extend this list to support
/// a new base model — the only edit a new format requires.
pub(super) fn tool_call_formats() -> &'static [&'static dyn ToolCallFormat] {
    &[
        &MistralToolCallsFormat,
        &EnvelopeFormat,
        &XmlParameterFormat,
        &TaggedFormat,
        &BareFormat,
        &BbcodeCallFormat,
        &BracketTagFormat,
        &SelfClosingTagFormat,
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
pub(super) struct MistralToolCallsFormat;

impl ToolCallFormat for MistralToolCallsFormat {
    fn id(&self) -> &'static str {
        "mistral-tool-calls"
    }
    fn parse(&self, text: &str, resolve_name: ResolveName) -> Vec<ToolCall> {
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
            let calls = fmt.parse(&cleaned, resolve_name);
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
/// The sibling failure [`attempted_tool_name`] cannot see: a citizen fences correct
/// ARGUMENTS and never names the tool, in any liftable position.
///
/// Sahar's verbatim emission, 2026-08-07:
///
/// ````text
/// I will release the card 392bc54e since it is already completed and merged
/// ```json
/// { "card_id": "392bc54e" }
/// ```
/// ````
///
/// Right intent, right argument, tool identity carried ONLY as English. Nothing lifts —
/// correctly, because binding prose intent would also fire on peer coaching (#144, where a
/// fabricated result became room truth in two turns). But nothing REPORTS either:
/// `attempted_tool_name` gates on the `[TOOL_CALLS]` marker as its very first check, and
/// there is no marker here. So the fence is silently dropped, she gets no feedback, and the
/// next generation repeats it. Measured live: the loop ran until the deadline.
///
/// That silence is the defect — not the parser, and not the menu (which already teaches
/// `code/read({…})` as its first line, and that form does lift). She is one token short of a
/// working call and nothing tells her which token.
///
/// Returns the offending snippet for the correction seam to REPORT. It must never be
/// executed: we cannot know which tool she meant, and guessing is exactly the false positive
/// the negatives in `bare_args_fence_with_named_tool_lifts_and_coaching_stays_inert` guard.
///
/// Deliberately conservative, because this fires on ordinary speech otherwise:
/// - nothing lifted anywhere in the turn (a real call means she managed it)
/// - no `[TOOL_CALLS]` marker (that case belongs to [`attempted_tool_name`])
/// - a FENCED JSON **object** (bare prose JSON is data, not an attempted call)
/// - argument-SHAPED: 1..=6 keys, all scalar values. A nested or long object is a data
///   payload a persona is legitimately showing, not an argument bag.
pub fn nameless_args_fence(text: &str, resolve_name: ResolveName) -> Option<String> {
    if text.contains("[TOOL_CALLS]") || !parse_tool_calls(text, resolve_name).is_empty() {
        return None;
    }
    for block in fenced_blocks(text) {
        // A shell fence is a script she is showing, never an argument bag.
        if fence_is_shell(&block) {
            continue;
        }
        let span = block.body.trim();
        if !span.starts_with('{') || !span.ends_with('}') {
            continue;
        }
        let Ok(serde_json::Value::Object(map)) = serde_json::from_str::<serde_json::Value>(span)
        else {
            continue;
        };
        if map.is_empty() || map.len() > 6 {
            continue;
        }
        // An argument bag is flat scalars. Anything structured is a payload she is showing.
        if map.values().any(|v| v.is_object() || v.is_array()) {
            continue;
        }
        return Some(span.to_string());
    }
    None
}

pub fn attempted_tool_name(text: &str, resolve_name: ResolveName) -> Option<String> {
    if !text.contains("[TOOL_CALLS]") {
        return None;
    }
    // A real call lifted — this is not a failed attempt, leave it to the normal path.
    if !parse_tool_calls(text, resolve_name).is_empty() {
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
pub(super) struct BbcodeCallFormat;

impl ToolCallFormat for BbcodeCallFormat {
    fn id(&self) -> &'static str {
        "bbcode-call"
    }
    fn parse(&self, text: &str, _resolve_name: ResolveName) -> Vec<ToolCall> {
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
            let Some(paren) = body.find('(') else {
                continue;
            };
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
pub fn paren_call_args(s: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
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
pub(super) struct BracketTagFormat;

impl ToolCallFormat for BracketTagFormat {
    fn id(&self) -> &'static str {
        "bracket-tag"
    }
    fn parse(&self, text: &str, _resolve_name: ResolveName) -> Vec<ToolCall> {
        let mut out = Vec::new();
        for line in text.lines() {
            let line = line.trim_end();
            let Some(open) = line.rfind('[') else {
                continue;
            };
            let Some(close_rel) = line[open..].find(']') else {
                continue;
            };
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
pub(super) fn bracket_tag_args(mut s: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
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

/// A self-closing XML-ish tag: `<write_file file_path="x.py" content="…" />`.
/// qwen2.5-coder emits this idiom (glass-boxed 2026-07-22 via `agent/solve`: the 7B
/// wrote `<write_file file_path="reverse.py" content="def reverse_string(s):\n…" />`
/// then `STOP` — the call never lifted, `acts:0`, empty patch, while the SAME model on
/// a different sample emitted a native tool_call that worked). It's the OpenHands/HTML
/// hybrid a coder model reaches for; the wire name (`write_file`) resolves to the
/// canonical command (`code/write`) downstream via `tool_dialect::from_wire_name` (#159),
/// exactly like the narrated snake_case path — so this format only has to LIFT it.
///
/// Precision guard that stays pure (no registry coupling, per the trait contract): the
/// tag name must contain `_` or `/`. That keeps every HTML void element inert (`<br/>`,
/// `<img src=…/>`, `<input .../>`, `<meta .../>`, `<hr/>` — none carry `_` or `/`) while
/// lifting `write_file` / `list_files` / `create_workspace` / `code/write`. Require ≥1
/// `key="value"` attribute (the BracketTag ≥1-arg precedent — a bare `<foo/>` stays
/// inert). Attribute VALUES are JSON-unescaped (`\n`→newline, `\t`, `\"`, `\\`): the model
/// emits code with JSON-style escapes inside the attr, and writing a literal `\n` would
/// corrupt the file. An invented name lifts and fails LOUD at the executor (NarratedBareArgs
/// precedent); ACL unchanged.
pub(super) struct SelfClosingTagFormat;

impl ToolCallFormat for SelfClosingTagFormat {
    fn id(&self) -> &'static str {
        "self-closing-tag"
    }
    fn parse(&self, text: &str, _resolve_name: ResolveName) -> Vec<ToolCall> {
        let mut out = Vec::new();
        let bytes = text;
        let mut search = 0;
        while let Some(rel_open) = bytes[search..].find('<') {
            let open = search + rel_open;
            // Find the self-closing terminator `/>` for THIS tag. Scan from just after
            // '<'; a nested '<' before any '/>' means this wasn't a self-closing tag.
            let after = open + 1;
            let Some(rel_close) = bytes[after..].find("/>") else {
                break; // no more self-closing tags anywhere
            };
            let close = after + rel_close;
            search = close + 2; // advance past this tag for the next iteration
            if bytes[after..close].contains('<') {
                continue; // a '<' inside → not a clean self-closing tag; skip
            }
            let inner = bytes[after..close].trim();
            let mut parts = inner.splitn(2, char::is_whitespace);
            let name = parts.next().unwrap_or("");
            // Tool guard: name must look like a tool (`_` or `/`), not an HTML void element.
            if !((name.contains('_') || name.contains('/'))
                && !name.contains(char::is_whitespace)
                && name.len() <= 64
                && !name.starts_with("http"))
            {
                continue;
            }
            let Some(mut args) = bracket_tag_args(parts.next().unwrap_or("")) else {
                continue; // leftover after key="value" pairs → whole tag inert
            };
            if args.is_empty() {
                continue; // require ≥1 attribute (bare `<write_file/>` stays inert)
            }
            // JSON-unescape each value so `\n`/`\t`/`\"` in code content become real chars.
            for v in args.values_mut() {
                if let serde_json::Value::String(s) = v {
                    *s = json_unescape_scalars(s);
                }
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

/// Unescape the common JSON string escapes a model writes inside an attribute value
/// (`\n`, `\t`, `\r`, `\"`, `\\`, `\/`). Deliberately minimal — NOT a full JSON parser;
/// unknown escapes (`\x`, `\u…`) are left verbatim so nothing is silently mangled.
pub(super) fn json_unescape_scalars(s: &str) -> String {
    if !s.contains('\\') {
        return s.to_string();
    }
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('n') => out.push('\n'),
            Some('t') => out.push('\t'),
            Some('r') => out.push('\r'),
            Some('"') => out.push('"'),
            Some('\\') => out.push('\\'),
            Some('/') => out.push('/'),
            Some(other) => {
                out.push('\\');
                out.push(other);
            }
            None => out.push('\\'),
        }
    }
    out
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
pub(super) struct CliFlagFormat;
