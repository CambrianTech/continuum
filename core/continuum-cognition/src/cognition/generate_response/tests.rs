#![cfg(test)]

use super::*;
use crate::cognition::should_respond::{AIDecisionContext, GatingConversationMessage, GatingMessageContent, GatingRagContext, GatingRagMetadata, GatingTriggerMessage};

// ─── Fixtures ─────────────────────────────────────────────────────

fn msg(
    role: &str,
    name: Option<&str>,
    content: &str,
    ts: Option<u64>,
) -> GatingConversationMessage {
    GatingConversationMessage {
        role: role.to_string(),
        content: content.to_string(),
        name: name.map(str::to_string),
        timestamp: ts,
    }
}

fn ctx(
    system_prompt: Option<&str>,
    history: Vec<GatingConversationMessage>,
) -> AIDecisionContext {
    AIDecisionContext {
        persona_id: "p-001".to_string(),
        persona_name: "Alice".to_string(),
        room_id: "r-001".to_string(),
        trigger_message: GatingTriggerMessage {
            id: "m-trigger".to_string(),
            sender_name: "human".to_string(),
            content: GatingMessageContent {
                text: "any".to_string(),
            },
        },
        rag_context: GatingRagContext {
            conversation_history: history,
            recipe_strategy: None,
            metadata: GatingRagMetadata { recipe_name: None },
        },
        system_prompt: system_prompt.map(str::to_string),
    }
}

fn text_of(msg: &ChatMessage) -> &str {
    match &msg.content {
        MessageContent::Text(s) => s.as_str(),
        _ => panic!("expected text content; ChatMessage carried a non-text variant"),
    }
}

// ─── format_current_time ──────────────────────────────────────────

/// What this catches: timestamp 1_700_000_000_000ms renders as
/// `11/14/2023 22:13` UTC. If the format string drifts (e.g. to
/// ISO 8601), the model sees a different prompt body and the
/// identity-reminder layer regresses silently.
#[test]
fn format_current_time_matches_mm_dd_yyyy_hh_mm() {
    // 1_700_000_000_000 ms = 2023-11-14 22:13:20 UTC
    assert_eq!(format_current_time(1_700_000_000_000), "11/14/2023 22:13");
}

/// What this catches: epoch 0 renders as `01/01/1970 00:00`.
/// Boundary check — verifies UTC + no off-by-one in the date
/// formatter.
#[test]
fn format_current_time_handles_epoch_zero() {
    assert_eq!(format_current_time(0), "01/01/1970 00:00");
}

// ─── extract_room_members ─────────────────────────────────────────

/// What this catches: well-formed system prompt with members line
/// — pulls out exactly the comma-separated list, trimmed.
#[test]
fn extract_members_pulls_line_after_prefix() {
    let prompt =
        "You are a helpful AI.\nCurrent room members: alice, bob, carol\nMore text below.";
    assert_eq!(extract_room_members(prompt), "alice, bob, carol");
}

/// What this catches: members line at end-of-string without
/// trailing newline — still extracts.
#[test]
fn extract_members_handles_no_trailing_newline() {
    let prompt = "Header line.\nCurrent room members: alice, bob";
    assert_eq!(extract_room_members(prompt), "alice, bob");
}

/// What this catches: missing prefix returns the canonical
/// `UNKNOWN_MEMBERS` string. Downstream prompt machinery may depend
/// on the literal value.
#[test]
fn extract_members_missing_returns_unknown() {
    let prompt = "Generic system prompt with no members line.";
    assert_eq!(extract_room_members(prompt), UNKNOWN_MEMBERS);
    assert_eq!(extract_room_members(""), UNKNOWN_MEMBERS);
}

/// What this catches: empty members list (just whitespace after the
/// prefix) falls back to `UNKNOWN_MEMBERS` — avoids emitting a
/// prompt that says "the room has ONLY these people: ." which is
/// worse than the explicit unknown-members value.
#[test]
fn extract_members_empty_after_prefix_returns_unknown() {
    let prompt = "Current room members: \nSomething else.";
    assert_eq!(extract_room_members(prompt), UNKNOWN_MEMBERS);
}

// ─── format_time_prefix ───────────────────────────────────────────

/// What this catches: present timestamp renders as `[HH:MM] ` UTC.
#[test]
fn format_time_prefix_renders_hh_mm_utc() {
    assert_eq!(format_time_prefix(Some(1_700_000_000_000)), "[22:13] ");
}

/// What this catches: missing timestamp returns empty string —
/// guard against `[00:00] ` for clockless messages (would mislead
/// the model).
#[test]
fn format_time_prefix_missing_returns_empty() {
    assert_eq!(format_time_prefix(None), "");
}

// ─── hour_gap_marker ──────────────────────────────────────────────

/// What this catches: gap < 1h returns None — no marker injected
/// for normal back-and-forth.
#[test]
fn hour_gap_marker_under_threshold_returns_none() {
    assert_eq!(hour_gap_marker(0), None);
    assert_eq!(hour_gap_marker(59 * 60 * 1000), None);
    assert_eq!(hour_gap_marker(HOUR_GAP_THRESHOLD_MS - 1), None);
}

/// What this catches: gap >= 1h returns the singular "1 hour"
/// marker. Plural/singular toggle catches a regression where the
/// `s` suffix bleeds into the 1-hour case.
#[test]
fn hour_gap_marker_one_hour_singular() {
    assert_eq!(
        hour_gap_marker(HOUR_GAP_THRESHOLD_MS).as_deref(),
        Some("⏱️ 1 hour passed - conversation resumed")
    );
}

/// What this catches: gap >= 2h renders plural "hours".
#[test]
fn hour_gap_marker_two_hours_plural() {
    assert_eq!(
        hour_gap_marker(3 * HOUR_GAP_THRESHOLD_MS).as_deref(),
        Some("⏱️ 3 hours passed - conversation resumed")
    );
}

// ─── build_identity_reminder ──────────────────────────────────────

/// What this catches: the reminder embeds persona name, members
/// list, and current time at the expected anchors. If any anchor
/// regresses (e.g. `format!` arg order), the prompt loses its
/// identity-establishing line and the model role-confuses.
#[test]
fn identity_reminder_embeds_persona_members_and_time() {
    let body = build_identity_reminder("Alice", "alice, bob, carol", "11/14/2023 22:13");
    assert!(body.starts_with("IDENTITY REMINDER: You are Alice."));
    assert!(body.contains("ONLY these people: alice, bob, carol."));
    assert!(body.contains("CURRENT TIME: 11/14/2023 22:13"));
    assert!(body.contains("CRITICAL TOPIC DETECTION PROTOCOL"));
}

/// What this catches: the four-step topic-detection rubric is
/// preserved end-to-end. If steps get dropped, the model loses the
/// constraint-extraction guidance.
#[test]
fn identity_reminder_preserves_four_step_protocol() {
    let body = build_identity_reminder("X", "y", "z");
    assert!(body.contains("Step 1: Check for EXPLICIT TOPIC MARKERS"));
    assert!(body.contains("Step 2: Extract HARD CONSTRAINTS"));
    assert!(body.contains("Step 3: Compare SUBJECT"));
    assert!(body.contains("Step 4: Determine response strategy"));
}

/// What this catches: the closing line about time-gap inference is
/// preserved. Removing it would break the model's "topic shift on
/// hour gap" heuristic which the runtime relies on.
#[test]
fn identity_reminder_preserves_time_gap_heuristic_line() {
    let body = build_identity_reminder("X", "y", "z");
    assert!(body.contains("Time gaps > 1 hour usually indicate topic changes"));
}

// ─── build_response_messages ──────────────────────────────────────

/// What this catches: smoke test — system prompt + history +
/// identity reminder all present in correct order. The "skeleton"
/// shape any future refactor must preserve.
#[test]
fn build_response_messages_emits_system_history_identity_in_order() {
    let context = ctx(
        Some("You are Alice in a chat."),
        vec![
            msg("user", Some("human"), "Hello?", Some(1_700_000_000_000)),
            msg("assistant", Some("Alice"), "Hi!", Some(1_700_000_060_000)),
        ],
    );
    let messages = build_response_messages(&context, 1_700_000_120_000);
    assert_eq!(messages.len(), 4, "1 system + 2 history + 1 identity");
    assert_eq!(messages[0].role, "system");
    assert_eq!(text_of(&messages[0]), "You are Alice in a chat.");
    assert_eq!(messages[1].role, "user");
    assert!(text_of(&messages[1]).contains("human: Hello?"));
    assert_eq!(messages[2].role, "assistant");
    assert!(text_of(&messages[2]).contains("Alice: Hi!"));
    assert_eq!(messages[3].role, "system");
    assert!(text_of(&messages[3]).starts_with("IDENTITY REMINDER: You are Alice."));
}

/// What this catches: missing system prompt skips the first message
/// but still emits the identity reminder.
#[test]
fn build_response_messages_omits_system_when_missing() {
    let context = ctx(None, vec![]);
    let messages = build_response_messages(&context, 0);
    assert_eq!(messages.len(), 1, "only identity reminder");
    assert!(text_of(&messages[0]).starts_with("IDENTITY REMINDER:"));
}

/// What this catches: empty-string system prompt is treated as
/// missing — avoids emitting a `{ role: "system", content: "" }`
/// row that some providers reject.
#[test]
fn build_response_messages_omits_system_when_empty_string() {
    let context = ctx(Some(""), vec![]);
    let messages = build_response_messages(&context, 0);
    assert_eq!(
        messages.len(),
        1,
        "only identity reminder; no empty system row"
    );
    assert!(text_of(&messages[0]).starts_with("IDENTITY REMINDER:"));
}

/// What this catches: hour-gap marker fires for a > 1h gap between
/// consecutive messages. The marker injects as its own system
/// message AFTER the older history line and BEFORE the newer one.
#[test]
fn build_response_messages_injects_hour_gap_marker() {
    let context = ctx(
        None,
        vec![
            msg("user", Some("human"), "Earlier?", Some(1_700_000_000_000)),
            // 2 hours later
            msg("user", Some("human"), "Later!", Some(1_700_007_200_000)),
        ],
    );
    let messages = build_response_messages(&context, 0);
    // Expected: [history-1, gap-marker, history-2, identity]
    assert_eq!(messages.len(), 4);
    assert_eq!(messages[0].role, "user");
    assert!(text_of(&messages[0]).contains("human: Earlier?"));
    assert_eq!(messages[1].role, "system");
    assert_eq!(
        text_of(&messages[1]),
        "⏱️ 2 hours passed - conversation resumed"
    );
    assert_eq!(messages[2].role, "user");
    assert!(text_of(&messages[2]).contains("human: Later!"));
    assert_eq!(messages[3].role, "system");
    assert!(text_of(&messages[3]).starts_with("IDENTITY REMINDER:"));
}

/// What this catches: gap markers DO NOT fire between messages
/// with sub-hour gaps — guards against an off-by-one where a
/// 59-minute gap accidentally triggers.
#[test]
fn build_response_messages_no_marker_under_one_hour() {
    let context = ctx(
        None,
        vec![
            msg("user", Some("h"), "A", Some(1_700_000_000_000)),
            // 30 minutes later
            msg("user", Some("h"), "B", Some(1_700_001_800_000)),
        ],
    );
    let messages = build_response_messages(&context, 0);
    // 2 history + 1 identity, no gap marker
    assert_eq!(messages.len(), 3);
    assert!(text_of(&messages[0]).contains("A"));
    assert!(text_of(&messages[1]).contains("B"));
}

/// What this catches: gap tracking only updates when a timestamp
/// is present — a clockless message in the middle doesn't reset
/// the gap-from-previous-timestamped-message counter incorrectly.
#[test]
fn build_response_messages_gap_tracking_ignores_clockless_messages() {
    let context = ctx(
        None,
        vec![
            msg("user", Some("h"), "A", Some(1_700_000_000_000)),
            msg("user", Some("h"), "B-clockless", None),
            // 3 hours after A
            msg("user", Some("h"), "C", Some(1_700_010_800_000)),
        ],
    );
    let messages = build_response_messages(&context, 0);
    // Expected: history-A, history-B-clockless, gap-marker (A→C 3h), history-C, identity
    assert_eq!(messages.len(), 5);
    assert!(text_of(&messages[0]).contains("[22:13] h: A"));
    assert_eq!(messages[1].role, "user");
    assert_eq!(text_of(&messages[1]), "h: B-clockless"); // no time prefix
    assert_eq!(messages[2].role, "system");
    assert!(text_of(&messages[2]).contains("3 hours passed"));
    assert!(text_of(&messages[3]).contains("h: C"));
}

/// What this catches: messages without a name use the bare time
/// prefix + content (no `name: ` chunk).
#[test]
fn build_response_messages_falls_back_when_name_missing() {
    let context = ctx(
        None,
        vec![msg("user", None, "bare content", Some(1_700_000_000_000))],
    );
    let messages = build_response_messages(&context, 0);
    // 1 history + 1 identity
    assert_eq!(messages.len(), 2);
    assert_eq!(text_of(&messages[0]), "[22:13] bare content");
}

/// What this catches: members extraction reads from the system
/// prompt body — the identity reminder gets the right list. Pins
/// the end-to-end path from system_prompt → extract_room_members
/// → build_identity_reminder.
#[test]
fn build_response_messages_extracts_members_for_identity_reminder() {
    let prompt = "You are Alice.\nCurrent room members: alice, bob, carol\nBe helpful.";
    let context = ctx(Some(prompt), vec![]);
    let messages = build_response_messages(&context, 1_700_000_000_000);
    let reminder = text_of(messages.last().expect("identity reminder present"));
    assert!(
        reminder.contains("ONLY these people: alice, bob, carol."),
        "identity reminder should embed members extracted from system prompt; got: {reminder}"
    );
    assert!(reminder.contains("CURRENT TIME: 11/14/2023 22:13"));
}

/// What this catches: missing members in the system prompt still
/// renders the identity reminder with the `UNKNOWN_MEMBERS`
/// unknown-members string. No panic on a recipe-less room.
#[test]
fn build_response_messages_unknown_members_when_prompt_missing_line() {
    let context = ctx(Some("Generic system prompt."), vec![]);
    let messages = build_response_messages(&context, 0);
    let reminder = text_of(messages.last().expect("identity reminder present"));
    assert!(
        reminder.contains(&format!("ONLY these people: {UNKNOWN_MEMBERS}.")),
        "missing members line must render unknown-members value; got: {reminder}"
    );
}

/// What this catches: when system_prompt is None entirely, the
/// identity reminder still composes with `UNKNOWN_MEMBERS` (no
/// panic from `unwrap_or("")` path).
#[test]
fn build_response_messages_no_system_prompt_falls_back_to_unknown_members() {
    let context = ctx(None, vec![]);
    let messages = build_response_messages(&context, 0);
    let reminder = text_of(messages.last().expect("identity reminder present"));
    assert!(reminder.contains(&format!("ONLY these people: {UNKNOWN_MEMBERS}.")));
}

/// What this catches: assistant + user roles round-trip in their
/// original case + spelling. Rust preserves whatever string the
/// message carried, which is the correct conservative choice
/// because provider routing depends on these exact strings.
#[test]
fn build_response_messages_preserves_role_strings() {
    let context = ctx(
        None,
        vec![
            msg("user", Some("h"), "U", None),
            msg("assistant", Some("a"), "A", None),
        ],
    );
    let messages = build_response_messages(&context, 0);
    assert_eq!(messages[0].role, "user");
    assert_eq!(messages[1].role, "assistant");
}

/// What this catches: empty conversation history still produces a
/// well-formed message list (system prompt if any + identity
/// reminder). Important for first-turn responses.
#[test]
fn build_response_messages_handles_empty_history() {
    let context = ctx(Some("sys"), vec![]);
    let messages = build_response_messages(&context, 0);
    assert_eq!(messages.len(), 2, "system + identity");
    assert_eq!(messages[0].role, "system");
    assert_eq!(text_of(&messages[0]), "sys");
    assert!(text_of(&messages[1]).starts_with("IDENTITY REMINDER:"));
}

// ─── build_response_generation_request ────────────────────────────

fn request_with_overrides(
    model: Option<&str>,
    temp: Option<f32>,
    max: Option<u32>,
    timeout: Option<u64>,
) -> GenerateResponseRequest {
    GenerateResponseRequest {
        context: ctx(Some("You are Alice."), vec![]),
        model: model.map(str::to_string),
        temperature: temp,
        max_tokens: max,
        timeout_ms: timeout,
    }
}

/// What this catches: defaults — no overrides — produces a
/// TextGenerationRequest routed to the llama-server gateway with an explicit model,
/// temperature=0.7, max_tokens=None (the model owns its length),
/// response_format=Text, purpose="cognition/generate-response", and
/// persona/room attribution carried from the context. Pins the wire
/// shape so downstream provider routing doesn't drift silently.
#[test]
fn generation_request_uses_documented_defaults() {
    let request = request_with_overrides(None, None, None, None);
    let inference =
        build_response_generation_request(&request, TEST_GENERATE_MODEL.to_string(), 0);
    assert_eq!(
        inference.provider.as_deref(),
        Some(DEFAULT_GENERATE_PROVIDER)
    );
    assert_eq!(inference.model.as_deref(), Some(TEST_GENERATE_MODEL));
    assert_eq!(inference.temperature, Some(DEFAULT_GENERATE_TEMPERATURE));
    // No override + no client default = the model owns its length.
    assert_eq!(inference.max_tokens, None);
    assert_eq!(
        inference.purpose.as_deref(),
        Some("cognition/generate-response")
    );
    assert_eq!(inference.persona_id.as_deref(), Some("p-001"));
    assert_eq!(inference.room_id.as_deref(), Some("r-001"));
    assert!(matches!(
        inference.response_format,
        Some(ResponseFormat::Text)
    ));
    // messages list = system prompt + identity reminder for an empty history
    assert_eq!(inference.messages.len(), 2);
}

/// What this catches: per-request overrides actually override
/// (temperature, max_tokens, model). Without this, a caller passing
/// `temperature=0.1` would silently get the default 0.7.
#[test]
fn generation_request_honors_overrides() {
    let request = request_with_overrides(Some("custom-model"), Some(0.1), Some(500), None);
    let inference = build_response_generation_request(&request, "custom-model".to_string(), 0);
    assert_eq!(inference.model.as_deref(), Some("custom-model"));
    assert_eq!(inference.temperature, Some(0.1));
    assert_eq!(inference.max_tokens, Some(500));
}

/// What this catches: build_response_generation_request embeds the
/// timestamp it's given into the identity reminder via
/// build_response_messages. Pins the time-flow through the layers.
#[test]
fn generation_request_embeds_caller_timestamp() {
    let request = request_with_overrides(None, None, None, None);
    let inference = build_response_generation_request(
        &request,
        TEST_GENERATE_MODEL.to_string(),
        1_700_000_000_000,
    );
    let identity = match &inference.messages.last().expect("identity present").content {
        MessageContent::Text(s) => s.clone(),
        _ => panic!("non-text identity"),
    };
    assert!(identity.contains("CURRENT TIME: 11/14/2023 22:13"));
}

// ─── result_from_response ─────────────────────────────────────────

fn fake_response(
    text: &str,
    total_tokens: u32,
    input: u32,
    output: u32,
) -> TextGenerationResponse {
    TextGenerationResponse {
        text: text.to_string(),
        finish_reason: crate::ai::types::FinishReason::Stop,
        model: "ignored".to_string(),
        provider: "local".to_string(),
        usage: crate::ai::types::UsageMetrics {
            input_tokens: input,
            output_tokens: output,
            total_tokens,
            estimated_cost: None,
        },
        response_time_ms: 0,
        request_id: "test".to_string(),
        content: None,
        tool_calls: None,
        reasoning: None,
        routing: None,
        error: None,
        timing: None,
    }
}

/// What this catches: result trims surrounding whitespace from the
/// provider's text. Models often emit leading/trailing newlines;
/// without trim the chat surface gets extra blank lines.
#[test]
fn result_trims_response_text() {
    let r = fake_response("  hello world\n\n", 0, 0, 0);
    let result = result_from_response(r, "m".to_string(), 0, 1000);
    assert_eq!(result.text, "hello world");
}

/// What this catches: model + timestamps stamped correctly on the
/// returned struct. response_time_ms = end - start, timestamp = end.
#[test]
fn result_stamps_model_and_timing() {
    let r = fake_response("body", 0, 0, 0);
    let result = result_from_response(r, "qwen3.5".to_string(), 1_000, 1_250);
    assert_eq!(result.model, "qwen3.5");
    assert_eq!(result.response_time_ms, 250);
    assert_eq!(result.timestamp, 1_250);
}

/// What this catches: total_tokens > 0 -> Some(TokenUsage) with all
/// three counts. The provider-reported case.
#[test]
fn result_populates_tokens_when_provider_reports() {
    let r = fake_response("body", 100, 40, 60);
    let result = result_from_response(r, "m".to_string(), 0, 0);
    assert_eq!(
        result.tokens_used,
        Some(TokenUsage {
            input: 40,
            output: 60,
            total: 100,
        })
    );
}

/// What this catches: total_tokens == 0 -> None. Avoids emitting
/// `{input:0, output:0, total:0}` as if the provider had measured
/// usage.
#[test]
fn result_tokens_none_when_provider_reports_zero() {
    let r = fake_response("body", 0, 0, 0);
    let result = result_from_response(r, "m".to_string(), 0, 0);
    assert_eq!(result.tokens_used, None);
}

/// What this catches: response_time_ms uses saturating subtraction
/// — if end_ms < start_ms (clock-backwards artifact, e.g. NTP
/// adjustment mid-call), result_time is 0, not a wrapped huge u64.
#[test]
fn result_response_time_saturates_when_clock_goes_backward() {
    let r = fake_response("body", 0, 0, 0);
    let result = result_from_response(r, "m".to_string(), 2_000, 1_000);
    assert_eq!(result.response_time_ms, 0);
}

// ─── GenerateResponseError ────────────────────────────────────────

/// What this catches: Display impl carries the provider + model
/// values in NoAdapter so debug logs surface what went unrouted.
#[test]
fn error_no_adapter_displays_provider_and_model() {
    let err = GenerateResponseError::NoAdapter {
        provider: "local".to_string(),
        model: Some("qwen3.5".to_string()),
    };
    let s = format!("{err}");
    assert!(s.contains("local"));
    assert!(s.contains("qwen3.5"));
}

/// What this catches: Display impl for Timeout includes the
/// configured timeout — diagnostic value for operators tuning
/// the value.
#[test]
fn error_timeout_displays_duration() {
    let err = GenerateResponseError::Timeout {
        timeout_ms: 180_000,
    };
    let s = format!("{err}");
    assert!(s.contains("180000"));
}
