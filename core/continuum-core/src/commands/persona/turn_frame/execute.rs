//! `persona/turn-execute` — chain a persona's full Rust turn in one IPC hop (typed,
//! dep-holding).
//!
//! Lane D of the persona substrate (alpha card #1409). Where
//! [`drain`](super::drain) stops at the replay-stable turn frame, this command carries
//! the frame all the way through inference:
//!
//! ```text
//!   drain inbox
//!     -> wrap in PersonaTurnFrame
//!     -> derive ResponsePrompt (lazy output)
//!     -> build InferenceRequest (prompt_text path)
//!     -> dispatch `inference/llm/request` via the Rust ModuleRegistry only
//!     -> bundle { replayRecord, inferenceResponse }
//! ```
//!
//! Why one command: the TS persona loop previously executed each stage with its own IPC
//! round-trip (drain, then build prompt, then call inference) — 3 round-trips per turn,
//! with prompt-building living in TS. Lane D pulls all three into the substrate so
//! (a) the prompt is built in Rust where the turn frame lives, (b) the production replay
//! record carries the exact prompt that fed inference, (c) the persona turn becomes one
//! observable unit on the bus.
//!
//! Captures the owning [`CognitionModule`](crate::modules::cognition::CognitionModule)'s
//! shared [`CognitionState`] — the per-persona inbox lives on it, and its
//! `module_registry` is the seam to the Rust inference module. Assembled by
//! [`command_objects`](super::command_objects), called from `CognitionModule::commands`.
//!
//! Fail-loud notes: a persona with no cognition engine is a caller bug
//! (`CommandError::Invalid`); a missing / unroutable Rust module registry refuses to
//! fall through to any TypeScript path (`CommandError::Internal` naming the cause). An
//! **empty** drain is a legitimate no-op: it returns `{ replayRecord: null,
//! inferenceResponse: null }` BEFORE any inference dispatch, never an error.
//!
//! `access: Internal` — substrate cognition IPC the host persona loop drives, not a
//! remote-callable persona toolbelt verb.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use crate::logging::TimingGuard;
use crate::modules::cognition::{record_drained_turn_frame, CognitionState};
use crate::persona::turn_frame::{PersonaTurnFrame, PersonaTurnFrameReplayRecord};
use crate::runtime::ModuleRegistry;
use crate::sdk_codegen::CommandError;

/// Default frame window (ms) when the caller omits `windowMs`. Transplanted from the arm.
fn default_window_ms() -> u64 {
    80
}

/// Default max messages a single drain pulls when the caller omits `maxItems`. From the arm.
fn default_max_items() -> u64 {
    16
}

/// Default generation cap (tokens) when the caller omits `maxTokens`: a conservative
/// bound so a misconfigured caller doesn't run unbounded inference. From the arm.
fn default_max_tokens() -> u64 {
    512
}

/// Default generation wall-clock cap (ms) when the caller omits `maxDurationMs`. From the arm.
fn default_max_duration_ms() -> u64 {
    10_000
}

/// Params for `persona/turn-execute`: which persona to turn, the drain frame bounds, and
/// the optional composition + generation budget. Everything but `personaId` falls back to
/// the substrate defaults, so the minimal call is `{ personaId }` — matching the legacy
/// `u64_or` / `uuid_opt` reads.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/TurnExecuteParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct TurnExecuteParams {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    #[serde(default = "default_window_ms")]
    #[ts(type = "number")]
    pub window_ms: u64,
    #[serde(default = "default_max_items")]
    #[ts(type = "number")]
    pub max_items: u64,
    /// Genome composition artifact to page in for the turn. Omitted → the nil artifact
    /// (the substrate's canonical "no explicit composition" sentinel).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub composition_artifact_id: Option<Uuid>,
    #[serde(default = "default_max_tokens")]
    #[ts(type = "number")]
    pub max_tokens: u64,
    #[serde(default = "default_max_duration_ms")]
    #[ts(type = "number")]
    pub max_duration_ms: u64,
}

/// The bundled outcome of one persona turn: the replay-stable turn frame plus the raw
/// inference response the Rust module returned. Both are `null` on an empty drain (no-op);
/// `inferenceResponse` is the untyped module cell projection (its shape belongs to the
/// inference module, not this command), so it rides as `unknown`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/persona/TurnExecuteResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct TurnExecuteResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub replay_record: Option<PersonaTurnFrameReplayRecord>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "unknown")]
    pub inference_response: Option<Value>,
}

crate::action_command! {
    /// Execute a persona's full turn in one hop: drain the inbox into a replay-stable
    /// frame, build the response prompt in Rust, dispatch inference through the Rust
    /// module registry, and bundle `{ replayRecord, inferenceResponse }`. Returns the
    /// null pair when the drain window was empty (no-op). Substrate cognition IPC the
    /// host persona loop drives; not a persona toolbelt verb.
    pub struct TurnExecute { state: Arc<CognitionState> }
    name: "persona/turn-execute",
    access: Internal,
    params: TurnExecuteParams,
    output: TurnExecuteResult,
    run(this, _ctx, params) => {
        let _timer = TimingGuard::new("module", "persona_turn_execute");

        let max_items = usize::try_from(params.max_items)
            .map_err(|_| CommandError::Invalid(format!("max_items too large: {}", params.max_items)))?;
        let max_tokens = u32::try_from(params.max_tokens)
            .map_err(|_| CommandError::Invalid("max_tokens too large for u32".to_string()))?;
        let max_duration_ms = u32::try_from(params.max_duration_ms)
            .map_err(|_| CommandError::Invalid("max_duration_ms too large for u32".to_string()))?;
        let composition_artifact_id = params.composition_artifact_id.unwrap_or(Uuid::nil());

        let persona = this
            .state
            .personas
            .get(&params.persona_id)
            .ok_or_else(|| CommandError::Invalid(format!("No cognition for {}", params.persona_id)))?;

        let raw_frame = persona.inbox.drain_frame(params.window_ms, max_items);
        record_drained_turn_frame(&raw_frame);

        // Empty drain: returned as the null pair, NOT an Err. Idle ticks are routine; a
        // no-op is the correct outcome, short-circuiting BEFORE any inference dispatch.
        let inbox_frame = match raw_frame {
            Some(f) => f,
            None => {
                return Ok(TurnExecuteResult {
                    replay_record: None,
                    inference_response: None,
                });
            }
        };

        let turn_frame = PersonaTurnFrame::from_inbox_frame(inbox_frame);
        let replay_record = turn_frame.replay_record();
        if let Some(ref rec) = replay_record {
            crate::persona::recorder::record_turn_frame_replay(rec);
        }

        let response_prompt = turn_frame.response_prompt().ok_or_else(|| {
            CommandError::Internal(format!(
                "persona/turn-execute: non-empty drain produced no ResponsePrompt for {}",
                params.persona_id
            ))
        })?;

        // Build the substrate InferenceRequest. request_id is fresh per-turn; the persona
        // + composition come from the turn frame + caller. prompt_text is the flattened
        // ResponsePrompt; prompt_tokens is empty (adapter-path).
        let inference_request = crate::inference::llm_module::InferenceRequest {
            request_id: crate::inference::llm_module::InferenceRequestId::new(Uuid::new_v4()),
            persona: crate::identity::PeerId::from_uuid(params.persona_id),
            composition: crate::inference::llm_module::CompositionPlan(
                crate::genome::working_set::ArtifactId::new(composition_artifact_id),
            ),
            prompt_tokens: vec![],
            prompt_text: Some(response_prompt.to_prompt_text()),
            budget: crate::inference::llm_module::GenerationBudget {
                max_tokens,
                max_duration_ms,
            },
            sampling: crate::inference::llm_module::SamplingParams::default(),
            stop_sequences: vec![],
        };

        let inference_response = execute_rust_module_json(
            this.state.module_registry.as_deref(),
            crate::inference::llm_module_service::COMMAND_REQUEST,
            serde_json::to_value(&inference_request).map_err(|e| {
                CommandError::Internal(format!("Serialize inference request: {e}"))
            })?,
        )
        .await
        .map_err(|e| {
            CommandError::Internal(format!(
                "persona/turn-execute: Rust inference dispatch failed for {}: {e}",
                params.persona_id
            ))
        })?;

        Ok(TurnExecuteResult {
            replay_record,
            inference_response: Some(inference_response),
        })
    }
}

/// Dispatch a Rust `ModuleRegistry` command and project its result cell into a plain JSON
/// [`Value`]. The sole seam by which `persona/turn-execute` reaches the inference module
/// in-process. Fails loud — a missing registry or unrouted command refuses to fall
/// through to any TypeScript path (moved here verbatim with its only consumer during the
/// Lane D migration off `CognitionModule::handle_command`).
async fn execute_rust_module_json(
    registry: Option<&ModuleRegistry>,
    command: &str,
    params: Value,
) -> Result<Value, String> {
    let registry = registry.ok_or_else(|| {
        format!("{command}: Rust module registry unavailable; refusing TypeScript fallback")
    })?;
    let (module, routed_command) = registry.route_command(command).ok_or_else(|| {
        format!("{command}: no Rust module route registered; refusing TypeScript fallback")
    })?;

    module
        .handle_command(&routed_command, params)
        .await?
        .to_json_value()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inference::llm_module_service::InferenceLlmModule;
    use crate::persona::{InboxMessage, Modality, PersonaCognition, SenderType};
    use crate::rag::RagEngine;
    use crate::sdk_codegen::{AccessLevel, ActionCommand, Ctx};

    // Build a TurnExecute command over a fresh CognitionState carrying one live persona,
    // optionally wired with a Rust ModuleRegistry (the inference dispatch seam). Mirrors
    // the legacy turn_execute_tests helpers, now targeting the migrated command object.
    fn command_with_persona(
        persona_id: Uuid,
        registry: Option<Arc<ModuleRegistry>>,
    ) -> (TurnExecute, Arc<CognitionState>) {
        let rag_engine = Arc::new(RagEngine::new());
        let mut state = CognitionState::new(rag_engine.clone());
        if let Some(registry) = registry {
            state = state.with_module_registry(registry);
        }
        let state = Arc::new(state);
        state.personas.insert(
            persona_id,
            PersonaCognition::new(persona_id, "Test Persona".to_string(), rag_engine),
        );
        (
            TurnExecute {
                state: state.clone(),
            },
            state,
        )
    }

    fn rust_inference_registry() -> Arc<ModuleRegistry> {
        let registry = Arc::new(ModuleRegistry::new());
        registry.register(Arc::new(InferenceLlmModule::new()));
        registry
    }

    fn enqueue_message(state: &CognitionState, persona_id: Uuid, content: &str, timestamp: u64) {
        let persona = state.personas.get(&persona_id).expect("test persona exists");
        persona.inbox.enqueue(InboxMessage {
            id: Uuid::new_v4(),
            room_id: Uuid::new_v4(),
            sender_id: Uuid::new_v4(),
            sender_name: "Joel".to_string(),
            sender_type: SenderType::Human,
            content: content.to_string(),
            timestamp,
            priority: 0.9,
            source_modality: Some(Modality::Chat),
            voice_session_id: None,
        });
    }

    fn params(persona_id: Uuid) -> TurnExecuteParams {
        serde_json::from_value(serde_json::json!({ "personaId": persona_id.to_string() }))
            .expect("minimal params deserialize")
    }

    // what this catches: the name + access contract. turn-execute is host-driven
    // substrate IPC (the persona loop turns its own inbox), so it stays Internal —
    // registered and grid-routable, never a remote-callable persona toolbelt verb.
    #[test]
    fn name_and_access_are_the_contract() {
        assert_eq!(TurnExecute::NAME, "persona/turn-execute");
        assert_eq!(TurnExecute::ACCESS, AccessLevel::Internal);
    }

    // what this catches: the generation-budget + frame defaults survive an absent-field
    // payload. The legacy arm read u64_or(.., 80/16/512/10_000); a `{ personaId }` call
    // must still get those bounds, not a deserialize error. Guards the serde(default) wiring
    // that replaced the u64_or reads.
    #[test]
    fn defaults_fill_absent_turn_bounds() {
        let p = params(Uuid::nil());
        assert_eq!(p.window_ms, 80);
        assert_eq!(p.max_items, 16);
        assert_eq!(p.max_tokens, 512);
        assert_eq!(p.max_duration_ms, 10_000);
        assert!(p.composition_artifact_id.is_none());
    }

    // what this catches: a turn for a persona with no cognition engine fails loud
    // (CommandError::Invalid naming the persona), never a silent empty bundle.
    #[tokio::test]
    async fn persona_not_found_fails_loud() {
        let rag_engine = Arc::new(RagEngine::new());
        let state = Arc::new(CognitionState::new(rag_engine));
        let cmd = TurnExecute { state };

        let missing = Uuid::new_v4();
        let err = cmd
            .run(&Ctx::default(), params(missing))
            .await
            .expect_err("missing persona must surface typed Err");
        match err {
            CommandError::Invalid(msg) => {
                assert!(msg.contains("No cognition for"), "got: {msg}");
                assert!(msg.contains(&missing.to_string()));
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }

    // what this catches: an empty inbox short-circuits to the null pair BEFORE any
    // inference dispatch — a no-op, not an error (idle ticks are routine).
    #[tokio::test]
    async fn empty_drain_returns_null_bundle() {
        let persona_id = Uuid::new_v4();
        let (cmd, _state) = command_with_persona(persona_id, None);

        let out = cmd
            .run(&Ctx::default(), params(persona_id))
            .await
            .expect("empty drain is a no-op, not an error");
        assert!(out.replay_record.is_none(), "empty drain → null replayRecord");
        assert!(
            out.inference_response.is_none(),
            "empty drain → null inferenceResponse (dispatch never ran)"
        );
    }

    // what this catches: a non-empty drain routes through the Rust inference module and
    // bundles the replay record + module response. The registered InferenceLlmModule stub
    // returns 3 tokens, proving Rust-only dispatch reached inference (no TS fallback), and
    // the turn drains the persona's inbox to empty.
    #[tokio::test]
    async fn success_routes_through_rust_inference_module() {
        let persona_id = Uuid::new_v4();
        let (cmd, state) = command_with_persona(persona_id, Some(rust_inference_registry()));
        enqueue_message(&state, persona_id, "what changed?", 20_000);

        let out = cmd
            .run(&Ctx::default(), params(persona_id))
            .await
            .expect("Rust inference module handles turn");

        let value = serde_json::to_value(&out).expect("result serializes");
        assert_eq!(
            value["replayRecord"]["responsePrompt"]["messages"][0]["content"],
            "Joel: what changed?"
        );
        assert_eq!(
            value["inferenceResponse"]["complete"]["tokensGenerated"], 3,
            "registered InferenceLlmModule stub proves Rust-only dispatch reached inference"
        );
        assert!(
            state
                .personas
                .get(&persona_id)
                .expect("persona remains")
                .inbox
                .is_empty(),
            "turn-execute drains one consolidated frame"
        );
    }

    // what this catches: a non-empty turn with NO Rust module registry refuses to fall
    // through to TypeScript — it fails loud (CommandError::Internal) naming the refusal.
    #[tokio::test]
    async fn missing_rust_registry_refuses_ts_fallback() {
        let persona_id = Uuid::new_v4();
        let (cmd, state) = command_with_persona(persona_id, None);
        enqueue_message(&state, persona_id, "do not fall back to ts", 30_000);

        let err = cmd
            .run(&Ctx::default(), params(persona_id))
            .await
            .expect_err("missing Rust registry must not fall through");
        match err {
            CommandError::Internal(msg) => assert!(
                msg.contains("refusing TypeScript fallback"),
                "expected loud no-TS-fallback refusal, got: {msg}"
            ),
            other => panic!("expected Internal, got {other:?}"),
        }
    }

    // what this catches: an out-of-range generation budget fails the u32 conversion loud
    // (CommandError::Invalid) rather than silently truncating. Pins the param-parse guard
    // the legacy arm expressed via u32::try_from on max_duration_ms.
    #[tokio::test]
    async fn overflow_budget_fails_loud() {
        let persona_id = Uuid::new_v4();
        let (cmd, _state) = command_with_persona(persona_id, None);

        let mut p = params(persona_id);
        p.max_duration_ms = u64::MAX;
        let err = cmd
            .run(&Ctx::default(), p)
            .await
            .expect_err("u64::MAX max_duration_ms must fail u32 conversion");
        match err {
            CommandError::Invalid(msg) => {
                assert!(msg.contains("max_duration_ms too large"), "got: {msg}")
            }
            other => panic!("expected Invalid, got {other:?}"),
        }
    }
}
