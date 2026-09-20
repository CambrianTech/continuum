//! Mention Detection — @mention and directed-address parsing
//!
//! Ported from PersonaMessageEvaluator.ts (lines 894-926).
//! Two checks combined into one IPC call to avoid 2x round-trip overhead.
//!
//! - `is_persona_mentioned`: @PersonaName, @uniqueid, or "Name," / "Name:" at start
//! - `has_directed_mention`: any @word pattern (detects messages aimed at a specific persona)
//!
//! Hot path: called once per message per persona per tick from the
//! unified evaluator pre-response gate (see
//! [`crate::persona::evaluator::full_evaluate`]). Pre-2026-05-30 this
//! function allocated up to 9 Strings per call (msg.to_lowercase() +
//! name.to_lowercase() + uid.to_lowercase() + 6 format!() markers for
//! the @prefix and trailing-comma/colon checks). Now: zero per-call
//! allocations via [`crate::utils::str_case::contains_ascii_case_insensitive`]
//! and [`crate::utils::str_case::starts_with_ascii_case_insensitive`],
//! both of which fold ASCII bytes inline without allocating a
//! lowercase copy. Persona names + uids are ASCII in continuum so the
//! ASCII fast path is sufficient.

use crate::utils::str_case::starts_with_ascii_case_insensitive;
use regex::Regex;
use std::sync::LazyLock;

/// Regex for detecting directed @mentions anywhere in text.
/// Matches @word at start or after whitespace. Excludes email-like patterns (word@word).
static DIRECTED_MENTION_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?:^|\s)@[a-zA-Z][\w\s-]*").expect("directed mention regex"));

/// Check if a specific persona is mentioned in the message text.
///
/// Supports:
/// - @mentions anywhere: `@PersonaName` or `@uniqueid`
/// - Direct address at start: `PersonaName,` or `PersonaName:` or `uniqueid,` or `uniqueid:`
///
/// All comparisons are ASCII case-insensitive. Persona names + uids
/// are ASCII; the ASCII fast path avoids the unicode-aware
/// `str::to_lowercase()` allocation per call.
///
/// To check "Name," at start (and similarly "Name:"), the function
/// folds the prefix bytes against `persona_display_name` and then
/// verifies the next byte is the literal `,` or `:`. The same logic
/// covers the `persona_unique_id` branch.
pub fn is_persona_mentioned(
    message_text: &str,
    persona_display_name: &str,
    persona_unique_id: &str,
) -> bool {
    // @mentions anywhere: scan for "@" + name / uid in the haystack.
    // The previous implementation pre-built `format!("@{name_lower}")`
    // every call; here we scan two passes (one for the @-bare-name
    // path, one for the rest-of-name), avoiding the marker String.
    if has_at_mention_of(message_text, persona_display_name) {
        return true;
    }
    if !persona_unique_id.is_empty() && has_at_mention_of(message_text, persona_unique_id) {
        return true;
    }

    // Direct address at start: "Name," / "Name:" / "uid," / "uid:".
    // starts_with_ascii_case_insensitive covers the name part; then
    // the next raw byte (not case-folded) must be the literal
    // separator.
    if starts_with_then_separator(message_text, persona_display_name) {
        return true;
    }
    if !persona_unique_id.is_empty() && starts_with_then_separator(message_text, persona_unique_id)
    {
        return true;
    }

    false
}

/// True when `haystack` contains `"@" + name` case-insensitively. Splits
/// the check into a scan for the `@` byte then a window match — avoids
/// allocating the `format!("@{name}")` marker.
fn has_at_mention_of(haystack: &str, name: &str) -> bool {
    let h = haystack.as_bytes();
    let n = name.as_bytes();
    if n.is_empty() {
        return false;
    }
    // Need at least "@" + 1 byte of name to match.
    if h.len() < n.len() + 1 {
        return false;
    }
    // Look for '@' at any position where `name.len()` more bytes still fit.
    for i in 0..=(h.len() - n.len() - 1) {
        if h[i] == b'@' && h[i + 1..i + 1 + n.len()].eq_ignore_ascii_case(n) {
            return true;
        }
    }
    false
}

/// True when `haystack` starts with `name` (case-insensitive ASCII) AND
/// the byte immediately after the name is `,` or `:`. Encodes the
/// "direct address" idiom — `"Name, ..."` / `"Name: ..."`.
fn starts_with_then_separator(haystack: &str, name: &str) -> bool {
    if !starts_with_ascii_case_insensitive(haystack, name) {
        return false;
    }
    let next = haystack.as_bytes().get(name.len()).copied();
    matches!(next, Some(b',') | Some(b':'))
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
        assert!(is_persona_mentioned(
            "Hey @Teacher AI what's up?",
            "Teacher AI",
            "teacher-ai"
        ));
    }

    #[test]
    fn test_at_mention_unique_id() {
        assert!(is_persona_mentioned(
            "Hey @teacher-ai what's up?",
            "Teacher AI",
            "teacher-ai"
        ));
    }

    #[test]
    fn test_at_mention_case_insensitive() {
        assert!(is_persona_mentioned(
            "yo @TEACHER AI help",
            "Teacher AI",
            "teacher-ai"
        ));
        assert!(is_persona_mentioned(
            "yo @TEACHER-AI help",
            "Teacher AI",
            "teacher-ai"
        ));
    }

    #[test]
    fn test_direct_address_comma() {
        assert!(is_persona_mentioned(
            "Teacher AI, explain closures",
            "Teacher AI",
            "teacher-ai"
        ));
    }

    #[test]
    fn test_direct_address_colon() {
        assert!(is_persona_mentioned(
            "teacher-ai: what's up",
            "Teacher AI",
            "teacher-ai"
        ));
    }

    #[test]
    fn test_not_mentioned_substring() {
        assert!(!is_persona_mentioned(
            "mentioned the teacher today",
            "Teacher AI",
            "teacher-ai"
        ));
    }

    #[test]
    fn test_not_mentioned_no_at() {
        assert!(!is_persona_mentioned(
            "Teacher AI is great",
            "Teacher AI",
            "teacher-ai"
        ));
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
