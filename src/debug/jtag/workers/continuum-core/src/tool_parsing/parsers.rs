//! Format-specific tool call parsers.
//!
//! Six formats supported (matching TypeScript ToolFormatAdapter hierarchy):
//! 1. Anthropic XML: `<tool_use><tool_name>X</tool_name><parameters>...</parameters></tool_use>`
//! 2. Function-style: `<function=tool_name>{"param": "value"}</function>`
//! 3. Bare JSON: `tool/name {"param": "value"}` or `tool_name {"param": "value"}</function>`
//! 4. JSON Object: `{"name": "tool_name", "parameters": {"param": "value"}}`
//! 5. Markdown backtick: `` `tool: name` `param=value` ``
//! 6. Old-style XML: `<tool name="X"><param>value</param></tool>`
//!
//! Handles both canonical (slash) and sanitized (underscore) tool names.
//! Sanitized names from native tool protocol (code_tree → code/tree) are
//! automatically unsanitized back to canonical form.

use regex::Regex;
use std::collections::HashMap;
use once_cell::sync::Lazy;

/// Internal representation of a matched tool call with position info.
pub struct RawToolMatch {
    pub tool_name: String,
    pub parameters: HashMap<String, String>,
    pub format: &'static str,
    pub start: usize,
    pub end: usize,
}

/// Parse all tool calls from response text using all 6 format adapters.
/// Returns matches in order of adapter priority (Anthropic first).
pub fn parse_all_formats(text: &str) -> Vec<RawToolMatch> {
    let mut results = Vec::new();
    results.extend(parse_anthropic(text));
    results.extend(parse_function_style(text));
    results.extend(parse_bare(text));
    results.extend(parse_json_object(text));
    results.extend(parse_markdown(text));
    results.extend(parse_old_style(text));
    results
}

// ─── Anthropic XML ──────────────────────────────────────────────────

static RE_ANTHROPIC: Lazy<Regex> = Lazy::new(||
    Regex::new(r"(?s)<tool_use>(.*?)</tool_use>").unwrap()
);
static RE_TOOL_NAME: Lazy<Regex> = Lazy::new(||
    Regex::new(r"(?s)<tool_name>(.*?)</tool_name>").unwrap()
);
static RE_PARAMS_BLOCK: Lazy<Regex> = Lazy::new(||
    Regex::new(r"(?s)<parameters>(.*?)</parameters>").unwrap()
);

fn parse_anthropic(text: &str) -> Vec<RawToolMatch> {
    RE_ANTHROPIC.find_iter(text).filter_map(|m| {
        let block = m.as_str();
        let name = RE_TOOL_NAME.captures(block)?.get(1)?.as_str().trim().to_string();
        let params_block = RE_PARAMS_BLOCK.captures(block)
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
    }).collect()
}

// ─── Function-style ─────────────────────────────────────────────────

// Match both proper XML and Groq's variant format:
//   <function=name>{"param": "value"}</function>   — standard
//   function=name>{"param": "value"}               — Groq variant (no < prefix, no closing tag)
static RE_FUNCTION: Lazy<Regex> = Lazy::new(||
    Regex::new(r"(?si)<?function=([^>\s]+)>\s*(\{[\s\S]*?\})\s*(?:</function>)?").unwrap()
);

fn parse_function_style(text: &str) -> Vec<RawToolMatch> {
    RE_FUNCTION.captures_iter(text).filter_map(|cap| {
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
    }).collect()
}

// ─── Bare JSON ──────────────────────────────────────────────────────

// Slash-based prefixes (canonical tool names: code/tree, data/list, etc.)
const TOOL_PREFIXES_SLASH: &[&str] = &[
    "code/", "data/", "collaboration/", "ai/", "voice/", "search/",
    "workspace/", "file/", "interface/", "genome/", "adapter/",
    "persona/", "runtime/", "session/", "user/", "logs/", "media/",
];

// Underscore-based prefixes (sanitized names from native tool protocol:
// code_tree, collaboration_chat_send, etc.)
const TOOL_PREFIXES_UNDERSCORE: &[&str] = &[
    "code_", "data_", "collaboration_", "ai_", "voice_", "search_",
    "workspace_", "file_", "interface_", "genome_", "adapter_",
    "persona_", "runtime_", "session_", "user_", "logs_", "media_",
];

fn all_prefix_pattern() -> String {
    TOOL_PREFIXES_SLASH.iter()
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
    )).unwrap()
});

static RE_BARE_PARSE: Lazy<Regex> = Lazy::new(|| {
    let prefix_pat = all_prefix_pattern();
    Regex::new(&format!(
        r"(?s)`?((?:{})[a-zA-Z0-9/_-]+)`?\s*(\{{.+?\}})\s*(?:</function>)?",
        prefix_pat
    )).unwrap()
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
        "collaboration", "code", "data", "ai", "voice", "search",
        "workspace", "file", "interface", "genome", "adapter",
        "persona", "runtime", "session", "user", "logs", "media",
    ];
    for root in prefix_roots {
        if name.starts_with(root) && name.len() > root.len() && name.as_bytes()[root.len()] == b'_' {
            // Replace ALL underscores with slashes (tool segments use camelCase, not snake_case)
            return name.replace('_', "/");
        }
    }
    // No known prefix — return as-is
    name.to_string()
}

fn parse_bare(text: &str) -> Vec<RawToolMatch> {
    RE_BARE.find_iter(text).filter_map(|m| {
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
    }).collect()
}

// ─── JSON Object ───────────────────────────────────────────────────

// Matches tool calls in JSON object format (two variants):
//   {"name": "code_tree", "parameters": {"path": "."}}
//   {"type": "function", "name": "code_git", "parameters": {"operation": "status"}}
// Used by Fireworks and some OpenAI-compatible models
static RE_JSON_TOOL: Lazy<Regex> = Lazy::new(||
    Regex::new(r#"\{(?:\s*"type"\s*:\s*"[^"]*"\s*,)?\s*"name"\s*:\s*"([^"]+)"\s*,\s*"parameters"\s*:\s*(\{[^{}]*(?:\{[^{}]*\}[^{}]*)*\})\s*\}"#).unwrap()
);

fn parse_json_object(text: &str) -> Vec<RawToolMatch> {
    RE_JSON_TOOL.captures_iter(text).filter_map(|cap| {
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
    }).collect()
}

// ─── Markdown backtick ──────────────────────────────────────────────

static RE_MD_TOOL: Lazy<Regex> = Lazy::new(||
    Regex::new(r"(?i)`tool:\s*([^`]+)`").unwrap()
);
static RE_MD_PARAM: Lazy<Regex> = Lazy::new(||
    Regex::new(r"`([^`=]+)=([^`]*)`").unwrap()
);

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
    let name = RE_MD_TOOL.captures(text)?.get(1)?.as_str().trim().to_string();
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

static RE_OLD_STYLE: Lazy<Regex> = Lazy::new(||
    Regex::new(r#"(?s)<tool\s+name="([^"]+)">(.*?)</tool>"#).unwrap()
);

fn parse_old_style(text: &str) -> Vec<RawToolMatch> {
    RE_OLD_STYLE.captures_iter(text).filter_map(|cap| {
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
    }).collect()
}

// ─── Helpers ────────────────────────────────────────────────────────

/// Regex to find opening XML tags: `<tagName>`
static RE_XML_OPEN: Lazy<Regex> = Lazy::new(||
    Regex::new(r"<(\w+)>").unwrap()
);

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
        Ok(serde_json::Value::Object(map)) => {
            map.into_iter().map(|(k, v)| {
                let s = match &v {
                    serde_json::Value::String(s) => s.clone(),
                    _ => v.to_string(),
                };
                (k, s)
            }).collect()
        }
        _ => {
            // Fallback: extract "key": "value" pairs
            static RE_KV: Lazy<Regex> = Lazy::new(||
                Regex::new(r#""([^"]+)":\s*"([^"]*)""#).unwrap()
            );
            RE_KV.captures_iter(json_str).filter_map(|cap| {
                Some((cap.get(1)?.as_str().to_string(), cap.get(2)?.as_str().to_string()))
            }).collect()
        }
    }
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
        assert_eq!(matches[0].parameters.get("pattern").unwrap(), "memory clustering");
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
        assert_eq!(matches[0].parameters.get("query").unwrap(), "embedding module");
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
        assert_eq!(matches.len(), 1, "Should match Groq's unwrapped function format");
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
        assert_eq!(matches[0].parameters.get("path").unwrap(), "/shared/repository");
    }

    #[test]
    fn function_style_groq_git_variant() {
        let text = r#"function=code_git>{"operation": "log", "count": 100, "cwd": "/shared/repository"}"#;
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
        assert_eq!(matches[0].parameters.get("query").unwrap(), "memory clustering");
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
        assert_eq!(matches[0].tool_name, "code/tree", "Should unsanitize back to slash-based name");
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
        let text = r#"{"type": "function", "name": "code_git", "parameters": {"operation": "status"}}"#;
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
        assert_eq!(matches.len(), 0, "Should not match JSON without name+parameters fields");
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
        assert_eq!(matches[0].parameters.get("filepath").unwrap(), "/path/to/file");
    }

    #[test]
    fn markdown_multiple_tools() {
        let text = "`tool: code/read` `filepath=a.ts`\n`tool: code/write` `filepath=b.ts` `content=hello`";
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
        let block = "<name>Joel</name><age>30</age>";
        let params = extract_xml_params(block);
        assert_eq!(params.get("name").unwrap(), "Joel");
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
        assert_eq!(unsanitize_tool_name("collaboration_chat_send"), "collaboration/chat/send");
        assert_eq!(unsanitize_tool_name("collaboration_decision_vote"), "collaboration/decision/vote");
    }

    #[test]
    fn unsanitize_already_slash() {
        assert_eq!(unsanitize_tool_name("code/tree"), "code/tree");
        assert_eq!(unsanitize_tool_name("collaboration/chat/send"), "collaboration/chat/send");
    }

    #[test]
    fn unsanitize_unknown_prefix() {
        // Unknown prefix stays as-is
        assert_eq!(unsanitize_tool_name("foobar_baz"), "foobar_baz");
    }

    // ─── Full parse_all_formats with sanitized names ────────────

    #[test]
    fn all_formats_catches_groq_output() {
        // Real Groq Llama output: sanitized tool name + JSON + stray </function>
        let text = r#"code_tree {"maxDepth": 1, "path": "."}</function>"#;
        let matches = parse_all_formats(text);
        assert!(matches.len() >= 1, "Should catch Groq's sanitized tool call");
        assert_eq!(matches[0].tool_name, "code/tree");
    }

    #[test]
    fn all_formats_catches_fireworks_json_object() {
        let text = r#"Let me check. {"name": "code_tree", "parameters": {"path": "."}}"#;
        let matches = parse_all_formats(text);
        assert!(matches.len() >= 1, "Should catch Fireworks JSON object tool call");
        // Find the json-object match specifically
        let json_match = matches.iter().find(|m| m.format == "json-object").unwrap();
        assert_eq!(json_match.tool_name, "code/tree");
    }
}
