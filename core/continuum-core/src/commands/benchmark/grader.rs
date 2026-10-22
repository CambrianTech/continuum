use super::*;

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
        if !matches!(
            self.name,
            "swe-bench-lite"
                | "swe-bench-verified"
                | "swe-bench-verified-mini"
                | "swe-bench-multilingual"
        ) {
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
            name: "swe-bench-verified-mini",
            description: "SWE-bench Verified-mini (MariusHobbhahn) — a 50-instance decimation of \
                          Verified with IDENTICAL instance fields, chosen so the env footprint is \
                          ~5GB instead of 130GB. The INNER DEV LOOP: cheap claim growth and fast \
                          harness iteration between full Verified rounds (VIRAL-LAUNCH-PLAN.md \
                          execution item 2; ADAPT-NOW #1 of the 2026-09 landscape sweep).",
            grader: Grader::Python,
            tasks: 50,
            eval_set: None,
            source_url: Some("https://huggingface.co/datasets/MariusHobbhahn/swe-bench-verified-mini"),
        },
        BenchmarkSpec {
            name: "swe-bench-multilingual",
            description: "SWE-bench Multilingual (MIT) — 300 instances across 9 languages, \
                          including 43 RUST instances on which NO open model has ever been \
                          scored (an empty board is a first-mover headline). eval_script and \
                          log_parser ship AS DATA per instance. Catalogued ahead of its \
                          adapter: the Rust slice needs cargo-based env staging, not the \
                          python venv path — see VIRAL-LAUNCH-PLAN.md execution item 3.",
            grader: Grader::Python,
            tasks: 300,
            eval_set: None,
            source_url: Some("https://huggingface.co/datasets/SWE-bench/SWE-bench_Multilingual"),
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
