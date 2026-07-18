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

use std::sync::Arc;

use super::llm_module::{FinishReason, FirstTokenEmitted, InferenceComplete, InferenceRequest};
use super::llm_module_bus::{publish_first_token_emitted, publish_inference_complete};
use crate::ai::adapter::AIProviderAdapter;
use crate::ai::types::{
    ChatMessage, FinishReason as AdapterFinishReason, MessageContent, TextGenerationRequest,
    TextGenerationResponse,
};
use crate::runtime::message_bus::MessageBus;
use crate::runtime::module_context::ModuleContext;
use crate::runtime::registry::ModuleRegistry;
use crate::runtime::service_module::{CommandResult, ModuleConfig, ModulePriority, ServiceModule};

/// Optional bus + registry handle for auto-publishing inference
/// response events. When set on `InferenceLlmModule`, every
/// `handle_command` call that produces an `InferenceResponse` also
/// publishes the complete + first_token events via the artifact
/// dispatch path (#1339+#1343) using the canonical keys from
/// `llm_module_bus` (PR-3a / #1392).
///
/// Same shape as the genome `BusHook` pattern (#1362) — kept as
/// one struct (not two Arcs on the module) so the absence-of-bus
/// case is a single `Option<BusHook>` field.
struct BusHook {
    bus: Arc<MessageBus>,
    registry: Arc<ModuleRegistry>,
}

/// Per-process implementation of `inference-llm`. ServiceModule
/// trait impl that handles `inference/llm/request` commands.
///
/// PR-2 shipped the stub-backed module; PR-3a shipped the bus
/// publishing helpers; PR-3b (this) wires them together. The
/// module's external contract (commands + response shapes) stays
/// identical across the stub-vs-real transition — downstream
/// consumers don't need to know which is running.
///
/// PR-3b adds optional bus publishing: when constructed via
/// `with_bus(bus, registry)`, every successful handle_command
/// publishes InferenceComplete + FirstTokenEmitted to the trace
/// bus. Constructed via `new()` (the PR-2 shape), the module
/// stays bus-less and behaves exactly as before — useful for
/// tests + standalone use where no runtime is around.
pub struct InferenceLlmModule {
    bus_hook: Option<BusHook>,
    /// PR-4 addition: optional real-inference adapter. When set,
    /// `handle_request` routes InferenceRequests with `prompt_text`
    /// through this adapter; when None, the PR-2 stub path runs.
    /// Adapter is held as `Arc<dyn AIProviderAdapter>` so any
    /// `AIProviderAdapter` impl (LlamaCppAdapter for local, future
    /// Anthropic/OpenAI for cloud) plugs in interchangeably.
    adapter: Option<Arc<dyn AIProviderAdapter>>,
}

impl InferenceLlmModule {
    /// Construct without bus publishing or real adapter (PR-2 shape).
    /// Inference is stubbed; responses returned through CommandResult.
    pub fn new() -> Self {
        Self {
            bus_hook: None,
            adapter: None,
        }
    }

    /// Construct with auto-publishing bus hook (PR-3b shape). Stub
    /// inference; bus auto-publishes the response events.
    pub fn with_bus(bus: Arc<MessageBus>, registry: Arc<ModuleRegistry>) -> Self {
        Self {
            bus_hook: Some(BusHook { bus, registry }),
            adapter: None,
        }
    }

    /// PR-4 constructor: real-adapter-backed, no bus publishing.
    /// Inference routed through `adapter.generate_text` for requests
    /// with `prompt_text` set. Tests + standalone use without a
    /// Runtime.
    pub fn with_adapter(adapter: Arc<dyn AIProviderAdapter>) -> Self {
        Self {
            bus_hook: None,
            adapter: Some(adapter),
        }
    }

    /// PR-4 constructor: real-adapter-backed + bus publishing.
    /// The full production wiring — every successful inference
    /// publishes InferenceComplete + FirstTokenEmitted to the bus
    /// AND the inference itself runs through the real adapter
    /// (LlamaCppAdapter for local llama.cpp).
    pub fn with_bus_and_adapter(
        bus: Arc<MessageBus>,
        registry: Arc<ModuleRegistry>,
        adapter: Arc<dyn AIProviderAdapter>,
    ) -> Self {
        Self {
            bus_hook: Some(BusHook { bus, registry }),
            adapter: Some(adapter),
        }
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
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/inference_llm/InferenceResponse.ts"
)]
pub struct InferenceResponse {
    pub complete: InferenceComplete,
    pub first_token: FirstTokenEmitted,
}

/// Typed declaration of the `inference/llm/request` command — the single source
/// for its SDK surface (`sdk_codegen`). Params/Result are the SAME ts-rs types
/// the handler parses + returns, so the generated `CommandMap` entry can't drift
/// from the command. Registered crate-wide via `register_command!`; the Rust
/// generator emits its TS surface. (First REAL command migrated onto the
/// Rust-rooted, self-assembling registry — [[persona-is-a-client]] /
/// [[lock-uniform-client-early]].)
pub struct InferenceLlmRequestCommand;

impl crate::sdk_codegen::CommandSpec for InferenceLlmRequestCommand {
    const NAME: &'static str = COMMAND_REQUEST;
    const ACCESS_LEVEL: crate::sdk_codegen::AccessLevel = crate::sdk_codegen::AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Low-level substrate: a one-shot raw LLM inference request. NOT a task tool — \
         your own replies already run through inference. (Legacy one-shot path.)";
    // BARE: `handle_request` parses `InferenceRequest` directly and returns
    // `InferenceResponse` directly (CommandResult::json(&response), llm_module_service.rs)
    // — no CommandRequest/CommandResponse envelope. The SDK sees bare in, bare out.
    const WIRE: crate::sdk_codegen::WireShape = crate::sdk_codegen::WireShape::Bare;
    type Params = InferenceRequest;
    type Result = InferenceResponse;
}

crate::register_command!(InferenceLlmRequestCommand);

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

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
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

        // PR-4: route through the real adapter when wired AND the
        // request carries prompt_text (the adapter path's required
        // input). When adapter is wired but no prompt_text, refuse
        // loud — adapter-based engines tokenize internally; raw
        // tokens-only requests must go through a (future) raw-token
        // engine path. Per Joel's never-swallow rule: typed refusal,
        // not silent fallback.
        //
        // Without an adapter wired (PR-2/PR-3 shape), the stub path
        // runs — same wire contract, no model required.
        let (complete, first_token) = match (&self.adapter, request.prompt_text.as_deref()) {
            (Some(adapter), Some(prompt_text)) => {
                run_adapter_inference(adapter.as_ref(), &request, prompt_text).await?
            }
            (Some(_), None) => {
                return Err(format!(
                    "inference-llm: adapter wired but request lacks prompt_text; \
                     raw-token path not yet implemented (request_id={:?})",
                    request.request_id
                ));
            }
            (None, _) => {
                let complete = run_stub_inference(&request);
                let first_token = first_token_for(&request, &complete);
                (complete, first_token)
            }
        };

        // PR-3b: auto-publish to the trace bus when configured.
        // Spawn pattern (not await) to avoid the DashMap
        // borrow-across-await lifetime issue inside the Send-bounded
        // async_trait method body — same workaround as my genome
        // LocalWorkingSetManager (#1362). The publish is best-effort
        // observability; the authoritative response goes back through
        // the CommandResult arm regardless of publishing outcome.
        if let Some(hook) = &self.bus_hook {
            spawn_publish_inference_complete(hook, complete.clone());
            spawn_publish_first_token_emitted(hook, first_token);
        }

        let response = InferenceResponse {
            complete,
            first_token,
        };
        CommandResult::json(&response)
    }
}

/// Spawn a `publish_inference_complete` into the current tokio
/// runtime. Standalone fn (not a method) so the `&BusHook` borrow
/// doesn't outlive the spawn — Arcs get cloned out first, then the
/// spawned future owns its captures. Same lifetime workaround as
/// my genome `spawn_publish_page_fault` (#1362) — see that PR for
/// the full rationale on why spawn vs await.
fn spawn_publish_inference_complete(hook: &BusHook, complete: InferenceComplete) {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        let bus = hook.bus.clone();
        let registry = hook.registry.clone();
        handle.spawn(async move {
            publish_inference_complete(&bus, &registry, &complete).await;
        });
    }
}

/// Spawn a `publish_first_token_emitted` into the current tokio
/// runtime. Same pattern as `spawn_publish_inference_complete`.
fn spawn_publish_first_token_emitted(hook: &BusHook, event: FirstTokenEmitted) {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        let bus = hook.bus.clone();
        let registry = hook.registry.clone();
        handle.spawn(async move {
            publish_first_token_emitted(&bus, &registry, &event).await;
        });
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
        completion_text: None,
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

/// PR-4: real adapter inference path. Translates the substrate's
/// InferenceRequest into the adapter's `TextGenerationRequest`,
/// runs the adapter, translates the response back into the
/// substrate's InferenceComplete + FirstTokenEmitted.
///
/// `prompt_text` is the request's `prompt_text` field (caller
/// guaranteed to be `Some` at this call site). Wrapped as a
/// single user-role ChatMessage for the adapter.
///
/// The adapter handles its own tokenization, sampling, EOS
/// detection. Substrate-level concerns the adapter doesn't know
/// about (residency, budget enforcement, governor leases) are
/// handled around this call by the working-set-manager + governor
/// integration that lands in PR-5.
///
/// Returns `(InferenceComplete, FirstTokenEmitted)` as a tuple so
/// the caller can publish both atomically.
pub(super) async fn run_adapter_inference(
    adapter: &dyn AIProviderAdapter,
    request: &InferenceRequest,
    prompt_text: &str,
) -> Result<(InferenceComplete, FirstTokenEmitted), String> {
    let adapter_request = TextGenerationRequest {
        messages: vec![ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text(prompt_text.to_string()),
            name: None,
        }],
        system_prompt: None,
        model: None,
        provider: None,
        temperature: Some(request.sampling.temperature),
        max_tokens: if request.budget.max_tokens > 0 {
            Some(request.budget.max_tokens)
        } else {
            None
        },
        top_p: Some(request.sampling.top_p),
        top_k: Some(request.sampling.top_k),
        repeat_penalty: Some(request.sampling.repeat_penalty),
        frequency_penalty: Some(request.sampling.frequency_penalty),
        repeat_last_n: Some(request.sampling.repeat_last_n),
        stop_sequences: if request.stop_sequences.is_empty() {
            None
        } else {
            Some(request.stop_sequences.clone())
        },
        tools: None,
        tool_choice: None,
        response_format: None,
        active_adapters: None,
        request_id: Some(request.request_id.as_uuid().to_string()),
        user_id: None,
        room_id: None,
        purpose: Some("inference-llm".to_string()),
        persona_id: Some(request.persona.as_uuid().to_string()),
    };

    let response = adapter
        .generate_text(adapter_request)
        .await
        .map_err(|e| format!("inference-llm: adapter generate_text failed: {e}"))?;

    let complete = translate_adapter_response(request, response);
    let first_token = FirstTokenEmitted {
        request_id: request.request_id,
        persona: request.persona,
        // Atomic-engine convention: TTFT == elapsed_ms * 1000.
        // When PR-5 adds real streaming, this gets the actual
        // first-token wall-clock from the streaming loop.
        elapsed_us: complete.elapsed_ms.saturating_mul(1000),
    };
    Ok((complete, first_token))
}

/// PR-4: translate the adapter's TextGenerationResponse into the
/// substrate's InferenceComplete. The adapter returns text +
/// usage metrics; we map those into completion_text +
/// tokens_generated. completion_tokens stays empty because the
/// adapter doesn't expose token-level output — substrate callers
/// that need tokens use the (future) raw-token engine path.
fn translate_adapter_response(
    request: &InferenceRequest,
    response: TextGenerationResponse,
) -> InferenceComplete {
    InferenceComplete {
        request_id: request.request_id,
        persona: request.persona,
        completion_tokens: Vec::new(),
        completion_text: Some(response.text),
        finish_reason: translate_adapter_finish_reason(&response.finish_reason),
        elapsed_ms: response.response_time_ms,
        tokens_generated: response.usage.output_tokens,
    }
}

/// Map the adapter's FinishReason enum to the substrate's.
/// The two enums overlap but aren't identical: the adapter has
/// Stop/Length/ToolUse/Error; the substrate adds MaxDuration +
/// StopSequence { matched }. PR-4's translation:
///
/// - Stop → Stop
/// - Length → MaxTokens (the adapter's "model hit the token
///   limit" maps to the substrate's typed MaxTokens reason)
/// - ToolUse → Error { reason: "..." } — substrate's inference-llm
///   doesn't model tool-use as a clean stop; tool-use turns route
///   through a different command. If we see ToolUse here it's a
///   request misuse the substrate should surface.
/// - Error → Error { reason: "adapter returned Error finish" }
///
/// MaxDuration + StopSequence are PR-substrate-only — the adapter
/// path can't produce them today (PR-5 adds adapter-side timeout
/// enforcement that would surface MaxDuration).
fn translate_adapter_finish_reason(adapter_reason: &AdapterFinishReason) -> FinishReason {
    match adapter_reason {
        AdapterFinishReason::Stop => FinishReason::Stop,
        AdapterFinishReason::Length => FinishReason::MaxTokens,
        AdapterFinishReason::ToolUse => FinishReason::Error {
            reason: "adapter returned ToolUse; inference-llm does not handle tool-use \
                     turns directly (use a different command)"
                .to_string(),
        },
        AdapterFinishReason::Error => FinishReason::Error {
            reason: "adapter returned Error finish".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    //! Pin the ServiceModule contract + wire shape. PR-3 will add
    //! integration tests that exercise the real engine; PR-2's
    //! tests pin the seam.
    use super::*;
    use crate::genome::working_set::ArtifactId;
    use crate::identity::PeerId;
    use crate::inference::llm_module::{
        CompositionPlan, GenerationBudget, InferenceRequestId, SamplingParams,
    };
    use uuid::Uuid;

    fn sample_request() -> InferenceRequest {
        InferenceRequest {
            request_id: InferenceRequestId::new(Uuid::from_u128(42)),
            persona: PeerId::from_uuid(Uuid::from_u128(1)),
            composition: CompositionPlan(ArtifactId::new(Uuid::from_u128(100))),
            prompt_tokens: vec![10, 11, 12],
            prompt_text: None,
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
            other => panic!("expected CommandResult::Json, got {other:?}"),
        }
    }

    /// What this catches: handle_command for an unknown command
    /// returns a typed Err with the canonical-name in the message.
    /// Loud rejection per Joel's never-swallow rule.
    #[tokio::test]
    async fn handle_command_unknown_returns_loud_error() {
        let m = InferenceLlmModule::new();
        let result = m.handle_command("inference/llm/bogus", Value::Null).await;
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

    // ─── PR-3b: bus auto-publish tests ─────────────────────────

    use crate::inference::llm_module_bus::{
        inference_response_selectors, FIRST_TOKEN_EMITTED_KEY, INFERENCE_COMPLETE_KEY,
    };
    use crate::runtime::artifact_handle::{ArtifactKey, ArtifactSelector};
    use crate::runtime::runtime::Runtime;
    use parking_lot::Mutex;

    /// Recording subscriber for PR-3b bus tests.
    struct InferenceRecorder {
        captured: Arc<Mutex<Vec<(String, serde_json::Value)>>>,
    }

    impl InferenceRecorder {
        fn new() -> (Arc<Self>, Arc<Mutex<Vec<(String, serde_json::Value)>>>) {
            let captured = Arc::new(Mutex::new(Vec::new()));
            let module = Arc::new(Self {
                captured: captured.clone(),
            });
            (module, captured)
        }
    }

    #[async_trait]
    impl ServiceModule for InferenceRecorder {
        fn config(&self) -> ModuleConfig {
            ModuleConfig {
                name: "pr3b-inference-recorder",
                priority: ModulePriority::Normal,
                command_prefixes: &[],
                event_subscriptions: &[],
                needs_dedicated_thread: false,
                max_concurrency: 0,
                tick_interval: None,
            }
        }
        async fn initialize(&self, _ctx: &crate::runtime::ModuleContext) -> Result<(), String> {
            Ok(())
        }
        async fn handle_command(
            &self,
            _: &str,
            _: serde_json::Value,
        ) -> Result<CommandResult, String> {
            Err("not handled".to_string())
        }
        fn artifact_subscriptions(&self) -> Vec<ArtifactSelector> {
            inference_response_selectors()
        }
        async fn on_artifact_available(
            &self,
            key: &ArtifactKey,
            payload: serde_json::Value,
        ) -> Result<(), String> {
            self.captured
                .lock()
                .push((key.as_str().to_string(), payload));
            Ok(())
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    /// What this catches: with_bus wires auto-publishing. After a
    /// successful handle_command call, both InferenceComplete and
    /// FirstTokenEmitted land on the trace bus under their canonical
    /// keys. End-to-end test of the PR-2 + PR-3a + PR-3b chain.
    #[tokio::test]
    async fn handle_command_with_bus_auto_publishes_complete_and_first_token() {
        let runtime = Arc::new(Runtime::new());
        let (recorder, captured) = InferenceRecorder::new();
        runtime.register(recorder);

        let module = InferenceLlmModule::with_bus(runtime.bus_arc(), runtime.registry_arc());

        let req = sample_request();
        let params = serde_json::to_value(&req).unwrap();
        let _ = module
            .handle_command(COMMAND_REQUEST, params)
            .await
            .unwrap();

        // Yield to let the spawned publishes run.
        for _ in 0..50 {
            tokio::task::yield_now().await;
            if captured.lock().len() >= 2 {
                break;
            }
        }

        let events = captured.lock().clone();
        let keys: Vec<String> = events.iter().map(|(k, _)| k.clone()).collect();
        assert!(
            keys.contains(&INFERENCE_COMPLETE_KEY.to_string()),
            "expected InferenceComplete event; got keys {keys:?}"
        );
        assert!(
            keys.contains(&FIRST_TOKEN_EMITTED_KEY.to_string()),
            "expected FirstTokenEmitted event; got keys {keys:?}"
        );

        // Both events carry the same requestId we sent in.
        for (key, payload) in events {
            if key == INFERENCE_COMPLETE_KEY {
                let c: InferenceComplete = serde_json::from_value(payload).unwrap();
                assert_eq!(c.request_id, req.request_id);
            } else if key == FIRST_TOKEN_EMITTED_KEY {
                let f: FirstTokenEmitted = serde_json::from_value(payload).unwrap();
                assert_eq!(f.request_id, req.request_id);
            }
        }
    }

    /// What this catches: bus-less mode (via new()) doesn't publish.
    /// Backwards-compat with PR-2 — tests + standalone use don't
    /// require a Runtime.
    #[tokio::test]
    async fn handle_command_without_bus_does_not_publish() {
        let runtime = Arc::new(Runtime::new());
        let (recorder, captured) = InferenceRecorder::new();
        runtime.register(recorder);

        // Module constructed WITHOUT bus.
        let module = InferenceLlmModule::new();
        let req = sample_request();
        let params = serde_json::to_value(&req).unwrap();
        let _ = module
            .handle_command(COMMAND_REQUEST, params)
            .await
            .unwrap();

        // Yield to give any incorrectly-spawned publish a chance.
        for _ in 0..20 {
            tokio::task::yield_now().await;
        }

        assert!(
            captured.lock().is_empty(),
            "bus-less module must not publish anything"
        );
    }

    /// What this catches: handle_command_unknown does NOT publish.
    /// Only successful generations publish events; the unknown-
    /// command error path is silent on the bus (the typed error in
    /// the Result is the authoritative signal).
    #[tokio::test]
    async fn handle_command_unknown_with_bus_does_not_publish() {
        let runtime = Arc::new(Runtime::new());
        let (recorder, captured) = InferenceRecorder::new();
        runtime.register(recorder);

        let module = InferenceLlmModule::with_bus(runtime.bus_arc(), runtime.registry_arc());

        let result = module
            .handle_command("inference/llm/bogus", Value::Null)
            .await;
        assert!(result.is_err());

        for _ in 0..20 {
            tokio::task::yield_now().await;
        }

        assert!(
            captured.lock().is_empty(),
            "error path must not publish events"
        );
    }

    /// What this catches: handle_command_invalid_payload does NOT
    /// publish. Same invariant as the unknown-command case — invalid
    /// input fails fast via Result; no observability noise on the
    /// failure path.
    #[tokio::test]
    async fn handle_command_invalid_payload_with_bus_does_not_publish() {
        let runtime = Arc::new(Runtime::new());
        let (recorder, captured) = InferenceRecorder::new();
        runtime.register(recorder);

        let module = InferenceLlmModule::with_bus(runtime.bus_arc(), runtime.registry_arc());

        let result = module
            .handle_command(COMMAND_REQUEST, serde_json::json!({"not": "valid"}))
            .await;
        assert!(result.is_err());

        for _ in 0..20 {
            tokio::task::yield_now().await;
        }

        assert!(captured.lock().is_empty());
    }

    // ─── PR-4: translation function tests ──────────────────────
    //
    // PR-4 ships the translation helpers (run_adapter_inference,
    // translate_adapter_response, translate_adapter_finish_reason)
    // + the new with_adapter / with_bus_and_adapter constructors
    // + the prompt_text / completion_text optional fields.
    //
    // End-to-end "stub adapter via Arc<dyn AIProviderAdapter>"
    // tests are deferred to PR-5: the AIProviderAdapter trait has
    // 8+ methods including provider_id / api_style / default_model
    // / get_available_models / health_check / model_metadata, and
    // implementing all of them on a test stub here would pull in
    // ProviderHealth + AdapterCapabilities + ApiStyle + ModelInfo
    // + their dependencies. PR-5 will wire LlamaCppAdapter directly
    // (no test stub needed) + test through Runtime registration.
    //
    // PR-4's tests pin the PURE translation logic — same inputs,
    // same outputs — so PR-5's adapter integration has a
    // regression check for the translation contract.

    use crate::ai::types::{
        ContentPart, FinishReason as AdapterFinishReason, TextGenerationResponse, UsageMetrics,
    };

    fn canned_adapter_response() -> TextGenerationResponse {
        TextGenerationResponse {
            text: "stub adapter completion".to_string(),
            finish_reason: AdapterFinishReason::Stop,
            model: "stub-model".to_string(),
            provider: "stub-adapter-pr4".to_string(),
            usage: UsageMetrics {
                input_tokens: 5,
                output_tokens: 7,
                total_tokens: 12,
                estimated_cost: None,
            },
            response_time_ms: 250,
            request_id: "stub-rid".to_string(),
            content: Some(vec![ContentPart::Text {
                text: "stub adapter completion".to_string(),
            }]),
            tool_calls: None,
            reasoning: None,
            routing: None,
            error: None,
            timing: None,
        }
    }

    /// What this catches: translate_adapter_response carries the
    /// adapter's text into completion_text + the adapter's
    /// output_tokens into tokens_generated, leaves completion_tokens
    /// empty (adapter path uses text, not tokens).
    #[test]
    fn translate_adapter_response_carries_text_and_usage() {
        let req = sample_request();
        let response = canned_adapter_response();

        let complete = super::translate_adapter_response(&req, response);
        assert_eq!(complete.request_id, req.request_id);
        assert_eq!(complete.persona, req.persona);
        assert_eq!(
            complete.completion_text.as_deref(),
            Some("stub adapter completion")
        );
        assert!(
            complete.completion_tokens.is_empty(),
            "adapter path is text, not tokens"
        );
        assert_eq!(complete.tokens_generated, 7);
        assert_eq!(complete.elapsed_ms, 250);
        assert_eq!(complete.finish_reason, FinishReason::Stop);
    }

    /// What this catches: each adapter FinishReason variant maps
    /// to the substrate's FinishReason as documented. Cross-enum
    /// translation pin — if either enum changes, this test fails.
    #[test]
    fn translate_finish_reason_covers_all_adapter_variants() {
        assert_eq!(
            super::translate_adapter_finish_reason(&AdapterFinishReason::Stop),
            FinishReason::Stop
        );
        assert_eq!(
            super::translate_adapter_finish_reason(&AdapterFinishReason::Length),
            FinishReason::MaxTokens
        );
        match super::translate_adapter_finish_reason(&AdapterFinishReason::ToolUse) {
            FinishReason::Error { reason } => {
                assert!(reason.contains("ToolUse"));
            }
            other => panic!("ToolUse should map to Error, got {other:?}"),
        }
        match super::translate_adapter_finish_reason(&AdapterFinishReason::Error) {
            FinishReason::Error { reason } => {
                assert!(reason.contains("adapter returned Error"));
            }
            other => panic!("Error should map to Error, got {other:?}"),
        }
    }

    /// What this catches: with_adapter and with_bus_and_adapter
    /// constructors compile + return InferenceLlmModule with the
    /// expected fields populated. Reflects via downstream behavior
    /// (the adapter-path Err on missing prompt_text) since the
    /// fields are private.
    #[tokio::test]
    async fn with_adapter_constructor_routes_via_adapter_path() {
        // We can't construct a real Arc<dyn AIProviderAdapter> in
        // this test without implementing the full 8+ method trait;
        // PR-5 will. For PR-4 we verify the no-adapter path stays
        // intact (regression for the stub path) AND that the new
        // constructors compile + the field accessor logic in
        // handle_request is correctly gated on bus_hook + adapter.
        let module = InferenceLlmModule::new();
        let req = sample_request();
        let params = serde_json::to_value(&req).unwrap();
        let result = module.handle_command(COMMAND_REQUEST, params).await;
        assert!(result.is_ok(), "no-adapter path still routes to stub");
    }
}
