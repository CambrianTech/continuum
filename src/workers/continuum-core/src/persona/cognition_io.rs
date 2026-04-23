//! Cognition I/O — value objects describing inputs to the cognition
//! layer.
//!
//! `Signal` is the host's raw event (chat message, video frame, code
//! diff, game tick, autonomous-loop poke). `PersonaContext` is the
//! per-persona stable state (identity, model, capabilities, history,
//! room membership). `build_respond_input` projects these into the
//! `RespondInput` the cognition layer consumes.
//!
//! # Why this is data + a free function (not a trait)
//!
//! Earlier shape: a `Recipe` trait with per-domain implementations
//! (`ChatRecipe`, `VisionRecipe`, …) baked into Rust. That shape was
//! wrong for two reasons:
//!
//! 1. Recipes are data. They live as JSON `RecipeEntity` rows,
//!    authored by users / AIs / shared via the grid. Hardcoding a
//!    Rust trait for each domain is the kernel-level commands +
//!    data-driven recipes anti-pattern (CLAUDE.md): commands are
//!    primitives, recipes are the data the executor walks.
//! 2. The projection from `(Signal, PersonaContext) → RespondInput`
//!    is one canonical mapping, not a per-domain one. Earlier
//!    `Recipe::build_input` implementations all did the same
//!    field-by-field projection with minor signal-kind validation
//!    in front. Wrapping that in a trait inflated it.
//!
//! The Rust-native pipeline executor (designed in
//! `docs/architecture/RECIPE-EXECUTION-RUNTIME.md`) walks recipe
//! data, dispatches kernel commands, and uses these value objects
//! to feed the cognition layer at the appropriate pipeline step.
//! That's the right shape; this file contains the value objects and
//! the canonical projection used by the executor.

use crate::cognition::tool_executor::types::MediaItemLite;
use crate::cognition::PersonaSlot;
use crate::cognition::RecentMessage;
use crate::model_registry::Capability;
use crate::persona::response::RespondInput;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

// ─── Signal ──────────────────────────────────────────────────────────

/// Hint about what kind of event produced this signal. The pipeline
/// executor may use it for routing decisions (e.g., a game pipeline
/// only acts on `FrameUpdate` or `AutonomousTick`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/recipe/SignalKind.ts")]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SignalKind {
    /// Chat message authored by a user or a persona in a room.
    ChatMessage,
    /// Tool/sentinel completion event — recipe may want to react to
    /// the result.
    ToolResult { tool_name: String },
    /// Tick from the autonomous loop — no external trigger, recipe
    /// decides if there's anything to do.
    AutonomousTick,
    /// Game / AR engine frame update.
    FrameUpdate,
    /// File / diff context for code work.
    CodeContext,
    /// Open-vocab kind for host extensions Rust hasn't seen.
    Custom { name: String },
}

/// Who emitted the signal — used for system-prompt composition + for
/// pipelines that filter by originator (e.g., a recipe step that
/// only responds to humans, not other personas).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/recipe/SignalOriginator.ts"
)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum SignalOriginator {
    User {
        #[ts(type = "string")]
        user_id: Uuid,
    },
    Persona {
        #[ts(type = "string")]
        persona_id: Uuid,
    },
    Tool {
        tool_name: String,
    },
    GameEngine,
    System,
}

/// Input to the cognition layer — the host's raw event, pre-cognition.
/// Open enough that ANY domain (chat, voice, video, code, game, AR)
/// emits the same shape.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../shared/generated/recipe/Signal.ts")]
#[serde(rename_all = "camelCase")]
pub struct Signal {
    /// Hint about the signal's nature. The pipeline executor uses it
    /// for routing decisions.
    pub kind: SignalKind,
    /// Text payload of the signal. Empty when purely media-driven
    /// (video frame, scene-graph blob without commentary).
    pub text: String,
    /// Attached media (images, audio, video frames, scene-graph blobs).
    /// Empty for pure-text signals.
    pub media: Vec<MediaItemLite>,
    /// Who emitted the signal.
    pub originator: SignalOriginator,
    /// Wall-clock time the signal was created (ms since UNIX_EPOCH).
    #[ts(type = "number")]
    pub timestamp_ms: u64,
    /// Optional message / event ID. Used for joining captures with
    /// host-side records (chat message ID, frame number, etc.).
    #[ts(optional, type = "string")]
    pub message_id: Option<Uuid>,
}

// ─── PersonaContext ──────────────────────────────────────────────────

/// Per-persona stable state needed by every cognition turn — identity,
/// model, capabilities, recent history, room membership. Built once
/// per turn by the host and handed to the executor; the executor and
/// the cognition layer must not mutate it.
///
/// Capabilities are `Vec<Capability>` on the wire (ts-rs friendlier
/// than HashSet); the projection converts to a HashSet at use site
/// for O(1) membership checks. Conversion happens once per
/// `build_respond_input` call — negligible vs the inference work
/// that follows.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../shared/generated/recipe/PersonaContext.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct PersonaContext {
    #[ts(type = "string")]
    pub persona_id: Uuid,
    pub display_name: String,
    pub specialty: String,
    /// The persona's render-time model id. Recipes use it directly
    /// (no global lookup); single source of truth.
    pub model: String,
    /// Resolved capability vocabulary for the persona's model. Caller
    /// declares; Rust consumes. Recipe steps may switch behavior on
    /// cap presence (vision-tagged step checks for `Capability::Vision`).
    pub capabilities: Vec<Capability>,
    /// Persona's RAG-built identity / system prompt.
    pub system_prompt: String,
    /// Recent conversation history (most-recent last). May be empty
    /// for recipes that don't use chat history (game pipelines).
    pub recent_history: Vec<RecentMessage>,
    /// Specialty identifiers in the room (for shared analysis).
    pub known_specialties: Vec<String>,
    /// Optional room id — present for chat-room recipes, absent for
    /// game/AR/embedded hosts that have no concept of "room".
    #[ts(optional, type = "string")]
    pub room_id: Option<Uuid>,
    /// Live-voice context flag — affects prompt assembly response
    /// style. Default false for non-voice signals.
    pub is_voice: bool,
}

impl PersonaContext {
    /// Build the `PersonaSlot` the cognition layer expects from this
    /// context. Convenience so the projection doesn't repeat the
    /// field copy.
    pub fn slot(&self) -> PersonaSlot {
        PersonaSlot {
            persona_id: self.persona_id,
            specialty: self.specialty.clone(),
            display_name: self.display_name.clone(),
        }
    }
}

// ─── Projection ──────────────────────────────────────────────────────

/// Project `(Signal, PersonaContext)` into the cognition layer's
/// `RespondInput`. The canonical mapping every chat-shaped pipeline
/// step uses; future non-chat pipelines (game action, AR scene
/// update) will use different projection functions tied to their
/// step kind.
///
/// Returns `Err` when the signal kind is unusable for the chat-
/// shaped projection (a `FrameUpdate` or `CodeContext` routed to a
/// chat-cognition step is a host bug — surface it loudly here, not
/// as silently-wrong cognition output downstream).
pub fn build_respond_input(
    signal: &Signal,
    ctx: &PersonaContext,
) -> Result<RespondInput, String> {
    match &signal.kind {
        SignalKind::ChatMessage
        | SignalKind::AutonomousTick
        | SignalKind::Custom { .. } => {}
        other => {
            return Err(format!(
                "build_respond_input: SignalKind::{:?} not supported by the \
                 chat-shaped cognition projection — route to the matching \
                 pipeline step (vision for image-bearing, code for \
                 CodeContext, etc.)",
                other
            ));
        }
    }

    let message_id = signal.message_id.unwrap_or(Uuid::nil());
    let room_id = ctx.room_id.unwrap_or(Uuid::nil());

    Ok(RespondInput {
        persona: ctx.slot(),
        room_id,
        message_id,
        message_text: signal.text.clone(),
        recent_history: ctx.recent_history.clone(),
        known_specialties: ctx.known_specialties.clone(),
        system_prompt: ctx.system_prompt.clone(),
        model: ctx.model.clone(),
        is_voice: ctx.is_voice,
        // Pass media through. Downstream MediaPolicy
        // (`AtMostOneLatest`) decides what attaches as bytes vs
        // becomes a description marker based on the persona's
        // capabilities. The projection stays out of that decision.
        message_media: signal.media.clone(),
        // Capabilities pass through unchanged — the persona
        // declared them at construction; the projection doesn't
        // second-guess.
        capabilities: ctx.capabilities.iter().copied().collect(),
    })
}

#[cfg(test)]
mod tests {
    //! Pure tests for the value objects and the projection. No I/O,
    //! no async. Validates: Signal serde round-trip, PersonaContext
    //! slot conversion, projection field mapping, signal-kind gate.
    use super::*;

    fn empty_ctx() -> PersonaContext {
        PersonaContext {
            persona_id: Uuid::nil(),
            display_name: String::new(),
            specialty: String::new(),
            model: String::new(),
            capabilities: Vec::new(),
            system_prompt: String::new(),
            recent_history: vec![],
            known_specialties: vec![],
            room_id: None,
            is_voice: false,
        }
    }

    fn chat_signal(text: &str) -> Signal {
        Signal {
            kind: SignalKind::ChatMessage,
            text: text.to_string(),
            media: vec![],
            originator: SignalOriginator::User { user_id: Uuid::nil() },
            timestamp_ms: 0,
            message_id: Some(Uuid::nil()),
        }
    }

    /// What this catches: Signal serializes through serde cleanly.
    /// The replay harness depends on Signal round-tripping through
    /// JSON; if a missing derive or renamed field drifts, captured
    /// fixtures stop replaying.
    #[test]
    fn signal_round_trips_through_serde() {
        let signal = Signal {
            kind: SignalKind::ChatMessage,
            text: "hello".to_string(),
            media: vec![],
            originator: SignalOriginator::User { user_id: Uuid::nil() },
            timestamp_ms: 1234,
            message_id: Some(Uuid::nil()),
        };
        let json = serde_json::to_string(&signal).expect("serializes");
        let back: Signal = serde_json::from_str(&json).expect("round-trips");
        assert_eq!(back.text, "hello");
        assert_eq!(back.timestamp_ms, 1234);
        assert!(matches!(back.kind, SignalKind::ChatMessage));
    }

    /// What this catches: `PersonaContext::slot()` mirrors the
    /// fields a `PersonaSlot` cares about. If `slot()` ever drops
    /// a field or adds drift, every `build_respond_input` call
    /// silently produces wrong cognition input.
    #[test]
    fn persona_context_slot_mirrors_fields() {
        let mut ctx = empty_ctx();
        ctx.persona_id = Uuid::nil();
        ctx.specialty = "vision".to_string();
        ctx.display_name = "Vision AI".to_string();
        let slot = ctx.slot();
        assert_eq!(slot.persona_id, ctx.persona_id);
        assert_eq!(slot.specialty, ctx.specialty);
        assert_eq!(slot.display_name, ctx.display_name);
    }

    /// What this catches: chat-shaped projection accepts a normal
    /// chat message and maps the fields verbatim into
    /// `RespondInput`. The trivial "the projection actually works"
    /// test.
    #[test]
    fn projection_accepts_chat_and_maps_fields() {
        let signal = chat_signal("hello");
        let mut ctx = empty_ctx();
        ctx.display_name = "Test Persona".to_string();
        ctx.specialty = "general".to_string();
        ctx.model = "test-model".to_string();
        ctx.system_prompt = "you are helpful".to_string();
        let input = build_respond_input(&signal, &ctx).expect("chat signal accepted");
        assert_eq!(input.message_text, "hello");
        assert_eq!(input.persona.display_name, "Test Persona");
        assert_eq!(input.model, "test-model");
        assert_eq!(input.system_prompt, "you are helpful");
        assert!(input.message_media.is_empty());
    }

    /// What this catches: chat-shaped projection rejects
    /// `FrameUpdate`. Loud `Err` instead of silently processing a
    /// video frame as a chat message — surfaces the host's routing
    /// bug in a debuggable place.
    #[test]
    fn projection_rejects_frame_update() {
        let mut signal = chat_signal("ignored");
        signal.kind = SignalKind::FrameUpdate;
        let err = build_respond_input(&signal, &empty_ctx())
            .expect_err("frame update should be rejected");
        assert!(err.contains("FrameUpdate"));
    }

    /// What this catches: `AutonomousTick` is accepted with empty
    /// text — the persona's own loop pinging "anything to do?"
    /// becomes a chat-shaped turn the persona's model decides about.
    #[test]
    fn projection_accepts_autonomous_tick() {
        let mut signal = chat_signal("");
        signal.kind = SignalKind::AutonomousTick;
        let input = build_respond_input(&signal, &empty_ctx())
            .expect("autonomous tick accepted");
        assert!(input.message_text.is_empty());
    }

    /// What this catches: media on the signal passes through to
    /// `RespondInput::message_media` unchanged. Downstream
    /// `MediaPolicy` decides byte-vs-marker; the projection stays
    /// out of the decision so vision-capable personas get bytes
    /// and text-only personas get description markers, both via
    /// the same projection.
    #[test]
    fn projection_passes_media_through() {
        let mut signal = chat_signal("look at this");
        signal.media = vec![MediaItemLite {
            item_type: "image".to_string(),
            base64: Some("AAAA".to_string()),
            mime_type: Some("image/png".to_string()),
            description: None,
        }];
        let input = build_respond_input(&signal, &empty_ctx())
            .expect("media-bearing chat accepted");
        assert_eq!(input.message_media.len(), 1);
        assert_eq!(input.message_media[0].item_type, "image");
        assert_eq!(input.message_media[0].base64.as_deref(), Some("AAAA"));
    }

    /// What this catches: capabilities round-trip from
    /// `PersonaContext` (Vec) into `RespondInput` (HashSet) without
    /// drop. If conversion ever drops or reorders, the downstream
    /// `MediaPolicy` gate sees wrong caps and silently wrong
    /// behavior follows.
    #[test]
    fn projection_capabilities_round_trip() {
        let mut ctx = empty_ctx();
        ctx.capabilities = vec![Capability::Vision, Capability::ToolUse];
        let input = build_respond_input(&chat_signal("hi"), &ctx).unwrap();
        assert!(input.capabilities.contains(&Capability::Vision));
        assert!(input.capabilities.contains(&Capability::ToolUse));
        assert_eq!(input.capabilities.len(), 2);
    }
}
