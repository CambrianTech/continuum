//! The one-time move of earlier builds' JSONL prompt captures into frames (card 699578cb, slice 5).
//!
//! Earlier builds wrote one `<persona>.jsonl` per persona (and a rotated `<persona>.prev.jsonl`),
//! flat in the same prompt-captures directory the frames now live under, with a `.pidx` sidecar
//! that only located rows. Every row is self-describing, so the move reads the rows alone, oldest
//! segment first, line by line (never a whole file in memory), and writes each as the frame the
//! live writer would have written, with its original capture time. A persona's JSONL files are
//! removed only after every call moved reads back through the call index; a persona that does not
//! verify keeps its files and is said, and the next start tries again.
//!
//! Rows, by what they carry:
//! - schema 4 `submitted`: a call's request -> a submission frame;
//! - schema 4 `completed` / `failed` / `cancelled`: its end -> an end frame;
//! - schema 4 `capture_incomplete`: a receipt of a record that could not be written -> an incomplete
//!   frame (it carried no time of its own; it is stamped with the row before it, written just before);
//! - schema 3 (no `event`): a pre-lifecycle call, request and response in one row -> a submission
//!   and a `Legacy` end, keyed by its response's request id.
//!
//! A row that does not parse is a torn tail only when it is the file's last line; anywhere else
//! the persona does not verify.

use std::collections::{BTreeSet, HashMap};
use std::io::{self, BufRead};
use std::path::PathBuf;

use continuum_engine::ai::types::{ChatMessage, TextGenerationRequest, TextGenerationResponse};
use continuum_engine::inference::wire_tap::Wire;
use continuum_kernel::system_resources::disk_reporters::ClassWriter;
use serde::Deserialize;
use uuid::Uuid;

use crate::call_frames::{CallKey, CallStatus, Ending, IncompleteHead, ReplaySource, Submission, SubmittedHead, TerminalHead};
use crate::call_index::CallIndex;
use crate::capture_log::{CaptureRecord, LogError, OpenLogs};
use crate::frame::FrameError;

/// Which kind of row a line is.
#[derive(Deserialize)]
struct Kind {
    schema_version: u32,
    event: Option<String>,
}

#[derive(Deserialize)]
struct SubmittedRow {
    captured_at_ms: u64,
    session_id: Uuid,
    build_sha: String,
    request_id: String,
    room_id: Uuid,
    cycle_id: Option<u64>,
    context_window: Option<u32>,
    cause: String,
    cause_root: Option<Uuid>,
    replay_of: Option<ReplaySource>,
    request: TextGenerationRequest,
}

#[derive(Deserialize)]
struct TerminalRow {
    event: CallStatus,
    captured_at_ms: u64,
    session_id: Uuid,
    request_id: String,
    elapsed_ms: u64,
    response: Option<TextGenerationResponse>,
    error: Option<String>,
    wire: Option<Wire>,
}

#[derive(Deserialize)]
struct IncompleteRow {
    session_id: Uuid,
    request_id: String,
    error: String,
}

#[derive(Deserialize)]
struct LegacyRow {
    captured_at_ms: u64,
    room_id: Uuid,
    cause: String,
    system: String,
    messages: Vec<ChatMessage>,
    response: TextGenerationResponse,
}

/// Why one row did not become frames.
#[derive(Debug, thiserror::Error)]
pub enum RowError {
    #[error("not a capture row: {0}")]
    Json(#[from] serde_json::Error),
    #[error(transparent)]
    Frame(#[from] FrameError),
    #[error("schema {schema} event {event:?} is not a capture row any build wrote")]
    Unknown { schema: u32, event: Option<String> },
}

/// Why a persona's captures did not move.
#[derive(Debug, thiserror::Error)]
pub enum MigrationError {
    #[error("capture row {line} of {}: {error}", .file.display())]
    Row { file: PathBuf, line: usize, error: RowError },
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error("the moved calls could not be read back: {0}")]
    Unindexed(#[from] LogError),
    #[error("{} moved but is not in the frames", crate::call_playback::cursor_of(.0))]
    Missing(CallKey),
}

/// One row as the frames it becomes, and the calls it names.
fn records(owner: Uuid, line: &str, previous_at: u64) -> Result<(Vec<CaptureRecord>, CallKey, u64), RowError> {
    let kind: Kind = serde_json::from_str(line)?;
    match (kind.schema_version, kind.event.as_deref()) {
        (4, Some("submitted")) => {
            let row: SubmittedRow = serde_json::from_str(line)?;
            let key = CallKey { session_id: row.session_id, request_id: row.request_id };
            let head = SubmittedHead {
                key: key.clone(),
                build_sha: row.build_sha,
                room_id: row.room_id,
                cycle_id: row.cycle_id,
                context_window: row.context_window,
                cause: row.cause,
                cause_root: row.cause_root,
                model: row.request.model.clone(),
                thread: None, // a JSONL row predates threads: none was recorded
            };
            let record = Submission { head, request: &row.request, replay: row.replay_of.as_ref() }.record(owner, row.captured_at_ms)?;
            Ok((vec![record], key, row.captured_at_ms))
        }
        (4, Some("capture_incomplete")) => {
            let row: IncompleteRow = serde_json::from_str(line)?;
            let key = CallKey { session_id: row.session_id, request_id: row.request_id };
            let record = IncompleteHead { key: key.clone(), error: row.error }.record(owner, previous_at)?;
            Ok((vec![record], key, previous_at))
        }
        (4, Some(_)) => {
            let row: TerminalRow = serde_json::from_str(line)?;
            let key = CallKey { session_id: row.session_id, request_id: row.request_id };
            let head = TerminalHead { key: key.clone(), status: row.event, elapsed_ms: row.elapsed_ms, error: row.error };
            let record = Ending { head, response: row.response.as_ref(), wire: row.wire.as_ref() }.record(owner, row.captured_at_ms)?;
            Ok((vec![record], key, row.captured_at_ms))
        }
        (3, None) => {
            let row: LegacyRow = serde_json::from_str(line)?;
            let key = CallKey { session_id: Uuid::nil(), request_id: row.response.request_id.clone() };
            let request = TextGenerationRequest { system_prompt: Some(row.system), messages: row.messages, ..Default::default() };
            let head = SubmittedHead {
                key: key.clone(),
                build_sha: String::new(), // a pre-lifecycle row named no build
                room_id: row.room_id,
                cycle_id: None,
                context_window: None,
                cause: row.cause,
                cause_root: None,
                model: Some(row.response.model.clone()),
                thread: None, // a JSONL row predates threads: none was recorded
            };
            let submitted = Submission { head, request: &request, replay: None }.record(owner, row.captured_at_ms)?;
            let ended = TerminalHead { key: key.clone(), status: CallStatus::Legacy, elapsed_ms: 0, error: None };
            let ending = Ending { head: ended, response: Some(&row.response), wire: None }.record(owner, row.captured_at_ms)?;
            Ok((vec![submitted, ending], key, row.captured_at_ms))
        }
        (schema, event) => Err(RowError::Unknown { schema, event: event.map(str::to_owned) }),
    }
}

/// The JSONL files of each persona in the prompt-captures directory, oldest segment first.
fn legacy_files(class: &ClassWriter) -> io::Result<HashMap<Uuid, Vec<PathBuf>>> {
    let mut found: HashMap<Uuid, Vec<PathBuf>> = HashMap::new();
    for name in class.files()? {
        let Some(stem) = name.to_str().and_then(|n| n.strip_suffix(".jsonl")) else { continue };
        let (persona, previous) = match stem.strip_suffix(".prev") {
            Some(persona) => (persona, true),
            None => (stem, false),
        };
        let Ok(persona) = Uuid::parse_str(persona) else { continue };
        let files = found.entry(persona).or_default();
        let path = class.path(&name)?;
        if previous { files.insert(0, path) } else { files.push(path) }
    }
    Ok(found)
}

/// Whether `path` ends in a newline: only a file that does not can end in a torn record.
fn ends_in_newline(path: &std::path::Path) -> io::Result<bool> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path)?;
    if file.metadata()?.len() == 0 {
        return Ok(true); // an empty file holds no torn record
    }
    file.seek(SeekFrom::End(-1))?;
    let mut last = [0u8; 1];
    file.read_exact(&mut last)?;
    Ok(last[0] == b'\n')
}

/// Move one persona's JSONL rows into frames through `write`, verify they read back, then remove
/// the JSONL files and their index sidecars. Returns the calls moved.
pub fn migrate_persona(
    class: &ClassWriter,
    persona: Uuid,
    files: &[PathBuf],
    write: &mut dyn FnMut(&CaptureRecord) -> io::Result<()>,
) -> Result<usize, MigrationError> {
    let mut moved = BTreeSet::new();
    for file in files {
        let torn_tail_possible = !ends_in_newline(file)?;
        let mut lines = io::BufReader::new(std::fs::File::open(file)?).lines().enumerate().peekable();
        let mut previous_at = 0;
        while let Some((at, line)) = lines.next() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            match records(persona, &line, previous_at) {
                Ok((records, key, at_ms)) => {
                    for record in &records {
                        write(record)?;
                    }
                    moved.insert(key);
                    previous_at = at_ms;
                }
                // a torn last line is a crash mid-write, never a call: only an UNTERMINATED one. A
                // complete line that does not parse is a record this build cannot read, kept with its file
                // (the move would otherwise delete the only copy; Codex on #6236).
                // and only a line cut short mid-JSON: a complete row of an unknown schema is kept (Codex).
                Err(RowError::Json(error)) if lines.peek().is_none() && torn_tail_possible && error.is_eof() => {}
                Err(error) => return Err(MigrationError::Row { file: file.clone(), line: at + 1, error }),
            }
        }
    }
    let mut index = CallIndex::new(class.owned_by(persona));
    index.refresh()?;
    let count = moved.len();
    if let Some(missing) = moved.into_iter().find(|key| index.get(key).is_none()) {
        return Err(MigrationError::Missing(missing));
    }
    for file in files {
        for path in [file.clone(), file.with_extension("pidx")] {
            match std::fs::remove_file(&path) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::NotFound => {} // a pre-index capture had no sidecar
                Err(error) => return Err(error.into()),
            }
        }
    }
    Ok(count)
}

/// Move every persona's JSONL captures under `class` into frames, appending through `logs` (the
/// writer's own open logs, so the move and the live writer never open competing generations).
/// Each persona is said moved or not; one that fails keeps its files for the next start.
pub fn migrate(class: &ClassWriter, logs: &OpenLogs) {
    let personas = match legacy_files(class) {
        Ok(personas) => personas,
        Err(error) => {
            continuum_base::probe!(class = "capture.migration.unlisted", error = %error, "the JSONL captures could not be listed; nothing moved this start");
            return;
        }
    };
    for (persona, files) in personas {
        let mut write = |record: &CaptureRecord| logs.append(class, record);
        match migrate_persona(class, persona, &files, &mut write) {
            Ok(calls) => continuum_base::probe!(class = "capture.migration.moved", persona = %persona, calls = calls as u64,
                "a persona's JSONL captures moved into frames, verified, and the JSONL removed"),
            Err(error) => continuum_base::probe!(class = "capture.migration.kept", persona = %persona, error = %error,
                "a persona's JSONL captures did not move; its files are kept for the next start"),
        }
    }
}

/// A persona's legacy file name in the class root, for a test that writes one.
#[cfg(test)]
fn legacy_name(persona: Uuid, previous: bool) -> PathBuf {
    PathBuf::from(format!("{persona}{}.jsonl", if previous { ".prev" } else { "" }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::call_playback::Playback;
    use std::num::NonZeroUsize;

    fn response(request_id: &str, text: &str) -> serde_json::Value {
        serde_json::json!({ "text": text, "finishReason": "stop", "model": "qwen", "provider": "local", "usage": { "inputTokens": 1, "outputTokens": 1, "totalTokens": 2, "estimatedCost": 0.0 },
            "responseTimeMs": 3, "requestId": request_id })
    }

    // what this catches: a migration that loses a call (a row kind it does not read, an end not
    // joined to its submission, the rotated segment skipped), that removes the JSONL before the
    // frames verify, that reads a torn last line as a failure, or that keeps an earlier build's
    // request or response other than as it was captured.
    #[test]
    fn every_capture_row_moves_into_frames_and_the_jsonl_goes_only_once_verified() {
        let dir = tempfile::tempdir().expect("test: dir");
        let class = crate::capture::fixture_class(dir.path());
        let persona = Uuid::new_v4();
        let session = Uuid::new_v4();
        let legacy = serde_json::json!({ "schema_version": 3, "captured_at_ms": 5, "persona_id": persona, "room_id": Uuid::new_v4(), "cause": "stimulus", "iteration": 0,
            "system": "You are Aris.", "messages": [{ "role": "user", "content": "old turn" }], "offered_tools": [], "response": response("old", "an old answer") });
        class.replace(&legacy_name(persona, true), format!("{legacy}\n").as_bytes()).expect("test: prev");
        let submitted = serde_json::json!({ "schema_version": 4, "event": "submitted", "captured_at_ms": 10, "session_id": session, "build_sha": "abc", "request_id": "r1",
            "persona_id": persona, "room_id": Uuid::new_v4(), "cycle_id": 3, "context_window": 32768, "cause": "chat", "cause_root": null,
            "request": { "messages": [{ "role": "user", "content": "close the stray cards" }], "maxTokens": 512 } });
        let ended = serde_json::json!({ "schema_version": 4, "event": "completed", "captured_at_ms": 12, "session_id": session, "request_id": "r1",
            "request_ref": { "generation": Uuid::nil(), "offset": 0, "bytes": 0 }, "elapsed_ms": 2, "response": response("r1", "closed 3"), "error": null,
            "wire": { "attempts": [["data: {}"]], "open_at_end": false } });
        let incomplete = serde_json::json!({ "schema_version": 4, "event": "capture_incomplete", "request_id": "r2", "session_id": session, "error": "row over the limit" });
        class.replace(&legacy_name(persona, false), format!("{submitted}\n{ended}\n{incomplete}\n{{\"schema_version\":4,\"ev").as_bytes()).expect("test: current");

        let logs = OpenLogs::default();
        let mut write = |record: &CaptureRecord| logs.append(&class, record);
        let files = legacy_files(&class).expect("test: list").remove(&persona).expect("test: the persona's files");
        assert_eq!(files.len(), 2, "both segments");
        assert_eq!(migrate_persona(&class, persona, &files, &mut write).expect("test: moved"), 3, "the legacy call, r1 and r2's receipt");
        let exists = |previous| class.path(&legacy_name(persona, previous)).expect("test: path").exists();
        assert!(!exists(false) && !exists(true), "the JSONL is gone once verified");

        let playback = Playback::at(dir.path(), persona);
        let page = playback.page(None, false, NonZeroUsize::new(10).expect("ten")).expect("test: page");
        let listed: Vec<(&str, CallStatus)> = page.entries.iter().map(|e| (e.request_id.as_str(), e.status)).collect();
        assert_eq!(listed, [("old", CallStatus::Legacy), ("r1", CallStatus::Completed)], "oldest first, at their original times");
        assert_eq!(page.issues.len(), 1, "r2 has a receipt and no submission: an issue, never a guessed header");
        let r1 = playback.detail(&page.entries[1].cursor).expect("test: r1");
        let submission = r1.submitted.expect("test: its request");
        assert_eq!((submission.head.context_window, submission.request.max_tokens), (Some(32_768), Some(512)));
        let end = r1.terminal.expect("test: its end");
        assert_eq!((end.response.map(|r| r.text), end.wire.map(|w| w.attempts.len())), (Some("closed 3".to_string()), Some(1)));
    }

    // what this catches: a migration that removes the JSONL while a row in the middle could not be
    // read, losing every call after it.
    #[test]
    fn a_persona_with_an_unreadable_row_keeps_its_files() {
        let dir = tempfile::tempdir().expect("test: dir");
        let class = crate::capture::fixture_class(dir.path());
        let persona = Uuid::new_v4();
        class.replace(&legacy_name(persona, false), b"not a row\n{\"schema_version\":3}\n").expect("test: current");
        let logs = OpenLogs::default();
        let mut write = |record: &CaptureRecord| logs.append(&class, record);
        let files = legacy_files(&class).expect("test: list").remove(&persona).expect("test: files");
        assert!(matches!(migrate_persona(&class, persona, &files, &mut write), Err(MigrationError::Row { line: 1, .. })));
        assert!(class.path(&legacy_name(persona, false)).expect("test: path").exists(), "kept for the next start");

        // what this catches (Codex on #6236): a complete last row this build cannot read, taken for a
        // torn tail because it is last, and its file deleted with the only copy of that record. Only
        // an unterminated last line is torn.
        let terminated = Uuid::new_v4();
        let legacy = serde_json::json!({ "schema_version": 3, "captured_at_ms": 5, "persona_id": terminated, "room_id": Uuid::new_v4(), "cause": "stimulus", "iteration": 0,
            "system": "s", "messages": [], "offered_tools": [], "response": response("old", "an old answer") });
        class.replace(&legacy_name(terminated, false), format!("{legacy}\n{{\"schema_version\":99}}\n").as_bytes()).expect("test: current");
        let files = legacy_files(&class).expect("test: list").remove(&terminated).expect("test: files");
        assert!(matches!(migrate_persona(&class, terminated, &files, &mut write), Err(MigrationError::Row { line: 2, .. })), "a complete unreadable last row is no torn tail");
        assert!(class.path(&legacy_name(terminated, false)).expect("test: path").exists(), "and its file is kept");

        // what this catches (Codex on #6236): a complete, valid JSON last row of a schema this build
        // does not know, taken for a torn tail because the file ends without a newline. Only JSON cut
        // short mid-row (a parse error at its end) is torn.
        let unknown = Uuid::new_v4();
        let legacy = serde_json::json!({ "schema_version": 3, "captured_at_ms": 5, "persona_id": unknown, "room_id": Uuid::new_v4(), "cause": "stimulus", "iteration": 0,
            "system": "s", "messages": [], "offered_tools": [], "response": response("old", "an old answer") });
        class.replace(&legacy_name(unknown, false), format!("{legacy}\n{{\"schema_version\":99}}").as_bytes()).expect("test: current");
        let files = legacy_files(&class).expect("test: list").remove(&unknown).expect("test: files");
        assert!(matches!(migrate_persona(&class, unknown, &files, &mut write), Err(MigrationError::Row { line: 2, .. })), "an unknown complete row is no torn tail, newline or not");
        assert!(class.path(&legacy_name(unknown, false)).expect("test: path").exists(), "and its file is kept");

        // what this catches: JSONL removed on the strength of writes that never landed (a writer
        // that reported success and wrote nothing): only frames that read back authorize removal.
        let other = Uuid::new_v4();
        let legacy = serde_json::json!({ "schema_version": 3, "captured_at_ms": 5, "persona_id": other, "room_id": Uuid::new_v4(), "cause": "stimulus", "iteration": 0,
            "system": "s", "messages": [], "offered_tools": [], "response": response("old", "an old answer") });
        class.replace(&legacy_name(other, false), format!("{legacy}\n").as_bytes()).expect("test: current");
        let files = legacy_files(&class).expect("test: list").remove(&other).expect("test: files");
        let mut lost = |_: &CaptureRecord| Ok(());
        assert!(matches!(migrate_persona(&class, other, &files, &mut lost), Err(MigrationError::Missing(_))));
        assert!(class.path(&legacy_name(other, false)).expect("test: path").exists(), "unverified: kept");
    }
}
