//! `benchmark/fetch`: staging a catalogued suite so it can be run, including `SourceReach` (where a suite's rows live and whether this tree can read them).

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};
use super::*;

// ---------------------------------------------------------------------------
// benchmark/fetch — stage a catalogued suite so it can actually be run (#370)
// ---------------------------------------------------------------------------

/// Where a catalogued suite's rows actually live, and whether anything in this tree can read
/// them. Four states, because the four have four different fixes and collapsing any two of
/// them produces a refusal the operator has to go do archaeology on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceReach {
    /// Servable by the HuggingFace rows API at these exact coordinates.
    Rows {
        dataset: &'static str,
        config: &'static str,
        split: &'static str,
    },
    /// HuggingFace-hosted, but a LOADING-SCRIPT dataset: the rows API refuses it outright
    /// ("runs arbitrary Python code"). No config or split makes it work, so a refusal that
    /// merely says "not found" invites an infinite guessing loop. Measured live 2026-08-19
    /// against `datasets-server.huggingface.co/splits` for both rows carrying this.
    HuggingFaceScriptDataset { dataset: &'static str },
    /// A real source, just not one the HF path can read (GitHub raw files, a repo to clone).
    /// Needs its own fetcher; naming that is the honest answer.
    ForeignSource { url: &'static str },
    /// Ships with the binary — there is nothing to pull.
    InTree,
}

impl BenchmarkSpec {
    /// The suite's fetch coordinates, as DATA rather than as an operator's memory.
    ///
    /// `config`/`split` are NOT derivable from the URL and are not uniformly `default`/`test`
    /// — bigcodebench versions its splits (`v0.1.4`), and a wrong guess returns an in-band HF
    /// error that a caller reads as "the suite is empty". So the exceptions live here, in the
    /// ONE place that knows, exactly as [`BenchmarkSpec::swe_dataset`] already does for row
    /// shape. The dataset id is still read back off `source_url` so it is never duplicated.
    pub fn reach(&self) -> SourceReach {
        let Some(url) = self.source_url else {
            return SourceReach::InTree;
        };
        let Some(dataset) = url
            .strip_prefix("https://huggingface.co/datasets/")
            .filter(|id| id.contains('/'))
        else {
            return SourceReach::ForeignSource { url };
        };
        // Loading-script datasets: the rows API cannot serve these at ANY coordinates.
        if matches!(dataset, "codeparrot/apps" | "livecodebench/code_generation_lite") {
            return SourceReach::HuggingFaceScriptDataset { dataset };
        }
        let (config, split) = match dataset {
            // bigcodebench publishes revisions as SPLITS; `test` does not exist. v0.1.4 is
            // the newest as of 2026-08-19 — bump it here when they publish, so the version
            // scored against is a recorded catalog fact and not whatever the default was.
            "bigcode/bigcodebench" => ("default", "v0.1.4"),
            _ => ("default", "test"),
        };
        SourceReach::Rows {
            dataset,
            config,
            split,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkFetchParams.ts"
)]
pub struct BenchmarkFetchParams {
    /// Which catalogued benchmark to stage, as it appears in `benchmark/list`.
    pub benchmark: String,
    /// Dataset config. Defaults to `default` — the HF convention.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub config: Option<String>,
    /// Split to pull. Defaults to `test`, which is what a benchmark is scored on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub split: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkFetchResult.ts"
)]
pub struct BenchmarkFetchResult {
    pub benchmark: String,
    pub dataset: String,
    pub config: String,
    pub split: String,
    /// Rows actually staged. This is the suite's REAL denominator — compare it against the
    /// catalog's `tasks` before trusting any rate computed from it.
    pub rows: usize,
    /// The catalog's declared task count, so a mismatch is visible at fetch time rather than
    /// discovered when a published number turns out to be over the wrong denominator.
    pub declared_tasks: u32,
    /// True when `rows` and `declared_tasks` agree. False is not fatal — datasets are revised
    /// upstream — but a rate published over a disagreeing denominator is not comparable.
    pub denominator_matches: bool,
    /// How many rows actually PROJECT into posable tasks through this suite's `SuiteAdapter`.
    /// `None` = the suite has no adapter yet: its rows are staged but cannot be posed to a
    /// citizen. Staged-but-unposable is the honest middle state, and reporting it as a distinct
    /// value is what keeps a fetched suite from LOOKING runnable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tasks: Option<usize>,
    /// Present only when projection is unavailable or failed, saying which of those it was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub adapter_note: Option<String>,
}

#[derive(Default)]
pub struct BenchmarkFetch;

#[async_trait]
impl ActionCommand for BenchmarkFetch {
    const NAME: &'static str = "benchmark/fetch";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Stage a catalogued benchmark's task list from its `source_url`, cached on disk (#370). \
         The catalog has carried ~20 suites with real source URLs while exactly ONE could be \
         pulled, because the only fetcher was fused to the SWE row shape — every other suite was \
         a name nothing could read. This pulls ANY HuggingFace-hosted suite through the same \
         paging+cache path already proven against SWE-bench Lite. Reports the REAL row count \
         beside the catalog's declared task count, because a pass rate over the wrong \
         denominator is not comparable to anyone else's number. Fails loud on an unknown \
         benchmark, a non-HF source, or a refused dataset — an empty pull is never reported as \
         an empty suite.";
    type Params = BenchmarkFetchParams;
    type Output = BenchmarkFetchResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: BenchmarkFetchParams,
    ) -> Result<BenchmarkFetchResult, CommandError> {
        let spec = known_benchmarks()
            .iter()
            .find(|b| b.name == p.benchmark)
            .ok_or_else(|| {
                CommandError::Invalid(format!(
                    "unknown benchmark `{}` — see `benchmark/list` for the catalogued names",
                    p.benchmark
                ))
            })?;
        // ds-1000 is fetched AND CONVERTED onto the gym rails in one step: the raw
        // rows are not runnable (their oracle is a program, not a test string), so a
        // fetch that stopped at rows would report "staged" for a suite nothing could
        // run — the exact #370 gap this command exists to close.
        if spec.name == "super-masked" {
            let (path, count) = crate::cognition::benchmark_super::materialize_gym(None)
                .await
                .map_err(CommandError::Invalid)?;
            return Ok(BenchmarkFetchResult {
                benchmark: spec.name.to_string(),
                dataset: "allenai/super".to_string(),
                config: "Masked".to_string(),
                split: "all_examples".to_string(),
                rows: count,
                declared_tasks: spec.tasks,
                denominator_matches: count as u32 == spec.tasks,
                tasks: Some(count),
                adapter_note: Some(format!(
                    "converted onto the gym rails at {} — dispatch with \
                     `benchmark/dispatch --name super-masked`",
                    path.display()
                )),
            });
        }
        if spec.name == "algotune" {
            let (path, count) = crate::cognition::benchmark_algotune::materialize_gym(None)
                .await
                .map_err(CommandError::Invalid)?;
            return Ok(BenchmarkFetchResult {
                benchmark: spec.name.to_string(),
                dataset: "github.com/oripress/AlgoTune".to_string(),
                config: "main".to_string(),
                split: "tasks".to_string(),
                rows: count,
                declared_tasks: spec.tasks,
                denominator_matches: count as u32 == spec.tasks,
                tasks: Some(count),
                adapter_note: Some(format!(
                    "cloned + converted onto the gym rails at {} — dispatch with \
                     `benchmark/dispatch --name algotune`",
                    path.display()
                )),
            });
        }

        if spec.name == "terminal-bench" {
            let outcome = crate::cognition::benchmark_terminalbench::materialize_gym(None)
                .await
                .map_err(CommandError::Invalid)?;
            // The skip tally is the Docker seam's honesty contract: the receipt names
            // every excluded task and why, so the denominator is never silently shrunk.
            let mut by_reason: std::collections::BTreeMap<&str, usize> =
                std::collections::BTreeMap::new();
            for (_, reason) in &outcome.skipped {
                // Group on the reason's stable head (before any per-task detail).
                let head = reason.split(':').next().unwrap_or(reason.as_str()); // split always yields ≥1 piece; this is belt-and-suspenders
                *by_reason.entry(head).or_default() += 1;
            }
            let breakdown = by_reason
                .iter()
                .map(|(r, n)| format!("{n}× {r}"))
                .collect::<Vec<_>>()
                .join(", ");
            return Ok(BenchmarkFetchResult {
                benchmark: spec.name.to_string(),
                dataset: "github.com/harbor-framework/terminal-bench-2-1".to_string(),
                config: "main".to_string(),
                split: "tasks".to_string(),
                rows: outcome.converted,
                declared_tasks: spec.tasks,
                denominator_matches: outcome.converted as u32 == spec.tasks,
                tasks: Some(outcome.converted),
                adapter_note: Some(format!(
                    "cloned + converted onto the gym rails at {} — {} of {} registry tasks \
                     converted; {} skipped by the declared Docker seam ({}). Dispatch with \
                     `benchmark/dispatch --name terminal-bench`",
                    outcome.path.display(),
                    outcome.converted,
                    outcome.converted + outcome.skipped.len(),
                    outcome.skipped.len(),
                    breakdown,
                )),
            });
        }

        if spec.name == "mirrorcode" {
            let (path, count) = crate::cognition::benchmark_mirrorcode::materialize_gym(None)
                .await
                .map_err(CommandError::Invalid)?;
            return Ok(BenchmarkFetchResult {
                benchmark: spec.name.to_string(),
                dataset: "github.com/epoch-research/MirrorCode".to_string(),
                config: "main".to_string(),
                split: "data/gold_outputs".to_string(),
                rows: count,
                declared_tasks: spec.tasks,
                denominator_matches: count as u32 == spec.tasks,
                tasks: Some(count),
                adapter_note: Some(format!(
                    "cloned + converted onto the gym rails at {} — one task per public \
                     program, graded on the full recorded case set (visible + hidden \
                     duals) by exact output match; Rust-language variant, visible cases \
                     staged WITH expected outputs — internal signal only; dispatch with \
                     `benchmark/dispatch --name mirrorcode`",
                    path.display()
                )),
            });
        }
        if spec.name == "ds-1000" {
            let (path, count) = crate::cognition::benchmark_ds1000::materialize_gym(None)
                .await
                .map_err(CommandError::Invalid)?;
            return Ok(BenchmarkFetchResult {
                benchmark: spec.name.to_string(),
                dataset: "xlangai/DS-1000".to_string(),
                config: "default".to_string(),
                split: "test".to_string(),
                rows: count,
                declared_tasks: spec.tasks,
                denominator_matches: count as u32 == spec.tasks,
                tasks: Some(count),
                adapter_note: Some(format!(
                    "converted onto the gym rails at {} — every task carries the official \
                     execution oracle as its dod; dispatch with `benchmark/dispatch --name ds-1000`",
                    path.display()
                )),
            });
        }
        let (dataset, def_config, def_split) = match spec.reach() {
            SourceReach::Rows {
                dataset,
                config,
                split,
            } => (dataset, config, split),
            SourceReach::InTree => {
                return Err(CommandError::Invalid(format!(
                    "`{}` is an in-tree suite with no source to pull — it ships with the binary \
                     and is already runnable via its eval_set",
                    spec.name
                )))
            }
            SourceReach::HuggingFaceScriptDataset { dataset } => {
                return Err(CommandError::Invalid(format!(
                    "`{}` is hosted at `{dataset}` as a LOADING-SCRIPT dataset — HuggingFace's \
                     rows API refuses those outright (\"runs arbitrary Python code\"), so NO \
                     config or split makes this work and retrying with different ones is wasted \
                     effort. It needs a fetcher that reads the repo's own files (or an upstream \
                     parquet conversion) before it can be staged.",
                    spec.name
                )))
            }
            SourceReach::ForeignSource { url } => {
                return Err(CommandError::Invalid(format!(
                    "`{}` is sourced from `{url}`, which is not a HuggingFace dataset. Only the \
                     HF rows path is wired; this suite needs its own fetcher.",
                    spec.name
                )))
            }
        };
        let config = p.config.unwrap_or_else(|| def_config.to_string());
        let split = p.split.unwrap_or_else(|| def_split.to_string());

        // PLAN BEFORE ALLOCATING (#56). Staging is a governed RAM consumer; it asks the
        // governor for the headroom it may plan against and refuses with a named shortfall
        // rather than allocating hopefully and letting the allocator arbitrate against a live
        // call. The estimate is the catalog's declared task count × a per-row budget — coarse,
        // and deliberately so: it is a SIZING input, not a measurement, and the footprint
        // reported to the governor after the fetch is the honest number.
        //
        // ~24 KiB/row is derived from the staged suites on disk (SWE rows carry a problem
        // statement + two patches; program rows carry a test body), rounded up so the estimate
        // errs toward refusing rather than toward an OOM.
        const EST_BYTES_PER_ROW: u64 = 24 * 1024;
        // The HF pager reads 100 rows at a time, so one page is the real peak of the streaming
        // mode — the floor below which not even adaptation can run.
        const ROWS_PER_PAGE: u64 = 100;
        let estimated = u64::from(spec.tasks) * EST_BYTES_PER_ROW;
        let page_bytes = ROWS_PER_PAGE * EST_BYTES_PER_ROW;
        let plan = crate::cognition::bench_staging::plan_against_governor(estimated, page_bytes);
        if let Some(why) = plan.explain_refusal() {
            crate::probe!(
                class = "benchmark.staging.refused",
                benchmark = spec.name,
                estimated_bytes = estimated,
                page_bytes = page_bytes,
                "staging refused: not even one page fits the governor's RAM plan",
            );
            return Err(CommandError::Denied(why));
        }
        let staging = crate::cognition::bench_staging::staging_area();
        // Declare the plan's PEAK, not the suite size — for a streamed plan those differ by the
        // whole point of streaming, and reporting the suite would have the governor evicting a
        // peer to make room for bytes staging is never going to hold.
        staging.hold(plan.peak_bytes());
        let streaming = matches!(plan, crate::cognition::bench_staging::StagingPlan::Streamed { .. });
        crate::probe!(
            class = "benchmark.staging.plan",
            benchmark = spec.name,
            streaming = streaming,
            peak_bytes = plan.peak_bytes(),
            estimated_bytes = estimated,
            "staging planned against the governor's RAM headroom",
        );

        // THE ADAPTATION. Both arms project every row and both report the same counts; they
        // differ only in whether the rows are ever all in memory at once. `count_projectable`
        // needs a slice, so the streamed arm projects page-locally through the same adapter —
        // one row alive at a time.
        let (row_count, tasks, adapter_note) = if streaming {
            let mut projected = 0usize;
            let mut failure: Option<String> = None;
            let streamed = crate::cognition::swe_bench::stream_hf_rows(
                dataset,
                &config,
                &split,
                |row| {
                    if failure.is_some() {
                        return Ok(());
                    }
                    match crate::cognition::bench_task::count_projectable(
                        spec.name,
                        std::slice::from_ref(row),
                    ) {
                        Ok(n) => projected += n,
                        // Record and keep draining: aborting mid-stream would leave the partial
                        // cache unpromoted AND lose the row count, so the caller could not tell
                        // a projection failure from a fetch failure.
                        Err(e) => failure = Some(e),
                    }
                    Ok(())
                },
            )
            .await
            .map_err(CommandError::Internal)?;
            match failure {
                Some(e) => (streamed, None, Some(e)),
                None => (streamed, Some(projected), None),
            }
        } else {
            let rows = crate::cognition::swe_bench::fetch_hf_rows(dataset, &config, &split)
                .await
                .map_err(CommandError::Internal)?;
            let (tasks, note) =
                match crate::cognition::bench_task::count_projectable(spec.name, &rows) {
                    Ok(n) => (Some(n), None),
                    Err(e) => (None, Some(e)),
                };
            let n = rows.len();
            drop(rows);
            (n, tasks, note)
        };
        // The footprint returns to zero the moment the rows are gone. A consumer that keeps
        // reporting bytes it no longer holds makes the governor evict a REAL holder to recover
        // memory nobody has — the mirror image of the OOM this whole path exists to prevent.
        staging.drop_all();

        let denominator_matches = row_count as u32 == spec.tasks;
        crate::probe!(
            class = "benchmark.suite.staged",
            benchmark = spec.name,
            dataset = dataset,
            rows = row_count,
            declared = spec.tasks,
            streamed = streaming,
            denominator_matches = denominator_matches,
            "benchmark suite staged from its catalog source",
        );
        if !denominator_matches {
            tracing::warn!(
                benchmark = %spec.name,
                staged = row_count,
                declared = spec.tasks,
                "staged row count disagrees with the catalog's declared tasks — any rate \
                 computed over this is NOT comparable until the denominator is reconciled"
            );
        }
        Ok(BenchmarkFetchResult {
            benchmark: spec.name.to_string(),
            dataset: dataset.to_string(),
            config,
            split,
            rows: row_count,
            declared_tasks: spec.tasks,
            denominator_matches,
            tasks,
            adapter_note,
        })
    }
}

crate::register_stateless_command!(BenchmarkRuns);
crate::register_stateless_command!(BenchmarkRounds);
crate::register_stateless_command!(BenchmarkFetch);
