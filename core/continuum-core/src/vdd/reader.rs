//! VDD record reader — walks `~/.continuum/vdd/<git_sha>/<scenario>/`
//! artifact directories and parses the `record.jsonl` files into
//! [`StandardVddRecord`] values.
//!
//! This is the read side of the artifact-writer (`artifacts.rs`) that the
//! `chat-roundtrip` harness writes through. The write side ships records
//! to disk; this side aggregates them back for inspection / reporting.
//!
//! Why a separate reader: the harness emits one record per run, but a
//! "VDD report" is a cross-run aggregation ("here is the latest pass on
//! Mac, the latest fail on Windows, the regressions since last release"
//! etc). The reader is the data-access primitive every reporting consumer
//! shares — the `vdd/report` IPC command is one of them; the precommit
//! ratchet + the CI dashboards are the next ones.

use crate::vdd::record::{StandardVddRecord, VddError};
use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

/// Options for filtering records when reading. Empty filters mean
/// "include everything"; non-empty filters narrow the result set.
///
/// Designed so callers can build "show me only Mac chat-roundtrip
/// records on this commit" queries without re-scanning the whole tree
/// twice. The reader applies filters at parse time, not after.
#[derive(Debug, Clone, Default)]
pub struct VddReadOptions {
    /// If set, only include records under this git_sha subdirectory.
    pub git_sha: Option<String>,
    /// If set, only include records whose `scenario` matches.
    pub scenario: Option<String>,
}

/// One entry returned by [`read_records`]: the parsed record + the file
/// it came from. The file path is included so callers (e.g. the report
/// IPC command) can surface "from artifacts at <path>" to humans and
/// LLM-driven CI dashboards alike.
#[derive(Debug, Clone)]
pub struct VddRecordEntry {
    pub record: StandardVddRecord,
    pub source: PathBuf,
}

/// Walk the artifact tree under `root` and return every record whose
/// `record.jsonl` parses cleanly + matches `opts`. Returns entries
/// sorted by (git_sha, scenario) for deterministic output.
///
/// Layout matches what `ArtifactWriter::write` produces:
///   `<root>/<git_sha>/<scenario>/record.jsonl`
///
/// Failure modes:
/// - `root` does not exist → returns empty Vec (NOT an error — a fresh
///   install has nothing to report, that's a valid state).
/// - A `record.jsonl` exists but won't parse → propagates the
///   `VddError::Json` from serde so the caller surfaces "this artifact
///   file is corrupt, here's the path" rather than silently dropping
///   it. Per Joel's never-swallow rule: bad data is loud.
pub fn read_records(
    root: impl AsRef<Path>,
    opts: &VddReadOptions,
) -> Result<Vec<VddRecordEntry>, VddError> {
    let root = root.as_ref();
    // A missing root is not an error — it just means no harness has
    // written yet. Common on fresh dev machines.
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut entries: Vec<VddRecordEntry> = Vec::new();
    for git_sha_dir in read_subdirs(root)? {
        let git_sha = file_name_string(&git_sha_dir);
        if let Some(ref want_sha) = opts.git_sha {
            if &git_sha != want_sha {
                continue;
            }
        }
        for scenario_dir in read_subdirs(&git_sha_dir)? {
            let scenario = file_name_string(&scenario_dir);
            if let Some(ref want_scen) = opts.scenario {
                if &scenario != want_scen {
                    continue;
                }
            }
            let record_path = scenario_dir.join("record.jsonl");
            if !record_path.exists() {
                // Scenario directory without a record file: skip silently.
                // The writer always writes record.jsonl, so this is either
                // a partially-cleaned-up dir or a foreign artifact — not
                // ours to interpret.
                continue;
            }
            for record in parse_record_jsonl(&record_path)? {
                entries.push(VddRecordEntry {
                    record,
                    source: record_path.clone(),
                });
            }
        }
    }
    // Deterministic sort: git_sha then scenario then status. Callers that
    // need cross-platform comparable output rely on this ordering
    // (so does the regression-detection logic in CI dashboards).
    entries.sort_by(|a, b| {
        (a.record.git_sha.as_str(), a.record.scenario.as_str())
            .cmp(&(b.record.git_sha.as_str(), b.record.scenario.as_str()))
    });
    Ok(entries)
}

/// Bucket records by `(git_sha, scenario)`. Each bucket carries the
/// latest record (by file mtime via natural disk order, since the
/// writer overwrites in place). Useful for reports that want "one
/// row per scenario on this commit" instead of every historical run.
pub fn latest_per_scenario(
    entries: Vec<VddRecordEntry>,
) -> BTreeMap<(String, String), VddRecordEntry> {
    let mut by_key: BTreeMap<(String, String), VddRecordEntry> = BTreeMap::new();
    for entry in entries {
        let key = (entry.record.git_sha.clone(), entry.record.scenario.clone());
        by_key.insert(key, entry);
    }
    by_key
}

fn read_subdirs(root: &Path) -> Result<Vec<PathBuf>, VddError> {
    let read = fs::read_dir(root).map_err(|source| VddError::Io {
        path: root.to_path_buf(),
        source,
    })?;
    let mut dirs: Vec<PathBuf> = Vec::new();
    for entry in read {
        let entry = entry.map_err(|source| VddError::Io {
            path: root.to_path_buf(),
            source,
        })?;
        let p = entry.path();
        if p.is_dir() {
            dirs.push(p);
        }
    }
    dirs.sort();
    Ok(dirs)
}

fn file_name_string(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .map(String::from)
        // Path components are valid UTF-8 by construction on our writers;
        // fall back to lossy if somehow not, so the reader doesn't crash
        // on a foreign-encoded directory name dropped into the artifact
        // root.
        .unwrap_or_else(|| path.to_string_lossy().to_string())
}

fn parse_record_jsonl(path: &Path) -> Result<Vec<StandardVddRecord>, VddError> {
    let file = fs::File::open(path).map_err(|source| VddError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let reader = BufReader::new(file);
    let mut records: Vec<StandardVddRecord> = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|source| VddError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let record: StandardVddRecord = serde_json::from_str(trimmed)?;
        records.push(record);
    }
    Ok(records)
}

#[cfg(test)]
mod tests {
    //! Pin the reader contract end-to-end against real on-disk
    //! artifacts (written by `ArtifactWriter`, the canonical writer).
    //! Using the real writer in tests catches schema-drift between
    //! writer and reader at unit-test time, not at "I shipped a VDD
    //! report and CI dashboards stopped parsing" time.
    use super::*;
    use crate::vdd::artifacts::{ArtifactWriter, ReproducibilityManifest};
    use crate::vdd::record::{HarnessStatus, StandardVddRecord};

    fn sample_record(git_sha: &str, scenario: &str) -> StandardVddRecord {
        StandardVddRecord {
            scenario: scenario.to_string(),
            platform: "darwin".to_string(),
            hardware: "m1-air-8gb".to_string(),
            backend: "metal".to_string(),
            git_sha: git_sha.to_string(),
            command: "npm start".to_string(),
            model: Some("qwen2-vl-7b-instruct".to_string()),
            gpu_layers: Some(32),
            unsupported_layers: Vec::new(),
            cold_start_ms: Some(8_000),
            first_token_ms: Some(450),
            first_response_ms: Some(1_200),
            all_responses_ms: Some(3_400),
            responses_expected: 4,
            responses_observed: 4,
            silence_reasons: Vec::new(),
            tok_per_sec: Some(28.6),
            cpu_pct_avg: Some(55.0),
            cpu_pct_peak: Some(98.0),
            rss_mb: Some(3_120),
            gpu_util_pct_avg: Some(72.0),
            gpu_memory_mb: Some(4_800),
            queue_wait_ms: Some(12),
            execution_ms: Some(820),
            coalesced_count: 1,
            deferred_count: 0,
            stale_drop_count: 0,
            error_count: 0,
            degraded_reason: None,
            log_refs: vec!["~/.continuum/sessions/.../logs/server.log".to_string()],
            next_bottleneck: None,
            policy_version: Some("v1".to_string()),
            cascade_step: Some(2),
            status: HarnessStatus::Pass,
        }
    }

    /// What this catches: missing artifact root is a normal "fresh
    /// install, no harness has run yet" state, not an error. Per
    /// the spec, the reader returns an empty Vec.
    #[test]
    fn missing_root_returns_empty_vec_not_error() {
        let tmp = tempfile::tempdir().unwrap();
        let nonexistent = tmp.path().join("never-created");

        let entries = read_records(&nonexistent, &VddReadOptions::default())
            .expect("missing root is not an error");
        assert!(entries.is_empty());
    }

    /// What this catches: an empty artifact root (exists but no
    /// git_sha subdirs) returns an empty Vec. Same "no data yet"
    /// shape as missing root, different filesystem state.
    #[test]
    fn empty_root_returns_empty_vec() {
        let tmp = tempfile::tempdir().unwrap();
        let entries =
            read_records(tmp.path(), &VddReadOptions::default()).expect("empty root reads cleanly");
        assert!(entries.is_empty());
    }

    /// What this catches: a single record round-trips through
    /// writer → disk → reader. End-to-end format pin against the
    /// real `ArtifactWriter`.
    #[test]
    fn single_record_round_trips_through_writer_reader() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = ArtifactWriter::new(tmp.path());
        let original = sample_record("abc1234", "chat-roundtrip-live-harness");
        let manifest = ReproducibilityManifest::from_record(&original, &[]);
        writer.write(&original, &manifest).expect("write succeeds");

        let entries = read_records(tmp.path(), &VddReadOptions::default()).expect("read succeeds");
        assert_eq!(entries.len(), 1);
        let entry = &entries[0];
        assert_eq!(entry.record.git_sha, "abc1234");
        assert_eq!(entry.record.scenario, "chat-roundtrip-live-harness");
        assert_eq!(entry.record.tok_per_sec, Some(28.6));
        assert_eq!(entry.record.status, HarnessStatus::Pass);
        // source path points at the actual record.jsonl on disk.
        assert!(entry.source.ends_with("record.jsonl"));
    }

    /// What this catches: multiple records under different git_shas
    /// + scenarios are all discovered + sorted deterministically.
    #[test]
    fn multiple_records_discovered_and_sorted_deterministically() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = ArtifactWriter::new(tmp.path());
        // Intentionally write in non-sorted order to verify sort.
        for (sha, scen) in [
            ("z9", "chat-roundtrip-live-harness"),
            ("a1", "vision-smoke"),
            ("a1", "chat-roundtrip-live-harness"),
            ("m5", "chat-roundtrip-live-harness"),
        ] {
            let r = sample_record(sha, scen);
            let m = ReproducibilityManifest::from_record(&r, &[]);
            writer.write(&r, &m).unwrap();
        }

        let entries = read_records(tmp.path(), &VddReadOptions::default()).expect("read succeeds");
        let pairs: Vec<(&str, &str)> = entries
            .iter()
            .map(|e| (e.record.git_sha.as_str(), e.record.scenario.as_str()))
            .collect();
        assert_eq!(
            pairs,
            vec![
                ("a1", "chat-roundtrip-live-harness"),
                ("a1", "vision-smoke"),
                ("m5", "chat-roundtrip-live-harness"),
                ("z9", "chat-roundtrip-live-harness"),
            ],
            "entries must sort by (git_sha, scenario) for deterministic reports"
        );
    }

    /// What this catches: `git_sha` filter narrows the result set
    /// to just that commit's records. Used by reports that ask
    /// "what's the VDD state on HEAD?" without rescanning history.
    #[test]
    fn git_sha_filter_narrows_results() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = ArtifactWriter::new(tmp.path());
        for sha in ["sha-a", "sha-b", "sha-c"] {
            let r = sample_record(sha, "chat-roundtrip-live-harness");
            let m = ReproducibilityManifest::from_record(&r, &[]);
            writer.write(&r, &m).unwrap();
        }

        let opts = VddReadOptions {
            git_sha: Some("sha-b".to_string()),
            scenario: None,
        };
        let entries = read_records(tmp.path(), &opts).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].record.git_sha, "sha-b");
    }

    /// What this catches: `scenario` filter works independently of
    /// git_sha. Reports that ask "show me every commit's
    /// vision-smoke status" use this.
    #[test]
    fn scenario_filter_narrows_results_across_shas() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = ArtifactWriter::new(tmp.path());
        for sha in ["sha-a", "sha-b"] {
            for scen in ["chat-roundtrip-live-harness", "vision-smoke"] {
                let r = sample_record(sha, scen);
                let m = ReproducibilityManifest::from_record(&r, &[]);
                writer.write(&r, &m).unwrap();
            }
        }

        let opts = VddReadOptions {
            git_sha: None,
            scenario: Some("vision-smoke".to_string()),
        };
        let entries = read_records(tmp.path(), &opts).unwrap();
        assert_eq!(entries.len(), 2);
        for e in &entries {
            assert_eq!(e.record.scenario, "vision-smoke");
        }
    }

    /// What this catches: `latest_per_scenario` collapses duplicate
    /// (git_sha, scenario) pairs to a single entry. Used by report
    /// queries that want one row per scenario per commit.
    #[test]
    fn latest_per_scenario_collapses_duplicates() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = ArtifactWriter::new(tmp.path());

        // First write: PASS.
        let mut r = sample_record("sha-x", "chat-roundtrip-live-harness");
        r.status = HarnessStatus::Pass;
        let m = ReproducibilityManifest::from_record(&r, &[]);
        writer.write(&r, &m).unwrap();

        // Second write to the same (git_sha, scenario): FAIL.
        // Writer overwrites in place; reader sees the latest.
        let mut r2 = sample_record("sha-x", "chat-roundtrip-live-harness");
        r2.status = HarnessStatus::Fail;
        r2.silence_reasons = vec!["model_load_timeout".to_string()];
        let m2 = ReproducibilityManifest::from_record(&r2, &[]);
        writer.write(&r2, &m2).unwrap();

        let entries = read_records(tmp.path(), &VddReadOptions::default()).unwrap();
        let latest = latest_per_scenario(entries);
        assert_eq!(latest.len(), 1);
        let entry = latest
            .get(&(
                "sha-x".to_string(),
                "chat-roundtrip-live-harness".to_string(),
            ))
            .expect("scenario present");
        assert_eq!(entry.record.status, HarnessStatus::Fail);
        assert_eq!(entry.record.silence_reasons, vec!["model_load_timeout"]);
    }

    /// What this catches: a corrupt `record.jsonl` produces a typed
    /// VddError::Json with the parse failure, NOT silent omission.
    /// Per Joel's never-swallow rule: bad data is loud.
    #[test]
    fn corrupt_record_returns_typed_json_error() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("sha-x").join("scen-x");
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("record.jsonl"), "{not valid json").unwrap();

        let result = read_records(tmp.path(), &VddReadOptions::default());
        match result {
            Err(VddError::Json(_)) => { /* expected */ }
            Ok(v) => panic!("corrupt jsonl must error, got {} entries", v.len()),
            Err(e) => panic!("expected Json error, got: {e}"),
        }
    }

    /// What this catches: scenario directory without a record.jsonl
    /// is skipped silently (NOT an error). This is the partially-
    /// cleaned-up-dir case; the writer's invariant is "directory
    /// only exists if it has record.jsonl," but external cleanup
    /// scripts can leave the directory behind.
    #[test]
    fn scenario_dir_without_record_jsonl_is_skipped() {
        let tmp = tempfile::tempdir().unwrap();
        let writer = ArtifactWriter::new(tmp.path());

        // Valid record.
        let r = sample_record("sha-real", "chat-roundtrip-live-harness");
        let m = ReproducibilityManifest::from_record(&r, &[]);
        writer.write(&r, &m).unwrap();

        // Empty scenario dir (no record.jsonl).
        let empty_dir = tmp.path().join("sha-empty").join("partial-cleanup");
        fs::create_dir_all(&empty_dir).unwrap();

        let entries = read_records(tmp.path(), &VddReadOptions::default()).unwrap();
        assert_eq!(entries.len(), 1, "only the real record is returned");
        assert_eq!(entries[0].record.git_sha, "sha-real");
    }
}
