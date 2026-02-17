//! Tool name and parameter corrections.
//!
//! LLMs confuse similarly-named tools and guess wrong parameter names.
//! Static lookup tables fix common mistakes before execution.
//!
//! Also handles content cleaning for code/write: CDATA stripping + HTML entity decode.

use std::collections::HashMap;
use once_cell::sync::Lazy;
use regex::Regex;
use super::types::CorrectedToolCall;

/// Tool name corrections: LLMs confuse similarly-named tools.
static TOOL_CORRECTIONS: Lazy<HashMap<&str, &str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("workspace/tree", "code/tree");
    m
});

/// Parameter name corrections per command.
/// Maps { wrongName -> correctName } for each command prefix.
static PARAM_CORRECTIONS: Lazy<HashMap<&str, Vec<(&str, &str)>>> = Lazy::new(|| {
    let mut m: HashMap<&str, Vec<(&str, &str)>> = HashMap::new();

    m.insert("code/write", vec![
        ("path", "filePath"), ("file", "filePath"), ("file_path", "filePath"),
        ("filepath", "filePath"), ("filename", "filePath"), ("file_name", "filePath"),
        ("name", "filePath"), ("contents", "content"), ("text", "content"),
        ("body", "content"), ("data", "content"), ("code", "content"),
        ("html", "content"), ("source", "content"),
    ]);

    m.insert("code/read", vec![
        ("path", "filePath"), ("file", "filePath"), ("file_path", "filePath"),
        ("filepath", "filePath"), ("filename", "filePath"), ("name", "filePath"),
        ("start", "startLine"), ("end", "endLine"),
        ("from", "startLine"), ("to", "endLine"),
    ]);

    m.insert("code/edit", vec![
        ("path", "filePath"), ("file", "filePath"), ("file_path", "filePath"),
        ("filepath", "filePath"), ("filename", "filePath"), ("name", "filePath"),
        ("mode", "editMode"), ("type", "editMode"),
    ]);

    m.insert("code/search", vec![
        ("query", "pattern"), ("search", "pattern"),
        ("term", "pattern"), ("regex", "pattern"),
        ("glob", "fileGlob"), ("filter", "fileGlob"),
    ]);

    m.insert("code/tree", vec![
        ("directory", "path"), ("dir", "path"), ("folder", "path"),
        ("depth", "maxDepth"),
    ]);

    m.insert("code/git", vec![
        ("subcommand", "operation"), ("command", "operation"),
        ("action", "operation"), ("op", "operation"),
        ("msg", "message"), ("files", "paths"),
    ]);

    m
});

/// Named HTML entities.
static NAMED_ENTITIES: Lazy<HashMap<&str, &str>> = Lazy::new(|| {
    let mut m = HashMap::new();
    m.insert("lt", "<");
    m.insert("gt", ">");
    m.insert("amp", "&");
    m.insert("quot", "\"");
    m.insert("apos", "'");
    m.insert("nbsp", " ");
    m
});

/// HTML entity regex.
static RE_ENTITY: Lazy<Regex> = Lazy::new(||
    Regex::new(r"&(#\d+|#x[\da-fA-F]+|[a-zA-Z]+);").unwrap()
);

/// Correct a tool call: name mapping, parameter mapping, content cleaning.
pub fn correct_tool_call(
    tool_name: &str,
    parameters: &HashMap<String, String>,
) -> CorrectedToolCall {
    let mut name = tool_name.to_string();
    let mut name_changed = false;
    let mut param_corrections = Vec::new();

    // Tool name correction
    if let Some(&corrected) = TOOL_CORRECTIONS.get(tool_name) {
        name = corrected.to_string();
        name_changed = true;
    }

    // Parameter correction
    let mut params = parameters.clone();
    if let Some(corrections) = PARAM_CORRECTIONS.get(name.as_str()) {
        for &(wrong, correct) in corrections {
            if params.contains_key(wrong) && !params.contains_key(correct) {
                let value = params.remove(wrong).unwrap();
                params.insert(correct.to_string(), value);
                param_corrections.push(format!("{} -> {}", wrong, correct));
            }
        }
    }

    // Content cleaning for code/write
    if name == "code/write" {
        if let Some(content) = params.get("content").cloned() {
            let cleaned = clean_content(&content);
            if cleaned != content {
                params.insert("content".to_string(), cleaned);
            }
        }
    }

    CorrectedToolCall {
        tool_name: name,
        parameters: params,
        name_changed,
        param_corrections,
    }
}

/// Clean content: strip CDATA wrappers, decode HTML entities.
fn clean_content(content: &str) -> String {
    let mut result = content.to_string();

    // Strip CDATA wrappers
    if result.starts_with("<![CDATA[") && result.ends_with("]]>") {
        result = result[9..result.len() - 3].to_string();
    }

    // Decode HTML entities
    result = decode_html_entities(&result);
    result
}

/// Decode HTML entities: &lt; &gt; &amp; &#123; &#x1F; etc.
fn decode_html_entities(text: &str) -> String {
    RE_ENTITY.replace_all(text, |caps: &regex::Captures| {
        let entity = &caps[1];
        if let Some(&replacement) = NAMED_ENTITIES.get(entity) {
            return replacement.to_string();
        }
        if let Some(hex) = entity.strip_prefix("#x") {
            if let Ok(code) = u32::from_str_radix(hex, 16) {
                if let Some(c) = char::from_u32(code) {
                    return c.to_string();
                }
            }
        }
        if let Some(dec) = entity.strip_prefix('#') {
            if let Ok(code) = dec.parse::<u32>() {
                if let Some(c) = char::from_u32(code) {
                    return c.to_string();
                }
            }
        }
        caps[0].to_string()
    }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── Tool name correction ───────────────────────────────────

    #[test]
    fn correct_workspace_tree() {
        let params = HashMap::new();
        let result = correct_tool_call("workspace/tree", &params);
        assert_eq!(result.tool_name, "code/tree");
        assert!(result.name_changed);
    }

    #[test]
    fn no_correction_needed() {
        let params = HashMap::new();
        let result = correct_tool_call("code/search", &params);
        assert_eq!(result.tool_name, "code/search");
        assert!(!result.name_changed);
    }

    // ─── Parameter correction ───────────────────────────────────

    #[test]
    fn correct_code_write_params() {
        let mut params = HashMap::new();
        params.insert("path".to_string(), "/test.ts".to_string());
        params.insert("text".to_string(), "hello world".to_string());
        let result = correct_tool_call("code/write", &params);
        assert_eq!(result.parameters.get("filePath").unwrap(), "/test.ts");
        assert_eq!(result.parameters.get("content").unwrap(), "hello world");
        assert!(result.param_corrections.contains(&"path -> filePath".to_string()));
        assert!(result.param_corrections.contains(&"text -> content".to_string()));
    }

    #[test]
    fn correct_code_read_params() {
        let mut params = HashMap::new();
        params.insert("file".to_string(), "main.ts".to_string());
        params.insert("start".to_string(), "10".to_string());
        params.insert("end".to_string(), "20".to_string());
        let result = correct_tool_call("code/read", &params);
        assert_eq!(result.parameters.get("filePath").unwrap(), "main.ts");
        assert_eq!(result.parameters.get("startLine").unwrap(), "10");
        assert_eq!(result.parameters.get("endLine").unwrap(), "20");
    }

    #[test]
    fn no_overwrite_existing_param() {
        let mut params = HashMap::new();
        params.insert("path".to_string(), "wrong.ts".to_string());
        params.insert("filePath".to_string(), "correct.ts".to_string());
        let result = correct_tool_call("code/write", &params);
        // Should NOT overwrite existing filePath
        assert_eq!(result.parameters.get("filePath").unwrap(), "correct.ts");
        assert!(result.param_corrections.is_empty());
    }

    #[test]
    fn correct_code_search_params() {
        let mut params = HashMap::new();
        params.insert("query".to_string(), "findMe".to_string());
        params.insert("glob".to_string(), "*.ts".to_string());
        let result = correct_tool_call("code/search", &params);
        assert_eq!(result.parameters.get("pattern").unwrap(), "findMe");
        assert_eq!(result.parameters.get("fileGlob").unwrap(), "*.ts");
    }

    #[test]
    fn correct_code_git_params() {
        let mut params = HashMap::new();
        params.insert("command".to_string(), "status".to_string());
        params.insert("msg".to_string(), "test commit".to_string());
        let result = correct_tool_call("code/git", &params);
        assert_eq!(result.parameters.get("operation").unwrap(), "status");
        assert_eq!(result.parameters.get("message").unwrap(), "test commit");
    }

    #[test]
    fn name_correction_then_param_correction() {
        // workspace/tree → code/tree, then directory → path
        let mut params = HashMap::new();
        params.insert("directory".to_string(), "./src".to_string());
        let result = correct_tool_call("workspace/tree", &params);
        assert_eq!(result.tool_name, "code/tree");
        assert!(result.name_changed);
        assert_eq!(result.parameters.get("path").unwrap(), "./src");
    }

    // ─── Content cleaning ───────────────────────────────────────

    #[test]
    fn clean_cdata_wrapper() {
        let mut params = HashMap::new();
        params.insert("filePath".to_string(), "test.ts".to_string());
        params.insert("content".to_string(), "<![CDATA[const x = 1;]]>".to_string());
        let result = correct_tool_call("code/write", &params);
        assert_eq!(result.parameters.get("content").unwrap(), "const x = 1;");
    }

    #[test]
    fn decode_html_entities_in_content() {
        let mut params = HashMap::new();
        params.insert("filePath".to_string(), "test.ts".to_string());
        params.insert("content".to_string(), "if (a &lt; b &amp;&amp; c &gt; d) { return &quot;ok&quot;; }".to_string());
        let result = correct_tool_call("code/write", &params);
        assert_eq!(
            result.parameters.get("content").unwrap(),
            r#"if (a < b && c > d) { return "ok"; }"#
        );
    }

    #[test]
    fn decode_numeric_entities() {
        assert_eq!(decode_html_entities("&#60;div&#62;"), "<div>");
        assert_eq!(decode_html_entities("&#x3C;div&#x3E;"), "<div>");
    }

    #[test]
    fn no_cleaning_for_non_write() {
        let mut params = HashMap::new();
        params.insert("pattern".to_string(), "a &lt; b".to_string());
        let result = correct_tool_call("code/search", &params);
        // Should NOT clean content for non-write commands
        assert_eq!(result.parameters.get("pattern").unwrap(), "a &lt; b");
    }
}
