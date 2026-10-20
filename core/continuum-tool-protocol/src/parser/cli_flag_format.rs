use super::*;

impl ToolCallFormat for CliFlagFormat {
    fn id(&self) -> &'static str {
        "cli-flag"
    }
    fn parse(&self, text: &str, resolve_name: ResolveName) -> Vec<ToolCall> {
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
            // The `[Action #N]` prefix strip meets Atlas's fake-transcript
            // idiom (below) in its single-line form too; the assignment strip
            // meets her script idiom (`result = code/read({...})` — Atlas live
            // 2026-07-22 ×2: subtract narrated it and asked us to run it;
            // multiply's whole call-script got WRITTEN INTO mathlib.py as file
            // content by the narrated-write path, correct fix trapped inside a
            // string). Stripping the binding makes the CALL lift here, and
            // format order does the rest: this format precedes NarratedWrite,
            // so a fence containing liftable call lines is a SCRIPT to
            // execute, never content to write.
            let line = strip_assignment_prefix(strip_action_bracket_prefix(line));
            if line.ends_with(')') {
                if let Some(call) = lift_sole_paren_call(line, resolve_name) {
                    out.push(call);
                    continue;
                }
            }
            // PRETTY-PRINTED multiline paren-call, optionally in a fake
            // transcript frame (Atlas live 2026-07-22, agent/solve maxof):
            //   [Action #2] edit_file({
            //     "file_path": "util.py", ...
            //   })
            //   Result: Fix applied.
            // Perfect args, FABRICATED receipts, zero execution — she mimics
            // the working-memory `[action #N]` shape and invents results. The
            // counter is organic, not a censor: lift the CALL (serde validates
            // the joined args; a valid JSON string can't contain a raw newline,
            // so a literal `})` line can't be value content), and the faculty's
            // one-call-per-generation contract discards her invented `Result:`
            // continuation — the REAL result enters working memory next turn.
            // [[execute-dont-narrate]], [[execution-confabulation-persists-claims-without-receipts]]
            if let Some(open) = line.find('(') {
                let opens_object = line[open + 1..].trim_start().starts_with('{');
                if opens_object && !line.ends_with(')') {
                    let mut span = line.to_string();
                    let mut j = i; // `i` already points past the current line
                    let mut closed = false;
                    while j < lines.len() && j - i < 40 {
                        let l = lines[j];
                        span.push('\n');
                        span.push_str(l);
                        j += 1;
                        if l.trim() == "})" {
                            closed = true;
                            break;
                        }
                    }
                    if closed {
                        if let Some(call) = lift_sole_paren_call(span.trim(), resolve_name) {
                            out.push(call);
                            i = j;
                            continue;
                        }
                    }
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

/// Strip a leading `[...]` bracket tag from a line ONLY when a paren-call shape
/// follows (`[Action #2] edit_file({` → `edit_file({`). This is Atlas's live
/// mimicry of the working-memory `[action #N]` framing (2026-07-22); bracket
/// path citations (`[docs/x.md]`) and substrate tags with prose after them are
/// untouched — the call-shape check is what keeps this narrow.
pub(super) fn strip_action_bracket_prefix(line: &str) -> &str {
    let Some(rest) = line.strip_prefix('[') else {
        return line;
    };
    let Some(close) = rest.find(']') else {
        return line;
    };
    let after = rest[close + 1..].trim_start();
    let looks_like_call = after
        .find('(')
        .map(|p| {
            let name = after[..p].trim();
            !name.is_empty()
                && name.len() <= 64
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '/' || c == '-')
        })
        .unwrap_or(false);
    if looks_like_call {
        after
    } else {
        line
    }
}

/// Strip a simple `ident = ` binding from a line ONLY when a paren-call shape
/// follows (`result = code/read({...})` → `code/read({...})`). The binding is
/// python fiction — there is no return-value plumbing; the observation enters
/// working memory — but the CALL is real intent. A slash-token name can never
/// be legit python anyway (`code/read` parses as division), and no-slash names
/// still pass the registry-resolution guard downstream, so ordinary
/// assignments (`x = compute(y)`, `new_content = """..."""`) stay speech.
pub(super) fn strip_assignment_prefix(line: &str) -> &str {
    let Some(eq) = line.find('=') else {
        return line;
    };
    let lhs = line[..eq].trim();
    let simple_ident = !lhs.is_empty()
        && lhs.len() <= 32
        && lhs.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
        && !lhs.chars().next().is_some_and(|c| c.is_ascii_digit());
    if !simple_ident {
        return line;
    }
    // `==` comparison is not a binding.
    let after = line[eq + 1..].trim_start();
    if after.starts_with('=') {
        return line;
    }
    let looks_like_call = after
        .find('(')
        .map(|p| {
            let name = after[..p].trim();
            !name.is_empty()
                && name.len() <= 64
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '/' || c == '-')
        })
        .unwrap_or(false);
    if looks_like_call {
        after
    } else {
        line
    }
}

/// Split a candidate CLI line into (tool-name, args-remainder). `None` unless the
/// first token is a plausible slash-token tool name (see [`CliFlagFormat`] guards).
pub(super) fn split_cli_head(line: &str) -> Option<(&str, &str)> {
    let line = line.trim_start();
    let head_end = line.find(char::is_whitespace).unwrap_or(line.len());
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
pub(super) fn unclosed_triple_quote(s: &str) -> bool {
    s.matches("\"\"\"").count() % 2 == 1
}

/// Parse the CLI-flag argument grammar for [`CliFlagFormat`]. `None` = this is
/// words, not a call (unparseable remainder, or a template-marker value).
pub(super) fn cli_flag_args(s: &str, tool: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
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
            let end = val_src.find(char::is_whitespace).unwrap_or(val_src.len());
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
pub(super) struct FencedCallFormat;

impl ToolCallFormat for FencedCallFormat {
    fn id(&self) -> &'static str {
        "fenced-call"
    }
    fn parse(&self, text: &str, resolve_name: ResolveName) -> Vec<ToolCall> {
        let mut out = Vec::new();
        let mut rest = text;
        while let Some(open) = rest.find("```") {
            let after = &rest[open + 3..];
            let Some(close) = after.find("```") else {
                break;
            };
            let mut span = after[..close].trim();
            rest = &after[close + 3..];
            // Drop a leading language token line (```python\ncode/list```):
            // a single '/‑less word on its own first line is fence metadata,
            // not call content.
            if let Some((first, body)) = span.split_once('\n') {
                let first = first.trim();
                if !first.is_empty() && !first.contains('/') && !first.contains(char::is_whitespace)
                {
                    span = body.trim();
                }
            }
            // Hash-commented call idiom (Atlas live, 2026-07-22, agent/solve
            // revlist): a ```python fence whose SOLE content is
            // `# edit_file({...})` — the model writes its tool call as a
            // pseudo-code comment, then asks someone to run it. The `#` (or
            // `//`) is presentation, not semantics: strip one leading comment
            // marker and let the ordinary sole-call guards decide. A fence with
            // real code AFTER the comment still refuses (not sole content).
            let span = span
                .strip_prefix("#")
                .or_else(|| span.strip_prefix("//"))
                .map(str::trim_start)
                .unwrap_or(span);
            let Some(call) = lift_sole_paren_call(span, resolve_name) else {
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
pub(super) fn lift_sole_paren_call(span: &str, resolve_name: ResolveName) -> Option<ToolCall> {
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
    // A no-slash name lifts ONLY when the live alias registry resolves it to a
    // real command (`edit_file` → code/edit, `run_code` → code/run) — the
    // strongest possible guard, and self-maintaining: a new command's ALIASES
    // become liftable here with zero parser edits, while `print({...})` /
    // `is_even({...})` stay speech forever (unknown names pass through
    // resolve unchanged, slashless → refused). Atlas live 2026-07-22: her
    // correct `edit_file({...})` sat DEAD in a fence over the missing slash,
    // and the graded patch scored a fake zero.
    let resolves_via_alias = |n: &str| !n.contains('/') && resolve_name(n).contains('/');
    let ok = (name.contains('/') || is_discovery(name) || resolves_via_alias(name))
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
pub(super) struct NarratedBareArgsFormat;

impl ToolCallFormat for NarratedBareArgsFormat {
    fn id(&self) -> &'static str {
        "narrated-bare-args"
    }
    fn parse(&self, text: &str, _resolve_name: ResolveName) -> Vec<ToolCall> {
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
pub(super) fn backticked_tool_token(narration: &str) -> Option<String> {
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
pub(super) struct EnvelopeFormat;

impl ToolCallFormat for EnvelopeFormat {
    fn id(&self) -> &'static str {
        "envelope"
    }
    fn parse(&self, text: &str, _resolve_name: ResolveName) -> Vec<ToolCall> {
        scan_objects(text, |obj| {
            serde_json::from_str::<ToolCallEnvelope>(obj)
                .ok()
                .map(|e| e.tool_call)
                .filter(|c| !c.name.trim().is_empty())
                .map(Into::into)
        })
    }
}

/// `<function=NAME>` then one `<parameter=KEY>VALUE</parameter>` per argument, closed
/// by `</function>` and usually wrapped in `<tool_call>`: the Qwen3-Coder XML dialect
/// that the engine's own template parser reads. This is the floor for a call the engine
/// refuses. Kimi (5090, 2026-09-27) wrote one parameter in the attribute spelling
/// `<parameter name="cmd">` and the next as `<parameter=timeout_ms>`; the engine
/// returned the whole call as content, nothing here read that dialect, and a `git show`
/// was posted to the room as speech instead of running. Both spellings are read.
pub(super) struct XmlParameterFormat;

impl ToolCallFormat for XmlParameterFormat {
    fn id(&self) -> &'static str {
        "xml-parameter"
    }
    fn parse(&self, text: &str, _resolve_name: ResolveName) -> Vec<ToolCall> {
        const OPEN: &str = "<function=";
        const CLOSE: &str = "</function>";
        let mut out = Vec::new();
        let mut rest = text;
        while let Some(open) = rest.find(OPEN) {
            let after = &rest[open + OPEN.len()..];
            let Some(name_end) = after.find('>') else {
                break;
            };
            let name = after[..name_end].trim().trim_matches(['"', '\'']);
            let body = &after[name_end + 1..];
            // No `</function>`, no call: a call cut at max tokens holds only the parameters it
            // finished (a shell cmd without its timeout), and running it would act on half an
            // intention (Cormac on #4447).
            let Some(close) = body.find(CLOSE) else {
                break;
            };
            let (body, next) = (&body[..close], &body[close + CLOSE.len()..]);
            let name_ok = !name.is_empty()
                && name.len() <= 64
                && name
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "/_-.".contains(c));
            if let Some(input) = xml_parameters(body).filter(|_| name_ok) {
                out.push(ToolCall {
                    id: format!("jip-{}", Uuid::new_v4()),
                    name: name.to_string(),
                    input: serde_json::Value::Object(input),
                });
            }
            rest = next;
        }
        out
    }
}

/// The arguments of one XML-dialect call. `None` when a parameter tag is malformed
/// (no key, no close), so a half-written call is never run with half its arguments.
/// A value that is a JSON number, boolean or null keeps that type (`timeout_ms` is a
/// number); anything else is text, so a file body that happens to be JSON is written
/// as the text it is. One newline directly inside each tag is layout, not content.
pub(super) fn xml_parameters(body: &str) -> Option<serde_json::Map<String, serde_json::Value>> {
    const OPEN: &str = "<parameter";
    const CLOSE: &str = "</parameter>";
    let mut map = serde_json::Map::new();
    let mut rest = body;
    while let Some(open) = rest.find(OPEN) {
        let after = &rest[open + OPEN.len()..];
        let tag_end = after.find('>')?;
        let head = after[..tag_end].trim();
        let key = head
            .strip_prefix('=')
            .or_else(|| head.strip_prefix("name="))?
            .trim()
            .trim_matches(['"', '\''])
            .trim();
        if key.is_empty() {
            return None;
        }
        let value = &after[tag_end + 1..];
        let close = value.find(CLOSE)?;
        let raw = value[..close].strip_prefix('\n').unwrap_or(&value[..close]);
        let raw = raw.strip_suffix('\n').unwrap_or(raw);
        let typed = match serde_json::from_str::<serde_json::Value>(raw.trim()) {
            Ok(
                v @ (serde_json::Value::Number(_)
                | serde_json::Value::Bool(_)
                | serde_json::Value::Null),
            ) => v,
            _ => serde_json::Value::String(raw.to_string()),
        };
        map.insert(key.to_string(), typed);
        rest = &value[close + CLOSE.len()..];
    }
    Some(map)
}

/// `<tool_call>{...}</tool_call>` — Qwen / Hermes / NousResearch style. Strips the
/// tags and parses the inner object as a bare or enveloped call.
pub(super) struct TaggedFormat;

impl ToolCallFormat for TaggedFormat {
    fn id(&self) -> &'static str {
        "tagged"
    }
    fn parse(&self, text: &str, resolve_name: ResolveName) -> Vec<ToolCall> {
        let mut out = Vec::new();
        let mut rest = text;
        while let Some(open) = rest.find("<tool_call>") {
            let after = &rest[open + "<tool_call>".len()..];
            let inner = match after.find("</tool_call>") {
                Some(close) => &after[..close],
                None => after, // unterminated tag — take the remainder
            };
            // Inner is usually bare `{name,arguments}`; tolerate an envelope too.
            out.extend(EnvelopeFormat.parse(inner, resolve_name));
            if out.is_empty() {
                out.extend(BareFormat.parse(inner, resolve_name));
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
/// generic. An explicit args-family key present → lift as before. NO args key
/// but SIBLING keys beside the name (`{"function": "code/list", "path": "."}`,
/// the #293 live idiom) → lift with the siblings as arguments, gated on a
/// TOOL-SHAPED name (slash-token, or a wire name the live alias registry
/// resolves to a slash command — the `lift_sole_paren_call` precedent) so prose
/// JSON like `{"name": "Asha", "age": 3}` stays inert. A stray `{"name": ...}`
/// with neither args nor siblings is never a call. (Tried after envelope/tagged,
/// so our precise formats never reach it.)
pub(super) struct BareFormat;

impl ToolCallFormat for BareFormat {
    fn id(&self) -> &'static str {
        "bare"
    }
    fn parse(&self, text: &str, resolve_name: ResolveName) -> Vec<ToolCall> {
        scan_objects(text, |obj| {
            let value: serde_json::Value = serde_json::from_str(obj).ok()?;
            let fields = value.as_object()?;
            let has_name = ["name", "function", "tool", "command"]
                .iter()
                .any(|k| fields.contains_key(*k));
            if !has_name {
                return None; // not a tool call — avoid false positives
            }
            let has_args = ["arguments", "parameters", "params", "input"]
                .iter()
                .any(|k| fields.contains_key(*k));
            let call = serde_json::from_str::<ToolCallJson>(obj).ok()?;
            if call.name.trim().is_empty() {
                return None;
            }
            // Bare JSON has no explicit call intent. Schema/data objects can
            // contain name + params/input too, including nested objects scanned
            // after their parent is rejected. Require the same command shape
            // for every bare lift; explicit envelopes and native calls retain
            // their own handling, including unknown-command feedback.
            let name = call.name.trim();
            let tool_shaped = name.contains('/') || resolve_name(name).contains('/');
            if !tool_shaped {
                return None;
            }
            // A name that could ONLY have come from the `command` key is held to
            // the tool-shaped bar even when an args key is present, because
            // `command` is `code/shell`'s own argument name: the inner object of
            // `{"name":"code/shell","arguments":{"command":"cargo test"}}` scans
            // as a candidate too, and without this a shell invocation would forge
            // a tool literally called "cargo test". Same discriminator the
            // sibling-args lift uses below — does the name RESOLVE, not does the
            // key exist.
            let command_keyed = !["name", "function", "tool"]
                .iter()
                .any(|k| fields.contains_key(*k));
            if command_keyed {
                // A tool-shaped `command` with NO args is a complete no-arg call —
                // the documented `{"tool_call": {"name": "ping"}}` form. Observed
                // live from a second persona minutes after the first:
                // `{"command": "file_tree"}`, meaning "tree the whole workspace".
                // The sibling-args requirement below exists to keep prose like
                // `{"name": "Asha"}` inert; the tool-shaped check already does
                // that job here, since a name only passes if the alias registry
                // resolves it to a real slash command.
                return Some(call.into());
            }
            if !has_args {
                // Sibling-args lift (#293): precision-first. The name must be
                // tool-shaped and at least one real sibling must have survived
                // (metadata-only objects and bare `{"name": …}` stay speech).
                let has_sibling_args = call.arguments.as_object().is_some_and(|o| !o.is_empty());
                if !has_sibling_args {
                    return None;
                }
            }
            Some(call.into())
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
pub(super) struct NarratedWriteFormat;
