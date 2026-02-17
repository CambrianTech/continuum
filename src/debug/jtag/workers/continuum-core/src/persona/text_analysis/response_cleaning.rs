//! Response Cleaning — Strip unwanted prefixes from AI-generated responses
//!
//! Ported from ResponseCleaner.ts (95 lines → ~70 lines Rust).
//! LLMs sometimes copy formatting from conversation history, adding
//! unwanted prefixes like "[HH:MM] Name: " to their responses.
//!
//! 4 regex patterns applied in order:
//! 1. `[HH:MM] Name: ` — timestamp + name
//! 2. `Name: ` — name only (starts with capital)
//! 3. `[HH:MM] ` — timestamp only
//! 4. `**Name:** ` or `*Name:* ` — markdown role markers

use std::sync::LazyLock;
use regex::Regex;

static PATTERN_TIMESTAMP_NAME: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\[\d{1,2}:\d{2}\]\s+[^:]+:\s*").expect("timestamp+name regex")
});

static PATTERN_NAME_ONLY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^[A-Z][A-Za-z\s]+:\s*").expect("name-only regex")
});

static PATTERN_TIMESTAMP_ONLY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\[\d{1,2}:\d{2}\]\s*").expect("timestamp-only regex")
});

static PATTERN_MARKDOWN_ROLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^\*{1,2}[A-Za-z\s]+:\*{1,2}\s*").expect("markdown role regex")
});

/// Clean an AI response by stripping unwanted prefixes.
///
/// Returns the cleaned text. Applies 4 regex patterns in order:
/// 1. `[HH:MM] Name: ` → strip timestamp + name
/// 2. `Name: ` → strip name-only prefix (starts with capital letter)
/// 3. `[HH:MM] ` → strip timestamp-only prefix
/// 4. `**Name:** ` or `*Name:* ` → strip markdown role markers
pub fn clean_response(response: &str) -> String {
    let mut cleaned = response.trim();

    // Apply patterns in priority order
    if let Some(m) = PATTERN_TIMESTAMP_NAME.find(cleaned) {
        cleaned = &cleaned[m.end()..];
    }
    if let Some(m) = PATTERN_NAME_ONLY.find(cleaned) {
        cleaned = &cleaned[m.end()..];
    }
    if let Some(m) = PATTERN_TIMESTAMP_ONLY.find(cleaned) {
        cleaned = &cleaned[m.end()..];
    }
    if let Some(m) = PATTERN_MARKDOWN_ROLE.find(cleaned) {
        cleaned = &cleaned[m.end()..];
    }

    cleaned.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Check if a response has a prefix that would be cleaned.
    fn has_prefix(response: &str) -> bool {
        let trimmed = response.trim();
        PATTERN_TIMESTAMP_NAME.is_match(trimmed)
            || PATTERN_NAME_ONLY.is_match(trimmed)
            || PATTERN_TIMESTAMP_ONLY.is_match(trimmed)
            || PATTERN_MARKDOWN_ROLE.is_match(trimmed)
    }

    #[test]
    fn test_strip_timestamp_and_name() {
        assert_eq!(clean_response("[11:59] GPT Assistant: Yes, Joel..."), "Yes, Joel...");
    }

    #[test]
    fn test_strip_name_only() {
        assert_eq!(clean_response("GPT Assistant: Yes, Joel..."), "Yes, Joel...");
    }

    #[test]
    fn test_strip_timestamp_only() {
        assert_eq!(clean_response("[11:59] message here"), "message here");
    }

    #[test]
    fn test_strip_markdown_double_star() {
        assert_eq!(clean_response("**Assistant:** answer here"), "answer here");
    }

    #[test]
    fn test_strip_markdown_single_star() {
        assert_eq!(clean_response("*Helper:* the answer"), "the answer");
    }

    #[test]
    fn test_no_prefix() {
        assert_eq!(clean_response("Just a normal response"), "Just a normal response");
    }

    #[test]
    fn test_preserves_content() {
        let input = "This response has no prefix but mentions [time] and Name: in the middle.";
        // Only patterns at the START get stripped — "Name:" at start will match though
        // Actually "This" starts with capital and matches `^[A-Z][A-Za-z\s]+:\s*`?
        // No — "This response has no prefix but mentions [time] and Name" contains non-alpha chars
        // Pattern 2 is `^[A-Z][A-Za-z\s]+:` which requires only letters and spaces before ':'
        // "This response has no prefix but mentions [time] and Name:" — nope, brackets break it
        assert_eq!(clean_response(input), input);
    }

    #[test]
    fn test_nested_prefixes() {
        // Timestamp+name stripped first, then if there's still a name prefix, strip that too
        assert_eq!(clean_response("[11:59] GPT: Assistant: hello"), "hello");
    }

    #[test]
    fn test_has_prefix_true() {
        assert!(has_prefix("[11:59] GPT: hello"));
        assert!(has_prefix("Assistant: hello"));
        assert!(has_prefix("[11:59] hello"));
        assert!(has_prefix("**Helper:** hello"));
    }

    #[test]
    fn test_has_prefix_false() {
        assert!(!has_prefix("Just a normal message"));
        assert!(!has_prefix("lowercase: not a name"));
        assert!(!has_prefix("123: not a name"));
    }

    #[test]
    fn test_empty_input() {
        assert_eq!(clean_response(""), "");
        assert!(!has_prefix(""));
    }

    #[test]
    fn test_whitespace_trimming() {
        assert_eq!(clean_response("  [11:59] GPT: hello  "), "hello");
    }
}
