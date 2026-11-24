//! Which session a call belongs to, and its capture.
//!
//! Until a session is seated as a mind (DRIVER-GATEWAY.md G2), its identity is its stable
//! head: the system prompt and the first user turn, which an agent re-sends unchanged on
//! every step of one session. Every call is recorded in the one capture the residents use
//! (`continuum-capture`), keyed by that id, so the act inspector, replay and Dreaming read an
//! agent's session exactly as they read a resident's acts.

use std::sync::Arc;

use continuum_capture::capture_writer::Delivery;
use continuum_capture::prompt_capture::{CaptureLease, FramePromptCaptureSink, PromptCall, PromptCaptureSink};
use continuum_engine::ai::{TextGenerationRequest, TextGenerationResponse};
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

/// Why a call cannot be recorded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Unrecordable {
    /// No capture writer runs on this node: a record said now would reach no disk.
    NoWriter,
    /// The call's submission could not be encoded as a frame.
    Unencodable,
}

/// The call cannot be recorded, so it is refused rather than served unrecorded (the capture
/// contract: no call outside the one capture, `continuum-capture`).
#[derive(Debug)]
pub(crate) struct CaptureUnavailable {
    session: Uuid,
    why: Unrecordable,
}

impl std::fmt::Display for CaptureUnavailable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let why = match self.why {
            Unrecordable::NoWriter => "no capture writer runs on this node",
            Unrecordable::Unencodable => "its submission could not be encoded",
        };
        write!(f, "this node cannot record session {} ({why}); the call is refused rather than served unrecorded", self.session)
    }
}

/// Start recording one call of `session` through the node's capture writer. A call that
/// cannot be recorded is refused, said in a probe and to the agent.
pub(crate) fn record(session: Uuid, request_id: &str, wire: &'static str, request: &TextGenerationRequest) -> Result<CaptureLease, CaptureUnavailable> {
    let refuse = |why: Unrecordable| {
        continuum_base::probe!(class = "gateway.capture_unavailable", session = %session, why = ?why, "this agent call is refused: it cannot be recorded");
        CaptureUnavailable { session, why }
    };
    if !continuum_capture::capture_writer::running() {
        return Err(refuse(Unrecordable::NoWriter));
    }
    let sink: Arc<dyn PromptCaptureSink> = Arc::new(FramePromptCaptureSink::new(session, Delivery::Node));
    let call = PromptCall {
        request_id: request_id.to_owned(),
        persona_id: session,
        room_id: Uuid::nil(),
        cycle_id: None,
        context_window: None,
        cause: wire,
        cause_root: None,
        replay_of: None,
        thread: None,
    };
    let lease = CaptureLease::start(sink, &call, request);
    match lease.cursor() {
        Some(_) => Ok(lease),
        None => Err(refuse(Unrecordable::Unencodable)),
    }
}

/// End a recorded call with the generation's outcome. A call that never reaches this (the
/// agent left, dropping the generation) is recorded as cancelled by the lease's `Drop`.
pub(crate) fn settle(lease: &mut CaptureLease, outcome: &Result<TextGenerationResponse, crate::route::RouteError>) {
    match outcome {
        Ok(response) => lease.finish(Some(response), None),
        Err(error) => lease.finish(None, Some(&format!("{error:?}"))),
    };
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

    // what this catches: a call served unrecorded when nothing can record it (the probe-only
    // path of 34672ec73), invisible to the act inspector, replay and Dreaming. This test process
    // starts no capture writer, so the call is refused.
    #[test]
    fn a_call_no_writer_can_record_is_refused() {
        let call = request("You are a coder.", &[("user", "fix median")]);
        let Err(unavailable) = record(session_of(&call), "chatcmpl_1", "openai_chat", &call) else {
            panic!("test: a call with no capture writer must be refused");
        };
        let refused = crate::server::capture_refused::<crate::wire::chat::Chat>(&unavailable);
        assert_eq!(refused.status(), axum::http::StatusCode::SERVICE_UNAVAILABLE);
        assert!(unavailable.to_string().contains("no capture writer"), "the agent is told why: {unavailable}");
    }
}
