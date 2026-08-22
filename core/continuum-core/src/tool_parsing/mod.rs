//! Tool call parsing — 10 generic + 5 model-family format adapters + correction + codec in Rust.
//!
//! Stateless CPU work that runs on every LLM response. Sub-microsecond parsing
//! replaces 784 lines of TypeScript (ToolFormatAdapter hierarchy).
//!
//! Generic formats:
//! 1. Anthropic XML: `<tool_use>...<tool_name>X</tool_name><parameters>...</parameters></tool_use>`
//! 2. Function-style: `<function=tool_name>{"param": "value"}</function>`
//! 3. Bare JSON: `tool/name {"param": "value"}`
//! 4. JSON Object: `{"name": "tool_name", "parameters": {"param": "value"}}`
//! 5. Array-style: `["tool/name", {"param": "value"}]`
//! 6. Curly-shorthand: `{tool_name: {"param": "value"}}`
//! 7. Markdown backtick: `` `tool: name` `param=value` ``
//! 8. Old-style XML: `<tool name="X"><param>value</param></tool>`
//! 9. Colon shorthand variants
//!
//! Model-family formats (prioritized when model_family hint is provided):
//! - DeepSeek: Unicode fullwidth delimiters `＜｜tool▁calls▁begin｜＞`
//! - Llama: `<|python_tag|>` prefix + JSON
//! - Mistral: `[TOOL_CALLS]` prefix + JSON array
//! - Hermes: `<tool_call>` XML tags with JSON body
//! - Qwen: `<tool_call>` with double-encoded arguments

pub mod codec;
pub mod correction;
pub mod parsers;
pub mod types;

pub use codec::ToolNameCodec;
pub use types::*;

/// Parse tool calls from AI response text, apply corrections, strip tool blocks.
/// Single entry point combining all format adapters + correction.
///
/// When `model_family` is provided, the model-specific parser runs first.
/// Falls back to all 10 generic parsers if the model-specific parser finds nothing.
pub fn parse_and_correct(response_text: &str) -> ToolParseResult {
    parse_and_correct_with_family(response_text, None)
}

/// Parse tool calls with an optional model family hint for prioritized parsing.
pub fn parse_and_correct_with_family(
    response_text: &str,
    model_family: Option<&str>,
) -> ToolParseResult {
    let start = std::time::Instant::now();

    let family = model_family
        .map(parse_model_family)
        .unwrap_or(ModelFamily::Generic);

    // Parse with model-family priority, with truncation recovery
    let raw_matches = parsers::parse_with_truncation_recovery(response_text, family);

    // Apply corrections and collect results
    let tool_calls: Vec<ParsedToolCall> = raw_matches
        .iter()
        .map(|m| {
            let corrected = correction::correct_tool_call(&m.tool_name, &m.parameters);
            ParsedToolCall {
                tool_name: corrected.tool_name,
                parameters: corrected.parameters,
                format: m.format.to_string(),
                original_name: if corrected.name_changed {
                    Some(m.tool_name.clone())
                } else {
                    None
                },
                param_corrections: corrected.param_corrections,
            }
        })
        .collect();

    // Strip tool blocks from text
    let cleaned_text = strip_tool_blocks(response_text, &raw_matches);

    let elapsed = start.elapsed();
    ToolParseResult {
        tool_calls,
        cleaned_text,
        parse_time_us: elapsed.as_micros() as u64,
    }
}

/// Strip tool call blocks from response text, returning clean user-facing message.
fn strip_tool_blocks(text: &str, matches: &[parsers::RawToolMatch]) -> String {
    if matches.is_empty() {
        return text.to_string();
    }

    // Sort ranges descending by start position (remove from end to start)
    let mut ranges: Vec<(usize, usize)> = matches.iter().map(|m| (m.start, m.end)).collect();
    ranges.sort_by(|a, b| b.0.cmp(&a.0));

    let mut result = text.to_string();
    for (start, end) in ranges {
        if start <= result.len() && end <= result.len() {
            result = format!("{}{}", &result[..start], &result[end..]);
        }
    }
    result.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_and_correct_anthropic() {
        let text = r#"Let me search.
<tool_use>
  <tool_name>code/search</tool_name>
  <parameters>
    <query>memory clustering</query>
  </parameters>
</tool_use>
Done."#;

        let result = parse_and_correct(text);
        assert_eq!(result.tool_calls.len(), 1);
        // query -> pattern (param correction for code/search)
        assert_eq!(result.tool_calls[0].tool_name, "code/search");
        assert_eq!(
            result.tool_calls[0].parameters.get("pattern").unwrap(),
            "memory clustering"
        );
        assert!(!result.tool_calls[0].param_corrections.is_empty());
        assert_eq!(result.tool_calls[0].format, "anthropic-style");
        assert!(result.cleaned_text.contains("Let me search."));
        assert!(result.cleaned_text.contains("Done."));
        assert!(!result.cleaned_text.contains("tool_use"));
    }

    #[test]
    fn parse_and_correct_with_name_fix() {
        let text = r#"<tool_use><tool_name>workspace/tree</tool_name><parameters><directory>./src</directory></parameters></tool_use>"#;

        let result = parse_and_correct(text);
        assert_eq!(result.tool_calls.len(), 1);
        // workspace/tree -> code/tree (name correction)
        assert_eq!(result.tool_calls[0].tool_name, "code/tree");
        assert_eq!(
            result.tool_calls[0].original_name.as_deref(),
            Some("workspace/tree")
        );
        // directory -> path (param correction for code/tree)
        assert_eq!(
            result.tool_calls[0].parameters.get("path").unwrap(),
            "./src"
        );
    }

    #[test]
    fn parse_and_correct_code_write_content_cleaning() {
        let text = r#"<tool_use>
  <tool_name>code/write</tool_name>
  <parameters>
    <filePath>test.ts</filePath>
    <content><![CDATA[const x = 1 &lt; 2;]]></content>
  </parameters>
</tool_use>"#;

        let result = parse_and_correct(text);
        assert_eq!(result.tool_calls.len(), 1);
        // CDATA stripped + HTML entities decoded
        assert_eq!(
            result.tool_calls[0].parameters.get("content").unwrap(),
            "const x = 1 < 2;"
        );
    }

    #[test]
    fn strip_preserves_surrounding_text() {
        let text = "Hello\n<tool_use><tool_name>ping</tool_name><parameters></parameters></tool_use>\nWorld";
        let result = parse_and_correct(text);
        assert!(result.cleaned_text.starts_with("Hello"));
        assert!(result.cleaned_text.ends_with("World"));
    }

    #[test]
    fn no_tool_calls_returns_original() {
        let text = "Just a normal response.";
        let result = parse_and_correct(text);
        assert_eq!(result.tool_calls.len(), 0);
        assert_eq!(result.cleaned_text, text);
    }

    #[test]
    fn parse_time_is_measured() {
        let text = "<tool_use><tool_name>code/read</tool_name><parameters><filePath>x.ts</filePath></parameters></tool_use>";
        let result = parse_and_correct(text);
        // what this catches: the instrumentation going dead or garbage — the field
        // must carry a sane measurement. Deliberately NOT a perf bound in either
        // direction: the old `< 10_000us` form was a performance SLA on whatever
        // hardware runs the test and flaked on a contended CI runner 2026-08-22 (a
        // scheduler stall is not a parser regression), while `> 0` would flake on
        // fast machines (sub-µs parses round to 0). The 10s ceiling only catches an
        // uninitialized/garbage u64; perf regressions belong to a bench harness
        // with a baseline, not a unit test's stopwatch.
        assert!(
            result.parse_time_us < 10_000_000,
            "parse_time_us carries garbage, not a measurement: {}",
            result.parse_time_us
        );
    }

    #[test]
    fn multiple_formats_in_one_response() {
        let text = r#"First:
<tool_use><tool_name>code/read</tool_name><parameters><filePath>a.ts</filePath></parameters></tool_use>
Then:
<function=code/search>{"query": "test"}</function>
"#;
        let result = parse_and_correct(text);
        assert_eq!(result.tool_calls.len(), 2);
        assert_eq!(result.tool_calls[0].format, "anthropic-style");
        assert_eq!(result.tool_calls[1].format, "function-style");
        // query -> pattern for code/search
        assert_eq!(
            result.tool_calls[1].parameters.get("pattern").unwrap(),
            "test"
        );
    }

    #[test]
    fn parse_with_family_deepseek() {
        let text = "\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}begin\u{FF5C}\u{FF1E}\n{\"name\": \"code_search\", \"arguments\": {\"query\": \"test\"}}\n\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}end\u{FF5C}\u{FF1E}";
        let result = parse_and_correct_with_family(text, Some("deepseek"));
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].format, "deepseek");
        // query -> pattern (correction still applied)
        assert_eq!(
            result.tool_calls[0].parameters.get("pattern").unwrap(),
            "test"
        );
    }

    #[test]
    fn parse_with_family_llama() {
        let text =
            "<|python_tag|>{\"name\": \"code_read\", \"arguments\": {\"filePath\": \"test.ts\"}}";
        let result = parse_and_correct_with_family(text, Some("llama"));
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].format, "llama");
        assert_eq!(result.tool_calls[0].tool_name, "code/read");
    }

    #[test]
    fn parse_with_family_none_uses_generic() {
        let text = "<tool_use><tool_name>code/read</tool_name><parameters><filePath>x.ts</filePath></parameters></tool_use>";
        let result = parse_and_correct_with_family(text, None);
        assert_eq!(result.tool_calls.len(), 1);
        assert_eq!(result.tool_calls[0].format, "anthropic-style");
    }

    #[test]
    fn parse_with_family_corrections_applied() {
        // Hermes format with param correction
        let text = "<tool_call>\n{\"name\": \"code_search\", \"arguments\": {\"query\": \"memory\"}}\n</tool_call>";
        let result = parse_and_correct_with_family(text, Some("hermes"));
        assert_eq!(result.tool_calls.len(), 1);
        // query -> pattern correction
        assert!(result.tool_calls[0].parameters.contains_key("pattern"));
        assert!(!result.tool_calls[0].param_corrections.is_empty());
    }

    #[test]
    fn parse_with_family_cleans_text() {
        let text = "Hello\n\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}begin\u{FF5C}\u{FF1E}\n{\"name\": \"code_read\", \"arguments\": {\"filePath\": \"x.ts\"}}\n\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}end\u{FF5C}\u{FF1E}\nWorld";
        let result = parse_and_correct_with_family(text, Some("deepseek"));
        assert!(result.cleaned_text.contains("Hello"));
        assert!(result.cleaned_text.contains("World"));
        assert!(!result.cleaned_text.contains("tool\u{2581}calls"));
    }
}
