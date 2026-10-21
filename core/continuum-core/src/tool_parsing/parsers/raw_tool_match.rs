use super::*;

/// Internal representation of a matched tool call with position info.
pub struct RawToolMatch {
    pub tool_name: String,
    pub parameters: HashMap<String, String>,
    pub format: &'static str,
    pub start: usize,
    pub end: usize,
}

/// Parse all tool calls from response text using all format adapters.
/// Returns matches in order of adapter priority (Anthropic first).
pub fn parse_all_formats(text: &str) -> Vec<RawToolMatch> {
    let mut results = Vec::new();
    results.extend(parse_anthropic(text));
    results.extend(parse_function_style(text));
    results.extend(parse_bare(text));
    results.extend(parse_json_object(text));
    results.extend(parse_array_style(text));
    results.extend(parse_curly_shorthand(text));
    results.extend(parse_markdown(text));
    results.extend(parse_old_style(text));
    results.extend(parse_colon_shorthand(text));
    results
}

// ─── Anthropic XML ──────────────────────────────────────────────────
pub(super) static RE_ANTHROPIC: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)<tool_use>(.*?)</tool_use>").unwrap());

pub(super) static RE_TOOL_NAME: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)<tool_name>(.*?)</tool_name>").unwrap());

pub(super) static RE_PARAMS_BLOCK: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)<parameters>(.*?)</parameters>").unwrap());

pub(super) fn parse_anthropic(text: &str) -> Vec<RawToolMatch> {
    RE_ANTHROPIC
        .find_iter(text)
        .filter_map(|m| {
            let block = m.as_str();
            let name = RE_TOOL_NAME
                .captures(block)?
                .get(1)?
                .as_str()
                .trim()
                .to_string();
            let params_block = RE_PARAMS_BLOCK
                .captures(block)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str())
                .unwrap_or("");
            let parameters = extract_xml_params(params_block);
            Some(RawToolMatch {
                tool_name: name,
                parameters,
                format: "anthropic-style",
                start: m.start(),
                end: m.end(),
            })
        })
        .collect()
}

// ─── Function-style ─────────────────────────────────────────────────
// Match both proper XML and Groq's variant format:
//   <function=name>{"param": "value"}</function>   — standard
//   function=name>{"param": "value"}               — Groq variant (no < prefix, no closing tag)
pub(super) static RE_FUNCTION: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?si)<?function=([^>\s]+)>\s*(\{[\s\S]*?\})\s*(?:</function>)?").unwrap()
});

pub(super) fn parse_function_style(text: &str) -> Vec<RawToolMatch> {
    RE_FUNCTION
        .captures_iter(text)
        .filter_map(|cap| {
            let name = cap.get(1)?.as_str().trim().to_string();
            let body = cap.get(2).map(|m| m.as_str().trim()).unwrap_or("");
            let parameters = parse_json_params(body);
            let full_match = cap.get(0)?;
            Some(RawToolMatch {
                tool_name: name,
                parameters,
                format: "function-style",
                start: full_match.start(),
                end: full_match.end(),
            })
        })
        .collect()
}

// ─── Bare JSON ──────────────────────────────────────────────────────
// Slash-based prefixes (canonical tool names: code/tree, data/list, etc.)
pub(super) const TOOL_PREFIXES_SLASH: &[&str] = &[
    "code/",
    "data/",
    "collaboration/",
    "ai/",
    "voice/",
    "search/",
    "workspace/",
    "file/",
    "interface/",
    "genome/",
    "adapter/",
    "persona/",
    "runtime/",
    "session/",
    "user/",
    "logs/",
    "media/",
];

// Underscore-based prefixes (sanitized names from native tool protocol:
// code_tree, collaboration_chat_send, etc.)
pub(super) const TOOL_PREFIXES_UNDERSCORE: &[&str] = &[
    "code_",
    "data_",
    "collaboration_",
    "ai_",
    "voice_",
    "search_",
    "workspace_",
    "file_",
    "interface_",
    "genome_",
    "adapter_",
    "persona_",
    "runtime_",
    "session_",
    "user_",
    "logs_",
    "media_",
];

pub(super) fn all_prefix_pattern() -> String {
    TOOL_PREFIXES_SLASH
        .iter()
        .chain(TOOL_PREFIXES_UNDERSCORE.iter())
        .map(|p| regex::escape(p))
        .collect::<Vec<_>>()
        .join("|")
}

pub(super) static RE_BARE: Lazy<Regex> = Lazy::new(|| {
    let prefix_pat = all_prefix_pattern();
    // Match tool call with optional backticks, optional trailing </function>
    Regex::new(&format!(
        r"`?(?:{})[a-zA-Z0-9/_-]+`?\s*\{{[^{{}}]*(?:\{{[^{{}}]*\}}[^{{}}]*)*\}}\s*(?:</function>)?",
        prefix_pat
    ))
    .unwrap()
});

pub(super) static RE_BARE_PARSE: Lazy<Regex> = Lazy::new(|| {
    let prefix_pat = all_prefix_pattern();
    Regex::new(&format!(
        r"(?s)`?((?:{})[a-zA-Z0-9/_-]+)`?\s*(\{{.+?\}})\s*(?:</function>)?",
        prefix_pat
    ))
    .unwrap()
});

/// Unsanitize a tool name: convert underscore-based names back to slash-based.
/// e.g. "code_tree" → "code/tree", "collaboration_chat_send" → "collaboration/chat/send"
///
/// Tool names in this system use camelCase within segments (never snake_case),
/// so all underscores in a sanitized name are path separators.
pub(super) fn unsanitize_tool_name(name: &str) -> String {
    // Already uses slashes — canonical form
    if name.contains('/') {
        return name.to_string();
    }
    // Check if name starts with a known prefix root
    let prefix_roots: &[&str] = &[
        "collaboration",
        "code",
        "data",
        "ai",
        "voice",
        "search",
        "workspace",
        "file",
        "interface",
        "genome",
        "adapter",
        "persona",
        "runtime",
        "session",
        "user",
        "logs",
        "media",
    ];
    for root in prefix_roots {
        if name.starts_with(root) && name.len() > root.len() && name.as_bytes()[root.len()] == b'_'
        {
            // Replace ALL underscores with slashes (tool segments use camelCase, not snake_case)
            return name.replace('_', "/");
        }
    }
    // No known prefix — return as-is
    name.to_string()
}

pub(super) fn parse_bare(text: &str) -> Vec<RawToolMatch> {
    RE_BARE
        .find_iter(text)
        .filter_map(|m| {
            let full = m.as_str();
            let cap = RE_BARE_PARSE.captures(full)?;
            let raw_name = cap.get(1)?.as_str().trim();
            let name = unsanitize_tool_name(raw_name);
            let json_str = cap.get(2)?.as_str().trim();
            let parameters = parse_json_params(json_str);
            Some(RawToolMatch {
                tool_name: name,
                parameters,
                format: "bare-tool-call",
                start: m.start(),
                end: m.end(),
            })
        })
        .collect()
}

// ─── JSON Object ───────────────────────────────────────────────────
// Matches tool calls in JSON object format (two variants):
//   {"name": "code_tree", "parameters": {"path": "."}}
//   {"type": "function", "name": "code_git", "parameters": {"operation": "status"}}
// Used by Fireworks and some OpenAI-compatible models
pub(super) static RE_JSON_TOOL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\{(?:\s*"type"\s*:\s*"[^"]*"\s*,)?\s*"name"\s*:\s*"([^"]+)"\s*,\s*"parameters"\s*:\s*(\{[^{}]*(?:\{[^{}]*\}[^{}]*)*\})\s*\}"#).unwrap()
});

pub(super) fn parse_json_object(text: &str) -> Vec<RawToolMatch> {
    RE_JSON_TOOL
        .captures_iter(text)
        .filter_map(|cap| {
            let raw_name = cap.get(1)?.as_str().trim();
            let name = unsanitize_tool_name(raw_name);
            let json_str = cap.get(2)?.as_str().trim();
            let parameters = parse_json_params(json_str);
            let full_match = cap.get(0)?;
            Some(RawToolMatch {
                tool_name: name,
                parameters,
                format: "json-object",
                start: full_match.start(),
                end: full_match.end(),
            })
        })
        .collect()
}

// ─── Array-style ───────────────────────────────────────────────────
// Matches: ["code/search", {"pattern": "test"}]
//          ["collaboration_chat_send", {"room": "general", "message": "hello"}]
pub(super) static RE_ARRAY_STYLE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\[\s*"([^"]+)"\s*,\s*(\{[^{}]*(?:\{[^{}]*\}[^{}]*)*\})\s*\]"#).unwrap()
});

/// Check if a name looks like a tool name (has slash or starts with known prefix after unsanitize).
pub(super) fn looks_like_tool_name(name: &str) -> bool {
    if name.contains('/') {
        return TOOL_PREFIXES_SLASH.iter().any(|p| name.starts_with(p));
    }
    TOOL_PREFIXES_UNDERSCORE.iter().any(|p| name.starts_with(p))
}

pub(super) fn parse_array_style(text: &str) -> Vec<RawToolMatch> {
    RE_ARRAY_STYLE
        .captures_iter(text)
        .filter_map(|cap| {
            let raw_name = cap.get(1)?.as_str().trim();
            if !looks_like_tool_name(raw_name) {
                return None;
            }
            let name = unsanitize_tool_name(raw_name);
            let json_str = cap.get(2)?.as_str().trim();
            let parameters = parse_json_params(json_str);
            let full_match = cap.get(0)?;
            Some(RawToolMatch {
                tool_name: name,
                parameters,
                format: "array-style",
                start: full_match.start(),
                end: full_match.end(),
            })
        })
        .collect()
}

// ─── Curly-brace shorthand ─────────────────────────────────────────
// Matches: {collaboration_wall_write: {"append": true, "content": "hello"}}
//          {code_tree: {"path": "."}}
pub(super) fn parse_curly_shorthand(text: &str) -> Vec<RawToolMatch> {
    // Find JSON objects via serde, check if single-key with tool-like name
    // Use regex to find candidate { ... } blocks first, then validate with serde
    static RE_OUTER_BRACE: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"\{[^{}]*(?:\{[^{}]*\}[^{}]*)*\}").unwrap());

    RE_OUTER_BRACE
        .find_iter(text)
        .filter_map(|m| {
            let block = m.as_str();
            let parsed: serde_json::Value = serde_json::from_str(block).ok()?;
            let obj = parsed.as_object()?;

            // Must be single-key object
            if obj.len() != 1 {
                return None;
            }

            let (raw_name, value) = obj.iter().next()?;

            // Key must look like a tool name
            if !looks_like_tool_name(raw_name) {
                return None;
            }

            // Value must be an object (the parameters)
            let params_obj = value.as_object()?;

            let name = unsanitize_tool_name(raw_name);
            let parameters: HashMap<String, String> = params_obj
                .iter()
                .map(|(k, v)| {
                    let s = match v {
                        serde_json::Value::String(s) => s.clone(),
                        _ => v.to_string(),
                    };
                    (k.clone(), s)
                })
                .collect();

            Some(RawToolMatch {
                tool_name: name,
                parameters,
                format: "curly-shorthand",
                start: m.start(),
                end: m.end(),
            })
        })
        .collect()
}

// ─── Markdown backtick ──────────────────────────────────────────────
pub(super) static RE_MD_TOOL: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)`tool:\s*([^`]+)`").unwrap());

pub(super) static RE_MD_PARAM: Lazy<Regex> = Lazy::new(|| Regex::new(r"`([^`=]+)=([^`]*)`").unwrap());

pub(super) fn parse_markdown(text: &str) -> Vec<RawToolMatch> {
    let mut results = Vec::new();
    let mut current_lines: Vec<&str> = Vec::new();
    let mut current_start = 0usize;
    let mut char_offset = 0usize;

    for line in text.split('\n') {
        if RE_MD_TOOL.is_match(line) {
            // Flush previous match
            if !current_lines.is_empty() {
                let combined = current_lines.join(" ");
                if let Some((name, params)) = parse_markdown_match(&combined) {
                    results.push(RawToolMatch {
                        tool_name: name,
                        parameters: params,
                        format: "markdown-backtick",
                        start: current_start,
                        end: char_offset,
                    });
                }
            }
            current_lines = vec![line];
            current_start = char_offset;
        } else if !current_lines.is_empty() && line.contains('`') && line.contains('=') {
            current_lines.push(line);
        }
        char_offset += line.len() + 1; // +1 for newline
    }

    // Final match
    if !current_lines.is_empty() {
        let combined = current_lines.join(" ");
        if let Some((name, params)) = parse_markdown_match(&combined) {
            results.push(RawToolMatch {
                tool_name: name,
                parameters: params,
                format: "markdown-backtick",
                start: current_start,
                end: char_offset,
            });
        }
    }

    results
}

pub(super) fn parse_markdown_match(text: &str) -> Option<(String, HashMap<String, String>)> {
    let name = RE_MD_TOOL
        .captures(text)?
        .get(1)?
        .as_str()
        .trim()
        .to_string();
    let mut params = HashMap::new();
    for cap in RE_MD_PARAM.captures_iter(text) {
        if let (Some(k), Some(v)) = (cap.get(1), cap.get(2)) {
            let key = k.as_str().trim();
            if key != "tool" {
                params.insert(key.to_string(), v.as_str().trim().to_string());
            }
        }
    }
    Some((name, params))
}

// ─── Old-style XML ──────────────────────────────────────────────────
pub(super) static RE_OLD_STYLE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?s)<tool\s+name="([^"]+)">(.*?)</tool>"#).unwrap());

pub(super) fn parse_old_style(text: &str) -> Vec<RawToolMatch> {
    RE_OLD_STYLE
        .captures_iter(text)
        .filter_map(|cap| {
            let name = cap.get(1)?.as_str().trim().to_string();
            let body = cap.get(2).map(|m| m.as_str()).unwrap_or("");
            let parameters = extract_xml_params(body);
            let full_match = cap.get(0)?;
            Some(RawToolMatch {
                tool_name: name,
                parameters,
                format: "old-style",
                start: full_match.start(),
                end: full_match.end(),
            })
        })
        .collect()
}

// ─── Colon shorthand ─────────────────────────────────────────────────
//
// QAT-trained local models (14B Q5_K_S) generate tool calls in a natural
// colon-separated format instead of `<tool_use>` XML. Patterns:
//
// JSON params:  `code/read: {filePath: "path/to/file"}`
// Write+block:  `code/write: {filePath: "app.py",\n```python\ncontent\n````
// Bare value:   `code/write: hello.py\n```python\ncontent\n\`\`\``
// Bare simple:  `code/shell/execute: ls -la`
//
// Unquoted JSON keys (e.g. `filePath: "value"`) are handled in parse_colon_params.
// For `code/write`, content embedded in a trailing markdown code block is extracted.
// Incomplete code blocks (truncated responses) are handled via open-block fallback.
// Tool name prefix set for colon-shorthand (matches known tool namespaces).
pub(super) static COLON_TOOL_PREFIX_RE: &str = r"(?:code|data|collaboration|ai|voice|search|workspace|file|interface|genome|adapter|persona|runtime|session|user|logs|media)/[\w/]+";

// JSON-params variant: `tool/name: {key: "value"...}`
pub(super) static RE_COLON_TOOL_LINE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r"(?m)^({}):\s*\{{([^\n\}}]*)",
        COLON_TOOL_PREFIX_RE
    ))
    .unwrap()
});

// Bare-value variant: `tool/name: bare_value` (no leading `{`)
// Captures the rest of the line after the colon-space.
// JSON-params cases (value starts with `{`) are filtered out in the parse loop.
pub(super) static RE_COLON_BARE_LINE: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!(r"(?m)^({}):\s*([^\n]+)", COLON_TOOL_PREFIX_RE)).unwrap());

/// Map a tool name to its first (only positional) parameter key.
pub(super) fn first_positional_param(tool: &str) -> &'static str {
    match tool {
        "code/write" | "code/read" | "code/edit" | "code/diff" | "code/undo" => "filePath",
        "code/tree" => "path",
        "code/shell/execute" | "code/shell/watch" | "code/shell/status" => "command",
        "code/search" => "query",
        _ => "path",
    }
}

/// Extract code block content from text, handling incomplete (truncated) blocks.
/// Returns (content, bytes_consumed_in_after).
pub(super) fn extract_code_block(after: &str) -> Option<(String, usize)> {
    // Complete code block: ```lang\ncontent\n```
    static RE_CODE_BLOCK: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"(?s)```(?:\w*)\n(.*?)\n```").unwrap());
    // Open/incomplete code block: ```lang\ncontent (no closing ```)
    static RE_OPEN_BLOCK: Lazy<Regex> =
        Lazy::new(|| Regex::new(r"```(?:\w*)\n([\s\S]+)$").unwrap());

    let trimmed = after.trim_start_matches([',', '}', '\n', '\r', ' ']);
    let offset = after.len() - trimmed.len();

    if let Some(cb) = RE_CODE_BLOCK.captures(trimmed) {
        let content = cb.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
        let consumed = offset + cb.get(0).unwrap().end();
        Some((content, consumed))
    } else if let Some(ob) = RE_OPEN_BLOCK.captures(trimmed) {
        // Truncated response: accept content up to end of string
        let content = ob
            .get(1)
            .map(|m| m.as_str().trim_end())
            .unwrap_or("")
            .to_string();
        if content.is_empty() {
            return None;
        }
        let consumed = after.len(); // consumed everything
        Some((content, consumed))
    } else {
        None
    }
}

pub(super) fn parse_colon_shorthand(text: &str) -> Vec<RawToolMatch> {
    let mut results = Vec::new();

    // ── Pass 1: JSON-params variant `tool: {key: "val"...}` ──────────
    for cap in RE_COLON_TOOL_LINE.captures_iter(text) {
        let full_match = cap.get(0).unwrap();
        let name = cap.get(1).unwrap().as_str().trim().to_string();
        let inline_params = cap.get(2).map(|m| m.as_str()).unwrap_or("");

        let mut parameters = parse_colon_params(inline_params);

        let match_end = full_match.end();
        let after = &text[match_end..];

        if (name == "code/write" && !parameters.contains_key("content"))
            || (name == "code/edit" && !parameters.contains_key("newString"))
        {
            let param_key = if name == "code/write" {
                "content"
            } else {
                "newString"
            };
            if let Some((content, consumed)) = extract_code_block(after) {
                parameters.insert(param_key.to_string(), content);
                results.push(RawToolMatch {
                    tool_name: name,
                    parameters,
                    format: "colon-shorthand",
                    start: full_match.start(),
                    end: match_end + consumed,
                });
                continue;
            }
        }

        if !parameters.is_empty() {
            results.push(RawToolMatch {
                tool_name: name,
                parameters,
                format: "colon-shorthand",
                start: full_match.start(),
                end: match_end,
            });
        }
    }

    // ── Pass 2: Bare-value variant `tool: bare_value` ────────────────
    // Only emit if no JSON-params match already covers this position.
    'bare: for cap in RE_COLON_BARE_LINE.captures_iter(text) {
        let full_match = cap.get(0).unwrap();
        let start = full_match.start();

        // Skip if a JSON-params result already covers this offset.
        for r in &results {
            if r.start <= start && start < r.end {
                continue 'bare;
            }
        }

        let name = cap.get(1).unwrap().as_str().trim().to_string();
        let raw_value = cap.get(2).map(|m| m.as_str().trim()).unwrap_or("");

        // Skip JSON-params cases (value starts with `{`) — handled by Pass 1.
        if raw_value.starts_with('{') {
            continue;
        }

        // Strip surrounding quotes if the model quoted the value.
        let value = raw_value.trim_matches('"').trim_matches('\'').to_string();
        if value.is_empty() {
            continue;
        }

        let param_key = first_positional_param(&name);
        let mut parameters = HashMap::new();
        parameters.insert(param_key.to_string(), value);

        let match_end = full_match.end();
        let after = &text[match_end..];

        // For code/write, look for trailing code block (content).
        if name == "code/write" {
            if let Some((content, consumed)) = extract_code_block(after) {
                parameters.insert("content".to_string(), content);
                results.push(RawToolMatch {
                    tool_name: name,
                    parameters,
                    format: "colon-shorthand-bare",
                    start: full_match.start(),
                    end: match_end + consumed,
                });
                continue;
            }
        }

        results.push(RawToolMatch {
            tool_name: name,
            parameters,
            format: "colon-shorthand-bare",
            start: full_match.start(),
            end: match_end,
        });
    }

    results
}

/// Extract `key: "value"` pairs from colon-shorthand parameter strings.
/// Handles both `"key": "value"` (quoted key) and `key: "value"` (unquoted key).
pub(super) fn parse_colon_params(s: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();

    // Try valid JSON first (keys and values both quoted).
    let json_candidate = format!("{{{}}}", s);
    if let Ok(serde_json::Value::Object(map)) =
        serde_json::from_str::<serde_json::Value>(&json_candidate)
    {
        return map
            .into_iter()
            .map(|(k, v)| {
                let val = match v {
                    serde_json::Value::String(s) => s,
                    _ => v.to_string(),
                };
                (k, val)
            })
            .collect();
    }

    // Fallback: extract any `key: "value"` or `"key": "value"` pairs.
    static RE_PARAM: Lazy<Regex> = Lazy::new(|| {
        // Matches: optionally-quoted key, colon, then a quoted value.
        Regex::new(r#""?([a-zA-Z_][a-zA-Z0-9_]*)"?\s*:\s*"([^"]*)""#).unwrap()
    });
    for cap in RE_PARAM.captures_iter(s) {
        if let (Some(k), Some(v)) = (cap.get(1), cap.get(2)) {
            params.insert(k.as_str().to_string(), v.as_str().to_string());
        }
    }
    params
}

// ─── Helpers ────────────────────────────────────────────────────────
/// Regex to find opening XML tags: `<tagName>`
pub(super) static RE_XML_OPEN: Lazy<Regex> = Lazy::new(|| Regex::new(r"<(\w+)>").unwrap());

/// Extract `<paramName>value</paramName>` pairs from an XML block.
/// Uses a two-pass approach since Rust regex doesn't support backreferences.
pub fn extract_xml_params(block: &str) -> HashMap<String, String> {
    let mut params = HashMap::new();
    for cap in RE_XML_OPEN.captures_iter(block) {
        let tag_name = cap.get(1).unwrap().as_str();
        let open_tag = cap.get(0).unwrap();
        let after_open = open_tag.end();

        // Look for the matching closing tag
        let close_tag = format!("</{}>", tag_name);
        if let Some(close_pos) = block[after_open..].find(&close_tag) {
            let value = &block[after_open..after_open + close_pos];
            params.insert(tag_name.to_string(), value.trim().to_string());
        }
    }
    params
}

/// Parse JSON object into string parameters (non-strings are JSON-stringified).
pub fn parse_json_params(json_str: &str) -> HashMap<String, String> {
    if json_str.is_empty() {
        return HashMap::new();
    }
    match serde_json::from_str::<serde_json::Value>(json_str) {
        Ok(serde_json::Value::Object(map)) => map
            .into_iter()
            .map(|(k, v)| {
                let s = match &v {
                    serde_json::Value::String(s) => s.clone(),
                    _ => v.to_string(),
                };
                (k, s)
            })
            .collect(),
        _ => {
            // Fallback: extract "key": "value" pairs
            static RE_KV: Lazy<Regex> =
                Lazy::new(|| Regex::new(r#""([^"]+)":\s*"([^"]*)""#).unwrap());
            RE_KV
                .captures_iter(json_str)
                .filter_map(|cap| {
                    Some((
                        cap.get(1)?.as_str().to_string(),
                        cap.get(2)?.as_str().to_string(),
                    ))
                })
                .collect()
        }
    }
}

/// Parse tool calls with model-family-specific parser first, then generic fallback.
/// When model_family is Generic or None, falls through to all 10 generic parsers.
pub fn parse_with_model_family(text: &str, family: ModelFamily) -> Vec<RawToolMatch> {
    // Try model-family-specific parser first
    let family_results = match family {
        ModelFamily::DeepSeek => parse_deepseek(text),
        ModelFamily::Llama => parse_llama(text),
        ModelFamily::Mistral => parse_mistral(text),
        ModelFamily::Hermes => parse_hermes(text),
        ModelFamily::Qwen => parse_qwen(text),
        ModelFamily::Generic => Vec::new(),
    };

    if !family_results.is_empty() {
        return family_results;
    }

    // Fall through to all generic parsers
    parse_all_formats(text)
}

// ─── DeepSeek ──────────────────────────────────────────────────────
//
// DeepSeek v3/R1/Coder uses Unicode fullwidth delimiters:
//   ＜｜tool▁calls▁begin｜＞
//   {"name": "code_search", "arguments": {"pattern": "test"}}
//   ＜｜tool▁calls▁end｜＞
//
// Characters:
//   ＜ = U+FF1C (fullwidth less-than)
//   ＞ = U+FF1E (fullwidth greater-than)
//   ｜ = U+FF5C (fullwidth vertical line)
//   ▁ = U+2581 (lower one eighth block, used as word separator)
pub(super) static RE_DEEPSEEK_BLOCK: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}begin\u{FF5C}\u{FF1E}(.*?)\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}end\u{FF5C}\u{FF1E}")
        .unwrap()
});

/// Parse a single DeepSeek tool call JSON object.
/// Handles both `arguments` as object and `arguments` as double-encoded string.
pub(super) fn parse_deepseek_tool_json(
    value: &serde_json::Value,
) -> Option<(String, HashMap<String, String>)> {
    let obj = value.as_object()?;
    let name = obj.get("name").and_then(|v| v.as_str())?.to_string();
    let params = match obj.get("arguments").or_else(|| obj.get("parameters")) {
        Some(serde_json::Value::Object(map)) => map
            .iter()
            .map(|(k, v)| {
                let s = match v {
                    serde_json::Value::String(s) => s.clone(),
                    _ => v.to_string(),
                };
                (k.clone(), s)
            })
            .collect(),
        Some(serde_json::Value::String(s)) => {
            // Double-encoded: arguments is a JSON string
            parse_json_params(s)
        }
        _ => HashMap::new(),
    };
    Some((name, params))
}
