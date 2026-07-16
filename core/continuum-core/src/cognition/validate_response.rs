//! Rust-owned response-validation decision.
//!
//! Oxidizer for `AIValidateResponseServerCommand` (TS, see
//! `src/commands/ai/validate-response/server/AIValidateResponseServerCommand.ts`).
//! Sibling to the closed `check_redundancy` (#1375) + `generate_response`
//! (#1385) oxidizers. Same shape, same discipline.
//!
//! Per Joel directive 2026-05-18 19:44Z: zero-users full-blown-Rust-dev
//! mode — this is shipped as ONE PR (add Rust + delete TS predecessor
//! in same commit), not the 4-PR migration cadence.
//!
//! ## Scope
//!
//! - `ValidateResponseRequest` (ts-rs) — IPC request
//! - `ValidateResponseDecision` (ts-rs) — IPC response carrying
//!   `decision: SUBMIT | CLARIFY | SILENT`, confidence, reason, model,
//!   timestamp
//! - `ResponseDecision` enum (ts-rs) — three-way decision shape
//! - `ValidateResponseError` — typed: NoAdapter, Generation
//! - `build_validate_prompt(&request) -> String` — pure
//! - `parse_decision(ai_text) -> ResponseDecision` — pure
//! - `evaluate_validate_response(request) -> Result<ValidateResponseDecision, _>`
//!   — async (calls Groq via existing registry, parses decision, stamps)
//!
//! ## Failure discipline
//!
//! - All errors typed.
//! - parse_decision defaults to SUBMIT when AI returns unrecognized text
//!   — matches TS behavior (the choice is "fail open: submit the draft"
//!   rather than "fail closed: silence the persona"). Documented at the
//!   parser; caller can compare against `decision == SUBMIT && reason
//!   == DEFAULT_REASON_SUBMIT` if they want to detect parse-fallthrough.
//! - No JSON parsing — model is asked for a single word, not JSON.
//!   Different from check_redundancy.

use crate::ai::adapter::InferenceDevice;
use crate::ai::types::ResponseFormat;
use crate::ai::{ChatMessage, MessageContent, TextGenerationRequest, TextGenerationResponse};
use crate::modules::ai_provider::global_registry;
use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use ts_rs::TS;

const VALIDATE_PROVIDER: &str = "groq";
const DEFAULT_VALIDATE_MODEL: &str = "llama-3.1-8b-instant";
const VALIDATE_TEMPERATURE: f32 = 0.1;
const VALIDATE_CONFIDENCE: f32 = 0.9;

const REASON_SUBMIT: &str = "Response appears relevant to the question";
const REASON_CLARIFY: &str = "Uncertain if response answers question, should ask for clarification";
const REASON_SILENT: &str = "Response is off-topic or does not address the question";

// ─── Wire types ───────────────────────────────────────────────────────

/// Three-way decision: SUBMIT (post the draft), CLARIFY (ask follow-up),
/// SILENT (drop the draft). Mirrors TS `ResponseDecision`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ResponseDecision.ts"
)]
pub enum ResponseDecision {
    #[serde(rename = "SUBMIT")]
    Submit,
    #[serde(rename = "CLARIFY")]
    Clarify,
    #[serde(rename = "SILENT")]
    Silent,
}

/// IPC request: ask cognition whether a draft response actually answers
/// the original question.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ValidateResponseRequest.ts"
)]
pub struct ValidateResponseRequest {
    pub generated_response: String,
    pub original_question: String,
    pub question_sender: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
}

/// IPC response: the validation decision + provenance.
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ValidateResponseDecision.ts"
)]
pub struct ValidateResponseDecision {
    pub decision: ResponseDecision,
    pub confidence: f32,
    pub reason: String,
    pub model: String,
    #[ts(type = "number")]
    pub timestamp: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum ValidateResponseError {
    #[error("no AI adapter for provider={provider:?} model={model:?}")]
    NoAdapter {
        provider: String,
        model: Option<String>,
    },
    #[error("generation failed: {0}")]
    Generation(String),
}

// ─── Pure prompt builder ──────────────────────────────────────────────

/// Build the one-word-answer prompt sent to the validator model. Pure.
pub fn build_validate_prompt(request: &ValidateResponseRequest) -> String {
    format!(
        "You generated this response:\n\
\"{}\"\n\
\n\
Original question from {}:\n\
\"{}\"\n\
\n\
Does your response actually answer their question?\n\
\n\
Reply with ONLY ONE WORD:\n\
- SUBMIT (your response clearly answers the question)\n\
- CLARIFY (you're unsure, should ask for clarification)\n\
- SILENT (your response is off-topic, stay silent)",
        request.generated_response, request.question_sender, request.original_question
    )
}

/// Parse the validator model's one-word answer. Pure.
///
/// Match precedence:
///   1. Contains "CLARIFY" → Clarify
///   2. Contains "SILENT" → Silent
///   3. Otherwise → Submit (fail-open default)
///
/// Mirrors TS `parseDecision` ordering exactly. The fail-open default
/// matches the TS behavior — when the validator can't decide, ship the
/// draft rather than silence the persona (silence is more user-hostile
/// than a slightly-off-topic response).
pub fn parse_decision(ai_text: &str) -> ResponseDecision {
    let upper = ai_text.trim().to_ascii_uppercase();
    if upper.contains("CLARIFY") {
        ResponseDecision::Clarify
    } else if upper.contains("SILENT") {
        ResponseDecision::Silent
    } else {
        ResponseDecision::Submit
    }
}

/// Canonical reason string for a decision — for callers that just want
/// to surface "why" without re-stringifying the variant. Pure.
pub fn reason_for(decision: ResponseDecision) -> &'static str {
    match decision {
        ResponseDecision::Submit => REASON_SUBMIT,
        ResponseDecision::Clarify => REASON_CLARIFY,
        ResponseDecision::Silent => REASON_SILENT,
    }
}

// ─── Async orchestrator (PR — IPC handler) ────────────────────────────

/// Run validation against the configured Groq adapter. No fallback path
/// — provider failures surface as typed errors so the caller decides
/// policy.
pub async fn evaluate_validate_response(
    request: ValidateResponseRequest,
) -> Result<ValidateResponseDecision, ValidateResponseError> {
    let model = request
        .model
        .clone()
        .unwrap_or_else(|| DEFAULT_VALIDATE_MODEL.to_string());
    let inference_request = build_validate_generation_request(&request, model.clone());

    let registry_arc = global_registry();
    let registry = registry_arc.read().await;
    // Device = `Auto` — cognition is model-driven, not device-driven.
    // See cognition/generate_response.rs:285 doctrine note.
    let (_provider_id, adapter) = registry
        .select(
            Some(VALIDATE_PROVIDER),
            Some(&model),
            InferenceDevice::Auto,
        )
        .ok_or_else(|| ValidateResponseError::NoAdapter {
            provider: VALIDATE_PROVIDER.to_string(),
            model: Some(model.clone()),
        })?;

    let response: TextGenerationResponse = adapter
        .generate_text(inference_request)
        .await
        .map_err(ValidateResponseError::Generation)?;

    let decision = parse_decision(&response.text);
    Ok(ValidateResponseDecision {
        decision,
        confidence: VALIDATE_CONFIDENCE,
        reason: reason_for(decision).to_string(),
        model,
        timestamp: now_ms(),
    })
}

fn build_validate_generation_request(
    request: &ValidateResponseRequest,
    model: String,
) -> TextGenerationRequest {
    TextGenerationRequest {
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: MessageContent::Text(
                    "You are a response validator. Reply ONLY with one word: SUBMIT, CLARIFY, or SILENT."
                        .to_string(),
                ),
                name: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text(build_validate_prompt(request)),
                name: None,
            },
        ],
        system_prompt: None,
        model: Some(model),
        provider: Some(VALIDATE_PROVIDER.to_string()),
        temperature: Some(VALIDATE_TEMPERATURE),
        // Model owns its length (None → adapter forwards no ceiling). A 10-token cap
        // here guaranteed an empty verdict on any reasoning model (the `<think>` block
        // alone exceeds it); brevity comes from the prompt + JSON response_format.
        max_tokens: None,
        top_p: None,
        top_k: None,
        repeat_penalty: None,
        frequency_penalty: None,
        repeat_last_n: None,
        stop_sequences: None,
        tools: None,
        tool_choice: None,
        response_format: Some(ResponseFormat::Text),
        active_adapters: None,
        request_id: None,
        user_id: None,
        room_id: None,
        purpose: Some("cognition/validate-response-decision".to_string()),
        persona_id: None,
    }
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn req(draft: &str, question: &str) -> ValidateResponseRequest {
        ValidateResponseRequest {
            generated_response: draft.to_string(),
            original_question: question.to_string(),
            question_sender: "alice".to_string(),
            model: None,
        }
    }

    // ─── build_validate_prompt ────────────────────────────────────────

    #[test]
    fn prompt_embeds_draft_question_sender() {
        let p = build_validate_prompt(&req("the answer is 42", "what is 2+2?"));
        assert!(p.contains("the answer is 42"));
        assert!(p.contains("what is 2+2?"));
        assert!(p.contains("from alice"));
    }

    #[test]
    fn prompt_includes_three_option_instructions() {
        let p = build_validate_prompt(&req("d", "q"));
        assert!(p.contains("- SUBMIT"));
        assert!(p.contains("- CLARIFY"));
        assert!(p.contains("- SILENT"));
        assert!(p.contains("ONLY ONE WORD"));
    }

    // ─── parse_decision ───────────────────────────────────────────────

    /// Bare SUBMIT → Submit.
    #[test]
    fn parse_bare_submit() {
        assert_eq!(parse_decision("SUBMIT"), ResponseDecision::Submit);
        assert_eq!(parse_decision("submit"), ResponseDecision::Submit);
    }

    /// CLARIFY wins over SUBMIT when text contains both (mirrors TS
    /// `if (text.includes('CLARIFY'))` taking precedence).
    #[test]
    fn parse_clarify_wins_when_present() {
        assert_eq!(parse_decision("CLARIFY"), ResponseDecision::Clarify);
        assert_eq!(
            parse_decision("clarify, not sure"),
            ResponseDecision::Clarify
        );
    }

    /// SILENT recognized over SUBMIT, but CLARIFY takes precedence over
    /// SILENT when both present (matches TS branch order).
    #[test]
    fn parse_silent_recognized() {
        assert_eq!(parse_decision("SILENT"), ResponseDecision::Silent);
        assert_eq!(parse_decision("silent please"), ResponseDecision::Silent);
    }

    #[test]
    fn parse_clarify_beats_silent_when_both_present() {
        // TS branch order: CLARIFY check comes before SILENT, so a
        // model that emits "CLARIFY (or silent if unclear)" resolves
        // to Clarify.
        assert_eq!(
            parse_decision("CLARIFY or SILENT"),
            ResponseDecision::Clarify
        );
    }

    /// Unrecognized text → SUBMIT (fail-open). Pins the TS behavior;
    /// if a future refactor changes the default, this test breaks
    /// deliberately.
    #[test]
    fn parse_unrecognized_defaults_to_submit() {
        assert_eq!(parse_decision("yes, ship it"), ResponseDecision::Submit);
        assert_eq!(parse_decision(""), ResponseDecision::Submit);
        assert_eq!(parse_decision("garbage"), ResponseDecision::Submit);
    }

    /// Whitespace + casing tolerance (TS does `.trim().toUpperCase()`).
    #[test]
    fn parse_tolerates_whitespace_and_casing() {
        assert_eq!(parse_decision("   silent\n"), ResponseDecision::Silent);
        assert_eq!(parse_decision("Clarify"), ResponseDecision::Clarify);
    }

    // ─── reason_for ───────────────────────────────────────────────────

    #[test]
    fn reason_strings_are_stable() {
        assert_eq!(reason_for(ResponseDecision::Submit), REASON_SUBMIT);
        assert_eq!(reason_for(ResponseDecision::Clarify), REASON_CLARIFY);
        assert_eq!(reason_for(ResponseDecision::Silent), REASON_SILENT);
    }

    // ─── build_validate_generation_request ────────────────────────────

    #[test]
    fn generation_request_uses_groq_defaults() {
        let r = req("d", "q");
        let g = build_validate_generation_request(&r, DEFAULT_VALIDATE_MODEL.to_string());
        assert_eq!(g.provider.as_deref(), Some(VALIDATE_PROVIDER));
        assert_eq!(g.model.as_deref(), Some(DEFAULT_VALIDATE_MODEL));
        assert_eq!(g.temperature, Some(VALIDATE_TEMPERATURE));
        // No client-imposed ceiling — the model owns its generation length.
        assert_eq!(g.max_tokens, None);
        assert_eq!(
            g.purpose.as_deref(),
            Some("cognition/validate-response-decision")
        );
        assert_eq!(g.messages.len(), 2);
        assert_eq!(g.messages[0].role, "system");
        assert_eq!(g.messages[1].role, "user");
    }

    // ─── ValidateResponseError Display ────────────────────────────────

    #[test]
    fn error_no_adapter_displays_provider_and_model() {
        let e = ValidateResponseError::NoAdapter {
            provider: "groq".to_string(),
            model: Some("llama-3.1-8b-instant".to_string()),
        };
        let s = format!("{e}");
        assert!(s.contains("groq"));
        assert!(s.contains("llama-3.1-8b-instant"));
    }
}
