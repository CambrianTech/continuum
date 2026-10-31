use super::*;

/// One deliberation on a forked cycle: deliver `prompt` through the SAME live burst formatter
/// the heartbeat uses, then drive to settlement (directed — an exam question put TO her). The
/// shared motion behind each teammate's turn in `run_pass_team`; mirrors the solo `run_pass`
/// delivery byte-for-byte so a team-vs-solo delta is coordination, not a framing difference.
/// Caller does the exam-hygiene reset/rewind (isolation is per-cycle).
pub(super) async fn eval_settle(
    cycle: &crate::cognition::workspace::WorkspaceCycle,
    room: crate::identity::ActivityRoom,
    prompt: &str,
    max_acts: usize,
) -> crate::cognition::act_observe::SettleOutcome {
    let delivery = crate::cognition::rag_budget::RagDelivery {
        source_id: "airc".to_string(),
        items: vec![crate::cognition::rag_budget::RagItem {
            key: String::new(),
            summary: None,
            pointer: None,
            content: prompt.to_string(),
            tokens: 0,
            metadata: serde_json::json!({ "peer_id": "peer", "occurred_at_ms": EVAL_EPOCH_MS }),
        }],
        tokens_used: 0,
        truncated: false,
    };
    let burst = crate::cognition::workspace::Burst::from_turns(
        room,
        crate::cognition::conversation::workspace_turns::build_workspace_turns(
            std::slice::from_ref(&delivery),
            "",
            "",
            None,
        ),
    );
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
    let mut infra_faults = 0u32;
    let mut infra_reason: Option<String> = None;
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
        let writer_answer = w.spoken.clone().unwrap_or_default();

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
        let answer = r.spoken.clone().unwrap_or_default();

        // Infra fault on EITHER teammate's generation → the lane failed, not a wrong
        // answer. Team mode has no per-task retry loop (the solo run_pass owns bounded
        // recovery); here we classify + ABORT so an infra fault never fake-zeros the team
        // number. Threading the full re-verify+retry into the two-solver loop is a scoped
        // follow-up. [[proctored-exam-session-dependable-benchmark]]
        if let Some(cause) = r
            .failure
            .clone()
            .or_else(|| w.failure.clone())
        {
            infra_faults += 1;
            if infra_reason.is_none() {
                infra_reason = Some(format!("task '{}': {cause}", t.id));
            }
            results.push(EvalTaskResult {
                id: t.id.clone(),
                ok: false,
                grade: format!("settlement infrastructure unavailable: {cause}"),
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
            crate::genome::gym_grader::test_grade_file(
                t.workspace_root
                    .as_deref()
                    .or(workspace_root)
                    .map(std::path::Path::new),
                file,
                t.lang.as_deref().unwrap_or("rust"),
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
        infra_faults,
        infra_reason,
    }
}

// Stateless → self-register onto the ONE registry (descriptor + runtime object).
crate::register_stateless_command!(CognitionEval);
