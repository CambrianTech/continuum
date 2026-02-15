//! Mention Detection — @mention and directed-address parsing
//!
//! Ported from PersonaMessageEvaluator.ts (lines 894-926).
//! Two checks combined into one IPC call to avoid 2x round-trip overhead.
//!
//! - `is_persona_mentioned`: @PersonaName, @uniqueid, or "Name," / "Name:" at start
//! - `has_directed_mention`: any @word pattern (detects messages aimed at a specific persona)

use std::sync::LazyLock;
use regex::Regex;

/// Regex for detecting directed @mentions anywhere in text.
/// Matches @word at start or after whitespace. Excludes email-like patterns (word@word).
static DIRECTED_MENTION_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?:^|\s)@[a-zA-Z][\w\s-]*").expect("directed mention regex")
});

/// Check if a specific persona is mentioned in the message text.
///
/// Supports:
/// - @mentions anywhere: `@PersonaName` or `@uniqueid`
/// - Direct address at start: `PersonaName,` or `PersonaName:` or `uniqueid,` or `uniqueid:`
///
/// All comparisons are case-insensitive.
pub fn is_persona_mentioned(
    message_text: &str,
    persona_display_name: &str,
    persona_unique_id: &str,
) -> bool {
    let msg_lower = message_text.to_lowercase();
    let name_lower = persona_display_name.to_lowercase();
    let uid_lower = persona_unique_id.to_lowercase();

    // @mentions anywhere: "@PersonaName" or "@uniqueid"
    if msg_lower.contains(&format!("@{name_lower}")) {
        return true;
    }
    if !uid_lower.is_empty() && msg_lower.contains(&format!("@{uid_lower}")) {
        return true;
    }

    // Direct address at start: "PersonaName," or "PersonaName:" or "uniqueid," or "uniqueid:"
    if msg_lower.starts_with(&format!("{name_lower},"))
        || msg_lower.starts_with(&format!("{name_lower}:"))
    {
        return true;
    }
    if !uid_lower.is_empty()
        && (msg_lower.starts_with(&format!("{uid_lower},"))
            || msg_lower.starts_with(&format!("{uid_lower}:")))
    {
        return true;
    }

    false
}

/// Check if a message contains ANY directed @mention (aimed at any persona).
/// Used to prevent dog-piling: when someone @mentions a specific AI, others stay silent.
///
/// Matches `@word` at start or after whitespace. Excludes email-like patterns.
pub fn has_directed_mention(text: &str) -> bool {
    DIRECTED_MENTION_RE.is_match(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    // === is_persona_mentioned ===

    #[test]
    fn test_at_mention_display_name() {
        assert!(is_persona_mentioned("Hey @Teacher AI what's up?", "Teacher AI", "teacher-ai"));
    }

    #[test]
    fn test_at_mention_unique_id() {
        assert!(is_persona_mentioned("Hey @teacher-ai what's up?", "Teacher AI", "teacher-ai"));
    }

    #[test]
    fn test_at_mention_case_insensitive() {
        assert!(is_persona_mentioned("yo @TEACHER AI help", "Teacher AI", "teacher-ai"));
        assert!(is_persona_mentioned("yo @TEACHER-AI help", "Teacher AI", "teacher-ai"));
    }

    #[test]
    fn test_direct_address_comma() {
        assert!(is_persona_mentioned("Teacher AI, explain closures", "Teacher AI", "teacher-ai"));
    }

    #[test]
    fn test_direct_address_colon() {
        assert!(is_persona_mentioned("teacher-ai: what's up", "Teacher AI", "teacher-ai"));
    }

    #[test]
    fn test_not_mentioned_substring() {
        assert!(!is_persona_mentioned("mentioned the teacher today", "Teacher AI", "teacher-ai"));
    }

    #[test]
    fn test_not_mentioned_no_at() {
        assert!(!is_persona_mentioned("Teacher AI is great", "Teacher AI", "teacher-ai"));
    }

    #[test]
    fn test_not_mentioned_empty_message() {
        assert!(!is_persona_mentioned("", "Teacher AI", "teacher-ai"));
    }

    #[test]
    fn test_empty_unique_id() {
        assert!(!is_persona_mentioned("hello", "Teacher AI", ""));
        assert!(is_persona_mentioned("@teacher ai hello", "Teacher AI", ""));
    }

    // === has_directed_mention ===

    #[test]
    fn test_directed_at_start() {
        assert!(has_directed_mention("@deepseek fix the bug"));
    }

    #[test]
    fn test_directed_after_space() {
        assert!(has_directed_mention("Hey @someone check this"));
    }

    #[test]
    fn test_no_directed_mention() {
        assert!(!has_directed_mention("No mentions here"));
    }

    #[test]
    fn test_email_excluded() {
        // "contact@example" — the @ is preceded by a non-whitespace char,
        // so the regex won't match it as a directed mention.
        assert!(!has_directed_mention("contact@example.com"));
    }

    #[test]
    fn test_at_symbol_alone() {
        assert!(!has_directed_mention("@ "));
    }
}
