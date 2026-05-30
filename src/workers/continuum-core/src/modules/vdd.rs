//! `vdd/report` IPC module — Lane C PR-3 of the doc's
//! [Lane C VDD telemetry substrate] sequence.
//!
//! Consumes the pure read-side primitive from
//! `crate::vdd::reader` and emits a structured JSON report so
//! callers (CI dashboards, the chat-roundtrip post-mortem
//! command, sentinel attribution) stop scraping random console
//! text. Every claim "VDD: tokens/sec improved from X → Y" in a
//! PR body should be a query against this command, not a paste
//! from a terminal.
//!
//! Commands:
//! - `vdd/report` — read records from `~/.continuum/vdd/...`,
//!   apply optional git_sha / scenario filters, return list of
//!   matching records + a small aggregate summary.
//!
//! Failure modes (per Joel's never-swallow rule):
//! - Corrupt `record.jsonl` → typed Err, surface the parse error
//!   with the file path so the caller can `cat` the bad artifact.
//! - Missing artifact root → empty result (NOT error); fresh dev
//!   machine has nothing to report and that's a valid state.
//!
//! NOT in this slice:
//! - Cross-PR regression detection (compare two git_shas + flag
//!   tokens/sec regressions). That's a separate report mode that
//!   builds on this primitive — adds a `mode: "regression"` param.
//! - Subscribing to live `RuntimeMetric` events from inference
//!   paths (Lane C PR-1/PR-2 prereqs). This command reads what
//!   the harness has already written; the live-emit path lands
//!   when those PRs are bound.

use crate::logging::TimingGuard;
use crate::runtime::{CommandResult, ModuleConfig, ModuleContext, ModulePriority, ServiceModule};
use crate::utils::params::Params;
use crate::vdd::reader::{latest_per_scenario, read_records, VddReadOptions, VddRecordEntry};
use crate::vdd::record::HarnessStatus;
use async_trait::async_trait;
use serde::Serialize;
use serde_json::Value;
use std::any::Any;
use std::path::{Path, PathBuf};

pub struct VddModule {
    /// Artifact root. In production this points at
    /// `~/.continuum/vdd`; in tests, the harness wires a temp
    /// dir so test data doesn't leak into the dev's real
    /// artifact store.
    artifact_root: PathBuf,
}

impl VddModule {
    pub fn new() -> Self {
        Self {
            artifact_root: default_artifact_root(),
        }
    }

    /// Constructor for tests + non-default deployments. Allows
    /// pointing the module at any artifact root.
    pub fn with_root(root: impl Into<PathBuf>) -> Self {
        Self {
            artifact_root: root.into(),
        }
    }
}

impl Default for VddModule {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve `~/.continuum/vdd` as the canonical artifact root.
/// Matches `vdd::ArtifactWriter::continuum_default()` — that's the
/// writer's path; this is the reader's path; they must agree.
fn default_artifact_root() -> PathBuf {
    dirs::home_dir()
        .expect("home directory must exist for VDD artifact reads")
        .join(".continuum")
        .join("vdd")
}

#[async_trait]
impl ServiceModule for VddModule {
    fn config(&self) -> ModuleConfig {
        ModuleConfig {
            name: "vdd",
            priority: ModulePriority::Background,
            command_prefixes: &["vdd/"],
            event_subscriptions: &[],
            needs_dedicated_thread: false,
            // Pure-read + bounded fs scan; no need to cap fan-out.
            max_concurrency: 0,
            tick_interval: None,
        }
    }

    async fn initialize(&self, _ctx: &ModuleContext) -> Result<(), String> {
        Ok(())
    }

    async fn handle_command(&self, command: &str, params: Value) -> Result<CommandResult, String> {
        match command {
            "vdd/report" => {
                let _timer = TimingGuard::new("module", "vdd_report");
                let p = Params::new(&params);

                let opts = VddReadOptions {
                    git_sha: p.str_opt("git_sha").map(String::from),
                    scenario: p.str_opt("scenario").map(String::from),
                };
                let latest_only = p.bool_or("latest_only", false);

                let entries =
                    read_records(&self.artifact_root, &opts).map_err(|e| e.to_string())?;

                let report = if latest_only {
                    let collapsed = latest_per_scenario(entries);
                    build_report(
                        collapsed.into_values().collect(),
                        &self.artifact_root,
                        &opts,
                    )
                } else {
                    build_report(entries, &self.artifact_root, &opts)
                };

                Ok(CommandResult::Json(
                    serde_json::to_value(&report)
                        .map_err(|e| format!("Serialize VDD report: {e}"))?,
                ))
            }

            other => Err(format!("Unknown vdd command: {other}")),
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// On-the-wire shape returned by `vdd/report`. Stable, camelCase
/// for the TS / CI-dashboard side that consumes it.
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VddReport {
    /// Absolute path the records were read from. Surfaces "where
    /// the harness is writing" to humans + LLM consumers — the
    /// "where did this come from" answer is one field away.
    pub artifact_root: String,
    /// The filters applied. Empty fields are reported back as
    /// null so the consumer's expectation matches what was asked.
    pub filters: VddReportFilters,
    /// Headline counts. Cheap to compute, surface in a banner /
    /// PR-body snippet without iterating the full record list.
    pub summary: VddReportSummary,
    /// The matching records, sorted deterministically by
    /// (git_sha, scenario). The detail layer for any consumer
    /// that wants to drill in on a specific row.
    pub records: Vec<VddReportEntry>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VddReportFilters {
    pub git_sha: Option<String>,
    pub scenario: Option<String>,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VddReportSummary {
    pub total: usize,
    pub passed: usize,
    pub failed: usize,
    pub prerequisite_missing: usize,
}

#[derive(Debug, Clone, Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VddReportEntry {
    pub git_sha: String,
    pub scenario: String,
    pub platform: String,
    pub hardware: String,
    pub backend: String,
    pub status: HarnessStatus,
    pub first_token_ms: Option<u64>,
    pub tok_per_sec: Option<f64>,
    pub responses_observed: u32,
    pub responses_expected: u32,
    pub degraded_reason: Option<String>,
    pub silence_reasons: Vec<String>,
    /// Path to the on-disk `record.jsonl` for this entry. Lets
    /// the consumer fetch the FULL StandardVddRecord (not just
    /// the headline fields surfaced here) on demand without the
    /// report itself carrying every byte of every record.
    pub source: String,
}

fn build_report(
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

#[cfg(test)]
mod tests {
    //! Pin the IPC contract end-to-end: command name + param
    //! parsing + filter passthrough + summary aggregation + JSON
    //! wire shape. Each test seeds a temp artifact root via the
    //! real `ArtifactWriter` so writer/reader/report drift fails
    //! at unit-test time.
    use super::*;
    use crate::vdd::artifacts::{ArtifactWriter, ReproducibilityManifest};
    use crate::vdd::record::{HarnessStatus, StandardVddRecord};

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

    /// What this catches: config exposes the canonical `vdd/`
    /// prefix + module name. If either drifts, the registry routes
    /// the command elsewhere.
    #[test]
    fn config_reports_name_and_prefix() {
        let m = VddModule::new();
        let cfg = m.config();
        assert_eq!(cfg.name, "vdd");
        assert_eq!(cfg.command_prefixes, &["vdd/"]);
    }

    /// What this catches: with no artifact root + no records, the
    /// command returns an empty report (not an error). Fresh dev
    /// machine == valid state.
    #[tokio::test]
    async fn report_with_missing_root_returns_empty_report() {
        let tmp = tempfile::tempdir().unwrap();
        let nonexistent = tmp.path().join("never-created");
        let module = VddModule::with_root(&nonexistent);

        let result = module
            .handle_command("vdd/report", serde_json::json!({}))
            .await
            .expect("empty root returns Ok");

        match result {
            CommandResult::Json(v) => {
                let report: VddReport = serde_json::from_value(v).unwrap();
                assert_eq!(report.summary.total, 0);
                assert_eq!(report.summary.passed, 0);
                assert!(report.records.is_empty());
            }
            _ => panic!("expected Json"),
        }
    }

    /// What this catches: end-to-end command path bundles the
    /// reader's output into the wire report. Aggregates the
    /// summary correctly across pass/fail/prerequisite_missing.
    #[tokio::test]
    async fn report_aggregates_summary_across_record_statuses() {
        let tmp = tempfile::tempdir().unwrap();
        // 2 pass on different shas.
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
        // 1 fail.
        write(
            tmp.path(),
            "sha-c",
            "chat-roundtrip-live-harness",
            HarnessStatus::Fail,
        );
        // 1 prerequisite_missing.
        write(
            tmp.path(),
            "sha-d",
            "chat-roundtrip-live-harness",
            HarnessStatus::PrerequisiteMissing,
        );

        let module = VddModule::with_root(tmp.path());
        let result = module
            .handle_command("vdd/report", serde_json::json!({}))
            .await
            .unwrap();
        let v = match result {
            CommandResult::Json(v) => v,
            _ => panic!("expected Json"),
        };
        let report: VddReport = serde_json::from_value(v).unwrap();
        assert_eq!(report.summary.total, 4);
        assert_eq!(report.summary.passed, 2);
        assert_eq!(report.summary.failed, 1);
        assert_eq!(report.summary.prerequisite_missing, 1);
        assert_eq!(report.records.len(), 4);
    }

    /// What this catches: the `git_sha` filter narrows the result
    /// to one commit's records + reports back the filter on the
    /// wire so the consumer knows what query produced the report.
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

        let module = VddModule::with_root(tmp.path());
        let result = module
            .handle_command("vdd/report", serde_json::json!({"git_sha": "sha-b"}))
            .await
            .unwrap();
        let v = match result {
            CommandResult::Json(v) => v,
            _ => panic!("expected Json"),
        };
        let report: VddReport = serde_json::from_value(v).unwrap();
        assert_eq!(report.summary.total, 1);
        assert_eq!(report.records[0].git_sha, "sha-b");
        // Filter is echoed back so consumers can verify what they queried.
        assert_eq!(report.filters.git_sha.as_deref(), Some("sha-b"));
        assert_eq!(report.filters.scenario, None);
    }

    /// What this catches: `latest_only=true` collapses duplicate
    /// (git_sha, scenario) entries to one row. Used by PR-body
    /// snippets that want "the most recent result per scenario."
    #[tokio::test]
    async fn report_latest_only_collapses_duplicate_scenario_per_sha() {
        let tmp = tempfile::tempdir().unwrap();
        // Two writes to same (sha, scenario): writer overwrites
        // in place, so reader sees the latest.
        write(tmp.path(), "sha-x", "chat-roundtrip", HarnessStatus::Pass);
        write(tmp.path(), "sha-x", "chat-roundtrip", HarnessStatus::Fail);
        // Different scenario on the same sha — should NOT collapse.
        write(tmp.path(), "sha-x", "vision-smoke", HarnessStatus::Pass);

        let module = VddModule::with_root(tmp.path());
        let result = module
            .handle_command("vdd/report", serde_json::json!({"latest_only": true}))
            .await
            .unwrap();
        let v = match result {
            CommandResult::Json(v) => v,
            _ => panic!("expected Json"),
        };
        let report: VddReport = serde_json::from_value(v).unwrap();
        assert_eq!(report.summary.total, 2);
        // (sha-x, chat-roundtrip) entry reports the latest = Fail.
        let chat = report
            .records
            .iter()
            .find(|r| r.scenario == "chat-roundtrip")
            .expect("chat-roundtrip row present");
        assert_eq!(chat.status, HarnessStatus::Fail);
    }

    /// What this catches: unknown vdd command returns a typed Err
    /// per Joel's never-swallow rule. The error mentions the
    /// unknown command so callers debug from the message.
    #[tokio::test]
    async fn unknown_command_returns_loud_error() {
        let tmp = tempfile::tempdir().unwrap();
        let module = VddModule::with_root(tmp.path());
        let result = module
            .handle_command("vdd/bogus", serde_json::json!({}))
            .await;
        match result {
            Err(msg) => {
                assert!(msg.contains("Unknown vdd command"));
                assert!(msg.contains("vdd/bogus"));
            }
            Ok(_) => panic!("unknown command must Err"),
        }
    }

    /// What this catches: wire-shape stability for the
    /// VddReportEntry — surfaces the headline VDD fields (tokens/sec,
    /// first_token_ms, status) AND the source path so consumers can
    /// fetch the full record on demand. PR-body snippets read these
    /// directly.
    #[tokio::test]
    async fn report_entry_carries_headline_fields_and_source_path() {
        let tmp = tempfile::tempdir().unwrap();
        write(
            tmp.path(),
            "sha-w",
            "chat-roundtrip-live-harness",
            HarnessStatus::Pass,
        );

        let module = VddModule::with_root(tmp.path());
        let result = module
            .handle_command("vdd/report", serde_json::json!({}))
            .await
            .unwrap();
        let v = match result {
            CommandResult::Json(v) => v,
            _ => panic!("expected Json"),
        };
        let report: VddReport = serde_json::from_value(v).unwrap();
        let entry = &report.records[0];
        assert_eq!(entry.git_sha, "sha-w");
        assert_eq!(entry.first_token_ms, Some(450));
        assert_eq!(entry.tok_per_sec, Some(28.6));
        assert_eq!(entry.status, HarnessStatus::Pass);
        assert!(
            entry.source.ends_with("record.jsonl"),
            "source path points at the on-disk record file"
        );
        assert!(
            report
                .artifact_root
                .contains(tmp.path().file_name().unwrap().to_str().unwrap()),
            "artifact_root surfaces the resolved root path"
        );
    }
}
