//! A persona's captured calls read back (card 699578cb, slice 4): pages of calls for the glass box,
//! one call whole, and the completed calls training and replay learn from. Every read goes through
//! the owner's [`CallIndex`] (refreshed from each generation's index tail) and decodes frames with
//! the one decode in `call_frames`; nothing here parses JSON.
//!
//! A call's cursor is its identity, `<session>/<request id>`, known the moment the call is captured:
//! the writer appends behind the turn, so a reader handed a cursor before its frames are written
//! reads `NotCaptured`, the same answer a call retention removed gets, and asks again later.

use std::collections::HashMap;
use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock};

use continuum_engine::ai::types::{TextGenerationRequest, TextGenerationResponse};
use continuum_engine::inference::wire_tap::Wire;
use continuum_kernel::system_resources::disk_reporters::ClassWriter;
use serde::Serialize;
use ts_rs::TS;
use uuid::Uuid;

use crate::call_frames::{self, CallFrame, CallKey, CallStatus, ReplaySource, SubmittedHead, TerminalHead};
use crate::call_index::{CallIndex, CallPlaces, Placed};
use crate::capture_log::{self, LogError};
use crate::frame::{self, FrameError};

/// Why captured calls could not be read.
#[derive(Debug, thiserror::Error)]
pub enum PlaybackError {
    #[error("{0:?} is not a capture cursor (<session>/<request id>)")]
    BadCursor(String),
    #[error("no captured frames for {}: not written yet, or removed by retention", cursor_of(.0))]
    NotCaptured(CallKey),
    #[error("{}: {refusal}", cursor_of(key))]
    Undecodable { key: CallKey, refusal: DecodeRefusal },
    #[error(transparent)]
    Log(#[from] LogError),
    #[error(transparent)]
    Frame(#[from] FrameError),
}

/// A call's cursor: its identity.
pub fn cursor_of(key: &CallKey) -> String {
    format!("{}/{}", key.session_id, key.request_id)
}

/// The call a cursor names.
pub fn key_of(cursor: &str) -> Result<CallKey, PlaybackError> {
    let bad = || PlaybackError::BadCursor(cursor.to_owned());
    let (session, request_id) = cursor.split_once('/').ok_or_else(bad)?;
    let session_id = Uuid::parse_str(session).map_err(|_| bad())?;
    if request_id.is_empty() {
        return Err(bad());
    }
    Ok(CallKey { session_id, request_id: request_id.to_owned() })
}

/// One call as the glass box lists it.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/CallHeader.ts")]
pub struct CallHeader {
    pub request_id: String,
    #[ts(type = "string")]
    pub session_id: Uuid,
    #[ts(type = "number | null")]
    pub cycle_id: Option<u64>,
    #[ts(type = "string")]
    pub room_id: Uuid,
    pub cause: String,
    #[ts(type = "string | null")]
    pub cause_root: Option<Uuid>,
    /// When the call's newest frame was captured.
    #[ts(type = "number")]
    pub captured_at_ms: u64,
    /// When the call was submitted.
    #[ts(type = "number")]
    pub started_at_ms: u64,
    pub status: CallStatus,
    pub model: Option<String>,
    /// The call's identity, stable across its lifecycle: a refresh upserts by it.
    pub cursor: String,
}

/// A page of calls, oldest first.
#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/PlaybackPage.ts")]
pub struct PlaybackPage {
    pub entries: Vec<CallHeader>,
    pub older: Option<String>,
    pub newer: Option<String>,
    pub issues: Vec<String>,
}

/// A call's submission read back.
#[derive(Debug, Serialize)]
pub struct CapturedSubmission {
    pub head: SubmittedHead,
    pub request: TextGenerationRequest,
    pub replay: Option<ReplaySource>,
}

/// A call's end read back.
#[derive(Debug, Serialize)]
pub struct CapturedEnd {
    pub head: TerminalHead,
    pub response: Option<TextGenerationResponse>,
    pub wire: Option<Wire>,
}

/// One call whole.
#[derive(Debug, Serialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/cognition/PlaybackDetail.ts")]
pub struct PlaybackDetail {
    pub header: CallHeader,
    #[ts(type = "unknown")]
    pub submitted: Option<CapturedSubmission>,
    #[ts(type = "unknown")]
    pub terminal: Option<CapturedEnd>,
    pub issues: Vec<String>,
}

/// The exact request she was served and, when the call finished, the exact response she gave.
#[derive(Debug, Serialize)]
pub struct CapturedCall {
    /// The submission's fields: its room, cause, window and identity.
    pub head: SubmittedHead,
    pub request: TextGenerationRequest,
    pub response: Option<TextGenerationResponse>,
}

/// Why a call's detail is not a whole call.
#[derive(Debug, thiserror::Error)]
pub enum DecodeRefusal {
    #[error("capture integrity: {}", .0.join("; "))]
    Integrity(Vec<String>),
    #[error("capture has no submitted request")]
    NoSubmission,
}

/// Decode a call's detail into its typed call; a call with integrity issues or no submission is
/// refused by name, never guessed at.
pub fn decode(detail: PlaybackDetail) -> Result<CapturedCall, DecodeRefusal> {
    if !detail.issues.is_empty() {
        return Err(DecodeRefusal::Integrity(detail.issues));
    }
    let submitted = detail.submitted.ok_or(DecodeRefusal::NoSubmission)?;
    Ok(CapturedCall {
        head: submitted.head,
        request: submitted.request,
        response: detail.terminal.and_then(|end| end.response),
    })
}

/// Each owner's index, held for the process and refreshed on every read.
static INDEXES: LazyLock<parking_lot::Mutex<HashMap<PathBuf, Arc<parking_lot::Mutex<CallIndex>>>>> = LazyLock::new(Default::default);

/// One persona's captured calls.
pub struct Playback {
    writer: ClassWriter,
}

impl Playback {
    /// `persona`'s calls in `class` (the prompt-captures scope, `capture::prompt_class`).
    pub fn of(class: &ClassWriter, persona: Uuid) -> Self {
        Self { writer: class.owned_by(persona) }
    }

    /// `persona`'s calls under a `fixtures` class rooted at `dir`, for a test or tool.
    #[cfg(any(test, feature = "test-fixtures"))]
    pub fn at(dir: &std::path::Path, persona: Uuid) -> Self {
        Self::of(&crate::capture::fixture_class(dir), persona)
    }

    /// The owner's index, refreshed with what was appended since the last read.
    fn index(&self) -> Result<Arc<parking_lot::Mutex<CallIndex>>, PlaybackError> {
        let index = Arc::clone(INDEXES.lock().entry(self.writer.dir()).or_insert_with(|| Arc::new(parking_lot::Mutex::new(CallIndex::new(self.writer.clone())))));
        index.lock().refresh()?;
        Ok(index)
    }

    fn frame(&self, placed: Placed) -> Result<CallFrame, PlaybackError> {
        let bytes = capture_log::frame_at(&self.writer, placed.cursor.generation, &placed.entry).map_err(LogError::from)?;
        let (frame, _) = frame::decode(&bytes)?;
        Ok(call_frames::read(&frame)?)
    }

    fn header(&self, key: &CallKey, places: &CallPlaces) -> Result<CallHeader, PlaybackError> {
        let submitted = places.submitted.ok_or_else(|| PlaybackError::NotCaptured(key.clone()))?;
        let (_, raw) = capture_log::frame_head(&self.writer, submitted.cursor.generation, &submitted.entry)?;
        let head: SubmittedHead = rmp_serde::from_slice(&raw).map_err(FrameError::from)?;
        let newest = [places.submitted, places.terminal, places.incomplete].iter().flatten().map(|p| p.entry.captured_at_ms).max();
        Ok(CallHeader {
            cursor: cursor_of(key),
            request_id: head.key.request_id,
            session_id: head.key.session_id,
            cycle_id: head.cycle_id,
            room_id: head.room_id,
            cause: head.cause,
            cause_root: head.cause_root,
            captured_at_ms: newest.unwrap_or(submitted.entry.captured_at_ms), // unwrap_or: the submission is one of the places, so a newest exists
            started_at_ms: submitted.entry.captured_at_ms,
            status: places.status(),
            model: head.model,
        })
    }

    /// Up to `limit` calls older than `position` (the newest when none), or newer with `newer`;
    /// oldest first. A call whose submission retention removed is an issue, never a guessed header.
    pub fn page(&self, position: Option<&str>, newer: bool, limit: NonZeroUsize) -> Result<PlaybackPage, PlaybackError> {
        let index = self.index()?;
        let index = index.lock();
        let calls = index.oldest_first();
        let mut issues = Vec::new();
        let listed: Vec<(&CallKey, &CallPlaces)> = calls
            .into_iter()
            .filter(|(key, places)| {
                let whole = places.submitted.is_some();
                if !whole {
                    issues.push(format!("{}: its submission was removed by retention", cursor_of(key)));
                }
                whole
            })
            .collect();
        let boundary = match position {
            Some(position) => {
                let key = key_of(position)?;
                let at = listed.iter().position(|(k, _)| **k == key).ok_or(PlaybackError::NotCaptured(key))?;
                at + usize::from(newer)
            }
            None if newer => 0,
            None => listed.len(),
        };
        let (start, end) = if newer { (boundary, (boundary + limit.get()).min(listed.len())) } else { (boundary.saturating_sub(limit.get()), boundary) };
        let entries = listed[start..end].iter().map(|(key, places)| self.header(key, places)).collect::<Result<Vec<_>, _>>()?;
        Ok(PlaybackPage {
            older: if start > 0 { entries.first().map(|e| e.cursor.clone()) } else { None },
            // a tail cursor even at the end: the next refresh reads only what is newer
            newer: entries.last().map(|e| e.cursor.clone()).or_else(|| position.map(str::to_owned)),
            entries,
            issues,
        })
    }

    /// The call `cursor` names, whole. A frame that does not decode is an issue beside what did.
    pub fn detail(&self, cursor: &str) -> Result<PlaybackDetail, PlaybackError> {
        let key = key_of(cursor)?;
        let index = self.index()?;
        let places = index.lock().get(&key).cloned().ok_or_else(|| PlaybackError::NotCaptured(key.clone()))?;
        let header = self.header(&key, &places)?;
        let mut issues = Vec::new();
        let mut read = |placed: Option<Placed>| match placed.map(|p| self.frame(p)) {
            Some(Ok(frame)) => Some(frame),
            Some(Err(error)) => {
                issues.push(error.to_string());
                None
            }
            None => None,
        };
        let submitted = match read(places.submitted) {
            Some(CallFrame::Submitted { head, request, replay }) => Some(CapturedSubmission { head, request, replay }),
            _ => None,
        };
        let terminal = match read(places.terminal) {
            Some(CallFrame::Terminal { head, response, wire }) => Some(CapturedEnd { head, response, wire }),
            _ => None,
        };
        Ok(PlaybackDetail { header, submitted, terminal, issues })
    }

    /// The call `cursor` names as its typed request and response, or why it is not whole.
    pub fn call(&self, cursor: &str) -> Result<CapturedCall, PlaybackError> {
        let key = key_of(cursor)?;
        decode(self.detail(cursor)?).map_err(|refusal| PlaybackError::Undecodable { key, refusal })
    }

    /// The newest `limit` calls that completed with a response that is not a generation error,
    /// oldest first: what replay and training learn from. Failed, cancelled and pending calls stay
    /// visible through `page`, never promoted to examples.
    pub fn completed(&self, limit: NonZeroUsize) -> Result<Vec<CapturedCall>, PlaybackError> {
        let index = self.index()?;
        let candidates: Vec<CallKey> = index
            .lock()
            .oldest_first()
            .into_iter()
            .rev()
            .filter(|(_, places)| places.ended == Some(CallStatus::Completed) && places.submitted.is_some())
            .map(|(key, _)| key.clone())
            .collect();
        let mut calls = Vec::new();
        for key in candidates {
            let call = self.call(&cursor_of(&key))?;
            if call.response.as_ref().is_some_and(|r| r.generation_error().is_none()) {
                calls.push(call);
            }
            if calls.len() == limit.get() {
                break;
            }
        }
        calls.reverse();
        Ok(calls)
    }
}
