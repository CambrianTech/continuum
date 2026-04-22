//! Per-persona response orchestration in Rust. The Rust replacement for
//! `PersonaResponseGenerator.ts` — owns the cognitive verb of "this
//! persona, given this message in this room, produces this response."
//!
//! See docs/architecture/SHARED-COGNITION.md for the architectural
//! picture and docs/architecture/PERSONA-COGNITION-RUST-MIGRATION.md
//! for the migration discipline this module is the first rung of.
//!
//! Pipeline (per persona, per inbound message):
//!
//!   1. cognition::analyze(...)   — shared, cached. Provides the
//!                                  prompt-time hint map (suggested
//!                                  angles per specialty) but does NOT
//!                                  gate response. Informational only.
//!   2. prompt_assembly::build(...) — persona-specific prompt: voice,
//!                                    LoRA-rendered specialty, RAG
//!                                    context interleaving, native
//!                                    multimodal attachment per the
//!                                    persona's resolved capabilities.
//!   3. ai_provider::generate_text(...) — inference. The persona's
//!                                        own model decides what to
//!                                        say. Personas emulate
//!                                        humans — they choose for
//!                                        themselves whether to
//!                                        engage; no external scorer
//!                                        vetoes them.
//!   4. strip_thinks_emit_events(...) — extract <think>...</think>
//!                                       blocks, emit them as
//!                                       cognition:think-block events
//!                                       for the (future) hippocampus
//!                                       to consume, return clean
//!                                       speech for posting.
//!   5. Return Spoke { text, ... } with timing + diagnostic fields.
//!      Silent is still a valid return when the persona's own model
//!      produces empty / "I'll pass" output — but it's the persona's
//!      cognitive output, not a pre-inference veto.
//!
//! Why this is in Rust (not just a port):
//!   - Cognition is where the mind/machine line gets drawn — concurrency
//!     primitives matter (Joel, 2026-04-19).
//!   - SharedAnalysis cache lives here; needs lock-free DashMap for
//!     concurrent personas hitting the same message.
//!   - Per-persona renders run in parallel tokio tasks; Node's single
//!     event loop blocks every persona on every other persona's
//!     inference call.
//!   - <think> parsing is a hot path on every response; regex/str
//!     manipulation in Rust is ~100x what TS does on the same input.

use crate::cognition::tool_executor::types::MediaItemLite;
use crate::cognition::{analyze, AnalysisInput, PersonaSlot, RecentMessage, SharedAnalysis};
use serde::{Deserialize, Serialize};
use std::time::SystemTime;
use ts_rs::TS;
use uuid::Uuid;

/// Input to `respond()`. Caller (chat path / PRG.ts shim) collects this
/// from the room state. Carries everything needed for ONE persona's
/// response cycle — analysis is shared via cache, so no need to pass
/// other personas in.
#[derive(Debug, Clone)]
pub struct RespondInput {
    /// THIS persona's identity + specialty for scoring.
    pub persona: PersonaSlot,
    pub room_id: Uuid,
    pub message_id: Uuid,
    /// The new message that triggered this response cycle.
    pub message_text: String,
    /// Recent messages for analysis context. Most-recent last.
    pub recent_history: Vec<RecentMessage>,
    /// Stable specialty identifiers in the room (all personas in the
    /// room, not just this one). The analyzer uses this list to know
    /// which `suggested_angles` keys to populate. This persona's own
    /// specialty must appear here.
    pub known_specialties: Vec<String>,
    /// Persona's RAG-built identity / system prompt. Caller-supplied
    /// because the persona's identity comes from RAG (which knows the
    /// persona entity, the active adapters, the user-personalization
    /// bits). The render concatenates this with the matched angle from
    /// the shared analysis.
    pub system_prompt: String,
    /// THIS persona's model identifier. Render-time choice, NOT the
    /// analysis model. Shared-cognition architecture: 1 cheap analysis
    /// on a base model + N specialty renders each on the persona's own
    /// (potentially LoRA-adapted) model. Using analysis.model_used here
    /// would defeat the entire premise — every persona would render with
    /// the same base model.
    pub model: String,
    /// True if this is a live-voice context (changes response style
    /// instructions in the assembled prompt). False for normal chat.
    pub is_voice: bool,
    /// Media (images/audio/video) attached to the current message. When
    /// present AND `capabilities` includes the matching variant
    /// (`Vision` for images, `AudioInput` for audio), the render path
    /// constructs `MessageContent::Parts` with a real
    /// `ContentPart::Image`/`Audio` instead of `MessageContent::Text` —
    /// preserving the natively-multimodal model's ability to see / hear
    /// directly. **No text-description bridging when the model IS
    /// capable** — that's the regression Joel called out 2026-04-21.
    /// Bridge layer (VisionDescriptionService) remains for genuinely
    /// text-only models as the floor, not the default.
    /// See docs/architecture/PERSONA-CONTEXT-PAGING.md §0.5.X.
    pub message_media: Vec<MediaItemLite>,
    /// Persona's resolved model capabilities. Caller (PRG) supplies them
    /// from the persona's ModelConfig — they're a property of the
    /// caller's request, not something Rust looks up mid-flight.
    ///
    /// Why this isn't a registry lookup: `getThatThingIShouldHaveJustBeenGiven`
    /// (Joel rule). The IPC already names the model; the caller already
    /// knows what it can do; passing it across removes a global lookup
    /// that silently failed when registry keys diverged from request
    /// model strings (capabilities came back empty → image bytes
    /// demoted to text marker → vision encoder never called even though
    /// the bytes were sitting right there in `message_media`). Now the
    /// declaration travels with the request — registry-key drift can't
    /// silently disable vision.
    pub capabilities: std::collections::HashSet<crate::model_registry::Capability>,
}

/// Build a `RespondInput` from a JSON wire payload. Single source of
/// truth for the transformation — the IPC handler in
/// `modules/cognition.rs` calls this AND every test that wants to
/// exercise the same code path the live system uses calls this.
///
/// # Why this exists
///
/// 2026-04-22: Joel called out that "integration tests" written in
/// parallel with the prod IPC handler could drift silently — pass in
/// the test, fail in prod, or vice versa. The fix is to make the
/// JSON → RespondInput transformation a single function both call.
/// Tests that reconstruct `RespondInput` by hand from a fixture's
/// `rust_request` aren't testing the live path; they're testing a
/// hand-rolled twin of it. This function eliminates the twin.
///
/// Wire field names are camelCase per the ts-rs export convention
/// (`PersonaRespondRequest` in `bindings/modules/cognition.ts`),
/// with `_` aliases accepted for back-compat with older fixtures.
/// `model`, `messageText`, and `capabilities` are required (hard
/// error on absence — same contract as the IPC handler).
pub fn respond_input_from_value(
    payload: &serde_json::Value,
) -> Result<RespondInput, String> {
    use crate::cognition::PersonaSlot;
    use crate::cognition::tool_executor::types::MediaItemLite;

    let get_str = |key_camel: &str, key_snake: &str| -> Option<String> {
        payload
            .get(key_camel)
            .or_else(|| payload.get(key_snake))
            .and_then(|v| v.as_str())
            .map(String::from)
    };
    let get_uuid = |key_camel: &str, key_snake: &str| -> Result<Uuid, String> {
        let s = get_str(key_camel, key_snake)
            .ok_or_else(|| format!("missing required uuid field '{key_camel}'/'{key_snake}'"))?;
        Uuid::parse_str(&s).map_err(|e| format!("invalid uuid for '{key_camel}': {e}"))
    };

    let persona_id = get_uuid("personaId", "persona_id")?;
    let room_id = get_uuid("roomId", "room_id")?;
    let message_id = get_uuid("messageId", "message_id")?;
    let message_text = get_str("messageText", "message_text")
        .ok_or_else(|| "missing required field 'messageText'".to_string())?;
    let persona_name = get_str("personaName", "persona_name").unwrap_or_else(|| "AI".to_string());
    let specialty = get_str("specialty", "specialty").unwrap_or_else(|| "general".to_string());
    let model = get_str("model", "model")
        .ok_or_else(|| "missing required field 'model'".to_string())?;
    let system_prompt = get_str("systemPrompt", "system_prompt").unwrap_or_default();
    let is_voice = payload
        .get("isVoice")
        .or_else(|| payload.get("is_voice"))
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    // recent_history: array of { id, sender_name|senderName, text }.
    // Most-recent last; chat path / PRG.ts shim builds this from the
    // room's recent messages. Items that don't parse drop silently —
    // single bad row in history shouldn't kill the call.
    let recent_history: Vec<RecentMessage> = payload
        .get("recentHistory")
        .or_else(|| payload.get("recent_history"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let id = item.get("id")?.as_str()?.parse::<Uuid>().ok()?;
                    let sender_name = item
                        .get("senderName")
                        .or_else(|| item.get("sender_name"))?
                        .as_str()?
                        .to_string();
                    let text = item.get("text")?.as_str()?.to_string();
                    Some(RecentMessage {
                        id,
                        sender_name,
                        text,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    let known_specialties: Vec<String> = payload
        .get("knownSpecialties")
        .or_else(|| payload.get("known_specialties"))
        .and_then(|v| serde_json::from_value::<Vec<String>>(v.clone()).ok())
        .unwrap_or_else(|| vec![specialty.clone()]);

    // Native multimodal: walk message_media into MediaItemLite.
    // itemType + base64 + mimeType + description (camelCase wire).
    // Items missing item_type drop silently (defensive — same shape
    // the prior IPC handler used).
    let message_media: Vec<MediaItemLite> = payload
        .get("messageMedia")
        .or_else(|| payload.get("message_media"))
        .and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|item| {
                    let item_type = item
                        .get("itemType")
                        .or_else(|| item.get("item_type"))?
                        .as_str()?
                        .to_string();
                    let base64 = item
                        .get("base64")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let mime_type = item
                        .get("mimeType")
                        .or_else(|| item.get("mime_type"))
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    let description = item
                        .get("description")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                    Some(MediaItemLite {
                        item_type,
                        base64,
                        mime_type,
                        description,
                    })
                })
                .collect()
        })
        .unwrap_or_default();

    // Capabilities: REQUIRED. Caller (PRG) must populate from the
    // persona's resolved ModelConfig — the whole point of caller-
    // declared capabilities is to remove a global registry lookup
    // that was silently returning empty mid-flight. Hard error on
    // absence so the regression surfaces at the seam, not three
    // layers down as silently-broken multimodal.
    let capabilities: std::collections::HashSet<crate::model_registry::Capability> = payload
        .get("capabilities")
        .ok_or_else(|| {
            "missing required field 'capabilities' — caller MUST declare \
             the persona's resolved model capability vocabulary"
                .to_string()
        })?
        .as_array()
        .ok_or_else(|| "'capabilities' must be a JSON array of strings".to_string())?
        .iter()
        .filter_map(|s| s.as_str())
        .filter_map(|s| serde_json::from_value(serde_json::Value::String(s.to_string())).ok())
        .collect();

    Ok(RespondInput {
        persona: PersonaSlot {
            persona_id,
            specialty,
            display_name: persona_name,
        },
        room_id,
        message_id,
        message_text,
        recent_history,
        known_specialties,
        system_prompt,
        model,
        is_voice,
        message_media,
        capabilities,
    })
}

/// What `respond()` returns.
///
/// `Silent` is a first-class outcome: the persona considered the message,
/// found nothing additive to add, and chose not to speak. The reason is
/// observable for trainability + the persona's own meta-cognitive trace.
/// Not the same as a failure.
///
/// `Spoke` is the response that should be posted to the room.
// NOTE on field casing: ts-rs does not propagate `rename_all = "camelCase"`
// through enum variant FIELDS (only through variant TAGS). Forcing camelCase
// on the serde side without ts-rs honoring it would silently diverge the
// wire format from the generated TS bindings (caught during initial review).
// Snake_case on both sides keeps them in lockstep. Variant tags ("silent",
// "spoke") are handled by the tag rename below.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "lowercase")]
#[ts(
    export,
    export_to = "../../../shared/generated/cognition/PersonaResponse.ts"
)]
pub enum PersonaResponse {
    /// Persona chose silence. Reason carried for observability + training.
    Silent {
        #[ts(type = "string")]
        persona_id: Uuid,
        reason: String,
        /// Relevance score that drove the decision. 0.0..1.0. Carried so
        /// downstream telemetry can analyze the silence-rate by score
        /// distribution.
        relevance_score: f32,
    },
    /// Persona produced a response. The text is the CLEANED visible
    /// speech (`<think>` blocks already stripped + emitted as events).
    Spoke {
        #[ts(type = "string")]
        persona_id: Uuid,
        /// Cleaned visible speech to post to the room. `<think>` blocks
        /// have been stripped; the visible response is what the user
        /// sees in chat.
        text: String,
        /// Model that produced the response (post-routing).
        model_used: String,
        /// Duration of the inference call itself (not including
        /// analysis or scoring — those are separate).
        #[ts(type = "number")]
        inference_ms: u64,
        /// Total duration end-to-end (analysis + scoring + inference +
        /// parsing + event emission).
        #[ts(type = "number")]
        total_ms: u64,
        /// Number of `<think>` blocks extracted (for telemetry —
        /// the actual content was emitted as events for hippocampus).
        think_blocks_emitted: u32,
    },
}

/// THE per-persona response cycle. Public entry point.
///
/// Called by the chat path (or the PRG.ts shim that the chat path
/// currently calls). Each call is for ONE persona; the shared analysis
/// is cached at the cognition layer, so M personas calling this
/// concurrently for the same message do M renders + 1 analysis (not M).
///
/// Returns `Result` because inference can genuinely fail (model down,
/// timeout, bad output we can't parse). Failure should propagate to
/// the caller for proper user-facing error reporting; we don't
/// silently fall back to "Silent" because that would hide real bugs.
pub async fn respond(input: RespondInput) -> Result<PersonaResponse, String> {
    use crate::persona::trace::{
        CognitionTrace, SEAM_ANALYZE, SEAM_INFERENCE, SEAM_POST_PROCESS,
    };

    let total_start = now_ms();
    let mut trace = CognitionTrace::new();

    // 1. Shared analysis (cached per message+room+history fingerprint).
    //    Provides matched-angle hints for the prompt — informational,
    //    NOT gating. The persona's own model is the only thing that
    //    decides what to say (or whether to stay quiet).
    let analyze_start = now_ms();
    let analysis = analyze(AnalysisInput {
        message_id: input.message_id,
        room_id: input.room_id,
        text: input.message_text.clone(),
        recent_history: input.recent_history.clone(),
        known_specialties: input.known_specialties.clone(),
    })
    .await?;
    trace.record(
        SEAM_ANALYZE,
        analyze_start,
        now_ms().saturating_sub(analyze_start),
        serde_json::json!({
            "from_cache": analysis.from_cache,
            "model_used": analysis.model_used,
            "duration_ms_internal": analysis.duration_ms,
        }),
    );

    // 2. Render. No external "should this persona respond" gate. Joel
    //    rule (2026-04-22): personas emulate humans — they choose
    //    themselves whether to engage. The earlier `score_persona`
    //    + suggested_angles[specialty] vetoed vision-capable personas
    //    on image-bearing messages because the analyzer's text-domain
    //    map didn't tag "general" as relevant — silenced the only
    //    persona that could SEE the image. Mechanical routing
    //    masquerading as cognition. Removed.
    //
    //    A persona may still emit Silence as its OWN cognitive
    //    output (its model returns "I'll pass on this one" or
    //    similar) — that's organic. What's gone is the external
    //    veto that decided FOR the persona.
    //
    //    `analysis.suggested_angles` remains as a prompt-time hint:
    //    if the analyzer extracted a per-specialty angle, the prompt
    //    assembler injects it; if not, the persona just sees the
    //    plain message + history + media, same as a human.
    let inference_start = now_ms();
    let raw_response = run_render(&input, &analysis).await?;
    let inference_ms = now_ms().saturating_sub(inference_start);
    trace.record(
        SEAM_INFERENCE,
        inference_start,
        inference_ms,
        serde_json::json!({
            "model_used": raw_response.model_used,
            "raw_text_chars": raw_response.text.len(),
            "media_attached": input.message_media.len(),
        }),
    );

    let post_start = now_ms();
    let (visible_text, think_count) = strip_thinks_emit_events(
        &raw_response.text,
        input.persona.persona_id,
        input.message_id,
    );
    trace.record(
        SEAM_POST_PROCESS,
        post_start,
        now_ms().saturating_sub(post_start),
        serde_json::json!({
            "think_blocks": think_count,
            "visible_chars": visible_text.len(),
        }),
    );

    let response = PersonaResponse::Spoke {
        persona_id: input.persona.persona_id,
        text: visible_text,
        model_used: raw_response.model_used,
        inference_ms,
        total_ms: now_ms().saturating_sub(total_start),
        think_blocks_emitted: think_count,
    };

    // Best-effort turn capture for observability + replay. Failures
    // log inside the recorder but never propagate — the persona's
    // response is the product, the recording is observability. Any
    // host (TS server, Unreal plugin, Swift app) gets this for free
    // because it lives Rust-side, next to `respond()`.
    crate::persona::recorder::record_turn(&input, &response, &trace);

    Ok(response)
}

/// What the render step returns internally (private — public type is
/// `PersonaResponse`).
struct RawRenderOutput {
    text: String,
    model_used: String,
}

/// Runs the prompt-assembly + inference for one persona's render.
///
/// 1. Pulls the matched angle for THIS persona's specialty from the
///    shared analysis (the orchestrator's "what your perspective adds
///    here" signal).
/// 2. Calls `prompt_assembly::assemble()` (memento's pure function port
///    of the TS PromptAssembler) to build the system message + chat
///    history with proper time-gap markers, social-awareness blocks,
///    and the matched-angle injection.
/// 3. Selects an inference adapter via the global registry. Routes by
///    capability — `provider="local"` + `device=Gpu` lets the registry
///    pick DMR / Vulkan / whichever GPU adapter actually supports the
///    requested model. No hardcoded provider name. Hard error if
///    nothing matches (the existing rule: never silent CPU fallback).
/// 4. Calls `adapter.generate_text(...)` and returns the raw output.
///    `<think>` parsing happens in the caller (`respond()`).
async fn run_render(
    input: &RespondInput,
    analysis: &SharedAnalysis,
) -> Result<RawRenderOutput, String> {
    use crate::ai::adapter::InferenceDevice;
    use crate::ai::types::TextGenerationRequest;
    use crate::persona::prompt_assembly::{assemble, HistoryMessage, PromptAssemblyInput};

    // 1. The matched angle for this persona's specialty. Empty string
    //    means "no specific angle" — assemble() handles that gracefully
    //    (no angle injection in the system prompt).
    let matched_angle = analysis
        .suggested_angles
        .get(&input.persona.specialty)
        .cloned()
        .unwrap_or_default();

    // 2. Convert RecentMessage → HistoryMessage. RecentMessage is
    //    intentionally minimal (analysis-only). The render uses what
    //    we have; if the chat path later wants role/timestamp distinction,
    //    extend RecentMessage and the conversion follows.
    let history: Vec<HistoryMessage> = input
        .recent_history
        .iter()
        .map(|m| HistoryMessage {
            role: "user".to_string(),
            name: Some(m.sender_name.clone()),
            content: m.text.clone(),
            timestamp_ms: None,
        })
        .collect();

    let current_message = HistoryMessage {
        role: "user".to_string(),
        name: None,
        content: input.message_text.clone(),
        timestamp_ms: None,
    };

    // Multi-party chat shape comes from the model registry — single
    // source of truth per the OOP-adapter rule. Code never branches on
    // model name. Default applies if the registry has no row (e.g. a
    // brand-new cloud model not yet declared).
    let multi_party_strategy = crate::model_registry::try_global()
        .and_then(|reg| reg.model(&input.model))
        .map(|m| m.multi_party_strategy.clone())
        .unwrap_or_default();

    let prompt_input = PromptAssemblyInput {
        persona_name: input.persona.display_name.clone(),
        system_prompt: input.system_prompt.clone(),
        matched_angle,
        history,
        current_message,
        is_voice: input.is_voice,
        social_signals: None,
        multi_party_strategy,
    };

    let assembled = assemble(&prompt_input);

    // 3. Build the inference request from the assembled prompt.
    //
    // Native multimodal: if the caller passed media AND the persona's
    // resolved model declares the matching sensory capability
    // (Vision for image, AudioInput for audio), we attach the media
    // DIRECTLY as `ContentPart::Image` / `ContentPart::Audio` on the
    // FINAL user-role message — the one carrying the current message.
    // The model sees / hears the source bytes, no description bridge.
    //
    // When the model lacks the capability we fall through to the
    // text-only path. The sensory bridge (`VisionDescriptionService`,
    // STT) would inject a description upstream — that's the leveler
    // for genuinely text-only models, not the default route.
    //
    // See docs/architecture/PERSONA-CONTEXT-PAGING.md §0.5.X.
    //
    // Capabilities come WITH the request — no global registry lookup. The
    // prior shape (try_global → reg.model(&input.model)) silently returned
    // empty caps when the registry's lookup key didn't match `input.model`
    // verbatim; image bytes were already in `message_media` but the empty
    // caps demoted them to text markers, so the vision encoder never got
    // called even on a vision-capable persona. Caller-declared
    // capabilities removes the silent-drop seam (Joel rule:
    // "getThatThingIShouldHaveJustBeenGiven").
    let messages = build_messages_with_media(
        assembled.messages,
        &input.message_media,
        &input.capabilities,
    );

    let request = TextGenerationRequest {
        messages,
        system_prompt: Some(assembled.system_message),
        model: Some(input.model.clone()),
        provider: Some("local".to_string()),
        temperature: Some(0.7),
        // No cap. The adapter falls back to backend.n_ctx_train() when
        // None, giving the model its full trained context window.
        // Hardcoding 1024 here was clipping qwen3.5 mid-<think>, leaving
        // unterminated reasoning that leaked '<think>' into chat.
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
        room_id: Some(input.room_id.to_string()),
        purpose: Some("persona-respond".to_string()),
        // The whole point of this request is to generate a response on
        // behalf of THIS persona — its KV bytes belong in this persona's
        // attribution bucket. Adapters that honor persona_id (LlamaCpp)
        // route the seq slot's KV into the FootprintRegistry under this
        // id; adapters that don't (DMR, cloud) ignore it.
        persona_id: Some(input.persona.persona_id.to_string()),
    };

    // 4. Pick an adapter via the global registry — capability-routed,
    //    no hardcoded provider name. "local" + Gpu = "best available
    //    GPU adapter that honestly supports the requested model".
    let registry_arc = crate::modules::ai_provider::global_registry();
    let registry = registry_arc.read().await;
    let (_provider_id, adapter) = registry
        .select(Some("local"), Some(&input.model), InferenceDevice::Gpu)
        .ok_or_else(|| {
            format!(
                "no GPU adapter supports model '{}' (registered: {:?}). \
                 Pull into DMR or install the right backend.",
                input.model,
                registry.available()
            )
        })?;

    let response = adapter.generate_text(request).await?;

    Ok(RawRenderOutput {
        text: response.text,
        model_used: response.model,
    })
}

/// Extract `<think>...</think>` blocks from the model's output. Emits
/// Convert assembled prompt messages into `ChatMessage`s, attaching any
/// caller-supplied `MediaItemLite`s as `ContentPart::Image`/`Audio` on
/// the FINAL user-role message — but only when the persona's resolved
/// model declares the matching capability (`Vision` for image,
/// `AudioInput` for audio). Native-multimodal models receive the source
/// bytes directly; text-only models fall back to the simple text path
/// (the sensory bridge would inject a description upstream — its job,
/// not ours).
///
/// Behavior contract:
///   - empty `media` → identical to the legacy text-only path.
///   - non-empty `media` + model has Vision/AudioInput → last user
///     message becomes `MessageContent::Parts(text + media)`.
///   - non-empty `media` + model lacks the capability → text-only
///     path; the bridge layer (VisionDescriptionService etc.) is
///     expected to have already converted media → text upstream.
///   - `media` items whose `item_type` doesn't match a capability the
///     model has are dropped (e.g. audio sent to a vision-only model).
///   - no user-role messages found → media silently dropped (rare —
///     would mean the assembler produced an unusual shape).
pub fn build_messages_with_media(
    prompt_messages: Vec<crate::persona::prompt_assembly::PromptMessage>,
    media: &[MediaItemLite],
    model_caps: &std::collections::HashSet<crate::model_registry::Capability>,
) -> Vec<crate::ai::types::ChatMessage> {
    use crate::ai::types::{AudioInput, ChatMessage, ContentPart, ImageInput, MessageContent};
    use crate::persona::media_policy::MediaPolicy;

    // Default text-only path. Always start here; we may rewrite the
    // last user message below if the policy chose an attachable item.
    let mut messages: Vec<ChatMessage> = prompt_messages
        .into_iter()
        .map(|m| ChatMessage {
            role: m.role,
            content: MessageContent::Text(m.content),
            name: None,
        })
        .collect();

    if media.is_empty() {
        return messages;
    }

    // Apply the AT-MOST-ONE-LATEST policy. The byte-attachment slot
    // is exclusive — at most one media item ever rides as bytes per
    // inference call, and it's the LATEST item the model can natively
    // consume. Everything else (older items, items the model can't
    // natively consume) becomes a text description marker. This is
    // the architectural guard against the multi-encoder Metal brick
    // (each per-call mtmd context allocates ~2 GB; two concurrent
    // image attachments = two concurrent encoder ops = mouse-frozen
    // hard reset). See `persona/media_policy.rs` for the rule + tests.
    //
    // Joel rule (2026-04-22): "i would never let more than ONE message
    // deliver an image or tell the ais the image link". The policy
    // makes that rule a typed value, not a `for` loop.
    let plan = MediaPolicy::AtMostOneLatest.plan(media, model_caps);

    let mut emitted_parts: Vec<ContentPart> = Vec::with_capacity(plan.descriptions.len() + 1);

    // Bytes slot first (when present). Marker placement: the byte
    // attachment goes BEFORE description markers so the model
    // encounters the real sensory input before any text fallback for
    // older media. mtmd_tokenize splices the model's media marker at
    // ContentPart::Image position; description markers are inert.
    if let Some(item) = plan.attachable {
        let part = match item.item_type.as_str() {
            "image" => ContentPart::Image {
                image: ImageInput {
                    url: None,
                    base64: item.base64.clone(),
                    mime_type: item.mime_type.clone(),
                },
            },
            "audio" => ContentPart::Audio {
                audio: AudioInput {
                    url: None,
                    base64: item.base64.clone(),
                    mime_type: item.mime_type.clone(),
                },
            },
            // Policy guarantees attachable is natively-supported, so
            // any other branch is a contract violation. Falling
            // through silently would resurrect the silent-drop bug
            // we're refactoring away — make it loud instead.
            other => unreachable!(
                "MediaPolicy returned attachable item with unsupported type '{other}' — \
                 is_natively_supported is out of sync with the ContentPart variants here"
            ),
        };
        emitted_parts.push(part);
    }

    // Description markers for everything else. Pre-computed
    // `description` (from the upstream sensory bridge) gets used when
    // present; otherwise a do-not-speculate marker signals "an
    // attachment exists, you can't see it, do not invent content".
    // The marker is deliberately unhelpful — we don't want text-only
    // models inventing details from prompt context (verified
    // 2026-04-21: text-only personas hallucinated "kitten upright and
    // alert" given zero info, dropped into loop-spam patterns).
    for item in &plan.descriptions {
        let other = item.item_type.as_str();
        let text = match item.description.as_deref() {
            Some(d) if !d.trim().is_empty() => format!("[Attached {other}: {d}]"),
            _ => format!(
                "[Attached {other} — no description available; \
                 do not describe or speculate about its contents]"
            ),
        };
        emitted_parts.push(ContentPart::Text { text });
    }

    if emitted_parts.is_empty() {
        return messages;
    }

    // Find the LAST user-role message and convert it to Parts (text +
    // attached media). The current message is always the last user
    // turn after assemble().
    let last_user_idx = messages.iter().rposition(|m| m.role == "user");
    let Some(idx) = last_user_idx else {
        // No user message to attach to. Drop media silently — caller
        // shape was unusual; assembling new user messages here would
        // hide the actual bug.
        return messages;
    };

    let existing_text = match &messages[idx].content {
        MessageContent::Text(t) => t.clone(),
        // Defensive: if the assembler somehow already produced Parts,
        // we don't try to merge — leave it alone.
        MessageContent::Parts(_) => return messages,
    };

    let mut parts: Vec<ContentPart> = Vec::with_capacity(emitted_parts.len() + 1);
    if !existing_text.is_empty() {
        parts.push(ContentPart::Text {
            text: existing_text,
        });
    }
    parts.extend(emitted_parts);
    messages[idx].content = MessageContent::Parts(parts);
    messages
}

/// each as a `cognition:think-block` event for the (future) hippocampus
/// to consume. Returns the cleaned visible text + the count of blocks
/// emitted (for telemetry).
///
/// A.6: this is the strip-AND-emit pair. Stripping kills the persona
/// feedback loop where personas re-analyzed each other's working
/// memory; emitting preserves the trace for memory consolidation.
///
/// Today: the event surface is observable for debugging; nothing
/// listens. Tomorrow: hippocampus subscribes and turns each think
/// block into a long-term memory entity.
fn strip_thinks_emit_events(raw: &str, persona_id: Uuid, message_id: Uuid) -> (String, u32) {
    // Match <think>...</think> non-greedy across newlines. Standalone
    // helper kept simple; if think blocks ever start nesting (they
    // don't today), this needs to grow.
    let mut visible = String::with_capacity(raw.len());
    let mut count: u32 = 0;
    let mut cursor = 0usize;
    let bytes = raw.as_bytes();
    while cursor < bytes.len() {
        if let Some(open_off) = find_at(bytes, cursor, b"<think>") {
            // Append everything before the open tag to visible.
            visible.push_str(&raw[cursor..open_off]);
            let after_open = open_off + b"<think>".len();
            if let Some(close_off) = find_at(bytes, after_open, b"</think>") {
                let think_text = &raw[after_open..close_off];
                emit_think_block(persona_id, message_id, think_text);
                count = count.saturating_add(1);
                cursor = close_off + b"</think>".len();
            } else {
                // Unterminated <think> — keep raw as visible to avoid
                // losing data; log + continue. Rare: model truncated
                // mid-think due to max_tokens.
                visible.push_str(&raw[open_off..]);
                break;
            }
        } else {
            // No more think blocks — append the tail.
            visible.push_str(&raw[cursor..]);
            break;
        }
    }
    // Cleanup: collapse leading/trailing whitespace introduced by
    // adjacent strips. Preserve internal formatting otherwise.
    (visible.trim().to_string(), count)
}

fn find_at(haystack: &[u8], from: usize, needle: &[u8]) -> Option<usize> {
    if from >= haystack.len() {
        return None;
    }
    haystack[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|p| p + from)
}

/// Emit a `cognition:think-block` event so the (future) hippocampus
/// can subscribe and consolidate.
///
/// **STUB** — wired during chat-path integration. Will go through the
/// existing event-broadcast mechanism (TBD: confirm path during
/// integration; either ServiceModule's event channel or the SSE/IPC
/// broadcast surface). Today: writes to the cognition log so the
/// blocks are observable for debugging.
fn emit_think_block(persona_id: Uuid, message_id: Uuid, think_text: &str) {
    // TODO(A.6 integration): replace with real event emission via the
    // existing broadcast channel. Tracing log is the temporary
    // observable surface — hippocampus subscribers will be wired in
    // the dedicated migration PR.
    tracing::debug!(
        target: "cognition::think_block",
        persona_id = %persona_id,
        message_id = %message_id,
        think_text_len = think_text.len(),
        "captured think block (event emission TBD)"
    );
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    //! Pure-logic tests for the parts that don't require live inference.
    //! End-to-end inference test happens through chat-validation.
    use super::*;

    #[test]
    fn strip_thinks_extracts_single_block() {
        let raw = "<think>I should be helpful</think>Hello there.";
        let (visible, count) = strip_thinks_emit_events(raw, Uuid::nil(), Uuid::nil());
        assert_eq!(visible, "Hello there.");
        assert_eq!(count, 1);
    }

    #[test]
    fn strip_thinks_extracts_multiple_blocks() {
        let raw = "<think>plan</think>First sentence. <think>double-check</think>Second.";
        let (visible, count) = strip_thinks_emit_events(raw, Uuid::nil(), Uuid::nil());
        assert_eq!(visible, "First sentence. Second.");
        assert_eq!(count, 2);
    }

    #[test]
    fn strip_thinks_handles_multiline_thinks() {
        let raw = "<think>\nLine one\nLine two\n</think>\nVisible response.";
        let (visible, count) = strip_thinks_emit_events(raw, Uuid::nil(), Uuid::nil());
        assert_eq!(visible, "Visible response.");
        assert_eq!(count, 1);
    }

    #[test]
    fn strip_thinks_no_block_returns_unchanged() {
        let raw = "Just a normal response with no thinking.";
        let (visible, count) = strip_thinks_emit_events(raw, Uuid::nil(), Uuid::nil());
        assert_eq!(visible, "Just a normal response with no thinking.");
        assert_eq!(count, 0);
    }

    #[test]
    fn strip_thinks_unterminated_keeps_text() {
        // Model truncated mid-think (rare but real). Don't lose data.
        let raw = "<think>This was cut off because max_tokens";
        let (visible, count) = strip_thinks_emit_events(raw, Uuid::nil(), Uuid::nil());
        assert!(visible.contains("<think>"));
        assert_eq!(count, 0);
    }

    // ─── Native multimodal helper tests ─────────────────────────────
    //
    // build_messages_with_media is the convergence point for sensory
    // inputs. These tests pin its contract — no media → text path
    // unchanged; media + capability → ContentPart::Image/Audio
    // attached to the LAST user message; media without capability →
    // text path (the bridge is upstream's job, not ours).

    use crate::ai::types::{ContentPart, MessageContent};
    use crate::cognition::tool_executor::types::MediaItemLite;
    use crate::model_registry::Capability;
    use crate::persona::prompt_assembly::PromptMessage;
    use std::collections::HashSet;

    fn pm(role: &str, text: &str) -> PromptMessage {
        PromptMessage {
            role: role.to_string(),
            content: text.to_string(),
        }
    }

    fn img_b64(b64: &str) -> MediaItemLite {
        MediaItemLite {
            item_type: "image".to_string(),
            base64: Some(b64.to_string()),
            mime_type: Some("image/png".to_string()),
            description: None,
        }
    }

    /// What this catches: empty media short-circuit ever rewriting
    /// the message shape into Parts. Without media, the text-only
    /// path must remain byte-for-byte identical to before this
    /// feature landed — otherwise we silently regress every existing
    /// caller.
    ///
    /// Validated 2026-04-21: removed the `if media.is_empty() return`
    /// early-exit so the function falls through to the parts-building
    /// branch with empty supported_parts; test passes trivially because
    /// supported_parts.is_empty() also returns the text path. So the
    /// short-circuit is redundant for correctness but reduces work.
    /// Stronger mutation: changed the text-path map to wrap in Parts
    /// instead of Text; test fails on the assert_eq with MessageContent::Text.
    /// Reverted.
    #[test]
    fn no_media_returns_text_only_messages() {
        let prompt = vec![pm("system", "you are helpful"), pm("user", "hello")];
        let caps = HashSet::new();
        let out = build_messages_with_media(prompt, &[], &caps);
        assert_eq!(out.len(), 2);
        assert!(matches!(out[0].content, MessageContent::Text(_)));
        assert!(matches!(out[1].content, MessageContent::Text(_)));
    }

    /// What this catches: media present but model lacks Vision —
    /// we MUST NOT attach the image. The bridge layer
    /// (VisionDescriptionService) is responsible for converting
    /// media→text upstream for incapable models; if we attached
    /// raw image parts to a text-only model the inference call
    /// would fail at the adapter or be silently ignored.
    ///
    /// Validated 2026-04-21: removed the `model_caps.contains(...)`
    /// guard from the image branch (always emit ContentPart::Image),
    /// test fails because supported_parts is non-empty for a
    /// no-capability model and the user message becomes Parts;
    /// reverted.
    #[test]
    fn media_dropped_when_model_lacks_capability() {
        let prompt = vec![pm("user", "describe this")];
        let media = vec![img_b64("AAAA")];
        let caps = HashSet::new(); // model has NO Vision capability
        let out = build_messages_with_media(prompt, &media, &caps);
        assert_eq!(out.len(), 1);
        // New contract (2026-04-22): when model lacks the matching
        // capability, ContentPart::Image bytes MUST NOT attach. The
        // wrapper MAY be MessageContent::Parts(...) containing
        // ContentPart::Text description markers — that's an
        // improvement over silently dropping the attachment, because
        // the model now knows "an image was attached" without being
        // shown bytes it can't process.
        let has_image_bytes = match &out[0].content {
            MessageContent::Text(_) => false,
            MessageContent::Parts(parts) => parts
                .iter()
                .any(|p| matches!(p, ContentPart::Image { .. })),
        };
        assert!(
            !has_image_bytes,
            "image bytes MUST NOT attach when model lacks Vision capability — got: {:?}",
            out[0].content
        );
    }

    /// What this catches: with media + Vision capability, the LAST
    /// user message MUST become MessageContent::Parts containing
    /// the original text + a ContentPart::Image carrying the base64
    /// payload. Native sight on natively-capable models is the
    /// thesis (per Joel 2026-04-21 + README "Full embodiment");
    /// failing this means we silently revert to bridging.
    ///
    /// Validated 2026-04-21: changed Capability::Vision to
    /// Capability::AudioInput in the image branch's match, test
    /// fails because supported_parts is empty for a Vision-only
    /// model and the user message stays as Text; reverted.
    #[test]
    fn vision_model_receives_native_image_part() {
        let prompt = vec![
            pm("system", "you describe images"),
            pm("user", "what is this?"),
        ];
        let media = vec![img_b64("PNG_BASE64_DATA")];
        let mut caps = HashSet::new();
        caps.insert(Capability::Vision);
        let out = build_messages_with_media(prompt, &media, &caps);
        assert_eq!(out.len(), 2);
        // System message untouched.
        assert!(matches!(out[0].content, MessageContent::Text(_)));
        // User message converted to Parts(text + image).
        let parts = match &out[1].content {
            MessageContent::Parts(p) => p,
            _ => panic!("expected Parts on user message"),
        };
        assert_eq!(parts.len(), 2);
        match &parts[0] {
            ContentPart::Text { text } => assert_eq!(text, "what is this?"),
            _ => panic!("first part should be the original text"),
        }
        match &parts[1] {
            ContentPart::Image { image } => {
                assert_eq!(image.base64.as_deref(), Some("PNG_BASE64_DATA"));
                assert_eq!(image.mime_type.as_deref(), Some("image/png"));
            }
            _ => panic!("second part should be the image"),
        }
    }

    /// What this catches: media attaches to the LAST user-role
    /// message, not the first or to a system message. With
    /// multi-turn history the most recent user turn carries the
    /// current message + the image the user just shared.
    ///
    /// Validated 2026-04-21: changed `messages.iter().rposition` to
    /// `position` (first instead of last), test fails because the
    /// FIRST user message gets the image instead of the last;
    /// reverted.
    #[test]
    fn image_attaches_to_last_user_turn_not_first() {
        let prompt = vec![
            pm("user", "earlier turn"),
            pm("assistant", "earlier reply"),
            pm("user", "current turn"),
        ];
        let media = vec![img_b64("X")];
        let mut caps = HashSet::new();
        caps.insert(Capability::Vision);
        let out = build_messages_with_media(prompt, &media, &caps);
        // First user message stays text.
        match &out[0].content {
            MessageContent::Text(t) => assert_eq!(t, "earlier turn"),
            _ => panic!("first user turn should remain text"),
        }
        // Last user message becomes Parts.
        match &out[2].content {
            MessageContent::Parts(p) => {
                assert!(
                    p.iter().any(|x| matches!(x, ContentPart::Image { .. })),
                    "last user turn should carry the image"
                );
            }
            _ => panic!("last user turn should be Parts"),
        }
    }

    /// What this catches: audio attachment requires the AudioInput
    /// capability — Vision alone does NOT permit audio. Each modality
    /// has its own capability gate; no cross-bleed.
    ///
    /// Validated 2026-04-21: changed `Capability::AudioInput` to
    /// `Capability::Vision` in the audio match arm, test fails
    /// because vision-only model wrongly receives audio; reverted.
    #[test]
    fn audio_requires_audio_input_capability() {
        let prompt = vec![pm("user", "what did i say")];
        let audio = MediaItemLite {
            item_type: "audio".to_string(),
            base64: Some("WAV_DATA".to_string()),
            mime_type: Some("audio/wav".to_string()),
            description: None,
        };
        let mut vision_only = HashSet::new();
        vision_only.insert(Capability::Vision);
        let out = build_messages_with_media(prompt.clone(), &[audio.clone()], &vision_only);
        // Vision-only model: audio bytes MUST NOT attach. Wrapper MAY
        // be Parts(Text-marker) per the new policy contract — what
        // matters is no ContentPart::Audio carrying real bytes.
        let has_audio_bytes = match &out[0].content {
            MessageContent::Text(_) => false,
            MessageContent::Parts(parts) => parts
                .iter()
                .any(|p| matches!(p, ContentPart::Audio { .. })),
        };
        assert!(
            !has_audio_bytes,
            "audio bytes MUST NOT attach when model lacks AudioInput capability — got: {:?}",
            out[0].content
        );

        let mut audio_capable = HashSet::new();
        audio_capable.insert(Capability::AudioInput);
        let out = build_messages_with_media(prompt, &[audio], &audio_capable);
        // Audio-capable model: audio attaches.
        match &out[0].content {
            MessageContent::Parts(p) => {
                assert!(p.iter().any(|x| matches!(x, ContentPart::Audio { .. })));
            }
            _ => panic!("audio-capable model should receive Parts"),
        }
    }
}
