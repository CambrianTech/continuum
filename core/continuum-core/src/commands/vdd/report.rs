//! `vdd/report` — read VDD telemetry records from the artifact store, apply
//! optional git_sha / scenario filters, and return matching records + a small
//! aggregate summary.
//!
//! Dep-holding: the command captures the artifact root (`~/.continuum/vdd` in
//! production; a temp dir under test) handed in by
//! [`VddModule`](crate::modules::vdd::VddModule)'s `commands()`. The read +
//! projection are pure functions over [`crate::vdd::reader`]; this is the typed
//! surface over them.
//!
//! Every claim "VDD: tokens/sec improved from X → Y" in a PR body should be a
//! query against this command, not a paste from a terminal.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::logging::TimingGuard;
use crate::vdd::reader::{latest_per_scenario, read_records, VddReadOptions, VddRecordEntry};
use crate::vdd::record::HarnessStatus;

/// Params for `vdd/report` — optional filters + the latest-only collapse.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vdd/VddReportParams.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct VddReportParams {
    /// Narrow to one commit's records.
    #[serde(default)]
    pub git_sha: Option<String>,
    /// Narrow to one scenario.
    #[serde(default)]
    pub scenario: Option<String>,
    /// Collapse duplicate `(git_sha, scenario)` rows to the latest per scenario —
    /// the "most recent result per scenario" view PR-body snippets want.
    #[serde(default)]
    pub latest_only: bool,
}

/// On-the-wire shape returned by `vdd/report`. Stable, camelCase for the
/// TS / CI-dashboard side that consumes it.
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/vdd/VddReport.ts")]
#[serde(rename_all = "camelCase")]
pub struct VddReport {
    /// Absolute path the records were read from. Surfaces "where the harness is
    /// writing" to humans + LLM consumers — the "where did this come from" answer
    /// is one field away.
    pub artifact_root: String,
    /// The filters applied. Empty fields are reported back as null so the
    /// consumer's expectation matches what was asked.
    pub filters: VddReportFilters,
    /// Headline counts. Cheap to compute, surface in a banner / PR-body snippet
    /// without iterating the full record list.
    pub summary: VddReportSummary,
    /// The matching records, sorted deterministically by (git_sha, scenario). The
    /// detail layer for any consumer that wants to drill in on a specific row.
    pub records: Vec<VddReportEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vdd/VddReportFilters.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct VddReportFilters {
    #[ts(optional)]
    pub git_sha: Option<String>,
    #[ts(optional)]
    pub scenario: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vdd/VddReportSummary.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct VddReportSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub prerequisite_missing: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/vdd/VddReportEntry.ts"
)]
#[serde(rename_all = "camelCase")]
pub struct VddReportEntry {
    pub git_sha: String,
    pub scenario: String,
    pub platform: String,
    pub hardware: String,
    pub backend: String,
    pub status: HarnessStatus,
    #[ts(optional)]
    pub first_token_ms: Option<u64>,
    #[ts(optional)]
    pub tok_per_sec: Option<f64>,
    pub responses_observed: u32,
    pub responses_expected: u32,
    #[ts(optional)]
    pub degraded_reason: Option<String>,
    pub silence_reasons: Vec<String>,
    /// Path to the on-disk `record.jsonl` for this entry. Lets the consumer fetch
    /// the FULL StandardVddRecord (not just the headline fields surfaced here) on
    /// demand without the report itself carrying every byte of every record.
    pub source: String,
}

/// Project the reader's records into the wire report, computing the summary.
/// Pure — no I/O.
pub(crate) fn build_report(
    entries: Vec<VddRecordEntry>,
    artifact_root: &Path,
    opts: &VddReadOptions,
) -> VddReport {
    let mut summary = VddReportSummary {
        total: entries.len(),
        passed: 0,
        failed: 0,
        prerequisite_missing: 0,
    };
    let mut records: Vec<VddReportEntry> = Vec::with_capacity(entries.len());
    for e in entries {
        match e.record.status {
            HarnessStatus::Pass => summary.passed += 1,
            HarnessStatus::Fail => summary.failed += 1,
            HarnessStatus::PrerequisiteMissing => summary.prerequisite_missing += 1,
        }
        records.push(VddReportEntry {
            git_sha: e.record.git_sha,
            scenario: e.record.scenario,
            platform: e.record.platform,
            hardware: e.record.hardware,
            backend: e.record.backend,
            status: e.record.status,
            first_token_ms: e.record.first_token_ms,
            tok_per_sec: e.record.tok_per_sec,
            responses_observed: e.record.responses_observed,
            responses_expected: e.record.responses_expected,
            degraded_reason: e.record.degraded_reason,
            silence_reasons: e.record.silence_reasons,
            source: e.source.to_string_lossy().into_owned(),
        });
    }
    records.sort_by(|a, b| {
        (a.git_sha.as_str(), a.scenario.as_str()).cmp(&(b.git_sha.as_str(), b.scenario.as_str()))
    });
    VddReport {
        artifact_root: artifact_root.to_string_lossy().into_owned(),
        filters: VddReportFilters {
            git_sha: opts.git_sha.clone(),
            scenario: opts.scenario.clone(),
        },
        summary,
        records,
    }
}

crate::action_command! {
    /// Read VDD telemetry records from the artifact store (`~/.continuum/vdd`),
    /// apply optional git_sha / scenario filters, and return the matching records
    /// plus a headline summary (total / passed / failed / prerequisite-missing).
    /// `latestOnly` collapses to the most recent result per scenario. A missing
    /// artifact root is a valid empty state (fresh machine), not an error;
    /// a corrupt record.jsonl fails loud with its path. Read-only.
    pub struct VddReportQuery { artifact_root: PathBuf }
    name: "vdd/report",
    access: AiSafe,
    params: VddReportParams,
    output: VddReport,
    run(this, _ctx, p) => {
        let _timer = TimingGuard::new("module", "vdd_report");
        let opts = VddReadOptions {
            git_sha: p.git_sha.clone(),
            scenario: p.scenario.clone(),
        };
        let entries = read_records(&this.artifact_root, &opts).map_err(|e| e.to_string())?;
        let report = if p.latest_only {
            let collapsed = latest_per_scenario(entries);
            build_report(collapsed.into_values().collect(), &this.artifact_root, &opts)
        } else {
            build_report(entries, &this.artifact_root, &opts)
        };
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    //! Pin the command contract end-to-end: name/access + filter passthrough +
    //! summary aggregation + wire shape. Each test seeds a temp artifact root via
    //! the real `ArtifactWriter` so writer/reader/report drift fails at unit time.
    use super::*;
    use crate::sdk_codegen::{ActionCommand, Ctx};
    use crate::vdd::artifacts::{ArtifactWriter, ReproducibilityManifest};
    use crate::vdd::record::StandardVddRecord;

    fn cmd(root: &Path) -> VddReportQuery {
        VddReportQuery {
            artifact_root: root.to_path_buf(),
        }
    }

    fn sample_record(git_sha: &str, scenario: &str, status: HarnessStatus) -> StandardVddRecord {
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
            responses_observed: if status == HarnessStatus::Pass { 4 } else { 1 },
            silence_reasons: if status == HarnessStatus::Fail {
                vec!["model_load_timeout".to_string()]
            } else {
                Vec::new()
            },
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
            log_refs: Vec::new(),
            next_bottleneck: None,
            policy_version: Some("v1".to_string()),
            cascade_step: Some(2),
            status,
        }
    }

    fn write(tmp_root: &Path, sha: &str, scen: &str, status: HarnessStatus) {
        let writer = ArtifactWriter::new(tmp_root);
        let r = sample_record(sha, scen, status);
        let m = ReproducibilityManifest::from_record(&r, &[]);
        writer.write(&r, &m).unwrap();
    }

    async fn report(root: &Path, params: VddReportParams) -> VddReport {
        cmd(root)
            .run(&Ctx::default(), params)
            .await
            .expect("report read must succeed")
    }

    // what this catches: name/access wiring — VDD telemetry reads are AiSafe (a
    // persona inspecting its own perf history, no privileged surface).
    #[test]
    fn name_and_access_wired() {
        assert_eq!(VddReportQuery::NAME, "vdd/report");
        assert!(matches!(
            VddReportQuery::ACCESS,
            crate::sdk_codegen::AccessLevel::AiSafe
        ));
    }

    // what this catches: with no artifact root + no records, the command returns an
    // empty report (not an error). Fresh dev machine == valid state.
    #[tokio::test]
    async fn report_with_missing_root_returns_empty_report() {
        let tmp = tempfile::tempdir().unwrap();
        let nonexistent = tmp.path().join("never-created");
        let r = report(&nonexistent, VddReportParams::default()).await;
        assert_eq!(r.summary.total, 0);
        assert_eq!(r.summary.passed, 0);
        assert!(r.records.is_empty());
    }

    // what this catches: end-to-end command path bundles the reader's output into
    // the wire report. Aggregates the summary across pass/fail/prerequisite_missing.
    #[tokio::test]
    async fn report_aggregates_summary_across_record_statuses() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "sha-a",
            "chat-roundtrip-live-harness",
            HarnessStatus::Pass,
        );
        write(
            tmp.path(),
            "sha-b",
            "chat-roundtrip-live-harness",
            HarnessStatus::Pass,
        );
        write(
            tmp.path(),
            "sha-c",
            "chat-roundtrip-live-harness",
            HarnessStatus::Fail,
        );
        write(
            tmp.path(),
            "sha-d",
            "chat-roundtrip-live-harness",
            HarnessStatus::PrerequisiteMissing,
        );

        let r = report(tmp.path(), VddReportParams::default()).await;
        assert_eq!(r.summary.total, 4);
        assert_eq!(r.summary.passed, 2);
        assert_eq!(r.summary.failed, 1);
        assert_eq!(r.summary.prerequisite_missing, 1);
        assert_eq!(r.records.len(), 4);
    }

    // what this catches: the git_sha filter narrows the result to one commit's
    // records + echoes the filter back so the consumer knows what query produced it.
    #[tokio::test]
    async fn report_git_sha_filter_narrows_results_and_echoes_back() {
        let tmp = tempfile::tempdir().unwrap();
        for sha in ["sha-a", "sha-b", "sha-c"] {
            write(
                tmp.path(),
                sha,
                "chat-roundtrip-live-harness",
                HarnessStatus::Pass,
            );
        }

        let r = report(
            tmp.path(),
            VddReportParams {
                git_sha: Some("sha-b".into()),
                ..Default::default()
            },
        )
        .await;
        assert_eq!(r.summary.total, 1);
        assert_eq!(r.records[0].git_sha, "sha-b");
        assert_eq!(r.filters.git_sha.as_deref(), Some("sha-b"));
        assert_eq!(r.filters.scenario, None);
    }

    // what this catches: `latest_only=true` collapses duplicate (git_sha, scenario)
    // entries to one row — the "most recent result per scenario" snippet view.
    #[tokio::test]
    async fn report_latest_only_collapses_duplicate_scenario_per_sha() {
        let tmp = tempfile::tempdir().unwrap();
        write(tmp.path(), "sha-x", "chat-roundtrip", HarnessStatus::Pass);
        write(tmp.path(), "sha-x", "chat-roundtrip", HarnessStatus::Fail);
        write(tmp.path(), "sha-x", "vision-smoke", HarnessStatus::Pass);

        let r = report(
            tmp.path(),
            VddReportParams {
                latest_only: true,
                ..Default::default()
            },
        )
        .await;
        assert_eq!(r.summary.total, 2);
        let chat = r
            .records
            .iter()
            .find(|rec| rec.scenario == "chat-roundtrip")
            .expect("chat-roundtrip row present");
        assert_eq!(chat.status, HarnessStatus::Fail);
    }

    // what this catches: wire-shape stability for VddReportEntry — surfaces the
    // headline VDD fields (tokens/sec, first_token_ms, status) AND the source path
    // so consumers can fetch the full record on demand.
    #[tokio::test]
    async fn report_entry_carries_headline_fields_and_source_path() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "sha-w",
            "chat-roundtrip-live-harness",
            HarnessStatus::Pass,
        );

        let r = report(tmp.path(), VddReportParams::default()).await;
        let entry = &r.records[0];
        assert_eq!(entry.git_sha, "sha-w");
        assert_eq!(entry.first_token_ms, Some(450));
        assert_eq!(entry.tok_per_sec, Some(28.6));
        assert_eq!(entry.status, HarnessStatus::Pass);
        assert!(
            entry.source.ends_with("record.jsonl"),
            "source path points at the on-disk record file"
        );
        assert!(
            r.artifact_root
                .contains(tmp.path().file_name().unwrap().to_str().unwrap()),
            "artifact_root surfaces the resolved root path"
        );
    }
}
