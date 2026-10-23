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
use crate::persona::turn_context::TurnContext;
use crate::persona::types::{InboxMessage, Modality, SenderType};
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

// ─── Signal ──────────────────────────────────────────────────────────

/// Hint about what kind of event produced this signal. The pipeline
/// executor may use it for routing decisions (e.g., a game pipeline
/// only acts on `FrameUpdate` or `AutonomousTick`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/recipe/SignalKind.ts"
)]
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/recipe/SignalOriginator.ts"
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
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/recipe/Signal.ts")]
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
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/recipe/PersonaContext.ts"
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
    /// Display names of OTHER personas this persona shares the room
    /// with (excluding self). Used by `prompt_assembly` for the
    /// `ProperChatMlSingleParty` strategy: history entries whose
    /// `name` is in this set are dropped from the rendered prompt
    /// because single-party-trained models (qwen3.5) cannot
    /// coherently process other-AI turns and produce echo loops /
    /// name-prefix leaks when shown them.
    ///
    /// Empty for: rooms with only this persona, hosts that don't
    /// expose a roster, or models that handle multi-party natively
    /// (the `NamePrefixedUserTurns` strategy ignores this field).
    /// Joel 2026-04-24, task #75 (PR-blocker): the source-level fix
    /// for "no band aids — engineering path" — see
    /// MultiPartyChatStrategy::ProperChatMlSingleParty doc.
    #[serde(default)]
    pub other_persona_names: Vec<String>,
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

// ─── Respond request envelope ────────────────────────────────────────

/// Wire envelope for `cognition/respond`: the host's raw event
/// ([`Signal`]) plus the per-persona stable context ([`PersonaContext`]).
///
/// The legacy `handle_command` arm read these as two separate top-level
/// params (`signal` + `personaContext`); this struct deserializes the
/// same `{ signal, personaContext }` payload in one step. No `recipe`
/// field — recipes are JSON data the host walks, not something the
/// cognition layer projects; an old-shape caller that still sends a
/// `recipe` key has it ignored (serde drops unknown fields), matching
/// the arm's documented "extra `recipe` field ignored" behavior.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/recipe/RespondRequest.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct RespondRequest {
    /// The host's raw pre-cognition event.
    pub signal: Signal,
    /// The persona's stable per-turn context (identity, model, caps,
    /// history, room). Field serializes as `personaContext` on the wire.
    pub persona_context: PersonaContext,
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
pub fn build_respond_input(signal: &Signal, ctx: &PersonaContext) -> Result<RespondInput, String> {
    match &signal.kind {
        SignalKind::ChatMessage | SignalKind::AutonomousTick | SignalKind::Custom { .. } => {}
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

    // Per-turn shared context. Hoisting the room-level fields
    // (room_id + recent_history + known_specialties) into an
    // Arc<TurnContext> is the continuum#1206 perf move: with N
    // personas responding to the same message, every persona's
    // RespondInput now shares one allocation instead of N deep
    // clones of identical data. Internally inside respond() the
    // savings compound (analyze + render + recorder all share via
    // the Arc instead of cloning). When the IPC layer later batches
    // N personas into one call (#1206 PR-2 / #1201 RTOS-for-AI),
    // building the TurnContext once and Arc-cloning it per persona
    // is the unblocked next step.
    let turn_context = TurnContext::arc(
        room_id,
        ctx.recent_history.clone(),
        ctx.known_specialties.clone(),
    );

    Ok(RespondInput {
        persona: ctx.slot(),
        turn_context,
        message_id,
        message_text: signal.text.clone(),
        other_persona_names: ctx.other_persona_names.clone(),
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
        // Recalled engrams default empty here. The IPC layer
        // (`cognition/respond` handler in modules/cognition.rs)
        // populates this AFTER the inline admission gate runs and
        // BEFORE calling respond(). Keeping the default empty means
        // any RespondInput constructed outside the IPC path (tests,
        // direct callers) gets a no-op memory render — same shape
        // as the system pre-#1211 PR-2.
        recalled_engrams: Vec::new(),
        // Room roster flows through the service-loop projection (which
        // has the RoomRosterSource delivery). This signal→RespondInput
        // projection doesn't carry compose deliveries, so it defaults
        // empty — no [Present in this room] block, backwards-compatible.
        room_roster: Vec::new(),
        room_doctrine: None,
    })
}

// ─── Signal → InboxMessage projection ────────────────────────────────
//
// The admission gate (`AdmissionState::admit`) consumes `InboxMessage`,
// not `Signal`. To run admission inline on the chat hot path
// (continuum#1211 — wire admission into `respond()`), the cognition/respond
// IPC handler needs to project the inbound `Signal + PersonaContext`
// into an `InboxMessage` BEFORE calling `respond()`.
//
// One canonical projection. Lives next to `build_respond_input` so the
// two projections evolve together.
//
// **Sender mapping** is the only non-trivial part: `SignalOriginator` is
// open-vocab (User | Persona | Tool | GameEngine | System) and
// `SenderType` is closed (Human | Persona | Agent | System). The mapping:
//
//   User      → Human       (with user_id as sender_id)
//   Persona   → Persona     (with persona_id as sender_id)
//   Tool      → Agent       (Uuid::nil sender_id; `Tool` carries no id)
//   GameEngine→ System      (Uuid::nil sender_id)
//   System    → System      (Uuid::nil sender_id)
//
// **Modality**: derived from `ctx.is_voice` (true → Voice, false → Chat).
// **Priority**: 0.5 default — the host doesn't carry per-message priority
// in `Signal` today; admission's own scoring re-evaluates anyway.
// **voice_session_id**: None (Signal doesn't carry one in v1).

/// Project `(Signal, PersonaContext) → InboxMessage` so the admission
/// gate can score the inbound event. The projection is total — every
/// `SignalOriginator` variant maps to a `SenderType` (with `Uuid::nil()`
/// for variants that don't carry an id).
pub fn signal_to_inbox_message(signal: &Signal, ctx: &PersonaContext) -> InboxMessage {
    let (sender_id, sender_name, sender_type) = match &signal.originator {
        SignalOriginator::User { user_id } => (*user_id, String::new(), SenderType::Human),
        SignalOriginator::Persona { persona_id } => {
            // Best-effort name — the originator's display name isn't on
            // Signal. Empty string is acceptable; admission scoring uses
            // sender_type, not the name.
            (*persona_id, String::new(), SenderType::Persona)
        }
        SignalOriginator::Tool { tool_name } => (Uuid::nil(), tool_name.clone(), SenderType::Agent),
        SignalOriginator::GameEngine => {
            (Uuid::nil(), "game-engine".to_string(), SenderType::System)
        }
        SignalOriginator::System => (Uuid::nil(), "system".to_string(), SenderType::System),
    };

    InboxMessage {
        id: signal.message_id.unwrap_or_else(Uuid::new_v4),
        room_id: ctx.room_id.unwrap_or(Uuid::nil()),
        sender_id,
        sender_name,
        sender_type,
        content: signal.text.clone(),
        timestamp: signal.timestamp_ms,
        priority: 0.5,
        source_modality: Some(if ctx.is_voice {
            Modality::Voice
        } else {
            Modality::Chat
        }),
        voice_session_id: None,
    }
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
            other_persona_names: vec![],
            room_id: None,
            is_voice: false,
        }
    }

    fn chat_signal(text: &str) -> Signal {
        Signal {
            kind: SignalKind::ChatMessage,
            text: text.to_string(),
            media: vec![],
            originator: SignalOriginator::User {
                user_id: Uuid::nil(),
            },
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
            originator: SignalOriginator::User {
                user_id: Uuid::nil(),
            },
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
        let input = build_respond_input(&signal, &empty_ctx()).expect("autonomous tick accepted");
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
        let input =
            build_respond_input(&signal, &empty_ctx()).expect("media-bearing chat accepted");
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

    /// What this catches (continuum#1206): the projection populates
    /// `turn_context` with the room-level fields from PersonaContext.
    /// Hoisted fields are no longer accessed via `input.room_id`
    /// etc. — they live on `input.turn_context`. If a future refactor
    /// accidentally puts `room_id` back on `RespondInput` directly,
    /// this test catches the regression.
    #[test]
    fn projection_populates_turn_context() {
        let mut ctx = empty_ctx();
        let room_id = Uuid::new_v4();
        ctx.room_id = Some(room_id);
        ctx.known_specialties = vec!["code".to_string(), "general".to_string()];

        let input = build_respond_input(&chat_signal("hi"), &ctx).unwrap();
        assert_eq!(input.turn_context.room_id, room_id);
        assert_eq!(
            input.turn_context.known_specialties,
            vec!["code".to_string(), "general".to_string()],
        );
        assert!(input.turn_context.recent_history.is_empty());
    }

    // ─── signal_to_inbox_message ────────────────────────────────────

    /// What this catches: a User-originated chat Signal projects to
    /// SenderType::Human with the user_id preserved. Admission scoring
    /// keys off sender_type for trust-mapping; if Human messages got
    /// labeled as Agent, the trust threshold would silently downgrade.
    #[test]
    fn signal_to_inbox_user_origin_maps_to_human() {
        let mut signal = chat_signal("hi");
        let user_id = Uuid::new_v4();
        signal.originator = SignalOriginator::User { user_id };
        signal.timestamp_ms = 12345;
        let mut ctx = empty_ctx();
        ctx.room_id = Some(Uuid::new_v4());

        let msg = signal_to_inbox_message(&signal, &ctx);
        assert_eq!(msg.sender_id, user_id);
        assert!(matches!(msg.sender_type, SenderType::Human));
        assert_eq!(msg.content, "hi");
        assert_eq!(msg.timestamp, 12345);
        assert_eq!(msg.room_id, ctx.room_id.unwrap());
    }

    /// What this catches: Persona-originated signals correctly become
    /// SenderType::Persona with the persona_id preserved. Without this,
    /// AI-to-AI messages would route through the Human trust mapping
    /// and admission's loop-prevention heuristics would silently misfire.
    #[test]
    fn signal_to_inbox_persona_origin_maps_to_persona() {
        let mut signal = chat_signal("from another persona");
        let persona_id = Uuid::new_v4();
        signal.originator = SignalOriginator::Persona { persona_id };

        let msg = signal_to_inbox_message(&signal, &empty_ctx());
        assert_eq!(msg.sender_id, persona_id);
        assert!(matches!(msg.sender_type, SenderType::Persona));
    }

    /// What this catches: Tool/GameEngine/System originators map
    /// without panicking and use Uuid::nil() as a stable sender_id
    /// (since these variants carry no id). The match is exhaustive —
    /// adding a new SignalOriginator variant later WILL be caught at
    /// compile time, not at runtime.
    #[test]
    fn signal_to_inbox_handles_all_originator_variants() {
        let cases = [
            (
                SignalOriginator::Tool {
                    tool_name: "search".to_string(),
                },
                SenderType::Agent,
            ),
            (SignalOriginator::GameEngine, SenderType::System),
            (SignalOriginator::System, SenderType::System),
        ];
        for (origin, expected) in cases {
            let mut signal = chat_signal("noop");
            signal.originator = origin;
            let msg = signal_to_inbox_message(&signal, &empty_ctx());
            assert_eq!(msg.sender_id, Uuid::nil(), "non-id originators use nil");
            assert!(
                std::mem::discriminant(&msg.sender_type) == std::mem::discriminant(&expected),
                "expected SenderType variant didn't match",
            );
        }
    }

    /// What this catches: voice context flows from PersonaContext
    /// through to InboxMessage::source_modality. Admission policy may
    /// score voice messages differently in future; preserving the
    /// modality bit ensures it can.
    #[test]
    fn signal_to_inbox_modality_follows_is_voice() {
        let mut ctx = empty_ctx();
        ctx.is_voice = true;
        let msg = signal_to_inbox_message(&chat_signal("hello"), &ctx);
        assert!(matches!(msg.source_modality, Some(Modality::Voice)));

        ctx.is_voice = false;
        let msg = signal_to_inbox_message(&chat_signal("hello"), &ctx);
        assert!(matches!(msg.source_modality, Some(Modality::Chat)));
    }

    /// What this catches: when Signal carries a message_id, the
    /// projection preserves it (admission dedup keys off content_hash
    /// but consumers may want to correlate the engram to the original
    /// chat message). When absent, the projection generates a fresh
    /// Uuid — never panics, never returns nil.
    #[test]
    fn signal_to_inbox_preserves_or_generates_id() {
        let known_id = Uuid::new_v4();
        let mut signal = chat_signal("known id");
        signal.message_id = Some(known_id);
        assert_eq!(signal_to_inbox_message(&signal, &empty_ctx()).id, known_id);

        signal.message_id = None;
        let generated = signal_to_inbox_message(&signal, &empty_ctx()).id;
        assert_ne!(generated, Uuid::nil(), "fresh id, not nil");
    }
}
