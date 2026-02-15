//! Garbage Detection
//!
//! Validates AI model output for garbage/gibberish before posting.
//! Direct port from GarbageDetector.ts — single source of truth in Rust.
//!
//! 8 checks: empty → encoding errors → inference errors → unicode garbage →
//! repetition → truncation markers → excessive punctuation → token boundary garbage

use regex::Regex;
use std::collections::HashMap;
use std::sync::LazyLock;

use super::types::{GarbageCheckResult, GarbageReason};

// Pre-compiled regex patterns (compiled once, used forever)
static REPLACEMENT_CHAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"\x{FFFD}").unwrap());
static CONTROL_CHARS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\x00-\x08\x0B\x0C\x0E-\x1F]").unwrap());
static PRINTABLE_ASCII: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\x20-\x7E\n\r\t]").unwrap());
static EMOJI_RANGE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[\x{1F300}-\x{1F9FF}]").unwrap());
// Note: no EXACT_REPEAT regex — Rust regex doesn't support backreferences.
// Repetition detection uses algorithmic sliding window instead (faster).
static PUNCT_CHARS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r#"[.!?,;:'"()\{\}\[\]<>/\\|@#$%^&*~`]"#).unwrap());
static LETTER_CHARS: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[a-zA-Z]").unwrap());
static REPEATED_PUNCT: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[.!?]{5,}").unwrap());
static NON_ASCII_CHAR: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^\x00-\x7F]").unwrap());
// ASCII_LETTER removed — use LETTER_CHARS (identical regex, one source of truth)
static ERROR_PREFIX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)^(error|failed|cannot|unable|timeout|invalid):").unwrap());

// Inference error patterns
static INFERENCE_PATTERNS: LazyLock<Vec<(Regex, &'static str)>> = LazyLock::new(|| {
    vec![
        // Sampling errors (Candle)
        (Regex::new(r"(?i)sampling failed:?\s+").unwrap(), "Sampling failure"),
        (Regex::new(r"(?i)a weight is (negative|invalid|too large)").unwrap(), "Invalid weights"),
        (Regex::new(r"(?i)invalid probability distribution").unwrap(), "Invalid distribution"),
        // Memory errors
        (Regex::new(r"(?i)out of memory:?\s+").unwrap(), "OOM error"),
        (Regex::new(r"(?i)memory allocation failed").unwrap(), "Memory allocation"),
        // Timeout errors
        (Regex::new(r"(?i)generation timed out").unwrap(), "Generation timeout"),
        (Regex::new(r"(?i)request timed out after").unwrap(), "Request timeout"),
        (Regex::new(r"(?i)deadline exceeded").unwrap(), "Deadline exceeded"),
        // Connection errors
        (Regex::new(r"(?i)cannot connect to inference server").unwrap(), "Connection error"),
        (Regex::new(r"(?i)grpc.*unavailable").unwrap(), "gRPC unavailable"),
        // Model errors
        (Regex::new(r"(?i)model not (found|loaded)").unwrap(), "Model not found"),
        (Regex::new(r"(?i)forward pass failed").unwrap(), "Forward pass error"),
        (Regex::new(r"(?i)narrow invalid args").unwrap(), "Tensor shape error"),
        (Regex::new(r"(?i)rope.*position").unwrap(), "RoPE position error"),
        // Generic error patterns
        (Regex::new(r"(?i)this usually means:\s*\n").unwrap(), "Error with help text"),
        (Regex::new(r"(?i)try:\s+\n?•").unwrap(), "Error suggestions"),
    ]
});

/// Run all garbage detection checks on text.
/// Returns on first failure (short-circuit).
pub fn is_garbage(text: &str) -> GarbageCheckResult {
    // Empty / null
    let trimmed = text.trim();
    if trimmed.len() < 5 {
        return GarbageCheckResult {
            is_garbage: true,
            reason: GarbageReason::Empty,
            details: format!("Only {} non-whitespace characters", trimmed.len()),
            score: 1.0,
        };
    }

    // Check order matches TS exactly
    if let Some(r) = check_encoding_errors(text) {
        return r;
    }
    if let Some(r) = check_inference_error(text) {
        return r;
    }
    if let Some(r) = check_unicode_garbage(text) {
        return r;
    }
    if let Some(r) = check_repetition(text) {
        return r;
    }
    if let Some(r) = check_truncation_markers(text) {
        return r;
    }
    if let Some(r) = check_excessive_punctuation(text) {
        return r;
    }
    if let Some(r) = check_token_boundary_garbage(text) {
        return r;
    }

    GarbageCheckResult {
        is_garbage: false,
        reason: GarbageReason::None,
        details: String::new(),
        score: 0.0,
    }
}

fn check_encoding_errors(text: &str) -> Option<GarbageCheckResult> {
    // U+FFFD replacement characters
    let replacement_count = REPLACEMENT_CHAR.find_iter(text).count();
    if replacement_count > 3 {
        return Some(GarbageCheckResult {
            is_garbage: true,
            reason: GarbageReason::EncodingErrors,
            details: format!("{} replacement characters (U+FFFD)", replacement_count),
            score: (replacement_count as f64 / 10.0).min(1.0),
        });
    }

    // Null bytes
    if text.contains('\0') {
        return Some(GarbageCheckResult {
            is_garbage: true,
            reason: GarbageReason::EncodingErrors,
            details: "Contains null bytes".to_string(),
            score: 1.0,
        });
    }

    // Control characters (except newlines, tabs, carriage returns)
    let control_count = CONTROL_CHARS.find_iter(text).count();
    if control_count > 5 {
        return Some(GarbageCheckResult {
            is_garbage: true,
            reason: GarbageReason::EncodingErrors,
            details: format!("{} control characters", control_count),
            score: (control_count as f64 / 10.0).min(1.0),
        });
    }

    None
}

fn check_inference_error(text: &str) -> Option<GarbageCheckResult> {
    for (pattern, label) in INFERENCE_PATTERNS.iter() {
        if pattern.is_match(text) {
            let first_line = text.lines().next().unwrap_or("").chars().take(100).collect::<String>();
            return Some(GarbageCheckResult {
                is_garbage: true,
                reason: GarbageReason::InferenceError,
                details: format!("{}: \"{}...\"", label, first_line),
                score: 1.0,
            });
        }
    }

    // Error-like structure: starts with error keyword + colon
    let trimmed = text.trim();
    if ERROR_PREFIX.is_match(trimmed) {
        let first_line = trimmed.lines().next().unwrap_or("").chars().take(100).collect::<String>();
        return Some(GarbageCheckResult {
            is_garbage: true,
            reason: GarbageReason::InferenceError,
            details: format!("Error prefix: \"{}\"", first_line),
            score: 0.9,
        });
    }

    None
}

fn check_unicode_garbage(text: &str) -> Option<GarbageCheckResult> {
    let total = text.len();
    if total <= 20 {
        return None;
    }

    let printable_count = PRINTABLE_ASCII.find_iter(text).count();
    let non_ascii_ratio = 1.0 - (printable_count as f64 / total as f64);

    if non_ascii_ratio > 0.3 {
        let emoji_count = EMOJI_RANGE.find_iter(text).count();
        let emoji_ratio = emoji_count as f64 / total as f64;

        if emoji_ratio < 0.2 {
            let sample: String = text.chars().take(50).collect();
            return Some(GarbageCheckResult {
                is_garbage: true,
                reason: GarbageReason::UnicodeGarbage,
                details: format!(
                    "{:.1}% non-ASCII: \"{}...\"",
                    non_ascii_ratio * 100.0,
                    sample
                ),
                score: non_ascii_ratio,
            });
        }
    }

    None
}

/// Find a substring of `min_len`+ chars repeated `min_count`+ times consecutively.
/// Scans pattern lengths from `min_len` up to text.len()/min_count (max 200).
/// Returns (pattern, count) on first match.
fn find_consecutive_repeat(text: &str, min_len: usize, min_count: usize) -> Option<(String, usize)> {
    // Work at byte level to avoid UTF-8 char boundary panics.
    // Repetition is byte-identical — no character semantics needed.
    let bytes = text.as_bytes();
    let len = bytes.len();
    if len < min_len * min_count {
        return None;
    }

    let max_pattern = (len / min_count).min(200);
    for pattern_len in min_len..=max_pattern {
        let mut start = 0;
        while start + pattern_len * min_count <= len {
            let pattern = &bytes[start..start + pattern_len];
            let mut count = 1usize;
            let mut pos = start + pattern_len;
            while pos + pattern_len <= len && bytes[pos..pos + pattern_len] == *pattern {
                count += 1;
                pos += pattern_len;
            }
            if count >= min_count {
                return Some((String::from_utf8_lossy(pattern).into_owned(), count));
            }
            start += 1;
        }
    }
    None
}

fn check_repetition(text: &str) -> Option<GarbageCheckResult> {
    // Exact phrase repetition (10+ chars repeated 3+ consecutive times)
    // Algorithmic sliding window — no backreference regex needed.
    if let Some((pattern, count)) = find_consecutive_repeat(text, 10, 3) {
        let preview: String = pattern.chars().take(30).collect();
        return Some(GarbageCheckResult {
            is_garbage: true,
            reason: GarbageReason::Repetition,
            details: format!("\"{}...\" repeated {}x", preview, count),
            score: (count as f64 / 5.0).min(1.0),
        });
    }

    // Word repetition (single word >25% of all words)
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() > 15 {
        let mut word_counts: HashMap<String, usize> = HashMap::new();
        for word in &words {
            let lower = word.to_lowercase();
            if lower.len() > 2 {
                *word_counts.entry(lower).or_insert(0) += 1;
            }
        }

        let mut max_count = 0usize;
        let mut max_word = String::new();
        for (word, count) in &word_counts {
            if *count > max_count {
                max_count = *count;
                max_word = word.clone();
            }
        }

        let repeat_ratio = max_count as f64 / words.len() as f64;
        if repeat_ratio > 0.25 && max_count > 5 {
            return Some(GarbageCheckResult {
                is_garbage: true,
                reason: GarbageReason::Repetition,
                details: format!(
                    "\"{}\" appears {}/{} times ({:.1}%)",
                    max_word,
                    max_count,
                    words.len(),
                    repeat_ratio * 100.0
                ),
                score: repeat_ratio,
            });
        }
    }

    None
}

fn check_truncation_markers(text: &str) -> Option<GarbageCheckResult> {
    let trimmed = text.trim();
    let markers = [
        "[truncated]",
        "...[truncated]",
        "[cut off]",
        "[output truncated]",
        "...",
        "\u{2026}", // ellipsis character
    ];

    for marker in &markers {
        if trimmed == *marker || (trimmed.len() < 20 && trimmed.contains(marker)) {
            return Some(GarbageCheckResult {
                is_garbage: true,
                reason: GarbageReason::TruncationMarker,
                details: format!("Response is only: \"{}\"", trimmed),
                score: 1.0,
            });
        }
    }

    None
}

fn check_excessive_punctuation(text: &str) -> Option<GarbageCheckResult> {
    let punctuation = PUNCT_CHARS.find_iter(text).count();
    let letters = LETTER_CHARS.find_iter(text).count();

    if punctuation > letters && punctuation > 20 {
        return Some(GarbageCheckResult {
            is_garbage: true,
            reason: GarbageReason::ExcessivePunctuation,
            details: format!("{} punctuation vs {} letters", punctuation, letters),
            score: (punctuation as f64 / (letters as f64 + 1.0)).min(1.0),
        });
    }

    // Repeated punctuation sequences (5+ in a row, any longer than 10)
    for m in REPEATED_PUNCT.find_iter(text) {
        if m.as_str().len() > 10 {
            return Some(GarbageCheckResult {
                is_garbage: true,
                reason: GarbageReason::ExcessivePunctuation,
                details: format!(
                    "Repeated punctuation: \"{}...\"",
                    &m.as_str()[..m.as_str().len().min(20)]
                ),
                score: 0.8,
            });
        }
    }

    None
}

fn check_token_boundary_garbage(text: &str) -> Option<GarbageCheckResult> {
    let words: Vec<&str> = text.split_whitespace().filter(|w| !w.is_empty()).collect();
    if words.len() < 5 {
        return None;
    }

    let mut weird_count = 0usize;

    for word in &words {
        if word.len() <= 1 {
            continue;
        }

        let has_lower = word.chars().any(|c| c.is_ascii_lowercase());
        let has_upper = word.chars().any(|c| c.is_ascii_uppercase());
        let starts_upper = word.starts_with(|c: char| c.is_ascii_uppercase());
        let all_upper = word.chars().all(|c| !c.is_ascii_lowercase() || !c.is_alphabetic());

        let normal_case = !has_lower || !has_upper || starts_upper || all_upper;

        // Weird mixed case
        if has_lower && has_upper && !normal_case {
            weird_count += 1;
        }

        // Non-ASCII mixed with ASCII in same word
        let non_ascii = NON_ASCII_CHAR.find_iter(word).count();
        let ascii = LETTER_CHARS.find_iter(word).count();
        if non_ascii > 0 && ascii > 0 && (non_ascii as i32 - ascii as i32).unsigned_abs() < 3 {
            weird_count += 1;
        }
    }

    let weird_ratio = weird_count as f64 / words.len() as f64;
    if weird_ratio > 0.3 && weird_count > 3 {
        return Some(GarbageCheckResult {
            is_garbage: true,
            reason: GarbageReason::TokenBoundaryGarbage,
            details: format!("{}/{} words appear malformed", weird_count, words.len()),
            score: weird_ratio,
        });
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_text_passes() {
        let r = is_garbage("Hello, this is a normal AI response about programming.");
        assert!(!r.is_garbage);
    }

    #[test]
    fn test_empty_text() {
        assert!(is_garbage("").is_garbage);
        assert!(is_garbage("   ").is_garbage);
        assert!(is_garbage("hi").is_garbage);
        assert_eq!(is_garbage("").reason, GarbageReason::Empty);
    }

    #[test]
    fn test_null_bytes() {
        let r = is_garbage("Hello\0World\0Test string here");
        assert!(r.is_garbage);
        assert_eq!(r.reason, GarbageReason::EncodingErrors);
    }

    #[test]
    fn test_replacement_chars() {
        let r = is_garbage("Hello \u{FFFD}\u{FFFD}\u{FFFD}\u{FFFD} broken text here");
        assert!(r.is_garbage);
        assert_eq!(r.reason, GarbageReason::EncodingErrors);
    }

    #[test]
    fn test_inference_error() {
        let r = is_garbage("sampling failed: A weight is negative, too large or not a valid number");
        assert!(r.is_garbage);
        assert_eq!(r.reason, GarbageReason::InferenceError);
    }

    #[test]
    fn test_error_prefix() {
        let r = is_garbage("error: connection refused to inference server at localhost:8080");
        assert!(r.is_garbage);
        assert_eq!(r.reason, GarbageReason::InferenceError);
    }

    #[test]
    fn test_exact_repetition() {
        let r = is_garbage("Hello World! Hello World! Hello World! Hello World!");
        assert!(r.is_garbage);
        assert_eq!(r.reason, GarbageReason::Repetition);
    }

    #[test]
    fn test_word_repetition() {
        // >25% of words being the same, >5 occurrences, >15 words total
        let r = is_garbage("the the the the the the the dog cat bird fish car boat tree house sun moon");
        assert!(r.is_garbage);
        assert_eq!(r.reason, GarbageReason::Repetition);
    }

    #[test]
    fn test_truncation_marker() {
        assert!(is_garbage("[truncated]").is_garbage);
        assert!(is_garbage("...").is_garbage);
        assert!(is_garbage("\u{2026}").is_garbage);
    }

    #[test]
    fn test_excessive_punctuation() {
        let r = is_garbage("???!!!...???!!!...???!!!...???!!!...");
        assert!(r.is_garbage);
        assert_eq!(r.reason, GarbageReason::ExcessivePunctuation);
    }

    #[test]
    fn test_valid_response_with_punctuation() {
        let r = is_garbage("Sure! I can help with that. Here's what you need to know: first, create a file; second, add content.");
        assert!(!r.is_garbage);
    }

    #[test]
    fn test_valid_response_with_code() {
        let r = is_garbage("To fix this, update the function:\n```rust\nfn main() {\n    println!(\"Hello, world!\");\n}\n```");
        assert!(!r.is_garbage);
    }

    #[test]
    fn test_emoji_and_multibyte_no_panic() {
        // Must not panic on multi-byte UTF-8 (emojis = 4 bytes, accented chars = 2 bytes)
        let r = is_garbage("**EVERYONE STOP.** 🛑\n\nThis conversation has completely derailed. Let me reset and explain what happened.");
        assert!(!r.is_garbage);

        let r2 = is_garbage("Hey, I'm trying to use the Rust Sentinel module with Node.js. I've got the café résumé naïve file ready.");
        assert!(!r2.is_garbage);

        // Emoji-heavy but valid response (emojis exempt from unicode garbage)
        let r3 = is_garbage("Great work team! 🎉🚀✨ Let's keep pushing forward on the project. Here are the next steps we should take.");
        assert!(!r3.is_garbage);
    }

    #[test]
    fn test_oom_error() {
        let r = is_garbage("out of memory: failed to allocate 4096 bytes for model weights");
        assert!(r.is_garbage);
        assert_eq!(r.reason, GarbageReason::InferenceError);
    }

    #[test]
    fn test_grpc_unavailable() {
        let r = is_garbage("gRPC service unavailable: connection to localhost:50051 refused");
        assert!(r.is_garbage);
        assert_eq!(r.reason, GarbageReason::InferenceError);
    }
}
