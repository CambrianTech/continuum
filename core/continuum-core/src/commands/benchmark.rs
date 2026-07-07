//! `benchmark/*` — first-class, persona-callable benchmark competitions, managed like the
//! model catalog. A declarative catalog of known benchmark collections (add one = one row,
//! same as a `ModelSpec`) plus commands to LIST them and RUN one by name. `benchmark/run` is a
//! thin wrapper over `cognition/eval` — it resolves a benchmark and delegates, never
//! reimplementing the grader. This lives in Rust, on the DynCommand registry, so it is
//! ON-GRID: discoverable, persona-callable, and manageable by the daemons — unlike the
//! toolchain-free `benchmarks/coder/oneshot_opponent.py` script, whose ONLY job is letting an
//! OUTSIDER replicate our numbers against their own `/v1` without our stack. Operational
//! benchmarking is Rust; the replication convenience is the lone edge script.

use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::cognition::eval::{CognitionEval, CognitionEvalParams};
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

/// How a benchmark's solutions are scored.
#[derive(Debug, Clone, Copy, Serialize, TS)]
pub enum Grader {
    /// Compile + run each solution (rustc). Live today via cognition/eval's `test_grade`.
    Rust,
    /// Execute each solution (python). Catalogued; grader lands with the python collections.
    Python,
}

/// One known benchmark collection — mirrors a `model_registry::ModelSpec` row.
pub struct BenchmarkSpec {
    pub name: &'static str,
    pub description: &'static str,
    pub grader: Grader,
    pub tasks: u32,
    /// What to hand `cognition/eval` as its `eval_set`: a committed/embedded gym name for the
    /// in-tree ones (resolved by the gym resolver, works deployed), or a cache filename for a
    /// pulled collection. `None` = catalogued but not yet runnable through the eval grader.
    pub eval_set: Option<&'static str>,
    /// Source to pull a large collection from (cached under ~/.continuum/benchmarks/).
    pub source_url: Option<&'static str>,
}

/// THE benchmark catalog. Add a respected collection (SWE-bench, LiveCodeBench, …) = one row.
pub fn known_benchmarks() -> &'static [BenchmarkSpec] {
    &[
        BenchmarkSpec {
            name: "humaneval-rs",
            description: "HumanEval ported to Rust — 164 tasks, graded by rustc compile+run.",
            grader: Grader::Rust,
            tasks: 164,
            eval_set: Some("humaneval-rs.jsonl"),
            source_url: None,
        },
        BenchmarkSpec {
            name: "humaneval",
            description: "OpenAI HumanEval (Python) — the original, 164 tasks.",
            grader: Grader::Python,
            tasks: 164,
            eval_set: None,
            source_url: Some("https://github.com/openai/human-eval/raw/master/data/HumanEval.jsonl.gz"),
        },
        BenchmarkSpec {
            name: "mbpp",
            description: "MBPP (Mostly Basic Python Problems) — ~974 tasks.",
            grader: Grader::Python,
            tasks: 974,
            eval_set: None,
            source_url: Some("https://raw.githubusercontent.com/google-research/google-research/master/mbpp/mbpp.jsonl"),
        },
    ]
}

// ---- benchmark/list ------------------------------------------------------------------------

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
pub struct BenchmarkListParams {}

#[derive(Debug, Clone, Serialize, TS)]
pub struct BenchmarkRow {
    pub name: String,
    pub description: String,
    pub grader: String,
    #[ts(type = "number")]
    pub tasks: u32,
    /// True when it can be RUN today (has an eval_set the grader understands).
    pub runnable: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct BenchmarkListResult {
    pub benchmarks: Vec<BenchmarkRow>,
}

/// `benchmark/list` — the catalog of known benchmark competitions.
#[derive(Default)]
pub struct BenchmarkList;

#[async_trait]
impl ActionCommand for BenchmarkList {
    const NAME: &'static str = "benchmark/list";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "List the known benchmark competitions (name, grader, task count, whether it can be run \
         now). Use `benchmark/run` with a name to compete on one.";
    type Params = BenchmarkListParams;
    type Output = BenchmarkListResult;

    async fn run(&self, _ctx: &Ctx, _p: BenchmarkListParams) -> Result<BenchmarkListResult, CommandError> {
        Ok(BenchmarkListResult {
            benchmarks: known_benchmarks()
                .iter()
                .map(|b| BenchmarkRow {
                    name: b.name.to_string(),
                    description: b.description.to_string(),
                    grader: format!("{:?}", b.grader).to_lowercase(),
                    tasks: b.tasks,
                    runnable: b.eval_set.is_some(),
                })
                .collect(),
        })
    }
}
crate::register_stateless_command!(BenchmarkList);

// ---- benchmark/run -------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct BenchmarkRunParams {
    /// The persona (UUID) to put through the benchmark — her real cognition competes.
    pub persona_id: String,
    /// The benchmark name (see `benchmark/list`), e.g. `humaneval-rs`.
    pub name: String,
    /// How many tasks (from the top). Omit for a default slice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub limit: Option<u32>,
    /// Max act→observe cycles per task. Default 6.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub max_acts: Option<u32>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct BenchmarkRunResult {
    pub benchmark: String,
    #[ts(type = "number")]
    pub score: u32,
    #[ts(type = "number")]
    pub total: u32,
    #[ts(type = "number")]
    pub pass_rate: f64,
}

/// `benchmark/run` — compete a persona on a named benchmark. Thin wrapper over
/// `cognition/eval`: resolve the benchmark → delegate → return the objective pass-rate.
#[derive(Default)]
pub struct BenchmarkRun;

#[async_trait]
impl ActionCommand for BenchmarkRun {
    const NAME: &'static str = "benchmark/run";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Compete a persona on a named benchmark (see benchmark/list). Runs her real cognition \
         through the benchmark and returns the objective pass-rate. e.g. name=\"humaneval-rs\".";
    type Params = BenchmarkRunParams;
    type Output = BenchmarkRunResult;

    async fn run(&self, ctx: &Ctx, p: BenchmarkRunParams) -> Result<BenchmarkRunResult, CommandError> {
        let spec = known_benchmarks()
            .iter()
            .find(|b| b.name == p.name)
            .ok_or_else(|| {
                CommandError::NotFound(format!(
                    "unknown benchmark '{}'. Known: {}. Call benchmark/list.",
                    p.name,
                    known_benchmarks().iter().map(|b| b.name).collect::<Vec<_>>().join(", "),
                ))
            })?;
        let eval_set = spec.eval_set.ok_or_else(|| {
            CommandError::Invalid(format!(
                "benchmark '{}' is catalogued but not yet runnable through the grader (needs its \
                 dataset pulled + a {:?} grader). Runnable today: {}.",
                spec.name,
                spec.grader,
                known_benchmarks().iter().filter(|b| b.eval_set.is_some()).map(|b| b.name).collect::<Vec<_>>().join(", "),
            ))
        })?;

        // The eval runs a whole eval_set and has no task limit; a full 164-task agentic run is
        // impractical live. Resolve the gym CONTENT (embedded or on-disk), slice to `limit`,
        // and hand the eval a temp file. Default a small, quick slice.
        let (_gym_name, content) =
            crate::cognition::gym::resolve_gym(eval_set).map_err(CommandError::Invalid)?;
        let limit = p.limit.unwrap_or(20) as usize;
        let sliced: String = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .take(limit)
            .collect::<Vec<_>>()
            .join("\n");
        let tmp = std::env::temp_dir()
            .join(format!("benchmark_{}_{}.jsonl", spec.name, std::process::id()));
        std::fs::write(&tmp, &sliced)
            .map_err(|e| CommandError::Internal(format!("benchmark slice write failed: {e}")))?;

        // Delegate to the ONE grader (cognition/eval) — never reimplement it here.
        let result = CognitionEval
            .run(
                ctx,
                CognitionEvalParams {
                    persona_id: p.persona_id,
                    gene: None,
                    room_id: None,
                    tasks: None,
                    eval_set: Some(tmp.display().to_string()),
                    max_acts: p.max_acts.or(Some(6)),
                    max_retries: Some(0),
                    note: Some(format!("benchmark/run {}", spec.name)),
                },
            )
            .await;
        let _ = std::fs::remove_file(&tmp);
        let result = result?;

        Ok(BenchmarkRunResult {
            benchmark: spec.name.to_string(),
            score: result.score,
            total: result.total,
            pass_rate: result.pass_rate,
        })
    }
}
crate::register_stateless_command!(BenchmarkRun);

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the catalog is non-empty, humaneval-rs is present + runnable (has an
    // eval_set the grader understands), and every runnable benchmark names a real committed gym.
    // Guards the persona-callable competition surface from a stale/broken catalog row.
    #[test]
    fn catalog_has_a_runnable_committed_benchmark() {
        let ks = known_benchmarks();
        assert!(!ks.is_empty(), "the benchmark catalog must not be empty");
        let hr = ks.iter().find(|b| b.name == "humaneval-rs").expect("humaneval-rs catalogued");
        assert!(hr.eval_set.is_some(), "humaneval-rs must be runnable");
        assert!(matches!(hr.grader, Grader::Rust));
        // Every runnable benchmark's eval_set must resolve through the gym resolver.
        for b in ks.iter().filter(|b| b.eval_set.is_some()) {
            crate::cognition::gym::resolve_gym(b.eval_set.unwrap())
                .unwrap_or_else(|e| panic!("benchmark '{}' eval_set does not resolve: {e}", b.name));
        }
    }
}
