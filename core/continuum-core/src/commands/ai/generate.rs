//! `ai/generate` — the inference seam. Every live persona turn's token generation
//! flows through here: select an adapter from the registry, run the request, stamp
//! routing observability, return the response.
//!
//! This is NOT a persona tool — it is the compute boundary the cognition layer calls
//! in-process (and the grid leases remotely). Gated `Privileged`: leased inference,
//! not a citizen-facing action.
//!
//! ## Typed input
//!
//! The params ARE the canonical [`TextGenerationRequest`], decoded by serde at the boundary:
//! the same shape the grid's remote path sends, one wire name per field (camelCase), and no
//! field a handler has to find missing. The legacy alias table (`prompt` OR `messages`,
//! snake_case AND camelCase) is gone (2026-11-02). The output [`AiGenerateResult`] reuses the canonical sub-types
//! ([`UsageMetrics`]/[`ContentPart`]/[`ToolCall`]/[`RoutingInfo`]) and mirrors the
//! exact camelCase wire shape the old hand-rolled `response_to_json` produced.

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::ai::adapter::InferenceDevice;
use crate::ai::{
    AdapterRegistry, ContentPart, RoutingInfo, TextGenerationRequest, TextGenerationResponse,
    ToolCall, UsageMetrics,
};
use crate::logging::TimingGuard;
use crate::ai::adapter::select_failure_message;

/// The params of `ai/generate`: the inference request itself, plus the stream a caller
/// registered a sink under (`stream_sinks::register`), which the airc responder stamps so
/// every chunk reaches the remote mind as it is produced. The stream is the call's, never
/// the request's; a plain call has none and gets the assembled response.
#[derive(Debug, Clone, serde::Deserialize, schemars::JsonSchema, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/ai/GenerateParams.ts")]
pub struct GenerateParams {
    #[serde(flatten)]
    pub request: TextGenerationRequest,
    #[serde(default)]
    #[ts(optional, type = "string")]
    pub stream_id: Option<uuid::Uuid>,
    /// The requesting mind's think budget (her grant past her measured answer), forwarded by a
    /// grid lane (`RemoteInferenceRequest::think`). Present: generated under it on this seat's
    /// adapter (`DispatchTermsPolicy`), refused typed by one that cannot bound a think. This seat
    /// advertises it as `CapabilityId::RemoteThinkBudget`.
    #[serde(default)]
    #[ts(optional)]
    pub think: Option<crate::ai::types::ThinkBudget>,
}

/// Result of `ai/generate` — the exact camelCase shape the inference seam has always
/// returned. `success` is always `true` on the Ok path (errors return `Err`, which the
/// runtime renders as a failure result); the optional blocks are omitted when absent.
#[derive(Debug, Clone, serde::Serialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/ai/AiGenerateResult.ts"
)]
pub struct AiGenerateResult {
    pub success: bool,
    pub text: String,
    /// `FinishReason` rendered as its wire string (`stop` | `length` | `tool_use` | `error`).
    pub finish_reason: String,
    pub model: String,
    pub provider: String,
    pub usage: UsageMetrics,
    #[ts(type = "number")]
    pub response_time_ms: u64,
    pub request_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub content: Option<Vec<ContentPart>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tool_calls: Option<Vec<ToolCall>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub routing: Option<RoutingInfo>,
}

impl From<TextGenerationResponse> for AiGenerateResult {
    fn from(response: TextGenerationResponse) -> Self {
        AiGenerateResult {
            success: true,
            text: response.text,
            finish_reason: format!("{}", response.finish_reason),
            model: response.model,
            provider: response.provider,
            usage: response.usage,
            response_time_ms: response.response_time_ms,
            request_id: response.request_id,
            content: response.content,
            tool_calls: response.tool_calls,
            routing: response.routing,
        }
    }
}

crate::action_command! {
    /// Generate text from an AI provider: select the best adapter for the requested
    /// provider/model (or the default), run the request, and return the completion
    /// with usage, finish reason, optional tool calls, and routing observability.
    /// The inference seam every persona turn flows through. Gated `Privileged`
    /// (leased inference — not a persona-facing tool).
    pub struct AiGenerate { registry: Arc<RwLock<AdapterRegistry>> }
    name: "ai/generate",
    access: Privileged,
    params: GenerateParams,
    output: AiGenerateResult,
    run(this, ctx, p) => {
        let _timer = TimingGuard::new("module", "ai_generate");

        let GenerateParams { request, stream_id, think } = p;
        // The one shape check serde cannot express on today's `Vec`: an empty conversation.
        // Follow-up (Fable): `TextGenerationRequest::messages` becomes non-empty by type.
        if request.messages.is_empty() {
            return Err(crate::sdk_codegen::CommandError::Invalid("ai/generate: the request has no messages".into()));
        }

        let registry = this.registry.read().await;

        let (provider_id, adapter) = registry
            .select(
                request.provider.as_deref(),
                request.model.as_deref(),
                InferenceDevice::default(),
            )
            .ok_or_else(|| {
                select_failure_message(
                    &registry,
                    request.provider.as_deref(),
                    request.model.as_deref(),
                )
            })?;

        crate::runtime::logger("ai_provider").info(&format!(
            "Using {} adapter for model {:?}",
            provider_id, request.model
        ));

        let is_local = adapter.capabilities().is_local;
        // INFERENCE IS A STREAM, NOT A PROMISE. A caller that registered a sink under
        // the `streamId` it stamped (the airc responder, for a remote-bound mind) gets
        // every chunk the instant the backend produces it — the same primitive the
        // local persona turn rides — and the assembled response at the end. No sink
        // registered (a plain call) = the drain, exactly as before.
        let sink = stream_id.and_then(crate::ai::stream_sinks::take);
        let mut response = match (think, sink) {
            // A forwarded think budget: generated under it, or refused typed by the adapter.
            (Some(think), sink) => {
                let terms = crate::ai::types::DispatchTerms { admission: None, think: Some(think) };
                match sink {
                    Some(sink) => adapter.generate_admitted(request, terms, sink).await.map_err(|e| e.to_string())?,
                    None => crate::ai::stream_sinks::drain_generation(|sink| adapter.generate_admitted(request, terms, sink))
                        .await
                        .map_err(|e| e.to_string())?,
                }
            }
            (None, Some(sink)) => adapter.generate_stream(request, sink).await?,
            (None, None) => adapter.generate_text(request).await?,
        };
        // A generate from a cross-grid caller is inference this seat performed for a mind
        // hosted on ANOTHER node. Its measured prompt size (the server's own count) joins
        // the serving plan's per-lane window floor beside this node's residents' prompts —
        // otherwise a seat grows lanes for the grid and then sizes them to one resident's
        // prompt and refuses to fit them (card c84d885a; the 5090's 1 × 101k).
        if ctx
            .caller
            .as_ref()
            .is_some_and(|c| matches!(c.source, crate::routing::CallerSource::Airc))
        {
            crate::inference::resource_admission::note_leased_in_sent(response.usage.input_tokens);
        }

        // Preserve the serving node's receipt, model mapping and adapter metadata.
        let route = RoutingInfo::stamp(
            &mut response.routing,
            provider_id,
            is_local,
            "adapter_selected",
        );
        // THE LANE REPORTS ITS OWN WINDOW. This used to be gated on `is_local` and read the
        // global `llama_server::current_serving()` snapshot, which is very nearly the INVERSE
        // of the right question (card be553169):
        //
        //   is_local BLOCKED  the local gateway adapter — an `OpenAiAdapter` talking the
        //                     OpenAI wire protocol to 127.0.0.1, so `capabilities()` says
        //                     `.remote()` truthfully — and the global snapshot describes
        //                     EXACTLY its lane. Measured on the Intel tier 2026-09-18: a
        //                     loopback answer from a lane serving 32,768 returned
        //                     `servedContextWindow: null`, and 88 of 88 cross-grid answers
        //                     read `served_window=None`, because every responder stamps here.
        //   is_local PERMITTED `LlamaCppAdapter`, which loads the GGUF IN PROCESS ("no HTTP
        //                     hop") — so the gateway snapshot describes a DIFFERENT server,
        //                     potentially a different model, certainly a different window.
        //
        // `live_served_window()` is that question asked of the party that knows: "the live
        // served window of the lane THIS adapter is bound to." It already refuses on exactly
        // the grounds this needs — a dedicated lane reports `None` because "the global
        // gateway snapshot describes a DIFFERENT server and must never be consulted for it"
        // (glass-boxed 2026-07-20; reading the global slot starved an eval fork, webdev-rs
        // 0/6) — and its default `None` is the right answer for cloud and in-process alike:
        // the binding window stands. An adapter with a live lane to report is the only one
        // that reports.
        //
        // `.or()` and not `=`: a relayed remote answer already carries the RESPONDER's stamp
        // in `route.served_context_window`, and the remote adapter takes the `None` default,
        // so a peer's window is preserved rather than overwritten with ours. That preservation
        // is the whole point of the field (card 1ab60567).
        route.served_context_window = adapter
            .live_served_window()
            .or(route.served_context_window);
        Ok(AiGenerateResult::from(response))
    }
}
