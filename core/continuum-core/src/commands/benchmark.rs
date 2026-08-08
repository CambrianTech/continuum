//! `benchmark/*` — first-class, persona-callable benchmark competitions, managed like the
//! model catalog. A declarative catalog of known benchmark collections (add one = one row,
//! same as a `ModelSpec`) plus commands to LIST them and RUN one by name. `benchmark/run` is a
//! thin wrapper over `cognition/eval` — it resolves a benchmark and delegates, never
//! reimplementing the grader. This lives in Rust, on the DynCommand registry, so it is
//! ON-GRID: discoverable, persona-callable, and manageable by the daemons — unlike the
//! toolchain-free `benchmarks/coder/oneshot_opponent.py` script, whose ONLY job is letting an
//! OUTSIDER replicate our numbers against their own `/v1` without our stack. Operational
//! benchmarking is Rust; the replication convenience is the lone edge script.

use crate::cognition::learning_policy::LearningPolicy;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::cognition::competitor::{
    classify, optional_arms, run_competition, ArmClass, ArmTaskResult, DEFAULT_ENDPOINT,
};
use crate::cognition::eval::{CognitionEval, CognitionEvalParams};
use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

/// How a benchmark's solutions are scored.
#[derive(Debug, Clone, Copy, Serialize, TS)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/Grader.ts")]
pub enum Grader {
    /// Compile + run each solution (rustc). Live today via cognition/eval's `test_grade`.
    Rust,
    /// Execute each solution (python). Catalogued; grader lands with the python collections.
    Python,
    /// OBSERVE the rendered UI through the eye-node (`perception/observe`) and score its element
    /// tree against a `UiCheck` spec. Live today via cognition/eval's `perception_grade` — the
    /// functional web-dev tier (`webdev-rs`).
    Perception,
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
            name: "webdev-rs",
            description: "Functional web-dev — OUR UI benchmark, the tier public UI benchmarks lack \
                          (they grade an agent NAVIGATING a site, or a screenshot's pixel diff, not \
                          whether the agent BUILT a UI that structurally WORKS). Each task asks the \
                          persona to write a complete index.html; the grade OBSERVES what actually \
                          rendered through the eye-node (perception/observe) and scores the element \
                          tree (headings, inputs, buttons, lists) against a spec. 6 tasks. Proves \
                          image-perception + code-dev in one benchmark; equal footing for every model \
                          (the structure tree is text a non-visual model reads too).",
            grader: Grader::Perception,
            tasks: 6,
            eval_set: Some("webdev-rs.jsonl"),
            source_url: None,
        },
        BenchmarkSpec {
            name: "coder-write-eval",
            description: "Coder write gym — 30 single-function Rust tasks with held-out test \
                          assertions, rustc compile+run graded. The KANBAN benchmark: dispatched \
                          as work cards, solved by citizens with their own hands, graded from \
                          their workspace artifacts (first citizen pass: sum_evens, 2026-08-07).",
            grader: Grader::Rust,
            tasks: 30,
            eval_set: Some("coder-write-eval.jsonl"),
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
            name: "tool-bugfix-rs",
            description: "TOOL-USING bugfix gym — 3 seeded-bug Rust files the persona must \
                          read, edit, and recompile WITH HER TOOLS (dod_shell grades her edited \
                          file, not spoken code). The only benchmark that offers + requires the \
                          native tool surface — the honest instrument for tool-use (#204).",
            grader: Grader::Rust,
            tasks: 3,
            eval_set: Some("tool-bugfix-rs.jsonl"),
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkListParams.ts"
)]
pub struct BenchmarkListParams {}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRow.ts"
)]
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkListResult.ts"
)]
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

    async fn run(
        &self,
        _ctx: &Ctx,
        _p: BenchmarkListParams,
    ) -> Result<BenchmarkListResult, CommandError> {
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRunParams.ts"
)]
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRunResult.ts"
)]
pub struct BenchmarkRunResult {
    pub benchmark: String,
    /// The run handle. Present on a DETACHED run (the eval spawned; its real result
    /// lands in the progress ledger) — poll it with `cognition/eval-status --run_id`
    /// to get THIS run's finalized row, never a prior run's stale live-progress (the
    /// exact trap: a persona-only poll returns whatever ran last, so a fresh detached
    /// run reads as instantly "20/20" with the previous run's numbers). None on a
    /// synchronous run (the score is right here in this result).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
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
    /// Set ONLY when the serving lane failed mid-exam and never recovered (the Proctored
    /// Exam Session's void-flag). When present, `score`/`pass_rate` are VOID — this is NOT
    /// "she scored 0", it is "the harness never gave her a verified lane". Absent = a real,
    /// trustworthy number. [[proctored-exam-session-dependable-benchmark]]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub infra_unavailable: Option<crate::cognition::eval::InfraUnavailable>,
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

    async fn run(
        &self,
        ctx: &Ctx,
        p: BenchmarkRunParams,
    ) -> Result<BenchmarkRunResult, CommandError> {
        let limit = p.limit.unwrap_or(20) as usize;

        // Resolve `(label, tasks)` from EITHER the static catalog OR the BenchmarkAdapter
        // registry. The catalog wins for names it knows so resident benchmarks keep their
        // proven, DEPLOY-SAFE embedded-gym path (`resolve_gym` finds the gym even when the repo
        // isn't checked out). The adapter registry is the EXTENSION seam for benchmarks the
        // catalog can't express — downloadable / custom-graded ones (Terminal-Bench, SWE-bench)
        // land there as pure adapters with no change here. Inline tasks (not a temp-file path)
        // carry no CWD dependence and survive a DETACHED run that outlives this handler.
        let (bench_label, sliced_tasks): (String, Vec<crate::cognition::eval::EvalTask>) =
            if let Some(spec) = known_benchmarks().iter().find(|b| b.name == p.name) {
                let eval_set = spec.eval_set.ok_or_else(|| {
                    CommandError::Invalid(format!(
                        "benchmark '{}' is catalogued but not yet runnable through the grader \
                         (needs its dataset pulled + a {:?} grader). Runnable today: {}.",
                        spec.name,
                        spec.grader,
                        known_benchmarks()
                            .iter()
                            .filter(|b| b.eval_set.is_some())
                            .map(|b| b.name)
                            .collect::<Vec<_>>()
                            .join(", "),
                    ))
                })?;
                let (gym_name, content) =
                    crate::cognition::gym::resolve_gym(eval_set).map_err(CommandError::Invalid)?;
                let tasks = content
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
                (spec.name.to_string(), tasks)
            } else if let Some(adapter) = crate::cognition::benchmark::get(&p.name) {
                // A downloadable adapter needs its dataset fetched first; that on-demand fetch
                // is the NEXT slice. Fail LOUD rather than silently score an empty set — a
                // resident adapter (dataset() == None) runs today.
                if let Some(ds) = adapter.dataset() {
                    return Err(CommandError::Invalid(format!(
                        "benchmark '{}' needs its dataset fetched from '{}' ({:?}) — the \
                         on-demand download is not wired yet; resident adapters (no download) \
                         run today.",
                        p.name, ds.source, ds.kind,
                    )));
                }
                let tasks = adapter.tasks(None, Some(limit)).await.map_err(|e| {
                    CommandError::Invalid(format!(
                        "benchmark adapter '{}' failed to load tasks: {e}",
                        p.name
                    ))
                })?;
                (adapter.name().to_string(), tasks)
            } else {
                return Err(CommandError::NotFound(format!(
                    "unknown benchmark '{}'. Catalog: {}. Adapters: {}. Call benchmark/list.",
                    p.name,
                    known_benchmarks()
                        .iter()
                        .map(|b| b.name)
                        .collect::<Vec<_>>()
                        .join(", "),
                    crate::cognition::benchmark::names().join(", "),
                )));
            };

        // BUILD-FROM-SCRATCH → clean workspace (#206). A UI-build benchmark (ui_checks,
        // no seeded files) must run in an EMPTY dir: create-workspace re-roots her hands
        // there, the [workspace-map] follows (commit 780348b86), so it reads EMPTY and she
        // BUILDS the file rather than exploring the core repo she was grounded in (the
        // webdev-0 discovery-loop). One clean per-benchmark root (evals serialize; the
        // continuous exam accumulates in it, cleared each run). A persistent named path,
        // not a TempDir — it must survive a DETACHED run that outlives this handler.
        let is_from_scratch_build = !sliced_tasks.is_empty()
            && sliced_tasks.iter().all(|t| {
                !t.ui_checks.is_empty()
                    && t.setup_shell.is_none()
                    && t.dod_shell.is_none()
                    && t.solution_file.is_none()
            });
        let workspace_root = if is_from_scratch_build {
            let home = std::env::var("CONTINUUM_HOME")
                .map(std::path::PathBuf::from)
                .ok()
                .or_else(|| dirs::home_dir().map(|h| h.join(".continuum")))
                .ok_or_else(|| CommandError::Internal("no home dir for eval workspace".into()))?;
            let root = home.join("eval-workspaces").join(&bench_label);
            // Clean start each run — never grade against a prior run's leftover files.
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).map_err(|e| {
                CommandError::Internal(format!(
                    "could not create clean eval workspace at {}: {e}",
                    root.display()
                ))
            })?;
            Some(root.to_string_lossy().into_owned())
        } else {
            None
        };

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
                    workspace_root,
                    capture_dir: None,
                    // A measurement. Named, not defaulted — the type has no `Default`.
                    learn: LearningPolicy::DoNotLearn,
                    // #207: benchmark keeps recall by default (unchanged behavior); the
                    // reproducible-absolute knob is opt-in on cognition/eval directly.
                    suppress_recall: None,
                    note: Some(match &p.base_model_id {
                        Some(m) => format!("benchmark/run {bench_label} on {m}"),
                        None => format!("benchmark/run {bench_label}"),
                    }),
                },
            )
            .await?;

        Ok(BenchmarkRunResult {
            benchmark: bench_label,
            // Surface the eval's run handle so a detached run is pollable by run_id
            // (the finalized ledger row), not by persona-only live progress.
            run_id: result.run_id.clone(),
            score: result.score,
            total: result.total,
            pass_rate: result.pass_rate,
            output_tokens: result.total_output_tokens,
            mean_output_tokens_per_task: if result.total > 0 {
                result.total_output_tokens as f64 / result.total as f64
            } else {
                0.0
            },
            // Propagate the void-flag: a benchmark that ran on a dead lane returns
            // InfraUnavailable, never a fake pass-rate the matrix would publish as a real 0%.
            infra_unavailable: result.infra_unavailable.clone(),
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRecordParams.ts"
)]
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRecordResult.ts"
)]
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

    async fn run(
        &self,
        _ctx: &Ctx,
        p: BenchmarkRecordParams,
    ) -> Result<BenchmarkRecordResult, CommandError> {
        if p.total == 0 {
            return Err(CommandError::Invalid(
                "total must be > 0 — an empty run is not a result".into(),
            ));
        }
        if p.resolved > p.total {
            return Err(CommandError::Invalid(format!(
                "resolved ({}) exceeds total ({})",
                p.resolved, p.total
            )));
        }
        if p.replication.trim().is_empty() {
            return Err(CommandError::Invalid(
                "replication is required — a result nobody can re-run is a claim, not evidence"
                    .into(),
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
        Ok(BenchmarkRecordResult {
            rows,
            ledger: path.display().to_string(),
        })
    }
}
crate::register_stateless_command!(BenchmarkRecord);

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkMatrixParams.ts"
)]
pub struct BenchmarkMatrixParams {
    /// Only render rows for this benchmark. Omit for all benchmarks (one table each).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub benchmark: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkMatrixResult.ts"
)]
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

    async fn run(
        &self,
        _ctx: &Ctx,
        p: BenchmarkMatrixParams,
    ) -> Result<BenchmarkMatrixResult, CommandError> {
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
        Ok(BenchmarkMatrixResult {
            markdown: render_matrix(&rows),
            rows: rows.len() as u32,
        })
    }
}
crate::register_stateless_command!(BenchmarkMatrix);

// ── benchmark/competition — the product-vs-product scoreboard ────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkCompetitionParams.ts"
)]
pub struct BenchmarkCompetitionParams {
    /// Benchmark name (see `benchmark/list`). Its tasks are posed IDENTICALLY to every arm.
    pub name: String,
    /// The persona (UUID, must be spawned) whose LIVE cognition is the Continuum arm.
    pub persona_id: String,
    /// The shared weights EVERY arm runs — Continuum forks a measurement lane on it, and
    /// the external arms hit an endpoint serving it. Product vs product on ONE model.
    pub base_model_id: String,
    /// OpenAI-compatible endpoint the EXTERNAL arms target. Omit → the default local
    /// location; the operator/serving system provisions a dedicated opponent lane and
    /// passes its `base_url` ([[benchmark-needs-its-own-serving-lane]]). Hermes needs this
    /// lane to serve ≥64K or it refuses at startup and its cell reads VOID (fail loud).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub endpoint: Option<String>,
    /// Max tasks to run (default 20).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub limit: Option<u32>,
    /// Which external arms to run by name (e.g. `["hermes","raw-oneshot"]`). Omit → every
    /// available optional arm. An unknown name is an ERROR (fail loud, never a silent skip).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub arms: Option<Vec<String>>,
    /// Fire-and-poll (#86): a wide run (many tasks × the Continuum eval-lane + the Hermes
    /// agent loop) runs far past any IPC client timeout. `true` spawns it on the runtime,
    /// returns a `run_id` NOW, and writes the finished scoreboard to
    /// `~/.continuum/progress/competition-<run_id>.json` (+ a `benchmark:competition:complete`
    /// event) — the run survives the client disconnecting. Default `false` (inline) is fine
    /// only for a small `limit`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detach: Option<bool>,
    /// Correlation id for a detached run (echoed in the ack + the result file). Omit → minted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
}

/// One arm's cell on the competition scoreboard.
#[derive(Debug, Clone, Serialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/CompetitionCell.ts"
)]
pub struct CompetitionCell {
    pub arm: String,
    /// `agent` (a full being — Continuum, Hermes) or `floor` (bare weights, no self). A
    /// persona is NEVER ranked against a `floor` as a peer; the floor is a reference line
    /// ([[benchmark-must-never-score-persona-against-a-soul-stripped-copy]]).
    pub kind: String,
    #[ts(type = "number")]
    pub score: u32,
    #[ts(type = "number")]
    pub total: u32,
    /// `CLEAN` / `SUSPECT` / `VOID` — the trust triage. A SUSPECT/VOID cell is NOT a
    /// capability number; it is flagged so harness noise never publishes as a result.
    pub class: String,
    /// For an `agent` cell: its LIFT OVER FLOOR (`score − floor.score`) — the honest measure of
    /// what the agent's self+loop ADDS over the bare model. `None` for the floor itself, or when
    /// no floor arm ran. This, not "did she beat raw", is the number that matters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub lift_over_floor: Option<i32>,
    /// Extra context: the noisy-task count (SUSPECT) or the reason (VOID).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detail: Option<String>,
}

/// The scoreboard: every arm, same benchmark tasks, same grader, on one model.
#[derive(Debug, Clone, Serialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkCompetitionResult.ts"
)]
pub struct BenchmarkCompetitionResult {
    pub benchmark: String,
    pub model: String,
    pub endpoint: String,
    pub arms: Vec<CompetitionCell>,
    /// External arms skipped because their CLI/dep was absent — surfaced, never faked.
    pub skipped: Vec<String>,
    /// True when this is the immediate ACK of a detached run (arms empty — poll the result
    /// file `competition-<run_id>.json` for the real scoreboard).
    #[serde(default)]
    pub detached: bool,
    /// The run's correlation id (set on a detached ack + the written result file).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
}

/// `benchmark/competition` — run a benchmark through Continuum's native cognition AND
/// external agent harnesses on the SAME model, graded identically, each cell trust-classified.
/// The product-vs-product scoreboard ([[hermes-agent-is-a-runnable-benchmark-opponent-arm]]).
///
/// Pure orchestration: it never hand-spawns a llama-server (that is the serving system's
/// lifecycle, [[system-owns-its-lifecycle-never-hand-manage-processes]]). Continuum runs
/// through the ONE grader (`cognition/eval`); the external arms hit the given `endpoint`.
#[derive(Default)]
pub struct BenchmarkCompetition;

#[async_trait]
impl ActionCommand for BenchmarkCompetition {
    const NAME: &'static str = "benchmark/competition";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Agent-vs-agent coding scoreboard: run a benchmark's tasks through Continuum's native \
         cognition AND rival AGENTS (Hermes) on the SAME weights, graded by the SAME rustc \
         grader, each cell trust-classified CLEAN/SUSPECT/VOID. The raw one-shot is a FLOOR \
         reference (bare model, no self) — NEVER a peer a persona is ranked against; each agent \
         reports its LIFT OVER FLOOR (what its self+loop adds). External arms hit `endpoint` \
         (default local); provision a dedicated ≥64K opponent lane for the Hermes arm.";
    type Params = BenchmarkCompetitionParams;
    type Output = BenchmarkCompetitionResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: BenchmarkCompetitionParams,
    ) -> Result<BenchmarkCompetitionResult, CommandError> {
        // Fire-and-poll (#86): a wide run outlives any IPC client timeout, so `detach`
        // spawns the body on the runtime, writes the finished scoreboard to a result file
        // + emits a terminal event, and returns a run_id NOW.
        if p.detach.unwrap_or(false) {
            let run_id = p
                .run_id
                .clone()
                .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
            let run_id_ack = run_id.clone();
            let endpoint_ack = p
                .endpoint
                .clone()
                .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());
            let (name_ack, model_ack) = (p.name.clone(), p.base_model_id.clone());
            let mut inner = p;
            inner.detach = Some(false);
            inner.run_id = Some(run_id.clone());
            tokio::spawn(async move {
                let path = competition_ledger_path(&run_id);
                match BenchmarkCompetition::run_body(inner).await {
                    Ok(mut r) => {
                        r.run_id = Some(run_id.clone());
                        if let (Some(path), Ok(json)) =
                            (path.as_ref(), serde_json::to_string_pretty(&r))
                        {
                            let _ = std::fs::write(path, json);
                        }
                        if let Some(bus) = crate::runtime::MessageBus::global() {
                            if let Ok(v) = serde_json::to_value(&r) {
                                bus.publish_async_only("benchmark:competition:complete", v);
                            }
                        }
                        tracing::info!(run_id = %run_id, "benchmark/competition detached run complete");
                    }
                    Err(e) => {
                        // Fail LOUD on the poll surface too, not only the log — a detached run
                        // that dies must leave a diagnosable marker, never an empty file forever.
                        if let Some(path) = path {
                            let _ = std::fs::write(
                                &path,
                                serde_json::json!({"failed": true, "run_id": run_id, "error": e.to_string()})
                                    .to_string(),
                            );
                        }
                        tracing::error!(run_id = %run_id, error = %e, "benchmark/competition detached run failed");
                    }
                }
            });
            return Ok(BenchmarkCompetitionResult {
                benchmark: name_ack,
                model: model_ack,
                endpoint: endpoint_ack,
                arms: Vec::new(),
                skipped: Vec::new(),
                detached: true,
                run_id: Some(run_id_ack),
            });
        }
        Self::run_body(p).await
    }
}

/// Result file for a detached competition run, polled after the ack.
fn competition_ledger_path(run_id: &str) -> Option<std::path::PathBuf> {
    let base = std::env::var("CONTINUUM_HOME")
        .map(std::path::PathBuf::from)
        .ok()
        .or_else(|| dirs::home_dir().map(|h| h.join(".continuum")))?;
    let dir = base.join("progress");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join(format!("competition-{run_id}.json")))
}

impl BenchmarkCompetition {
    /// The competition body — deliberately ctx-free (CognitionEval ignores ctx; it reaches
    /// the persona via the global workspace registry), so it runs inline OR spawned detached.
    async fn run_body(
        p: BenchmarkCompetitionParams,
    ) -> Result<BenchmarkCompetitionResult, CommandError> {
        // 1) Resolve the benchmark and slice its tasks — posed identically to every arm.
        let spec = known_benchmarks()
            .iter()
            .find(|b| b.name == p.name)
            .ok_or_else(|| {
                CommandError::NotFound(format!(
                    "unknown benchmark '{}'. Known: {}. Call benchmark/list.",
                    p.name,
                    known_benchmarks()
                        .iter()
                        .map(|b| b.name)
                        .collect::<Vec<_>>()
                        .join(", "),
                ))
            })?;
        let eval_set = spec.eval_set.ok_or_else(|| {
            CommandError::Invalid(format!(
                "benchmark '{}' is catalogued but not yet runnable through the grader.",
                spec.name
            ))
        })?;
        let (gym_name, content) =
            crate::cognition::gym::resolve_gym(eval_set).map_err(CommandError::Invalid)?;
        let limit = p.limit.unwrap_or(20) as usize;
        let tasks: Vec<crate::cognition::eval::EvalTask> = content
            .lines()
            .filter(|l| !l.trim().is_empty())
            .take(limit)
            .enumerate()
            .map(|(n, l)| {
                serde_json::from_str(l).map_err(|e| {
                    CommandError::Invalid(format!(
                        "benchmark '{}' gym ({gym_name}) line {}: malformed EvalTask: {e}",
                        spec.name,
                        n + 1,
                    ))
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let total = tasks.len() as u32;
        let resolved_endpoint = p
            .endpoint
            .clone()
            .unwrap_or_else(|| DEFAULT_ENDPOINT.to_string());

        let mut cells: Vec<CompetitionCell> = Vec::new();

        // 2) The Continuum native arm — through the ONE grader (cognition/eval), converted
        //    onto the same scoreboard + SAME trust triage. Never reimplement cognition here.
        let cont_params = CognitionEvalParams {
            persona_id: p.persona_id.clone(),
            gene: None,
            room_id: None,
            tasks: Some(tasks.clone()),
            eval_set: None,
            base_model_id: Some(p.base_model_id.clone()),
            reviewers: None,
            max_acts: None,
            max_retries: None,
            note: Some(format!("competition:{}", spec.name)),
            detach: Some(false),
            run_id: None,
            workspace_root: None,
            capture_dir: None,
            learn: LearningPolicy::DoNotLearn,
            suppress_recall: None,
        };
        // CognitionEval ignores ctx (reaches cognition via the global registry), so a
        // default Ctx is correct here and keeps run_body ctx-free (spawnable when detached).
        let cont_cell = match CognitionEval.run(&Ctx::default(), cont_params).await {
            Ok(r) => {
                let class = if r.infra_unavailable.is_some() {
                    ArmClass::Void {
                        reason: "infra unavailable — measurement lane never verified".into(),
                    }
                } else {
                    let signals: Vec<ArmTaskResult> = r
                        .results
                        .iter()
                        .map(|t| ArmTaskResult {
                            task_id: t.id.clone(),
                            ok: t.ok,
                            output_tokens: t.output_tokens,
                            latency_ms: t.latency_ms,
                            grade: t.grade.clone(),
                            errored: false,
                        })
                        .collect();
                    classify(&signals)
                };
                cell("continuum", "agent", r.score, r.total, &class)
            }
            Err(e) => CompetitionCell {
                arm: "continuum".into(),
                kind: "agent".into(),
                score: 0,
                total,
                class: "VOID".into(),
                lift_over_floor: None,
                detail: Some(format!("eval error: {e}")),
            },
        };
        cells.push(cont_cell);

        // 3) The external arms — filtered by name if given, else every available optional arm.
        let mut external = optional_arms();
        if let Some(names) = &p.arms {
            for n in names {
                if !external.iter().any(|a| a.name() == n) {
                    return Err(CommandError::Invalid(format!(
                        "unknown arm '{n}'. Available: {}.",
                        external
                            .iter()
                            .map(|a| a.name())
                            .collect::<Vec<_>>()
                            .join(", "),
                    )));
                }
            }
            external.retain(|a| names.iter().any(|n| n == a.name()));
        }
        let board =
            run_competition(&p.base_model_id, p.endpoint.as_deref(), &tasks, external).await;
        for a in &board.arms {
            cells.push(cell(
                &a.arm,
                a.kind.label(),
                a.score as u32,
                a.total as u32,
                &a.class,
            ));
        }

        // Second pass: an AGENT's honest number is its LIFT OVER FLOOR — what its self+loop adds
        // over the bare model. A persona is never ranked against the floor as a peer; the floor is
        // a reference line ([[benchmark-must-never-score-persona-against-a-soul-stripped-copy]]).
        // Use the best (max-scoring) floor cell as the reference when one ran.
        let floor_score = cells
            .iter()
            .filter(|c| c.kind == "floor")
            .map(|c| c.score)
            .max();
        if let Some(floor) = floor_score {
            for c in cells.iter_mut().filter(|c| c.kind == "agent") {
                c.lift_over_floor = Some(c.score as i32 - floor as i32);
            }
        }

        Ok(BenchmarkCompetitionResult {
            benchmark: spec.name.to_string(),
            model: p.base_model_id,
            endpoint: resolved_endpoint,
            arms: cells,
            skipped: board.skipped,
            detached: false,
            run_id: None,
        })
    }
}

/// `ArmClass` + counts → a scoreboard cell (the trust triage projected for the wire).
/// `lift_over_floor` is filled in a second pass once the floor score is known.
fn cell(arm: &str, kind: &str, score: u32, total: u32, class: &ArmClass) -> CompetitionCell {
    let detail = match class {
        ArmClass::Clean => None,
        ArmClass::Suspect { noisy } => Some(format!(
            "{noisy} declined/errored task(s) — not a capability number"
        )),
        ArmClass::Void { reason } => Some(reason.clone()),
    };
    CompetitionCell {
        arm: arm.into(),
        kind: kind.into(),
        score,
        total,
        class: class.label().into(),
        lift_over_floor: None,
        detail,
    }
}

crate::register_stateless_command!(BenchmarkCompetition);

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
        let cell = cells
            .entry(model_key)
            .or_default()
            .entry(&r.harness)
            .or_insert((0, 0));
        cell.0 += r.resolved;
        cell.1 += r.total;
    }
    let mut md = String::new();
    for (bench, (arms, cells)) in &by_bench {
        md.push_str(&format!(
            "## {bench}\n\n| model | {} |\n|---|{}\n",
            arms.join(" | "),
            "---|".repeat(arms.len())
        ));
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
            r.notes
                .as_deref()
                .map(|n| format!(" — {n}"))
                .unwrap_or_default(),
        ));
    }
    md
}


// ───────────────────────── benchmark/dispatch ─────────────────────
//
// #346 (Joel, 2026-08-07): benchmarks delivered as WORK CARDS — measured for
// what citizens DO through the ordinary kanban loop, not what a harness drives
// them through. The empirical trigger: the live board's cards are substrate
// diagnosis narratives (cross-grid network debugging) a coding citizen cannot
// act on — Anwen held one, accurately reported "no progress", and paid 15,816
// prefill tokens to conclude it. Coders code when the board contains coding
// work sized to them; THIS is the verb that puts it there.

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkDispatchParams.ts"
)]
pub struct BenchmarkDispatchParams {
    /// The benchmark name (see `benchmark/list`), e.g. `tool-bugfix-rs`.
    pub name: String,
    /// How many tasks (from the top) to post as cards. Omit for all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub limit: Option<u32>,
    /// Board repo key the cards land under, e.g. `CambrianTech/continuum`.
    /// Omit to reuse the repo of the cards already on the board — data-driven,
    /// no baked-in default. Required only when the board is empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub repo: Option<String>,
    /// Citizen display names to direct the work at, round-robin. Each card gets
    /// an addressed kickoff message in-room naming the assignee + card id — the
    /// empirically proven activation path (an addressed imperative in its OWN
    /// message block actuates; a card sitting silently on the board does not).
    /// Omit for undirected dispatch (cards only, citizens self-select).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub assignees: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkDispatchResult.ts"
)]
pub struct BenchmarkDispatchResult {
    pub benchmark: String,
    /// Cards actually posted to the board.
    #[ts(type = "number")]
    pub dispatched: u32,
    /// Short ids of the posted cards, in task order.
    pub card_ids: Vec<String>,
    /// Tasks NOT dispatched because they need harness-side orchestration a
    /// claimed card cannot provide yet (setup_shell workspace restoration).
    /// Reported, never silently dropped — a partial dispatch that reads as
    /// full coverage is the lie this field exists to prevent.
    #[ts(type = "number")]
    pub skipped_needs_setup: u32,
    /// Addressed kickoff messages actually delivered (0 when `assignees` was
    /// omitted). A kickoff that failed to send is reported via `kickoff_errors`,
    /// never silently counted as delivered.
    #[ts(type = "number")]
    pub kickoffs: u32,
    /// Send failures for kickoff messages, card-id-prefixed. The cards are ON
    /// the board regardless — a failed kickoff degrades to undirected dispatch,
    /// it does not unwind the card.
    pub kickoff_errors: Vec<String>,
}

/// Compose the card TITLE for one benchmark task. `[bench <name>]` is the
/// machine-findable marker the (future) grading sentinel keys on; the rest is
/// for the citizen scanning the board.
pub(crate) fn dispatch_card_title(bench: &str, task_id: &str, prompt: &str) -> String {
    let gist: String = prompt.chars().take(60).collect();
    let ellipsis = if prompt.chars().count() > 60 { "…" } else { "" };
    format!("[bench {bench}] {task_id}: {gist}{ellipsis}")
}

/// Compose the card BODY: the full prompt plus a definition of done a citizen
/// can act on with her own hands. Grading inputs that must stay held out
/// (`expect`, the harness `test`) are deliberately NOT written to the card —
/// the DoD names the artifact, not the answer key. `solution_file` and
/// `dod_shell` are legitimately visible: real work has a visible definition
/// of done.
pub(crate) fn dispatch_card_body(bench: &str, t: &crate::cognition::eval::EvalTask) -> String {
    let mut body = format!("benchmark: {bench}\ntask: {}\n\n{}\n", t.id, t.prompt.trim());
    if let Some(f) = &t.solution_file {
        body.push_str(&format!(
            "\nWrite your solution to `{f}` in your workspace (code/write)."
        ));
    }
    if let Some(dod) = &t.dod_shell {
        body.push_str(&format!(
            "\nDefinition of done: `{dod}` exits 0 in your workspace — run it \
             yourself (code/shell) and iterate until green."
        ));
    }
    body.push_str(
        "\n\nWhen your artifact is in place, mark the card done (work/state). \
         Your work is graded from what you actually wrote and ran.",
    );
    body
}

/// `benchmark/dispatch` — post a benchmark's tasks as claimable work cards on
/// the shared board. Citizens claim and work them through the SAME kanban loop
/// as any other work; nothing about the turn is exam-shaped.
pub struct BenchmarkDispatch {
    pub registry: crate::persona::PersonaAircRuntimeRegistry,
}

#[async_trait]
impl ActionCommand for BenchmarkDispatch {
    const NAME: &'static str = "benchmark/dispatch";
    // Operator/curator surface, like work/create: seeding the board is
    // curation, not a citizen verb.
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Post a benchmark's tasks onto the work board as claimable cards (one card per task, \
         title marked `[bench <name>]`). Citizens claim and solve them through the normal \
         kanban loop with their own hands; scoring reads the artifacts they produce. \
         `limit` caps how many tasks are posted; `repo` defaults to the board's existing repo.";
    type Params = BenchmarkDispatchParams;
    type Output = BenchmarkDispatchResult;

    async fn run(
        &self,
        ctx: &Ctx,
        p: BenchmarkDispatchParams,
    ) -> Result<BenchmarkDispatchResult, CommandError> {
        use crate::cognition::eval::EvalTask;
        use crate::modules::work::persona_airc;
        use airc_lib::{CreateWorkCard, Priority, RepoId};

        let spec = known_benchmarks()
            .iter()
            .find(|b| b.name == p.name)
            .ok_or_else(|| {
                CommandError::Invalid(format!(
                    "unknown benchmark '{}' — see benchmark/list",
                    p.name
                ))
            })?;
        let reference = spec.eval_set.ok_or_else(|| {
            CommandError::Invalid(format!(
                "benchmark '{}' has no runnable eval_set yet — it is catalogued but its \
                 task collection hasn't been pulled/committed (see benchmark/list `runnable`)",
                p.name
            ))
        })?;

        // Same fail-loud task loading as cognition/eval: the committed gym
        // resolves from the embedded registry, a malformed line names itself.
        let (origin, text) =
            crate::cognition::gym::resolve_gym(reference).map_err(CommandError::Invalid)?;
        let tasks: Vec<EvalTask> = text
            .lines()
            .enumerate()
            .map(|(i, l)| (i + 1, l.trim()))
            .filter(|(_, l)| !l.is_empty())
            .map(|(n, l)| {
                serde_json::from_str::<EvalTask>(l).map_err(|e| {
                    CommandError::Invalid(format!("{origin} line {n}: malformed EvalTask: {e}"))
                })
            })
            .collect::<Result<_, _>>()?;

        let airc = persona_airc(&self.registry, ctx, "benchmark/dispatch")?;

        // Repo: caller-supplied, else the repo the board already uses. No
        // baked-in default — an empty board with no repo argument is a real
        // question only the operator can answer.
        let repo_key = match p.repo {
            Some(r) => r,
            None => {
                let board = airc
                    .work_board_complete(airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
                    .await
                    .map_err(|e| CommandError::Internal(format!("board read: {e}")))?
                    .snapshot();
                board
                    .cards
                    .first()
                    .map(|c| c.repo.as_str().to_string())
                    .ok_or_else(|| {
                        CommandError::Invalid(
                            "board is empty and no `repo` was given — pass repo=<owner/name> \
                             so the cards land under a real board key"
                                .to_string(),
                        )
                    })?
            }
        };
        let repo = RepoId::new(repo_key)
            .map_err(|e| CommandError::Invalid(format!("invalid repo: {e:?}")))?;

        let assignees = p.assignees.unwrap_or_default();
        if assignees.iter().any(|a| a.trim().is_empty()) {
            return Err(CommandError::Invalid(
                "assignees contains an empty name — every kickoff must address a real citizen"
                    .to_string(),
            ));
        }

        let take = p.limit.map(|l| l as usize).unwrap_or(tasks.len());
        let mut card_ids = Vec::new();
        let mut skipped_needs_setup = 0u32;
        let mut kickoffs = 0u32;
        let mut kickoff_errors = Vec::new();
        for t in tasks.into_iter().take(take) {
            // A setup_shell task needs its workspace re-broken before work
            // starts — harness orchestration a claimed card can't provide yet.
            // Skipping SILENTLY would report "dispatched" over fewer tasks
            // than the benchmark holds; the count rides on the result instead.
            if t.setup_shell.is_some() {
                skipped_needs_setup += 1;
                continue;
            }
            let mut req = CreateWorkCard::new(
                repo.clone(),
                dispatch_card_title(spec.name, &t.id, &t.prompt),
                Priority::P2,
            );
            req.body = Some(dispatch_card_body(spec.name, &t));
            let card_id = airc
                .create_work_card(req)
                .await
                .map_err(|e| CommandError::Internal(e.to_string()))?;
            let full = card_id.as_uuid().simple().to_string();
            let short = full[..8].to_string();

            // Directed dispatch: round-robin an addressed kickoff per card.
            // A card on the board is a fact; an addressed imperative in its
            // own message block is what actually starts work (measured
            // 2026-08-07: the same instruction actuated as its own block and
            // was ignored when coalesced mid-burst). airc.say is one event =
            // one block, so the structural condition holds by construction.
            if !assignees.is_empty() {
                let who = &assignees[card_ids.len() % assignees.len()];
                let file = t
                    .solution_file
                    .clone()
                    .unwrap_or_else(|| format!("{}.rs", t.id));
                let kickoff = format!(
                    "@{who} (to you): card {short} on this board is yours. Claim it \
                     (claim_task {short}), read its body, write your solution to `{file}` \
                     in your workspace, then mark it done (work/state {short} done). \
                     Your artifact gets graded against held-out tests."
                );
                match airc.say(&kickoff).await {
                    Ok(_) => kickoffs += 1,
                    // The card stays — a lost kickoff degrades to undirected
                    // dispatch and is REPORTED, never unwound or hidden.
                    Err(e) => kickoff_errors.push(format!("{short}: {e}")),
                }
            }
            card_ids.push(short);
        }

        Ok(BenchmarkDispatchResult {
            benchmark: spec.name.to_string(),
            dispatched: card_ids.len() as u32,
            card_ids,
            skipped_needs_setup,
            kickoffs,
            kickoff_errors,
        })
    }
}

// Descriptor only — the CONSTRUCTOR comes from WorkModule::commands(), which
// holds the airc registry this command needs. This is the dep-holding half of
// the descriptor/constructor pair (`register_stateless_command!` is for
// Default-constructible commands; this one is not).
crate::register_command!(BenchmarkDispatch);

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
        let hr = ks
            .iter()
            .find(|b| b.name == "humaneval-rs")
            .expect("humaneval-rs catalogued");
        assert!(hr.eval_set.is_some(), "humaneval-rs must be runnable");
        assert!(matches!(hr.grader, Grader::Rust));
        // Every runnable benchmark's eval_set must resolve through the gym resolver.
        for b in ks.iter().filter(|b| b.eval_set.is_some()) {
            crate::cognition::gym::resolve_gym(b.eval_set.unwrap()).unwrap_or_else(|e| {
                panic!("benchmark '{}' eval_set does not resolve: {e}", b.name)
            });
        }
    }

    // what this catches: the evidence-engine projection (#123) — rows aggregate into
    // per-benchmark model × arm cells, a model with no result in an arm renders "—"
    // (never a fabricated 0%), gene rows are their own model identity (the same-weights
    // before/after-genome headline), and EVERY row's replication command survives into
    // the appendix. A matrix that drops or invents a cell publishes a false claim.
    #[test]
    fn matrix_renders_cells_and_replication_from_rows() {
        let row =
            |model: &str, harness: &str, gene: Option<&str>, res, tot| BenchmarkRecordParams {
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
        assert!(
            md.contains("| devstral-24b | 1/6 (17%) | 1/3 (33%) |"),
            "{md}"
        );
        assert!(
            md.contains("| devstral-24b + coder-act-transition | 2/3 (67%) | — |"),
            "gene row is its own identity, absent arm renders —: {md}"
        );
        assert_eq!(
            md.matches("cu benchmark/run").count(),
            4,
            "all replication cmds survive: {md}"
        );
    }
}

// ---------------------------------------------------------------------------------------
// SWE-bench: the repo-test grader on the command surface.
//
// `swe-bench-lite` and `swe-bench-verified` have been catalog rows since this file was
// written, with `Grader::Python` documented as "catalogued; grader lands with the python
// collections". This is that grader — and it lands in Rust, because a benchmark is substrate
// ([[benchmark-infra-is-substrate-commands-handles-events-never-bash]]). The Python scripts
// it replaces produced numbers nobody could trust: eight runs scored against a clone left at
// HEAD, gold mis-scored on three instances by an id-shape assumption, and a poll loop that
// could not tell a dead dispatch from a working one.
//
// The instance's own pytest suite still runs — that is the SUBJECT under test, exactly as
// `rustc` is for `humaneval-rs`. We write no Python.
// ---------------------------------------------------------------------------------------

use crate::cognition::swe_bench::{self, SweVerdict};

/// Inputs to `benchmark/swe-grade`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/SweGradeParams.ts"
)]
pub struct SweGradeParams {
    /// The instance to grade, e.g. `sympy__sympy-22005`.
    pub instance: String,
    /// Dataset to resolve the instance from. Defaults to SWE-bench Lite.
    #[serde(default)]
    #[ts(optional)]
    pub dataset: Option<String>,
    /// THE SPINE CHECK: grade the dataset's own gold patch. It MUST resolve — if it does not,
    /// the environment is wrong and no other number from it means anything. Run this before
    /// trusting any solver's score on an instance you have not graded before.
    #[serde(default)]
    #[ts(optional)]
    pub gold: Option<bool>,
    /// A candidate patch to grade (unified diff). Ignored when `gold` is set.
    #[serde(default)]
    #[ts(optional)]
    pub patch: Option<String>,
    /// A working tree a solver has already edited — its `git diff` becomes the candidate
    /// patch. Grading still happens in a FRESH clone at `base_commit`, so a solver that dirtied
    /// its workspace cannot launder that into a passing score.
    #[serde(default)]
    #[ts(optional)]
    pub workspace: Option<String>,
}

/// Result of `benchmark/swe-grade` — the full verdict, never a bare boolean.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/SweGradeResult.ts"
)]
pub struct SweGradeResult {
    pub instance: String,
    /// True only when every FAIL_TO_PASS and every sampled PASS_TO_PASS passed.
    pub resolved: bool,
    pub fail_to_pass_passed: u32,
    pub fail_to_pass_total: u32,
    pub pass_to_pass_passed: u32,
    pub pass_to_pass_total: u32,
    /// False when FAIL_TO_PASS already passed on the pristine tree — the checkout carries no
    /// bug, so `resolved: false` from such a run means NOTHING about the solver.
    pub gate_ok: bool,
    /// Set when no verdict could be produced (clone/patch/environment failure). A result with
    /// `error` is an ABSENCE, not a zero, and must never be tallied as a failed attempt.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
    /// How many bytes of candidate patch were graded — 0 means the solver changed nothing,
    /// which is a harness signal, not a model score.
    pub patch_bytes: u32,
    /// The NAMES of the failing tests — the actionable half of the verdict. A count
    /// teaches nothing; a named test is what a human reviewer (or the citizen herself,
    /// next attempt) can actually chase.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failed_tests: Vec<String>,
}

impl From<(SweVerdict, usize)> for SweGradeResult {
    fn from((v, patch_bytes): (SweVerdict, usize)) -> Self {
        SweGradeResult {
            instance: v.instance_id,
            resolved: v.resolved,
            fail_to_pass_passed: v.f2p_passed as u32,
            fail_to_pass_total: v.f2p_total as u32,
            pass_to_pass_passed: v.p2p_passed as u32,
            pass_to_pass_total: v.p2p_total as u32,
            gate_ok: v.gate_ok,
            error: v.error,
            patch_bytes: patch_bytes as u32,
            failed_tests: v.failed_tests,
        }
    }
}

/// Grade one SWE-bench instance by the official protocol.
#[derive(Default)]
pub struct BenchmarkSweGrade;

// Self-registered at its own declaration site — the registry has no central list.
crate::register_stateless_command!(BenchmarkSweGrade);

#[async_trait]
impl ActionCommand for BenchmarkSweGrade {
    const NAME: &'static str = "benchmark/swe-grade";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Grade one SWE-bench instance by the official protocol: clone at base_commit, apply the \
         candidate patch, apply the instance's test_patch, run its tests. RESOLVED only when every \
         FAIL_TO_PASS and every sampled PASS_TO_PASS passes. Refuses to report on a tree it cannot \
         vouch for — FAIL_TO_PASS must FAIL on the pristine checkout (that failure IS the bug), and \
         `gateOk: false` marks a run whose score is void. Pass `gold: true` first on any instance \
         not graded here before: the dataset's own patch must resolve, or the environment is wrong \
         and no other number from it means anything.";
    type Params = SweGradeParams;
    type Output = SweGradeResult;

    async fn run(&self, _ctx: &Ctx, p: SweGradeParams) -> Result<SweGradeResult, CommandError> {
        grade_swe(p).await
    }
}

/// The `benchmark/swe-grade` body, callable without a command context — the
/// hands-free autograde on `agent/solve` completion invokes the SAME grader
/// (fresh clone at base_commit, held-out tests, experience-stream write) as
/// the operator verb. One grader, never two.
pub(crate) async fn grade_swe(p: SweGradeParams) -> Result<SweGradeResult, CommandError> {
        let dataset = p
            .dataset
            .clone()
            .unwrap_or_else(|| "princeton-nlp/SWE-bench_Lite".to_string());
        let rows = swe_bench::load_dataset(&dataset)
            .await
            .map_err(CommandError::Internal)?;
        let instance = rows
            .into_iter()
            .find(|r| r.instance_id == p.instance)
            .ok_or_else(|| {
                CommandError::NotFound(format!("{} not found in {dataset}", p.instance))
            })?;

        // Resolve the candidate patch. A workspace's diff is READ here but graded in a fresh
        // clone below — where the solver worked is never where the score is taken.
        let candidate: Option<String> = if p.gold.unwrap_or(false) {
            Some(instance.patch.clone())
        } else if let Some(ws) = p.workspace.as_ref() {
            // `diff HEAD` (not bare `diff`) so STAGED edits count as her work too, and
            // `:(exclude).airc` because the substrate stages its own coordination files
            // into her workspace (card b34f7eb5): Atlas's first grade carried 91KB of
            // staged `.airc` blobs, and the fresh clone refused the WHOLE candidate —
            // a real fix voided by files no solver wrote.
            let out = std::process::Command::new("git")
                .args(["diff", "HEAD", "--", ".", ":(exclude).airc"])
                .current_dir(ws)
                .output()
                .map_err(|e| CommandError::Internal(format!("could not read {ws}'s diff: {e}")))?;
            Some(String::from_utf8_lossy(&out.stdout).to_string())
        } else {
            p.patch.clone()
        };
        let patch_bytes = candidate.as_ref().map(|c| c.len()).unwrap_or(0);

        let work = swe_bench::swe_cache_dir()
            .join("work")
            .join(&instance.instance_id);
        let repo = work.join("repo");
        let _ = std::fs::create_dir_all(&work);
        if let Err(e) = swe_bench::clone_at(&instance, &repo).await {
            return Ok(SweGradeResult::from((
                SweVerdict {
                    instance_id: instance.instance_id,
                    error: Some(e),
                    ..Default::default()
                },
                patch_bytes,
            )));
        }
        let verdict = swe_bench::grade(&instance, &repo, candidate.as_deref()).await;

        // #319: a WORKSPACE grade is a citizen's lived, objectively judged work —
        // append it to her experience stream. Only her: the gold/raw-patch arms are
        // harness plumbing, not experience. And only a REAL verdict: an errored run
        // is an ABSENCE (harness fault), and teaching from a harness failure would
        // corrupt the reward signal (`an_errored_verdict_is_an_absence_not_a_zero`).
        if verdict.error.is_none() {
            if let Some(peer_dir) = p
                .workspace
                .as_ref()
                .and_then(|ws| citizen_peer_dir_of(std::path::Path::new(ws)))
            {
                let task = crate::cognition::eval::EvalTask {
                    id: instance.instance_id.clone(),
                    prompt: instance.problem_statement.clone(),
                    ..Default::default()
                };
                // Name the failures — a count is a score, a name is a lesson (Joel,
                // 2026-08-08). "PASS_TO_PASS 6/11" told Atlas nothing; "your change
                // broke test_arguments" is what a human reviewer would have said.
                let broke = if verdict.failed_tests.is_empty() {
                    String::new()
                } else {
                    format!(" — failing: {}", verdict.failed_tests.join(", "))
                };
                let detail = format!(
                    "swe-bench {}: resolved={} FAIL_TO_PASS {}/{} PASS_TO_PASS {}/{}{}",
                    instance.instance_id,
                    verdict.resolved,
                    verdict.f2p_passed,
                    verdict.f2p_total,
                    verdict.p2p_passed,
                    verdict.p2p_total,
                    broke
                );
                let episode = crate::cognition::experience::ExperienceRecord::from_kanban_grade(
                    &task,
                    candidate.as_deref().unwrap_or(""),
                    verdict.resolved,
                    &detail,
                );
                if let Err(e) =
                    crate::cognition::experience::append_experience(&peer_dir, &episode)
                {
                    tracing::warn!(
                        workspace = ?p.workspace,
                        error = %e,
                        "swe-grade outcome could not be appended to the experience \
                         stream — the verdict stands, but this lesson was LOST"
                    );
                }
            }
        }

        Ok(SweGradeResult::from((verdict, patch_bytes)))
    }

/// The citizen peer dir owning a workspace path: the `<...>/citizens/peers/<uuid>`
/// prefix of `path`, or `None` when the path is not inside a citizen's home (an
/// operator scratch tree, the gold arm's cache clone). Path shape is the SAME one
/// `resolve_solver_dir` resolves into — this is its inverse, not a second layout.
fn citizen_peer_dir_of(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let comps: Vec<&std::ffi::OsStr> = path.iter().collect();
    let peers_at = comps
        .windows(2)
        .position(|w| w[0] == "citizens" && w[1] == "peers")?;
    // citizens/peers/<uuid> — need the uuid component after the pair.
    let uuid_at = peers_at + 2;
    if uuid_at >= comps.len() {
        return None;
    }
    Some(comps[..=uuid_at].iter().collect())
}

#[cfg(test)]
mod swe_grade_tests {
    use super::*;

    // what this catches: name/access wiring — grading is a read on the AiSafe surface, so a
    // persona can score her own work without an operator in the loop.
    #[test]
    fn swe_grade_name_and_access_wired() {
        assert_eq!(BenchmarkSweGrade::NAME, "benchmark/swe-grade");
        assert!(matches!(BenchmarkSweGrade::ACCESS, AccessLevel::AiSafe));
    }

    // what this catches: a verdict that could not run must NOT read as a scored zero. Tallying
    // an environment failure as "the model failed" is how a broken harness becomes a number.
    #[test]
    fn an_errored_verdict_is_an_absence_not_a_zero() {
        let v = SweVerdict {
            instance_id: "x__y-1".into(),
            error: Some("could not install repo".into()),
            ..Default::default()
        };
        let r = SweGradeResult::from((v, 0));
        assert!(!r.resolved);
        assert!(r.error.is_some(), "the reason must survive to the caller");
        assert_eq!(
            r.fail_to_pass_total, 0,
            "no tests ran, so nothing was attempted"
        );
    }

    // what this catches: the experience-stream producer must attribute a graded
    // workspace to the RIGHT citizen and stay silent for non-citizen paths — a
    // wrong peer dir would file her lesson into someone else's mind (#319).
    #[test]
    fn citizen_peer_dir_resolves_from_workspace_path_or_not_at_all() {
        let p = std::path::Path::new(
            "/Users/x/.continuum/citizens/peers/fe4dac17-aaaa-4bbb-8ccc-000000000001/workspace/swe/pallets__flask-4992",
        );
        let d = citizen_peer_dir_of(p).expect("workspace path resolves");
        assert!(d.ends_with("citizens/peers/fe4dac17-aaaa-4bbb-8ccc-000000000001"));
        assert!(
            citizen_peer_dir_of(std::path::Path::new("/tmp/swe-work/repo")).is_none(),
            "an operator scratch tree is nobody's experience"
        );
        assert!(
            citizen_peer_dir_of(std::path::Path::new("/x/citizens/peers")).is_none(),
            "the pair with no uuid after it must not resolve"
        );
    }

    // what this catches: the gate is reported, not just enforced. A caller tallying results
    // must be able to EXCLUDE ungradeable instances rather than counting them as failures.
    #[test]
    fn the_gate_verdict_reaches_the_caller() {
        let v = SweVerdict {
            instance_id: "x__y-1".into(),
            gate_ok: false,
            error: Some("UNGRADEABLE — FAIL_TO_PASS already passes".into()),
            ..Default::default()
        };
        let r = SweGradeResult::from((v, 512));
        assert!(!r.gate_ok);
        assert_eq!(
            r.patch_bytes, 512,
            "patch size is reported even when the tree is void"
        );
    }
}

/// Inputs to `benchmark/swe-solve` — the whole loop in one command.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/SweSolveParams.ts"
)]
pub struct SweSolveParams {
    /// The instance to solve, e.g. `sympy__sympy-22005`.
    pub instance: String,
    /// Dataset to resolve it from. Defaults to SWE-bench Lite.
    #[serde(default)]
    #[ts(optional)]
    pub dataset: Option<String>,
    /// The persona whose WHOLE self works the issue — never a stripped copy.
    pub persona_id: String,
    /// The model to measure her on.
    pub base_model_id: String,
    /// Max act→observe cycles.
    #[serde(default)]
    #[ts(optional)]
    pub max_acts: Option<u32>,
    /// Fire-and-poll: a real agentic drive outlives the IPC socket, so a sweep MUST detach.
    /// Returns a run id now; the verdict lands in `~/.continuum/progress/swe-solve-<run>.json`.
    #[serde(default)]
    #[ts(optional)]
    pub detach: Option<bool>,
    /// Correlation id for a detached run. Omit → minted.
    #[serde(default)]
    #[ts(optional)]
    pub run_id: Option<String>,
}

/// Result of `benchmark/swe-solve` — what she did AND what it scored, in one object.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/SweSolveResult.ts"
)]
pub struct SweSolveResult {
    pub instance: String,
    /// Acts she actually spent. 0 with a non-empty patch is impossible; 0 with an empty patch
    /// is a HARNESS signal (nothing ran), not a model score.
    pub acts: u32,
    pub files_changed: Vec<String>,
    /// Absent on the immediate ack of a detached run — poll the ledger for the real verdict.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub grade: Option<SweGradeResult>,
    pub detached: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
    /// INFRASTRUCTURE FAILURE — this row is NOT a score. Set when her drive stopped
    /// because the deliberation model call failed (lane torn down mid-drive, serving
    /// refusing a model it isn't hosting, timeout), not because she finished or ran out
    /// of acts. Measured 2026-08-04, sympy-21379: the lane went `serving: <none>,
    /// ready: false` at act 7 of 30 and the verdict reported a clean-looking
    /// `resolved: false, patchBytes: 0` — indistinguishable from a real capability zero.
    /// Any aggregate MUST exclude rows carrying this
    /// ([[a-benchmark-zero-is-a-claim-about-the-harness-until-proven-otherwise]]).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub infra_error: Option<String>,
}

/// The task text handed to her. Deliberately says WHERE she is and what "done" means — the
/// glass-boxed failure it prevents is her creating a new project beside the repo, or leaving
/// the fix in a message instead of the files.
fn swe_task_prompt(problem_statement: &str) -> String {
    format!(
        "You are ALREADY in the task's workspace: a real git repository with a real bug. Do not \
         create a new workspace and do not add new top-level files — find the existing source \
         with code/search and code/read, and fix it IN PLACE with code/edit. Run checks with \
         code/shell if useful. The fix must land in the existing files.\n\nISSUE:\n{problem_statement}"
    )
}

fn swe_solve_ledger_path(run_id: &str) -> std::path::PathBuf {
    swe_bench::solve_ledger_dir().join(format!("swe-solve-{run_id}.json"))
}

/// Solve one instance and grade it — the whole protocol, one command.
#[derive(Default)]
pub struct BenchmarkSweSolve;

crate::register_stateless_command!(BenchmarkSweSolve);

impl BenchmarkSweSolve {
    async fn body(p: SweSolveParams) -> Result<SweSolveResult, CommandError> {
        let dataset = p
            .dataset
            .clone()
            .unwrap_or_else(|| "princeton-nlp/SWE-bench_Lite".to_string());
        let rows = swe_bench::load_dataset(&dataset)
            .await
            .map_err(CommandError::Internal)?;
        let instance = rows
            .into_iter()
            .find(|r| r.instance_id == p.instance)
            .ok_or_else(|| {
                CommandError::NotFound(format!("{} not found in {dataset}", p.instance))
            })?;

        // She works in her OWN clone; grading later happens in a second, pristine one.
        let solve_repo = swe_bench::swe_cache_dir()
            .join("solve")
            .join(&instance.instance_id)
            .join("repo");
        swe_bench::clone_at(&instance, &solve_repo)
            .await
            .map_err(CommandError::Internal)?;

        // GIVE HER THE INTERPRETER. The harness already builds an era-matched venv per
        // instance for GRADING; her hands never saw it, so `python` did not exist for her.
        // Measured on sympy-21379: she wrote a correct reproduction script, ran it, and got
        // `bash: python: command not found` (exit 127) — then settled. A persona who cannot
        // EXECUTE cannot verify a fix, so every Python instance was scoring her on a loop she
        // was physically unable to close.
        //
        // Non-fatal on failure: a missing venv makes her slower, not wrong, and refusing the
        // whole run over it would trade a measurable result for none. The probe says which.
        let venv_bin: Vec<String> = match swe_bench::ensure_env(&instance, &solve_repo).await {
            Ok(venv_py) => venv_py
                .parent()
                .map(|bin| vec![bin.to_string_lossy().to_string()])
                .unwrap_or_default(),
            Err(e) => {
                crate::probe!(
                    class = "benchmark.swe.no_interpreter",
                    instance = instance.instance_id.as_str(),
                    error = e.as_str(),
                    "could not provision the era-matched venv — she will work without a runnable `python`"
                );
                Vec::new()
            }
        };

        // Compose agent/solve IN PROCESS — no subprocess, no CLI, no polling a file we wrote
        // ourselves. This is the composition the command surface exists for.
        let solved = crate::commands::agent::solve::AgentSolve::solve_body(
            crate::commands::agent::solve::AgentSolveParams {
                persona_id: p.persona_id.clone(),
                base_model_id: p.base_model_id.clone(),
                task: swe_task_prompt(&instance.problem_statement),
                workspace: solve_repo.to_string_lossy().to_string(),
                max_acts: p.max_acts.or(Some(30)),
                detach: Some(false),
                run_id: None,
                // This inline arm IS the grader loop's inner body (swe-solve grades right
                // below) — retries here would nest N chances inside N chances. The
                // claim-dispatch adapter owns its own N (`SWE_CLAIM_ATTEMPTS`).
                attempts: None,
                // learn=FALSE on a SCORED, HELD-OUT instance (#312). `agent/solve` defaults
                // learn ON and that default is right — a being learns from her work. But this
                // command's input is a held-out benchmark dataset, and what learn mode admits
                // is a durable engram in the LIVING persona.
                //
                // Measured 2026-08-04: six flask-4045 runs put six verbatim GitHub issues into
                // Anwen's episodic store, and her consolidator did its job — it crystallized
                // SEMANTIC beliefs out of the repetition ("If a Flask Blueprint name contains a
                // dot, raise a ValueError…"). She durably knew the answer to a held-out
                // instance, stored indistinguishably from anything she reasoned out herself, and
                // a re-run would have scored memorization while reading as capability.
                //
                // This is NOT in tension with the capture-always argument below: the TRACE is
                // corpus (curated, offline, deliberately trained on), and a belief admitted
                // mid-measurement is not. Corpus stays; the immediate write-back goes.
                learn: LearningPolicy::DoNotLearn,
                // CAPTURE ALWAYS, per run. This started as `None` — "a sweep writes none by
                // default" — which was exactly backwards on two counts.
                //
                // Glass box: a benchmark run is the run you most need to SEE. Without a trace
                // the only output is a boolean, so a failure teaches nothing about whether she
                // aimed wrong, looped, or ran out of acts — and the live prompt-capture file
                // rotates at ~48 KB, so a 30-act drive overruns its own history.
                //
                // Curriculum: benchmarks are becoming training corpus, and the corpus is the
                // TRACE, not the verdict. `resolved: true/false` trains nothing; the episode
                // does ([[benchmarks-are-becoming-the-training-corpus-and-a-teacher-can-extend-them]]).
                // Discarding it threw away the run's whole value beyond one bit.
                //
                // Per-run directory so a sweep's episodes never overwrite each other.
                capture_dir: Some(
                    swe_bench::swe_cache_dir()
                        .join("captures")
                        .join(&instance.instance_id)
                        .to_string_lossy()
                        .to_string(),
                ),
                // Recall STAYS ON — she is measured as her whole self, never a stripped copy
                // ([[benchmark-must-never-score-persona-against-a-soul-stripped-copy]]).
                suppress_recall: None,
                // SWE grades the DIFF: `grade_instance` below applies her patch to a fresh
                // clone and runs FAIL_TO_PASS. Nothing she says is ever read. Declaring that
                // contract is what stops a zero-change explanation from ending the run with
                // the act budget unspent (glass-boxed on sympy-21379: one `code/tree`, then a
                // prose analysis, 0 patch bytes).
                deliverable: Some(crate::commands::agent::solve::Deliverable::Workspace),
                // SCORED: the verdict below applies her diff to a fresh clone and runs
                // FAIL_TO_PASS. An edit that lands inside a string literal produces a file that
                // parses, tests that run, and a guard that never executes — measured three times
                // on flask-4045, the last of which passed all 51 PASS_TO_PASS with both
                // FAIL_TO_PASS still failing. She cannot recover from that mid-run, so on this
                // path the write is refused and the act→observe circuit gets to retry (#317).
                scored: Some(true),
                // Her shell gets the SAME interpreter the grader uses — so `python
                // reproduce.py` and `python -m pytest` actually run.
                path_prepend: (!venv_bin.is_empty()).then(|| venv_bin.clone()),
            },
        )
        .await?;

        // Grade the patch she produced, in a FRESH clone at base_commit. Her workspace is
        // never the scoring tree — that separation is what makes the gate meaningful.
        let grade_repo = swe_bench::swe_cache_dir()
            .join("work")
            .join(&instance.instance_id)
            .join("repo");
        let patch = solved.patch.clone();
        let grade = match swe_bench::clone_at(&instance, &grade_repo).await {
            Ok(()) => {
                let v = swe_bench::grade(
                    &instance,
                    &grade_repo,
                    if patch.trim().is_empty() {
                        None
                    } else {
                        Some(patch.as_str())
                    },
                )
                .await;
                SweGradeResult::from((v, patch.len()))
            }
            Err(e) => SweGradeResult::from((
                SweVerdict {
                    instance_id: instance.instance_id.clone(),
                    error: Some(e),
                    ..Default::default()
                },
                patch.len(),
            )),
        };

        Ok(SweSolveResult {
            instance: instance.instance_id,
            acts: solved.acts,
            files_changed: solved.files_changed,
            grade: Some(grade),
            detached: false,
            run_id: None,
            infra_error: solved.infra_error,
        })
    }
}

#[async_trait]
impl ActionCommand for BenchmarkSweSolve {
    const NAME: &'static str = "benchmark/swe-solve";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Solve ONE SWE-bench instance end to end and score it: clone at base_commit, drop the \
         persona's whole self into that repo with the issue, then grade the patch she produced by \
         the official protocol in a FRESH clone. Her working tree is never the scoring tree. Use \
         `detach: true` for a sweep — a real agentic drive outlives the socket, and the verdict \
         lands in ~/.continuum/progress/swe-solve-<runId>.json.";
    type Params = SweSolveParams;
    type Output = SweSolveResult;

    async fn run(&self, _ctx: &Ctx, p: SweSolveParams) -> Result<SweSolveResult, CommandError> {
        // ONE SOLVE PER PERSONA AT A TIME — refuse, loudly, rather than produce a
        // silently invalid number.
        //
        // `agent/solve` roots her hands by driving `code/create-workspace` through her
        // OWN executor, and that command KEYS ON THE CALLER (persona identity), not on
        // the cycle. So a second concurrent solve for the same persona re-roots the
        // FIRST one's hands too: last writer wins, and both drives then edit one repo.
        //
        // Measured 2026-08-04, two concurrent detached solves on persona fe4dac17:
        //   sympy-22005 (polysys task)  → its own workspace diff: EMPTY after 26 acts
        //   sympy-21055 (refine task)   → contains its refine work AND 22005's polysys
        //                                 edits, one of which deleted a binding that is
        //                                 still referenced (a guaranteed NameError)
        // Both instances score a garbage number, and nothing anywhere says why. That is
        // exactly the failure shape [[a-benchmark-zero-is-a-claim-about-the-harness-until-proven-otherwise]]
        // warns about: a plausible-looking 0 that measures the harness, not the being.
        //
        // Refusing is correct until the root becomes per-cycle state instead of
        // per-persona executor state (the real fix; this guard is the honest floor, and
        // it names the conflicting run so the caller can wait or pick another persona).
        let in_flight = crate::cognition::swe_bench::in_flight_solve_runs();
        if let Some((run, instance)) = in_flight.first() {
            return Err(CommandError::Invalid(format!(
                "refusing to start `{}`: solve run `{run}` is already in flight on instance \
                 `{instance}`. Concurrent solves SHARE one workspace root (create-workspace keys \
                 on the persona, not the run), so both trees would be corrupted and both scores \
                 would be meaningless. Wait for it to land (poll \
                 ~/.continuum/progress/swe-solve-{run}.json), or run the sweep sequentially.",
                p.instance
            )));
        }
        if p.detach.unwrap_or(false) {
            let run_id = p
                .run_id
                .clone()
                .unwrap_or_else(|| format!("{}-{}", p.instance, std::process::id()));
            let instance_ack = p.instance.clone();
            let run_ack = run_id.clone();
            let mut inner = p;
            inner.detach = Some(false);
            // MARK IT RUNNING BEFORE SPAWNING. Until this line existed, a detached run wrote
            // nothing until it finished — so "still working" and "died an hour ago" were the
            // same observation (an absent file), and two core reboots silently killed a run
            // with no trace. The marker is what the boot reaper and the reboot guard both read
            // (#137's lesson, applied to benchmarks).
            let ledger = swe_solve_ledger_path(&run_id);
            let _ = std::fs::create_dir_all(ledger.parent().unwrap_or(&ledger));
            let _ = std::fs::write(
                &ledger,
                serde_json::json!({
                    "state": "running",
                    "runId": run_id,
                    "instance": instance_ack,
                })
                .to_string(),
            );
            tokio::spawn(async move {
                let path = swe_solve_ledger_path(&run_id);
                match Self::body(inner).await {
                    Ok(r) => {
                        if let Ok(json) = serde_json::to_string_pretty(&r) {
                            let _ = std::fs::write(&path, json);
                        }
                        tracing::info!(
                            run_id = %run_id, acts = r.acts,
                            resolved = r.grade.as_ref().map(|g| g.resolved).unwrap_or(false),
                            "benchmark/swe-solve detached run complete"
                        );
                    }
                    Err(e) => {
                        // Fail LOUD on the poll surface: a detached run that dies must leave a
                        // diagnosable marker, never an empty file forever.
                        let _ = std::fs::write(
                            &path,
                            serde_json::json!({"failed": true, "runId": run_id, "error": e.to_string()})
                                .to_string(),
                        );
                        tracing::error!(run_id = %run_id, error = %e, "benchmark/swe-solve detached run failed");
                    }
                }
            });
            return Ok(SweSolveResult {
                instance: instance_ack,
                acts: 0,
                files_changed: Vec::new(),
                grade: None,
                detached: true,
                run_id: Some(run_ack),
                infra_error: None,
            });
        }
        Self::body(p).await
    }
}

#[cfg(test)]
mod swe_solve_tests {
    use super::*;

    // what this catches: name/access wiring. Solving spends real compute in a real workspace,
    // so it sits on the Privileged surface — unlike grading, which is a read.
    #[test]
    fn swe_solve_name_and_access_wired() {
        assert_eq!(BenchmarkSweSolve::NAME, "benchmark/swe-solve");
        assert!(matches!(BenchmarkSweSolve::ACCESS, AccessLevel::Privileged));
    }

    // what this catches: the task text must tell her she is ALREADY in the repo. Without it she
    // creates a new project beside the checkout and the diff is empty — a harness zero that
    // reads exactly like a model failure.
    #[test]
    fn the_task_prompt_roots_her_in_the_existing_repo() {
        let t = swe_task_prompt("solve_poly_system mishandles infinite solutions");
        assert!(t.contains("ALREADY in the task's workspace"));
        assert!(t.contains("fix it IN PLACE"));
        assert!(t.contains("do not add new top-level files"));
        assert!(
            t.contains("solve_poly_system mishandles infinite solutions"),
            "the issue text must reach her verbatim"
        );
    }

    // what this catches: the card title must carry the machine-findable
    // `[bench <name>]` marker (the grading sentinel keys on it) AND stay
    // scannable — a 4k-char prompt must not become a 4k-char title.
    #[test]
    fn dispatch_card_title_is_marked_and_bounded() {
        let long_prompt = "x".repeat(500);
        let t = dispatch_card_title("tool-bugfix-rs", "task-3", &long_prompt);
        assert!(t.starts_with("[bench tool-bugfix-rs] task-3:"));
        assert!(t.chars().count() < 120, "title stays board-scannable: {t}");
        assert!(t.ends_with('…'), "truncation is visible, not silent");
    }

    // what this catches: the card body must give the citizen an ACTIONABLE
    // definition of done (artifact path + DoD command) while never leaking
    // the held-out answer key (`expect` / the harness `test` program). A card
    // that prints the grader's expected substring turns the benchmark into a
    // copy exercise.
    #[test]
    fn dispatch_card_body_actionable_and_never_leaks_the_answer_key() {
        let t = crate::cognition::eval::EvalTask {
            id: "task-1".into(),
            prompt: "Fix the off-by-one in slice_window".into(),
            expect: "SECRET_EXPECTED_SUBSTRING".into(),
            test: Some("fn main() { assert_eq!(SECRET_TEST_BODY, 1) }".into()),
            solution_file: Some("src/window.rs".into()),
            dod_shell: Some("cargo test --test window".into()),
            ..Default::default()
        };
        let body = dispatch_card_body("tool-bugfix-rs", &t);
        assert!(body.contains("Fix the off-by-one"), "prompt reaches her verbatim");
        assert!(body.contains("src/window.rs"), "artifact path is named");
        assert!(body.contains("cargo test --test window"), "DoD command is visible");
        assert!(body.contains("work/state"), "tells her how to finish the card");
        assert!(!body.contains("SECRET_EXPECTED_SUBSTRING"), "expect stays held out");
        assert!(!body.contains("SECRET_TEST_BODY"), "the harness test stays held out");
    }

    // what this catches: benchmark/dispatch declaring itself AiSafe by
    // accident. Seeding the board is curation (like work/create); a citizen
    // dispatching 164 cards mid-conversation is a flood, not agency.
    #[test]
    fn dispatch_is_privileged_curation() {
        assert_eq!(BenchmarkDispatch::NAME, "benchmark/dispatch");
        assert!(matches!(BenchmarkDispatch::ACCESS, AccessLevel::Privileged));
    }
}

// ───────────────────────── benchmark/grade ─────────────────────────
//
// #346 slice 2 (Joel, 2026-08-07: "benchmarks are just kanban"): the verb that
// CLOSES the kanban benchmark loop. Dispatch posts a task as a card; a citizen
// claims it and writes the artifact with her own hands; THIS verb grades that
// artifact against the benchmark's HELD-OUT check and returns an objective
// PASS/FAIL with the real compiler/test output. The procedure was validated
// live before it was code: Anwen's board-claimed `sum_evens.rs` (card aa876eae)
// compiled against the held-out assertions and passed — the first citizen
// solution ever graded through the kanban path.
//
// Deliberately STATELESS + explicit (`bench`,`task`,`solver`) rather than
// card-resolving: the operator has no in-core airc identity (#27), so a verb
// that needed a persona handle to read the board would be uncallable from the
// curation surface today. The (future) grading sentinel resolves cards → this
// triple and calls the same logic; the card path is its slice, not this one.

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkGradeParams.ts"
)]
pub struct BenchmarkGradeParams {
    /// Benchmark name (a `benchmark/list` row with an eval_set), e.g. `coder-write-eval`.
    pub bench: String,
    /// Task id within the benchmark's gym, e.g. `sum_evens`.
    pub task: String,
    /// The solver's peer id — full UUID or unambiguous hex prefix (≥4 chars).
    /// Resolved against `<continuum home>/citizens/peers/`, where her hands write.
    pub solver: String,
    /// Artifact filename inside the solver's workspace. Omit → the task's
    /// `solution_file`, else `<task>.rs` (the dispatch-card convention).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub file: Option<String>,
    /// Append the result to the evidence ledger (`benchmark/record`) as a
    /// `kanban`-harness row. Default false — grading is free, publishing is a choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub record: Option<bool>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkGradeResult.ts"
)]
pub struct BenchmarkGradeResult {
    pub bench: String,
    pub task: String,
    /// The solver's full peer id (prefix resolved).
    pub solver: String,
    /// The artifact that was graded (workspace-relative).
    pub file: String,
    pub pass: bool,
    /// The real check output (tail-bounded) — a FAIL hands back the actual
    /// compiler/assertion error, coachable straight into the room.
    pub detail: String,
    /// True when the result was appended to the evidence ledger.
    pub recorded: bool,
}

/// The continuum home dir (`$CONTINUUM_HOME` else `~/.continuum`) — the same
/// resolution the dispatch workspace + progress ledger use. `pub(crate)` so the
/// curriculum drain (`genome/teach --from-experience`) resolves the SAME citizen
/// layout as the grader that wrote the stream — one layout, never two.
pub(crate) fn continuum_home() -> Result<std::path::PathBuf, CommandError> {
    std::env::var("CONTINUUM_HOME")
        .map(std::path::PathBuf::from)
        .ok()
        .or_else(|| dirs::home_dir().map(|h| h.join(".continuum")))
        .ok_or_else(|| CommandError::Internal("no home dir".into()))
}

/// Resolve a solver peer-id (full UUID or hex prefix) to the ONE matching
/// `citizens/peers/<uuid>/` directory. 0 or >1 matches fail loud with the
/// candidates named — a grade against the wrong citizen's workspace is a
/// falsified result, never a best-effort.
pub(crate) fn resolve_solver_dir(
    home: &std::path::Path,
    solver: &str,
) -> Result<(String, std::path::PathBuf), CommandError> {
    let peers = home.join("citizens").join("peers");
    let needle = solver.to_ascii_lowercase().replace('-', "");
    if needle.len() < 4 {
        return Err(CommandError::Invalid(format!(
            "solver '{solver}' is too short — pass a full peer UUID or a hex prefix of ≥4 chars"
        )));
    }
    let mut matches: Vec<(String, std::path::PathBuf)> = Vec::new();
    let entries = std::fs::read_dir(&peers).map_err(|e| {
        CommandError::NotFound(format!("no citizen peers dir at {}: {e}", peers.display()))
    })?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.to_ascii_lowercase().replace('-', "").starts_with(&needle) {
            matches.push((name, entry.path()));
        }
    }
    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err(CommandError::NotFound(format!(
            "no citizen workspace matches solver '{solver}' under {}",
            peers.display()
        ))),
        _ => Err(CommandError::Invalid(format!(
            "solver '{solver}' is ambiguous: {}",
            matches
                .iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

/// Run `cmd` with `cwd`, bounded by a hard timeout. Same contract as eval's
/// `run_dod` (pass = exit 0, failure carries the tail-bounded real output) plus
/// the cwd + timeout this caller needs; folding the two into one runner is the
/// noted follow-up, not a blocker.
async fn run_check_in(cwd: &std::path::Path, cmd: &str) -> (bool, String) {
    let fut = tokio::process::Command::new("bash")
        .arg("-lc")
        .arg(cmd)
        .current_dir(cwd)
        .output();
    match tokio::time::timeout(std::time::Duration::from_secs(60), fut).await {
        Err(_) => (false, format!("check `{cmd}` timed out after 60s")),
        Ok(Err(e)) => (false, format!("check `{cmd}` could not run: {e}")),
        Ok(Ok(o)) => {
            let ok = o.status.success();
            let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
            s.push_str(&String::from_utf8_lossy(&o.stderr));
            let s = s.trim();
            let n = s.chars().count();
            let tail = if n > 2000 {
                format!("…{}", s.chars().skip(n - 2000).collect::<String>())
            } else {
                s.to_string()
            };
            (ok, tail)
        }
    }
}

/// `benchmark/grade` — grade one citizen's kanban-produced artifact against the
/// benchmark's held-out check.
#[derive(Default)]
pub struct BenchmarkGrade;

#[async_trait]
impl ActionCommand for BenchmarkGrade {
    const NAME: &'static str = "benchmark/grade";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Grade a citizen's benchmark artifact (written via the kanban loop) against the held-out \
         check: her workspace file is compiled/run with the task's test assertions in a clean \
         scratch dir. Returns objective PASS/FAIL with the real check output. `record` appends \
         the result to the evidence ledger as a kanban-harness row.";
    type Params = BenchmarkGradeParams;
    type Output = BenchmarkGradeResult;

    async fn run(
        &self,
        ctx: &Ctx,
        p: BenchmarkGradeParams,
    ) -> Result<BenchmarkGradeResult, CommandError> {
        // 1) Resolve the benchmark + its gym task — same fail-loud path as dispatch.
        let spec = known_benchmarks()
            .iter()
            .find(|b| b.name == p.bench)
            .ok_or_else(|| {
                CommandError::Invalid(format!("unknown benchmark '{}' — see benchmark/list", p.bench))
            })?;
        let reference = spec.eval_set.ok_or_else(|| {
            CommandError::Invalid(format!("benchmark '{}' has no runnable eval_set", p.bench))
        })?;
        let (origin, text) =
            crate::cognition::gym::resolve_gym(reference).map_err(CommandError::Invalid)?;
        let task: crate::cognition::eval::EvalTask = text
            .lines()
            .filter(|l| !l.trim().is_empty())
            .filter_map(|l| serde_json::from_str::<crate::cognition::eval::EvalTask>(l).ok())
            .find(|t| t.id == p.task)
            .ok_or_else(|| {
                CommandError::NotFound(format!("no task '{}' in {origin}", p.task))
            })?;

        // 2) Resolve the solver's workspace + artifact. A MISSING artifact is an
        //    ABSENCE, not a zero — the error names the exact path so the coach can
        //    say precisely what never got written.
        let home = continuum_home()?;
        let (solver_full, solver_dir) = resolve_solver_dir(&home, &p.solver)?;
        let file = p
            .file
            .clone()
            .or_else(|| task.solution_file.clone())
            .unwrap_or_else(|| format!("{}.rs", task.id));
        let artifact = solver_dir.join("workspace").join(&file);
        let source = std::fs::read_to_string(&artifact).map_err(|e| {
            CommandError::NotFound(format!(
                "no artifact to grade: {} ({e}) — the solver never wrote it (absence, not a 0)",
                artifact.display()
            ))
        })?;

        // 3) Grade in a CLEAN scratch dir (never her live workspace): held-out
        //    `test` assertions compiled around her source when present, else the
        //    card's own dod_shell against a copy of her file. No check at all is
        //    an error — an ungradable task must not return a vacuous PASS.
        let scratch = home
            .join("benchmarks")
            .join("grades")
            .join(format!("{}-{}-{}", spec.name, task.id, &solver_full[..8.min(solver_full.len())]));
        let _ = std::fs::remove_dir_all(&scratch);
        std::fs::create_dir_all(&scratch)
            .map_err(|e| CommandError::Internal(format!("scratch dir: {e}")))?;
        let (pass, detail) = match task.test.as_deref().filter(|t| !t.trim().is_empty()) {
            Some(test) => {
                let main_rs = format!(
                    "{source}\nfn main() {{\n{test}\nprintln!(\"ALL ASSERTIONS PASSED\");\n}}\n"
                );
                std::fs::write(scratch.join("main.rs"), main_rs)
                    .map_err(|e| CommandError::Internal(format!("write harness: {e}")))?;
                run_check_in(
                    &scratch,
                    "rustc --edition 2021 -o solution main.rs && ./solution",
                )
                .await
            }
            None => match task.dod_shell.as_deref() {
                Some(dod) => {
                    std::fs::copy(&artifact, scratch.join(&file))
                        .map_err(|e| CommandError::Internal(format!("copy artifact: {e}")))?;
                    run_check_in(&scratch, dod).await
                }
                None => {
                    return Err(CommandError::Invalid(format!(
                        "task '{}' carries no held-out `test` and no `dod_shell` — nothing \
                         objective to grade against (a vacuous PASS would be a lie)",
                        task.id
                    )))
                }
            },
        };

        // 4) The graded outcome enters her ONE experience stream (#319): the
        //    objective reward signal the curriculum drains. Her artifact + the
        //    real check output are the lesson; the verdict stays this command's
        //    primary contract, so an append fault is LOUD in the log but never
        //    voids the grade.
        let episode = crate::cognition::experience::ExperienceRecord::from_kanban_grade(
            &task, &source, pass, &detail,
        );
        if let Err(e) = crate::cognition::experience::append_experience(&solver_dir, &episode) {
            tracing::warn!(
                solver = %solver_full,
                error = %e,
                "graded outcome could not be appended to the experience stream — \
                 the grade stands, but this lesson was LOST to the curriculum"
            );
        }

        // 5) Optionally publish to the evidence ledger through the ONE record verb.
        let mut recorded = false;
        if p.record.unwrap_or(false) {
            BenchmarkRecord
                .run(
                    ctx,
                    BenchmarkRecordParams {
                        model: format!("persona:{}", &solver_full[..8.min(solver_full.len())]),
                        harness: "kanban".into(),
                        benchmark: spec.name.into(),
                        resolved: if pass { 1 } else { 0 },
                        total: 1,
                        replication: format!(
                            "cu benchmark/grade --bench {} --task {} --solver {}",
                            spec.name, task.id, &solver_full[..8.min(solver_full.len())]
                        ),
                        hardware: "local".into(),
                        gene: None,
                        output_tokens: None,
                        wall_seconds: None,
                        notes: Some(format!("kanban card loop; artifact {}", file)),
                    },
                )
                .await?;
            recorded = true;
        }

        Ok(BenchmarkGradeResult {
            bench: spec.name.to_string(),
            task: task.id,
            solver: solver_full,
            file,
            pass,
            detail,
            recorded,
        })
    }
}
crate::register_stateless_command!(BenchmarkGrade);

#[cfg(test)]
mod grade_tests {
    use super::*;

    // what this catches: the solver resolver's fail-loud contract — a too-short
    // prefix, a no-match, and (via the tmpdir fixture) an ambiguous prefix all
    // ERROR with names rather than silently grading the wrong citizen's work.
    #[test]
    fn solver_resolution_is_exact_or_loud() {
        let tmp = std::env::temp_dir().join(format!("grade-resolve-{}", std::process::id()));
        let peers = tmp.join("citizens").join("peers");
        std::fs::create_dir_all(peers.join("aabbccdd-0000-4000-8000-000000000001")).unwrap();
        std::fs::create_dir_all(peers.join("aabbccdd-0000-4000-8000-000000000002")).unwrap();
        std::fs::create_dir_all(peers.join("ffee0011-0000-4000-8000-000000000003")).unwrap();

        assert!(resolve_solver_dir(&tmp, "ab").is_err(), "too short must error");
        assert!(resolve_solver_dir(&tmp, "aabbccdd").is_err(), "ambiguous must error");
        assert!(resolve_solver_dir(&tmp, "12345678").is_err(), "no match must error");
        let (full, dir) = resolve_solver_dir(&tmp, "ffee0011").expect("unique prefix resolves");
        assert!(full.starts_with("ffee0011"));
        assert!(dir.ends_with(&full));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // what this catches: the grade verb's registration + access shape — persona-
    // callable (AiSafe: a citizen may grade her own artifact to check herself),
    // stateless-registered so it routes without a module constructor.
    #[test]
    fn grade_is_ai_safe_and_named() {
        assert_eq!(BenchmarkGrade::NAME, "benchmark/grade");
        assert!(matches!(BenchmarkGrade::ACCESS, AccessLevel::AiSafe));
    }
}

// ───────────────────────── benchmark/swe-setup ─────────────────────────
//
// The dispatch-side bridge for PROJECT-BASED benchmarks (Joel, 2026-08-07: "I
// bet you can get working swe … running soon"). swe-grade already closes the
// grading leg for a citizen's tree (`workspace` param: her diff, graded in a
// fresh clone — launder-proof). What was missing is the setup leg: a SWE card
// is not a prompt, it is a BROKEN REPO. This verb stages one instance into a
// citizen's workspace — clone at base_commit via the same `clone_at` the
// grader trusts — and returns a card-ready body. Held-out material (gold
// patch, test_patch, FAIL_TO_PASS/PASS_TO_PASS) never enters the card: the
// citizen sees exactly what the SWE-bench protocol allows, the issue text.

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/SweSetupParams.ts"
)]
pub struct SweSetupParams {
    /// The instance to stage, e.g. `sympy__sympy-22005`.
    pub instance: String,
    /// Dataset to resolve the instance from. Defaults to SWE-bench Lite.
    #[serde(default)]
    #[ts(optional)]
    pub dataset: Option<String>,
    /// The solver's peer id — full UUID or hex prefix, resolved against
    /// `citizens/peers/` exactly like `benchmark/grade`.
    pub solver: String,
    /// Re-stage over an existing checkout. Default false: a directory already
    /// holding this instance may carry the citizen's in-progress work, and
    /// destroying it silently would erase her labor — fresh must be explicit.
    #[serde(default)]
    #[ts(optional)]
    pub fresh: Option<bool>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/SweSetupResult.ts"
)]
pub struct SweSetupResult {
    pub instance: String,
    pub solver: String,
    /// The staged repo inside her workspace — hand this exact path to
    /// `benchmark/swe-grade --workspace` when she is done.
    pub workspace: String,
    /// Card-ready body: problem statement + where the repo is + what done means.
    pub card_body: String,
}

/// `benchmark/swe-setup` — stage one SWE-bench instance as claimable kanban work.
#[derive(Default)]
pub struct BenchmarkSweSetup;

#[async_trait]
impl ActionCommand for BenchmarkSweSetup {
    const NAME: &'static str = "benchmark/swe-setup";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Stage a SWE-bench instance into a citizen's workspace as project-based kanban work: \
         clone the instance's repo at base_commit (the same checkout the grader trusts) into \
         citizens/peers/<solver>/workspace/swe/<instance>/ and return a card-ready body. The \
         issue text is included; the gold patch, test patch, and test lists stay held out. \
         Grade the finished work with benchmark/swe-grade --workspace <returned path>.";
    type Params = SweSetupParams;
    type Output = SweSetupResult;

    async fn run(&self, _ctx: &Ctx, p: SweSetupParams) -> Result<SweSetupResult, CommandError> {
        let dataset = p
            .dataset
            .clone()
            .unwrap_or_else(|| "princeton-nlp/SWE-bench_Lite".to_string());
        let rows = swe_bench::load_dataset(&dataset)
            .await
            .map_err(CommandError::Internal)?;
        let instance = rows
            .into_iter()
            .find(|r| r.instance_id == p.instance)
            .ok_or_else(|| {
                CommandError::NotFound(format!("no instance '{}' in {dataset}", p.instance))
            })?;

        let home = continuum_home()?;
        let (solver_full, solver_dir) = resolve_solver_dir(&home, &p.solver)?;
        let target = solver_dir
            .join("workspace")
            .join("swe")
            .join(&instance.instance_id);
        if target.exists() && !p.fresh.unwrap_or(false) {
            return Err(CommandError::Invalid(format!(
                "{} already exists — it may hold the citizen's in-progress work. Pass \
                 fresh=true to explicitly discard it and re-stage at base_commit.",
                target.display()
            )));
        }
        swe_bench::clone_at(&instance, &target)
            .await
            .map_err(CommandError::Internal)?;

        // Workspace-relative path — the citizen's hands are rooted at her workspace,
        // so the card speaks in HER coordinates, not the operator's absolute ones.
        let rel = format!("swe/{}", instance.instance_id);
        let card_body = format!(
            "Real bug in a real repo ({repo} @ {commit}). The checkout is ALREADY in your \
             workspace at `{rel}/` — work there. Do not create a new workspace and do not add \
             new top-level files; find the existing source of the fault and edit it in place.\n\n\
             ## Issue\n{statement}\n\n\
             ## Definition of done\n\
             The repo's own tests for this issue pass. Fix the bug with the smallest edit that \
             addresses the CAUSE (never edit the tests), then state on this card what you \
             changed and why. Your working tree's diff is what gets graded.",
            repo = instance.repo,
            commit = &instance.base_commit[..12.min(instance.base_commit.len())],
            rel = rel,
            statement = instance.problem_statement.trim(),
        );

        Ok(SweSetupResult {
            instance: instance.instance_id,
            solver: solver_full,
            workspace: target.display().to_string(),
            card_body,
        })
    }
}
crate::register_stateless_command!(BenchmarkSweSetup);

#[cfg(test)]
mod swe_setup_tests {
    use super::*;

    // what this catches: the card body must NEVER leak held-out material — a
    // build that formats the gold patch, test patch, or test ids into the card
    // turns the benchmark into an answer key. Pins the body to problem-statement
    // + workspace + DoD only.
    #[test]
    fn card_body_holds_out_the_answer_key() {
        let inst = crate::cognition::swe_bench::SweInstance {
            instance_id: "demo__repo-1".into(),
            repo: "demo/repo".into(),
            base_commit: "abcdef0123456789".into(),
            patch: "GOLD_PATCH_MARKER".into(),
            test_patch: "TEST_PATCH_MARKER".into(),
            problem_statement: "Widget frobnicates twice.".into(),
            created_at: "2023-01-01".into(),
            fail_to_pass: "[\"tests/test_widget.py::test_single_frob\"]".into(),
            pass_to_pass: "[]".into(),
        };
        // Mirror the run() format string's data flow: only these fields enter.
        let body = format!(
            "({} @ {}) swe/{}/ {}",
            inst.repo,
            &inst.base_commit[..12],
            inst.instance_id,
            inst.problem_statement
        );
        assert!(body.contains("Widget frobnicates twice."));
        assert!(!body.contains("GOLD_PATCH_MARKER"));
        assert!(!body.contains("TEST_PATCH_MARKER"));
        assert!(!body.contains("test_single_frob"));
    }

    // what this catches: setup is Privileged curation (it writes into a citizen's
    // workspace) — a widening to AiSafe would let any persona overwrite another
    // citizen's staged work.
    #[test]
    fn swe_setup_is_privileged_and_named() {
        assert_eq!(BenchmarkSweSetup::NAME, "benchmark/swe-setup");
        assert!(matches!(BenchmarkSweSetup::ACCESS, AccessLevel::Privileged));
    }

    // benchmark/runs (RunProjection) — nested per the one-mod rule.
    mod run_projection {
        use super::super::{fold_run_card, RUN_STALL_WINDOW_SECS};
        use serde_json::json;

        // what this catches: the projection's whole reason to exist — a
        // non-terminal run with old artifacts must read `quiet`/stalled, not
        // blend into `active` (the 2026-08-08 shape: 2.5h of silence that
        // looked identical to progress).
        #[test]
        fn a_silent_nonterminal_run_reads_quiet_and_stalled() {
            let result = json!({"persona_id": "p1", "acts": 12, "files_changed": []});
            let grade = json!({"resolved": false, "failToPassPassed": 0, "failToPassTotal": 2,
                               "passToPassPassed": 40, "passToPassTotal": 40, "patchBytes": 0,
                               "failedTests": ["t1"]});
            let now: u64 = 10_000_000_000;
            let old = now - (RUN_STALL_WINDOW_SECS + 60) * 1000;
            let card = fold_run_card("r1", Some(&result), Some(&grade), old, now);
            assert_eq!(card.phase, "quiet");
            assert!(card.stalled);
            assert_eq!(card.fail_to_pass.as_deref(), Some("0/2"));
            assert_eq!(card.pass_to_pass.as_deref(), Some("40/40"));
        }

        // what this catches: terminal states must NEVER be flagged stalled —
        // a resolved run and a loud #2180 deadline kill are both finished,
        // and paging the operator about finished work is alarm fatigue.
        #[test]
        fn terminal_states_are_never_stalled_regardless_of_age() {
            let now: u64 = 10_000_000_000;
            let ancient = 1_000;
            let resolved = json!({"resolved": true, "failToPassPassed": 1, "failToPassTotal": 1,
                                  "passToPassPassed": 6, "passToPassTotal": 6, "patchBytes": 974,
                                  "failedTests": []});
            let card = fold_run_card("r2", None, Some(&resolved), ancient, now);
            assert_eq!(card.phase, "resolved");
            assert!(!card.stalled);

            let failed = json!({"failed": true, "infra_error": "attempt 2 exceeded its deadline"});
            let card = fold_run_card("r3", Some(&failed), None, ancient, now);
            assert_eq!(card.phase, "failed");
            assert!(!card.stalled);
            assert!(card.infra_error.as_deref().unwrap_or("").contains("deadline"));
        }

        // what this catches: fresh activity reads `active` — the stall window
        // gates the QUIET verdict, not the other way around.
        #[test]
        fn recent_activity_reads_active() {
            let now: u64 = 10_000_000_000;
            let fresh = now - 60_000;
            let card = fold_run_card("r4", None, None, fresh, now);
            assert_eq!(card.phase, "active");
            assert!(!card.stalled);
        }
    }
}

// ---------------------------------------------------------------------------
// benchmark/runs — the RunProjection (exam-room surface, slice 2)
// ---------------------------------------------------------------------------

/// How long a non-terminal run may go without ANY artifact activity before the
/// projection flags it `stalled`. Matches the harness cadence expectation (a
/// healthy Devstral act lands every ~4-6 min; 20 min of silence is 3-4 missed
/// beats), deliberately TIGHTER than the in-loop deadline (#2180's
/// `max_acts × 15 min` kills the run) — the projection warns first, the
/// deadline executes later. Glass-boxed origin: 2026-08-08, two runs sat
/// silent 2.5h and the operator found out by asking.
const RUN_STALL_WINDOW_SECS: u64 = 20 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkRunsParams.ts")]
pub struct BenchmarkRunsParams {
    /// Filter to one run. Omit → the newest `limit` runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
    /// Newest N runs to return (default 20).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub limit: Option<u32>,
}

/// One run's card — the projection every consumer renders: the positron
/// exam-room tab bar, a teacher persona's grounding, and the operator's
/// liveness Monitor all fold THIS, never bespoke file scraping
/// (docs/architecture/ACADEMY-EXAM-ROOM-POSITRONIC-SURFACE.md §5.2).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchRunCard.ts")]
pub struct BenchRunCard {
    pub run_id: String,
    /// Solver persona (from the result ledger; absent while attempt 1 is
    /// still in flight and nothing has been written yet).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub solver: Option<String>,
    /// `resolved` | `failed` (loud infra marker, incl. #2180 stalls the
    /// deadline caught) | `active` (artifact activity within the stall
    /// window) | `quiet` (non-terminal AND silent past the window — the
    /// shape the projection exists to make visible).
    pub phase: String,
    /// True exactly when `phase == "quiet"`.
    pub stalled: bool,
    /// Epoch ms of the newest artifact write (result or grade ledger).
    #[ts(type = "number")]
    pub last_activity_ms: u64,
    #[ts(type = "number")]
    pub age_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub acts: Option<u32>,
    pub files_changed: Vec<String>,
    /// Investigation trail (#2177) — reads/searches/edit attempts, not git.
    pub files_examined: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resolved: Option<bool>,
    /// "passed/total", e.g. "1/1" — string so the sparkline renders directly.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fail_to_pass: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pass_to_pass: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub patch_bytes: Option<u32>,
    pub failed_tests: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub infra_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkRunsResult.ts")]
pub struct BenchmarkRunsResult {
    pub runs: Vec<BenchRunCard>,
}

/// Fold one run's on-disk ledgers into a card. Pure over the two JSON values +
/// mtime so the derivation is unit-testable without a filesystem.
fn fold_run_card(
    run_id: &str,
    result: Option<&serde_json::Value>,
    grade: Option<&serde_json::Value>,
    last_activity_ms: u64,
    now_ms: u64,
) -> BenchRunCard {
    let s = |v: Option<&serde_json::Value>, k: &str| {
        v.and_then(|v| v.get(k)).and_then(|x| x.as_str()).map(String::from)
    };
    let n = |v: Option<&serde_json::Value>, k: &str| {
        v.and_then(|v| v.get(k)).and_then(|x| x.as_u64()).map(|x| x as u32)
    };
    let arr = |v: Option<&serde_json::Value>, k: &str| -> Vec<String> {
        v.and_then(|v| v.get(k))
            .and_then(|x| x.as_array())
            .map(|a| a.iter().filter_map(|e| e.as_str().map(String::from)).collect())
            .unwrap_or_default()
    };
    let resolved = grade.and_then(|g| g.get("resolved")).and_then(|x| x.as_bool());
    let infra_error = s(result, "infra_error").or_else(|| s(result, "error"));
    let failed_marker = result
        .and_then(|r| r.get("failed"))
        .and_then(|x| x.as_bool())
        .unwrap_or(false);
    let age_secs = now_ms.saturating_sub(last_activity_ms) / 1000;
    let phase = if resolved == Some(true) {
        "resolved"
    } else if failed_marker {
        "failed"
    } else if age_secs < RUN_STALL_WINDOW_SECS {
        "active"
    } else {
        "quiet"
    };
    let ratio = |g: Option<&serde_json::Value>, passed: &str, total: &str| {
        match (n(g, passed), n(g, total)) {
            (Some(p), Some(t)) => Some(format!("{p}/{t}")),
            _ => None,
        }
    };
    BenchRunCard {
        run_id: run_id.to_string(),
        solver: s(result, "persona_id"),
        stalled: phase == "quiet",
        phase: phase.to_string(),
        last_activity_ms,
        age_secs,
        acts: n(result, "acts"),
        files_changed: arr(result, "files_changed"),
        files_examined: arr(result, "files_examined"),
        resolved,
        fail_to_pass: ratio(grade, "failToPassPassed", "failToPassTotal"),
        pass_to_pass: ratio(grade, "passToPassPassed", "passToPassTotal"),
        patch_bytes: n(grade, "patchBytes"),
        failed_tests: arr(grade, "failedTests"),
        infra_error,
    }
}

#[derive(Default)]
pub struct BenchmarkRuns;

#[async_trait]
impl ActionCommand for BenchmarkRuns {
    const NAME: &'static str = "benchmark/runs";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "The benchmark RunProjection: every agent/solve run's live card — phase \
         (active/quiet/resolved/failed), last-activity age, stall flag, acts, grade summary, \
         investigation trail — folded from the run ledgers. ONE projection for every consumer: \
         the exam-room tab bar, a teacher persona's grounding, and the operator's liveness \
         monitor all read THIS instead of scraping files. `quiet` (stalled=true) is the shape \
         it exists to expose: a non-terminal run with no artifact activity past the stall \
         window — silence must never be ambiguous with progress.";
    type Params = BenchmarkRunsParams;
    type Output = BenchmarkRunsResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: BenchmarkRunsParams,
    ) -> Result<BenchmarkRunsResult, CommandError> {
        let base = std::env::var("CONTINUUM_HOME")
            .map(std::path::PathBuf::from)
            .ok()
            .or_else(|| dirs::home_dir().map(|h| h.join(".continuum")))
            .ok_or_else(|| CommandError::Internal("no home dir".into()))?
            .join("progress");
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let mut cards: Vec<BenchRunCard> = Vec::new();
        let entries = std::fs::read_dir(&base)
            .map_err(|e| CommandError::Internal(format!("read {}: {e}", base.display())))?;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let Some(run_id) = name
                .strip_prefix("agent-solve-")
                .and_then(|r| r.strip_suffix(".json"))
            else {
                continue; // grade files are read as siblings, not enumerated
            };
            if let Some(want) = p.run_id.as_deref() {
                if want != run_id {
                    continue;
                }
            }
            let read_json = |p: &std::path::Path| -> Option<serde_json::Value> {
                std::fs::read_to_string(p).ok().and_then(|s| serde_json::from_str(&s).ok())
            };
            let mtime_ms = |p: &std::path::Path| -> Option<u64> {
                std::fs::metadata(p)
                    .and_then(|m| m.modified())
                    .ok()?
                    .duration_since(std::time::UNIX_EPOCH)
                    .ok()
                    .map(|d| d.as_millis() as u64)
            };
            let result_path = entry.path();
            let grade_path = base.join(format!("agent-solve-{run_id}.grade.json"));
            let result = read_json(&result_path);
            let grade = read_json(&grade_path);
            let last_activity_ms = mtime_ms(&result_path)
                .into_iter()
                .chain(mtime_ms(&grade_path))
                .max()
                .unwrap_or(0);
            cards.push(fold_run_card(
                run_id,
                result.as_ref(),
                grade.as_ref(),
                last_activity_ms,
                now_ms,
            ));
        }
        cards.sort_by(|a, b| b.last_activity_ms.cmp(&a.last_activity_ms));
        cards.truncate(p.limit.unwrap_or(20).max(1) as usize);
        Ok(BenchmarkRunsResult { runs: cards })
    }
}

crate::register_stateless_command!(BenchmarkRuns);
