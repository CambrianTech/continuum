//! The eval's wire types and their defaults: the task row and its grading-modality predicates,
//! the gene under test, the command's parameters with each default resolved in one place, and the
//! per-task, infra-void and run results.

use super::*;

/// The committed, discriminating coder set used when the caller passes neither
/// inline `tasks` nor an explicit `eval_set` path. Resolved (not read raw)
/// through [`crate::cognition::gym::resolve_gym`], so the default — like every committed
/// gym — comes from the embedded registry, CWD- and deployment-independent. A
/// *custom* set still uses `--eval_set <path>` (an existing on-disk file wins);
/// only changing a *committed* gym needs a rebuild — the right trade, since the
/// committed gyms must be reliable. The whole class of "core launched from a
/// different cwd → file-not-found → silent degrade" is killed by going through
/// the resolver instead of `std::fs::read_to_string`.
pub(super) const DEFAULT_EVAL_SET: &str = "docs/genome/coder-eval.jsonl";

/// How many act→observe cycles a single task may take before it counts as
/// unfinished, when neither the caller nor the task row sets `max_acts`.
///
/// Raised 8 → 32 on 2026-08-23 after the MirrorCode baseline showed what 8
/// buys on a frontier task: Atlas spent the whole budget on a genuinely
/// competent case-bucketing analysis and was graded with an EMPTY src/ — the
/// cap measured our patience, not the model ("repeated engineering process
/// till finished … who cares if it has to cycle to do it" — the doctrine the
/// old default contradicted). Tasks that need fewer acts settle on their own;
/// the cap only exists to bound a runaway, so it must sit ABOVE what honest
/// engineering needs. Per-task rows override via `EvalTask::max_acts`.
pub(super) const DEFAULT_MAX_ACTS: u32 = 32;

/// How many times a FAILED (test-graded) task is handed its compiler/test output to fix
/// before we score it a miss. This is the agentic-recovery budget — the edge over one-shot
/// inference. 3 keeps a full eval bounded while giving real iteration room.
const MAX_FAIL_RETRIES: u32 = 3;

/// How much of a graded answer to persist in the result. A coder answer (code + reasoning)
/// fits far under this; the generous bound exists ONLY to cap a pathological loop-to-length
/// generation (which is captured up to here, still a diagnosable signal). The old 200-char
/// cap stored only the preamble and blinded every failure diagnosis + correction-corpus mine.
// context-budget-exempt: observability: how much of an answer is CAPTURED into the run ledger for humans, never text sent to a model
pub(super) const ANSWER_CAPTURE_CHARS: usize = 24_000;

/// One eval task. Both the JSONL rows and inline `tasks` deserialize into this;
/// every field is optional so an authoring typo degrades to a benign empty rather
/// than failing the whole run. A task is TEST-GRADED when it carries `test`, else
/// substring-graded against `expect`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, JsonSchema)]
// Exported to the canonical protocol root (#247/#80) so the parents that embed it
// (GenomeTeachParams / MinedTask / RedactMemoryParams — all already emitting under
// protocol/typescript/) import a SIBLING instead of reaching back into the legacy
// crate bindings/ dir. A derive without an export once left this file a silently
// rotting orphan; the explicit export is the fix, the canonical path is #424.
#[ts(export, export_to = "../../../protocol/typescript/cognition/EvalTask.ts")]
pub struct EvalTask {
    /// Stable id for the task (echoed in results so a regression is identifiable).
    #[serde(default)]
    pub id: String,
    /// Per-task act→observe budget override — budget-as-data, so a gym can size
    /// patience to its task class (a mirror task needs analysis + implementation
    /// + verification; a one-liner doesn't). `None` inherits the run's budget.
    #[serde(default)]
    #[ts(optional)]
    pub max_acts: Option<u32>,
    /// The prompt posed to the persona, framed as a room message.
    #[serde(default)]
    pub prompt: String,
    /// Substring the answer must contain (case-insensitive) for descriptive tasks.
    /// Ignored when `test` is present.
    #[serde(default)]
    pub expect: String,
    /// SILENCE-graded: the task passes when she says NOTHING (a Pass, no spoken
    /// text). The speech-discipline gym's quiet half — a wake-shaped burst with
    /// nothing to answer must end in silence, not in the burst read back. `expect`
    /// is ignored when this is set.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub silence: bool,
    /// A test program appended to her extracted code and RUN; pass = exit 0. When
    /// present, this objective grade supersedes `expect`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub test: Option<String>,
    /// Language of `test` (the gym grades `rust` only). Defaults to `rust`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub lang: Option<String>,
    /// A REAL definition-of-done: a shell command run in the persona's workspace AFTER she
    /// acts (edits files with her tools). Pass = exit 0. This is what makes a task REAL vs
    /// a HumanEval toy — the grade checks the actual repo state her edits produced (e.g.
    /// `cargo test --test foo`), not code extracted from her chat answer. Supersedes
    /// `test`/`expect`; the recovery loop feeds its stdout+stderr back on failure so she
    /// iterates against the real compiler/test output until it goes green.
    #[serde(default, alias = "dodShell", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub dod_shell: Option<String>,
    /// A HELD-OUT grade: a shell command run in her workspace once `dod_shell` passes, and
    /// never shown to her. Not in the card, not in the verify loop's re-drive, not in a
    /// verdict line (a failure says only that a requirement the task did not state was not
    /// met). `dod_shell` is the definition of done she works against; this checks something
    /// she should already know without being told, which is what a learned correction is.
    /// Without it the only workspace oracle was the visible DoD, so a check of a learned rule
    /// had to be printed in the card, which taught every arm the rule. Pass = both exit 0.
    #[serde(default, alias = "heldOutShell", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub held_out_shell: Option<String>,
    /// ARTIFACT grade: a relative in-workspace path she is told to write her solution to. When
    /// set alongside `test`, the grade reads HER FILE (her hands) instead of extracting a code
    /// block from her spoken answer (her mouth), then runs the SAME harness (strip her `main`,
    /// append `test`, compile, run). This is how an ACTING persona is measured — the act→verify
    /// loop is only visible if we grade what she actually wrote + compiled, not what she narrated.
    /// The file lands in the workspace root (= core cwd, where `code/write` sandboxes writes).
    #[serde(
        default,
        alias = "solutionFile",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub solution_file: Option<String>,
    /// Task-state SETUP: a shell command run BEFORE the prompt is posed, restoring the
    /// task's initial workspace state so runs are repeatable (a `gym/mine` task re-breaks
    /// its checkout: `git checkout <commit>^ -- src/lib.rs`). Setup failure is a named
    /// infra grade, never a silent broken workspace ([[fallbacks-are-illegal-fail-loud]]).
    #[serde(default, alias = "setupShell", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub setup_shell: Option<String>,
    /// FUNCTIONAL WEB-DEV grade: structural acceptance criteria checked against what the
    /// persona's UI ACTUALLY RENDERED. When non-empty, the grade OBSERVES `target` through
    /// HER OWN eyes (the `perception/observe` eye-node path — the same purity as every other
    /// tool) and scores the element tree with `perception::scoring::grade_ui`. This measures a
    /// persona on building a UI that WORKS, on equal footing for every model: the structure tree
    /// is plain text a non-visual model reads exactly like a VLM ([[built-to-teach-lesser-tuned-intelligences-win]]).
    /// The SAME `UiScore` is her self-check, her training label, and this benchmark score.
    #[serde(default, alias = "uiChecks", skip_serializing_if = "Vec::is_empty")]
    pub ui_checks: Vec<crate::perception::scoring::UiCheck>,
    /// What to observe for a web-dev task: a workspace-relative path (absolutized to a `file://`
    /// URL against her workspace root — where `code/write` sandboxes her files) or an explicit
    /// URL (e.g. an `http://localhost` dev server she started). Defaults to `index.html`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub target: Option<String>,
    /// Fraction of `ui_checks` that must hold to PASS (`1.0` = every criterion; the fractional
    /// score always rides along in the grade line). Defaults to `1.0` — "the UI works".
    #[serde(
        default,
        alias = "uiPassThreshold",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub ui_pass_threshold: Option<f32>,
    /// Does answering this task REQUIRE tools — regardless of how it's graded? The derived
    /// default (see [`needs_tools`](Self::needs_tools)) keys off the GRADING modality: a task
    /// whose grade reads a file she wrote (`solution_file`), a workspace DoD (`dod_shell`), a
    /// pinned repo (`workspace_root`), or a rendered UI (`ui_checks`) obviously needs hands.
    /// But a MOUTH-graded task (`expect`/`test`) can ALSO require tools to PRODUCE the answer —
    /// a repo-navigation exam ("Which file defines `fn build_workspace_burst`?") is graded by a
    /// spoken substring yet is unanswerable without `code/search`/`code/read`. The derived
    /// signal misses that, so the whole gym runs speak-only and scores a SILENT 0 for every
    /// model (#208, coder-eval). This EXPLICIT declaration overrides the default: `Some(true)`
    /// forces tools on for the run; `Some(false)` pins a task speak-only even if it looks
    /// hands-graded. `None` (the common case) falls back to the grading-modality derivation.
    #[serde(default, alias = "needsTools", skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub needs_tools: Option<bool>,
    /// The checkout THIS task lives in — her hands get rooted here before the task runs.
    ///
    /// The run-level `workspace_root` param pins ONE repo for a whole exam, which is right for a
    /// SWE-bench instance (one clone, one task). It is wrong for a MINED gym: `gym/mine` gives
    /// every task its own git worktree so tasks are independent and repeatable, so the root is a
    /// property of the TASK. Without this the evaluator hands her one root for all N tasks — the
    /// wrong one for every task — and she is told to fix a bug in a directory her sandboxed tools
    /// cannot reach. That is not a hard exam, it is an unrunnable one, and it is why mined gyms
    /// had never produced a number.
    ///
    /// It is DECLARED here, not applied mid-run. Rooting a persona is two operations — her hands
    /// (`root_acting_workspace`, callable any time) and her eyes (`repoint_workspace_map_if_pinned`,
    /// fork-time only, because the cycle's faculties are immutable once built). Applying just the
    /// first splits her: hands in one tree, map of another, which is the #206 shape and produces a
    /// zero that lies about the solver with no probe to show it. So a task whose root differs from
    /// the run's is REFUSED with a named infra grade telling the caller to run one eval per root —
    /// the run-level pin does both halves at fork, so that path is correct today. When the map
    /// derives its root from her hands (one source of truth), this becomes a live re-root and the
    /// refusal is deleted.
    /// [[re-rooting-a-persona-is-two-operations-moving-one-is-worse-than-moving-neither]]
    #[serde(
        default,
        alias = "workspaceRoot",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub workspace_root: Option<String>,
}

impl EvalTask {
    /// Is this task GRADED ON THE WORKSPACE (a DoD shell reads files, a
    /// solution file is collected, UI checks screenshot artifacts) rather than
    /// on her spoken answer? Decides the turn's [`TurnFraming::on_workspace`]
    /// contract — which in turn arms the discovery-saturation gate, the
    /// no-deliverable nudge, and the act-budget proprioception. Before this
    /// predicate the eval framed EVERY task as directed-speech, so none of
    /// those protections ever applied to the tasks they were built for
    /// (glass-boxed 2026-08-23: MirrorCode graded an empty src/ after a
    /// discovery-only turn the saturation gate should have interrupted).
    pub fn workspace_deliverable(&self) -> bool {
        self.dod_shell.is_some() || self.solution_file.is_some() || !self.ui_checks.is_empty()
    }

    /// Is this task graded by something that RUNS: a compiled `test`, or a workspace
    /// definition of done? The ONE predicate for "a lesson whose verdict is objective":
    /// coursework keeps only these, and the room's gym grader has an arm for each.
    pub fn has_run_oracle(&self) -> bool {
        self.test.is_some() || self.dod_shell.is_some()
    }

    /// The task's source language. An omitted `lang` means Rust: the coder gyms were
    /// authored Rust-first and every grader arm compiles Rust by default.
    pub(super) fn language(&self) -> &str {
        self.lang.as_deref().unwrap_or("rust") // unwrap_or: an omitted `lang` is Rust by the gym authoring contract
    }

    /// This task's act→observe budget: the row's own `max_acts`, else the run's.
    pub(super) fn act_budget(&self, run_budget: usize) -> usize {
        self.max_acts.map_or(run_budget, |v| v as usize)
    }

    /// The page a UI-checked task is observed through. Omitted means the conventional
    /// `index.html` entry point a web task builds.
    pub(super) fn ui_target(&self) -> &str {
        self.target.as_deref().unwrap_or("index.html") // unwrap_or: an omitted target is the conventional web entry point
    }

    /// The fraction of UI checks that must pass. Omitted means every check ("it works").
    pub(super) fn ui_threshold(&self) -> f32 {
        self.ui_pass_threshold.unwrap_or(1.0) // unwrap_or: an omitted threshold means every check must pass
    }
    /// Whether answering THIS task requires the persona's hands. An explicit
    /// [`needs_tools`](Self::needs_tools) declaration wins; otherwise it's derived from the
    /// GRADING modality — a grade that reads a written file / DoD / rendered UI needs hands.
    /// The run-level decision is `any task needs tools` (tools are offered to the whole run),
    /// so a mixed gym with one repo-nav task correctly arms tools for all. See #208.
    pub fn needs_tools(&self) -> bool {
        self.needs_tools.unwrap_or_else(|| { // unwrap_or_else: an undeclared need is derived from the grading modality, by design
            self.solution_file.is_some()
                || self.dod_shell.is_some()
                || self.workspace_root.is_some()
                || !self.ui_checks.is_empty()
        })
    }

    /// CODE IS BUILT, NEVER SPOKEN. A task carrying a compile-and-run `test` grades a PROGRAM,
    /// and a program's deliverable is a FILE. If the task didn't name one, name it here — the
    /// grade then reads what her hands wrote, and the spoken-code payout stops existing.
    ///
    /// Joel, 2026-08-04, on why this is not merely a measurement fix: *"any benchmark, at least
    /// for code, that has them speak code is actually more of a problem for learning persona with
    /// minds. It's going to train them to think speak code (or anything that needs an action) is
    /// building projects. That's exactly the opposite of our goals for first class citizens."*
    ///
    /// That is the load-bearing argument. Our graded episodes do not stop at a scoreboard — they
    /// enter the SAME experience loop as lived ones (engram → sentinel → curriculum → genome,
    /// [[one-experience-loop-benchmark-lessons-are-engrams-dream-sentinels-train-them]]). So a
    /// grader that pays out for a fence does not just mis-measure the fence→act gap, it TEACHES
    /// it, and the layer promoted on that reward carries "describing is doing" into every room —
    /// not just the exam. 404 of 458 gym tasks paid out that way when this was written.
    ///
    /// Doing it at LOAD is the compression: the alternative already existed as
    /// `hard-rs-acted.jsonl`, a hand-forked twin of `hard-rs.jsonl` differing only by this
    /// preamble and this filename. One rule in the loader replaces N forked corpora that would
    /// each drift. A gym author still WINS by naming `solution_file` (and its prompt) explicitly;
    /// this only ensures no code task can be graded by mouth because someone forgot.
    ///
    /// `expect`-graded knowledge tasks are untouched — answering a question IS speaking, and
    /// nothing is confused about that. The rule keys on `test`: only a task that compiles and
    /// runs code has a program as its deliverable.
    ///
    /// `pub(crate)` because THIS is the one derivation of a code task's artifact name:
    /// `benchmark/dispatch` must run the same rule before composing a work card, or the card
    /// tells the citizen nothing about WHERE to write while the grade reads the derived path
    /// (glass-boxed 2026-08-15: a frontier-rs claimer inventing `swe/benchmarks/.../lib.rs`).
    pub(crate) fn require_hands_for_code(&mut self) {
        if self.test.is_none()
            || self.solution_file.is_some()
            || self.dod_shell.is_some()
            || self.workspace_root.is_some()
        {
            return;
        }
        let ext = match self.language() {
            "rust" | "rs" => "rs",
            "python" | "py" => "py",
            "typescript" | "ts" => "ts",
            "javascript" | "js" => "js",
            "go" => "go",
            "c" => "c",
            "cpp" | "c++" => "cpp",
            other => other,
        };
        let safe: String = self
            .id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let file = format!("sol_{safe}.{ext}");
        // The prompt must ASK for what the grade READS — a derived file behind an unchanged
        // "return the complete function" prompt would score an honest 0 for the wrong reason
        // (she answered the question she was asked). Same wording the hand-authored acted gym
        // used, so migrated tasks read identically to ones an author wrote by hand.
        self.prompt = format!(
            "Implement the following, and VERIFY it before finishing. Write your solution to \
             the file `{file}` (a relative path in your workspace) using your write tool, then \
             compile and run it with your shell tool against a few checks you devise; if it \
             fails, read the compiler/test output and fix it. Only finish once it actually \
             compiles and runs. Leave the final, working code in `{file}`.\n\n{}",
            self.prompt
        );
        self.solution_file = Some(file);
    }
}

/// A gene to page in for the candidate arm of an A/B. The persona runs the eval
/// once on the base model (gene paged OUT), then again with this gene paged IN;
/// the result carries both pass-rates and their `lift`. The gene must already be
/// REGISTERED with the serving endpoint (the adapter resolves `name`/`path` → the
/// server's LoRA load-index); continuum holds the handle, the custodian owns the
/// bytes ([[model-endpoint-fabric-adapter-router]]).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct EvalGene {
    /// The gene's name — how the serving endpoint catalogs it (`/lora-adapters`).
    pub name: String,
    /// The gene's on-disk path (the custodian-owned GGUF/safetensors LoRA). The
    /// adapter matches this against the registered catalog when `name` doesn't.
    #[serde(default)]
    pub path: String,
    /// Influence dial in [0,1+] — 0 = base, 1 = full gene. The page-in is analog:
    /// the per-request scale rides into `"lora":[{id,scale}]`. Defaults to 1.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub scale: Option<f64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
pub struct CognitionEvalParams {
    /// Which persona to put through the gym — a full UUID, an 8-char short-id, or a
    /// name. Resolved against the live roster before anything runs, so a garbage or
    /// unknown reference fails loud here instead of dying later as a misleading
    /// "not assembled at spawn". Must be spawned (have a live `WorkspaceCycle`) —
    /// the eval drives her real cognition, not a stand-in.
    pub persona_id: crate::identity::PersonaRef,
    /// Optional gene to MEASURE: when set, the eval runs base vs gene as an A/B and
    /// reports the `lift`. When omitted, a single pass on whatever genome is
    /// currently paged in (base, by default).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub gene: Option<EvalGene>,
    /// Room context the eval turns are scoped to. Omit for the nil room.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub room_id: Option<String>,
    /// Inline tasks. When set, takes precedence over `eval_set`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub tasks: Option<Vec<EvalTask>>,
    /// Path to a JSONL eval set (one `EvalTask` per line). Defaults to the committed
    /// `docs/genome/coder-eval.jsonl` when neither this nor `tasks` is given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub eval_set: Option<String>,
    /// Measure THIS base model through the full loop, in its OWN ephemeral lane, without
    /// re-homing or disturbing the living persona (#59). The clean same-model control:
    /// hold the harness fixed, vary the model. Ignored when a `gene` is set (a gene names
    /// its own forged base). None → fork onto her live lane as before. Must be a loadable
    /// id from `ai/inference/models`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub base_model_id: Option<String>,
    /// Team mode: with `Some(n>=1)`, add a REVIEWER teammate (a fresh fork of the SAME
    /// persona/model) that reviews + corrects the writer's answer before grading — the
    /// undeniable team proof (same model, same tasks, +1 teammate → does coordination lift?).
    /// None/0 = solo. First slice supports one reviewer; live single-pass path only (no gene,
    /// no base_model_id).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub reviewers: Option<u32>,
    /// Max act→observe cycles per task before it counts as unfinished. Default 32
    /// (see DEFAULT_MAX_ACTS); a task row's own max_acts overrides per task.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub max_acts: Option<u32>,
    /// Agentic-recovery budget: how many times a FAILED test-graded task is handed its
    /// compiler/test output to fix before it scores a miss. Default [`MAX_FAIL_RETRIES`].
    /// Set `0` for the ONE-SHOT baseline (what plain inference / unsloth gets on the same
    /// weights) — so a `0` vs `N` A/B on the identical model+benchmark measures exactly the
    /// edge our agentic loop adds, repeatably, from one param.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub max_retries: Option<u32>,
    /// Decoding temperature pin for the WHOLE run. **Omitted → her LIVED
    /// sampling** (the engineerable form — Joel 2026-08-23: "the old form is
    /// useless"): greedy 0.0 measured pathological on long-context agentic
    /// work (16k-token think spirals hitting the reply ceiling exactly, 4+min
    /// decodes) while every serious agentic harness samples. Pass
    /// `Some(0.0)` to opt INTO the old byte-reproducible greedy pin. A run's
    /// number is only comparable to runs at the same pin either way.
    #[serde(default)]
    #[ts(optional)]
    pub temperature: Option<f32>,
    /// Free-text label for THIS run, written to the progress ledger so a trend
    /// line is readable: "baseline", "taught show-output", "genome v2", etc.
    /// The ledger is how you "mark improvement as you go" — every eval leaves a
    /// dated, labelled, test-anchored mark at `~/.continuum/progress/<id>.jsonl`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub note: Option<String>,
    /// Fire-and-poll (#86): when true, the eval is spawned DETACHED — `run` returns a job
    /// handle immediately (so it survives the IPC client disconnecting on a long acting run)
    /// and the REAL result lands in the progress ledger (`~/.continuum/progress/<persona>.jsonl`),
    /// which the caller polls by `note`. Default/false = run inline and block as before.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detach: Option<bool>,
    /// Handle for a detached run (#86 fire-and-poll). Minted by the command when omitted;
    /// the progress-ledger row carries it, and `cognition/eval-status` polls by it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
    /// Repo-work seam (#49): pin the persona's file engine at this directory (e.g. a SWE-bench
    /// target-repo clone) BEFORE her cycle, by invoking `code/create-workspace` through HER OWN
    /// identity-bearing executor. Deterministic rooting — no reliance on the model choosing to
    /// call create-workspace itself. `None` → she uses the default root (core cwd) exactly as
    /// before, so this is opt-in with zero change to every existing eval path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub workspace_root: Option<String>,
    /// Glass-box seam (task #14): if set, wrap the eval-fork's cognition in the JSONL
    /// [`JsonlWorkspaceCaptureSink`] so every tick's bids + DECISION + timings land in
    /// `<capture_dir>/<persona_id>.jsonl` — makes a MEASURED run inspectable (did she Act or
    /// Respond? which tool? why 0 edits?) without touching her live mind. `None` → Noop capture
    /// exactly as before. Opt-in: zero change to every existing eval path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub capture_dir: Option<String>,
    /// LEARN mode (the exam as a legitimate teacher). When true, after the exam the
    /// redacted lesson of each task — "I was asked X; I solved it / did not (grade Y)" with
    /// the held-out answer key scrubbed — is admitted into the LIVING persona via
    /// `admit_reflection`, so she carries the *experience* forward and gets better across
    /// retakes WITHOUT ever memorizing the crib sheet. The exam still runs on the fork
    /// (#59: living persona never frozen/degraded); only the redacted lesson crosses back.
    /// This is what makes "learn from the exam" honest — provably clean, encouraged. Default
    /// false = pure measurement (the discarded fork teaches nothing, as before). Single-pass
    /// only in this slice (ignored under a `gene` A/B). [[redaction-makes-exam-learning-honest-so-encourage-it]]
    /// No `Default` on [`LearningPolicy`] — every Rust caller must state which this run is.
    #[serde(default = "LearningPolicy::wire_default")]
    #[ts(type = "boolean", optional)]
    pub learn: LearningPolicy,
    /// REPRODUCIBLE-ABSOLUTE mode (#207): suppress the fork's episodic recall so a
    /// self-contained proctored exam scores the SAME absolute number run-to-run. Each eval
    /// re-forks from her LIVING durable engram store, which grows as she lives between runs;
    /// injecting it into a self-contained task (HumanEval, a repo-nav question, a from-scratch
    /// UI build) recalls unrelated room chatter that drifts the prompt — noise AND
    /// nondeterminism, unrelated to sampling (greedy is already pinned) or serving. `true`
    /// omits the recall faculty from the fork; system + task + grounding (roster/doctrine/
    /// workspace-map) remain. The LIFT (base vs gene in one fork) is reproducible either way;
    /// this pins the ABSOLUTE baseline so today's number compares to last week's. `None`/false =
    /// memories intact (the natural persona), unchanged for every existing path. NOT a life
    /// knob — a benchmark control, sibling of the greedy-temperature and directed-turn pins.
    /// [[eval-reproducibility-is-two-tier-lift-controlled-absolute-drifts]]
    #[serde(
        default,
        alias = "suppressRecall",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(optional)]
    pub suppress_recall: Option<bool>,
    /// HELP ARM (rung one of room collaboration, 2026-08-24): when true and the
    /// eval is room-scoped, the exam condition DECLARES that asking peers is
    /// legal — the task prompts say so, and a listener folds live room speech
    /// from OTHERS into the fork's working memory so she perceives replies
    /// between acts. Exchanges are receipts in the transcript; score under this
    /// arm is reported as OURS+help, never conflated with solo. Default off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub help: Option<bool>,
}

/// Each optional wire parameter's default, resolved in exactly one place.
impl CognitionEvalParams {
    /// The gym reference this run grades and records: the caller's `eval_set`, else the
    /// committed default.
    pub(super) fn eval_set_reference(&self) -> &str {
        self.eval_set.as_deref().unwrap_or(DEFAULT_EVAL_SET) // unwrap_or: an omitted eval_set means the committed default set genuinely runs
    }

    /// The run id carried on every row of this run, minting one when the caller set none.
    pub(super) fn run_id_or_mint(&self) -> String {
        self.run_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string()) // unwrap_or_else: a caller-less run still needs one id for its rows and its world
    }

    /// The act→observe budget per task.
    pub(super) fn max_acts(&self) -> usize {
        self.max_acts.unwrap_or(DEFAULT_MAX_ACTS) as usize // unwrap_or: an omitted budget is the documented default
    }

    /// How many times a failed test-graded task is handed its output to fix.
    pub(super) fn max_retries(&self) -> u32 {
        self.max_retries.unwrap_or(MAX_FAIL_RETRIES) // unwrap_or: an omitted retry budget is the documented default
    }

    /// Fire-and-poll mode. Omitted means the caller waits for the result.
    pub(super) fn detached(&self) -> bool {
        self.detach.unwrap_or(false) // unwrap_or: an omitted detach flag is the inline run
    }

    /// The help arm. Omitted means the solo arm, the default exam condition.
    pub(super) fn help_arm(&self) -> bool {
        self.help.unwrap_or(false) // unwrap_or: an omitted arm flag is the solo arm
    }

    /// Suppress the fork's episodic recall. Omitted keeps her memories intact.
    pub(super) fn suppress_recall(&self) -> bool {
        self.suppress_recall.unwrap_or(false) // unwrap_or: an omitted flag keeps the natural persona
    }

    /// TEAM mode: at least one reviewer on a live single pass (never an A/B or a pinned base).
    pub(super) fn wants_team(&self) -> bool {
        self.reviewers.is_some_and(|n| n >= 1) && self.gene.is_none() && self.base_model_id.is_none()
    }
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct EvalTaskResult {
    pub id: String,
    /// Did the task pass (test exit 0, or substring matched)?
    pub ok: bool,
    /// Human-readable verdict: "tests passed" / a trimmed traceback / "timeout
    /// (10s)" for test tasks; "substring match" / "no match" for descriptive ones.
    pub grade: String,
    /// How many times she acted (ran code / read / searched) before settling.
    #[ts(type = "number")]
    pub acts: u32,
    /// What she SPOKE once settled — the COMPLETE generation (empty if she ran out of
    /// the act budget mid-action — an honest "did not finish", never fabricated). For a
    /// coder exam the answer IS the code, so this must carry the whole thing: a 200-char
    /// cap stored only the "Sure, I can help..." preamble and made every failure
    /// undiagnosable AND un-minable for a correction corpus (2026-07-21). Bounded by
    /// ANSWER_CAPTURE_CHARS against a pathological loop-to-length, which is itself a
    /// signal worth capturing up to the cap.
    pub answer: String,
    /// Wall-clock latency to SETTLE this task: the summed deliberation-generation
    /// time across every act→observe tick (the model's own measured request time,
    /// not the harness's). The latency axis of the scoreboard, per task.
    #[ts(type = "number")]
    pub latency_ms: u64,
    /// Completion tokens the model emitted across the task's deliberation turns —
    /// the work done, paired with `latency_ms` to derive throughput.
    #[ts(type = "number")]
    pub output_tokens: u32,
    /// Decode throughput for this task: `output_tokens / (latency_ms / 1000)`. The
    /// speed axis of the scoreboard, per task. 0 when the gateway omitted `usage`.
    /// NOTE: WALL-CLOCK tok/s — diluted by prefill + cognition overhead. See
    /// `decode_tokens_per_second` for the lane's undiluted rate.
    pub tokens_per_second: f64,
    /// REAL decode throughput from the lane clock (`output_tokens / decode_ms`),
    /// undiluted by prefill. The honest generation speed. 0 when the lane omitted
    /// timings (cloud / older endpoints).
    pub decode_tokens_per_second: f64,
    /// Fraction of prompt tokens served from KV cache across this task's acts —
    /// `cached / (cached + prefilled)`. Low = re-encoding the ~2000-token prompt
    /// every act (the dominant Metal inefficiency). 0 when no lane timings.
    pub cache_hit_rate: f64,
    /// Lane wall-ms spent PREFILLING across this task's acts (the re-rasterization
    /// tax). On Metal this dwarfs `decode_ms`; the lever to drive down.
    #[ts(type = "number")]
    pub prefill_ms: u64,
    /// Lane wall-ms spent DECODING across this task's acts (actual generation).
    #[ts(type = "number")]
    pub decode_ms: u64,
}

/// Why a run could NOT produce a trustworthy score — the serving lane never held a
/// verified, decode-capable context for the whole exam. This is NOT "she failed": the
/// harness never gave her a working lane, so the accuracy signal is meaningless. A run
/// carrying this is reported as infra-unavailable, and `score`/`pass_rate`/`results`
/// MUST be read as void, never as a real "she scored 0" — the exact fake-zero the
/// Proctored Exam Session exists to make impossible.
/// [[proctored-exam-session-dependable-benchmark]] [[benchmark-needs-its-own-serving-lane]]
#[derive(Debug, Clone, Serialize, TS)]
// Exported for the same reason as `EvalTask` above: this struct is a field of the
// exported result (`infra_unavailable`), so ts-rs emits a binding on every build
// regardless — a derive without an export left it an UNTRACKED orphan that
// regenerated forever (audit card #424). Canonical protocol root, sibling of
// `cognition/EvalTask.ts`.
#[ts(export, export_to = "../../../protocol/typescript/cognition/InfraUnavailable.ts")]
pub struct InfraUnavailable {
    /// Human cause: which axis broke (not-ready / connect-refused / not-the-served-model
    /// / compute-error / stream-idle timeout), naming the task it broke on. Display-only
    /// — the classification that produced it is [`SettleOutcome::inference_error`], never
    /// re-parsed from this string.
    pub reason: String,
    /// How many tasks were attempted before the run aborted. The exam is INCOMPLETE — a
    /// non-zero `score` over `tasks_attempted` is still void (some attempts phantom-failed
    /// on a dead lane), which is why the presence of this whole struct — not the count —
    /// is the void flag.
    #[ts(type = "number")]
    pub tasks_attempted: u32,
    /// How many attempted tasks hit an UNRECOVERABLE infra fault (the lane never returned
    /// to decode-ready within the bounded re-verify+retry budget). ≥1 ⇒ the lane died
    /// mid-exam and continuing would only accrue more phantom zeros.
    #[ts(type = "number")]
    pub infra_faults: u32,
}

#[derive(Debug, Clone, Serialize, TS, Default)]
pub struct CognitionEvalResult {
    /// The run handle (#86): present on a detached ack AND on the ledger row, so the
    /// two halves of fire-and-poll join on one id.
    pub run_id: Option<String>,
    /// Who the run is about.
    ///
    /// STILL `String`, and deliberately so pending the next slice: this struct
    /// derives `Default` across 21 fields, and a persona reference has NO sensible
    /// default — an empty one is a nonsense value that would read as a real answer.
    /// The fix is to split the fire-and-poll HANDLE from the completed RESULT (they
    /// are two different things wearing one struct: a handle knows only the
    /// requested `PersonaRef`, a result knows the resolved `PeerId`), which is a
    /// bigger change than this one. Fabricating a default to make the type check
    /// would be the same reflex as `unwrap_or` — it makes the compiler quiet and the
    /// runtime wrong.
    pub persona_id: String,
    /// True = this is a fire-and-poll JOB HANDLE (#86), NOT a completed run: the eval was
    /// spawned detached and its real result is in the progress ledger, not in these fields
    /// (which are all zero on a handle). Poll `~/.continuum/progress/<persona_id>.jsonl`,
    /// filtering by the `note` you passed. False/absent = a normal, complete result.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub detached: bool,
    /// Tasks passed.
    #[ts(type = "number")]
    pub score: u32,
    /// Tasks attempted.
    #[ts(type = "number")]
    pub total: u32,
    /// `score / total` — THE number a change is measured against. In A/B mode
    /// (a `gene` was given) this is the CANDIDATE (gene-paged-in) pass-rate.
    pub pass_rate: f64,
    /// Fraction of tasks where she ACTED at least once (ran code / read / searched)
    /// before settling — i.e. she verified instead of asserting. Memory flags
    /// `acts=0` (answers from the model's head, never runs it) as the #1 trainable
    /// lever; this is that lever as a tracked number, climbing toward 1.0 as the
    /// verify reflex takes hold. Reality-anchored alongside `pass_rate`: a high
    /// pass_rate with a low self_verify_rate means she's guessing right, not knowing.
    pub self_verify_rate: f64,
    /// Mean per-task settle latency across the set (ms). The headline LATENCY number
    /// — the branch's namesake — reported next to `pass_rate` so accuracy and speed
    /// move on the same scoreboard, never one traded silently for the other.
    pub mean_latency_ms: f64,
    /// 95th-percentile per-task settle latency (ms). The tail a mean hides: a model
    /// that's fast-on-average but occasionally stalls shows it here.
    #[ts(type = "number")]
    pub p95_latency_ms: u64,
    /// Mean WALL-CLOCK decode throughput across the set (tokens/sec) — diluted by
    /// prefill + cognition overhead. Kept as the gross headline; `mean_decode_tps`
    /// is the lane's honest rate.
    pub mean_tokens_per_second: f64,
    /// Mean REAL decode throughput across the set (lane clock, undiluted by
    /// prefill). This is the true generation speed — the number to push UP. The gap
    /// between this and `mean_tokens_per_second` IS the prefill+overhead tax.
    pub mean_decode_tokens_per_second: f64,
    /// Mean KV-cache hit-rate across the set (`cached / total prompt tokens`). Low =
    /// the ~2000-token prompt is re-prefilled every act — the dominant Metal sink.
    /// The number to push toward 1.0 via prompt-order / cache-reuse levers.
    pub mean_cache_hit_rate: f64,
    /// Total lane wall-ms spent PREFILLING across the whole set vs `total_decode_ms`.
    /// Measured 77% of eval time was here, not decode — the headline "where the time
    /// goes" split that makes speed iterable instead of one conflated tok/s.
    #[ts(type = "number")]
    pub total_prefill_ms: u64,
    /// Total lane wall-ms spent DECODING across the whole set.
    #[ts(type = "number")]
    pub total_decode_ms: u64,
    /// Total completion tokens emitted across the whole set — the gross work done,
    /// for cost/throughput accounting against wall-clock.
    #[ts(type = "number")]
    pub total_output_tokens: u32,
    pub results: Vec<EvalTaskResult>,
    /// The gene measured (A/B mode only) — its name, echoed so a run is traceable.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub gene_id: Option<String>,
    /// The BASELINE pass-rate (base model, gene paged out) — A/B mode only. `score`/
    /// `pass_rate`/`results` above are the candidate arm; this is what it's measured
    /// against.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub base_pass_rate: Option<f64>,
    /// `pass_rate - base_pass_rate` — the LIFT the gene produced (A/B mode only).
    /// Positive = the gene made her a better coder; the measure→decide gate adopts
    /// only `lift > 0`. Negative = an overfit/regressing gene, correctly rejected.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub lift: Option<f64>,
    /// Which device the measurement lane ran on: `"gpu"` or `"cpu"` (A/B mode), or
    /// `"gpu (live persona lane)"` in single-pass mode. GPU-FIRST is the policy — a
    /// `"cpu"` here means the GPU was genuinely full, and `lane_placement_reason`
    /// says why. Surfaced so the harness never hides a ~10×-slower CPU run behind a
    /// quiet `mean_tokens_per_second` ([[optimization-is-always-first]]).
    pub lane_placement: String,
    /// Why that device was chosen (the GPU-first decision, in words).
    pub lane_placement_reason: String,
    /// Live free VRAM (bytes) at lane spawn, net of the resident living lane — the
    /// headroom the GPU-first decision saw. `None` when no GPU monitor / single-pass.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub lane_free_vram_bytes: Option<u64>,
    /// Estimated weight+scratch footprint (bytes) of the measurement lane's base —
    /// what GPU-first weighed against free VRAM. `None` when unsized / single-pass.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub lane_estimated_footprint_bytes: Option<u64>,
    /// Set ONLY when infra prevented a trustworthy measurement (the shared serving lane
    /// flipped not-ready / refused / compute-errored and never recovered within the
    /// re-verify+retry budget). When present, `score`/`pass_rate`/`results` are VOID —
    /// the run never completed on a verified lane, and reading `pass_rate: 0.0` as "she
    /// scored 0" is precisely the fake-zero this guards against. Absent = a real, scored
    /// run whose number can be trusted. Every human-facing render site (ledger row,
    /// eval-status, benchmark/run) branches on this before showing a pass-rate.
    /// [[proctored-exam-session-dependable-benchmark]]
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub infra_unavailable: Option<InfraUnavailable>,
}
