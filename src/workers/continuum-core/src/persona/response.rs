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
//!   1. cognition::analyze(...)   — shared, cached. Run once per
//!                                  message; this persona's call hits
//!                                  the cache after the first.
//!   2. cognition::score_persona(...) — local. Just THIS persona's
//!                                       relevance; no need to know
//!                                       about others.
//!   3. If !should_respond → return Silent { reason }. First-class
//!      outcome — silence with an observable reason, not a hidden skip.
//!   4. prompt_assembly::build(...) — persona-specific prompt: voice,
//!                                    LoRA-rendered specialty, RAG
//!                                    context interleaving. (TODO:
//!                                    memento's persona/prompt_assembly
//!                                    module ships in this PR.)
//!   5. ai_provider::generate_text(...) — inference (DMR or whatever
//!                                        adapter the registry picks).
//!   6. strip_thinks_emit_events(...) — extract <think>...</think>
//!                                       blocks, emit them as
//!                                       cognition:think-block events
//!                                       for the (future) hippocampus
//!                                       to consume, return clean
//!                                       speech for posting.
//!   7. Return Spoke { text, ... } with timing + diagnostic fields.
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
use crate::cognition::types::ResponderDecision;
use crate::cognition::{
    AnalysisInput, DEFAULT_RELEVANCE_THRESHOLD, PersonaSlot, RecentMessage, SharedAnalysis,
    analyze, score_persona,
};
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
    /// present AND the persona's resolved model has the matching
    /// `Capability` (`Vision` for images, `AudioInput` for audio), the
    /// render path constructs `MessageContent::Parts` with a real
    /// `ContentPart::Image`/`Audio` instead of `MessageContent::Text` —
    /// preserving the natively-multimodal model's ability to see / hear
    /// directly. **No text-description bridging when the model IS
    /// capable** — that's the regression Joel called out 2026-04-21.
    /// Bridge layer (VisionDescriptionService) remains for genuinely
    /// text-only models as the floor, not the default.
    /// See docs/architecture/PERSONA-CONTEXT-PAGING.md §0.5.X.
    pub message_media: Vec<MediaItemLite>,
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
    let total_start = now_ms();

    // 1. Shared analysis (cached per message+room+history fingerprint).
    let analysis = analyze(AnalysisInput {
        message_id: input.message_id,
        room_id: input.room_id,
        text: input.message_text.clone(),
        recent_history: input.recent_history.clone(),
        known_specialties: input.known_specialties.clone(),
    })
    .await?;

    // 2. Local score for THIS persona only. No need to know about others.
    let decision = score_persona(&analysis, &input.persona, DEFAULT_RELEVANCE_THRESHOLD);

    // 3. Silent path is first-class.
    if !decision.should_respond {
        return Ok(PersonaResponse::Silent {
            persona_id: input.persona.persona_id,
            reason: decision.explanation,
            relevance_score: decision.relevance_score,
        });
    }

    // 4–6. Build prompt, run inference, parse <think>.
    //
    // The prompt-assembly + inference + parse work is the next chunk
    // of this PR. Memento is taking persona/prompt_assembly.rs (port
    // of PersonaPromptAssembler.ts logic). My piece here calls into
    // his module + ai_provider + strip_thinks_emit_events.
    //
    // Stubbed for now so this file compiles + the shape is reviewable.
    // Will be filled in (no port debt — this is the final-form code,
    // just incomplete) before the chat-validation gate.
    let inference_start = now_ms();
    let raw_response = run_render(&input, &analysis, &decision).await?;
    let inference_ms = now_ms().saturating_sub(inference_start);

    let (visible_text, think_count) = strip_thinks_emit_events(
        &raw_response.text,
        input.persona.persona_id,
        input.message_id,
    );

    Ok(PersonaResponse::Spoke {
        persona_id: input.persona.persona_id,
        text: visible_text,
        model_used: raw_response.model_used,
        inference_ms,
        total_ms: now_ms().saturating_sub(total_start),
        think_blocks_emitted: think_count,
    })
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
    _decision: &ResponderDecision,
) -> Result<RawRenderOutput, String> {
    use crate::ai::adapter::InferenceDevice;
    use crate::ai::types::TextGenerationRequest;
    use crate::persona::prompt_assembly::{HistoryMessage, PromptAssemblyInput, assemble};

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
    let model_caps: std::collections::HashSet<crate::model_registry::Capability> =
        crate::model_registry::try_global()
            .and_then(|reg| reg.model(&input.model))
            .map(|m| m.capabilities.iter().copied().collect())
            .unwrap_or_default();
    let messages = build_messages_with_media(assembled.messages, &input.message_media, &model_caps);

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
fn build_messages_with_media(
    prompt_messages: Vec<crate::persona::prompt_assembly::PromptMessage>,
    media: &[MediaItemLite],
    model_caps: &std::collections::HashSet<crate::model_registry::Capability>,
) -> Vec<crate::ai::types::ChatMessage> {
    use crate::ai::types::{AudioInput, ChatMessage, ContentPart, ImageInput, MessageContent};
    use crate::model_registry::Capability;

    // Default text-only path. Always start here; we may rewrite the
    // last user message below if media + capability align.
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

    // Walk media items per persona-capability and emit:
    //   - vision-capable persona + image → ContentPart::Image (raw bytes)
    //   - audio-capable persona + audio → ContentPart::Audio (raw bytes)
    //   - text-only persona + media → ContentPart::Text with the
    //     pre-computed `description` from the upstream sensory bridge,
    //     or a `[MEDIA: <type>, no description available]` marker if
    //     the bridge didn't run (so the model knows something is there
    //     and doesn't hallucinate from prompt context — verified
    //     2026-04-21 with cat photo: text-only personas hallucinated
    //     "kitten upright and alert" when given zero info, dropped
    //     into loop-spam patterns when prompt context dominated).
    //
    // The marker for missing-description is deliberately unhelpful —
    // we don't want models inventing details. Pre-populating
    // `MediaItemLite.description` from VisionDescriptionService at
    // the TS chat-send step is the proper fix; this fallback exists
    // so a missed bridge call doesn't silently produce hallucinated
    // "vision" responses.
    let mut emitted_parts: Vec<ContentPart> = Vec::with_capacity(media.len());
    for m in media.iter() {
        let part = match m.item_type.as_str() {
            "image" if model_caps.contains(&Capability::Vision) => ContentPart::Image {
                image: ImageInput {
                    url: None,
                    base64: m.base64.clone(),
                    mime_type: m.mime_type.clone(),
                },
            },
            "audio" if model_caps.contains(&Capability::AudioInput) => ContentPart::Audio {
                audio: AudioInput {
                    url: None,
                    base64: m.base64.clone(),
                    mime_type: m.mime_type.clone(),
                },
            },
            // Text-only persona OR unsupported modality → emit text
            // description from the bridge if we have one, else a
            // marker that signals "an attachment exists, you can't
            // see it, do not invent content".
            other => {
                let text = match m.description.as_deref() {
                    Some(d) if !d.trim().is_empty() => format!("[Attached {other}: {d}]"),
                    _ => format!(
                        "[Attached {other} — no description available; \
                         do not describe or speculate about its contents]"
                    ),
                };
                ContentPart::Text { text }
            }
        };
        emitted_parts.push(part);
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
        match &out[0].content {
            MessageContent::Text(t) => assert_eq!(t, "describe this"),
            _ => panic!("expected Text content for capability-less model, got Parts"),
        }
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
        };
        let mut vision_only = HashSet::new();
        vision_only.insert(Capability::Vision);
        let out = build_messages_with_media(prompt.clone(), &[audio.clone()], &vision_only);
        // Vision-only model: audio must NOT attach (no AudioInput cap).
        assert!(matches!(out[0].content, MessageContent::Text(_)));

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
