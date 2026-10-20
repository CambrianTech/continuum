use super::*;

pub(super) fn parse_deepseek(text: &str) -> Vec<RawToolMatch> {
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

pub(super) static RE_LLAMA_BLOCK: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)<\|python_tag\|>\s*(.+?)(?:<\|eot_id\|>|\z)").unwrap());

pub(super) fn parse_llama(text: &str) -> Vec<RawToolMatch> {
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

pub(super) fn parse_llama_tool_json(
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
pub(super) fn extract_arguments_field(
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

pub(super) static RE_MISTRAL_BLOCK: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)\[TOOL_CALLS\]\s*(\[.+?\])").unwrap());

pub(super) fn parse_mistral(text: &str) -> Vec<RawToolMatch> {
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

pub(super) static RE_HERMES_BLOCK: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)<tool_call>\s*(.*?)\s*</tool_call>").unwrap());

pub(super) fn parse_hermes(text: &str) -> Vec<RawToolMatch> {
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

pub(super) fn parse_qwen(text: &str) -> Vec<RawToolMatch> {
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
