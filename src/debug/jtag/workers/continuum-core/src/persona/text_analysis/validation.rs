//! Combined Validation — orchestrates all gates into one ValidationResult.
//!
//! Composes garbage detection, loop detection, truncated tool call detection,
//! and semantic loop detection into a single result struct.
//! Called by the cognition module's `validate-response` handler.

use super::{
    is_garbage, has_truncated_tool_call, check_semantic_loop,
    GarbageCheckResult, GarbageReason, SemanticLoopResult, ValidationResult,
    ConversationMessage, LoopDetector,
};
use uuid::Uuid;

/// Run all 4 validation gates and return a combined result.
///
/// Short-circuits on first failure (garbage → loop → truncated → semantic).
pub fn validate_response(
    response_text: &str,
    persona_id: Uuid,
    has_tool_calls: bool,
    conversation_history: &[ConversationMessage],
    loop_detector: &LoopDetector,
) -> ValidationResult {
    let start = std::time::Instant::now();

    // Gate 1: Garbage detection (skip if has native tool calls — empty text + tools is valid)
    let garbage_result = if has_tool_calls {
        GarbageCheckResult {
            is_garbage: false,
            reason: GarbageReason::None,
            details: String::new(),
            score: 0.0,
        }
    } else {
        is_garbage(response_text)
    };

    if garbage_result.is_garbage {
        return failed("garbage", garbage_result, start);
    }

    // Gate 2: Response loop detection (skip if has tool calls)
    let (is_loop, dup_count) = if has_tool_calls {
        (false, 0)
    } else {
        loop_detector.check_response_loop(persona_id, response_text)
    };

    if is_loop {
        return ValidationResult {
            passed: false,
            gate_failed: Some("response_loop".to_string()),
            garbage_result,
            is_response_loop: true,
            loop_duplicate_count: dup_count as u64,
            has_truncated_tool_call: false,
            semantic_result: SemanticLoopResult::none(),
            total_time_us: start.elapsed().as_micros() as u64,
        };
    }

    // Gate 3: Truncated tool call detection
    let truncated = has_truncated_tool_call(response_text);
    if truncated {
        return ValidationResult {
            passed: false,
            gate_failed: Some("truncated_tool_call".to_string()),
            garbage_result,
            is_response_loop: false,
            loop_duplicate_count: dup_count as u64,
            has_truncated_tool_call: true,
            semantic_result: SemanticLoopResult::none(),
            total_time_us: start.elapsed().as_micros() as u64,
        };
    }

    // Gate 4: Semantic loop detection
    let semantic_result = if conversation_history.is_empty() {
        SemanticLoopResult {
            should_block: false,
            similarity: 0.0,
            reason: "No conversation history provided".to_string(),
        }
    } else {
        check_semantic_loop(response_text, conversation_history, 10)
    };

    let passed = !semantic_result.should_block;
    let gate_failed = if semantic_result.should_block {
        Some("semantic_loop".to_string())
    } else {
        None
    };

    ValidationResult {
        passed,
        gate_failed,
        garbage_result,
        is_response_loop: false,
        loop_duplicate_count: dup_count as u64,
        has_truncated_tool_call: false,
        semantic_result,
        total_time_us: start.elapsed().as_micros() as u64,
    }
}

/// Build a failed ValidationResult (short-circuit helper for garbage gate).
fn failed(gate: &str, garbage_result: GarbageCheckResult, start: std::time::Instant) -> ValidationResult {
    ValidationResult {
        passed: false,
        gate_failed: Some(gate.to_string()),
        garbage_result,
        is_response_loop: false,
        loop_duplicate_count: 0,
        has_truncated_tool_call: false,
        semantic_result: SemanticLoopResult::none(),
        total_time_us: start.elapsed().as_micros() as u64,
    }
}

impl SemanticLoopResult {
    /// A "not blocked" result for short-circuit cases.
    pub fn none() -> Self {
        Self {
            should_block: false,
            similarity: 0.0,
            reason: String::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_detector() -> LoopDetector {
        LoopDetector::new()
    }

    #[test]
    fn test_clean_response_passes() {
        let detector = make_detector();
        let id = Uuid::new_v4();
        let result = validate_response("Hello, I can help you with that!", id, false, &[], &detector);
        assert!(result.passed);
        assert!(result.gate_failed.is_none());
    }

    #[test]
    fn test_garbage_detected() {
        let detector = make_detector();
        let id = Uuid::new_v4();
        let result = validate_response("", id, false, &[], &detector);
        assert!(!result.passed);
        assert_eq!(result.gate_failed.as_deref(), Some("garbage"));
    }

    #[test]
    fn test_tool_calls_skip_garbage_and_loop() {
        let detector = make_detector();
        let id = Uuid::new_v4();
        // Empty text with tool calls should pass garbage and loop gates
        let result = validate_response("", id, true, &[], &detector);
        assert!(result.passed);
    }

    #[test]
    fn test_truncated_tool_call() {
        let detector = make_detector();
        let id = Uuid::new_v4();
        let text = r#"Let me check. <tool name="code/read"><path>/tmp"#;
        let result = validate_response(text, id, false, &[], &detector);
        assert!(!result.passed);
        assert_eq!(result.gate_failed.as_deref(), Some("truncated_tool_call"));
    }

    #[test]
    fn test_semantic_loop_detected() {
        let detector = make_detector();
        let id = Uuid::new_v4();
        let history = vec![
            ConversationMessage {
                role: "assistant".to_string(),
                content: "The answer to life is forty-two, as we all know from the guide.".to_string(),
                name: None,
            },
        ];
        // Same content should trigger semantic loop
        let result = validate_response(
            "The answer to life is forty-two, as we all know from the guide.",
            id, false, &history, &detector,
        );
        assert!(!result.passed);
        assert_eq!(result.gate_failed.as_deref(), Some("semantic_loop"));
    }

    #[test]
    fn test_response_loop_detected() {
        let detector = make_detector();
        let id = Uuid::new_v4();
        // Send the same response 4 times to trigger loop detection
        for _ in 0..4 {
            let _ = validate_response("Same response every time.", id, false, &[], &detector);
        }
        let result = validate_response("Same response every time.", id, false, &[], &detector);
        assert!(!result.passed);
        assert_eq!(result.gate_failed.as_deref(), Some("response_loop"));
    }
}
