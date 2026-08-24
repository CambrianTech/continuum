//! `benchmark/round-report` — the round, readable by a stranger.
//!
//! Step 7 of ONE-COMMAND-ROUND: the round's OUTPUT is a report — per-task
//! grades, pace, preserved solution artifacts — assembled read-only from the
//! durable sources the round already writes (the per-task ledger rows, the
//! terminal run row, the bench-receipts solutions dir). Works mid-round (live
//! standings from the streamed rows) and after (full results with latency from
//! the terminal row). Never a new store: this verb only READS.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::cognition::eval::{find_run_row_any_persona, progress_ledger_dir};
use crate::sdk_codegen::command::ActionCommand;
use crate::sdk_codegen::handler::Ctx;
use crate::sdk_codegen::{AccessLevel, CommandError};

#[derive(Debug, Clone, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRoundReportParams.ts"
)]
pub struct BenchmarkRoundReportParams {
    /// The round's run handle. Omit to report the NEWEST round of `benchmark`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
    /// Gym reference — used to find the newest round when `run_id` is omitted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub benchmark: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/RoundTaskLine.ts"
)]
pub struct RoundTaskLine {
    pub task_id: String,
    pub ok: bool,
    #[ts(type = "number")]
    pub acts: u32,
    /// Wall-ms this task took to settle — from the terminal row's results when
    /// the round completed; absent mid-round (the streamed row doesn't carry it).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub latency_ms: Option<u64>,
    /// Preserved solution artifact, when the task wrote one (durable — survives
    /// the world sweep; read it to judge the code, not just the grade).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub solution_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRoundReport.ts"
)]
pub struct BenchmarkRoundReport {
    pub run_id: String,
    pub benchmark: String,
    pub persona_id: String,
    /// false while the round is still grading — the report is live standings.
    pub complete: bool,
    #[ts(type = "number")]
    pub graded: u32,
    #[ts(type = "number")]
    pub passed: u32,
    /// Pass rate over GRADED tasks (live) — the honest running number, never a
    /// projection over ungraded work.
    pub pass_rate_graded: f64,
    pub tasks: Vec<RoundTaskLine>,
    /// Median / p95 settle latency in ms over tasks that carry one.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub median_latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub p95_latency_ms: Option<u64>,
    /// Where the preserved solutions live — the judge-the-code entry point.
    pub receipts_dir: String,
}

/// Parse the streamed `kind:"task"` rows for one run out of a ledger body.
/// (taskId, ok, acts), first-graded first. Pure for tests.
fn task_lines_from(text: &str, run_id: &str) -> Vec<(String, bool, u32)> {
    text.lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v.get("kind").and_then(|k| k.as_str()) == Some("task"))
        .filter(|v| v.get("runId").and_then(|r| r.as_str()) == Some(run_id))
        .filter_map(|v| {
            Some((
                v.get("taskId")?.as_str()?.to_string(),
                v.get("ok").and_then(|o| o.as_bool()).unwrap_or(false), // absent ok on a task row = not a pass; the row writer always sets it
                v.get("acts").and_then(|a| a.as_u64()).unwrap_or(0) as u32, // display-only count; absent = 0 shown
            ))
        })
        .collect()
}

/// The newest task row's runId for a benchmark across one ledger body — mirror
/// of the round verb's resume scan (same key: the row's evalSet is the
/// benchmark REFERENCE as given to the verb).
fn newest_run_for(text: &str, benchmark: &str) -> Option<String> {
    text.lines()
        .rev()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v.get("kind").and_then(|k| k.as_str()) == Some("task"))
        .find(|v| v.get("evalSet").and_then(|e| e.as_str()) == Some(benchmark))
        .and_then(|v| Some(v.get("runId")?.as_str()?.to_string()))
}

#[derive(Default)]
pub struct BenchmarkRoundReport_;

#[async_trait]
impl ActionCommand for BenchmarkRoundReport_ {
    const NAME: &'static str = "benchmark/round-report";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "The round, readable by a stranger: per-task grades, pass rate over graded work, pace \
         stats, and the preserved solution artifacts to judge the code by. Pass run_id, or \
         benchmark to report its newest round. Works mid-round (live standings) and after \
         (full results with latency).";
    type Params = BenchmarkRoundReportParams;
    type Output = BenchmarkRoundReport;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: BenchmarkRoundReportParams,
    ) -> Result<BenchmarkRoundReport, CommandError> {
        let dir = progress_ledger_dir().ok_or_else(|| {
            CommandError::Internal("no HOME — the progress ledger has no location".into())
        })?;
        // One pass over every persona ledger: find the run (by id, or newest for
        // the benchmark) and remember which ledger held it (its file name IS the
        // persona id — the round's sitter).
        let mut run_id = p.run_id.clone();
        let mut persona_id = String::new();
        let mut ledger_text = String::new();
        let entries = std::fs::read_dir(&dir)
            .map_err(|e| CommandError::Internal(format!("progress ledger unreadable: {e}")))?;
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|x| x.to_str()) != Some("jsonl") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let hit = match (&run_id, &p.benchmark) {
                (Some(rid), _) => text.contains(rid.as_str()),
                (None, Some(bench)) => {
                    if let Some(rid) = newest_run_for(&text, bench) {
                        run_id = Some(rid);
                        true
                    } else {
                        false
                    }
                }
                (None, None) => {
                    return Err(CommandError::Invalid(
                        "pass run_id, or benchmark to report its newest round".into(),
                    ))
                }
            };
            if hit {
                persona_id = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default() // non-UTF8 ledger filename cannot happen for uuid-named files; empty = unknown sitter, report still stands
                    .to_string();
                ledger_text = text;
                break;
            }
        }
        let Some(run_id) = run_id else {
            return Err(CommandError::Invalid(format!(
                "no round found for benchmark '{}' in any ledger — has one been dispatched?",
                p.benchmark.as_deref().unwrap_or("<none>") // error-message rendering only, never budgeted
            )));
        };
        if ledger_text.is_empty() {
            return Err(CommandError::Invalid(format!(
                "run {run_id} appears in no persona ledger — wrong id, or the round has not \
                 graded its first task yet"
            )));
        }

        // Terminal row (if the round completed): benchmark identity + per-task latency.
        let terminal = find_run_row_any_persona(&run_id);
        let complete = terminal.is_some();
        let benchmark = task_benchmark(&ledger_text, &run_id)
            .or_else(|| p.benchmark.clone())
            .unwrap_or_else(|| "<unknown>".into()); // identity row absent mid-round with no --benchmark given: label, never a key
        let latency_by_task: std::collections::HashMap<String, u64> = terminal
            .as_ref()
            .and_then(|row| row.get("results")?.as_array().cloned())
            .unwrap_or_default() // in-flight round: no terminal row yet, latency column simply absent
            .iter()
            .filter_map(|r| {
                Some((
                    r.get("id")?.as_str()?.to_string(),
                    r.get("latency_ms").or_else(|| r.get("latencyMs"))?.as_u64()?,
                ))
            })
            .collect();

        // Preserved solutions, keyed by best-effort filename match.
        let receipts_dir = dirs::home_dir()
            .map(|h| h.join(".continuum/bench-receipts").join(&run_id))
            .map(|p| p.display().to_string())
            .unwrap_or_default(); // no HOME already failed above; unreachable in practice
        let solutions: Vec<String> = std::fs::read_dir(format!("{receipts_dir}/solutions"))
            .map(|rd| {
                rd.flatten()
                    .map(|e| e.path().display().to_string())
                    .collect()
            })
            .unwrap_or_default(); // no solutions dir yet = none preserved yet, an honest empty

        let lines = task_lines_from(&ledger_text, &run_id);
        let graded = lines.len() as u32;
        let passed = lines.iter().filter(|(_, ok, _)| *ok).count() as u32;
        let tasks: Vec<RoundTaskLine> = lines
            .into_iter()
            .map(|(task_id, ok, acts)| {
                let solution_path = solutions
                    .iter()
                    .find(|s| {
                        std::path::Path::new(s)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .is_some_and(|n| task_id.contains(n.split('.').next().unwrap_or(n)) || n.contains(task_id.as_str())) // no-dot filename: match on the whole name
                    })
                    .cloned();
                RoundTaskLine {
                    latency_ms: latency_by_task.get(&task_id).copied(),
                    solution_path,
                    task_id,
                    ok,
                    acts,
                }
            })
            .collect();
        let mut lats: Vec<u64> = tasks.iter().filter_map(|t| t.latency_ms).collect();
        lats.sort_unstable();
        let median_latency_ms = (!lats.is_empty()).then(|| lats[lats.len() / 2]);
        let p95_latency_ms = (!lats.is_empty()).then(|| lats[(lats.len() * 95 / 100).min(lats.len() - 1)]);

        Ok(BenchmarkRoundReport {
            run_id,
            benchmark,
            persona_id,
            complete,
            graded,
            passed,
            pass_rate_graded: passed as f64 / graded.max(1) as f64,
            tasks,
            median_latency_ms,
            p95_latency_ms,
            receipts_dir,
        })
    }
}

/// The benchmark reference from the run's own task rows (their evalSet field) —
/// the identity as the round verb was invoked, not the resolved path.
fn task_benchmark(text: &str, run_id: &str) -> Option<String> {
    text.lines()
        .filter_map(|l| serde_json::from_str::<serde_json::Value>(l).ok())
        .filter(|v| v.get("kind").and_then(|k| k.as_str()) == Some("task"))
        .find(|v| v.get("runId").and_then(|r| r.as_str()) == Some(run_id))
        .and_then(|v| Some(v.get("evalSet")?.as_str()?.to_string()))
}

crate::register_stateless_command!(BenchmarkRoundReport_);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the report's grade lines and pass counts must come ONLY
    // from THIS run's task rows — the exact unscoped-counting failure ("51 graded"
    // while the live round had zero) this report exists to end. Other runs' rows,
    // run-summary rows, and malformed lines must all be invisible to it.
    #[test]
    fn report_lines_are_scoped_to_the_run() {
        let text = [
            r#"{"kind":"task","runId":"r1","evalSet":"g.jsonl","taskId":"a","ok":true,"acts":3}"#,
            r#"{"kind":"task","runId":"OTHER","evalSet":"g.jsonl","taskId":"x","ok":true,"acts":1}"#,
            r#"{"runId":"r1","score":1,"total":2}"#,
            "not json at all",
            r#"{"kind":"task","runId":"r1","evalSet":"g.jsonl","taskId":"b","ok":false,"acts":9}"#,
        ]
        .join("\n");
        let lines = task_lines_from(&text, "r1");
        assert_eq!(
            lines,
            vec![("a".into(), true, 3), ("b".into(), false, 9)],
            "exactly this run's task rows, in grade order"
        );
        assert_eq!(newest_run_for(&text, "g.jsonl").as_deref(), Some("r1"));
        assert_eq!(task_benchmark(&text, "r1").as_deref(), Some("g.jsonl"));
    }
}
