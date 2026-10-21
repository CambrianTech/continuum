use super::*;

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
                // Per-attempt STALL WATCHDOG (harnesses-first, Joel 2026-08-08): a
                // wedged fork/lane used to stall this loop SILENTLY FOREVER — glass-
                // boxed live: both graded runs froze after attempt 2 for 2.5h with zero
                // ticks, zero markers, and the operator found out by ASKING. Silence
                // must never be ambiguous with progress.
                //
                // The verdict is ADVANCEMENT, not wall-clock (dd71a114, Joaquin's
                // instrument; Benchy's order: progressing → stalled → ceiling). The
                // previous shape was one flat timeout of `max_acts × 8 min` over the whole
                // attempt: sized for the ~5.5 min/act of August, it ended EVERY run on the
                // M5 of 2026-09-16 — acts there cost a median 21 min (1.5 min of model, the
                // rest lane queueing on a 16-mind roster), so 32 acts is ~11 h and the
                // 4.3 h deadline killed two control solves at 26 and 22 acts, the resume
                // pass re-fired them from act 1, and no solve on that box could ever
                // complete: 4.3 h of work, "stall", restart, forever. A run whose ledger
                // advances is not stalled at any pace; a run whose ledger has not moved
                // for ACT_STALL_ALLOWANCE is — that is the wedge this exists to catch. A
                // far ceiling remains and is NAMED as a ceiling when it fires.
                let attempt_ceiling = std::time::Duration::from_secs(
                    inner.max_acts.unwrap_or(DEFAULT_MAX_ACTS).max(1) as u64
                        * ATTEMPT_CEILING_PER_ACT_SECS,
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
                        stall_allowance_s = ACT_STALL_ALLOWANCE.as_secs(),
                        ceiling_s = attempt_ceiling.as_secs(),
                        "solve attempt starting — pulse anchor for run-liveness watchers"
                    );
                    let stop_asked = {
                        let run_id = run_id.clone();
                        move || crate::cognition::eval::eval_run_cancelled(&run_id)
                    };
                    let body = match attempt_with_advancement_watchdog(
                        path.as_deref(),
                        ACT_STALL_ALLOWANCE,
                        attempt_ceiling,
                        stop_asked,
                        AgentSolve::solve_body(this_attempt),
                    )
                    .await
                    {
                        Ok(body) => body,
                        Err(halt) => {
                            let cancelled = matches!(halt, AttemptHalt::Cancelled { .. });
                            let msg = if cancelled {
                                format!("attempt {attempt} of {max_attempts} {halt}")
                            } else {
                                format!(
                                    "attempt {attempt} of {max_attempts} {halt} — an INFRA fault, \
                                     never a capability verdict"
                                )
                            };
                            crate::probe!(
                                class = "benchmark.stall",
                                run_id = %run_id,
                                attempt,
                                verdict = halt.kind(),
                                acts = halt.acts(),
                                allowance_s = ACT_STALL_ALLOWANCE.as_secs(),
                                ceiling_s = attempt_ceiling.as_secs(),
                                "solve attempt halted by the watchdog — ending the run loudly"
                            );
                            if let Some(path) = path.as_ref() {
                                let _ = std::fs::write(
                                    path,
                                    serde_json::json!({
                                        "failed": true,
                                        "cancelled": cancelled,
                                        "infra_error": msg,
                                        "run_id": run_id,
                                        "attempt": attempt,
                                    })
                                    .to_string(),
                                );
                            }
                            if cancelled {
                                crate::cognition::eval::clear_eval_cancel(&run_id);
                                tracing::info!(run_id = %run_id, attempt, "agent/solve detached attempt cancelled by round-stop");
                            } else {
                                tracing::error!(run_id = %run_id, attempt, "agent/solve detached attempt stalled past deadline");
                            }
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
                                .unwrap_or_default(); // missing progress file = empty history; the redrive covers absence
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
                                    // THE REVIEW-LANDING EVENT (teams arc, 2026-08-30).
                                    // A charged reviewer who wakes to an empty room
                                    // correctly idles — and nothing called her back when
                                    // there was finally a diff to read. This is the
                                    // moment a reviewer is worth the most: a diff EXISTS,
                                    // the grader named what it misses, and a retry is
                                    // about to spend acts. Solver-voiced (so teammates'
                                    // inbound carries it — never self-filtered for THEM),
                                    // addressed by name, in the solve room. Fire-and-
                                    // continue: the retry never waits on review (the
                                    // recipe-rule gate is the designed next lap); a
                                    // reviewer's findings land in the room and the
                                    // solver's own multi-room perception folds them in
                                    // mid-retry.
                                    if !inner.teammates.is_empty() && g.patch_bytes > 0 {
                                        if let (Some(room), Some(reg)) = (
                                            inner.room,
                                            crate::persona::airc_runtime_registry::PersonaAircRuntimeRegistry::try_global(),
                                        ) {
                                            if let Some(solver_rt) = uuid::Uuid::parse_str(inner.persona_id.as_str())
                                                .ok()
                                                .and_then(|u| reg.get(u))
                                            {
                                                let mates: Vec<String> = inner
                                                    .teammates
                                                    .iter()
                                                    .filter_map(|m| reg.get(*m))
                                                    .map(|rt| format!("@{}", rt.agent_name()))
                                                    .collect();
                                                if !mates.is_empty() {
                                                    let req = format!(
                                                        "{} (to you, review request): my attempt {attempt} on `{instance}` \
                                                         did not resolve — the grader's held-out tests still failing: {}. \
                                                         My diff is live in this room's workspace (`git diff HEAD` in \
                                                         swe/{instance}/); edited: {}. I am starting attempt {} now — \
                                                         read the diff and SPEAK what you see wrong here, a catch \
                                                         before I resubmit is the whole point of this team.",
                                                        mates.join(" "),
                                                        if g.failed_tests.is_empty() { "(unnamed)".to_string() } else { g.failed_tests.join(", ") },
                                                        if r.files_changed.is_empty() { "(none listed)".to_string() } else { r.files_changed.join(", ") },
                                                        attempt + 1,
                                                    );
                                                    match crate::persona::airc_citizen::publish_text_in_room(
                                                        solver_rt.airc(),
                                                        room,
                                                        &req,
                                                    )
                                                    .await
                                                    {
                                                        Ok(_) => crate::probe!(
                                                            class = "team.review.requested",
                                                            run_id = %run_id,
                                                            instance = %instance,
                                                            attempt,
                                                            mates = %mates.join(","),
                                                            "failed attempt published a solver-voiced review request — the review-landing event"
                                                        ),
                                                        Err(e) => crate::probe!(
                                                            class = "team.review.request_failed",
                                                            run_id = %run_id,
                                                            instance = %instance,
                                                            error = %e.to_string(),
                                                            "review request did not send — retry proceeds solo, loudly"
                                                        ),
                                                    }
                                                }
                                            }
                                        }
                                    }
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
                // A3 — HARNESS-ONLY TERMINAL CLOSE (BENCHMARK-AS-KANBAN: the citizen
                // may move working states; the terminal transition belongs to the
                // harness). The run carries a final verdict on disk (grade.json);
                // closing the card here is what lets observe_card_event settle the
                // round and — on the last card — reach Done. An UNGRADEABLE verdict
                // (env fault) leaves the card OPEN on purpose: a resume re-fires it,
                // which for an env fault is the owed retake, never a burial.
                close_claim_card_if_graded(&run_id).await;
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
