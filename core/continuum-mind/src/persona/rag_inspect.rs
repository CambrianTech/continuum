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
//! - Inspection is side-effect free: a source delivers everything it holds
//!   and does not advance its read cursor on `deliver` (that is a separate
//!   mark_read), and no budget is allocated here; her prompt view owns the
//!   one allocation.
//! - [[source-drain-is-the-universal-pattern]] — the inspection IS
//!   the drain for in-flight RAG decisions. Without it those
//!   decisions are sources without drains, which is the leak shape.

use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::cognition::conversation::transcript_reader::AircTranscriptReader;
use crate::persona::airc_source::AircRagSource;
use crate::cognition::rag_budget::{RagContext, RagSource};
use crate::persona::rag_capture::{
    JsonlRagCaptureSink, NoopRagCaptureSink, RagCaptureEvent, RagCaptureSink, RecordingRagSource,
};



/// Tunable inputs for one inspection, built via `for_persona` (her profile's window) or
/// `for_window` (the served window, or one the caller names).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RagInspectionRequest {
    pub persona_id: Uuid,
    pub persona_name: String,
    /// Reported back in the inspection; nothing is allocated against it here.
    pub context_window: u32,
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

    /// THE derivation both constructors share. The source delivers whole and her
    /// prompt view owns the one allocation, so the window is only reported.
    pub fn for_window(
        persona_id: Uuid,
        persona_name: String,
        now_ms: u64,
        context_window: u32,
    ) -> Self {
        Self {
            persona_id,
            persona_name,
            context_window,
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

/// The honest-look result. Carries the per-source delivery details with the mechanic-grade rationale
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
/// inspection caller needs: actual usage, continuation flag, and the full list of items packed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceDeliveryInspection {
    pub source_id: String,
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
    pub content: String,
    pub peer_id_shown: String,
    pub lamport: u64,
    pub age_s: u64,
    /// The item has a coarser summary level her view can fall back to.
    pub has_summary: bool,
    /// The item has a pointer level (what it is and how to reopen it).
    pub has_pointer: bool,
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
/// - "Which layer is broken?" — per-source deliveries and each item's
///   levels (whole, summary, pointer).
/// - "Is this contextually relevant?" — per-item score + age +
///   peer_id_shown in the deliveries (unchanged).
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

    sink.record(RagCaptureEvent::TurnStart {
        captured_at_ms: request.now_ms,
        persona_id: request.persona_id,
        turn_id: Some(turn_id),
        context: ctx.clone(),
    });

    let delivery = recorded.deliver(&ctx).await;

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
            let peer_id_shown = item
                .metadata
                .get("peer_id")
                .and_then(|v| v.as_str())
                .map(|s| uuid::Uuid::parse_str(s).map_or_else(|_| s.to_owned(), airc_core::shown_id::shown_form)) // map_or_else: a peer id that is not a uuid is shown whole
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
            let content = item.content.clone();
            InspectedItem {
                index: idx,
                tokens: item.tokens,
                score,
                content,
                peer_id_shown,
                lamport,
                age_s,
                has_summary: item.summary.is_some(),
                has_pointer: item.pointer.is_some(),
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
        deliveries: vec![SourceDeliveryInspection {
            source_id: delivery.source_id.clone(),
            tokens_used: delivery.tokens_used,
            has_continuation: delivery.truncated,
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
    items: &[crate::cognition::rag_budget::RagItem],
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
        // ceiling when None (llama.cpp run to the model's own stop
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
        tail_tools: None,
        view_messages: None,
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
        turn_bound: None,
    };

    let items_count = items.len();
    let total_item_tokens: u32 = items.iter().map(|i| i.tokens).sum();
    let prompt_chars = prompt_text.chars().count();
    // Sizes, not a cut of the text: the items themselves are in the prompt capture.
    let last_item_tokens = items.last().map_or(0, |i| i.tokens); // map_or: no items, nothing delivered

    tracing::info!(
        persona = %persona_name,
        items_count,
        total_item_tokens,
        prompt_chars,
        last_item_tokens,
        "rag_inspect cognition turn — input shape before LLM call"
    );

    for (idx, item) in items.iter().enumerate() {
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
            content = %item.content,
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
mod tests;
