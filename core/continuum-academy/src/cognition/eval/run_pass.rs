use super::*;

pub(super) async fn run_pass(
    cycle: &crate::cognition::workspace::WorkspaceCycle,
    isolation: &crate::cognition::workspace::EvalIsolation,
    tasks: &[EvalTask],
    room: crate::identity::ActivityRoom,
    max_acts: usize,
    max_retries: u32,
    // The pinned workspace root the persona's hands were rooted at (from-scratch build /
    // SWE-bench). UI grading MUST observe the artifact HERE, where she wrote it — not the
    // core cwd — or a correct render scores a false zero (#49 dual-root gap). `None` = the
    // hands used the default root (core cwd) and grading follows.
    workspace_root: Option<&str>,
    // Per-task lesson streaming (learn mode): `Some` = teach each graded task
    // at grade time. `None` = A/B arms and non-learning runs (an A/B measures
    // the gene, never teaches — adopting is a separate decision).
    lesson_sink: Option<&LessonSink>,
) -> PassOutcome {
    let mut pass = 0u32;
    let mut results = Vec::with_capacity(tasks.len());
    let mut infra_faults = 0u32;
    let mut infra_reason: Option<String> = None;
    // THE NATURAL PROCTORED EXAM (Joel: "we use our own persona as naturally as possible, as
    // if these are proctored exams... natural personas even in tests, with memories intact").
    // She sits down ONCE: rewind to the pre-eval frame at PASS start (each A/B arm still
    // starts from the identical memory — arm fairness preserved), clear the volatile
    // mid-thought scratch (she walks into the exam room attending to the exam), and then
    // she works the whole task sheet CONTINUOUSLY — carrying what she learns on task 3 into
    // task 4, exactly like a real student. The old per-task reset ("exam hygiene") measured
    // a rigged amnesiac who could never accumulate — the very cognition+learning the thesis
    // claims helps was structurally excluded from the measurement. If cross-task memory
    // helps, that's the system WORKING; if it distracts, that's a real cognition gap the
    // honest number should show. The exam EPISODE is dropped afterward (the fork is
    // discarded; `AdmissionState::forget_context` is the amnesia flash for live-self runs),
    // so the answer key never leaks into training.
    // [[benchmarks-are-proctored-exams-of-the-natural-living-persona]]
    cycle.reset_working_memory();
    isolation.rewind();
    for t in tasks {
        // Task-start receipt: the health verdicts downstream (session gates, the
        // bench view) judge a running task against the round's OWN completed-task
        // latencies — which needs an exact start signal, not an inference from
        // the previous grade's absence (Joel 2026-08-23: a stalled task must be
        // known in minutes, adaptively, "better fix" than a fixed window).
        continuum_base::probe!(
            class = "eval.task.started",
            task = %t.id,
            "task drive begins — elapsed-vs-round-median verdicts key on this row"
        );
        // #209 — per-task CLEAN WORKSPACE for from-scratch build gyms. A from-scratch build
        // task (ui_checks present, and NO setup/dod/solution that establishes or depends on
        // prior repo state) is graded by observing the artifact the persona just wrote into
        // the pinned workspace_root. If the PREVIOUS task's files still sit there, this task's
        // grade observes a stale/foreign artifact — the exact cross-task contamination that
        // scored webdev 0/6 while she built a correct page each time (harness noise, not a
        // capability miss). Wipe the pinned root's CONTENTS (not the dir itself — her hands are
        // rooted at it) so every task grades EXACTLY the artifact it produced. Only from-scratch
        // build tasks are cleaned; setup/dod/solution tasks OWN their state and are never wiped.
        // No pinned root (hands on the core cwd) → nothing to clean.
        // INDEPENDENT-TASK COGNITION RESET. Every coder benchmark here is a battery of
        // UNRELATED tasks (conway_step vs edit_distance vs a login-form) — each a standalone
        // problem. The continuous-session design (reset once at pass start, carry task-to-task)
        // is right for a RELATED sequence, but for an independent battery it accumulates each
        // prior task's answer into working memory + episodic recall, and that drift both
        // overflows a small lane window (→ per-slot compute-error wedge → empty 0-token
        // generations) AND nudges the model into declining (glass-boxed 2026-07-21: games-rs
        // 6/6 in isolation but 0/6 with mid-battery declines under accumulation). A task starts
        // clean UNLESS it explicitly establishes cross-task state via `setup_shell` (gym/mine's
        // re-break-the-checkout case — the only genuine continuity). So reset the cognitive
        // slate before every independent task, exactly as #209 wipes an independent build's
        // WORKSPACE. [[llama-compute-error-wedge-is-per-slot-context-overflow]]
        let independent_task = t.setup_shell.is_none();
        if independent_task {
            cycle.reset_working_memory();
            isolation.rewind();
        }
        // Workspace wipe: only a from-scratch UI build (ui_checks, no setup/dod/solution) grades
        // an OBSERVED artifact FILE, so only it needs the pinned root cleared between tasks
        // (#209) — a stale index.html from the prior task would score a false pass/fail.
        let from_scratch_build = !t.ui_checks.is_empty()
            && t.setup_shell.is_none()
            && t.dod_shell.is_none()
            && t.solution_file.is_none();
        if from_scratch_build {
            if let Some(root) = t.workspace_root.as_deref().or(workspace_root) {
                if let Err(e) = clean_dir_contents(root) {
                    tracing::warn!(
                        probe_class = "eval.task.clean_workspace",
                        root = root,
                        error = %e,
                        "failed to clean the pinned workspace before a from-scratch build task \
                         — grading may observe a stale artifact (#209)"
                    );
                }
            }
        }
        // PER-TASK ROOT (mined gyms) — DECLARED, and REFUSED when it would split her in half.
        //
        // Each mined task owns its own git worktree, so the root is a property of the TASK. The
        // obvious move is to re-root her here. Do NOT: rooting a persona is TWO operations, and
        // only one of them can run at this point.
        //   * `root_acting_workspace` moves where her hands WRITE — callable any time.
        //   * `repoint_workspace_map_if_pinned` moves what her eyes SEE — mutates
        //     `PersonaBrainConfig`, so it runs only at FORK time; `WorkspaceCycle.faculties` is
        //     immutable after build.
        // Moving only the hands gives her a map of one tree and a file engine in another — the
        // #206 shape, where she reasons over a layout that isn't hers and explores instead of
        // building. Glass-boxed 2026-08-05: the re-root probe fired with the right checkout while
        // her workspace-map, 4.8s later, still described the previous root. It LOOKED fixed.
        //
        // Half-rooted is worse than not rooted, because the resulting zero is a lie about the
        // solver that no probe reports. So refuse, and name the working path: the RUN-level
        // `workspace_root` does BOTH halves at fork time, so ONE EVAL PER ROOT is correct today.
        // [[fallbacks-are-illegal-fail-loud]]
        // [[re-rooting-a-persona-is-two-operations-moving-one-is-worse-than-moving-neither]]
        //
        // The real fix (then this refusal goes away): her eyes read the root FROM her hands —
        // one source of truth — the way `CitizenLayerWorkspaceLayoutReader` already calls the
        // same `ensure_citizen_layer` the hands call, "so map and tools can never again describe
        // different worlds".
        let task_root: Option<&str> = t.workspace_root.as_deref().or(workspace_root);
        if let Some(want) = t.workspace_root.as_deref() {
            if Some(want) != workspace_root {
                results.push(EvalTaskResult {
                    id: t.id.clone(),
                    ok: false,
                    grade: format!(
                        "infra: task '{}' declares its own workspace_root '{want}' but this run \
                         was forked at {:?}. Re-rooting mid-run would move her HANDS without \
                         moving her EYES (the workspace-map is baked at fork), so she would be \
                         graded while reading a tree she is not working in. Run one eval per \
                         root: pass --workspace_root='{want}' with just this task.",
                        t.id, workspace_root
                    ),
                    answer: String::new(),
                    acts: 0,
                    latency_ms: 0,
                    output_tokens: 0,
                    tokens_per_second: 0.0,
                    decode_tokens_per_second: 0.0,
                    cache_hit_rate: 0.0,
                    prefill_ms: 0,
                    decode_ms: 0,
                });
                continue;
            }
        }
        // Task-state SETUP (gym/mine tasks re-break their checkout so runs are
        // repeatable). A failed setup is a NAMED infra grade — the persona is
        // never examined against a workspace in an unknown state.
        if let Some(setup) = &t.setup_shell {
            let setup_v = run_dod(task_root.map(std::path::Path::new), setup).await;
            let setup_ok = setup_v.passed();
            let setup_out = setup_v.message().to_string();
            if !setup_ok {
                results.push(EvalTaskResult {
                    id: t.id.clone(),
                    ok: false,
                    grade: format!("setup failed (infra, not capability): {setup_out}"),
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
                continue;
            }
        }
        // Frame the task through the SAME burst formatter the live heartbeat uses
        // (service_loop::build_workspace_burst), as a single airc room message
        // from a peer — so her deliberation perceives an examiner's question with
        // the byte-identical envelope (`[room <id>]\n[t=<ms>] peer: <prompt>`) a
        // real peer post produces, instead of an eval-only `[eval]\npeer:` shape.
        // The grounding tier (recall/roster/doctrine) is already supplied by the
        // fork's faculties; we compose only the message delivery here. Routing
        // through the full compose_for_turn would DOUBLE-inject that grounding
        // (the open #8 broadcast==RAG-context convergence) and read the LIVE
        // (non-forked) memory — so it stays out until #8 lands. Then DRIVE her to
        // settlement: she may act (run code, read a file, search), observe the
        // result as memory, and re-perceive — the live act→observe motion, paced
        // by the grader because the eval room has no metronome.
        // Pose the task as an EXAMINER'S REQUEST TO HER, not a bare imperative. The
        // [Directed Presence] block answers "a task → answer it now" only
        // when the model RECOGNIZES the message as asking something OF IT; a terse gym
        // imperative ("Implement `fn eval`…") reads to a coder model as ambient text
        // "about" a task, so it takes the pleasantry-rest PASS hatch and scores 0
        // WITHOUT ATTEMPTING (glass-boxed 2026-07-19: hard-rs settled to Decision::Pass
        // twice with the task fully in view; humaneval — whose prompt already says
        // "Give your final answer as…" — engaged and scored 10/10). `directed=true`
        // alone no longer withholds the hatch since the directed block was softened for
        // live spiral-breaking, so the examiner must pose the ask itself. This is the
        // examiner doing its job — stating the question clearly so the exam measures
        // CAPABILITY, not whether the model happened to read a terse line as directed —
        // and it standardizes at ONE seam the imperative framing the stronger gym prompts
        // already carry inline. Verbatim task preserved; only a directed preamble added.
        let framed_prompt = format!(
            "This is a task for you to complete now. Provide your complete solution:\n\n{}",
            t.prompt.trim()
        );
        let task_delivery = crate::cognition::rag_budget::RagDelivery {
            source_id: "airc".to_string(),
            items: vec![crate::cognition::rag_budget::RagItem {
                key: String::new(),
                summary: None,
                pointer: None,
                content: framed_prompt,
                tokens: 0,
                metadata: serde_json::json!({
                    "peer_id": "peer",
                    "occurred_at_ms": EVAL_EPOCH_MS,
                }),
            }],
            tokens_used: 0,
            truncated: false,
        };
        // own_peer/agent_name attribute the persona's OWN past posts; a single-task
        // exam has none, so they're inert here (the item's peer_id "peer" ≠ "").
        // A CLOSURE, not a value: an infra-faulted task is retried (below), and
        // `drive_to_settle` consumes the burst, so each attempt mints a fresh one.
        let make_burst = || {
            crate::cognition::workspace::Burst::from_turns(
                room,
                crate::cognition::conversation::workspace_turns::build_workspace_turns(
                    std::slice::from_ref(&task_delivery),
                    "",
                    "",
                    // A single-task exam IS the stimulus — the task delivery is the
                    // whole thread; there is no out-of-band trigger to anchor.
                    None,
                ),
            )
        };
        // `directed = true`: an exam question is put TO the persona — it is not
        // ambient room chatter she may let pass. This withholds the bare-PASS silence
        // escape (the [Conversational Presence] block) for the eval turn, the same kind of
        // measurement control as the greedy-temperature pin: it isolates the coding
        // *capability* signal from the *participation* decision. Without it a coder
        // model takes the "reply PASS, nothing reaches the room" exit on a directed
        // question (reproduced via glass-box replay — 0/13 on the gym). She can still
        // decline in her own words; she just isn't handed the silent hatch. The live
        // path computes directedness from real addressing (TODO #9); pinning it here
        // is the eval's exam-is-directed control. See `Workspace::directed_at_self`.
        // LAST-RESORT hang backstop — NOT a wedge detector (2026-08-23, second cut).
        //
        // Wedge detection is the SERVING layer's job and it is event-based there: the
        // stream-liveness classifier (phase-aware budgets, 9e2f95820) kills a
        // zero-token stream at the source and the failure surfaces here as
        // `failure` — the event path `drive_to_settle` already returns on.
        // The old flat 600s deadline predated that sensor (#123, 2026-07-17) and was
        // never demoted once it shipped: it second-guessed the serving layer with a
        // stopwatch, declared a LIVE multi-minute e2e decode "wedged" (fluid-sim-e2e,
        // observed live tonight), and retried the task into a doomed loop. Cheap
        // wall-clock guesses about liveness always lose to the component that can SEE
        // the tokens ([[the-whole-system-is-event-based-not-polling]]).
        //
        // What remains is the hang class no event can cover BY DEFINITION: an await
        // that never returns because some path between inference calls lost its error
        // propagation (a code bug, not a serving state). For that, one enormous
        // garbage-ceiling timer — and its firing is a SUBSTRATE BUG REPORT (the probe
        // says which class to hunt), never routine control flow.
        // PROGRESS-AWARE (2026-08-24, third cut — fired the same day on a task
        // PROVABLY working: 82k prompt, 13.7k tokens decoded that very turn, acts
        // landing all afternoon; the flat 2h wall-clock killed it and the retry
        // re-ran the whole task into the same cliff — the mirrorcode-bitwise
        // double-death, reproduced on TB. Wall clock is not evidence; the lane's
        // own work stamps are). The backstop now measures IDLE: it fires only
        // when NO real prefill/decode progress has been stamped for its whole
        // window — the one hang class no event covers (a lost await between
        // inference calls). A slow task that keeps decoding can never trip it;
        // the act budget (max_acts) remains the legitimate bound on endless work.
        const PER_TASK_IDLE_BACKSTOP: std::time::Duration =
            std::time::Duration::from_secs(20 * 60);
        // BOUNDED INFRA-FAULT RECOVERY (Proctored Exam Session, Slice B). Drive to
        // settlement; if the model call itself FAILS (`failure`: lane not-ready /
        // connect-refused / "not the active served model" / compute-error / deadline-wedge)
        // that is an INFRA fault, never a wrong answer. Re-verify the lane and retry the
        // SAME task — the retried generation is itself the real decode proof — up to
        // INFRA_FAULT_RETRIES. A transient bounce (grow-back relaunch, brief not-ready)
        // recovers here and the task grades normally. Exhaustion means the lane is dead:
        // `settled` still carries the error and the abort below marks the run
        // InfraUnavailable, so a phantom generation on a dead lane can NEVER be published
        // as `pass_rate: 0.0`. [[proctored-exam-session-dependable-benchmark]]
        let mut settled;
        let mut infra_attempt = 0u32;
        loop {
            let drive = crate::cognition::act_observe::drive_to_settle(
                cycle,
                make_burst(),
                t.max_acts.map(|v| v as usize).unwrap_or(max_acts), // None = row sets no budget; the run's budget is the documented inherit
                if t.silence {
                    // A silence-graded task is AMBIENT by definition: the room moved and
                    // nothing addressed her. Posed as directed, a correct answer to an
                    // addressed line would be graded as a failure to keep quiet.
                    crate::cognition::workspace::TurnFraming::ambient()
                } else if t.workspace_deliverable() {
                    crate::cognition::workspace::TurnFraming::directed().on_workspace()
                } else {
                    crate::cognition::workspace::TurnFraming::directed()
                },
            );
            tokio::pin!(drive);
            // Idle supervision: wake every minute; a stamp of real lane work
            // (prefill advance OR decode) inside the window resets patience.
            let mut idle = std::time::Duration::ZERO;
            settled = loop {
                match tokio::time::timeout(std::time::Duration::from_secs(60), &mut drive).await {
                    Ok(s) => break s,
                    Err(_) => {
                        let worked_recently = crate::inference::llama_server::ms_since_real_work()
                            .is_some_and(|ms| ms < 60_000);
                        if worked_recently {
                            idle = std::time::Duration::ZERO;
                        } else {
                            idle += std::time::Duration::from_secs(60);
                        }
                        if idle < PER_TASK_IDLE_BACKSTOP {
                            continue;
                        }
                        break hang_backstop_outcome(&t.id, room, PER_TASK_IDLE_BACKSTOP);
                    }
                }
            };
            let Some(cause) = settled.failure.clone() else {
                break; // reached a verdict (answer OR graded-wrong) on a working lane
            };
            // Re-verifying an inference lane cannot repair checkpoint custody.
            if matches!(cause, crate::cognition::act_observe::SettlementFailure::ReceiptPersistence(_)) {
                break;
            }
            infra_attempt += 1;
            if infra_attempt > INFRA_FAULT_RETRIES {
                break;
            }
            tracing::warn!(
                probe_class = "eval.task.infra_fault",
                task = %t.id,
                attempt = infra_attempt,
                cause = %cause,
                "inference infra fault (not a wrong answer) — re-verifying the lane and retrying the task"
            );
            // Re-verify the lane is snapshot-ready before spending the retry. If it never
            // returns to ready within the budget the next attempt simply infra-faults again
            // and exhausts the retries → InfraUnavailable, never a phantom score.
            let _ = reverify_lane().await;
        }
        // UNRECOVERABLE infra fault: the lane never returned to a decode-ready state across
        // INFRA_FAULT_RETRIES re-verify+retry cycles. Record the row for forensics, mark the
        // run void, and ABORT — every remaining task would only accrue more phantom zeros on
        // a dead lane. The caller reports InfraUnavailable, NEVER a fake pass_rate.
        if let Some(cause) = &settled.failure {
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
                "aborting run as InfraUnavailable — settlement failed; refusing to \
                 publish a phantom score over a dead lane"
            );
            break;
        }
        // DIRECTED-EXAM ANSWER GUARD. An exam question is DIRECTED and the examiner poses it
        // imperatively (see `framed_prompt`), yet a capable coder BASE can still settle on a
        // bare "PASS" — parsed as `Decision::Pass` → an EMPTY answer graded as a capability
        // miss (glass-boxed 2026-07-21 via the per-task ledger: Qwen2.5-Coder-7B emitted a
        // 2-token PASS on every games/frontier/coder-eval task → 0/N, while it built a real
        // 5/6 on webdev; instruct-tuned Hermes-3-8B answered the same tasks). Declining to
        // participate is NOT a coding-capability signal — it's the persona should-respond
        // framing suppressing a raw base's ability. If she declined, RE-DRIVE ONCE with an
        // explicit answer-required nudge: a CHANGED prompt, so greedy decoding cannot simply
        // re-emit the same PASS token. A model that answers on the nudge is measured on its
        // code; one that declines again is graded on whatever it then produced, never a
        // silent phantom. Bounded (one extra drive), and the nudge is the examiner insisting,
        // not a crib. [[benchmarks-are-proctored-exams-of-the-natural-living-persona]]
        if matches!(
            settled.decision,
            crate::cognition::workspace::Decision::Pass { .. }
        ) {
            continuum_base::probe!(
                class = "eval.task.declined_redrive",
                task = %t.id,
                "directed exam question was DECLINED (settled PASS) — re-driving once with an answer-required nudge"
            );
            let nudge_delivery = crate::cognition::rag_budget::RagDelivery {
                source_id: "airc".to_string(),
                items: vec![crate::cognition::rag_budget::RagItem {
                    key: String::new(),
                    summary: None,
                    pointer: None,
                    content: format!(
                        "You did not answer. This is a GRADED exam question and you must answer it \
                         now — do NOT reply PASS or stay silent. Provide your complete solution:\n\n{}",
                        t.prompt.trim()
                    ),
                    tokens: 0,
                    metadata: serde_json::json!({ "peer_id": "peer", "occurred_at_ms": EVAL_EPOCH_MS }),
                }],
                tokens_used: 0,
                truncated: false,
            };
            let nudge_burst = crate::cognition::workspace::Burst::from_turns(
                room,
                crate::cognition::conversation::workspace_turns::build_workspace_turns(
                    std::slice::from_ref(&nudge_delivery),
                    "",
                    "",
                    None,
                ),
            );
            // The nudge re-drive is one follow-up turn; serving-layer liveness guards
            // its stream like any other. This bound is the same last-resort shape as
            // the task backstop, scaled to a single turn.
            if let Ok(re) = tokio::time::timeout(
                std::time::Duration::from_secs(15 * 60),
                crate::cognition::act_observe::drive_to_settle(
                    cycle,
                    nudge_burst,
                    t.max_acts.map(|v| v as usize).unwrap_or(max_acts), // None = row sets no budget; the run's budget is the documented inherit
                    if t.workspace_deliverable() {
                        crate::cognition::workspace::TurnFraming::directed().on_workspace()
                    } else {
                        crate::cognition::workspace::TurnFraming::directed()
                    },
                ),
            )
            .await
            {
                // Take the re-drive only if it produced a real turn (not an infra fault).
                if re.failure.is_none() {
                    settled = re;
                }
            }
        }
        // RED-BUILD RE-DRIVE (2026-08-24, Joel: "let's give her what she
        // needs"). The empty-diff/declined re-drives cover a settle with NO
        // work; this covers a settle with BROKEN work — glass-boxed on
        // bib2json the night it shipped: 621 lines, real iteration, settled
        // on a build with 9 compile errors and graded 0 with budget she never
        // knew she needed. Her own pre-submit CI: at settle on a DoD task,
        // run the DoD once; if red, hand her its OUTPUT (the compiler's own
        // words — ground truth, not steering) and one bounded re-drive to
        // fix. A green DoD here is also the grade (no double run below).
        let mut presettle_dod: Option<(bool, String)> = None;
        // An infra-classed DoD (the grader could not RUN) short-circuits the whole
        // verify loop to a NON-scored infra fault — never a re-drive (there is
        // nothing for her to fix) and never a taught miss.
        let mut dod_infra: Option<String> = None;
        // VERIFY LOOP, not a single retry (2026-08-24, the score-lever pass): while
        // budget remains, a DoD task may not settle red. Each iteration re-runs the
        // task's OWN definition-of-done and hands her its output — ground truth she
        // could produce herself (the DoD is part of the task, so no answer-key
        // leak; artifact-test tasks stay excluded for exactly that reason). Bounded
        // at 2 re-drives: the third red verdict grades as the miss it is.
        const DOD_REDRIVES: u32 = 2;
        let mut redrive_round = 0u32;
        if let Some(dod) = &t.dod_shell {
            loop {
                let verdict = run_dod(task_root.map(std::path::Path::new), dod).await;
                let (dod_ok, dod_out) = match verdict {
                    DodVerdict::Pass(m) => (true, m),
                    DodVerdict::Fail(m) => (false, m),
                    DodVerdict::InfraError(m) => {
                        // The grader itself broke — do NOT re-drive (nothing to fix) and
                        // do NOT score as a miss; mark it infra and leave the loop.
                        continuum_base::probe!(
                            class = "eval.task.dod_infra",
                            task = %t.id,
                            "definition-of-done could not RUN — harness/infra fault, not scored as a model miss"
                        );
                        dod_infra = Some(m.clone());
                        presettle_dod = Some((false, m));
                        break;
                    }
                };
                if dod_ok || redrive_round >= DOD_REDRIVES {
                    // The held-out check grades the settled work once and never re-drives:
                    // handing her its verdict would teach the rule it is there to measure.
                    let settled_verdict = if dod_ok {
                        run_held_out(task_root.map(std::path::Path::new), t, DodVerdict::Pass(dod_out)).await
                    } else {
                        DodVerdict::Fail(dod_out)
                    };
                    if let DodVerdict::InfraError(m) = &settled_verdict {
                        dod_infra = Some(m.clone());
                    }
                    presettle_dod = Some((settled_verdict.passed(), settled_verdict.message().to_string()));
                    break;
                }
                redrive_round += 1;
                {
                    continuum_base::probe!(
                        class = "eval.task.red_build_redrive",
                        task = %t.id,
                        round = redrive_round as u64,
                        "settled on a RED DoD — handing her the verdict with a bounded re-drive"
                    );
                    let red_delivery = crate::cognition::rag_budget::RagDelivery {
                    source_id: "airc".to_string(),
                    items: vec![crate::cognition::rag_budget::RagItem {
                        key: String::new(),
                        summary: None,
                        pointer: None,
                        content: format!(
                            "You settled, but the task's definition-of-done currently FAILS.                              Its output is:

{dod_out}

Fix the workspace and finish —                              the grade reads the files, and a red build scores zero."
                        ),
                        tokens: 0,
                        metadata: serde_json::json!({ "peer_id": "peer", "occurred_at_ms": EVAL_EPOCH_MS }),
                    }],
                    tokens_used: 0,
                    truncated: false,
                };
                let red_burst = crate::cognition::workspace::Burst::from_turns(
                    room,
                    crate::cognition::conversation::workspace_turns::build_workspace_turns(
                        std::slice::from_ref(&red_delivery),
                        "",
                        "",
                        None,
                    ),
                );
                if let Ok(re) = tokio::time::timeout(
                    std::time::Duration::from_secs(15 * 60),
                    crate::cognition::act_observe::drive_to_settle(
                        cycle,
                        red_burst,
                        t.max_acts.map(|v| v as usize).unwrap_or(max_acts), // None = row sets no budget; the run's budget is the documented inherit
                        crate::cognition::workspace::TurnFraming::directed().on_workspace(),
                    ),
                )
                .await
                {
                    if re.failure.is_none() {
                        settled = re;
                    }
                }
                // Loop re-runs the DoD fresh; green (or exhausted rounds) exits above.
            }
            }
        }
        let answer = settled.spoken.clone().unwrap_or_default();
        // DoD-INFRA GATE (2026-08-25): the grader itself could not run (missing
        // workspace, refused/not-found command). This is NOT a model miss — record it
        // as an infra fault (which flips the whole run to InfraUnavailable, so no
        // phantom score is published) and SKIP grading + teaching. Same protection the
        // gold gate gives, applied to the DoD's own execution — for every dod-graded
        // activity, benchmarks and real projects alike.
        if let Some(reason) = dod_infra {
            infra_faults += 1;
            if infra_reason.is_none() {
                infra_reason = Some(reason.clone());
            }
            continuum_base::probe!(
                class = "eval.task.infra_fault",
                task = %t.id,
                cause = %reason,
                "definition-of-done could not run — task marked infra, not a scored miss, not taught"
            );
            results.push(EvalTaskResult {
                id: t.id.clone(),
                ok: false,
                grade: format!("infra (DoD could not run, NOT a wrong answer): {reason}"),
                acts: settled.acts as u32,
                answer,
                latency_ms: settled.metrics.latency_ms,
                output_tokens: settled.metrics.output_tokens,
                tokens_per_second: settled.metrics.tokens_per_second(),
                decode_tokens_per_second: settled.metrics.decode_tokens_per_second(),
                cache_hit_rate: settled.metrics.cache_hit_rate(),
                prefill_ms: settled.metrics.prefill_ms,
                decode_ms: settled.metrics.decode_ms,
            });
            continue; // no grade, NO lesson — a harness fault teaches nothing
        }
        // `settled.failure` is None here — the abort above consumed every infra
        // fault, so this grades a REAL verdict (a working lane produced an answer, right or
        // wrong). No `failure` arm remains: an infra fault can no longer masquerade
        // as a graded miss.
        let (ok, grade) = if !t.ui_checks.is_empty() {
            // FUNCTIONAL WEB-DEV: grade what her UI ACTUALLY RENDERED. Observe `target` through
            // her own eyes (the eye-node path) and score the element tree — the money signal for
            // "can a persona build a UI that works", judged on the structure a non-visual model
            // reads too. Default target `index.html`; default threshold 1.0 ("it works").
            let target = t.target.as_deref().unwrap_or("index.html");
            let threshold = t.ui_pass_threshold.unwrap_or(1.0);
            perception_grade(cycle, target, &t.ui_checks, threshold, task_root, &answer).await
        } else if let (Some(file), Some(test)) = (&t.solution_file, &t.test) {
            // ARTIFACT-graded: she was told to WRITE her solution to `file` and verify it with her
            // own tools. Grade her HANDS (the file she wrote + compiled), not her MOUTH (spoken
            // text) — the only way the act→verify loop shows up in the score. Same harness as
            // test_grade (strip her main, append test, compile, run).
            let lang = t.lang.as_deref().unwrap_or("rust");
            crate::genome::gym_grader::test_grade_file(
                task_root.map(std::path::Path::new),
                file,
                lang,
                test,
            )
            .await
        } else if let Some(dod) = &t.dod_shell {
            // REAL task: run the definition-of-done against the repo state her edits produced.
            match presettle_dod.take() {
                // The verify loop's LAST DoD run IS the grade — green or exhausted-red,
                // never a double run.
                Some(v) => v,
                // Unreachable for dod tasks (the loop always sets it); the honest
                // fallback re-runs and folds any infra classification into the pair.
                None => {
                    let v = run_workspace_grade(task_root.map(std::path::Path::new), t, dod).await;
                    (v.passed(), v.message().to_string())
                }
            }
        } else if t.test.is_some() {
            // UNREACHABLE by construction: `require_hands_for_code` gives every `test`-carrying
            // task a `solution_file` at load, so a code task always takes the ARTIFACT arm above.
            // This arm used to call `test_grade(&answer, ..)` — extract code from her SPOKEN
            // answer, compile it, and PASS her. That is deleted, and it stays deleted: it paid
            // out for narration, and because graded episodes feed the genome, that payout was a
            // training signal for "describing is doing". Fail loud rather than let it grow back.
            (
                false,
                "harness defect: a code task reached grading without a solution_file — every \
                 test-graded task must be normalized by require_hands_for_code at load. Code is \
                 graded on the file she wrote, never on text extracted from her answer."
                    .to_string(),
            )
        } else {
            substring_or_silence_grade(t, &answer)
        };
        // NO test-only recovery wrapper here. Iterating on a failure is a PRODUCTION
        // persona behavior — she runs her own verification (a shell/compile tool) inside
        // `drive_to_settle`, sees the failure as an ordinary tool observation (threaded back
        // by the full-result fix), and fixes, all in the ONE shared act→observe loop. The
        // eval must exercise that identical path — a grader-fed retry loop that only exists
        // at eval time would measure a fiction (Joel: "you can't code the test and prod path
        // differently, or you never get a valid test"). So: one settle, then grade the
        // artifact. `max_retries` no longer drives a harness loop; `max_acts` is the real
        // knob — it bounds the persona's OWN act→observe iterations, in test and prod alike.
        let _ = max_retries;
        let total_acts = settled.acts as u32;
        if ok {
            pass += 1;
        }
        // ARTIFACT PRESERVATION (2026-08-23, Joel: "honest isn't good — are
        // they making code that's decent?"). The world sweep destroys her
        // solution at the next provision, so no citizen code ever survived
        // grading for review — quality was unjudgeable after the fact and the
        // genome had no diff to learn from. Copy the graded solution file into
        // the durable receipts dir at grade time (bounded: one file per task
        // per run, beside the ledger — a governed, TrackedDir-registered
        // location, not a new cache class).
        if let (Some(sol), Some(root)) = (t.solution_file.as_deref(), workspace_root) {
            let src = std::path::Path::new(root).join(sol);
            if src.is_file() {
                if let Some(home) = dirs::home_dir() {
                    let run_tag = live_eval_run_id().unwrap_or_else(|| "unkeyed".into()); // no live run registered = a test/dev drive; still preserved, under one bucket
                    let dst_dir = home
                        .join(".continuum/bench-receipts")
                        .join(run_tag)
                        .join("solutions");
                    let dst = dst_dir.join(sol.replace('/', "__"));
                    let copied = std::fs::create_dir_all(&dst_dir)
                        .and_then(|_| std::fs::copy(&src, &dst));
                    if let Err(e) = copied {
                        tracing::warn!(task = %t.id, error = %e, "graded solution not preserved");
                    } else {
                        continuum_base::probe!(
                            class = "eval.artifact.preserved",
                            task = %t.id,
                            path = %dst.display(),
                            ok = ok,
                            "graded solution copied to durable receipts — reviewable after the world sweeps"
                        );
                    }
                }
            }
        }
        // Speed/latency of THIS task — the accumulated deliberation cost the settle
        // loop folded across every act→observe tick (the model's own measured time
        // + tokens). Reported next to the grade so accuracy and speed sit on one row.
        let m = settled.metrics;
        let task_result = EvalTaskResult {
            id: t.id.clone(),
            ok,
            grade,
            acts: total_acts,
            answer,
            latency_ms: m.latency_ms,
            output_tokens: m.output_tokens,
            tokens_per_second: m.tokens_per_second(),
            decode_tokens_per_second: m.decode_tokens_per_second(),
            cache_hit_rate: m.cache_hit_rate(),
            prefill_ms: m.prefill_ms,
            decode_ms: m.decode_ms,
        };
        // Teach at GRADE TIME — an interrupted round keeps this lesson.
        if let Some(sink) = lesson_sink {
            sink.teach(t, &task_result);
        }
        results.push(task_result);
        report_task_graded(
            &t.id,
            ok,
            total_acts,
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
