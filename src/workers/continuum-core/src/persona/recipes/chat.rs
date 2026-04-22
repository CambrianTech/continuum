//! `ChatRecipe` — text baseline. Maps a chat-message Signal +
//! PersonaContext into the cognition layer's `RespondInput`. The
//! shape every other recipe extends or contrasts.
//!
//! # Contract
//!
//! - Accepts: `SignalKind::ChatMessage`, `SignalKind::AutonomousTick`,
//!   `SignalKind::Custom { .. }` (host-defined chat-shaped signals).
//! - Rejects (returns Err): `SignalKind::FrameUpdate`, `SignalKind::CodeContext`,
//!   `SignalKind::ToolResult` — these belong to other recipes; routing
//!   them here is a host bug we surface loudly instead of silently
//!   processing as text.
//! - Media on the signal: passed through to RespondInput's
//!   `message_media`. `respond()` + `MediaPolicy::AtMostOneLatest`
//!   handle whether bytes attach (they will, when the persona has
//!   the matching capability). ChatRecipe doesn't pre-strip media —
//!   text-only personas already get text-marker fallback in the
//!   message-build seam, so this stays simple.
//! - validate_output: default Forward. Chat path posts whatever
//!   cognition produced.

use crate::persona::recipe::{
    ModalityKind, PersonaContext, Recipe, Signal, SignalKind,
};
use crate::persona::response::RespondInput;
use uuid::Uuid;

pub struct ChatRecipe;

impl Recipe for ChatRecipe {
    fn name(&self) -> &'static str {
        "chat"
    }

    fn modalities(&self) -> &[ModalityKind] {
        // Lazy static would be cleaner but the slice-from-Vec dance
        // doesn't constify. Returning a `&'static [ModalityKind]` would
        // require const construction of the wrapper struct, which the
        // String backing prevents. Instead callers that want a typed
        // list iterate `Recipe::modalities()` and compare strings.
        // For ChatRecipe specifically, modalities are "text" (always)
        // and "image" (because chat carries media that downstream
        // recipes care about — ChatRecipe tolerates media without
        // gating on it).
        //
        // We return a reference to a static slice. The slice lives
        // as a const elsewhere in this module so it has 'static
        // lifetime.
        MODALITIES
    }

    fn build_input(
        &self,
        signal: &Signal,
        ctx: &PersonaContext,
    ) -> Result<RespondInput, String> {
        // Reject signals that belong to a different recipe. Host's
        // routing bug surfaces here as a loud error instead of as
        // silently-wrong cognition output downstream.
        match &signal.kind {
            SignalKind::ChatMessage
            | SignalKind::AutonomousTick
            | SignalKind::Custom { .. } => {}
            other => {
                return Err(format!(
                    "ChatRecipe doesn't accept SignalKind::{:?} — route to the \
                     correct recipe (Vision for image-bearing, Code for \
                     CodeContext, etc.)",
                    other
                ));
            }
        }

        // The message_id on the IPC payload identifies WHICH chat
        // message this turn services. Empty uuid = autonomous tick
        // or signal without a persisted message.
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
            // Pass media through. Downstream MediaPolicy (currently
            // AtMostOneLatest) decides what attaches as bytes vs
            // becomes a description marker based on the persona's
            // capabilities. ChatRecipe stays out of that decision so
            // VisionRecipe can override the policy and ChatRecipe
            // doesn't have to know about Vision-specific concerns.
            message_media: signal.media.clone(),
            // Capabilities pass through unchanged — the persona
            // declared them at construction; recipes don't second-
            // guess.
            capabilities: ctx.capabilities.iter().copied().collect(),
        })
    }
}

/// Static modality slice. Const-constructed so `modalities()` can
/// return `&'static [ModalityKind]` without a heap allocation per call.
static MODALITIES: &[ModalityKind] = &[];

// Note on the empty MODALITIES slice: ModalityKind wraps a String,
// which can't be const-constructed. Returning an empty slice means
// "this recipe doesn't enforce a modality filter" — host routing
// uses the recipe NAME for dispatch (caller picks "chat" for chat
// signals), not a runtime modality match. If we later need typed
// modality lookup, switch ModalityKind to a `Cow<'static, str>` or
// const enum and remove this workaround.

#[cfg(test)]
mod tests {
    //! Pure-function tests. No I/O, no inference. Validates
    //! ChatRecipe's contract: which signal kinds it accepts, how it
    //! maps fields from Signal+PersonaContext into RespondInput.
    //! Behavior tests (does the model produce reasonable text?)
    //! live in the replay harness — those use real captured
    //! fixtures, not synthetic mocks.
    use super::*;
    use crate::persona::recipe::{Signal, SignalOriginator};

    fn ctx() -> PersonaContext {
        PersonaContext {
            persona_id: Uuid::nil(),
            display_name: "Test Persona".to_string(),
            specialty: "general".to_string(),
            model: "test-model".to_string(),
            capabilities: vec![],
            system_prompt: "you are helpful".to_string(),
            recent_history: vec![],
            known_specialties: vec!["general".to_string()],
            room_id: Some(Uuid::nil()),
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

    /// What this catches: ChatRecipe accepts a normal chat message
    /// and produces a RespondInput whose fields mirror the inputs.
    /// The trivial "the recipe actually works" test.
    #[test]
    fn accepts_chat_message_and_maps_fields() {
        let recipe = ChatRecipe;
        let signal = chat_signal("hello");
        let input = recipe.build_input(&signal, &ctx()).expect("chat signal accepted");
        assert_eq!(input.message_text, "hello");
        assert_eq!(input.persona.display_name, "Test Persona");
        assert_eq!(input.model, "test-model");
        assert_eq!(input.system_prompt, "you are helpful");
        assert!(input.message_media.is_empty());
    }

    /// What this catches: ChatRecipe REJECTS signal kinds that
    /// belong to other recipes. Loud Err instead of silently
    /// processing a video frame as a chat message. The error
    /// message identifies which kind was wrong so the host's
    /// routing bug is debuggable.
    #[test]
    fn rejects_frame_update_signal() {
        let recipe = ChatRecipe;
        let mut signal = chat_signal("ignored");
        signal.kind = SignalKind::FrameUpdate;
        let err = recipe.build_input(&signal, &ctx()).expect_err("frame update should be rejected");
        assert!(err.contains("ChatRecipe"));
        assert!(err.contains("FrameUpdate"));
    }

    /// What this catches: ChatRecipe REJECTS code-context signals.
    /// Same loud-fail principle — code context belongs to CodeRecipe.
    #[test]
    fn rejects_code_context_signal() {
        let recipe = ChatRecipe;
        let mut signal = chat_signal("ignored");
        signal.kind = SignalKind::CodeContext;
        let err = recipe.build_input(&signal, &ctx()).expect_err("code context should be rejected");
        assert!(err.contains("CodeContext"));
    }

    /// What this catches: ChatRecipe accepts AutonomousTick (the
    /// persona's own loop pinging "anything to do?"). Treats it as
    /// a chat-shaped turn with possibly empty text — the persona's
    /// own model decides whether to speak.
    #[test]
    fn accepts_autonomous_tick() {
        let recipe = ChatRecipe;
        let mut signal = chat_signal("");
        signal.kind = SignalKind::AutonomousTick;
        let input = recipe.build_input(&signal, &ctx()).expect("autonomous tick accepted");
        assert!(input.message_text.is_empty());
    }

    /// What this catches: ChatRecipe passes media through unchanged
    /// to RespondInput. Downstream MediaPolicy decides byte-vs-marker
    /// based on persona capability — ChatRecipe doesn't pre-strip,
    /// because that would defeat VisionRecipe-equivalent personas
    /// that DO have Vision capability and want the bytes.
    #[test]
    fn passes_media_through_unchanged() {
        use crate::cognition::tool_executor::types::MediaItemLite;
        let recipe = ChatRecipe;
        let mut signal = chat_signal("look at this");
        signal.media = vec![MediaItemLite {
            item_type: "image".to_string(),
            base64: Some("AAAA".to_string()),
            mime_type: Some("image/png".to_string()),
            description: None,
        }];
        let input = recipe.build_input(&signal, &ctx()).expect("media-bearing chat accepted");
        assert_eq!(input.message_media.len(), 1);
        assert_eq!(input.message_media[0].item_type, "image");
        assert_eq!(input.message_media[0].base64.as_deref(), Some("AAAA"));
    }

    /// What this catches: ChatRecipe forwards capabilities from
    /// PersonaContext into RespondInput as a HashSet. If the
    /// conversion ever drops or reorders, the downstream
    /// MediaPolicy gate sees wrong caps and silently wrong
    /// behavior follows.
    #[test]
    fn capabilities_round_trip_to_respond_input() {
        use crate::model_registry::Capability;
        let recipe = ChatRecipe;
        let mut c = ctx();
        c.capabilities = vec![Capability::Vision, Capability::ToolUse];
        let input = recipe.build_input(&chat_signal("hi"), &c).unwrap();
        assert!(input.capabilities.contains(&Capability::Vision));
        assert!(input.capabilities.contains(&Capability::ToolUse));
        assert_eq!(input.capabilities.len(), 2);
    }
}
