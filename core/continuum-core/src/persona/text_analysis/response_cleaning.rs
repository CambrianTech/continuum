//! Response Cleaning — Strip unwanted prefixes and thinking tags from AI-generated responses
//!
//! Ported from ResponseCleaner.ts (95 lines → ~70 lines Rust).
//! LLMs sometimes copy formatting from conversation history, adding
//! unwanted prefixes like "[HH:MM] Name: " to their responses.
//! Models like DeepSeek/Qwen also emit `<thinking>...</thinking>` chain-of-thought
//! blocks that should be stripped before displaying to users.
//!
//! Processing order:
//! 0. Strip `<thinking>` / `<think>` blocks (can appear anywhere)
//! 1. `[HH:MM] Name: ` — timestamp + name
//! 2. `Name: ` — name only (starts with capital)
//! 3. `[HH:MM] ` — timestamp only
//! 4. `**Name:** ` or `*Name:* ` — markdown role markers

use regex::Regex;
use std::sync::LazyLock;

/// Result of response cleaning with optional extracted thinking content.
pub struct CleanResult {
    /// Cleaned response text with thinking blocks and prefixes stripped
    pub text: String,
    /// Extracted thinking/reasoning content, if any was found
    pub thinking: Option<String>,
}

/// Regex to match `<thinking>...</thinking>` and `<think>...</think>` blocks.
/// Case-insensitive, dotall (. matches newline), non-greedy.
static PATTERN_THINKING: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?si)<think(?:ing)?>.*?</think(?:ing)?>").expect("thinking regex")
});

static PATTERN_TIMESTAMP_NAME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[\d{1,2}:\d{2}\]\s+[^:]+:\s*").expect("timestamp+name regex"));

static PATTERN_NAME_ONLY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[A-Z][A-Za-z\s]+:\s*").expect("name-only regex"));

static PATTERN_TIMESTAMP_ONLY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\[\d{1,2}:\d{2}\]\s*").expect("timestamp-only regex"));

static PATTERN_MARKDOWN_ROLE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\*{1,2}[A-Za-z\s]+:\*{1,2}\s*").expect("markdown role regex"));

/// Regex to extract the content INSIDE thinking tags (for downstream use).
static PATTERN_THINKING_CONTENT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?si)<think(?:ing)?>(.*?)</think(?:ing)?>").expect("thinking content regex")
});

/// Our OWN internal perception-frame section labels. Some models, asked to reflect,
/// emit this deliberation scaffold as VISIBLE text instead of a clean turn — glass-boxed
/// live 2026-07-14: a persona posted `[working-memory] … [analysis] … [what I propose] …
/// [example] "…"` straight into the room. That's #158 reserved-vocabulary mimicry: she
/// mirrored the framing we feed her back as speech. This is not a turn; it's leaked
/// internals.
const SCAFFOLD_LABELS: [&str; 8] = [
    "[working-memory]",
    "[your recent messages]",
    "[recent messages]",
    "[analysis]",
    "[what i propose]",
    "[what changed]",
    "[perception]",
    "[your recent acts]",
];

/// Is this response a leaked deliberation scaffold rather than a turn? Trigger only on
/// TWO+ distinct internal labels — one alone ("my [analysis] shows…") can be legitimate
/// prose, but two+ of our own frame labels together is unambiguous. Conservative by
/// design: a false negative just posts a slightly odd turn; a false positive would eat a
/// real message, so we demand the stronger signal.
pub(crate) fn is_leaked_deliberation_scaffold(text: &str) -> bool {
    let lower = text.to_lowercase();
    SCAFFOLD_LABELS.iter().filter(|l| lower.contains(*l)).count() >= 2
}

/// Clean an AI response by stripping thinking blocks and unwanted prefixes.
///
/// Returns a `CleanResult` with the cleaned text and any extracted thinking content.
///
/// Processing order:
/// 0. Strip `<thinking>` / `<think>` blocks (extracted for downstream use)
/// 1. `[HH:MM] Name: ` → strip timestamp + name
/// 2. `Name: ` → strip name-only prefix (starts with capital letter)
/// 3. `[HH:MM] ` → strip timestamp-only prefix
/// 4. `**Name:** ` or `*Name:* ` → strip markdown role markers
pub fn clean_response(response: &str) -> CleanResult {
    // Phase 0: Extract and strip thinking blocks
    let mut thinking_parts: Vec<String> = Vec::new();
    for cap in PATTERN_THINKING_CONTENT.captures_iter(response) {
        if let Some(content) = cap.get(1) {
            let trimmed = content.as_str().trim();
            if !trimmed.is_empty() {
                thinking_parts.push(trimmed.to_string());
            }
        }
    }
    let thinking = if thinking_parts.is_empty() {
        None
    } else {
        Some(thinking_parts.join("\n\n"))
    };

    let after_thinking = PATTERN_THINKING.replace_all(response, "");
    let mut cleaned = after_thinking.trim();

    // Phase 0.5: a LEAKED deliberation scaffold is not a turn — suppress it. Preserve the
    // whole block as thinking (hippocampus still learns from the reasoning) and return
    // empty text so the caller posts nothing; she re-turns cleanly next tick. Better a
    // silent beat than her internal perception frame in the room. (#158)
    if is_leaked_deliberation_scaffold(cleaned) {
        let mut leaked = thinking_parts;
        leaked.push(cleaned.to_string());
        return CleanResult {
            text: String::new(),
            thinking: Some(leaked.join("\n\n")),
        };
    }

    // Phase 1-4: Apply prefix patterns in priority order
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

    CleanResult {
        text: cleaned.trim().to_string(),
        thinking,
    }
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
        assert_eq!(
            clean_response("[11:59] GPT Assistant: Yes, Joel...").text,
            "Yes, Joel..."
        );
    }

    #[test]
    fn test_strip_name_only() {
        assert_eq!(
            clean_response("GPT Assistant: Yes, Joel...").text,
            "Yes, Joel..."
        );
    }

    #[test]
    fn test_strip_timestamp_only() {
        assert_eq!(clean_response("[11:59] message here").text, "message here");
    }

    #[test]
    fn test_strip_markdown_double_star() {
        assert_eq!(
            clean_response("**Assistant:** answer here").text,
            "answer here"
        );
    }

    #[test]
    fn test_strip_markdown_single_star() {
        assert_eq!(clean_response("*Helper:* the answer").text, "the answer");
    }

    #[test]
    fn test_no_prefix() {
        let result = clean_response("Just a normal response");
        assert_eq!(result.text, "Just a normal response");
        assert!(result.thinking.is_none());
    }

    #[test]
    fn test_preserves_content() {
        let input = "This response has no prefix but mentions [time] and Name: in the middle.";
        assert_eq!(clean_response(input).text, input);
    }

    #[test]
    fn test_nested_prefixes() {
        assert_eq!(
            clean_response("[11:59] GPT: Assistant: hello").text,
            "hello"
        );
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
        assert_eq!(clean_response("").text, "");
        assert!(!has_prefix(""));
    }

    #[test]
    fn test_whitespace_trimming() {
        assert_eq!(clean_response("  [11:59] GPT: hello  ").text, "hello");
    }

    // ─── Thinking tag tests ────────────────────────────────────

    #[test]
    fn test_strip_thinking_tags() {
        let input = "<thinking>Let me analyze this...</thinking>Here is my response.";
        let result = clean_response(input);
        assert_eq!(result.text, "Here is my response.");
        assert_eq!(result.thinking.as_deref(), Some("Let me analyze this..."));
    }

    #[test]
    fn test_strip_think_tags_qwen_variant() {
        let input = "<think>I need to consider the options carefully.</think>\nThe answer is 42.";
        let result = clean_response(input);
        assert_eq!(result.text, "The answer is 42.");
        assert_eq!(
            result.thinking.as_deref(),
            Some("I need to consider the options carefully.")
        );
    }

    #[test]
    fn test_thinking_tags_case_insensitive() {
        let input = "<THINKING>Deep thought here</THINKING>Result text.";
        let result = clean_response(input);
        assert_eq!(result.text, "Result text.");
        assert_eq!(result.thinking.as_deref(), Some("Deep thought here"));
    }

    #[test]
    fn test_thinking_multiline() {
        let input = "<thinking>\nStep 1: analyze\nStep 2: decide\nStep 3: respond\n</thinking>\nHere's my answer.";
        let result = clean_response(input);
        assert_eq!(result.text, "Here's my answer.");
        assert!(result.thinking.is_some());
        assert!(result.thinking.as_deref().unwrap().contains("Step 1"));
        assert!(result.thinking.as_deref().unwrap().contains("Step 3"));
    }

    #[test]
    fn test_multiple_thinking_blocks() {
        let input =
            "<thinking>First thought</thinking>Part one. <think>Second thought</think>Part two.";
        let result = clean_response(input);
        assert_eq!(result.text, "Part one. Part two.");
        let thinking = result.thinking.unwrap();
        assert!(thinking.contains("First thought"));
        assert!(thinking.contains("Second thought"));
    }

    #[test]
    fn test_no_thinking_tags() {
        let input = "Just a normal response with no thinking tags.";
        let result = clean_response(input);
        assert_eq!(result.text, input);
        assert!(result.thinking.is_none());
    }

    #[test]
    fn test_thinking_plus_prefix_stripping() {
        // Thinking tags stripped first, then prefix stripping runs on remainder
        let input = "<thinking>reasoning here</thinking>**Assistant:** The answer is yes.";
        let result = clean_response(input);
        assert_eq!(result.text, "The answer is yes.");
        assert_eq!(result.thinking.as_deref(), Some("reasoning here"));
    }

    #[test]
    fn test_empty_thinking_tags() {
        let input = "<thinking></thinking>Actual response.";
        let result = clean_response(input);
        assert_eq!(result.text, "Actual response.");
        assert!(result.thinking.is_none()); // Empty thinking is not stored
    }

    #[test]
    fn test_thinking_with_only_whitespace() {
        let input = "<thinking>   \n  \n   </thinking>Response here.";
        let result = clean_response(input);
        assert_eq!(result.text, "Response here.");
        assert!(result.thinking.is_none()); // Whitespace-only thinking is not stored
    }

    /// what this catches: a persona's INTERNAL deliberation scaffold leaking into the
    /// room as a turn (glass-boxed live 2026-07-14 — Anwen posted her whole
    /// [working-memory]/[analysis]/[what I propose]/[example] frame). Two+ of our own
    /// frame labels ⇒ suppress: empty posted text, whole block preserved as thinking so
    /// hippocampus still learns. Regression for #158.
    #[test]
    fn leaked_deliberation_scaffold_is_suppressed_not_posted() {
        let leaked = "[working-memory]\nYour recent acts and the room's state: ...\n\
            [your recent messages]\n1. \"...\"\n2. \"...\"\n[analysis]\nYou've repeated \
            yourself.\n[what I propose]\nAcknowledge and pivot.\n[example]\n\"Let's look at \
            models.rs next.\"";
        let out = clean_response(leaked);
        assert_eq!(out.text, "", "leaked scaffold must NOT post");
        assert!(
            out.thinking.as_deref().unwrap_or("").contains("what I propose"),
            "the leaked frame is preserved as thinking for memory"
        );
    }

    /// what this catches: over-stripping a LEGITIMATE turn that merely mentions one label
    /// in prose. One label is not a leak; only 2+ of our frame labels together is.
    #[test]
    fn single_label_in_prose_is_kept() {
        let legit = "My [analysis] of the code shows a race condition in the lock path.";
        let out = clean_response(legit);
        assert_eq!(
            out.text, legit,
            "a single incidental label must not trigger suppression"
        );
    }

    #[test]
    fn normal_turn_untouched_by_scaffold_gate() {
        let msg = "Sure — I'll open models.rs and summarize how providers are wired.";
        assert_eq!(clean_response(msg).text, msg);
    }
}
