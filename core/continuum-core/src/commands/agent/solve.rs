//! `agent/solve` — the headless-agent keystone. Drop a persona's WHOLE self into a working
//! directory, hand her one task, let her work it with her hands, and return the patch.
//!
//! This is the primitive every external benchmark harness composes on
//! ([[terminal-bench-is-the-meta-harness-build-one-agent-adapter]],
//! `docs/architecture/BENCHMARK-HARNESS-INTEGRATION.md`): Terminal-Bench, SWE-bench, Aider
//! Polyglot all reduce to "put an agent in a sandbox cwd with a task, collect the diff, grade
//! with our own tests." A benchmark being "LLM-shaped" is an ergonomics problem solved by the
//! RAG (the task is layered into her situation) — NEVER by stripping her
//! ([[benchmark-must-never-score-persona-against-a-soul-stripped-copy]]).
//!
//! She competes WHOLE: `with_tools: true` (hands on), `suppress_recall: false` (memory/RAG on),
//! her genome paged in on the measurement lane. Same drive machinery as `cognition/eval`
//! (`fork_eval_cycle_with_adapter` → `drive_to_settle`), minus the grader — the external harness
//! grades. One drive, two consumers (eval scores; agent/solve emits a patch).

use crate::cognition::learning_policy::LearningPolicy;
use async_trait::async_trait;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use ts_rs::TS;
use uuid::Uuid;

use crate::sdk_codegen::{AccessLevel, ActionCommand, CommandError, Ctx};

/// Max act→observe cycles she may take on one task before it counts as unfinished. Agentic
/// SWE tasks (read → edit → compile → fix) need several; default generously.
// 32, up from 12 (glass-boxed 2026-08-08, benchy-sympy-22840-n9 attempt 1): the
// 12-act cap — not the clock — was the binding constraint. She was cut off
// mid-recovery (edit failed NOT-FOUND → the error taught verbatim-copy → she was
// re-reading the target region when the budget expired) with 2.5 of the 3
// deadline hours unused. Field SWE agents routinely take 30–80 steps; a count
// cap that binds before the deadline is the hardcoded-LCD-clamp shape the
// cognition pipeline doc forbids. The deadline stays derived from this budget
// (× PER_ACT_ALLOWANCE_SECS), so the wedge watchdog scales with it.
const DEFAULT_MAX_ACTS: u32 = 32;
/// How long to wait for the forked cognition template (post-spawn `register_from_cfg` race).
const FORK_WAIT_TRIES: u32 = 20;

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/agent/AgentSolveParams.ts"
)]
pub struct AgentSolveParams {
    /// The persona (UUID, spawned) whose FULL cognition works the task.
    pub persona_id: crate::identity::PersonaRef,
    /// The model to measure her on — forged into a dedicated measurement lane (her genome pages
    /// in on top). A loadable id from `ai/inference/models`.
    pub base_model_id: String,
    /// The task instruction, verbatim (layered into her situation/RAG).
    pub task: String,
    /// The working directory she acts in — her tools (`code/write`, `code/shell`, …) are rooted
    /// here. Ideally a git repo so the result is a clean diff; a bare dir also works.
    pub workspace: String,
    /// Max act→observe cycles (default 12).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub max_acts: Option<u32>,
    /// The ROOM this run happens in — `benchmark/dispatch`'s per-run activity room
    /// (#329). Every act she executes radiates a `persona:act` receipt into it, so
    /// the run's work lands in the room's transcript as collapsed receipts (#243)
    /// and anyone standing there — human screen or citizen mind — perceives it
    /// through the ONE ViewState pipe.
    ///
    /// Omitted → `Uuid::nil()`, which is the ROOMLESS shape: `apply_act` skips
    /// receipt radiation entirely for a nil room (radiating them stole the
    /// single-room chat projection onto a phantom, live-proven 2026-08-12), so a
    /// roomless solve does its work invisibly. That was every dispatched benchmark
    /// run until this param existed — the exact disconnection
    /// BENCHMARKS-ARE-ADAPTERS-NOT-A-RUNNER.md names as the failure mode, and the
    /// reason the flywheel saw no turns from a full graded attempt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "string")]
    pub room: Option<Uuid>,
    // See `RunVisibility` below: the roomless case is now DECLARED rather than defaulted,
    // because a silent invisible run is how 13,209 of them accumulated unnoticed (#425).
    /// Fire-and-poll (#86): when true, the solve is spawned DETACHED — `run` returns a job
    /// handle NOW (arms empty, `detached: true`) and the REAL result (patch + acts) lands in
    /// `~/.continuum/progress/agent-solve-<run_id>.json`. A real agentic drive (N full-generation
    /// acts) outlives the IPC client timeout — a Devstral write→compile→fix loop took 12 min —
    /// so a Terminal-Bench/SWE-bench harness MUST fire-and-poll, never block on the socket.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub detach: Option<bool>,
    /// Correlation id for a detached run (echoed in the ack + the result file). Omit → minted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
    /// LEARN mode (#221 slice 3 — the loop-closer): after the solve, admit the EXPERIENCE
    /// of the work (task, act count, files touched — never the patch content, never any
    /// grader state) into the LIVING persona's memory, exactly as `cognition/eval`'s
    /// learn mode transfers redacted exam lessons. This is what lets work supersede
    /// stale beliefs: a day of python tasks consolidates into python facts, and the
    /// dream's supersession review demotes "you work with main.rs" — work IS training.
    /// The measurement fork itself stays #59-isolated either way; only the lesson
    /// crosses back.
    ///
    /// There is NO default on the Rust side — [`LearningPolicy`] has no `Default` impl, so
    /// every construction site must state whether this run is a measurement or her life. See
    /// [`crate::cognition::learning_policy`] for why (BigMama, 2026-08-06: two modules had
    /// opposite defaults for this one field, and a fail-safe default would only have changed
    /// which forgetful caller got burned). An OMITTED wire field resolves to
    /// [`LearningPolicy::DoNotLearn`] — this command is the headless BENCHMARK entrypoint
    /// (#218), so its population is measurement-heavy and omission must fail safe.
    #[serde(default = "LearningPolicy::wire_default")]
    #[ts(type = "boolean", optional)]
    pub learn: LearningPolicy,
    /// GLASS-BOX (opt-in): directory for the JSONL turn-capture sink — every tick's bids +
    /// DECISION + timings append to `<dir>/<persona_id>.jsonl`, same sink `cognition/eval`
    /// wires (task #14). THE tool for diagnosing an acts=1 silent settle: the capture says
    /// whether she chose Act/Respond/Pass and why, where the bare ledger only shows the
    /// aftermath. Fork-only, never her live mind.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub capture_dir: Option<String>,
    /// HARNESS-INTERNAL (set by the attempts loop, never by callers): sha256 of
    /// the previous FAILED attempt's patch. When this attempt settles with a
    /// byte-identical diff, ONE bounded re-drive fires with the hash-proven
    /// fact — catching the resubmission BEFORE a redundant grade burns the
    /// attempt (round E receipts: BOTH citizens copied their failed patch on
    /// attempt 3, and post-grade detection could only warn an attempt 4 that
    /// never exists).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(skip)]
    pub prev_failed_patch_sha: Option<String>,
    /// DIAGNOSTIC ONLY (default false — she competes WHOLE, memory ON). When true, her
    /// durable episodic/semantic recall is suppressed for this run — the same probe
    /// `cognition/eval` exposes ([[eval-measures-the-true-full-being-not-a-stripped-copy]]).
    /// Its ONLY legitimate use is glass-boxing session contamination: run a failing task
    /// with recall OFF — PASS ⇒ the failure is recall-mediated (stale/cross-task engrams
    /// surfacing), STILL FAIL ⇒ the failure is deeper (pipeline/model). NEVER a scoring
    /// mode — a benchmark number produced with recall off measures a stripped copy, not the
    /// being ([[benchmark-must-never-score-persona-against-a-soul-stripped-copy]]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub suppress_recall: Option<bool>,
    /// What the CALLER will grade — the turn's contract, declared by the harness that
    /// holds the answer key. Default [`Deliverable::Answer`]: her utterance is the
    /// result (#220's answer-graded tasks). A SWE-style harness that applies the DIFF
    /// and never reads the speech declares [`Deliverable::Workspace`], and the settle
    /// driver stops treating a zero-change explanation as a finished turn. Structural,
    /// caller-owned; it steers nothing about WHAT she does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub deliverable: Option<Deliverable>,
    /// This run is SCORED — a benchmark instance whose verdict comes from applying her diff and
    /// running the tests, not from anything she says. Hardens her write path: an edit whose code
    /// would land inside a string literal is refused rather than warned (#317), because the run
    /// cannot recover from a file it believes it fixed.
    ///
    /// Default `false`, and deliberately NOT inferred from `deliverable` — `agent/solve` also
    /// does real work for real teammates, and a citizen doing real work writes code as text
    /// whenever she means to (a docstring example, a fixture). Only the caller that is GRADING
    /// her knows the ambiguity is gone, so only that caller sets this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub scored: Option<bool>,
    /// Directories to PREPEND to `PATH` for her shell — the interpreter/toolchain this
    /// task needs in order to be RUNNABLE. A SWE harness passes the era-matched venv's
    /// `bin` here so `python` and `pytest` exist for her.
    ///
    /// Without it she can write a fix but never execute anything to check it. Measured
    /// on sympy-21379: a correct reproduction script met `bash: python: command not
    /// found`, and the run scored as a capability failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub path_prepend: Option<Vec<String>>,
    /// N CHANCES (Joel, 2026-08-08: "they too need to learn how to investigate and fix
    /// their code"): how many graded attempts this detached run may take. After a
    /// non-resolved auto-grade, the NEXT attempt re-enters the SAME workspace (her
    /// previous edits intact) with the grader's verdict — named failing tests — appended
    /// to the task, so investigating her own failure IS the work. Only meaningful on a
    /// detached, auto-graded run; a resolved grade, an ungradeable tree, or a harness
    /// fault ends the run early. Default 1 (one shot, exactly the old behavior) — the
    /// per-benchmark adapter that dispatches the run owns its N, not this abstraction.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub attempts: Option<u32>,
}

/// Whether this run's acts will be PERCEIVABLE, decided once and named.
///
/// # Why this is a type and not `p.room.unwrap_or_else(Uuid::nil)`
///
/// It was that `unwrap_or_else` (#425). A nil room makes `apply_act` skip receipt radiation
/// entirely, so the run executes normally and lands in NO transcript — and nothing anywhere
/// said so. 13,209 turns accumulated in that state (8.7% of all turns; 35% for one citizen)
/// before anyone measured it, because an invisible run and a visible one produce identical
/// logs. That is the same defect shape as a fetch cap that silently truncates: the system
/// took a consequential branch and declined to mention it.
///
/// A roomless run is still LEGITIMATE — a bare `agent/solve` with no activity behind it has
/// no room to radiate into, and inventing one would put receipts on a phantom (live-proven
/// 2026-08-12, it stole the single-room chat projection). So this does not refuse. It
/// DECLARES, so the invisibility is a stated property of the run rather than a silence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunVisibility {
    /// Acts radiate `persona:act` receipts into this room — perceivable by a human screen
    /// and a citizen mind through the one ViewState pipe.
    InRoom(Uuid),
    /// No room: the work executes and is perceived by nobody, and no curriculum-visible
    /// room turn is produced.
    Invisible,
}

impl RunVisibility {
    /// The single place the room param becomes a decision.
    pub fn resolve(room: Option<Uuid>) -> Self {
        match room {
            // A caller passing the nil uuid EXPLICITLY means the same thing as omitting it;
            // treating them differently would let a nil slip through as "in room", which is
            // exactly the silent branch this type exists to close.
            Some(r) if !r.is_nil() => RunVisibility::InRoom(r),
            _ => RunVisibility::Invisible,
        }
    }

    /// The uuid the act pipeline expects — nil for the invisible case, which is the shape
    /// `apply_act` already keys its skip on.
    pub fn room_id(&self) -> Uuid {
        match self {
            RunVisibility::InRoom(r) => *r,
            RunVisibility::Invisible => Uuid::nil(),
        }
    }

    /// What to say when the run will be invisible. `None` when it is perceivable — a
    /// visible run needs no announcement, and warning on the happy path trains people to
    /// ignore the warning.
    pub fn warning(&self) -> Option<&'static str> {
        match self {
            RunVisibility::InRoom(_) => None,
            RunVisibility::Invisible => Some(
                "this run has NO room: its acts execute but radiate no receipts, so nobody \
                 — human or citizen — can perceive the work, and it produces no room turn. \
                 Pass `room` (benchmark/dispatch supplies its per-run activity room) to make \
                 the run perceivable.",
            ),
        }
    }
}

/// What the caller grades when the solve returns. Two genuinely different contracts,
/// so it is an enum on the wire, never a magic string ([[strings-to-enums]]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/agent/Deliverable.ts"
)]
pub enum Deliverable {
    /// Her spoken answer is the result (the default — every non-diff task).
    #[default]
    Answer,
    /// The state of the workspace is the result; the grader applies the diff.
    Workspace,
}

#[derive(Debug, Clone, Serialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/agent/AgentSolveResult.ts"
)]
pub struct AgentSolveResult {
    pub persona_id: crate::identity::PersonaRef,
    pub model: String,
    /// How many times she acted (edited / ran / read) before settling.
    #[ts(type = "number")]
    pub acts: u32,
    /// Her final spoken answer (the "mouth" — some benchmarks grade this; SWE grades the patch).
    pub spoken: String,
    /// Unified `git diff` of everything she changed in the workspace — the "hands" artifact the
    /// SWE/Terminal-Bench harness applies + tests. Empty if she made no file changes (or the
    /// workspace is not a git repo).
    pub patch: String,
    /// Paths she touched (from `git diff --name-only`).
    pub files_changed: Vec<String>,
    /// Paths her acts NAMED (reads, searches, edit attempts — any `file_path`/`path`
    /// arg on an executed call), first-touch order. The investigation trail as STATE:
    /// a failed edit or a read appears here and nowhere else. The N-chances retry
    /// threads these into the next attempt's task, because a retry is a FRESH turn
    /// with fresh working memory — without this, "the file you already identified"
    /// names knowledge the next attempt does not have (glass-boxed 2026-08-08,
    /// benchy-sympy-22840-n4 attempt 2: 10 of 12 acts re-deriving cse_main.py).
    #[serde(default)]
    pub files_examined: Vec<String>,
    /// True when this is the immediate ACK of a detached run (`acts`/`patch` empty — poll the
    /// result file `agent-solve-<run_id>.json` for the real outcome).
    pub detached: bool,
    /// The run's correlation id (set on a detached ack + the written result file).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
    /// INFRASTRUCTURE FAILURE, never a wrong answer. `Some(cause)` when the settle loop
    /// stopped because the deliberation model call FAILED (lane torn down mid-drive, a
    /// serving lane refusing a model it isn't hosting, a timeout) rather than because she
    /// finished. `SettleOutcome::inference_error` has carried this all along and its own
    /// doc says the grader MUST treat it as infra — but NOTHING READ IT, so a run
    /// truncated at act 7 of 30 reported `acts: 7, patchBytes: 0` and was indistinguishable
    /// from a persona who simply failed. Measured 2026-08-04 on sympy-21379: the lane went
    /// `serving: <none>, ready: false` mid-drive and the verdict said nothing at all.
    /// A number produced with this set is NOT a score
    /// ([[a-benchmark-zero-is-a-claim-about-the-harness-until-proven-otherwise]]).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub infra_error: Option<String>,
}

/// `agent/solve` — headless single-task agent run. Pure orchestration over the eval drive
/// machinery; never hand-spawns a llama-server (the serving system owns lifecycle) — it stands up
/// a measurement lane the same way `cognition/eval` does, holds it for the drive, drops it after.
#[derive(Default)]
pub struct AgentSolve;

#[async_trait]
impl ActionCommand for AgentSolve {
    const NAME: &'static str = "agent/solve";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Headless agent: drop a persona's WHOLE self (genome, memory, RAG, tools, act→verify loop) \
         into a working directory, hand her one task, and return the git patch she produced + her \
         answer + act count. The primitive every external benchmark harness (Terminal-Bench, \
         SWE-bench, Aider Polyglot) composes on. She is never stripped to fit a benchmark.";
    type Params = AgentSolveParams;
    type Output = AgentSolveResult;

    async fn run(&self, _ctx: &Ctx, p: AgentSolveParams) -> Result<AgentSolveResult, CommandError> {
        // Fire-and-poll (#86): a real agentic drive outlives the IPC client timeout, so `detach`
        // spawns the body on the runtime, writes the finished result to a poll file + emits a
        // terminal event, and returns a run_id NOW. The body is ctx-free (reaches the persona via
        // the global workspace registry), so it runs identically inline or detached.
        if p.detach.unwrap_or(false) {
            let run_id = p
                .run_id
                .clone()
                .unwrap_or_else(|| Uuid::new_v4().to_string());
            let run_id_ack = run_id.clone();
            let (persona_ack, model_ack) = (p.persona_id.clone(), p.base_model_id.clone());
            let mut inner = p;
            inner.detach = Some(false);
            inner.run_id = Some(run_id.clone());
            // HANDS-FREE GRADE eligibility, captured before `inner` moves: a SCORED,
            // workspace-deliverable run inside a citizen's staged SWE checkout
            // (`citizens/peers/<uuid>/workspace/swe/<instance>`) grades itself the
            // moment it settles — #346 slice 2's first cut. The instance id is the
            // checkout's own directory name (the shape `benchmark/swe-setup` stages).
            // #365: a scored run inside a staged SWE checkout that left `deliverable`
            // unset used to slip past this gate and burn its chances as ONE ungraded
            // attempt with no warning (live 2026-08-08: two sympy runs ended patchless
            // and silent). Unspecified is not a choice — infer the workspace
            // deliverable and say so. An EXPLICIT Deliverable::Answer is respected,
            // loudly, because it disarms grading on a run that asked to be scored.
            let swe_checkout = inner.workspace.contains("/workspace/swe/");
            if inner.scored.unwrap_or(false) && swe_checkout {
                match inner.deliverable {
                    None => {
                        inner.deliverable = Some(Deliverable::Workspace);
                        tracing::warn!(
                            run_id = %run_id,
                            "agent/solve: scored SWE-checkout run without `deliverable` — \
                             inferring deliverable=workspace so autograde + attempts arm (#365)"
                        );
                    }
                    Some(Deliverable::Answer) => {
                        tracing::warn!(
                            run_id = %run_id,
                            "agent/solve: scored SWE-checkout run with EXPLICIT deliverable=answer — \
                             autograde disarmed; attempts collapse to one ungraded run (#365)"
                        );
                    }
                    Some(Deliverable::Workspace) => {}
                }
            }
            let autograde_workspace = (inner.scored.unwrap_or(false)
                && matches!(inner.deliverable, Some(Deliverable::Workspace))
                && swe_checkout)
                .then(|| inner.workspace.clone());
            tokio::spawn(async move {
                let path = agent_solve_ledger_path(&run_id);
                // JOURNAL `state: running` NOW, before attempt 1 does anything (#2246,
                // live 2026-08-11): the result file used to be written only when an
                // attempt ENDED, so `benchmark/runs` — the projection whose whole job
                // is "silence must never be ambiguous with progress" — could not list
                // a run at all for the entire first attempt (an hour-plus on a full
                // SWE budget). Four dispatched solves ran invisible for 17 minutes
                // while every watcher read the empty projection as "nothing started".
                // The marker folds as `active` with the solver named (fold_run_card
                // reads `persona_id`); each finished attempt overwrites it with the
                // real result, exactly as before.
                // The instance under test — the staged checkout's own dir name (the
                // shape benchmark/swe-setup stages). Carried on EVERY ledger write so
                // the board (#329) names WHAT is being worked from second zero, not
                // just who; None outside a staged SWE checkout (a plain agent run).
                let instance: Option<String> = inner
                    .workspace
                    .contains("/workspace/swe/")
                    .then(|| {
                        std::path::Path::new(&inner.workspace)
                            .file_name()
                            .map(|s| s.to_string_lossy().to_string())
                    })
                    .flatten();
                if let Some(p) = path.as_ref() {
                    let _ = std::fs::write(
                        p,
                        serde_json::json!({
                            "state": "running",
                            "run_id": run_id,
                            "persona_id": inner.persona_id,
                            "workspace": inner.workspace,
                            "instance": instance,
                        })
                        .to_string(),
                    );
                }
                // HOLD THE LANE STEADY for the run's whole lifetime — the same RAII pin a
                // living-persona eval binds ([[benchmark-is-a-governor-preemption-lease]]).
                // Without it, the OPTIONAL grow-back re-home relaunches the lane under the
                // solve's first in-flight generation: measured THREE times on 2026-08-08
                // (benchy-22840-n7 and atlas-24066-n5 both died at act 0 to "stream read
                // error" when the post-boot window grow bounced the lane). A scored run is
                // exactly the demand the hold exists for; a real pressure emergency still
                // preempts (the hold only suppresses the optional grow, never a shrink).
                let _steady =
                    crate::modules::serving_daemon::ServingSteadyHold::acquire(run_id.clone());
                // N CHANCES: attempts loop. Each attempt is a full solve; a non-resolved
                // auto-grade re-enters the SAME workspace with the verdict appended to the
                // task (named failing tests — the teachable half of the grade). The loop
                // ends on: resolved, attempts exhausted, an ungradeable tree (gate/error —
                // a harness fault must never burn her chances), a non-graded run (nothing
                // to iterate against), or a solve failure. Result/grade files are OVERWRITTEN
                // per attempt — the poll surface shows the LATEST state; history lives on
                // the `benchmark.autograde` probe (attempt field) and the capture sink.
                let max_attempts = inner.attempts.unwrap_or(1).max(1);
                let base_task = inner.task.clone();
                let mut next_task = base_task.clone();
                // #379 follow-through (round D's learning-stuck finding): the sha of the
                // previous FAILED attempt's patch. When the current attempt's patch hashes
                // the same, the resubmission is detected as STATE — not inferred from
                // size — and the next contract leads with that fact. Round B and round D
                // both burned attempts on byte-identical resubmits the verdict prose
                // never surfaced as such.
                let mut prev_patch_sha: Option<String> = None;
                // Per-attempt DEADLINE (harnesses-first, Joel 2026-08-08): a wedged
                // fork/lane used to stall this loop SILENTLY FOREVER — glass-boxed
                // live: both graded runs froze after attempt 2 for 2.5h with zero
                // ticks, zero markers, and the operator found out by ASKING. Silence
                // must never be ambiguous with progress. Derived from the act budget,
                // never flat (eval's 600s bounds ONE small task; a 12-act SWE attempt
                // legitimately runs ~1h): budget × per-act allowance, generous 3×
                // headroom over the measured ~5.5 min/act. On expiry: loud probe +
                // loud marker on the poll surface, and the run ENDS — a retry into
                // the same wedge would be a loop, and detection is the job here.
                // 8 min/act: still ~1.5× the measured ~5.5 min/act worst case (and
                // 3× the ~2.4 min/act measured on the n9/n6 rounds), while keeping
                // the full-budget deadline at 32 × 8min ≈ 4.3h — comparable wedge
                // detection to the old 12 × 15min = 3h, at 2.7× the act budget.
                const PER_ACT_ALLOWANCE_SECS: u64 = 8 * 60;
                let attempt_deadline = std::time::Duration::from_secs(
                    inner.max_acts.unwrap_or(DEFAULT_MAX_ACTS).max(1) as u64
                        * PER_ACT_ALLOWANCE_SECS,
                );
                // #384: the attempt counter is MANUAL so an infra-void attempt (she
                // never worked — zero acts, no error, empty patch: the F1 signature,
                // where six null-decision settles during a serving transition graded
                // as capability zeros) can retry WITHOUT burning her chances. A zero
                // is a harness claim; the harness must not launder its own faults
                // into her record.
                let mut attempt = 1;
                let mut infra_void_retries: u32 = 0;
                const INFRA_VOID_RETRIES_MAX: u32 = 2;
                while attempt <= max_attempts {
                    let mut this_attempt = inner.clone();
                    this_attempt.task = next_task.clone();
                    this_attempt.prev_failed_patch_sha = prev_patch_sha.clone();
                    crate::probe!(
                        class = "benchmark.attempt.start",
                        run_id = %run_id,
                        attempt,
                        max_attempts,
                        deadline_s = attempt_deadline.as_secs(),
                        "solve attempt starting — pulse anchor for run-liveness watchers"
                    );
                    let body = match tokio::time::timeout(
                        attempt_deadline,
                        AgentSolve::solve_body(this_attempt),
                    )
                    .await
                    {
                        Ok(body) => body,
                        Err(_) => {
                            let msg = format!(
                                "attempt {attempt} of {max_attempts} exceeded its deadline \
                                 ({}s = max_acts × {}s) with no settlement — fork/lane wedge, \
                                 an INFRA fault, never a capability verdict",
                                attempt_deadline.as_secs(),
                                PER_ACT_ALLOWANCE_SECS,
                            );
                            crate::probe!(
                                class = "benchmark.stall",
                                run_id = %run_id,
                                attempt,
                                deadline_s = attempt_deadline.as_secs(),
                                "solve attempt DEADLINE EXCEEDED — ending the run loudly"
                            );
                            if let Some(path) = path.as_ref() {
                                let _ = std::fs::write(
                                    path,
                                    serde_json::json!({
                                        "failed": true,
                                        "infra_error": msg,
                                        "run_id": run_id,
                                        "attempt": attempt,
                                    })
                                    .to_string(),
                                );
                            }
                            tracing::error!(run_id = %run_id, attempt, "agent/solve detached attempt stalled past deadline");
                            break;
                        }
                    };
                    match body {
                        Ok(r) => {
                            // Ledger write carries the board facts the result struct
                            // doesn't: WHICH instance, attempt N of M (#329) — injected
                            // as JSON rather than widening AgentSolveResult, whose wire
                            // shape non-benchmark callers also consume.
                            if let (Some(path), Ok(mut v)) =
                                (path.as_ref(), serde_json::to_value(&r))
                            {
                                if let Some(obj) = v.as_object_mut() {
                                    obj.insert("attempt".into(), attempt.into());
                                    obj.insert("max_attempts".into(), max_attempts.into());
                                    if let Some(inst) = instance.clone() {
                                        obj.insert("instance".into(), inst.into());
                                    }
                                }
                                if let Ok(json) = serde_json::to_string_pretty(&v) {
                                    let _ = std::fs::write(path, json);
                                }
                            }
                            tracing::info!(run_id = %run_id, acts = r.acts, attempt, "agent/solve detached run complete");
                            // #384: ZERO acts + NO error + EMPTY patch = she never
                            // worked at all — the serving-transition signature (F1:
                            // decision:null ticks, ~60ms "deliberations", lane not
                            // resident at pre-flight). That is INFRA, never a
                            // capability verdict: retry the SAME attempt after a
                            // settling pause, bounded; exhausted retries end the run
                            // with a loud infra marker instead of a graded zero.
                            // #386 extension: an attempt whose settle carries ANY
                            // infra_error died to INFRASTRUCTURE by definition (the
                            // inference path failed her — round G: wedge-killed
                            // attempts with 'no TOKEN progress' still graded and
                            // burned). Same arm, same bound; her partial work stays
                            // in the workspace and the retry resumes from it.
                            if r.infra_error.is_some() || (r.acts == 0 && r.patch.is_empty()) {
                                if infra_void_retries < INFRA_VOID_RETRIES_MAX {
                                    infra_void_retries += 1;
                                    crate::probe!(
                                        class = "benchmark.attempt.infra_void",
                                        run_id = %run_id,
                                        attempt,
                                        retry = infra_void_retries,
                                        "attempt produced ZERO work with no error — \
                                         infra void (serving transition); retrying the \
                                         SAME attempt, her chances unburned (#384)"
                                    );
                                    tokio::time::sleep(std::time::Duration::from_secs(90)).await;
                                    continue;
                                }
                                crate::probe!(
                                    class = "benchmark.attempt.infra_void",
                                    run_id = %run_id,
                                    attempt,
                                    retry = infra_void_retries,
                                    "infra-void retries exhausted — run ends as INFRA, \
                                     never graded (#384)"
                                );
                                if let Some(path) = path.as_ref() {
                                    let _ = std::fs::write(
                                        path,
                                        serde_json::json!({
                                            "failed": true,
                                            "infra_error": "attempt produced zero work \
                                             with no error repeatedly — serving never \
                                             delivered a working mind (#384); this run \
                                             is an INFRA VOID, not a capability result",
                                            "run_id": run_id,
                                            "attempt": attempt,
                                        })
                                        .to_string(),
                                    );
                                }
                                break;
                            }
                            // The claim ran to a diff; now the diff runs to a VERDICT with
                            // no human in the loop. Same grader as `benchmark/swe-grade`
                            // (fresh clone, held-out tests) — its verdict path also writes
                            // the citizen's experience stream, so solve→grade→lesson is one
                            // unbroken chain. The verdict lands on the poll surface beside
                            // the result (`...<run_id>.grade.json`) + the probe stream.
                            let Some(ws) = autograde_workspace.clone() else {
                                break; // ungraded run — nothing to iterate against
                            };
                            let instance = std::path::Path::new(&ws)
                                .file_name()
                                .map(|s| s.to_string_lossy().to_string())
                                .unwrap_or_default();
                            // #379: the attempt's PATCH is a receipt, not a transient.
                            // Read the exact candidate the grader is about to read (same
                            // helper — one definition of "her diff"), persist it beside
                            // the run's captures, and put its sha256 on the wire. Round D
                            // (2026-08-08) needed "is att3 byte-identical to att2?" and
                            // NO artifact could answer: probes carried size only. Hash
                            // custody per the transcript standard (#377); the persisted
                            // patch is what a verdict-as-state lever will compare against.
                            let patch_sha256 =
                                match crate::commands::benchmark::workspace_candidate_diff(&ws) {
                                    Ok(diff) => {
                                        use sha2::{Digest, Sha256};
                                        let sha = format!("{:x}", Sha256::digest(diff.as_bytes()));
                                        // CUSTODY IS NOT OPTIONAL. The workspace is reset for
                                        // the next attempt, so this write is the only moment
                                        // her diff exists anywhere durable. Every failure to
                                        // keep it is announced — a silent drop is how a whole
                                        // round of evidence was lost (see `run_artifact_dir`).
                                        match run_artifact_dir(&run_id, inner.capture_dir.as_deref())
                                        {
                                            Some(dir) => {
                                                let path = dir
                                                    .join(format!("attempt-{attempt}.patch"));
                                                if let Err(e) = std::fs::create_dir_all(&dir)
                                                    .and_then(|_| std::fs::write(&path, &diff))
                                                {
                                                    tracing::error!(
                                                        run_id = %run_id,
                                                        attempt,
                                                        path = %path.display(),
                                                        error = %e,
                                                        "PATCH CUSTODY LOST — her diff could not \
                                                         be persisted and the workspace is about \
                                                         to be reset; this attempt's verdict will \
                                                         have no evidence behind it"
                                                    );
                                                } else {
                                                    crate::probe!(
                                                        class = "benchmark.patch.kept",
                                                        run_id = %run_id,
                                                        attempt,
                                                        bytes = diff.len(),
                                                        sha256 = %sha,
                                                        path = %path.display(),
                                                        "attempt patch persisted — the verdict has \
                                                         evidence behind it"
                                                    );
                                                }
                                            }
                                            None => tracing::error!(
                                                run_id = %run_id,
                                                attempt,
                                                "PATCH CUSTODY LOST — no artifact directory could \
                                                 be resolved (no CONTINUUM_HOME, no home dir)"
                                            ),
                                        }
                                        sha
                                    }
                                    Err(e) => {
                                        tracing::warn!(
                                            run_id = %run_id,
                                            attempt,
                                            error = %e,
                                            "attempt patch receipt could not be read — \
                                             verdict proceeds, custody hole logged"
                                        );
                                        String::new()
                                    }
                                };
                            let grade = crate::commands::benchmark::grade_swe(
                                crate::commands::benchmark::SweGradeParams {
                                    instance: instance.clone(),
                                    dataset: None,
                                    gold: None,
                                    patch: None,
                                    workspace: Some(ws),
                                },
                            )
                            .await;
                            match grade {
                                Ok(g) => {
                                    // attempt.end — the FULL verdict on the wire, the
                                    // bookend of benchmark.attempt.start (Joel 2026-08-08:
                                    // "emit events for everything — need to know"). Grades
                                    // were file-only; every wire consumer (probe router →
                                    // rooms, exam-room widgets, the pulse monitors) had to
                                    // scrape the ledger to learn an attempt's outcome.
                                    // ABSENCE vs ZERO, carried on the wire. `SweGradeResult.error`
                                    // documents its own contract — "a result with `error` is an
                                    // ABSENCE, not a zero, and must never be tallied as a failed
                                    // attempt" — and the grader earns it honestly (it re-runs the
                                    // PRISTINE tree before declaring an env fault, so a broken
                                    // patch is never mislabelled). That classification used to die
                                    // in the ledger file: attempt.end published `resolved=false
                                    // gate_ok=false` and nothing else, which every wire consumer
                                    // reads as a citizen who tried and failed. Measured 2026-08-13:
                                    // 8 of 36 instances (22%) grade UNGRADEABLE on this box, so the
                                    // unlabelled zeros were poisoning the denominator of every rate
                                    // computed off this stream. The flag rides the same event as
                                    // the verdict so no consumer has to scrape a file to tell a
                                    // capability zero from an absent measurement.
                                    let ungradeable = g.error.is_some();
                                    crate::probe!(
                                        class = "benchmark.attempt.end",
                                        run_id = %run_id,
                                        instance = %instance,
                                        attempt,
                                        max_attempts,
                                        resolved = g.resolved,
                                        gate_ok = g.gate_ok,
                                        ungradeable,
                                        grade_error = %g.error.as_deref().unwrap_or(""),
                                        f2p_passed = g.fail_to_pass_passed,
                                        f2p_total = g.fail_to_pass_total,
                                        p2p_passed = g.pass_to_pass_passed,
                                        p2p_total = g.pass_to_pass_total,
                                        patch_bytes = g.patch_bytes,
                                        patch_sha256 = %patch_sha256,
                                        failed_tests = %g.failed_tests.join(","),
                                        "solve attempt graded — the verdict, on the wire"
                                    );
                                    crate::probe!(
                                        class = "benchmark.autograde",
                                        run_id = %run_id,
                                        instance = %instance,
                                        resolved = g.resolved,
                                        gate_ok = g.gate_ok,
                                        ungradeable,
                                        grade_error = %g.error.as_deref().unwrap_or(""),
                                        attempt,
                                        max_attempts,
                                        "solve completion auto-graded"
                                    );
                                    if let Some(p) = agent_solve_ledger_path(&run_id) {
                                        let gp = p.with_extension("grade.json");
                                        if let Ok(j) = serde_json::to_string_pretty(&g) {
                                            let _ = std::fs::write(gp, j);
                                        }
                                    }
                                    if g.resolved || !g.gate_ok || g.error.is_some() {
                                        break;
                                    }
                                    // Identical-resubmit detection: a receipt comparison,
                                    // never a guess. Only meaningful for a real diff (two
                                    // empty patches hash equal vacuously — the zero-diff
                                    // arm owns that case).
                                    let identical_resubmit = g.patch_bytes > 0
                                        && !patch_sha256.is_empty()
                                        && prev_patch_sha.as_deref() == Some(patch_sha256.as_str());
                                    if identical_resubmit {
                                        crate::probe!(
                                            class = "benchmark.resubmit.identical",
                                            run_id = %run_id,
                                            instance = %instance,
                                            attempt,
                                            patch_sha256 = %patch_sha256,
                                            "attempt resubmitted a BYTE-IDENTICAL patch — \
                                             the previous verdict did not teach (learning-stuck \
                                             signature, on the wire the moment it happens)"
                                        );
                                    }
                                    prev_patch_sha = Some(patch_sha256.clone());
                                    if attempt == max_attempts {
                                        break;
                                    }
                                    // The next chance carries the verdict — what a human
                                    // reviewer would hand back. Built from the BASE task
                                    // each round so feedback never stacks into a scroll.
                                    //
                                    // The contract FORKS on whether the graded attempt
                                    // produced a diff. The investigate-and-fix wording is
                                    // only true when edits exist; handed to a zero-diff
                                    // attempt it re-authorizes another discovery loop
                                    // ("investigate… run them…") — glass-boxed live
                                    // 2026-08-08: three graded attempts, 224 acts, zero
                                    // code/write|code/edit calls, each retry re-entering
                                    // the same read/search spiral. A retry's objective is
                                    // STATE, so the zero-diff arm changes the objective:
                                    // an edit is the only move that earns feedback.
                                    // HELD-OUT honesty (due-diligence find, 2026-08-08): the
                                    // named failing tests come from the grader's fresh clone
                                    // + the instance's held-out test_patch — they DO NOT
                                    // EXIST in her workspace. The old wording ("Failing
                                    // tests: X … run them") was an unfollowable instruction:
                                    // atlas-24066-n5 was told to run test_issue_24062, which
                                    // no grep of her tree can find. Name them as the
                                    // grader's, and point her at the reproduction she CAN
                                    // run — the example in the task's own issue text.
                                    let failing = if g.failed_tests.is_empty() {
                                        String::new()
                                    } else {
                                        format!(
                                            " The grader's held-out tests still failing: {} \
                                             (these are NOT in your workspace — do not search \
                                             for them; reproduce the problem with the example \
                                             from the task description instead, and verify \
                                             your fix against that).",
                                            g.failed_tests.join(", ")
                                        )
                                    };
                                    // The OUTPUT half of the verdict (atlas-sympy-24066-n4,
                                    // 2026-08-08): she rebuilt ~90% of the gold patch and
                                    // missed on one predicate; the verdict named the failing
                                    // test but not what it PRINTED. The assertion diff — the
                                    // leftover `Dimension(impedance*capacitance/time)` — is
                                    // the fact a next attempt reasons from. What a human
                                    // reviewer would paste, so the grader pastes it.
                                    let output = match g.failure_excerpt.as_deref() {
                                        Some(x) if !x.trim().is_empty() => format!(
                                            "\n\nFailing test output (what the test run printed):\n{x}\n"
                                        ),
                                        _ => String::new(),
                                    };
                                    // A retry is a FRESH turn with fresh working memory, so
                                    // "the file you already identified" names knowledge the
                                    // next attempt does not have (glass-boxed 2026-08-08,
                                    // 22840-n4 attempt 2: her recall carried only a generic
                                    // "I worked a coding task" reflection, and she spent 10
                                    // of 12 acts re-deriving cse_main.py). The trail is
                                    // STATE the substrate holds — hand it back explicitly.
                                    // DEAD-ATTEMPT honesty (due-diligence find, 2026-08-08):
                                    // an attempt that died before acting (infra fault, acts
                                    // 0) leaves a junk trail — atlas-24066-n5 attempt 2 was
                                    // told "go straight to the file you already identified.
                                    // Files examined: sympy/physics" after attempt 1 died at
                                    // act 0. The verdict asserted history that never
                                    // happened. A trail only rides when the attempt actually
                                    // worked, and directory fragments are filtered — only
                                    // entries that look like FILES teach.
                                    let attempt_worked = r.acts > 0;
                                    let file_entries: Vec<&String> = r
                                        .files_examined
                                        .iter()
                                        .filter(|p| {
                                            p.rsplit('/').next().is_some_and(|s| s.contains('.'))
                                        })
                                        .collect();
                                    let trail = if !attempt_worked || file_entries.is_empty() {
                                        String::new()
                                    } else {
                                        format!(
                                            " Files your previous attempt examined (in order): {}.",
                                            file_entries
                                                .iter()
                                                .map(|s| s.as_str())
                                                .collect::<Vec<_>>()
                                                .join(", ")
                                        )
                                    };
                                    next_task = if g.patch_bytes == 0 {
                                        // "the file you already identified" is only true when
                                        // the trail actually names one; a dead or fileless
                                        // attempt gets a fresh-start objective instead of a
                                        // reference to history that never happened.
                                        let go = if trail.is_empty() {
                                            "Find the file at fault and apply your best-guess fix"
                                        } else {
                                            "Go straight to the file you already identified and apply your best-guess fix"
                                        };
                                        format!(
                                            "{base_task}\n\n[grader verdict — attempt {attempt} of {max_attempts} produced NO EDIT] \
                                             You changed no files, so the grader had nothing to run.{failing}{trail} \
                                             Reading and searching cannot score; only an edit can. This attempt: {go} \
                                             with code/edit — a wrong edit earns failing-test feedback to iterate on; \
                                             no edit earns nothing.{output}",
                                        )
                                    } else {
                                        let edited = if r.files_changed.is_empty() {
                                            String::new()
                                        } else {
                                            format!(
                                                " Your edits are in: {}.",
                                                r.files_changed.join(", ")
                                            )
                                        };
                                        // The resubmit fact LEADS the contract when it fired:
                                        // round D proved a verdict buried mid-prose does not
                                        // alter resubmission behavior. This is the receipt
                                        // (sha equality) speaking, and it names the ONLY
                                        // moves that can change the next verdict.
                                        let resubmit = if identical_resubmit {
                                            format!(
                                                " STOP AND READ: attempt {attempt}'s patch was \
                                                 BYTE-IDENTICAL to attempt {}'s (verified by \
                                                 hash). The grader ran the exact same diff and \
                                                 returned the exact same failure. Submitting it \
                                                 a third time cannot change anything. Before any \
                                                 other work: run `git diff HEAD` to SEE your \
                                                 current patch, then either fix the part the \
                                                 failing tests name, or revert it \
                                                 (`git checkout -- <file>`) and take a different \
                                                 approach.",
                                                attempt - 1
                                            )
                                        } else {
                                            String::new()
                                        };
                                        format!(
                                            "{base_task}\n\n[grader verdict — attempt {attempt} of {max_attempts} did not resolve]{resubmit} \
                                             FAIL_TO_PASS {}/{}, PASS_TO_PASS {}/{}.{failing} \
                                             Your previous edits are still in this workspace.{edited}{trail} \
                                             Reproduce the problem with the task's own example, fix in place, and \
                                             verify against that example without breaking what passes.{output}",
                                            g.fail_to_pass_passed,
                                            g.fail_to_pass_total,
                                            g.pass_to_pass_passed,
                                            g.pass_to_pass_total,
                                        )
                                    };
                                }
                                Err(e) => {
                                    // A failed GRADE is not a failed solve — surface it
                                    // loud on its own channel; the solve result stands.
                                    crate::probe!(
                                        class = "benchmark.autograde",
                                        run_id = %run_id,
                                        instance = %instance,
                                        error = %e.to_string(),
                                        attempt,
                                        "auto-grade FAILED — solve result stands, verdict missing"
                                    );
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            // Fail LOUD on the poll surface too — a detached run that dies must leave a
                            // diagnosable marker, never an empty file forever.
                            if let Some(path) = path.as_ref() {
                                let _ = std::fs::write(
                                    path,
                                    serde_json::json!({"failed": true, "run_id": run_id, "error": e.to_string()})
                                        .to_string(),
                                );
                            }
                            tracing::error!(run_id = %run_id, error = %e, attempt, "agent/solve detached run failed");
                            break;
                        }
                    }
                    // Manual increment (#384): only a GENUINE attempt outcome advances
                    // the counter — the infra-void arm `continue`s above this line.
                    attempt += 1;
                }
            });
            return Ok(AgentSolveResult {
                persona_id: persona_ack,
                model: model_ack,
                acts: 0,
                spoken: String::new(),
                patch: String::new(),
                files_changed: Vec::new(),
                files_examined: Vec::new(),
                detached: true,
                run_id: Some(run_id_ack),
                infra_error: None,
            });
        }
        Self::solve_body(p).await
    }
}

/// Result file for a detached solve run, polled after the ack (mirrors the eval/competition
/// progress-ledger convention: `~/.continuum/progress/agent-solve-<run_id>.json`).
///
/// Both the directory and the file name come from `cognition::swe_bench`, which is also where
/// the boot reaper and the reboot guard READ them. They were spelled out independently here
/// until 2026-08-18 and the names did not match, so neither reader ever saw a run this writer
/// produced ([`crate::cognition::swe_bench::SOLVE_LEDGER_PREFIX`] carries the measurement).
fn agent_solve_ledger_path(run_id: &str) -> Option<std::path::PathBuf> {
    let dir = crate::cognition::swe_bench::solve_ledger_dir();
    let _ = std::fs::create_dir_all(&dir);
    Some(crate::cognition::swe_bench::solve_ledger_path(&dir, run_id))
}

/// Where THIS run's artifacts live — the patch above all. One definition, so a run's
/// evidence never depends on how it happened to be launched.
///
/// `capture_dir` is an OPTIONAL caller courtesy (a hand-launched run naming its own
/// folder). It was also, until 2026-08-18, the ONLY thing standing between an attempt
/// and total evidence loss: the patch write sat behind `if let Some(dir) =
/// capture_dir` with no else, so every run that did not pass one silently discarded
/// the diff. Measured that day: all 25 patches on this box live under
/// `benchmarks/swe/captures/run-*` — hand-launched runs. Every CITIZEN-dispatched run
/// (`claim-*`, i.e. the entire path the benchmark actually runs on) kept none. A
/// citizen wrote 41,166 bytes against sympy-13480, broke 40 previously-passing tests,
/// and the one artifact that could say whether that was a surgical edit or a clobber
/// was gone before anyone could read it.
///
/// So custody stops being a parameter. Absent an explicit dir, it derives from
/// [`swe_cache_dir`] — the benchmarks root, whose own doc says to read it from there and
/// never from a remembered path.
///
/// It lands in `benchmarks/swe/captures/run-<id>/`, which is EXACTLY where the 25
/// hand-launched patches already live. That is deliberate on two counts, and my first cut
/// got both wrong by inventing `progress/run-<id>/` instead (caught by Joel the same
/// hour):
///
/// 1. **One home per artifact class.** A second location for "her patch" is the parallel
///    allocator this codebase keeps paying for — the exact sin I had written down that
///    morning and then committed.
/// 2. **It must be a GOVERNED directory.** `benchmarks` is a registered `TrackedDir` with
///    a decided eviction story; `progress` is neither tracked nor decided, so patches
///    there would have been unbounded growth in an unmanaged dir — precisely what
///    CLAUDE.md's "no new cache dir without an eviction decision" rule exists to stop
///    (the 460 GB incident).
fn run_artifact_dir(run_id: &str, capture_dir: Option<&str>) -> Option<std::path::PathBuf> {
    if let Some(d) = capture_dir {
        return Some(std::path::PathBuf::from(d));
    }
    Some(
        crate::cognition::swe_bench::swe_cache_dir()
            .join("captures")
            .join(format!("run-{run_id}")),
    )
}

/// Global admission gate for scored solve DRIVES — the fix for the lane-thrash death
/// (glass-boxed 2026-08-11, build 4627): with solves finally firing (dispatch auto-fire +
/// claim + durable-restore all launch `dispatch_staged_swe_solve`), MORE solves than the
/// box has serving lanes ran their generation phase at once and thrashed one llama-server
/// lane to a Connection-refused death mid-solve ("6 mid-relaunch retries exhausted — lane
/// never came back"). This caps concurrent solve drives at the live serving-lane budget:
/// the (lanes+1)th solve WAITS for a permit instead of oversubscribing and killing the
/// lane. The permit is held for the whole drive and released on drop — panic-safe, so a
/// stalled or panicking solve can never leak a slot. Sized ONCE at first use from the live
/// lane count (min 1); a later lane shrink is a v1 mismatch (rare, and quiesce_others still
/// protects the KV prefix). One place, both triggers respect it, because every solve —
/// inline or detached, dispatch or claim — flows through `solve_body`.
/// [[measured-work-gets-an-exclusive-warm-slot-quiesce-others]]
fn solve_admission() -> &'static tokio::sync::Semaphore {
    static SLOTS: std::sync::OnceLock<tokio::sync::Semaphore> = std::sync::OnceLock::new();
    SLOTS.get_or_init(|| {
        let lanes = crate::inference::llama_server::current_serving()
            .lanes
            .max(1) as usize;
        tokio::sync::Semaphore::new(lanes)
    })
}

impl AgentSolve {
    /// The solve body — deliberately ctx-free (reaches the persona via the global workspace
    /// registry), so it runs inline OR spawned detached with the same code path.
    pub(crate) async fn solve_body(p: AgentSolveParams) -> Result<AgentSolveResult, CommandError> {
        let run_id = p.run_id.clone();
        // Short-form persona ids resolve too (#164): rosters/benchmark harnesses DISPLAY
        // 8-char short ids, so accept the id a caller was shown — a clean UUID passes
        // straight through, a short/mistyped form expands against the live persona registry
        // (the ONE shared id_resolve primitive), instead of failing "is not a UUID".
        let persona_uuid = crate::id_resolve::resolve(
            p.persona_id.as_str().trim(),
            &crate::persona::card::ids(),
            "persona",
        )
        .map_err(CommandError::Invalid)?;
        let workspace = p.workspace.trim().to_string();
        if !std::path::Path::new(&workspace).is_dir() {
            return Err(CommandError::Invalid(format!(
                "workspace '{workspace}' does not exist or is not a directory"
            )));
        }
        let max_acts = p.max_acts.unwrap_or(DEFAULT_MAX_ACTS).max(1) as usize;

        // 1) Stand up a dedicated measurement lane for the model (her genome pages in on top),
        //    exactly as cognition/eval does — held for the whole drive, dropped after.
        //
        //    BOUNDED, loudly (glass-boxed 2026-08-08, n8/n11: both forks sat 2h+ with
        //    ZERO generations, parked somewhere inside lane acquisition — three
        //    candidate parks (warm-pool spawn gate held by a wedged cold-load; the
        //    share-check's adapter.initialize() HTTP round-trip against a saturated
        //    lane, whose sibling endpoint is DOCUMENTED to block mid-generation;
        //    pressure defer) and no receipt discriminated them because the whole
        //    acquisition was one silent await. The timeout converts any park into a
        //    loud named error; the bracket probes make the NEXT stall name its line.
        // Admission gate (held for the whole drive, released on drop): WAIT for one of the
        // serving-lane solve slots before acquiring/using a lane, so a dispatch fan-out +
        // claim + restore can never run more solves than the box has lanes and thrash one to
        // a Connection-refused death mid-generation. `.ok()` because the semaphore is never
        // closed; binding to `_solve_permit` keeps the permit alive across the drive.
        let _solve_permit = solve_admission().acquire().await.ok();
        crate::probe!(
            class = "benchmark.solve.phase",
            run_id = %run_id.as_deref().unwrap_or("-"),
            phase = "admission.acquired",
            available_slots = solve_admission().available_permits() as u64,
            "solve admitted — holding one serving-lane solve slot for the drive"
        );
        // 1a) Quiesce BEFORE the lane, and let the plan RESETTLE before anything
        //     pins it (glass-boxed 2026-08-11, Atlas on sympy-24152). The old order
        //     was lane-then-quiesce, which built a catch-22: the lease's lowered
        //     lane demand (#2238/#2239) would let the planner collapse a crowded
        //     4-lane × 16k layout into one big-window lane — but by then the solve
        //     already held the lane, and the reconcile (correctly) refuses to
        //     relaunch under a live measurement ("eval holds the lane steady"). The
        //     solve froze the cramped layout under itself, and the persona worked a
        //     SWE repo through a ~400-token keyhole: 13.1k of her 16.4k window was
        //     fixed overhead (tools 6300 + framing 2741 + completion reserve 4096),
        //     over-window on every single compose. Quiesce-first + an EVENT-GATED
        //     settle (the daemon's own snapshot watch — never a sleep-poll) means
        //     the lane below is acquired against the settled big-window plan, and
        //     `lane.served_ctx` carries it into her fork with no further plumbing.
        //     Bounded loudly: a planner that keeps the layout is a legitimate
        //     outcome, so `Unchanged` proceeds — it can never park the solve.
        let _quiesce_lease =
            crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global().map(
                |reg| {
                    let lease = reg.quiesce_others(persona_uuid);
                    crate::probe!(
                        class = "benchmark.solve.phase",
                        run_id = %run_id.as_deref().unwrap_or("-"),
                        phase = "quiesce_others",
                        quiesced_peers = lease.count() as u64,
                        "measured solve holds an exclusive warm slot — idle citizens quiesced so the KV prefix survives turn-to-turn"
                    );
                    lease
                },
            );
        if let Some(rx) = crate::inference::llama_server::serving_state_receiver() {
            let pre = crate::inference::llama_server::current_serving();
            // Wide enough for one planner tick + a resident-model relaunch (weights
            // already on disk); a settle that needs longer is a serving fault the
            // lane-acquire timeout below will name, not something to wait out here.
            const SERVING_RESETTLE_BOUND: Duration = Duration::from_secs(180);
            let outcome = crate::inference::llama_server::await_snapshot_resettle(
                rx,
                pre.lanes,
                pre.served_context_window,
                SERVING_RESETTLE_BOUND,
            )
            .await;
            match outcome {
                crate::inference::llama_server::SnapshotSettle::Resettled { lanes, window } => {
                    crate::probe!(
                        class = "benchmark.solve.phase",
                        run_id = %run_id.as_deref().unwrap_or("-"),
                        phase = "serving_resettled",
                        pre_lanes = pre.lanes as u64,
                        pre_window = pre.served_context_window as u64,
                        lanes = lanes as u64,
                        window = window as u64,
                        "quiesce lowered demand and the plan RESETTLED — the solve \
                         drives on the settled layout instead of freezing the crowded one"
                    );
                }
                crate::inference::llama_server::SnapshotSettle::Unchanged => {
                    crate::probe!(
                        class = "benchmark.solve.phase",
                        run_id = %run_id.as_deref().unwrap_or("-"),
                        phase = "serving_unchanged",
                        lanes = pre.lanes as u64,
                        window = pre.served_context_window as u64,
                        "plan held its layout under lowered demand — proceeding on the \
                         current lanes/window (legitimate: it may already be optimal)"
                    );
                }
            }
        }
        crate::probe!(
            class = "benchmark.solve.phase",
            run_id = %run_id.as_deref().unwrap_or("-"),
            phase = "lane_acquire.start",
            base_model = %p.base_model_id,
            "solve prelude: acquiring measurement lane"
        );
        const LANE_ACQUIRE_TIMEOUT: Duration = Duration::from_secs(15 * 60);
        let lane = match tokio::time::timeout(
            LANE_ACQUIRE_TIMEOUT,
            crate::cognition::eval::spawn_base_eval_lane(&p.base_model_id),
        )
        .await
        {
            Ok(lane) => lane?,
            Err(_) => {
                crate::probe!(
                    class = "benchmark.solve.phase",
                    run_id = %run_id.as_deref().unwrap_or("-"),
                    phase = "lane_acquire.timeout",
                    base_model = %p.base_model_id,
                    "lane acquisition exceeded its bound — INFRA fault, run ends loudly"
                );
                return Err(CommandError::Internal(format!(
                    "measurement-lane acquisition for '{}' exceeded {}s — an infra \
                     stall (spawn gate, share-check HTTP, or pressure defer), never a \
                     capability verdict. See eval.lane.* / benchmark.solve.phase probes \
                     for the parked step.",
                    p.base_model_id,
                    LANE_ACQUIRE_TIMEOUT.as_secs()
                )));
            }
        };
        crate::probe!(
            class = "benchmark.solve.phase",
            run_id = %run_id.as_deref().unwrap_or("-"),
            phase = "lane_acquire.done",
            "solve prelude: lane acquired"
        );

        // (The exclusive-warm-slot quiesce lease — KV-prefix protection, panic-safe
        // RAII, she is never suspended, only idle contenders' autonomic ticks —
        // is acquired in step 1a ABOVE the lane, so its lowered demand shapes the
        // plan the lane is acquired against. [[benchmark-is-a-governor-preemption-lease]]
        // [[first-class-citizens-even-during-benchmarks]])

        // 2) Fork her WHOLE cognition onto that lane, rooted at the workspace: tools ON, recall ON.
        //    A brief wait covers the post-spawn template race (same as the eval fork-waiter).
        let registry = crate::cognition::persona_workspace::global();
        // The MISSION rides as standing framing, not as the opening burst alone (#390,
        // glass-boxed 2026-08-12 on pytest-5221): the task text delivered once was evicted
        // from every captured prompt after act ~6 of a 24-act solve, and the persona
        // literally asked "could you describe the symptoms of the issue?" — anchor loss,
        // the dominant patch-0 shape. A `[mission]` StandingFraming block survives every
        // compose of the drive, exactly like the pinned board (#347). The burst below
        // still fires — it is the directed TRIGGER; this is the PERSISTENCE.
        let mission = std::sync::Arc::new(crate::persona::mission_source::MissionSource::new(
            persona_uuid,
            format!(
                "YOUR ONE JOB this whole session (re-read this every step):\n{}\n\nWork in \
                 `{workspace}` — that directory IS the task's repo. The deliverable is the \
                 edit your tools leave there; a session that only reads has failed.",
                p.task.trim()
            ),
        ));
        let mut cycle = None;
        for attempt in 0..FORK_WAIT_TRIES {
            cycle = registry.fork_eval_cycle_with_adapter(
                &persona_uuid,
                lane.adapter.clone(),
                lane.served_ctx,
                true,                               // with_tools — her hands are ON
                Some(&workspace),                   // roots the ToolExecutor at the sandbox cwd
                p.suppress_recall.unwrap_or(false), // memory/RAG ON by default; the diagnostic knob
                vec![
                    crate::cognition::persona_workspace::GroundingSource::framing(mission.clone()),
                ],
            );
            if cycle.is_some() {
                break;
            }
            if attempt + 1 < FORK_WAIT_TRIES {
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
        }
        let cycle = cycle.ok_or_else(|| {
            CommandError::NotFound(format!(
                "no workspace template for persona {persona_uuid} — spawn her (register_from_cfg) \
                 before agent/solve can fork a measurement copy of her mind"
            ))
        })?;

        // 2b) Root her HANDS at the sandbox cwd. `fork_eval_cycle_with_adapter`'s `workspace_root`
        //     only repoints what she SEES (the workspace-map RAG block) — her `ToolExecutor` still
        //     writes to her durable per-persona workspace. Without this she writes reverse.py to
        //     `<home>/citizens/peers/<id>/workspace/` and our `git diff` on the sandbox scores a
        //     false ZERO (glass-boxed 2026-07-22: Devstral did 2 real acts, wrote the correct file,
        //     patch was empty). Same fail-loud mechanism cognition/eval uses to root a measurement
        //     persona at a target repo.
        //
        //     The re-root is PROCESS-GLOBAL and outlives this fork (`code/create-workspace`
        //     keys the file engine on the caller identity, and the fork shares the living
        //     persona's executor AND her id — see `ActingHands`). So the hands handle is
        //     lifted out BEFORE the cycle is consumed, and every exit path below returns her
        //     to her own workspace. Without that, #312: after a flask solve, Anwen's LIVE
        //     self was still running `code/read(src/flask/app.py)` in her room hours later.
        let hands = crate::cognition::persona_workspace::ActingHands::of(&cycle);
        crate::cognition::persona_workspace::root_acting_workspace(
            &cycle,
            &workspace,
            p.path_prepend.as_deref().unwrap_or(&[]),
            p.scored.unwrap_or(false),
        )
        .await?;

        // Everything the ROOTED hands touch lives in this one fallible region, so the
        // restore below runs on Ok AND on Err. A `?` added anywhere inside stays covered.
        let outcome = async {
            // GLASS-BOX (same seam as cognition/eval, task #14): opt-in JSONL turn capture on
            // the fork — bids + DECISION + timings per tick, the instrument that turns an
            // acts=1 silent settle from a mystery into a mechanism.
            let cycle = match &p.capture_dir {
                Some(dir) => cycle.with_capture(std::sync::Arc::new(
                    crate::cognition::workspace_capture::JsonlWorkspaceCaptureSink::open(
                        std::path::Path::new(dir),
                        persona_uuid,
                    )
                    .map_err(|e| {
                        CommandError::Internal(format!(
                            "failed to open agent/solve capture_dir '{dir}': {e}"
                        ))
                    })?,
                )),
                None => cycle,
            };

            // 3) Layer the task into her situation as a directed, TOOL-FORCING request. The dominant
            //    misfit-coder failure (glass-boxed 2026-07-22): a 7B answers with the code in a message
            //    ("here's reverse.py: ```…```" / "I saved it to reverse.py") instead of CALLING the
            //    write tool → the graded artifact (the git patch) is empty. The old "Provide your
            //    complete solution" framing literally invited that own-goal. This is the standard SWE /
            //    Terminal-Bench harness contract: the deliverable is what her TOOLS put in the
            //    workspace; narrating it does not perform it. Meeting the misfit where it is — an
            //    ergonomic/adapter fix ([[use-adapters-dont-dumb-it-down]]), not a capability demand —
            //    and honest (it states the real I/O contract; it does not hand her the answer). Then
            //    DRIVE her to settlement (read → edit → run → fix, her real act→observe loop).
            // The run's ROOM (see `AgentSolveParams::room`). `Uuid::nil()` is the
            // honest roomless fallback for a bare `agent/solve` with no activity
            // behind it; a DISPATCHED run always carries one, and that is what
            // turns her acts into room receipts instead of invisible work.
            let visibility = RunVisibility::resolve(p.room);
            if let Some(why) = visibility.warning() {
                // Announce ONCE, at the one place the branch is taken. This is the whole
                // fix for #425's remaining half: the roomless state was never wrong, it was
                // never SAID, and 13,209 turns went by in it.
                crate::probe!(
                    class = "agent.solve.roomless",
                    run_id = %p.run_id.clone().unwrap_or_default(),
                    persona_id = %p.persona_id,
                    "solve run is INVISIBLE — no room, so no receipts and no room turn (#425)",
                );
                tracing::warn!(run_id = ?p.run_id, "agent/solve: {why}");
            }
            let room = visibility.room_id();
            // The workspace-grounding sentence counters the observed "new project ritual"
            // (glass-boxed 2026-07-22 via turn capture: her first act on a seeded task was
            // code/create-workspace("my_stack_project") + a Rust hello-world + git/commit —
            // her habitual onboarding sequence replaying from memory — which re-roots her
            // hands OFF the graded tree, then she passes to a silent settle). Honest
            // contract language, same class as the tool-forcing framing: it states where
            // the work IS, it does not hand her the answer or gate her tools.
            // The wrapper states the I/O CONTRACT (only tool calls take effect) and nothing about
            // the SHAPE of the deliverable — because the TASK owns that, and the two used to
            // contradict each other outright.
            //
            // The old text said "writing files with code/write" and "graded on the files your tools
            // WRITE". That was written for from-scratch build gyms, where new files ARE the
            // deliverable. Nested beneath it, the SWE task text says the opposite: "do not add new
            // top-level files … find the existing source of the fault and edit it in place." That
            // text is the dispatch CARD BODY (`benchmark::BenchmarkSweSetup`) — the card IS the
            // task, so the card owns the deliverable shape.
            //
            // Outer contract first, inner constraint buried under "Task:" — and she obeyed the
            // outer one. Three consecutive sympy-21379 runs, all full-effort, all writing NEW files
            // and never editing the library:
            //   v3  8 acts → reproduce_piecewise_error.py
            //   v4 30 acts → reproduce_bug.py, test_sympy_error.py, test_sympy_issue.py
            //   v5 18 acts → reproduce_error.py, test_sympy_error.py
            // I read that as a judgement gap for a whole session. It was two halves of my own
            // framing disagreeing about what the deliverable IS.
            //
            // Now: "as your tools leave it" covers an edit and a new file equally, and `code/edit`
            // joins the exemplar verbs so the anti-narration force survives without smuggling in a
            // deliverable shape. Steering nothing — the task still says what to build or fix.
            let framed = frame_task(&p.task);
            let task_delivery = crate::persona::rag_budget::RagDelivery {
                source_id: "airc".to_string(),
                items: vec![crate::persona::rag_budget::RagItem {
                    content: framed,
                    tokens: 0,
                    metadata: serde_json::json!({
                        "peer_id": "peer",
                        "occurred_at_ms": crate::persona::trace::now_ms(),
                    }),
                }],
                tokens_used: 0,
                continuation: None,
                resolution_used: crate::persona::rag_budget::ResolutionPreference::Raw,
            };
            let burst = crate::cognition::workspace::Burst::from_turns(
                room,
                crate::persona::service_loop::build_workspace_turns(
                    std::slice::from_ref(&task_delivery),
                    "",
                    "",
                    None,
                ),
            );
            let workspace_deliverable =
                matches!(p.deliverable.unwrap_or_default(), Deliverable::Workspace);
            let framing = {
                let f = crate::cognition::workspace::TurnFraming::directed();
                if workspace_deliverable {
                    f.on_workspace()
                } else {
                    f
                }
            };
            // THE RUN PULSES WHILE IT RUNS (#371 law 2: liveness is a pulse, never a
            // terminal artifact).
            //
            // The ledger used to be written ONCE per attempt, at settlement. So for the
            // entire attempt — legitimately HOURS on a full SWE budget — `benchmark/runs`
            // read `acts: 0` and a `last_activity` frozen at run start. Against a 20-minute
            // stall window that means a HEALTHY first attempt is guaranteed to read `quiet`,
            // every time, and the projection whose stated purpose is "silence must never be
            // ambiguous with progress" was structurally unable to tell them apart. Measured
            // 2026-08-16: two dispatched solves read `acts=0, stalled=false` for ten straight
            // minutes, and the driver watching them could not distinguish working from wedged
            // — which is exactly how a vacuous "no faults" gets reported as a green.
            //
            // `select!` over the drive future and an interval: no spawn, so the cycle stays
            // BORROWED (no 'static bound, no Arc juggling, no parallel allocator). Each tick
            // reads the persona's own monotonic act counter — a wait-free atomic load — and
            // rewrites the running marker, which moves BOTH `acts` and the file mtime that
            // `last_activity_ms` folds from. The counter is the same one perception renders,
            // so the board and her own proprioception can never disagree.
            //
            // Cadence: well under RUN_STALL_WINDOW_SECS so a live run can never age into
            // `quiet`, and far above act cadence (~2-6 min) so it costs a tiny JSON write
            // per tick and nothing else.
            const RUN_PULSE: std::time::Duration = std::time::Duration::from_secs(60);
            // Same ledger the detached wrapper journals `state: running` into, and the
            // SAME derivation of every field, so a pulse can never contradict the marker
            // it refreshes. `None` run_id (an attached call) → no ledger → no pulse, which
            // is correct: nothing is polling a run that returns inline.
            let pulse_run_id = p.run_id.clone().unwrap_or_default();
            let pulse_path = p
                .run_id
                .as_deref()
                .and_then(agent_solve_ledger_path);
            let pulse_persona = p.persona_id.clone();
            let pulse_instance: Option<String> = workspace
                .contains("/workspace/swe/")
                .then(|| {
                    std::path::Path::new(&workspace)
                        .file_name()
                        .map(|s| s.to_string_lossy().to_string())
                })
                .flatten();
            let mut settled = {
                let drive = crate::cognition::act_observe::drive_to_settle(
                    &cycle, burst, room, max_acts, framing,
                );
                tokio::pin!(drive);
                let mut ticker = tokio::time::interval(RUN_PULSE);
                ticker.tick().await; // interval fires immediately; consume that tick
                loop {
                    tokio::select! {
                        outcome = &mut drive => break outcome,
                        _ = ticker.tick() => {
                            // COGNITION PULSE: a running solve IS her working, and the
                            // claim-renewal gate must be able to see it.
                            //
                            // `cognition_pulse::touch` had exactly two callers — the
                            // service loop's airc turn start, and (as of the sibling fix)
                            // spawn. A detached solve is NEITHER: it drives acts for hours
                            // without producing a single airc turn (#425), so the renewal
                            // gate read a genuinely-working citizen as silent, denied her
                            // renewals, and let the 30-minute lease lapse UNDER a live run.
                            // The card then returned to claimable and was re-claimed —
                            // spawning a fresh run that discarded the first one's work.
                            // Observed live 2026-08-21 on pallets__flask-4045: the same
                            // citizen re-claimed the same card three times in one evening,
                            // each new run starting at `acts: 0`.
                            //
                            // The tick IS the proof, which is why this is unconditional:
                            // `select!` only reaches this arm while the drive future is
                            // still pending, so a tick firing means the drive is live this
                            // instant. That is a STRONGER witness than the act counter
                            // below (which can legitimately read 0 or None early in a run,
                            // and would then deny a renewal to a citizen mid-first-act).
                            // The gate's policy is untouched — it simply stops being blind
                            // to the one path where the hardest work happens.
                            crate::persona::cognition_pulse::touch(
                                persona_uuid,
                                std::time::SystemTime::now()
                                    .duration_since(std::time::UNIX_EPOCH)
                                    .map(|d| d.as_millis() as u64)
                                    .unwrap_or_default(),
                            );
                            // Best-effort by construction: a failed pulse must never
                            // disturb the work it is only reporting on.
                            if let (Some(p), Some(acts)) =
                                (pulse_path.as_ref(), cycle.actions_taken())
                            {
                                let _ = std::fs::write(p, serde_json::json!({
                                    "state": "running",
                                    "run_id": pulse_run_id,
                                    "persona_id": pulse_persona,
                                    "workspace": workspace,
                                    "instance": pulse_instance,
                                    // Acts SHE has executed, live — not a count that
                                    // materializes only once the work is already over.
                                    "acts": acts,
                                }).to_string());
                            }
                        }
                    }
                }
            };

            // 4) Collect the HANDS artifact: everything she changed in the workspace as a unified diff
            //    (new files included), plus the touched paths. This is what SWE/Terminal-Bench apply.
            let (mut patch, mut files_changed) = workspace_patch(&workspace).await;

            // EMPTY-DIFF RE-DRIVE — the two-gates doctrine made mechanism (glass-boxed
            // 2026-08-08, atlas-sympy-24066-n6 attempts 2+3): on a Workspace-deliverable
            // task she settled by SPEAKING after ONE act — a generic file summary, zero
            // edits — leaving 11 of 12 acts unused, twice, near-verbatim. Working is not
            // speaking: when the deliverable is the workspace diff, an attempt ending with
            // an EMPTY diff and real remaining budget must not end silently. ONE bounded
            // re-drive (a retry, never a nag loop): state the structural fact, hand back
            // the remaining budget. If she ends on an empty diff again, THAT settles —
            // honestly graded, with the fact on the record.
            //
            // This fires on ANY non-infra end with budget remaining — a Speak, the #206
            // stuck backstop, or the #390 discovery-saturation gate (which deliberately
            // ends the drive EARLY, at half budget, precisely so this re-drive still has
            // budget to hand back; see `drive_to_settle`). It used to require
            // `spoken.is_some()`, which structurally excluded the gated endings — the one
            // population that most needs the redirect. TRUE budget exhaustion is still
            // excluded by `acts + 1 < max_acts` (nothing left to hand back), and infra
            // failures by `inference_error` — those grade honestly as before.
            if workspace_deliverable
                && patch.is_empty()
                && settled.inference_error.is_none()
                && settled.acts + 1 < max_acts
            {
                let remaining = max_acts - settled.acts;
                crate::probe!(
                    class = "benchmark.empty_diff_redrive",
                    run_id = %run_id.as_deref().unwrap_or("-"),
                    acts_used = settled.acts,
                    acts_remaining = remaining,
                    "workspace-deliverable attempt ended with an EMPTY diff and remaining \
                     act budget (Speak, stuck backstop, or #390 saturation gate) — one \
                     bounded re-drive with the structural fact"
                );
                let fact = format!(
                    "Status check from the grading harness (a structural fact, not a person): \
                 your workspace diff is EMPTY — no file here differs from where you \
                 started, so as of now there is NOTHING to grade. Speaking does not \
                 submit work: this task is graded ONLY on the changes your tools make \
                 to the files in this workspace. You have {remaining} actions left. \
                 Use them now: reproduce the problem with the example in the task \
                 description, find the faulty code, and change it in place with \
                 code/edit."
                );
                (patch, files_changed) = redrive_with_fact(
                    &cycle,
                    room,
                    framing,
                    remaining,
                    fact,
                    &mut settled,
                    &workspace,
                )
                .await;
            }

            // IDENTICAL-DIFF RE-DRIVE — the empty-diff block's sibling (round E
            // sha receipts, 2026-08-08: BOTH citizens settled attempt 3 with a
            // patch byte-identical to the attempt-2 patch that had just failed —
            // Atlas c4dbfba9…×2, Benchy 531a03d2…×2 — and the post-grade detector
            // could only address an attempt 4 that never exists). Same patch ⇒
            // same verdict, deterministically: settling on it re-buys a failure.
            // ONE bounded re-drive with the hash-proven fact, at the only moment
            // it can still change the attempt's outcome. If she settles identical
            // AGAIN, that grades honestly — fact on the record, never a nag loop.
            if workspace_deliverable
                && !patch.is_empty()
                && settled.inference_error.is_none()
                && settled.spoken.is_some()
                && settled.acts + 1 < max_acts
            {
                let sha = {
                    use sha2::{Digest, Sha256};
                    format!("{:x}", Sha256::digest(patch.as_bytes()))
                };
                if p.prev_failed_patch_sha.as_deref() == Some(sha.as_str()) {
                    let remaining = max_acts - settled.acts;
                    crate::probe!(
                        class = "benchmark.identical_diff_redrive",
                        run_id = %run_id.as_deref().unwrap_or("-"),
                        patch_sha256 = %sha,
                        acts_remaining = remaining,
                        "settle produced a patch BYTE-IDENTICAL to the previous failed \
                         attempt's — one bounded re-drive with the hash-proven fact, \
                         before a redundant grade burns the attempt"
                    );
                    let fact = format!(
                        "Status check from the grading harness (a structural fact, not a \
                     person): your workspace diff right now is BYTE-IDENTICAL to the \
                     patch that was already graded and FAILED on the previous attempt \
                     (verified by hash). Submitting it again will produce the exact \
                     same failure. You have {remaining} actions left. First run \
                     `git diff HEAD` with code/shell to SEE your current patch. Then \
                     either fix the specific part the failing tests named, or revert \
                     it (`git checkout -- <file>`) and take a genuinely different \
                     approach. Do not settle until the diff has changed."
                    );
                    (patch, files_changed) = redrive_with_fact(
                        &cycle,
                        room,
                        framing,
                        remaining,
                        fact,
                        &mut settled,
                        &workspace,
                    )
                    .await;
                }
            }

            // IN-LOOP TEST VERIFIER — the structural gap between this exam room and the
            // field harnesses that pass with the SAME model (scoreboard 2026-08-09: six
            // rounds, zero resolves; field agents iterate against real test output every
            // few edits, our citizens got one verdict per attempt and settled hopeful —
            // producing the signature "on-target, harmless, doesn't fix" patch). When a
            // workspace-deliverable settle carries a non-empty diff, run the REPO'S OWN
            // tests for the files she touched (the held-out FAIL_TO_PASS stays held out —
            // this is the regression half of feedback, the same loop a field harness
            // closes) and on failure re-drive with the ACTUAL test output. Bounded at
            // VERIFIER_ROUNDS; green tests, an unchanged diff, no test mapping, or an env
            // fault all end the loop (loudly, never silently).
            const VERIFIER_ROUNDS: usize = 3;
            let mut verifier_round = 0usize;
            let mut last_verified_sha = String::new();
            while workspace_deliverable
                && verifier_round < VERIFIER_ROUNDS
                && !patch.is_empty()
                && settled.inference_error.is_none()
                && settled.acts + 1 < max_acts
            {
                let sha = {
                    use sha2::{Digest, Sha256};
                    format!("{:x}", Sha256::digest(patch.as_bytes()))
                };
                if sha == last_verified_sha {
                    break; // re-drive produced no new diff — nothing new to verify
                }
                let tests = mapped_test_files(&workspace, &files_changed);
                if tests.is_empty() {
                    crate::probe!(
                        class = "benchmark.verifier.no_mapping",
                        run_id = %run_id.as_deref().unwrap_or("-"),
                        files = %files_changed.join(","),
                        "in-loop verifier found no test files for the touched paths — \
                         settle stands unverified"
                    );
                    break;
                }
                let py = p
                    .path_prepend
                    .as_ref()
                    .and_then(|v| v.first())
                    .map(|bin| format!("{bin}/python"))
                    .filter(|py| std::path::Path::new(py).exists())
                    .unwrap_or_else(|| "python3".to_string());
                let mut args: Vec<&str> = vec!["-m", "pytest"];
                for t in &tests {
                    args.push(t);
                }
                args.extend(["-q", "--no-header", "-p", "no:cacheprovider"]);
                match crate::cognition::swe_bench::run(
                    &py,
                    &args,
                    Some(std::path::Path::new(&workspace)),
                )
                .await
                {
                    Ok(out) if out.status.success() => {
                        crate::probe!(
                            class = "benchmark.verifier.green",
                            run_id = %run_id.as_deref().unwrap_or("-"),
                            tests = %tests.join(","),
                            round = verifier_round,
                            "in-loop verifier: touched-file tests PASS — settle stands"
                        );
                        break;
                    }
                    Ok(out) => {
                        verifier_round += 1;
                        last_verified_sha = sha;
                        let report = format!(
                            "{}{}",
                            String::from_utf8_lossy(&out.stdout),
                            String::from_utf8_lossy(&out.stderr)
                        );
                        let tail: String = report
                            .chars()
                            .rev()
                            .take(2000)
                            .collect::<String>()
                            .chars()
                            .rev()
                            .collect();
                        crate::probe!(
                            class = "benchmark.verifier.fail",
                            run_id = %run_id.as_deref().unwrap_or("-"),
                            tests = %tests.join(","),
                            round = verifier_round,
                            "in-loop verifier: touched-file tests FAIL — re-driving with \
                             the real output"
                        );
                        let remaining = max_acts - settled.acts;
                        let fact = format!(
                            "Status check from the grading harness (a structural fact, not a \
                         person): I ran the repo's own tests for the files you changed \
                         ({}) and they FAIL with your current edits. Test output:\n{}\n\
                         You have {} actions left. Fix your edit so these tests pass — \
                         or revert the part that broke them (`git diff HEAD` shows your \
                         changes) — and run the tests yourself with code/shell before \
                         settling.",
                            files_changed.join(", "),
                            tail,
                            remaining
                        );
                        (patch, files_changed) = redrive_with_fact(
                            &cycle,
                            room,
                            framing,
                            remaining,
                            fact,
                            &mut settled,
                            &workspace,
                        )
                        .await;
                    }
                    Err(e) => {
                        crate::probe!(
                            class = "benchmark.verifier.error",
                            run_id = %run_id.as_deref().unwrap_or("-"),
                            error = %e,
                            "in-loop verifier could not run tests — env fault, settle \
                             stands (never blocks the attempt)"
                        );
                        break;
                    }
                }
            }

            // 5) LEARN mode (#221 slice 3): carry the EXPERIENCE back to the living self —
            //    the same one-way bridge cognition/eval's learn mode uses. The lesson is
            //    experience-shaped (task + how she worked + which files), deliberately
            //    excluding the patch content and her final answer: the python-context
            //    signal that drives dream supersession rides the task text and file
            //    names; verbatim solutions would let a re-run score memorization instead
            //    of capability. Solve carries no held-out answer key in-band (the harness
            //    grades externally), so there is nothing to redact.
            if p.learn.learns() {
                let admitted = transfer_solve_experience(
                    &persona_uuid,
                    room,
                    &p.task,
                    settled.acts,
                    &files_changed,
                );
                tracing::info!(
                    persona = %persona_uuid,
                    admitted,
                    acts = settled.acts,
                    "agent/solve learn mode: work experience admitted to the living self"
                );
            }

            // Lane drops here (end of scope) — measurement copy torn down, living personas untouched.
            drop(lane);

            Ok(AgentSolveResult {
                persona_id: p.persona_id.clone(),
                model: p.base_model_id.clone(),
                acts: settled.acts as u32,
                spoken: settled.spoken.unwrap_or_default(),
                patch,
                files_changed,
                files_examined: settled.touched_paths.clone(),
                detached: false,
                run_id,
                infra_error: settled.inference_error,
            })
        }
        .await;

        // MEASUREMENT OVER — give her back her own hands (#312). Best-effort but LOUD: a
        // failed restore leaves the living persona standing in the exam repo, which is a
        // real defect, but it must not overwrite the measurement's own verdict.
        if let Some(hands) = &hands {
            if let Err(e) =
                crate::cognition::persona_workspace::restore_acting_workspace(hands).await
            {
                tracing::error!(
                    persona = %persona_uuid,
                    error = %e,
                    "agent/solve could NOT return the persona's hands to her own workspace — she is \
                     still rooted at the exam sandbox and her live turns will act there (#312)"
                );
            }
        }
        outcome
    }
}

/// How much of the task text the durable lesson may carry. A lesson is a MEMORY OF
/// WORKING, not a copy of the assignment — and the task is caller-supplied text of
/// unbounded size. A SWE-bench problem statement is a full GitHub issue; six solve runs
/// put six of them, verbatim, into Anwen's episodic store, and the consolidator did what
/// it is supposed to do with repeated episodic content: it crystallized SEMANTIC beliefs
/// out of them ("When a Blueprint name in Flask contains a dot, raise ValueError…").
/// She now durably believes things about flask that she learned in an exam room.
///
/// The domain signal the dream's supersession review actually feeds on rides `files_changed`
/// (`mathlib.py` → python), which the lesson keeps in full. The task text is context, and
/// context does not need to be verbatim.
///
/// context-budget-exempt: this is a CONTAMINATION bound, not a context/prompt budget. It caps
/// how much EXAM TEXT may enter her durable memory, and deriving it from the served window
/// would invert the intent — a bigger window would admit MORE of the assignment, which is the
/// exact failure (#312, second vector) this constant exists to stop.
const LESSON_TASK_EXCERPT_CHARS: usize = 200;

/// Build the durable EXPERIENCE string for one solve — what she worked on and how,
/// never what she produced (no patch content, no spoken answer: the lesson teaches
/// context, not solutions, so re-runs measure capability rather than memorization).
/// Pure; unit-testable.
fn format_solve_lesson(task: &str, acts: usize, files_changed: &[String]) -> String {
    let worked = if files_changed.is_empty() {
        "I changed no files".to_string()
    } else {
        format!("I changed: {}", files_changed.join(", "))
    };
    let task = task.trim();
    let excerpt = match task.char_indices().nth(LESSON_TASK_EXCERPT_CHARS) {
        Some((cut, _)) => format!("{}…", &task[..cut]),
        None => task.to_string(),
    };
    format!(
        "I worked a real coding task in my workspace: {excerpt} — I acted {acts} time(s); {worked}."
    )
}

/// Admit one solve's experience lesson into the LIVING persona (never the fork) —
/// `cognition/eval::transfer_redacted_lessons`' one-way bridge, solve-shaped.
/// Returns 1 if a fresh lesson was admitted (identical re-run lessons dedup
/// idempotently via `admit_reflection`'s content hash), else 0.
fn transfer_solve_experience(
    persona_uuid: &Uuid,
    room: Uuid,
    task: &str,
    acts: usize,
    files_changed: &[String],
) -> usize {
    let Some(admission) = crate::cognition::persona_workspace::global()
        .get(persona_uuid)
        .and_then(|cycle| cycle.acting().map(|a| a.admission.clone()))
    else {
        tracing::warn!(
            persona = %persona_uuid,
            "agent/solve learn mode: no live admission — experience not transferred \
             (she was measured, but the living self is not resident to learn)"
        );
        return 0;
    };
    let mut recall_keys = vec!["agent-solve".to_string()];
    recall_keys.extend(files_changed.iter().cloned());
    let engram = crate::persona::engram::Engram {
        id: Uuid::new_v4(),
        context_id: Some(room),
        kind: crate::persona::engram::EngramKind::Episodic,
        content: format_solve_lesson(task, acts, files_changed),
        origin: crate::persona::engram::EngramOrigin::SelfReflection {
            parent_engram_id: Uuid::nil(),
        },
        recall_keys,
        admitted_at_ms: crate::persona::trace::now_ms(),
        trust_state_at_admission: crate::persona::engram::TrustState::SelfTrust,
        admission_trace_id: None,
    };
    match admission.admit_reflection(engram) {
        Ok(crate::persona::engram::AdmissionDecision::Admit { .. }) => 1,
        _ => 0,
    }
}

/// What is NOT part of a solution lives in ONE place, beside the other reading of her work:
/// [`crate::commands::benchmark::SOLUTION_PATH_EXCLUDES`]. This file used to carry its own
/// near-copy that omitted `.airc`, which is precisely the drift `workspace_candidate_diff`'s
/// doc warned about — see the shared constant for the two incidents.
use crate::commands::benchmark::SOLUTION_PATH_EXCLUDES as PATCH_EXCLUDES;

/// Unified diff of the SOLUTION changes in the workspace (tracked edits + new files), and the
/// touched paths — build/cache byproducts ([`PATCH_EXCLUDES`]) filtered out so the graded artifact
/// is source-only. `git add -N` stages new files as intent-to-add so `git diff` includes them
/// without committing content; the same excludes keep junk from being intent-added in the first
/// place. Non-repo or git-less environments return empty (honest — no hands artifact).
/// The touched-file → test-file mapping for the in-loop verifier. Deliberately
/// dumb v1: a source file maps to a sibling `tests/test_<stem>.py` (sympy's
/// layout) or a top-level `tests/test_<stem>.py` (flask/requests layout).
/// No mapping found → the verifier skips, loudly. Repo-specific tables can
/// grow here as data when the dumb version's misses are measured.
fn mapped_test_files(workspace: &str, files_changed: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for f in files_changed {
        let path = std::path::Path::new(f);
        let Some(stem) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Some(parent) = path.parent() else {
            continue;
        };
        let candidates = [
            parent.join("tests").join(format!("test_{stem}.py")),
            std::path::PathBuf::from("tests").join(format!("test_{stem}.py")),
        ];
        for cand in candidates {
            if std::path::Path::new(workspace).join(&cand).exists() {
                let c = cand.to_string_lossy().to_string();
                if !out.contains(&c) {
                    out.push(c);
                }
            }
        }
    }
    out
}

/// ONE bounded re-drive with a structural fact injected as the next burst —
/// the shared plumbing of the empty-diff and identical-diff re-drives (the
/// two blocks differ only in trigger and fact text; a second inline copy
/// would drift on the fold rules). Folds the re-drive into the attempt's
/// outcome — totals sum, the final verdict/world-state are the re-drive's
/// (it is the attempt's true end), the spoken text falls back to the first
/// settle's if the re-drive ended un-spoken — and returns the workspace's
/// post-re-drive (patch, files_changed).
async fn redrive_with_fact(
    cycle: &crate::cognition::workspace::WorkspaceCycle,
    room: Uuid,
    framing: crate::cognition::workspace::TurnFraming,
    remaining: usize,
    fact: String,
    settled: &mut crate::cognition::act_observe::SettleOutcome,
    workspace: &str,
) -> (String, Vec<String>) {
    let redelivery = crate::persona::rag_budget::RagDelivery {
        source_id: "airc".to_string(),
        items: vec![crate::persona::rag_budget::RagItem {
            content: fact,
            tokens: 0,
            metadata: serde_json::json!({
                "peer_id": "peer",
                "occurred_at_ms": crate::persona::trace::now_ms(),
            }),
        }],
        tokens_used: 0,
        continuation: None,
        resolution_used: crate::persona::rag_budget::ResolutionPreference::Raw,
    };
    let reburst = crate::cognition::workspace::Burst::from_turns(
        room,
        crate::persona::service_loop::build_workspace_turns(
            std::slice::from_ref(&redelivery),
            "",
            "",
            None,
        ),
    );
    let redriven =
        crate::cognition::act_observe::drive_to_settle(cycle, reburst, room, remaining, framing)
            .await;
    settled.acts += redriven.acts;
    settled.decision = redriven.decision;
    settled.spoken = redriven.spoken.or(settled.spoken.take());
    settled.world_state = redriven.world_state;
    settled.inference_error = redriven.inference_error;
    for path in redriven.touched_paths {
        if !settled.touched_paths.contains(&path) {
            settled.touched_paths.push(path);
        }
    }
    settled.metrics.input_tokens += redriven.metrics.input_tokens;
    settled.metrics.output_tokens += redriven.metrics.output_tokens;
    settled.metrics.latency_ms += redriven.metrics.latency_ms;
    settled.metrics.cached_tokens += redriven.metrics.cached_tokens;
    settled.metrics.prefill_tokens += redriven.metrics.prefill_tokens;
    settled.metrics.prefill_ms += redriven.metrics.prefill_ms;
    settled.metrics.decode_ms += redriven.metrics.decode_ms;
    workspace_patch(workspace).await
}

async fn workspace_patch(workspace: &str) -> (String, Vec<String>) {
    let git = |args: &[&str]| {
        let mut c = tokio::process::Command::new("git");
        c.arg("-C").arg(workspace).args(args);
        c
    };
    // Build the `-- . :(exclude…)` pathspec tail shared by add-N and both diffs.
    let mut pathspec: Vec<&str> = vec!["--", "."];
    pathspec.extend_from_slice(PATCH_EXCLUDES);
    let with_paths = |head: &[&str]| -> Vec<String> {
        head.iter()
            .chain(pathspec.iter())
            .map(|s| s.to_string())
            .collect()
    };
    // Non-fatal: a bare (non-git) workspace just yields no patch.
    let _ = git(&with_paths(&["add", "-A", "-N"])
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>())
    .output()
    .await;
    let diff_args = with_paths(&["diff"]);
    let names_args = with_paths(&["diff", "--name-only"]);
    let diff = git(&diff_args.iter().map(String::as_str).collect::<Vec<_>>())
        .output()
        .await
        .ok();
    let names = git(&names_args.iter().map(String::as_str).collect::<Vec<_>>())
        .output()
        .await
        .ok();
    let patch = diff
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned())
        .unwrap_or_default();
    let files_changed = names
        .filter(|o| o.status.success())
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .filter(|l| !l.trim().is_empty())
                .map(|l| l.to_string())
                .collect()
        })
        .unwrap_or_default();
    (patch, files_changed)
}

crate::register_stateless_command!(AgentSolve);

/// The generic solve CONTRACT wrapped around a task.
///
/// It states HOW acts take effect (only tool calls do; narration does not) and never WHAT the
/// deliverable looks like — the task owns that, and the two used to contradict each other.
///
/// The old text said "writing files with code/write" and "graded on the files your tools WRITE",
/// which is right for a from-scratch build gym. Nested beneath it, the SWE task text says the
/// opposite: "do not add new top-level files … find the existing source of the fault and edit it
/// in place" — that text is the dispatch CARD BODY (`benchmark::BenchmarkSweSetup`), which owns
/// the deliverable shape because the card IS the task.
/// Outer contract first, inner constraint under "Task:" — and she obeyed
/// the outer one. Three consecutive full-effort sympy-21379 runs wrote NEW repro scripts and never
/// edited the library (v3: 1 file, v4: 3 files, v5: 2 files; 0 edits every time). That read as a
/// judgement gap for a whole session; it was two halves of one framing disagreeing.
///
/// FORCE vs SHAPE, learned the hard way. The first attempt at this fix removed the
/// contradiction and the tool-forcing PRESSURE in the same edit — "graded exactly as your
/// tools leave it" is shape-neutral but passive. Measured immediately (v6, same instance,
/// same persona): 8 acts, ZERO files, 0 patch bytes — worse than the three contradictory
/// runs before it, which at least produced repro scripts. She drifted out of task mode
/// entirely and ended the run replying to her OWN `work/list` output as though a peer had
/// posted it in chat.
///
/// So the wording must be BOTH: imperative about the contract, silent about the artifact.
/// "graded ONLY on the CHANGES your tools make" is forceful and neutral — an edit and a new
/// file are both changes; a narration is not.
///
/// Pure so the contract is testable in isolation ([[the-compression-principle]]: one place).
fn frame_task(task: &str) -> String {
    format!(
        "This is a task you must COMPLETE NOW by USING YOUR TOOLS in your workspace — editing \
         files with code/edit, writing them with code/write, running commands with code/shell, \
         etc. Only what your tools actually do takes effect: code shown in a message, or a claim \
         that you saved a file, does NOT create or change anything — you are graded ONLY on the \
         CHANGES your tools make to this workspace — an explanation earns nothing. You are ALREADY in the task's workspace: work on the \
         files that are here. Do not create a new workspace or start a new project — grading only \
         sees this one. Follow the task's own instructions about WHAT to change. Do the work with \
         tool calls, then stop.\n\nTask:\n{}",
        task.trim()
    )
}

#[cfg(test)]
mod tests {
    mod run_visibility {
        use super::super::RunVisibility;
        use uuid::Uuid;

        // what this catches: the roomless branch going quiet again. It was
        // `p.room.unwrap_or_else(Uuid::nil)` — a consequential branch taken in silence — and
        // 13,209 turns (8.7% of all turns; 35% for one citizen) executed invisibly before
        // anyone measured it, because an invisible run and a visible one log identically.
        // The type exists so the branch has a NAME and the invisible case carries a sentence.
        #[test]
        fn a_roomless_run_is_declared_invisible_and_says_why() {
            let v = RunVisibility::resolve(None);
            assert_eq!(v, RunVisibility::Invisible);
            assert!(v.room_id().is_nil(), "the act pipeline keys its skip on nil");
            let why = v.warning().expect("an invisible run MUST announce itself");
            assert!(
                why.contains("NO room") && why.contains("perceive"),
                "the warning must say what is lost, not just that a field was absent: {why}"
            );
            assert!(
                why.contains("room"),
                "and name the param that fixes it: {why}"
            );
        }

        // what this catches: an EXPLICIT nil uuid slipping through as "in room". A caller
        // passing Uuid::nil() means exactly what omitting it means; treating the two
        // differently would reopen the silent branch through the other door.
        #[test]
        fn an_explicit_nil_room_is_the_same_as_no_room() {
            assert_eq!(
                RunVisibility::resolve(Some(Uuid::nil())),
                RunVisibility::Invisible
            );
        }

        // what this catches: warning on the happy path. A visible run needs no announcement,
        // and a warning that fires every time trains everyone to ignore it.
        #[test]
        fn a_run_with_a_real_room_is_visible_and_stays_quiet() {
            let room = Uuid::from_u128(7);
            let v = RunVisibility::resolve(Some(room));
            assert_eq!(v, RunVisibility::InRoom(room));
            assert_eq!(v.room_id(), room, "the room must survive unchanged");
            assert!(v.warning().is_none(), "a perceivable run must not warn");
        }
    }

    mod patch_custody {
        // what this catches: patch custody going back to being a caller courtesy. It WAS
        // one — the write sat behind `if let Some(capture_dir)` with no else — and the
        // consequence was measured on 2026-08-18: every hand-launched run kept its diff
        // (25 patches under benchmarks/swe/captures/run-*), and every citizen-dispatched
        // `claim-*` run, which is the entire path the benchmark actually runs on, kept
        // none. A 41,166-byte patch that broke 40 passing tests was unrecoverable hours
        // later because the workspace had already been reset. A run that cannot produce
        // the artifact behind its own verdict is an anecdote, not a measurement.
        #[test]
        fn a_run_that_names_no_capture_dir_still_gets_one() {
            let derived = super::super::run_artifact_dir("claim-abc123", None)
                .expect("a run always resolves an artifact dir");
            assert!(
                derived.ends_with("run-claim-abc123"),
                "custody must be derived from the run itself, not left to the caller: {}",
                derived.display()
            );
            // It lands under the GOVERNED benchmarks root, in the same `captures/` folder
            // the 25 hand-launched patches already occupy. Two invariants in one
            // assertion, both of which my first cut broke by inventing `progress/`:
            // one home per artifact class, and that home is a registered TrackedDir with
            // a decided eviction story (an unmanaged dir growing patches forever is the
            // 460 GB shape).
            let expected =
                crate::cognition::swe_bench::swe_cache_dir().join("captures");
            assert_eq!(
                derived.parent(),
                Some(expected.as_path()),
                "patches belong where patches already live, under the tracked benchmarks \
                 root — never a second location: {}",
                derived.display()
            );
        }

        // what this catches: an explicit capture_dir being ignored once the fallback
        // exists — the hand-launched runs that DO name a folder must keep landing there,
        // or the 25 existing patches stop being where every prior receipt says they are.
        #[test]
        fn an_explicit_capture_dir_still_wins() {
            let dir = super::super::run_artifact_dir("run-18057-h1", Some("/tmp/named-run"))
                .expect("explicit dir resolves");
            assert_eq!(dir, std::path::PathBuf::from("/tmp/named-run"));
        }
    }

    // what this catches: the wrapper asserting a DELIVERABLE SHAPE that the task contradicts.
    // The generic framing exists to kill narration ("only tool calls take effect"). It must not
    // also claim the grade is about "files your tools WRITE" — the SWE dispatch card body
    // (`benchmark::BenchmarkSweSetup`) says the opposite ("do not add new top-level files …
    // edit it in place"), and the
    // wrapper comes FIRST. Three consecutive sympy-21379 runs obeyed the wrapper and wrote new
    // repro scripts instead of editing the library. The contract may describe HOW acts take
    // effect; only the task may describe WHAT to change.
    #[test]
    fn the_generic_framing_never_dictates_the_deliverable_shape() {
        let framed = super::frame_task("fix the bug IN PLACE");
        let lower = framed.to_lowercase();
        assert!(
            lower.contains("only what your tools actually do takes effect"),
            "the anti-narration contract must survive: {framed}"
        );
        assert!(
            lower.contains("code/edit"),
            "editing must be a first-class exemplar verb, not just writing: {framed}"
        );
        assert!(
            !lower.contains("graded on the files your tools write"),
            "must NOT assert new-files-are-the-deliverable — that contradicts a fix-in-place task"
        );
        // …and must NOT go limp while removing that. The first attempt did, and the very next
        // live run produced 0 acts of work: shape-neutral is necessary, force is too.
        assert!(
            lower.contains("only on the changes your tools make"),
            "the contract must stay IMPERATIVE about changes, not merely descriptive: {framed}"
        );
        assert!(
            framed.contains("fix the bug IN PLACE"),
            "the task's own words are carried through verbatim"
        );
    }

    use super::*;
    use tokio::process::Command;

    async fn git(dir: &std::path::Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .await
            .expect("git runs");
        assert!(
            out.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    // what this catches: the patch is the benchmark's HANDS artifact — the SWE/Terminal-Bench
    // harness applies it and runs the repo's tests. If workspace_patch dropped NEW files (the
    // common case for a from-scratch task) it would silently score 0 on every such task while
    // looking like the agent "did nothing". This pins that `git add -A -N` + `git diff` includes
    // both an edit to a tracked file AND a brand-new untracked file.
    #[tokio::test]
    async fn workspace_patch_includes_edits_and_new_files() {
        let dir = std::env::temp_dir().join(format!("cu-agent-solve-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        git(&dir, &["init", "-q"]).await;
        git(&dir, &["config", "user.email", "t@t"]).await;
        git(&dir, &["config", "user.name", "t"]).await;
        std::fs::write(dir.join("tracked.txt"), "one\n").unwrap();
        git(&dir, &["add", "-A"]).await;
        git(&dir, &["commit", "-qm", "base"]).await;

        // The agent's "hands": edit a tracked file + create a new one.
        std::fs::write(dir.join("tracked.txt"), "one\ntwo\n").unwrap();
        std::fs::write(dir.join("brand_new.rs"), "fn main() {}\n").unwrap();

        let (patch, files) = workspace_patch(dir.to_str().unwrap()).await;
        assert!(
            patch.contains("tracked.txt"),
            "edit missing from patch:\n{patch}"
        );
        assert!(
            patch.contains("brand_new.rs"),
            "NEW file missing from patch:\n{patch}"
        );
        assert!(patch.contains("+two"), "edit content missing:\n{patch}");
        assert!(
            patch.contains("fn main()"),
            "new-file content missing:\n{patch}"
        );
        assert!(files.iter().any(|f| f == "tracked.txt"));
        assert!(files.iter().any(|f| f == "brand_new.rs"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: a CREDENTIAL reaching a graded patch, files_changed, or the curriculum.
    // Live 2026-08-18: sympy-22714's tree held `.airc/identity.key` (a private keypair) at git
    // status `A` — already intent-added, because this path's exclude list omitted `.airc` while
    // the grader's inline list had it. airc creates its scope at the enclosing git root, so a
    // citizen working inside a cloned bench repo gets one written under the repo she is graded
    // on. files_changed feeds format_solve_lesson, so an unexcluded key becomes the training
    // sentence "I changed: .airc/identity.key". Also the b34f7eb5 shape: 91KB of staged .airc
    // blobs once voided a REAL fix because the fresh clone refused the whole candidate.
    #[tokio::test]
    async fn workspace_patch_never_carries_agent_scope_state_or_credentials() {
        let dir = std::env::temp_dir().join(format!("cu-agent-solve-airc-{}", Uuid::new_v4()));
        std::fs::create_dir_all(dir.join(".airc/work-board-cache")).unwrap();
        git(&dir, &["init", "-q"]).await;
        git(&dir, &["config", "user.email", "t@t"]).await;
        git(&dir, &["config", "user.name", "t"]).await;
        // her actual solution
        std::fs::write(dir.join("point.py"), "def dot(a, b):\n    return a * b\n").unwrap();
        // what the SUBSTRATE wrote into her tree — never authored by the solver
        std::fs::write(dir.join(".airc/identity.key"), "SUPERSECRETKEYMATERIAL").unwrap();
        std::fs::write(dir.join(".airc/events.sqlite"), b"SQLite format 3\x00").unwrap();
        std::fs::write(dir.join(".airc/work-board-cache/x.json"), "{}").unwrap();

        let (patch, files) = workspace_patch(dir.to_str().unwrap()).await;
        assert!(
            patch.contains("point.py"),
            "her solution must still be in the patch:\n{patch}"
        );
        assert!(
            !patch.contains("SUPERSECRETKEYMATERIAL"),
            "KEY MATERIAL must never reach a patch:\n{patch}"
        );
        assert!(
            !patch.contains(".airc"),
            "no agent-scope path may appear in the patch:\n{patch}"
        );
        assert_eq!(
            files,
            vec!["point.py".to_string()],
            "files_changed feeds the curriculum lesson — it must name only her work"
        );
    }

    // what this catches: verification byproducts (Python bytecode, __pycache__) must NOT pollute
    // the graded patch — glass-boxed 2026-07-22 when a `python3 -c "from calc import ..."` verify
    // step left calc.cpython-314.pyc in the diff. Source is kept; the cache junk is filtered.
    #[tokio::test]
    async fn workspace_patch_excludes_build_byproducts() {
        let dir = std::env::temp_dir().join(format!("cu-agent-solve-junk-{}", Uuid::new_v4()));
        std::fs::create_dir_all(dir.join("__pycache__")).unwrap();
        git(&dir, &["init", "-q"]).await;
        git(&dir, &["config", "user.email", "t@t"]).await;
        git(&dir, &["config", "user.name", "t"]).await;
        // her solution + the byproducts a verify run leaves behind
        std::fs::write(dir.join("calc.py"), "def add(a, b):\n    return a + b\n").unwrap();
        std::fs::write(
            dir.join("__pycache__/calc.cpython-314.pyc"),
            b"\x00\x01bytecode",
        )
        .unwrap();
        std::fs::create_dir_all(dir.join("node_modules/x")).unwrap();
        std::fs::write(dir.join("node_modules/x/index.js"), "module.exports={}").unwrap();

        let (patch, files) = workspace_patch(dir.to_str().unwrap()).await;
        assert!(
            patch.contains("calc.py"),
            "the solution source must be in the patch:\n{patch}"
        );
        assert!(
            !patch.contains(".pyc"),
            "bytecode must be excluded:\n{patch}"
        );
        assert!(
            !patch.contains("__pycache__"),
            "cache dir must be excluded:\n{patch}"
        );
        assert!(
            !patch.contains("node_modules"),
            "deps must be excluded:\n{patch}"
        );
        assert_eq!(
            files,
            vec!["calc.py".to_string()],
            "only source is a changed file: {files:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: a non-git workspace must degrade to an empty patch, never panic — a
    // bare sandbox dir (some benchmarks hand the agent a plain cwd) is a legitimate input.
    #[tokio::test]
    async fn workspace_patch_is_empty_on_non_git_dir() {
        let dir = std::env::temp_dir().join(format!("cu-agent-solve-bare-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("x.txt"), "hi\n").unwrap();
        let (patch, files) = workspace_patch(dir.to_str().unwrap()).await;
        assert!(
            patch.is_empty(),
            "bare dir should yield no patch, got:\n{patch}"
        );
        assert!(files.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: the learn-mode lesson teaches CONTEXT, never solutions —
    // it must carry the task text + how she worked + which files (the language/
    // domain signal dream-supersession feeds on) and must NOT be empty-file
    // fragile. If patch content or answers ever leak into the lesson, battery
    // re-runs would measure memorization instead of capability.
    #[test]
    fn solve_lesson_is_experience_shaped() {
        let l = format_solve_lesson(
            "There is a bug in mathlib.py: multiply returns a+b. Fix it.",
            3,
            &["mathlib.py".to_string()],
        );
        assert!(
            l.contains("mathlib.py"),
            "domain signal rides the file name: {l}"
        );
        assert!(l.contains("acted 3 time(s)"));
        assert!(l.contains("I changed: mathlib.py"));
        let none = format_solve_lesson("task", 0, &[]);
        assert!(none.contains("I changed no files"));
    }

    // what this catches: an unbounded task text turning a durable lesson into a verbatim
    // copy of the assignment. Measured — six SWE-bench solves wrote six full GitHub issues
    // into Anwen's episodic store, and the consolidator distilled SEMANTIC beliefs about
    // flask internals out of the repetition. A lesson is a memory of WORKING; the file
    // names carry the domain signal and must survive the bound intact.
    #[test]
    fn a_lesson_excerpts_the_task_it_never_copies_it() {
        let issue = format!(
            "Flask raises an unhelpful error when a Blueprint name contains a dot. {}",
            "Blueprint names should be validated at __init__ time. ".repeat(40)
        );
        let l = format_solve_lesson(&issue, 5, &["src/flask/blueprints.py".to_string()]);
        assert!(
            l.len() < issue.len() / 2,
            "the assignment must not land in memory verbatim ({} chars of a {}-char issue)",
            l.len(),
            issue.len()
        );
        assert!(
            l.contains('…'),
            "a truncated lesson must SAY it was truncated: {l}"
        );
        assert!(
            l.contains("src/flask/blueprints.py"),
            "the domain signal rides the file names and is never truncated: {l}"
        );
        assert!(l.contains("acted 5 time(s)"));
    }

    // what this catches: the wire name must mirror the file path (commands/agent/solve.rs ⟺
    // "agent/solve") and stay Privileged — it drives arbitrary shell + writes files in the cwd,
    // the same authority boundary as agent/start. Drift silently breaks routing or widens the ACL.
    #[test]
    fn name_and_access_hold_the_contract() {
        assert_eq!(AgentSolve::NAME, "agent/solve");
        assert!(matches!(AgentSolve::ACCESS, AccessLevel::Privileged));
    }

    // what this catches (found by BigMama 2026-08-06, reading before wiring the consolidator):
    // the SAME field with OPPOSITE defaults in two modules — `agent/solve` defaulted learn ON
    // while `cognition/eval` defaulted it OFF, and the only thing keeping exam text out of
    // episodic was one explicit `Some(false)` at a single call site.
    //
    // The Rust half of that hazard is now gone by CONSTRUCTION: `LearningPolicy` has no
    // `Default`, so a caller who omits the decision does not compile. What CANNOT be closed by
    // the type system is the wire — a JSON or CLI caller can always omit a field — so that one
    // remaining door is what this test guards, on BOTH param types at once.
    //
    // Pins the invariant, not the number: an omitted `learn` deserializes to DO NOT LEARN
    // everywhere. If a future edit gives `LearningPolicy` a `Default`, or points either
    // `#[serde(default = ...)]` at a different function, this reds.
    #[test]
    fn an_omitted_learn_flag_means_do_not_learn_on_every_wire_path() {
        let solve: AgentSolveParams = serde_json::from_str(
            r#"{"persona_id":"p","base_model_id":"m","task":"x","workspace":"w"}"#,
        )
        .expect("solve params without `learn`");
        assert!(
            !solve.learn.learns(),
            "agent/solve is the headless BENCHMARK entrypoint — an omitted learn flag must not \
             admit exam experience into the living persona (#312)"
        );

        let eval: crate::cognition::eval::CognitionEvalParams =
            serde_json::from_str(r#"{"persona_id":"p"}"#).expect("eval params without `learn`");
        assert!(
            !eval.learn.learns(),
            "cognition/eval measures; an omitted learn flag must not write back"
        );

        // The two readers must AGREE on what omission means. One field name with two
        // meanings, decided by which module you happened to reach, is the original defect.
        assert_eq!(
            solve.learn, eval.learn,
            "both wire paths must resolve an omitted `learn` identically"
        );
    }
}
