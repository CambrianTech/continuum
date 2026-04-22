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
//! The `-rust` suffix distinguishes Rust-emitted captures from the
//! TS-emitted captures (which carry additional outer context — the
//! original chat message, the full RAG conversationHistory, etc.).
//! Both can coexist in the same dir, joined by `messageId`. As Phase
//! B/C land, RAG construction migrates Rust-side and the TS capture
//! disappears; the Rust capture becomes the single artifact.
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
use crate::runtime;
use serde::Serialize;
use serde_json::json;
use std::path::{Path, PathBuf};
use uuid::Uuid;

/// Cap on captured fixtures per dir. Matches the TS writer's cap so
/// neither side independently blows the dir up. If you need a longer
/// retention window for incident analysis, copy fixtures out before
/// the cap rotates them.
const FIXTURE_CAP_PER_DIR: usize = 200;

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
            room_id: input.room_id,
            message_id: input.message_id,
            message_text: &input.message_text,
            system_prompt: &input.system_prompt,
            model: &input.model,
            is_voice: input.is_voice,
            capabilities,
            recent_history: input
                .recent_history
                .iter()
                .map(|m| RecentEcho {
                    id: m.id,
                    sender_name: &m.sender_name,
                    text: &m.text,
                })
                .collect(),
            message_media: input
                .message_media
                .iter()
                .map(media_echo)
                .collect(),
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

/// Persist a completed turn. Best-effort: failures log + return
/// `Ok(())` so a recording problem never breaks cognition.
pub fn record_turn(
    input: &RespondInput,
    response: &PersonaResponse,
    trace: &CognitionTrace,
) {
    if disabled() {
        return;
    }
    let dir = match fixture_dir() {
        Some(d) => d,
        None => return, // HOME unset; treat as opted-out, no warning spam
    };
    if let Err(e) = std::fs::create_dir_all(&dir) {
        runtime::logger("recorder").warn(&format!(
            "couldn't create fixture dir {}: {e} — recording skipped",
            dir.display()
        ));
        return;
    }
    let fname = filename_for(&input.persona.display_name, input.message_id);
    let path = dir.join(&fname);
    let payload = json!({
        "schemaVersion": 1,
        "capturedAtMs": crate::persona::trace::now_ms(),
        "personaId": input.persona.persona_id,
        "personaName": input.persona.display_name,
        "messageId": input.message_id,
        "roomId": input.room_id,
        "model": input.model,
        "rustRequest": RequestEcho::from(input),
        "rustResponse": response,
        "cognitionTrace": trace,
    });
    let serialized = match serde_json::to_vec_pretty(&payload) {
        Ok(b) => b,
        Err(e) => {
            runtime::logger("recorder")
                .warn(&format!("turn capture serialize failed: {e}"));
            return;
        }
    };
    // Atomic write: tmp file + rename, so a crash mid-write leaves a
    // missing file rather than a half-written one that breaks parsers.
    let tmp_path = path.with_extension("json.tmp");
    if let Err(e) = std::fs::write(&tmp_path, &serialized) {
        runtime::logger("recorder").warn(&format!(
            "turn capture write failed: {e} (target: {})",
            path.display()
        ));
        return;
    }
    if let Err(e) = std::fs::rename(&tmp_path, &path) {
        runtime::logger("recorder").warn(&format!(
            "turn capture rename failed: {e} (target: {})",
            path.display()
        ));
        let _ = std::fs::remove_file(&tmp_path); // best-effort cleanup
        return;
    }
    trim_fifo(&dir);
}

fn disabled() -> bool {
    std::env::var(DISABLE_ENV)
        .map(|v| matches!(v.as_str(), "1" | "true" | "TRUE"))
        .unwrap_or(false)
}

fn fixture_dir() -> Option<PathBuf> {
    std::env::var("HOME")
        .ok()
        .map(|h| PathBuf::from(h).join(".continuum/fixtures/persona-respond"))
}

/// Filename: `<persona>-<msgid_prefix>-<ts>-rust.json`. The `-rust`
/// suffix distinguishes Rust-emitted captures from any TS-emitted
/// twin in the same dir. Persona name spaces collapsed to underscores
/// for filesystem safety.
fn filename_for(persona_name: &str, message_id: Uuid) -> String {
    let safe_name = persona_name.replace(char::is_whitespace, "_");
    let id_prefix: String = message_id
        .to_string()
        .chars()
        .take(8)
        .collect();
    let ts = chrono_like_ts(crate::persona::trace::now_ms());
    format!("{safe_name}-{id_prefix}-{ts}-rust.json")
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
    format!(
        "{year:04}-{month:02}-{day:02}T{h:02}-{m:02}-{s:02}-{sub_ms:03}Z"
    )
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
    use crate::persona::response::PersonaResponse;
    use std::collections::HashSet;

    fn fake_input() -> RespondInput {
        RespondInput {
            persona: PersonaSlot {
                persona_id: Uuid::nil(),
                specialty: "general".to_string(),
                display_name: "Test Persona".to_string(),
            },
            room_id: Uuid::nil(),
            message_id: Uuid::nil(),
            message_text: "hello".to_string(),
            recent_history: vec![],
            known_specialties: vec!["general".to_string()],
            system_prompt: "you are helpful".to_string(),
            model: "test-model".to_string(),
            is_voice: false,
            message_media: vec![],
            capabilities: HashSet::new(),
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
        let response = PersonaResponse::Spoke {
            persona_id: Uuid::nil(),
            text: "hi".to_string(),
            model_used: "test".to_string(),
            inference_ms: 1,
            total_ms: 2,
            think_blocks_emitted: 0,
        };
        let trace = CognitionTrace::new();
        let payload = json!({
            "schemaVersion": 1,
            "capturedAtMs": 0u64,
            "personaId": input.persona.persona_id,
            "personaName": input.persona.display_name,
            "messageId": input.message_id,
            "roomId": input.room_id,
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
}
