#![cfg(test)]

    use super::*;

    #[test]
    pub(super) fn anthropic_basic() {
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
    pub(super) fn anthropic_multiple() {
        let text = r#"<tool_use><tool_name>code/read</tool_name><parameters><filePath>main.ts</filePath></parameters></tool_use>
Then:
<tool_use><tool_name>code/write</tool_name><parameters><filePath>main.ts</filePath><content>hello</content></parameters></tool_use>"#;

        let matches = parse_anthropic(text);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].tool_name, "code/read");
        assert_eq!(matches[1].tool_name, "code/write");
    }

    #[test]
    pub(super) fn anthropic_no_params() {
        let text = "<tool_use><tool_name>collaboration/decision/vote</tool_name><parameters><proposalId>abc-123</proposalId><rankedChoices>[\"opt1\",\"opt2\"]</rankedChoices></parameters></tool_use>";
        let matches = parse_anthropic(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "collaboration/decision/vote");
        assert_eq!(matches[0].parameters.get("proposalId").unwrap(), "abc-123");
    }

    #[test]
    pub(super) fn function_style_json() {
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
    pub(super) fn function_style_no_spaces() {
        let text = r#"<function=code/search>{"query": "memory clustering"}</function>"#;
        let matches = parse_function_style(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/search");
    }

    #[test]
    pub(super) fn function_style_non_string_value() {
        let text = r#"<function=data/list>{"collection": "users", "limit": 10}</function>"#;
        let matches = parse_function_style(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].parameters.get("limit").unwrap(), "10");
    }

    #[test]
    pub(super) fn function_style_groq_variant_no_xml_wrapper() {
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
    pub(super) fn function_style_groq_tree_variant() {
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
    pub(super) fn function_style_groq_git_variant() {
        let text =
            r#"function=code_git>{"operation": "log", "count": 100, "cwd": "/shared/repository"}"#;
        let matches = parse_function_style(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code_git");
        assert_eq!(matches[0].parameters.get("operation").unwrap(), "log");
    }

    #[test]
    pub(super) fn bare_basic() {
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
    pub(super) fn bare_backtick_wrapped() {
        let text = r#"`code/tree` {"path": "."}"#;
        let matches = parse_bare(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/tree");
    }

    #[test]
    pub(super) fn bare_no_match_for_unknown_prefix() {
        let text = r#"unknown/tool {"query": "test"}"#;
        let matches = parse_bare(text);
        assert_eq!(matches.len(), 0, "Should not match unknown prefix");
    }

    #[test]
    pub(super) fn bare_sanitized_underscore_name() {
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
    pub(super) fn bare_sanitized_deep_name() {
        // Multi-level sanitized name: collaboration_chat_send → collaboration/chat/send
        let text = r#"collaboration_chat_send {"room": "general", "message": "hello"}"#;
        let matches = parse_bare(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "collaboration/chat/send");
        assert_eq!(matches[0].parameters.get("room").unwrap(), "general");
    }

    #[test]
    pub(super) fn bare_sanitized_without_function_tag() {
        let text = r#"code_read {"filePath": "main.ts"}"#;
        let matches = parse_bare(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/read");
    }

    #[test]
    pub(super) fn json_object_basic() {
        let text = r#"I'll check that. {"name": "code_tree", "parameters": {"path": "."}}"#;
        let matches = parse_json_object(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/tree");
        assert_eq!(matches[0].parameters.get("path").unwrap(), ".");
        assert_eq!(matches[0].format, "json-object");
    }

    #[test]
    pub(super) fn json_object_with_slash_name() {
        let text = r#"{"name": "collaboration/chat/send", "parameters": {"message": "hello", "room": "general"}}"#;
        let matches = parse_json_object(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "collaboration/chat/send");
        assert_eq!(matches[0].parameters.get("message").unwrap(), "hello");
    }

    #[test]
    pub(super) fn json_object_with_type_field() {
        // Fireworks format with "type": "function" prefix
        let text =
            r#"{"type": "function", "name": "code_git", "parameters": {"operation": "status"}}"#;
        let matches = parse_json_object(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/git");
        assert_eq!(matches[0].parameters.get("operation").unwrap(), "status");
    }

    #[test]
    pub(super) fn json_object_no_match_for_normal_json() {
        // Should not match arbitrary JSON objects
        let text = r#"{"status": "ok", "count": 5}"#;
        let matches = parse_json_object(text);
        assert_eq!(
            matches.len(),
            0,
            "Should not match JSON without name+parameters fields"
        );
    }

    #[test]
    pub(super) fn markdown_basic() {
        let text = "`tool: collaboration/dm` `participants=helper`";
        let matches = parse_markdown(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "collaboration/dm");
        assert_eq!(matches[0].parameters.get("participants").unwrap(), "helper");
        assert_eq!(matches[0].format, "markdown-backtick");
    }

    #[test]
    pub(super) fn markdown_multi_param() {
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
    pub(super) fn markdown_multiple_tools() {
        let text =
            "`tool: code/read` `filepath=a.ts`\n`tool: code/write` `filepath=b.ts` `content=hello`";
        let matches = parse_markdown(text);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].tool_name, "code/read");
        assert_eq!(matches[1].tool_name, "code/write");
    }

    #[test]
    pub(super) fn old_style_basic() {
        let text = r#"<tool name="code/search"><pattern>hello</pattern><path>./src</path></tool>"#;
        let matches = parse_old_style(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/search");
        assert_eq!(matches[0].parameters.get("pattern").unwrap(), "hello");
        assert_eq!(matches[0].format, "old-style");
    }

    #[test]
    pub(super) fn old_style_multiline() {
        let text = r#"<tool name="code/write">
  <filePath>test.ts</filePath>
  <content>function hello() { return 42; }</content>
</tool>"#;
        let matches = parse_old_style(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].parameters.get("filePath").unwrap(), "test.ts");
    }

    #[test]
    pub(super) fn all_formats_mixed() {
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
    pub(super) fn no_tool_calls() {
        let text = "Just a normal response with no tool calls at all.";
        let matches = parse_all_formats(text);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    pub(super) fn xml_params_extraction() {
        let block = "<name>Operator</name><age>30</age>";
        let params = extract_xml_params(block);
        assert_eq!(params.get("name").unwrap(), "Operator");
        assert_eq!(params.get("age").unwrap(), "30");
    }

    #[test]
    pub(super) fn json_params_valid() {
        let json = r#"{"query": "test", "limit": 10, "flag": true}"#;
        let params = parse_json_params(json);
        assert_eq!(params.get("query").unwrap(), "test");
        assert_eq!(params.get("limit").unwrap(), "10");
        assert_eq!(params.get("flag").unwrap(), "true");
    }

    #[test]
    pub(super) fn json_params_invalid_fallback() {
        let json = r#"{"query": "test", bad json"#;
        let params = parse_json_params(json);
        assert_eq!(params.get("query").unwrap(), "test");
    }

    #[test]
    pub(super) fn json_params_empty() {
        assert!(parse_json_params("").is_empty());
    }

    #[test]
    pub(super) fn unsanitize_simple() {
        assert_eq!(unsanitize_tool_name("code_tree"), "code/tree");
        assert_eq!(unsanitize_tool_name("data_list"), "data/list");
    }

    #[test]
    pub(super) fn unsanitize_deep() {
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
    pub(super) fn unsanitize_already_slash() {
        assert_eq!(unsanitize_tool_name("code/tree"), "code/tree");
        assert_eq!(
            unsanitize_tool_name("collaboration/chat/send"),
            "collaboration/chat/send"
        );
    }

    #[test]
    pub(super) fn unsanitize_unknown_prefix() {
        // Unknown prefix stays as-is
        assert_eq!(unsanitize_tool_name("foobar_baz"), "foobar_baz");
    }

    #[test]
    pub(super) fn array_style_basic() {
        let text = r#"["code/search", {"pattern": "test"}]"#;
        let matches = parse_array_style(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/search");
        assert_eq!(matches[0].parameters.get("pattern").unwrap(), "test");
        assert_eq!(matches[0].format, "array-style");
    }

    #[test]
    pub(super) fn array_style_sanitized_name() {
        let text = r#"["collaboration_chat_send", {"room": "general", "message": "hello"}]"#;
        let matches = parse_array_style(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "collaboration/chat/send");
        assert_eq!(matches[0].parameters.get("room").unwrap(), "general");
    }

    #[test]
    pub(super) fn array_style_no_match_unknown_prefix() {
        let text = r#"["unknown/tool", {"param": "value"}]"#;
        let matches = parse_array_style(text);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    pub(super) fn array_style_no_match_non_tool_array() {
        let text = r#"["hello", "world"]"#;
        let matches = parse_array_style(text);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    pub(super) fn array_style_with_surrounding_text() {
        let text = r#"I'll search for that. ["code/search", {"pattern": "genome"}] Let me check."#;
        let matches = parse_array_style(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/search");
    }

    #[test]
    pub(super) fn curly_shorthand_basic() {
        let text = r#"{"code_tree": {"path": "."}}"#;
        let matches = parse_curly_shorthand(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/tree");
        assert_eq!(matches[0].parameters.get("path").unwrap(), ".");
        assert_eq!(matches[0].format, "curly-shorthand");
    }

    #[test]
    pub(super) fn curly_shorthand_deep_name() {
        let text = r#"{"collaboration_wall_write": {"append": true, "content": "hello"}}"#;
        let matches = parse_curly_shorthand(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "collaboration/wall/write");
        assert_eq!(matches[0].parameters.get("content").unwrap(), "hello");
        assert_eq!(matches[0].parameters.get("append").unwrap(), "true");
    }

    #[test]
    pub(super) fn curly_shorthand_slash_name() {
        let text = r#"{"code/tree": {"path": "./src"}}"#;
        let matches = parse_curly_shorthand(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/tree");
    }

    #[test]
    pub(super) fn curly_shorthand_no_match_multi_key() {
        // Multi-key objects are NOT tool calls
        let text = r#"{"name": "test-user", "age": 30}"#;
        let matches = parse_curly_shorthand(text);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    pub(super) fn curly_shorthand_no_match_unknown_prefix() {
        let text = r#"{"foobar_baz": {"param": "val"}}"#;
        let matches = parse_curly_shorthand(text);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    pub(super) fn curly_shorthand_no_match_non_object_value() {
        let text = r#"{"code_tree": "not an object"}"#;
        let matches = parse_curly_shorthand(text);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    pub(super) fn all_formats_catches_groq_output() {
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
    pub(super) fn all_formats_catches_fireworks_json_object() {
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
    pub(super) fn all_formats_catches_array_style() {
        let text = r#"Let me search. ["code/search", {"pattern": "genome"}]"#;
        let matches = parse_all_formats(text);
        let array_match = matches.iter().find(|m| m.format == "array-style");
        assert!(array_match.is_some(), "Should catch array-style tool call");
        assert_eq!(array_match.unwrap().tool_name, "code/search");
    }

    #[test]
    pub(super) fn all_formats_catches_curly_shorthand() {
        let text = r#"{"collaboration_wall_write": {"content": "hello", "append": true}}"#;
        let matches = parse_all_formats(text);
        let curly_match = matches.iter().find(|m| m.format == "curly-shorthand");
        assert!(
            curly_match.is_some(),
            "Should catch curly-shorthand tool call"
        );
        assert_eq!(curly_match.unwrap().tool_name, "collaboration/wall/write");
    }

    #[test]
    pub(super) fn colon_shorthand_simple_read() {
        // code/read: {filePath: "app.py"}
        let text = "code/read: {filePath: \"app.py\"}";
        let matches = parse_colon_shorthand(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/read");
        assert_eq!(matches[0].parameters.get("filePath").unwrap(), "app.py");
        assert_eq!(matches[0].format, "colon-shorthand");
    }

    #[test]
    pub(super) fn colon_shorthand_shell_command() {
        // code/shell/execute: {command: "ls -la"}
        let text = "code/shell/execute: {command: \"ls -la\"}";
        let matches = parse_colon_shorthand(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/shell/execute");
        assert_eq!(matches[0].parameters.get("command").unwrap(), "ls -la");
    }

    #[test]
    pub(super) fn colon_shorthand_tree() {
        let text = "code/tree: {path: \".\"}";
        let matches = parse_colon_shorthand(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/tree");
        assert_eq!(matches[0].parameters.get("path").unwrap(), ".");
    }

    #[test]
    pub(super) fn colon_shorthand_write_with_codeblock() {
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
    pub(super) fn colon_shorthand_write_malformed_filename() {
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
    pub(super) fn colon_shorthand_all_formats_integration() {
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
    pub(super) fn colon_shorthand_no_match_unknown_prefix() {
        let text = "unknown/tool: {param: \"value\"}";
        let matches = parse_colon_shorthand(text);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    pub(super) fn colon_bare_read() {
        let text = "code/read: hello.py";
        let matches = parse_colon_shorthand(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/read");
        assert_eq!(matches[0].parameters.get("filePath").unwrap(), "hello.py");
        assert_eq!(matches[0].format, "colon-shorthand-bare");
    }

    #[test]
    pub(super) fn colon_bare_shell() {
        let text = "code/shell/execute: ls -la";
        let matches = parse_colon_shorthand(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/shell/execute");
        assert_eq!(matches[0].parameters.get("command").unwrap(), "ls -la");
    }

    #[test]
    pub(super) fn colon_bare_write_with_codeblock() {
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
    pub(super) fn colon_bare_write_incomplete_codeblock() {
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
    pub(super) fn colon_bare_write_with_garbage_then_codeblock() {
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
    pub(super) fn colon_bare_no_double_match_with_json_variant() {
        // JSON-params variant should take precedence; bare variant should NOT also fire.
        let text = "code/read: {filePath: \"main.ts\"}";
        let matches = parse_colon_shorthand(text);
        // Should only get one match (the JSON-params one)
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].format, "colon-shorthand");
    }

    #[test]
    pub(super) fn colon_bare_tree() {
        let text = "code/tree: ./src";
        let matches = parse_colon_shorthand(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/tree");
        assert_eq!(matches[0].parameters.get("path").unwrap(), "./src");
    }

    #[test]
    pub(super) fn colon_params_unquoted_key() {
        let params = parse_colon_params(r#"filePath: "app.py""#);
        assert_eq!(params.get("filePath").unwrap(), "app.py");
    }

    #[test]
    pub(super) fn colon_params_quoted_key() {
        let params = parse_colon_params(r#""filePath": "app.py""#);
        assert_eq!(params.get("filePath").unwrap(), "app.py");
    }

    #[test]
    pub(super) fn colon_params_multiple() {
        let params = parse_colon_params(r#"query: "flask", path: "./src""#);
        assert_eq!(params.get("query").unwrap(), "flask");
        assert_eq!(params.get("path").unwrap(), "./src");
    }

    #[test]
    pub(super) fn deepseek_single_tool() {
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
    pub(super) fn deepseek_multiple_tools() {
        let text = "\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}begin\u{FF5C}\u{FF1E}\n{\"name\": \"code_read\", \"arguments\": {\"filePath\": \"main.ts\"}}\n{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"TODO\"}}\n\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}end\u{FF5C}\u{FF1E}";
        let matches = parse_deepseek(text);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].tool_name, "code/read");
        assert_eq!(matches[1].tool_name, "code/search");
    }

    #[test]
    pub(super) fn deepseek_json_array() {
        let text = "\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}begin\u{FF5C}\u{FF1E}\n[{\"name\": \"code_tree\", \"arguments\": {\"path\": \"./src\"}}]\n\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}end\u{FF5C}\u{FF1E}";
        let matches = parse_deepseek(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/tree");
        assert_eq!(matches[0].parameters.get("path").unwrap(), "./src");
    }

    #[test]
    pub(super) fn deepseek_double_encoded_arguments() {
        let text = "\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}begin\u{FF5C}\u{FF1E}\n{\"name\": \"code_search\", \"arguments\": \"{\\\"pattern\\\": \\\"test\\\"}\"}\n\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}end\u{FF5C}\u{FF1E}";
        let matches = parse_deepseek(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].parameters.get("pattern").unwrap(), "test");
    }

    #[test]
    pub(super) fn deepseek_mixed_with_text() {
        let text = "I'll look at the code.\n\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}begin\u{FF5C}\u{FF1E}\n{\"name\": \"code_read\", \"arguments\": {\"filePath\": \"test.ts\"}}\n\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}end\u{FF5C}\u{FF1E}\nHere are the results.";
        let matches = parse_deepseek(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/read");
    }

    #[test]
    pub(super) fn deepseek_no_match_without_markers() {
        let text = "{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"test\"}}";
        let matches = parse_deepseek(text);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    pub(super) fn llama_basic() {
        let text =
            "<|python_tag|>{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"test\"}}";
        let matches = parse_llama(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/search");
        assert_eq!(matches[0].parameters.get("pattern").unwrap(), "test");
        assert_eq!(matches[0].format, "llama");
    }

    #[test]
    pub(super) fn llama_function_wrapper() {
        let text = "<|python_tag|>{\"type\": \"function\", \"function\": {\"name\": \"code_read\", \"arguments\": {\"filePath\": \"app.py\"}}}";
        let matches = parse_llama(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/read");
        assert_eq!(matches[0].parameters.get("filePath").unwrap(), "app.py");
    }

    #[test]
    pub(super) fn llama_with_eot_id() {
        let text =
            "<|python_tag|>{\"name\": \"code_tree\", \"arguments\": {\"path\": \".\"}}<|eot_id|>";
        let matches = parse_llama(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/tree");
    }

    #[test]
    pub(super) fn llama_multiple_calls() {
        let text = "<|python_tag|>[{\"name\": \"code_read\", \"arguments\": {\"filePath\": \"a.ts\"}}, {\"name\": \"code_read\", \"arguments\": {\"filePath\": \"b.ts\"}}]<|eot_id|>";
        let matches = parse_llama(text);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].parameters.get("filePath").unwrap(), "a.ts");
        assert_eq!(matches[1].parameters.get("filePath").unwrap(), "b.ts");
    }

    #[test]
    pub(super) fn llama_no_match_without_tag() {
        let text = "{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"test\"}}";
        let matches = parse_llama(text);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    pub(super) fn mistral_basic() {
        let text =
            "[TOOL_CALLS] [{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"test\"}}]";
        let matches = parse_mistral(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/search");
        assert_eq!(matches[0].parameters.get("pattern").unwrap(), "test");
        assert_eq!(matches[0].format, "mistral");
    }

mod part_2;
