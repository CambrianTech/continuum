//! Rust-owned response-generation prompt assembly.
//!
//! Oxidizer for `AIDecisionService.generateResponse` (TS, see
//! `src/system/ai/server/AIDecisionService.ts:316-452`). Sibling to
//! `check_redundancy.rs` (#1375) + `should_respond.rs` (already
//! oxidized). TypeScript continues to own slot coordination + logging;
//! Rust owns the response-generation contract, prompt assembly, and
//! identity-reminder template.
//!
//! ## Scope of this PR (PR-1 — pure types + prompt builder)
//!
//! - `GenerateResponseRequest` — IPC request (ts-rs)
//! - `GenerateResponseResult` — IPC response (ts-rs)
//! - `TokenUsage` — token-count breakdown (ts-rs)
//! - `build_response_messages(&AIDecisionContext, current_time_ms)
//!   -> Vec<ChatMessage>` — pure. Composes:
//!     - System-prompt message (from context.system_prompt)
//!     - Conversation history with [HH:MM] time prefix + hour-gap
//!       markers
//!     - Identity-reminder system message at end
//! - `build_identity_reminder(persona_name, members, current_time)
//!   -> String` — pure. The canonical ~50-line critical-topic-detection
//!   prompt template.
//! - `extract_room_members(system_prompt) -> &str` — pure. Regex
//!   pulls `Current room members: ...` out of a system prompt body.
//! - `format_current_time(ms) -> String` — pure. UTC `MM/DD/YYYY HH:MM`.
//! - `format_time_prefix(Option<ms>) -> String` — pure. UTC `[HH:MM] `.
//! - `hour_gap_marker(gap_ms) -> Option<String>` — pure.
//!
//! ## NOT in this PR
//!
//! - **PR-2**: `cognition/generate-response` IPC handler — async
//!   composer that calls `build_response_messages` → AI provider call
//!   (existing local Qwen router) → `GenerateResponseResult` with
//!   `tokio::time::timeout` replacing the TS Promise.race.
//! - **PR-3**: TS shim — `AIDecisionService.generateResponse` delegates
//!   to `RustCoreIPCClient.cognitionGenerateResponse`.
//! - **PR-4**: Delete dead TS — `buildResponseMessages` + the inline
//!   identity-reminder template (~250 LOC removed).
//!
//! ## Failure-mode discipline
//!
//! Same posture as `check_redundancy.rs` + `should_respond.rs`:
//!   - All errors typed (`GenerateResponseError` — PR-2 surfaces it).
//!   - Pure prompt builder uses UTC (removes hidden TZ dependency the
//!     TS version's `toLocaleDateString` had — server timezone was
//!     bleeding into model prompts depending on host).
//!   - No silent default-on-error in the parser layer (PR-2).
//!   - Members extraction falls back to the literal `"unknown members"`
//!     string when the regex misses — matches TS behavior exactly so
//!     no template regression.

use crate::ai::adapter::InferenceDevice;
use crate::ai::types::ResponseFormat;
use crate::ai::{ChatMessage, MessageContent, TextGenerationRequest, TextGenerationResponse};
use crate::cognition::should_respond::AIDecisionContext;
use crate::modules::ai_provider::global_registry;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use ts_rs::TS;

/// Default fallback string returned by `extract_room_members` when the
/// system prompt doesn't contain a `Current room members:` line.
/// Matches the TS literal exactly so prompts don't regress.
pub const UNKNOWN_MEMBERS: &str = "unknown members";

/// Minimum hour-gap (in milliseconds) that triggers a "⏱️ N hour passed"
/// marker in the conversation history. Matches TS `gapMinutes > 60`.
const HOUR_GAP_THRESHOLD_MS: u64 = 60 * 60 * 1000;

/// Routing sentinel for the best available local Qwen/llama.cpp runtime.
/// Matches the TS `provider: 'local'` value the adapter registry routes
/// against.
const DEFAULT_GENERATE_PROVIDER: &str = "local";

/// Default model when caller doesn't override. Matches TS
/// `LOCAL_MODELS.DEFAULT` exactly.
const DEFAULT_GENERATE_MODEL: &str = "continuum-ai/qwen3.5-4b-code-forged-GGUF";

/// Default sampling temperature. Matches TS default 0.7 — moderate
/// creativity for natural-language responses.
const DEFAULT_GENERATE_TEMPERATURE: f32 = 0.7;

/// Default max tokens. Matches TS default 150 — short conversational
/// responses; caller can raise for long-form.
const DEFAULT_GENERATE_MAX_TOKENS: u32 = 150;

/// Default timeout. Matches TS default 180_000ms (3 minutes) — Qwen
/// local can be slow under load; this is the hard ceiling before
/// `tokio::time::timeout` returns Err.
const DEFAULT_GENERATE_TIMEOUT_MS: u64 = 180_000;

// ─── IPC request + response shapes ────────────────────────────────────

/// IPC request: ask the cognition service to assemble a response-prompt
/// and (in PR-2) run it through the local inference provider.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/GenerateResponseRequest.ts"
)]
pub struct GenerateResponseRequest {
    /// Reuses the gating context. The TS shim resolves
    /// `ragContext.identity.systemPrompt` (the persona's identity
    /// system prompt with `Current room members: ...`) into
    /// `context.system_prompt` before sending — keeps Rust independent
    /// of `RAGContext.identity` shape.
    pub context: AIDecisionContext,
    /// Optional model override. PR-2 defaults to the local-Qwen routing
    /// sentinel when unset (matches TS `LOCAL_MODELS.DEFAULT`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    /// Sampling temperature. TS default is 0.7; PR-2 carries the same
    /// default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub temperature: Option<f32>,
    /// Max tokens to generate. TS default is 150; PR-2 carries the
    /// same default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub max_tokens: Option<u32>,
    /// Hard cap on how long PR-2's async composer waits before
    /// returning timeout. TS default is 180_000ms (Qwen local can
    /// be slow under load).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub timeout_ms: Option<u64>,
}

/// IPC response: generated text plus timing + token telemetry.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/GenerateResponseResult.ts"
)]
pub struct GenerateResponseResult {
    pub text: String,
    pub model: String,
    #[ts(type = "number")]
    pub response_time_ms: u64,
    #[ts(type = "number")]
    pub timestamp: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tokens_used: Option<TokenUsage>,
}

/// Token-count breakdown — present when the provider reports usage,
/// `None` when the provider does not (e.g. local Qwen without
/// instrumentation).
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/TokenUsage.ts"
)]
pub struct TokenUsage {
    pub input: u32,
    pub output: u32,
    pub total: u32,
}

/// Typed errors from `evaluate_response`. No silent default-on-error;
/// the caller (TS shim or other Rust client) decides policy explicitly.
#[derive(Debug, thiserror::Error)]
pub enum GenerateResponseError {
    /// The provider registry had no adapter capable of serving this
    /// model + provider tuple. PR-3's TS shim translates this back into
    /// an `Error` for the persona scheduler.
    #[error("no AI adapter available for provider={provider:?} model={model:?}")]
    NoAdapter {
        provider: String,
        model: Option<String>,
    },
    /// Provider returned an error during generation (network, model
    /// refused, etc.). The string is the raw provider message — caller
    /// should log + surface, never silently default.
    #[error("generation failed: {0}")]
    Generation(String),
    /// `tokio::time::timeout` fired before the provider returned.
    /// Mirrors the TS `Promise.race` timeout branch (TS default
    /// 180_000ms). The persona scheduler should treat this as a
    /// transient failure and back off, not a permanent decision.
    #[error("generation timed out after {timeout_ms} ms")]
    Timeout {
        #[allow(dead_code)] // surfaced via Display
        timeout_ms: u64,
    },
}

/// Run the response-generation against the registered AI provider.
///
/// Composes:
///   1. `build_response_messages(&request.context, now)` for the
///      message array (system prompt + history + identity reminder).
///   2. `TextGenerationRequest` with provider="local" + model +
///      temperature + max_tokens defaults from `DEFAULT_GENERATE_*`
///      constants (each overridable per-request).
///   3. `tokio::time::timeout` wraps the provider call (TS Promise.race
///      equivalent).
///   4. Stamps `GenerateResponseResult` with model + response_time_ms +
///      timestamp + optional token usage (when the provider reports it).
///
/// No fallback path: provider failures, timeouts, and missing adapters
/// all surface as typed errors. Caller decides policy explicitly.
pub async fn evaluate_response(
    request: GenerateResponseRequest,
) -> Result<GenerateResponseResult, GenerateResponseError> {
    let start_ms = now_ms();
    let model = request
        .model
        .clone()
        .unwrap_or_else(|| DEFAULT_GENERATE_MODEL.to_string());
    let timeout_ms = request.timeout_ms.unwrap_or(DEFAULT_GENERATE_TIMEOUT_MS);

    let inference_request = build_response_generation_request(&request, model.clone(), start_ms);

    let registry_arc = global_registry();
    let registry = registry_arc.read().await;
    let (_provider_id, adapter) = registry
        .select(
            Some(DEFAULT_GENERATE_PROVIDER),
            Some(&model),
            InferenceDevice::default(),
        )
        .ok_or_else(|| GenerateResponseError::NoAdapter {
            provider: DEFAULT_GENERATE_PROVIDER.to_string(),
            model: Some(model.clone()),
        })?;

    let response: TextGenerationResponse =
        match tokio::time::timeout(Duration::from_millis(timeout_ms), adapter.generate_text(inference_request))
            .await
        {
            Ok(Ok(resp)) => resp,
            Ok(Err(e)) => return Err(GenerateResponseError::Generation(e)),
            Err(_) => return Err(GenerateResponseError::Timeout { timeout_ms }),
        };

    let end_ms = now_ms();
    Ok(result_from_response(response, model, start_ms, end_ms))
}

/// Build the `TextGenerationRequest` the adapter consumes.
/// Pure: caller passes `request`, `model`, and the start-timestamp so
/// tests can assert the request shape without time interference.
pub fn build_response_generation_request(
    request: &GenerateResponseRequest,
    model: String,
    start_ms: u64,
) -> TextGenerationRequest {
    TextGenerationRequest {
        messages: build_response_messages(&request.context, start_ms),
        system_prompt: None,
        model: Some(model),
        provider: Some(DEFAULT_GENERATE_PROVIDER.to_string()),
        temperature: Some(
            request
                .temperature
                .unwrap_or(DEFAULT_GENERATE_TEMPERATURE),
        ),
        max_tokens: Some(request.max_tokens.unwrap_or(DEFAULT_GENERATE_MAX_TOKENS)),
        top_p: None,
        top_k: None,
        repeat_penalty: None,
        stop_sequences: None,
        tools: None,
        tool_choice: None,
        // Local Qwen takes plain text; no JSON-mode constraint here.
        response_format: Some(ResponseFormat::Text),
        active_adapters: None,
        request_id: None,
        user_id: None,
        room_id: Some(request.context.room_id.clone()),
        purpose: Some("cognition/generate-response".to_string()),
        persona_id: Some(request.context.persona_id.clone()),
    }
}

/// Pure: compose the IPC response from the provider's text + timing.
/// Trims the response text to match TS `response.text.trim()`.
///
/// `tokens_used` is `None` when the provider reported `total_tokens == 0`
/// — mirrors TS truthiness check on the optional usage object, avoids
/// emitting `{input:0,output:0,total:0}` as if the provider had measured
/// (it usually means the provider doesn't instrument usage at all).
pub fn result_from_response(
    response: TextGenerationResponse,
    model: String,
    start_ms: u64,
    end_ms: u64,
) -> GenerateResponseResult {
    let tokens_used = if response.usage.total_tokens > 0 {
        Some(TokenUsage {
            input: response.usage.input_tokens,
            output: response.usage.output_tokens,
            total: response.usage.total_tokens,
        })
    } else {
        None
    };
    GenerateResponseResult {
        text: response.text.trim().to_string(),
        model,
        response_time_ms: end_ms.saturating_sub(start_ms),
        timestamp: end_ms,
        tokens_used,
    }
}

/// Current unix-ms timestamp. Private helper — internal use only.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

// ─── Pure prompt builder ──────────────────────────────────────────────

/// Build the full message array sent to the local inference provider.
///
/// Pure — no I/O, no clock. Caller (PR-2's `generate_response`) passes
/// the current time so this function stays deterministic in tests.
///
/// Composition order matches the TS implementation:
///   1. System prompt (if `context.system_prompt` is set)
///   2. Conversation history with `[HH:MM] {name}: {content}` rows,
///      interspersed with `⏱️ N hours passed` markers for gaps > 1h
///   3. Final identity-reminder system message with persona name +
///      members + current time + the critical-topic-detection protocol
pub fn build_response_messages(
    context: &AIDecisionContext,
    current_time_ms: u64,
) -> Vec<ChatMessage> {
    let mut messages: Vec<ChatMessage> = Vec::new();

    // 1. System prompt
    if let Some(prompt) = context.system_prompt.as_deref() {
        if !prompt.is_empty() {
            messages.push(ChatMessage {
                role: "system".to_string(),
                content: MessageContent::Text(prompt.to_string()),
                name: None,
            });
        }
    }

    // 2. Conversation history with time prefix + hour-gap markers
    let mut last_timestamp: Option<u64> = None;
    for msg in &context.rag_context.conversation_history {
        let time_prefix = format_time_prefix(msg.timestamp);

        if let (Some(prev), Some(now)) = (last_timestamp, msg.timestamp) {
            if now > prev {
                if let Some(marker) = hour_gap_marker(now - prev) {
                    messages.push(ChatMessage {
                        role: "system".to_string(),
                        content: MessageContent::Text(marker),
                        name: None,
                    });
                }
            }
        }

        if msg.timestamp.is_some() {
            last_timestamp = msg.timestamp;
        }

        let formatted_content = match &msg.name {
            Some(name) => format!("{time_prefix}{name}: {}", msg.content),
            None => format!("{time_prefix}{}", msg.content),
        };

        messages.push(ChatMessage {
            role: msg.role.clone(),
            content: MessageContent::Text(formatted_content),
            name: None,
        });
    }

    // 3. Identity reminder at end
    let system_prompt_body = context.system_prompt.as_deref().unwrap_or("");
    let members = extract_room_members(system_prompt_body);
    let current_time = format_current_time(current_time_ms);
    let reminder = build_identity_reminder(&context.persona_name, members, &current_time);
    messages.push(ChatMessage {
        role: "system".to_string(),
        content: MessageContent::Text(reminder),
        name: None,
    });

    messages
}

/// Format the canonical identity-reminder system message. Mirrors the
/// TS template byte-for-byte modulo substitutions. Public so PR-2's
/// observability can log a snippet without re-building the whole
/// message list.
pub fn build_identity_reminder(persona_name: &str, members: &str, current_time: &str) -> String {
    format!(
        "IDENTITY REMINDER: You are {persona_name}. Respond naturally with JUST your message - NO name prefix, NO \"A:\" or \"H:\" labels, NO fake conversations. The room has ONLY these people: {members}.\n\
\n\
CURRENT TIME: {current_time}\n\
\n\
CRITICAL TOPIC DETECTION PROTOCOL:\n\
\n\
Step 1: Check for EXPLICIT TOPIC MARKERS in the most recent message\n\
- \"New topic:\", \"Different question:\", \"Changing subjects:\", \"Unrelated, but...\"\n\
- If present: STOP. Ignore ALL previous context. This is a NEW conversation.\n\
\n\
Step 2: Extract HARD CONSTRAINTS from the most recent message\n\
- Look for: \"NOT\", \"DON'T\", \"WITHOUT\", \"NEVER\", \"AVOID\", \"NO\"\n\
- Example: \"NOT triggering the app to foreground\" = YOUR SOLUTION MUST NOT DO THIS\n\
- Example: \"WITHOUT user interaction\" = YOUR SOLUTION MUST BE AUTOMATIC\n\
- Your answer MUST respect these constraints or you're wrong.\n\
\n\
Step 3: Compare SUBJECT of most recent message to previous 2-3 messages\n\
- Previous: \"Worker Threads\" → Recent: \"Webview authentication\" = DIFFERENT SUBJECTS\n\
- Previous: \"TypeScript code\" → Recent: \"What's 2+2?\" = TEST QUESTION\n\
- Previous: \"Worker pools\" → Recent: \"Should I use 5 or 10 workers?\" = SAME SUBJECT\n\
\n\
Step 4: Determine response strategy\n\
IF EXPLICIT TOPIC MARKER or COMPLETELY DIFFERENT SUBJECT:\n\
- Respond ONLY to the new topic\n\
- Ignore old messages (they're from a previous discussion)\n\
- Focus 100% on the most recent message\n\
- Address the constraints explicitly\n\
\n\
IF SAME SUBJECT (continued conversation):\n\
- Use full conversation context\n\
- Build on previous responses\n\
- Still check for NEW constraints in the recent message\n\
- Avoid redundancy\n\
\n\
CRITICAL READING COMPREHENSION:\n\
- Read the ENTIRE most recent message carefully\n\
- Don't skim - every word matters\n\
- Constraints are REQUIREMENTS, not suggestions\n\
- If the user says \"NOT X\", suggesting X is a failure\n\
\n\
Time gaps > 1 hour usually indicate topic changes, but IMMEDIATE semantic shifts (consecutive messages about different subjects) are also topic changes."
    )
}

/// Extract the `Current room members: ...` line from a system prompt
/// body. Returns the captured contents up to the next newline.
/// Returns `UNKNOWN_MEMBERS` if no match — same fallback as TS.
pub fn extract_room_members(system_prompt: &str) -> &str {
    const PREFIX: &str = "Current room members: ";
    let Some(start) = system_prompt.find(PREFIX) else {
        return UNKNOWN_MEMBERS;
    };
    let after = &system_prompt[start + PREFIX.len()..];
    let end = after.find('\n').unwrap_or(after.len());
    let captured = after[..end].trim_end();
    if captured.is_empty() {
        UNKNOWN_MEMBERS
    } else {
        captured
    }
}

/// Format a unix-ms timestamp as UTC `MM/DD/YYYY HH:MM` — the format
/// the TS implementation used (via `toLocaleDateString` /
/// `toLocaleTimeString`). UTC instead of local timezone removes the
/// host-TZ dependency that the TS version had.
pub fn format_current_time(time_ms: u64) -> String {
    let dt = DateTime::<Utc>::from_timestamp_millis(time_ms as i64)
        .unwrap_or_else(Utc::now);
    dt.format("%m/%d/%Y %H:%M").to_string()
}

/// Format a unix-ms timestamp as `[HH:MM] ` UTC for inline prefixing
/// of conversation messages. Returns empty string when timestamp is
/// missing — same as TS `if (msg.timestamp)` guard.
fn format_time_prefix(timestamp_ms: Option<u64>) -> String {
    let Some(ms) = timestamp_ms else {
        return String::new();
    };
    let total_seconds = ms / 1000;
    let hours = (total_seconds / 3600) % 24;
    let minutes = (total_seconds / 60) % 60;
    format!("[{hours:02}:{minutes:02}] ")
}

/// Return a `⏱️ N hour passed` marker if `gap_ms` exceeds the
/// threshold. Returns `None` for gaps under 1 hour. Matches TS
/// `Math.floor(gapMinutes / 60)` semantics.
fn hour_gap_marker(gap_ms: u64) -> Option<String> {
    if gap_ms < HOUR_GAP_THRESHOLD_MS {
        return None;
    }
    let gap_hours = gap_ms / HOUR_GAP_THRESHOLD_MS;
    let plural = if gap_hours > 1 { "s" } else { "" };
    Some(format!(
        "⏱️ {gap_hours} hour{plural} passed - conversation resumed"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::should_respond::{
        AIDecisionContext, GatingConversationMessage, GatingMessageContent, GatingRagContext,
        GatingRagMetadata, GatingTriggerMessage,
    };

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

    fn ctx(system_prompt: Option<&str>, history: Vec<GatingConversationMessage>) -> AIDecisionContext {
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
        let prompt = "You are a helpful AI.\nCurrent room members: alice, bob, carol\nMore text below.";
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
    /// `UNKNOWN_MEMBERS` fallback. Same string the TS version uses —
    /// downstream prompt machinery may depend on the literal value.
    #[test]
    fn extract_members_missing_returns_unknown() {
        let prompt = "Generic system prompt with no members line.";
        assert_eq!(extract_room_members(prompt), UNKNOWN_MEMBERS);
        assert_eq!(extract_room_members(""), UNKNOWN_MEMBERS);
    }

    /// What this catches: empty members list (just whitespace after the
    /// prefix) falls back to `UNKNOWN_MEMBERS` — avoids emitting a
    /// prompt that says "the room has ONLY these people: ." which is
    /// worse than the honest fallback.
    #[test]
    fn extract_members_empty_after_prefix_returns_unknown() {
        let prompt = "Current room members: \nSomething else.";
        assert_eq!(extract_room_members(prompt), UNKNOWN_MEMBERS);
    }

    // ─── format_time_prefix ───────────────────────────────────────────

    /// What this catches: present timestamp renders as `[HH:MM] ` UTC.
    /// Same shape as `check_redundancy.rs` for consistency.
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
    /// but still emits the identity reminder. Mirrors TS guard `if
    /// (context.systemPrompt ?? ...)`.
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
        assert_eq!(messages.len(), 1, "only identity reminder; no empty system row");
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
    /// (TS: `if (msg.timestamp) { ... lastTimestamp = msg.timestamp; }`)
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
    /// prefix + content (no `name: ` chunk). Mirrors TS ternary on
    /// `msg.name`.
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
    /// fallback string. Same TS behavior — no panic on a recipe-less
    /// room.
    #[test]
    fn build_response_messages_unknown_members_when_prompt_missing_line() {
        let context = ctx(Some("Generic system prompt."), vec![]);
        let messages = build_response_messages(&context, 0);
        let reminder = text_of(messages.last().expect("identity reminder present"));
        assert!(
            reminder.contains(&format!("ONLY these people: {UNKNOWN_MEMBERS}.")),
            "missing members line must render fallback; got: {reminder}"
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
    /// original case + spelling. The TS version casts `msg.role as
    /// 'user' | 'assistant'` blindly — Rust preserves whatever string
    /// the message carried, which is the correct conservative choice
    /// (provider routing depends on these exact strings).
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
    /// TextGenerationRequest with provider="local", model=Qwen-default,
    /// temperature=0.7, max_tokens=150, response_format=Text,
    /// purpose="cognition/generate-response", and persona/room
    /// attribution carried from the context. Pins the wire shape so
    /// downstream provider routing doesn't drift silently.
    #[test]
    fn generation_request_uses_documented_defaults() {
        let request = request_with_overrides(None, None, None, None);
        let inference = build_response_generation_request(
            &request,
            DEFAULT_GENERATE_MODEL.to_string(),
            0,
        );
        assert_eq!(inference.provider.as_deref(), Some(DEFAULT_GENERATE_PROVIDER));
        assert_eq!(inference.model.as_deref(), Some(DEFAULT_GENERATE_MODEL));
        assert_eq!(inference.temperature, Some(DEFAULT_GENERATE_TEMPERATURE));
        assert_eq!(inference.max_tokens, Some(DEFAULT_GENERATE_MAX_TOKENS));
        assert_eq!(inference.purpose.as_deref(), Some("cognition/generate-response"));
        assert_eq!(inference.persona_id.as_deref(), Some("p-001"));
        assert_eq!(inference.room_id.as_deref(), Some("r-001"));
        assert!(matches!(inference.response_format, Some(ResponseFormat::Text)));
        // messages list = system prompt + identity reminder for an empty history
        assert_eq!(inference.messages.len(), 2);
    }

    /// What this catches: per-request overrides actually override
    /// (temperature, max_tokens, model). Without this, a caller passing
    /// `temperature=0.1` would silently get the default 0.7.
    #[test]
    fn generation_request_honors_overrides() {
        let request = request_with_overrides(Some("custom-model"), Some(0.1), Some(500), None);
        let inference = build_response_generation_request(
            &request,
            "custom-model".to_string(),
            0,
        );
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
            DEFAULT_GENERATE_MODEL.to_string(),
            1_700_000_000_000,
        );
        let identity = match &inference.messages.last().expect("identity present").content {
            MessageContent::Text(s) => s.clone(),
            _ => panic!("non-text identity"),
        };
        assert!(identity.contains("CURRENT TIME: 11/14/2023 22:13"));
    }

    // ─── result_from_response ─────────────────────────────────────────

    fn fake_response(text: &str, total_tokens: u32, input: u32, output: u32) -> TextGenerationResponse {
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
            routing: None,
            error: None,
        }
    }

    /// What this catches: result trims surrounding whitespace from the
    /// provider's text — TS does `response.text.trim()`. Models often
    /// emit leading/trailing newlines; without trim the chat surface
    /// gets extra blank lines.
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

    /// What this catches: total_tokens == 0 -> None. Mirrors TS
    /// truthiness check on usage object; avoids emitting
    /// `{input:0, output:0, total:0}` as if the provider had measured
    /// (usually means the provider didn't instrument usage at all).
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
        let err = GenerateResponseError::Timeout { timeout_ms: 180_000 };
        let s = format!("{err}");
        assert!(s.contains("180000"));
    }
}
