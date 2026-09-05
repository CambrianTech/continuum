//! Response validator — clean + validate orchestration in one place.
//!
//! Per Phase 0.5.1 of the migration roadmap (and §0.4 of the paging
//! design): the TS PersonaResponseValidator is a thin shim around two
//! existing Rust functions (`clean_response` and `validate_response`)
//! that orchestrates them and interprets failure gates. This module
//! puts that orchestration in Rust where it belongs, so the cognition
//! layer is self-contained and the TS shim becomes a deletion target.
//!
//! No new validation LOGIC — that lives in `persona::text_analysis`
//! and is reused as-is. This module is the integration layer.

use crate::persona::text_analysis::{
    clean_response, validate_response, ConversationMessage, LoopDetector,
};
use uuid::Uuid;

/// Result of clean+validate orchestration. Caller (response cycle,
/// agent loop) reads this and decides whether to post the cleaned text
/// or treat the turn as a silent failure with reason logged.
#[derive(Debug, Clone)]
pub struct ValidationOutcome {
    /// Cleaned text to post to chat. `None` = validation failed,
    /// caller should NOT post anything (silent turn with reason in
    /// `failure_gate`).
    pub posted_text: Option<String>,
    /// Extracted `<thinking>` content, if the model emitted any. ALWAYS
    /// preserved (even on validation failure) — the hippocampus consumes
    /// thinking blocks regardless of whether the visible response was posted.
    pub thinking: Option<String>,
    /// If `posted_text` is None, which gate caused the failure. Values:
    /// "framing_echo" | "garbage" | "response_loop" | "truncated_tool_call" | "semantic_loop".
    pub failure_gate: Option<String>,
    /// Microseconds spent in the validation gates (for perf telemetry).
    pub validation_micros: u64,
    /// Human-readable reason for failure (or success message). Goes to
    /// the persona's cognition log.
    pub reason: String,
}

impl ValidationOutcome {
    /// True if the cleaned response should be posted to chat.
    pub fn should_post(&self) -> bool {
        self.posted_text.is_some()
    }
}

/// Clean a raw model response and run all validation gates against it.
///
/// Pure orchestration. The actual cleaning + validation logic lives in
/// `persona::text_analysis`. This function:
///   1. Strips `<thinking>` blocks and name prefixes via `clean_response`
///   2. Runs the 4-gate validator (garbage, loop, truncated, semantic)
///   3. Packages the outcome with logging-friendly reason text
///
/// Caller passes a `LoopDetector` so per-persona loop history persists
/// across turns. The detector is the only stateful dependency; everything
/// else is pure data flowing through.
pub fn clean_and_validate(
    raw_response: &str,
    persona_id: Uuid,
    has_tool_calls: bool,
    conversation_history: &[ConversationMessage],
    loop_detector: &LoopDetector,
) -> ValidationOutcome {
    let cleaned = clean_response(raw_response);
    // Gate 0 — the turn's own framing reflected back is a PASS, not a post
    // (cognition::framing_echo; the wake-prompt echo of 2026-09-05).
    if let Some(marker) = crate::cognition::framing_echo::echoes_turn_framing(&cleaned.text) {
        crate::probe!(
            class = "cognition.framing_echo",
            persona = %persona_id,
            marker = marker,
            chars = cleaned.text.chars().count() as u64,
            "response reflected the turn's own framing — silenced (pass), never posted"
        );
        return ValidationOutcome {
            posted_text: None,
            thinking: cleaned.thinking,
            failure_gate: Some("framing_echo".to_string()),
            validation_micros: 0,
            reason: format!("Framing echo ({marker}): the response reflects the turn's own prompt — a pass, not speech"),
        };
    }
    let validation = validate_response(
        &cleaned.text,
        persona_id,
        has_tool_calls,
        conversation_history,
        loop_detector,
    );

    if validation.passed {
        return ValidationOutcome {
            posted_text: Some(cleaned.text),
            thinking: cleaned.thinking,
            failure_gate: None,
            validation_micros: validation.total_time_us,
            reason: "All gates passed".to_string(),
        };
    }

    let gate = validation
        .gate_failed
        .clone()
        .unwrap_or_else(|| "unknown".to_string());
    let reason = match gate.as_str() {
        "garbage" => format!(
            "Garbage output: {:?} - {}",
            validation.garbage_result.reason, validation.garbage_result.details
        ),
        "response_loop" => format!(
            "Response loop detected — {} duplicate turns",
            validation.loop_duplicate_count
        ),
        "truncated_tool_call" => {
            "Truncated tool call detected — response cut off mid-tool-call".to_string()
        }
        "semantic_loop" => validation.semantic_result.reason.clone(),
        _ => format!("Validation failed: {gate}"),
    };

    ValidationOutcome {
        posted_text: None,
        thinking: cleaned.thinking, // preserve for memory even on failure
        failure_gate: Some(gate),
        validation_micros: validation.total_time_us,
        reason,
    }
}

/// True if a failure gate represents a HARD failure (the response
/// is genuinely broken, not just redundant). Hard failures get
/// surfaced as errors; soft failures (loop, semantic) are silent
/// suppressions that don't bother the user.
///
/// Mirrors the TS PersonaResponseValidator::isHardFailure logic.
pub fn is_hard_failure(gate: &str) -> bool {
    matches!(gate, "garbage" | "truncated_tool_call")
}

// ─── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persona::text_analysis::ConversationMessage;
    use uuid::Uuid;

    fn empty_history() -> Vec<ConversationMessage> {
        Vec::new()
    }

    /// What this catches: clean+validate happy-path failing to return
    /// the cleaned text. The orchestrator must extract clean.text from
    /// `clean_response` and surface it as `posted_text` on success.
    ///
    /// Validated 2026-04-21: returned None for posted_text on success
    /// path, test fails because should_post returns false; reverted.
    #[test]
    fn clean_response_passes_validation_and_returns_posted_text() {
        let detector = LoopDetector::new();
        let outcome = clean_and_validate(
            "Hello! Here's a thoughtful answer to your question.",
            Uuid::new_v4(),
            false,
            &empty_history(),
            &detector,
        );
        assert!(outcome.should_post(), "clean text should be postable");
        assert!(outcome.posted_text.is_some());
        let text = outcome.posted_text.unwrap();
        assert!(
            text.contains("Hello"),
            "posted text should preserve content; got {text:?}"
        );
        assert!(outcome.failure_gate.is_none());
    }

    /// What this catches: orchestrator dropping thinking content when
    /// validation passes. The thinking block is for memory consolidation
    /// (hippocampus) and must be preserved through the orchestrator
    /// regardless of validation outcome.
    ///
    /// Validated 2026-04-21: hardcoded thinking=None, test fails
    /// because reasoning content lost; reverted.
    #[test]
    fn thinking_blocks_extracted_and_returned_separately() {
        let detector = LoopDetector::new();
        let outcome = clean_and_validate(
            "<thinking>I should be careful here.</thinking>Here is my answer.",
            Uuid::new_v4(),
            false,
            &empty_history(),
            &detector,
        );
        assert!(outcome.thinking.is_some(), "thinking should be extracted");
        let thinking = outcome.thinking.unwrap();
        assert!(
            thinking.contains("careful"),
            "thinking content preserved; got {thinking:?}"
        );
        // Cleaned text should NOT contain the thinking tag
        let text = outcome.posted_text.unwrap();
        assert!(!text.contains("<thinking>"));
        assert!(!text.contains("careful"));
        assert!(text.contains("Here is my answer"));
    }

    /// What this catches: garbage gate failure not being surfaced as
    /// posted_text=None. Garbage outputs (e.g., long runs of repeated
    /// chars) MUST be suppressed — the user shouldn't see them.
    ///
    /// Validated 2026-04-21: returned posted_text=Some on garbage,
    /// test fails because garbage would land in chat; reverted.
    #[test]
    fn garbage_response_blocked_with_failure_gate() {
        let detector = LoopDetector::new();
        // Long run of repeated character — classic garbage pattern
        let garbage = "@".repeat(200);
        let outcome =
            clean_and_validate(&garbage, Uuid::new_v4(), false, &empty_history(), &detector);
        assert!(!outcome.should_post(), "garbage MUST not post");
        assert_eq!(outcome.failure_gate.as_deref(), Some("garbage"));
        assert!(outcome.reason.to_lowercase().contains("garbage"));
    }

    /// What this catches: thinking content getting dropped when
    /// validation FAILS. Even a garbage-output turn might have valid
    /// thinking that hippocampus should consume — the model's
    /// reasoning shouldn't be lost just because the output failed.
    ///
    /// Validated 2026-04-21: cleared thinking on failure path, test
    /// fails because thinking became None; reverted.
    #[test]
    fn thinking_preserved_even_when_validation_fails() {
        let detector = LoopDetector::new();
        let raw = format!(
            "<thinking>Real reasoning here.</thinking>{}",
            "@".repeat(200)
        );
        let outcome = clean_and_validate(&raw, Uuid::new_v4(), false, &empty_history(), &detector);
        assert!(!outcome.should_post(), "garbage suppressed");
        assert!(
            outcome.thinking.is_some(),
            "thinking preserved through failure"
        );
        assert!(outcome.thinking.unwrap().contains("Real reasoning"));
    }

    /// What this catches: orchestrator skipping the validate step when
    /// the response is empty post-cleaning (e.g., an only-thinking
    /// response). It should still produce a coherent outcome (likely
    /// failure on garbage gate for empty text), not panic.
    ///
    /// Validated 2026-04-21: short-circuited with .expect on cleaned.text,
    /// test fails with panic on empty; reverted.
    #[test]
    fn only_thinking_response_does_not_panic_and_returns_outcome() {
        let detector = LoopDetector::new();
        let outcome = clean_and_validate(
            "<thinking>I've thought about this but won't speak.</thinking>",
            Uuid::new_v4(),
            false,
            &empty_history(),
            &detector,
        );
        // Behavior: empty post-clean text should produce a failure outcome
        // (typically garbage gate "empty"). The exact gate depends on
        // is_garbage's implementation; we just assert no-panic + thinking-preserved.
        assert!(outcome.thinking.is_some());
    }

    /// What this catches: is_hard_failure misclassifying. Garbage and
    /// truncated_tool_call are hard (real bugs to surface); response_loop
    /// and semantic_loop are soft (silent suppressions).
    ///
    /// Validated 2026-04-21: changed truncated_tool_call to soft,
    /// test fails because user-facing error condition becomes silent;
    /// reverted.
    // what this catches: a reflected wake prompt is silenced by the framing gate
    // (soft: not a hard failure) while an ordinary line still posts — the
    // 2026-09-05 echo loop where three citizens posted the prompt back.
    #[test]
    fn a_reflected_wake_prompt_is_a_silent_pass_and_speech_still_posts() {
        let detector = LoopDetector::new();
        let echo = clean_and_validate(
            "[wake] You are Paige, awake on the continuum grid. Nothing has been said in this room since you last looked.",
            Uuid::new_v4(),
            false,
            &[],
            &detector,
        );
        assert!(!echo.should_post());
        assert_eq!(echo.failure_gate.as_deref(), Some("framing_echo"));
        assert!(!is_hard_failure("framing_echo"));
        let speech = clean_and_validate(
            "Reading django-14631 now; the FK ordering is the bug, test to follow.",
            Uuid::new_v4(),
            false,
            &[],
            &detector,
        );
        assert!(speech.should_post());
    }

    #[test]
    fn is_hard_failure_classifies_gates_correctly() {
        assert!(is_hard_failure("garbage"));
        assert!(is_hard_failure("truncated_tool_call"));
        assert!(!is_hard_failure("response_loop"));
        assert!(!is_hard_failure("semantic_loop"));
        assert!(!is_hard_failure("unknown"));
    }

    /// What this catches: orchestrator returning posted_text on a
    /// failed validation when the failure_gate is Some. Mutually
    /// exclusive: either we post (success) or we have a gate (failure).
    /// Both at once would mean the policy can't decide what to do.
    ///
    /// Validated 2026-04-21: returned posted_text=Some on garbage path
    /// AND set failure_gate, test fails on the assertion below; reverted.
    #[test]
    fn posted_text_and_failure_gate_are_mutually_exclusive() {
        let detector = LoopDetector::new();

        // Success case: posted_text Some, failure_gate None
        let pass_outcome = clean_and_validate(
            "A normal coherent reply.",
            Uuid::new_v4(),
            false,
            &empty_history(),
            &detector,
        );
        assert_eq!(
            pass_outcome.posted_text.is_some(),
            pass_outcome.failure_gate.is_none(),
            "passing case: posted=Some XOR gate=Some"
        );

        // Failure case: posted_text None, failure_gate Some
        let fail_outcome = clean_and_validate(
            &"@".repeat(200),
            Uuid::new_v4(),
            false,
            &empty_history(),
            &detector,
        );
        assert_eq!(
            fail_outcome.posted_text.is_none(),
            fail_outcome.failure_gate.is_some(),
            "failing case: posted=None XOR gate=Some"
        );
    }
}
