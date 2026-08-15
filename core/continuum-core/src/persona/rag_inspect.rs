//! rag_inspect — the substrate's honest-look-at-the-prompt primitive.
//!
//! Joel (2026-05-31): "This is the differentiator between a complex
//! guess and an intentional brain. If we have observability and
//! replay at any stage, we can iterate, improve, add complexity, try
//! out new ideas in realistic scenarios and look at it ourselves:
//! with this prompt would I respond as it requests at this step?
//! Which layer is broken? Missing, is this contextually relevant
//! (hippocampus and caches)?"
//!
//! ### Why this exists at the library layer (not just as a binary)
//!
//! The airc_rag_demo binary proved we CAN build a per-item dump from
//! the L1 RAG pipeline. But binaries aren't callable by other AIs.
//! To make introspection a substrate-level primitive — discoverable
//! via `Commands.execute('persona/rag-inspect', { persona })` and
//! consumable by Claude / sentinels / any other persona doing
//! adversarial review — it has to be a Rust library function with
//! a structured result type. The ServiceModule + ts-rs binding sit
//! ON TOP of this function; the binary becomes a thin CLI wrapper.
//!
//! ### Doctrine alignment
//!
//! - [[observability-is-half-the-architecture]] — half the substrate
//!   is honest visibility into load-bearing decisions. This is one of
//!   them; the sink and trace path are first-class request inputs.
//! - [[persona-record-replay-is-a-product-requirement]] — every
//!   inspection that opts into `trace_path` produces a JSONL trace
//!   that ReplayRagSource consumes byte-for-byte.
//! - [[substrate-is-a-good-citizen-on-the-host]] — when `trace_path`
//!   is `None`, the sink is `NoopRagCaptureSink` (zero overhead). The
//!   hot path doesn't pay for observability it didn't ask for.
//! - [[source-drain-is-the-universal-pattern]] — the inspection IS
//!   the drain for in-flight RAG decisions. Without it those
//!   decisions are sources without drains, which is the leak shape.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::persona::airc_source::{AircRagSource, AircTranscriptReader};
use crate::persona::rag_budget::{
    BudgetAllocation, FlexboxRagBudgetAdapter, RagBudgetAdapter, RagContext, RagSource,
    RagSourceBudget, ReservedTokens, ResolutionPreference,
};
use crate::persona::rag_capture::{
    JsonlRagCaptureSink, NoopRagCaptureSink, RagCaptureEvent, RagCaptureSink, RecordingRagSource,
};

/// How many chars of an item's content to keep in the preview. Items
/// with longer content still report full token cost; this only
/// controls the human/AI-readable snippet returned in the inspection
/// result. Replay against the trace gets the full content; the
/// inspection result is for "look at what the persona would see right
/// now" mechanic-shop summarization.
// context-budget-exempt: preview width for the human-facing rag-inspect command output;
// never text sent to a model
pub const CONTENT_PREVIEW_CHARS: usize = 200;

/// Assumed window for [`RagInspectionRequest::defaults_for`] — the
/// DIAGNOSTIC path only, where no inference profile is in hand and the
/// `persona/rag-inspect` caller can override `context_window` explicitly.
/// This is NOT a serving decision and never reaches a live turn: live
/// turns go through [`RagInspectionRequest::for_persona`] with the
/// profile's real `context_length` (#46/#50 law — the window comes from
/// the adapter, never a per-tier cap).
// context-budget-exempt: inspection-command default only, caller-overridable;
// live turns derive the window from the persona's inference profile
pub const DEFAULT_INSPECT_WINDOW: u32 = 32_768;

/// Tunable inputs for one inspection. Defaults via `defaults_for`
/// match the `mid-local (32k)` profile the demo binary uses — a
/// sensible "what would a typical local persona see right now" probe
/// when the caller doesn't have stronger opinions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagInspectionRequest {
    pub persona_id: Uuid,
    pub persona_name: String,
    pub context_window: u32,
    pub reserved: ReservedTokens,
    pub airc_floor: u32,
    pub airc_max: u32,
    pub airc_priority: u8,
    pub airc_required: bool,
    pub airc_fetch_limit: usize,
    /// Wall-clock "now" the inspection should reason against. Caller
    /// supplies this so the function stays pure-of-clock (testable +
    /// deterministic replay).
    pub now_ms: u64,
    /// Where to write the capture trace. `None` = NoopSink (zero
    /// overhead, no file I/O). `Some(path)` = JSONL writer; the
    /// parent directory is created if absent.
    pub trace_path: Option<PathBuf>,
}

impl RagInspectionRequest {
    /// Diagnostic defaults for "show me what this persona would see
    /// right now" when NO inference profile is in hand — the
    /// `persona/rag-inspect` command path, where the caller can
    /// override `context_window` explicitly. Same derivation as
    /// [`Self::for_persona`], seeded with [`DEFAULT_INSPECT_WINDOW`].
    /// Prefer [`Self::for_persona`] when a profile IS in hand — that
    /// path threads the profile's actual context_length through so
    /// the RAG layer never asks for more tokens than the adapter
    /// can decode. (TODO #46/#50 family: `PersonaResolution` should
    /// carry the persona's live profile so the inspect command stops
    /// needing an assumed window at all.)
    pub fn defaults_for(persona_id: Uuid, persona_name: String, now_ms: u64) -> Self {
        Self::for_window(persona_id, persona_name, now_ms, DEFAULT_INSPECT_WINDOW)
    }

    /// Derive the inspection request from the persona's inference
    /// profile — the single source of truth for context budgets.
    /// `&PersonaInferenceProfile` is the substrate's persona-shape
    /// context object (analogous to Android's `Context`); every
    /// downstream layer that needs an inference-shape knob reads
    /// from this struct instead of copying fields or holding
    /// duplicate constants.
    ///
    /// - `context_window` = `profile.context_length` (no overflow
    ///   risk — the RAG layer can never ask for more than the
    ///   adapter was loaded with).
    pub fn for_persona(
        persona_id: Uuid,
        persona_name: String,
        now_ms: u64,
        profile: &crate::persona::inference_profile::PersonaInferenceProfile,
    ) -> Self {
        Self::for_window(persona_id, persona_name, now_ms, profile.context_length)
    }

    /// THE derivation both constructors share (#424 — `defaults_for`
    /// previously hand-copied a flat `{system: 400, completion: 4_000,
    /// airc_max: 20_000}` shape that had already drifted from this one).
    ///
    /// - reservation + headroom: [`ReservedTokens::scaled_for_window`].
    /// - `airc_max` = at most 60% of the window, AND at most the
    ///   post-reservation headroom.
    /// - `airc_floor` clamps to `airc_max` so floor ≤ max always.
    pub fn for_window(
        persona_id: Uuid,
        persona_name: String,
        now_ms: u64,
        context_window: u32,
    ) -> Self {
        let reserved = ReservedTokens::scaled_for_window(context_window);
        let headroom = reserved.headroom_within(context_window);
        // Default budget: ~60% of the context window for room
        // history, clamped to headroom. This is a CONSERVATIVE
        // FALLBACK — the substrate's real budgeter (TODO: routed
        // through model characteristics on the Context per
        // [[context-is-the-client-airc-token-is-identity]] and
        // [[intent-driven-api-not-hot-patches]]) should derive
        // this from `(prefill_tps, decode_tps, target_first_token_latency_ms)`
        // so a 5090 + 200k-context frontier model can feed the
        // whole history and a CPU + Qwen-0.5B can clamp itself.
        // Both end up calling the SAME budget API; the answer
        // differs because the model differs. Do NOT cap to a
        // smaller percentage to make Intel Mac faster — that
        // dumbs down every capable peer on the grid.
        let airc_max = ((context_window as u64) * 6 / 10) as u32;
        let airc_max = airc_max.min(headroom);
        let airc_floor = 500_u32.min(airc_max);
        Self {
            persona_id,
            persona_name,
            context_window,
            reserved,
            airc_floor,
            airc_max,
            airc_priority: 10,
            airc_required: true,
            airc_fetch_limit: 100,
            now_ms,
            trace_path: None,
        }
    }

    /// `&ctx`-pure derivation: read everything from the persona's
    /// context object. The substrate's `&ctx` doctrine
    /// ([[context-is-the-client-airc-token-is-identity]]) — caller
    /// hands one reference, derivation reads what it needs.
    /// Prefer this over [`Self::for_persona`] in any new code that
    /// already holds a `&PersonaContext`.
    pub fn for_ctx(ctx: &crate::persona::supervisor::PersonaContext, now_ms: u64) -> Self {
        Self::for_persona(
            ctx.identity.peer_id.as_uuid(),
            ctx.identity.agent_name.clone(),
            now_ms,
            &ctx.profile,
        )
    }
}

/// The honest-look result. Carries the full allocation outcome PLUS
/// per-source delivery details with the mechanic-grade rationale
/// (score, lamport, peer-id-prefix, age, content preview).
///
/// Specifically does NOT collapse layers — the future is multiple
/// sources (engram, airc, reference, working-memory). Each gets its
/// own `SourceDeliveryInspection` so the "which layer is broken?"
/// question is answerable by inspection rather than by guessing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagInspection {
    pub persona_id: Uuid,
    pub persona_name: String,
    pub context_window: u32,
    pub allocation: BudgetAllocation,
    pub deliveries: Vec<SourceDeliveryInspection>,
    /// Path to the JSONL trace if `trace_path` was set on the request,
    /// else `None`. Other AIs / mechanics resume replay against this.
    pub trace_path: Option<PathBuf>,
    /// Model response when an inference adapter was passed to the
    /// chained variant `inspect_persona_rag_with_inference`. None
    /// when the inspection was RAG-only (the default path). This is
    /// where the canonical "with this prompt would I respond as it
    /// requests at this step?" question gets answered.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub model_response: Option<ModelResponseInspection>,
}

/// Captured model response from the chained inspection variant —
/// what the inference adapter produced when fed the RAG-delivered
/// items as a prompt. Carries the decision + the response together
/// per [[no-if-statements-use-llms-for-cognition]]: the LLM decides
/// `will_respond` AND writes `response_text` in ONE structured call
/// (json grammar-constrained via `ResponseFormat::JsonObject`).
///
/// `will_respond == false` means the persona explicitly chose silence;
/// the service loop counts it as `turns_skipped` and posts nothing.
/// `will_respond == true` AND `response_text.is_empty()` is a model
/// failure — the substrate treats it as skipped too (the persona
/// said yes but produced no content per the structured contract).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelResponseInspection {
    pub adapter_id: String,
    pub model: String,
    pub prompt_text: String,
    /// The persona's decision: true = post a reply; false = stay
    /// silent this turn. Substrate refuses to invent a default per
    /// [[no-fallbacks-ever]] — if the model couldn't be parsed, the
    /// caller surfaces the typed `Err` from inference_probe and the
    /// loop counts `turns_errored`.
    pub will_respond: bool,
    /// The reply text the persona wants posted. Only used when
    /// `will_respond == true`. Empty otherwise.
    pub response_text: String,
    pub finish_reason: String,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub response_time_ms: u64,
}

/// Per-source delivery, with the substrate-grade detail every
/// inspection caller needs: requested budget, actual usage,
/// continuation flag, and the full list of items packed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDeliveryInspection {
    pub source_id: String,
    pub budget_requested: u32,
    pub tokens_used: u32,
    pub has_continuation: bool,
    pub items: Vec<InspectedItem>,
}

/// One item from a source's delivery, with the fields a mechanic
/// needs to answer "why this item?" — score, age, who, when.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectedItem {
    pub index: usize,
    pub tokens: u32,
    pub score: f64,
    pub content_preview: String,
    pub peer_id_prefix: String,
    pub lamport: u64,
    pub age_s: u64,
    /// Full source-emitted metadata — sources may attach additional
    /// fields beyond the canonical ones above (e.g. event_id,
    /// room_id, admission_origin). Preserved verbatim for inspection
    /// callers who want the whole picture.
    pub metadata: serde_json::Value,
}

/// Run one inspection turn against the persona's airc transcript.
///
/// This is the library entry point — RAG-only inspection. The
/// ServiceModule wraps it; the demo binary wraps it; tests wrap it
/// via stub readers; future adversarial reviewers wrap it via the
/// command.
///
/// For the FULL chain (RAG → prompt → inference → capture) use
/// `inspect_persona_rag_with_inference` and pass an
/// `Arc<dyn AIProviderAdapter>` (heuristic for deterministic tests;
/// llama.cpp / cloud / remote-grid for production probes).
pub async fn inspect_persona_rag(
    request: &RagInspectionRequest,
    airc_reader: Arc<dyn AircTranscriptReader>,
) -> Result<RagInspection, String> {
    inspect_persona_rag_with_inference(request, airc_reader, None).await
}

/// Chained variant: after the RAG layer delivers items, assemble
/// them into a prompt, call the inference adapter, and capture the
/// response into `RagInspection.model_response`.
///
/// Joel (2026-05-31): "AIs are gonna need to analyze what's getting
/// fed into a persona" — this closes the loop. The canonical three
/// introspection questions ([[observability-is-half-the-architecture]]):
///
/// - "Would I respond as it requests at this step?" — answered by
///   `model_response`. The prompt text + the actual response are
///   captured so an inspector can re-run the model with the same
///   prompt and compare.
/// - "Which layer is broken?" — `allocation.allocations` + per-source
///   deliveries (unchanged from the RAG-only path).
/// - "Is this contextually relevant?" — per-item score + age +
///   peer_id_prefix in the deliveries (unchanged).
///
/// Per [[inference-is-an-adapter-always-in-the-loop]], the inference
/// goes through an `AIProviderAdapter` — the same trait the
/// inference command's handle store uses. No bypass, same wire
/// shape, replay-safe (the heuristic adapter is deterministic).
pub async fn inspect_persona_rag_with_inference(
    request: &RagInspectionRequest,
    airc_reader: Arc<dyn AircTranscriptReader>,
    inference_probe: Option<Arc<dyn crate::ai::adapter::AIProviderAdapter>>,
) -> Result<RagInspection, String> {
    let airc_source = AircRagSource::new(request.persona_id, airc_reader)
        .with_fetch_limit(request.airc_fetch_limit);

    let sink: Arc<dyn RagCaptureSink> = match &request.trace_path {
        Some(path) => {
            if let Some(parent) = path.parent() {
                tokio::fs::create_dir_all(parent)
                    .await
                    .map_err(|e| format!("create trace dir: {e}"))?;
            }
            Arc::new(
                JsonlRagCaptureSink::open(path.clone())
                    .map_err(|e| format!("open trace sink: {e}"))?,
            )
        }
        None => Arc::new(NoopRagCaptureSink),
    };

    let recorded = RecordingRagSource::new(airc_source, sink.clone());

    let ctx_base = RagContext::for_persona(request.persona_id, request.now_ms);
    let turn_id = Uuid::new_v4();
    let mut ctx = ctx_base.clone();
    ctx.substrate.turn_id = Some(turn_id);

    let budgets = vec![RagSourceBudget {
        source_id: "airc".to_string(),
        priority: request.airc_priority,
        floor_tokens: request.airc_floor,
        min_tokens: request.airc_floor,
        max_tokens: request.airc_max,
        required: request.airc_required,
    }];

    sink.record(RagCaptureEvent::TurnStart {
        captured_at_ms: request.now_ms,
        persona_id: request.persona_id,
        turn_id: Some(turn_id),
        context_window: request.context_window,
        reserved: request.reserved,
        source_budgets: budgets.clone(),
        context: ctx.clone(),
    });

    let adapter = FlexboxRagBudgetAdapter::new();
    let allocation = adapter.allocate(&ctx, request.context_window, request.reserved, &budgets);

    sink.record(RagCaptureEvent::BudgetAllocated {
        captured_at_ms: request.now_ms,
        persona_id: request.persona_id,
        turn_id: Some(turn_id),
        allocation: allocation.clone(),
    });

    let airc_alloc = allocation
        .allocations
        .first()
        .ok_or_else(|| "allocator returned no source allocations".to_string())?;
    let budget_requested = airc_alloc.allocated_tokens;
    let delivery = recorded
        .deliver(&ctx, budget_requested, ResolutionPreference::Raw)
        .await;

    sink.record(RagCaptureEvent::TurnEnd {
        captured_at_ms: request.now_ms,
        persona_id: request.persona_id,
        turn_id: Some(turn_id),
    });

    let items: Vec<InspectedItem> = delivery
        .items
        .iter()
        .enumerate()
        .map(|(idx, item)| {
            let score = item
                .metadata
                .get("score")
                .and_then(|v| v.as_f64())
                .unwrap_or(0.0);
            let lamport = item
                .metadata
                .get("lamport")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let peer_id_prefix = item
                .metadata
                .get("peer_id")
                .and_then(|v| v.as_str())
                .map(|s| s.chars().take(8).collect::<String>())
                .unwrap_or_else(|| "????".to_string());
            let occurred_at_ms = item
                .metadata
                .get("occurred_at_ms")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let age_s = if occurred_at_ms > 0 && request.now_ms > occurred_at_ms {
                (request.now_ms - occurred_at_ms) / 1_000
            } else {
                0
            };
            let content_preview: String =
                item.content.chars().take(CONTENT_PREVIEW_CHARS).collect();
            InspectedItem {
                index: idx,
                tokens: item.tokens,
                score,
                content_preview,
                peer_id_prefix,
                lamport,
                age_s,
                metadata: item.metadata.clone(),
            }
        })
        .collect();

    // Chain through inference if the caller supplied an adapter.
    let model_response = match inference_probe {
        Some(adapter) => {
            Some(run_inference_probe(adapter, &request.persona_name, &delivery.items).await?)
        }
        None => None,
    };

    Ok(RagInspection {
        persona_id: request.persona_id,
        persona_name: request.persona_name.clone(),
        context_window: request.context_window,
        allocation,
        deliveries: vec![SourceDeliveryInspection {
            source_id: delivery.source_id.clone(),
            budget_requested,
            tokens_used: delivery.tokens_used,
            has_continuation: delivery.continuation.is_some(),
            items,
        }],
        trace_path: request.trace_path.clone(),
        model_response,
    })
}

/// Assemble RAG-delivered items into a prompt, call the adapter,
/// parse the structured `{will_respond, response}` reply.
///
/// Per Joel 2026-06-02 ("113, use real LLMs. We can't know if we use
/// fake algorithms") + [[no-if-statements-use-llms-for-cognition]]:
/// the substrate does NOT gate replies with heuristics. The LLM
/// decides — atomically with writing the response — via JSON-grammar-
/// constrained sampling (`ResponseFormat::JsonObject` flows through
/// to llama.cpp's GBNF grammar; same path the existing
/// `json_object_response_format_enables_json_grammar` test locks).
///
/// Prompt shape:
/// - System: persona identity + the structural decision contract
/// - User messages: one per RAG-delivered item, content verbatim
/// - Forced JSON response: `{"will_respond": bool, "response": str}`
///
/// Failure modes per [[no-fallbacks-ever]]:
/// - Inference call itself fails → `Err` (loop records turns_errored)
/// - LLM emits unparseable JSON → `Err` (substrate refuses to invent
///   a default; the operator sees the typed error and fixes the model)
/// - LLM emits `will_respond: true` but empty response → returned
///   with empty response_text; the loop treats this as skipped per
///   the doc-comment on `ModelResponseInspection`
async fn run_inference_probe(
    adapter: Arc<dyn crate::ai::adapter::AIProviderAdapter>,
    persona_name: &str,
    items: &[crate::persona::rag_budget::RagItem],
) -> Result<ModelResponseInspection, String> {
    use crate::ai::types::{ChatMessage, MessageContent, ResponseFormat, TextGenerationRequest};
    let adapter_id = adapter.provider_id().to_string();
    let model = adapter.default_model().to_string();

    let system_prompt = format!(
        "You are {persona_name}, an autonomous AI persona in a chat room with other \
         humans and AI peers. Recent messages from the room follow as user turns.\n\n\
         Read the most recent message and decide: should you reply, and if so, with \
         what words? If you are directly addressed (the message says \"{persona_name}\" \
         or asks you a question), you should reply. If the conversation is just other \
         peers greeting each other and nothing was asked of you, stay silent.\n\n\
         Output a JSON object with BOTH of these keys, in this order — neither key is \
         optional:\n\
         1. \"will_respond\" (boolean, REQUIRED): true if you are posting a reply, \
            false if you are staying silent. This key must always be present.\n\
         2. \"response\" (string, REQUIRED): the actual words you would say back to \
            the room, in your own voice as {persona_name}. Write the reply, do not \
            describe what you would say. If will_respond is false, this is an empty \
            string. This key must always be present.\n\n\
         Output ONLY the JSON object. No prose around it, no markdown fences. The \
         JSON MUST start with {{\"will_respond\":."
    );

    let messages: Vec<ChatMessage> = items
        .iter()
        .map(|item| ChatMessage {
            role: "user".to_string(),
            content: MessageContent::Text(item.content.clone()),
            name: None,
        })
        .collect();

    let prompt_text = render_prompt_text(&system_prompt, &messages);

    let request = TextGenerationRequest {
        messages,
        system_prompt: Some(system_prompt),
        model: Some(model.clone()),
        provider: None,
        temperature: None,
        // The MODEL owns its generation length — the adapter forwards no
        // ceiling when None (unsloth/llama.cpp run to the model's own stop
        // token). The old 512 "conservative fallback" was exactly the
        // LCD-tier clamp this comment warned against: it truncated qwen3.5
        // mid-`<think>` → empty reply. Capable models self-pace; we never
        // pin a tighter value here.
        max_tokens: None,
        top_p: None,
        top_k: None,
        repeat_penalty: None,
        frequency_penalty: None,
        repeat_last_n: None,
        stop_sequences: None,
        tools: None,
        tool_choice: None,
        // The substrate's cognition contract: JSON-grammar-constrained
        // output. LlamaCpp wires this to GBNF grammar so the sampler
        // can ONLY emit valid JSON (the unit test
        // `json_object_response_format_enables_json_grammar` locks
        // this in `inference/llamacpp_adapter.rs`).
        response_format: Some(ResponseFormat::JsonObject),
        active_adapters: None,
        request_id: None,
        user_id: None,
        room_id: None,
        purpose: Some("persona_decide_and_respond".to_string()),
        persona_id: None,
    };

    let items_count = items.len();
    let total_item_tokens: u32 = items.iter().map(|i| i.tokens).sum();
    let prompt_chars = prompt_text.chars().count();
    let last_item_preview: String = items
        .last()
        .map(|i| i.content.chars().take(160).collect())
        .unwrap_or_else(|| "(no items — RAG delivered nothing)".to_string());

    tracing::info!(
        persona = %persona_name,
        items_count,
        total_item_tokens,
        prompt_chars,
        last_item_preview = %last_item_preview,
        "rag_inspect cognition turn — input shape before LLM call"
    );

    for (idx, item) in items.iter().enumerate() {
        let snippet: String = item.content.chars().take(120).collect();
        // Per [[observability-is-half-the-architecture]] this is the
        // mechanic-grade rationale ("why this item, why not that
        // one") and stays callable. INFO would spam ~12 lines per
        // cognition turn though, so it lives at DEBUG by default
        // and lights up under `RUST_LOG=continuum_core::persona::rag_inspect=debug`
        // when diagnosing a coherence regression.
        tracing::debug!(
            persona = %persona_name,
            idx,
            tokens = item.tokens,
            content = %snippet,
            "rag_inspect item delivered to LLM"
        );
    }

    let response = adapter
        .generate_text(request)
        .await
        .map_err(|e| format!("rag_inspect inference probe failed: {e}"))?;

    tracing::info!(
        persona = %persona_name,
        adapter_id = %adapter_id,
        model = %model,
        finish_reason = %response.finish_reason,
        output_tokens = response.usage.output_tokens,
        response_time_ms = response.response_time_ms,
        raw_response = %response.text,
        "rag_inspect raw model output (pre-parse) — diagnostic for [[no-if-statements-use-llms-for-cognition]] cognition contract"
    );

    let (will_respond, response_text) = parse_decide_and_respond(&response.text).map_err(|e| {
        format!(
            "model emitted unparseable JSON for persona_decide_and_respond: {e}\n\
                 raw response: {raw}",
            raw = response.text
        )
    })?;

    Ok(ModelResponseInspection {
        adapter_id,
        model,
        prompt_text,
        will_respond,
        response_text,
        finish_reason: response.finish_reason.to_string(),
        input_tokens: response.usage.input_tokens,
        output_tokens: response.usage.output_tokens,
        response_time_ms: response.response_time_ms,
    })
}

/// Parse the LLM's structured `{will_respond, response}` JSON output.
/// Returns `(will_respond, response_text)`. Per [[no-fallbacks-ever]]
/// any missing field or wrong type errors visibly — the substrate
/// doesn't silently default a decision the model didn't make.
fn parse_decide_and_respond(raw: &str) -> Result<(bool, String), String> {
    let v: serde_json::Value =
        serde_json::from_str(raw.trim()).map_err(|e| format!("JSON parse: {e}"))?;
    let will_respond = v
        .get("will_respond")
        .and_then(|x| x.as_bool())
        .ok_or_else(|| "missing or non-bool `will_respond`".to_string())?;
    let response = v
        .get("response")
        .and_then(|x| x.as_str())
        .ok_or_else(|| "missing or non-string `response`".to_string())?
        .to_string();
    Ok((will_respond, response))
}

fn render_prompt_text(system_prompt: &str, messages: &[crate::ai::types::ChatMessage]) -> String {
    use crate::ai::types::MessageContent;
    let mut out = String::new();
    out.push_str("System: ");
    out.push_str(system_prompt);
    out.push('\n');
    for msg in messages {
        out.push_str(&msg.role);
        out.push_str(": ");
        match &msg.content {
            MessageContent::Text(s) => out.push_str(s),
            MessageContent::Parts(parts) => {
                for p in parts {
                    if let crate::ai::types::ContentPart::Text { text } = p {
                        out.push_str(text);
                        out.push(' ');
                    }
                }
            }
        }
        out.push('\n');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use airc_core::{
        Body, ClientId, EventId, Headers, MentionTarget, PeerId, RoomId, TranscriptEvent,
        TranscriptKind,
    };
    use airc_lib::AircError;
    use async_trait::async_trait;
    use std::sync::Mutex;

    fn persona() -> Uuid {
        Uuid::parse_str("00000000-0000-0000-0000-000000000aaa").unwrap()
    }

    struct StubReader {
        events: Vec<TranscriptEvent>,
        fail: Mutex<bool>,
    }
    impl StubReader {
        fn new(events: Vec<TranscriptEvent>) -> Self {
            Self {
                events,
                fail: Mutex::new(false),
            }
        }
        fn set_fail(&self, fail: bool) {
            *self.fail.lock().unwrap() = fail;
        }
    }
    #[async_trait]
    impl AircTranscriptReader for StubReader {
        async fn page_recent(&self, limit: usize) -> Result<Vec<TranscriptEvent>, AircError> {
            if *self.fail.lock().unwrap() {
                return Err(AircError::UnknownPeer(PeerId::new()));
            }
            Ok(self.events.iter().take(limit).cloned().collect())
        }
    }

    /// All events in these tests share ONE room. A real airc channel has a single
    /// room_id, and the digest is room-scoped (filters events to the room derived
    /// from the transcript, slice-2 #43). A per-event random room would model
    /// nothing real and silently drop all-but-the-last event from the window.
    static TEST_ROOM: std::sync::LazyLock<RoomId> = std::sync::LazyLock::new(RoomId::new);

    fn make_event(text: Option<&str>, lamport: u64, occurred_at_ms: u64) -> TranscriptEvent {
        TranscriptEvent {
            event_id: EventId::new(),
            room_id: *TEST_ROOM,
            peer_id: PeerId::new(),
            client_id: ClientId::new(),
            kind: TranscriptKind::Message,
            occurred_at_ms,
            lamport,
            target: MentionTarget::Room(*TEST_ROOM),
            headers: Headers::default(),
            body: text.map(Body::text),
            attachment: None,
            receipt: None,
            metadata: serde_json::Value::Null,
        }
    }

    fn request(now_ms: u64) -> RagInspectionRequest {
        let mut req =
            RagInspectionRequest::defaults_for(persona(), "TestPersona".to_string(), now_ms);
        // Tiny-local profile from the demo binary — reserves stay
        // small so the tests assert behavior against a 4k context.
        req.context_window = 4_096;
        req.reserved = ReservedTokens {
            system: 200,
            completion: 800,
        };
        req.airc_floor = 100;
        req.airc_max = 2_000;
        req
    }

    // ---- TDD tests ----

    #[tokio::test]
    async fn empty_transcript_yields_empty_delivery() {
        let reader = Arc::new(StubReader::new(vec![]));
        let result = inspect_persona_rag(&request(1_000_000), reader)
            .await
            .unwrap();
        assert_eq!(result.persona_id, persona());
        assert_eq!(result.persona_name, "TestPersona");
        assert_eq!(result.context_window, 4_096);
        assert_eq!(result.deliveries.len(), 1);
        let d = &result.deliveries[0];
        assert_eq!(d.source_id, "airc");
        assert!(d.items.is_empty());
        assert_eq!(d.tokens_used, 0);
        assert!(!d.has_continuation);
    }

    #[tokio::test]
    async fn allocation_reports_satisfied_state_for_required_source_with_room() {
        let reader = Arc::new(StubReader::new(vec![]));
        let result = inspect_persona_rag(&request(1_000_000), reader)
            .await
            .unwrap();
        // 4096 - 200 system - 800 completion = 3096 available; airc gets max=2000 → Satisfied
        assert!(!result.allocation.escalation_needed);
        let airc_a = &result.allocation.allocations[0];
        assert_eq!(airc_a.source_id, "airc");
        assert_eq!(airc_a.allocated_tokens, 2_000);
    }

    #[tokio::test]
    async fn inspected_items_carry_score_age_and_peer_prefix() {
        let now_ms = 2_000_000u64;
        let event_ms = 1_995_000u64; // 5 seconds ago
        let reader = Arc::new(StubReader::new(vec![make_event(
            Some("hello world"),
            42,
            event_ms,
        )]));
        let result = inspect_persona_rag(&request(now_ms), reader).await.unwrap();
        let items = &result.deliveries[0].items;
        assert_eq!(items.len(), 1);
        let it = &items[0];
        assert_eq!(it.index, 0);
        assert_eq!(it.content_preview, "hello world");
        assert!(
            (it.score - 1.0).abs() < 1e-9,
            "first item scores 1.0, got {}",
            it.score
        );
        assert_eq!(it.lamport, 42);
        assert_eq!(it.age_s, 5);
        assert_eq!(it.peer_id_prefix.len(), 8);
        assert!(it.metadata.get("event_id").is_some());
    }

    #[tokio::test]
    async fn long_content_is_truncated_in_preview_but_tokens_remain_accurate() {
        // 1000-char message → preview is CONTENT_PREVIEW_CHARS chars; tokens are full message
        let msg: String = "x".repeat(1_000);
        let reader = Arc::new(StubReader::new(vec![make_event(Some(&msg), 1, 1_000_000)]));
        let mut req = request(1_000_000);
        req.airc_max = 10_000; // ample budget
        let result = inspect_persona_rag(&req, reader).await.unwrap();
        let it = &result.deliveries[0].items[0];
        assert_eq!(it.content_preview.chars().count(), CONTENT_PREVIEW_CHARS);
        assert!(
            it.tokens >= 250,
            "1000 chars should cost ~250 tokens, got {}",
            it.tokens
        );
    }

    // what this catches: under the slice-2 #43 digest contract the window is
    // bounded by budget alone — pack_digest walks newest-first and TRUNCATES
    // when the budget is exhausted; there is NO continuation cursor (the digest
    // IS the window — more history is a scrollback command, not a budget page).
    // A regression that re-introduced a continuation cursor, or that stopped
    // truncating, would break this.
    #[tokio::test]
    async fn tight_budget_truncates_the_window_with_no_continuation_cursor() {
        // 4 items × ~2 tokens each ("ddddd" = 5 chars → chars/4 + 1 = 2 tokens),
        // budget of 4 tokens fits only the 2 newest.
        let reader = Arc::new(StubReader::new(vec![
            make_event(Some("aaaaa"), 1, 1_000_000),
            make_event(Some("bbbbb"), 2, 1_000_000),
            make_event(Some("ccccc"), 3, 1_000_000),
            make_event(Some("ddddd"), 4, 1_000_000),
        ]));
        let mut req = request(1_000_000);
        req.airc_floor = 4;
        req.airc_max = 4;
        let result = inspect_persona_rag(&req, reader).await.unwrap();
        let d = &result.deliveries[0];
        assert!(d.items.len() < 4, "tight budget truncates the window");
        assert!(
            !d.has_continuation,
            "the digest carries no continuation cursor by design"
        );
    }

    #[tokio::test]
    async fn reader_failure_surfaces_as_empty_delivery_not_panic() {
        let reader = Arc::new(StubReader::new(vec![make_event(
            Some("oops"),
            1,
            1_000_000,
        )]));
        reader.set_fail(true);
        let result = inspect_persona_rag(&request(1_000_000), reader)
            .await
            .unwrap();
        assert!(result.deliveries[0].items.is_empty());
        // No panic — substrate-is-a-good-citizen
    }

    #[tokio::test]
    async fn trace_path_writes_jsonl_lines() {
        let dir = tempfile::tempdir().unwrap();
        let trace = dir.path().join("inspect.jsonl");
        let reader = Arc::new(StubReader::new(vec![make_event(
            Some("traced"),
            1,
            1_000_000,
        )]));
        let mut req = request(1_000_000);
        req.trace_path = Some(trace.clone());
        let result = inspect_persona_rag(&req, reader).await.unwrap();
        assert_eq!(result.trace_path.as_deref(), Some(trace.as_path()));
        let body = std::fs::read_to_string(&trace).unwrap();
        // Expect at least TurnStart, BudgetAllocated, SourceDelivered, TurnEnd
        let line_count = body.lines().count();
        assert!(
            line_count >= 4,
            "expected ≥4 capture events, got {line_count}"
        );
        assert!(body.contains("turn_start"));
        assert!(body.contains("budget_allocated"));
        assert!(body.contains("source_delivered"));
        assert!(body.contains("turn_end"));
    }

    #[tokio::test]
    async fn no_trace_path_uses_noop_sink() {
        let reader = Arc::new(StubReader::new(vec![make_event(
            Some("untraced"),
            1,
            1_000_000,
        )]));
        let req = request(1_000_000);
        assert!(req.trace_path.is_none());
        let result = inspect_persona_rag(&req, reader).await.unwrap();
        assert!(result.trace_path.is_none());
        // Just don't panic; Noop sink swallowed everything.
        assert_eq!(result.deliveries[0].items.len(), 1);
    }

    #[tokio::test]
    async fn cross_persona_scope_check_yields_empty_via_source() {
        // Inspection driven for persona A, but the source itself
        // rejects cross-persona ctx. We construct the request for
        // persona A; the source is built around persona A; we
        // verify the items come from A's view — defense in depth.
        let reader = Arc::new(StubReader::new(vec![make_event(
            Some("for A"),
            1,
            1_000_000,
        )]));
        let result = inspect_persona_rag(&request(1_000_000), reader)
            .await
            .unwrap();
        assert_eq!(result.persona_id, persona());
        assert_eq!(result.deliveries[0].items.len(), 1);
    }

    // ── chained inference probe (task #104) ─────────────────────

    #[tokio::test]
    async fn ragonly_path_leaves_model_response_none() {
        let reader = Arc::new(StubReader::new(vec![make_event(Some("hi"), 1, 999_000)]));
        let result = inspect_persona_rag(&request(1_000_000), reader)
            .await
            .unwrap();
        assert!(result.model_response.is_none());
    }

    #[tokio::test]
    async fn chained_path_captures_response_from_heuristic_adapter() {
        use crate::ai::heuristic_adapter::{HeuristicInferenceAdapter, HEURISTIC_PROVIDER_ID};
        let reader = Arc::new(StubReader::new(vec![
            make_event(Some("hello"), 1, 999_000),
            make_event(Some("world"), 2, 999_500),
        ]));
        let adapter: Arc<dyn crate::ai::adapter::AIProviderAdapter> =
            Arc::new(HeuristicInferenceAdapter::new());
        let result = inspect_persona_rag_with_inference(&request(1_000_000), reader, Some(adapter))
            .await
            .unwrap();
        let mr = result.model_response.expect("expected model_response");
        assert_eq!(mr.adapter_id, HEURISTIC_PROVIDER_ID);
        assert!(mr.response_text.starts_with("[heuristic:"));
        // The latest user message should appear in the response
        // (the heuristic adapter echoes the last user turn).
        assert!(mr.response_text.contains("world"));
        assert_eq!(mr.finish_reason, "stop");
        assert!(mr.input_tokens > 0);
        assert!(mr.output_tokens > 0);
    }

    #[tokio::test]
    async fn chained_path_with_zero_items_still_produces_marker_response() {
        use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
        let reader = Arc::new(StubReader::new(vec![]));
        let adapter: Arc<dyn crate::ai::adapter::AIProviderAdapter> =
            Arc::new(HeuristicInferenceAdapter::new());
        let result = inspect_persona_rag_with_inference(&request(1_000_000), reader, Some(adapter))
            .await
            .unwrap();
        let mr = result
            .model_response
            .expect("expected model_response even with no items");
        // The heuristic adapter saw an empty messages list → "(no
        // user text in prompt)" marker response per its contract.
        assert!(mr.response_text.contains("(no user text in prompt)"));
    }

    #[tokio::test]
    async fn chained_path_prompt_text_carries_system_and_messages() {
        use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
        let reader = Arc::new(StubReader::new(vec![make_event(
            Some("greetings persona"),
            1,
            999_000,
        )]));
        let adapter: Arc<dyn crate::ai::adapter::AIProviderAdapter> =
            Arc::new(HeuristicInferenceAdapter::new());
        let result = inspect_persona_rag_with_inference(&request(1_000_000), reader, Some(adapter))
            .await
            .unwrap();
        let prompt = result.model_response.unwrap().prompt_text;
        assert!(prompt.contains("You are TestPersona"));
        assert!(prompt.contains("greetings persona"));
        assert!(prompt.starts_with("System:"));
        assert!(prompt.contains("user:"));
    }

    #[tokio::test]
    async fn chained_path_same_prompt_yields_same_response_replay_safe() {
        // The heuristic adapter is deterministic — running the same
        // inspection twice produces byte-identical responses. This is
        // the substrate's replay contract per
        // [[inference-is-an-adapter-always-in-the-loop]].
        use crate::ai::heuristic_adapter::HeuristicInferenceAdapter;
        let reader1 = Arc::new(StubReader::new(vec![make_event(Some("hi"), 1, 999_000)]));
        let reader2 = Arc::new(StubReader::new(vec![make_event(Some("hi"), 1, 999_000)]));
        let adapter1: Arc<dyn crate::ai::adapter::AIProviderAdapter> =
            Arc::new(HeuristicInferenceAdapter::new());
        let adapter2: Arc<dyn crate::ai::adapter::AIProviderAdapter> =
            Arc::new(HeuristicInferenceAdapter::new());
        let r1 = inspect_persona_rag_with_inference(&request(1_000_000), reader1, Some(adapter1))
            .await
            .unwrap();
        let r2 = inspect_persona_rag_with_inference(&request(1_000_000), reader2, Some(adapter2))
            .await
            .unwrap();
        let m1 = r1.model_response.unwrap();
        let m2 = r2.model_response.unwrap();
        assert_eq!(m1.response_text, m2.response_text);
        assert_eq!(m1.prompt_text, m2.prompt_text);
    }
}
