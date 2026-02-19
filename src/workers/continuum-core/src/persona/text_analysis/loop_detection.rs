//! Loop Detection
//!
//! Per-persona response loop state in DashMap.
//! Replaces static TypeScript Map in PersonaResponseGenerator.
//!
//! Also includes truncated tool call detection.

use std::time::Instant;

use dashmap::DashMap;
use uuid::Uuid;

use super::similarity::jaccard_char_bigram_similarity;

/// Per-persona loop detection state
pub struct LoopDetector {
    /// Map of persona_id → recent response hashes with timestamps
    states: DashMap<Uuid, Vec<ResponseEntry>>,
}

struct ResponseEntry {
    hash: String,
    timestamp: Instant,
}

/// Constants matching the TypeScript originals exactly
const RESPONSE_LOOP_WINDOW_MS: u128 = 600_000; // 10 minutes
const RESPONSE_LOOP_THRESHOLD: usize = 3; // Block after 3 similar responses
const RESPONSE_HASH_LENGTH: usize = 200; // First 200 chars for comparison

impl LoopDetector {
    pub fn new() -> Self {
        Self {
            states: DashMap::new(),
        }
    }

    /// Check if a response is a loop (repeating similar content).
    /// Also records the response in the history.
    /// Returns (is_loop, duplicate_count)
    pub fn check_response_loop(
        &self,
        persona_id: Uuid,
        response_text: &str,
    ) -> (bool, usize) {
        let hash = hash_response(response_text);
        let now = Instant::now();

        let mut entries = self.states.entry(persona_id).or_default();

        // Clean old entries outside window
        entries.retain(|e| now.duration_since(e.timestamp).as_millis() < RESPONSE_LOOP_WINDOW_MS);

        // Count similar responses
        let mut duplicate_count = 0;
        for entry in entries.iter() {
            let similarity = jaccard_char_bigram_similarity(&entry.hash, &hash);
            if similarity > 0.8 {
                duplicate_count += 1;
            }
        }

        // Record this response
        entries.push(ResponseEntry {
            hash,
            timestamp: now,
        });

        let is_loop = duplicate_count >= RESPONSE_LOOP_THRESHOLD;
        (is_loop, duplicate_count)
    }

    /// Clear loop history for a persona
    pub fn clear_history(&self, persona_id: Uuid) {
        self.states.remove(&persona_id);
    }
}

/// Normalize and truncate response text for comparison.
/// Matches TypeScript hashResponse() exactly.
fn hash_response(text: &str) -> String {
    text.to_lowercase()
        .trim()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(RESPONSE_HASH_LENGTH)
        .collect()
}

/// Check for truncated tool calls (open XML tags without closing).
/// DeepSeek's issue: response cut off mid-tool-call.
pub fn has_truncated_tool_call(text: &str) -> bool {
    let has_start = text.contains("<tool_use>") || text.contains("<tool ");
    let has_end = text.contains("</tool_use>") || text.contains("</tool>");
    has_start && !has_end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_response_normalizes() {
        assert_eq!(hash_response("  Hello   World  "), "hello world");
    }

    #[test]
    fn test_hash_response_truncates() {
        let long = "a ".repeat(200);
        let hash = hash_response(&long);
        assert!(hash.len() <= RESPONSE_HASH_LENGTH);
    }

    #[test]
    fn test_no_loop_first_response() {
        let detector = LoopDetector::new();
        let id = Uuid::new_v4();
        let (is_loop, count) = detector.check_response_loop(id, "Hello, how can I help?");
        assert!(!is_loop);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_loop_after_threshold() {
        let detector = LoopDetector::new();
        let id = Uuid::new_v4();

        // Send same response 4 times (threshold is 3)
        detector.check_response_loop(id, "I can help with that!");
        detector.check_response_loop(id, "I can help with that!");
        detector.check_response_loop(id, "I can help with that!");
        let (is_loop, count) = detector.check_response_loop(id, "I can help with that!");
        assert!(is_loop);
        assert!(count >= RESPONSE_LOOP_THRESHOLD);
    }

    #[test]
    fn test_different_responses_no_loop() {
        let detector = LoopDetector::new();
        let id = Uuid::new_v4();

        detector.check_response_loop(id, "First unique response about topic A");
        detector.check_response_loop(id, "Second different response about topic B");
        detector.check_response_loop(id, "Third completely unrelated response about topic C");
        let (is_loop, _) =
            detector.check_response_loop(id, "Fourth response, still different topic D");
        assert!(!is_loop);
    }

    #[test]
    fn test_clear_history() {
        let detector = LoopDetector::new();
        let id = Uuid::new_v4();

        detector.check_response_loop(id, "Repeated response!");
        detector.check_response_loop(id, "Repeated response!");
        detector.check_response_loop(id, "Repeated response!");

        detector.clear_history(id);

        let (is_loop, count) = detector.check_response_loop(id, "Repeated response!");
        assert!(!is_loop);
        assert_eq!(count, 0);
    }

    #[test]
    fn test_truncated_tool_call_detected() {
        assert!(has_truncated_tool_call("Here's my answer <tool_use> some content"));
        assert!(has_truncated_tool_call("Using <tool name=\"search\">query"));
    }

    #[test]
    fn test_complete_tool_call_passes() {
        assert!(!has_truncated_tool_call(
            "<tool_use>search</tool_use> done"
        ));
        assert!(!has_truncated_tool_call("No tool calls at all"));
    }

    #[test]
    fn test_per_persona_isolation() {
        let detector = LoopDetector::new();
        let id_a = Uuid::new_v4();
        let id_b = Uuid::new_v4();

        // Persona A loops
        detector.check_response_loop(id_a, "Same response");
        detector.check_response_loop(id_a, "Same response");
        detector.check_response_loop(id_a, "Same response");

        // Persona B should not be affected
        let (is_loop, _) = detector.check_response_loop(id_b, "Same response");
        assert!(!is_loop);
    }
}
