//! Rust-owned "is my draft response redundant?" check.
//!
//! Oxidizer for `AIDecisionService.checkRedundancy` (TS, see
//! `src/system/ai/server/AIDecisionService.ts:165-308`). Mirrors the
//! shape of `should_respond.rs` — the gating arm that already moved to
//! Rust. TypeScript will continue to own slot coordination + logging;
//! Rust owns the redundancy-check decision contract, prompt
//! construction, and response parsing.
//!
//! ## Scope of this PR (PR-1 — pure types + prompt + parser)
//!
//! - `RedundancyCheckRequest` — IPC request shape (ts-rs exported)
//! - `RedundancyDecision` — IPC response shape (ts-rs exported)
//! - `ParsedRedundancyResponse` — internal parser output (no timestamp /
//!   model — those get filled by the caller of `evaluate_redundancy` in
//!   PR-2)
//! - `RedundancyParseError` — typed parser errors
//! - `build_redundancy_prompt(&AIDecisionContext, draft_text) -> String`
//!   — pure
//! - `parse_redundancy_response(&str) -> Result<ParsedRedundancyResponse,
//!   RedundancyParseError>` — pure
//!
//! ## NOT in this PR (deferred)
//!
//! - **PR-2**: `cognition/check-redundancy` IPC handler — composes
//!   build_redundancy_prompt → AI provider call (via existing Groq
//!   router) → parse_redundancy_response → RedundancyDecision (with
//!   model + timestamp set).
//! - **PR-3**: TS `AIDecisionService.checkRedundancy` shim — replaces
//!   inline prompt + `AIProviderDaemon.generateText` with the IPC call.
//! - **PR-4**: Delete dead TS code (the inline prompt template + JSON
//!   parsing — should have no remaining production callers after PR-3).
//!
//! ## Failure-mode discipline
//!
//! Same posture as `should_respond.rs`: the parser is total (always
//! returns `Result`, never panics), no silent default-on-error. Callers
//! decide whether to "fail open" (treat malformed as not-redundant —
//! preserves autonomy) or "fail closed" — both are explicit choices on
//! `Result` rather than hidden defaults inside the parser.
//!
//! ## TS source-of-truth note
//!
//! The prompt template here is the canonical version. Once PR-3 lands
//! the TS shim, the TS-side prompt body should be deleted entirely (no
//! drift surface). The current TS file uses the legacy template; this
//! Rust version is byte-for-byte the same modulo a `format!` call.

use crate::ai::types::ResponseFormat;
use crate::ai::{ChatMessage, MessageContent, TextGenerationRequest};
use crate::cognition::should_respond::{AIDecisionContext, GatingConversationMessage};
use crate::modules::ai_provider::{generate_text, global_registry};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};
use ts_rs::TS;

/// Maximum number of recent conversation messages included in the
/// redundancy-check prompt. Matches the TS implementation's
/// `slice(-10)` behavior.
pub const REDUNDANCY_CONVERSATION_WINDOW: usize = 10;

const REDUNDANCY_PROVIDER: &str = "groq";
const DEFAULT_REDUNDANCY_MODEL: &str = "llama-3.1-8b-instant";
const DEFAULT_REDUNDANCY_TEMPERATURE: f32 = 0.2;

// ─── IPC request + response shapes ────────────────────────────────────

/// IPC request: ask the cognition service whether a draft response is
/// redundant given the conversation so far.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/RedundancyCheckRequest.ts"
)]
pub struct RedundancyCheckRequest {
    /// Reuses the gating context — same shape, same source. The
    /// `trigger_message` is informational here; the parser uses
    /// `rag_context.conversation_history` to detect redundancy.
    pub context: AIDecisionContext,
    /// The draft response we want to check.
    pub draft_text: String,
    /// Optional model override. PR-2 defaults to the same Groq model
    /// the gating arm uses (cheap + fast) when unset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
}

/// IPC response: the redundancy decision plus the model that produced
/// it and the timestamp it was produced at.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/RedundancyDecision.ts"
)]
pub struct RedundancyDecision {
    pub is_redundant: bool,
    pub reason: String,
    pub model: String,
    #[ts(type = "number")]
    pub timestamp: u64,
}

/// Internal parser output — what the AI's text response decoded to,
/// before the caller stamps it with `model` + `timestamp`.
/// Not ts-rs exported; this never crosses the IPC seam.
#[derive(Debug, Clone, PartialEq)]
pub struct ParsedRedundancyResponse {
    pub is_redundant: bool,
    pub reason: String,
}

#[derive(Debug, thiserror::Error)]
pub enum RedundancyEvaluateError {
    #[error("generation failed: {0}")]
    Generation(String),
    #[error("parse failed: {0}")]
    Parse(#[from] RedundancyParseError),
}

/// Typed parser errors. The caller (PR-2's `evaluate_redundancy`)
/// decides the fail-open / fail-closed policy — this module never
/// invents a default; the parser only reports what went wrong.
#[derive(Debug, thiserror::Error, PartialEq)]
pub enum RedundancyParseError {
    /// AI text contained no JSON-object substring. Could be a refusal,
    /// markdown wrapping the wrong way, or a model that ignored the
    /// "JSON only" instruction.
    #[error("no JSON object found in response: {0:?}")]
    NoJsonObject(String),
    /// JSON parsed but was malformed (not an object, or top-level wasn't
    /// a `{...}` Map).
    #[error("JSON did not contain an object body")]
    NotAnObject,
    /// The decoded JSON did not have the required `isRedundant` field
    /// (or it wasn't a bool). The cascade has no honest fallback here —
    /// caller must decide fail-open vs fail-closed explicitly.
    #[error("missing or non-boolean isRedundant field")]
    MissingIsRedundant,
}

/// Run the redundancy check against the registered AI provider.
///
/// No fallback path: provider failures and malformed model output return
/// typed errors so the caller chooses its policy explicitly.
pub async fn evaluate_redundancy(
    request: RedundancyCheckRequest,
) -> Result<RedundancyDecision, RedundancyEvaluateError> {
    let model = request
        .model
        .clone()
        .unwrap_or_else(|| DEFAULT_REDUNDANCY_MODEL.to_string());
    let inference_request = build_redundancy_generation_request(&request, model.clone());

    let registry = global_registry();
    let registry_guard = registry.read().await;
    let response = generate_text(&registry_guard, inference_request)
        .await
        .map_err(RedundancyEvaluateError::Generation)?;

    let parsed = parse_redundancy_response(&response.text)?;
    Ok(decision_from_parsed(parsed, model, now_ms()))
}

fn build_redundancy_generation_request(
    request: &RedundancyCheckRequest,
    model: String,
) -> TextGenerationRequest {
    TextGenerationRequest {
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: MessageContent::Text(
                    "You decide whether a draft response repeats an answer already present. Respond ONLY with JSON."
                        .to_string(),
                ),
                name: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text(build_redundancy_prompt(
                    &request.context,
                    &request.draft_text,
                )),
                name: None,
            },
        ],
        system_prompt: None,
        model: Some(model),
        provider: Some(REDUNDANCY_PROVIDER.to_string()),
        temperature: Some(DEFAULT_REDUNDANCY_TEMPERATURE),
        // Model owns its length (None → adapter forwards no ceiling). The prompt asks
        // for a short verdict; we never cap generation with a const of our own.
        max_tokens: None,
        top_p: None,
        top_k: None,
        repeat_penalty: None,
        frequency_penalty: None,
        repeat_last_n: None,
        stop_sequences: None,
        tools: None,
        tool_choice: None,
        response_format: Some(ResponseFormat::JsonObject),
        active_adapters: None,
        request_id: None,
        user_id: None,
        room_id: Some(request.context.room_id.clone()),
        purpose: Some("cognition/check-redundancy".to_string()),
        persona_id: Some(request.context.persona_id.clone()),
    }
}

fn decision_from_parsed(
    parsed: ParsedRedundancyResponse,
    model: String,
    timestamp: u64,
) -> RedundancyDecision {
    RedundancyDecision {
        is_redundant: parsed.is_redundant,
        reason: parsed.reason,
        model,
        timestamp,
    }
}

// ─── Pure prompt builder ──────────────────────────────────────────────

/// Build the prompt sent to the redundancy-check model. Pure — no I/O,
/// no clock, no global state.
///
/// Takes the same `AIDecisionContext` the gating arm uses, plus the
/// draft response we're checking. Uses the most recent
/// `REDUNDANCY_CONVERSATION_WINDOW` messages from the rag context.
pub fn build_redundancy_prompt(context: &AIDecisionContext, draft_text: &str) -> String {
    let recent: Vec<&GatingConversationMessage> = context
        .rag_context
        .conversation_history
        .iter()
        .rev()
        .take(REDUNDANCY_CONVERSATION_WINDOW)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    let conversation_text = recent
        .iter()
        .map(|msg| {
            let speaker = msg.name.as_deref().unwrap_or(&msg.role);
            let time_prefix = format_time_prefix(msg.timestamp);
            format!("{time_prefix}{speaker}: {}", msg.content)
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "**Recent conversation (includes questions and answers):**\n\
{conversation_text}\n\n\
**My draft response:**\n\
{draft_text}\n\n\
**Critical Question**: Has the ORIGINAL question/topic that I'm responding to been adequately answered already?\n\n\
**IMPORTANT Guidelines**:\n\
- **UNANSWERED question = NOT redundant** (even if other topics were discussed)\n\
- **PARTIALLY answered = NOT redundant** (can add more detail)\n\
- Same answer to SAME question = REDUNDANT\n\
- Correcting a wrong answer = NOT redundant\n\
- **NEW question after time gap = NOT redundant**\n\
- Different programming language/framework = NOT redundant\n\n\
**Respond with JSON only:**\n\
{{\n\
  \"isRedundant\": true/false,\n\
  \"reason\": \"brief explanation\"\n\
}}"
    )
}

/// Format a unix-ms timestamp as `[HH:MM] ` for prompt readability.
/// Returns empty string when timestamp is missing (TS version does the
/// same — no spurious `[00:00] ` for clockless messages).
fn format_time_prefix(timestamp_ms: Option<u64>) -> String {
    let Some(ms) = timestamp_ms else {
        return String::new();
    };
    // Render in UTC. The TS version uses local timezone; for the
    // prompt-builder layer that's a presentation detail the model
    // ignores anyway. Keeping UTC removes a hidden TZ dependency from
    // a function that should be pure.
    let total_seconds = ms / 1000;
    let hours = (total_seconds / 3600) % 24;
    let minutes = (total_seconds / 60) % 60;
    format!("[{hours:02}:{minutes:02}] ")
}

// ─── Pure response parser ─────────────────────────────────────────────

/// Parse the AI's text response into a `ParsedRedundancyResponse`.
/// Pure — no I/O, no clock. Returns `Err` for malformed inputs; caller
/// decides fail-open vs fail-closed.
pub fn parse_redundancy_response(
    ai_text: &str,
) -> Result<ParsedRedundancyResponse, RedundancyParseError> {
    let json = extract_json_object(ai_text)
        .ok_or_else(|| RedundancyParseError::NoJsonObject(snippet(ai_text)))?;
    let value: Value = serde_json::from_str(json)
        .map_err(|_| RedundancyParseError::NoJsonObject(snippet(json)))?;
    let obj = value.as_object().ok_or(RedundancyParseError::NotAnObject)?;
    let is_redundant = obj
        .get("isRedundant")
        .and_then(Value::as_bool)
        .ok_or(RedundancyParseError::MissingIsRedundant)?;
    let reason = obj
        .get("reason")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| "No reason provided".to_string());
    Ok(ParsedRedundancyResponse {
        is_redundant,
        reason,
    })
}

/// Pull the first balanced `{...}` substring from `text`. Duplicated
/// from `should_respond.rs` for the PR-1 atomic slice — promoting to a
/// shared `cognition/util.rs` is a separate concern (and would mix
/// concerns into this PR).
fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let mut depth = 0_i32;
    for (i, c) in text[start..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&text[start..start + i + 1]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Truncate a string for inclusion in error messages — bounded so
/// `RedundancyParseError::NoJsonObject` doesn't carry a megabyte of
/// upstream garbage.
fn snippet(s: &str) -> String {
    const MAX: usize = 200;
    if s.len() <= MAX {
        s.to_string()
    } else {
        format!("{}…", &s[..MAX])
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
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

    fn ctx_with_history(history: Vec<GatingConversationMessage>) -> AIDecisionContext {
        AIDecisionContext {
            persona_id: "p-001".to_string(),
            persona_name: "TestPersona".to_string(),
            room_id: "r-001".to_string(),
            trigger_message: GatingTriggerMessage {
                id: "m-trigger".to_string(),
                sender_name: "alice".to_string(),
                content: GatingMessageContent {
                    text: "any trigger".to_string(),
                },
            },
            rag_context: GatingRagContext {
                conversation_history: history,
                recipe_strategy: None,
                metadata: GatingRagMetadata { recipe_name: None },
            },
            system_prompt: None,
        }
    }

    // ─── build_redundancy_prompt ──────────────────────────────────────

    /// What this catches: the prompt embeds the draft text verbatim and
    /// the recent conversation in the canonical "[HH:MM] speaker: content"
    /// shape. If the formatter regresses, the AI model sees garbage and
    /// the redundancy detector's accuracy collapses.
    #[test]
    fn prompt_embeds_draft_and_conversation_lines() {
        let ctx = ctx_with_history(vec![
            msg(
                "user",
                Some("alice"),
                "what is 2+2?",
                Some(1_700_000_000_000),
            ),
            msg("assistant", Some("bob"), "4", Some(1_700_000_060_000)),
        ]);
        let prompt = build_redundancy_prompt(&ctx, "Actually it's 4.");
        assert!(prompt.contains("Actually it's 4."), "draft text missing");
        assert!(prompt.contains("alice: what is 2+2?"), "alice line missing");
        assert!(prompt.contains("bob: 4"), "bob line missing");
        // Time prefix renders in UTC: 1_700_000_000_000 ms = 2023-11-14 22:13:20 UTC
        assert!(prompt.contains("[22:13]"), "time prefix missing");
    }

    /// What this catches: messages without a `name` fall back to `role`
    /// — matches the TS `msg.name ?? msg.role` shape. If this regresses
    /// the prompt shows `assistant: foo` even when a persona name was
    /// available, hurting the redundancy detector's ability to attribute.
    #[test]
    fn prompt_falls_back_to_role_when_name_missing() {
        let ctx = ctx_with_history(vec![msg("system", None, "hello", None)]);
        let prompt = build_redundancy_prompt(&ctx, "draft");
        assert!(
            prompt.contains("system: hello"),
            "should use role when name is absent"
        );
    }

    /// What this catches: messages without timestamp do NOT get a
    /// spurious `[00:00] ` prefix. The TS version checks the timestamp
    /// before rendering; this pins parity.
    #[test]
    fn prompt_omits_time_prefix_when_timestamp_missing() {
        let ctx = ctx_with_history(vec![msg("user", Some("alice"), "hi", None)]);
        let prompt = build_redundancy_prompt(&ctx, "draft");
        assert!(prompt.contains("alice: hi"), "should still render the line");
        assert!(
            !prompt.contains("[00:00]"),
            "no time prefix expected when timestamp is None"
        );
    }

    /// What this catches: only the last
    /// REDUNDANCY_CONVERSATION_WINDOW messages are included, and they
    /// appear in chronological order (oldest first). The TS version
    /// does `slice(-10)` which preserves chronological order; pinning
    /// the same here so the AI sees recency at the bottom.
    #[test]
    fn prompt_uses_only_last_n_messages_in_chronological_order() {
        let mut history = Vec::new();
        // 15 messages — older than window should be dropped
        for i in 0..15 {
            history.push(msg(
                "user",
                Some("alice"),
                &format!("msg-{i}"),
                Some(1_700_000_000_000 + i * 60_000),
            ));
        }
        let ctx = ctx_with_history(history);
        let prompt = build_redundancy_prompt(&ctx, "draft");
        // Messages 0..4 should NOT appear (older than window of 10)
        for i in 0..5 {
            assert!(
                !prompt.contains(&format!("msg-{i}\n"))
                    && !prompt.contains(&format!("msg-{i}\n\n")),
                "msg-{i} should be dropped (older than window)"
            );
        }
        // Messages 5..14 should appear in order
        for i in 5..15 {
            assert!(
                prompt.contains(&format!("msg-{i}")),
                "msg-{i} should be in window"
            );
        }
        // Chronological order: msg-5 appears BEFORE msg-14
        let pos_5 = prompt.find("msg-5").expect("msg-5 in prompt");
        let pos_14 = prompt.find("msg-14").expect("msg-14 in prompt");
        assert!(pos_5 < pos_14, "chronological order: oldest first");
    }

    /// What this catches: empty conversation history still produces a
    /// valid prompt (the JSON instructions + draft text section), just
    /// with an empty conversation block. Avoids a panic on a fresh
    /// persona's first turn.
    #[test]
    fn prompt_handles_empty_conversation() {
        let ctx = ctx_with_history(vec![]);
        let prompt = build_redundancy_prompt(&ctx, "draft");
        assert!(prompt.contains("**My draft response:**\ndraft"));
        assert!(prompt.contains("Respond with JSON only"));
    }

    /// What this catches: the JSON-only instruction is rendered without
    /// `format!` mangling the literal `{` `}` braces. If brace escaping
    /// breaks, the model would see `Respond with JSON only:` with no
    /// example schema after it — and the parser would see free-form
    /// text instead of `{ "isRedundant": ... }`.
    #[test]
    fn prompt_includes_unescaped_json_schema_example() {
        let ctx = ctx_with_history(vec![]);
        let prompt = build_redundancy_prompt(&ctx, "draft");
        assert!(
            prompt.contains("\"isRedundant\": true/false"),
            "JSON schema example missing"
        );
        assert!(
            prompt.contains("\"reason\": \"brief explanation\""),
            "JSON reason field example missing"
        );
    }

    // ─── evaluate_redundancy orchestration seams ─────────────────────

    /// What this catches: the async evaluator's provider request stays
    /// constrained to JSON, attributed to the persona + room, and routed
    /// through the intended fast Groq model. This is the no-network proof
    /// for the IPC orchestration shape; the provider registry itself is
    /// covered by ai_provider tests.
    #[test]
    fn generation_request_uses_json_mode_and_persona_metadata() {
        let ctx = ctx_with_history(vec![msg("user", Some("alice"), "answered already", None)]);
        let request = RedundancyCheckRequest {
            context: ctx,
            draft_text: "same answer".to_string(),
            model: None,
        };

        let inference =
            build_redundancy_generation_request(&request, DEFAULT_REDUNDANCY_MODEL.to_string());

        assert_eq!(inference.provider.as_deref(), Some(REDUNDANCY_PROVIDER));
        assert_eq!(inference.model.as_deref(), Some(DEFAULT_REDUNDANCY_MODEL));
        assert_eq!(inference.temperature, Some(DEFAULT_REDUNDANCY_TEMPERATURE));
        // No client-imposed ceiling — the model owns its generation length.
        assert_eq!(inference.max_tokens, None);
        assert_eq!(
            inference.response_format,
            Some(crate::ai::types::ResponseFormat::JsonObject)
        );
        assert_eq!(inference.room_id.as_deref(), Some("r-001"));
        assert_eq!(inference.persona_id.as_deref(), Some("p-001"));
        assert_eq!(
            inference.purpose.as_deref(),
            Some("cognition/check-redundancy")
        );
        assert_eq!(inference.messages.len(), 2);

        match &inference.messages[1].content {
            MessageContent::Text(prompt) => {
                assert!(prompt.contains("answered already"));
                assert!(prompt.contains("same answer"));
            }
            other => panic!("expected text prompt, got {other:?}"),
        }
    }

    /// What this catches: per-call model override is honored without
    /// changing provider, JSON mode, or attribution. This keeps the
    /// command flexible for hardware-specific routing without allowing
    /// TS to own the prompt/parser contract.
    #[test]
    fn generation_request_honors_model_override() {
        let request = RedundancyCheckRequest {
            context: ctx_with_history(vec![]),
            draft_text: "draft".to_string(),
            model: Some("llama-3.3-70b-versatile".to_string()),
        };

        let inference =
            build_redundancy_generation_request(&request, request.model.clone().expect("override"));

        assert_eq!(inference.model.as_deref(), Some("llama-3.3-70b-versatile"));
        assert_eq!(inference.provider.as_deref(), Some(REDUNDANCY_PROVIDER));
    }

    /// What this catches: parser output is stamped into the wire response
    /// with the exact model + timestamp supplied by the evaluator. No
    /// hidden clock or provider read happens in the pure conversion seam.
    #[test]
    fn decision_from_parsed_stamps_model_and_timestamp() {
        let parsed = ParsedRedundancyResponse {
            is_redundant: false,
            reason: "new angle".to_string(),
        };

        let decision = decision_from_parsed(parsed, "model-x".to_string(), 42);

        assert_eq!(
            decision,
            RedundancyDecision {
                is_redundant: false,
                reason: "new angle".to_string(),
                model: "model-x".to_string(),
                timestamp: 42,
            }
        );
    }

    /// What this catches: the IPC request wire is camelCase and accepts
    /// the optional model field generated for TS callers.
    #[test]
    fn redundancy_check_request_serde_camelcase() {
        let request = RedundancyCheckRequest {
            context: ctx_with_history(vec![]),
            draft_text: "draft".to_string(),
            model: Some("model-x".to_string()),
        };

        let json = serde_json::to_string(&request).expect("serialize");

        assert!(json.contains("\"draftText\":\"draft\""));
        assert!(json.contains("\"model\":\"model-x\""));
        assert!(json.contains("\"personaId\":\"p-001\""));
    }

    // ─── parse_redundancy_response ────────────────────────────────────

    /// What this catches: happy path — bare JSON object with both
    /// fields parses to the expected `ParsedRedundancyResponse`.
    #[test]
    fn parse_bare_json_object() {
        let resp = parse_redundancy_response(r#"{"isRedundant": true, "reason": "same answer"}"#)
            .expect("happy path parse");
        assert_eq!(
            resp,
            ParsedRedundancyResponse {
                is_redundant: true,
                reason: "same answer".to_string(),
            }
        );
    }

    /// What this catches: the parser tolerates JSON wrapped in
    /// surrounding markdown / prose — same as the TS regex
    /// `match(/\{[\s\S]*\}/)`. Models often prefix "Here is the
    /// JSON:..." before the object; if the parser regresses to
    /// requiring bare JSON, every such response becomes a parse error.
    #[test]
    fn parse_extracts_json_from_surrounding_prose() {
        let ai_text = "Here is my analysis:\n\
            ```json\n\
            {\"isRedundant\": false, \"reason\": \"new question\"}\n\
            ```\n\
            Hope that helps.";
        let resp = parse_redundancy_response(ai_text).expect("should extract from prose");
        assert_eq!(resp.is_redundant, false);
        assert_eq!(resp.reason, "new question");
    }

    /// What this catches: missing `reason` field falls back to the
    /// canonical "No reason provided" string — matches the TS
    /// `parsed.reason ?? 'No reason provided'` behavior. If this
    /// regresses, downstream UI / logs would surface `null` or
    /// undefined.
    #[test]
    fn parse_uses_default_reason_when_missing() {
        let resp = parse_redundancy_response(r#"{"isRedundant": false}"#).expect("ok");
        assert_eq!(resp.is_redundant, false);
        assert_eq!(resp.reason, "No reason provided");
    }

    /// What this catches: no JSON object at all returns the typed
    /// `NoJsonObject` error with a bounded snippet of the input. Pure
    /// errors only — never `Ok(default)`.
    #[test]
    fn parse_no_json_returns_typed_err() {
        let result = parse_redundancy_response("I refuse to answer this question");
        match result {
            Err(RedundancyParseError::NoJsonObject(snip)) => {
                assert!(snip.contains("refuse"), "snippet should carry context");
            }
            other => panic!("expected NoJsonObject, got {other:?}"),
        }
    }

    /// What this catches: malformed JSON (unterminated brace) returns
    /// `NoJsonObject` — the extractor needs balanced braces, so an open
    /// `{` with no matching `}` is functionally "no JSON found".
    #[test]
    fn parse_unbalanced_braces_returns_typed_err() {
        let result = parse_redundancy_response("{\"isRedundant\": true ");
        assert!(matches!(result, Err(RedundancyParseError::NoJsonObject(_))));
    }

    /// What this catches: JSON parsed to a non-object (array, number,
    /// string) returns `NotAnObject` distinctly from `NoJsonObject`.
    /// The model returning `["true", "same"]` is a different failure
    /// than the model refusing — caller can react differently.
    #[test]
    fn parse_top_level_array_returns_not_an_object_err() {
        // The extractor only looks for `{...}`. An array `[...]` won't
        // match — so this is `NoJsonObject` rather than `NotAnObject`.
        // A `{...}` that happens to decode to a non-object Value is
        // currently unreachable through extract_json_object + serde
        // because `{...}` always decodes to a Value::Object. The variant
        // exists for future hardening (e.g., if the extractor changes
        // to accept top-level arrays).
        let result = parse_redundancy_response("[\"isRedundant\", true]");
        assert!(matches!(result, Err(RedundancyParseError::NoJsonObject(_))));
    }

    /// What this catches: missing the required `isRedundant` field
    /// returns the distinct `MissingIsRedundant` error — caller can
    /// distinguish "model returned JSON with the wrong schema" from
    /// "model returned no JSON at all" and react accordingly.
    #[test]
    fn parse_missing_is_redundant_returns_typed_err() {
        let result = parse_redundancy_response(r#"{"reason": "vague"}"#);
        assert!(matches!(
            result,
            Err(RedundancyParseError::MissingIsRedundant)
        ));
    }

    /// What this catches: non-boolean `isRedundant` (string "true"
    /// instead of `true`) also returns `MissingIsRedundant`. Strict
    /// type contract — no silent coerce from string truthiness.
    #[test]
    fn parse_non_boolean_is_redundant_returns_typed_err() {
        let result = parse_redundancy_response(r#"{"isRedundant": "true", "reason": "x"}"#);
        assert!(matches!(
            result,
            Err(RedundancyParseError::MissingIsRedundant)
        ));
    }

    /// What this catches: nested JSON inside the response (e.g. model
    /// wraps its decision in an outer envelope) — the extractor pulls
    /// the FIRST balanced object, which would be the outer envelope.
    /// Pins this behavior so a future change to extract the "best
    /// candidate" doesn't silently flip semantics.
    #[test]
    fn parse_extracts_first_balanced_object_when_nested() {
        let ai_text = r#"{"isRedundant": true, "reason": "outer", "meta": {"inner": "field"}}"#;
        let resp = parse_redundancy_response(ai_text).expect("ok");
        assert_eq!(resp.is_redundant, true);
        assert_eq!(resp.reason, "outer");
    }

    // ─── snippet bounding ─────────────────────────────────────────────

    /// What this catches: the error-context snippet is bounded so a
    /// megabyte of upstream garbage doesn't end up in a typed error +
    /// log line. Pins the 200-char limit + ellipsis marker.
    #[test]
    fn snippet_truncates_long_input() {
        let huge = "x".repeat(10_000);
        let result = parse_redundancy_response(&huge);
        match result {
            Err(RedundancyParseError::NoJsonObject(s)) => {
                // 200-byte ASCII prefix + 3-byte UTF-8 ellipsis '…' = 203 bytes.
                assert!(s.len() <= 203, "snippet should be bounded; got {}", s.len());
                assert!(s.ends_with('…'), "long snippet should end with ellipsis");
            }
            other => panic!("expected NoJsonObject, got {other:?}"),
        }
    }
}
