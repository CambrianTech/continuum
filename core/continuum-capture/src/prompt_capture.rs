//! The deliberation capture: a call's submission before inference and its end after, said as
//! frames (`call_frames`) to the capture writer, which appends them behind the turn. The turn never
//! waits on the disk. Playback (`call_playback`) reads them back and never runs cognition.
use continuum_base::time::EpochMs;
use continuum_engine::ai::types::{TextGenerationRequest, TextGenerationResponse};
use serde::Serialize;
use std::sync::Arc;
use uuid::Uuid;

use crate::call_frames::{CallKey, Ending, IncompleteHead, Submission, SubmittedHead, TerminalHead};
use crate::capture_writer::Delivery;

#[path = "capture_owner.rs"]
pub mod owner;

pub use crate::call_frames::{CallStatus, InputComparison, ReplaySource, ToolSchemaComparison};
pub use crate::call_playback::{decode, CallHeader, CapturedCall, CapturedEnd, CapturedSubmission, DecodeRefusal, Playback, PlaybackDetail, PlaybackError, PlaybackPage};

#[derive(Debug, Serialize)]
pub struct PromptCall {
    pub request_id: String,
    pub persona_id: Uuid,
    pub room_id: Uuid,
    pub cycle_id: Option<u64>,
    /// Live binding window, or unknown for a replay provider without a window receipt.
    pub context_window: Option<u32>,
    pub cause: &'static str,
    pub cause_root: Option<Uuid>,
    /// Present only for an isolated replay; the original record remains untouched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_of: Option<ReplaySource>,
}

/// A submitted call's identity, held by its lease until the call ends.
#[derive(Debug, Clone)]
pub struct CaptureToken {
    key: CallKey,
    cursor: String,
}

/// Where a call's lifecycle is captured: its submission, then its end. No response is
/// manufactured for a failed inference.
pub trait PromptCaptureSink: Send + Sync {
    fn submitted(&self, call: &PromptCall, request: &TextGenerationRequest) -> Option<CaptureToken>;
    /// Record the call's end. Returns the call's cursor, which reads back as the request AND the
    /// response once the writer has appended both.
    fn terminal(
        &self,
        token: &CaptureToken,
        status: CallStatus,
        response: Option<&TextGenerationResponse>,
        error: Option<&str>,
        elapsed_ms: u64,
    ) -> Option<String>;
}

/// Contains only a sink handle and small metadata; cancellation cannot orphan a
/// submitted call while the core remains alive. Process death is honestly left
/// pending, not fabricated as a completion by a later reader.
pub struct CaptureLease {
    sink: Arc<dyn PromptCaptureSink>,
    token: Option<CaptureToken>,
    started: std::time::Instant,
}
impl CaptureLease {
    pub fn start(
        sink: Arc<dyn PromptCaptureSink>,
        call: &PromptCall,
        request: &TextGenerationRequest,
    ) -> Self {
        // A turn in her mind room is hers (PRIVACY-OF-THOUGHT.md §4, sink 2): the lease is
        // inert, so neither `finish` nor `Drop` writes, and every dataset built from
        // captures (sink 3) has nothing to read.
        let token = if continuum_rooms::mind_room::is_private_room(call.persona_id, call.room_id) {
            continuum_rooms::mind_room::note_withheld(call.persona_id, "prompt_capture");
            None
        } else {
            sink.submitted(call, request)
        };
        // A recorded call keeps its raw stream too, so the terminal entry can show the tokens
        // the parse turned into this response (card fb6d7912).
        if token.is_some() {
            continuum_engine::inference::wire_tap::arm(&call.request_id);
        }
        Self {
            sink,
            token,
            started: std::time::Instant::now(),
        }
    }
    pub fn cursor(&self) -> Option<&str> {
        self.token
            .as_ref()
            .map(|token| token.cursor.as_str())
    }
    /// End the call. Returns the completed entry's cursor: the one entry that reads back
    /// as the request AND the response (the submission's own cursor holds only the request).
    pub fn finish(&mut self, response: Option<&TextGenerationResponse>, error: Option<&str>) -> Option<String> {
        let status = if response.is_some_and(|response| response.generation_error().is_none()) {
            CallStatus::Completed
        } else {
            CallStatus::Failed
        };
        self.end(status, response, error)
    }
    /// ABANDONED BEFORE DISPATCH, SAID BY NAME (card ebce2ba0). A call the substrate
    /// gave up on at an admission gate — it never reached the model — is still a
    /// `Cancelled` row, but with a reason instead of the `Drop` impl's generic
    /// "request future dropped before a terminal response". Those two strings carried
    /// the SAME meaning for eighteen of fifty generations on the M5 (2026-09-20), so a
    /// reader could not tell a mind starved at the serving gate from a generation the
    /// model was still producing when its awaiting side walked away. Naming the seam is
    /// the whole difference.
    pub fn abandon(&mut self, reason: &str) {
        self.end(CallStatus::Cancelled, None, Some(reason));
    }
    /// The one way a call ends: record its terminal entry, then release its raw-stream
    /// watch, so no armed watch outlives its call whichever sink is attached (card fb6d7912).
    fn end(
        &mut self,
        status: CallStatus,
        response: Option<&TextGenerationResponse>,
        error: Option<&str>,
    ) -> Option<String> {
        let token = self.token.take()?;
        let cursor = self.sink.terminal(
            &token,
            status,
            response,
            error,
            self.started.elapsed().as_millis() as u64,
        );
        continuum_engine::inference::wire_tap::take(&token.key.request_id);
        cursor
    }
}
impl Drop for CaptureLease {
    fn drop(&mut self) {
        self.end(
            CallStatus::Cancelled,
            None,
            Some("request future dropped before a terminal response"),
        );
    }
}

/// The capture of one persona's calls, delivered to the node's writer (or inline, where a process
/// has none). Each construction is its own session, so restarted cycle counters never collide.
pub struct FramePromptCaptureSink {
    persona_id: Uuid,
    session_id: Uuid,
    delivery: Delivery,
}

impl FramePromptCaptureSink {
    pub fn new(persona_id: Uuid, delivery: Delivery) -> Self {
        Self { persona_id, session_id: Uuid::new_v4(), delivery }
    }

    /// A sink appending inline under `dir` (the root of a `fixtures` class in a test or tool).
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn at(dir: &std::path::Path, persona_id: Uuid) -> Self {
        Self::new(persona_id, Delivery::inline(crate::capture::fixture_class(dir)))
    }

    pub fn with_session(mut self, session_id: Uuid) -> Self {
        self.session_id = session_id;
        self
    }

    /// A record that could not be encoded is said in its place, never silently dropped.
    fn incomplete(&self, key: &CallKey, error: &crate::frame::FrameError) {
        tracing::warn!(target: "cognition::capture", persona = %self.persona_id, %error, "prompt lifecycle capture incomplete");
        let receipt = IncompleteHead { key: key.clone(), error: error.to_string() };
        match receipt.record(self.persona_id, EpochMs::now().millis()) {
            Ok(record) => self.delivery.deliver(record),
            Err(error) => continuum_base::probe!(class = "capture.receipt_unencodable", persona = %self.persona_id, error = %error,
                "a capture record and its incomplete receipt both failed to encode: nothing of this call is recorded"),
        }
    }
}

impl PromptCaptureSink for FramePromptCaptureSink {
    fn submitted(&self, call: &PromptCall, request: &TextGenerationRequest) -> Option<CaptureToken> {
        let key = CallKey { session_id: self.session_id, request_id: call.request_id.clone() };
        let head = SubmittedHead {
            key: key.clone(),
            build_sha: env!("CONTINUUM_BUILD_GIT_SHA").to_owned(),
            room_id: call.room_id,
            cycle_id: call.cycle_id,
            context_window: call.context_window,
            cause: call.cause.to_owned(),
            cause_root: call.cause_root,
            model: request.model.clone(),
        };
        match (Submission { head, request, replay: call.replay_of.as_ref() }).record(self.persona_id, EpochMs::now().millis()) {
            Ok(record) => self.delivery.deliver(record),
            Err(error) => {
                self.incomplete(&key, &error);
                return None;
            }
        }
        Some(CaptureToken { cursor: crate::call_playback::cursor_of(&key), key })
    }

    fn terminal(
        &self,
        token: &CaptureToken,
        status: CallStatus,
        response: Option<&TextGenerationResponse>,
        error: Option<&str>,
        elapsed_ms: u64,
    ) -> Option<String> {
        let wire = continuum_engine::inference::wire_tap::take(&token.key.request_id);
        let head = TerminalHead { key: token.key.clone(), status, elapsed_ms, error: error.map(str::to_owned) };
        match (Ending { head, response, wire: wire.as_ref() }).record(self.persona_id, EpochMs::now().millis()) {
            Ok(record) => {
                self.delivery.deliver(record);
                Some(token.cursor.clone())
            }
            Err(error) => {
                self.incomplete(&token.key, &error);
                None
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_engine::ai::types::ChatMessage;
    use std::num::NonZeroUsize;

    fn call(persona: Uuid, room: Uuid, id: &str) -> PromptCall {
        PromptCall { request_id: id.into(), persona_id: persona, room_id: room, cycle_id: Some(1), context_window: Some(8192), cause: "synthetic", cause_root: None, replay_of: None }
    }

    /// Every byte under `dir`, by path (`fs::metadata`, never a listing's size: on Windows a listing
    /// reports a file's size as of its open while a writer holds it, BigMama).
    fn bytes(dir: &std::path::Path) -> u64 {
        std::fs::read_dir(dir).into_iter().flatten().flatten().map(|e| e.path()).filter_map(|p| if p.is_dir() { Some(bytes(&p)) } else { std::fs::metadata(&p).ok().map(|m| m.len()) }).sum()
    }

    // what this catches: a turn in her mind room landing in the prompt capture (and so in every SFT
    // dataset built from it). Sink 2 of PRIVACY-OF-THOUGHT.md: zero bytes, through start, finish
    // AND the lease's Drop; an ordinary room still captures.
    #[test]
    fn a_turn_in_her_mind_room_writes_zero_capture_bytes() {
        let dir = tempfile::tempdir().expect("capture fixture directory");
        let persona = Uuid::new_v4();
        let sink: Arc<dyn PromptCaptureSink> = Arc::new(FramePromptCaptureSink::at(dir.path(), persona));
        let request = TextGenerationRequest { messages: vec![ChatMessage::text("user", "a private thought")], ..Default::default() };
        let mind = continuum_rooms::mind_room::mind_room_id(persona);
        let mut finished = CaptureLease::start(Arc::clone(&sink), &call(persona, mind, "r"), &request);
        assert_eq!(finished.finish(None, Some("done")), None, "an inert lease names no entry");
        drop(CaptureLease::start(Arc::clone(&sink), &call(persona, mind, "r"), &request));
        assert_eq!(bytes(dir.path()), 0, "her mind room: nothing captured, even by Drop");
        drop(CaptureLease::start(Arc::clone(&sink), &call(persona, Uuid::new_v4(), "r"), &request));
        assert!(bytes(dir.path()) > 0, "an ordinary room still captures");
    }

    // what this catches: a recorded call whose wrong tool call cannot be traced to the model or to
    // the parse (card fb6d7912: Kimi's 'x' calls, no raw tokens anywhere). A recorded call's end
    // carries its raw stream in order, the watch is released when the call ends, and a mind-room
    // call never arms one.
    #[test]
    fn a_recorded_call_keeps_its_raw_stream_in_its_terminal_entry() {
        let dir = tempfile::tempdir().expect("capture fixture directory");
        let persona = Uuid::new_v4();
        let sink: Arc<dyn PromptCaptureSink> = Arc::new(FramePromptCaptureSink::at(dir.path(), persona));
        let request_id = format!("wire-{}", Uuid::new_v4());
        let request = TextGenerationRequest { messages: vec![ChatMessage::text("user", "close the stray cards")], ..Default::default() };
        let mut lease = CaptureLease::start(Arc::clone(&sink), &call(persona, Uuid::new_v4(), &request_id), &request);
        let mut stream = continuum_engine::inference::wire_tap::WireTap::open(Some(&request_id));
        stream.push(r#"{"choices":[{"delta":{"reasoning_content":"close the stray cards"}}]}"#);
        stream.push(r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"name":"work/create","arguments":"{\"title\":\"x\"}"}}]}}]}"#);
        drop(stream);
        let cursor = lease.finish(None, Some("parse refused")).expect("the call's end is recorded");
        let end = Playback::at(dir.path(), persona).detail(&cursor).expect("read back").terminal.expect("its end");
        let wire = end.wire.expect("the raw stream is in its end");
        assert_eq!(wire.attempts.len(), 1, "one attempt");
        assert_eq!(wire.attempts[0].len(), 2);
        assert!(wire.attempts[0][1].contains("work/create"), "as received, in order");
        assert!(!wire.open_at_end);
        assert_eq!(end.head.error.as_deref(), Some("parse refused"));
        assert_eq!(continuum_engine::inference::wire_tap::take(&request_id), None, "released when the call ended");

        let private_id = format!("wire-{}", Uuid::new_v4());
        let mind = continuum_rooms::mind_room::mind_room_id(persona);
        drop(CaptureLease::start(Arc::clone(&sink), &call(persona, mind, &private_id), &request));
        let mut thought = continuum_engine::inference::wire_tap::WireTap::open(Some(&private_id));
        thought.push("a private token");
        drop(thought);
        assert_eq!(continuum_engine::inference::wire_tap::take(&private_id), None, "her mind room arms nothing");
    }

    // what this catches: two sinks of one persona (an eval fork assembled while the first call
    // runs) that append over each other or share a session, so one call's end is joined to the
    // other's request; or a sink made after the last one dropped (a core restart) that loses the
    // history before it.
    #[test]
    fn concurrent_capture_wrappers_share_one_owner_and_keep_distinct_sessions() {
        let dir = tempfile::tempdir().expect("capture fixture directory");
        let persona = Uuid::new_v4();
        let first = FramePromptCaptureSink::at(dir.path(), persona);
        let request = TextGenerationRequest {
            system_prompt: Some("actual identity".into()),
            messages: vec![ChatMessage::text("user", "original task")],
            temperature: Some(0.3),
            ..Default::default()
        };
        let first_token = first.submitted(&call(persona, Uuid::nil(), "first"), &request).expect("submitted first");
        let second = FramePromptCaptureSink::at(dir.path(), persona);
        assert_ne!(first.session_id, second.session_id, "construction sessions disambiguate restarted cycle counters");
        let second_token = second.submitted(&call(persona, Uuid::nil(), "second"), &request).expect("submitted second");
        second.terminal(&second_token, CallStatus::Failed, None, Some("fixture failure"), 2);
        first.terminal(&first_token, CallStatus::Cancelled, None, Some("fixture cancellation"), 4);
        drop((first, second));
        let reopened = FramePromptCaptureSink::at(dir.path(), persona);
        let third = reopened.submitted(&call(persona, Uuid::nil(), "third"), &request).expect("submitted after the restart");

        let playback = Playback::at(dir.path(), persona);
        let page = playback.page(None, false, NonZeroUsize::new(10).expect("ten")).expect("interleaved lifecycle");
        assert!(page.issues.is_empty(), "{:?}", page.issues);
        let listed: Vec<(&str, CallStatus)> = page.entries.iter().map(|e| (e.request_id.as_str(), e.status)).collect();
        assert_eq!(listed, [("first", CallStatus::Cancelled), ("second", CallStatus::Failed), ("third", CallStatus::Submitted)], "one entry per call, history kept across the restart");
        for entry in &page.entries {
            let detail = playback.detail(&entry.cursor).expect("matching request");
            let submitted = detail.submitted.expect("its request");
            assert_eq!((submitted.head.key.request_id.as_str(), submitted.head.key.session_id), (entry.request_id.as_str(), entry.session_id));
            assert_eq!(detail.terminal.as_ref().map(|end| &end.head.key), (entry.status != CallStatus::Submitted).then_some(&submitted.head.key), "an end joined to its own call only");
        }
        assert_eq!(page.entries[2].cursor, third.cursor, "the cursor a lease hands out is the one playback lists");
    }
}
