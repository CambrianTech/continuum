//! `BenchmarkAdapter` — the ONE interface every benchmark plugs into, so "run, target,
//! and LEARN from GOOD benchmarks" becomes a grid-transparent primitive, not a pile of
//! bespoke python ([[regression-test-on-good-agentic-benchmarks-never-humaneval]]).
//!
//! The contract, in Joel's words: "any command can run anywhere, so can benchmarks — a
//! persona in any continuum can bench anywhere." That settles the architecture:
//!
//! - The RUNNER is the `benchmark/run` **DynCommand** (see `commands/benchmark/run.rs`).
//!   Because it is a command, `Commands.execute("benchmark/run", …)` routes local-or-remote
//!   over airc exactly like every other command — dispatch a heavy SWE-bench sweep to the
//!   5090, a quick HumanEval pulse to a laptop; a persona self-benchmarks or benchmarks a
//!   served model on another node. Python scripts can't route the mesh; a command can.
//!   ([[commands-are-agency-algs-are-pathways]], [[microkernel-command-event-stream-decomposition-is-why-misfits-beat-cloud]])
//!
//! - Each benchmark is an ADAPTER: an OPTIONAL dataset download + a reusable loader + a
//!   grader. The DATASET is never bundled — [`BenchmarkAdapter::dataset`] declares where to
//!   fetch it and the runner downloads on demand and caches it. The ADAPTER is the reusable
//!   unit others pull and run.
//!
//! - Tasks are yielded in the CANONICAL shape ([`crate::cognition::eval::EvalTask`]) so the
//!   SAME `agent/solve` path runs them — the persona plugs in as a whole AGENT, never a bare
//!   LLM ([[eval-measures-the-true-full-being-not-a-stripped-copy]], #218).
//!
//! - Results are CURRICULUM, not just a scoreboard: a failed task flows into the
//!   salience→curriculum→train loop (#116/#122). RUN → TARGET → LEARN.
//!
//! Outlier-validation build order (prove the interface on the two most different, then the
//! rest are data): OUTLIER A = HumanEval (tiny download, static, test-graded — the quick
//! cognition pulse); OUTLIER B = the Terminal-Bench `ContinuumAgent` adapter (agentic,
//! external meta-harness that unlocks TB's whole registry — see
//! `docs/architecture/BENCHMARK-HARNESS-INTEGRATION.md`).

use std::collections::HashMap;
use std::sync::{Arc, LazyLock, RwLock};

use async_trait::async_trait;

use crate::cognition::eval::EvalTask;
use crate::sdk_codegen::CommandError;

/// Where a benchmark's dataset comes from — declared by the adapter, fetched ON DEMAND by
/// the runner (never bundled in the repo). "Just the optional download and the adapters."
#[derive(Debug, Clone)]
pub struct DatasetSpec {
    /// Hugging Face repo id (`org/name`) or a plain HTTPS URL to fetch from.
    pub source: String,
    /// Whether `source` is an HF dataset repo (`hf`) vs. a direct URL (`url`) vs. a git repo
    /// (`git`, for real-repo agentic benchmarks like SWE-bench). Kept as an enum-friendly
    /// string so the runner picks the fetch strategy without the adapter knowing transport.
    pub kind: DatasetKind,
    /// Subdirectory under the shared benchmark cache (`~/.continuum/benchmarks/<cache_key>`)
    /// where the dataset materializes. One canonical cache so a downloaded set is reused
    /// across runs and personas, never re-fetched per invocation.
    pub cache_key: String,
    /// Optional glob/pattern restricting what to fetch (e.g. only `*.jsonl`, skip weights).
    pub allow_patterns: Vec<String>,
}

/// The fetch strategy for a [`DatasetSpec`]. Enumerated (not stringly-typed) so a new source
/// kind forces the runner's fetch match to be revisited, never silently skipped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatasetKind {
    /// Hugging Face dataset/model repo — `snapshot_download`-style fetch.
    HuggingFace,
    /// A single HTTPS artifact (a `.jsonl`/`.zip` the adapter parses).
    Url,
    /// A git repository (real-repo agentic benchmarks clone per-task from here).
    Git,
}

/// A resource hint the grid placement uses to route a `benchmark/run` to a capable node —
/// the same demand-vs-resource negotiation the governor already does for serving/eval lanes
/// ([[governor-and-config-sync-are-grid-daemons-negotiating-between-elastic-machines-not-hardcoded]]).
/// All fields optional: an unknown hint means "place anywhere," never a hard block.
#[derive(Debug, Clone, Default)]
pub struct BenchResourceHint {
    /// Approx disk the dataset needs once materialized (bytes). Guides which node caches it.
    pub dataset_bytes: Option<u64>,
    /// True if grading needs a container runtime (SWE-bench docker, Terminal-Bench). A node
    /// without it is not a candidate — surfaced as data, not a crash.
    pub needs_container: bool,
    /// True if tasks need outbound web access (BrowseComp et al.).
    pub needs_network: bool,
}

/// One benchmark, as a plug-in. Implemented once per benchmark; the runner + grid + learning
/// loop treat every adapter identically.
#[async_trait]
pub trait BenchmarkAdapter: Send + Sync {
    /// Stable slug used on the CLI + the leaderboard (`humaneval`, `terminal-bench`,
    /// `swe-bench-verified`, …). Must match the registry key.
    fn name(&self) -> &str;

    /// Where the dataset comes from, or `None` for a benchmark whose tasks are generated /
    /// already resident (no download). The runner materializes this before `tasks`.
    fn dataset(&self) -> Option<DatasetSpec> {
        None
    }

    /// What kind of node can run this — for grid placement. Default: place anywhere.
    fn resources(&self) -> BenchResourceHint {
        BenchResourceHint::default()
    }

    /// Load the benchmark's items into the canonical [`EvalTask`] shape, from the
    /// already-materialized dataset at `dataset_root` (the runner passes the resolved cache
    /// path, or `None` when [`Self::dataset`] is `None`). `limit` caps items for a quick
    /// pulse (`None` = full suite). This is the ONLY per-benchmark parsing; everything
    /// downstream is generic.
    async fn tasks(
        &self,
        dataset_root: Option<&std::path::Path>,
        limit: Option<usize>,
    ) -> Result<Vec<EvalTask>, CommandError>;

    /// How a completed task is graded. The DEFAULT delegates to the `EvalTask`'s own grader
    /// (substring `expect` / `test` program — already implemented in `cognition::eval`),
    /// which covers HumanEval-style benchmarks. Real-repo benchmarks (SWE-bench: apply the
    /// patch, run the repo's tests) OVERRIDE this to grade the actual workspace state after
    /// the agent acted. Returns pass + a 0..1 score + a short reason for the receipt.
    async fn grade(
        &self,
        _task: &EvalTask,
        outcome: &TaskOutcome,
    ) -> Result<BenchGrade, CommandError> {
        // Default: trust the harness's built-in grade of this task (set by cognition::eval /
        // agent::solve when it ran the EvalTask's own `test`/`expect`).
        Ok(BenchGrade {
            passed: outcome.harness_passed,
            score: if outcome.harness_passed { 1.0 } else { 0.0 },
            reason: outcome.harness_detail.clone(),
        })
    }
}

/// The artifacts of running ONE task through `agent/solve` — handed to [`BenchmarkAdapter::grade`].
/// Backend-neutral: it carries the persona's spoken answer, the workspace diff (the "hands"
/// artifact SWE-style benchmarks test), and whatever the generic harness already graded.
#[derive(Debug, Clone)]
pub struct TaskOutcome {
    /// The persona's final spoken answer (mouth) — some benchmarks grade this.
    pub spoken: String,
    /// Unified `git diff` of everything the persona changed (hands) — SWE/Terminal grade this.
    pub patch: String,
    /// The workspace path after the agent acted (for graders that re-run repo tests).
    pub workspace: std::path::PathBuf,
    /// Whether the generic `EvalTask` grader already passed it (the default-grade path).
    pub harness_passed: bool,
    /// Human-readable detail from the generic grade.
    pub harness_detail: String,
}

/// A per-task grade — pass + normalized score + a receipt reason. Aggregated into the run's
/// scorecard and, on failure, fed to the curriculum/training loop.
#[derive(Debug, Clone)]
pub struct BenchGrade {
    pub passed: bool,
    pub score: f64,
    pub reason: String,
}

/// Process-global registry of benchmark adapters, keyed by [`BenchmarkAdapter::name`].
/// Adapters register at startup; `benchmark/run` looks one up by name. Same shape as the
/// RAG source registry — one canonical place, discovered not hard-switched.
static REGISTRY: LazyLock<RwLock<HashMap<String, Arc<dyn BenchmarkAdapter>>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Register (or replace) a benchmark adapter. Idempotent by name.
pub fn register(adapter: Arc<dyn BenchmarkAdapter>) {
    REGISTRY
        .write()
        .unwrap_or_else(|p| p.into_inner())
        .insert(adapter.name().to_string(), adapter);
}

/// Look up an adapter by name, or `None` if unregistered.
pub fn get(name: &str) -> Option<Arc<dyn BenchmarkAdapter>> {
    REGISTRY
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .get(name)
        .cloned()
}

/// All registered benchmark names, sorted — for `benchmark/list` + a fail-loud "unknown
/// benchmark 'X'; known: …" error instead of a silent miss.
pub fn names() -> Vec<String> {
    let mut v: Vec<String> = REGISTRY
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .keys()
        .cloned()
        .collect();
    v.sort();
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    struct StubBench;
    #[async_trait]
    impl BenchmarkAdapter for StubBench {
        fn name(&self) -> &str {
            "stub"
        }
        async fn tasks(
            &self,
            _root: Option<&std::path::Path>,
            _limit: Option<usize>,
        ) -> Result<Vec<EvalTask>, CommandError> {
            Ok(vec![EvalTask::default()])
        }
    }

    // what this catches: the registry is the single lookup seam `benchmark/run` resolves
    // against — register→get round-trips by name, an unknown name is a clean miss (fail-loud
    // upstream, never a silent skip), and `names()` lists what's available for the error msg.
    #[test]
    fn registry_round_trips_and_reports_unknown() {
        register(Arc::new(StubBench));
        assert!(get("stub").is_some(), "a registered adapter resolves by name");
        assert!(
            get("does-not-exist").is_none(),
            "an unknown benchmark is a clean None, so the runner can fail loud with the list"
        );
        assert!(names().contains(&"stub".to_string()));
    }

    // what this catches: the DEFAULT grade delegates to the harness verdict — a HumanEval-
    // style benchmark whose EvalTask carried its own `test`/`expect` grade needs NO custom
    // grader, so the trait's default must faithfully pass the harness result through.
    #[tokio::test]
    async fn default_grade_delegates_to_harness_verdict() {
        let b = StubBench;
        let pass = TaskOutcome {
            spoken: String::new(),
            patch: String::new(),
            workspace: std::path::PathBuf::from("/tmp"),
            harness_passed: true,
            harness_detail: "ran the unit test, exit 0".into(),
        };
        let g = b.grade(&EvalTask::default(), &pass).await.unwrap();
        assert!(g.passed && g.score == 1.0);
        let fail = TaskOutcome { harness_passed: false, ..pass };
        let g = b.grade(&EvalTask::default(), &fail).await.unwrap();
        assert!(!g.passed && g.score == 0.0);
    }
}
