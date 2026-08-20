//! Rust-owned "should this persona respond?" gating.
//!
//! This replaces the TypeScript prompt-builder/parser in
//! AIDecisionService.evaluateGating. TypeScript still owns platform concerns
//! around slot coordination and logging; Rust owns the cognition decision
//! contract, prompt construction, model call, and response parsing.

use crate::ai::adapter::InferenceDevice;
use crate::ai::types::ResponseFormat;
use crate::ai::{ChatMessage, MessageContent, TextGenerationRequest, TextGenerationResponse};
use crate::modules::ai_provider::global_registry;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};
use ts_rs::TS;

const GATING_PROVIDER: &str = "groq";
const DEFAULT_GATING_MODEL: &str = "llama-3.1-8b-instant";

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/AIDecisionContext.ts"
)]
pub struct AIDecisionContext {
    pub persona_id: String,
    pub persona_name: String,
    pub room_id: String,
    pub trigger_message: GatingTriggerMessage,
    pub rag_context: GatingRagContext,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub system_prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/GatingTriggerMessage.ts"
)]
pub struct GatingTriggerMessage {
    pub id: String,
    pub sender_name: String,
    pub content: GatingMessageContent,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/GatingMessageContent.ts"
)]
pub struct GatingMessageContent {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/GatingRagContext.ts"
)]
pub struct GatingRagContext {
    #[serde(default)]
    pub conversation_history: Vec<GatingConversationMessage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub recipe_strategy: Option<GatingRecipeStrategy>,
    #[serde(default)]
    pub metadata: GatingRagMetadata,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/GatingRagMetadata.ts"
)]
pub struct GatingRagMetadata {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub recipe_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/GatingConversationMessage.ts"
)]
pub struct GatingConversationMessage {
    pub role: String,
    pub content: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub timestamp: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/GatingRecipeStrategy.ts"
)]
pub struct GatingRecipeStrategy {
    pub conversation_pattern: String,
    #[serde(default)]
    pub response_rules: Vec<String>,
    #[serde(default)]
    pub decision_criteria: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/AIGatingDecisionFactors.ts"
)]
pub struct AIGatingDecisionFactors {
    pub mentioned: bool,
    pub question_asked: bool,
    pub domain_relevant: bool,
    pub recently_spoke: bool,
    pub others_answered: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/AIGatingDecision.ts"
)]
pub struct AIGatingDecision {
    pub should_respond: bool,
    pub confidence: f32,
    pub reason: String,
    pub model: String,
    #[ts(type = "number")]
    pub timestamp: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub factors: Option<AIGatingDecisionFactors>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/cognition/ShouldRespondRequest.ts"
)]
pub struct ShouldRespondRequest {
    pub context: AIDecisionContext,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub temperature: Option<f32>,
}

#[derive(Debug, thiserror::Error)]
pub enum ShouldRespondError {
    #[error("no AI adapter available for provider={provider:?} model={model:?}")]
    NoAdapter {
        provider: String,
        model: Option<String>,
    },
    #[error("generation failed: {0}")]
    Generation(String),
}

pub async fn evaluate_gating(
    request: ShouldRespondRequest,
) -> Result<AIGatingDecision, ShouldRespondError> {
    let model = request
        .model
        .clone()
        .unwrap_or_else(|| DEFAULT_GATING_MODEL.to_string());
    let prompt = build_gating_prompt(&request.context);

    let gen_request = TextGenerationRequest {
        messages: vec![
            ChatMessage {
                role: "system".to_string(),
                content: MessageContent::Text(
                    "You are a conversation coordinator. Respond ONLY with JSON.".to_string(),
                ),
                name: None,
            },
            ChatMessage {
                role: "user".to_string(),
                content: MessageContent::Text(prompt),
                name: None,
            },
        ],
        system_prompt: None,
        model: Some(model.clone()),
        provider: Some(GATING_PROVIDER.to_string()),
        temperature: Some(request.temperature.unwrap_or(0.3)),
        // Model owns its length — the adapter forwards no ceiling (None). The gating
        // prompt asks for a short verdict; brevity is the model's to give, not ours
        // to guillotine (a hard cap truncates a reasoning model mid-thought → empty).
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
        purpose: Some("cognition/should-respond".to_string()),
        persona_id: Some(request.context.persona_id.clone()),
    };

    let registry_arc = global_registry();
    let registry = registry_arc.read().await;
    // Device = `Auto`: cognition has no opinion on placement.
    // Per task #162 follow-up: the registered adapter is the
    // authority on its own device class; filtering by Gpu here
    // wrongly excluded CPU-only adapters even when they were the
    // only ones claiming the requested model.
    let (_provider_id, adapter) = registry
        .select(Some(GATING_PROVIDER), Some(&model), InferenceDevice::Auto)
        .ok_or_else(|| ShouldRespondError::NoAdapter {
            provider: GATING_PROVIDER.to_string(),
            model: Some(model.clone()),
        })?;

    let response: TextGenerationResponse = adapter
        .generate_text(gen_request)
        .await
        .map_err(ShouldRespondError::Generation)?;

    let parsed = parse_gating_response(&response.text);
    Ok(AIGatingDecision {
        should_respond: parsed.should_respond,
        confidence: parsed.confidence,
        reason: parsed.reason,
        model,
        timestamp: now_ms(),
        factors: parsed.factors,
    })
}

pub fn build_gating_prompt(context: &AIDecisionContext) -> String {
    let recent_messages = context
        .rag_context
        .conversation_history
        .iter()
        .rev()
        .take(10)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>();

    let trigger_text = &context.trigger_message.content.text;
    let trigger_sender = &context.trigger_message.sender_name;
    let mut trigger_in_history = false;
    let mut conversation_lines = Vec::with_capacity(recent_messages.len() + 1);

    for msg in recent_messages {
        let speaker = msg.name.as_deref().unwrap_or(&msg.role);
        let line = format!("{speaker}: {}", msg.content);
        let is_trigger = msg.content == *trigger_text && speaker == trigger_sender;
        if is_trigger {
            trigger_in_history = true;
            conversation_lines.push(format!(">>> {line} <<<"));
        } else {
            conversation_lines.push(line);
        }
    }

    if !trigger_in_history {
        conversation_lines.push(format!(">>> {trigger_sender}: {trigger_text} <<<"));
    }

    let recipe_rules = context
        .rag_context
        .recipe_strategy
        .as_ref()
        .map(|strategy| {
            let recipe_name = context
                .rag_context
                .metadata
                .recipe_name
                .as_deref()
                .unwrap_or("room recipe");
            format!(
                "\n\n**RECIPE RULES (from {recipe_name}):**\n\nConversation Pattern: {}\n\nResponse Rules:\n{}\n\nDecision Criteria:\n{}\n\n",
                strategy.conversation_pattern,
                strategy
                    .response_rules
                    .iter()
                    .map(|rule| format!("- {rule}"))
                    .collect::<Vec<_>>()
                    .join("\n"),
                strategy
                    .decision_criteria
                    .iter()
                    .map(|criterion| format!("- {criterion}"))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
        })
        .unwrap_or_default();

    format!(
        "You are \"{}\" in a group chat. Should you respond to the message marked >>> like this <<<?\n\n\
**PHILOSOPHY: Only gate if it makes the conversation confusing**\n\n\
When to RESPOND:\n\
- Someone asks a question -> respond if you have relevant knowledge\n\
- Someone makes a statement -> respond if you have insights to add\n\
- Multiple AIs responding is GOOD -> diverse perspectives enrich conversation\n\
- Someone already responded -> still respond if you have DIFFERENT angle or additional info\n\
- Human asks \"who is here?\" -> always respond to identify yourself\n\n\
When to STAY QUIET:\n\
- You'd just repeat exactly what was already said -> stay quiet\n\
- The answer is perfect and complete -> stay quiet\n\
- You have nothing valuable to add -> stay quiet\n\
- Conversation moved to a different topic -> stay quiet\n\n\
**IMPORTANT - Be Confident:**\n\
- If you have relevant knowledge, SHARE IT - don't be shy\n\
- Multiple responses are ENRICHING, not confusing\n\
- Your perspective is valuable even if someone else responded\n\
- \"Already answered\" is NOT a reason to stay quiet unless answer is PERFECT\n\
- Direct questions from humans deserve responses from ALL who can help{recipe_rules}\n\
**Recent conversation:**\n{}\n\n\
Respond with JSON:\n\
{{\n  \"shouldRespond\": true/false,\n  \"confidence\": 0.0-1.0,\n  \"reason\": \"brief why/why not\"\n}}",
        context.persona_name,
        conversation_lines.join("\n")
    )
}

pub fn parse_gating_response(ai_text: &str) -> AIGatingDecision {
    if let Some(json) = extract_json_object(ai_text) {
        if let Ok(value) = serde_json::from_str::<Value>(json) {
            return decision_from_json(&value);
        }
    }

    let lower = ai_text.to_ascii_lowercase();
    let should_respond = lower.contains("shouldrespond\": true")
        || lower.contains("\"respond\"")
        || starts_with_word(&lower, "yes")
        || lower.contains("should respond")
        || lower.contains("would respond")
        || lower.contains("will respond")
        || lower.contains("should answer")
        || lower.contains("would answer")
        || lower.contains("will answer")
        || lower.contains("should reply")
        || lower.contains("would reply")
        || lower.contains("will reply");
    let should_stay_silent = lower.contains("shouldrespond\": false")
        || lower.contains("\"silent\"")
        || contains_word(&lower, "no")
        || contains_word(&lower, "silent")
        || contains_word(&lower, "pass")
        || contains_word(&lower, "skip")
        || lower.contains("should not respond");

    AIGatingDecision {
        should_respond: should_respond || !should_stay_silent,
        confidence: extract_confidence(ai_text).unwrap_or(0.5),
        reason: extract_reason(ai_text),
        model: String::new(),
        timestamp: 0,
        factors: None,
    }
}

fn decision_from_json(value: &Value) -> AIGatingDecision {
    let confidence = value
        .get("confidence")
        .and_then(Value::as_f64)
        .map(|v| v.clamp(0.0, 1.0) as f32)
        .unwrap_or(0.5);
    let factors = value
        .get("factors")
        .and_then(|v| serde_json::from_value::<AIGatingDecisionFactors>(v.clone()).ok());

    AIGatingDecision {
        should_respond: value
            .get("shouldRespond")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        confidence,
        reason: value
            .get("reason")
            .and_then(Value::as_str)
            .unwrap_or("No reason provided")
            .to_string(),
        model: String::new(),
        timestamp: 0,
        factors,
    }
}

fn extract_json_object(text: &str) -> Option<&str> {
    let start = text.find('{')?;
    let end = text.rfind('}')?;
    (end >= start).then(|| &text[start..=end])
}

fn extract_confidence(text: &str) -> Option<f32> {
    let lower = text.to_ascii_lowercase();
    let idx = lower.find("confidence")?;
    let tail = &lower[idx + "confidence".len()..];
    let number = tail
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect::<String>();
    number.parse::<f32>().ok().map(|v| v.clamp(0.0, 1.0))
}

fn extract_reason(text: &str) -> String {
    if let Some(idx) = text.to_ascii_lowercase().find("because") {
        let reason = text[idx + "because".len()..]
            .split(['.', '\n', '}'])
            .next()
            .unwrap_or("")
            .trim();
        if !reason.is_empty() {
            return reason.to_string();
        }
    }

    text.lines()
        .find(|line| line.trim().len() >= 10)
        .map(|line| line.trim().chars().take(100).collect())
        .unwrap_or_else(|| "Extracted from natural language response".to_string())
}

fn contains_word(text: &str, needle: &str) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .any(|word| word == needle)
}

fn starts_with_word(text: &str, needle: &str) -> bool {
    text.split(|c: char| !c.is_ascii_alphanumeric())
        .find(|word| !word.is_empty())
        .is_some_and(|word| word == needle)
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context() -> AIDecisionContext {
        AIDecisionContext {
            persona_id: "persona-1".to_string(),
            persona_name: "Ada".to_string(),
            room_id: "room-1".to_string(),
            trigger_message: GatingTriggerMessage {
                id: "message-1".to_string(),
                sender_name: "Operator".to_string(),
                content: GatingMessageContent {
                    text: "who is here?".to_string(),
                },
            },
            rag_context: GatingRagContext {
                conversation_history: vec![GatingConversationMessage {
                    role: "user".to_string(),
                    content: "who is here?".to_string(),
                    name: Some("Operator".to_string()),
                    timestamp: Some(1),
                }],
                recipe_strategy: Some(GatingRecipeStrategy {
                    conversation_pattern: "collaborative".to_string(),
                    response_rules: vec!["answer direct questions".to_string()],
                    decision_criteria: vec!["identity questions should respond".to_string()],
                }),
                metadata: GatingRagMetadata {
                    recipe_name: Some("standup".to_string()),
                },
            },
            system_prompt: None,
        }
    }

    #[test]
    fn build_prompt_marks_trigger_and_includes_recipe_rules() {
        let prompt = build_gating_prompt(&context());
        assert!(prompt.contains("You are \"Ada\""));
        assert!(prompt.contains(">>> Operator: who is here? <<<"));
        assert!(prompt.contains("RECIPE RULES (from standup)"));
        assert!(prompt.contains("- answer direct questions"));
    }

    #[test]
    fn parse_json_response_clamps_confidence_and_keeps_factors() {
        let parsed = parse_gating_response(
            r#"{"shouldRespond":true,"confidence":1.7,"reason":"direct question","factors":{"mentioned":true,"questionAsked":true,"domainRelevant":false,"recentlySpoke":false,"othersAnswered":false}}"#,
        );
        assert!(parsed.should_respond);
        assert_eq!(parsed.confidence, 1.0);
        assert_eq!(parsed.reason, "direct question");
        assert_eq!(
            parsed.factors,
            Some(AIGatingDecisionFactors {
                mentioned: true,
                question_asked: true,
                domain_relevant: false,
                recently_spoke: false,
                others_answered: false,
            })
        );
    }

    #[test]
    fn parse_plain_text_no_stays_silent() {
        let parsed =
            parse_gating_response("No, should stay silent because the answer is complete.");
        assert!(!parsed.should_respond);
        assert_eq!(parsed.confidence, 0.5);
        assert_eq!(parsed.reason, "the answer is complete");
    }
}
