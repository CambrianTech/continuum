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
            name: "hard-rs",
            description: "Hard Rust problems (expression evaluators, algorithmics) — 8 tasks, \
                          rustc compile+run graded. The difficulty ceiling above humaneval-rs.",
            grader: Grader::Rust,
            tasks: 8,
            eval_set: Some("hard-rs.jsonl"),
            source_url: None,
        },
        BenchmarkSpec {
            name: "frontier-rs",
            description: "Frontier Rust — real algorithms (Levenshtein, Dijkstra, O(n log n) LIS, \
                          topo-sort, bignum add, precedence calculator, regex, word-break, min-window, \
                          N-queens count, coin-change, median-of-two-sorted). 12 tasks; the strive-toward \
                          tier where the write→compile→test→fix loop earns problems a small model rarely \
                          nails one-shot.",
            grader: Grader::Rust,
            tasks: 12,
            eval_set: Some("frontier-rs.jsonl"),
            source_url: None,
        },
        BenchmarkSpec {
            name: "coder-eval",
            description: "Continuum coder gym — 13 mixed practical Rust tasks, rustc compile+run \
                          graded (the original held-out genome-loop eval set).",
            grader: Grader::Rust,
            tasks: 13,
            eval_set: Some("coder-eval.jsonl"),
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
        BenchmarkSpec {
            name: "livecodebench",
            description: "LiveCodeBench — contamination-free competitive-programming problems, refreshed over time.",
            grader: Grader::Python,
            tasks: 500,
            eval_set: None,
            source_url: Some("https://huggingface.co/datasets/livecodebench/code_generation_lite"),
        },
        BenchmarkSpec {
            name: "swe-bench-lite",
            description: "SWE-bench Lite — real GitHub issues + repos; a solution is a patch that makes the repo's tests pass. The agentic gold standard; grader is a repo test harness.",
            grader: Grader::Python,
            tasks: 300,
            eval_set: None,
            source_url: Some("https://huggingface.co/datasets/princeton-nlp/SWE-bench_Lite"),
        },
        BenchmarkSpec {
            name: "swe-bench-verified",
            description: "SWE-bench Verified — the 500 human-validated instances (OpenAI). The current \
                          agentic headline the frontier labs report; solution = a repo patch that passes \
                          the real test suite. Official swebench Docker scorer.",
            grader: Grader::Python,
            tasks: 500,
            eval_set: None,
            source_url: Some("https://huggingface.co/datasets/princeton-nlp/SWE-bench_Verified"),
        },
        BenchmarkSpec {
            name: "bigcodebench",
            description: "BigCodeBench — practical tasks with real library calls + rich function-call \
                          reasoning (harder + more realistic than HumanEval). Complete + Instruct splits.",
            grader: Grader::Python,
            tasks: 1140,
            eval_set: None,
            source_url: Some("https://huggingface.co/datasets/bigcode/bigcodebench"),
        },
        BenchmarkSpec {
            name: "evalplus",
            description: "EvalPlus (HumanEval+ / MBPP+) — the same prompts with ~80x more test cases, \
                          catching the subtle-bug passes HumanEval misses. The contamination-resistant \
                          upgrade of the classic function benchmarks.",
            grader: Grader::Python,
            tasks: 164,
            eval_set: None,
            source_url: Some("https://huggingface.co/datasets/evalplus/humanevalplus"),
        },
        BenchmarkSpec {
            name: "apps",
            description: "APPS — 10,000 competitive-programming problems (Introductory / Interview / \
                          Competition). Genuinely hard: full-program synthesis graded on hidden test \
                          cases. The reach tier for the grid.",
            grader: Grader::Python,
            tasks: 10_000,
            eval_set: None,
            source_url: Some("https://huggingface.co/datasets/codeparrot/apps"),
        },
        BenchmarkSpec {
            name: "cruxeval",
            description: "CRUXEval — code REASONING, not generation: predict a function's output for a \
                          given input (and the inverse). Measures the model's execution model, which \
                          the recovery loop + tools should dominate.",
            grader: Grader::Python,
            tasks: 800,
            eval_set: None,
            source_url: Some("https://huggingface.co/datasets/cruxeval-org/cruxeval"),
        },
        BenchmarkSpec {
            name: "aider-polyglot",
            description: "Aider polyglot — 225 hard Exercism problems across 6 languages, graded by \
                          EDITING existing files (not writing from scratch). The real-workflow edit \
                          benchmark; plays directly to code/edit + the recovery loop.",
            grader: Grader::Python,
            tasks: 225,
            eval_set: None,
            source_url: Some("https://github.com/Aider-AI/polyglot-benchmark"),
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
    /// Measure THIS model through the full loop (its own ephemeral lane, living persona
    /// untouched) instead of whatever she's served on — the same-model control. A loadable
    /// id from `ai/inference/models`, e.g. `continuum-ai/qwen2.5-coder-1.5b-instruct-GGUF`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base_model_id: Option<String>,
    /// Team mode: `Some(n>=1)` adds a reviewer teammate (same persona/model) that reviews +
    /// corrects each answer before grading — the undeniable team-vs-solo proof. None = solo.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub reviewers: Option<u32>,
    /// Fire-and-poll (#86): run the eval DETACHED (returns immediately; result lands in the
    /// progress ledger). Essential for long acting/team runs that outlive the client timeout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detach: Option<bool>,
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
    /// Total tokens the model actually GENERATED across the set. A 0% that produced
    /// almost no output is a SERVING failure, not a model score — the harness reads
    /// mean tokens/task to flag a degenerate lane instead of publishing a false 0%
    /// (forged-4B ~65 tok/answer, 14B ~2 tok/answer under GPU contention, 2026-07-10).
    #[serde(rename = "outputTokens")]
    #[ts(type = "number")]
    pub output_tokens: u32,
    /// `output_tokens / total` — mean generated tokens per task. The matrix flags a
    /// cell as "degenerate output (serving suspect)" below a floor rather than 0%.
    #[serde(rename = "meanOutputTokensPerTask")]
    #[ts(type = "number")]
    pub mean_output_tokens_per_task: f64,
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
                    base_model_id: p.base_model_id.clone(),
                    reviewers: p.reviewers,
                    detach: p.detach,
                    run_id: None,
                    max_acts: p.max_acts.or(Some(6)),
                    max_retries: Some(0),
                    workspace_root: None,
                    capture_dir: None,
                    note: Some(match &p.base_model_id {
                        Some(m) => format!("benchmark/run {} on {m}", spec.name),
                        None => format!("benchmark/run {}", spec.name),
                    }),
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
            output_tokens: result.total_output_tokens,
            mean_output_tokens_per_task: if result.total > 0 {
                result.total_output_tokens as f64 / result.total as f64
            } else {
                0.0
            },
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
