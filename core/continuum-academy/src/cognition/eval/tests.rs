#![cfg(test)]
    #[test]
    fn eval_gene_path_wins_over_reused_alias_and_ambiguity_refuses() {
        // Regression for #4504: never credit a different base selected by an alias collision.
        use crate::genome::adapter_manifest::TrainedAdapter;
        let manifest = vec![
            TrainedAdapter { alias: "same".into(), path: "/old".into(), base_model_id: "a".into() },
            TrainedAdapter { alias: "same".into(), path: "/new".into(), base_model_id: "b".into() },
        ];
        let selected = resolve_eval_gene(&manifest, "same", "/new").expect("explicit artifact");
        assert_eq!(selected.base_model_id, "b");
        assert_eq!(selected.path, std::path::Path::new("/new"));
        assert!(resolve_eval_gene(&manifest, "same", "").is_err());
        assert!(resolve_eval_gene(&manifest, "same", "/missing").is_err());
        assert!(resolve_eval_gene(&manifest[..1], "same", "").is_ok());
    }

    use super::*;
    use crate::inference::llama_server::LanePlacement;

    // what this catches: defect 2's sibling (2026-08-23, MirrorCode maiden round):
    // run_dod spawned bash in the PROCESS CWD, so setup_shell staged fixtures into
    // the real repo checkout while her hands and artifacts lived in the run's
    // clone — false fails plus operator-tree pollution. The DoD must run in the
    // run's world; None preserves cwd for callers that genuinely run there.
    // what this catches (2026-08-25, the systemic hardening): the DoD classifier must
    // separate "the grader could not RUN" (infra — missing workspace, refused command,
    // command-not-found, infra exit) from "the grader ran and it's red" (a real miss).
    // If they conflate, a harness bug scores as a model failure AND teaches her she
    // failed — the astropy-13236 false-failure, and a bug ANY dod-graded project hits.
    #[test]
    fn dod_infra_is_distinguished_from_a_real_red() {
        // Infra: could-not-run phrases + reserved infra exit codes.
        assert!(dod_output_is_infra(None, "substrate refused command benchmark/swe-grade"));
        assert!(dod_output_is_infra(None, "could not read swe/x's diff: No such file or directory"));
        assert!(dod_output_is_infra(Some(127), "continuum: not found"));
        assert!(dod_output_is_infra(Some(4), "terminal-bench harness: staging corrupt"));
        assert!(dod_output_is_infra(Some(5), "pytest is not installed"));
        // REAL red: tests ran and failed — must NOT be classed infra.
        assert!(!dod_output_is_infra(Some(1), "5 failed, 1 passed in 1.61s"));
        assert!(!dod_output_is_infra(Some(1), "AssertionError: assert 0 == 2"));
        assert!(!dod_output_is_infra(Some(1), "FAILED test_outputs.py::test_thing"));
    }

    // what this catches: defect 2's sibling (2026-08-23, MirrorCode maiden round):
    // run_dod spawned bash in the PROCESS CWD, so setup staged into the real repo
    // while her artifacts lived in the run clone. The DoD must run in the run root.
    #[tokio::test]
    async fn run_dod_executes_in_the_run_root() {
        let dir = tempfile::tempdir().expect("tempdir");
        let v = run_dod(Some(dir.path()), "touch dod-was-here").await;
        assert!(v.passed());
        assert!(
            dir.path().join("dod-was-here").exists(),
            "the DoD's world must be the run root, not the process cwd"
        );
        assert!(
            !std::path::Path::new("dod-was-here").exists(),
            "nothing may land in the process cwd when a root is given"
        );
    }

    // what this catches: the orphan sweep deleting a CONCURRENT run's live world —
    // the one direction that turns crash-path hygiene into data loss mid-exam.
    // regression for the 2026-08-23 finding (a SIGTERM'd reboot orphaned a 31 GB
    // eval clone that Drop could never reap; the sweep is the crash-path owner,
    // and live_eval_roots() is what scopes it to true orphans).
    #[test]
    fn orphan_sweep_removes_dead_worlds_and_never_live_ones() {
        let parent = tempfile::tempdir().expect("tempdir");
        let live = parent.path().join("live-run");
        let orphan = parent.path().join("orphan-run");
        std::fs::create_dir_all(&live).expect("mkdir live");
        std::fs::create_dir_all(&orphan).expect("mkdir orphan");
        let keep = parent.path().join("being-provisioned");
        std::fs::create_dir_all(&keep).expect("mkdir keep");

        let holder = EphemeralEvalRoot::new(live.clone());
        sweep_orphan_eval_roots(parent.path(), &keep);

        assert!(live.is_dir(), "a registered live world must survive the sweep");
        assert!(keep.is_dir(), "the world being provisioned must survive the sweep");
        assert!(!orphan.is_dir(), "an unregistered world is an orphan and must be removed");
        drop(holder); // Drop removes the live world AND its registry entry
        assert!(!live.is_dir(), "Drop still owns the in-process cleanup path");

        // A world whose fork's hands weren't confirmed ended is kept (Astra's re-review of
        // #5005). A later process's sweep (simulated: the registry no longer holds it) deletes
        // it only once every recorded pid is positively absent; a restart alone proves nothing.
        let alive = parent.path().join("unresolved-alive");
        let ended = parent.path().join("unresolved-ended");
        std::fs::create_dir_all(&alive).expect("mkdir alive");
        std::fs::create_dir_all(&ended).expect("mkdir ended");
        let mut exited = std::process::Command::new("true").spawn().expect("spawn a child that exits");
        let exited_pid = exited.id();
        exited.wait().expect("reap it, so its pid is gone");
        EphemeralEvalRoot::new(alive.clone()).retain_unresolved(Some(vec![std::process::id()]));
        EphemeralEvalRoot::new(ended.clone()).retain_unresolved(Some(vec![exited_pid]));
        sweep_orphan_eval_roots(parent.path(), &keep);
        assert!(alive.is_dir() && ended.is_dir(), "this process's registry spares every kept world");
        if let Ok(mut live) = live_eval_roots().lock() {
            live.remove(&alive);
            live.remove(&ended);
        }
        sweep_orphan_eval_roots(parent.path(), &keep);
        assert!(alive.is_dir(), "a world whose recorded process is still present is never swept");
        assert!(!ended.is_dir(), "a world whose every recorded process is gone is swept");
    }

    // what this catches: #310 — the /v1/models context_length contract for gateway-routed
    // eval lanes. The ds4 sidecar serves 8192 while its catalog row states the model's 1M
    // capability; budgeting the fork against the row overflows the real server. Pins:
    // exact-id match wins, missing id/field degrades to None (caller falls back to the
    // row), never a panic on a shape drift.
    #[test]
    fn provider_context_length_parses_the_ds4_shape_and_degrades_to_none() {
        let body: serde_json::Value = serde_json::json!({
            "object": "list",
            "data": [{ "id": "deepseek-v4-flash", "object": "model", "context_length": 8192 }]
        });
        assert_eq!(
            parse_provider_context_length(&body, "deepseek-v4-flash"),
            Some(8192)
        );
        assert_eq!(
            parse_provider_context_length(&body, "some-other-model"),
            None
        );
        let no_field: serde_json::Value =
            serde_json::json!({ "data": [{ "id": "deepseek-v4-flash" }] });
        assert_eq!(
            parse_provider_context_length(&no_field, "deepseek-v4-flash"),
            None
        );
        assert_eq!(
            parse_provider_context_length(&serde_json::json!({}), "deepseek-v4-flash"),
            None
        );
    }

    // what this catches: the web-dev mouth-or-hands capture (#206/#143 fence→act gap).
    // A capable model that emits a complete page in a ```html fence but never calls
    // code/write must still be graded on what it BUILT — else the benchmark scores
    // tool-routing, not ability (Qwen2.5-Coder-7B wrote a flawless login form in a fence,
    // wrote no file, scored 0/6, below a weaker model, 2026-07-21). Regression pins:
    // last fence wins, doctype/`<html` detected with or without a lang tag, and a
    // prose-only answer yields None — nothing spoken to NAME (the detector diagnoses now,
    // it does not rescue; see the no-mouth-for-hands block in the web-dev grade).
    #[test]
    fn html_artifact_extracts_the_spoken_page_or_nothing() {
        // tagged ```html fence
        let a = "Here is the page:\n```html\n<!DOCTYPE html><html><body><h1>Sign in</h1></body></html>\n```\nDone.";
        assert_eq!(
            html_artifact_from_answer(a).as_deref(),
            Some("<!DOCTYPE html><html><body><h1>Sign in</h1></body></html>")
        );
        // untagged fence whose body is a page (doctype sniff)
        let b = "```\n<html><body><form></form></body></html>\n```";
        assert!(html_artifact_from_answer(b).unwrap().contains("<form>"));
        // last qualifying fence wins (a corrected draft supersedes the first)
        let c = "```html\n<html>OLD</html>\n```\nfixed:\n```html\n<html>NEW</html>\n```";
        assert_eq!(
            html_artifact_from_answer(c).as_deref(),
            Some("<html>NEW</html>")
        );
        // bare page answer, no fence
        let d = "<!doctype html>\n<html><h1>Hi</h1></html>";
        assert_eq!(html_artifact_from_answer(d).as_deref(), Some(d.trim()));
        // prose-only / a rust fence → nothing to materialize
        assert_eq!(
            html_artifact_from_answer("I would build a login form with an h1."),
            None
        );
        assert_eq!(
            html_artifact_from_answer("```rust\nfn main() {}\n```"),
            None
        );
    }

    /// what this catches: THE RESCUE COMING BACK. The web-dev grade used to materialize a page
    /// the persona SPOKE when she never wrote the file, and grade that. It was disclosed and
    /// well-meant — but it meant no benchmark in the suite ever required her hands, so the
    /// fence→act failure was never scored and therefore never fixed. SWE-bench, which has no
    /// such rescue, then showed it at full strength: 30 acts, 0 files, 0 refusals.
    ///
    /// A persona who describes a page has not built one. Grading the description as the artifact
    /// is a scam we play on ourselves, and the honest zero is the finding. This pins the
    /// contract: the extractor still detects a spoken artifact, but ONLY to name the gap.
    #[test]
    fn a_spoken_artifact_is_never_accepted_in_place_of_a_written_one() {
        let spoken =
            "Here is the page:\n```html\n<!DOCTYPE html><html><body><h1>Hi</h1></body></html>\n```";
        // The detector still SEES it — that capability is what makes the failure diagnosable.
        assert!(
            html_artifact_from_answer(spoken).is_some(),
            "the detector must still recognise a spoken artifact — it names the gap now"
        );
        // And seeing it must never become materialising it. The grade path writes no file:
        // the only writer of the graded artifact is her own `code/write`.
        let dir = std::env::temp_dir().join(format!("no-mouth-for-hands-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let target = dir.join("index.html");
        let _ = std::fs::remove_file(&target);
        assert!(
            !target.exists(),
            "precondition: she has not written the file with her hands"
        );
        // (The grade fn needs a live workspace; the invariant asserted here is the one a
        // reviewer must not break — nothing in this module may create `target` from `spoken`.)
        assert!(
            !std::fs::read_to_string(&target)
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false),
            "a spoken artifact must never materialise into the graded file"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn code_task(id: &str) -> EvalTask {
        let mut t: EvalTask = serde_json::from_str(
            r#"{"id":"x","prompt":"Implement `pub fn f() -> i32`. Return the complete function.",
                "test":"assert_eq!(f(), 1);","lang":"rust","expect":""}"#,
        )
        .expect("fixture parses");
        t.id = id.to_string();
        t
    }

    /// what this catches: a code task reaching the grader with no artifact to grade — which is
    /// how the harness used to pay out for a fence. `require_hands_for_code` gives every
    /// test-graded task a file, so the ARTIFACT arm is the only arm code can take.
    #[test]
    fn every_code_task_is_normalized_to_grade_the_file_her_hands_wrote() {
        let mut t = code_task("atoi");
        assert!(t.solution_file.is_none(), "fixture starts mouth-graded");
        t.require_hands_for_code();
        assert_eq!(t.solution_file.as_deref(), Some("sol_atoi.rs"));
        // and the ASK must match what the grade READS, or she scores 0 for answering the
        // question she was actually asked.
        assert!(
            t.prompt.contains("sol_atoi.rs"),
            "prompt names the file: {}",
            t.prompt
        );
        assert!(
            t.prompt.contains("write tool"),
            "prompt tells her to WRITE it"
        );
        assert!(
            t.prompt.contains("Implement `pub fn f()"),
            "original task text survives"
        );
        // normalization needs hands, so tools get offered — otherwise the whole gym scores a
        // silent 0 for lack of a write tool (#208's failure shape).
        assert!(t.needs_tools(), "a file-graded task must arm tools");
    }

    /// what this catches: the rule silently un-applying for a whole class of gyms. Every
    /// committed gym line must survive normalization with an artifact to grade — this is the
    /// corpus-wide assertion, not a single-task one.
    #[test]
    fn no_committed_gym_can_pay_out_for_spoken_code() {
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/genome");
        let Ok(entries) = std::fs::read_dir(&root) else {
            return; // corpora not present in this checkout — the unit rule above still holds
        };
        let mut checked = 0usize;
        for e in entries.flatten() {
            let path = e.path();
            if path.extension().and_then(|s| s.to_str()) != Some("jsonl") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            for (n, line) in text.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let Ok(mut t) = serde_json::from_str::<EvalTask>(line) else {
                    continue;
                };
                if t.test.is_none() {
                    continue; // knowledge task — answering by speaking is correct
                }
                t.require_hands_for_code();
                assert!(
                    t.solution_file.is_some() || t.dod_shell.is_some(),
                    "{}:{} ({}) is a code task with no artifact to grade — it would pay out for \
                     spoken code, which trains 'describing is doing'",
                    path.display(),
                    n + 1,
                    t.id
                );
                checked += 1;
            }
        }
        assert!(
            checked > 100,
            "expected the code corpora to be present, checked only {checked}"
        );
    }

    /// what this catches: someone re-adding a mouth grade for a task that ALREADY names a file.
    /// Normalization must not clobber an author's explicit choice, in either direction.
    #[test]
    fn an_explicit_solution_file_and_a_dod_are_both_left_alone() {
        let mut t = code_task("keep");
        t.solution_file = Some("mine.rs".into());
        let before = t.prompt.clone();
        t.require_hands_for_code();
        assert_eq!(
            t.solution_file.as_deref(),
            Some("mine.rs"),
            "author's file wins"
        );
        assert_eq!(
            t.prompt, before,
            "no duplicated preamble on an already-acted task"
        );

        let mut d = code_task("dod");
        d.dod_shell = Some("cargo test".into());
        d.require_hands_for_code();
        assert!(
            d.solution_file.is_none(),
            "a DoD already grades the workspace her hands changed"
        );
    }

    /// what this catches: a mined gym being unrunnable. `gym/mine` gives every task its OWN git
    /// worktree, so the root is a property of the TASK. When only the RUN could carry a root, an
    /// 8-task mined gym ran with one root — wrong for every task — and she was asked to fix a bug
    /// in a directory her sandboxed tools could not reach. She scored zero, and the zero described
    /// the harness. Glass-boxed 2026-08-05: her workspace-map showed the continuum tree while the
    /// prompt named a serde_json checkout.
    ///
    /// Also pins that a repo task is NOT mangled by the code-must-be-written normalization: it is
    /// already hands-graded by its `dod_shell` against a real test suite, and deriving a
    /// `sol_<id>.rs` for it would invent a deliverable the task never wanted.
    #[test]
    fn a_mined_repo_task_carries_its_own_root_and_is_not_renormalized() {
        let mut t: EvalTask = serde_json::from_str(
            r#"{"id":"mine_2037b634","prompt":"Real bug, real repo.","expect":"",
                "workspaceRoot":"/tmp/gym/task_2037b634",
                "dodShell":"cd /tmp/gym/task_2037b634 && cargo test",
                "setupShell":"git checkout HEAD^ -- src/de.rs"}"#,
        )
        .expect("the mined wire shape (camelCase) parses");
        assert_eq!(t.workspace_root.as_deref(), Some("/tmp/gym/task_2037b634"));
        assert!(
            t.needs_tools(),
            "a task pinned to a repo obviously needs hands"
        );
        let before = t.prompt.clone();
        t.require_hands_for_code();
        assert!(
            t.solution_file.is_none(),
            "a repo task is graded by its DoD against the real suite — never by an invented file"
        );
        assert_eq!(
            t.prompt, before,
            "no acting preamble bolted onto a repo task"
        );
    }

    /// what this catches: the per-task root silently not overriding the run-level pin (or vice
    /// versa). Precedence has to be unambiguous — the task's own checkout wins, and a task with
    /// no root of its own still inherits the run's pin, so SWE-bench (one clone, one task) and a
    /// mined gym (N clones, N tasks) both work through the SAME field.
    /// what this catches: someone "finishing" the per-task root by re-rooting mid-run. That moves
    /// her HANDS while her EYES stay baked at the fork root — glass-boxed 2026-08-05, the probe
    /// fired green while her workspace-map still described the old tree. This pins the contract
    /// that a differing task root is REFUSED (loud, actionable) rather than half-applied (silent,
    /// scores a lie). Delete this only together with the eyes-derive-from-hands fix.
    #[test]
    fn a_task_root_that_differs_from_the_runs_is_refused_not_half_applied() {
        let run_pin = Some("/repo/from-run");
        let mut t = code_task("x");
        // No task root → inherits the run pin, which DID move both halves at fork. Fine.
        assert_eq!(t.workspace_root.as_deref().or(run_pin), run_pin);
        let matches_run = t.workspace_root.as_deref().map(|w| Some(w) != run_pin);
        assert_eq!(matches_run, None, "no task root is never a conflict");
        // A DIFFERENT task root is the refusal case — mid-run re-rooting would split her.
        t.workspace_root = Some("/repo/from-task".into());
        assert!(
            t.workspace_root.as_deref() != run_pin,
            "differing root must be detected as a conflict, not silently preferred"
        );
        // Same root declared on both is harmless (the fork already rooted both halves there).
        t.workspace_root = Some("/repo/from-run".into());
        assert_eq!(
            t.workspace_root.as_deref(),
            run_pin,
            "agreement is not a conflict"
        );
    }

    // what this catches: the mid-run scoreboard's poll surface (#123/#141). One
    // report_task_graded call must land on the watch every reader shares — done/total/
    // pass/current task — so cognition/eval-status (no run_id) and widget bridges see
    // live totals instead of a multi-hour void. If the watch write is dropped, long
    // runs go dark again (the exact gap Joel called out mid-run 2026-07-16).
    #[test]
    fn report_task_graded_lands_on_the_progress_watch() {
        report_task_graded("HumanEval/7", true, 2, 45_000, 128, 6, 7, 164);
        let snap = subscribe_eval_progress()
            .borrow()
            .clone()
            .expect("a graded task must publish progress");
        assert_eq!(snap.done, 7);
        assert_eq!(snap.total, 164);
        assert_eq!(snap.pass, 6);
        assert_eq!(snap.current_task, "HumanEval/7");
        assert!(snap.last_ok);
        assert!(snap.updated_at_ms > 0);
        // Bare unit test: no global authority published → honest None, never a
        // fabricated 0 ([[fallbacks-are-illegal-fail-loud]] applied to metrics).
        assert!(snap.vram_free_gb.is_none());
    }

    fn task_result(latency_ms: u64, output_tokens: u32, tps: f64) -> EvalTaskResult {
        EvalTaskResult {
            id: "t".into(),
            ok: true,
            grade: "tests passed".into(),
            acts: 0,
            answer: String::new(),
            latency_ms,
            output_tokens,
            tokens_per_second: tps,
            decode_tokens_per_second: 0.0,
            cache_hit_rate: 0.0,
            prefill_ms: 0,
            decode_ms: 0,
        }
    }

    /// A graded row with an explicit ok, for building fault-injection PassOutcomes.
    fn graded_row(id: &str, ok: bool, grade: &str) -> EvalTaskResult {
        EvalTaskResult {
            id: id.into(),
            ok,
            grade: grade.into(),
            acts: 0,
            answer: String::new(),
            latency_ms: 10,
            output_tokens: 5,
            tokens_per_second: 0.5,
            decode_tokens_per_second: 0.5,
            cache_hit_rate: 0.0,
            prefill_ms: 1,
            decode_ms: 9,
        }
    }

    // what this catches (Proctored Exam Session, Slice B): the fake-zero. A run whose
    // serving lane died mid-exam (≥1 unrecoverable infra fault) must resolve to
    // InfraUnavailable, NAMING the task and cause — NEVER a scored `pass_rate: 0.0` that
    // reads as "she got them all wrong". If infra_verdict ever returned None on a faulted
    // run, the harness would publish a phantom 0% over a dead lane again — the exact
    // untrustworthy-by-construction bug this slice exists to kill.
    // [[proctored-exam-session-dependable-benchmark]]
    #[test]
    fn infra_faulted_run_is_infra_unavailable_never_a_fake_zero() {
        // Two tasks graded, then the lane died on the third and never came back.
        let outcome = PassOutcome {
            pass: 1,
            results: vec![
                graded_row("t1", true, "tests passed"),
                graded_row("t2", false, "no match"),
                graded_row(
                    "t3",
                    false,
                    "infra unavailable (lane never recovered): model 'X' is not the active served model",
                ),
            ],
            infra_faults: 1,
            infra_reason: Some(
                "task 't3': model 'X' is not the active served model (serving: <none>, ready: false)"
                    .into(),
            ),
        };
        let verdict =
            infra_verdict(&outcome).expect("a run with an infra fault must be InfraUnavailable");
        assert_eq!(verdict.infra_faults, 1);
        assert_eq!(
            verdict.tasks_attempted, 3,
            "attempted-so-far, exam incomplete"
        );
        assert!(
            verdict.reason.contains("t3") && verdict.reason.contains("active served model"),
            "the reason names the task + the infra cause: {}",
            verdict.reason
        );
    }

    // what this catches: the OTHER half of trustworthiness — a genuinely-wrong answer on a
    // WORKING lane is a REAL 0, counted, NOT masked as infra. If infra_verdict fired on any
    // failed task (rather than only on unrecoverable inference faults), we'd hide real
    // capability misses behind "infra unavailable" and the number would over-report. Zero
    // infra faults ⇒ Scored, the wrong answers counted.
    #[test]
    fn wrong_answers_on_a_working_lane_are_scored_not_infra() {
        let outcome = PassOutcome {
            pass: 0,
            results: vec![
                graded_row("t1", false, "no match"),
                graded_row("t2", false, "assertion failed"),
            ],
            infra_faults: 0,
            infra_reason: None,
        };
        assert!(
            infra_verdict(&outcome).is_none(),
            "a real 0 on a verified lane is Scored, never InfraUnavailable"
        );
    }

    // what this catches: LEARN mode's lesson keeps the EXPERIENCE (what she was
    // asked + how she did) while the redaction policy scrubs the held-out answer
    // key — so the exam teaches her without ever letting her memorize the crib
    // sheet. If the format changed to drop the experience, or redaction stopped
    // catching the key, "learn from exams" would either teach nothing or cheat.
    #[test]
    fn learn_mode_lesson_keeps_experience_but_scrubs_the_answer_key() {
        use crate::cognition::redaction::{HeldOutAnswerDetector, RedactionClass, RedactionPolicy};
        let task = EvalTask {
            id: "loop-file".into(),
            prompt: "Which file holds the service loop, service_loop.rs perhaps?".into(),
            expect: "service_loop.rs".into(),
            ..Default::default()
        };
        let mut result = task_result(1000, 10, 5.0);
        result.grade = "answer contained service_loop.rs".into();

        let lesson = format_exam_lesson(&task, &result);
        assert!(lesson.contains("I was asked"));
        assert!(lesson.contains("I solved it"));

        let policy = RedactionPolicy::new(vec![Box::new(HeldOutAnswerDetector::new(
            [task.expect.clone()],
            HeldOutAnswerDetector::DEFAULT_MIN_LEN,
        ))]);
        let (redacted, report) = policy.redact(&lesson);
        assert!(
            !redacted.contains("service_loop.rs"),
            "answer key must be scrubbed"
        );
        assert!(report.count(RedactionClass::HeldOutAnswer) >= 1);
        assert!(redacted.contains("I was asked"), "the experience survives");
        assert!(redacted.contains("I solved it"), "the outcome survives");
    }

    // what this catches: BELIEF HYGIENE — a graded MISS must teach a PROVISIONAL
    // single-attempt lesson (retryable, supersedable), never a hardened verdict on
    // ability. The false-failure incident (a broken env graded her code ok=False)
    // must not become "I am bad at X" self-belief; a PASS stays confident.
    #[test]
    fn a_miss_teaches_provisionally_a_pass_teaches_confidently() {
        let task = EvalTask {
            id: "cancel-async".into(),
            prompt: "Fix the async cancellation.".into(),
            ..Default::default()
        };
        let mut miss = task_result(1000, 32, 5.0);
        miss.ok = false;
        miss.grade = "5 failed, 1 passed".into();
        let lesson = format_exam_lesson(&task, &miss);
        assert!(lesson.contains("did not pass yet"), "miss must be provisional: {lesson}");
        assert!(lesson.contains("not a settled verdict"), "miss must not harden into ability belief: {lesson}");
        assert!(lesson.contains("supersedes"), "a later pass must be able to supersede: {lesson}");
        assert!(!lesson.contains("did NOT solve"), "the flat-conviction phrasing is gone: {lesson}");

        let pass = task_result(1000, 10, 5.0); // ok=true
        let plesson = format_exam_lesson(&task, &pass);
        assert!(plesson.contains("I solved it"), "a pass stays confident: {plesson}");
        assert!(!plesson.contains("not a settled verdict"), "a pass carries no provisional hedge");
    }

    // what this catches: the speed/latency aggregate math behind the scoreboard —
    // specifically the P95 index (ceil(0.95*n)-1, clamped), which is the easy place
    // to land off-by-one. For n=4, P95 is the max (idx 3); means average per-task
    // (not total/total); total_output sums. An empty set must be all-zero, never a
    // divide-by-zero. If this drifts, the latency/speed numbers a run reports — and
    // the trend ledger — silently lie.
    #[test]
    fn speed_latency_aggregates_compute_mean_p95_and_totals() {
        let empty = speed_latency_aggregates(&[]);
        assert_eq!(empty.mean_latency_ms, 0.0, "empty set → zero, never NaN");
        assert_eq!(empty.total_output_tokens, 0);
        assert_eq!(empty.total_prefill_ms, 0);

        let results = vec![
            task_result(100, 10, 50.0),
            task_result(200, 20, 40.0),
            task_result(300, 30, 30.0),
            task_result(400, 40, 20.0),
        ];
        let agg = speed_latency_aggregates(&results);
        assert_eq!(
            agg.mean_latency_ms, 250.0,
            "mean latency = (100+200+300+400)/4"
        );
        assert_eq!(
            agg.p95_latency_ms, 400,
            "P95 of 4 tasks is the slowest (idx ceil(3.8)-1=3)"
        );
        assert_eq!(
            agg.mean_tokens_per_second, 35.0,
            "mean throughput averages per-task, not total/total"
        );
        assert_eq!(
            agg.total_output_tokens, 100,
            "total output tokens sum across the set"
        );
    }

    // what this catches: the GPU-FIRST placement policy for the coexisting eval lane
    // (Joel: "fill GPU lanes first, ~100% utilization; CPU is spillover of last
    // resort"). The lane goes on the GPU whenever it fits in live free VRAM, and on a
    // 64GB unified-memory box a 4B base (~3GB) alongside a resident 14B (~9GB) MUST
    // land on GPU — the regression this guards is the old hardcoded `Cpu` pin that ran
    // the gym at 4 tok/s with the accelerator idle. CPU is chosen ONLY when there's no
    // GPU at all, or the GPU genuinely can't hold the lane (then it's a visible spill,
    // never a silent default).
    #[test]
    fn placement_is_gpu_first_cpu_only_when_full_or_absent() {
        const GB: u64 = 1024 * 1024 * 1024;
        // 64GB box, 14B resident → ~50GB free; 4B base ~3GB footprint → GPU.
        let (p, why) = crate::inference::single_purpose_lane::choose_lane_placement(Some(50 * GB), Some(3 * GB));
        assert_eq!(p, LanePlacement::Gpu, "fits with room to spare: {why}");
        // GPU genuinely full (free below footprint+margin) → CPU spill, and SAID so.
        let (p, why) = crate::inference::single_purpose_lane::choose_lane_placement(Some(2 * GB), Some(3 * GB));
        assert_eq!(p, LanePlacement::Cpu, "no headroom must spill to CPU");
        assert!(
            why.contains("GPU full"),
            "the spill must name the reason: {why}"
        );
        // No GPU monitor on the node → CPU is the only device (honest, not a fallback).
        let (p, _) = crate::inference::single_purpose_lane::choose_lane_placement(None, Some(3 * GB));
        assert_eq!(
            p,
            LanePlacement::Cpu,
            "no GPU backend → CPU is the only device"
        );
        // Couldn't size the base → GPU-first optimism, never idle the accelerator.
        let (p, _) = crate::inference::single_purpose_lane::choose_lane_placement(Some(50 * GB), None);
        assert_eq!(
            p,
            LanePlacement::Gpu,
            "unknown footprint defaults to GPU-first"
        );
        // Exactly at the margin edge counts as fitting (>= margin).
        let (p, _) = crate::inference::single_purpose_lane::choose_lane_placement(Some(3 * GB + crate::inference::single_purpose_lane::GPU_PLACEMENT_MARGIN_BYTES), Some(3 * GB));
        assert_eq!(
            p,
            LanePlacement::Gpu,
            "free == footprint+margin fits on GPU"
        );
    }

    // what this catches: eval-status resolves a terminal ledger row by run_id NEWEST-first
    // and only on an exact match — so `benchmark/run`'s completed row is found, not a
    // prior run's stale line, and an in-flight/unknown run resolves to None (pending), not
    // a wrong row. This is the matcher behind the run_id-only poll fix (2026-07-20): the
    // load test hit a false `complete:false` because the poll lacked persona_id; run_id is
    // globally unique, so it must be a sufficient key. Guards the "hang forever on a false
    // pending" footgun a matrix/CI poller would otherwise trip.
    #[test]
    fn eval_status_row_matches_run_id_newest_first() {
        let ledger = concat!(
            r#"{"runId":"aaa","score":3,"total":10}"#,
            "\n",
            r#"{"runId":"bbb","score":7,"total":10}"#,
            "\n",
            r#"{"runId":"bbb","score":10,"total":10}"#, // a later row for bbb wins
            "\n",
        );
        let hit = row_with_run_id(ledger, "bbb").expect("bbb row present");
        assert_eq!(
            hit.get("score").and_then(|s| s.as_i64()),
            Some(10),
            "newest bbb row wins"
        );
        let first = row_with_run_id(ledger, "aaa").expect("aaa row present");
        assert_eq!(first.get("score").and_then(|s| s.as_i64()), Some(3));
        // An unknown / still-in-flight run → None (pending), never a wrong row.
        assert!(
            row_with_run_id(ledger, "ccc").is_none(),
            "no row → pending, not a mismatch"
        );
        // A malformed line is skipped, not fatal.
        assert!(row_with_run_id("not json\n{bad\n", "bbb").is_none());
    }

    // what this catches: TurnMetrics throughput + accumulation, the per-turn cost
    // the settle loop folds. tokens_per_second guards div-by-zero (0ms → 0.0, never
    // NaN/inf that would poison the mean); accumulate sums each field so a multi-act
    // task reports the TOTAL generation cost, not just the last turn's.
    #[test]
    fn turn_metrics_throughput_and_accumulation() {
        use crate::cognition::workspace::TurnMetrics;
        let zero = TurnMetrics::default();
        assert_eq!(zero.tokens_per_second(), 0.0, "0ms must not divide-by-zero");

        let mut acc = TurnMetrics {
            input_tokens: 5,
            output_tokens: 20,
            latency_ms: 1000,
            ..Default::default()
        };
        assert_eq!(acc.tokens_per_second(), 20.0, "20 tokens / 1s = 20 tok/s");
        acc.accumulate(TurnMetrics {
            input_tokens: 3,
            output_tokens: 10,
            latency_ms: 1000,
            ..Default::default()
        });
        assert_eq!(acc.output_tokens, 30, "accumulate sums completion tokens");
        assert_eq!(acc.latency_ms, 2000, "accumulate sums latency across turns");
        assert_eq!(acc.tokens_per_second(), 15.0, "30 tokens / 2s = 15 tok/s");
    }

    // what this catches: the lane prefill/decode split folds correctly and the
    // derived rates (real decode tok/s, cache-hit-rate) come off the LANE clock,
    // not the diluted wall-clock — the speed-harness numbers must not lie.
    #[test]
    fn turn_metrics_prefill_decode_split_and_rates() {
        use crate::cognition::workspace::TurnMetrics;
        let mut acc = TurnMetrics {
            output_tokens: 20,
            latency_ms: 5000, // wall-clock diluted by prefill + cognition overhead
            cached_tokens: 1500,
            prefill_tokens: 500,
            prefill_ms: 2000,
            decode_ms: 1000, // lane decoded 20 tok in 1s → 20 tok/s real
            ..Default::default()
        };
        assert_eq!(
            acc.decode_tokens_per_second(),
            20.0,
            "real decode tok/s comes off decode_ms (1s), not the 5s wall-clock"
        );
        assert!(
            (acc.cache_hit_rate() - 0.75).abs() < 1e-9,
            "1500 cached / 2000 prompt = 0.75 hit-rate"
        );
        // wall-clock tok/s is the DILUTED number (20 tok / 5s = 4) — the gap vs the
        // 20 tok/s real decode IS the prefill+overhead tax the harness surfaces.
        assert_eq!(
            acc.tokens_per_second(),
            4.0,
            "wall-clock tok/s stays diluted"
        );

        acc.accumulate(TurnMetrics {
            output_tokens: 10,
            cached_tokens: 1900,
            prefill_tokens: 100,
            prefill_ms: 500,
            decode_ms: 500,
            ..Default::default()
        });
        assert_eq!(acc.prefill_ms, 2500, "prefill_ms sums across acts");
        assert_eq!(acc.decode_ms, 1500, "decode_ms sums across acts");
        assert_eq!(acc.cached_tokens, 3400, "cached_tokens sums across acts");
        assert_eq!(acc.prefill_tokens, 600, "prefill_tokens sums across acts");
        assert_eq!(
            acc.decode_tokens_per_second(),
            20.0,
            "30 tok / 1.5s lane decode = 20 tok/s real"
        );
    }

    // what this catches: the memory veto that was MISSING when the dev machine
    // kernel-panicked (2026-07-22, watchdog timeout, compressed-pages 100%) — an
    // eval lane must be REFUSED at High/Critical system pressure or a closed
    // sustained-pressure gate, and must pass at Normal/Warning so ordinary load
    // never blocks measurement. Pure-core test; no process-global mutation.
    #[test]
    fn eval_lane_memory_veto_refuses_at_high_pressure_or_closed_gate() {
        use crate::system_resources::PressureLevel;
        assert!(crate::inference::single_purpose_lane::lane_memory_veto(PressureLevel::Normal, false).is_ok());
        assert!(crate::inference::single_purpose_lane::lane_memory_veto(PressureLevel::Warning, false).is_ok());
        assert!(crate::inference::single_purpose_lane::lane_memory_veto(PressureLevel::High, false).is_err());
        assert!(crate::inference::single_purpose_lane::lane_memory_veto(PressureLevel::Critical, false).is_err());
        // Sustained gate closed vetoes even when the instantaneous level looks calm
        // (the gate encodes "we were drowning seconds ago — don't re-load yet").
        assert!(crate::inference::single_purpose_lane::lane_memory_veto(PressureLevel::Normal, true).is_err());
        let err = crate::inference::single_purpose_lane::lane_memory_veto(PressureLevel::Critical, false).unwrap_err();
        assert!(
            err.to_string().contains("memory pressure"),
            "refusal must name the cause for the detached ledger: {err}"
        );
    }

    // what this catches (glass-boxed 2026-07-27): the SIGKILL wall the pressure LEVEL
    // never saw — a 24B eval lane co-resident with a 24B live lane, macOS reporting
    // "Normal" atop 9.6 GB free, spawned into a jetsam kill (exit 137, zero-byte log).
    // The RAM veto must REFUSE when a KNOWN footprint won't fit in the KNOWN free bytes
    // (+headroom), and must NOT refuse when either number is unknown (never starve a
    // node whose probe is momentarily blind — pressure gate + placement lease backstop).
    #[test]
    fn eval_lane_ram_veto_refuses_only_a_known_oversize_lane() {
        const H: u64 = crate::inference::single_purpose_lane::LANE_RAM_HEADROOM_BYTES;
        let gb = |n: u64| n * 1024 * 1024 * 1024;
        // 14 GB lane + 2 GB headroom = 16 GB needed; 9.6 GB free → refuse (the exact case).
        let err = crate::inference::single_purpose_lane::lane_ram_veto(Some(9_600_000_000), Some(gb(14)), H).unwrap_err();
        assert!(
            err.to_string().contains("free physical memory") && err.to_string().contains("OOM"),
            "refusal must name the OOM wall for the detached ledger: {err}"
        );
        // Comfortably fits (50 GB free, 14 GB lane) → pass.
        assert!(crate::inference::single_purpose_lane::lane_ram_veto(Some(gb(50)), Some(gb(14)), H).is_ok());
        // Exactly footprint+headroom is enough (>= boundary), one byte less is not.
        assert!(crate::inference::single_purpose_lane::lane_ram_veto(Some(gb(14) + H), Some(gb(14)), H).is_ok());
        assert!(crate::inference::single_purpose_lane::lane_ram_veto(Some(gb(14) + H - 1), Some(gb(14)), H).is_err());
        // Unknown footprint OR unknown free bytes → never a veto (can't size = don't block).
        assert!(crate::inference::single_purpose_lane::lane_ram_veto(Some(gb(1)), None, H).is_ok());
        assert!(crate::inference::single_purpose_lane::lane_ram_veto(None, Some(gb(14)), H).is_ok());
        assert!(crate::inference::single_purpose_lane::lane_ram_veto(None, None, H).is_ok());
    }

    // what this catches: the warm-pool key lifecycle — a dead Weak (last handle
    // dropped) must NOT satisfy a lookup, and a live one must. If lookup ever
    // returned dangling lanes, a solve would hand its forked cognition an adapter
    // pointed at a killed llama-server (connection-refused mid-drive).
    #[test]
    fn warm_lane_lookup_prunes_dead_entries() {
        let key = format!("test-warm-lane-{}", uuid::Uuid::new_v4());
        // No entry → miss.
        assert!(lookup_warm_eval_lane(&key).is_none());
        // Registered but immediately dropped inner → the Weak is dead → miss + prune.
        {
            let map = &*WARM_EVAL_LANES;
            map.lock()
                .unwrap()
                .insert(key.clone(), std::sync::Weak::new());
        }
        assert!(
            lookup_warm_eval_lane(&key).is_none(),
            "dead Weak must not satisfy"
        );
        assert!(
            !WARM_EVAL_LANES.lock().unwrap().contains_key(&key),
            "dead entry must be pruned on the miss"
        );
    }

    // what this catches: a refused teacher lane must fail the real synthesis path,
    // not fall through to registry inference and return an empty/failed-item corpus.
    #[tokio::test]
    async fn refused_lane_stops_both_synthesis_paths_before_generation() {
        let refused = || std::future::ready(Err(CommandError::Internal("lane refused".into())));
        let tasks = [EvalTask {
            id: "refused-lane".into(),
            prompt: "write a function".into(),
            test: Some("assert!(true);".into()),
            ..Default::default()
        }];
        let remediation =
            synthesize_remediation_with_lane(&tasks, "teacher", 0.0, Some(1), refused()).await;
        assert!(
            matches!(remediation, Err(CommandError::Internal(ref message)) if message == "lane refused")
        );
        let expansion = synthesize_lived_expansion_with_lane(
            &["a lived question".into()],
            "teacher",
            0.0,
            refused(),
        )
        .await;
        assert!(
            matches!(expansion, Err(CommandError::Internal(ref message)) if message == "lane refused")
        );

        // Empty lived input remains a no-op and never polls lane acquisition.
        let empty = synthesize_lived_expansion_with_lane(&["  ".into()], "teacher", 0.0, async {
            panic!("empty input acquired a lane")
        })
        .await
        .unwrap();
        assert!(empty.is_empty());
    }
