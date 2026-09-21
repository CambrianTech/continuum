//! Replay the recorded inference boundary, without entering the live mind or hands.
//! This is a fresh model experiment, not deterministic reproduction of GPU state.
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use ts_rs::TS;
use uuid::Uuid;

use crate::ai::{TextGenerationRequest, TextGenerationResponse};
use crate::cognition::prompt_capture::{
    self, CaptureLease, JsonlPromptCaptureSink, PromptCall, ReplaySource,
};
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

#[derive(Default)]
pub struct CognitionReplayRequest;

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct ReplayRequestParams {
    pub persona_id: crate::identity::PersonaRef,
    /// Stable cursor from cognition/playback, never a caller-supplied file path.
    pub selected: String,
    /// Explicit currently registered provider. Replay must not silently switch models.
    pub provider: String,
    /// Optional experiment: change only the output allowance, retaining the input.
    #[serde(default)]
    #[ts(optional)]
    pub max_tokens: Option<u32>,
}

#[derive(Debug, Serialize, TS)]
pub struct ReplayOutcome {
    pub finish_reason: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[ts(type = "number")]
    pub response_time_ms: u64,
    pub timing: Option<crate::ai::types::GenerationTiming>,
    pub answer_chars: u32,
    pub tool_calls: u32,
    pub model: String,
    pub provider: String,
}

impl From<&TextGenerationResponse> for ReplayOutcome {
    fn from(response: &TextGenerationResponse) -> Self {
        Self {
            finish_reason: response.finish_reason.to_string(),
            input_tokens: response.usage.input_tokens,
            output_tokens: response.usage.output_tokens,
            response_time_ms: response.response_time_ms,
            timing: response.timing.clone(),
            answer_chars: response.text.chars().count() as u32,
            tool_calls: response
                .tool_calls
                .as_ref()
                .map_or(0, |calls| calls.len() as u32),
            model: response.model.clone(),
            provider: response.provider.clone(),
        }
    }
}

#[derive(Debug, Serialize, TS)]
pub struct ReplayRequestResult {
    /// Separate capture owner/cache identity. Inspect it with cognition/playback.
    pub replay_persona_id: String,
    /// Submission cursor; list this replay identity to inspect its terminal event.
    pub selected: String,
    pub source_persona_id: String,
    pub source_selected: String,
    pub original_max_tokens: Option<u32>,
    pub replay_max_tokens: Option<u32>,
    pub original: Option<ReplayOutcome>,
    pub replay: ReplayOutcome,
    /// Always false: proposed model calls are recorded, never dispatched by replay.
    pub tools_executed: bool,
}

struct Prepared {
    request: TextGenerationRequest,
    call: PromptCall,
    original: Option<ReplayOutcome>,
    original_max_tokens: Option<u32>,
}

fn prepare(
    mut detail: prompt_capture::PlaybackDetail,
    p: &ReplayRequestParams,
    persona: Uuid,
) -> Result<Prepared, CommandError> {
    if !detail.issues.is_empty() {
        return Err(CommandError::Invalid(format!(
            "capture integrity: {}",
            detail.issues.join("; ")
        )));
    }
    if p.provider.trim().is_empty() || p.max_tokens == Some(0) {
        return Err(CommandError::Invalid(
            "provider must be explicit and max_tokens must be positive".into(),
        ));
    }
    let mut submitted = detail
        .submitted
        .take()
        .ok_or_else(|| CommandError::Invalid("capture has no submitted request".into()))?;
    if submitted.get("schema_version").and_then(|v| v.as_u64()) != Some(4) {
        return Err(CommandError::Invalid(
            "unsupported captured-request schema; replay requires schema 4".into(),
        ));
    }
    let context_window: Option<u32> = serde_json::from_value(submitted["context_window"].take())
        .map_err(|e| CommandError::Invalid(format!("captured context window: {e}")))?;
    let mut request: TextGenerationRequest = serde_json::from_value(submitted["request"].take())
        .map_err(|e| CommandError::Invalid(format!("captured request: {e}")))?;
    if request.model.as_deref().is_none_or(str::is_empty) {
        return Err(CommandError::Invalid(
            "capture has no explicit model; replay cannot substitute one".into(),
        ));
    }
    let original = match detail.terminal.as_mut().and_then(|v| v.get_mut("response")) {
        Some(value) if !value.is_null() => {
            let response: TextGenerationResponse = serde_json::from_value(value.take())
                .map_err(|e| CommandError::Invalid(format!("captured response: {e}")))?;
            Some(ReplayOutcome::from(&response))
        }
        _ => None,
    };
    let original_max_tokens = request.max_tokens;
    if let Some(tokens) = p.max_tokens {
        request.max_tokens = Some(tokens);
    }
    let replay = Uuid::new_v4();
    let room = Uuid::new_v4();
    let request_id = Uuid::new_v4().to_string();
    // Fresh identity isolates KV ownership. Prompt/model/genome/sampling remain recorded.
    request.persona_id = Some(replay.to_string());
    request.room_id = Some(room.to_string());
    request.request_id = Some(request_id.clone());
    request.provider = Some(p.provider.clone());
    request.purpose = Some("cognition/replay-request".into());
    Ok(Prepared {
        request,
        call: PromptCall {
            request_id,
            persona_id: replay,
            room_id: room,
            cycle_id: None,
            context_window,
            cause: "captured-request-replay",
            cause_root: None,
            replay_of: Some(ReplaySource {
                persona_id: persona,
                cursor: p.selected.clone(),
            }),
        },
        original,
        original_max_tokens,
    })
}

#[async_trait]
impl ActionCommand for CognitionReplayRequest {
    const NAME: &'static str = "cognition/replay-request";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str = "Rerun a captured model request with optional max_tokens override, using an explicit provider and isolated cache identity. Records original provenance and new lifecycle; returns outcome comparison. Never dispatches tool calls or changes live persona memory. Fresh inference, not deterministic full-mind replay.";
    type Params = ReplayRequestParams;
    type Output = ReplayRequestResult;

    async fn run(&self, _ctx: &Ctx, p: Self::Params) -> Result<Self::Output, CommandError> {
        let persona = Uuid::parse_str(p.persona_id.as_str())
            .map_err(|e| CommandError::Invalid(format!("persona_id: {e}")))?;
        let selected = p.selected.clone();
        let prepared = tokio::task::spawn_blocking(move || {
            let dir = crate::persona::recorder::fixture_dir(prompt_capture::FIXTURE_DIR)
                .ok_or_else(|| CommandError::Internal("capture directory unavailable".into()))?;
            let detail = prompt_capture::detail(&dir, persona, &selected)
                .map_err(|e| CommandError::Invalid(format!("capture: {e}")))?;
            prepare(detail, &p, persona)
        })
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))??;
        let Prepared {
            request,
            call,
            original,
            original_max_tokens,
        } = prepared;
        let provider = request
            .provider
            .as_deref()
            .ok_or_else(|| CommandError::Internal("replay provider missing".into()))?;
        let registry = crate::modules::ai_provider::global_registry();
        let adapter = registry.read().await.get_arc(provider).ok_or_else(|| {
            CommandError::Invalid(format!("provider '{provider}' is not registered"))
        })?;
        let model = request
            .model
            .as_deref()
            .ok_or_else(|| CommandError::Internal("replay model missing".into()))?;
        if !adapter.supports_model(model) {
            return Err(CommandError::Invalid(format!(
                "provider '{provider}' does not serve captured model '{model}'"
            )));
        }
        let replay_persona = call.persona_id;
        let sink = tokio::task::spawn_blocking(move || {
            JsonlPromptCaptureSink::open_for_persona(replay_persona)
        })
        .await
        .map_err(|e| CommandError::Internal(e.to_string()))?
        .map_err(|e| CommandError::Internal(format!("replay capture: {e}")))?;
        execute(
            Prepared {
                request,
                call,
                original,
                original_max_tokens,
            },
            adapter,
            sink,
        )
        .await
    }
}

async fn execute(
    prepared: Prepared,
    adapter: Arc<dyn crate::ai::adapter::AIProviderAdapter>,
    sink: JsonlPromptCaptureSink,
) -> Result<ReplayRequestResult, CommandError> {
    let Prepared {
        request,
        mut call,
        original,
        original_max_tokens,
    } = prepared;
    call.context_window = adapter.live_served_window();
    let replay_persona = call.persona_id;
    let mut capture = CaptureLease::start(Arc::new(sink), &call, &request);
    let selected = capture
        .cursor()
        .ok_or_else(|| {
            CommandError::Internal("replay submission was not recorded; inference refused".into())
        })?
        .to_string();
    let replay_max_tokens = request.max_tokens;
    // No faculty invocation, tool executor, memory writer or learning update.
    let response = adapter.generate_text(request).await;
    match response {
        Ok(response) => {
            capture.finish(Some(&response), None);
            let source = call
                .replay_of
                .ok_or_else(|| CommandError::Internal("replay provenance missing".into()))?;
            Ok(ReplayRequestResult {
                replay_persona_id: replay_persona.to_string(),
                selected,
                source_persona_id: source.persona_id.to_string(),
                source_selected: source.cursor,
                original_max_tokens,
                replay_max_tokens,
                original,
                replay: ReplayOutcome::from(&response),
                tools_executed: false,
            })
        }
        Err(error) => {
            capture.finish(None, Some(&error.to_string()));
            Err(CommandError::Internal(format!(
                "replay {replay_persona} capture {selected}: {error}"
            )))
        }
    }
}

crate::register_stateless_command!(CognitionReplayRequest);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::ChatMessage;

    #[tokio::test]
    async fn recorded_request_replays_without_rebuilding_prompt_or_reusing_live_identity() {
        let dir = tempfile::tempdir().expect("capture directory");
        let persona = Uuid::new_v4();
        let room = Uuid::new_v4();
        let request = TextGenerationRequest {
            model: Some("captured-model".into()),
            provider: Some("original-provider".into()),
            persona_id: Some(persona.to_string()),
            room_id: Some(room.to_string()),
            request_id: Some("original-request".into()),
            system_prompt: Some("Original identity and instructions".into()),
            messages: vec![ChatMessage::text(
                "user",
                "The actual task and prior tool evidence",
            )],
            max_tokens: Some(16_384),
            temperature: Some(0.35),
            stop_sequences: Some(vec!["original-stop".into()]),
            turn_bound: Some(std::time::Duration::from_secs(675)),
            ..Default::default()
        };
        let call = PromptCall {
            request_id: "original-request".into(),
            persona_id: persona,
            room_id: room,
            cycle_id: Some(37),
            context_window: Some(101_120),
            cause: "synthetic",
            cause_root: None,
            replay_of: None,
        };
        let sink = JsonlPromptCaptureSink::open(dir.path(), persona).expect("capture sink");
        let mut capture = CaptureLease::start(Arc::new(sink), &call, &request);
        let cursor = capture.cursor().expect("durable submission").to_string();
        capture.finish(None, Some("original interrupted"));
        let p = ReplayRequestParams {
            persona_id: persona.to_string().into(),
            selected: cursor.clone(),
            provider: "explicit-provider".into(),
            max_tokens: Some(41_053),
        };
        let read =
            || prompt_capture::detail(dir.path(), persona, &cursor).expect("original capture");
        let prepared = prepare(read(), &p, persona).expect("prepare replay");
        let actual = serde_json::to_value(&prepared.request).expect("request value");
        let mut expected = serde_json::to_value(&request).expect("original value");
        for field in ["requestId", "personaId", "roomId"] {
            assert_ne!(
                actual[field], expected[field],
                "replay must isolate {field}"
            );
            expected[field] = actual[field].clone();
        }
        expected["provider"] = serde_json::json!("explicit-provider");
        expected["purpose"] = serde_json::json!("cognition/replay-request");
        assert_eq!(
            crate::inference::slots::class_for(prepared.request.purpose.as_deref()),
            crate::inference::slots::SlotClass::Probe
        );
        expected["maxTokens"] = serde_json::json!(41_053);
        assert_eq!(
            actual, expected,
            "all prompt/model/genome/sampling fields stay captured"
        );
        assert_eq!(prepared.original_max_tokens, Some(16_384));
        // The provenance rides the existing capture format, not an unrelated log.
        let replay_id = prepared.call.persona_id;
        let replay_sink = JsonlPromptCaptureSink::open(dir.path(), replay_id).expect("replay sink");
        let replay = CaptureLease::start(Arc::new(replay_sink), &prepared.call, &prepared.request);
        let replay_cursor = replay.cursor().expect("replay capture").to_string();
        drop(replay); // cancellation is a terminal event, never a successful replay
        let detail =
            prompt_capture::detail(dir.path(), replay_id, &replay_cursor).expect("replay detail");
        let submitted = detail.submitted.expect("replay submitted");
        assert_eq!(submitted["replay_of"]["persona_id"], persona.to_string());
        assert_eq!(submitted["replay_of"]["cursor"], cursor);
        assert_eq!(
            read().submitted.expect("original retained")["request"]["maxTokens"],
            16_384
        );
        let mut broken = read();
        broken.issues.push("request payload missing".into());
        assert!(prepare(broken, &p, persona).is_err());
        let mut missing_model = read();
        missing_model.submitted.as_mut().expect("submitted")["request"]["model"] =
            serde_json::Value::Null;
        assert!(prepare(missing_model, &p, persona).is_err());
        let prepared = prepare(read(), &p, persona).expect("fresh replay");
        let replay_id = prepared.call.persona_id;
        let sink = JsonlPromptCaptureSink::open(dir.path(), replay_id).expect("execution sink");
        let target = dir.path().join("must-not-be-written.txt");
        let response = TextGenerationResponse {
            text: String::new(),
            finish_reason: crate::ai::FinishReason::ToolUse,
            model: "captured-model".into(),
            provider: "explicit-provider".into(),
            usage: crate::ai::UsageMetrics::default(),
            response_time_ms: 0,
            request_id: "provider-response".into(),
            content: None,
            tool_calls: Some(vec![crate::ai::ToolCall {
                id: "proposed-write".into(),
                name: "code/write".into(),
                input: serde_json::json!({"path": target, "content": "forbidden replay effect"}),
            }]),
            reasoning: None,
            routing: None,
            error: None,
            timing: None,
        };
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let adapter = crate::ai::heuristic_adapter::HeuristicInferenceAdapter::new()
            .with_responses(vec![response])
            .with_request_recorder(seen.clone());
        let result = execute(prepared, Arc::new(adapter), sink)
            .await
            .expect("replay execution");
        assert_eq!(result.replay.tool_calls, 1);
        assert!(!result.tools_executed);
        assert!(
            !target.exists(),
            "replay must never execute proposed writes"
        );
        assert_eq!(seen.lock().expect("recorded calls").len(), 1);
        let page =
            prompt_capture::page(dir.path(), replay_id, None, false, 10).expect("replay lifecycle");
        let terminal = page.entries.last().expect("terminal event");
        assert!(matches!(
            terminal.status,
            prompt_capture::CallStatus::Completed
        ));
        let final_detail = prompt_capture::detail(dir.path(), replay_id, &terminal.cursor)
            .expect("terminal detail");
        assert_eq!(
            final_detail.terminal.expect("terminal")["response"]["toolCalls"][0]["name"],
            "code/write"
        );
    }
}
