//! Persisted capture decoding and bounded playback, shared by replay and training.
use super::{store, detail, page, CallStatus, PlaybackDetail};
use continuum_engine::ai::types::{TextGenerationRequest, TextGenerationResponse};
use std::path::Path;
use uuid::Uuid;

#[derive(serde::Deserialize)]
#[serde(try_from = "u32")]
struct CaptureSchema4;
impl TryFrom<u32> for CaptureSchema4 {
    type Error = String;
    fn try_from(version: u32) -> Result<Self, Self::Error> {
        match version {
            4 => Ok(Self),
            other => Err(format!("unsupported capture schema {other}; supported schema is 4")),
        }
    }
}

#[derive(serde::Deserialize)]
struct SubmittedCapture {
    #[serde(rename = "schema_version")]
    _schema: CaptureSchema4,
    context_window: Option<u32>,
    request: TextGenerationRequest,
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
    let submitted: SubmittedCapture = serde_json::from_value( // BOUNDARY: deserialize the persisted capture submission.
        detail.submitted.take().ok_or("capture has no submitted request")?,
    )
    .map_err(|error| format!("captured submission: {error}"))?;
    let response = match detail.terminal.as_mut().and_then(|v| v.get_mut("response")) {
        Some(value) if !value.is_null() => Some(
            continuum_engine::ai::recorded_response::decode(value.take()) // BOUNDARY: the persisted terminal response, including pre-receipt history.
                .map_err(|e| format!("captured response: {e}"))?,
        ),
        _ => None,
    };
    Ok(CapturedCall {
        context_window: submitted.context_window,
        request: submitted.request,
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

/// A capture segment retains its directory instead of recovering it from a path.
/// External directory entries cross the single-file-name boundary once.
pub struct CaptureSegment {
    directory: std::path::PathBuf,
    path: std::path::PathBuf,
}
impl CaptureSegment {
    pub fn for_persona(directory: &Path, persona: Uuid, previous: bool) -> Self {
        let suffix = if previous { ".prev" } else { "" };
        Self { directory: directory.to_owned(), path: directory.join(format!("{persona}{suffix}.jsonl")) }
    }
    pub fn from_file_name(directory: &Path, name: impl AsRef<std::ffi::OsStr>) -> std::io::Result<Self> {
        let name = name.as_ref();
        let mut components = Path::new(name).components();
        match (components.next(), components.next()) {
            (Some(std::path::Component::Normal(part)), None) if part == name =>
                Ok(Self { directory: directory.to_owned(), path: directory.join(name) }),
            _ => Err(std::io::Error::new(std::io::ErrorKind::InvalidInput, "capture segment must be one file name")),
        }
    }
}

/// Compatibility reader for a single capture segment. Index scans are bounded
/// by retention; payload IO is bounded separately. Offline dataset consumers use
/// the same completed-call projection instead of guessing lifecycle joins.
pub fn completed_file(segment: &CaptureSegment, limit: usize) -> std::io::Result<Vec<serde_json::Value>> {
    use std::io::Read;
    let path = &segment.path;
    let dir = &segment.directory;
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
    fn segments_keep_the_directory_and_accept_only_one_file_name() {
        let dir = Path::new("capture-root");
        for name in ["", ".", "..", "../other", "nested/file", "/absolute", "file/"] {
            assert!(CaptureSegment::from_file_name(dir, name).is_err(), "{name:?}");
        }
        #[cfg(windows)]
        for name in [r"C:\absolute", r"nested\file", r"..\other", "C:relative"] {
            assert!(CaptureSegment::from_file_name(dir, name).is_err(), "{name:?}");
        }
        let legacy = CaptureSegment::from_file_name(dir, "legacy.jsonl").expect("single name");
        assert_eq!(legacy.directory, dir);
        assert_eq!(legacy.path, dir.join("legacy.jsonl"));
        let persona = Uuid::nil();
        for previous in [false, true] {
            let segment = CaptureSegment::for_persona(dir, persona, previous);
            let suffix = if previous { ".prev" } else { "" };
            assert_eq!(segment.directory, dir);
            assert_eq!(segment.path, dir.join(format!("{persona}{suffix}.jsonl")));
        }
    }

    #[test]
    fn submitted_capture_requires_a_supported_schema_and_typed_request() {
        let valid = serde_json::json!({
            "schema_version": 4,
            "context_window": null,
            "request": TextGenerationRequest::default(),
        });
        assert!(serde_json::from_value::<SubmittedCapture>(valid.clone()).is_ok());
        for version in [serde_json::Value::Null, serde_json::json!(3), serde_json::json!(5), serde_json::json!("4")] {
            let mut row = valid.clone();
            row["schema_version"] = version;
            assert!(serde_json::from_value::<SubmittedCapture>(row).is_err());
        }
        for field in ["schema_version", "request"] {
            let mut row = valid.clone();
            row.as_object_mut().expect("fixture object").remove(field);
            assert!(serde_json::from_value::<SubmittedCapture>(row).is_err());
        }
        let mut malformed = valid;
        malformed["request"] = serde_json::json!("not a request");
        assert!(serde_json::from_value::<SubmittedCapture>(malformed).is_err());
    }
}
