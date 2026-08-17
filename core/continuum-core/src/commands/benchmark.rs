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
}

/// Resolve the citizens a directed dispatch addresses — GENERALIZED for any repo user's
/// roster, never our specific names. Pure over the live snapshot so it is unit-testable
/// without a running airc daemon (a real `PersonaSlot` needs one); the wrapper in `run`
/// just feeds `registry.roster_snapshot()` in.
///
/// - `requested` empty → the WHOLE live roster (whoever THIS machine spawned). This is the
///   "dispatch to my citizens, whoever they are" default: a fresh clone runs
///   `benchmark/dispatch --name=…` with no `--assignees` and it targets their own online
///   citizens. Directed dispatch is what actuates (a silent card does not — measured
///   2026-08-07), so defaulting to the live roster keeps the loop autonomous everywhere.
/// - `requested` non-empty → every name MUST resolve to a live citizen; an unknown name
///   FAILS LOUD listing who is online (never silently addresses a ghost that never claims,
///   and never silently skips SWE staging). Order is preserved for a stable round-robin.
/// - roster empty → `Denied` (nobody online — `persona/spawn` first; the fix is a citizen,
///   not an invented identity).
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

fn resolve_dispatch_roster(
    live: &[(String, uuid::Uuid)],
    requested: &[String],
) -> Result<Vec<(String, uuid::Uuid)>, CommandError> {
    if live.is_empty() {
        return Err(CommandError::Denied(
            "no citizens are online to work the cards — spawn a persona (persona/spawn) \
             first, then dispatch."
                .to_string(),
        ));
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
        return Err(CommandError::Invalid(format!(
            "assignee(s) not online: {}. Citizens currently online: [{}]. Pass names from \
             that list, or omit --assignees to dispatch to all of them.",
            unknown.join(", "),
            online.join(", "),
        )));
    }
    Ok(resolved)
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

        let spec = known_benchmarks()
            .iter()
            .find(|b| b.name == p.name)
            .ok_or_else(|| {
                CommandError::Invalid(format!(
                    "unknown benchmark '{}' — see benchmark/list",
                    p.name
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
            instances
                .into_iter()
                .map(|i| PreparedCard {
                    title: dispatch_card_title(spec.name, &i.instance_id, &i.problem_statement),
                    body: dispatch_swe_card_body(spec.name, &i),
                    needs_setup: false,
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
                    p.name
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
                        work: CardWork::Gym { solution_file },
                    })
                })
                .collect::<Result<_, CommandError>>()?
        };

        // Curator seed: a persona dispatching through her toolbelt authors as herself;
        // the operator with no self-peer (#27) authors through a live citizen (benchmarks
        // ARE their work). See `curator_airc`.
        let airc = curator_airc(&self.registry, ctx, "benchmark/dispatch")?;

        let requested = p.assignees.clone().unwrap_or_default();
        if requested.iter().any(|a| a.trim().is_empty()) {
            return Err(CommandError::Invalid(
                "assignees contains an empty name — every kickoff must address a real citizen"
                    .to_string(),
            ));
        }
        // Resolve the dispatch roster against THIS machine's live citizens (never our
        // names): empty request → the whole live roster; explicit names → validated or
        // fail-loud. This is the generalization for all repo users — dispatch targets the
        // citizens they actually spawned, whoever those are.
        //
        // Resolved BEFORE the room exists because the roster decides WHO gets moved into
        // it: a run room nobody is standing in is the other half of the bug this verb is
        // fixing ("old rooms flooded, or ones with nothing").
        let roster = resolve_dispatch_roster(&self.registry.roster_snapshot(), &requested)?;

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
        let solve_cap: u32 = match crate::inference::llama_server::await_ready_serving(
            std::time::Duration::from_secs(30),
        )
        .await
        {
            Some(s) => s.lanes.max(1),
            None => 0,
        };
        for pc in prepared.into_iter().take(take) {
            // A gym setup_shell task needs its workspace re-broken before work
            // starts — harness orchestration a claimed card can't provide yet.
            // Skipping SILENTLY would report "dispatched" over fewer tasks
            // than the benchmark holds; the count rides on the result instead.
            if pc.needs_setup {
                skipped_needs_setup += 1;
                continue;
            }

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
            let mut staged_ok = false;
            if let (CardWork::Swe { instance }, Some(home)) = (&pc.work, stage_home.as_ref()) {
                let dir = home
                    .join("citizens")
                    .join("peers")
                    .join(who_peer.to_string())
                    .join("workspace")
                    .join("swe")
                    .join(&instance.instance_id);
                if dir.join(".git").exists() {
                    staged_ok = true; // already staged (a prior claim / dispatch)
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

            // Directed gym card: CLAIM IT FOR HER at dispatch, under her own airc
            // identity. The SWE arm below already fires her scored solve directly
            // (dispatch_staged_swe_solve — "we don't wait on her to re-derive a
            // work/claim from the kickoff"); gym cards never got the same cut, so
            // every round spent its first multi-minute turn per card on claim
            // ceremony the dispatcher had already decided (Joel 2026-08-15:
            // "taking 30 minutes to start coding sure is a flawed design").
            // #425-compatible: the claim is administrative — the WORK stays hers,
            // in-room, through her own cognition. Best-effort: a failed pre-claim
            // is REPORTED and the card stays claimable by hand.
            let mut pre_claimed = false;
            if let CardWork::Gym { .. } = &pc.work {
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
                match airc.say(&kickoff).await {
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
            if staged_ok && solves_fired < solve_cap {
                if let CardWork::Swe { .. } = &pc.work {
                    // The run room goes WITH the solve: her acts radiate receipts
                    // into the room this dispatch just spawned, so the round's work
                    // is visible where the round lives (#243/#329) instead of only
                    // in a ledger file that lands when it is already over.
                    crate::modules::work::dispatch_staged_swe_solve(
                        ctx,
                        &airc,
                        *who_peer,
                        card_id,
                        Some(room.room_id.as_uuid()),
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
        crate::cognition::bench_round::register_round(
            room.room_id.as_uuid(),
            spec.name,
            &card_uuids,
        );

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
    use super::*;

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
        // Every runnable benchmark's eval_set must resolve through the gym resolver.
        for b in ks.iter().filter(|b| b.eval_set.is_some()) {
            crate::cognition::gym::resolve_gym(b.eval_set.unwrap()).unwrap_or_else(|e| {
                panic!("benchmark '{}' eval_set does not resolve: {e}", b.name)
            });
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

/// The candidate diff of a solver workspace — the ONE reading of "her work"
/// (grade_swe's candidate arm and agent/solve's attempt-patch receipt both
/// call this; a second inline `git diff` would drift on the exclude rules).
/// `diff HEAD` (not bare `diff`) so STAGED edits count as her work too, and
/// `:(exclude).airc` because the substrate stages its own coordination files
/// into her workspace (card b34f7eb5): Atlas's first grade carried 91KB of
/// staged `.airc` blobs, and the fresh clone refused the WHOLE candidate —
/// a real fix voided by files no solver wrote.
pub(crate) fn workspace_candidate_diff(ws: &str) -> Result<String, CommandError> {
    let out = std::process::Command::new("git")
        .args(["diff", "HEAD", "--", ".", ":(exclude).airc"])
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
        .ok_or_else(|| CommandError::NotFound(format!("{} not found in {dataset}", p.instance)))?;

    // Resolve the candidate patch. A workspace's diff is READ here but graded in a fresh
    // clone below — where the solver worked is never where the score is taken.
    let candidate: Option<String> = if p.gold.unwrap_or(false) {
        Some(instance.patch.clone())
    } else if let Some(ws) = p.workspace.as_ref() {
        Some(workspace_candidate_diff(ws)?)
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
    // THE SPINE CHECK IS NOW ENFORCED, not just run. `gold` graded through the plain
    // `grade` path returned a bare `resolved: false`, which is byte-identical downstream to
    // a citizen's capability zero — so the `gold` doc's own demand ("if it does not, the
    // environment is wrong and no other number from it means anything") was a sentence
    // addressed to a human and enforced by nobody. `gold_gate` stamps the verdict's `error`
    // with WHY, and an `error` is contractually an ABSENCE, never a tallied failure
    // (see `SweVerdict::error`). One path, so every caller inherits the labelling.
    let verdict = if p.gold.unwrap_or(false) {
        swe_bench::gold_gate(&instance, &repo).await
    } else {
        swe_bench::grade(&instance, &repo, candidate.as_deref()).await
    };

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
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRunsResult.ts"
)]
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
        let runs = scan_run_cards(p.run_id.as_deref(), p.limit.unwrap_or(20).max(1) as usize)
            .map_err(CommandError::Internal)?;
        Ok(BenchmarkRunsResult { runs })
    }
}

/// The ONE run-ledger scan behind every consumer: the `benchmark/runs`
/// command AND the positron `kind="bench"` board emitter (#329) fold THIS —
/// never a parallel file scrape ([[the-compression-principle]]). Synchronous
/// fs I/O: async callers wrap it in `spawn_blocking`.
pub(crate) fn scan_run_cards(
    run_id_filter: Option<&str>,
    limit: usize,
) -> Result<Vec<BenchRunCard>, String> {
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
        let Some(run_id) = name
            .strip_prefix("agent-solve-")
            .and_then(|r| r.strip_suffix(".json"))
        else {
            continue;
        };
        // Grade files are read as SIBLINGS of their run below, never
        // enumerated as runs (live first use showed `X.grade` phantoms:
        // `agent-solve-X.grade.json` survives the prefix/suffix strip).
        if run_id.ends_with(".grade") {
            continue;
        }
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
    cards.sort_by(|a, b| b.last_activity_ms.cmp(&a.last_activity_ms));
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
    Ok(cards)
}

crate::register_stateless_command!(BenchmarkRuns);
