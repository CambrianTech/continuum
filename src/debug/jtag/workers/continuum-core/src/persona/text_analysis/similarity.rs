//! Unified Text Similarity
//!
//! ONE Jaccard implementation for the entire system.
//! Replaces 3 duplicate TypeScript implementations:
//! - PersonaResponseGenerator.calculateSimilarity() (char bigrams)
//! - PersonaResponseGenerator.jaccardSimilarity() (word + bigram)
//! - PersonaMessageEvaluator.computeTextSimilarity() (word + bigram)
//!
//! Also used by loop detection, semantic loop checking, topic detection, etc.

use std::collections::HashSet;
use super::types::{ConversationMessage, SemanticLoopResult};

/// Character bigram Jaccard similarity.
///
/// Computes Jaccard coefficient over character bigram sets.
/// Used for fast loop detection (comparing response hashes).
///
/// Port of PersonaResponseGenerator.calculateSimilarity()
pub fn jaccard_char_bigram_similarity(a: &str, b: &str) -> f64 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    if a == b {
        return 1.0;
    }

    let bigrams_a = char_bigrams(a);
    let bigrams_b = char_bigrams(b);

    let intersection = bigrams_a.intersection(&bigrams_b).count();
    let union = bigrams_a.union(&bigrams_b).count();

    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

/// Word + bigram Jaccard similarity (n-gram).
///
/// Computes Jaccard coefficient over unigrams + bigrams.
/// More semantic than character-level — captures word co-occurrence.
///
/// Port of PersonaResponseGenerator.jaccardSimilarity() and
/// PersonaMessageEvaluator.computeTextSimilarity()
pub fn jaccard_ngram_similarity(text1: &str, text2: &str) -> f64 {
    if text1.is_empty() || text2.is_empty() {
        return 0.0;
    }
    if text1 == text2 {
        return 1.0;
    }

    let set1 = word_ngrams(text1);
    let set2 = word_ngrams(text2);

    if set1.is_empty() || set2.is_empty() {
        return 0.0;
    }

    let intersection = set1.intersection(&set2).count();
    let union = set1.union(&set2).count();

    if union == 0 {
        0.0
    } else {
        intersection as f64 / union as f64
    }
}

/// Check if a response is semantically looping against recent conversation history.
///
/// Compares response text against the last N messages using ngram similarity.
/// Thresholds: WARN at 0.80, BLOCK at 0.95.
///
/// Port of PersonaResponseGenerator.checkSemanticLoop()
pub fn check_semantic_loop(
    response_text: &str,
    history: &[ConversationMessage],
    max_history: usize,
) -> SemanticLoopResult {
    const WARN_THRESHOLD: f64 = 0.80;
    const BLOCK_THRESHOLD: f64 = 0.95;

    if response_text.len() < 50 {
        return SemanticLoopResult {
            should_block: false,
            similarity: 0.0,
            reason: "Response too short for semantic check".into(),
        };
    }

    let recent = if history.len() > max_history {
        &history[history.len() - max_history..]
    } else {
        history
    };

    let mut max_similarity: f64 = 0.0;

    for msg in recent {
        if msg.content.len() < 20 {
            continue;
        }
        let similarity = jaccard_ngram_similarity(response_text, &msg.content);
        if similarity > max_similarity {
            max_similarity = similarity;
        }
    }

    if max_similarity >= BLOCK_THRESHOLD {
        SemanticLoopResult {
            should_block: true,
            similarity: max_similarity,
            reason: format!(
                "{}% similar to recent message",
                (max_similarity * 100.0).round() as u32
            ),
        }
    } else if max_similarity >= WARN_THRESHOLD {
        SemanticLoopResult {
            should_block: false,
            similarity: max_similarity,
            reason: "Similar but allowing for autonomy".into(),
        }
    } else {
        SemanticLoopResult {
            should_block: false,
            similarity: max_similarity,
            reason: "Low similarity".into(),
        }
    }
}

// ============================================================================
// Internal helpers
// ============================================================================

/// Extract character bigrams from a string.
fn char_bigrams(s: &str) -> HashSet<String> {
    let chars: Vec<char> = s.chars().collect();
    let mut bigrams = HashSet::new();
    if chars.len() < 2 {
        return bigrams;
    }
    for i in 0..chars.len() - 1 {
        let mut bigram = String::with_capacity(8);
        bigram.push(chars[i]);
        bigram.push(chars[i + 1]);
        bigrams.insert(bigram);
    }
    bigrams
}

/// Tokenize text into lowercase words, then build unigram + bigram set.
fn word_ngrams(text: &str) -> HashSet<String> {
    let lower = text.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();

    let mut ngrams = HashSet::new();

    // Unigrams
    for word in &words {
        ngrams.insert((*word).to_string());
    }

    // Bigrams (space-separated, matching TS implementation)
    for i in 0..words.len().saturating_sub(1) {
        ngrams.insert(format!("{} {}", words[i], words[i + 1]));
    }

    ngrams
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ---- Character bigram tests ----

    #[test]
    fn test_char_bigram_identical() {
        assert_eq!(jaccard_char_bigram_similarity("hello", "hello"), 1.0);
    }

    #[test]
    fn test_char_bigram_empty() {
        assert_eq!(jaccard_char_bigram_similarity("", "hello"), 0.0);
        assert_eq!(jaccard_char_bigram_similarity("hello", ""), 0.0);
        assert_eq!(jaccard_char_bigram_similarity("", ""), 0.0);
    }

    #[test]
    fn test_char_bigram_completely_different() {
        let sim = jaccard_char_bigram_similarity("abc", "xyz");
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_char_bigram_partial_overlap() {
        // "hello" bigrams: {"he", "el", "ll", "lo"} = 4
        // "help"  bigrams: {"he", "el", "lp"} = 3
        // intersection: {"he", "el"} = 2
        // union: {"he", "el", "ll", "lo", "lp"} = 5
        // Jaccard = 2/5 = 0.4
        let sim = jaccard_char_bigram_similarity("hello", "help");
        assert!((sim - 0.4).abs() < 1e-10);
    }

    #[test]
    fn test_char_bigram_single_char() {
        // Single char has no bigrams → empty set → 0.0
        assert_eq!(jaccard_char_bigram_similarity("a", "b"), 0.0);
    }

    // ---- Word n-gram tests ----

    #[test]
    fn test_ngram_identical() {
        assert_eq!(jaccard_ngram_similarity("hello world", "hello world"), 1.0);
    }

    #[test]
    fn test_ngram_empty() {
        assert_eq!(jaccard_ngram_similarity("", "hello"), 0.0);
        assert_eq!(jaccard_ngram_similarity("hello", ""), 0.0);
    }

    #[test]
    fn test_ngram_case_insensitive() {
        assert_eq!(
            jaccard_ngram_similarity("Hello World", "hello world"),
            1.0
        );
    }

    #[test]
    fn test_ngram_partial_overlap() {
        // "the cat sat" → unigrams: {the, cat, sat}, bigrams: {the cat, cat sat} = 5
        // "the dog sat" → unigrams: {the, dog, sat}, bigrams: {the dog, dog sat} = 5
        // intersection: {the, sat} = 2
        // union: {the, cat, sat, dog, the cat, cat sat, the dog, dog sat} = 8
        // Jaccard = 2/8 = 0.25
        let sim = jaccard_ngram_similarity("the cat sat", "the dog sat");
        assert!((sim - 0.25).abs() < 1e-10);
    }

    #[test]
    fn test_ngram_completely_different() {
        let sim = jaccard_ngram_similarity("alpha beta", "gamma delta");
        assert_eq!(sim, 0.0);
    }

    #[test]
    fn test_ngram_punctuation_stripped() {
        // Punctuation is split boundary, so "hello, world!" → words: ["hello", "world"]
        let sim = jaccard_ngram_similarity("hello, world!", "hello world");
        assert_eq!(sim, 1.0);
    }

    #[test]
    fn test_ngram_known_value() {
        // "I like cats" → unigrams: {i, like, cats}, bigrams: {i like, like cats} = 5
        // "I like dogs" → unigrams: {i, like, dogs}, bigrams: {i like, like dogs} = 5
        // intersection: {i, like, i like} = 3
        // union: {i, like, cats, dogs, i like, like cats, like dogs} = 7
        // Jaccard = 3/7 ≈ 0.42857
        let sim = jaccard_ngram_similarity("I like cats", "I like dogs");
        assert!((sim - 3.0 / 7.0).abs() < 1e-10);
    }

    // ---- Semantic loop tests ----

    #[test]
    fn test_semantic_loop_short_response() {
        let result = check_semantic_loop("hi", &[], 10);
        assert!(!result.should_block);
        assert_eq!(result.similarity, 0.0);
    }

    #[test]
    fn test_semantic_loop_no_history() {
        let long_response = "This is a sufficiently long response that should pass the length check for semantic analysis";
        let result = check_semantic_loop(long_response, &[], 10);
        assert!(!result.should_block);
        assert_eq!(result.similarity, 0.0);
    }

    #[test]
    fn test_semantic_loop_identical_blocks() {
        let response = "This is a sufficiently long response that we will also put in the history to test blocking behavior";
        let history = vec![ConversationMessage {
            role: "assistant".into(),
            content: response.into(),
            name: None,
        }];

        let result = check_semantic_loop(response, &history, 10);
        assert!(result.should_block);
        assert_eq!(result.similarity, 1.0);
    }

    #[test]
    fn test_semantic_loop_different_content() {
        let response = "This is a brand new response about artificial intelligence and machine learning models";
        let history = vec![ConversationMessage {
            role: "user".into(),
            content: "Tell me about the weather forecast for tomorrow in the northern hemisphere".into(),
            name: None,
        }];

        let result = check_semantic_loop(response, &history, 10);
        assert!(!result.should_block);
        assert!(result.similarity < 0.5);
    }

    #[test]
    fn test_semantic_loop_respects_max_history() {
        let response = "This is a sufficiently long response that appears in old history but not recent";
        let mut history = Vec::new();
        // Identical message at position 0
        history.push(ConversationMessage {
            role: "assistant".into(),
            content: response.into(),
            name: None,
        });
        // 15 different messages after
        for i in 0..15 {
            history.push(ConversationMessage {
                role: "user".into(),
                content: format!("Completely unrelated message number {} about topic {}", i, i * 7),
                name: None,
            });
        }

        // With max_history=10, the identical message at position 0 is outside the window
        let result = check_semantic_loop(response, &history, 10);
        assert!(!result.should_block);
    }
}
