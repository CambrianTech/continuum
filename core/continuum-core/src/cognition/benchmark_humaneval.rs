//! HumanEval-rs — OUTLIER A for the benchmark-adapter framework ([[regression-test-on-good-agentic-benchmarks-never-humaneval]]).
//!
//! The quick-cognition smoke rung (a step above arithmetic, per Joel): 156 Rust-translated
//! HumanEval tasks that our EXISTING Rust grader runs directly. Deliberately the SIMPLEST
//! adapter — tiny, RESIDENT (no download), static, test-graded — so that pairing it with a
//! maximally-different OUTLIER B (the Terminal-Bench agentic meta-harness: big download,
//! real-repo, container-graded) proves the [`BenchmarkAdapter`] interface across both
//! extremes (CLAUDE.md outlier-validation). If one trait fits both, every other benchmark
//! is data.
//!
//! It's nearly free because the in-repo `docs/genome/humaneval-rs.jsonl` rows ARE serialized
//! [`EvalTask`]s (`{id, prompt, test, lang}`) — the same shape `cognition::eval` already
//! deserializes — so the adapter is a per-line `serde_json::from_str`. This also exercises
//! the trait's NO-DOWNLOAD branch (`dataset() == None`): a benchmark small enough to bundle
//! needs no fetch, only the big ones (SWE-bench) do.

use std::path::Path;

use async_trait::async_trait;

use crate::cognition::benchmark::{BenchResourceHint, BenchmarkAdapter};
use crate::cognition::eval::EvalTask;
use crate::sdk_codegen::CommandError;

/// Repo-relative location of the bundled task set. Resolved against the core's working
/// directory (the repo root, where personas already act) OR an explicit `dataset_root`.
const HUMANEVAL_RS_JSONL: &str = "docs/genome/humaneval-rs.jsonl";

/// The HumanEval-rs benchmark as a plug-in adapter.
pub struct HumanEvalRsAdapter;

// Self-register at link time — the same `inventory` mechanism commands use, so
// `benchmark::get("humaneval-rs")` resolves with no boot hook (a resident adapter needs no
// dataset download, so it is safe to expose unconditionally).
inventory::submit! {
    crate::cognition::benchmark::BuiltinBenchmarkAdapter {
        make: || std::sync::Arc::new(HumanEvalRsAdapter),
    }
}

#[async_trait]
impl BenchmarkAdapter for HumanEvalRsAdapter {
    fn name(&self) -> &str {
        "humaneval-rs"
    }

    // Resident (bundled in-repo, ~200 KB) → no dataset download. `dataset()` stays `None`,
    // exercising the trait's no-fetch branch; the runner passes `dataset_root: None`.

    fn resources(&self) -> BenchResourceHint {
        // Tiny, self-contained; grades with the in-process Rust harness (no container, no
        // network). Runnable on the leanest node — the quick pulse.
        BenchResourceHint::default()
    }

    async fn tasks(
        &self,
        dataset_root: Option<&Path>,
        limit: Option<usize>,
    ) -> Result<Vec<EvalTask>, CommandError> {
        // `dataset_root` is `None` for a resident benchmark; fall back to the repo-relative
        // path. An explicit root (e.g. a node that materialized it elsewhere) wins.
        let path = match dataset_root {
            Some(root) => root.join(HUMANEVAL_RS_JSONL),
            None => Path::new(HUMANEVAL_RS_JSONL).to_path_buf(),
        };
        let contents = std::fs::read_to_string(&path).map_err(|e| {
            CommandError::NotFound(format!(
                "humaneval-rs task set not found at {} ({e}) — it ships in-repo at \
                 {HUMANEVAL_RS_JSONL}; a grid node running this benchmark needs the repo checked out",
                path.display()
            ))
        })?;
        parse_humaneval_rs(&contents, limit)
    }
}

/// Deserialize the jsonl into [`EvalTask`]s — PURE so the load-bearing parse is unit-tested
/// without touching the filesystem. Each non-blank line is one serialized `EvalTask`
/// (`{id, prompt, test, lang}`; the task's other fields default). A malformed line FAILS
/// LOUD with its line number rather than silently dropping a task and under-reporting the
/// score ([[fallbacks-are-illegal-fail-loud]], and a dropped task would inflate pass-rate).
pub(crate) fn parse_humaneval_rs(
    contents: &str,
    limit: Option<usize>,
) -> Result<Vec<EvalTask>, CommandError> {
    let mut tasks = Vec::new();
    for (i, line) in contents.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let task: EvalTask = serde_json::from_str(line).map_err(|e| {
            CommandError::Invalid(format!(
                "humaneval-rs line {} is not a valid EvalTask: {e}",
                i + 1
            ))
        })?;
        tasks.push(task);
        if limit.is_some_and(|n| tasks.len() >= n) {
            break;
        }
    }
    Ok(tasks)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Two real-shaped rows from docs/genome/humaneval-rs.jsonl.
    const SAMPLE: &str = concat!(
        r#"{"id":"HumanEval_0_x","prompt":"Implement foo","test":"assert_eq!(foo(),1);","lang":"rust"}"#,
        "\n",
        r#"{"id":"HumanEval_1_y","prompt":"Implement bar","test":"assert_eq!(bar(),2);","lang":"rust"}"#,
        "\n",
    );

    // what this catches: the jsonl rows ARE serialized EvalTasks, so the adapter is a
    // per-line deserialize — parse must map {id,prompt,test,lang} onto EvalTask (test→Some,
    // other fields default), honor `limit`, skip blank lines, and FAIL LOUD (not silently
    // drop) on a malformed line so a parse error can never inflate the pass rate.
    #[test]
    fn parse_maps_rows_to_evaltasks_honors_limit_and_fails_loud() {
        let all = parse_humaneval_rs(SAMPLE, None).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].id, "HumanEval_0_x");
        assert_eq!(all[0].prompt, "Implement foo");
        assert_eq!(all[0].test.as_deref(), Some("assert_eq!(foo(),1);"));
        assert_eq!(all[0].lang.as_deref(), Some("rust"));
        // `expect` and the other fields default — the row only carried the four keys.
        assert!(all[0].expect.is_empty());

        // limit caps the set (the quick-pulse mode).
        let one = parse_humaneval_rs(SAMPLE, Some(1)).unwrap();
        assert_eq!(one.len(), 1);

        // blank lines are skipped, not counted or errored.
        let with_blanks = format!("\n{SAMPLE}\n   \n");
        assert_eq!(parse_humaneval_rs(&with_blanks, None).unwrap().len(), 2);

        // a malformed line fails loud with its 1-based line number.
        let bad = "{not json}\n";
        let err = parse_humaneval_rs(bad, None).unwrap_err().to_string();
        assert!(
            err.contains("line 1"),
            "must name the offending line: {err}"
        );
    }

    // what this catches: the adapter registers under the slug `benchmark/run` resolves, and
    // reports itself resident (no dataset download) — the no-fetch branch of the trait.
    #[test]
    fn adapter_identity_and_no_download() {
        let a = HumanEvalRsAdapter;
        assert_eq!(a.name(), "humaneval-rs");
        assert!(a.dataset().is_none(), "resident benchmark = no download");
    }

    // what this catches: the `inventory::submit!` above must make the adapter resolvable
    // through the registry with NO boot hook — this is what lets `benchmark/run` fall back to
    // the adapter registry by name. A missing/typo'd submission fails here, not silently at
    // dispatch time as an "unknown benchmark".
    #[test]
    fn humaneval_self_registers_via_inventory() {
        let a = crate::cognition::benchmark::get("humaneval-rs")
            .expect("humaneval-rs must self-register via inventory (no boot hook)");
        assert_eq!(a.name(), "humaneval-rs");
        assert!(
            crate::cognition::benchmark::names().contains(&"humaneval-rs".to_string()),
            "names() must fold in the inventory builtins for benchmark/list + fail-loud errors"
        );
    }
}
