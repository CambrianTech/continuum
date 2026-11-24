//! A prompt call as frames (card 699578cb, slice 4): what the capture writer appends for a call's
//! submission, its end and an unwritable record, and the one decode of each.
//!
//! The header is the call's small typed fields as a MessagePack array (`frame`); each is a struct of
//! this file, with no field skipped, so its positions hold. Every optional or variable part is a run
//! of its own: the request without its messages, then one run per message (text stays raw UTF-8
//! inside MessagePack, never escaped), the replay source, the response and the raw stream. Those
//! runs are `rmp_serde::to_vec_named` (field names kept), because the engine's request and response
//! skip absent fields and a positional array would shift under them. An absent part is a nil run of
//! one byte, never a flag beside it.

use std::sync::Arc;

use continuum_engine::ai::types::{ChatMessage, TextGenerationRequest, TextGenerationResponse};
use continuum_engine::inference::wire_tap::Wire;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use crate::capture_log::CaptureRecord;
use crate::frame::{FrameError, FrameKind, FrameRef};

/// How a captured call stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, export_to = "../../../protocol/typescript/cognition/CallStatus.ts")]
pub enum CallStatus {
    Submitted,
    Completed,
    Failed,
    Cancelled,
    Incomplete,
    Legacy,
}

/// Input-only diagnostic provenance; hashes precede normal replay identity overrides.
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
pub struct InputComparison {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tool_schema: Option<ToolSchemaComparison>,
    pub omitted_message_indices: Vec<u32>,
    pub captured_request_sha256: String,
    pub comparison_request_sha256: String,
}

/// The selected schema and whole request remain in the existing capture payload.
#[derive(Debug, Clone, Serialize, Deserialize, ts_rs::TS)]
pub struct ToolSchemaComparison {
    pub tool_name: String,
    pub captured_schema_sha256: String,
    pub comparison_schema_sha256: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReplaySource {
    pub input_comparison: InputComparison,
    /// Whether replay pinned an implicit model using its recorded terminal response.
    pub model_from_response: bool,
    pub persona_id: Uuid,
    pub cursor: String,
}


/// One call's identity in its owner's log: the session that made it and its submitted request id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CallKey {
    pub session_id: Uuid,
    pub request_id: String,
}

/// A submission's typed fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SubmittedHead {
    pub key: CallKey,
    pub build_sha: String,
    pub room_id: Uuid,
    pub cycle_id: Option<u64>,
    /// The live binding window, or unknown for a replay provider without a window receipt.
    pub context_window: Option<u32>,
    pub cause: String,
    pub cause_root: Option<Uuid>,
    pub model: Option<String>,
    /// The room thread this request's history was rendered from: its revision and the airc events it
    /// used, so replay and a dataset can read them back from the room. Absent where the request had
    /// no thread (and in a frame written before this field existed: none was recorded).
    #[serde(default)]
    pub thread: Option<ThreadRef>,
}

/// Which room thread a request was rendered from (Fable's thread-history slice sets it).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadRef {
    pub room: airc_core::RoomId,
    pub revision: u64,
    /// The room events its history rendered, oldest first.
    pub events: Vec<airc_core::EventId>,
}

/// A call's end: its typed fields.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TerminalHead {
    pub key: CallKey,
    pub status: CallStatus,
    pub elapsed_ms: u64,
    pub error: Option<String>,
}

/// A record that could not be written whole, said in its place.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IncompleteHead {
    pub key: CallKey,
    pub error: String,
}

/// A call's submission as it is captured.
pub struct Submission<'a> {
    pub head: SubmittedHead,
    pub request: &'a TextGenerationRequest,
    pub replay: Option<&'a ReplaySource>,
}

/// A call's end as it is captured.
pub struct Ending<'a> {
    pub head: TerminalHead,
    pub response: Option<&'a TextGenerationResponse>,
    pub wire: Option<&'a Wire>,
}

/// One call frame read back, typed.
#[derive(Debug)]
pub enum CallFrame {
    Submitted { head: SubmittedHead, request: TextGenerationRequest, replay: Option<ReplaySource> },
    Terminal { head: TerminalHead, response: Option<TextGenerationResponse>, wire: Option<Wire> },
    Incomplete(IncompleteHead),
}

impl CallFrame {
    pub fn key(&self) -> &CallKey {
        match self {
            Self::Submitted { head, .. } => &head.key,
            Self::Terminal { head, .. } => &head.key,
            Self::Incomplete(head) => &head.key,
        }
    }
}

fn run(value: &impl Serialize) -> Result<Arc<[u8]>, FrameError> {
    Ok(rmp_serde::to_vec_named(value)?.into())
}

fn part<T: DeserializeOwned>(runs: &[&[u8]], at: usize) -> Result<T, FrameError> {
    let bytes = runs.get(at).ok_or(FrameError::Short { needed: at + 1, available: runs.len() })?;
    Ok(rmp_serde::from_slice(bytes)?)
}

impl Submission<'_> {
    /// The record the writer appends for `owner`: runs [request without messages, replay, message...].
    pub fn record(self, owner: Uuid, captured_at_ms: u64) -> Result<CaptureRecord, FrameError> {
        let rest = TextGenerationRequest { messages: Vec::new(), ..self.request.clone() };
        let mut runs = Vec::with_capacity(2 + self.request.messages.len());
        runs.push(run(&rest)?);
        runs.push(run(&self.replay)?);
        for message in &self.request.messages {
            runs.push(run(message)?);
        }
        Ok(CaptureRecord { owner, kind: FrameKind::Submitted, header: rmp_serde::to_vec(&self.head)?, runs, captured_at_ms })
    }
}

impl Ending<'_> {
    /// The record the writer appends for `owner`: runs [response, raw stream].
    pub fn record(self, owner: Uuid, captured_at_ms: u64) -> Result<CaptureRecord, FrameError> {
        let runs = vec![run(&self.response)?, run(&self.wire)?];
        Ok(CaptureRecord { owner, kind: FrameKind::Terminal, header: rmp_serde::to_vec(&self.head)?, runs, captured_at_ms })
    }
}

impl IncompleteHead {
    pub fn record(&self, owner: Uuid, captured_at_ms: u64) -> Result<CaptureRecord, FrameError> {
        Ok(CaptureRecord { owner, kind: FrameKind::Incomplete, header: rmp_serde::to_vec(self)?, runs: Vec::new(), captured_at_ms })
    }
}

/// The one decode of a call frame.
pub fn read(frame: &FrameRef<'_>) -> Result<CallFrame, FrameError> {
    Ok(match frame.kind {
        FrameKind::Submitted => {
            let mut request: TextGenerationRequest = part(&frame.runs, 0)?;
            request.messages = (2..frame.runs.len()).map(|at| part::<ChatMessage>(&frame.runs, at)).collect::<Result<_, _>>()?;
            CallFrame::Submitted { head: frame.header()?, request, replay: part(&frame.runs, 1)? }
        }
        FrameKind::Terminal => CallFrame::Terminal { head: frame.header()?, response: part(&frame.runs, 0)?, wire: part(&frame.runs, 1)? },
        FrameKind::Incomplete => CallFrame::Incomplete(frame.header()?),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use continuum_engine::ai::types::{ContentPart, MessageContent};

    fn decoded(record: CaptureRecord) -> CallFrame {
        let runs: Vec<&[u8]> = record.runs.iter().map(AsRef::as_ref).collect();
        let mut bytes = Vec::new();
        crate::frame::encode_parts(&mut bytes, crate::frame::Parts { kind: record.kind, header: &record.header, runs: &runs }).expect("test: encode");
        let (frame, _) = crate::frame::decode(&bytes).expect("test: decode");
        read(&frame).expect("test: read")
    }

    // what this catches: a captured request that loses the room thread it was rendered from (replay
    // and a dataset could not name the airc events its history used), or a frame written before the
    // field existed that no longer reads at all.
    #[test]
    fn a_submission_names_its_thread_and_an_older_frame_reads_as_having_none() {
        let thread = ThreadRef { room: airc_core::RoomId::new(), revision: 7, events: vec![airc_core::EventId::new(), airc_core::EventId::new()] };
        let head = SubmittedHead {
            key: CallKey { session_id: Uuid::new_v4(), request_id: "r-t".into() },
            build_sha: "abc".into(),
            room_id: Uuid::new_v4(),
            cycle_id: Some(1),
            context_window: Some(8192),
            cause: "chat".into(),
            cause_root: None,
            model: None,
            thread: Some(thread.clone()),
        };
        let request = TextGenerationRequest::default();
        let record = Submission { head: head.clone(), request: &request, replay: None }.record(Uuid::nil(), 1).expect("test: record");
        let CallFrame::Submitted { head: read, .. } = decoded(record) else { panic!("test: a submission") };
        assert_eq!(read.thread, Some(thread), "the thread revision and every event it rendered, in order");

        // a header encoded before the field existed (positions, so the array is one element shorter)
        #[derive(Serialize)]
        struct Before { key: CallKey, build_sha: String, room_id: Uuid, cycle_id: Option<u64>, context_window: Option<u32>, cause: String, cause_root: Option<Uuid>, model: Option<String> }
        let before = Before { key: head.key.clone(), build_sha: head.build_sha.clone(), room_id: head.room_id, cycle_id: head.cycle_id, context_window: head.context_window, cause: head.cause.clone(), cause_root: None, model: None };
        let older: SubmittedHead = rmp_serde::from_slice(&rmp_serde::to_vec(&before).expect("test: encode")).expect("test: an older frame still reads");
        assert_eq!((older.thread, older.key), (None, head.key), "it recorded no thread, and reads as having none");
    }

    // what this catches: a request that does not come back as it was served (an absent field
    // shifting a positional encoding, a message dropped or reordered, multimodal parts flattened),
    // a response or raw stream lost from its end, or an absent part read as present.
    #[test]
    fn a_call_reads_back_as_it_was_captured() {
        let key = CallKey { session_id: Uuid::new_v4(), request_id: "r-1".into() };
        let request = TextGenerationRequest {
            messages: vec![
                ChatMessage::text("system", "you are Aris"),
                ChatMessage { role: "user".into(), content: MessageContent::Parts(vec![ContentPart::Text { text: "the \"diff\"\n\tbelow".into() }]), name: None },
                ChatMessage::text("assistant", "ok"),
            ],
            max_tokens: Some(512),
            temperature: None,
            stop_sequences: Some(vec!["</s>".into()]),
            turn_bound: Some(std::time::Duration::from_millis(1500)),
            ..Default::default()
        };
        let head = SubmittedHead {
            key: key.clone(),
            build_sha: "abc".into(),
            room_id: Uuid::new_v4(),
            cycle_id: None,
            context_window: Some(32_768),
            cause: "chat".into(),
            cause_root: None,
            model: Some("qwen".into()),
            thread: None,
        };
        let record = Submission { head: head.clone(), request: &request, replay: None }.record(Uuid::nil(), 7).expect("test: record");
        assert_eq!(record.runs.len(), 2 + 3, "the rest, the replay, one run per message");
        let CallFrame::Submitted { head: read_head, request: read_request, replay } = decoded(record) else { panic!("test: a submission") };
        assert_eq!(read_head, head);
        assert!(replay.is_none(), "an absent replay reads as absent");
        assert_eq!(serde_json::to_value(&read_request).expect("test: json"), serde_json::to_value(&request).expect("test: json"), "the request as served");

        let wire = Wire { attempts: vec![vec!["data: {}".into()]], open_at_end: false };
        let ending = TerminalHead { key, status: CallStatus::Failed, elapsed_ms: 12, error: Some("parse refused".into()) };
        let record = Ending { head: ending.clone(), response: None, wire: Some(&wire) }.record(Uuid::nil(), 8).expect("test: record");
        let CallFrame::Terminal { head, response, wire: read_wire } = decoded(record) else { panic!("test: an end") };
        assert_eq!((head, response.is_none(), read_wire), (ending, true, Some(wire)));

        let served = continuum_engine::ai::types::TextGenerationResponse {
            text: "closed 3 cards".into(),
            finish_reason: continuum_engine::ai::types::FinishReason::Stop,
            model: "qwen".into(),
            provider: "local".into(),
            usage: Default::default(),
            response_time_ms: 40,
            request_id: "r-1".into(),
            content: None,
            tool_calls: None,
            reasoning: Some("the board first".into()),
            routing: None,
            error: None,
            timing: None,
        };
        let done = TerminalHead { key: CallKey { session_id: Uuid::nil(), request_id: "r-1".into() }, status: CallStatus::Completed, elapsed_ms: 40, error: None };
        let record = Ending { head: done, response: Some(&served), wire: None }.record(Uuid::nil(), 9).expect("test: record");
        let CallFrame::Terminal { response, wire, .. } = decoded(record) else { panic!("test: an end") };
        assert!(wire.is_none(), "an unwatched stream reads as absent");
        assert_eq!(serde_json::to_value(&response).expect("test: json"), serde_json::to_value(Some(&served)).expect("test: json"), "her response as given");
    }
}
