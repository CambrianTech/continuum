//! InferenceHandleModule — ServiceModule wrapper exposing the
//! handle store as `ai/inference/open` / `ai/inference/generate` /
//! `ai/inference/close` / `ai/inference/inspect` commands.
//!
//! Joel (2026-05-31): "Yeah the inference command doesn't do this.
//! It's smart subsystems and daemons. Commands are dumb and short."
//!
//! ### What this module is
//!
//! The dumb-command layer. Routes open/generate/close to
//! `InferenceHandleStore` with minimal logic — parse envelope,
//! validate, call store, materialize response. NO scheduling, NO
//! batching, NO LoRA paging policy, NO base-model-sharing decisions.
//! Those live in the smart subsystems + daemons that sit BEHIND
//! this command surface ([[inference-scarcity-economics]] task #109).
//!
//! ### Adapter resolution
//!
//! The `open` handler resolves the requested provider via the
//! shared `AdapterRegistry` (the same registry that `inference/llm/
//! request` uses). The handle store doesn't touch the registry —
//! the module is the bridge between "I want provider X" (the
//! caller's view) and "I have an Arc<dyn AIProviderAdapter>" (the
//! store's contract).
//!
//! ### Doctrine alignment
//!
//! - [[commands-are-kernel-level-and-compose]] — this module ONLY
//!   handles command envelopes + routing; never reaches into
//!   adapter or store internals
//! - [[inference-is-an-adapter-always-in-the-loop]] — these are the
//!   canonical handle-shape commands callers + tests will route
//!   through; one-shot `inference/llm/request` remains the legacy
//!   path for migration
//! - [[inference-scarcity-economics]] §"commands are dumb, daemons
//!   are smart" — this module is the dumb interface; smart bits
//!   land later as separate components without changing this
//!   command surface
//! - [[rust-is-the-core-node-is-the-shell]] — pure Rust ServiceModule

use std::sync::Arc;

use async_trait::async_trait;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use ts_rs::TS;
use uuid::Uuid;

use crate::ai::adapter::AIProviderAdapter;
use crate::ai::types::{ActiveAdapterRequest, TextGenerationRequest, TextGenerationResponse};
use crate::inference::handle_store::{
    InferenceHandleStore, OpenSessionRequest, HANDLE_OWNER, HANDLE_TYPE_TAG,
};
use crate::runtime::cell_shapes::HandleRef;
use crate::runtime::{
    CommandRequest, CommandResponse, CommandResult, ModuleConfig, ModulePriority, ServiceModule,
};

// ── Command name constants ─────────────────────────────────────────

pub const COMMAND_OPEN: &str = "ai/inference/open";
pub const COMMAND_GENERATE: &str = "ai/inference/generate";
pub const COMMAND_CLOSE: &str = "ai/inference/close";
pub const COMMAND_INSPECT: &str = "ai/inference/inspect";

// ── Typed params ───────────────────────────────────────────────────

/// Params for `ai/inference/open`.
///
/// The caller specifies the provider by name; the module resolves
/// via the AdapterRegistry. Sticky session inputs (system_prompt,
/// model override, active LoRA adapters, persona scope) all flow
/// through here and live on the session for the handle's lifetime.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/ai_inference/OpenParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct OpenParams {
    /// Provider ID from the AdapterRegistry (e.g. "anthropic",
    /// "heuristic", "llamacpp"). Required.
    pub provider: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub system_prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub active_adapters: Option<Vec<ActiveAdapterRequest>>,
    /// Persona scope. When set, every subsequent generate against
    /// this handle MUST carry a matching persona_id. Defense in
    /// depth at the inference layer.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub persona_id: Option<Uuid>,
}

/// Result of `ai/inference/open`. The minted handle is carried by
/// the CommandResponse envelope's top-level `handle` field; these
/// payload fields hold only the open-call's report.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/ai_inference/OpenResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct OpenResult {
    /// Echo of the resolved provider, so callers can confirm the
    /// adapter the module routed to (especially useful when the
    /// caller's open params lean on defaults).
    pub provider: String,
}

/// Params for `ai/inference/generate`.
///
/// The handle is carried by the CommandRequest envelope's top-level
/// `handle` field (per substrate convention) — these params hold
/// only the per-call generation request. The session's defaults
/// (system_prompt, model, active_adapters) fill in any unset fields
/// on `request` at generate time.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/ai_inference/GenerateParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct GenerateParams {
    pub request: TextGenerationRequest,
}

/// Result of `ai/inference/generate`.
pub type GenerateResult = TextGenerationResponse;

/// Params for `ai/inference/close`. Handle is in the envelope.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/ai_inference/CloseParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct CloseParams {}

/// Result of `ai/inference/close`.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/ai_inference/CloseResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct CloseResult {
    /// True if the handle was open at close time. False = already
    /// closed or evicted; callers can treat this as idempotent.
    pub released: bool,
}

/// Params for `ai/inference/inspect`. Handle is in the envelope.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/ai_inference/InspectParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct InspectParams {}

/// Result of `ai/inference/inspect`. The observability snapshot
/// per [[observability-is-half-the-architecture]].
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/ai_inference/InspectResult.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct InspectResult {
    pub provider_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "string")]
    pub persona_id: Option<Uuid>,
    #[ts(type = "number")]
    pub created_at_ms: u64,
    #[ts(type = "number")]
    pub last_used_ms: u64,
    #[ts(type = "number")]
    pub generation_count: u64,
    pub has_system_prompt: bool,
    #[ts(type = "number")]
    pub active_adapter_count: u32,
}

// ── Module ─────────────────────────────────────────────────────────

/// The ServiceModule. Holds Arc<InferenceHandleStore> + a local
/// adapter map (provider_id → Arc<dyn AIProviderAdapter>) populated
/// at wiring time.
///
/// **Why a local adapter map instead of reading from AdapterRegistry**:
/// AdapterRegistry stores `Box<dyn AIProviderAdapter>` and uses
/// `&mut self` methods (initialize / shutdown) — it's the ownership
/// authority for adapter lifecycle. The handle store needs
/// `Arc<dyn AIProviderAdapter>` so multiple handles can share a
/// session-state-free reference. Wiring decides: stateless adapters
/// (HeuristicAdapter) construct fresh instances for the module;
/// stateful adapters (LlamaCppAdapter) share an Arc between
/// registry + module so the model bytes load once.
///
/// A future refactor (after task #109 lands) can fold this into a
/// unified Arc-based registry; for now keeping the two surfaces
/// independent makes slice B small and reviewable.
pub struct InferenceHandleModule {
    store: Arc<InferenceHandleStore>,
    providers: Arc<DashMap<String, Arc<dyn AIProviderAdapter>>>,
}

impl InferenceHandleModule {
    pub fn new(store: Arc<InferenceHandleStore>) -> Self {
        Self {
            store,
            providers: Arc::new(DashMap::new()),
        }
    }

    /// Register an adapter under a provider_id. Called at wiring
    /// time before the module is exposed to commands. Returns the
    /// previous adapter if one was registered for this provider_id
    /// (so callers can decide whether to log a swap).
    pub fn register_adapter(
        &self,
        provider_id: impl Into<String>,
        adapter: Arc<dyn AIProviderAdapter>,
    ) -> Option<Arc<dyn AIProviderAdapter>> {
        self.providers.insert(provider_id.into(), adapter)
    }

    pub fn store(&self) -> Arc<InferenceHandleStore> {
        self.store.clone()
    }
}

#[async_trait]
impl ServiceModule for InferenceHandleModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "ai-inference-handle",
            priority: ModulePriority::High,
            command_prefixes: &["ai/inference/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            // The handle store uses DashMap + per-session atomics;
            // it's safe under arbitrary concurrency. The scheduler
            // (task #109) will introduce slot caps; until then, 0
            // = unlimited (module manages own concurrency).
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(
        &self,
        _ctx: &crate::runtime::ModuleContext,
    ) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(
        &self,
        command: &str,
        params: Value,
    ) -> Result<CommandResult, String> {
        match command {
            COMMAND_OPEN => {
                let req = CommandRequest::<OpenParams>::from_value(params)
                    .map_err(|e| format!("{COMMAND_OPEN}: invalid params: {e}"))?;
                let (handle, payload) = self.open(req.params).await?;
                CommandResponse::ok(payload)
                    .with_handle_ref(handle)
                    .into_command_result()
            }
            COMMAND_GENERATE => {
                let req = CommandRequest::<GenerateParams>::from_value(params)
                    .map_err(|e| format!("{COMMAND_GENERATE}: invalid params: {e}"))?;
                let handle = req.handle.ok_or_else(|| {
                    format!("{COMMAND_GENERATE}: missing required `handle` field on envelope")
                })?;
                let result = self.generate(handle, req.params.request).await?;
                CommandResponse::ok(result).into_command_result()
            }
            COMMAND_CLOSE => {
                let req = CommandRequest::<CloseParams>::from_value(params)
                    .map_err(|e| format!("{COMMAND_CLOSE}: invalid params: {e}"))?;
                let handle = req.handle.ok_or_else(|| {
                    format!("{COMMAND_CLOSE}: missing required `handle` field on envelope")
                })?;
                let result = self.close(handle).await?;
                CommandResponse::ok(result).into_command_result()
            }
            COMMAND_INSPECT => {
                let req = CommandRequest::<InspectParams>::from_value(params)
                    .map_err(|e| format!("{COMMAND_INSPECT}: invalid params: {e}"))?;
                let handle = req.handle.ok_or_else(|| {
                    format!("{COMMAND_INSPECT}: missing required `handle` field on envelope")
                })?;
                let result = self.inspect(handle).await?;
                CommandResponse::ok(result).into_command_result()
            }
            other => Err(format!(
                "ai-inference-handle: unknown command '{other}' \
                 (known: {COMMAND_OPEN}, {COMMAND_GENERATE}, {COMMAND_CLOSE}, {COMMAND_INSPECT})"
            )),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl InferenceHandleModule {
    async fn open(&self, params: OpenParams) -> Result<(HandleRef, OpenResult), String> {
        let adapter = self
            .providers
            .get(&params.provider)
            .map(|entry| entry.value().clone())
            .ok_or_else(|| {
                let available: Vec<String> =
                    self.providers.iter().map(|e| e.key().clone()).collect();
                format!(
                    "{COMMAND_OPEN}: provider '{}' not registered (available: {:?})",
                    params.provider, available
                )
            })?;

        let handle = self.store.open(
            adapter,
            OpenSessionRequest {
                model: params.model,
                system_prompt: params.system_prompt,
                active_adapters: params.active_adapters,
                persona_id: params.persona_id,
            },
        );
        Ok((
            handle,
            OpenResult {
                provider: params.provider,
            },
        ))
    }

    async fn generate(
        &self,
        handle: HandleRef,
        request: TextGenerationRequest,
    ) -> Result<GenerateResult, String> {
        self.store
            .generate(&handle, request)
            .await
            .map_err(|e| format!("{COMMAND_GENERATE}: {e}"))
    }

    async fn close(&self, handle: HandleRef) -> Result<CloseResult, String> {
        let released = self
            .store
            .close(&handle)
            .map_err(|e| format!("{COMMAND_CLOSE}: {e}"))?;
        Ok(CloseResult { released })
    }

    async fn inspect(&self, handle: HandleRef) -> Result<InspectResult, String> {
        let snapshot = self
            .store
            .inspect(&handle)
            .map_err(|e| format!("{COMMAND_INSPECT}: {e}"))?;
        Ok(InspectResult {
            provider_id: snapshot.provider_id,
            model: snapshot.model,
            persona_id: snapshot.persona_id,
            created_at_ms: snapshot.created_at_ms,
            last_used_ms: snapshot.last_used_ms,
            generation_count: snapshot.generation_count,
            has_system_prompt: snapshot.has_system_prompt,
            active_adapter_count: snapshot.active_adapter_count as u32,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai::heuristic_adapter::{HeuristicInferenceAdapter, HEURISTIC_PROVIDER_ID};
    use crate::ai::types::{ChatMessage, MessageContent};

    fn user_msg(text: &str) -> ChatMessage {
        ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text(text.to_string()),
            name: None,
        }
    }

    fn empty_request() -> TextGenerationRequest {
        TextGenerationRequest {
            messages: vec![user_msg("test prompt")],
            system_prompt: None,
            model: None,
            provider: None,
            temperature: None,
            max_tokens: None,
            top_p: None,
            top_k: None,
            repeat_penalty: None,
            stop_sequences: None,
            tools: None,
            tool_choice: None,
            response_format: None,
            active_adapters: None,
            request_id: None,
            user_id: None,
            room_id: None,
            purpose: None,
            persona_id: None,
        }
    }

    fn module_with_heuristic() -> InferenceHandleModule {
        let store = Arc::new(InferenceHandleStore::new());
        let module = InferenceHandleModule::new(store);
        module.register_adapter(
            HEURISTIC_PROVIDER_ID,
            Arc::new(HeuristicInferenceAdapter::new()) as Arc<dyn AIProviderAdapter>,
        );
        module
    }

    // ── command surface ────────────────────────────────────────────

    #[test]
    fn config_reports_canonical_module_name_and_prefix() {
        let m = module_with_heuristic();
        let cfg = m.config();
        assert_eq!(cfg.name, "ai-inference-handle");
        assert_eq!(cfg.command_prefixes, &["ai/inference/"]);
        assert!(!cfg.needs_dedicated_thread);
    }

    #[tokio::test]
    async fn open_through_command_returns_handleref_with_canonical_tags() {
        let m = module_with_heuristic();
        let envelope = serde_json::to_value(CommandRequest::new(OpenParams {
            provider: HEURISTIC_PROVIDER_ID.to_string(),
            ..Default::default()
        }))
        .unwrap();
        let result = m.handle_command(COMMAND_OPEN, envelope).await.unwrap();
        let json = match result {
            CommandResult::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        };
        // CommandResponse flattens — `data` is NOT a nested object;
        // the OpenResult fields + envelope handle live at the top
        // level alongside `success`.
        let response = json.as_object().unwrap();
        assert_eq!(response.get("success").unwrap(), &Value::Bool(true));
        let handle = response.get("handle").unwrap().as_object().unwrap();
        assert_eq!(handle.get("owner").unwrap(), HANDLE_OWNER);
        assert_eq!(handle.get("type_tag").unwrap(), HANDLE_TYPE_TAG);
        assert_eq!(response.get("provider").unwrap(), HEURISTIC_PROVIDER_ID);
    }

    #[tokio::test]
    async fn open_with_unregistered_provider_returns_typed_error() {
        let m = module_with_heuristic();
        let envelope = serde_json::to_value(CommandRequest::new(OpenParams {
            provider: "no-such-provider".to_string(),
            ..Default::default()
        }))
        .unwrap();
        let result = m.handle_command(COMMAND_OPEN, envelope).await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(
            err.contains("not registered"),
            "expected adapter-not-registered error, got: {err}"
        );
    }

    #[tokio::test]
    async fn generate_through_command_routes_to_adapter() {
        let m = module_with_heuristic();
        let (opened_handle, _opened) = m
            .open(OpenParams {
                provider: HEURISTIC_PROVIDER_ID.to_string(),
                ..Default::default()
            })
            .await
            .unwrap();
        let envelope = serde_json::to_value(
            CommandRequest::new(GenerateParams {
                request: empty_request(),
            })
            .with_handle(opened_handle.clone()),
        )
        .unwrap();
        let result = m.handle_command(COMMAND_GENERATE, envelope).await.unwrap();
        let json = match result {
            CommandResult::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        };
        let response = json.as_object().unwrap();
        assert_eq!(response.get("success").unwrap(), &Value::Bool(true));
        // GenerateResult (TextGenerationResponse) fields flatten —
        // `text` lives at the top level, not under `data`.
        let text = response.get("text").unwrap().as_str().unwrap();
        assert!(
            text.starts_with("[heuristic:"),
            "expected heuristic adapter output, got: {text}"
        );
    }

    #[tokio::test]
    async fn close_through_command_releases_session() {
        let m = module_with_heuristic();
        let (opened_handle, _opened) = m
            .open(OpenParams {
                provider: HEURISTIC_PROVIDER_ID.to_string(),
                ..Default::default()
            })
            .await
            .unwrap();
        let envelope = serde_json::to_value(
            CommandRequest::new(CloseParams::default()).with_handle(opened_handle.clone()),
        )
        .unwrap();
        let result = m.handle_command(COMMAND_CLOSE, envelope).await.unwrap();
        let json = match result {
            CommandResult::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        };
        // CloseResult.released is flattened at the top level.
        assert_eq!(json.get("released").unwrap(), &Value::Bool(true));

        // Generate now fails — handle is closed.
        let envelope = serde_json::to_value(
            CommandRequest::new(GenerateParams {
                request: empty_request(),
            })
            .with_handle(opened_handle),
        )
        .unwrap();
        let result = m.handle_command(COMMAND_GENERATE, envelope).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("not found"));
    }

    #[tokio::test]
    async fn inspect_through_command_returns_session_snapshot() {
        let m = module_with_heuristic();
        let (opened_handle, _opened) = m
            .open(OpenParams {
                provider: HEURISTIC_PROVIDER_ID.to_string(),
                system_prompt: Some("inspect me".to_string()),
                ..Default::default()
            })
            .await
            .unwrap();
        let envelope = serde_json::to_value(
            CommandRequest::new(InspectParams::default()).with_handle(opened_handle),
        )
        .unwrap();
        let result = m.handle_command(COMMAND_INSPECT, envelope).await.unwrap();
        let json = match result {
            CommandResult::Json(v) => v,
            other => panic!("expected Json, got {other:?}"),
        };
        // InspectResult fields flatten at the top level.
        assert_eq!(json.get("providerId").unwrap(), HEURISTIC_PROVIDER_ID);
        assert_eq!(json.get("hasSystemPrompt").unwrap(), &Value::Bool(true));
        assert_eq!(json.get("generationCount").unwrap().as_u64().unwrap(), 0);
    }

    #[tokio::test]
    async fn unknown_command_returns_loud_error() {
        let m = module_with_heuristic();
        let result = m
            .handle_command("ai/inference/something-bogus", Value::Null)
            .await;
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.contains("unknown command"));
        assert!(err.contains(COMMAND_OPEN));
    }

    #[tokio::test]
    async fn full_open_generate_close_round_trip_through_command_surface() {
        let m = module_with_heuristic();
        let (opened_handle, _opened) = m
            .open(OpenParams {
                provider: HEURISTIC_PROVIDER_ID.to_string(),
                ..Default::default()
            })
            .await
            .unwrap();
        let handle = opened_handle.clone();
        // Generate twice — same handle, two responses (increments
        // generation_count to 2).
        let r1 = m
            .generate(handle.clone(), empty_request())
            .await
            .unwrap();
        let r2 = m
            .generate(handle.clone(), empty_request())
            .await
            .unwrap();
        // Same prompt → same response (determinism contract).
        assert_eq!(r1.text, r2.text);
        // Inspect sees 2 generations.
        let snap = m.inspect(handle.clone()).await.unwrap();
        assert_eq!(snap.generation_count, 2);
        // Close releases.
        let closed = m.close(handle).await.unwrap();
        assert!(closed.released);
    }

    #[tokio::test]
    async fn generate_without_envelope_handle_returns_loud_error() {
        let m = module_with_heuristic();
        let envelope = serde_json::to_value(CommandRequest::new(GenerateParams {
            request: empty_request(),
        }))
        .unwrap();
        let result = m.handle_command(COMMAND_GENERATE, envelope).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("missing required `handle`"));
    }
}
