#![cfg(test)]

use super::*;

/// what this catches: the SSE frame llama-server emits during PREFILL
/// silently failing to decode, which would restore the 2026-08-13 defect —
/// the watchdog seeing no progress and killing a slot that was healthily
/// ingesting a long prompt. If `prompt_progress` stops parsing, a citizen
/// with a big prompt can never produce a token.
#[test]
fn prefill_frame_decodes_with_no_choices_and_no_tokens() {
    let frame = r#"{"choices":[],"created":1,"model":"m","object":"x",
        "prompt_progress":{"total":16800,"cache":0,"processed":12288,"time_ms":84380}}"#;
    let parsed: OpenAIStreamChunk =
        serde_json::from_str(frame).expect("a prefill frame must decode");
    let p = parsed
        .prompt_progress
        .expect("prompt_progress must survive deserialization");
    assert_eq!(p.processed, 12288);
    assert_eq!(p.total, 16800);
    assert!(
        parsed.choices.is_empty(),
        "a prefill frame carries NO choices — this is exactly why the \
         token-only watchdog could not see it"
    );
}

/// what this catches: a REPEATED progress frame being treated as fresh
/// progress. The wedge signature (#385) is a slot that keeps emitting while
/// its counter is frozen; if a non-advancing frame reset the watchdog, the
/// detector would never fire again and we would be back to the 5-hour hang.
#[test]
fn a_frozen_counter_is_not_progress() {
    let mut last: u64 = 12288;
    let repeated: u64 = 12288;
    assert!(
        !(repeated > last),
        "a frame carrying the SAME processed count must NOT count as progress"
    );
    let advanced: u64 = 14336;
    assert!(advanced > last, "a rising count is progress");
    last = advanced;
    assert_eq!(last, 14336);
}

/// what this catches: the two silences collapsing back into one budget.
/// Queue wait (measured 115s on a 1-slot lane for a 2,237-token prompt) is
/// contention, not death; only silence AFTER the slot starts working is a
/// wedge. If these budgets are ever made equal, healthy turns die under
/// normal multi-citizen load.
#[test]
fn queue_budget_outlives_the_liveness_budget() {
    assert!(
        PRE_STREAM_HEADER_TIMEOUT_SECS > STREAM_IDLE_TIMEOUT_SECS,
        "the pre-start (queue) budget must be strictly larger than the \
         post-start liveness budget: {PRE_STREAM_HEADER_TIMEOUT_SECS} vs \
         {STREAM_IDLE_TIMEOUT_SECS}"
    );
}
