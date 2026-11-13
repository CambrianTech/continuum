//! Garbage Detection
//!
//! Validates AI model output for garbage/gibberish before posting.
//! Direct port from GarbageDetector.ts — single source of truth in Rust.
//!
//! 8 checks: empty → encoding errors → inference errors → unicode garbage →
//! repetition → truncation markers → excessive punctuation → token boundary garbage
//!
//! Every pattern is a hand-written matcher over [`crate::scan`] (they were regexes
//! compiled with `.unwrap()`); the tests pin each against the regex it replaced.

use std::collections::{HashMap, HashSet};

use super::scan::{capitalized_word, fold_occurrences, fold_prefix, is_digit, is_space, line_start_matches, take_one, take_while};
use super::types::{GarbageCheckResult, GarbageReason};

/// `[\x00-\x08\x0B\x0C\x0E-\x1F]`: control characters other than tab, newline and CR.
fn is_control(c: char) -> bool {
    matches!(c, '\0'..='\x08' | '\x0B' | '\x0C' | '\x0E'..='\x1F')
}

/// `[\x20-\x7E\n\r\t]`: printable ASCII and the three whitespace controls.
fn is_printable_ascii(c: char) -> bool {
    matches!(c, '\x20'..='\x7E' | '\n' | '\r' | '\t')
}

/// `[\x{1F300}-\x{1F9FF}]`: the pictograph and emoji blocks.
fn is_emoji(c: char) -> bool {
    matches!(c, '\u{1F300}'..='\u{1F9FF}')
}

/// `[.!?,;:'"(){}\[\]<>/\\|@#$%^&*~\`]`: the punctuation the excessive-punctuation check counts.
fn is_counted_punctuation(c: char) -> bool {
    matches!(
        c,
        '.' | '!' | '?' | ',' | ';' | ':' | '\'' | '"' | '(' | ')' | '{' | '}' | '[' | ']' | '<' | '>' | '/'
            | '\\' | '|' | '@' | '#' | '$' | '%' | '^' | '&' | '*' | '~' | '`'
    )
}

/// `[.!?]`: sentence-ending punctuation, whose long runs read as garbage.
fn is_terminal_punctuation(c: char) -> bool {
    matches!(c, '.' | '!' | '?')
}

/// What must follow an inference-error phrase for the pattern to match.
#[derive(Clone, Copy)]
enum Tail {
    /// Nothing.
    None,
    /// `:?\s+` — whitespace, optionally after one colon.
    SpaceAfterOptionalColon,
    /// `\s*\n` — a whitespace run that contains a newline.
    SpaceRunWithNewline,
    /// `\s+\n?•` — a whitespace run, then a bullet.
    SpaceRunThenBullet,
}

impl Tail {
    fn accepts(self, rest: &str) -> bool {
        match self {
            Tail::None => true,
            Tail::SpaceAfterOptionalColon => {
                take_one(rest, is_space).is_some()
                    || take_one(rest, |c| c == ':').is_some_and(|after| take_one(after, is_space).is_some())
            }
            Tail::SpaceRunWithNewline => {
                let (_, after) = take_while(rest, is_space);
                rest[..rest.len() - after.len()].contains('\n')
            }
            Tail::SpaceRunThenBullet => {
                let (len, after) = take_while(rest, is_space);
                len > 0 && after.starts_with('•')
            }
        }
    }
}

/// An inference-error signature, matched case-insensitively (`(?i)`) anywhere in the text.
enum InferenceSignature {
    /// Any of these phrases, followed by the tail.
    Phrase(&'static [&'static str], Tail),
    /// `first.*second`: the first phrase, then the second later on the same line.
    SameLine(&'static str, &'static str),
}

impl InferenceSignature {
    fn is_in(&self, text: &str) -> bool {
        match self {
            InferenceSignature::Phrase(phrases, tail) => phrases.iter().any(|phrase| {
                fold_occurrences(text, phrase).any(|(_, end)| tail.accepts(&text[end..]))
            }),
            InferenceSignature::SameLine(first, second) => text.split('\n').any(|line| {
                fold_occurrences(line, first)
                    .next()
                    .is_some_and(|(_, end)| fold_occurrences(&line[end..], second).next().is_some())
            }),
        }
    }
}

/// Inference error signatures and their labels, checked in order (the first that matches
/// names the failure).
const INFERENCE_PATTERNS: [(InferenceSignature, &str); 16] = [
    // Sampling errors (Candle)
    (InferenceSignature::Phrase(&["sampling failed"], Tail::SpaceAfterOptionalColon), "Sampling failure"),
    (
        InferenceSignature::Phrase(&["a weight is negative", "a weight is invalid", "a weight is too large"], Tail::None),
        "Invalid weights",
    ),
    (InferenceSignature::Phrase(&["invalid probability distribution"], Tail::None), "Invalid distribution"),
    // Memory errors
    (InferenceSignature::Phrase(&["out of memory"], Tail::SpaceAfterOptionalColon), "OOM error"),
    (InferenceSignature::Phrase(&["memory allocation failed"], Tail::None), "Memory allocation"),
    // Timeout errors
    (InferenceSignature::Phrase(&["generation timed out"], Tail::None), "Generation timeout"),
    (InferenceSignature::Phrase(&["request timed out after"], Tail::None), "Request timeout"),
    (InferenceSignature::Phrase(&["deadline exceeded"], Tail::None), "Deadline exceeded"),
    // Connection errors
    (InferenceSignature::Phrase(&["cannot connect to inference server"], Tail::None), "Connection error"),
    (InferenceSignature::SameLine("grpc", "unavailable"), "gRPC unavailable"),
    // Model errors
    (InferenceSignature::Phrase(&["model not found", "model not loaded"], Tail::None), "Model not found"),
    (InferenceSignature::Phrase(&["forward pass failed"], Tail::None), "Forward pass error"),
    (InferenceSignature::Phrase(&["narrow invalid args"], Tail::None), "Tensor shape error"),
    (InferenceSignature::SameLine("rope", "position"), "RoPE position error"),
    // Generic error patterns
    (InferenceSignature::Phrase(&["this usually means:"], Tail::SpaceRunWithNewline), "Error with help text"),
    (InferenceSignature::Phrase(&["try:"], Tail::SpaceRunThenBullet), "Error suggestions"),
];

/// `(?i)^(error|failed|cannot|unable|timeout|invalid):` — an error-shaped first word.
fn has_error_prefix(trimmed: &str) -> bool {
    ["error", "failed", "cannot", "unable", "timeout", "invalid"]
        .iter()
        .any(|word| fold_prefix(trimmed, word).is_some_and(|len| trimmed[len..].starts_with(':')))
}

/// `\d{lo,hi}` followed by something that is not a digit: the run of digits at the start of
/// `s` when its length is within bounds, and the rest after it.
fn digits(s: &str, lo: usize, hi: usize) -> Option<&str> {
    let (len, rest) = take_while(s, is_digit);
    (lo..=hi).contains(&len).then_some(rest)
}

/// `\s+`: at least one whitespace char, all of the run consumed.
fn spaces(s: &str) -> Option<&str> {
    let (len, rest) = take_while(s, is_space);
    (len > 0).then_some(rest)
}

/// `(?:\s+[A-Z][a-zA-Z]+)*`: further capitalized words, each after whitespace.
fn more_words(mut s: &str) -> &str {
    while let Some(rest) = spaces(s).and_then(capitalized_word) {
        s = rest;
    }
    s
}

/// At a line start: `\s*\d{1,4}[/-]\d{1,2}[/-]\d{1,4}\s+\d{1,2}:\d{2}\s+[A-Z][a-zA-Z]+(?:\s+[A-Z][a-zA-Z]+)*:\s`
/// ("02/16/2026 09:51 General AI: text"), returning the matched byte length.
fn timestamp_speaker_at(s: &str) -> Option<usize> {
    let date_sep = |c| c == '/' || c == '-';
    let (_, rest) = take_while(s, is_space);
    let rest = take_one(digits(rest, 1, 4)?, date_sep)?;
    let rest = take_one(digits(rest, 1, 2)?, date_sep)?;
    let rest = spaces(digits(rest, 1, 4)?)?;
    let rest = take_one(digits(rest, 1, 2)?, |c| c == ':')?;
    let rest = spaces(digits(rest, 2, 2)?)?;
    let rest = more_words(capitalized_word(rest)?);
    let rest = take_one(take_one(rest, |c| c == ':')?, is_space)?;
    Some(s.len() - rest.len())
}

/// At a line start: `[A-Z][a-zA-Z]+\s+[A-Z][a-zA-Z]+(?:\s+[A-Z][a-zA-Z]+)*:\s+\S`
/// ("General AI: text"), returning the matched byte length.
fn multi_word_speaker_at(s: &str) -> Option<usize> {
    let rest = capitalized_word(spaces(capitalized_word(s)?)?)?;
    let rest = take_one(more_words(rest), |c| c == ':')?;
    let rest = take_one(spaces(rest)?, |c| !is_space(c))?;
    Some(s.len() - rest.len())
}

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
    if let Some(r) = check_fabricated_conversation(text) {
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
    let replacement_count = text.chars().filter(|&c| c == '\u{FFFD}').count();
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
    let control_count = text.chars().filter(|&c| is_control(c)).count();
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
    for (signature, label) in INFERENCE_PATTERNS.iter() {
        if signature.is_in(text) {
            let first_line = text
                .lines()
                .next()
                .unwrap_or("")
                .chars()
                .take(100)
                .collect::<String>();
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
    if has_error_prefix(trimmed) {
        let first_line = trimmed
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(100)
            .collect::<String>();
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

    let printable_count = text.chars().filter(|&c| is_printable_ascii(c)).count();
    let non_ascii_ratio = 1.0 - (printable_count as f64 / total as f64);

    if non_ascii_ratio > 0.3 {
        let emoji_count = text.chars().filter(|&c| is_emoji(c)).count();
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
fn find_consecutive_repeat(
    text: &str,
    min_len: usize,
    min_count: usize,
) -> Option<(String, usize)> {
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
    let punctuation = text.chars().filter(|&c| is_counted_punctuation(c)).count();
    let letters = text.chars().filter(char::is_ascii_alphabetic).count();

    if punctuation > letters && punctuation > 20 {
        return Some(GarbageCheckResult {
            is_garbage: true,
            reason: GarbageReason::ExcessivePunctuation,
            details: format!("{} punctuation vs {} letters", punctuation, letters),
            score: (punctuation as f64 / (letters as f64 + 1.0)).min(1.0),
        });
    }

    // Repeated punctuation sequences: the first run of `.`, `!`, `?` longer than 10.
    if let Some(run) = terminal_punctuation_runs(text).find(|run| run.len() > 10) {
        return Some(GarbageCheckResult {
            is_garbage: true,
            reason: GarbageReason::ExcessivePunctuation,
            details: format!("Repeated punctuation: \"{}...\"", &run[..run.len().min(20)]),
            score: 0.8,
        });
    }

    None
}

/// Every maximal run of `[.!?]` in `text`, in order. (The pattern this replaced,
/// `[.!?]{5,}`, matched only runs of five or more; the one caller wants runs longer than
/// ten, so the shorter ones it would have skipped cannot change the answer.)
fn terminal_punctuation_runs(text: &str) -> impl Iterator<Item = &str> {
    text.split(|c: char| !is_terminal_punctuation(c)).filter(|run| !run.is_empty())
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
        let all_upper = word
            .chars()
            .all(|c| !c.is_ascii_lowercase() || !c.is_alphabetic());

        let normal_case = !has_lower || !has_upper || starts_upper || all_upper;

        // Weird mixed case
        if has_lower && has_upper && !normal_case {
            weird_count += 1;
        }

        // Non-ASCII mixed with ASCII in same word
        let non_ascii = word.chars().filter(|c| !c.is_ascii()).count();
        let ascii = word.chars().filter(char::is_ascii_alphabetic).count();
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

/// Detect fabricated multi-party conversations within a single response.
///
/// LLMs (especially small local models) sometimes generate entire conversations
/// with multiple speaker entries instead of responding as themselves. E.g.:
///   "02/16/2026 09:51 General AI: What's the best way to...
///    02/16/2026 09:51 Local Assistant: You can use..."
///
/// Two checks:
/// 1. Timestamped speaker lines (highly specific, zero false positives)
/// 2. Multi-word speaker prefixes with 2+ distinct names (e.g. "General AI:", "Local Assistant:")
fn check_fabricated_conversation(text: &str) -> Option<GarbageCheckResult> {
    // Only check responses with enough content to contain a fabricated conversation
    if text.len() < 60 {
        return None;
    }

    // Check 1: Timestamped speaker lines — "MM/DD/YYYY HH:MM Name: text"
    let timestamp_count = line_start_matches(text, timestamp_speaker_at).len();
    if timestamp_count >= 3 {
        return Some(GarbageCheckResult {
            is_garbage: true,
            reason: GarbageReason::FabricatedConversation,
            details: format!(
                "{} timestamped speaker lines in one response",
                timestamp_count
            ),
            score: (timestamp_count as f64 / 5.0).min(1.0),
        });
    }

    // Check 2: Multi-word speaker prefixes with distinct names
    // "General AI: text", "Local Assistant: text" — requires 2+ capitalized words before ":"
    let mut speakers: HashSet<String> = HashSet::new();
    let mut speaker_line_count = 0usize;
    for range in line_start_matches(text, multi_word_speaker_at) {
        let matched = &text[range];
        if let Some(colon_pos) = matched.find(':') {
            let name = matched[..colon_pos].trim().to_string();
            speakers.insert(name);
            speaker_line_count += 1;
        }
    }

    if speaker_line_count >= 4 && speakers.len() >= 2 {
        let sample_names: Vec<&String> = speakers.iter().take(3).collect();
        return Some(GarbageCheckResult {
            is_garbage: true,
            reason: GarbageReason::FabricatedConversation,
            details: format!(
                "{} speaker lines from {} speakers ({})",
                speaker_line_count,
                speakers.len(),
                sample_names
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            score: (speaker_line_count as f64 / 6.0).min(1.0),
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
        let r =
            is_garbage("sampling failed: A weight is negative, too large or not a valid number");
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
        let r = is_garbage(
            "the the the the the the the dog cat bird fish car boat tree house sun moon",
        );
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

    #[test]
    fn test_fabricated_conversation_with_timestamps() {
        let fabricated = r#"02/16/2026 09:51 General AI: What's the best way to optimize code?
02/16/2026 09:51 Local Assistant: You can use caching and memoization.
02/16/2026 09:51 General AI: That's a good start, what about frameworks?
02/16/2026 09:51 Local Assistant: Yes, frameworks can simplify the process."#;
        let r = is_garbage(fabricated);
        assert!(r.is_garbage);
        assert_eq!(r.reason, GarbageReason::FabricatedConversation);
    }

    #[test]
    fn test_fabricated_conversation_multi_word_speakers() {
        let fabricated = "General AI: Hello there!\nLocal Assistant: Hi, how can I help?\nGeneral AI: I need some advice.\nLocal Assistant: Sure, what about?\nGeneral AI: Performance tuning.";
        let r = is_garbage(fabricated);
        assert!(r.is_garbage);
        assert_eq!(r.reason, GarbageReason::FabricatedConversation);
    }

    #[test]
    fn test_normal_response_not_fabricated() {
        // A normal response mentioning people should NOT be flagged
        let r = is_garbage("I agree with what was discussed earlier. Here are my thoughts on the topic of performance optimization and code readability.");
        assert!(!r.is_garbage);
    }

    #[test]
    fn test_single_speaker_prefix_not_fabricated() {
        // A single "Name:" at the start is handled by response_cleaning, not garbage detection
        let r =
            is_garbage("General AI: I think we should use caching for this particular use case.");
        assert!(!r.is_garbage);
    }

    #[test]
    fn test_list_with_colons_not_fabricated() {
        // A list with colons should NOT be flagged
        let r = is_garbage("Here are some tips:\n- Performance: Use caching\n- Readability: Use clear names\n- Testing: Write unit tests\n- Documentation: Keep it updated");
        assert!(!r.is_garbage);
    }

    /// The hand matchers against the regexes they replaced: same answer on every string of
    /// a corpus built from the tokens each pattern turns on (case-folded letters, Unicode
    /// digits and spaces, newlines inside runs, near-misses).
    mod matches_the_replaced_regex {
        use super::*;
        use crate::scan::tests::corpus;
        use regex::Regex;

        const INFERENCE_REGEXES: [&str; 16] = [
            r"(?i)sampling failed:?\s+",
            r"(?i)a weight is (negative|invalid|too large)",
            r"(?i)invalid probability distribution",
            r"(?i)out of memory:?\s+",
            r"(?i)memory allocation failed",
            r"(?i)generation timed out",
            r"(?i)request timed out after",
            r"(?i)deadline exceeded",
            r"(?i)cannot connect to inference server",
            r"(?i)grpc.*unavailable",
            r"(?i)model not (found|loaded)",
            r"(?i)forward pass failed",
            r"(?i)narrow invalid args",
            r"(?i)rope.*position",
            r"(?i)this usually means:\s*\n",
            r"(?i)try:\s+\n?•",
        ];

        // what this catches: an inference signature drifting from its regex (the optional
        // colon, the newline inside a whitespace run, `.*` crossing a line, a folded
        // KELVIN SIGN or long s standing in for a letter).
        #[test]
        fn inference_signatures() {
            let tokens = [
                "sampling failed", "SAMPLING FAILED", "ſampling failed", "out of memory", "memory",
                "a weight is ", "negative", "too large", "invalid", "model not ", "found", "loaded",
                "grpc", "GRPC", "unavailable", "rope", "position", "this usually means:", "try:",
                "•", ":", " ", "\n", "\t", "\u{2003}", "x", "deadline exceeded", "narrow invalid args",
                "forward pass failed", "generation timed out", "request timed out after",
                "cannot connect to inference server", "invalid probability distribution",
                "memory allocation failed", "\u{212A}",
            ];
            let regexes: Vec<Regex> = INFERENCE_REGEXES.iter().map(|r| Regex::new(r).unwrap()).collect();
            for text in corpus(&tokens, 30_000, 6) {
                for ((signature, label), re) in INFERENCE_PATTERNS.iter().zip(&regexes) {
                    assert_eq!(signature.is_in(&text), re.is_match(&text), "{label}: {text:?}");
                }
            }
        }

        // what this catches: the error-prefix check drifting from its anchored regex.
        #[test]
        fn error_prefix() {
            let re = Regex::new(r"(?i)^(error|failed|cannot|unable|timeout|invalid):").unwrap();
            let tokens = ["error", "ERROR", "Failed", "cannot", "unable", "timeout", "invalid", ":", " ", "x", "\n"];
            for text in corpus(&tokens, 5_000, 4) {
                assert_eq!(has_error_prefix(&text), re.is_match(&text), "{text:?}");
            }
        }

        // what this catches: the fabricated-conversation scans drifting from their
        // multiline find_iter (digit runs past their bounds, Unicode digits, whitespace
        // runs crossing lines, the trailing `\S` consuming the next line's first char).
        #[test]
        fn fabricated_speaker_lines() {
            let timestamp = Regex::new(r"(?m)^\s*\d{1,4}[/-]\d{1,2}[/-]\d{1,4}\s+\d{1,2}:\d{2}\s+[A-Z][a-zA-Z]+(?:\s+[A-Z][a-zA-Z]+)*:\s").unwrap();
            let multi = Regex::new(r"(?m)^[A-Z][a-zA-Z]+\s+[A-Z][a-zA-Z]+(?:\s+[A-Z][a-zA-Z]+)*:\s+\S").unwrap();
            let tokens = [
                "02/16/2026 ", "2026-02-16 ", "12345/1/1 ", "1/123/1 ", "٠٢/١٦/٢٠٢٦ ", "09:51 ", "9:5 ", "09:511 ",
                "General ", "AI", "Local Assistant", ": ", ":", ":\n", "\n", " ", "x", "Note: ", "General AI: hi\n", "é",
            ];
            for text in corpus(&tokens, 30_000, 8) {
                let ts: Vec<_> = timestamp.find_iter(&text).map(|m| m.range()).collect();
                assert_eq!(line_start_matches(&text, timestamp_speaker_at), ts, "timestamp {text:?}");
                let mw: Vec<_> = multi.find_iter(&text).map(|m| m.range()).collect();
                assert_eq!(line_start_matches(&text, multi_word_speaker_at), mw, "multi-word {text:?}");
            }
        }

        // what this catches: a char-class predicate disagreeing with its class on any char
        // of the BMP and the emoji planes, or the punctuation runs disagreeing with
        // `[.!?]{5,}` on which run is the first longer than ten.
        #[test]
        fn char_classes_and_punctuation_runs() {
            let all: String = (0u32..0x2_0000).filter_map(char::from_u32).collect();
            let class = |re: &str| -> Vec<char> {
                Regex::new(re).unwrap().find_iter(&all).flat_map(|m| m.as_str().chars()).collect()
            };
            let ours = |pred: fn(char) -> bool| -> Vec<char> { all.chars().filter(|&c| pred(c)).collect() };
            assert_eq!(ours(is_control), class(r"[\x00-\x08\x0B\x0C\x0E-\x1F]"));
            assert_eq!(ours(is_printable_ascii), class(r"[\x20-\x7E\n\r\t]"));
            assert_eq!(ours(is_emoji), class(r"[\x{1F300}-\x{1F9FF}]"));
            assert_eq!(ours(is_counted_punctuation), class(r#"[.!?,;:'"()\{\}\[\]<>/\\|@#$%^&*~`]"#));
            assert_eq!(ours(|c| c.is_ascii_alphabetic()), class(r"[a-zA-Z]"));
            assert_eq!(ours(|c| !c.is_ascii()), class(r"[^\x00-\x7F]"));

            let repeated = Regex::new(r"[.!?]{5,}").unwrap();
            for text in corpus(&[".", "!", "?", "x", " ", "....."], 10_000, 16) {
                let want = repeated.find_iter(&text).map(|m| m.as_str()).find(|run| run.len() > 10);
                assert_eq!(terminal_punctuation_runs(&text).find(|run| run.len() > 10), want, "{text:?}");
            }
        }
    }
}
