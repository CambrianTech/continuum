//! Verbatim LLM-I/O capture for the deliberation faculty — the glass box on the
//! EXACT tokens a persona is fed and what it returned, per agent-loop iteration.
//!
//! The [`WorkspaceCaptureSink`](super::workspace_capture) records the attention
//! competition (bids/context/decision) — the SEMANTIC input. This records the
//! LITERAL request the model saw (system prompt + full message thread incl. tool
//! turns) and its raw response (text + separated reasoning + finish_reason +
//! tool_calls), so "what exactly was she prompted with, and what did she emit?"
//! is answerable token-for-token, not inferred.
//!
//! Best-effort, same contract as the workspace sink: a write failure is logged
//! and dropped, NEVER fails the cognition turn — observability is not load-bearing.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use uuid::Uuid;

use crate::ai::types::{ChatMessage, TextGenerationResponse};

/// Bumped when the on-disk record shape changes (replay readers gate on it).
/// v2: added `offered_tools` (the native tool specs' WIRE names sent with the
/// request) — the first mined exam was unauditable on "were edit_file/write_file
/// even offered?" because the tools param was the one request axis not captured.
const SCHEMA_VERSION: u32 = 2;

/// Records the verbatim request/response of one deliberation LLM call. A `None`
/// sink (the default) means no capture — zero hot-path cost.
pub trait PromptCaptureSink: Send + Sync {
    /// Capture ONE LLM call this tick. `iteration` is the agent-loop round (0 =
    /// first generation; >0 = re-prompt after a tool round). `messages` is the
    /// EXACT thread sent (burst, then any assistant tool_use + tool_results turns).
    /// `offered_tools` is the wire-dialect NAME of every native tool spec sent
    /// with this request (names only — schemas would bloat every line).
    fn record(
        &self,
        persona_id: Uuid,
        room_id: Uuid,
        iteration: usize,
        system: &str,
        messages: &[ChatMessage],
        offered_tools: &[String],
        response: &TextGenerationResponse,
    );
}

#[derive(Debug, Serialize)]
struct PromptCaptureRecord {
    schema_version: u32,
    captured_at_ms: u64,
    persona_id: String,
    room_id: String,
    /// Agent-loop round: 0 = first generation, >0 = re-prompt after tools.
    iteration: usize,
    /// The literal system prompt (identity + assembled RAG + how-to-participate).
    system: String,
    /// The exact message thread sent this call (user burst + tool turns).
    messages: serde_json::Value,
    /// Wire-dialect names of the native tool specs sent with this request —
    /// the auditable answer to "was she OFFERED that verb?". Empty = no tools.
    offered_tools: Vec<String>,
    /// The raw model response (text + reasoning + finish_reason + tool_calls).
    response: serde_json::Value,
}

/// Appends one JSON line per LLM call to `<dir>/<persona_id>.jsonl`. Mirrors
/// [`JsonlWorkspaceCaptureSink`](super::workspace_capture::JsonlWorkspaceCaptureSink).
pub struct JsonlPromptCaptureSink {
    persona_id: Uuid,
    file: Mutex<File>,
}

impl JsonlPromptCaptureSink {
    /// Open (create + append) the per-persona capture file under `dir`. Errors
    /// only on filesystem failure; the caller degrades to no capture (never fails
    /// persona spawn).
    ///
    /// ROTATES on open: `open` is called once per spawn (= once per core session),
    /// so this is the session boundary. A non-empty existing capture is rolled to
    /// `<persona_id>.prev.jsonl` and the live file starts fresh. Without this the
    /// file grew unbounded and a `tail`/read BLENDED turns from sessions hours
    /// apart — the stale-capture trap that nearly caused a misdiagnosis (a "tools
    /// don't work" read was last session's ghost). One `.prev` is kept for
    /// diffing; disk is bounded to two sessions, never an unbounded archive
    /// ([[disk-is-a-governed-resource]]).
    pub fn open(dir: &Path, persona_id: Uuid) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{persona_id}.jsonl"));
        if std::fs::metadata(&path).map(|m| m.len() > 0).unwrap_or(false) {
            // Best-effort roll — a rename failure just means we append to the
            // existing file (old behavior), never a spawn failure.
            let prev = dir.join(format!("{persona_id}.prev.jsonl"));
            let _ = std::fs::rename(&path, &prev);
        }
        let file = OpenOptions::new().create(true).append(true).open(&path)?;
        Ok(Self {
            persona_id,
            file: Mutex::new(file),
        })
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

impl PromptCaptureSink for JsonlPromptCaptureSink {
    fn record(
        &self,
        persona_id: Uuid,
        room_id: Uuid,
        iteration: usize,
        system: &str,
        messages: &[ChatMessage],
        offered_tools: &[String],
        response: &TextGenerationResponse,
    ) {
        let rec = PromptCaptureRecord {
            schema_version: SCHEMA_VERSION,
            captured_at_ms: Self::now_ms(),
            persona_id: persona_id.to_string(),
            room_id: room_id.to_string(),
            iteration,
            system: system.to_string(),
            messages: serde_json::to_value(messages).unwrap_or(serde_json::Value::Null),
            offered_tools: offered_tools.to_vec(),
            response: serde_json::to_value(response).unwrap_or(serde_json::Value::Null),
        };
        let line = match serde_json::to_string(&rec) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(target: "cognition::capture", error = %e, "prompt capture serialize failed; dropped");
                return;
            }
        };
        if let Ok(mut f) = self.file.lock() {
            if let Err(e) = writeln!(f, "{line}") {
                tracing::warn!(target: "cognition::capture", error = %e, "prompt capture write failed; dropped");
            }
        }
        let _ = self.persona_id; // retained for symmetry / future per-sink routing
    }
}
