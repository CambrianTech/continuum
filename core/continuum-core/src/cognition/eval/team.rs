//! Cooperation inside an exam: the reviewer-teammate pass (writer solves, a fresh reviewer
//! produces the final code) with its shared settle motion, and the help arm that lets peers'
//! room speech reach the examinee's perception for the pass.

use super::*;

/// RAII abort for a spawned helper task — dropped with the pass, so the help
/// listener can never outlive the exam it serves.
pub(super) struct AbortOnDrop(tokio::task::JoinHandle<()>);
impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// One deliberation on a forked cycle: deliver `prompt` through the SAME live burst formatter
/// the heartbeat uses, then drive to settlement (directed — an exam question put TO her). The
/// shared motion behind each teammate's turn in `run_pass_team`; mirrors the solo `run_pass`
/// delivery byte-for-byte so a team-vs-solo delta is coordination, not a framing difference.
/// Caller does the exam-hygiene reset/rewind (isolation is per-cycle).
async fn eval_settle(
    cycle: &crate::cognition::workspace::WorkspaceCycle,
    room: crate::identity::ActivityRoom,
    prompt: &str,
    max_acts: usize,
) -> crate::cognition::act_observe::SettleOutcome {
    let burst = exam_burst(room, prompt);
    crate::cognition::act_observe::drive_to_settle(
        cycle,
        burst,
        max_acts,
        crate::cognition::workspace::TurnFraming::directed(),
    )
    .await
}

/// TEAM pass — the Continuum's molecule: two forks of the SAME persona (same base model) as
/// WRITER + REVIEWER. The writer solves each task; a FRESH reviewer (clean context — a genuine
/// second set of eyes, which is exactly what a solo who can't catch its own bug lacks) reviews
/// the writer's solution, finds defects, and produces the FINAL code. The reviewer's answer is
/// graded — same tasks, same grader, same model as the solo `run_pass`, so any team-vs-solo
/// delta is PURE coordination value, not model-fit. This is the undeniable proof of teams:
/// hold everything fixed but add a teammate. [[coordination-learning-flywheel]]
pub(super) async fn run_pass_team(
    writer: &crate::cognition::workspace::WorkspaceCycle,
    writer_iso: &crate::cognition::workspace::EvalIsolation,
    reviewer: &crate::cognition::workspace::WorkspaceCycle,
    reviewer_iso: &crate::cognition::workspace::EvalIsolation,
    tasks: &[EvalTask],
    room: crate::identity::ActivityRoom,
    max_acts: usize,
    // The run's resolved eval root (explicit pin or #312 ephemeral clone) —
    // artifact grading resolves solution files against it, same as solo.
    workspace_root: Option<&str>,
) -> PassOutcome {
    let mut pass = 0u32;
    let mut results = Vec::with_capacity(tasks.len());
    let mut infra: Option<InfraFaults> = None;
    // Natural proctored exam, team edition: both teammates sit down ONCE (rewind to the
    // pre-eval frame at pass start) and then work the sheet CONTINUOUSLY — the writer carries
    // what she learned on earlier tasks; the reviewer builds a sense of the writer's habits.
    // That accumulated familiarity IS part of what a team is. Same rationale as run_pass —
    // [[benchmarks-are-proctored-exams-of-the-natural-living-persona]].
    writer.reset_working_memory();
    writer_iso.rewind();
    reviewer.reset_working_memory();
    reviewer_iso.rewind();
    for t in tasks {
        // WRITER solves (same delivery as solo run_pass).
        let w = eval_settle(writer, room, &t.prompt, max_acts).await;
        let writer_answer = w.spoken_answer();

        // REVIEWER reviews the writer's solution and produces the FINAL code.
        // The reviewer's job is QUALITY CONTROL, not rewriting. The failure mode a glass-box
        // trace exposed: the reviewer eyeballs (acts=0), narrates a review, and "optimizes"
        // WORKING code — regressing tasks the writer got right. So the mandate is explicit:
        // verify by ACTUALLY compiling/running; leave passing code UNTOUCHED; change only a
        // proven defect; output only code. This is the coordination discipline a good human
        // reviewer has (don't break what works), made explicit for the model.
        let review_prompt = format!(
            "You are the QUALITY-CONTROL reviewer on a teammate's solution below — not a \
             rewriter. FIRST, actually COMPILE AND RUN it against the task using your tools \
             (do not just read it). If it compiles and passes, return it EXACTLY as-is — do \
             NOT optimize, refactor, or 'improve' working code; that only introduces bugs. \
             ONLY if it fails to compile or a case is wrong, fix the SPECIFIC defect and \
             nothing else. Output ONLY the final complete code — no commentary.\n\n\
             TASK:\n{}\n\nTEAMMATE'S SOLUTION:\n{}",
            t.prompt, writer_answer
        );
        let r = eval_settle(reviewer, room, &review_prompt, max_acts).await;
        let answer = r.spoken_answer();

        // Infra fault on EITHER teammate's generation → the lane failed, not a wrong
        // answer. Team mode has no per-task retry loop (the solo run_pass owns bounded
        // recovery); here we classify + ABORT so an infra fault never fake-zeros the team
        // number. Threading the full re-verify+retry into the two-solver loop is a scoped
        // follow-up. [[proctored-exam-session-dependable-benchmark]]
        if let Some(cause) = r
            .inference_error
            .clone()
            .or_else(|| w.inference_error.clone())
        {
            InfraFaults::record(&mut infra, format!("task '{}': {cause}", t.id));
            results.push(EvalTaskResult {
                id: t.id.clone(),
                ok: false,
                grade: format!("infra unavailable (lane never recovered): {cause}"),
                acts: 0,
                answer: String::new(),
                latency_ms: 0,
                output_tokens: 0,
                tokens_per_second: 0.0,
                decode_tokens_per_second: 0.0,
                cache_hit_rate: 0.0,
                prefill_ms: 0,
                decode_ms: 0,
            });
            tracing::warn!(
                probe_class = "eval.run.infra_unavailable",
                task = %t.id,
                attempted = results.len(),
                cause = %cause,
                "aborting team run as InfraUnavailable — serving lane failed mid-exam"
            );
            break;
        }
        // Grade the team's FINAL deliverable — SAME branches as solo run_pass, including the
        // rule that code is graded on the file their hands produced. A team that TALKS its way
        // to a beautiful spoken solution and writes nothing has shipped nothing; two personas
        // narrating at each other is the exact failure this arm must not reward (and the exact
        // one a review/handoff loop makes MORE likely, not less).
        let (ok, grade) = if let Some(dod) = &t.dod_shell {
            let v = run_workspace_grade(
                t.workspace_root.as_deref().or(workspace_root).map(std::path::Path::new),
                t,
                dod,
            )
            .await;
            (v.passed(), v.message().to_string())
        } else if let (Some(file), Some(test)) = (&t.solution_file, &t.test) {
            crate::cognition::gym_grader::test_grade_file(
                t.workspace_root
                    .as_deref()
                    .or(workspace_root)
                    .map(std::path::Path::new),
                file,
                t.language(),
                test,
            )
            .await
        } else if t.test.is_some() {
            (
                false,
                "harness defect: a code task reached team grading without a solution_file — \
                 require_hands_for_code normalizes every test-graded task at load."
                    .to_string(),
            )
        } else {
            substring_or_silence_grade(t, &answer)
        };
        if ok {
            pass += 1;
        }
        let acts = w.acts as u32 + r.acts as u32;
        let m = r.metrics;
        results.push(EvalTaskResult {
            id: t.id.clone(),
            ok,
            grade,
            acts,
            answer: answer.chars().take(ANSWER_CAPTURE_CHARS).collect(),
            latency_ms: m.latency_ms,
            output_tokens: m.output_tokens,
            tokens_per_second: m.tokens_per_second(),
            decode_tokens_per_second: m.decode_tokens_per_second(),
            cache_hit_rate: m.cache_hit_rate(),
            prefill_ms: m.prefill_ms,
            decode_ms: m.decode_ms,
        });
        report_task_graded(
            &t.id,
            ok,
            acts,
            m.latency_ms,
            m.output_tokens,
            pass,
            results.len(),
            tasks.len(),
        );
    }
    PassOutcome {
        pass,
        results,
        infra,
    }
}

/// HELP ARM: subscribe the fork's perception to live room speech for the
/// exam's duration. Peers' words land in her working memory as labeled
/// voices (perception through a channel, never a memory write from
/// outside); her own posts are skipped. Guard aborts with the pass.
/// Every exam has a REAL activity room now (#425), so the old nil guard is
/// gone — the help arm is purely the flag.
pub(super) fn spawn_help_arm(
    cycle: &crate::cognition::workspace::WorkspaceCycle,
    room: crate::identity::ActivityRoom,
    persona_uuid: Uuid,
) -> Option<AbortOnDrop> {
    let wm = cycle.acting().map(|b| std::sync::Arc::clone(&b.working_memory));
    wm.map(|wm| {
        let mut rx = crate::cognition::deliberation_budget::subscribe_room_speech();
        let me = persona_uuid;
        let handle = tokio::spawn(async move {
            loop {
                match rx.recv().await {
                    Ok(sp) => {
                        if sp.room != room.as_uuid() || sp.sender == Some(me) {
                            continue;
                        }
                        let who = sp
                            .sender
                            .map(|u| u.to_string()[..8].to_string())
                            .unwrap_or_else(|| "peer".into()); // legacy senderless call site: an anonymous voice, labeled as such
                        wm.record_dispatch_event(
                            Uuid::new_v4(),
                            &format!("voice:{who}"),
                            &sp.content,
                            crate::cognition::working_memory::DispatchStatus::Done,
                        );
                        crate::probe!(
                            class = "eval.help.voice_folded",
                            room = %room,
                            sender = %who,
                            "help arm: a peer's words reached the examinee's perception"
                        );
                    }
                    Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }
        });
        AbortOnDrop(handle)
    })
}
