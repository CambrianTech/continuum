//! ASCII case-insensitive string helpers — zero-alloc primitives for
//! hot paths that previously reached for `.to_lowercase().contains(...)`
//! and `.to_lowercase().starts_with(...)` (which allocate a `String`
//! sized to the haystack length on every call).
//!
//! Used by [`crate::persona::cognition::PersonaCognitionEngine::is_mentioned`]
//! (cached mention marker check) and
//! [`crate::persona::text_analysis::mention_detection::is_persona_mentioned`]
//! (@mention + direct-address parsing, called once per message per
//! persona per tick from the unified evaluator pre-response gate).
//!
//! Persona names in continuum are ASCII (Helper AI, Teacher AI, etc.),
//! so the ASCII fast path is sufficient for the @mention path. Non-ASCII
//! content bytes compare byte-for-byte and can't false-match an ASCII
//! needle byte: [`u8::eq_ignore_ascii_case`] only folds bytes in the
//! alphabetic ASCII range (0x41-0x5A, 0x61-0x7A) and treats all others
//! literally. Emoji-heavy or unicode-rich chat content stays correct.
//!
//! Per [[rust-prioritize-hyper-efficiency]] and
//! [[optimizing-for-low-end-compounds-on-high-end]]: every alloc you
//! skip in the per-tick path on Mac Intel becomes M5 perceived
//! snappiness. These helpers are the primitive that makes that easy.

/// Return `true` when `haystack` contains `needle`, comparing
/// alphabetic ASCII bytes case-insensitively and all other bytes
/// literally. Zero-allocation. O((haystack_len - needle_len + 1) *
/// needle_len) — naive scan, no preprocessing.
///
/// Replaces the panic-and-alloc-prone idiom:
///   ```ignore
///   haystack.to_lowercase().contains(&needle.to_lowercase())
///   ```
/// which allocates two Strings per call AND folds Unicode (overkill
/// when both inputs are ASCII as they are in continuum's @mention
/// paths).
///
/// Empty needle always matches (mirrors `str::contains("")`). Needle
/// longer than haystack always fails.
pub fn contains_ascii_case_insensitive(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    let h = haystack.as_bytes();
    let n = needle.as_bytes();
    if n.len() > h.len() {
        return false;
    }
    h.windows(n.len()).any(|w| w.eq_ignore_ascii_case(n))
}

/// Return `true` when `haystack` begins with `prefix`, comparing
/// alphabetic ASCII bytes case-insensitively and all other bytes
/// literally. Zero-allocation. O(prefix_len).
///
/// Replaces the alloc-prone idiom:
///   ```ignore
///   haystack.to_lowercase().starts_with(&prefix.to_lowercase())
///   ```
/// which allocates two Strings per call.
///
/// Empty prefix always matches. Prefix longer than haystack always
/// fails.
pub fn starts_with_ascii_case_insensitive(haystack: &str, prefix: &str) -> bool {
    if prefix.is_empty() {
        return true;
    }
    let h = haystack.as_bytes();
    let p = prefix.as_bytes();
    if p.len() > h.len() {
        return false;
    }
    h[..p.len()].eq_ignore_ascii_case(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── contains_ascii_case_insensitive ────────────────────────────────

    #[test]
    fn contains_matches_exact_case() {
        assert!(contains_ascii_case_insensitive("hello world", "hello"));
        assert!(contains_ascii_case_insensitive("hello world", "world"));
        assert!(contains_ascii_case_insensitive("hello world", "lo wo"));
    }

    #[test]
    fn contains_matches_case_insensitively() {
        assert!(contains_ascii_case_insensitive("Hello World", "hello"));
        assert!(contains_ascii_case_insensitive(
            "HELLO WORLD",
            "hello world"
        ));
        // Non-alpha bytes (@) must match literally — alphabetic chars after
        // can still case-fold.
        assert!(contains_ascii_case_insensitive(
            "Yo @HELPER are you",
            "@helper"
        ));
    }

    #[test]
    fn contains_rejects_when_needle_absent() {
        assert!(!contains_ascii_case_insensitive("hello world", "goodbye"));
        assert!(!contains_ascii_case_insensitive(
            "short",
            "much longer needle"
        ));
        // Needle has '@' but haystack doesn't.
        assert!(!contains_ascii_case_insensitive("HEY HELPER", "@helper"));
    }

    #[test]
    fn contains_empty_needle_always_matches() {
        assert!(contains_ascii_case_insensitive("anything", ""));
        assert!(contains_ascii_case_insensitive("", ""));
    }

    #[test]
    fn contains_non_ascii_does_not_false_match_ascii() {
        // 'é' (0xc3 0xa9) shares one byte with no ASCII letter; the second
        // byte (0xa9) is outside alpha-fold range so compares literally
        // and won't match 'e' (0x65).
        assert!(!contains_ascii_case_insensitive("hé", "he"));
        assert!(!contains_ascii_case_insensitive(
            "\u{1F44B} hello",
            "\u{1F44B} world"
        ));
        // ASCII substring inside unicode-rich content still matches.
        assert!(contains_ascii_case_insensitive(
            "\u{1F44B} Helper AI",
            "helper ai"
        ));
    }

    // ─── starts_with_ascii_case_insensitive ─────────────────────────────

    #[test]
    fn starts_with_matches_exact_case() {
        assert!(starts_with_ascii_case_insensitive("hello world", "hello"));
        assert!(starts_with_ascii_case_insensitive("hello", "hello"));
    }

    #[test]
    fn starts_with_matches_case_insensitively() {
        assert!(starts_with_ascii_case_insensitive("HELLO world", "hello"));
        assert!(starts_with_ascii_case_insensitive(
            "Teacher AI, explain",
            "teacher ai"
        ));
        assert!(starts_with_ascii_case_insensitive(
            "Teacher AI: explain",
            "teacher ai"
        ));
    }

    #[test]
    fn starts_with_rejects_substring_not_at_start() {
        // "world" IS in "hello world" but not at the start.
        assert!(!starts_with_ascii_case_insensitive("hello world", "world"));
    }

    #[test]
    fn starts_with_rejects_prefix_longer_than_haystack() {
        assert!(!starts_with_ascii_case_insensitive("hi", "hello"));
    }

    #[test]
    fn starts_with_empty_prefix_always_matches() {
        assert!(starts_with_ascii_case_insensitive("anything", ""));
        assert!(starts_with_ascii_case_insensitive("", ""));
    }

    #[test]
    fn starts_with_non_ascii_does_not_false_match_ascii() {
        assert!(!starts_with_ascii_case_insensitive("\u{1F44B} hi", "hello"));
        // ASCII prefix on unicode content works as expected.
        assert!(starts_with_ascii_case_insensitive(
            "hello \u{1F44B}",
            "hello"
        ));
    }
}
