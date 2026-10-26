use super::*;

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
                    "converted onto the gym rails at {} — run it as a round with \
                     `activity/spawn --name <room> --recipe benchmark/round --params '{{\"suite\":\"super-masked\",\"repo\":\"<owner/name>\"}}'`",
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
                    "cloned + converted onto the gym rails at {} — run it as a round with \
                     `activity/spawn --name <room> --recipe benchmark/round --params '{{\"suite\":\"algotune\",\"repo\":\"<owner/name>\"}}'`",
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
                     converted; {} skipped by the declared Docker seam ({}). Run it as a round with \
                     `activity/spawn --name <room> --recipe benchmark/round --params '{{\"suite\":\"terminal-bench\",\"repo\":\"<owner/name>\"}}'`",
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
                     staged WITH expected outputs — internal signal only; run it as a round with \
                     `activity/spawn --name <room> --recipe benchmark/round --params '{{\"suite\":\"mirrorcode\",\"repo\":\"<owner/name>\"}}'`",
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
                     execution oracle as its dod; run it as a round with `activity/spawn --name <room> --recipe benchmark/round --params '{{\"suite\":\"ds-1000\",\"repo\":\"<owner/name>\"}}'`",
                    path.display()
                )),
            });
        }
        let (dataset, def_config, def_split) = match source_reach(spec) {
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
        let plan = crate::cognition::measurement_staging::plan_against_governor(estimated, page_bytes);
        if let Some(why) = plan.explain_refusal() {
            continuum_base::probe!(
                class = "benchmark.staging.refused",
                benchmark = spec.name,
                estimated_bytes = estimated,
                page_bytes = page_bytes,
                "staging refused: not even one page fits the governor's RAM plan",
            );
            return Err(CommandError::Denied(why));
        }
        let staging = crate::cognition::measurement_staging::staging_area();
        // Declare the plan's PEAK, not the suite size — for a streamed plan those differ by the
        // whole point of streaming, and reporting the suite would have the governor evicting a
        // peer to make room for bytes staging is never going to hold.
        staging.hold(plan.peak_bytes());
        let streaming = matches!(plan, crate::cognition::measurement_staging::StagingPlan::Streamed { .. });
        continuum_base::probe!(
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
        continuum_base::probe!(
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


crate::register_stateless_command!(BenchmarkFetch);
