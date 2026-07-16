//! AI-driven rater for response proposals. Wires the prompt+parser shipped
//! in PR-1 to `AIProviderRegistry::generate_text` so the chat substrate's
//! peer-review flow can call into Rust instead of `ProposalRatingAdapter.ts`.
//!
//! Mirror of TS `rateProposalsWithAI` (system/user/server/modules/cognition/
//! ProposalRatingAdapter.ts:46-84). The TS version goes through
//! `AIProviderDaemon.generateText` which itself goes through the IPC mixin
//! to this same Rust adapter — so by collapsing into Rust we drop one TS
//! hop AND eliminate the duplicate parser/prompt code.
//!
//! ## Why no fallback
//!
//! If inference fails, return the typed error. The TS `createFallbackRatings`
//! helper that returns neutral 0.5 scores on AI failure isn't ported — it
//! masks real provider outages and was caught as a silent-success vector in
//! the no-CPU-fallback audit (#1262). Callers (PR-3 TS shim) will surface
//! `Err` to the chat substrate; the substrate already handles "no rater
//! responded" by skipping peer-review for that round (no degraded scoring).

use crate::ai::{ChatMessage, MessageContent, TextGenerationRequest};
use crate::cognition::rate_proposals::parser::{parse_ratings_from_ai_response, ParseConfig};
use crate::cognition::rate_proposals::prompt::build_rating_prompt;
use crate::cognition::rate_proposals::types::{ProposalRating, RatingContext};
use crate::modules::ai_provider::{generate_text, global_registry};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Request shape for the rater. Mirrors the TS `params` object that
/// `rateProposalsWithAI` accepts. ts-rs exports the camelCase wire so the
/// PR-3 TS shim binds against generated types instead of hand-writing a
/// duplicate.
///
/// `temperature` defaults to 0.7 if omitted (same default as TS).
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/RateProposalsRequest.ts"
)]
pub struct RateProposalsRequest {
    pub reviewer_name: String,
    pub model_provider: String,
    pub model_id: String,
    #[ts(optional)]
    pub temperature: Option<f32>,
    pub context: RatingContext,
}

/// Response shape — just the ratings. Errors propagate as typed
/// `Err(String)` over IPC; PR-3 TS shim surfaces them to the chat substrate.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/RateProposalsResponse.ts"
)]
pub struct RateProposalsResponse {
    pub ratings: Vec<ProposalRating>,
}

/// Default temperature when the caller omits it. Matches TS
/// `temperature ?? 0.7` in ProposalRatingAdapter.ts:67.
const DEFAULT_TEMPERATURE: f32 = 0.7;

/// Run AI-driven rating against the registered provider. Pure async; no
/// global state mutation. Each call is independent — no caching at this
/// layer because (a) ratings are turn-specific and (b) the upstream
/// proposal aggregator needs fresh judgments to weight reviewers.
pub async fn rate_proposals_with_ai(
    request: RateProposalsRequest,
) -> Result<RateProposalsResponse, String> {
    let RateProposalsRequest {
        reviewer_name,
        model_provider,
        model_id,
        temperature,
        context,
    } = request;

    let prompt_text = build_rating_prompt(&context, &reviewer_name);

    let inference_request = TextGenerationRequest {
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: MessageContent::Text(format!(
                    "You are {reviewer_name}, an AI evaluating response proposals from your peers."
                )),
                name: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text(prompt_text),
                name: None,
            },
        ],
        system_prompt: None,
        model: Some(model_id),
        provider: Some(model_provider),
        temperature: Some(temperature.unwrap_or(DEFAULT_TEMPERATURE)),
        // Model owns its length (None → adapter forwards no ceiling). The JSON
        // response_format + prompt bound the rater's output, not a const of ours.
        max_tokens: None,
        top_p: None,
        top_k: None,
        repeat_penalty: None,
        frequency_penalty: None,
        repeat_last_n: None,
        stop_sequences: None,
        tools: None,
        tool_choice: None,
        response_format: None,
        active_adapters: None,
        request_id: None,
        user_id: None,
        room_id: None,
        purpose: Some("cognition-rate-proposals".to_string()),
        persona_id: None,
    };

    let registry = global_registry();
    let registry_guard = registry.read().await;
    let response = generate_text(&registry_guard, inference_request).await?;

    let ratings =
        parse_ratings_from_ai_response(&response.text, &context.proposals, &ParseConfig::default());

    Ok(RateProposalsResponse { ratings })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::rate_proposals::types::{RatingMessage, ResponseProposal};

    /// What this catches: ts-rs generates a `RateProposalsRequest` TS type
    /// with camelCase fields and the optional temperature marked as `?:`.
    /// The TS shim in PR-3 binds against this generated type — drift here
    /// would break the IPC wire between the shim and this orchestrator.
    #[test]
    fn rate_proposals_request_serde_camelcase() {
        let req = RateProposalsRequest {
            reviewer_name: "claude".into(),
            model_provider: "anthropic".into(),
            model_id: "claude-opus-4-7".into(),
            temperature: Some(0.7),
            context: RatingContext {
                original_message: RatingMessage {
                    sender_name: "joel".into(),
                    content: "?".into(),
                    timestamp: 0,
                },
                recent_messages: vec![],
                proposals: vec![ResponseProposal {
                    proposal_id: "p-1".into(),
                    proposer_name: "alice".into(),
                    response_text: "42".into(),
                    confidence: 0.9,
                }],
            },
        };
        let j = serde_json::to_string(&req).unwrap();
        assert!(j.contains("\"reviewerName\":\"claude\""));
        assert!(j.contains("\"modelProvider\":\"anthropic\""));
        assert!(j.contains("\"modelId\":\"claude-opus-4-7\""));
        assert!(j.contains("\"temperature\":0.7"));
        let back: RateProposalsRequest = serde_json::from_str(&j).unwrap();
        assert_eq!(back.reviewer_name, "claude");
        assert_eq!(back.context.proposals.len(), 1);
    }

    /// What this catches: serde accepts a request with `temperature` omitted
    /// and the orchestrator falls back to DEFAULT_TEMPERATURE. The TS shim
    /// callers may not always pass temperature; the contract has to match.
    #[test]
    fn rate_proposals_request_temperature_optional() {
        let json = r#"{
            "reviewerName": "claude",
            "modelProvider": "local",
            "modelId": "qwen",
            "context": {
                "originalMessage": {"senderName":"joel","content":"?","timestamp":0},
                "recentMessages": [],
                "proposals": []
            }
        }"#;
        let req: RateProposalsRequest = serde_json::from_str(json).unwrap();
        assert!(req.temperature.is_none());
        // The orchestrator substitutes DEFAULT_TEMPERATURE — verify the
        // const stays at the documented 0.7 so callers without temperature
        // see consistent behavior across releases.
        assert!((DEFAULT_TEMPERATURE - 0.7).abs() < 1e-9);
    }

    /// What this catches: response shape ts-rs export. PR-3 shim awaits
    /// `Commands.execute<RateProposalsResponse>(...)` — the wire field
    /// must stay `ratings` (camelCase, plural, array).
    #[test]
    fn rate_proposals_response_serde_shape() {
        let resp = RateProposalsResponse { ratings: vec![] };
        let j = serde_json::to_string(&resp).unwrap();
        assert!(j.contains("\"ratings\":[]"));
        let back: RateProposalsResponse = serde_json::from_str(&j).unwrap();
        assert_eq!(back.ratings.len(), 0);
    }
}
