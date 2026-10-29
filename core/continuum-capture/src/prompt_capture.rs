//! The existing deliberation capture owner: borrowed request submission before
//! inference, then a small terminal record. Playback reads never run cognition.
use continuum_base::time::EpochMs;
use continuum_engine::ai::types::{ChatMessage, TextGenerationRequest, TextGenerationResponse};
use serde::Serialize;
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};
use uuid::Uuid;

#[path = "capture_owner.rs"]
pub mod owner;

#[path = "prompt_capture_store.rs"]
mod store;
pub use store::{detail, page};
pub use store::{CallHeader, CallStatus, PlaybackDetail, PlaybackPage};
use store::{PayloadRef, Store};
pub const FIXTURE_DIR: &str = "fixtures/prompt-captures";

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

/// Input-only diagnostic provenance; hashes precede normal replay identity overrides.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct InputComparison {
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tool_schema: Option<ToolSchemaComparison>,
    pub omitted_message_indices: Vec<u32>,
    pub captured_request_sha256: String,
    pub comparison_request_sha256: String,
}

/// The selected schema and whole request remain in the existing capture payload.
#[derive(Debug, Clone, Serialize, ts_rs::TS)]
pub struct ToolSchemaComparison {
    pub tool_name: String,
    pub captured_schema_sha256: String,
    pub comparison_schema_sha256: String,
}

#[derive(Debug, Serialize)]
pub struct ReplaySource {
    pub input_comparison: InputComparison,
    /// Whether replay pinned an implicit model using its recorded terminal response.
    pub model_from_response: bool,
    pub persona_id: Uuid,
    pub cursor: String,
}
#[derive(Debug, Clone)]
pub struct CaptureToken {
    header: CallHeader,
}

/// Legacy sinks need only implement record. Lifecycle-aware sinks override the
/// two typed callbacks; no response is manufactured for a failed inference.
pub trait PromptCaptureSink: Send + Sync {
    fn lifecycle_enabled(&self) -> bool {
        false
    }
    fn submitted(
        &self,
        _call: &PromptCall,
        _request: &TextGenerationRequest,
    ) -> Option<CaptureToken> {
        None
    }
    /// Record the call's end. Returns the cursor of the completed entry, which holds
    /// both the request and the response, when the sink recorded one.
    fn terminal(
        &self,
        _token: &CaptureToken,
        _status: CallStatus,
        _response: Option<&TextGenerationResponse>,
        _error: Option<&str>,
        _elapsed_ms: u64,
    ) -> Option<String> {
        None
    }
    fn record(
        &self,
        persona_id: Uuid,
        room_id: Uuid,
        cause: &'static str,
        iteration: usize,
        system: &str,
        messages: &[ChatMessage],
        offered_tools: &[String],
        response: &TextGenerationResponse,
    );
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
            .map(|token| token.header.cursor.as_str())
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
        continuum_engine::inference::wire_tap::take(&token.header.request_id);
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

#[derive(Serialize)]
struct SubmittedRecord<'a> {
    schema_version: u32,
    event: &'static str,
    captured_at_ms: u64,
    session_id: Uuid,
    build_sha: &'static str,
    #[serde(flatten)]
    call: &'a PromptCall,
    request: &'a TextGenerationRequest,
}
#[derive(Serialize)]
struct TerminalRecord<'a> {
    schema_version: u32,
    event: &'a CallStatus,
    captured_at_ms: u64,
    session_id: Uuid,
    request_id: &'a str,
    request_ref: &'a PayloadRef,
    elapsed_ms: u64,
    response: Option<&'a TextGenerationResponse>,
    error: Option<&'a str>,
    /// The raw stream the response was parsed from: each attempt's SSE events as received
    /// (normalization named in `inference::wire_tap`), the last attempt being the response's
    /// (card fb6d7912). Absent when no stream was watched.
    #[serde(skip_serializing_if = "Option::is_none")]
    wire: Option<continuum_engine::inference::wire_tap::Wire>,
    /// Why a watched stream is NOT in this entry, when it is not: a stream that would push the
    /// entry past the capture's row limit is dropped from it, so it is never the cause of an
    /// unrecordable entry, and the drop is said.
    #[serde(skip_serializing_if = "Option::is_none")]
    wire_omitted: Option<String>,
}
/// The terminal record, kept within the capture's row limit by dropping its raw stream and
/// saying so, measured with the store's own row contract (JSON plus newline). It removes the
/// wire as a cause of an unrecordable entry; it cannot make an entry recordable whose response
/// alone exceeds the limit, or survive a storage failure. An omitted wire is diagnostic
/// incompleteness, never retention of the whole experience.
fn within_row_limit(mut record: TerminalRecord<'_>, limit: u64) -> TerminalRecord<'_> {
    if record.wire.is_none() {
        return record;
    }
    let bytes = serde_json::to_vec(&record).map_or(u64::MAX, |v| v.len() as u64);
    if store::row_bytes(bytes) > limit {
        record.wire = None;
        record.wire_omitted = Some(format!(
            "the raw stream made this entry {bytes} bytes, over the capture's {limit}-byte row limit"
        ));
    }
    record
}
#[derive(Serialize)]
struct LegacyRecord<'a> {
    schema_version: u32,
    captured_at_ms: u64,
    persona_id: Uuid,
    room_id: Uuid,
    cause: &'static str,
    iteration: usize,
    system: &'a str,
    messages: &'a [ChatMessage],
    offered_tools: &'a [String],
    response: &'a TextGenerationResponse,
}

pub struct JsonlPromptCaptureSink {
    persona_id: Uuid,
    session_id: Uuid,
    store: Arc<Mutex<Store>>,
}
impl JsonlPromptCaptureSink {
    pub fn open_for_persona(persona_id: Uuid) -> std::io::Result<Self> {
        let dir = crate::capture::fixture_dir(FIXTURE_DIR).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "cannot resolve capture home directory",
            )
        })?;
        Self::open(&dir, persona_id)
    }
    pub fn open(dir: &Path, persona_id: Uuid) -> std::io::Result<Self> {
        static OWNERS: LazyLock<owner::CaptureOwners<Store>> =
            LazyLock::new(owner::CaptureOwners::default);
        let path = dir.join(format!("{persona_id}.jsonl"));
        let store = OWNERS.acquire(&path, |path| {
            Store::open(
                path.parent()
                    .ok_or_else(|| std::io::Error::other("capture directory missing"))?,
                persona_id,
            )
        })?;
        Ok(Self {
            persona_id,
            session_id: Uuid::new_v4(),
            store,
        })
    }
    pub fn with_session(mut self, session_id: Uuid) -> Self {
        self.session_id = session_id;
        self
    }
    fn append(
        &self,
        value: &impl Serialize,
        header: CallHeader,
        terminal: bool,
    ) -> Option<CallHeader> {
        let result = self
            .store
            .lock()
            .map_err(|_| std::io::Error::other("capture writer lock poisoned"))
            .and_then(|mut store| {
                match store.append(value, header.clone(), terminal) {
                    Ok(reference) => Ok(reference),
                    Err(error) => {
                        // Capacity failure leaves an explicit small receipt in the
                        // same owner. IO/index failures may also prevent this; the
                        // reader then exposes the unindexed tail/generation error.
                        let failure = serde_json::json!({ "schema_version": 4,
                            "event": "capture_incomplete", "request_id": header.request_id, "session_id": header.session_id,
                            "error": error.to_string() });
                        let mut failed = header;
                        failed.status = CallStatus::Incomplete;
                        let _ = store.append(&failure, failed, terminal);
                        Err(error)
                    }
                }
            });
        match result {
            Ok(reference) => Some(reference),
            Err(error) => {
                tracing::warn!(target: "cognition::capture", persona = %self.persona_id, %error,
                    "prompt lifecycle capture incomplete");
                None
            }
        }
    }
}
impl PromptCaptureSink for JsonlPromptCaptureSink {
    fn lifecycle_enabled(&self) -> bool {
        true
    }
    fn submitted(
        &self,
        call: &PromptCall,
        request: &TextGenerationRequest,
    ) -> Option<CaptureToken> {
        // A pre-1970 clock reads 0 here: an unsigned timestamp cannot go lower; zero marks that boundary.
        let captured_at_ms = EpochMs::now().millis();
        let header = CallHeader {
            request_id: call.request_id.clone(),
            session_id: self.session_id,
            cycle_id: call.cycle_id,
            room_id: call.room_id,
            cause: call.cause.into(),
            cause_root: call.cause_root,
            captured_at_ms,
            started_at_ms: captured_at_ms,
            status: CallStatus::Submitted,
            model: request.model.clone(),
            request: PayloadRef {
                generation: Uuid::nil(),
                offset: 0,
                bytes: 0,
            },
            terminal: None,
            cursor: String::new(),
        };
        let record = SubmittedRecord {
            schema_version: 4,
            event: "submitted",
            captured_at_ms,
            session_id: header.session_id,
            build_sha: env!("CONTINUUM_BUILD_GIT_SHA"),
            call,
            request,
        };
        let header = self.append(&record, header, false)?;
        Some(CaptureToken { header })
    }
    fn terminal(
        &self,
        token: &CaptureToken,
        status: CallStatus,
        response: Option<&TextGenerationResponse>,
        error: Option<&str>,
        elapsed_ms: u64,
    ) -> Option<String> {
        let mut header = token.header.clone();
        header.status = status;
        header.captured_at_ms = EpochMs::now().millis();
        let record = TerminalRecord {
            schema_version: 4,
            event: &header.status,
            captured_at_ms: header.captured_at_ms,
            session_id: header.session_id,
            request_id: &header.request_id,
            request_ref: &header.request,
            elapsed_ms,
            response,
            error,
            wire: continuum_engine::inference::wire_tap::take(&header.request_id),
            wire_omitted: None,
        };
        let record = within_row_limit(record, store::MAX_PAYLOAD_BYTES);
        self.append(&record, header.clone(), true)
            .map(|recorded| recorded.cursor)
            .filter(|cursor| !cursor.is_empty())
    }
    fn record(
        &self,
        persona_id: Uuid,
        room_id: Uuid,
        cause: &'static str,
        iteration: usize,
        system: &str,
        messages: &[ChatMessage],
        offered_tools: &[String],
        response: &TextGenerationResponse,
    ) {
        let captured_at_ms = EpochMs::now().millis();
        let record = LegacyRecord {
            schema_version: 3,
            captured_at_ms,
            persona_id,
            room_id,
            cause,
            iteration,
            system,
            messages,
            offered_tools,
            response,
        };
        let header = CallHeader {
            request_id: response.request_id.clone(),
            session_id: Uuid::nil(),
            cycle_id: None,
            room_id,
            cause: cause.into(),
            cause_root: None,
            captured_at_ms,
            started_at_ms: captured_at_ms,
            status: CallStatus::Legacy,
            model: Some(response.model.clone()),
            request: PayloadRef {
                generation: Uuid::nil(),
                offset: 0,
                bytes: 0,
            },
            terminal: None,
            cursor: String::new(),
        };
        self.append(&record, header, false);
    }
}

/// One recorded call read back from its capture: the exact request she was served
/// and, when the call finished, the exact response she gave. The single decode of a
/// schema-4 capture; replay and the training lifter both read through it.
pub struct CapturedCall {
    pub context_window: Option<u32>,
    pub request: TextGenerationRequest,
    pub response: Option<TextGenerationResponse>,
}

/// Decode a capture's detail into its typed call. Refuses a capture with integrity
/// issues or an older schema rather than guessing at its shape.
pub fn decode(mut detail: PlaybackDetail) -> Result<CapturedCall, String> {
    if !detail.issues.is_empty() {
        return Err(format!("capture integrity: {}", detail.issues.join("; ")));
    }
    let mut submitted = detail
        .submitted
        .take()
        .ok_or("capture has no submitted request")?;
    if submitted.get("schema_version").and_then(|v| v.as_u64()) != Some(4) {
        return Err("unsupported captured-request schema; schema 4 is required".into());
    }
    let context_window: Option<u32> = serde_json::from_value(submitted["context_window"].take()) // BOUNDARY: decode the persisted capture header, preserving unknown window as None.
        .map_err(|e| format!("captured context window: {e}"))?;
    let request: TextGenerationRequest = serde_json::from_value(submitted["request"].take()) // BOUNDARY: the on-disk request payload into the adapter's typed input.
        .map_err(|e| format!("captured request: {e}"))?;
    let response = match detail.terminal.as_mut().and_then(|v| v.get_mut("response")) {
        Some(value) if !value.is_null() => Some(
            serde_json::from_value::<TextGenerationResponse>(value.take()) // BOUNDARY: the persisted terminal response.
                .map_err(|e| format!("captured response: {e}"))?,
        ),
        _ => None,
    };
    Ok(CapturedCall {
        context_window,
        request,
        response,
    })
}

/// Old combined rows already cross the JSON compatibility boundary. Inspect
/// only their typed status fields, without cloning/deserializing the transcript.
fn recorded_response_failed(response: &serde_json::Value) -> bool {
    let finish = response
        .get("finishReason")
        .or_else(|| response.get("finish_reason"));
    finish.is_some_and(|value| {
        <continuum_engine::ai::types::FinishReason as serde::Deserialize<'_>>::deserialize(value)
            .map_or(true, |reason| {
                reason == continuum_engine::ai::types::FinishReason::Error
            })
    }) || response.get("error").is_some_and(|error| !error.is_null())
}

/// Compatibility projection for existing prompt inspection and dataset miners.
/// Only a completed response is a training candidate. Failed/cancelled/pending
/// calls remain visible through playback, never promoted to completed examples.
pub fn legacy_projection(
    detail: PlaybackDetail,
) -> std::io::Result<Option<serde_json::Value>> {
    let malformed =
        || std::io::Error::other("completed capture payload is missing required structure");
    let mut submitted = detail.submitted.ok_or_else(malformed)?;
    if matches!(detail.header.status, CallStatus::Legacy) {
        if submitted
            .get("response")
            .is_some_and(recorded_response_failed)
        {
            return Ok(None);
        }
        if let Some(record) = submitted.as_object_mut() {
            record.insert("provenance".into(), "legacy_missing".into());
        }
        return Ok(Some(submitted));
    }
    if !matches!(detail.header.status, CallStatus::Completed) {
        return Ok(None);
    }
    let mut terminal = detail.terminal.ok_or_else(malformed)?;
    if terminal
        .get("response")
        .is_some_and(recorded_response_failed)
    {
        return Ok(None);
    }
    let request = submitted.get_mut("request").ok_or_else(malformed)?.take();
    let mut record = submitted.as_object().ok_or_else(malformed)?.clone();
    record.remove("event");
    record.insert(
        "system".into(),
        request
            .get("systemPrompt")
            .cloned()
            .unwrap_or(serde_json::Value::String(String::new())), // Legacy combined records represent an absent optional system prompt as empty.
    );
    record.insert(
        "messages".into(),
        request.get("messages").ok_or_else(malformed)?.clone(),
    );
    record.insert(
        "response".into(),
        terminal.get_mut("response").ok_or_else(malformed)?.take(),
    );
    record.insert("iteration".into(), 0.into());
    record.insert(
        "offered_tools".into(),
        serde_json::Value::Array(
            request
                .get("tools")
                .and_then(|v| v.as_array())
                .into_iter()
                .flatten()
                .filter_map(|tool| tool.get("name").cloned())
                .collect(),
        ),
    );
    record.insert("request".into(), request);
    if !detail.issues.is_empty() {
        record.insert("capture_issues".into(), serde_json::json!(detail.issues));
    }
    Ok(Some(record.into()))
}

/// Compatibility reader for a single capture segment. Index scans are bounded
/// by retention; payload IO is bounded separately. Offline dataset consumers use
/// the same completed-call projection instead of guessing lifecycle joins.
pub fn completed_file(path: &Path, limit: usize) -> std::io::Result<Vec<serde_json::Value>> {
    use std::io::Read;
    const READ_BUDGET: u64 = 64 * 1024 * 1024;
    let limit = limit.max(1);
    let mut file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    if !path.with_extension("pidx").try_exists()? {
        if file.metadata()?.len() > READ_BUDGET {
            return Err(std::io::Error::other("legacy capture exceeds bounded read capacity; split/archive the capture before importing"));
        }
        let mut text = String::new();
        (&mut file)
            .take(READ_BUDGET + 1)
            .read_to_string(&mut text)?;
        if text.len() as u64 > READ_BUDGET {
            return Err(std::io::Error::other(
                "legacy capture grew beyond bounded read capacity",
            ));
        }
        let mut result = Vec::new();
        for line in text.lines().rev().filter(|line| !line.trim().is_empty()) {
            let mut value: serde_json::Value = serde_json::from_str(line)?;
            if value.get("schema_version").and_then(|v| v.as_u64()) == Some(4) {
                return Err(std::io::Error::other(
                    "lifecycle capture has no index; complete history is unavailable",
                ));
            }
            if value
                .get("response")
                .is_some_and(|response| !recorded_response_failed(response))
            {
                if let Some(record) = value.as_object_mut() {
                    record.insert("provenance".into(), "legacy_missing".into());
                }
                result.push(value);
            }
            if result.len() >= limit {
                break;
            }
        }
        result.reverse();
        return Ok(result);
    }
    let filename = path
        .file_stem()
        .and_then(|v| v.to_str())
        .ok_or_else(|| std::io::Error::other("invalid capture file name"))?;
    let persona = Uuid::parse_str(filename.strip_suffix(".prev").unwrap_or(filename)) // Both current and previous segment names encode the same persona UUID.
        .map_err(|_| std::io::Error::other("capture file name must identify its persona"))?;
    let dir = path
        .parent()
        .ok_or_else(|| std::io::Error::other("capture directory missing"))?;
    let generation = store::file_generation(path)?;
    let mut position = None;
    let mut calls = Vec::new();
    let mut bytes = 0_u64;
    loop {
        let page = page(dir, persona, position.as_deref(), false, store::PAGE_LIMIT)?;
        for header in page.entries.into_iter().rev().filter(|header| {
            header.request.generation == generation
                || header
                    .terminal
                    .as_ref()
                    .is_some_and(|terminal| terminal.generation == generation)
        }) {
            let own_generation = header
                .terminal
                .as_ref()
                .unwrap_or(&header.request) // A submission/legacy entry owns its request payload; terminal entries own their terminal payload.
                .generation;
            if own_generation != generation
                || !matches!(header.status, CallStatus::Completed | CallStatus::Legacy)
            {
                continue;
            }
            bytes = bytes
                .saturating_add(header.request.bytes)
                .saturating_add(header.terminal.as_ref().map_or(0, |v| v.bytes));
            if bytes > READ_BUDGET {
                return Err(std::io::Error::other(
                    "completed capture page exceeds payload read capacity; request fewer calls",
                ));
            }
            let captured = detail(dir, persona, &header.cursor)?;
            if captured.submitted.is_none()
                || (matches!(captured.header.status, CallStatus::Completed)
                    && captured.terminal.is_none())
            {
                return Err(std::io::Error::other(format!(
                    "completed capture payload unavailable: {}",
                    captured.issues.join("; ")
                )));
            }
            // A retained legacy row can contain a typed provider failure even
            // though old capture code called every Ok(response) completed.
            if let Some(value) = legacy_projection(captured)? {
                calls.push(value);
            }
            if calls.len() >= limit {
                break;
            }
        }
        if calls.len() >= limit || page.older.is_none() {
            break;
        }
        position = page.older;
    }
    calls.reverse();
    Ok(calls)
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: a turn in her mind room landing in the prompt capture (and so in
    // every SFT dataset built from it). Sink 2 of PRIVACY-OF-THOUGHT.md: zero bytes, through
    // start, finish AND the lease's Drop; an ordinary room still captures.
    #[test]
    fn a_turn_in_her_mind_room_writes_zero_capture_bytes() {
        let dir = tempfile::tempdir().expect("capture fixture directory");
        let persona = Uuid::new_v4();
        let sink: Arc<dyn PromptCaptureSink> =
            Arc::new(JsonlPromptCaptureSink::open(dir.path(), persona).expect("capture owner"));
        let call = |room| PromptCall {
            request_id: "r".into(),
            persona_id: persona,
            room_id: room,
            cycle_id: Some(1),
            context_window: Some(8192),
            cause: "synthetic",
            cause_root: None,
            replay_of: None,
        };
        let request = TextGenerationRequest {
            messages: vec![ChatMessage::text("user", "a private thought")],
            ..Default::default()
        };
        // `fs::metadata` on the path, never `DirEntry::metadata`: on Windows the listing
        // reports a file's size as of open while the store's writer holds it, so a write
        // would be invisible and the privacy assertion below could pass falsely (BigMama).
        let bytes = |dir: &std::path::Path| -> u64 {
            std::fs::read_dir(dir)
                .expect("dir")
                .filter_map(|e| std::fs::metadata(e.ok()?.path()).ok())
                .map(|m| m.len())
                .sum()
        };
        // Opening the store may write its own index; the turn must add nothing beyond it.
        let opened = bytes(dir.path());
        let mind = continuum_rooms::mind_room::mind_room_id(persona);
        let mut finished = CaptureLease::start(Arc::clone(&sink), &call(mind), &request);
        assert_eq!(finished.finish(None, Some("done")), None, "an inert lease names no entry");
        drop(CaptureLease::start(Arc::clone(&sink), &call(mind), &request));
        assert_eq!(bytes(dir.path()), opened, "her mind room: nothing captured, even by Drop");
        drop(CaptureLease::start(Arc::clone(&sink), &call(Uuid::new_v4()), &request));
        assert!(bytes(dir.path()) > opened, "an ordinary room still captures");
    }
    // what this catches: a recorded call whose wrong tool call cannot be traced to the model
    // or to the parse (card fb6d7912: Kimi's 'x' calls, no raw tokens anywhere). A recorded
    // call's terminal entry carries its raw stream payloads in order, the watch is released
    // when the call ends, and a mind-room call never arms one.
    #[test]
    fn a_recorded_call_keeps_its_raw_stream_in_its_terminal_entry() {
        let dir = tempfile::tempdir().expect("capture fixture directory");
        let persona = Uuid::new_v4();
        let sink: Arc<dyn PromptCaptureSink> =
            Arc::new(JsonlPromptCaptureSink::open(dir.path(), persona).expect("capture owner"));
        let request_id = format!("wire-{}", Uuid::new_v4());
        let call = |room, id: &str| PromptCall {
            request_id: id.into(),
            persona_id: persona,
            room_id: room,
            cycle_id: Some(1),
            context_window: Some(8192),
            cause: "synthetic",
            cause_root: None,
            replay_of: None,
        };
        let request = TextGenerationRequest {
            messages: vec![ChatMessage::text("user", "close the stray cards")],
            ..Default::default()
        };
        let mut lease = CaptureLease::start(Arc::clone(&sink), &call(Uuid::new_v4(), &request_id), &request);
        let mut stream = continuum_engine::inference::wire_tap::WireTap::open(Some(&request_id));
        stream.push(r#"{"choices":[{"delta":{"reasoning_content":"close the stray cards"}}]}"#);
        stream.push(r#"{"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"name":"work/create","arguments":"{\"title\":\"x\"}"}}]}}]}"#);
        drop(stream);
        let cursor = lease.finish(None, Some("parse refused")).expect("the call's end is recorded");
        let terminal = detail(dir.path(), persona, &cursor)
            .expect("read back")
            .terminal
            .expect("terminal entry");
        let attempts = terminal["wire"]["attempts"].as_array().expect("the raw stream is in the entry");
        assert_eq!(attempts.len(), 1, "one attempt");
        let events = attempts[0].as_array().unwrap();
        assert_eq!(events.len(), 2);
        assert!(events[1].as_str().unwrap().contains("work/create"), "as received, in order");
        assert_eq!(terminal["wire"]["open_at_end"], false);
        assert_eq!(continuum_engine::inference::wire_tap::take(&request_id), None, "released when the call ended");

        // A stream too large for a row is dropped from the entry and said, never allowed to
        // make the response itself unrecordable.
        let header_ref = PayloadRef { generation: Uuid::nil(), offset: 0, bytes: 0 };
        let status = CallStatus::Completed;
        let big = || TerminalRecord {
            schema_version: 4,
            event: &status,
            captured_at_ms: 0,
            session_id: Uuid::nil(),
            request_id: "r",
            request_ref: &header_ref,
            elapsed_ms: 0,
            response: None,
            error: None,
            wire: Some(continuum_engine::inference::wire_tap::Wire {
                attempts: vec![vec!["data: ".to_string() + &"t".repeat(4096)]],
                open_at_end: false,
            }),
            wire_omitted: None,
        };
        let json = serde_json::to_vec(&big()).expect("json").len() as u64;
        let at_json = within_row_limit(big(), json);
        assert!(at_json.wire.is_none(), "JSON exactly at the limit: its newline does not fit");
        assert!(at_json.wire_omitted.as_deref().is_some_and(|why| why.contains("row limit")));
        let with_newline = within_row_limit(big(), store::row_bytes(json));
        assert!(with_newline.wire.is_some() && with_newline.wire_omitted.is_none(), "JSON plus newline fits");

        let private_id = format!("wire-{}", Uuid::new_v4());
        let mind = continuum_rooms::mind_room::mind_room_id(persona);
        drop(CaptureLease::start(Arc::clone(&sink), &call(mind, &private_id), &request));
        let mut thought = continuum_engine::inference::wire_tap::WireTap::open(Some(&private_id));
        thought.push("a private token");
        drop(thought);
        assert_eq!(continuum_engine::inference::wire_tap::take(&private_id), None, "her mind room arms nothing");
    }
    #[test]
    fn concurrent_capture_wrappers_share_one_owner_and_keep_distinct_sessions() {
        let dir = tempfile::tempdir().expect("capture fixture directory");
        let persona = Uuid::new_v4();
        let first = JsonlPromptCaptureSink::open(dir.path(), persona).expect("first capture owner");
        let make_call = |id: &str| PromptCall {
            request_id: id.into(),
            persona_id: persona,
            room_id: Uuid::nil(),
            cycle_id: Some(1),
            context_window: Some(8192),
            cause: "synthetic",
            cause_root: None,
            replay_of: None,
        };
        let request = TextGenerationRequest {
            request_id: Some("first".into()),
            system_prompt: Some("actual identity".into()),
            messages: vec![ChatMessage::text("user", "original task")],
            temperature: Some(0.3),
            ..Default::default()
        };
        let first_token = first
            .submitted(&make_call("first"), &request)
            .expect("submitted first");
        // An eval fork is assembled while the first model call is still running.
        let second =
            JsonlPromptCaptureSink::open(dir.path(), persona).expect("fork capture wrapper");
        assert!(
            Arc::ptr_eq(&first.store, &second.store),
            "fork must not open or rotate beneath live writer"
        );
        assert_ne!(
            first.session_id, second.session_id,
            "construction sessions disambiguate restarted cycle counters"
        );
        let second_request = TextGenerationRequest {
            request_id: Some("second".into()),
            ..request.clone()
        };
        let second_token = second
            .submitted(&make_call("second"), &second_request)
            .expect("submitted second");
        second.terminal(
            &second_token,
            CallStatus::Failed,
            None,
            Some("fixture failure"),
            2,
        );
        first.terminal(
            &first_token,
            CallStatus::Cancelled,
            None,
            Some("fixture cancellation"),
            4,
        );
        let page = page(dir.path(), persona, None, false, 10).expect("interleaved lifecycle");
        assert_eq!(page.entries.len(), 4);
        assert!(page.issues.is_empty());
        assert_eq!(
            std::fs::read_dir(dir.path())
                .expect("retained files")
                .count(),
            2,
            "fork did not rotate"
        );
        for entry in page.entries.iter().skip(2) {
            let detail =
                detail(dir.path(), persona, &entry.cursor).expect("matching interleaved request");
            assert!(detail.issues.is_empty());
            let submitted = detail.submitted.expect("retained request");
            assert_eq!(submitted["request_id"], entry.request_id);
            assert_eq!(submitted["session_id"], entry.session_id.to_string());
        }
        drop(first);
        drop(second);
        // A new owner after the last drop (a core restart) continues the whole segment: her
        // history survives the restart instead of rolling to `.prev` (prompt_capture_store
        // `resume`), and every row stays readable.
        let reopened =
            JsonlPromptCaptureSink::open(dir.path(), persona).expect("new owner after last drop");
        assert_eq!(
            std::fs::read_dir(dir.path())
                .expect("retained files")
                .count(),
            2,
            "a restart continues the segment"
        );
        let after = store::page(dir.path(), persona, None, false, 10).expect("history across the restart");
        assert_eq!(after.entries.len(), 4);
        assert!(after.issues.is_empty(), "{:?}", after.issues);
        drop(reopened);
    }

    #[test]
    fn pre_lifecycle_capture_projection_marks_missing_provenance() {
        let dir = tempfile::tempdir().expect("capture fixture directory");
        let path = dir.path().join(format!("{}.jsonl", Uuid::new_v4()));
        std::fs::write(&path, "{\"schema_version\":3,\"system\":\"identity\",\"messages\":[],\"response\":{\"text\":\"answer\"}}\n")
            .expect("legacy capture");
        let rows = completed_file(&path, 1).expect("bounded legacy reader");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["response"]["text"], "answer");
        assert_eq!(rows[0]["provenance"], "legacy_missing");
    }

    #[test]
    fn legacy_error_responses_are_not_completed_training_examples() {
        let dir = tempfile::tempdir().expect("legacy fixture directory");
        let path = dir.path().join("legacy.jsonl");
        let rows = [
            serde_json::json!({"finishReason": "error", "text": "partial provider output"}),
            serde_json::json!({"finishReason": "stop", "error": "provider failure", "text": "partial output"}),
            serde_json::json!({"finishReason": "stop", "error": null, "text": "valid answer"}),
            serde_json::json!({"finishReason": "length", "error": null, "text": "completed inference at its token limit"}),
        ].into_iter().map(|response| serde_json::json!({"schema_version": 3, "response": response}).to_string())
            .collect::<Vec<_>>().join("\n");
        std::fs::write(&path, rows).expect("legacy file");
        let rows = completed_file(&path, 10).expect("legacy compatibility projection");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["response"]["text"], "valid answer");
        assert_eq!(rows[1]["response"]["finishReason"], "length");
    }
}
