//! The existing deliberation capture owner: borrowed request submission before
//! inference, then a small terminal record. Playback reads never run cognition.
use crate::ai::types::{ChatMessage, TextGenerationRequest, TextGenerationResponse};
use serde::Serialize;
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[path = "capture_owner.rs"]
pub(super) mod owner;

#[path = "prompt_capture_store.rs"]
mod store;
pub(crate) use store::{detail, page};
pub use store::{CallHeader, CallStatus, PlaybackDetail, PlaybackPage};
use store::{PayloadRef, Store};
pub(crate) const FIXTURE_DIR: &str = ".continuum/fixtures/prompt-captures";

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0) // A pre-epoch clock has no representable unsigned timestamp; zero marks that boundary.
}

#[derive(Debug, Serialize)]
pub struct PromptCall {
    pub request_id: String,
    pub persona_id: Uuid,
    pub room_id: Uuid,
    pub cycle_id: Option<u64>,
    pub context_window: u32,
    pub cause: &'static str,
    pub cause_root: Option<Uuid>,
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
    fn terminal(
        &self,
        _token: &CaptureToken,
        _status: CallStatus,
        _response: Option<&TextGenerationResponse>,
        _error: Option<&str>,
        _elapsed_ms: u64,
    ) {
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
pub(crate) struct CaptureLease {
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
        let token = sink.submitted(call, request);
        Self {
            sink,
            token,
            started: std::time::Instant::now(),
        }
    }
    pub fn finish(&mut self, response: Option<&TextGenerationResponse>, error: Option<&str>) {
        if let Some(token) = self.token.take() {
            self.sink.terminal(
                &token,
                if response.is_some_and(|response| response.generation_error().is_none()) {
                    CallStatus::Completed
                } else {
                    CallStatus::Failed
                },
                response,
                error,
                self.started.elapsed().as_millis() as u64,
            );
        }
    }
}
impl Drop for CaptureLease {
    fn drop(&mut self) {
        if let Some(token) = self.token.take() {
            self.sink.terminal(
                &token,
                CallStatus::Cancelled,
                None,
                Some("request future dropped before a terminal response"),
                self.started.elapsed().as_millis() as u64,
            );
        }
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
    pub(crate) fn open_for_persona(persona_id: Uuid) -> std::io::Result<Self> {
        let dir = crate::persona::recorder::fixture_dir(FIXTURE_DIR).ok_or_else(|| {
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
    pub(crate) fn with_session(mut self, session_id: Uuid) -> Self {
        self.session_id = session_id;
        self
    }
    fn append(
        &self,
        value: &impl Serialize,
        header: CallHeader,
        terminal: bool,
    ) -> Option<PayloadRef> {
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
        let captured_at_ms = now_ms();
        let mut header = CallHeader {
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
        header.request = self.append(&record, header.clone(), false)?;
        Some(CaptureToken { header })
    }
    fn terminal(
        &self,
        token: &CaptureToken,
        status: CallStatus,
        response: Option<&TextGenerationResponse>,
        error: Option<&str>,
        elapsed_ms: u64,
    ) {
        let mut header = token.header.clone();
        header.status = status;
        header.captured_at_ms = now_ms();
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
        };
        self.append(&record, header.clone(), true);
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
        let captured_at_ms = now_ms();
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

/// Old combined rows already cross the JSON compatibility boundary. Inspect
/// only their typed status fields, without cloning/deserializing the transcript.
fn recorded_response_failed(response: &serde_json::Value) -> bool {
    let finish = response
        .get("finishReason")
        .or_else(|| response.get("finish_reason"));
    finish.is_some_and(|value| {
        <crate::ai::types::FinishReason as serde::Deserialize<'_>>::deserialize(value)
            .map_or(true, |reason| {
                reason == crate::ai::types::FinishReason::Error
            })
    }) || response.get("error").is_some_and(|error| !error.is_null())
}

/// Compatibility projection for existing prompt inspection and dataset miners.
/// Only a completed response is a training candidate. Failed/cancelled/pending
/// calls remain visible through playback, never promoted to completed examples.
pub(crate) fn legacy_projection(
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
pub(crate) fn completed_file(path: &Path, limit: usize) -> std::io::Result<Vec<serde_json::Value>> {
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
            context_window: 8192,
            cause: "synthetic",
            cause_root: None,
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
        let reopened =
            JsonlPromptCaptureSink::open(dir.path(), persona).expect("new owner after last drop");
        assert_eq!(
            std::fs::read_dir(dir.path())
                .expect("rotated files")
                .count(),
            4
        );
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
