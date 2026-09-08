//! JSONL capture for live `WorkspaceCycle` ticks — the mechanic's glass box on
//! the **live** brain.
//!
//! The always-on persona recorder (`persona::recorder`) watches the LEGACY
//! `respond()` path. The live decision path is the [`WorkspaceCycle`], whose
//! [`WorkspaceCaptureSink`] defaults to `Noop` — so today the real mind runs
//! unobserved. This is the recording impl that fixes that: every tick → one
//! JSONL line carrying **every faculty bid (winners AND losers)** with its
//! content + salience, the **assembled context the decider actually saw** (the
//! "RAG"), and the **decision**. So "did recall surface the engram into what the
//! model reasoned over?" is a one-look answer, never a guess.
//!
//! Per OBSERVABILITY-AS-SUBSTRATE.md (capture is half the brain) + VDD: knowing
//! the exact inputs + the full attention competition beats any log. Best-effort:
//! a write failure is logged and dropped, NEVER fails the cognition turn —
//! observability is not load-bearing ([[substrate-is-a-good-citizen-on-the-host]]).

use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;
use uuid::Uuid;

use super::workspace::{
    Contribution, Decision, FacultyTiming, WorkspaceCaptureSink, WorkspaceTrace,
};

/// Bumped when the on-disk record shape changes (replay readers gate on it).
/// v2 added per-faculty `timings` (the speed axis / dashboard feed).
/// v3 adds full source-labelled supplemental room inputs.
const SCHEMA_VERSION: u32 = 4;

pub(crate) const FIXTURE_DIR: &str = ".continuum/fixtures/workspace-traces";

/// One faculty's bid, projected to a serializable shape. The internal
/// [`Contribution`] is intentionally NOT `Serialize` (it's a live cognition
/// type) — we own the wire format here so the capture schema can evolve
/// independently of the in-memory type.
#[derive(Debug, Serialize)]
struct BidRecord<'a> {
    /// Which faculty bid (recall / world-model / deliberation / …).
    faculty: &'a str,
    /// The faculty's self-assigned salience (0..=1) — why it won or lost.
    salience: f32,
    /// The faculty's audit reasoning.
    reasoning: &'a str,
    /// The actual content it surfaced — for recall, THIS is the engram text the
    /// decider would see; the load-bearing field for "was memory present?".
    content: &'a str,
    /// True for the deliberation faculty's verdict bid (the one carrying a Decision).
    is_decision: bool,
    /// The model's VERBATIM generation for a verdict bid — the raw response text
    /// before the tool-call/PASS parser lifted the `decision` above (#210). Present
    /// only on the deliberation verdict; omitted from the line for every context bid.
    /// This is what makes a fumbled artifact (a stray `<<!DOCTYPE`, a malformed tool
    /// envelope) attributable to the MODEL vs the HARNESS from a single capture line:
    /// compare `raw_generation` (what it emitted) against `decision` (what we parsed).
    #[serde(skip_serializing_if = "Option::is_none")]
    raw_generation: Option<&'a str>,
}

impl<'a> From<&'a Contribution> for BidRecord<'a> {
    fn from(c: &'a Contribution) -> Self {
        Self {
            faculty: c.faculty.as_str(),
            salience: c.salience,
            reasoning: &c.reasoning,
            content: &c.content,
            is_decision: c.decision.is_some(),
            raw_generation: c.raw_generation.as_deref(),
        }
    }
}

/// One faculty's wall-clock for this tick, projected to the wire — the speed
/// axis. Reading these back across a fixture's lines is how "did deferral push
/// the perception tier to ~0µs and leave only the LLM on the critical path?"
/// becomes a measured fact, not a hope.
#[derive(Debug, Serialize)]
struct TimingRecord<'a> {
    faculty: &'a str,
    elapsed_us: u128,
    /// `false` = perception tier, `true` = deliberation tier.
    deliberation: bool,
    /// Whether the faculty produced a bid (vs abstained) — a slow abstainer is
    /// still latency you need to see.
    bid: bool,
}

impl<'a> From<&'a FacultyTiming> for TimingRecord<'a> {
    fn from(t: &'a FacultyTiming) -> Self {
        Self {
            faculty: t.faculty.as_str(),
            elapsed_us: t.elapsed_us,
            deliberation: t.deliberation,
            bid: t.bid,
        }
    }
}

/// One serialized workspace tick — the full mechanic's view of one turn's mind.
#[derive(Debug, Serialize)]
struct WorkspaceTraceRecord<'a> {
    session_id: Uuid,
    cycle_id: Option<u64>,
    cause: &'static str,
    cause_root: Option<Uuid>,
    build_sha: &'static str,
    schema_version: u32,
    captured_at_ms: u64,
    persona_id: Uuid,
    room_id: Uuid,
    /// The consolidated burst the mind reasoned over this tick.
    world_state: &'a str,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    room_updates: Vec<&'a crate::persona::service_loop::IncomingMessage>,
    /// EVERY bid this tick (winners + losers, both phases) — the full competition.
    bids: Vec<BidRecord<'a>>,
    /// The assembled context that won attention and reached the decider (the RAG).
    context: Vec<BidRecord<'a>>,
    /// The participation decision that emerged, if any (kebab-tagged).
    decision: Option<&'a Decision>,
    /// Per-faculty wall-clock for this tick (the speed axis / dashboard feed).
    timings: Vec<TimingRecord<'a>>,
}

/// Appends one JSON line per workspace tick to a per-persona JSONL file
/// (`<dir>/<persona_id>.jsonl`). Mirrors `JsonlRagCaptureSink`'s shape.
pub struct JsonlWorkspaceCaptureSink {
    persona_id: Uuid,
    session_id: Uuid,
    file: Arc<Mutex<BufWriter<File>>>,
}

impl JsonlWorkspaceCaptureSink {
    /// Use the same native-aware fixture root as introspection and replay.
    pub(crate) fn open_for_persona(persona_id: Uuid) -> std::io::Result<Self> {
        let dir = crate::persona::recorder::fixture_dir(FIXTURE_DIR).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "cannot resolve capture home directory",
            )
        })?;
        Self::open(&dir, persona_id)
    }

    /// Open (create + append) the per-persona trace file under `dir`, creating
    /// `dir` if needed. Returns an error only on filesystem failure; the caller
    /// degrades to `Noop` capture on error (never fails persona spawn).
    pub fn open(dir: &Path, persona_id: Uuid) -> std::io::Result<Self> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{persona_id}.jsonl"));
        static OWNERS: LazyLock<super::prompt_capture::owner::CaptureOwners<BufWriter<File>>> =
            LazyLock::new(super::prompt_capture::owner::CaptureOwners::default);
        let file = OWNERS.acquire(&path, |path| {
            OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .map(|file| BufWriter::with_capacity(64 * 1024, file))
        })?;
        Ok(Self {
            persona_id,
            session_id: Uuid::new_v4(),
            file,
        })
    }

    pub(crate) fn with_session(mut self, session_id: Uuid) -> Self {
        self.session_id = session_id;
        self
    }

    fn now_ms() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

impl WorkspaceCaptureSink for JsonlWorkspaceCaptureSink {
    fn record(&self, trace: &WorkspaceTrace) {
        let rec = WorkspaceTraceRecord {
            session_id: self.session_id,
            cycle_id: (trace.cycle != super::workspace::CycleId::UNSTAMPED)
                .then_some(trace.cycle.0),
            cause: trace.cause.as_str(),
            cause_root: trace.cause.root(),
            build_sha: env!("CONTINUUM_BUILD_GIT_SHA"),
            schema_version: SCHEMA_VERSION,
            captured_at_ms: Self::now_ms(),
            persona_id: self.persona_id,
            room_id: trace.room_id,
            world_state: &trace.world_state,
            room_updates: trace.room_updates.iter().map(AsRef::as_ref).collect(),
            bids: trace.bids.iter().map(BidRecord::from).collect(),
            context: trace
                .context_broadcast
                .iter()
                .map(BidRecord::from)
                .collect(),
            decision: trace.decision.as_ref(),
            timings: trace.timings.iter().map(TimingRecord::from).collect(),
        };
        // One shared buffered writer serializes borrowed records directly; no
        // intermediate full Value/String or competing append lock per eval fork.
        if let Ok(mut file) = self.file.lock() {
            if let Err(error) = serde_json::to_writer(&mut *file, &rec)
                .map_err(std::io::Error::from)
                .and_then(|()| file.write_all(b"\n"))
                .and_then(|()| file.flush())
            {
                tracing::warn!(target: "cognition::capture", %error, "workspace trace write failed; capture incomplete");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cognition::workspace::{Contribution, CycleId, Decision, FacultyId, FacultyTiming};

    // what this catches: THE core VDD property — a captured tick must round-trip
    // to disk with every faculty's bid CONTENT intact (so "was the recalled engram
    // actually present for the decider?" is answerable from the fixture), plus the
    // assembled context and the decision. If this regresses, the live brain goes
    // dark again and we're back to guessing from logs.
    #[tokio::test]
    async fn captures_bids_context_and_decision_to_jsonl() {
        let dir = std::env::temp_dir().join(format!("ws-cap-{}", Uuid::new_v4()));
        let persona = Uuid::new_v4();
        let room = Uuid::new_v4();
        let sink = JsonlWorkspaceCaptureSink::open(&dir, persona).unwrap();

        let recall = Contribution {
            faculty: FacultyId::Recall,
            cycle: CycleId::UNSTAMPED,
            content: "the deploy went red after the auth migration".to_string(),
            salience: 0.8,
            reasoning: "relevant past engram".to_string(),
            decision: None,
            metrics: None,
            stable: false,
            standing_grounding: false,
            fault: None,
            raw_generation: None,
            trailing: false,
            parts: Vec::new(),
            expand_command: None,
        };
        let verdict = Contribution {
            faculty: FacultyId::Deliberation,
            cycle: CycleId::UNSTAMPED,
            content: "Let's roll back the migration.".to_string(),
            salience: 0.9,
            reasoning: "decider".to_string(),
            decision: Some(Decision::Speak {
                text: "Let's roll back the migration.".to_string(),
            }),
            metrics: None,
            stable: false,
            standing_grounding: false,
            fault: None,
            // #210: the verbatim generation carries a leading-char fumble the parser
            // tolerated — the capture must preserve it so model-vs-harness is decidable.
            raw_generation: Some("<Let's roll back the migration.".to_string()),
            trailing: false,
            parts: Vec::new(),
            expand_command: None,
        };
        let trace = WorkspaceTrace {
            cycle: crate::cognition::workspace::CycleId::UNSTAMPED,
            cause: crate::cognition::workspace::Cause::Synthetic,
            world_state: "teammate: what should we do about the red deploy?".to_string(),
            room_updates: Default::default(),
            room_id: room,
            bids: vec![recall.clone(), verdict.clone()],
            context_broadcast: vec![recall.clone()],
            broadcast: vec![recall, verdict],
            decision: Some(Decision::Speak {
                text: "Let's roll back the migration.".to_string(),
            }),
            timings: vec![
                FacultyTiming {
                    faculty: FacultyId::Recall,
                    elapsed_us: 12,
                    deliberation: false,
                    bid: true,
                },
                FacultyTiming {
                    faculty: FacultyId::Deliberation,
                    elapsed_us: 4200,
                    deliberation: true,
                    bid: true,
                },
            ],
        };
        sink.record(&trace);

        // Read the one JSONL line back and assert the load-bearing fields survived.
        let path = dir.join(format!("{persona}.jsonl"));
        let body = std::fs::read_to_string(&path).unwrap();
        let line = body.lines().next().expect("one trace line");
        let v: serde_json::Value = serde_json::from_str(line).unwrap();

        assert_eq!(v["persona_id"], persona.to_string());
        assert_eq!(v["room_id"], room.to_string());
        // The recall ENGRAM content is captured (the whole point — memory visibility).
        let bids = v["bids"].as_array().unwrap();
        assert!(
            bids.iter().any(|b| b["faculty"] == "recall"
                && b["content"].as_str().unwrap().contains("auth migration")),
            "recall bid content must be captured: {bids:?}"
        );
        // The assembled context (what the decider saw) is captured separately.
        assert_eq!(v["context"].as_array().unwrap().len(), 1);
        // The decision round-trips with its kebab tag.
        assert_eq!(v["decision"]["kind"], "speak");
        // #210: the verdict bid's VERBATIM generation round-trips (raw + parsed in ONE
        // line), so a fumbled artifact is attributable to the model vs the harness; a
        // context bid (recall) carries no raw_generation and the field is omitted there.
        assert!(
            bids.iter().any(|b| b["is_decision"] == true
                && b["raw_generation"]
                    .as_str()
                    .is_some_and(|s| s.starts_with('<'))),
            "verdict raw_generation must be captured verbatim: {bids:?}"
        );
        assert!(
            bids.iter()
                .any(|b| b["faculty"] == "recall" && b["raw_generation"].is_null()),
            "context bids must omit raw_generation: {bids:?}"
        );
        // Per-faculty timing (the speed axis) round-trips: the deliberation tier is
        // captured and flagged, so "where did the turn's latency go?" is answerable.
        let timings = v["timings"].as_array().unwrap();
        assert!(
            timings.iter().any(|t| t["faculty"] == "deliberation"
                && t["deliberation"] == true
                && t["elapsed_us"].as_u64().unwrap() == 4200),
            "deliberation timing must be captured: {timings:?}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: append semantics — two ticks = two JSONL lines, so a
    // turn-by-turn history accumulates (replay reads line-by-line).
    #[tokio::test]
    async fn appends_one_line_per_tick() {
        let dir = std::env::temp_dir().join(format!("ws-cap-{}", Uuid::new_v4()));
        let persona = Uuid::new_v4();
        let sink = JsonlWorkspaceCaptureSink::open(&dir, persona).unwrap();
        let mk = |room| WorkspaceTrace {
            cycle: crate::cognition::workspace::CycleId::UNSTAMPED,
            cause: crate::cognition::workspace::Cause::Synthetic,
            world_state: "burst".to_string(),
            room_updates: Default::default(),
            room_id: room,
            bids: vec![],
            context_broadcast: vec![],
            broadcast: vec![],
            decision: None,
            timings: vec![],
        };
        sink.record(&mk(Uuid::new_v4()));
        sink.record(&mk(Uuid::new_v4()));
        let body = std::fs::read_to_string(dir.join(format!("{persona}.jsonl"))).unwrap();
        assert_eq!(body.lines().count(), 2, "one line per tick");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
