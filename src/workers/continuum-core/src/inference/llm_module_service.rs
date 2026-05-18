//! `inference-llm` PR-2: `InferenceLlmModule` ServiceModule impl.
//!
//! PR-1 (#1387) shipped the typed event surface. PR-2 wires the
//! ServiceModule that accepts InferenceRequest commands + emits
//! the response events. The actual llama.cpp invoke lands in PR-3;
//! PR-2 ships a STUB inference that returns canned tokens so the
//! seam is testable end-to-end + downstream consumers
//! (sentinel-observer, VDD harness) can wire to it today.
//!
//! ## What PR-2 ships
//!
//! - `InferenceLlmModule` struct implementing `ServiceModule`
//! - `inference/llm/request` command — accepts InferenceRequest
//!   JSON, runs the stub inference, returns InferenceComplete +
//!   FirstTokenEmitted as JSON
//! - Stub inference returns 3 canned tokens [1, 2, 3] with
//!   `FinishReason::Stop`. Documented as PR-3 deferral.
//! - Tests pin the wire contract: request → response correlation
//!   via `requestId`, finish reason, token count, TTFT field
//!
//! ## What PR-2 does NOT ship (PR-3)
//!
//! - Real llama.cpp invocation (`LlamaCppAdapter` integration)
//! - Tokenizer (composition_plan → prompt_tokens)
//! - Token streaming via channels (PR-2 is request/response)
//! - Bus-event subscription path (`artifact_subscriptions`)
//! - ResidencyFault emission on missing-page (needs working-set
//!   integration)
//! - Runtime registration (separate wiring PR or registers when
//!   PR-3 lands the real engine)

use async_trait::async_trait;
use serde_json::Value;
use std::any::Any;

use super::llm_module::{
    FinishReason, FirstTokenEmitted, InferenceComplete, InferenceRequest,
};
use crate::runtime::module_context::ModuleContext;
use crate::runtime::service_module::{
    CommandResult, ModuleConfig, ModulePriority, ServiceModule,
};

/// Per-process implementation of `inference-llm`. ServiceModule
/// trait impl that handles `inference/llm/request` commands.
///
/// PR-2 is stub-backed (canned tokens); PR-3 replaces the stub
/// with the real `LlamaCppAdapter` invoke. The module's external
/// contract (commands + response shapes) stays identical across
/// the stub-vs-real transition — downstream consumers don't
/// need to know which is running.
pub struct InferenceLlmModule;

impl InferenceLlmModule {
    pub fn new() -> Self {
        Self
    }
}

impl Default for InferenceLlmModule {
    fn default() -> Self {
        Self::new()
    }
}

/// The command the module accepts. Producers (persona-cognition)
/// send the InferenceRequest as JSON to this command and receive
/// an InferenceComplete + FirstTokenEmitted bundle in the
/// `CommandResult::Json` payload.
pub const COMMAND_REQUEST: &str = "inference/llm/request";

/// PR-2 stub inference output. Canned 3-token response so tests
/// can pin the wire contract without requiring a real model load.
/// PR-3 replaces with real generation.
const STUB_COMPLETION_TOKENS: &[u32] = &[1, 2, 3];

/// Result of one (stubbed) inference call: the complete event +
/// the first-token event. The command returns both as a JSON
/// object so the caller can publish them individually if it
/// wants, or treat the pair atomically.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InferenceResponse {
    pub complete: InferenceComplete,
    pub first_token: FirstTokenEmitted,
}

#[async_trait]
impl ServiceModule for InferenceLlmModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "inference-llm",
            priority: ModulePriority::High,
            command_prefixes: &["inference/llm/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            // Inference is single-flight per persona; the substrate
            // serializes per-persona at a higher layer. PR-2's stub
            // is reentrant + cheap; PR-3 may need a semaphore when
            // the real backend lands. 0 = unlimited (module manages
            // own concurrency).
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(
        &self,
        command: &str,
        params: Value,
    ) -> Result<CommandResult, String> {
        match command {
            COMMAND_REQUEST => self.handle_request(params).await,
            other => Err(format!(
                "inference-llm: unknown command '{other}' (expected '{COMMAND_REQUEST}')"
            )),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl InferenceLlmModule {
    /// Run the (stubbed) inference for one request. PR-3 replaces
    /// the body with the real llama.cpp invoke path; the outer
    /// shape (params → request, generate, complete + first-token)
    /// stays the same.
    async fn handle_request(&self, params: Value) -> Result<CommandResult, String> {
        let request: InferenceRequest = serde_json::from_value(params)
            .map_err(|e| format!("inference-llm: invalid InferenceRequest payload: {e}"))?;

        // PR-2 stub: pretend we ran a model + emit canned tokens.
        // PR-3 replaces this block with the real LlamaCppAdapter
        // invoke. The InferenceComplete + FirstTokenEmitted wire
        // shapes stay identical across the transition.
        let complete = run_stub_inference(&request);
        let first_token = first_token_for(&request, &complete);

        let response = InferenceResponse {
            complete,
            first_token,
        };
        CommandResult::json(&response)
    }
}

/// PR-2 stub inference. Returns the canned 3-token response with
/// FinishReason::Stop. Useful for testing the request/response
/// wire shape end-to-end without loading a real model.
///
/// Visibility: `pub(super)` so PR-3 can call it from a test that
/// pins "stub vs real produce same wire shape" before swapping
/// the implementation. Production code calls the trait method, not
/// this directly.
pub(super) fn run_stub_inference(request: &InferenceRequest) -> InferenceComplete {
    InferenceComplete {
        request_id: request.request_id,
        persona: request.persona,
        completion_tokens: STUB_COMPLETION_TOKENS.to_vec(),
        finish_reason: FinishReason::Stop,
        elapsed_ms: 1, // stub is fast; real engine fills in real time
        tokens_generated: STUB_COMPLETION_TOKENS.len() as u32,
    }
}

/// Build the FirstTokenEmitted event paired with a completion.
/// PR-2's stub emits TTFT ≈ 0 (inference was instant). PR-3
/// will capture the real first-token wall-clock from inside the
/// streaming generation loop.
pub(super) fn first_token_for(
    request: &InferenceRequest,
    complete: &InferenceComplete,
) -> FirstTokenEmitted {
    let _ = complete; // PR-3 will use complete.elapsed_ms for atomic-engine fallback
    FirstTokenEmitted {
        request_id: request.request_id,
        persona: request.persona,
        elapsed_us: 0, // stub: instant TTFT
    }
}

#[cfg(test)]
mod tests {
    //! Pin the ServiceModule contract + wire shape. PR-3 will add
    //! integration tests that exercise the real engine; PR-2's
    //! tests pin the seam.
    use super::*;
    use crate::genome::working_set::{ArtifactId, PersonaId};
    use crate::inference::llm_module::{
        CompositionPlan, GenerationBudget, InferenceRequestId, SamplingParams,
    };
    use uuid::Uuid;

    fn sample_request() -> InferenceRequest {
        InferenceRequest {
            request_id: InferenceRequestId::new(Uuid::from_u128(42)),
            persona: PersonaId::new(Uuid::from_u128(1)),
            composition: CompositionPlan(ArtifactId::new(Uuid::from_u128(100))),
            prompt_tokens: vec![10, 11, 12],
            budget: GenerationBudget {
                max_tokens: 100,
                max_duration_ms: 5000,
            },
            sampling: SamplingParams::default(),
            stop_sequences: vec![],
        }
    }

    /// What this catches: module config reports its name +
    /// command prefix. The registry uses this for routing; if the
    /// prefix drifts, persona-cognition's request goes to the
    /// wrong module.
    #[test]
    fn config_reports_name_and_command_prefix() {
        let m = InferenceLlmModule::new();
        let cfg = m.config();
        assert_eq!(cfg.name, "inference-llm");
        assert_eq!(cfg.command_prefixes, &["inference/llm/"]);
        assert!(!cfg.needs_dedicated_thread);
    }

    /// What this catches: the module returns High priority. Local
    /// inference is on the user-perceived critical path; the
    /// scheduler treats this above Background but below Realtime
    /// (which is reserved for audio/voice).
    #[test]
    fn config_priority_is_high() {
        let m = InferenceLlmModule::new();
        assert_eq!(m.config().priority, ModulePriority::High);
    }

    /// What this catches: COMMAND_REQUEST constant matches the
    /// canonical wire name. Consumers refer to the constant via
    /// `inference::llm_module_service::COMMAND_REQUEST` so renames
    /// propagate; the literal string here is what drift on.
    #[test]
    fn command_request_has_canonical_string_value() {
        assert_eq!(COMMAND_REQUEST, "inference/llm/request");
    }

    /// What this catches: handle_command routes the canonical
    /// command to the stub inference; the response carries the
    /// expected InferenceComplete + FirstTokenEmitted bundle.
    /// End-to-end test of the seam.
    #[tokio::test]
    async fn handle_command_routes_request_to_stub_inference() {
        let m = InferenceLlmModule::new();
        let req = sample_request();
        let params = serde_json::to_value(&req).unwrap();

        let result = m.handle_command(COMMAND_REQUEST, params).await.unwrap();
        match result {
            CommandResult::Json(v) => {
                let response: InferenceResponse = serde_json::from_value(v).unwrap();
                assert_eq!(response.complete.request_id, req.request_id);
                assert_eq!(response.complete.persona, req.persona);
                assert_eq!(response.complete.completion_tokens, vec![1, 2, 3]);
                assert_eq!(response.complete.finish_reason, FinishReason::Stop);
                assert_eq!(response.complete.tokens_generated, 3);
                assert_eq!(response.first_token.request_id, req.request_id);
            }
            CommandResult::Binary { .. } => panic!("expected Json response"),
        }
    }

    /// What this catches: handle_command for an unknown command
    /// returns a typed Err with the canonical-name in the message.
    /// Loud rejection per Joel's never-swallow rule.
    #[tokio::test]
    async fn handle_command_unknown_returns_loud_error() {
        let m = InferenceLlmModule::new();
        let result = m
            .handle_command("inference/llm/bogus", Value::Null)
            .await;
        match result {
            Err(msg) => {
                assert!(msg.contains("unknown command"));
                assert!(msg.contains(COMMAND_REQUEST));
                assert!(msg.contains("bogus"));
            }
            Ok(_) => panic!("unknown command must return Err"),
        }
    }

    /// What this catches: handle_command for a malformed payload
    /// returns a typed Err with the serde error context. Loud
    /// rejection again — caller can debug from the message.
    #[tokio::test]
    async fn handle_command_invalid_payload_returns_typed_error() {
        let m = InferenceLlmModule::new();
        let result = m
            .handle_command(COMMAND_REQUEST, serde_json::json!({"not": "a request"}))
            .await;
        match result {
            Err(msg) => {
                assert!(msg.contains("invalid InferenceRequest payload"));
            }
            Ok(_) => panic!("invalid payload must return Err"),
        }
    }

    /// What this catches: the InferenceResponse bundle round-trips
    /// through serde. Wire-stable shape for callers that decompose
    /// the bundle into the two events for separate publishing.
    #[tokio::test]
    async fn inference_response_round_trips_through_serde() {
        let req = sample_request();
        let complete = run_stub_inference(&req);
        let first_token = first_token_for(&req, &complete);
        let response = InferenceResponse {
            complete,
            first_token,
        };
        let json = serde_json::to_string(&response).unwrap();
        let back: InferenceResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(back.complete.request_id, req.request_id);
        assert_eq!(back.first_token.request_id, req.request_id);
    }

    /// What this catches: object-safety + dyn dispatch. The
    /// registry holds `Arc<dyn ServiceModule>`; if a future PR
    /// adds a generic method, this construction fails.
    #[tokio::test]
    async fn module_is_object_safe_for_dyn_service_module() {
        let module: std::sync::Arc<dyn ServiceModule> =
            std::sync::Arc::new(InferenceLlmModule::new());
        let cfg = module.config();
        assert_eq!(cfg.name, "inference-llm");

        let req = sample_request();
        let params = serde_json::to_value(&req).unwrap();
        let result = module
            .handle_command(COMMAND_REQUEST, params)
            .await
            .unwrap();
        match result {
            CommandResult::Json(v) => {
                let response: InferenceResponse = serde_json::from_value(v).unwrap();
                assert_eq!(response.complete.request_id, req.request_id);
            }
            _ => panic!("expected Json"),
        }
    }
}
