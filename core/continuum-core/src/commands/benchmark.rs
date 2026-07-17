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
            name: "games-rs",
            description: "Games-Rust — OUR games benchmark, the tier public benchmarks lack (they \
                          grade an agent PLAYING a game, not BUILDING one). Auto-verifiable game LOGIC: \
                          Conway's Life step, tic-tac-toe + connect-4 win-checkers, a 2048 merge, chess \
                          knight moves, minesweeper counts. 6 tasks, reference-verified; the runnable \
                          complement to the whole-game project cards.",
            grader: Grader::Rust,
            tasks: 6,
            eval_set: Some("games-rs.jsonl"),
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
        // ── Whole-app / whole-project / website tier — the real 'build the thing' benchmarks ──
        BenchmarkSpec {
            name: "swe-lancer",
            description: "SWE-Lancer (OpenAI) — real freelance software-engineering gigs (whole \
                          features + management decisions) worth real money, graded by end-to-end \
                          tests. The closest public benchmark to 'ship a real feature for pay'.",
            grader: Grader::Python,
            tasks: 1488,
            eval_set: None,
            source_url: Some("https://github.com/openai/SWELancer-Benchmark"),
        },
        BenchmarkSpec {
            name: "commit0",
            description: "Commit0 — build an ENTIRE library from a spec + its unit tests (from-scratch \
                          whole-project synthesis, not a patch). Graded by the library's own test suite.",
            grader: Grader::Python,
            tasks: 54,
            eval_set: None,
            source_url: Some("https://github.com/commit-0/commit0"),
        },
        BenchmarkSpec {
            name: "mle-bench",
            description: "MLE-bench (OpenAI) — end-to-end Kaggle competitions: a whole ML project \
                          (data → model → submission) graded on the real leaderboard metric. \
                          Long-horizon agentic project work.",
            grader: Grader::Python,
            tasks: 75,
            eval_set: None,
            source_url: Some("https://github.com/openai/mle-bench"),
        },
        BenchmarkSpec {
            name: "design2code",
            description: "Design2Code — generate a working webpage from a SCREENSHOT (visual → HTML/CSS), \
                          scored on visual + structural fidelity. The website-building benchmark; pairs \
                          with the Screenshotter vision family.",
            grader: Grader::Python,
            tasks: 484,
            eval_set: None,
            source_url: Some("https://github.com/NoviScl/Design2Code"),
        },
        BenchmarkSpec {
            name: "webarena",
            description: "WebArena — an agent operating REAL self-hosted websites (e-commerce, forums, \
                          CMS, GitLab) to complete multi-step tasks. The web-agent gold standard; graded \
                          by task-outcome checks on the live sites.",
            grader: Grader::Python,
            tasks: 812,
            eval_set: None,
            source_url: Some("https://github.com/web-arena-x/webarena"),
        },
        BenchmarkSpec {
            name: "appworld",
            description: "AppWorld — control 9 real apps (calendar, email, shopping, …) via their APIs on \
                          complex day-in-the-life tasks. Interactive multi-app coordination; graded by \
                          world-state outcomes.",
            grader: Grader::Python,
            tasks: 750,
            eval_set: None,
            source_url: Some("https://github.com/StonyBrookNLP/appworld"),
        },
        BenchmarkSpec {
            name: "terminal-bench",
            description: "Terminal-Bench — end-to-end tasks completed in a real terminal (build, debug, \
                          configure, script). Graded by outcome checks; plays directly to code/shell + \
                          the recovery loop.",
            grader: Grader::Python,
            tasks: 100,
            eval_set: None,
            source_url: Some("https://github.com/laude-institute/terminal-bench"),
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
        // impractical live. Resolve the gym CONTENT (embedded or on-disk), slice to `limit`, and
        // hand the eval the parsed tasks INLINE. Inline `tasks` takes precedence over `eval_set`
        // and — unlike a temp-file path — carries no CWD dependence and no cleanup race: a DETACHED
        // run's eval executes in a spawned task that outlives this handler, so a temp file we wrote
        // here and removed on return (the old shape) had already vanished by the time the detached
        // eval tried to read it ("no such file on disk"). Inline tasks live in the params moved
        // into the spawned task — they cannot go missing. Default a small, quick slice.
        let (gym_name, content) =
            crate::cognition::gym::resolve_gym(eval_set).map_err(CommandError::Invalid)?;
        let limit = p.limit.unwrap_or(20) as usize;
        let sliced_tasks: Vec<crate::cognition::eval::EvalTask> = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .take(limit)
            .enumerate()
            .map(|(n, l)| {
                serde_json::from_str::<crate::cognition::eval::EvalTask>(l).map_err(|e| {
                    CommandError::Invalid(format!(
                        "benchmark '{}' gym ({gym_name}) line {}: malformed EvalTask: {e}",
                        spec.name,
                        n + 1,
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;

        // Delegate to the ONE grader (cognition/eval) — never reimplement it here.
        let result = CognitionEval
            .run(
                ctx,
                CognitionEvalParams {
                    persona_id: p.persona_id,
                    gene: None,
                    room_id: None,
                    tasks: Some(sliced_tasks),
                    eval_set: None,
                    base_model_id: p.base_model_id.clone(),
                    reviewers: p.reviewers,
                    detach: p.detach,
                    run_id: None,
                    max_acts: p.max_acts.or(Some(6)),
                    max_retries: Some(0),
                    workspace_root: None,
                    capture_dir: None,
                    learn: None,
                    note: Some(match &p.base_model_id {
                        Some(m) => format!("benchmark/run {} on {m}", spec.name),
                        None => format!("benchmark/run {}", spec.name),
                    }),
                },
            )
            .await?;

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

// ---- benchmark/record + benchmark/matrix (the evidence engine, #123) -----------------------
//
// Every comparative claim we ever publish must decompose into ledger ROWS a stranger
// can re-run: one row = one (model × harness-arm × benchmark) result CARRYING its own
// replication command. The matrix is a PROJECTION of those rows — never hand-authored
// (the same recipe→artifact doctrine as the forge alloy). "Undeniable" = the row says
// exactly how to reproduce the cell; "viral" = anyone can render the table themselves.

/// The one benchmark results ledger (`~/.continuum/benchmarks/ledger.jsonl`).
fn benchmark_ledger_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    std::path::PathBuf::from(home)
        .join(".continuum")
        .join("benchmarks")
        .join("ledger.jsonl")
}

/// One comparative result — a cell contribution in the models × harness matrix.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../protocol/typescript/benchmark/BenchmarkRecordParams.ts")]
pub struct BenchmarkRecordParams {
    /// Model identity for the row — the served id (e.g. `unsloth/Devstral-Small-2507-GGUF`),
    /// with any gene noted in `gene`, never folded into this string.
    pub model: String,
    /// Harness arm: `raw` (bare /v1), `opencode`, `ours`, `ours+genome` — or a named
    /// competitor (`hermes`). Free string so new arms need no code change.
    pub harness: String,
    /// Benchmark name (a `benchmark/list` row, e.g. `swe-bench-lite`, `humaneval-rs`).
    pub benchmark: String,
    /// Tasks resolved / passed.
    #[ts(type = "number")]
    pub resolved: u32,
    /// Tasks attempted.
    #[ts(type = "number")]
    pub total: u32,
    /// The EXACT command that reproduces this row. Required — a result nobody can
    /// re-run is a claim, not evidence.
    pub replication: String,
    /// Hardware tier the run executed on (e.g. `macbook-m4-pro-64gb`, `rtx5090`).
    pub hardware: String,
    /// Genome layer identity when the arm ran with a gene paged in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub gene: Option<String>,
    /// Total generated tokens (degenerate-serving guard — see BenchmarkRunResult).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub output_tokens: Option<u32>,
    /// Wall-clock seconds for the run (feeds cost-per-resolved-task).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub wall_seconds: Option<u32>,
    /// Free-text context (instrument caveats, instance list, capture dir).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct BenchmarkRecordResult {
    /// Ledger rows now on file (this row included).
    #[ts(type = "number")]
    pub rows: u32,
    pub ledger: String,
}

/// `benchmark/record` — append one comparative result to the evidence ledger.
#[derive(Default)]
pub struct BenchmarkRecord;

#[async_trait]
impl ActionCommand for BenchmarkRecord {
    const NAME: &'static str = "benchmark/record";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Record one benchmark result (model × harness × benchmark, resolved/total, hardware, and \
         the EXACT replication command) into the evidence ledger. benchmark/matrix renders the \
         comparison table from these rows.";
    type Params = BenchmarkRecordParams;
    type Output = BenchmarkRecordResult;

    async fn run(&self, _ctx: &Ctx, p: BenchmarkRecordParams) -> Result<BenchmarkRecordResult, CommandError> {
        if p.total == 0 {
            return Err(CommandError::Invalid("total must be > 0 — an empty run is not a result".into()));
        }
        if p.resolved > p.total {
            return Err(CommandError::Invalid(format!(
                "resolved ({}) exceeds total ({})",
                p.resolved, p.total
            )));
        }
        if p.replication.trim().is_empty() {
            return Err(CommandError::Invalid(
                "replication is required — a result nobody can re-run is a claim, not evidence".into(),
            ));
        }
        let path = benchmark_ledger_path();
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)
                .map_err(|e| CommandError::Internal(format!("create {}: {e}", dir.display())))?;
        }
        let mut row = serde_json::to_value(&p)
            .map_err(|e| CommandError::Internal(format!("encode row: {e}")))?;
        row["atMs"] = serde_json::json!(chrono::Utc::now().timestamp_millis());
        let line = serde_json::to_string(&row)
            .map_err(|e| CommandError::Internal(format!("encode row: {e}")))?;
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
            .map_err(|e| CommandError::Internal(format!("open {}: {e}", path.display())))?;
        writeln!(f, "{line}").map_err(|e| CommandError::Internal(format!("append: {e}")))?;
        let rows = std::fs::read_to_string(&path)
            .map(|s| s.lines().filter(|l| !l.trim().is_empty()).count() as u32)
            .unwrap_or(0);
        Ok(BenchmarkRecordResult { rows, ledger: path.display().to_string() })
    }
}
crate::register_stateless_command!(BenchmarkRecord);

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkMatrixParams {
    /// Only render rows for this benchmark. Omit for all benchmarks (one table each).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub benchmark: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct BenchmarkMatrixResult {
    /// The rendered comparison — GitHub-flavored markdown, ready to paste anywhere.
    pub markdown: String,
    /// Ledger rows that fed the render.
    #[ts(type = "number")]
    pub rows: u32,
}

/// `benchmark/matrix` — render the models × harness comparison from the evidence ledger.
#[derive(Default)]
pub struct BenchmarkMatrix;

#[async_trait]
impl ActionCommand for BenchmarkMatrix {
    const NAME: &'static str = "benchmark/matrix";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Render the models × harness benchmark comparison table (markdown) from the evidence \
         ledger benchmark/record writes. Every cell aggregates its rows; the replication \
         appendix lists the exact command behind each row.";
    type Params = BenchmarkMatrixParams;
    type Output = BenchmarkMatrixResult;

    async fn run(&self, _ctx: &Ctx, p: BenchmarkMatrixParams) -> Result<BenchmarkMatrixResult, CommandError> {
        let path = benchmark_ledger_path();
        let raw = std::fs::read_to_string(&path).map_err(|_| {
            CommandError::NotFound(format!(
                "no evidence ledger at {} — record a result with benchmark/record first",
                path.display()
            ))
        })?;
        let rows: Vec<BenchmarkRecordParams> = raw
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str(l).ok())
            .filter(|r: &BenchmarkRecordParams| {
                p.benchmark.as_deref().is_none_or(|b| r.benchmark == b)
            })
            .collect();
        if rows.is_empty() {
            return Err(CommandError::NotFound(match &p.benchmark {
                Some(b) => format!("no ledger rows for benchmark {b:?}"),
                None => "the evidence ledger is empty".into(),
            }));
        }
        Ok(BenchmarkMatrixResult { markdown: render_matrix(&rows), rows: rows.len() as u32 })
    }
}
crate::register_stateless_command!(BenchmarkMatrix);

/// Pure render: ledger rows → one markdown table per benchmark (models down, harness
/// arms across, `resolved/total (rate)` cells aggregated over contributing rows) + a
/// replication appendix. Split from the command so the projection is unit-testable.
fn render_matrix(rows: &[BenchmarkRecordParams]) -> String {
    use std::collections::BTreeMap;
    // benchmark → arm set + (model[, gene]) → arm → (resolved, total)
    let mut by_bench: BTreeMap<&str, (Vec<&str>, BTreeMap<String, BTreeMap<&str, (u32, u32)>>)> =
        BTreeMap::new();
    for r in rows {
        let (arms, cells) = by_bench.entry(&r.benchmark).or_default();
        if !arms.contains(&r.harness.as_str()) {
            arms.push(&r.harness);
        }
        let model_key = match &r.gene {
            Some(g) => format!("{} + {}", r.model, g),
            None => r.model.clone(),
        };
        let cell = cells.entry(model_key).or_default().entry(&r.harness).or_insert((0, 0));
        cell.0 += r.resolved;
        cell.1 += r.total;
    }
    let mut md = String::new();
    for (bench, (arms, cells)) in &by_bench {
        md.push_str(&format!("## {bench}\n\n| model | {} |\n|---|{}\n", arms.join(" | "),
            "---|".repeat(arms.len())));
        for (model, per_arm) in cells {
            let row_cells: Vec<String> = arms
                .iter()
                .map(|a| match per_arm.get(a) {
                    Some((res, tot)) => {
                        format!("{res}/{tot} ({:.0}%)", (*res as f64 / *tot as f64) * 100.0)
                    }
                    None => "—".to_string(),
                })
                .collect();
            md.push_str(&format!("| {model} | {} |\n", row_cells.join(" | ")));
        }
        md.push('\n');
    }
    md.push_str("### Replication\n\n");
    for r in rows {
        md.push_str(&format!(
            "- **{} × {} × {}** on `{}`: `{}`{}\n",
            r.model,
            r.harness,
            r.benchmark,
            r.hardware,
            r.replication,
            r.notes.as_deref().map(|n| format!(" — {n}")).unwrap_or_default(),
        ));
    }
    md
}

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

    // what this catches: the evidence-engine projection (#123) — rows aggregate into
    // per-benchmark model × arm cells, a model with no result in an arm renders "—"
    // (never a fabricated 0%), gene rows are their own model identity (the same-weights
    // before/after-genome headline), and EVERY row's replication command survives into
    // the appendix. A matrix that drops or invents a cell publishes a false claim.
    #[test]
    fn matrix_renders_cells_and_replication_from_rows() {
        let row = |model: &str, harness: &str, gene: Option<&str>, res, tot| BenchmarkRecordParams {
            model: model.into(),
            harness: harness.into(),
            benchmark: "swe-bench-lite".into(),
            resolved: res,
            total: tot,
            replication: format!("cu benchmark/run --name swe-bench-lite --arm {harness}"),
            hardware: "macbook-m4-pro-64gb".into(),
            gene: gene.map(Into::into),
            output_tokens: None,
            wall_seconds: None,
            notes: None,
        };
        let rows = vec![
            row("devstral-24b", "ours", None, 0, 3),
            row("devstral-24b", "ours", None, 1, 3), // same cell aggregates: 1/6
            row("devstral-24b", "opencode", None, 1, 3),
            row("devstral-24b", "ours", Some("coder-act-transition"), 2, 3),
        ];
        let md = render_matrix(&rows);
        assert!(md.contains("## swe-bench-lite"), "{md}");
        assert!(md.contains("| devstral-24b | 1/6 (17%) | 1/3 (33%) |"), "{md}");
        assert!(
            md.contains("| devstral-24b + coder-act-transition | 2/3 (67%) | — |"),
            "gene row is its own identity, absent arm renders —: {md}"
        );
        assert_eq!(md.matches("cu benchmark/run").count(), 4, "all replication cmds survive: {md}");
    }
}
