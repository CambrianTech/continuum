#![cfg(test)]
use super::*;

    #[test]
    pub(super) fn mistral_multiple() {
        let text = "[TOOL_CALLS] [{\"name\": \"code_read\", \"arguments\": {\"filePath\": \"a.ts\"}}, {\"name\": \"code_write\", \"arguments\": {\"filePath\": \"b.ts\", \"content\": \"hello\"}}]";
        let matches = parse_mistral(text);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].tool_name, "code/read");
        assert_eq!(matches[1].tool_name, "code/write");
    }

    #[test]
    pub(super) fn mistral_no_match_without_prefix() {
        let text = "[{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"test\"}}]";
        let matches = parse_mistral(text);
        assert_eq!(matches.len(), 0);
    }

    #[test]
    pub(super) fn hermes_basic() {
        let text = "<tool_call>\n{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"test\"}}\n</tool_call>";
        let matches = parse_hermes(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/search");
        assert_eq!(matches[0].parameters.get("pattern").unwrap(), "test");
        assert_eq!(matches[0].format, "hermes");
    }

    #[test]
    pub(super) fn hermes_multiple_blocks() {
        let text = "<tool_call>\n{\"name\": \"code_read\", \"arguments\": {\"filePath\": \"a.ts\"}}\n</tool_call>\nSome text.\n<tool_call>\n{\"name\": \"code_read\", \"arguments\": {\"filePath\": \"b.ts\"}}\n</tool_call>";
        let matches = parse_hermes(text);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].parameters.get("filePath").unwrap(), "a.ts");
        assert_eq!(matches[1].parameters.get("filePath").unwrap(), "b.ts");
    }

    #[test]
    pub(super) fn hermes_with_surrounding_text() {
        let text = "Let me check.\n<tool_call>\n{\"name\": \"code_tree\", \"arguments\": {\"path\": \".\"}}\n</tool_call>\nDone.";
        let matches = parse_hermes(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/tree");
    }

    #[test]
    pub(super) fn qwen_basic() {
        let text = "<tool_call>\n{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"test\"}}\n</tool_call>";
        let matches = parse_qwen(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/search");
        assert_eq!(matches[0].format, "qwen");
    }

    #[test]
    pub(super) fn qwen_double_encoded_arguments() {
        let text = "<tool_call>\n{\"name\": \"code_search\", \"arguments\": \"{\\\"pattern\\\": \\\"test\\\"}\"}\n</tool_call>";
        let matches = parse_qwen(text);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].parameters.get("pattern").unwrap(), "test");
    }

    #[test]
    pub(super) fn qwen_multiple() {
        let text = "<tool_call>\n{\"name\": \"code_read\", \"arguments\": {\"filePath\": \"x.ts\"}}\n</tool_call>\n<tool_call>\n{\"name\": \"code_write\", \"arguments\": \"{\\\"filePath\\\": \\\"y.ts\\\", \\\"content\\\": \\\"hello\\\"}\"}\n</tool_call>";
        let matches = parse_qwen(text);
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0].parameters.get("filePath").unwrap(), "x.ts");
        assert_eq!(matches[1].parameters.get("filePath").unwrap(), "y.ts");
        assert_eq!(matches[1].parameters.get("content").unwrap(), "hello");
    }

    #[test]
    pub(super) fn model_family_deepseek_prioritized() {
        let text = "\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}begin\u{FF5C}\u{FF1E}\n{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"test\"}}\n\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}end\u{FF5C}\u{FF1E}";
        let matches = parse_with_model_family(text, ModelFamily::DeepSeek);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].format, "deepseek");
    }

    #[test]
    pub(super) fn model_family_falls_through_to_generic() {
        // DeepSeek family hint but Anthropic format — should fall through
        let text = "<tool_use><tool_name>code/read</tool_name><parameters><filePath>x.ts</filePath></parameters></tool_use>";
        let matches = parse_with_model_family(text, ModelFamily::DeepSeek);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].format, "anthropic-style");
    }

    #[test]
    pub(super) fn model_family_generic_uses_all_parsers() {
        let text = "<tool_use><tool_name>code/search</tool_name><parameters><pattern>test</pattern></parameters></tool_use>";
        let matches = parse_with_model_family(text, ModelFamily::Generic);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].format, "anthropic-style");
    }

    #[test]
    pub(super) fn truncation_recovery_tool_call() {
        let text =
            "<tool_call>\n{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"test\"}}";
        // No closing </tool_call>
        let matches = parse_with_truncation_recovery(text, ModelFamily::Hermes);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/search");
    }

    #[test]
    pub(super) fn truncation_recovery_tool_use() {
        let text =
            "<tool_use><tool_name>code/read</tool_name><parameters><filePath>test.ts</filePath>";
        // No closing </parameters> or </tool_use>
        let matches = parse_with_truncation_recovery(text, ModelFamily::Generic);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/read");
    }

    #[test]
    pub(super) fn truncation_recovery_deepseek() {
        let text = format!(
            "\u{FF1C}\u{FF5C}tool\u{2581}calls\u{2581}begin\u{FF5C}\u{FF1E}\n{{\"name\": \"code_search\", \"arguments\": {{\"pattern\": \"test\"}}"
        );
        let matches = parse_with_truncation_recovery(&text, ModelFamily::DeepSeek);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].tool_name, "code/search");
    }

    #[test]
    pub(super) fn truncation_recovery_not_needed() {
        // Complete text — should parse normally
        let text = "<tool_call>\n{\"name\": \"code_search\", \"arguments\": {\"pattern\": \"test\"}}\n</tool_call>";
        let matches = parse_with_truncation_recovery(text, ModelFamily::Hermes);
        assert_eq!(matches.len(), 1);
    }
