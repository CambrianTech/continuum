//! Post-inference adequacy check.
//!
//! ONE Rust call replaces N individual text-similarity IPC calls. Given
//! the original message text + a list of recent AI responses, decides
//! whether any prior response already adequately answers the question
//! — used to suppress redundant follow-up replies.
//!
//! Thresholds:
//! - Minimum response length: 100 chars
//! - Minimum similarity: 0.2 (word n-gram Jaccard)
//! - Confidence: similarity + 0.5 (capped at 1.0)
//!
//! Extracted from `evaluator.rs` (continuum#1208).

use crate::persona::text_analysis;
use serde::{Deserialize, Serialize};
use std::time::Instant;
use ts_rs::TS;

/// A recent AI response to check for adequacy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentResponse {
    pub sender_name: String,
    pub text: String,
}

/// Result of the post-inference adequacy check.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/persona/AdequacyResult.ts"
)]
pub struct AdequacyResult {
    pub is_adequate: bool,
    pub confidence: f32,
    pub reason: String,
    /// Name of the AI that already answered (if adequate)
    #[ts(optional)]
    pub responder_name: Option<String>,
    /// How long the check took (microseconds)
    #[ts(type = "number")]
    pub check_time_us: u64,
}

/// Check if any existing AI responses already adequately answer the original question.
pub fn check_response_adequacy(
    original_text: &str,
    responses: &[RecentResponse],
) -> AdequacyResult {
    let start = Instant::now();

    // Pre-compute original text ngrams once — reuse across all response comparisons
    let original_ngrams = text_analysis::build_word_ngrams(original_text);

    for response in responses {
        // Skip short responses (likely not adequate)
        if response.text.len() < 100 {
            continue;
        }

        // Check if response is related to original question
        let response_ngrams = text_analysis::build_word_ngrams(&response.text);
        let similarity = text_analysis::jaccard_from_sets(&original_ngrams, &response_ngrams);

        // Substantial response (>100 chars) that's related to the question (>0.2 similarity)
        if similarity > 0.2 {
            let confidence = (similarity as f32 + 0.5).min(1.0);
            return AdequacyResult {
                is_adequate: true,
                confidence,
                reason: format!(
                    "{} already provided a substantial response ({} chars, {}% related)",
                    response.sender_name,
                    response.text.len(),
                    (similarity * 100.0) as u32
                ),
                responder_name: Some(response.sender_name.clone()),
                check_time_us: start.elapsed().as_micros() as u64,
            };
        }
    }

    AdequacyResult {
        is_adequate: false,
        confidence: 0.0,
        reason: "No adequate responses found".into(),
        responder_name: None,
        check_time_us: start.elapsed().as_micros() as u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adequacy_no_responses() {
        let result = check_response_adequacy("What is Rust?", &[]);
        assert!(!result.is_adequate);
        assert_eq!(result.confidence, 0.0);
    }

    #[test]
    fn test_adequacy_short_response_ignored() {
        let responses = vec![RecentResponse {
            sender_name: "Helper".into(),
            text: "Rust is good.".into(), // < 100 chars
        }];
        let result = check_response_adequacy("What is Rust?", &responses);
        assert!(!result.is_adequate, "Short response should be ignored");
    }

    #[test]
    fn test_adequacy_substantial_related_response() {
        // Jaccard n-gram = |intersection|/|union|. Long responses dilute the score
        // because the union grows much faster than the intersection. Use a focused
        // response that echoes question terms without excessive additional vocabulary.
        let original = "Can someone explain how PersonaGenome activateSkill works with LRU eviction and memory budget for paging adapters in and out?";
        let response_text = "PersonaGenome activateSkill works by checking LRU eviction \
                   scores against memory budget. Adapters with low LRU scores get paged \
                   out to free budget for the new skill adapter being paged in.";
        let sim = text_analysis::jaccard_ngram_similarity(original, response_text);
        let responses = vec![RecentResponse {
            sender_name: "CodeReview AI".into(),
            text: response_text.into(),
        }];
        let result = check_response_adequacy(original, &responses);
        assert!(
            result.is_adequate,
            "Substantial related response should be adequate (similarity={sim:.3})"
        );
        assert!(result.confidence > 0.5);
        assert_eq!(result.responder_name.as_deref(), Some("CodeReview AI"));
    }

    #[test]
    fn test_adequacy_unrelated_long_response() {
        let original = "What is Rust?";
        let responses = vec![RecentResponse {
            sender_name: "Helper".into(),
            text: "The weather today is absolutely wonderful with clear skies and temperatures around \
                   seventy degrees. Perfect conditions for outdoor activities like hiking, swimming, \
                   or simply enjoying a picnic in the park with friends and family members.".into(),
        }];
        let result = check_response_adequacy(original, &responses);
        assert!(
            !result.is_adequate,
            "Unrelated response should not be adequate"
        );
    }

    #[test]
    fn test_adequacy_first_adequate_wins() {
        // Longer question with more terms gives Jaccard more intersection surface area
        let original = "How does Rust handle memory management with ownership borrowing and lifetimes for safe concurrent access?";
        let responses = vec![
            RecentResponse {
                sender_name: "Short AI".into(),
                text: "Ownership.".into(), // Too short (<100 chars)
            },
            RecentResponse {
                sender_name: "First Good AI".into(),
                text: "Rust handle memory management with ownership and borrowing rules. \
                       Lifetimes ensure safe concurrent access. Memory management in Rust \
                       is ownership borrowing and lifetimes working together for safe access."
                    .into(),
            },
            RecentResponse {
                sender_name: "Second Good AI".into(),
                text: "Rust handle memory management with ownership borrowing and lifetimes. \
                       Safe concurrent access is guaranteed by the borrowing rules and lifetimes \
                       for memory management in Rust."
                    .into(),
            },
        ];
        let result = check_response_adequacy(original, &responses);
        assert!(result.is_adequate);
        assert_eq!(
            result.responder_name.as_deref(),
            Some("First Good AI"),
            "First adequate response should win"
        );
    }

    #[test]
    fn test_adequacy_check_is_fast() {
        let original = "What is the meaning of life?";
        let responses: Vec<RecentResponse> = (0..10).map(|i| RecentResponse {
            sender_name: format!("AI-{i}"),
            text: format!("Response number {i} that contains enough text to exceed the minimum character \
                           threshold of one hundred characters to be considered for adequacy checking purposes. \
                           This should be sufficient length."),
        }).collect();
        let result = check_response_adequacy(original, &responses);
        assert!(
            result.check_time_us < 10_000,
            "10 responses should be checked in <10ms, took {}μs",
            result.check_time_us
        );
    }

    #[test]
    fn export_bindings_adequacyresult() {
        let cfg = ts_rs::Config::default();
        AdequacyResult::export_all(&cfg).unwrap();
    }
}
