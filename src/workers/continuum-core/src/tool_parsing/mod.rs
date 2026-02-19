//! Tool call parsing — 5 format adapters + correction + codec in Rust.
//!
//! Stateless CPU work that runs on every LLM response. Sub-microsecond parsing
//! replaces 784 lines of TypeScript (ToolFormatAdapter hierarchy).
//!
//! Formats supported:
//! 1. Anthropic XML: `<tool_use>...<tool_name>X</tool_name><parameters>...</parameters></tool_use>`
//! 2. Function-style: `<function=tool_name>{"param": "value"}</function>`
//! 3. Bare JSON: `tool/name {"param": "value"}`
//! 4. Markdown backtick: `` `tool: name` `param=value` ``
//! 5. Old-style XML: `<tool name="X"><param>value</param></tool>`

pub mod types;
pub mod parsers;
pub mod correction;
pub mod codec;

pub use types::*;
pub use codec::ToolNameCodec;

/// Parse tool calls from AI response text, apply corrections, strip tool blocks.
/// Single entry point combining all 5 format adapters + correction.
pub fn parse_and_correct(response_text: &str) -> ToolParseResult {
    let start = std::time::Instant::now();

    // Parse all formats
    let raw_matches = parsers::parse_all_formats(response_text);

    // Apply corrections and collect results
    let tool_calls: Vec<ParsedToolCall> = raw_matches.iter().map(|m| {
        let corrected = correction::correct_tool_call(&m.tool_name, &m.parameters);
        ParsedToolCall {
            tool_name: corrected.tool_name,
            parameters: corrected.parameters,
            format: m.format.to_string(),
            original_name: if corrected.name_changed { Some(m.tool_name.clone()) } else { None },
            param_corrections: corrected.param_corrections,
        }
    }).collect();

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
        assert_eq!(result.tool_calls[0].parameters.get("pattern").unwrap(), "memory clustering");
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
        assert_eq!(result.tool_calls[0].original_name.as_deref(), Some("workspace/tree"));
        // directory -> path (param correction for code/tree)
        assert_eq!(result.tool_calls[0].parameters.get("path").unwrap(), "./src");
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
        assert_eq!(result.tool_calls[0].parameters.get("content").unwrap(), "const x = 1 < 2;");
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
        // Should complete in microseconds
        assert!(result.parse_time_us < 10_000, "Parse should be sub-10ms, was {}us", result.parse_time_us);
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
        assert_eq!(result.tool_calls[1].parameters.get("pattern").unwrap(), "test");
    }
}
