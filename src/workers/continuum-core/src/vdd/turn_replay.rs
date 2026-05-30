//! Live persona-turn replay fixture — bundles the input + output
//! of a single prod persona turn into one machine-readable JSON
//! record per Joel's "live record/replay proof" ask.
//!
//! Why this exists separately from `persona::recorder` and the
//! VDD `StandardVddRecord`:
//!
//! - `persona::recorder` writes per-turn cognition fixtures under
//!   `~/.continuum/fixtures/persona-respond/` — input + output +
//!   cognition trace. Keyed by persona + message id + ts. Optimized
//!   for replay determinism (rerun the same cognition turn against
//!   a new build).
//!
//! - `vdd::artifacts::ArtifactWriter` writes harness scenario
//!   records under `~/.continuum/vdd/<git_sha>/<scenario>/record.jsonl`
//!   — pass/fail summary, hardware/backend, latency metrics. Keyed
//!   by git_sha + scenario for cross-PR comparison. Optimized for
//!   "did this commit regress vs the last one."
//!
//! - THIS module writes "live turn replay" fixtures under
//!   `~/.continuum/vdd/<git_sha>/turn-replays/<turn_id>.json` —
//!   bundles the substrate-side view of one persona turn (the
//!   `PersonaTurnFrameReplayRecord` v2 input, the
//!   `InferenceComplete` output, the `FirstTokenEmitted` event,
//!   plus capture metadata). Keyed by git_sha + turn_id. Purpose:
//!   PROOF that on this commit, on this hardware, a real persona
//!   turn end-to-end produced this exact output for this exact
//!   input. Not aggregated — the unit IS the proof.
//!
//! The hook into `persona/turn-execute` (Lane D #1409) that
//! actually writes these fixtures lands in a follow-up PR — this
//! PR ships the data substrate (schema + writer + reader + tests)
//! so the hook PR is small and reviewable.

use crate::inference::llm_module::{FirstTokenEmitted, InferenceComplete};
use crate::persona::PersonaTurnFrameReplayRecord;
use crate::vdd::record::VddError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Schema version for the live turn-replay fixture. Bump when the
/// shape changes; `#[serde(default)]` on optional fields keeps old
/// fixtures readable across versions (same convention as
/// PersonaTurnFrameReplayRecord v1→v2 migration in #1412).
pub const LIVE_TURN_REPLAY_FIXTURE_SCHEMA_VERSION: u32 = 1;

/// One captured live persona turn — input + output + capture
/// metadata. Bundles `PersonaTurnFrameReplayRecord` (the input
/// the substrate saw) with `InferenceComplete` (the output the
/// inference engine returned) so a replay can verify both halves
/// without re-running inference.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveTurnReplayFixture {
    pub schema_version: u32,
    /// Wall-clock when the turn finished + we captured. Lets a
    /// replay reader correlate against system logs / metrics
    /// dashboards on the same machine.
    pub captured_at_ms: u64,
    /// Git SHA the substrate was built from when the turn ran.
    /// VDD scenario bucketing uses this to compare "same turn on
    /// commit A vs commit B."
    pub git_sha: String,
    /// Optional scenario label set by the caller (e.g.
    /// "chat-roundtrip-live", "vision-smoke"). When absent the
    /// reader defaults to "ad-hoc" — fine for one-off captures,
    /// noisy for harness-driven scenarios.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scenario: Option<String>,
    /// The substrate's view of the turn input — drained inbox +
    /// consolidated chunk + rag seed + response prompt (v2 schema).
    pub persona_turn_frame: PersonaTurnFrameReplayRecord,
    /// What the inference engine returned. Pair with
    /// first_token_emitted for the full output observability set.
    pub inference_complete: InferenceComplete,
    /// TTFT event that paired with the completion. Same event the
    /// substrate publishes on the bus; captured here so the fixture
    /// is self-contained for replay (no bus subscription needed).
    pub first_token_emitted: FirstTokenEmitted,
}

impl LiveTurnReplayFixture {
    /// Construct a fixture from the substrate's typed inputs +
    /// outputs. Caller is responsible for capturing
    /// `captured_at_ms` from a clock (UNIX ms preferred for
    /// cross-platform consistency) and `git_sha` from the build
    /// info (continuum-core exposes a const GIT_SHA at build time).
    pub fn new(
        captured_at_ms: u64,
        git_sha: impl Into<String>,
        scenario: Option<String>,
        persona_turn_frame: PersonaTurnFrameReplayRecord,
        inference_complete: InferenceComplete,
        first_token_emitted: FirstTokenEmitted,
    ) -> Self {
        Self {
            schema_version: LIVE_TURN_REPLAY_FIXTURE_SCHEMA_VERSION,
            captured_at_ms,
            git_sha: git_sha.into(),
            scenario,
            persona_turn_frame,
            inference_complete,
            first_token_emitted,
        }
    }
}

/// Writer for live turn-replay fixtures. Path layout:
///   `<root>/<git_sha>/turn-replays/<turn_id>.json`
///
/// Each turn gets its own file (not a single jsonl) because the
/// fixture is read individually — replay tools fetch one turn by
/// id, not the whole stream. Single-file-per-turn also means
/// concurrent writes from parallel persona turns don't contend
/// on a shared append-only file.
#[derive(Debug, Clone)]
pub struct LiveTurnReplayWriter {
    root: PathBuf,
}

impl LiveTurnReplayWriter {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Production default — writes under `~/.continuum/vdd`.
    /// Matches `ArtifactWriter::continuum_default()` so both
    /// writers share the same artifact root.
    pub fn continuum_default() -> Self {
        let home =
            dirs::home_dir().expect("home directory must exist for VDD turn-replay artifacts");
        Self::new(home.join(".continuum").join("vdd"))
    }

    /// Write a fixture to its on-disk path. `turn_id` is the
    /// stable identifier the caller chooses — typically the
    /// inference `request_id` so the fixture file name correlates
    /// 1:1 with the inference event.
    ///
    /// Returns the path the fixture landed at. Caller can log
    /// the path so humans + LLM-driven dashboards can find it.
    pub fn write(
        &self,
        fixture: &LiveTurnReplayFixture,
        turn_id: &str,
    ) -> Result<PathBuf, VddError> {
        let dir = self.root.join(&fixture.git_sha).join("turn-replays");
        fs::create_dir_all(&dir).map_err(|source| VddError::Io {
            path: dir.clone(),
            source,
        })?;

        // Sanitize the turn_id for filesystem safety — replace any
        // path-separator characters so a caller-provided id like
        // "request/123" can't escape the turn-replays dir.
        let safe = sanitize_for_filename(turn_id);
        let path = dir.join(format!("{safe}.json"));

        let body = serde_json::to_string_pretty(fixture)?;
        let mut file = fs::File::create(&path).map_err(|source| VddError::Io {
            path: path.clone(),
            source,
        })?;
        file.write_all(body.as_bytes())
            .map_err(|source| VddError::Io {
                path: path.clone(),
                source,
            })?;
        // Trailing newline — convention for cat / grep ergonomics.
        file.write_all(b"\n").map_err(|source| VddError::Io {
            path: path.clone(),
            source,
        })?;
        Ok(path)
    }
}

/// Read a fixture back from its on-disk path. Pair with the
/// writer for replay tooling — the same file the writer emits
/// round-trips through here.
pub fn read_fixture(path: impl AsRef<Path>) -> Result<LiveTurnReplayFixture, VddError> {
    let path = path.as_ref();
    let text = fs::read_to_string(path).map_err(|source| VddError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let fixture: LiveTurnReplayFixture = serde_json::from_str(&text)?;
    Ok(fixture)
}

fn sanitize_for_filename(s: &str) -> String {
    // Conservative — keep ASCII alphanumeric + dash + underscore;
    // map everything else (slashes, dots, spaces, control chars,
    // unicode) to '_'. Keeps the filename predictable across
    // POSIX + Windows, and prevents path traversal via id values.
    s.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    //! Schema round-trip + filename safety + writer/reader pair
    //! tests. Pinning the fixture format so the hook PR (which
    //! actually emits fixtures from persona/turn-execute) lands
    //! against a stable contract.
    use super::*;
    use crate::genome::working_set::{ArtifactId, PersonaId};
    use crate::inference::llm_module::{
        CompositionPlan, FinishReason, GenerationBudget, InferenceRequestId, SamplingParams,
    };
    use crate::persona::inbox::{PersonaInboxFrame, PersonaInboxFrameMetrics};
    use crate::persona::turn_frame::{
        ConsolidatedInboxChunk, RagAssemblySeed, PERSONA_TURN_FRAME_REPLAY_SCHEMA_VERSION,
    };
    use uuid::Uuid;

    fn sample_persona_turn_frame() -> PersonaTurnFrameReplayRecord {
        let persona_id = Uuid::from_u128(1);
        let room_id = Uuid::from_u128(2);
        PersonaTurnFrameReplayRecord {
            schema_version: PERSONA_TURN_FRAME_REPLAY_SCHEMA_VERSION,
            persona_id,
            room_id,
            inbox_frame: PersonaInboxFrame {
                persona_id,
                room_id,
                messages: vec![],
                metrics: PersonaInboxFrameMetrics {
                    queue_depth_before: 0,
                    queue_depth_after: 0,
                    messages_drained: 0,
                    oldest_timestamp: 0,
                    newest_timestamp: 0,
                    frame_span_ms: 0,
                    drain_duration_us: 0,
                },
            },
            consolidated_inbox: ConsolidatedInboxChunk {
                persona_id,
                room_id,
                trigger_message_id: Uuid::from_u128(3),
                messages: vec![],
                transcript: String::new(),
                source_count: 0,
                span_ms: 0,
            },
            rag_seed: RagAssemblySeed {
                persona_id,
                room_id,
                query_text: String::new(),
                source_message_ids: vec![],
            },
            response_prompt: None,
        }
    }

    fn sample_inference_complete() -> InferenceComplete {
        InferenceComplete {
            request_id: InferenceRequestId::new(Uuid::from_u128(100)),
            persona: PersonaId::new(Uuid::from_u128(1)),
            completion_tokens: vec![1, 2, 3],
            completion_text: Some("hello world".to_string()),
            finish_reason: FinishReason::Stop,
            elapsed_ms: 1234,
            tokens_generated: 3,
        }
    }

    fn sample_first_token() -> FirstTokenEmitted {
        FirstTokenEmitted {
            request_id: InferenceRequestId::new(Uuid::from_u128(100)),
            persona: PersonaId::new(Uuid::from_u128(1)),
            elapsed_us: 250_000,
        }
    }

    fn sample_fixture() -> LiveTurnReplayFixture {
        LiveTurnReplayFixture::new(
            1_715_625_600_000,
            "abc1234",
            Some("chat-roundtrip-live".to_string()),
            sample_persona_turn_frame(),
            sample_inference_complete(),
            sample_first_token(),
        )
    }

    /// What this catches: fixture constructor stamps the current
    /// schema version + threads all input fields through unchanged.
    #[test]
    fn new_stamps_schema_version_and_carries_inputs() {
        let f = sample_fixture();
        assert_eq!(f.schema_version, LIVE_TURN_REPLAY_FIXTURE_SCHEMA_VERSION);
        assert_eq!(f.captured_at_ms, 1_715_625_600_000);
        assert_eq!(f.git_sha, "abc1234");
        assert_eq!(f.scenario.as_deref(), Some("chat-roundtrip-live"));
        assert_eq!(f.inference_complete.tokens_generated, 3);
        assert_eq!(f.first_token_emitted.elapsed_us, 250_000);
    }

    /// What this catches: serde round-trip preserves every field.
    /// If the camelCase rename or any field's serialize hint drifts,
    /// the round-trip equality fails.
    #[test]
    fn fixture_round_trips_through_serde() {
        let original = sample_fixture();
        let json = serde_json::to_string(&original).unwrap();
        // Wire shape: camelCase fields on the outer struct.
        assert!(json.contains("\"schemaVersion\":"), "got {json}");
        assert!(json.contains("\"capturedAtMs\":"), "got {json}");
        assert!(json.contains("\"gitSha\":"), "got {json}");
        assert!(json.contains("\"personaTurnFrame\":"), "got {json}");
        assert!(json.contains("\"inferenceComplete\":"), "got {json}");
        assert!(json.contains("\"firstTokenEmitted\":"), "got {json}");

        let back: LiveTurnReplayFixture = serde_json::from_str(&json).unwrap();
        assert_eq!(back.schema_version, original.schema_version);
        assert_eq!(back.captured_at_ms, original.captured_at_ms);
        assert_eq!(back.git_sha, original.git_sha);
        assert_eq!(back.scenario, original.scenario);
        assert_eq!(
            back.inference_complete.request_id,
            original.inference_complete.request_id
        );
        assert_eq!(
            back.first_token_emitted.elapsed_us,
            original.first_token_emitted.elapsed_us
        );
    }

    /// What this catches: scenario=None omits the field from the
    /// wire shape (via skip_serializing_if). Keeps the JSON terse
    /// for ad-hoc captures that don't have a scenario.
    #[test]
    fn scenario_none_omits_field_on_wire() {
        let mut f = sample_fixture();
        f.scenario = None;
        let json = serde_json::to_string(&f).unwrap();
        assert!(
            !json.contains("\"scenario\""),
            "None scenario must be omitted (skip_serializing_if); got {json}"
        );
        // Round-trip still works.
        let back: LiveTurnReplayFixture = serde_json::from_str(&json).unwrap();
        assert!(back.scenario.is_none());
    }

    /// What this catches: writer creates the expected directory
    /// structure + the fixture file round-trips through the reader.
    #[test]
    fn writer_round_trips_through_reader() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = LiveTurnReplayWriter::new(tmp.path());
        let original = sample_fixture();

        let path = writer
            .write(&original, "request-100")
            .expect("write succeeds");

        // Path layout: <root>/<git_sha>/turn-replays/<turn_id>.json
        let expected = tmp
            .path()
            .join("abc1234")
            .join("turn-replays")
            .join("request-100.json");
        assert_eq!(path, expected);
        assert!(path.exists(), "writer must create the file");

        let back = read_fixture(&path).expect("reader round-trips");
        assert_eq!(back.schema_version, original.schema_version);
        assert_eq!(back.git_sha, original.git_sha);
        assert_eq!(
            back.inference_complete.tokens_generated,
            original.inference_complete.tokens_generated
        );
    }

    /// What this catches: turn_id values with path-separator
    /// characters are sanitized — a malicious or careless caller
    /// passing "../../etc/passwd" can't escape the turn-replays dir.
    #[test]
    fn writer_sanitizes_turn_id_to_prevent_path_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = LiveTurnReplayWriter::new(tmp.path());
        let f = sample_fixture();

        let path = writer
            .write(&f, "../../escape-attempt")
            .expect("sanitized path still writes");

        // The actual file lives inside the turn-replays subdir,
        // with dots/slashes replaced by underscores.
        assert!(
            path.starts_with(tmp.path().join("abc1234").join("turn-replays")),
            "path must remain inside the turn-replays dir; got {}",
            path.display()
        );
        let file_name = path.file_name().and_then(|n| n.to_str()).unwrap();
        assert!(
            !file_name.contains('/'),
            "sanitized filename must not contain path separators"
        );
        assert!(
            !file_name.contains(".."),
            "sanitized filename must not contain parent-dir markers; got {file_name}"
        );
    }

    /// What this catches: read_fixture surfaces typed parse errors
    /// for corrupt fixtures per Joel's never-swallow rule.
    #[test]
    fn read_fixture_returns_typed_error_for_corrupt_json() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("bogus.json");
        fs::write(&path, "{not valid json").unwrap();

        let result = read_fixture(&path);
        match result {
            Err(VddError::Json(_)) => { /* expected */ }
            Ok(_) => panic!("corrupt fixture must error"),
            Err(e) => panic!("expected Json error, got: {e}"),
        }
    }

    /// What this catches: read_fixture for a missing path returns
    /// a typed Io error (not a panic, not a silent default).
    #[test]
    fn read_fixture_returns_typed_error_for_missing_path() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("does-not-exist.json");

        let result = read_fixture(&path);
        match result {
            Err(VddError::Io { .. }) => { /* expected */ }
            Ok(_) => panic!("missing file must error"),
            Err(e) => panic!("expected Io error, got: {e}"),
        }
    }

    /// What this catches: multiple fixtures for the same git_sha
    /// share the turn-replays/ dir + don't clobber each other.
    /// Common case — one harness run produces many turns.
    #[test]
    fn writer_supports_multiple_turns_per_git_sha() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = LiveTurnReplayWriter::new(tmp.path());
        let f = sample_fixture();

        let path1 = writer.write(&f, "turn-001").unwrap();
        let path2 = writer.write(&f, "turn-002").unwrap();
        let path3 = writer.write(&f, "turn-003").unwrap();

        assert_ne!(path1, path2);
        assert_ne!(path2, path3);
        for p in [&path1, &path2, &path3] {
            assert!(p.exists(), "fixture file must exist: {}", p.display());
        }
    }
}
