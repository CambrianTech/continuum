//! Glass box on GPU-vs-CPU lane PLACEMENT decisions — the structured log of every
//! "which device does this lane run on, and why" verdict.
//!
//! Companion to the coordinator's [`LaneCaptureEvent`](super::coordinator::LaneCaptureEvent)
//! family (which records lane *lifecycle* — open/close/evict — with `target_silicon`)
//! and to [`JsonlPromptCaptureSink`](crate::cognition::prompt_capture): same shape,
//! different stream. This one answers "was the accelerator filled first, or did a
//! lane silently fall to CPU?" — token-for-token auditable, never inferred from a
//! `mean_tokens_per_second` that quietly collapsed to 4 tok/s.
//!
//! Per [[optimization-is-always-first]] (GPU-FIRST is the policy) and
//! [[observability-is-half-the-architecture]]: a placement is a load-bearing
//! decision, so it gets a capture event, NOT a bare `tracing::info!`. The Noop sink
//! is the production default at zero cost; the Jsonl sink is the mechanic-shop
//! observer. When the `ResourceGovernor` (#56) takes ownership of placement, it
//! emits into this same stream — the eval ephemeral lane is just the first writer.
//!
//! Best-effort, same contract as the prompt sink: a write failure is logged and
//! dropped, NEVER fails the lane spawn — observability is not load-bearing.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::Path;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

/// Bumped when the on-disk record shape changes (replay readers gate on it).
const SCHEMA_VERSION: u32 = 1;

/// One placement verdict: which device a lane was placed on, the GPU-first
/// reasoning, and the live evidence the decision weighed. Self-contained
/// primitives (no cognition/eval types) so the inference layer owns it cleanly.
#[derive(Debug, Clone, Serialize)]
pub struct PlacementDecisionRecord {
    pub schema_version: u32,
    pub captured_at_ms: u64,
    /// What asked for the lane, e.g. `"eval-lane gene:coder-reflex-v1"` — the
    /// equivalent of a span name, so a decisions log is greppable by purpose.
    pub context: String,
    /// The base model the lane serves.
    pub model: String,
    /// Served context window for this lane (KV-cache sizing input).
    pub served_ctx: u32,
    /// `"gpu"` or `"cpu"` — the chosen device.
    pub device: String,
    /// The GPU-first decision, in words (fits / GPU-full-spill / no-backend / …).
    pub reason: String,
    /// Live free VRAM (bytes) the decision saw, net of resident lanes. `None` when
    /// no GPU monitor on the node.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub free_vram_bytes: Option<u64>,
    /// Estimated weight+scratch footprint (bytes) weighed against free VRAM.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub footprint_bytes: Option<u64>,
    /// The edge-margin (bytes) GPU-first kept free so a brim-full placement can't
    /// trip decode-time OOM — recorded so the threshold the decision used is in the
    /// log, not just in the code.
    pub margin_bytes: u64,
}

/// Records ONE placement verdict. A `Noop` sink (the default) means no capture —
/// zero cost on any hot placement path the governor may later run.
pub trait PlacementCaptureSink: Send + Sync {
    fn record(&self, record: &PlacementDecisionRecord);
}

/// Zero-cost default. Drops every decision.
pub struct NoopPlacementCaptureSink;

impl PlacementCaptureSink for NoopPlacementCaptureSink {
    fn record(&self, _record: &PlacementDecisionRecord) {}
}

/// Appends one JSON line per placement decision to `<dir>/decisions.jsonl`.
/// Mirrors [`JsonlPromptCaptureSink`](crate::cognition::prompt_capture::JsonlPromptCaptureSink).
pub struct JsonlPlacementCaptureSink {
    file: Mutex<File>,
}

impl JsonlPlacementCaptureSink {
    /// Open (create + append) the shared decisions log under `dir`. Errors only on
    /// filesystem failure; callers degrade to no capture (never fail lane spawn).
    pub fn open(dir: &Path) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join("decisions.jsonl");
        let file = OpenOptions::new().create(true).append(true).open(path)?;
        Ok(Self {
            file: Mutex::new(file),
        })
    }

    /// The default glass-box location: `~/.continuum/fixtures/placement-decisions/`,
    /// alongside the prompt-captures glass box. Returns a Noop sink (boxed) if the
    /// home dir or file can't be opened — capture degrades, spawn proceeds.
    pub fn glass_box() -> Box<dyn PlacementCaptureSink> {
        let dir = dirs::home_dir().map(|h| h.join(".continuum/fixtures/placement-decisions"));
        match dir.and_then(|d| Self::open(&d).ok()) {
            Some(sink) => Box::new(sink),
            None => Box::new(NoopPlacementCaptureSink),
        }
    }

    pub fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

impl PlacementCaptureSink for JsonlPlacementCaptureSink {
    fn record(&self, record: &PlacementDecisionRecord) {
        let line = match serde_json::to_string(record) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!(target: "inference::placement", error = %e, "placement capture serialize failed; dropped");
                return;
            }
        };
        if let Ok(mut f) = self.file.lock() {
            if let Err(e) = writeln!(f, "{line}") {
                tracing::warn!(target: "inference::placement", error = %e, "placement capture write failed; dropped");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the Jsonl sink actually appends a parseable line carrying
    // the device + reason + evidence (the audit trail Joel asked for — "a log of GPU
    // and cpu lane decisions, like how we did rag"). If the record stops serializing
    // or the schema drifts, a replay reader gating on schema_version breaks here, not
    // silently in production. Noop must stay a true no-op (no file, no panic).
    #[test]
    fn jsonl_sink_appends_one_decision_line() {
        let dir =
            std::env::temp_dir().join(format!("placement-capture-test-{}", std::process::id()));
        let sink = JsonlPlacementCaptureSink::open(&dir).expect("open sink");
        let rec = PlacementDecisionRecord {
            schema_version: SCHEMA_VERSION,
            captured_at_ms: 1_000,
            context: "eval-lane gene:coder-reflex-v1".to_string(),
            model: "qwen3.5-4b-code-forged".to_string(),
            served_ctx: 16_384,
            device: "gpu".to_string(),
            reason: "GPU-first: lane fits in free VRAM alongside resident lanes".to_string(),
            free_vram_bytes: Some(50 * 1024 * 1024 * 1024),
            footprint_bytes: Some(3 * 1024 * 1024 * 1024),
            margin_bytes: 256 * 1024 * 1024,
        };
        sink.record(&rec);

        let body = std::fs::read_to_string(dir.join("decisions.jsonl")).expect("read log");
        let parsed: serde_json::Value =
            serde_json::from_str(body.lines().next().expect("one line")).expect("valid json");
        assert_eq!(parsed["device"], "gpu");
        assert_eq!(parsed["schema_version"], SCHEMA_VERSION);
        assert!(parsed["reason"].as_str().unwrap().contains("GPU-first"));

        // Noop is a true no-op.
        NoopPlacementCaptureSink.record(&rec);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
