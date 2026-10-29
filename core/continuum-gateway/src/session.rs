//! Which session a call belongs to, and its capture.
//!
//! Until a session is seated as a mind (DRIVER-GATEWAY.md G2), its identity is its stable
//! head: the system prompt and the first user turn, which an agent re-sends unchanged on
//! every step of one session. Every call is recorded in the one capture the residents use
//! (`continuum-capture`), keyed by that id, so the act inspector, replay and Dreaming read an
//! agent's session exactly as they read a resident's acts.

use std::sync::Arc;

use continuum_capture::prompt_capture::{CaptureLease, JsonlPromptCaptureSink, PromptCall, PromptCaptureSink};
use continuum_engine::ai::TextGenerationRequest;
use uuid::Uuid;

/// The namespace driver session ids are derived in. Fixed forever: changing it would split
/// every recorded session from its later calls.
const DRIVER_SESSIONS: Uuid = Uuid::from_bytes(*b"driver-sessions!");

/// The session a call belongs to: the same id for every step of one agent session.
pub(crate) fn session_of(request: &TextGenerationRequest) -> Uuid {
    let mut head = request.system_prompt.clone().unwrap_or_default(); // unwrap_or_default: an agent may send no system prompt
    if let Some(first) = request.messages.iter().find(|m| m.role == "user") {
        head.push('\u{1f}');
        head.push_str(&crate::wire::plain_text(&first.content));
    }
    Uuid::new_v5(&DRIVER_SESSIONS, head.as_bytes())
}

/// Start recording one call of `session`. `None` when the capture home cannot be opened,
/// said in a probe: an unrecorded call is still served.
pub(crate) fn record(session: Uuid, request_id: &str, wire: &'static str, request: &TextGenerationRequest) -> Option<CaptureLease> {
    let sink: Arc<dyn PromptCaptureSink> = match JsonlPromptCaptureSink::open_for_persona(session) {
        Ok(sink) => Arc::new(sink),
        Err(error) => {
            continuum_base::probe!(class = "gateway.capture_unavailable", session = %session, error = %error,
                "this agent call is served but not recorded");
            return None;
        }
    };
    let call = PromptCall {
        request_id: request_id.to_owned(),
        persona_id: session,
        room_id: Uuid::nil(),
        cycle_id: None,
        context_window: None,
        cause: wire,
        cause_root: None,
        replay_of: None,
    };
    Some(CaptureLease::start(sink, &call, request))
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_engine::ai::{ChatMessage, MessageContent};

    fn request(system: &str, turns: &[(&str, &str)]) -> TextGenerationRequest {
        TextGenerationRequest {
            system_prompt: Some(system.into()),
            messages: turns
                .iter()
                .map(|(role, text)| ChatMessage { role: (*role).into(), content: MessageContent::Text((*text).into()), name: None })
                .collect(),
            ..Default::default()
        }
    }

    // what this catches: one agent session recorded as many (its later steps keep the same
    // head and append), or two sessions merged (a different task is a different head).
    #[test]
    fn every_step_of_a_session_is_one_session_and_another_task_is_another() {
        let first = request("You are a coder.", &[("user", "fix median")]);
        let later = request("You are a coder.", &[("user", "fix median"), ("assistant", "reading"), ("user", "[tool] ok")]);
        let other = request("You are a coder.", &[("user", "add a mode() function")]);
        assert_eq!(session_of(&first), session_of(&later));
        assert_ne!(session_of(&first), session_of(&other));
    }
}
