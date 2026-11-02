//! The per-turn persona response contract: `RespondInput` (what a mind's turn
//! carries into prompt assembly), `PersonaResponse` (what a turn produced, the
//! shape the recorder writes and the dreaming dataset reads), and
//! `build_messages_with_media` (attaching native media to the final user turn
//! when the model can perceive it).

use crate::cognition::tool_executor::types::MediaItemLite;
use crate::cognition::PersonaSlot;
use crate::persona::turn_context::TurnContext;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use ts_rs::TS;
use uuid::Uuid;

/// One persona's per-turn input. Carries everything needed for ONE
/// persona's response cycle; the per-turn shared context is an `Arc`.
#[derive(Debug, Clone)]
pub struct RespondInput {
    /// THIS persona's identity + specialty for scoring.
    pub persona: PersonaSlot,
    /// Per-turn shared context (room_id + recent_history +
    /// known_specialties). All personas responding to the same
    /// message share an `Arc` to the same `TurnContext` instance —
    /// no per-persona deep clone of the same data (continuum#1206).
    pub turn_context: Arc<TurnContext>,
    pub message_id: Uuid,
    /// The new message that triggered this response cycle.
    pub message_text: String,
    /// Display names of OTHER personas in the room (excluding self).
    /// Forwarded to `prompt_assembly` so the
    /// `ProperChatMlSingleParty` strategy can drop other-AI history
    /// turns that single-party-trained models cannot coherently
    /// process. Empty when the host doesn't expose a roster or when
    /// the active model uses a strategy that doesn't need it
    /// (`NamePrefixedUserTurns` ignores).
    pub other_persona_names: Vec<String>,
    /// Persona's RAG-built identity / system prompt. Caller-supplied
    /// because the persona's identity comes from RAG (which knows the
    /// persona entity, the active adapters, the user-personalization
    /// bits).
    pub system_prompt: String,
    /// THIS persona's own (potentially LoRA-adapted) model identifier.
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
    /// Recalled engrams (per-persona admitted memory) injected as
    /// system-prompt context (continuum#1211 PR-2). The IPC layer
    /// pulls these from `AdmissionState::recall_recent` after the
    /// inline admission gate runs, then passes them through so
    /// `prompt_assembly` can render them as a `[Recent Memory]`
    /// section. Empty when the persona has no admission state OR no
    /// admitted engrams yet — both are normal early-life states and
    /// neither blocks the response cycle.
    ///
    /// Per-persona (each persona's admission store is independent)
    /// so this lives on `RespondInput`, not the per-turn-shared
    /// `TurnContext` (#1206) — different personas in the same room
    /// recall different memory.
    ///
    /// `String` (the engram's content text) rather than `Engram`
    /// because prompt_assembly only needs the text. Keeping the full
    /// `Engram` type out of this layer means a future structural
    /// change to engrams (kind enum, embeddings, recall_keys reshape)
    /// doesn't ripple into the prompt path.
    pub recalled_engrams: Vec<String>,
    /// Roster of OTHER citizens currently present in the room — one
    /// pre-formatted line per peer (`name [runtime] — availability`),
    /// produced by `RoomRosterSource` from airc `active_agents` and
    /// projected here by the service loop. Rendered by `prompt_assembly`
    /// as a `[Present in this room]` block so the persona is grounded in
    /// who is present and who is NOT itself — the fix for the persona
    /// confabulating other citizens' turns (see
    /// docs/grid/AIRC-NATIVE-IDENTITY-ROOMS-SECURITY.md §5 slice 1).
    /// Empty when no roster source is bound or the room is otherwise
    /// quiet — backwards-compatible (no block rendered).
    pub room_roster: Vec<String>,
    /// The room's operating doctrine — the airc-published contract for
    /// what kind of activity this room is (chat vs coordination vs game
    /// vs …), produced by `RoomDoctrineSource` and projected here by the
    /// service loop. Rendered by `prompt_assembly` as a `[Room operating
    /// doctrine]` block so the persona calibrates participation to the
    /// room's nature (slice 2). `None` when the room has no published
    /// doctrine — backwards-compatible (no block rendered).
    pub room_doctrine: Option<String>,
}

/// What a persona's turn produced.
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
    export_to = "../../../protocol/typescript/cognition/PersonaResponse.ts"
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
    prompt_messages: Vec<crate::cognition::prompt_assembly::PromptMessage>,
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

#[cfg(test)]
mod tests;
