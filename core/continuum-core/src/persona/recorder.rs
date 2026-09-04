//! Per-turn cognition recorder. Writes a self-contained turn capture
//! (request + response + trace) from inside `respond()`, so EVERY host
//! that links the persona library — TS server, Unreal plugin, Swift
//! Vision Pro app, raw Rust binary — gets recordings for free without
//! depending on the host language for the recording mechanism itself.
//!
//! # Why this exists Rust-side
//!
//! Before this module, the fixture write lived in
//! `system/user/server/modules/PersonaResponseGenerator.ts` — fine for
//! the chat surface (Node host), useless for any non-Node embedding.
//! "If I wanted to put a persona inside an Unreal video game or AR/VR
//! system on Vision Pro, I'd want to be able to do so without Node"
//! — Joel, 2026-04-22.
//!
//! The recorder lives next to `respond()` so the act of running a
//! cognition turn is the act of recording it. Hosts can opt OUT via
//! the disable env var if they want to throw away recordings (perf
//! tests, ephemeral hosts), but the default is "always record" — F1
//! cars don't ship without telemetry either.
//!
//! # Format
//!
//! JSON, one file per turn at:
//!
//!   `~/.continuum/fixtures/persona-respond/<persona>-<msgid>-<ts>-rust.json`
//!
//! The `-rust` suffix marks the Rust-emitted capture. This is now the
//! single persona-turn fixture source: the TypeScript chat shim builds
//! the IPC request, but recording belongs beside `respond()` so non-Node
//! hosts get the same telemetry and replay corpus.
//!
//! Schema (`schemaVersion: 1`):
//! - `capturedAtMs` — wall-clock when the turn finished
//! - `personaId`, `personaName`, `messageId`, `roomId`, `model` —
//!   identity for joining + filtering
//! - `rustRequest` — echo of the input that drove the call
//! - `rustResponse` — `PersonaResponse` returned
//! - `cognitionTrace` — per-seam timing + metadata
//!
//! # FIFO trim
//!
//! The fixture dir is FIFO-trimmed at `FIXTURE_CAP_PER_DIR` (200)
//! entries. Recent slice without unbounded growth — replay tests run
//! against whatever's there; older captures drop when the cap fills.
//! Same policy the TS writer uses, kept aligned so neither side
//! produces a runaway dir.
//!
//! # Failure mode
//!
//! Recording is BEST-EFFORT. A failure to write the fixture must NOT
//! propagate as a cognition error — the persona's response is the
//! product, the recording is observability. Failures log a warning
//! and the turn returns its real result.

use crate::cognition::tool_executor::types::MediaItemLite;
use crate::persona::response::{PersonaResponse, RespondInput};
use crate::persona::trace::CognitionTrace;
use crate::persona::{
    PersonaTurnFrame, PersonaTurnFrameReplayRecord, PERSONA_TURN_FRAME_REPLAY_SCHEMA_VERSION,
};
use crate::runtime;
use serde::Serialize;
use serde_json::json;
use std::fmt;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Cap on captured fixtures per dir. Matches the TS writer's cap so
/// neither side independently blows the dir up. If you need a longer
/// retention window for incident analysis, copy fixtures out before
/// the cap rotates them.
const FIXTURE_CAP_PER_DIR: usize = 200;
const RESPOND_FIXTURE_DIR: &str = ".continuum/fixtures/persona-respond";
const TURN_FRAME_FIXTURE_DIR: &str = ".continuum/fixtures/persona-turn-frame";

/// Env var to fully disable recording. Set to `1` / `true` for hosts
/// that don't want disk writes (perf benchmarks, ephemeral CLI runs).
const DISABLE_ENV: &str = "CONTINUUM_DISABLE_TURN_RECORD";

/// Echo of the inbound request, with media base64 PRESERVED. Replay
/// tests replay the exact bytes, so stripping payload here would
/// neuter the test bench. Disk usage is bounded by the FIFO trim.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RequestEcho<'a> {
    persona_id: Uuid,
    persona_specialty: &'a str,
    persona_display_name: &'a str,
    room_id: Uuid,
    message_id: Uuid,
    message_text: &'a str,
    system_prompt: &'a str,
    model: &'a str,
    is_voice: bool,
    capabilities: Vec<String>,
    recent_history: Vec<RecentEcho<'a>>,
    message_media: Vec<MediaEcho<'a>>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct RecentEcho<'a> {
    id: Uuid,
    sender_name: &'a str,
    text: &'a str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct MediaEcho<'a> {
    item_type: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    base64: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mime_type: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    description: Option<&'a str>,
}

impl<'a> From<&'a RespondInput> for RequestEcho<'a> {
    fn from(input: &'a RespondInput) -> Self {
        let capabilities = input
            .capabilities
            .iter()
            .filter_map(|c| serde_json::to_value(c).ok())
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        Self {
            persona_id: input.persona.persona_id,
            persona_specialty: &input.persona.specialty,
            persona_display_name: &input.persona.display_name,
            room_id: input.turn_context.room_id,
            message_id: input.message_id,
            message_text: &input.message_text,
            system_prompt: &input.system_prompt,
            model: &input.model,
            is_voice: input.is_voice,
            capabilities,
            recent_history: input
                .turn_context
                .recent_history
                .iter()
                .map(|m| RecentEcho {
                    id: m.id,
                    sender_name: &m.sender_name,
                    text: &m.text,
                })
                .collect(),
            message_media: input.message_media.iter().map(media_echo).collect(),
        }
    }
}

fn media_echo(m: &MediaItemLite) -> MediaEcho<'_> {
    MediaEcho {
        item_type: &m.item_type,
        base64: m.base64.as_deref(),
        mime_type: m.mime_type.as_deref(),
        description: m.description.as_deref(),
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct TurnError {
    error_msg: String,
    last_completed_seam: Option<String>,
    partial_trace_seams: usize,
    total_ms: u64,
}

/// Persist a completed turn. Best-effort: failures log + return
/// `Ok(())` so a recording problem never breaks cognition.
pub fn record_turn(input: &RespondInput, response: &PersonaResponse, trace: &CognitionTrace) {
    let payload = json!({
        "schemaVersion": 1,
        "capturedAtMs": crate::persona::trace::now_ms(),
        "personaId": input.persona.persona_id,
        "personaName": input.persona.display_name,
        "messageId": input.message_id,
        "roomId": input.turn_context.room_id,
        "model": input.model,
        "rustRequest": RequestEcho::from(input),
        "rustResponse": response,
        "rustError": null,
        "cognitionTrace": trace,
    });
    persist_turn_payload(input, payload);
}

/// Persist a failed turn. `respond()` still returns `Err` to its caller; this
/// recorder-only artifact preserves the input and partial trace for replay.
pub fn record_failed_turn(
    input: &RespondInput,
    error_msg: &str,
    total_ms: u64,
    trace: &CognitionTrace,
) {
    let error = TurnError {
        error_msg: error_msg.to_string(),
        last_completed_seam: trace.last_seam_name().map(str::to_string),
        partial_trace_seams: trace.seam_count(),
        total_ms,
    };
    let payload = json!({
        "schemaVersion": 1,
        "capturedAtMs": crate::persona::trace::now_ms(),
        "personaId": input.persona.persona_id,
        "personaName": input.persona.display_name,
        "messageId": input.message_id,
        "roomId": input.turn_context.room_id,
        "model": input.model,
        "rustRequest": RequestEcho::from(input),
        "rustResponse": null,
        "rustError": error,
        "cognitionTrace": trace,
    });
    persist_turn_payload(input, payload);
}

/// Persist the per-persona inbox/RAG seed frame that preceded cognition.
///
/// This captures the inspectable Rust boundary before retrieval or model
/// inference runs: raw drained inbox frame, consolidated transcript, and the
/// deterministic RAG seed. It is intentionally separate from the completed
/// `respond()` capture so a stuck or skipped model turn still leaves replayable
/// evidence of what the persona saw.
pub fn record_turn_frame_replay(record: &PersonaTurnFrameReplayRecord) {
    if disabled() {
        return;
    }
    let dir = match fixture_dir(TURN_FRAME_FIXTURE_DIR) {
        Some(d) => d,
        None => return,
    };
    let fname = turn_frame_filename_for(record);
    persist_json_payload(&dir, &fname, record);
}

#[derive(Debug)]
pub enum TurnFrameReplayLoadError {
    Read {
        path: PathBuf,
        source: std::io::Error,
    },
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    UnsupportedSchema {
        path: PathBuf,
        expected: u32,
        actual: u32,
    },
    InvalidRecord {
        path: PathBuf,
        reason: String,
    },
}

impl fmt::Display for TurnFrameReplayLoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => {
                write!(
                    f,
                    "turn-frame fixture read failed for {}: {source}",
                    path.display()
                )
            }
            Self::Parse { path, source } => {
                write!(
                    f,
                    "turn-frame fixture parse failed for {}: {source}",
                    path.display()
                )
            }
            Self::UnsupportedSchema {
                path,
                expected,
                actual,
            } => write!(
                f,
                "turn-frame fixture {} has schemaVersion {actual}, expected {expected}",
                path.display()
            ),
            Self::InvalidRecord { path, reason } => {
                write!(
                    f,
                    "turn-frame fixture {} is invalid: {reason}",
                    path.display()
                )
            }
        }
    }
}

impl std::error::Error for TurnFrameReplayLoadError {}

/// Load and validate a Rust-owned turn-frame replay fixture.
///
/// Validation recomputes the derived consolidated inbox and RAG seed from the
/// raw inbox frame. A fixture whose derived fields do not match its raw frame is
/// rejected instead of being treated as replayable evidence.
pub fn load_turn_frame_replay_fixture(
    path: impl AsRef<Path>,
) -> Result<PersonaTurnFrameReplayRecord, TurnFrameReplayLoadError> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(|source| TurnFrameReplayLoadError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    let record: PersonaTurnFrameReplayRecord =
        serde_json::from_slice(&bytes).map_err(|source| TurnFrameReplayLoadError::Parse {
            path: path.to_path_buf(),
            source,
        })?;
    validate_turn_frame_replay_record(path, &record)?;
    Ok(record)
}

pub fn validate_turn_frame_replay_record(
    path: impl AsRef<Path>,
    record: &PersonaTurnFrameReplayRecord,
) -> Result<(), TurnFrameReplayLoadError> {
    let path = path.as_ref();
    if record.schema_version != PERSONA_TURN_FRAME_REPLAY_SCHEMA_VERSION {
        return Err(TurnFrameReplayLoadError::UnsupportedSchema {
            path: path.to_path_buf(),
            expected: PERSONA_TURN_FRAME_REPLAY_SCHEMA_VERSION,
            actual: record.schema_version,
        });
    }
    if record.persona_id != record.inbox_frame.persona_id {
        return invalid_record(path, "personaId does not match inboxFrame.personaId");
    }
    if record.room_id != record.inbox_frame.room_id {
        return invalid_record(path, "roomId does not match inboxFrame.roomId");
    }

    let turn_frame = PersonaTurnFrame::from_inbox_frame(record.inbox_frame.clone());
    let expected_consolidated =
        turn_frame
            .consolidated_inbox()
            .ok_or_else(|| TurnFrameReplayLoadError::InvalidRecord {
                path: path.to_path_buf(),
                reason: "inboxFrame is empty".to_string(),
            })?;
    if record.consolidated_inbox != expected_consolidated {
        return invalid_record(path, "consolidatedInbox does not match inboxFrame");
    }

    let expected_rag_seed =
        turn_frame
            .rag_seed()
            .ok_or_else(|| TurnFrameReplayLoadError::InvalidRecord {
                path: path.to_path_buf(),
                reason: "ragSeed cannot be derived from inboxFrame".to_string(),
            })?;
    if record.rag_seed != expected_rag_seed {
        return invalid_record(path, "ragSeed does not match inboxFrame");
    }

    Ok(())
}

fn invalid_record<T>(path: &Path, reason: &str) -> Result<T, TurnFrameReplayLoadError> {
    Err(TurnFrameReplayLoadError::InvalidRecord {
        path: path.to_path_buf(),
        reason: reason.to_string(),
    })
}

fn persist_turn_payload(input: &RespondInput, payload: serde_json::Value) {
    if disabled() {
        return;
    }
    let dir = match fixture_dir(RESPOND_FIXTURE_DIR) {
        Some(d) => d,
        None => return, // HOME unset; treat as opted-out, no warning spam
    };
    let fname = filename_for(&input.persona.display_name, input.message_id);
    persist_json_payload(&dir, &fname, &payload);
}

fn persist_json_payload<T: Serialize>(dir: &Path, fname: &str, payload: &T) {
    if let Err(e) = std::fs::create_dir_all(dir) {
        runtime::logger("recorder").warn_fmt(format_args!(
            "couldn't create fixture dir {}: {e} — recording skipped",
            dir.display()
        ));
        return;
    }
    let path = dir.join(fname);
    let serialized = match serde_json::to_vec_pretty(&payload) {
        Ok(b) => b,
        Err(e) => {
            runtime::logger("recorder")
                .warn_fmt(format_args!("turn capture serialize failed: {e}"));
            return;
        }
    };
    // Atomic write: tmp file + rename, so a crash mid-write leaves a
    // missing file rather than a half-written one that breaks parsers.
    let tmp_path = path.with_extension("json.tmp");
    if let Err(e) = std::fs::write(&tmp_path, &serialized) {
        runtime::logger("recorder").warn_fmt(format_args!(
            "turn capture write failed: {e} (target: {})",
            path.display()
        ));
        return;
    }
    if let Err(e) = std::fs::rename(&tmp_path, &path) {
        runtime::logger("recorder").warn_fmt(format_args!(
            "turn capture rename failed: {e} (target: {})",
            path.display()
        ));
        let _ = std::fs::remove_file(&tmp_path); // best-effort cleanup
        return;
    }
    trim_fifo(dir);
}

// Test-only per-thread overrides (#7). The recorder's tests used to mutate
// process-global HOME/DISABLE env under a private lock — but that lock only
// serialized RECORDER tests; every other test in the parallel suite that read
// HOME (or triggered a recorder write) raced the swap window, and a parallel
// write landing in the swapped tempdir broke this file's `len == 1` assertion.
// A thread-local override is parallel-safe by construction: `record_turn` runs
// on the caller's thread, so each test sees exactly its own root and the
// process environment is never touched.
#[cfg(test)]
thread_local! {
    static TEST_FIXTURE_ROOT: std::cell::RefCell<Option<PathBuf>> =
        const { std::cell::RefCell::new(None) };
    static TEST_DISABLED: std::cell::RefCell<Option<bool>> =
        const { std::cell::RefCell::new(None) };
}

fn disabled() -> bool {
    #[cfg(test)]
    if let Some(v) = TEST_DISABLED.with(|d| *d.borrow()) {
        return v;
    }
    std::env::var(DISABLE_ENV)
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE"))
        .unwrap_or(false)
}

fn fixture_dir(relative: &str) -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(root) = TEST_FIXTURE_ROOT.with(|r| r.borrow().clone()) {
        return Some(root.join(relative));
    }
    std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(relative))
}

/// Filename: `<persona>-<msgid_prefix>-<ts>-rust.json`. The `-rust`
/// suffix marks the Rust-owned capture. Persona name spaces collapsed
/// to underscores for filesystem safety.
fn filename_for(persona_name: &str, message_id: Uuid) -> String {
    let safe_name = persona_name.replace(char::is_whitespace, "_");
    let id_prefix: String = message_id.to_string().chars().take(8).collect();
    let ts = chrono_like_ts(crate::persona::trace::now_ms());
    format!("{safe_name}-{id_prefix}-{ts}-rust.json")
}

/// Filename: `frame-<persona_prefix>-<trigger_msg_prefix>-<ts>-rust.json`.
/// The trigger id ties the fixture to the consolidated frame without needing
/// a persona display name at this layer.
fn turn_frame_filename_for(record: &PersonaTurnFrameReplayRecord) -> String {
    let persona_prefix: String = record.persona_id.to_string().chars().take(8).collect();
    let trigger_prefix: String = record
        .consolidated_inbox
        .trigger_message_id
        .to_string()
        .chars()
        .take(8)
        .collect();
    let ts = chrono_like_ts(crate::persona::trace::now_ms());
    format!("frame-{persona_prefix}-{trigger_prefix}-{ts}-rust.json")
}

/// Build an ISO-8601-like compact timestamp from ms-since-epoch. We
/// avoid pulling chrono just for this — the format is filename-only,
/// not parseable round-trip.
fn chrono_like_ts(ms: u64) -> String {
    let secs = ms / 1000;
    let sub_ms = ms % 1000;
    // Approximate UTC components — for filename ordering only.
    // Days since epoch, then HH:MM:SS via integer math.
    let days = secs / 86_400;
    let secs_of_day = secs % 86_400;
    let h = secs_of_day / 3600;
    let m = (secs_of_day % 3600) / 60;
    let s = secs_of_day % 60;
    // Year 1970 + days approximation. Good enough for FIFO ordering;
    // not used for parsing.
    let year = 1970 + (days / 365);
    let day_of_year = days % 365;
    let month = (day_of_year / 30) + 1;
    let day = (day_of_year % 30) + 1;
    format!("{year:04}-{month:02}-{day:02}T{h:02}-{m:02}-{s:02}-{sub_ms:03}Z")
}

/// FIFO trim: drop the oldest captures (by mtime) until count <= cap.
/// Best-effort; logging-only on errors. Same algorithm the TS writer
/// uses so neither side rotates the dir out from under the other.
fn trim_fifo(dir: &Path) {
    let entries = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    let mut files: Vec<(PathBuf, std::time::SystemTime)> = entries
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|s| s.to_str()) != Some("json") {
                return None;
            }
            let mtime = e.metadata().ok()?.modified().ok()?;
            Some((p, mtime))
        })
        .collect();
    if files.len() <= FIXTURE_CAP_PER_DIR {
        return;
    }
    files.sort_by_key(|(_, t)| *t);
    let to_remove = files.len() - FIXTURE_CAP_PER_DIR;
    for (p, _) in files.into_iter().take(to_remove) {
        let _ = std::fs::remove_file(p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::PersonaSlot;
    use crate::persona::inbox::{PersonaInboxFrame, PersonaInboxFrameMetrics};
    use crate::persona::response::PersonaResponse;
    use crate::persona::{InboxMessage, Modality, PersonaTurnFrame, SenderType};
    use std::collections::HashSet;
    use tempfile::tempdir;

    fn fake_input() -> RespondInput {
        use crate::persona::turn_context::TurnContext;
        RespondInput {
            persona: PersonaSlot {
                persona_id: Uuid::nil(),
                specialty: "general".to_string(),
                display_name: "Test Persona".to_string(),
            },
            turn_context: TurnContext::arc(Uuid::nil(), vec![], vec!["general".to_string()]),
            message_id: Uuid::nil(),
            message_text: "hello".to_string(),
            other_persona_names: vec![],
            system_prompt: "you are helpful".to_string(),
            model: "test-model".to_string(),
            is_voice: false,
            message_media: vec![],
            capabilities: HashSet::new(),
            recalled_engrams: vec![],
            room_roster: vec![],
            room_doctrine: None,
        }
    }

    fn fake_response() -> PersonaResponse {
        PersonaResponse::Spoke {
            persona_id: Uuid::nil(),
            text: "hi".to_string(),
            model_used: "test".to_string(),
            inference_ms: 1,
            total_ms: 2,
            think_blocks_emitted: 0,
        }
    }

    fn fake_turn_frame_replay_record() -> PersonaTurnFrameReplayRecord {
        let persona_id = Uuid::new_v4();
        let room_id = Uuid::new_v4();
        let messages = vec![
            InboxMessage {
                id: Uuid::new_v4(),
                room_id,
                sender_id: Uuid::new_v4(),
                sender_name: "Operator".to_string(),
                sender_type: SenderType::Human,
                content: "what changed?".to_string(),
                timestamp: 10_000,
                priority: 0.9,
                source_modality: Some(Modality::Chat),
                voice_session_id: None,
            },
            InboxMessage {
                id: Uuid::new_v4(),
                room_id,
                sender_id: Uuid::new_v4(),
                sender_name: "Mira".to_string(),
                sender_type: SenderType::Persona,
                content: "the frame records replay state".to_string(),
                timestamp: 10_040,
                priority: 0.7,
                source_modality: Some(Modality::Chat),
                voice_session_id: None,
            },
        ];
        let frame = PersonaInboxFrame {
            persona_id,
            room_id,
            messages,
            metrics: PersonaInboxFrameMetrics {
                queue_depth_before: 2,
                queue_depth_after: 0,
                messages_drained: 2,
                oldest_timestamp: 10_000,
                newest_timestamp: 10_040,
                frame_span_ms: 40,
                drain_duration_us: 8,
            },
        };
        PersonaTurnFrame::from_inbox_frame(frame)
            .replay_record()
            .expect("fixture frame is non-empty")
    }

    /// Per-thread fixture-root override (#7): points THIS thread's recorder at
    /// the test's tempdir (plus an optional disable flag) without touching the
    /// process environment. The old HOME-swap under a private lock only
    /// serialized recorder tests against each other — every other parallel
    /// test reading HOME, or writing a fixture mid-swap, raced it. Same
    /// `install(path, disabled)` shape as the env version it replaces.
    struct EnvRestore;

    impl EnvRestore {
        fn install(home: &std::path::Path, disabled: Option<&str>) -> Self {
            TEST_FIXTURE_ROOT.with(|r| *r.borrow_mut() = Some(home.to_path_buf()));
            // None mirrors the old remove_var: an inherited process-level
            // disable must not leak into a test that expects writes.
            TEST_DISABLED
                .with(|d| *d.borrow_mut() = Some(matches!(disabled, Some("1" | "true" | "TRUE"))));
            Self
        }
    }

    impl Drop for EnvRestore {
        fn drop(&mut self) {
            TEST_FIXTURE_ROOT.with(|r| *r.borrow_mut() = None);
            TEST_DISABLED.with(|d| *d.borrow_mut() = None);
        }
    }

    /// What this catches: filename includes persona name (whitespace
    /// collapsed), message-id prefix, and ends with `-rust.json`. A
    /// test runner downstream filters captures by suffix; breaking
    /// the suffix breaks the filter.
    #[test]
    fn filename_shape_is_stable() {
        let f = filename_for("Vision AI", Uuid::nil());
        assert!(f.starts_with("Vision_AI-00000000-"));
        assert!(f.ends_with("-rust.json"));
    }

    /// What this catches: `RequestEcho::from` preserves the media's
    /// base64 payload (no stripping). Replay tests need the exact
    /// bytes; quietly trimming would neuter the test bench.
    #[test]
    fn request_echo_preserves_media_base64() {
        let mut input = fake_input();
        input.message_media = vec![MediaItemLite {
            item_type: "image".to_string(),
            base64: Some("PAYLOAD".to_string()),
            mime_type: Some("image/png".to_string()),
            description: None,
        }];
        let echo = RequestEcho::from(&input);
        assert_eq!(echo.message_media.len(), 1);
        assert_eq!(echo.message_media[0].base64, Some("PAYLOAD"));
        assert_eq!(echo.message_media[0].item_type, "image");
    }

    /// What this catches: capabilities flow as kebab-case strings,
    /// matching the wire format the IPC handler also uses. Drift here
    /// would mean Rust-recorded captures don't replay through the same
    /// `respond_input_from_value` path the live IPC uses.
    #[test]
    fn capabilities_serialize_as_kebab_case_strings() {
        use crate::model_registry::Capability;
        let mut input = fake_input();
        input.capabilities.insert(Capability::Vision);
        input.capabilities.insert(Capability::AudioInput);
        let echo = RequestEcho::from(&input);
        assert!(echo.capabilities.iter().any(|s| s == "vision"));
        assert!(echo.capabilities.iter().any(|s| s == "audio-input"));
    }

    /// What this catches: full payload serializes through serde
    /// without panicking. Schema changes that introduce a non-
    /// serializable type would fail here before reaching disk.
    #[test]
    fn turn_payload_serializes() {
        let input = fake_input();
        let response = fake_response();
        let trace = CognitionTrace::new();
        let payload = json!({
            "schemaVersion": 1,
            "capturedAtMs": 0u64,
            "personaId": input.persona.persona_id,
            "personaName": input.persona.display_name,
            "messageId": input.message_id,
            "roomId": input.turn_context.room_id,
            "model": input.model,
            "rustRequest": RequestEcho::from(&input),
            "rustResponse": &response,
            "cognitionTrace": &trace,
        });
        let s = serde_json::to_string(&payload).expect("payload serializes");
        assert!(s.contains("\"schemaVersion\":1"));
        assert!(s.contains("\"rustRequest\""));
        assert!(s.contains("\"rustResponse\""));
        assert!(s.contains("\"cognitionTrace\""));
    }

    /// What this catches: `record_turn` performs the actual Rust-owned
    /// side effect TS used to perform — fixture dir creation, one JSON
    /// write, request echo, response, and trace in one artifact.
    #[test]
    fn record_turn_writes_fixture_json_under_home() {
        let tmp = tempdir().expect("temp home");
        let _restore = EnvRestore::install(tmp.path(), None);
        let input = fake_input();
        let response = fake_response();
        let trace = CognitionTrace::new();

        record_turn(&input, &response, &trace);

        let dir = tmp.path().join(".continuum/fixtures/persona-respond");
        let entries: Vec<_> = std::fs::read_dir(&dir)
            .expect("fixture dir exists")
            .map(|e| e.expect("fixture entry").path())
            .collect();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].to_string_lossy().ends_with("-rust.json"));

        let body = std::fs::read_to_string(&entries[0]).expect("fixture json readable");
        let json: serde_json::Value = serde_json::from_str(&body).expect("fixture json parses");
        assert_eq!(json["schemaVersion"], 1);
        assert_eq!(json["personaName"], "Test Persona");
        assert_eq!(json["rustRequest"]["messageText"], "hello");
        assert_eq!(json["rustResponse"]["text"], "hi");
        assert!(json.get("cognitionTrace").is_some());
    }

    /// What this catches: perf/ephemeral hosts can opt out of fixture disk
    /// writes, and the Rust recorder honors that without asking TS to help.
    #[test]
    fn record_turn_respects_disable_env() {
        let tmp = tempdir().expect("temp home");
        let _restore = EnvRestore::install(tmp.path(), Some("true"));

        record_turn(&fake_input(), &fake_response(), &CognitionTrace::new());

        let dir = tmp.path().join(".continuum/fixtures/persona-respond");
        assert!(!dir.exists());
    }

    /// What this catches: failure-path captures land on disk without
    /// widening the chat-facing `PersonaResponse` enum. Before this,
    /// `record_turn` only ran on the Ok-path of `respond()`, so failure
    /// turns left no fixture and the most diagnostic captures were lost.
    #[test]
    fn record_failed_turn_writes_error_with_partial_trace() {
        use crate::persona::trace::SEAM_ANALYZE;
        let tmp = tempdir().expect("temp home");
        let _restore = EnvRestore::install(tmp.path(), None);
        let input = fake_input();
        let mut trace = CognitionTrace::new();
        trace.record(SEAM_ANALYZE, 1000, 50, json!({"from_cache": false}));

        record_failed_turn(&input, "render adapter timed out at 30s", 30_125, &trace);

        let dir = tmp.path().join(".continuum/fixtures/persona-respond");
        let entries: Vec<_> = std::fs::read_dir(&dir)
            .expect("failure fixture dir exists")
            .map(|e| e.expect("entry").path())
            .collect();
        assert_eq!(entries.len(), 1);
        let body = std::fs::read_to_string(&entries[0]).expect("failure fixture readable");
        let parsed: serde_json::Value =
            serde_json::from_str(&body).expect("failure fixture parses");
        assert_eq!(parsed["rustResponse"], serde_json::Value::Null);
        assert_eq!(
            parsed["rustError"]["lastCompletedSeam"],
            json!(SEAM_ANALYZE)
        );
        assert_eq!(
            parsed["rustError"]["errorMsg"],
            json!("render adapter timed out at 30s")
        );
        assert_eq!(parsed["rustError"]["partialTraceSeams"], json!(1));
        assert_eq!(parsed["rustError"]["totalMs"], json!(30_125));
        // The partial trace must survive too — replay tooling needs to
        // see WHERE in the pipeline the failure landed, not just that
        // it failed. `cognitionTrace.seams` should include the analyze
        // seam that DID complete before the error.
        assert_eq!(
            parsed["cognitionTrace"]["seams"][0]["name"],
            json!(SEAM_ANALYZE)
        );
    }

    /// What this catches: the frame replay fixture is Rust-owned and captures
    /// the pre-inference boundary: raw inbox frame, consolidated transcript,
    /// and deterministic RAG seed in one parseable artifact.
    #[test]
    fn record_turn_frame_replay_writes_fixture_json_under_home() {
        let tmp = tempdir().expect("temp home");
        let _restore = EnvRestore::install(tmp.path(), None);
        let record = fake_turn_frame_replay_record();

        record_turn_frame_replay(&record);

        let dir = tmp.path().join(TURN_FRAME_FIXTURE_DIR);
        let entries: Vec<_> = std::fs::read_dir(&dir)
            .expect("turn-frame fixture dir exists")
            .map(|e| e.expect("fixture entry").path())
            .collect();
        assert_eq!(entries.len(), 1);
        assert!(entries[0].to_string_lossy().contains("/frame-"));
        assert!(entries[0].to_string_lossy().ends_with("-rust.json"));

        let body = std::fs::read_to_string(&entries[0]).expect("fixture json readable");
        let json: serde_json::Value = serde_json::from_str(&body).expect("fixture json parses");
        assert_eq!(
            json["schemaVersion"],
            crate::persona::PERSONA_TURN_FRAME_REPLAY_SCHEMA_VERSION
        );
        assert_eq!(json["inboxFrame"]["metrics"]["messagesDrained"], 2);
        assert_eq!(
            json["consolidatedInbox"]["transcript"],
            "Operator: what changed?\nMira: the frame records replay state"
        );
        assert_eq!(
            json["ragSeed"]["queryText"],
            "Operator: what changed?\nMira: the frame records replay state"
        );
    }

    /// What this catches: the same recorder opt-out used by response fixtures
    /// applies to turn-frame fixtures, so perf harnesses can disable disk I/O
    /// without branching in the caller.
    #[test]
    fn record_turn_frame_replay_respects_disable_env() {
        let tmp = tempdir().expect("temp home");
        let _restore = EnvRestore::install(tmp.path(), Some("true"));

        record_turn_frame_replay(&fake_turn_frame_replay_record());

        let dir = tmp.path().join(TURN_FRAME_FIXTURE_DIR);
        assert!(!dir.exists());
    }

    /// What this catches: replay tooling can load the exact fixture emitted by
    /// the Rust recorder and gets the typed replay record back only after the
    /// duplicate derived fields validate against the raw inbox frame.
    #[test]
    fn load_turn_frame_replay_fixture_accepts_recorder_output() {
        let tmp = tempdir().expect("temp home");
        let _restore = EnvRestore::install(tmp.path(), None);
        let record = fake_turn_frame_replay_record();
        let expected_query = record.rag_seed.query_text.clone();

        record_turn_frame_replay(&record);

        let dir = tmp.path().join(TURN_FRAME_FIXTURE_DIR);
        let entry = std::fs::read_dir(&dir)
            .expect("turn-frame fixture dir exists")
            .next()
            .expect("fixture exists")
            .expect("fixture entry")
            .path();
        let loaded = load_turn_frame_replay_fixture(&entry).expect("fixture loads");

        assert_eq!(
            loaded.schema_version,
            PERSONA_TURN_FRAME_REPLAY_SCHEMA_VERSION
        );
        assert_eq!(loaded.rag_seed.query_text, expected_query);
        assert_eq!(loaded.consolidated_inbox.source_count, 2);
    }

    /// What this catches: schemaVersion is a real compatibility gate. Replay
    /// tools must reject unknown fixture schemas instead of trying to guess.
    #[test]
    fn load_turn_frame_replay_fixture_rejects_unknown_schema() {
        let tmp = tempdir().expect("temp home");
        let record = fake_turn_frame_replay_record();
        let mut json = serde_json::to_value(&record).expect("record to json");
        json["schemaVersion"] = serde_json::json!(999);
        let path = tmp.path().join("bad-schema.json");
        std::fs::write(&path, serde_json::to_vec_pretty(&json).expect("json bytes"))
            .expect("write fixture");

        let error = load_turn_frame_replay_fixture(&path).expect_err("schema rejected");

        match error {
            TurnFrameReplayLoadError::UnsupportedSchema {
                expected, actual, ..
            } => {
                assert_eq!(expected, PERSONA_TURN_FRAME_REPLAY_SCHEMA_VERSION);
                assert_eq!(actual, 999);
            }
            other => panic!("expected UnsupportedSchema, got {other:?}"),
        }
    }

    /// What this catches: the loader does not trust duplicated derived fields.
    /// If someone edits the stored transcript without changing the raw frame,
    /// replay rejects the fixture as non-evidence.
    #[test]
    fn load_turn_frame_replay_fixture_rejects_tampered_consolidation() {
        let tmp = tempdir().expect("temp home");
        let record = fake_turn_frame_replay_record();
        let mut json = serde_json::to_value(&record).expect("record to json");
        json["consolidatedInbox"]["transcript"] = serde_json::json!("tampered");
        let path = tmp.path().join("tampered.json");
        std::fs::write(&path, serde_json::to_vec_pretty(&json).expect("json bytes"))
            .expect("write fixture");

        let error = load_turn_frame_replay_fixture(&path).expect_err("tamper rejected");

        match error {
            TurnFrameReplayLoadError::InvalidRecord { reason, .. } => {
                assert!(reason.contains("consolidatedInbox"));
            }
            other => panic!("expected InvalidRecord, got {other:?}"),
        }
    }
}
