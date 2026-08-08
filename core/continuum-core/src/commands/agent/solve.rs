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
#[ts(export, export_to = "../../../protocol/typescript/agent/AgentSolveParams.ts")]
pub struct AgentSolveParams {
    /// The persona (UUID, spawned) whose FULL cognition works the task.
    pub persona_id: String,
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
    #[ts(optional, type = "number")]
    pub max_acts: Option<u32>,
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
    #[ts(optional, type = "number")]
    pub attempts: Option<u32>,
}

/// What the caller grades when the solve returns. Two genuinely different contracts,
/// so it is an enum on the wire, never a magic string ([[strings-to-enums]]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "lowercase")]
#[ts(export, export_to = "../../../protocol/typescript/agent/Deliverable.ts")]
pub enum Deliverable {
    /// Her spoken answer is the result (the default — every non-diff task).
    #[default]
    Answer,
    /// The state of the workspace is the result; the grader applies the diff.
    Workspace,
}

#[derive(Debug, Clone, Serialize, TS, JsonSchema)]
#[ts(export, export_to = "../../../protocol/typescript/agent/AgentSolveResult.ts")]
pub struct AgentSolveResult {
    pub persona_id: String,
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
            let run_id = p.run_id.clone().unwrap_or_else(|| Uuid::new_v4().to_string());
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
                for attempt in 1..=max_attempts {
                    let mut this_attempt = inner.clone();
                    this_attempt.task = next_task.clone();
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
                            if let (Some(path), Ok(json)) =
                                (path.as_ref(), serde_json::to_string_pretty(&r))
                            {
                                let _ = std::fs::write(path, json);
                            }
                            if let Some(bus) = crate::runtime::MessageBus::global() {
                                if let Ok(v) = serde_json::to_value(&r) {
                                    bus.publish_async_only("agent:solve:complete", v);
                                }
                            }
                            tracing::info!(run_id = %run_id, acts = r.acts, attempt, "agent/solve detached run complete");
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
                                    crate::probe!(
                                        class = "benchmark.attempt.end",
                                        run_id = %run_id,
                                        instance = %instance,
                                        attempt,
                                        max_attempts,
                                        resolved = g.resolved,
                                        gate_ok = g.gate_ok,
                                        f2p_passed = g.fail_to_pass_passed,
                                        f2p_total = g.fail_to_pass_total,
                                        p2p_passed = g.pass_to_pass_passed,
                                        p2p_total = g.pass_to_pass_total,
                                        patch_bytes = g.patch_bytes,
                                        failed_tests = %g.failed_tests.join(","),
                                        "solve attempt graded — the verdict, on the wire"
                                    );
                                    crate::probe!(
                                        class = "benchmark.autograde",
                                        run_id = %run_id,
                                        instance = %instance,
                                        resolved = g.resolved,
                                        gate_ok = g.gate_ok,
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
                                        .filter(|p| p.rsplit('/').next().is_some_and(|s| s.contains('.')))
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
                                            format!(" Your edits are in: {}.", r.files_changed.join(", "))
                                        };
                                        format!(
                                            "{base_task}\n\n[grader verdict — attempt {attempt} of {max_attempts} did not resolve] \
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
fn agent_solve_ledger_path(run_id: &str) -> Option<std::path::PathBuf> {
    let base = std::env::var("CONTINUUM_HOME")
        .map(std::path::PathBuf::from)
        .ok()
        .or_else(|| dirs::home_dir().map(|h| h.join(".continuum")))?;
    let dir = base.join("progress");
    let _ = std::fs::create_dir_all(&dir);
    Some(dir.join(format!("agent-solve-{run_id}.json")))
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
            p.persona_id.trim(),
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
        let lane = crate::cognition::eval::spawn_base_eval_lane(&p.base_model_id).await?;

        // 2) Fork her WHOLE cognition onto that lane, rooted at the workspace: tools ON, recall ON.
        //    A brief wait covers the post-spawn template race (same as the eval fork-waiter).
        let registry = crate::cognition::persona_workspace::global();
        let mut cycle = None;
        for attempt in 0..FORK_WAIT_TRIES {
            cycle = registry.fork_eval_cycle_with_adapter(
                &persona_uuid,
                lane.adapter.clone(),
                lane.served_ctx,
                true,             // with_tools — her hands are ON
                Some(&workspace), // roots the ToolExecutor at the sandbox cwd
                p.suppress_recall.unwrap_or(false), // memory/RAG ON by default; the diagnostic knob
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
        let room = Uuid::nil();
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
        // deliverable. Nested beneath it, `swe_task_prompt` says the opposite: "do not add new
        // top-level files — fix it IN PLACE with code/edit. The fix must land in the existing
        // files."
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
        let mut settled =
            crate::cognition::act_observe::drive_to_settle(&cycle, burst, room, max_acts, framing)
                .await;

        // 4) Collect the HANDS artifact: everything she changed in the workspace as a unified diff
        //    (new files included), plus the touched paths. This is what SWE/Terminal-Bench apply.
        let (mut patch, mut files_changed) = workspace_patch(&workspace).await;

        // EMPTY-DIFF RE-DRIVE — the two-gates doctrine made mechanism (glass-boxed
        // 2026-08-08, atlas-sympy-24066-n6 attempts 2+3): on a Workspace-deliverable
        // task she settled by SPEAKING after ONE act — a generic file summary, zero
        // edits — leaving 11 of 12 acts unused, twice, near-verbatim. Working is not
        // speaking: when the deliverable is the workspace diff, a Speak with an EMPTY
        // diff and real remaining budget must not end the attempt. ONE bounded
        // re-drive (a retry, never a nag loop): state the structural fact, hand back
        // the remaining budget. If she speaks to an empty diff again, THAT settles —
        // honestly graded, with the fact on the record. Not fired on budget
        // exhaustion (spoken=None un-driven Act) or infra failure — those already
        // grade honestly.
        if workspace_deliverable
            && patch.is_empty()
            && settled.inference_error.is_none()
            && settled.spoken.is_some()
            && settled.acts + 1 < max_acts
        {
            let remaining = max_acts - settled.acts;
            crate::probe!(
                class = "benchmark.empty_diff_redrive",
                run_id = %run_id.as_deref().unwrap_or("-"),
                acts_used = settled.acts,
                acts_remaining = remaining,
                "Speak settled a workspace-deliverable attempt with an EMPTY diff and \
                 remaining act budget — one bounded re-drive with the structural fact"
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
            let redriven = crate::cognition::act_observe::drive_to_settle(
                &cycle, reburst, room, remaining, framing,
            )
            .await;
            // Fold the re-drive into the attempt's outcome: totals sum, the final
            // verdict/world-state are the re-drive's (it is the attempt's true end),
            // the spoken text falls back to the first settle's if the re-drive
            // ended un-spoken (budget-exhausted Act grades as did-not-finish).
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
            (patch, files_changed) = workspace_patch(&workspace).await;
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
            if let Err(e) = crate::cognition::persona_workspace::restore_acting_workspace(hands)
                .await
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

/// Git pathspecs excluding the universal never-a-solution byproducts a verification run leaves
/// behind — Python bytecode/caches, tool caches, JS deps, OS cruft. Glass-boxed 2026-07-22: a
/// `python3 -c "from calc import ..."` verify step left `__pycache__/calc.cpython-314.pyc` in the
/// patch, polluting the graded artifact — real SWE-bench/aider patches are SOURCE-only. These are
/// never a solution, so they're excluded from both the diff and files_changed; anything a task
/// might legitimately produce (incl. `build`/`dist`/`target`) is kept.
const PATCH_EXCLUDES: &[&str] = &[
    ":(exclude,glob)**/__pycache__/**",
    ":(exclude,glob)**/*.pyc",
    ":(exclude,glob)**/*.pyo",
    ":(exclude,glob)**/.pytest_cache/**",
    ":(exclude,glob)**/.mypy_cache/**",
    ":(exclude,glob)**/.ruff_cache/**",
    ":(exclude,glob)**/node_modules/**",
    ":(exclude,glob)**/.DS_Store",
];

/// Unified diff of the SOLUTION changes in the workspace (tracked edits + new files), and the
/// touched paths — build/cache byproducts ([`PATCH_EXCLUDES`]) filtered out so the graded artifact
/// is source-only. `git add -N` stages new files as intent-to-add so `git diff` includes them
/// without committing content; the same excludes keep junk from being intent-added in the first
/// place. Non-repo or git-less environments return empty (honest — no hands artifact).
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
        head.iter().chain(pathspec.iter()).map(|s| s.to_string()).collect()
    };
    // Non-fatal: a bare (non-git) workspace just yields no patch.
    let _ = git(&with_paths(&["add", "-A", "-N"]).iter().map(String::as_str).collect::<Vec<_>>())
        .output()
        .await;
    let diff_args = with_paths(&["diff"]);
    let names_args = with_paths(&["diff", "--name-only"]);
    let diff = git(&diff_args.iter().map(String::as_str).collect::<Vec<_>>()).output().await.ok();
    let names = git(&names_args.iter().map(String::as_str).collect::<Vec<_>>()).output().await.ok();
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
/// which is right for a from-scratch build gym. Nested beneath it, `swe_task_prompt` says the
/// opposite: "do not add new top-level files — fix it IN PLACE with code/edit. The fix must land
/// in the existing files." Outer contract first, inner constraint under "Task:" — and she obeyed
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
    // what this catches: the wrapper asserting a DELIVERABLE SHAPE that the task contradicts.
    // The generic framing exists to kill narration ("only tool calls take effect"). It must not
    // also claim the grade is about "files your tools WRITE" — `swe_task_prompt` says the
    // opposite ("do not add new top-level files … fix it IN PLACE with code/edit"), and the
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
        assert!(out.status.success(), "git {:?} failed: {}", args, String::from_utf8_lossy(&out.stderr));
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
        assert!(patch.contains("tracked.txt"), "edit missing from patch:\n{patch}");
        assert!(patch.contains("brand_new.rs"), "NEW file missing from patch:\n{patch}");
        assert!(patch.contains("+two"), "edit content missing:\n{patch}");
        assert!(patch.contains("fn main()"), "new-file content missing:\n{patch}");
        assert!(files.iter().any(|f| f == "tracked.txt"));
        assert!(files.iter().any(|f| f == "brand_new.rs"));

        let _ = std::fs::remove_dir_all(&dir);
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
        std::fs::write(dir.join("__pycache__/calc.cpython-314.pyc"), b"\x00\x01bytecode").unwrap();
        std::fs::create_dir_all(dir.join("node_modules/x")).unwrap();
        std::fs::write(dir.join("node_modules/x/index.js"), "module.exports={}").unwrap();

        let (patch, files) = workspace_patch(dir.to_str().unwrap()).await;
        assert!(patch.contains("calc.py"), "the solution source must be in the patch:\n{patch}");
        assert!(!patch.contains(".pyc"), "bytecode must be excluded:\n{patch}");
        assert!(!patch.contains("__pycache__"), "cache dir must be excluded:\n{patch}");
        assert!(!patch.contains("node_modules"), "deps must be excluded:\n{patch}");
        assert_eq!(files, vec!["calc.py".to_string()], "only source is a changed file: {files:?}");
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
        assert!(patch.is_empty(), "bare dir should yield no patch, got:\n{patch}");
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
        assert!(l.contains("mathlib.py"), "domain signal rides the file name: {l}");
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
        assert!(l.contains('…'), "a truncated lesson must SAY it was truncated: {l}");
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
        let solve: AgentSolveParams =
            serde_json::from_str(r#"{"persona_id":"p","base_model_id":"m","task":"x","workspace":"w"}"#)
                .expect("solve params without `learn`");
        assert!(
            !solve.learn.learns(),
            "agent/solve is the headless BENCHMARK entrypoint — an omitted learn flag must not \
             admit exam experience into the living persona (#312)"
        );

        let eval: crate::cognition::eval::CognitionEvalParams =
            serde_json::from_str(r#"{"persona_id":"p"}"#)
                .expect("eval params without `learn`");
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
