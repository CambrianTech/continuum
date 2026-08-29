//! `benchmark/*` — first-class benchmark collections, managed like the model catalog. A
//! declarative catalog of known benchmarks (add one = one row, same as a `ModelSpec`) plus
//! `benchmark/list` to see them and `benchmark/dispatch` to adapt one INTO the work board as
//! claimable cards. There is no divergent "run a benchmark" verb: every benchmark goes through
//! the natural kanban loop — citizens claim a card and solve it with their own hands, and the
//! result is graded from the artifacts they produce. This lives in Rust, on the DynCommand
//! registry, so it is
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
    /// Held-out expected-answer substring (case-insensitive) — cognition/eval's `expect`
    /// grade. Live today for the input-side vision tier (`vision-qa`: SEE an image with
    /// vision/look, answer objectively).
    Answer,
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

impl BenchmarkSpec {
    /// The HuggingFace dataset id for a SWE-bench-INSTANCE-shaped collection (each row a
    /// real repo + `base_commit` + gold `patch` + `problem_statement` + held-out tests),
    /// pulled on demand by [`crate::cognition::swe_bench::load_dataset`] and dispatched as
    /// real full-project cards. `None` for the gym collections and for non-SWE datasets.
    ///
    /// Instance-shape is a property of the COLLECTION, not derivable from the URL alone —
    /// `livecodebench`/`bigcodebench` are also HF datasets but do NOT speak `SweInstance` —
    /// so the SWE family is named here (the ONE place that knows), and the id itself is read
    /// back off `source_url` so it is never duplicated. Add `swe-lancer` etc. here when its
    /// loader lands. This is what makes `runnable` true and `benchmark/dispatch` work for the
    /// real-project tier the frontier models fight over (Joel's target, 2026-08-10).
    pub fn swe_dataset(&self) -> Option<&'static str> {
        if !matches!(self.name, "swe-bench-lite" | "swe-bench-verified") {
            return None;
        }
        self.source_url?
            .strip_prefix("https://huggingface.co/datasets/")
    }
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
            name: "vision-qa",
            description: "Vision-QA — OUR input-side vision benchmark: SEE a generated image \
                          (vision/look through her real sensory bridge) and answer an objective \
                          question — 16 contamination-free tasks, held-out substring oracle.",
            grader: Grader::Answer,
            tasks: 16,
            eval_set: Some("vision-qa"),
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
            name: "super-masked",
            description: "SUPER-Masked (AI2, EMNLP'24) — 152 checkpointed sub-scenarios of \
                          'make a real research repo run': clone at commit, wrangle the env, \
                          execute the prior-work scaffold, produce the answer JSON. Graded by \
                          upstream's own evaluate() (float epsilon 1e-2) ported verbatim; \
                          landmark partial-credit deferred and DECLARED in every receipt. \
                          Fetch first (`benchmark/fetch --benchmark super-masked`).",
            grader: Grader::Python,
            tasks: 152,
            eval_set: Some("super-masked.jsonl"),
            source_url: Some("https://huggingface.co/datasets/allenai/super"),
        },
        BenchmarkSpec {
            name: "algotune",
            description: "AlgoTune (NeurIPS'25) — 150+ 'beat the reference library' tasks \
                          (SciPy/NumPy/sklearn/CVXPY): structurally contamination-proof (no \
                          hidden answers — the reference is public and the task is to be \
                          FASTER, correctness-gated by the task's own checker on held-out \
                          seeds). Harness: warmup + min-of-10, machine-relative calibration; \
                          pass = parity, score = speedup (in the dod receipt). Fetch first \
                          (`benchmark/fetch --benchmark algotune`).",
            grader: Grader::Python,
            tasks: 154,
            eval_set: Some("algotune.jsonl"),
            source_url: Some("https://github.com/oripress/AlgoTune"),
        },
        BenchmarkSpec {
            name: "mirrorcode",
            description: "MirrorCode (Epoch AI × METR) — reimplement an ENTIRE program \
                          (Unix utils, format tools, a C preprocessor, a CAS subset, a \
                          scripting-language CLI) from observable behavior only: 26 public \
                          target programs, one task each, graded by exact stdout/stderr/\
                          exit-code match over the FULL recorded case set (visible cases + \
                          hidden anti-hardcoding duals; 100% required, upstream's own bar). \
                          Frontier 2026-08: Claude Fable 5 64%, GPT-5.6 Sol 20% \
                          (epoch.ai/benchmarks/mirrorcode, Go/Ada targets, 10B-token \
                          attempts). OUR run is a DECLARED VARIANT, internal signal only: \
                          Rust target language, expected outputs staged in place of the \
                          reference binary, our own act budget — and the whole oracle \
                          (hidden duals included) is public upstream, so contamination is \
                          assumed. Fetch first (`benchmark/fetch --benchmark mirrorcode`).",
            grader: Grader::Rust,
            tasks: 26,
            eval_set: Some("mirrorcode.jsonl"),
            source_url: Some("https://github.com/epoch-research/MirrorCode"),
        },
        BenchmarkSpec {
            name: "ds-1000",
            description: "DS-1000 (XLang/HKU, ICML'23) — 1,000 data-science problems over \
                          Pandas/NumPy/SciPy/sklearn/Matplotlib/PyTorch/TF, graded by the \
                          OFFICIAL execution oracle (her solution.py substituted at the \
                          [insert] marker of each row's code_context and RUN, 120s cap). \
                          Tier-1 pick of the 2026-08-22 landscape sweep: execution-graded, \
                          local, seconds/task. Fetch first (`benchmark/fetch --benchmark \
                          ds-1000`) — that converts the HF rows onto the gym rails.",
            grader: Grader::Python,
            tasks: 1000,
            eval_set: Some("ds-1000.jsonl"),
            source_url: Some("https://huggingface.co/datasets/xlangai/DS-1000"),
        },
        BenchmarkSpec {
            name: "terminal-bench",
            description: "Terminal-Bench 2.1 (Stanford × Laude Institute) — 89 real terminal \
                          tasks from the Harbor registry (compile a COBOL modernization, \
                          recover a WAL-corrupted db, configure nginx), each graded by its \
                          own pytest oracle over the FINAL workspace state, with the task's \
                          own verifier timeout. The frontier's agentic mid-rung: TB 2.1 \
                          harness+model pairs score 74–84% (Fable 5 + Claude Code 83.8%); \
                          TB 3.0 'Frontier Bench' ceilings at 34.4% (GPT-5.6 Sol). Docker-skip \
                          policy: tasks whose initial state is BUILT inside their container \
                          image (compiles, generated data, cloned repos, multi-stage/compose \
                          topologies) cannot be reproduced as plain file staging and are \
                          excluded as COUNTED, named skips in the fetch receipt — 53 of 89 \
                          convert on the 2026-08-23 registry. Fetch first (`benchmark/fetch \
                          --benchmark terminal-bench`).",
            grader: Grader::Python,
            tasks: 89,
            eval_set: Some("terminal-bench.jsonl"),
            source_url: Some("https://github.com/harbor-framework/terminal-bench-2-1"),
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
            description: "SWE-bench Verified — the 500 human-validated instances (OpenAI). SATURATED \
                          as a frontier signal (Opus 5 at 97.0%, seven models ≥95%, vals.ai 2026-08-19) \
                          — keep as a floor/sanity check, never a headline. Solution = a repo patch \
                          that passes the real test suite.",
            grader: Grader::Python,
            tasks: 500,
            eval_set: None,
            source_url: Some("https://huggingface.co/datasets/princeton-nlp/SWE-bench_Verified"),
        },
        BenchmarkSpec {
            name: "swe-rebench",
            description: "SWE-rebench (Nebius) — continuously-mined real GitHub issue→PR tasks, \
                          CONTAMINATION-PROOF BY CONSTRUCTION: rolling time-windows mean only \
                          instances newer than a model's cutoff count. Frontier mid-2026: Fable 5 \
                          64.5%, Opus 5 63.4% on the May–Jul window. Same instance schema as \
                          SWE-bench (list-shaped F2P/P2P normalized by the tolerant mapper); full \
                          test split 21,336 instances — an HONEST run selects a window newer than \
                          the model's training cutoff via each instance's created_at and says so \
                          in the receipt. Tier-1 pick of the 2026-08-23 frontier-landscape sweep \
                          (docs/planning/FRONTIER-BENCHMARK-LANDSCAPE-2026-08.md).",
            grader: Grader::Python,
            tasks: 21336,
            eval_set: None,
            source_url: Some("https://huggingface.co/datasets/nebius/SWE-rebench"),
        },
        BenchmarkSpec {
            name: "swe-bench-pro",
            description: "SWE-bench Pro (Scale AI) — the Verified successor: 731 public instances \
                          (of 1,865) across 41 professional repos, long-horizon multi-file issues, \
                          copyleft/held-out contamination barrier. Frontier mid-2026: 46–61% \
                          (Gemini 3.1 Pro 46.1%, Opus 4.6 51.9%, GPT-5.4 59.1%) — frontier-HARD. \
                          MULTI-LANGUAGE repos (js/go/py — `repo_language` per row): python \
                          instances grade through the existing era-venv runners today; other \
                          languages need their runner seam before their rows grade (#383's \
                          repo→runner map is the extension point, expressed as data). Tier-1 pick \
                          of the 2026-08-23 landscape sweep.",
            grader: Grader::Python,
            tasks: 731,
            eval_set: None,
            source_url: Some("https://huggingface.co/datasets/ScaleAI/SWE-bench_Pro"),
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
        "List the known benchmarks (name, grader, task count, whether it can be run \
         now). Use `benchmark/dispatch` with a name to post its tasks onto the work board.";
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
                    runnable: b.eval_set.is_some() || b.swe_dataset().is_some(),
                })
                .collect(),
        })
    }
}
crate::register_stateless_command!(BenchmarkList);

// ---- benchmark/record (the evidence ledger, #123) -----------------------
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
    #[ts(optional)]
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
         the EXACT replication command) into the evidence ledger. `benchmark/runs` reads them \
         back.";
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
    /// Optional when `recipe` is given (the recipe's rows carry the names).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub name: Option<String>,
    /// Execute a stored RECIPE by name instead of hand-assembled flags: a row
    /// in the `benchmark_recipes` collection (author with `data/create`)
    /// carrying the model to serve and the dispatches to fire. Dispatch pins
    /// the model (fit-gated), awaits lane readiness, then fires every entry —
    /// the whole experiment is two commands: `reboot` + `dispatch --recipe X`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub recipe: Option<String>,
    /// Parameters for a TEMPLATE recipe: every `{key}` placeholder in the
    /// row's string fields is substituted from this map before execution, so
    /// one recipe ("challenge: candidate takes the incumbent's misses") serves
    /// every model — the model is an ARGUMENT, never data baked into the row.
    /// Unresolved placeholders fail loud, naming the missing key.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "Record<string, string>")]
    pub params: Option<std::collections::BTreeMap<String, String>>,
    /// How many tasks (from the top) to post as cards. Omit for all.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
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
    /// Every name MUST be a citizen currently online (dispatch fails loud on an
    /// unknown name, listing who is online) — never our specific roster, always
    /// this machine's live citizens. OMIT to dispatch to the WHOLE live roster
    /// (whoever this repo user has spawned) — the general default.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub assignees: Option<Vec<String>>,
    /// Restrict a SWE-class dispatch to these exact `instance_id`s (e.g.
    /// `sympy__sympy-24152`), in this order — instead of taking the first `limit` from the
    /// dataset. Substring match, so a short id (`sympy-24152`) also selects. Omit to dispatch
    /// the dataset head. Lets a caller target a KNOWN-buildable instance rather than whatever
    /// sits first in the dataset (astropy's C-extension build is the hard tail, #383, and it
    /// leads swe-bench-lite). Ignored for gym-class benchmarks.
    #[serde(default)]
    pub instances: Option<Vec<String>>,
    /// Deterministic RANDOM SAMPLE: take this many instances chosen by `seed`
    /// instead of the dataset head. `(dataset, seed, sample)` fully determines
    /// the list on every machine — the flag pair IS the replication recipe, so
    /// publish both alongside the score. Combines with `limit` (sample wins),
    /// refused alongside explicit `instances`. SWE-class benchmarks only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub sample: Option<u32>,
    /// RNG seed for `sample` (default 0). Same LCG as the generated gyms — no
    /// platform rand, byte-stable selection forever.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub seed: Option<u64>,
    /// The room this run lives in. Omit to get a FRESH one per run, named
    /// `bench-<benchmark>-<epoch>`.
    ///
    /// **A run is an activity, and an activity is a room.** Before this existed,
    /// dispatch had no way to say where — so every suite, every run, forever, piled
    /// into whichever room the curator happened to be standing in. Measured on one
    /// 37-minute window of that pile: 136 cards with 66 already CLOSED and still
    /// resident, and 5,336 of 5,345 inbound events discarded as bookkeeping —
    /// ~48 wake-ups per minute per citizen, of which NINE in 37 minutes were
    /// something a mind could actually read. Claim heartbeats scale as
    /// `O(claims × citizens)`, so the more work a citizen held, the less capacity
    /// it had to do any of it.
    ///
    /// A fresh room per run makes the run's board its OWN denominator, lets the
    /// round END, and puts the assignees somewhere they can hear each other. Pass
    /// an explicit name to join an existing run (it must already exist — dispatch
    /// spawns a room it names, and never silently adopts a stranger's).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub room: Option<String>,
    /// Also CLOSE this benchmark's redundant duplicate cards, converging the board
    /// to one live card per task. Off by default — a dispatch that silently closed
    /// cards would be a surprising verb.
    ///
    /// A card someone is genuinely working (a live claim) is never closed; if two
    /// citizens hold the SAME task, both are kept and the contention is reported
    /// rather than resolved by cancelling one of them. Pair with `limit=0` to prune
    /// without dispatching anything new.
    #[serde(default)]
    pub prune: Option<bool>,
    /// Who works this round's cards: `detached_solve` (default) or `citizen`.
    ///
    /// - `detached_solve` — a forked copy of the citizen solves each card through
    ///   `agent/solve`, with an exclusive warm slot. Proven; it produced our one SWE
    ///   pass. It also produces no room turn, so the round teaches nobody (#456).
    /// - `citizen` — nothing detached fires. The kickoff drives her to claim, and she
    ///   works the card on her own held-work turn: hands rooted at the staged checkout,
    ///   acts radiating into the run room, and the turn feeding the training producer.
    ///
    /// The score and the learning are both the objective, and only `citizen` can
    /// deliver the second one — but it depends on the kickoff→claim hop that used to
    /// stall rounds, so it is opt-in until that hop is proven under residency.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub drive: Option<crate::cognition::bench_round::WorkDriver>,
    /// Stage the round even though serving is NOT decode-verified (#442).
    ///
    /// Off by default, and the default is the point: dispatch refuses to post cards no
    /// citizen can work, because a round staged into a dead lane looks dispatched and is
    /// inert (#455). This is the explicit operator override — same contract as
    /// `start --force` (#420) — and it announces itself in the log rather than passing
    /// silently, since a gate that can be skipped without a trace is not a gate.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub force: Option<bool>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkDispatchResult.ts"
)]
pub struct BenchmarkDispatchResult {
    pub benchmark: String,
    /// The room this run lives in — where its board, its kickoffs and its citizens are.
    /// Returned so the caller never has to guess where the work went.
    pub room: String,
    /// That room's airc channel id — the TYPE, not a uuid-shaped string. See
    /// `ActivitySpawnResult::room_id`; this field is that value passed through.
    #[ts(type = "string")]
    pub room_id: airc_core::RoomId,
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
    /// Instances withheld because THIS box already proved their (repo, era) env
    /// class red via `benchmark/validate`. Reported, never silent — the operator
    /// must see that the round is smaller than requested and WHY (the named wall
    /// rides in `kickoff_errors`).
    #[serde(default)]
    pub skipped_known_red: u32,
    /// Tasks NOT dispatched because a LIVE card for that exact task is already on
    /// the board (same `[bench <name>] <task_id>:` key, in any non-terminal state).
    /// Dispatch is idempotent per task: re-running it tops the board up to one card
    /// per task instead of posting a second copy.
    ///
    /// Why this exists: without it, every re-dispatch re-posted the dataset head as
    /// brand-new cards. Measured on the live board 2026-08-13 — 124 bench cards for
    /// 51 distinct tasks, `sympy__sympy-24152` alone holding 15 copies, with two
    /// citizens solving the SAME instance in parallel. That wastes scarce lanes and
    /// leaves the pass rate with no honest denominator.
    #[ts(type = "number")]
    pub skipped_already_on_board: u32,
    /// Redundant duplicate cards CLOSED by this call (only when `prune` was set).
    /// Cards under a live claim are never counted here because they are never
    /// closed — see `contended_tasks`.
    #[ts(type = "number")]
    pub pruned_duplicates: u32,
    /// Tasks where MORE THAN ONE citizen holds a live claim on a duplicate card.
    /// The prune leaves all of them alone: cancelling one would destroy real
    /// in-flight work, so this is surfaced as a coordination fact for the room to
    /// settle rather than resolved silently.
    #[ts(type = "number")]
    pub contended_tasks: u32,
    /// Addressed kickoff messages actually delivered (one per dispatched card —
    /// every card is directed at a live citizen). A kickoff that failed to send is
    /// reported via `kickoff_errors`, never silently counted as delivered.
    #[ts(type = "number")]
    pub kickoffs: u32,
    /// Scored solves FIRED directly for directed SWE assignees — the loop starting work
    /// without waiting on a cognitive `work/claim` (the hop that stalls under warm-slot
    /// starvation). One per staged directed SWE card. `dispatched - solves_fired` are the
    /// gym / undirected cards that still start on organic claim.
    #[ts(type = "number")]
    pub solves_fired: u32,
    /// Send failures for kickoff messages, card-id-prefixed. The cards are ON
    /// the board regardless — a failed kickoff is reported, not unwound; the
    /// citizen can still find and claim the card off the board.
    pub kickoff_errors: Vec<String>,
}

/// The IDENTITY of a benchmark card: which task of which benchmark it is.
///
/// This is the ONE definition of "same task" on the board, and it is a strict
/// prefix of the rendered title — so the key a dispatch computes and the key
/// parsed back off a live card can never drift (pinned by
/// `a_rendered_title_yields_back_its_own_key`). `dispatch_card_title` builds on
/// it rather than re-forming the marker, per the compression rule: one logical
/// decision, one place.
pub(crate) fn dispatch_card_key(bench: &str, task_id: &str) -> String {
    format!("[bench {bench}] {task_id}:")
}

/// Recover the identity key from a rendered card title, or `None` when the
/// title is not a benchmark card at all (a hand-written card on the same board
/// must never collide with a task key). Reads the prefix through the FIRST `:`
/// after the `]` marker — a gist containing colons cannot widen the key.
pub(crate) fn bench_card_key(title: &str) -> Option<&str> {
    if !title.starts_with("[bench ") {
        return None;
    }
    let marker_end = title.find("] ")? + 2;
    let colon = title[marker_end..].find(':')? + marker_end;
    Some(&title[..=colon])
}

/// Which of a task's duplicate cards to CLOSE, and whether the duplication is
/// contended. Pure over the claim states so the rule is testable without a
/// board — the caller maps the returned indices back to cards.
///
/// The rule, in priority order:
///  • A card someone is genuinely ON (`Hold::Held`) is NEVER closed. Duplicates
///    are board litter; an in-flight claim is a citizen's work, and destroying
///    it to tidy up would cost more than the duplication does.
///  • If no card is held, keep the FIRST and close the rest — they are
///    interchangeable, so the choice only has to be deterministic.
///  • If MORE THAN ONE card is held, every held card is kept and the caller is
///    told (`contended`). Two citizens really are on the same task; that is a
///    coordination fact for them to settle, not something a prune should
///    silently resolve by cancelling someone.
fn duplicates_to_close(holds: &[crate::persona::card_holder::Hold]) -> (Vec<usize>, bool) {
    use crate::persona::card_holder::Hold;
    if holds.len() <= 1 {
        return (Vec::new(), false);
    }
    let held: Vec<usize> = holds
        .iter()
        .enumerate()
        .filter(|(_, h)| matches!(h, Hold::Held))
        .map(|(i, _)| i)
        .collect();
    let to_close = if held.is_empty() {
        (1..holds.len()).collect()
    } else {
        (0..holds.len()).filter(|i| !held.contains(i)).collect()
    };
    (to_close, held.len() > 1)
}

/// Compose the card TITLE for one benchmark task. `[bench <name>]` is the
/// machine-findable marker the (future) grading sentinel keys on; the rest is
/// for the citizen scanning the board.
pub(crate) fn dispatch_card_title(bench: &str, task_id: &str, prompt: &str) -> String {
    let gist: String = prompt.chars().take(60).collect();
    let ellipsis = if prompt.chars().count() > 60 {
        "…"
    } else {
        ""
    };
    format!("{} {gist}{ellipsis}", dispatch_card_key(bench, task_id))
}

/// Compose the card BODY: the full prompt plus a definition of done a citizen
/// can act on with her own hands. Grading inputs that must stay held out
/// (`expect`, the harness `test`) are deliberately NOT written to the card —
/// the DoD names the artifact, not the answer key. `solution_file` and
/// `dod_shell` are legitimately visible: real work has a visible definition
/// of done.
pub(crate) fn dispatch_card_body(bench: &str, t: &crate::cognition::eval::EvalTask) -> String {
    let mut body = format!(
        "benchmark: {bench}\ntask: {}\n\n{}\n",
        t.id,
        t.prompt.trim()
    );
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

/// Compose the card BODY for a REAL SWE-bench project instance — a live open-source issue
/// on a real repo. The issue text plus a definition of done a citizen acts on with her own
/// git hands. The gold `patch` and the held-out `test_patch` are deliberately NOT written:
/// the card names the repo + commit + which tests must pass, never the answer key (same
/// held-out discipline as [`dispatch_card_body`]). This is the real-project tier — "full
/// projects, not stupid homework" (Joel, 2026-08-10).
pub(crate) fn dispatch_swe_card_body(
    bench: &str,
    i: &crate::cognition::swe_bench::SweInstance,
) -> String {
    let f2p = i.f2p();
    let tests = if f2p.is_empty() {
        "the repo's held-out failing tests".to_string()
    } else {
        f2p.join(", ")
    };
    format!(
        "benchmark: {bench}\ninstance: {}\nrepo: {} @ {}\n\n{}\n\n\
         This is a REAL open-source issue. When you CLAIM this card, the repo is already \
         staged in your workspace at `swe/{}/` (checked out at the buggy commit) and your \
         scored solve starts automatically — fix the bug IN PLACE in that checkout. Definition \
         of done: these tests pass — {}. Your DIFF is graded against the repo's held-out test \
         suite; do not edit the tests.",
        i.instance_id,
        i.repo,
        i.base_commit,
        i.problem_statement.trim(),
        i.instance_id,
        tests,
    )
}

/// `benchmark/dispatch` — post a benchmark's tasks as claimable work cards on
/// the shared board. Citizens claim and work them through the SAME kanban loop
/// as any other work; nothing about the turn is exam-shaped.
pub struct BenchmarkDispatch {
    pub registry: crate::persona::PersonaAircRuntimeRegistry,
    /// Late-bound substrate executor (ChatModule pattern) — the recipe path
    /// composes `data/list` (load the recipe row) and `serving/pin` (re-home
    /// the lane, with pin.rs's full fit-gating) as COMMANDS, the universal
    /// primitive, never cross-module state threading.
    pub executor_slot: std::sync::Arc<
        crate::runtime::LateBound<crate::runtime::command_executor::CommandExecutor>,
    >,
}

/// A stored EXPERIMENT — one row in the `benchmark_recipes` collection,
/// authored through the data layer (`data/create`), executed by
/// `benchmark/dispatch --recipe <name>`. DATA, not code: fields grow
/// (serde-tolerant) without touching this file, and the sophistication lives
/// in the row — which model to serve, which dispatches to fire, and later
/// caps, condition labels, team shapes. Nothing model-specific is ever
/// hardcoded here; serving behavior comes from the model's own catalog row.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkRecipe {
    /// The name `--recipe` selects.
    pub name: String,
    /// One line of intent, shown when listing/erroring.
    #[serde(default)]
    pub description: String,
    /// Model this run must be SERVING before any card fires. `None` = run on
    /// whatever is live. `Some` = dispatch pins it (fit-gated by serving/pin)
    /// and awaits lane readiness before the first card.
    #[serde(default)]
    pub model_id: Option<String>,
    /// The dispatches to fire, in order — one experiment may span datasets.
    pub dispatches: Vec<RecipeDispatch>,
}

/// One dispatch inside a recipe: a benchmark plus the exact instances.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecipeDispatch {
    /// Benchmark name (see `benchmark/list`).
    pub benchmark: String,
    /// Exact instance ids. Empty = the dataset head up to `limit`.
    #[serde(default)]
    pub instances: Vec<String>,
    /// Optional per-dispatch card cap.
    #[serde(default)]
    pub limit: Option<u32>,
}

/// Resolve the citizens a directed dispatch addresses — GENERALIZED for any repo user's
/// roster, never our specific names. Pure over the snapshots so it is unit-testable
/// without a running airc daemon (a real `PersonaSlot` needs one); the wrapper in `run`
/// feeds `registry.resident_snapshot()` and `registry.roster_snapshot()` in.
///
/// - `requested` empty → the WHOLE RESIDENT roster (whoever THIS machine has in the room).
///   This is the "dispatch to my citizens, whoever they are" default: a fresh clone runs
///   `benchmark/dispatch --name=…` with no `--assignees` and it targets their own resident
///   citizens. Directed dispatch is what actuates (a silent card does not — measured
///   2026-08-07), so defaulting to the resident roster keeps the loop autonomous everywhere.
/// - `requested` non-empty → every name MUST resolve to a RESIDENT citizen; anything else
///   FAILS LOUD listing who is resident (never silently addresses a citizen who cannot
///   hear, and never silently skips SWE staging). Order is preserved for round-robin.
/// - nobody resident → `Denied`, and the message distinguishes "not spawned" (fix:
///   `persona/spawn`) from "spawned but not hosted yet" (fix: wait — see below).
/// Seconds since the epoch — the only impurity `default_run_room_name` needs, kept out
/// of it so the name itself is a pure function with a real unit test.
fn epoch_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// The room a run lands in when the caller names none: `bench-<benchmark>-<epoch>`.
///
/// Named for the ACTIVITY and stamped so it is THIS run — the naming rule
/// `activity/spawn` documents, and the reason it matters here specifically: a room named
/// for a SUBSYSTEM (`#academy`, `#benchmarks`) never finishes, so it reads as a permanent
/// place and quietly becomes the room every run reuses forever. That is precisely the
/// 136-card pile this parameter exists to end.
///
/// Flattened with `-` rather than the `academy/bench/<run>` path form the design of record
/// uses, because airc channel names accept only `[a-z0-9_-]` (`ChannelName::new` rejects
/// `/`). The tree is a naming convention waiting on a channel-name grammar, not something
/// this function can invent unilaterally.
fn default_run_room_name(benchmark: &str, epoch_secs: u64) -> String {
    let slug: String = benchmark
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    format!("bench-{slug}-{epoch_secs}")
}

/// `live` is the RESIDENT snapshot (service loop attached and running), NOT the registered
/// roster — `registered` is passed alongside it purely so a refusal can tell the operator
/// WHICH of the two states they are in. That distinction is the whole point:
///
/// - registered ∧ ¬resident → she exists but has no perception stream. Hosting is waiting
///   on something (usually a serving lane proving it can decode, #363). Work staged now is
///   posted into an empty room and never worked.
/// - ¬registered → nobody spawned her. The fix is `persona/spawn`, a different action.
///
/// Measured 2026-08-18: dispatching against the REGISTERED roster in the first state
/// reported `dispatched: 2, kickoffs: 2, kickoff_errors: []` and produced zero turns.
fn resolve_dispatch_roster(
    live: &[(String, uuid::Uuid)],
    registered: &[(String, uuid::Uuid)],
    requested: &[String],
) -> Result<Vec<(String, uuid::Uuid)>, CommandError> {
    if live.is_empty() {
        if registered.is_empty() {
            return Err(CommandError::Denied(
                "no citizens are online to work the cards — spawn a persona (persona/spawn) \
                 first, then dispatch."
                    .to_string(),
            ));
        }
        let names: Vec<&str> = registered.iter().map(|(n, _)| n.as_str()).collect();
        return Err(CommandError::Denied(format!(
            "citizen(s) [{}] are registered but NOT RESIDENT — no service loop, so no \
             perception stream, so nothing dispatched here would be heard. Hosting is \
             normally waiting on the serving lane to prove it can decode; watch \
             `inference.lane_relaunch_retry` and `persona.inbound.subscribe_opened`, and \
             re-dispatch once `persona/roster` reports resident_count > 0. Staging a round \
             into this window posts cards nobody can see.",
            names.join(", "),
        )));
    }
    if requested.is_empty() {
        return Ok(live.to_vec());
    }
    let mut resolved = Vec::with_capacity(requested.len());
    let mut unknown = Vec::new();
    for name in requested {
        match live.iter().find(|(n, _)| n == name) {
            Some(pair) => resolved.push(pair.clone()),
            None => unknown.push(name.clone()),
        }
    }
    if !unknown.is_empty() {
        let online: Vec<&str> = live.iter().map(|(n, _)| n.as_str()).collect();
        // Name the registered-but-not-resident case separately: "not online" reads as a
        // typo, and sending an operator hunting for a misspelling when the real answer is
        // "she is here but not hosted yet" is the same lie in a smaller box.
        let not_resident: Vec<&str> = unknown
            .iter()
            .filter(|n| registered.iter().any(|(r, _)| r == *n))
            .map(|s| s.as_str())
            .collect();
        let residency_note = if not_resident.is_empty() {
            String::new()
        } else {
            format!(
                " NOTE: [{}] are registered but not resident — they exist, they just have \
                 no service loop yet (hosting is likely waiting on a serving lane). That \
                 is a wait, not a typo.",
                not_resident.join(", ")
            )
        };
        return Err(CommandError::Invalid(format!(
            "assignee(s) not resident: {}. Citizens resident right now: [{}]. Pass names \
             from that list, or omit --assignees to dispatch to all of them.{}",
            unknown.join(", "),
            online.join(", "),
            residency_note,
        )));
    }
    Ok(resolved)
}

impl BenchmarkDispatch {
    /// The substrate executor, or a loud error naming the wiring gap — never a
    /// silent no-op (the slot is installed by `install_executor_on_all` at boot).
    fn executor(
        &self,
    ) -> Result<std::sync::Arc<crate::runtime::command_executor::CommandExecutor>, CommandError>
    {
        self.executor_slot.get().cloned().ok_or_else(|| {
            CommandError::Internal(
                "command executor not installed on benchmark/dispatch — boot wiring gap".into(),
            )
        })
    }

    /// Load a recipe row from the `benchmark_recipes` collection via `data/list`
    /// — the recipe is DATA authored with `data/create`, never code.
    async fn load_recipe(
        &self,
        name: &str,
        params: &std::collections::BTreeMap<String, String>,
    ) -> Result<BenchmarkRecipe, CommandError> {
        let exec = self.executor()?;
        let out = exec
            .execute(
                "data/list",
                serde_json::json!({
                    "collection": "benchmark_recipes",
                    "filter": { "name": name },
                    "limit": 1,
                }),
            )
            .await
            .map_err(|e| CommandError::Internal(format!("data/list failed: {e}")))?;
        let crate::runtime::CommandResult::Json(v) = out else {
            return Err(CommandError::Internal(
                "data/list returned a non-JSON result".into(),
            ));
        };
        let item = v
            .get("items")
            .and_then(|i| i.as_array())
            .and_then(|a| a.first())
            .cloned()
            .ok_or_else(|| {
                CommandError::NotFound(format!(
                    "no recipe named '{name}' in `benchmark_recipes` — author one with \
                     data/create (fields: name, description, model_id?, dispatches: \
                     [{{benchmark, instances[], limit?}}])"
                ))
            })?;
        let item = Self::instantiate_recipe(item, params)?;
        serde_json::from_value::<BenchmarkRecipe>(item).map_err(|e| {
            CommandError::Invalid(format!(
                "recipe '{name}' exists but does not parse as a BenchmarkRecipe: {e}"
            ))
        })
    }

    /// TEMPLATE → INSTANCE: substitute `{key}` placeholders in every string of
    /// the recipe row from the caller's params (the recipe-doctrine split — a
    /// recipe is a reusable template; the invocation supplies the specifics).
    /// Pure so it is unit-testable; unresolved placeholders are a loud error
    /// naming the key, never a silently-literal "{model}" reaching dispatch.
    fn instantiate_recipe(
        mut row: serde_json::Value,
        params: &std::collections::BTreeMap<String, String>,
    ) -> Result<serde_json::Value, CommandError> {
        fn walk(
            v: &mut serde_json::Value,
            params: &std::collections::BTreeMap<String, String>,
            missing: &mut Vec<String>,
        ) {
            match v {
                serde_json::Value::String(s) => {
                    if s.contains('{') {
                        let mut out = s.clone();
                        for (k, val) in params {
                            out = out.replace(&format!("{{{k}}}"), val);
                        }
                        if let (Some(a), Some(b)) = (out.find('{'), out.find('}')) {
                            if a < b {
                                missing.push(out[a + 1..b].to_string());
                            }
                        }
                        *s = out;
                    }
                }
                serde_json::Value::Array(a) => a.iter_mut().for_each(|x| walk(x, params, missing)),
                serde_json::Value::Object(o) => {
                    o.values_mut().for_each(|x| walk(x, params, missing))
                }
                _ => {}
            }
        }
        let mut missing = Vec::new();
        walk(&mut row, params, &mut missing);
        if missing.is_empty() {
            Ok(row)
        } else {
            missing.sort();
            missing.dedup();
            Err(CommandError::Invalid(format!(
                "recipe placeholders unresolved: {{{}}} — pass them via --params",
                missing.join("}, {")
            )))
        }
    }

    /// Bring the lane to the recipe's model BEFORE any card fires: pin via the
    /// `serving/pin` COMMAND (its fit-gate refuses loud), then await readiness
    /// on the daemon's own snapshot — bounded, probed, never a silent hang.
    /// The lane's readiness smoke probe doubles as the first-request warmup
    /// models with a serving contract require.
    async fn ensure_recipe_model(&self, recipe: &BenchmarkRecipe) -> Result<(), CommandError> {
        let Some(model_id) = recipe.model_id.as_deref() else {
            return Ok(()); // recipe runs on whatever is live
        };
        let live = crate::inference::llama_server::current_serving();
        if live.ready && live.active_model.as_deref() == Some(model_id) {
            return Ok(());
        }
        let exec = self.executor()?;
        exec.execute("serving/pin", serde_json::json!({ "model_id": model_id }))
            .await
            .map_err(|e| {
                CommandError::Denied(format!(
                    "recipe names model '{model_id}' but serving/pin refused: {e}"
                ))
            })?;
        // Await the swap. Generous bound: a cold multi-shard load is minutes.
        const RECIPE_SERVE_DEADLINE: std::time::Duration =
            std::time::Duration::from_secs(15 * 60);
        let started = std::time::Instant::now();
        loop {
            if let Some(s) = crate::inference::llama_server::await_ready_serving(
                std::time::Duration::from_secs(30),
            )
            .await
            {
                if s.ready && s.active_model.as_deref() == Some(model_id) {
                    crate::probe!(
                        class = "benchmark.recipe.model_ready",
                        model = model_id,
                        waited_ms = started.elapsed().as_millis() as u64,
                        "recipe's model is serving — dispatch proceeds"
                    );
                    return Ok(());
                }
            }
            if started.elapsed() > RECIPE_SERVE_DEADLINE {
                return Err(CommandError::Internal(format!(
                    "pinned '{model_id}' but it did not become the ready served model within \
                     {}s — check serving/status and the lane log",
                    RECIPE_SERVE_DEADLINE.as_secs()
                )));
            }
            crate::probe!(
                class = "benchmark.recipe.awaiting_model",
                model = model_id,
                waited_ms = started.elapsed().as_millis() as u64,
                "recipe model not ready yet — still awaiting the lane swap"
            );
        }
    }
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
        use crate::modules::work::curator_airc;
        use airc_lib::{CreateWorkCard, Priority, RepoId};

        // ── RECIPE PATH: the whole experiment by name ──────────────────────
        // `--recipe X` loads a `benchmark_recipes` row (data, not code), pins
        // the model it names (fit-gated by serving/pin), awaits lane readiness,
        // then fires every dispatch entry through THIS SAME verb. The design
        // from the start: two primitives, commands composing commands.
        if let Some(recipe_name) = p.recipe.clone() {
            if p.name.is_some() || p.instances.is_some() {
                return Err(CommandError::Invalid(
                    "pass either --recipe OR --name/--instances — the recipe row carries                      its own dispatches"
                        .into(),
                ));
            }
            let recipe = self
                .load_recipe(&recipe_name, &p.params.clone().unwrap_or_default())
                .await?;
            self.ensure_recipe_model(&recipe).await?;
            let mut agg: Option<BenchmarkDispatchResult> = None;
            for d in &recipe.dispatches {
                let sub = BenchmarkDispatchParams {
                    name: Some(d.benchmark.clone()),
                    recipe: None,
                    instances: if d.instances.is_empty() {
                        None
                    } else {
                        Some(d.instances.clone())
                    },
                    limit: d.limit.or(p.limit),
                    ..p.clone()
                };
                let r = Box::pin(self.run(ctx, sub)).await?;
                agg = Some(match agg.take() {
                    None => r,
                    Some(mut a) => {
                        a.dispatched += r.dispatched;
                        a.card_ids.extend(r.card_ids);
                        a.skipped_needs_setup += r.skipped_needs_setup;
                        a.skipped_known_red += r.skipped_known_red;
                        a.skipped_already_on_board += r.skipped_already_on_board;
                        a.pruned_duplicates += r.pruned_duplicates;
                        a.contended_tasks += r.contended_tasks;
                        a.kickoffs += r.kickoffs;
                        a.solves_fired += r.solves_fired;
                        a.kickoff_errors.extend(r.kickoff_errors);
                        a.benchmark = format!("recipe:{recipe_name}");
                        a
                    }
                });
            }
            return agg.ok_or_else(|| {
                CommandError::Invalid(format!(
                    "recipe '{recipe_name}' has no dispatches — author at least one"
                ))
            });
        }
        let name = p.name.clone().ok_or_else(|| {
            CommandError::Invalid(
                "pass --name <benchmark> (see benchmark/list) or --recipe <name>".into(),
            )
        })?;

        let spec = known_benchmarks()
            .iter()
            .find(|b| b.name == name)
            .ok_or_else(|| {
                CommandError::Invalid(format!(
                    "unknown benchmark '{}' — see benchmark/list",
                    name
                ))
            })?;
        // Two shapes of dispatchable benchmark, ONE card loop below. A `PreparedCard`
        // normalizes both so create + kickoff is source-agnostic:
        //  • gym collections resolve a committed JSONL of `EvalTask`s (write→compile→test);
        //  • SWE-bench-INSTANCE collections pull real GitHub-issue PROJECTS via the proven
        //    `swe_bench` loader — the real-project tier "the frontier models fight over"
        //    (Joel, 2026-08-10). Each instance becomes a clone-and-fix card.
        enum CardWork {
            /// Gym: write a solution file; DoD is a compile/test shell.
            Gym { solution_file: String },
            /// SWE: the pulled instance. Dispatch STAGES it into the directed assignee's
            /// workspace/swe/<instance> so her claim auto-fires the scored solve (#346's
            /// dispatch_staged_swe_solve) — the loop closes with nobody in it.
            Swe {
                instance: Box<crate::cognition::swe_bench::SweInstance>,
            },
        }
        struct PreparedCard {
            title: String,
            body: String,
            work: CardWork,
            /// Gym-only: a task needing a workspace re-break the card can't orchestrate yet.
            needs_setup: bool,
            /// The gym task's workspace-preparation shell, staged into the ASSIGNEE's
            /// workspace at dispatch (same contract as SWE checkout staging below) --
            /// idempotent by adapter convention (mkdir -p + overwrite-decode).
            setup_shell: Option<String>,
        }

        let prepared: Vec<PreparedCard> = if let Some(dataset) = spec.swe_dataset() {
            // Real-project tier: pull instances on demand (cached on first use), one
            // full-project card each. Reuses the SAME loader agent/solve grades against —
            // no second source of truth. THIS is what killed the "no runnable eval_set"
            // refusal that had blocked every SWE dispatch (Joel: "fix the goddamn thing").
            let mut instances = crate::cognition::swe_bench::load_dataset(dataset)
                .await
                .map_err(|e| {
                    CommandError::Internal(format!("swe dataset '{dataset}' load failed: {e}"))
                })?;
            // Deterministic sample: (dataset, seed, n) → the same list on every
            // machine. Fisher-Yates over the dataset order with the shared LCG —
            // the command IS the replication recipe (no operator-side scripts).
            if let Some(n) = p.sample.filter(|n| *n > 0) {
                if p.instances.as_ref().is_some_and(|w| !w.is_empty()) {
                    return Err(CommandError::Invalid(
                        "pass either `sample` (seeded random) or `instances` (explicit list),                          not both — they are competing selection recipes"
                            .into(),
                    ));
                }
                let n = (n as usize).min(instances.len());
                let mut rng = crate::cognition::gym_rng::Lcg::new(p.seed.unwrap_or(0)); // documented default seed: 0 is part of the replication contract, not a guess
                for i in 0..n {
                    let j = i + rng.next(instances.len() - i);
                    instances.swap(i, j);
                }
                instances.truncate(n);
                let list: Vec<&str> =
                    instances.iter().map(|i| i.instance_id.as_str()).collect();
                tracing::info!(
                    probe_class = "benchmark.dispatch.sample",
                    dataset,
                    n,
                    seed = p.seed.unwrap_or(0), // same documented default as the draw above
                    instances = ?list,
                    "seeded sample selected — publish (dataset, seed, n) with the score"
                );
            }
            // Caller-targeted instances win over dataset order — select by substring (so a
            // short id resolves) and preserve the CALLER's ordering, fail loud on a miss so a
            // typo never silently dispatches the wrong (or whole) set.
            if let Some(wanted) = p.instances.as_ref().filter(|w| !w.is_empty()) {
                let mut picked: Vec<crate::cognition::swe_bench::SweInstance> = Vec::new();
                for want in wanted {
                    match instances
                        .iter()
                        .position(|i| i.instance_id.contains(want.as_str()))
                    {
                        Some(idx) => picked.push(instances.remove(idx)),
                        None => {
                            return Err(CommandError::Invalid(format!(
                                "no instance in '{dataset}' matches '{want}' — check the id (e.g. sympy__sympy-24152)"
                            )));
                        }
                    }
                }
                instances = picked;
            }
            // ENV PRE-WARM (background): a cold native build (scikit's cython,
            // matplotlib's freetype) mid-round burns a solve attempt on an ENV
            // failure and reads as a model miss. Build every instance's env
            // AHEAD of the driver, in REVERSE card order so the warmer and the
            // solver approach from opposite ends (ensure_env itself holds the
            // per-instance lock, so even a meeting in the middle is safe).
            // Fire-and-forget: a prewarm failure is probed — the SAME failure
            // the solve would hit, surfaced hours earlier and attributable.
            {
                let mut warm = instances.clone();
                warm.reverse();
                tokio::spawn(async move {
                    for inst in warm {
                        let checkout =
                            match crate::cognition::swe_bench::ensure_grade_checkout(&inst).await {
                                Ok(dir) => dir,
                                Err(e) => {
                                    crate::probe!(
                                        class = "benchmark.env.prewarm_failed",
                                        instance = %inst.instance_id,
                                        stage = "checkout",
                                        error = %e,
                                        "env pre-warm could not stage a checkout — the solve \
                                         will hit this same wall; this is an ENV failure, not \
                                         a model result"
                                    );
                                    continue;
                                }
                            };
                        match crate::cognition::swe_bench::ensure_env(&inst, &checkout).await {
                            Ok(_) => crate::probe!(
                                class = "benchmark.env.prewarmed",
                                instance = %inst.instance_id,
                                "env ready ahead of the driver"
                            ),
                            Err(e) => crate::probe!(
                                class = "benchmark.env.prewarm_failed",
                                instance = %inst.instance_id,
                                stage = "env",
                                error = %e,
                                "env pre-warm FAILED — the solve will hit this same wall; \
                                 an ENV failure, never a model result"
                            ),
                        }
                    }
                });
            }
            instances
                .into_iter()
                .map(|i| PreparedCard {
                    title: dispatch_card_title(spec.name, &i.instance_id, &i.problem_statement),
                    body: dispatch_swe_card_body(spec.name, &i),
                    needs_setup: false,
                    setup_shell: None,
                    work: CardWork::Swe {
                        instance: Box::new(i),
                    },
                })
                .collect()
        } else {
            let reference = spec.eval_set.ok_or_else(|| {
                CommandError::Invalid(format!(
                    "benchmark '{}' has no runnable eval_set yet — it is catalogued but its \
                     task collection hasn't been pulled/committed (see benchmark/list `runnable`)",
                    name
                ))
            })?;
            // Same fail-loud task loading as cognition/eval: the committed gym resolves
            // from the embedded registry, a malformed line names itself.
            let (origin, text) =
                crate::cognition::gym::resolve_gym(reference).map_err(CommandError::Invalid)?;
            text.lines()
                .enumerate()
                .map(|(i, l)| (i + 1, l.trim()))
                .filter(|(_, l)| !l.is_empty())
                .map(|(n, l)| {
                    let mut t: EvalTask = serde_json::from_str(l).map_err(|e| {
                        CommandError::Invalid(format!("{origin} line {n}: malformed EvalTask: {e}"))
                    })?;
                    // Title gist comes from the AUTHORED prompt: require_hands_for_code
                    // prepends the same write-and-verify preamble to every code task, and a
                    // board of 12 cards all titled "Implement the following, and VERIFY…"
                    // is unscannable for the citizen AND breaks dispatch_card_key parsing.
                    let headline = t.prompt.clone();
                    // THE artifact rule — the same normalization cognition/eval applies at
                    // load. Before this, the card body named NO file (the gym rows carry no
                    // solution_file) while the grade read the derived one: the citizen was
                    // graded against a path she was never told. One derivation, both readers.
                    t.require_hands_for_code();
                    let solution_file = t
                        .solution_file
                        .clone()
                        .unwrap_or_else(|| format!("{}.rs", t.id));
                    Ok(PreparedCard {
                        title: dispatch_card_title(spec.name, &t.id, &headline),
                        body: dispatch_card_body(spec.name, &t),
                        needs_setup: t.setup_shell.is_some(),
                        setup_shell: t.setup_shell.clone(),
                        work: CardWork::Gym { solution_file },
                    })
                })
                .collect::<Result<_, CommandError>>()?
        };

        let requested = p.assignees.clone().unwrap_or_default();
        if requested.iter().any(|a| a.trim().is_empty()) {
            return Err(CommandError::Invalid(
                "assignees contains an empty name — every kickoff must address a real citizen"
                    .to_string(),
            ));
        }
        // #442 (roster half) + #412 + #455: a dispatch fired inside the post-boot resume
        // window used to find an EMPTY roster and refuse instantly — so the operator
        // hand-rolled a sleep-loop around dispatch (run by hand twice on 2026-08-17; a
        // runbook line is a design defect). The serving half of #442 already parks
        // (`await_ready_serving` below); the roster half parks the same way, bounded.
        //
        // #455 is what this loop keys on NOW: RESIDENCY, not registration. Waiting for the
        // roster to be non-empty released the wait ~10 minutes too early (#412) — citizens
        // are registered, presence-pumping and renewing claims long before the supervisor
        // attaches a service loop, so the old condition cleared while nobody could hear a
        // thing. Measured 2026-08-18: roster listed 2, `subscribe_opened` was 0, a whole
        // round went onto the board and produced zero turns.
        //
        // An unknown NAME against a RESIDENT roster still fails fast — that error means a
        // typo, never a resume in progress.
        const ROSTER_RESUME_WAIT: std::time::Duration = std::time::Duration::from_secs(180);
        const ROSTER_RESUME_POLL: std::time::Duration = std::time::Duration::from_secs(5);
        let wait_started = std::time::Instant::now();
        let mut resident = self.registry.resident_snapshot().await;
        while resident.is_empty() && wait_started.elapsed() < ROSTER_RESUME_WAIT {
            tracing::info!(
                waited_s = wait_started.elapsed().as_secs(),
                registered = self.registry.roster_snapshot().len(),
                probe_class = "benchmark.dispatch.awaiting_residency",
                "dispatch: no RESIDENT citizen yet (registered != in the room, #412/#455) \
                 — waiting for a service loop rather than staging into an empty room"
            );
            tokio::time::sleep(ROSTER_RESUME_POLL).await;
            resident = self.registry.resident_snapshot().await;
        }

        // Curator seed — resolved AFTER the residency park on purpose. It authors
        // through a live citizen when the operator has no self-peer (#27), and it
        // used to run BEFORE the park, so a dispatch fired inside the post-boot
        // window refused instantly with "none are online" while the 180s wait that
        // exists precisely for that window sat unreachable 30 lines below
        // (measured live 2026-08-26). Order: wait for a citizen, then author.
        let airc = curator_airc(&self.registry, ctx, "benchmark/dispatch")?;

        // Resolve the dispatch roster against THIS machine's live citizens (never our
        // names): empty request → the whole live roster; explicit names → validated or
        // fail-loud. This is the generalization for all repo users — dispatch targets the
        // citizens they actually spawned, whoever those are.
        //
        // Resolved BEFORE the room exists because the roster decides WHO gets moved into
        // it: a run room nobody is standing in is the other half of the bug this verb is
        // fixing ("old rooms flooded, or ones with nothing").
        let roster =
            resolve_dispatch_roster(&resident, &self.registry.roster_snapshot(), &requested)?;

        // Repo hint, read from the board the curator is standing in RIGHT NOW — before we
        // move her, and ONLY when the caller named no repo. The run room is fresh, so its
        // board is empty and cannot answer "what repo key do cards use here"; the room she
        // came from can. Keeps `repo` optional exactly as before, and is the ONLY thing the
        // old room still contributes to a run.
        let repo_hint = match &p.repo {
            Some(_) => None,
            None => airc
                .work_board_complete(airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
                .await
                .ok()
                .and_then(|b| {
                    b.snapshot()
                        .cards
                        .first()
                        .map(|c| c.repo.as_str().to_string())
                }),
        };

        // ── THE RUN'S OWN ROOM ────────────────────────────────────────────────────
        // A benchmark run is an ACTIVITY, and an activity is a ROOM. Spawned through the
        // same `activity/spawn` path a citizen uses, so the room carries its recipe binding
        // and projects as a benchmark rather than a plain chat — a hand-made room would
        // carry neither ([[benchmarks-must-be-positronic-activities-not-a-parallel-subsystem]]).
        //
        // The `join` inside also MOVES the curator's current-room pointer, which is exactly
        // what makes the rest of this function land in the run room: the board read, every
        // `create_work_card`, and every kickoff `say` are all current-room operations. That
        // pointer move is a documented gap for other callers (activity.rs) and the mechanism
        // for this one.
        let room_name = match &p.room {
            Some(r) => r.trim().to_string(),
            None => default_run_room_name(spec.name, epoch_secs()),
        };
        // The room binds to the SHIPPED benchmark recipe's declared purpose,
        // resolved from its constant — never a re-typed string. The old literal
        // here was "benchmark" while the recipe declares "benchmark/hard-rs",
        // so every run room resolved to no manifest and rendered as plain chat
        // (#431 — the scoreboard region was the whole point of the recipe).
        let bench_recipe = crate::experience::source::RecipeExperienceSource::shipped_purpose(
            crate::experience::source::shipped::BENCHMARK_HARD_RS,
        )
        .ok_or_else(|| {
            CommandError::Internal(
                "shipped benchmark recipe missing from the embedded set — \
                 build-time authoring bug"
                    .into(),
            )
        })?;
        // The run's REAL targeting rides the binding (#433): suite is the
        // resolved spec's name, instances the caller's explicit selection,
        // team the resolved roster. Anything not set here (budget) stays at
        // the recipe's declared default — the binding is the room's honest
        // self-description, readable through the same pipe as everything else.
        let mut run_params = std::collections::BTreeMap::new();
        run_params.insert("suite".to_string(), serde_json::json!(spec.name));
        if let Some(instances) = &p.instances {
            run_params.insert("instances".to_string(), serde_json::json!(instances));
        }
        run_params.insert(
            "team".to_string(),
            serde_json::json!(roster.iter().map(|(who, _)| who).collect::<Vec<_>>()),
        );
        let room = crate::modules::activity::spawn_activity_room(
            &airc,
            &room_name,
            &bench_recipe,
            None,
            &run_params,
        )
        .await?;

        // Move every assignee INTO the run — a citizen who is not subscribed never sees the
        // board, the kickoff, or the peers working beside her. This is the members[] half
        // of #274, done for the one activity that needs it most.
        let mut room_join_errors: Vec<String> = Vec::new();
        for (who, peer) in &roster {
            match self.registry.get(*peer) {
                Some(rt) => {
                    // join_room, not airc().join: it bumps the membership epoch so
                    // her LIVE perception stream re-opens with the new room in its
                    // channel snapshot. A bare join grants durable membership to a
                    // room she structurally cannot hear (P0 20b44763 — three rounds
                    // of kickoffs into deaf run rooms, zero turns).
                    if let Err(e) = rt.join_room(&room_name).await {
                        room_join_errors.push(format!("{who}: {e}"));
                    }
                }
                None => room_join_errors.push(format!("{who}: no live airc runtime")),
            }
        }

        // The RUN ROOM's board — fresh, so the idempotence gate and prune below reason
        // about THIS run and nothing else. That is the point: a run's board is finally its
        // own honest denominator instead of a shared pile 136 cards deep.
        let board = airc
            .work_board_complete(airc_lib::WORK_BOARD_PROJECTION_PAGE_SIZE)
            .await
            .map_err(|e| CommandError::Internal(format!("board read: {e}")))?
            .snapshot();

        // Tasks that ALREADY have a live card. A card in a terminal state (Closed /
        // Merged) is finished work and must NOT block a re-dispatch — that is how a
        // benchmark gets legitimately re-run. Everything else (Open, Claimed,
        // InProgress, Blocked, Review) is live work; posting a second card for it
        // just splits effort across duplicates.
        // Grouped by task key, because the same map answers BOTH questions: "does
        // this task already have a card?" (the idempotence gate) and "which of its
        // cards are redundant?" (the optional prune below).
        let mut live_by_task: std::collections::HashMap<&str, Vec<&airc_lib::WorkCard>> =
            std::collections::HashMap::new();
        for c in board.cards.iter().filter(|c| {
            !matches!(
                c.state,
                airc_lib::CardState::Closed | airc_lib::CardState::Merged
            )
        }) {
            if let Some(k) = bench_card_key(&c.title) {
                live_by_task.entry(k).or_default().push(c);
            }
        }

        // Repo: caller-supplied, else the repo the board already uses. No
        // baked-in default — an empty board with no repo argument is a real
        // question only the operator can answer.
        // Repo key, most-explicit-first:
        //   1. what the caller named;
        //   2. what the room they came from already uses (unchanged legacy behaviour —
        //      matches existing cards so a re-dispatch never splits a live board);
        //   3. THE CHECKOUT'S OWN `origin` — the repo key is a fact about this clone,
        //      not a string to retype.
        //
        // (3) exists because per-run rooms removed (2)'s source by construction: the
        // FIRST dispatch leaves the curator standing in a fresh empty bench room, so the
        // SECOND one had nothing to infer from and failed asking for `--repo`. Deriving
        // it from `origin` fixes that at the root and is right for any repo user on a
        // fresh clone, the same way `resolve_dispatch_roster` refuses to bake in names.
        let repo_key = match p.repo.clone() {
            Some(r) => r,
            None => repo_hint
                .or_else(|| {
                    // Process cwd, because `git` walks UP to find `.git` — any directory
                    // inside the checkout answers. The core is launched from the repo by
                    // start-server.sh, and cognition/eval + gym resolve their roots the
                    // same way. That is a real cwd dependency of the #195 class, not a
                    // pretence otherwise: if the core is ever launched from elsewhere this
                    // returns None and the caller is asked for `repo` — a loud fallback,
                    // never a wrong board key.
                    std::env::current_dir()
                        .ok()
                        .and_then(|cwd| crate::code::git_bridge::origin_repo_slug(&cwd))
                })
                .ok_or_else(|| {
                    CommandError::Invalid(
                        "no `repo` was given, the room you dispatched from has no cards to \
                         infer one from, and this checkout has no `origin` remote to derive \
                         one from — pass repo=<owner/name> so the cards land under a real \
                         board key"
                            .to_string(),
                    )
                })?,
        };
        let repo = RepoId::new(repo_key)
            .map_err(|e| CommandError::Invalid(format!("invalid repo: {e:?}")))?;

        let take = p.limit.map(|l| l as usize).unwrap_or(prepared.len());
        // Every task key THIS benchmark owns — captured before the loop consumes
        // `prepared`, and deliberately NOT limited by `take`: a prune must be able
        // to clean duplicates for tasks outside the current dispatch window, which
        // is what makes `limit=0 prune=true` a pure board-cleanup call.
        let planned_keys: Vec<String> = prepared
            .iter()
            .filter_map(|pc| bench_card_key(&pc.title).map(str::to_string))
            .collect();
        // Continuum home under which each citizen's workspace lives (for SWE staging).
        let stage_home = continuum_home().ok();
        let mut card_ids = Vec::new();
        // FULL card uuids for the round tracker (#371) — the bus event carries the full
        // hyphenated uuid, so the round's membership set must too (the 8-char `card_ids`
        // shorts are the human/CLI handle, not the identity).
        let mut card_uuids: Vec<uuid::Uuid> = Vec::new();
        let mut skipped_needs_setup = 0u32;
        let mut skipped_known_red = 0u32;
        let mut skipped_already_on_board = 0u32;
        let mut kickoffs = 0u32;
        let mut solves_fired = 0u32;
        let mut kickoff_errors = Vec::new();
        // Auto-fire is capped at the live serving LANE count. A directed dispatch must never
        // fire more concurrent scored solves than the box can hold (glass-boxed 2026-08-11:
        // over-firing solves onto a 2-lane box thrashed the llama-server lane to a
        // Connection-refused death mid-solve).
        //
        // WAIT for the boot-gate, don't guard against it (Joel 2026-08-11: "persona should boot
        // beforehand … dispatch requires live is dumb"). Personas boot event-driven the moment
        // the serving lane proves it can decode (ipc/mod.rs spawn_all), so a dispatch that lands
        // in that ~10-15s window must PARK on serving readiness, not skip every solve and post
        // silent cards. `await_ready_serving` returns as soon as the lane is decode-verified, or
        // None after the deadline (a genuinely dead lane — then we stage + kick off but don't
        // launch a solve into a corpse, self-healing on the next dispatch). NOTE: this caps THIS
        // dispatch call; the global in-flight-solve admission gate shared with work/claim
        // (#385/#386) is the broader fix.
        //
        // #442, and the correction that makes it a GATE: the probe below was already here and
        // already correct — its answer was simply advisory. On a dead lane this set the cap to
        // zero and STAGED THE CARDS ANYWAY, posting a full round of work to a board with
        // nothing on the box able to decode a token (#455: "we stage work into the gap"). A
        // not-ready lane is now a STATE the round stops at, not a parameter that silently
        // degrades it into an empty round.
        //
        // The wait budget is DERIVED, never invented: `DEFAULT_SERVING_WAIT` is
        // `READY_TIMEOUT + margin` — the spawner's own load budget — so this gate can never
        // declare failure before a legitimate cold load has had its full window. The flat 30s
        // that used to be here was exactly that bug in miniature.
        let awaited = crate::inference::llama_server::await_ready_serving(
            crate::inference::llama_server::DEFAULT_SERVING_WAIT,
        )
        .await;
        let solve_cap: u32 = {
            use crate::cognition::round_readiness::{decide, RoundReadiness};
            let current = crate::inference::llama_server::current_serving();
            match decide(awaited.as_ref(), Some(&current)) {
                RoundReadiness::Ready { lanes } => lanes,
                RoundReadiness::Blocked(reason) => {
                    let why = reason.explain();
                    crate::probe!(
                        class = "bench.round.staging_blocked",
                        benchmark = spec.name,
                        forced = p.force.unwrap_or(false),
                        reason = why.as_str(),
                        "STAGING → READY refused: serving cannot work this round (#442)",
                    );
                    // Refuse, with the override announcing itself — same contract as
                    // `start --force` (#420). A gate with no escape gets worked around;
                    // a silent escape is worse than no gate.
                    if !p.force.unwrap_or(false) {
                        return Err(CommandError::Invalid(format!(
                            "benchmark/dispatch refused to stage `{}`: {why}\n\
                             (pass --force to stage anyway — it will post cards that cannot be \
                             worked until a lane comes up)",
                            spec.name
                        )));
                    }
                    tracing::warn!(
                        benchmark = %spec.name,
                        reason = %why,
                        "--force: staging a round into a lane that is NOT decode-verified"
                    );
                    0
                }
            }
        };
        // OPEN the round BEFORE the first card exists. Kickoffs go out inside the loop, so
        // a citizen can claim card 1 while card 2 is still being posted — and `work/claim`
        // asks the round who drives. Registering after the loop (as this did) left that
        // window answering with the default, which would silently fire the detached solver
        // on the first card of a citizen-driven round.
        let driver = p.drive.unwrap_or_default();
        crate::cognition::bench_round::open_round(room.room_id.as_uuid(), spec.name, driver);
        for pc in prepared.into_iter().take(take) {
            // A gym setup_shell card is prepared IN THE ASSIGNEE'S WORKSPACE at
            // dispatch, below (same contract as SWE checkout staging) — the early
            // unconditional skip that used to live here reported the entire ds-1000
            // maiden dispatch as `skipped_needs_setup: 4` (2026-08-22): the eval
            // path always ran setup_shell, but no card-dispatch orchestration did.

            // IDEMPOTENCE: this exact task already has a live card. Re-dispatching
            // would post a duplicate, and duplicates are not free — two citizens
            // claiming two cards for one instance burn two of a 2-4 lane box on the
            // same problem, and the resulting board has no honest denominator to
            // compute a pass rate from. The key comes from `pc.title` itself, so the
            // string we match on is the string that would have been posted.
            //
            // Skipping is REPORTED (`skipped_already_on_board`), never silent — same
            // contract as `skipped_needs_setup` above: a partial dispatch that reads
            // as full coverage is the lie these counters exist to prevent.
            if bench_card_key(&pc.title).is_some_and(|k| live_by_task.contains_key(k)) {
                skipped_already_on_board += 1;
                continue;
            }

            // The directed assignee (round-robin over the RESOLVED live roster). Always a
            // real online citizen — resolve_dispatch_roster guaranteed a non-empty roster
            // or errored. Her peer_id rides along, so SWE staging needs no second name
            // lookup (and can never silently no-stage on an unknown name). A SWE card stages
            // into HER workspace before it is claimable, so her claim auto-fires the scored
            // solve (#346 dispatch_staged_swe_solve).
            let (who, who_peer) = &roster[card_ids.len() % roster.len()];

            // STAGE the SWE checkout into the assignee's workspace/swe/<instance> BEFORE the
            // card is claimable, so `work/claim` finds it and launches the solve. Reuses the
            // proven swe_bench::clone_at (fast from the local mirror). Best-effort: a stage
            // failure is REPORTED and the card still posts — the loop never half-breaks.
            // GYM setup: run the task's setup_shell in the assignee's workspace NOW,
            // so the card she claims already has its context/grader staged. Adapter
            // setups are idempotent (mkdir -p + overwrite-decode), so a re-dispatch
            // re-stages harmlessly. Failure is REPORTED and the card is withheld —
            // posting a card whose grader never staged manufactures a permanent
            // infra-zero wearing a capability face.
            if let Some(setup) = pc.setup_shell.as_deref() {
                let ok = match stage_home.as_ref() {
                    Some(home) => {
                        let ws = crate::identity::citizen_peer_dir(
                            home,
                            crate::identity::PeerId::from_uuid(*who_peer),
                        )
                        .join("workspace");
                        let _ = std::fs::create_dir_all(&ws);
                        match tokio::process::Command::new("sh")
                            .arg("-c")
                            .arg(setup)
                            .current_dir(&ws)
                            .output()
                            .await
                        {
                            Ok(out) if out.status.success() => true,
                            Ok(out) => {
                                let head: String = pc.title.chars().take(48).collect();
                                kickoff_errors.push(format!(
                                    "setup {head}: {}",
                                    String::from_utf8_lossy(&out.stderr).trim()
                                ));
                                false
                            }
                            Err(e) => {
                                kickoff_errors.push(format!("setup spawn: {e}"));
                                false
                            }
                        }
                    }
                    None => false,
                };
                if !ok {
                    skipped_needs_setup += 1;
                    continue;
                }
            }

            let mut staged_ok = false;
            // THE COVERAGE GATE (the cheap half of `benchmark/validate`). If THIS
            // box has already PROVEN this instance's (repo, era) env class red,
            // do not spend a citizen's hours and a grader's slot discovering the
            // same wall a third time — name it and skip. Measured 2026-08-28:
            // astropy-6938 was dispatched into the numpy-2 wall while a validate
            // run minutes away already knew astropy/2017 was red.
            //
            // A DICTIONARY LOOKUP, never a build: dispatch must stay fast for a
            // repo user who has never run validate. Fail-open by construction —
            // no map, a green class, or a map earned on a different machine class
            // all return None and DISPATCH (see known_red_wall). Reported like
            // every other skip; a partial dispatch reading as full coverage is
            // the lie these counters exist to prevent.
            if let CardWork::Swe { instance } = &pc.work {
                if let Some(wall) =
                    known_red_wall(spec.name, &instance.repo, instance.year())
                {
                    skipped_known_red += 1;
                    kickoff_errors.push(format!(
                        "skipped {} — its env class ({} {}) is PROVEN RED on this box by \
                         benchmark/validate, so a solve here would burn hours on a known \
                         wall: {wall}",
                        instance.instance_id,
                        instance.repo,
                        instance.year()
                    ));
                    continue;
                }
            }

            if let (CardWork::Swe { instance }, Some(home)) = (&pc.work, stage_home.as_ref()) {
                let dir = crate::identity::citizen_peer_dir(
                    home,
                    crate::identity::PeerId::from_uuid(*who_peer),
                )
                .join("workspace")
                .join("swe")
                .join(&instance.instance_id);
                if dir.join(".git").exists() {
                    staged_ok = true; // already staged (a prior claim / dispatch)
                    // Self-heal pre-shield checkouts: clone_at shields NEW trees, but a
                    // checkout staged before the shield existed stays exposed forever
                    // without this — the 76-tree hand backfill of 2026-08-22, automated.
                    crate::cognition::swe_bench::shield_workspace_excludes(&dir);
                } else if let Err(e) = crate::cognition::swe_bench::clone_at(instance, &dir).await {
                    kickoff_errors.push(format!("stage {}: {e}", instance.instance_id));
                } else {
                    staged_ok = true;
                }
                // Build the per-instance venv NOW (with pytest + the repo installed) so her
                // HANDS have a working `pytest`/`python` the moment she starts — not only at
                // grade time. Without this the solve's `code/shell pytest` hits the system
                // interpreter and she loops trying to install pytest into it (glass-boxed
                // 2026-08-11 from Anon's astropy turn). ensure_env is idempotent and cached, so
                // the later grade reuses this exact venv. Best-effort: a build failure is
                // reported but the card still posts — the loop never half-breaks.
                if staged_ok {
                    if let Err(e) = crate::cognition::swe_bench::ensure_env(instance, &dir).await {
                        kickoff_errors.push(format!("env {}: {e}", instance.instance_id));
                        // A solve against an unbuildable env can ONLY void: she spends a
                        // full attempt (24 acts, live: pytest-5413 twice on 2026-08-12)
                        // in a workspace whose grade is a known-in-advance env fault —
                        // no verdict, no lesson (the failure is the env's, not hers).
                        // The card still posts (claimable once the env heals); the
                        // SCORED solve does not fire (Joel: "why run broken code
                        // knowing she's gonna struggle and fall").
                        staged_ok = false;
                    }
                }
            }

            let mut req = CreateWorkCard::new(repo.clone(), pc.title, Priority::P2);
            req.body = Some(pc.body);
            let card_id = airc
                .create_work_card(req)
                .await
                .map_err(|e| CommandError::Internal(e.to_string()))?;
            let full = card_id.as_uuid().simple().to_string();
            let short = full[..8].to_string();

            // The card joins the round the moment it exists — BEFORE the pre-claim and
            // the kickoff below, either of which can put it into someone's hands. From
            // here `work/claim` can read who drives it (see `open_round`).
            crate::cognition::bench_round::add_card(room.room_id.as_uuid(), card_id.as_uuid());
            // WHO works this card, recorded at staging (before any solve fires) —
            // the follow-on driver and the boot resume read it (plan A4/A5).
            crate::cognition::bench_round::record_card_assignee(card_id.as_uuid(), *who_peer);

            // Directed gym card: CLAIM IT FOR HER at dispatch, under her own airc
            // identity. The detached-solve SWE arm below fires her scored solve directly
            // (dispatch_staged_swe_solve — "we don't wait on her to re-derive a
            // work/claim from the kickoff"); gym cards never got the same cut, so
            // every round spent its first multi-minute turn per card on claim
            // ceremony the dispatcher had already decided (Joel 2026-08-15:
            // "taking 30 minutes to start coding sure is a flawed design").
            // #425-compatible: the claim is administrative — the WORK stays hers,
            // in-room, through her own cognition. Best-effort: a failed pre-claim
            // is REPORTED and the card stays claimable by hand.
            //
            // A CITIZEN-driven SWE card takes the same cut, and for the same reason it
            // was written: nothing detached will fire for it, so the ONLY thing standing
            // between the card and her work turn is the kickoff→claim hop that stalls
            // rounds. Pre-claiming removes that hop without moving the work — she still
            // does it herself, in her own loop, on the held-work turn. (This goes through
            // airc directly, not the `work/claim` verb, so it cannot re-enter the
            // detached-solve dispatcher from here.)
            let mut pre_claimed = false;
            //
            // `staged_ok` gates the SWE arm for the SAME reason it gates the detached
            // solve above ("why run broken code knowing she's gonna struggle and fall"):
            // an unbuildable env can only void, and pre-claiming would put her hands in it
            // for a full turn. The card still posts, claimable by hand once the env heals.
            // BOTH drivers pre-claim a staged SWE card now. DetachedSolve was
            // excluded, which left its cards Open forever: the solve never touches
            // card state and the lapse sweeper refuses unclaimed cards, so a
            // detached round could never reach Done and every boot reaped it
            // (mapped 2026-08-26). Claimed-by-the-assignee is what lets the
            // grade path close the card and the round complete.
            let pre_claim_this = matches!(pc.work, CardWork::Gym { .. })
                || (matches!(pc.work, CardWork::Swe { .. }) && staged_ok);
            if pre_claim_this {
                match self.registry.get(*who_peer) {
                    Some(rt) => {
                        match rt
                            .airc()
                            .claim_work_card(airc_lib::ClaimWorkCard {
                                card_id,
                                ttl_ms: crate::modules::work::DEFAULT_CLAIM_TTL_MS,
                            })
                            .await
                        {
                            Ok(_) => pre_claimed = true,
                            Err(e) => kickoff_errors.push(format!("pre-claim {short}: {e}")),
                        }
                    }
                    None => kickoff_errors
                        .push(format!("pre-claim {short}: {who} has no live airc runtime")),
                }
            }

            // Directed dispatch: round-robin an addressed kickoff per card. An addressed
            // imperative in its OWN message block is what actually starts work (measured
            // 2026-08-07: coalesced mid-burst it was ignored). airc.say is one event = one
            // block, so the structural condition holds by construction.
            {
                let kickoff = match &pc.work {
                    CardWork::Gym { solution_file } if pre_claimed => format!(
                        "@{who} (to you): card {short} is CLAIMED FOR YOU on this board — \
                         no claim step needed. Read its body, write your solution to \
                         `{solution_file}` in your workspace NOW, then mark it done \
                         (work/state {short} done). Your artifact gets graded against \
                         held-out tests."
                    ),
                    CardWork::Gym { solution_file } => format!(
                        "@{who} (to you): card {short} on this board is yours. Claim it \
                         (claim_task {short}), read its body, write your solution to \
                         `{solution_file}` in your workspace, then mark it done \
                         (work/state {short} done). Your artifact gets graded against \
                         held-out tests."
                    ),
                    CardWork::Swe { instance } => {
                        let staged = if staged_ok {
                            format!(
                                " The repo is STAGED in your workspace at `swe/{}/`.",
                                instance.instance_id
                            )
                        } else {
                            String::new()
                        };
                        format!(
                            "@{who} (to you): card {short} is a REAL {} issue (SWE-bench, a full \
                             project).{staged} I've STARTED your scored solve on it — fix the bug \
                             in `swe/{}/` (do not edit the tests); your diff is graded against the \
                             repo's held-out tests, and you get a few attempts to investigate your \
                             own failures. Watch the room for the verdict.",
                            instance.repo, instance.instance_id
                        )
                    }
                };
                // AUTHOR IT AS SOMEONE ELSE. A citizen's inbound stream skips messages
                // she is recorded as having said (correct — nobody answers their own
                // speech), so a kickoff addressed to her and AUTHORED by her is dropped
                // silently and she never takes a turn.
                //
                // That is not hypothetical: `curator_airc` falls back to
                // `any_live_citizen()` for the operator (no self-peer, #27), which picks
                // the lexicographically-lowest name. With the roster at `Atlas` + `Benchy`
                // that is ALWAYS Atlas — so a round directed at Atlas sent her three
                // kickoffs she authored herself, reported `kickoff_errors: []`, and
                // produced ZERO turns while a detached solver did the work beside her
                // (measured 2026-08-17). A bigger roster usually picked someone else,
                // which is why it read as intermittent (#417) rather than structural.
                let voice_rt = match self.registry.any_live_citizen_other_than(Some(*who_peer)) {
                    Some(rt) => rt,
                    None => {
                        // The only live citizen IS the addressee. Refuse loudly instead of
                        // sending a message that cannot be heard — the card stays on the
                        // board, and the operator learns the roster is too small to direct
                        // work at all ([[fallbacks-are-illegal-fail-loud]]).
                        kickoff_errors.push(format!(
                            "{short}: {who} is the only live citizen, so nobody else can \
                             voice a kickoff addressed to her — she would skip her own \
                             message and never take a turn. Spawn a second citizen \
                             (persona/spawn), then re-dispatch."
                        ));
                        continue;
                    }
                };
                // SAY IT IN THE ROUND'S ROOM — never `say()`. Plain `say` posts to the
                // VOICE's current room, and the voice is by construction a different
                // citizen than the addressee, standing wherever she happens to stand.
                // Measured 2026-08-21 (the third deaf-kickoff variant): two dispatches
                // reported `kickoffs: 1`, both messages landed in the voice's own room,
                // the addressee's stream never carried them — and Atlas spent the
                // afternoon solving a gym exercise she COULD see while her staged SWE
                // card sat silent. The voice also JOINS the round room first (idempotent,
                // epoch-bumping), because membership is what makes say_in deliverable —
                // the same rule the assignee join-loop above encodes for hearing.
                if let Err(e) = voice_rt.join_room(&room_name).await {
                    kickoff_errors.push(format!("{short}: voice join: {e}"));
                    continue;
                }
                match crate::persona::airc_citizen::publish_text_in_room(
                    voice_rt.airc(),
                    room.room_id.as_uuid(),
                    &kickoff,
                )
                .await
                {
                    Ok(_) => kickoffs += 1,
                    // The card stays claimable — a lost kickoff is REPORTED (never unwound
                    // or hidden); the citizen can still find and claim it off the board.
                    Err(e) => kickoff_errors.push(format!("{short}: {e}")),
                }
            }

            // DIRECTED SWE dispatch FIRES her scored solve directly — the repo is staged in
            // her workspace and the card is addressed to her, so we don't wait on her to
            // re-derive a work/claim from the kickoff (the hop that stalls under warm-slot
            // starvation — glass-boxed 2026-08-11: cards staged + assigned, zero claims,
            // zero solves). Her WHOLE cognition solves it with an exclusive warm slot; the
            // work/claim path stays the trigger for undirected / human-claimed cards. Only a
            // STAGED SWE card has a solve to fire here (a gym card self-grades differently).
            if staged_ok
                && solves_fired < solve_cap
                && driver == crate::cognition::bench_round::WorkDriver::DetachedSolve
            {
                if let CardWork::Swe { .. } = &pc.work {
                    // The run room goes WITH the solve: her acts radiate receipts
                    // into the room this dispatch just spawned, so the round's work
                    // is visible where the round lives (#243/#329) instead of only
                    // in a ledger file that lands when it is already over.
                    crate::modules::work::dispatch_staged_swe_solve(
                        ctx,
                        &airc,
                        crate::modules::work::StagedSolveDispatch {
                            // The roster still carries bare `(String, Uuid)` tuples — a
                            // loose-id smell of its own (#396). Typed at THIS boundary so
                            // the dispatch cannot confuse a peer with a room; typing the
                            // roster itself is that card's work, not a silent widening here.
                            claimer: crate::identity::PeerId::from_uuid(*who_peer),
                            card: card_id,
                            room: room.room_id,
                        },
                    )
                    .await;
                    solves_fired += 1;
                }
            }
            card_uuids.push(card_id.as_uuid());
            card_ids.push(short);
        }

        // THE ROUND BECOMES AN ENTITY (#371): register the dispatched card set under the
        // run room's own id (dispatch already mints one id per run — the room; never a
        // second one). From here the round has a lifecycle somebody owns: the
        // `work.card.state_changed` subscriber settles cards as they reach terminal
        // states and announces the END — instead of the round's fate being probe
        // archaeology ("random and directed by agent, not an ecosystem", Joel 8/16).
        crate::cognition::bench_round::seal_round(room.room_id.as_uuid());

        // PRUNE (opt-in): converge the board to one live card per task for THIS
        // benchmark. Scoped to the keys this dispatch planned, so pruning one
        // benchmark can never touch another's cards — or a hand-written one.
        //
        // Runs AFTER dispatch so a card just posted for a task is already in the
        // group and cannot be orphaned by a prune that ran against a stale read.
        let mut pruned_duplicates = 0u32;
        let mut contended_tasks = 0u32;
        if p.prune.unwrap_or(false) {
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            for key in planned_keys.iter() {
                let Some(group) = live_by_task.get(key.as_str()) else {
                    continue;
                };
                let holds: Vec<_> = group
                    .iter()
                    .map(|c| crate::persona::card_holder::hold_of(c, now_ms))
                    .collect();
                let (to_close, contended) = duplicates_to_close(&holds);
                if contended {
                    contended_tasks += 1;
                }
                for idx in to_close {
                    let card_id = group[idx].card_id;
                    match airc
                        .change_work_card_state(airc_lib::ChangeWorkCardState {
                            card_id,
                            state: airc_lib::CardState::Closed,
                        })
                        .await
                    {
                        Ok(_) => pruned_duplicates += 1,
                        // A close that fails is REPORTED, never silently dropped —
                        // the caller must not read a partial prune as a clean board.
                        Err(e) => kickoff_errors.push(format!(
                            "prune {}: {e}",
                            card_id.as_uuid().simple().to_string()[..8].to_string()
                        )),
                    }
                }
            }
        }

        // Room-join failures ride the SAME reported channel as kickoff failures — a
        // citizen who never made it into the run is exactly as invisible as a kickoff
        // that never landed, and neither may be silent.
        kickoff_errors.extend(room_join_errors.into_iter().map(|e| format!("join room: {e}")));

        Ok(BenchmarkDispatchResult {
            benchmark: spec.name.to_string(),
            room: room.name,
            room_id: room.room_id,
            dispatched: card_ids.len() as u32,
            card_ids,
            skipped_needs_setup,
            skipped_known_red,
            skipped_already_on_board,
            pruned_duplicates,
            contended_tasks,
            kickoffs,
            solves_fired,
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
    // what this catches: the recipe row's serde contract — a minimal hand-authored
    // `data/create` row (name + dispatches only) MUST parse, extra/unknown future
    // fields MUST be tolerated, and instances default empty. The recipe is data
    // authored by operators; a brittle parse turns a typo into a dead experiment
    // with no compiler to catch it (2026-08-29, the two-command flow).
    // what this catches: the template→instance substitution contract — `{model}`
    // resolves from params everywhere in the row, and an unresolved placeholder
    // is a LOUD error naming the key, never a literal "{model}" reaching a lane
    // pin (Joel 2026-08-29: the model is an ARGUMENT to the recipe, recipes are
    // templates — one "challenge" recipe serves every future model drop).
    #[test]
    fn recipe_templates_substitute_and_fail_loud_on_missing() {
        let row = serde_json::json!({
            "name": "challenge",
            "model_id": "{model}",
            "dispatches": [{"benchmark": "{dataset}", "instances": ["a__b-1"]}]
        });
        let mut params = std::collections::BTreeMap::new();
        params.insert("model".to_string(), "org/some-model".to_string());
        params.insert("dataset".to_string(), "swe-bench-lite".to_string());
        let out = BenchmarkDispatch::instantiate_recipe(row.clone(), &params).unwrap();
        assert_eq!(out["model_id"], "org/some-model");
        assert_eq!(out["dispatches"][0]["benchmark"], "swe-bench-lite");

        let err = BenchmarkDispatch::instantiate_recipe(row, &Default::default()).unwrap_err();
        assert!(format!("{err:?}").contains("model"), "names the missing key");
    }

    #[test]
    fn recipe_rows_parse_tolerantly() {
        let minimal: BenchmarkRecipe = serde_json::from_value(serde_json::json!({
            "name": "x",
            "dispatches": [{"benchmark": "swe-bench-lite"}]
        }))
        .expect("minimal row parses");
        assert!(minimal.model_id.is_none());
        assert!(minimal.dispatches[0].instances.is_empty());

        let rich: BenchmarkRecipe = serde_json::from_value(serde_json::json!({
            "name": "hard-eight",
            "description": "d",
            "model_id": "some/model",
            "dispatches": [
                {"benchmark": "swe-bench-lite", "instances": ["a__b-1"], "limit": 3},
                {"benchmark": "swe-bench-verified", "instances": ["c__d-2"]}
            ],
            "some_future_field": {"ignored": true}
        }))
        .expect("future fields tolerated");
        assert_eq!(rich.dispatches.len(), 2);
        assert_eq!(rich.dispatches[0].limit, Some(3));
    }
    mod coverage_gate {
        use super::super::*;

        // what this catches: the gate turning into a BLOCKER for people who have
        // never run benchmark/validate. Joel's constraint, 2026-08-28: "we don't
        // slow them down, but we run checks before wasting the persona and
        // graders time." So every ambiguous case must DISPATCH: no map on disk, a
        // green class, or a map earned on a DIFFERENT machine class (a coverage
        // claim is only true for the platform that earned it). Only a class this
        // very box proved red may withhold a card — and then it must name the
        // wall, because a silent skip is the partial-dispatch lie the counters
        // exist to prevent.
        #[test]
        fn the_coverage_gate_fails_open_and_only_blocks_a_locally_proven_red_class() {
            let dataset = "swe-bench-unit-test-fixture";
            let here = BenchmarkPlatformFingerprint::capture().machine_class;
            // A TEMPDIR, never the operator's real ~/.continuum: writing a
            // fixture into a person's live data directory is both a lie about
            // isolation and a way to clobber their state — and it made this very
            // test pass alone and FAIL in the full suite.
            let root = std::env::temp_dir().join(format!("cov-gate-{}", uuid::Uuid::new_v4()));
            let path = coverage_map_path_in(&root, dataset, &here);

            // 1. No map at all — the fresh-clone case. Must dispatch.
            assert_eq!(
                known_red_wall_in(&root, dataset, "astropy/astropy", 2017),
                None,
                "a user who never ran validate must never be gated"
            );

            // 2. A map from THIS box: red class blocks (with its wall), green does not.
            let mut map = BenchmarkValidateResult {
                platform: BenchmarkPlatformFingerprint::capture(),
                classes: vec![
                    BenchmarkValidateClass {
                        repo: "astropy/astropy".into(),
                        era: "2017".into(),
                        representative: "astropy__astropy-6938".into(),
                        covers: 1,
                        green: false,
                        wall: Some("numpy 2 rejects copy=False".into()),
                    },
                    BenchmarkValidateClass {
                        repo: "django/django".into(),
                        era: "2022".into(),
                        representative: "django__django-15252".into(),
                        covers: 9,
                        green: true,
                        wall: None,
                    },
                ],
                instances_green: 9,
                dataset_size: 10,
                summary: String::new(),
            };
            if let Some(d) = path.parent() {
                std::fs::create_dir_all(d).unwrap();
            }
            std::fs::write(&path, serde_json::to_string(&map).unwrap()).unwrap();

            assert_eq!(
                known_red_wall_in(&root, dataset, "astropy/astropy", 2017).as_deref(),
                Some("numpy 2 rejects copy=False"),
                "a locally-proven red class must withhold the card AND name the wall"
            );
            assert_eq!(
                known_red_wall_in(&root, dataset, "django/django", 2022),
                None,
                "a green class must dispatch"
            );
            assert_eq!(
                known_red_wall_in(&root, dataset, "astropy/astropy", 2023),
                None,
                "an era with no row is not evidence of anything — dispatch"
            );

            // 3. A map earned on ANOTHER machine class must not gate this one.
            map.platform.machine_class = format!("not-{here}");
            std::fs::write(&path, serde_json::to_string(&map).unwrap()).unwrap();
            assert_eq!(
                known_red_wall_in(&root, dataset, "astropy/astropy", 2017),
                None,
                "another box's coverage claim is not evidence about this box"
            );

            let _ = std::fs::remove_dir_all(&root);
        }
    }


    use super::*;

    fn citizen(name: &str) -> (String, uuid::Uuid) {
        (
            name.to_string(),
            uuid::Uuid::new_v5(&uuid::Uuid::NAMESPACE_OID, name.as_bytes()),
        )
    }

    // what this catches: #370. Every catalogued suite carried a real `source_url` and exactly
    // ONE of them could be pulled, because the only fetcher was fused to the SWE row shape.
    // This pins the derivation that makes the OTHER rows reachable — and pins that a non-HF
    // source (a GitHub raw .jsonl, of which the catalog has two) is REFUSED rather than
    // silently handed to the HF rows API, which would return an in-band error the caller
    // would read as "the suite is empty".
    #[test]
    // what this catches: a name collision in the catalog. Found live 2026-08-23:
    // the Terminal-Bench adapter's full row landed while the old stub row of the
    // same name survived — benchmark/list showed the name twice and fetch-by-name
    // silently took whichever matched first. A name IS the lookup key everywhere
    // (fetch, dispatch, verify), so duplicates are a routing hazard, not cosmetics.
    #[test]
    fn catalog_names_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for b in known_benchmarks() {
            assert!(
                seen.insert(b.name),
                "catalog name '{}' appears more than once — the name is the lookup \
                 key for fetch/dispatch/verify; merge the rows",
                b.name
            );
        }
    }

    fn every_hf_catalogued_suite_yields_coordinates_and_non_hf_is_refused() {
        let by_name = |n: &str| known_benchmarks().iter().find(|b| b.name == n).unwrap();

        assert_eq!(
            by_name("swe-bench-lite").reach(),
            SourceReach::Rows {
                dataset: "princeton-nlp/SWE-bench_Lite",
                config: "default",
                split: "test"
            }
        );
        assert!(
            matches!(by_name("humaneval").reach(), SourceReach::ForeignSource { .. }),
            "a GitHub raw .jsonl must NOT fall through to the HF rows API — that returns an \
             in-band error a caller reads as 'the suite is empty'"
        );
        assert!(matches!(by_name("hard-rs").reach(), SourceReach::InTree));

        // The two live-verified script datasets (2026-08-19): the rows API refuses these at
        // EVERY coordinate, so they must be told apart from a wrong-config miss or the
        // operator retries configs forever.
        for n in ["apps", "livecodebench"] {
            assert!(
                matches!(by_name(n).reach(), SourceReach::HuggingFaceScriptDataset { .. }),
                "`{n}` is a loading-script dataset; classifying it as fetchable sends the \
                 caller into a config-guessing loop that can never succeed"
            );
        }

        // what this catches specifically: bigcodebench publishes REVISIONS as splits and has
        // no `test` split at all. The default coordinates return "Unexpected error" — measured
        // live — so the version we score against has to be a recorded catalog fact.
        assert_eq!(
            by_name("bigcodebench").reach(),
            SourceReach::Rows {
                dataset: "bigcode/bigcodebench",
                config: "default",
                split: "v0.1.4"
            }
        );

        // And no catalogued row may be silently unclassifiable.
        for b in known_benchmarks() {
            let reach = b.reach();
            if let SourceReach::Rows { dataset, .. } = reach {
                assert!(
                    dataset.contains('/'),
                    "`{}` resolves to `{dataset}`, which the rows API cannot address",
                    b.name
                );
            }
        }
    }

    // what this catches: THE round-killer of 2026-08-18. Citizens registered but not yet
    // hosted made every readiness surface report a ready roster, and dispatch staged a full
    // round into a room where nobody had a perception stream — `dispatched: 2, kickoffs: 2,
    // kickoff_errors: []`, zero turns. Resolving against RESIDENCY must refuse instead, and
    // the refusal must say WHICH state this is, because the two have opposite fixes:
    // registered-but-not-resident is a WAIT, unregistered is `persona/spawn`.
    #[test]
    fn registered_but_not_resident_refuses_and_names_the_wait() {
        let registered = vec![citizen("Atlas"), citizen("Benchy")];
        let err = resolve_dispatch_roster(&[], &registered, &[]).unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            matches!(err, CommandError::Denied(_)),
            "staging into an empty room is denied, not a soft warning: {msg}"
        );
        assert!(
            msg.contains("Atlas") && msg.contains("Benchy"),
            "the refusal names WHO is registered so the operator can wait on them: {msg}"
        );
        assert!(
            msg.contains("NOT RESIDENT"),
            "and says the state plainly, not 'not online' (which reads as a typo): {msg}"
        );
    }

    // what this catches: the OTHER arm must stay distinguishable. Nobody registered at all
    // is a different problem with a different fix — `persona/spawn`, not a wait — and
    // collapsing the two would send an operator to wait forever for a citizen who was
    // never born.
    #[test]
    fn nobody_registered_at_all_points_at_spawn_not_at_waiting() {
        let err = resolve_dispatch_roster(&[], &[], &[]).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("persona/spawn"), "names the actual fix: {msg}");
        assert!(
            !msg.contains("NOT RESIDENT"),
            "must NOT claim a residency wait when there is nobody to wait for: {msg}"
        );
    }

    // what this catches: a named assignee who is registered but not resident must not be
    // reported as a typo. "not online" against a name the operator can see in
    // `persona/roster` sends them hunting for a misspelling that does not exist — the same
    // lie as the round-level one, in a smaller box.
    #[test]
    fn a_named_assignee_who_is_not_resident_is_told_apart_from_a_typo() {
        let resident = vec![citizen("Atlas")];
        let registered = vec![citizen("Atlas"), citizen("Benchy")];

        let err =
            resolve_dispatch_roster(&resident, &registered, &["Benchy".into()]).unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            msg.contains("registered but not resident") && msg.contains("not a typo"),
            "a real citizen who is merely unhosted must be named as a WAIT: {msg}"
        );

        // ...while a genuine typo still reads as one, with no residency excuse attached.
        let err = resolve_dispatch_roster(&resident, &registered, &["Atals".into()]).unwrap_err();
        let msg = format!("{err:?}");
        assert!(
            !msg.contains("not a typo"),
            "an unknown name gets no residency note — it IS a typo: {msg}"
        );
    }

    // what this catches: with everyone resident, dispatch behaves exactly as before —
    // empty request → the whole resident roster, in order. The residency gate must not
    // narrow the happy path (the default dispatch is what actuates a round at all).
    #[test]
    fn all_resident_dispatches_the_whole_roster_in_order() {
        let all = vec![citizen("Atlas"), citizen("Benchy")];
        let got = resolve_dispatch_roster(&all, &all, &[]).unwrap();
        assert_eq!(got, all, "empty request → everyone resident, order preserved");

        let got = resolve_dispatch_roster(&all, &all, &["Benchy".into()]).unwrap();
        assert_eq!(got, vec![citizen("Benchy")], "named assignee resolves");
    }

    // what this catches: a derived run-room name that airc REFUSES. `ChannelName::new` accepts
    // only `[a-z0-9_-]`, so a benchmark named with a `/`, a `.` or a capital (`swe-bench/lite`,
    // `humaneval-rs.v2`) would build a name that fails at `join` — dispatch would die at the
    // room, AFTER the caller believes a run started. Asserting through airc's own constructor
    // rather than a hand-copied charset, so the two can never drift.
    #[test]
    fn a_derived_run_room_name_is_always_a_name_airc_accepts() {
        for bench in [
            "humaneval-rs",
            "swe-bench/lite",       // the `/` the design-of-record path form wants
            "HumanEval.Rs v2",      // capitals, a dot, and a space
            "tool_bugfix_rs",
        ] {
            let name = default_run_room_name(bench, 1_786_000_000);
            airc_lib::ChannelName::new(&name)
                .unwrap_or_else(|e| panic!("derived room {name:?} from {bench:?} is unusable: {e}"));
        }
        // Stamped, so two runs of one benchmark are two rooms — the whole point.
        assert_ne!(
            default_run_room_name("humaneval-rs", 1),
            default_run_room_name("humaneval-rs", 2),
        );
    }

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
        // Every COMMITTED gym's eval_set must resolve through the gym resolver.
        // A FETCHED gym — the new class ds-1000 introduced: eval_set AND source_url
        // both present — legitimately resolves only after `benchmark/fetch` converts
        // it into the gym cache, so on a fresh checkout its eval_set correctly does
        // NOT resolve. The invariant for that class is different and equally pinned:
        // its eval_set must NOT shadow a committed gym (a name collision would make
        // the fetch silently overwrite a bundled suite's identity).
        for b in ks.iter().filter(|b| b.eval_set.is_some()) {
            let fetched = b.source_url.is_some();
            match crate::cognition::gym::resolve_gym(b.eval_set.unwrap()) {
                Ok((origin, _)) if fetched => assert!(
                    !origin.starts_with("embedded:"),
                    "fetched benchmark '{}' shadows committed gym '{origin}'",
                    b.name
                ),
                Ok(_) => {}
                Err(e) if fetched => assert!(
                    e.contains("could not be resolved"),
                    "fetched benchmark '{}' failed oddly: {e}",
                    b.name
                ),
                Err(e) => panic!("benchmark '{}' eval_set does not resolve: {e}", b.name),
            }
        }
    }

    // The dispatch idempotence key (#417). These pin the ONE property the gate
    // rests on: the key a dispatch computes for a task and the key parsed back
    // off that task's own live card are the same string. If they ever diverge,
    // dedupe silently stops working and the board refills with duplicates —
    // which is exactly the state these tests were written from (124 bench cards
    // for 51 distinct tasks, sympy__sympy-24152 holding 15 of them).
    mod dispatch_identity {
        use super::*;

        // what this catches: key/title drift. The key MUST be a prefix of the
        // rendered title, so a card on the board can be matched back to the task
        // that would produce it. A future edit to either function that breaks the
        // prefix relation fails here instead of silently duplicating cards.
        #[test]
        fn a_rendered_title_yields_back_its_own_key() {
            for (bench, task, prompt) in [
                ("swe-bench-lite", "sympy__sympy-24152", "Bug in expand of TensorProduct"),
                ("hard-rs", "rle_roundtrip", "Implement run-length encoding and decoding"),
                // A prompt long enough to be truncated with an ellipsis.
                ("frontier-rs", "dijkstra", &"x".repeat(200)),
                // A prompt containing colons must not widen the key past the FIRST one.
                ("hard-rs", "spiral_order", "note: returns Vec<i32>: in spiral order"),
            ] {
                let title = dispatch_card_title(bench, task, prompt);
                let key = dispatch_card_key(bench, task);
                assert!(
                    title.starts_with(&key),
                    "key must be a prefix of its own title\n  title: {title}\n  key:   {key}"
                );
                assert_eq!(
                    bench_card_key(&title),
                    Some(key.as_str()),
                    "parsing a rendered title must recover exactly the constructed key ({title})"
                );
            }
        }

        // what this catches: two DIFFERENT tasks (or the same task id under two
        // different benchmarks) must never collide onto one key — a collision
        // would suppress a legitimate card as a false duplicate.
        #[test]
        fn distinct_tasks_never_share_a_key() {
            let a = dispatch_card_key("swe-bench-lite", "sympy__sympy-24152");
            let b = dispatch_card_key("swe-bench-lite", "sympy__sympy-24066");
            let c = dispatch_card_key("swe-bench-verified", "sympy__sympy-24152");
            assert_ne!(a, b, "different task ids must differ");
            assert_ne!(a, c, "same task under a different benchmark must differ");
        }

        // what this catches: a hand-written card sharing the board must never be
        // read as a benchmark task key. `bench_card_key` returning Some for an
        // ordinary card would let unrelated work suppress a real dispatch.
        #[test]
        fn a_non_benchmark_card_has_no_task_key() {
            for title in [
                "Stage-on-claim: SWE checkout follows the CLAIMER",
                "[not-a-bench] whatever: text",
                "",
                // Marker present but no colon at all — not a dispatchable card.
                "[bench swe-bench-lite] malformed title with no colon",
            ] {
                assert_eq!(
                    bench_card_key(title),
                    None,
                    "non-benchmark title must yield no key: {title:?}"
                );
            }
        }
    }

    // The prune's selection rule. These guard a DESTRUCTIVE operation, so the
    // bar is: never close work someone is doing, and never resolve a genuine
    // two-citizen collision by cancelling one of them.
    mod duplicate_selection {
        use super::*;
        use crate::persona::card_holder::Hold;

        // what this catches: THE unacceptable failure — closing a card a citizen
        // is actively working. Every arrangement of a held card among duplicates
        // must leave that card open.
        #[test]
        fn a_held_card_is_never_closed() {
            for holds in [
                vec![Hold::Held, Hold::Unclaimed],
                vec![Hold::Unclaimed, Hold::Held],
                vec![Hold::Unclaimed, Hold::Held, Hold::Lapsed],
                vec![Hold::Lapsed, Hold::Lapsed, Hold::Held],
            ] {
                let held: Vec<usize> = holds
                    .iter()
                    .enumerate()
                    .filter(|(_, h)| matches!(h, Hold::Held))
                    .map(|(i, _)| i)
                    .collect();
                let (to_close, _) = duplicates_to_close(&holds);
                for h in held {
                    assert!(
                        !to_close.contains(&h),
                        "index {h} is HELD and must never be closed (holds={holds:?}, \
                         to_close={to_close:?})"
                    );
                }
                // And the prune must actually do something about the rest.
                assert_eq!(
                    to_close.len(),
                    holds.len() - 1,
                    "exactly one card survives when a single card is held"
                );
            }
        }

        // what this catches: two citizens genuinely on the same task. Cancelling
        // either would destroy real in-flight work, so BOTH survive and the caller
        // is told it is contended.
        #[test]
        fn a_contended_task_keeps_every_holder_and_is_reported() {
            let holds = vec![Hold::Held, Hold::Held, Hold::Unclaimed, Hold::Lapsed];
            let (to_close, contended) = duplicates_to_close(&holds);
            assert!(contended, "two live claims on one task must report contention");
            assert_eq!(
                to_close,
                vec![2, 3],
                "only the unheld duplicates are closed; both holders survive"
            );
        }

        // what this catches: the ordinary case — nobody is on any of them, so the
        // choice only has to be deterministic. Keep the first.
        #[test]
        fn unheld_duplicates_collapse_to_the_first() {
            let holds = vec![Hold::Unclaimed, Hold::Unclaimed, Hold::Lapsed];
            let (to_close, contended) = duplicates_to_close(&holds);
            assert_eq!(to_close, vec![1, 2]);
            assert!(!contended);
        }

        // what this catches: a task with ONE card is not a duplicate and must be
        // left completely alone — a prune that closed singletons would empty the
        // board instead of deduplicating it.
        #[test]
        fn a_single_card_is_never_touched() {
            for holds in [vec![], vec![Hold::Unclaimed], vec![Hold::Held], vec![Hold::Lapsed]] {
                let (to_close, contended) = duplicates_to_close(&holds);
                assert!(to_close.is_empty(), "singleton/empty must yield no closes: {holds:?}");
                assert!(!contended);
            }
        }
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
    /// The failing FAIL_TO_PASS run's output tail (capped) — the assertion diff.
    /// Names say WHICH test failed; this says WHAT it printed, which is the half
    /// a next attempt (or a human reviewer) actually reasons from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub failure_excerpt: Option<String>,
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
            failure_excerpt: v.failure_excerpt,
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

/// Paths that are NEVER part of a solution — the ONE list, shared by every reading of
/// "her work" ([`workspace_candidate_diff`] here and `agent::solve::workspace_patch`).
///
/// Two kinds, and the second is a SECURITY boundary, not tidiness:
///
/// 1. Build/cache byproducts. A `python3 -c ...` verify step left
///    `__pycache__/calc.cpython-314.pyc` in a graded patch; real SWE-bench/aider patches are
///    SOURCE-only. Anything a task might legitimately produce (`build`/`dist`/`target`) is kept.
///
/// 2. **Agent-scope state the SUBSTRATE writes into her tree.** airc creates its scope at the
///    enclosing git root, so a citizen working inside a cloned bench repo gets `.airc/` —
///    `events.sqlite`, a work-board cache, and **`identity.key`, a private keypair** — created
///    under the repo she is being graded on. This has bitten twice: card b34f7eb5, where Atlas's
///    first grade carried 91KB of staged `.airc` blobs and the fresh clone refused the WHOLE
///    candidate (a real fix voided by files no solver wrote); and 2026-08-18, where
///    sympy-22714's tree still held `.airc/identity.key` with git status `A` — already
///    intent-added, because `workspace_patch` ran `git add -A -N` with an exclude list that
///    lacked `.airc`. The grader was safe (it excluded `.airc` inline) but `workspace_patch`
///    was not, and IT is the reading that feeds `files_changed` → `format_solve_lesson` →
///    the curriculum. A credential could have been written into training data as
///    "I changed: .airc/identity.key".
///
/// That divergence is exactly what [`workspace_candidate_diff`]'s own doc warned about — "a
/// second inline `git diff` would drift on the exclude rules" — so the rule now lives in ONE
/// place and both readings consume it ([[the-compression-principle]]).
pub(crate) const SOLUTION_PATH_EXCLUDES: &[&str] = &[
    // Agent/substrate scope — never authored by the solver, and credential-bearing.
    ":(exclude,glob)**/.airc/**",
    ":(exclude,glob)**/.continuum/**",
    // Build + cache byproducts.
    ":(exclude,glob)**/__pycache__/**",
    ":(exclude,glob)**/*.pyc",
    ":(exclude,glob)**/*.pyo",
    ":(exclude,glob)**/.pytest_cache/**",
    ":(exclude,glob)**/.mypy_cache/**",
    ":(exclude,glob)**/.ruff_cache/**",
    ":(exclude,glob)**/node_modules/**",
    ":(exclude,glob)**/.DS_Store",
];

/// The candidate diff of a solver workspace — the ONE reading of "her work"
/// (grade_swe's candidate arm and agent/solve's attempt-patch receipt both
/// call this; a second inline `git diff` would drift on the exclude rules).
/// `diff HEAD` (not bare `diff`) so STAGED edits count as her work too, and
/// [`SOLUTION_PATH_EXCLUDES`] keeps substrate-authored files out — see its doc
/// for the two incidents that make the `.airc` entry load-bearing.
pub(crate) fn workspace_candidate_diff(ws: &str) -> Result<String, CommandError> {
    workspace_candidate_diff_from(ws, None)
}

/// Her work is everything since the instance's BASE COMMIT — committed,
/// staged, and unstaged alike. The old `diff HEAD` collector read only the
/// dirty tree, so a citizen who COMMITTED her fix (sympy-12481, 2026-08-27:
/// "Fix Permutation constructor to compose non-disjoint cycles left-to-right",
/// the exact task, sitting in a commit) graded as "no candidate patch" — the
/// harness punishing her best engineering habit. With `base` given, diff from
/// there; unknown rev (odd staging) falls back to the dirty-tree read rather
/// than failing the grade.
pub(crate) fn workspace_candidate_diff_from(
    ws: &str,
    base: Option<&str>,
) -> Result<String, CommandError> {
    if let Some(base) = base {
        let mut args: Vec<&str> = vec!["diff", base, "--", "."];
        args.extend_from_slice(SOLUTION_PATH_EXCLUDES);
        let out = std::process::Command::new("git")
            .args(&args)
            .current_dir(ws)
            .output()
            .map_err(|e| CommandError::Internal(format!("could not read {ws}'s diff: {e}")))?;
        if out.status.success() {
            return Ok(String::from_utf8_lossy(&out.stdout).to_string());
        }
        // Unknown base in this tree — fall through to the dirty-tree read.
    }
    let mut args: Vec<&str> = vec!["diff", "HEAD", "--", "."];
    args.extend_from_slice(SOLUTION_PATH_EXCLUDES);
    let out = std::process::Command::new("git")
        .args(&args)
        .current_dir(ws)
        .output()
        .map_err(|e| CommandError::Internal(format!("could not read {ws}'s diff: {e}")))?;
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

/// The `benchmark/swe-grade` body, callable without a command context — the
/// hands-free autograde on `agent/solve` completion invokes the SAME grader
/// (fresh clone at base_commit, held-out tests, experience-stream write) as
/// the operator verb. One grader, never two.
pub(crate) async fn grade_swe(p: SweGradeParams) -> Result<SweGradeResult, CommandError> {
    // Dataset resolution: an explicit dataset wins; otherwise SEARCH every known
    // SWE dataset for the instance. The old default hardcoded Lite, so any
    // Verified-only instance was ungradeable — glass-boxed live 2026-08-26:
    // astropy-13236 (a swe-bench-verified dispatch) auto-graded as
    // "[not_found] … not found in princeton-nlp/SWE-bench_Lite" and the whole
    // attempt's verdict vanished. The instance names its dataset; the grader's
    // job is to find it, not to guess one.
    let candidate_datasets: Vec<String> = match p.dataset.clone() {
        Some(d) => vec![d],
        None => known_benchmarks()
            .iter()
            .filter_map(|b| b.swe_dataset())
            .map(|d| d.to_string())
            .collect(),
    };
    let mut instance_row = None;
    let mut searched = Vec::new();
    for dataset in &candidate_datasets {
        let rows = swe_bench::load_dataset(dataset)
            .await
            .map_err(CommandError::Internal)?;
        if let Some(r) = rows.into_iter().find(|r| r.instance_id == p.instance) {
            instance_row = Some((r, dataset.clone()));
            break;
        }
        searched.push(dataset.clone());
    }
    let Some((instance, dataset)) = instance_row else {
        return Err(CommandError::NotFound(format!(
            "{} not found in any known SWE dataset (searched: {})",
            p.instance,
            searched.join(", ")
        )));
    };
    let _ = &dataset; // named for the receipt below; instance carries everything else

    // Resolve the candidate patch. A workspace's diff is READ here but graded in a fresh
    // clone below — where the solver worked is never where the score is taken.
    // RESOLVE WHICH COPY, rather than trusting the caller to have picked right.
    //
    // The same instance is legitimately staged into MULTIPLE citizens' workspaces —
    // dispatch round-robins over the roster. On 2026-08-18 `astropy__astropy-14995` sat in
    // Atlas's tree (dirty: a real fix) AND Asha's (clean: staged, never worked), and an
    // operator grade pointed at the clean one returned a confident `resolved: false` for
    // work that existed ten directories away. Deletion was never the risk; AMBIGUITY was.
    //
    // So an omitted `workspace` no longer means "no candidate" — it means ASK. The answer
    // comes from `staged_workspace`, the module that already owns "which checkout is this
    // instance", and it refuses on ambiguity for the same reason its sibling does: grading
    // either of two worked copies scores one citizen's diff against another's card.
    let resolved_workspace: Option<String> = match p.workspace.clone() {
        Some(ws) => Some(ws),
        None if p.gold.unwrap_or(false) || p.patch.is_some() => None,
        None => {
            use crate::persona::staged_workspace::{grade_target, owners_of, GradeTarget};
            let copies = owners_of(&instance.instance_id);
            match grade_target(&copies) {
                GradeTarget::One(path) => {
                    crate::probe!(
                        class = "benchmark.grade.workspace_resolved",
                        instance = %instance.instance_id,
                        staged_copies = copies.len(),
                        path = %path.display(),
                        "resolved the one WORKED staged copy — never guessed between citizens"
                    );
                    Some(path.to_string_lossy().to_string())
                }
                // No worked copy: fall through with no candidate. The empty-candidate guard
                // below turns that into an ABSENCE, which is the honest verdict.
                GradeTarget::NoWork => None,
                GradeTarget::Ambiguous(paths) => {
                    return Err(CommandError::Invalid(format!(
                        "{} is staged with real work in {} citizens' workspaces — refusing to \
                         guess which one this grade is about, because grading either scores one \
                         citizen's diff against the other's card. Pass workspace=<path> \
                         explicitly. Candidates: {}",
                        instance.instance_id,
                        paths.len(),
                        paths
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )));
                }
            }
        }
    };

    let candidate: Option<String> = if p.gold.unwrap_or(false) {
        Some(instance.patch.clone())
    } else if let Some(ws) = resolved_workspace.as_ref() {
        Some(workspace_candidate_diff_from(ws, Some(&instance.base_commit))?)
    } else {
        p.patch.clone()
    };
    let patch_bytes = candidate.as_ref().map(|c| c.len()).unwrap_or(0);

    // AN EMPTY CANDIDATE IS AN ABSENCE, NOT A ZERO — refuse before spending a clone on it.
    //
    // Grading a pristine tree at `base_commit` ALWAYS yields `resolved: false` with
    // `gate_ok: true`: the FAIL_TO_PASS test correctly fails because the bug is still there.
    // That is byte-identical downstream to a citizen who tried and missed, so recording it
    // manufactures a capability zero out of nothing — the #384/#386 class, and the exact
    // failure this arm's own `gold_gate` comment exists to prevent one screen above.
    //
    // Caught by the positive control on 2026-08-18, minutes after verdict persistence landed:
    // re-grading astropy-14995 (a REAL pass, watched at F2P 1/1 / P2P 40/40 that afternoon)
    // returned `patchBytes: 0, resolved: false` and PERSISTED it. Her tree had been re-cloned
    // — `git reflog` showed exactly two entries, `clone` then `checkout`, no work — so the
    // artifact was gone and the harness scored its own absence as her failure. One run of the
    // control turned a silent laundering bug into a named one.
    //
    // Also stops the experience stream teaching "you failed" from a tree that was wiped: the
    // append below gates on `error.is_none()`, so an absence correctly teaches nothing.
    if !p.gold.unwrap_or(false) && patch_bytes == 0 {
        return Ok(SweGradeResult::from((
            SweVerdict {
                instance_id: instance.instance_id.clone(),
                error: Some(format!(
                    "no candidate patch to grade for {} — the workspace holds no diff (a fresh \
                     or reset checkout), so there is nothing to score. This is an ABSENCE, not \
                     a failed attempt: grading a pristine tree would report resolved=false for \
                     a citizen who never got the chance.",
                    instance.instance_id
                )),
                ..Default::default()
            },
            patch_bytes,
        )));
    }

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
    // THE SPINE CHECK IS NOW ENFORCED, not just run. `gold` graded through the plain
    // `grade` path returned a bare `resolved: false`, which is byte-identical downstream to
    // a citizen's capability zero — so the `gold` doc's own demand ("if it does not, the
    // environment is wrong and no other number from it means anything") was a sentence
    // addressed to a human and enforced by nobody. `gold_gate` stamps the verdict's `error`
    // with WHY, and an `error` is contractually an ABSENCE, never a tallied failure
    // (see `SweVerdict::error`). One path, so every caller inherits the labelling.
    let verdict = if p.gold.unwrap_or(false) {
        swe_bench::gold_gate(&instance).await
    } else {
        swe_bench::grade(&instance, candidate.as_deref()).await
    };

    // PERSIST THE VERDICT before anything else consumes it. Until 2026-08-18 this arm
    // computed a score, taught from it, and returned it — writing nothing durable. Two real
    // Lite resolutions were watched passing that afternoon and left no trace; `benchmark/runs`
    // went on rendering both artifacts as `ungraded`, and the day's honest rate could not be
    // stated from anything the system held. A measurement the system cannot remember is not a
    // measurement. `record_verdict` itself refuses gold and errored verdicts, so the board can
    // never be laundered by a positive control or an env fault (see its doc).
    match swe_bench::record_verdict(&verdict, p.gold.unwrap_or(false)) {
        Ok(Some(path)) => crate::probe!(
            class = "benchmark.verdict.recorded",
            instance = %verdict.instance_id,
            resolved = verdict.resolved,
            path = %path.display(),
            "verdict persisted — the board and every later reader now see this grade"
        ),
        Ok(None) => {}
        // Fail LOUD in the log but never fail the grade: the verdict in hand is still true,
        // and refusing to return it would lose the measurement twice over.
        Err(e) => tracing::warn!(
            instance = %verdict.instance_id,
            error = %e,
            "VERDICT NOT PERSISTED — this grade is real but the system will forget it"
        ),
    }

    // #319: a WORKSPACE grade is a citizen's lived, objectively judged work —
    // append it to her experience stream. Only her: the gold/raw-patch arms are
    // harness plumbing, not experience. And only a REAL verdict: an errored run
    // is an ABSENCE (harness fault), and teaching from a harness failure would
    // corrupt the reward signal (`an_errored_verdict_is_an_absence_not_a_zero`).
    if verdict.error.is_none() {
        // The RESOLVED workspace, not the caller's parameter: a grade that resolved the
        // owner must teach THAT citizen, or the lesson lands on nobody (and, before
        // resolution existed, could have landed on whoever the operator happened to name).
        if let Some(peer_dir) = resolved_workspace
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
            if let Err(e) = crate::cognition::experience::append_experience(&peer_dir, &episode) {
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

/// Resolve a solver to the ONE matching `citizens/peers/<uuid>/` directory. Accepts a
/// citizen NAME (resolved through the live roster — the SAME `get_by_agent_name` identity
/// path `benchmark/dispatch` uses), a full peer UUID, or a hex prefix. 0 or >1 matches
/// fail loud with the candidates named — a grade against the wrong citizen's workspace is
/// a falsified result, never a best-effort. [[the-grid-identity-spine-durable-id-fluid-location]]
pub(crate) fn resolve_solver_dir(
    home: &std::path::Path,
    solver: &str,
) -> Result<(String, std::path::PathBuf), CommandError> {
    let peers = home.join("citizens").join("peers");

    // Identity first: a NAME ("Asha") resolves through the live roster to her durable
    // peer_id → her workspace dir. A name is never a hex UUID prefix, so without this a
    // `--solver=Asha` always failed "no citizen workspace matches" — the identity gap this
    // fixes. Only fall through to peer-UUID-prefix matching if the name doesn't resolve
    // (offline citizen, or the caller genuinely passed a uuid). None in unit tests (no live
    // roster) → straight to the prefix path, so existing behavior is preserved.
    if let Some(rt) = crate::persona::PersonaAircRuntimeRegistry::try_global()
        .and_then(|reg| reg.get_by_agent_name(solver))
    {
        let uuid = rt.airc().peer_id().as_uuid().to_string();
        let dir = peers.join(&uuid);
        if dir.is_dir() {
            return Ok((uuid, dir));
        }
    }

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
        if name
            .to_ascii_lowercase()
            .replace('-', "")
            .starts_with(&needle)
        {
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
    /// `citizens/peers/` exactly like `benchmark/swe-grade`.
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
            assert!(card
                .infra_error
                .as_deref()
                .unwrap_or("")
                .contains("deadline"));
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

        // what this catches: the dispatch-time `state: running` marker (#2246 —
        // four live solves ran INVISIBLE to this projection for their whole
        // first attempt because nothing was journaled until an attempt ended)
        // must fold as a live `active` card with the solver named, never as
        // failed/resolved, so the run is on the board from second zero.
        #[test]
        fn a_running_marker_folds_active_with_the_solver_named() {
            let now: u64 = 10_000_000_000;
            let fresh = now - 5_000;
            let marker = json!({"state": "running", "run_id": "r5",
                                "persona_id": "atlas-uuid", "workspace": "/w/swe/x"});
            let card = fold_run_card("r5", Some(&marker), None, fresh, now);
            assert_eq!(card.phase, "active");
            assert!(!card.stalled);
            assert_eq!(card.solver.as_deref(), Some("atlas-uuid"));
            assert_eq!(card.resolved, None, "no grade yet — never a verdict");
        }

        // what this catches: an UNGRADEABLE grade must fold as an ABSENCE, never
        // a capability zero. `SweGradeResult.error` documents the contract ("an
        // ABSENCE, not a zero, and must never be tallied as a failed attempt")
        // and the grader proves it — for the env class it re-runs the PRISTINE
        // tree before declaring one. This projection read only the RESULT's
        // error, so a grade-level fault folded `resolved: false` + phase
        // `failed`, indistinguishable from a citizen who tried and lost.
        // Measured 2026-08-13 on sympy__sympy-11400: p2p 0/29 on the pristine
        // tree, and 8 of 36 instances graded UNGRADEABLE on that box — the
        // denominator of every rate read off this projection was poisoned.
        #[test]
        fn an_ungradeable_grade_folds_as_absence_never_a_capability_zero() {
            let now: u64 = 10_000_000_000;
            let fresh = now - 5_000;
            let result = json!({"persona_id": "asha-uuid", "acts": 12,
                                "instance": "sympy__sympy-11400", "attempt": 1});
            let ungradeable = json!({
                "instance": "sympy__sympy-11400", "resolved": false, "gateOk": false,
                "passToPassPassed": 0, "passToPassTotal": 29, "patchBytes": 402,
                "error": "UNGRADEABLE — PASS_TO_PASS passes 0 of 29 on the PRISTINE tree: \
                          the suite does not run in this environment, so every score from \
                          this tree is an env fault, never a capability verdict."});
            let card = fold_run_card("r6", Some(&result), Some(&ungradeable), fresh, now);
            assert_eq!(
                card.resolved, None,
                "an env fault is an ABSENCE — `Some(false)` is the lie that reads as a \
                 citizen who tried and failed"
            );
            assert_eq!(card.phase, "ungradeable", "never `failed`, never `resolved`");
            assert!(
                card.infra_error
                    .as_deref()
                    .is_some_and(|e| e.contains("UNGRADEABLE")),
                "the REASON rides with the absence — one field means 'no valid verdict, \
                 and why', fed by both the result's error and the grade's"
            );

            // Positive control: the SAME shape with no grade error is a real
            // verdict and must still fold as a capability zero, or this test
            // would pass by simply never reporting failure.
            let honest_zero = json!({
                "instance": "sympy__sympy-11400", "resolved": false, "gateOk": true,
                "passToPassPassed": 29, "passToPassTotal": 29, "patchBytes": 402});
            let card = fold_run_card("r7", Some(&result), Some(&honest_zero), fresh, now);
            assert_eq!(card.resolved, Some(false), "a graded miss IS a zero");
            assert_ne!(card.phase, "ungradeable");
            assert!(card.infra_error.is_none());
        }

        // what this catches: the board facts (#329) — instance + attempt N/M
        // projected from the result ledger, and patch_bytes derived LIVE from
        // the result's own diff before any grade exists (the "patch is
        // forming" leading indicator), while a real grade's byte count stays
        // authoritative the moment one lands.
        #[test]
        fn board_facts_project_and_live_patch_yields_to_the_grade() {
            let now: u64 = 10_000_000_000;
            let fresh = now - 5_000;
            let result = json!({"persona_id": "anon-uuid", "acts": 7,
                                "instance": "sympy__sympy-21055",
                                "attempt": 2, "max_attempts": 3,
                                "patch": "diff --git a/x b/x\n+fix\n"});
            // Pre-grade: live patch length from the result's own diff.
            let card = fold_run_card("r6", Some(&result), None, fresh, now);
            assert_eq!(card.instance.as_deref(), Some("sympy__sympy-21055"));
            assert_eq!(card.attempt, Some(2));
            assert_eq!(card.max_attempts, Some(3));
            assert_eq!(card.patch_bytes, Some(24), "live diff length pre-grade");
            // Graded: the grade's byte count wins over the live derivation.
            let grade = json!({"resolved": false, "patchBytes": 1299});
            let card = fold_run_card("r6", Some(&result), Some(&grade), fresh, now);
            assert_eq!(card.patch_bytes, Some(1299), "grade is authoritative");
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRunsParams.ts"
)]
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchRunCard.ts"
)]
pub struct BenchRunCard {
    pub run_id: String,
    /// Instance under test ("sympy__sympy-24066") — from the result ledger's
    /// staged-checkout name (#329: the board names WHAT, not just who).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub instance: Option<String>,
    /// Attempt N of `max_attempts` — the N-chances counter, live per ledger write.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub attempt: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub max_attempts: Option<u32>,
    /// Solver persona (from the result ledger; absent while attempt 1 is
    /// still in flight and nothing has been written yet).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub solver: Option<String>,
    /// `resolved` | `failed` (loud infra marker, incl. #2180 stalls the
    /// deadline caught) | `active` (artifact activity within the stall
    /// window) | `quiet` (non-terminal AND silent past the window — the
    /// shape the projection exists to make visible) | `ungraded` (a staged
    /// workspace holds a real diff that no grade has ever seen — durable
    /// work awaiting a verdict, NOT a stall; see
    /// [`scan_workspace_artifact_cards`]).
    pub phase: String,
    /// True exactly when `phase == "quiet"`.
    pub stalled: bool,
    /// Epoch ms of the newest artifact write (result or grade ledger).
    #[ts(type = "number")]
    pub last_activity_ms: u64,
    #[ts(type = "number")]
    pub age_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
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
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRunsResult.ts"
)]
pub struct BenchmarkRunsResult {
    pub runs: Vec<BenchRunCard>,
    /// How many cards MATCHED before `limit` truncated — so a page is never mistaken
    /// for the whole set.
    ///
    /// Measured cost of not having this (2026-08-21, on me): `benchmark/runs` returned
    /// 20 rows carrying 1 resolved instance, and I read that as the system's whole
    /// history and as evidence that grades were being lost on the way to the board.
    /// Both wrong. The projection reads grade siblings AND durable verdicts correctly;
    /// there were simply 37 graded instances and `limit.unwrap_or(20)` showed the 20
    /// most recent. The SECOND real pass (`sympy__sympy-13480`) was older than the
    /// window, so the board looked like a 1-pass history and nobody knew otherwise.
    ///
    /// Same convention `debug/probes/query` already uses ("MATCHED versus returned, so
    /// a page is never mistaken for the whole"). Silent truncation reads as "that's
    /// everything" — on the command an operator uses to ask how the benchmark is going,
    /// that is the worst possible failure shape.
    #[ts(type = "number")]
    pub matched: u32,
    /// Human-readable statement of the two numbers, so the truncation is visible in a
    /// glance at the receipt and not only to a caller who compares two fields.
    pub summary: String,
}

/// One run-ledger scan: the cards a caller asked for, plus how many there were BEFORE
/// truncation.
///
/// Returned as a struct rather than a bare `Vec` so the count cannot be dropped on the
/// way out — the positron board consumes `.cards` and ignores the total, while the
/// command reports both. A tuple would have let either caller silently discard it.
pub(crate) struct RunScan {
    pub cards: Vec<BenchRunCard>,
    /// Cards that matched the filter before `limit` was applied.
    pub matched: usize,
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
        v.and_then(|v| v.get(k))
            .and_then(|x| x.as_str())
            .map(String::from)
    };
    let n = |v: Option<&serde_json::Value>, k: &str| {
        v.and_then(|v| v.get(k))
            .and_then(|x| x.as_u64())
            .map(|x| x as u32)
    };
    let arr = |v: Option<&serde_json::Value>, k: &str| -> Vec<String> {
        v.and_then(|v| v.get(k))
            .and_then(|x| x.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|e| e.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default()
    };
    // ABSENCE vs ZERO, on the board. A grade carrying `error` is a harness/env
    // fault the grader PROVED — for the env class it re-runs the PRISTINE tree
    // first, so a genuinely broken patch is never mislabelled — and
    // `SweGradeResult.error` documents the contract in its own doc comment: "a
    // result with `error` is an ABSENCE, not a zero, and must never be tallied
    // as a failed attempt." This projection honoured that for the RESULT's error
    // and ignored the GRADE's, so an env fault folded as `resolved: false` +
    // phase `failed` — indistinguishable from a citizen who tried and lost.
    // Measured 2026-08-13: 8 of 36 instances (22%) grade UNGRADEABLE on this box.
    // `infra_error` already means "no valid verdict, and why", so it takes both
    // sources rather than growing a second field, and `resolved` returns to None
    // — the same "no verdict" the pre-grade card carries, because that is the truth.
    let grade_error = s(grade, "error");
    let ungradeable = grade_error.is_some();
    let resolved = if ungradeable {
        None
    } else {
        grade
            .and_then(|g| g.get("resolved"))
            .and_then(|x| x.as_bool())
    };
    let infra_error = s(result, "infra_error")
        .or_else(|| s(result, "error"))
        .or(grade_error);
    let failed_marker = result
        .and_then(|r| r.get("failed"))
        .and_then(|x| x.as_bool())
        .unwrap_or(false);
    let age_secs = now_ms.saturating_sub(last_activity_ms) / 1000;
    let phase = if resolved == Some(true) {
        "resolved"
    } else if ungradeable {
        // Ahead of `failed`: a run can carry both a failed marker and an
        // ungradeable grade, and the absence is the more truthful of the two.
        "ungradeable"
    } else if failed_marker {
        "failed"
    } else if age_secs < RUN_STALL_WINDOW_SECS {
        "active"
    } else {
        "quiet"
    };
    let ratio = |g: Option<&serde_json::Value>, passed: &str, total: &str| match (
        n(g, passed),
        n(g, total),
    ) {
        (Some(p), Some(t)) => Some(format!("{p}/{t}")),
        _ => None,
    };
    BenchRunCard {
        run_id: run_id.to_string(),
        instance: s(result, "instance"),
        attempt: n(result, "attempt"),
        max_attempts: n(result, "max_attempts"),
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
        // Graded diff size when a grade exists; before any grade, the RESULT's
        // own patch length — the live "a patch is forming" leading indicator
        // (#329). One field, grade-authoritative, never both shown.
        patch_bytes: n(grade, "patchBytes").or_else(|| {
            result
                .and_then(|r| r.get("patch"))
                .and_then(|p| p.as_str())
                .map(|p| p.len() as u32)
        }),
        failed_tests: arr(grade, "failedTests"),
        infra_error,
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkScoreboardParams.ts")]
pub struct BenchmarkScoreboardParams {}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkScoreRow.ts")]
pub struct BenchmarkScoreRow {
    /// Catalog benchmark this tallies (e.g. `swe-bench-verified`).
    pub benchmark: String,
    /// Distinct instances with a REAL verdict (error-free; absences never tally).
    #[ts(type = "number")]
    pub attempted: u32,
    /// Of those, resolved (all fail-to-pass passed, gate held).
    #[ts(type = "number")]
    pub resolved: u32,
    /// The dataset's full size — the leaderboard denominator this samples.
    #[ts(type = "number")]
    pub dataset_size: u32,
    /// Resolved instance ids — the receipt pointers.
    pub resolved_instances: Vec<String>,
    /// The harness build(s) that produced this row's verdicts, newest-first.
    ///
    /// MORE THAN ONE MEANS THE ROW BLENDS ERAS — and a rate averaged over a
    /// moving instrument is not a measurement. Measured 2026-08-28: 19 of 32
    /// verdicts here had been scored across ten days by three harness builds,
    /// and regrading one from its IDENTICAL banked patch moved pass-to-pass
    /// from 0/40 to 40/40. Publishing an improvement curve over that is
    /// publishing the harness's changes as if they were hers. `<unstamped>`
    /// marks verdicts written before provenance existed.
    #[serde(default)]
    pub harness_builds: Vec<String>,
    /// ENV failures (verdict carries `error`): the harness could not measure
    /// the model at all — clone/env/patch infrastructure, NEVER a model miss.
    /// These are absences owing retakes, and they must read that way.
    #[ts(type = "number")]
    pub env_absences: u32,
    /// The env-absent instance ids, so the failure is chaseable per instance.
    pub env_absent_instances: Vec<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkScoreboardResult.ts")]
pub struct BenchmarkScoreboardResult {
    pub rows: Vec<BenchmarkScoreRow>,
    /// The REGIME every published number must carry: model, window, build, host.
    pub regime: String,
    pub summary: String,
}

#[derive(Default)]
pub struct BenchmarkScoreboard;

#[async_trait::async_trait]
impl ActionCommand for BenchmarkScoreboard {
    const NAME: &'static str = "benchmark/scoreboard";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "The SCORE rollup: per SWE-class benchmark, attempted vs resolved from the durable \
         verdicts on disk, with the serving REGIME (model, window, build sha, host) every \
         published claim must carry. ONE read for the operator, the README chart, and a \
         citizen grounding on how the team is scoring — instead of tallying verdict files \
         by hand.";
    type Params = BenchmarkScoreboardParams;
    type Output = BenchmarkScoreboardResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        _p: BenchmarkScoreboardParams,
    ) -> Result<BenchmarkScoreboardResult, CommandError> {
        let verdicts = swe_bench::recorded_verdicts();
        let mut rows = Vec::new();
        for spec in known_benchmarks() {
            let Some(dataset) = spec.swe_dataset() else {
                continue;
            };
            // Membership by the SAME loader the grade path searches with — one
            // source of truth for "which dataset does this instance belong to".
            let Ok(instances) = crate::cognition::swe_bench::load_dataset(dataset).await else {
                continue; // not fetched yet — an un-run benchmark, not an error
            };
            let ids: std::collections::HashSet<&str> =
                instances.iter().map(|i| i.instance_id.as_str()).collect();
            let mut attempted = 0u32;
            let mut resolved_instances = Vec::new();
            let mut env_absent_instances = Vec::new();
            // Which harness build(s) actually produced the verdicts behind this
            // rate. Collected from the TALLIED verdicts only — the absences are
            // not part of the claim.
            let mut builds: std::collections::BTreeSet<String> = Default::default();
            for (id, v) in &verdicts {
                if !ids.contains(id.as_str()) {
                    continue;
                }
                if v.error.is_some() || !v.gate_ok {
                    // Absence, never a tallied attempt — but never invisible
                    // either: an env failure the user can't see reads as a
                    // model miss in every retelling. A failed GATE is the same
                    // class: the control was broken, nothing was measured
                    // (belt-and-suspenders — record_verdict already refuses
                    // gate-failed verdicts, so this arm should never fire).
                    env_absent_instances.push(id.clone());
                    continue;
                }
                attempted += 1;
                builds.insert(if v.harness_build.is_empty() {
                    "<unstamped>".to_string()
                } else {
                    v.harness_build.clone()
                });
                if v.resolved {
                    resolved_instances.push(id.clone());
                }
            }
            rows.push(BenchmarkScoreRow {
                benchmark: spec.name.to_string(),
                attempted,
                resolved: resolved_instances.len() as u32,
                dataset_size: instances.len() as u32,
                resolved_instances,
                harness_builds: {
                    let mut b: Vec<String> = builds.into_iter().collect();
                    b.sort();
                    b
                },
                env_absences: env_absent_instances.len() as u32,
                env_absent_instances,
            });
        }
        let serving = crate::inference::llama_server::current_serving();
        let regime = format!(
            "model={} served_window={} build={} sha={} host={}-{}",
            serving.active_model.as_deref().unwrap_or("none-serving"), // absence stated, never a fake model id
            serving.served_context_window,
            env!("CONTINUUM_BUILD_NUMBER"),
            env!("CONTINUUM_BUILD_GIT_SHA"),
            std::env::consts::OS,
            std::env::consts::ARCH,
        );
        let summary = rows
            .iter()
            .filter(|r| r.attempted > 0 || r.env_absences > 0)
            .map(|r| {
                let env = if r.env_absences > 0 {
                    format!(
                        " · {} ENV failure(s) — not model misses, they owe retakes: {}",
                        r.env_absences,
                        r.env_absent_instances.join(", ")
                    )
                } else {
                    String::new()
                };
                // SAY IT WHEN THE RATE BLENDS ERAS. A number averaged over a
                // moving instrument is not a measurement, and this string is
                // what gets quoted into charts and READMEs — the exact place a
                // silent blend becomes a published claim.
                let mixed = if r.harness_builds.len() > 1 {
                    format!(
                        " · ⚠ MIXED HARNESS ERAS ({}) — regrade to one build before \
                         publishing this as a curve",
                        r.harness_builds.join(", ")
                    )
                } else {
                    String::new()
                };
                format!(
                    "{}: {}/{} resolved (of {} in the set){env}{mixed}",
                    r.benchmark, r.resolved, r.attempted, r.dataset_size
                )
            })
            .collect::<Vec<_>>()
            .join(" · ");
        let summary = if summary.is_empty() {
            "no real verdicts on disk yet — dispatch a round first".to_string()
        } else {
            summary
        };
        Ok(BenchmarkScoreboardResult {
            rows,
            regime,
            summary,
        })
    }
}

crate::register_stateless_command!(BenchmarkScoreboard);

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, ts_rs::TS, schemars::JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkValidateParams.ts")]
pub struct BenchmarkValidateParams {
    /// SWE-class benchmark to validate (default swe-bench-verified).
    #[serde(default)]
    #[ts(optional)]
    pub name: Option<String>,
    /// Cap on env classes to build (default all). Each class = one real
    /// checkout + env build for its representative instance.
    #[serde(default)]
    #[ts(optional, type = "number")]
    pub limit: Option<u32>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkValidateClass.ts")]
pub struct BenchmarkValidateClass {
    /// The env class: repo + era year.
    pub repo: String,
    pub era: String,
    /// The representative instance actually built.
    pub representative: String,
    /// Instances in the dataset this class covers.
    #[ts(type = "number")]
    pub covers: u32,
    /// Did checkout + env build + (for pytest repos) the trivial-test smoke pass?
    pub green: bool,
    /// The named wall when red — actionable, never a mystery.
    #[ts(optional)]
    pub wall: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkPlatformFingerprint.ts")]
pub struct BenchmarkPlatformFingerprint {
    /// The machine CLASS a coverage claim is keyed by (e.g. `m-series-macos`,
    /// `x86_64-linux`, `windows`) — the coarse key an alloy consumer matches.
    pub machine_class: String,
    pub os: String,
    pub arch: String,
    /// The load-bearing toolchain versions — the BITTEN-BY list, grown only
    /// when a new wall names a new dependency (2026-08-27 initial set: clang
    /// broke on `-march=native`, libomp/freetype were the matplotlib/sklearn
    /// walls, uv's interpreter shelf decided the py3.7 structural question).
    pub clang: String,
    pub libomp: bool,
    pub freetype: bool,
    pub uv: String,
    /// Interpreter majors uv can actually provide on this platform.
    pub pythons: Vec<String>,
}

impl BenchmarkPlatformFingerprint {
    fn capture() -> Self {
        let run = |cmd: &str, args: &[&str]| -> String {
            std::process::Command::new(cmd)
                .args(args)
                .output()
                .ok()
                .map(|o| String::from_utf8_lossy(&o.stdout).lines().next().unwrap_or("").trim().to_string())
                .filter(|s| !s.is_empty())
                .unwrap_or_else(|| "absent".into())
        };
        let machine_class = match (std::env::consts::OS, std::env::consts::ARCH) {
            ("macos", "aarch64") => "m-series-macos",
            ("macos", _) => "intel-macos",
            ("linux", a) if a == "aarch64" => "arm-linux",
            ("linux", _) => "x86_64-linux",
            ("windows", _) => "windows",
            _ => "other",
        }
        .to_string();
        let pythons = std::process::Command::new("uv")
            .args(["python", "list", "--only-installed"])
            .output()
            .ok()
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .filter_map(|l| l.split_whitespace().next().map(str::to_string))
                    .take(8)
                    .collect()
            })
            .unwrap_or_default();
        Self {
            machine_class,
            os: std::env::consts::OS.into(),
            arch: std::env::consts::ARCH.into(),
            clang: run("clang", &["--version"]),
            libomp: std::path::Path::new("/opt/homebrew/opt/libomp/include/omp.h").exists()
                || std::path::Path::new("/usr/lib/libomp.so").exists(),
            freetype: std::process::Command::new("pkg-config")
                .args(["--exists", "freetype2"])
                .status()
                .map(|s| s.success())
                .unwrap_or(false),
            uv: run("uv", &["--version"]),
            pythons,
        }
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, ts_rs::TS)]
#[ts(export, export_to = "../../../protocol/typescript/benchmark/BenchmarkValidateResult.ts")]
pub struct BenchmarkValidateResult {
    /// The platform this coverage map is TRUE FOR — coverage claims are always
    /// per-machine-class; an alloy consumer matches this before trusting them.
    pub platform: BenchmarkPlatformFingerprint,
    pub classes: Vec<BenchmarkValidateClass>,
    /// Instances covered by GREEN classes / dataset size — THE coverage number.
    #[ts(type = "number")]
    pub instances_green: u32,
    #[ts(type = "number")]
    pub dataset_size: u32,
    pub summary: String,
}

/// `benchmark/validate` — the harness proves ITSELF before anyone trusts a
/// round with it. One representative per (repo, era-year) class runs the SAME
/// checkout + env-build seams every real solve and grade use; the result is
/// the env-coverage map ("N of 500 instances sit in classes proven green on
/// this box") with every red class carrying its named wall. Run it before a
/// published round; cite it in the regime. No excuses, surprised-ourselves-first.
#[derive(Default)]
pub struct BenchmarkValidate;

#[async_trait::async_trait]
impl ActionCommand for BenchmarkValidate {
    const NAME: &'static str = "benchmark/validate";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Validate the benchmark harness against a dataset: build one representative env per \
         (repo, era) class through the real checkout/env seams and report the coverage map — \
         which instances sit in proven-green classes, and the named wall for every red one.";
    type Params = BenchmarkValidateParams;
    type Output = BenchmarkValidateResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        p: BenchmarkValidateParams,
    ) -> Result<BenchmarkValidateResult, CommandError> {
        let name = p.name.as_deref().unwrap_or("swe-bench-verified");
        let spec = known_benchmarks()
            .iter()
            .find(|b| b.name == name)
            .ok_or_else(|| CommandError::Invalid(format!("unknown benchmark '{name}'")))?;
        let dataset = spec.swe_dataset().ok_or_else(|| {
            CommandError::Invalid(format!("'{name}' is not an SWE-class benchmark"))
        })?;
        let instances = crate::cognition::swe_bench::load_dataset(dataset)
            .await
            .map_err(CommandError::Internal)?;
        let dataset_size = instances.len() as u32;
        // Class = (repo, created_at year): the era proxy the env machinery keys
        // dependency resolution on. First instance per class represents it.
        let mut classes: std::collections::BTreeMap<(String, String), (String, u32)> =
            Default::default();
        for i in &instances {
            let era = i.created_at.get(0..4).unwrap_or("????").to_string(); // dataset rows carry ISO dates; a malformed one groups under ???? visibly
            let e = classes
                .entry((i.repo.clone(), era))
                .or_insert_with(|| (i.instance_id.clone(), 0));
            e.1 += 1;
        }
        let cap = p.limit.unwrap_or(u32::MAX) as usize; // default: every class — a validation that samples silently is not a validation
        let mut rows = Vec::new();
        let mut instances_green = 0u32;
        for ((repo, era), (rep, covers)) in classes.into_iter().take(cap) {
            let inst = instances
                .iter()
                .find(|i| i.instance_id == rep)
                .expect("representative came from this same list"); // same vec, same loop — cannot miss
            let outcome = async {
                let dir = crate::cognition::swe_bench::ensure_grade_checkout(inst)
                    .await
                    .map_err(|e| format!("checkout: {e}"))?;
                crate::cognition::swe_bench::ensure_env(inst, &dir)
                    .await
                    .map_err(|e| format!("env: {e}"))?;
                Ok::<(), String>(())
            }
            .await;
            let green = outcome.is_ok();
            if green {
                instances_green += covers;
            }
            let wall = outcome.err().map(|e| {
                let t: String = e.chars().take(500).collect();
                t
            });
            crate::probe!(
                class = "benchmark.validate.class",
                repo = %repo,
                era = %era,
                green,
                covers = covers as u64,
                "env class validated through the real seams"
            );
            rows.push(BenchmarkValidateClass {
                repo,
                era,
                representative: rep,
                covers,
                green,
                wall,
            });
        }
        let summary = format!(
            "{instances_green}/{dataset_size} instances sit in proven-green env classes \
             ({} classes green, {} red)",
            rows.iter().filter(|r| r.green).count(),
            rows.iter().filter(|r| !r.green).count()
        );
        let result = BenchmarkValidateResult {
            platform: BenchmarkPlatformFingerprint::capture(),
            classes: rows,
            instances_green,
            dataset_size,
            summary,
        };
        // PERSIST, so the map can GATE. A coverage map that only ever exists in
        // one command's stdout cannot protect anything: dispatch had no way to
        // ask "is this instance's class known-red?", so a citizen could be sent
        // to spend hours inside an env class this box had already PROVEN cannot
        // build (measured 2026-08-28: astropy-6938 dispatched into the numpy-2
        // wall while a validate run sitting minutes away already knew that class
        // was red). Written per (dataset × machine_class) because a coverage
        // claim is only true for the platform that earned it.
        if let Err(e) = write_coverage_map(name, &result) {
            crate::probe!(
                class = "benchmark.validate.map_unwritten",
                error = %e,
                "coverage map could not be persisted — dispatch keeps its \
                 fail-open behaviour and gates nothing"
            );
        }
        Ok(result)
    }
}

/// Where a validated coverage map lives for one (dataset, machine-class) pair.
/// Under the governed benchmarks root, next to the verdicts it protects — one
/// small file per dataset, rewritten in place, so it needs no eviction story.
pub fn coverage_map_path(dataset: &str, machine_class: &str) -> std::path::PathBuf {
    coverage_map_path_in(
        &crate::cognition::swe_bench::swe_cache_dir(),
        dataset,
        machine_class,
    )
}

/// Root-injected core of [`coverage_map_path`] — the filesystem seam, so tests
/// run against a tempdir instead of the operator's real `~/.continuum` (the
/// pattern `tool_executor::spill` already sets). A test that writes into a
/// person's live data directory is both a lie about isolation and a way to
/// clobber their state.
pub fn coverage_map_path_in(
    root: &std::path::Path,
    dataset: &str,
    machine_class: &str,
) -> std::path::PathBuf {
    let safe: String = dataset
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
        .collect();
    let mc: String = machine_class
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '-' })
        .collect();
    root.join("coverage").join(format!("{safe}.{mc}.json"))
}

fn write_coverage_map(dataset: &str, result: &BenchmarkValidateResult) -> Result<(), String> {
    let path = coverage_map_path(dataset, &result.platform.machine_class);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(result).map_err(|e| e.to_string())?;
    std::fs::write(&path, json).map_err(|e| e.to_string())
}

/// The named wall for an instance's env class, when THIS box has already proven
/// that class red. `None` means "no map, or the class is green, or the map was
/// earned on a different machine class" — every one of which must DISPATCH, not
/// block. Fail-open is the contract: a repo user who has never run
/// `benchmark/validate` is never slowed down by a gate that has nothing to say.
pub fn known_red_wall(dataset: &str, repo: &str, era_year: u32) -> Option<String> {
    known_red_wall_in(
        &crate::cognition::swe_bench::swe_cache_dir(),
        dataset,
        repo,
        era_year,
    )
}

/// Root-injected core of [`known_red_wall`]. See [`coverage_map_path_in`].
pub fn known_red_wall_in(
    root: &std::path::Path,
    dataset: &str,
    repo: &str,
    era_year: u32,
) -> Option<String> {
    let machine_class = BenchmarkPlatformFingerprint::capture().machine_class;
    let raw = std::fs::read_to_string(coverage_map_path_in(root, dataset, &machine_class)).ok()?;
    let map: BenchmarkValidateResult = serde_json::from_str(&raw).ok()?;
    if map.platform.machine_class != machine_class {
        return None; // another box's claim is not evidence about this one
    }
    let era = era_year.to_string();
    map.classes
        .iter()
        .find(|c| c.repo == repo && c.era == era && !c.green)
        .map(|c| {
            c.wall
                .clone()
                .unwrap_or_else(|| "class proven red by benchmark/validate".to_string())
        })
}
crate::register_stateless_command!(BenchmarkValidate);

#[derive(Default)]
pub struct BenchmarkRuns;

#[async_trait]
impl ActionCommand for BenchmarkRuns {
    const NAME: &'static str = "benchmark/runs";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "The benchmark RunProjection: every agent/solve run's live card — phase \
         (active/quiet/resolved/failed/ungradeable), last-activity age, stall flag, acts, \
         grade summary, \
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
        // safe: `limit` is an OPTIONAL page size, so None means "caller didn't ask", not
        // "unknown quantity" — 20 is this command's documented default page and the value
        // the board already uses. `.max(1)` keeps an explicit 0 from returning nothing
        // silently. The count that must never be defaulted is `matched`, which comes from
        // the scan itself and is reported separately.
        let scan = scan_run_cards(p.run_id.as_deref(), p.limit.unwrap_or(20).max(1) as usize) // safe: see the 5 lines above
            .map_err(CommandError::Internal)?;
        let returned = scan.cards.len();
        let matched = scan.matched;
        // Say which of the two this is, in words. A caller comparing `runs.len()` to
        // `matched` would also learn it, but the receipt is what a human (or a citizen
        // reading the board) actually looks at, and the whole point is that truncation
        // must not be invisible there.
        let summary = if returned < matched {
            format!(
                "showing {returned} of {matched} run(s) — NEWEST first, older runs truncated \
                 by `limit`. Raise `--limit` to see the rest; this is a PAGE, not the whole \
                 history."
            )
        } else {
            format!("all {matched} run(s) — this is the complete set for these filters, not a page.")
        };
        Ok(BenchmarkRunsResult {
            runs: scan.cards,
            matched: matched as u32,
            summary,
        })
    }
}

/// The ONE run-ledger scan behind every consumer: the `benchmark/runs`
/// command AND the positron `kind="bench"` board emitter (#329) fold THIS —
/// never a parallel file scrape ([[the-compression-principle]]). Synchronous
/// fs I/O: async callers wrap it in `spawn_blocking`.
/// Cards for staged workspaces that hold REAL WORK no grade has ever seen.
///
/// Why this source exists (glass-boxed 2026-08-18, and it is the acceptance test from
/// docs/architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md failing): the board read ONLY
/// `progress/agent-solve-*.json`. Those files are written by a solve PROCESS. A process that
/// froze mid-flight never writes `files_changed`, and work done another way never writes a
/// file at all — so the board showed 20 runs with `files_changed: []` and phase `quiet`,
/// while three staged trees on the same disk held real in-place source edits. Two of them
/// were PASSES (astropy-14995, pytest-11143, both `resolved=true` once the grader could read
/// its own output). They were found by hand with `git -C … diff`, which is precisely the
/// "if answering needs a file read, it is disconnected and it failed" the doc names.
///
/// The workspace is DURABLE truth; a run file is an ephemeral progress marker. So the board
/// projects both, and an artifact nobody graded is a first-class row rather than an absence.
/// Instances already carrying a grade are skipped — a graded run is the authoritative card.
///
/// Bounded on purpose: at most [`WORKSPACE_ARTIFACT_SCAN_CAP`] trees per call, and the count
/// dropped is logged rather than silently truncated (no-silent-caps).
fn scan_workspace_artifact_cards(graded: &std::collections::HashSet<String>, now_ms: u64) -> Vec<BenchRunCard> {
    let Ok(home) = continuum_home() else {
        return Vec::new();
    };
    let peers = home.join("citizens").join("peers");
    let Ok(peer_entries) = std::fs::read_dir(&peers) else {
        return Vec::new();
    };
    let mut cards = Vec::new();
    let mut scanned = 0usize;
    let mut skipped_over_cap = 0usize;
    for peer in peer_entries.flatten() {
        let peer_id = peer.file_name().to_string_lossy().to_string();
        let swe = peer.path().join("workspace").join("swe");
        let Ok(instances) = std::fs::read_dir(&swe) else {
            continue;
        };
        for inst in instances.flatten() {
            if !inst.path().is_dir() {
                continue;
            }
            let instance = inst.file_name().to_string_lossy().to_string();
            if graded.contains(&instance) {
                continue;
            }
            if scanned >= WORKSPACE_ARTIFACT_SCAN_CAP {
                skipped_over_cap += 1;
                continue;
            }
            scanned += 1;
            let Some(ws) = inst.path().to_str().map(String::from) else {
                continue;
            };
            // The SAME reading of "her work" the grader uses — never a second inline diff.
            let Ok(diff) = workspace_candidate_diff(&ws) else {
                continue;
            };
            if diff.trim().is_empty() {
                continue;
            }
            // Touched paths straight off the diff header — no extra process spawn.
            let files_changed: Vec<String> = diff
                .lines()
                .filter_map(|l| l.strip_prefix("+++ b/"))
                .map(|p| p.to_string())
                .collect();
            let last_activity_ms = std::fs::metadata(inst.path())
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            cards.push(BenchRunCard {
                run_id: format!("workspace:{}:{instance}", &peer_id[..8.min(peer_id.len())]),
                instance: Some(instance),
                attempt: None,
                max_attempts: None,
                solver: Some(peer_id.clone()),
                // NOT "quiet": nothing is stalled here — a finished artifact is waiting for a
                // verdict. Conflating the two is what hid two passes for ~22 hours.
                phase: "ungraded".to_string(),
                stalled: false,
                last_activity_ms,
                age_secs: now_ms.saturating_sub(last_activity_ms) / 1000,
                acts: None,
                files_changed,
                files_examined: Vec::new(),
                resolved: None,
                fail_to_pass: None,
                pass_to_pass: None,
                patch_bytes: Some(diff.len() as u32),
                failed_tests: Vec::new(),
                infra_error: None,
            });
        }
    }
    if skipped_over_cap > 0 {
        tracing::warn!(
            scanned,
            skipped_over_cap,
            cap = WORKSPACE_ARTIFACT_SCAN_CAP,
            "benchmark/runs: workspace-artifact scan hit its cap — some trees were NOT examined \
             for ungraded work (raise the cap or narrow the query; this is not 'nothing found')"
        );
    }
    cards
}

/// How many staged trees one `benchmark/runs` call will diff. A `git diff` per tree is a
/// process spawn, and the board is polled; this bounds the cost. Over-cap trees are WARNED
/// about, never silently dropped.
const WORKSPACE_ARTIFACT_SCAN_CAP: usize = 200;

/// Cards for instances that carry a DURABLE VERDICT — the third row source, and the one that
/// makes a score visible at all.
///
/// # Why (2026-08-18, the last link in the grade tail)
///
/// Verdict persistence landed and the board still could not show a pass. Measured minutes
/// after: `astropy__astropy-14995` graded `resolved=true, F2P 1/1, P2P 40/40`, the verdict
/// was on disk and readable — and `benchmark/runs` reported `resolved: 1`, still counting only
/// an old sympy row. Three rows for 14995 read `failed`, which was HONEST: those are the three
/// solve RUNS that died at the reboot. A run and a verdict are different objects. The board
/// projected runs and artifacts; a verdict had nowhere to appear.
///
/// Until this, `recorded_verdicts` only SUBTRACTED — it marked an artifact as graded so the
/// artifact row disappeared, which made a scored instance LESS visible than an unscored one.
/// A verdict must EMIT.
///
/// This is the acceptance test from
/// [BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER](../../../docs/architecture/BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md):
/// *can a citizen standing in the room perceive the run's state through the same ViewState
/// pipe the human's screen uses?* A score answerable only by reading
/// `benchmarks/swe/verdicts/*.json` is disconnected, and it failed.
///
/// Cheap by construction: no `git diff`, no process spawn — one small JSON read per scored
/// instance, so this source needs no cap.
fn scan_verdict_cards(now_ms: u64) -> Vec<BenchRunCard> {
    swe_bench::recorded_verdicts()
        .into_iter()
        .map(|(instance, v)| {
            let last_activity_ms = std::fs::metadata(swe_bench::verdict_path(&instance))
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            BenchRunCard {
                run_id: format!("verdict:{instance}"),
                instance: Some(instance),
                attempt: None,
                max_attempts: None,
                solver: None,
                // The verdict IS the phase. `record_verdict` refuses gold and errored
                // verdicts, so every row here is a real capability result — never a control
                // and never an env fault dressed as a score.
                phase: if v.resolved { "resolved" } else { "failed" }.to_string(),
                stalled: false,
                last_activity_ms,
                age_secs: now_ms.saturating_sub(last_activity_ms) / 1000,
                acts: None,
                files_changed: Vec::new(),
                files_examined: Vec::new(),
                resolved: Some(v.resolved),
                fail_to_pass: Some(format!("{}/{}", v.f2p_passed, v.f2p_total)),
                pass_to_pass: Some(format!("{}/{}", v.p2p_passed, v.p2p_total)),
                patch_bytes: None,
                failed_tests: v.failed_tests.clone(),
                infra_error: None,
            }
        })
        .collect()
}

pub(crate) fn scan_run_cards(
    run_id_filter: Option<&str>,
    limit: usize,
) -> Result<RunScan, String> {
    let base = std::env::var("CONTINUUM_HOME")
        .map(std::path::PathBuf::from)
        .ok()
        .or_else(|| dirs::home_dir().map(|h| h.join(".continuum")))
        .ok_or_else(|| "no home dir".to_string())?
        .join("progress");
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let mut cards: Vec<BenchRunCard> = Vec::new();
    let entries = std::fs::read_dir(&base).map_err(|e| format!("read {}: {e}", base.display()))?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // Grade files are read as SIBLINGS of their run below, never enumerated as runs
        // (live first use showed `X.grade` phantoms). That rule and the prefix now live in
        // ONE place with the boot reaper and the reboot guard, which is what stops the
        // board and the reaper disagreeing about what a run ledger is called.
        let Some(run_id) = crate::cognition::swe_bench::solve_run_id_from_file_name(&name) else {
            continue;
        };
        if let Some(want) = run_id_filter {
            if want != run_id {
                continue;
            }
        }
        let read_json = |p: &std::path::Path| -> Option<serde_json::Value> {
            std::fs::read_to_string(p)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
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
    // Second source: staged trees holding work no grade has seen. Only when the caller is
    // asking for the BOARD — a run-id query is asking about one run's ledger, and a workspace
    // artifact has no run id to match. See `scan_workspace_artifact_cards` for why the board
    // cannot be run-files-only.
    if run_id_filter.is_none() {
        // "Scored" is the union of two sources, and it MUST be: a run ledger's own grade
        // sibling, AND the durable per-instance verdict record. An operator or workspace
        // grade has no run id at all, so before verdicts were recorded (2026-08-18) a real
        // pass could not make an artifact stop reading `ungraded` — two of them didn't.
        let mut graded: std::collections::HashSet<String> = cards
            .iter()
            .filter(|c| c.resolved.is_some())
            .filter_map(|c| c.instance.clone())
            .collect();
        // A verdict EMITS its own row, and that row is also what marks the instance graded —
        // so a scored instance is MORE visible than an unscored one, not less. Before this,
        // verdicts only subtracted: the artifact row vanished and no score took its place.
        let verdict_cards = scan_verdict_cards(now_ms);
        graded.extend(verdict_cards.iter().filter_map(|c| c.instance.clone()));
        cards.extend(verdict_cards);
        cards.extend(scan_workspace_artifact_cards(&graded, now_ms));
    }
    cards.sort_by(|a, b| b.last_activity_ms.cmp(&a.last_activity_ms));
    // Counted BEFORE the truncate, and carried out with the cards. Every consumer that
    // shows a bounded page has to be able to say how much it bounded away, or the page
    // reads as the whole history — see `BenchmarkRunsResult::matched` for the hour that
    // cost.
    let matched = cards.len();
    cards.truncate(limit);
    // The ledger stores the solver as her PERSONA UUID; the board speaks NAMES.
    // Resolve against the live workspace roster here — the ONE scan — so every
    // consumer (command + positron emitter) gets the same display identity. A
    // uuid not in the roster (despawned persona, operator-fired run) stays as-is;
    // the client compacts unresolved ids to short form (#161 vocabulary).
    let names: std::collections::HashMap<String, String> =
        crate::cognition::persona_workspace::global()
            .roster()
            .into_iter()
            .filter_map(|(id, name)| name.map(|n| (id.to_string(), n)))
            .collect();
    for card in &mut cards {
        if let Some(solver) = &card.solver {
            if let Some(name) = names.get(solver) {
                card.solver = Some(name.clone());
            }
        }
    }
    Ok(RunScan { cards, matched })
}

// ---------------------------------------------------------------------------
// benchmark/rounds — the ROUND lifecycle, askable (#371)
// ---------------------------------------------------------------------------

/// No parameters. Rounds in flight are few (usually one) and each is a handful of
/// fields, so paging and filtering would be ceremony over a list you always want whole.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRoundsParams.ts"
)]
pub struct BenchmarkRoundsParams {}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRoundsResult.ts"
)]
pub struct BenchmarkRoundsResult {
    /// Rounds currently in flight. EMPTY is a real, unambiguous answer — "no round is
    /// running" — and never "the question could not be reached". A round is removed the
    /// instant its last card settles, so a round that finished is absent by design and
    /// its END is on the `bench.round.done` probe.
    pub rounds: Vec<crate::cognition::bench_round::RoundSnapshot>,
    /// How many are in flight, so a reader that only needs the yes/no does not have to
    /// interpret an array's length.
    pub in_flight: usize,
}

#[derive(Default)]
pub struct BenchmarkRounds;

#[async_trait]
impl ActionCommand for BenchmarkRounds {
    const NAME: &'static str = "benchmark/rounds";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Every benchmark ROUND in flight, with its stage — the lifecycle question answered \
         by a QUERY instead of by probe archaeology (#371). A round is the card set one \
         `benchmark/dispatch` posted; its id IS its run room's id. Each row carries stage \
         (working|done), dispatched/settled/remaining, and the work DRIVER (citizen — works \
         in the room and feeds the curriculum — vs detached_solve). An EMPTY list is a real \
         answer meaning no round is running, never a failure to reach the question; a round \
         is dropped the moment its last card settles, and that END is the `bench.round.done` \
         probe. This is what a fresh driver reads to answer 'has it started, is it stuck, is \
         it done' with zero log reads.";
    type Params = BenchmarkRoundsParams;
    type Output = BenchmarkRoundsResult;

    async fn run(
        &self,
        _ctx: &Ctx,
        _p: BenchmarkRoundsParams,
    ) -> Result<BenchmarkRoundsResult, CommandError> {
        let rounds = crate::cognition::bench_round::live_rounds();
        Ok(BenchmarkRoundsResult {
            in_flight: rounds.len(),
            rounds,
        })
    }
}

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
