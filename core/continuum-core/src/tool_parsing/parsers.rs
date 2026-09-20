//! Format-specific tool call parsers.
//!
//! Formats supported (matching TypeScript ToolFormatAdapter hierarchy):
//! 1. Anthropic XML: `<tool_use><tool_name>X</tool_name><parameters>...</parameters></tool_use>`
//! 2. Function-style: `<function=tool_name>{"param": "value"}</function>`
//! 3. Bare JSON: `tool/name {"param": "value"}` or `tool_name {"param": "value"}</function>`
//! 4. JSON Object: `{"name": "tool_name", "parameters": {"param": "value"}}`
//! 5. Array-style: `["tool/name", {"param": "value"}]`
//! 6. Curly shorthand: `{"tool_name": {"param": "value"}}`
//! 7. Markdown backtick: `` `tool: name` `param=value` ``
//! 8. Old-style XML: `<tool name="X"><param>value</param></tool>`
//! 9. Colon shorthand: `tool/name: {param: "value"}` — QAT local model native format
//! 10. Colon bare: `tool/name: bare_value` — simplified positional variant (degraded models)
//!
//! Handles both canonical (slash) and sanitized (underscore) tool names.
//! Sanitized names from native tool protocol (code_tree → code/tree) are
//! automatically unsanitized back to canonical form.

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;

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

static RE_ANTHROPIC: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)<tool_use>(.*?)</tool_use>").unwrap());
static RE_TOOL_NAME: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)<tool_name>(.*?)</tool_name>").unwrap());
static RE_PARAMS_BLOCK: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)<parameters>(.*?)</parameters>").unwrap());

fn parse_anthropic(text: &str) -> Vec<RawToolMatch> {
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
static RE_FUNCTION: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?si)<?function=([^>\s]+)>\s*(\{[\s\S]*?\})\s*(?:</function>)?").unwrap()
});

fn parse_function_style(text: &str) -> Vec<RawToolMatch> {
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
const TOOL_PREFIXES_SLASH: &[&str] = &[
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
const TOOL_PREFIXES_UNDERSCORE: &[&str] = &[
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

fn all_prefix_pattern() -> String {
    TOOL_PREFIXES_SLASH
        .iter()
        .chain(TOOL_PREFIXES_UNDERSCORE.iter())
        .map(|p| regex::escape(p))
        .collect::<Vec<_>>()
        .join("|")
}

static RE_BARE: Lazy<Regex> = Lazy::new(|| {
    let prefix_pat = all_prefix_pattern();
    // Match tool call with optional backticks, optional trailing </function>
    Regex::new(&format!(
        r"`?(?:{})[a-zA-Z0-9/_-]+`?\s*\{{[^{{}}]*(?:\{{[^{{}}]*\}}[^{{}}]*)*\}}\s*(?:</function>)?",
        prefix_pat
    ))
    .unwrap()
});

static RE_BARE_PARSE: Lazy<Regex> = Lazy::new(|| {
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
fn unsanitize_tool_name(name: &str) -> String {
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

fn parse_bare(text: &str) -> Vec<RawToolMatch> {
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
static RE_JSON_TOOL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\{(?:\s*"type"\s*:\s*"[^"]*"\s*,)?\s*"name"\s*:\s*"([^"]+)"\s*,\s*"parameters"\s*:\s*(\{[^{}]*(?:\{[^{}]*\}[^{}]*)*\})\s*\}"#).unwrap()
});

fn parse_json_object(text: &str) -> Vec<RawToolMatch> {
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

static RE_ARRAY_STYLE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"\[\s*"([^"]+)"\s*,\s*(\{[^{}]*(?:\{[^{}]*\}[^{}]*)*\})\s*\]"#).unwrap()
});

/// Check if a name looks like a tool name (has slash or starts with known prefix after unsanitize).
fn looks_like_tool_name(name: &str) -> bool {
    if name.contains('/') {
        return TOOL_PREFIXES_SLASH.iter().any(|p| name.starts_with(p));
    }
    TOOL_PREFIXES_UNDERSCORE.iter().any(|p| name.starts_with(p))
}

fn parse_array_style(text: &str) -> Vec<RawToolMatch> {
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

fn parse_curly_shorthand(text: &str) -> Vec<RawToolMatch> {
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

static RE_MD_TOOL: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)`tool:\s*([^`]+)`").unwrap());
static RE_MD_PARAM: Lazy<Regex> = Lazy::new(|| Regex::new(r"`([^`=]+)=([^`]*)`").unwrap());

fn parse_markdown(text: &str) -> Vec<RawToolMatch> {
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

fn parse_markdown_match(text: &str) -> Option<(String, HashMap<String, String>)> {
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

static RE_OLD_STYLE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"(?s)<tool\s+name="([^"]+)">(.*?)</tool>"#).unwrap());

fn parse_old_style(text: &str) -> Vec<RawToolMatch> {
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
static COLON_TOOL_PREFIX_RE: &str = r"(?:code|data|collaboration|ai|voice|search|workspace|file|interface|genome|adapter|persona|runtime|session|user|logs|media)/[\w/]+";

// JSON-params variant: `tool/name: {key: "value"...}`
static RE_COLON_TOOL_LINE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(&format!(
        r"(?m)^({}):\s*\{{([^\n\}}]*)",
        COLON_TOOL_PREFIX_RE
    ))
    .unwrap()
});

// Bare-value variant: `tool/name: bare_value` (no leading `{`)
// Captures the rest of the line after the colon-space.
// JSON-params cases (value starts with `{`) are filtered out in the parse loop.
static RE_COLON_BARE_LINE: Lazy<Regex> =
    Lazy::new(|| Regex::new(&format!(r"(?m)^({}):\s*([^\n]+)", COLON_TOOL_PREFIX_RE)).unwrap());

/// Map a tool name to its first (only positional) parameter key.
fn first_positional_param(tool: &str) -> &'static str {
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
fn extract_code_block(after: &str) -> Option<(String, usize)> {
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

fn parse_colon_shorthand(text: &str) -> Vec<RawToolMatch> {
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
fn parse_colon_params(s: &str) -> HashMap<String, String> {
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
static RE_XML_OPEN: Lazy<Regex> = Lazy::new(|| Regex::new(r"<(\w+)>").unwrap());

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

// ═══════════════════════════════════════════════════════════════════════
// Model-family parsers — prioritized by model_family hint
// ═══════════════════════════════════════════════════════════════════════

use crate::tool_parsing::types::ModelFamily;

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

static RE_DEEPSEEK_BLOCK: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?s)\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}begin\u{FF5C}\u{FF1E}(.*?)\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}end\u{FF5C}\u{FF1E}")
        .unwrap()
});

/// Parse a single DeepSeek tool call JSON object.
/// Handles both `arguments` as object and `arguments` as double-encoded string.
fn parse_deepseek_tool_json(
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

fn parse_deepseek(text: &str) -> Vec<RawToolMatch> {
    let mut results = Vec::new();

    for block_match in RE_DEEPSEEK_BLOCK.find_iter(text) {
        let inner = RE_DEEPSEEK_BLOCK
            .captures(block_match.as_str())
            .and_then(|c| c.get(1))
            .map(|m| m.as_str().trim())
            .unwrap_or("");

        // Try parsing as JSON array first
        if let Ok(serde_json::Value::Array(arr)) = serde_json::from_str::<serde_json::Value>(inner)
        {
            for item in &arr {
                if let Some((raw_name, params)) = parse_deepseek_tool_json(item) {
                    let name = unsanitize_tool_name(&raw_name);
                    results.push(RawToolMatch {
                        tool_name: name,
                        parameters: params,
                        format: "deepseek",
                        start: block_match.start(),
                        end: block_match.end(),
                    });
                }
            }
        } else {
            // Try line-by-line JSON objects
            for line in inner.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || !trimmed.starts_with('{') {
                    continue;
                }
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    if let Some((raw_name, params)) = parse_deepseek_tool_json(&val) {
                        let name = unsanitize_tool_name(&raw_name);
                        results.push(RawToolMatch {
                            tool_name: name,
                            parameters: params,
                            format: "deepseek",
                            start: block_match.start(),
                            end: block_match.end(),
                        });
                    }
                }
            }
        }
    }

    results
}

// ─── Llama ─────────────────────────────────────────────────────────
//
// Llama 3.1+ uses `<|python_tag|>` prefix followed by JSON:
//   <|python_tag|>{"name": "code_search", "arguments": {"pattern": "test"}}
//
// Also handles the function-call wrapper format:
//   <|python_tag|>{"type": "function", "function": {"name": "...", "arguments": {...}}}

static RE_LLAMA_BLOCK: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)<\|python_tag\|>\s*(.+?)(?:<\|eot_id\|>|\z)").unwrap());

fn parse_llama(text: &str) -> Vec<RawToolMatch> {
    let mut results = Vec::new();

    for cap in RE_LLAMA_BLOCK.captures_iter(text) {
        let full_match = cap.get(0).unwrap();
        let body = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");

        // Try parsing the body as JSON
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(body) {
            match &val {
                serde_json::Value::Array(arr) => {
                    for item in arr {
                        if let Some(tm) =
                            parse_llama_tool_json(item, full_match.start(), full_match.end())
                        {
                            results.push(tm);
                        }
                    }
                }
                serde_json::Value::Object(_) => {
                    if let Some(tm) =
                        parse_llama_tool_json(&val, full_match.start(), full_match.end())
                    {
                        results.push(tm);
                    }
                }
                _ => {}
            }
        } else {
            // Try line-by-line
            for line in body.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || !trimmed.starts_with('{') {
                    continue;
                }
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    if let Some(tm) =
                        parse_llama_tool_json(&val, full_match.start(), full_match.end())
                    {
                        results.push(tm);
                    }
                }
            }
        }
    }

    results
}

fn parse_llama_tool_json(
    val: &serde_json::Value,
    start: usize,
    end: usize,
) -> Option<RawToolMatch> {
    let obj = val.as_object()?;

    // Check for nested function wrapper: {"type": "function", "function": {"name": "...", "arguments": {...}}}
    if obj.get("type").and_then(|v| v.as_str()) == Some("function") {
        if let Some(func) = obj.get("function").and_then(|v| v.as_object()) {
            let name = func.get("name").and_then(|v| v.as_str())?.to_string();
            let params = extract_arguments_field(func);
            return Some(RawToolMatch {
                tool_name: unsanitize_tool_name(&name),
                parameters: params,
                format: "llama",
                start,
                end,
            });
        }
    }

    // Direct format: {"name": "...", "arguments": {...}}
    let name = obj.get("name").and_then(|v| v.as_str())?.to_string();
    let params = extract_arguments_field(obj);
    Some(RawToolMatch {
        tool_name: unsanitize_tool_name(&name),
        parameters: params,
        format: "llama",
        start,
        end,
    })
}

/// Extract `arguments` (or `parameters`) from a tool call JSON object.
/// Handles both object and double-encoded string forms.
fn extract_arguments_field(
    obj: &serde_json::Map<String, serde_json::Value>,
) -> HashMap<String, String> {
    match obj.get("arguments").or_else(|| obj.get("parameters")) {
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
        Some(serde_json::Value::String(s)) => parse_json_params(s),
        _ => HashMap::new(),
    }
}

// ─── Mistral ───────────────────────────────────────────────────────
//
// Mistral/Mixtral uses `[TOOL_CALLS]` prefix followed by JSON array:
//   [TOOL_CALLS] [{"name": "code_search", "arguments": {"pattern": "test"}}]

static RE_MISTRAL_BLOCK: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)\[TOOL_CALLS\]\s*(\[.+?\])").unwrap());

fn parse_mistral(text: &str) -> Vec<RawToolMatch> {
    let mut results = Vec::new();

    for cap in RE_MISTRAL_BLOCK.captures_iter(text) {
        let full_match = cap.get(0).unwrap();
        let arr_str = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");

        if let Ok(serde_json::Value::Array(arr)) =
            serde_json::from_str::<serde_json::Value>(arr_str)
        {
            for item in &arr {
                if let Some(obj) = item.as_object() {
                    if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
                        let params = extract_arguments_field(obj);
                        results.push(RawToolMatch {
                            tool_name: unsanitize_tool_name(name),
                            parameters: params,
                            format: "mistral",
                            start: full_match.start(),
                            end: full_match.end(),
                        });
                    }
                }
            }
        }
    }

    results
}

// ─── Hermes ────────────────────────────────────────────────────────
//
// Hermes-tuned (Nous Research) uses `<tool_call>` tags with JSON body:
//   <tool_call>
//   {"name": "code_search", "arguments": {"pattern": "test"}}
//   </tool_call>

static RE_HERMES_BLOCK: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)<tool_call>\s*(.*?)\s*</tool_call>").unwrap());

fn parse_hermes(text: &str) -> Vec<RawToolMatch> {
    let mut results = Vec::new();

    for cap in RE_HERMES_BLOCK.captures_iter(text) {
        let full_match = cap.get(0).unwrap();
        let body = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");

        if let Ok(val) = serde_json::from_str::<serde_json::Value>(body) {
            if let Some(obj) = val.as_object() {
                if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
                    let params = extract_arguments_field(obj);
                    results.push(RawToolMatch {
                        tool_name: unsanitize_tool_name(name),
                        parameters: params,
                        format: "hermes",
                        start: full_match.start(),
                        end: full_match.end(),
                    });
                }
            }
        }
    }

    results
}

// ─── Qwen ──────────────────────────────────────────────────────────
//
// Qwen/Qwen3-Coder uses same `<tool_call>` tags as Hermes, but with a
// key quirk: `arguments` is sometimes a double-encoded JSON STRING:
//   <tool_call>
//   {"name": "code_search", "arguments": "{\"pattern\": \"test\"}"}
//   </tool_call>

fn parse_qwen(text: &str) -> Vec<RawToolMatch> {
    // Same regex as Hermes — the difference is in argument handling
    let mut results = Vec::new();

    for cap in RE_HERMES_BLOCK.captures_iter(text) {
        let full_match = cap.get(0).unwrap();
        let body = cap.get(1).map(|m| m.as_str().trim()).unwrap_or("");

        if let Ok(val) = serde_json::from_str::<serde_json::Value>(body) {
            if let Some(obj) = val.as_object() {
                if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
                    // Qwen-specific: handle double-encoded arguments
                    let params = extract_arguments_field(obj);
                    results.push(RawToolMatch {
                        tool_name: unsanitize_tool_name(name),
                        parameters: params,
                        format: "qwen",
                        start: full_match.start(),
                        end: full_match.end(),
                    });
                }
            }
        }
    }

    results
}

// ─── Truncation Recovery ───────────────────────────────────────────
//
// When LLM responses are truncated mid-output (max tokens hit), attempt
// to recover partial tool calls by closing unclosed tags/braces.

/// Attempt to recover tool calls from truncated text.
/// Tries to close unclosed XML tags and JSON braces before parsing.
pub fn parse_with_truncation_recovery(text: &str, family: ModelFamily) -> Vec<RawToolMatch> {
    // Try normal parsing first
    let results = parse_with_model_family(text, family);
    if !results.is_empty() {
        return results;
    }

    // Check for truncated tool_call block (Hermes/Qwen)
    if text.contains("<tool_call>") && !text.contains("</tool_call>") {
        let recovered = format!("{}\n</tool_call>", text);
        let results = parse_with_model_family(&recovered, family);
        if !results.is_empty() {
            return results;
        }
    }

    // Check for truncated tool_use block (Anthropic)
    if text.contains("<tool_use>") && !text.contains("</tool_use>") {
        let mut recovered = text.to_string();
        // Close open parameter tags
        if recovered.contains("<parameters>") && !recovered.contains("</parameters>") {
            recovered.push_str("</parameters>");
        }
        recovered.push_str("</tool_use>");
        let results = parse_with_model_family(&recovered, family);
        if !results.is_empty() {
            return results;
        }
    }

    // Check for truncated DeepSeek block
    let deepseek_begin = "\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}begin\u{FF5C}\u{FF1E}";
    let deepseek_end = "\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}end\u{FF5C}\u{FF1E}";
    if text.contains(deepseek_begin) && !text.contains(deepseek_end) {
        // Try to close any open JSON and add the end marker
        let mut recovered = text.to_string();
        let trimmed = recovered.trim_end();
        // Count unclosed braces
        let open = trimmed.chars().filter(|c| *c == '{').count();
        let close = trimmed.chars().filter(|c| *c == '}').count();
        for _ in 0..(open.saturating_sub(close)) {
            recovered.push('}');
        }
        recovered.push_str(&format!("\n{}", deepseek_end));
        let results = parse_with_model_family(&recovered, family);
        if !results.is_empty() {
            return results;
        }
    }

    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Anthropic XML ──────────────────────────────────────────

    #[test]
    fn anthropic_basic() {
        let text = r#"I'll search for that.
<tool_use>
  <tool_name>code/search</tool_name>
  <parameters>
    <pattern>memory clustering</pattern>
    <path>./src</path>
  </parameters>
</tool_use>
Let me check the results."#;

        let matches = parse_anthropic(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/search");
        assert_eq!(
            matches[0].parameters.get("pattern").unwrap(),
            "memory clustering"
        );
        assert_eq!(matches[0].parameters.get("path").unwrap(), "./src");
        assert_eq!(matches[0].format, "anthropic-style");
    }

    #[test]
    fn anthropic_multiple() {
        let text = r#"<tool_use><tool_name>code/read</tool_name><parameters><filePath>main.ts</filePath></parameters></tool_use>
Then:
<tool_use><tool_name>code/write</tool_name><parameters><filePath>main.ts</filePath><content>hello</content></parameters></tool_use>"#;

        let matches = parse_anthropic(text);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].tool_name, "code/read");
        assert_eq!(matches[1].tool_name, "code/write");
    }

    #[test]
    fn anthropic_no_params() {
        let text = "<tool_use><tool_name>collaboration/decision/vote</tool_name><parameters><proposalId>abc-123</proposalId><rankedChoices>[\"opt1\",\"opt2\"]</rankedChoices></parameters></tool_use>";
        let matches = parse_anthropic(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "collaboration/decision/vote");
        assert_eq!(matches[0].parameters.get("proposalId").unwrap(), "abc-123");
    }

    // ─── Function-style ─────────────────────────────────────────

    #[test]
    fn function_style_json() {
        let text = r#"<function=adapter_search> {"query": "embedding module"} </function>"#;
        let matches = parse_function_style(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "adapter_search");
        assert_eq!(
            matches[0].parameters.get("query").unwrap(),
            "embedding module"
        );
        assert_eq!(matches[0].format, "function-style");
    }

    #[test]
    fn function_style_no_spaces() {
        let text = r#"<function=code/search>{"query": "memory clustering"}</function>"#;
        let matches = parse_function_style(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/search");
    }

    #[test]
    fn function_style_non_string_value() {
        let text = r#"<function=data/list>{"collection": "users", "limit": 10}</function>"#;
        let matches = parse_function_style(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].parameters.get("limit").unwrap(), "10");
    }

    #[test]
    fn function_style_groq_variant_no_xml_wrapper() {
        // Groq outputs function calls without < prefix and without </function> closing
        let text = r#"function=code_shell_execute>{"cmd": "ping google.com", "wait": true}"#;
        let matches = parse_function_style(text);
        assert_eq!(
            matches.len(),
            1,
            "Should match Groq's unwrapped function format"
        );
        assert_eq!(matches[0].tool_name, "code_shell_execute");
        assert_eq!(matches[0].parameters.get("cmd").unwrap(), "ping google.com");
        assert_eq!(matches[0].parameters.get("wait").unwrap(), "true");
    }

    #[test]
    fn function_style_groq_tree_variant() {
        let text = r#"function=code_tree>{"includeHidden": true, "maxDepth": 10, "path": "/shared/repository"}"#;
        let matches = parse_function_style(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code_tree");
        assert_eq!(
            matches[0].parameters.get("path").unwrap(),
            "/shared/repository"
        );
    }

    #[test]
    fn function_style_groq_git_variant() {
        let text =
            r#"function=code_git>{"operation": "log", "count": 100, "cwd": "/shared/repository"}"#;
        let matches = parse_function_style(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code_git");
        assert_eq!(matches[0].parameters.get("operation").unwrap(), "log");
    }

    // ─── Bare JSON ──────────────────────────────────────────────

    #[test]
    fn bare_basic() {
        let text = r#"code/search {"query": "memory clustering", "path": "./src/"}"#;
        let matches = parse_bare(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/search");
        assert_eq!(
            matches[0].parameters.get("query").unwrap(),
            "memory clustering"
        );
        assert_eq!(matches[0].format, "bare-tool-call");
    }

    #[test]
    fn bare_backtick_wrapped() {
        let text = r#"`code/tree` {"path": "."}"#;
        let matches = parse_bare(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/tree");
    }

    #[test]
    fn bare_no_match_for_unknown_prefix() {
        let text = r#"unknown/tool {"query": "test"}"#;
        let matches = parse_bare(text);
        assert_eq!(matches.len(), 0, "Should not match unknown prefix");
    }

    #[test]
    fn bare_sanitized_underscore_name() {
        // Groq outputs sanitized names (code_tree instead of code/tree) with optional </function>
        let text = r#"code_tree {"maxDepth": 1, "path": "."}</function>"#;
        let matches = parse_bare(text);
        assert_eq!(matches.len(), 1, "Should match underscore-based tool name");
        assert_eq!(
            matches[0].tool_name, "code/tree",
            "Should unsanitize back to slash-based name"
        );
        assert_eq!(matches[0].parameters.get("path").unwrap(), ".");
    }

    #[test]
    fn bare_sanitized_deep_name() {
        // Multi-level sanitized name: collaboration_chat_send → collaboration/chat/send
        let text = r#"collaboration_chat_send {"room": "general", "message": "hello"}"#;
        let matches = parse_bare(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "collaboration/chat/send");
        assert_eq!(matches[0].parameters.get("room").unwrap(), "general");
    }

    #[test]
    fn bare_sanitized_without_function_tag() {
        let text = r#"code_read {"filePath": "main.ts"}"#;
        let matches = parse_bare(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/read");
    }

    // ─── JSON Object ────────────────────────────────────────────

    #[test]
    fn json_object_basic() {
        let text = r#"I'll check that. {"name": "code_tree", "parameters": {"path": "."}}"#;
        let matches = parse_json_object(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/tree");
        assert_eq!(matches[0].parameters.get("path").unwrap(), ".");
        assert_eq!(matches[0].format, "json-object");
    }

    #[test]
    fn json_object_with_slash_name() {
        let text = r#"{"name": "collaboration/chat/send", "parameters": {"message": "hello", "room": "general"}}"#;
        let matches = parse_json_object(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "collaboration/chat/send");
        assert_eq!(matches[0].parameters.get("message").unwrap(), "hello");
    }

    #[test]
    fn json_object_with_type_field() {
        // Fireworks format with "type": "function" prefix
        let text =
            r#"{"type": "function", "name": "code_git", "parameters": {"operation": "status"}}"#;
        let matches = parse_json_object(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/git");
        assert_eq!(matches[0].parameters.get("operation").unwrap(), "status");
    }

    #[test]
    fn json_object_no_match_for_normal_json() {
        // Should not match arbitrary JSON objects
        let text = r#"{"status": "ok", "count": 5}"#;
        let matches = parse_json_object(text);
        assert_eq!(
            matches.len(),
            0,
            "Should not match JSON without name+parameters fields"
        );
    }

    // ─── Markdown backtick ──────────────────────────────────────

    #[test]
    fn markdown_basic() {
        let text = "`tool: collaboration/dm` `participants=helper`";
        let matches = parse_markdown(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "collaboration/dm");
        assert_eq!(matches[0].parameters.get("participants").unwrap(), "helper");
        assert_eq!(matches[0].format, "markdown-backtick");
    }

    #[test]
    fn markdown_multi_param() {
        let text = "`tool: code/read` `filepath=/path/to/file` `startLine=10`";
        let matches = parse_markdown(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].parameters.len(), 2);
        assert_eq!(
            matches[0].parameters.get("filepath").unwrap(),
            "/path/to/file"
        );
    }

    #[test]
    fn markdown_multiple_tools() {
        let text =
            "`tool: code/read` `filepath=a.ts`\n`tool: code/write` `filepath=b.ts` `content=hello`";
        let matches = parse_markdown(text);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].tool_name, "code/read");
        assert_eq!(matches[1].tool_name, "code/write");
    }

    // ─── Old-style XML ──────────────────────────────────────────

    #[test]
    fn old_style_basic() {
        let text = r#"<tool name="code/search"><pattern>hello</pattern><path>./src</path></tool>"#;
        let matches = parse_old_style(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/search");
        assert_eq!(matches[0].parameters.get("pattern").unwrap(), "hello");
        assert_eq!(matches[0].format, "old-style");
    }

    #[test]
    fn old_style_multiline() {
        let text = r#"<tool name="code/write">
  <filePath>test.ts</filePath>
  <content>function hello() { return 42; }</content>
</tool>"#;
        let matches = parse_old_style(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].parameters.get("filePath").unwrap(), "test.ts");
    }

    // ─── parse_all_formats ──────────────────────────────────────

    #[test]
    fn all_formats_mixed() {
        let text = r#"
<tool_use><tool_name>code/read</tool_name><parameters><filePath>a.ts</filePath></parameters></tool_use>
Then also:
<function=code/search>{"query": "test"}</function>
"#;
        let matches = parse_all_formats(text);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].format, "anthropic-style");
        assert_eq!(matches[1].format, "function-style");
    }

    #[test]
    fn no_tool_calls() {
        let text = "Just a normal response with no tool calls at all.";
        let matches = parse_all_formats(text);
        assert_eq!(matches.len(), 0);
    }

    // ─── Helpers ────────────────────────────────────────────────

    #[test]
    fn xml_params_extraction() {
        let block = "<name>Operator</name><age>30</age>";
        let params = extract_xml_params(block);
        assert_eq!(params.get("name").unwrap(), "Operator");
        assert_eq!(params.get("age").unwrap(), "30");
    }

    #[test]
    fn json_params_valid() {
        let json = r#"{"query": "test", "limit": 10, "flag": true}"#;
        let params = parse_json_params(json);
        assert_eq!(params.get("query").unwrap(), "test");
        assert_eq!(params.get("limit").unwrap(), "10");
        assert_eq!(params.get("flag").unwrap(), "true");
    }

    #[test]
    fn json_params_invalid_fallback() {
        let json = r#"{"query": "test", bad json"#;
        let params = parse_json_params(json);
        assert_eq!(params.get("query").unwrap(), "test");
    }

    #[test]
    fn json_params_empty() {
        assert!(parse_json_params("").is_empty());
    }

    // ─── Unsanitize ─────────────────────────────────────────────

    #[test]
    fn unsanitize_simple() {
        assert_eq!(unsanitize_tool_name("code_tree"), "code/tree");
        assert_eq!(unsanitize_tool_name("data_list"), "data/list");
    }

    #[test]
    fn unsanitize_deep() {
        assert_eq!(
            unsanitize_tool_name("collaboration_chat_send"),
            "collaboration/chat/send"
        );
        assert_eq!(
            unsanitize_tool_name("collaboration_decision_vote"),
            "collaboration/decision/vote"
        );
    }

    #[test]
    fn unsanitize_already_slash() {
        assert_eq!(unsanitize_tool_name("code/tree"), "code/tree");
        assert_eq!(
            unsanitize_tool_name("collaboration/chat/send"),
            "collaboration/chat/send"
        );
    }

    #[test]
    fn unsanitize_unknown_prefix() {
        // Unknown prefix stays as-is
        assert_eq!(unsanitize_tool_name("foobar_baz"), "foobar_baz");
    }

    // ─── Array-style ─────────────────────────────────────────────

    #[test]
    fn array_style_basic() {
        let text = r#"["code/search", {"pattern": "test"}]"#;
        let matches = parse_array_style(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/search");
        assert_eq!(matches[0].parameters.get("pattern").unwrap(), "test");
        assert_eq!(matches[0].format, "array-style");
    }

    #[test]
    fn array_style_sanitized_name() {
        let text = r#"["collaboration_chat_send", {"room": "general", "message": "hello"}]"#;
        let matches = parse_array_style(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "collaboration/chat/send");
        assert_eq!(matches[0].parameters.get("room").unwrap(), "general");
    }

    #[test]
    fn array_style_no_match_unknown_prefix() {
        let text = r#"["unknown/tool", {"param": "value"}]"#;
        let matches = parse_array_style(text);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn array_style_no_match_non_tool_array() {
        let text = r#"["hello", "world"]"#;
        let matches = parse_array_style(text);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn array_style_with_surrounding_text() {
        let text = r#"I'll search for that. ["code/search", {"pattern": "genome"}] Let me check."#;
        let matches = parse_array_style(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/search");
    }

    // ─── Curly-shorthand ────────────────────────────────────────

    #[test]
    fn curly_shorthand_basic() {
        let text = r#"{"code_tree": {"path": "."}}"#;
        let matches = parse_curly_shorthand(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/tree");
        assert_eq!(matches[0].parameters.get("path").unwrap(), ".");
        assert_eq!(matches[0].format, "curly-shorthand");
    }

    #[test]
    fn curly_shorthand_deep_name() {
        let text = r#"{"collaboration_wall_write": {"append": true, "content": "hello"}}"#;
        let matches = parse_curly_shorthand(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "collaboration/wall/write");
        assert_eq!(matches[0].parameters.get("content").unwrap(), "hello");
        assert_eq!(matches[0].parameters.get("append").unwrap(), "true");
    }

    #[test]
    fn curly_shorthand_slash_name() {
        let text = r#"{"code/tree": {"path": "./src"}}"#;
        let matches = parse_curly_shorthand(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/tree");
    }

    #[test]
    fn curly_shorthand_no_match_multi_key() {
        // Multi-key objects are NOT tool calls
        let text = r#"{"name": "test-user", "age": 30}"#;
        let matches = parse_curly_shorthand(text);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn curly_shorthand_no_match_unknown_prefix() {
        let text = r#"{"foobar_baz": {"param": "val"}}"#;
        let matches = parse_curly_shorthand(text);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    fn curly_shorthand_no_match_non_object_value() {
        let text = r#"{"code_tree": "not an object"}"#;
        let matches = parse_curly_shorthand(text);
        assert_eq!(matches.len(), 0);
    }

    // ─── Full parse_all_formats with sanitized names ────────────

    #[test]
    fn all_formats_catches_groq_output() {
        // Real Groq Llama output: sanitized tool name + JSON + stray </function>
        let text = r#"code_tree {"maxDepth": 1, "path": "."}</function>"#;
        let matches = parse_all_formats(text);
        assert!(
            matches.len() >= 1,
            "Should catch Groq's sanitized tool call"
        );
        assert_eq!(matches[0].tool_name, "code/tree");
    }

    #[test]
    fn all_formats_catches_fireworks_json_object() {
        let text = r#"Let me check. {"name": "code_tree", "parameters": {"path": "."}}"#;
        let matches = parse_all_formats(text);
        assert!(
            matches.len() >= 1,
            "Should catch Fireworks JSON object tool call"
        );
        // Find the json-object match specifically
        let json_match = matches.iter().find(|m| m.format == "json-object").unwrap();
        assert_eq!(json_match.tool_name, "code/tree");
    }

    #[test]
    fn all_formats_catches_array_style() {
        let text = r#"Let me search. ["code/search", {"pattern": "genome"}]"#;
        let matches = parse_all_formats(text);
        let array_match = matches.iter().find(|m| m.format == "array-style");
        assert!(array_match.is_some(), "Should catch array-style tool call");
        assert_eq!(array_match.unwrap().tool_name, "code/search");
    }

    #[test]
    fn all_formats_catches_curly_shorthand() {
        let text = r#"{"collaboration_wall_write": {"content": "hello", "append": true}}"#;
        let matches = parse_all_formats(text);
        let curly_match = matches.iter().find(|m| m.format == "curly-shorthand");
        assert!(
            curly_match.is_some(),
            "Should catch curly-shorthand tool call"
        );
        assert_eq!(curly_match.unwrap().tool_name, "collaboration/wall/write");
    }

    // ─── Colon shorthand (QAT local model format) ──────────────────

    #[test]
    fn colon_shorthand_simple_read() {
        // code/read: {filePath: "app.py"}
        let text = "code/read: {filePath: \"app.py\"}";
        let matches = parse_colon_shorthand(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/read");
        assert_eq!(matches[0].parameters.get("filePath").unwrap(), "app.py");
        assert_eq!(matches[0].format, "colon-shorthand");
    }

    #[test]
    fn colon_shorthand_shell_command() {
        // code/shell/execute: {command: "ls -la"}
        let text = "code/shell/execute: {command: \"ls -la\"}";
        let matches = parse_colon_shorthand(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/shell/execute");
        assert_eq!(matches[0].parameters.get("command").unwrap(), "ls -la");
    }

    #[test]
    fn colon_shorthand_tree() {
        let text = "code/tree: {path: \".\"}";
        let matches = parse_colon_shorthand(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/tree");
        assert_eq!(matches[0].parameters.get("path").unwrap(), ".");
    }

    #[test]
    fn colon_shorthand_write_with_codeblock() {
        // Exact format produced by 14B QAT model:
        // code/write: {filePath: "app.py",
        // ```python
        // from flask import Flask
        // ...
        // ```
        let text = "code/write: {filePath: \"app.py\",\n```python\nfrom flask import Flask\napp = Flask(__name__)\n```";
        let matches = parse_colon_shorthand(text);
        assert_eq!(matches.len(), 1, "Should parse code/write with code block");
        assert_eq!(matches[0].tool_name, "code/write");
        assert_eq!(matches[0].parameters.get("filePath").unwrap(), "app.py");
        let content = matches[0].parameters.get("content").unwrap();
        assert!(
            content.contains("from flask import Flask"),
            "content should include Flask code"
        );
    }

    #[test]
    fn colon_shorthand_write_malformed_filename() {
        // Model sometimes produces {filePath: "app.py, without closing quote
        // The parameter extractor should still grab the filename up to comma/newline.
        let text = "code/write: {filePath: \"app.py,\n```python\nprint('hello')\n```";
        let matches = parse_colon_shorthand(text);
        // Should still parse content from code block
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/write");
        let content = matches[0].parameters.get("content").unwrap();
        assert!(content.contains("print("));
    }

    #[test]
    fn colon_shorthand_all_formats_integration() {
        let text = "code/read: {filePath: \"main.ts\"}";
        let matches = parse_all_formats(text);
        let colon_match = matches.iter().find(|m| m.format == "colon-shorthand");
        assert!(
            colon_match.is_some(),
            "parse_all_formats should include colon-shorthand"
        );
        assert_eq!(colon_match.unwrap().tool_name, "code/read");
    }

    #[test]
    fn colon_shorthand_no_match_unknown_prefix() {
        let text = "unknown/tool: {param: \"value\"}";
        let matches = parse_colon_shorthand(text);
        assert_eq!(matches.len(), 0);
    }

    // ─── Bare-value variant (degraded model output) ─────────────────

    #[test]
    fn colon_bare_read() {
        let text = "code/read: hello.py";
        let matches = parse_colon_shorthand(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/read");
        assert_eq!(matches[0].parameters.get("filePath").unwrap(), "hello.py");
        assert_eq!(matches[0].format, "colon-shorthand-bare");
    }

    #[test]
    fn colon_bare_shell() {
        let text = "code/shell/execute: ls -la";
        let matches = parse_colon_shorthand(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/shell/execute");
        assert_eq!(matches[0].parameters.get("command").unwrap(), "ls -la");
    }

    #[test]
    fn colon_bare_write_with_codeblock() {
        // Actual observed model output: bare filename + code block
        let text = "code/write: hello.py\n```python\nprint('Hello World')\n```";
        let matches = parse_colon_shorthand(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/write");
        assert_eq!(matches[0].parameters.get("filePath").unwrap(), "hello.py");
        assert_eq!(
            matches[0].parameters.get("content").unwrap(),
            "print('Hello World')"
        );
    }

    #[test]
    fn colon_bare_write_incomplete_codeblock() {
        // Truncated response: code block not closed
        let text = "code/write: hello.py\n```python\nprint('Hello World')\n# more code";
        let matches = parse_colon_shorthand(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/write");
        assert_eq!(matches[0].parameters.get("filePath").unwrap(), "hello.py");
        let content = matches[0].parameters.get("content").unwrap();
        assert!(content.contains("print("), "Should extract partial content");
    }

    #[test]
    fn colon_bare_write_with_garbage_then_codeblock() {
        // Model generates garbage between bare-filename line and code block.
        // This is the actual degraded output observed from the compacted 14B model.
        let text = "assistant\ncode/write: hello.py\nfrom typing import Path\n\n```python\nprint('Hello World')\n```";
        let matches = parse_colon_shorthand(text);
        let write_match = matches.iter().find(|m| m.tool_name == "code/write");
        assert!(write_match.is_some(), "Should find code/write");
        assert_eq!(
            write_match.unwrap().parameters.get("filePath").unwrap(),
            "hello.py"
        );
        assert!(write_match.unwrap().parameters.contains_key("content"));
    }

    #[test]
    fn colon_bare_no_double_match_with_json_variant() {
        // JSON-params variant should take precedence; bare variant should NOT also fire.
        let text = "code/read: {filePath: \"main.ts\"}";
        let matches = parse_colon_shorthand(text);
        // Should only get one match (the JSON-params one)
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].format, "colon-shorthand");
    }

    #[test]
    fn colon_bare_tree() {
        let text = "code/tree: ./src";
        let matches = parse_colon_shorthand(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/tree");
        assert_eq!(matches[0].parameters.get("path").unwrap(), "./src");
    }

    #[test]
    fn colon_params_unquoted_key() {
        let params = parse_colon_params(r#"filePath: "app.py""#);
        assert_eq!(params.get("filePath").unwrap(), "app.py");
    }

    #[test]
    fn colon_params_quoted_key() {
        let params = parse_colon_params(r#""filePath": "app.py""#);
        assert_eq!(params.get("filePath").unwrap(), "app.py");
    }

    #[test]
    fn colon_params_multiple() {
        let params = parse_colon_params(r#"query: "flask", path: "./src""#);
        assert_eq!(params.get("query").unwrap(), "flask");
        assert_eq!(params.get("path").unwrap(), "./src");
    }

    // ═══════════════════════════════════════════════════════════════
    // Model-family parsers
    // ═══════════════════════════════════════════════════════════════

    // ─── DeepSeek ──────────────────────────────────────────────────

    #[test]
    fn deepseek_single_tool() {
        let text = "Let me search for that.\n\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}begin\u{FF5C}\u{FF1E}\n{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"memory clustering\"}}\n\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}end\u{FF5C}\u{FF1E}\nDone.";
        let matches = parse_deepseek(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/search");
        assert_eq!(
            matches[0].parameters.get("pattern").unwrap(),
            "memory clustering"
        );
        assert_eq!(matches[0].format, "deepseek");
    }

    #[test]
    fn deepseek_multiple_tools() {
        let text = "\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}begin\u{FF5C}\u{FF1E}\n{\"name\": \"code_read\", \"arguments\": {\"filePath\": \"main.ts\"}}\n{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"TODO\"}}\n\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}end\u{FF5C}\u{FF1E}";
        let matches = parse_deepseek(text);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].tool_name, "code/read");
        assert_eq!(matches[1].tool_name, "code/search");
    }

    #[test]
    fn deepseek_json_array() {
        let text = "\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}begin\u{FF5C}\u{FF1E}\n[{\"name\": \"code_tree\", \"arguments\": {\"path\": \"./src\"}}]\n\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}end\u{FF5C}\u{FF1E}";
        let matches = parse_deepseek(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/tree");
        assert_eq!(matches[0].parameters.get("path").unwrap(), "./src");
    }

    #[test]
    fn deepseek_double_encoded_arguments() {
        let text = "\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}begin\u{FF5C}\u{FF1E}\n{\"name\": \"code_search\", \"arguments\": \"{\\\"pattern\\\": \\\"test\\\"}\"}\n\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}end\u{FF5C}\u{FF1E}";
        let matches = parse_deepseek(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].parameters.get("pattern").unwrap(), "test");
    }

    #[test]
    fn deepseek_mixed_with_text() {
        let text = "I'll look at the code.\n\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}begin\u{FF5C}\u{FF1E}\n{\"name\": \"code_read\", \"arguments\": {\"filePath\": \"test.ts\"}}\n\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}end\u{FF5C}\u{FF1E}\nHere are the results.";
        let matches = parse_deepseek(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/read");
    }

    #[test]
    fn deepseek_no_match_without_markers() {
        let text = "{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"test\"}}";
        let matches = parse_deepseek(text);
        assert_eq!(matches.len(), 0);
    }

    // ─── Llama ──────────────────────────────────────────────────────

    #[test]
    fn llama_basic() {
        let text =
            "<|python_tag|>{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"test\"}}";
        let matches = parse_llama(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/search");
        assert_eq!(matches[0].parameters.get("pattern").unwrap(), "test");
        assert_eq!(matches[0].format, "llama");
    }

    #[test]
    fn llama_function_wrapper() {
        let text = "<|python_tag|>{\"type\": \"function\", \"function\": {\"name\": \"code_read\", \"arguments\": {\"filePath\": \"app.py\"}}}";
        let matches = parse_llama(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/read");
        assert_eq!(matches[0].parameters.get("filePath").unwrap(), "app.py");
    }

    #[test]
    fn llama_with_eot_id() {
        let text =
            "<|python_tag|>{\"name\": \"code_tree\", \"arguments\": {\"path\": \".\"}}<|eot_id|>";
        let matches = parse_llama(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/tree");
    }

    #[test]
    fn llama_multiple_calls() {
        let text = "<|python_tag|>[{\"name\": \"code_read\", \"arguments\": {\"filePath\": \"a.ts\"}}, {\"name\": \"code_read\", \"arguments\": {\"filePath\": \"b.ts\"}}]<|eot_id|>";
        let matches = parse_llama(text);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].parameters.get("filePath").unwrap(), "a.ts");
        assert_eq!(matches[1].parameters.get("filePath").unwrap(), "b.ts");
    }

    #[test]
    fn llama_no_match_without_tag() {
        let text = "{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"test\"}}";
        let matches = parse_llama(text);
        assert_eq!(matches.len(), 0);
    }

    // ─── Mistral ────────────────────────────────────────────────────

    #[test]
    fn mistral_basic() {
        let text =
            "[TOOL_CALLS] [{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"test\"}}]";
        let matches = parse_mistral(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/search");
        assert_eq!(matches[0].parameters.get("pattern").unwrap(), "test");
        assert_eq!(matches[0].format, "mistral");
    }

    #[test]
    fn mistral_multiple() {
        let text = "[TOOL_CALLS] [{\"name\": \"code_read\", \"arguments\": {\"filePath\": \"a.ts\"}}, {\"name\": \"code_write\", \"arguments\": {\"filePath\": \"b.ts\", \"content\": \"hello\"}}]";
        let matches = parse_mistral(text);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].tool_name, "code/read");
        assert_eq!(matches[1].tool_name, "code/write");
    }

    #[test]
    fn mistral_no_match_without_prefix() {
        let text = "[{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"test\"}}]";
        let matches = parse_mistral(text);
        assert_eq!(matches.len(), 0);
    }

    // ─── Hermes ─────────────────────────────────────────────────────

    #[test]
    fn hermes_basic() {
        let text = "<tool_call>\n{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"test\"}}\n</tool_call>";
        let matches = parse_hermes(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/search");
        assert_eq!(matches[0].parameters.get("pattern").unwrap(), "test");
        assert_eq!(matches[0].format, "hermes");
    }

    #[test]
    fn hermes_multiple_blocks() {
        let text = "<tool_call>\n{\"name\": \"code_read\", \"arguments\": {\"filePath\": \"a.ts\"}}\n</tool_call>\nSome text.\n<tool_call>\n{\"name\": \"code_read\", \"arguments\": {\"filePath\": \"b.ts\"}}\n</tool_call>";
        let matches = parse_hermes(text);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].parameters.get("filePath").unwrap(), "a.ts");
        assert_eq!(matches[1].parameters.get("filePath").unwrap(), "b.ts");
    }

    #[test]
    fn hermes_with_surrounding_text() {
        let text = "Let me check.\n<tool_call>\n{\"name\": \"code_tree\", \"arguments\": {\"path\": \".\"}}\n</tool_call>\nDone.";
        let matches = parse_hermes(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/tree");
    }

    // ─── Qwen ───────────────────────────────────────────────────────

    #[test]
    fn qwen_basic() {
        let text = "<tool_call>\n{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"test\"}}\n</tool_call>";
        let matches = parse_qwen(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/search");
        assert_eq!(matches[0].format, "qwen");
    }

    #[test]
    fn qwen_double_encoded_arguments() {
        let text = "<tool_call>\n{\"name\": \"code_search\", \"arguments\": \"{\\\"pattern\\\": \\\"test\\\"}\"}\n</tool_call>";
        let matches = parse_qwen(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].parameters.get("pattern").unwrap(), "test");
    }

    #[test]
    fn qwen_multiple() {
        let text = "<tool_call>\n{\"name\": \"code_read\", \"arguments\": {\"filePath\": \"x.ts\"}}\n</tool_call>\n<tool_call>\n{\"name\": \"code_write\", \"arguments\": \"{\\\"filePath\\\": \\\"y.ts\\\", \\\"content\\\": \\\"hello\\\"}\"}\n</tool_call>";
        let matches = parse_qwen(text);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].parameters.get("filePath").unwrap(), "x.ts");
        assert_eq!(matches[1].parameters.get("filePath").unwrap(), "y.ts");
        assert_eq!(matches[1].parameters.get("content").unwrap(), "hello");
    }

    // ─── Model Family Priority ──────────────────────────────────────

    #[test]
    fn model_family_deepseek_prioritized() {
        let text = "\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}begin\u{FF5C}\u{FF1E}\n{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"test\"}}\n\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}end\u{FF5C}\u{FF1E}";
        let matches = parse_with_model_family(text, ModelFamily::DeepSeek);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].format, "deepseek");
    }

    #[test]
    fn model_family_falls_through_to_generic() {
        // DeepSeek family hint but Anthropic format — should fall through
        let text = "<tool_use><tool_name>code/read</tool_name><parameters><filePath>x.ts</filePath></parameters></tool_use>";
        let matches = parse_with_model_family(text, ModelFamily::DeepSeek);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].format, "anthropic-style");
    }

    #[test]
    fn model_family_generic_uses_all_parsers() {
        let text = "<tool_use><tool_name>code/search</tool_name><parameters><pattern>test</pattern></parameters></tool_use>";
        let matches = parse_with_model_family(text, ModelFamily::Generic);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].format, "anthropic-style");
    }

    // ─── Truncation Recovery ────────────────────────────────────────

    #[test]
    fn truncation_recovery_tool_call() {
        let text =
            "<tool_call>\n{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"test\"}}";
        // No closing </tool_call>
        let matches = parse_with_truncation_recovery(text, ModelFamily::Hermes);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/search");
    }

    #[test]
    fn truncation_recovery_tool_use() {
        let text =
            "<tool_use><tool_name>code/read</tool_name><parameters><filePath>test.ts</filePath>";
        // No closing </parameters> or </tool_use>
        let matches = parse_with_truncation_recovery(text, ModelFamily::Generic);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/read");
    }

    #[test]
    fn truncation_recovery_deepseek() {
        let text = format!(
            "\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}begin\u{FF5C}\u{FF1E}\n{{\"name\": \"code_search\", \"arguments\": {{\"pattern\": \"test\"}}"
        );
        let matches = parse_with_truncation_recovery(&text, ModelFamily::DeepSeek);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/search");
    }

    #[test]
    fn truncation_recovery_not_needed() {
        // Complete text — should parse normally
        let text = "<tool_call>\n{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"test\"}}\n</tool_call>";
        let matches = parse_with_truncation_recovery(text, ModelFamily::Hermes);
        assert_eq!(matches.len(), 1);
    }
}
