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
    /// Diagnostic only: explicit zero-based standalone collapsed receipt messages to omit.
    /// Requires expected_request_sha256; native protocol messages cannot be omitted.
    #[serde(default)]
    pub omit_message_indices: Vec<u32>,
    /// SHA-256 of the decoded captured request's compact, recursively key-sorted JSON,
    /// before replay identity/model overrides. Returned by every exact replay.
    #[serde(default)]
    #[ts(optional)]
    pub expected_request_sha256: Option<String>,
    /// Diagnostic only: replace one existing tool's input schema. Requires the
    /// captured request hash and cannot be combined with another experiment.
    #[serde(default)]
    #[ts(optional)]
    pub tool_schema_override: Option<ToolSchemaOverride>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ToolSchemaOverride {
    pub tool_name: String,
    /// JSON Schema object; equivalence to the original is the experimenter's claim,
    /// not inferred or certified by replay.
    pub input_schema: serde_json::Value,
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
    /// The submitted model was implicit; replay pins the recorded response model.
    pub model_from_response: bool,
    pub original_max_tokens: Option<u32>,
    pub replay_max_tokens: Option<u32>,
    pub original: Option<ReplayOutcome>,
    pub replay: ReplayOutcome,
    /// Always false: proposed model calls are recorded, never dispatched by replay.
    pub tools_executed: bool,
    pub input_comparison: InputComparison,
}

use super::super::prompt_capture::{InputComparison, ToolSchemaComparison};

fn request_hash(request: &TextGenerationRequest) -> Result<String, CommandError> {
    let value = serde_json::to_value(request) // boundary: canonical request digest shared with diagnostic caller
        .map_err(|e| CommandError::Internal(format!("replay request hash: {e}")))?;
    value_hash(value)
}

fn value_hash(mut value: serde_json::Value) -> Result<String, CommandError> {
    use sha2::{Digest, Sha256};
    value.sort_all_objects();
    let bytes = serde_json::to_vec(&value) // boundary serialization: canonical captured-request integrity digest
        .map_err(|e| CommandError::Internal(format!("replay request hash: {e}")))?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn compare_input(request: &mut TextGenerationRequest, p: &ReplayRequestParams) -> Result<InputComparison, CommandError> {
    let before = request_hash(request)?;
    if let Some(expected) = &p.expected_request_sha256 {
        if expected != &before {
            return Err(CommandError::Invalid(format!("captured request hash mismatch; actual {before}")));
        }
    } else if !p.omit_message_indices.is_empty() || p.tool_schema_override.is_some() {
        return Err(CommandError::Invalid("input comparison requires expected_request_sha256".into()));
    }
    if p.tool_schema_override.is_some() && (p.max_tokens.is_some() || !p.omit_message_indices.is_empty()) {
        return Err(CommandError::Invalid("tool schema override cannot be combined with another experiment".into()));
    }
    // Resolve and validate before mutation; never add, remove or rename tools.
    let schema_change = if let Some(change) = &p.tool_schema_override {
        if change.tool_name.trim().is_empty() || !change.input_schema.is_object() {
            return Err(CommandError::Invalid("tool schema override requires a nonempty tool name and schema object".into()));
        }
        let replacement: crate::ai::ToolInputSchema = serde_json::from_value(change.input_schema.clone()) // boundary: diagnostic schema enters the existing native tool protocol
            .map_err(|e| CommandError::Invalid(format!("invalid tool input schema: {e}")))?;
        let encoded = serde_json::to_value(&replacement) // boundary: refuse unsupported fields rather than silently dropping schema constraints
            .map_err(|e| CommandError::Internal(format!("tool input schema: {e}")))?;
        if replacement.schema_type != "object" || !replacement.properties.is_object() || encoded != change.input_schema {
            return Err(CommandError::Invalid("tool schema must round-trip exactly through the native object schema".into()));
        }
        let matches: Vec<_> = request.tools.as_deref().unwrap_or_default().iter().enumerate()
            .filter(|(_, tool)| tool.name == change.tool_name).collect();
        if matches.len() != 1 {
            return Err(CommandError::Invalid(format!("tool schema override must match exactly one captured tool: {}", change.tool_name)));
        }
        let (index, tool) = matches[0];
        Some((index, replacement, ToolSchemaComparison {
            tool_name: change.tool_name.clone(),
            captured_schema_sha256: value_hash(serde_json::to_value(&tool.input_schema) // boundary: canonical original tool schema provenance
                .map_err(|e| CommandError::Internal(format!("tool input schema: {e}")))?)?,
            comparison_schema_sha256: value_hash(change.input_schema.clone())?,
        }))
    } else { None };
    if !p.omit_message_indices.is_empty() && p.max_tokens.is_some() {
        return Err(CommandError::Invalid("message omission cannot also change output allowance".into()));
    }
    let mut indices = std::collections::BTreeSet::new();
    for &index in &p.omit_message_indices {
        if !indices.insert(index) {
            return Err(CommandError::Invalid(format!("duplicate omitted message index {index}")));
        }
        let message = request.messages.get(index as usize)
            .ok_or_else(|| CommandError::Invalid(format!("omitted message index {index} is out of range")))?;
        let allowed = message.role == "user" && message.name.is_none()
            && matches!(&message.content, crate::ai::MessageContent::Text(text)
                if crate::cognition::working_memory::is_collapsed_result_pointer(text));
        if !allowed {
            return Err(CommandError::Invalid(format!("message {index} is not a standalone collapsed historical receipt")));
        }
    }
    // Validate every index before changing anything. Protocol Parts are never eligible.
    let mut index = 0u32;
    request.messages.retain(|_| {
        let keep = !indices.contains(&index);
        index += 1;
        keep
    });
    let tool_schema = if let Some((index, replacement, audit)) = schema_change {
        // The index and both containers were validated above, before any mutation.
        request.tools.as_mut().expect("validated tool set")[index].input_schema =
            replacement;
        Some(audit)
    } else { None };
    Ok(InputComparison {
        tool_schema,
        omitted_message_indices: indices.into_iter().collect(),
        captured_request_sha256: before,
        comparison_request_sha256: request_hash(request)?,
    })
}

struct Prepared {
    request: TextGenerationRequest,
    call: PromptCall,
    original: Option<ReplayOutcome>,
    original_max_tokens: Option<u32>,
}

fn prepare(
    detail: prompt_capture::PlaybackDetail,
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
    let prompt_capture::CapturedCall {
        context_window,
        mut request,
        response,
    } = prompt_capture::decode(detail).map_err(CommandError::Invalid)?;
    let input_comparison = compare_input(&mut request, p)?;
    let original = response.as_ref().map(ReplayOutcome::from);
    // An implicit provider default is only reproducible when the terminal receipt
    // identifies what actually served it. Never substitute today's default.
    let model_from_response = request.model.as_deref().is_none_or(|m| m.trim().is_empty());
    if model_from_response {
        request.model = Some(
            original
                .as_ref()
                .filter(|r| !r.model.trim().is_empty())
                .ok_or_else(|| {
                    CommandError::Invalid(
                        "capture has no request or response model; replay cannot substitute one"
                            .into(),
                    )
                })?
                .model
                .clone(),
        );
    }
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
                input_comparison,
                model_from_response,
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
    const DESCRIPTION: &'static str = "Rerun a captured model request with optional max_tokens override, hash-bound collapsed-receipt omission, or one named tool schema override, using an explicit provider and isolated cache identity. Records original provenance and new lifecycle; returns outcome comparison. Never dispatches tool calls or changes live persona memory. Fresh inference, not deterministic full-mind replay.";
    type Params = ReplayRequestParams;
    type Output = ReplayRequestResult;

    async fn run(&self, _ctx: &Ctx, p: Self::Params) -> Result<Self::Output, CommandError> {
        let persona = Uuid::parse_str(p.persona_id.as_str())
            .map_err(|e| CommandError::Arguments(format!("persona_id: {e}")))?;
        let selected = p.selected.clone();
        let prepared = tokio::task::spawn_blocking(move || {
            let dir = crate::cognition::capture::fixture_dir(prompt_capture::FIXTURE_DIR)
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
        let registry = crate::ai::adapter::global_registry();
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
                model_from_response: source.model_from_response,
                original_max_tokens,
                replay_max_tokens,
                original,
                replay: ReplayOutcome::from(&response),
                tools_executed: false,
                input_comparison: source.input_comparison,
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
            active_adapters: Some(vec![crate::ai::ActiveAdapterRequest {
                name: "captured-adapter".into(), path: "captured.gguf".into(), domain: "coding".into(), scale: 0.75,
            }]),
            tools: Some(vec![crate::ai::NativeToolSpec {
                name: "edit_file".into(), description: "Captured editing contract".into(),
                input_schema: serde_json::from_value(serde_json::json!({
                    "type":"object", "properties":{"mode":{"$ref":"#/definitions/Mode"}},
                    "required":["mode"], "definitions":{"Mode":{"type":"string","enum":["append"]}}
                })).unwrap(),
            }]),
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
            omit_message_indices: Vec::new(),
            expected_request_sha256: None,
            tool_schema_override: None,
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
        // A diagnostic changes only named historical pointers, never tool protocol or settings.
        let mut diagnostic = request.clone();
        let wm = crate::cognition::working_memory::WorkingMemory::new(8);
        wm.record_receipt_typed(&[], "historical result", Some(room),
            crate::ai::types::ResultCustody::Whole { handle: "0123abcd".into() });
        wm.record_receipt("current result");
        let pointer = wm.recent_results_with_pointers().into_iter()
            .find_map(|(_, pointer)| pointer).expect("the production WM projection emits a pointer");
        diagnostic.messages.push(ChatMessage::text("user", pointer));
        diagnostic.messages.push(serde_json::from_value(serde_json::json!({
            "role":"assistant", "content":[{"type":"tool_use", "id":"native-call", "name":"read_file", "input":{"path":"task.rs"}}]
        })).unwrap());
        diagnostic.messages.push(serde_json::from_value(serde_json::json!({
            "role":"user", "content":[{"type":"tool_result", "tool_use_id":"native-call", "content":"current result"}]
        })).unwrap());
        let full = serde_json::to_value(&diagnostic).unwrap();
        let mut omission = p.clone();
        omission.max_tokens = None;
        omission.expected_request_sha256 = Some(request_hash(&diagnostic).unwrap());
        omission.omit_message_indices = vec![1];
        let audit = compare_input(&mut diagnostic, &omission).unwrap();
        let mut wanted = full.clone();
        wanted["messages"].as_array_mut().unwrap().remove(1);
        assert_eq!(serde_json::to_value(&diagnostic).unwrap(), wanted);
        assert_eq!(audit.omitted_message_indices, vec![1]);
        assert_ne!(audit.captured_request_sha256, audit.comparison_request_sha256);
        for bad in [vec![0], vec![2], vec![3], vec![99], vec![1, 1]] {
            let mut copy: TextGenerationRequest = serde_json::from_value(full.clone()).unwrap();
            omission.omit_message_indices = bad;
            assert!(compare_input(&mut copy, &omission).is_err());
            assert_eq!(serde_json::to_value(copy).unwrap(), full, "refusal is non-mutating");
        }
        omission.omit_message_indices = vec![1];
        for role in ["system", "assistant", "tool"] {
            let mut copy: TextGenerationRequest = serde_json::from_value(full.clone()).unwrap();
            copy.messages[1].role = role.into();
            omission.expected_request_sha256 = Some(request_hash(&copy).unwrap());
            assert!(compare_input(&mut copy, &omission).is_err(), "protected role {role}");
        }
        omission.expected_request_sha256 = Some("stale".into());
        assert!(compare_input(&mut request.clone(), &omission).is_err());
        omission.expected_request_sha256 = None;
        assert!(compare_input(&mut request.clone(), &omission).is_err());
        let exact = compare_input(&mut request.clone(), &p).unwrap();
        assert!(exact.omitted_message_indices.is_empty());
        assert_eq!(exact.captured_request_sha256, exact.comparison_request_sha256);

        // Regression: the named-schema experiment changes no tool identity, request,
        // adapter or protocol field, and all invalid variants refuse before mutation.
        // Compare the immutable capture in its own JSON representation: typed f32
        // serialization through Value widens 0.35, unlike the original JSON writer.
        let captured_request = read().submitted.unwrap()["request"].clone();
        let mut schema_params = p.clone();
        schema_params.max_tokens = None;
        schema_params.expected_request_sha256 = Some(request_hash(&request).unwrap());
        let inline = serde_json::json!({"type":"object", "properties":{"mode":{"type":"string","enum":["append"]}}, "required":["mode"]});
        schema_params.tool_schema_override = Some(ToolSchemaOverride {
            tool_name: "edit_file".into(), input_schema: inline.clone(),
        });
        let schema_prepared = prepare(read(), &schema_params, persona).unwrap();
        let mut changed = request.clone();
        let audit = compare_input(&mut changed, &schema_params).unwrap();
        let mut expected_schema = serde_json::to_value(&request).unwrap();
        expected_schema["tools"][0]["input_schema"] = inline.clone();
        assert_eq!(serde_json::to_value(&changed).unwrap(), expected_schema);
        let schema_audit = audit.tool_schema.as_ref().unwrap();
        assert_eq!(schema_audit.tool_name, "edit_file");
        assert_ne!(schema_audit.captured_schema_sha256, schema_audit.comparison_schema_sha256);
        assert_eq!(schema_audit.comparison_schema_sha256, value_hash(inline).unwrap());
        assert_ne!(audit.captured_request_sha256, audit.comparison_request_sha256);
        for bad in 0..7 {
            let mut params = schema_params.clone();
            match bad {
                0 => params.expected_request_sha256 = None,
                1 => params.expected_request_sha256 = Some("stale".into()),
                2 => params.max_tokens = Some(42),
                3 => params.omit_message_indices = vec![0],
                4 => params.tool_schema_override.as_mut().unwrap().tool_name = "absent".into(),
                5 => params.tool_schema_override.as_mut().unwrap().input_schema = serde_json::json!([]),
                _ => params.tool_schema_override.as_mut().unwrap().input_schema["additionalProperties"] = serde_json::json!(false),
            }
            let mut copy = request.clone();
            assert!(compare_input(&mut copy, &params).is_err());
            assert_eq!(serde_json::to_value(&copy).unwrap(), serde_json::to_value(&request).unwrap());
        }
        let mut duplicate = request.clone();
        let repeated_tool = duplicate.tools.as_ref().unwrap()[0].clone();
        duplicate.tools.as_mut().unwrap().push(repeated_tool);
        let mut duplicate_params = schema_params.clone();
        duplicate_params.expected_request_sha256 = Some(request_hash(&duplicate).unwrap());
        assert!(compare_input(&mut duplicate, &duplicate_params).is_err());
        assert_eq!(read().submitted.unwrap()["request"], captured_request);

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
        let prepared = schema_prepared;
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
        // Actual Kimi captures leave request.model null and report the serving
        // model only at completion. Replay must pin that receipt and expose it.
        let mut implicit = read();
        implicit.submitted.as_mut().expect("submitted")["request"]["model"] =
            serde_json::Value::Null;
        implicit.terminal = Some(serde_json::json!({"response": &response}));
        let implicit_replay = prepare(implicit, &p, persona).expect("observed model binding");
        assert_eq!(
            implicit_replay.request.model.as_deref(),
            Some("captured-model")
        );
        assert!(
            implicit_replay
                .call
                .replay_of
                .as_ref()
                .expect("provenance")
                .model_from_response
        );
        let implicit_id = implicit_replay.call.persona_id;
        let implicit_sink =
            JsonlPromptCaptureSink::open(dir.path(), implicit_id).expect("implicit sink");
        let implicit_capture = CaptureLease::start(
            Arc::new(implicit_sink),
            &implicit_replay.call,
            &implicit_replay.request,
        );
        let implicit_cursor = implicit_capture
            .cursor()
            .expect("implicit recorded")
            .to_string();
        drop(implicit_capture);
        let recorded = prompt_capture::detail(dir.path(), implicit_id, &implicit_cursor)
            .expect("binding record");
        assert_eq!(
            recorded.submitted.expect("binding submission")["replay_of"]["model_from_response"],
            true
        );
        let mut unknown = read();
        unknown.submitted.as_mut().expect("submitted")["request"]["model"] =
            serde_json::Value::Null;
        unknown.terminal = Some(serde_json::json!({"response": &response}));
        unknown.terminal.as_mut().expect("terminal")["response"]["model"] = serde_json::json!(" ");
        assert!(prepare(unknown, &p, persona).is_err());
        // A request binding takes precedence over a provider's different spelling.
        let mut explicit = read();
        explicit.terminal = Some(serde_json::json!({"response": &response}));
        explicit.terminal.as_mut().expect("terminal")["response"]["model"] =
            serde_json::json!("provider-alias");
        let explicit = prepare(explicit, &p, persona).expect("request binding");
        assert_eq!(explicit.request.model.as_deref(), Some("captured-model"));
        assert!(
            !explicit
                .call
                .replay_of
                .expect("request provenance")
                .model_from_response
        );
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
        let seen = seen.lock().expect("recorded calls");
        assert_eq!(seen.len(), 1);
        assert_eq!(serde_json::to_value(&seen[0].tools).unwrap(), expected_schema["tools"]);
        assert!(result.input_comparison.tool_schema.is_some());
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
