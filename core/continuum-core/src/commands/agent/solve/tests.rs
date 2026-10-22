#![cfg(test)]
    // what this catches (2026-09-16, the M5's 16-mind roster): the flat
    // `max_acts × 8 min` attempt timeout ended every run on a box whose acts cost
    // 21 min — two control solves killed at 26 and 22 acts and re-fired from act 1,
    // forever. The verdict is ADVANCEMENT: a ledger that keeps moving is never
    // stalled however slowly; a ledger that stops is, after the allowance; and the
    // far ceiling names itself. Paused tokio time so the whole story runs in ms.
    mod attempt_watchdog {
        use super::super::{attempt_with_advancement_watchdog, AttemptHalt};
        use std::time::Duration;

        fn write_acts(path: &std::path::Path, acts: u64) {
            std::fs::write(path, serde_json::json!({"state": "running", "acts": acts}).to_string())
                .expect("ledger write");
        }

        #[tokio::test(start_paused = true)]
        async fn a_slowly_advancing_attempt_is_never_stalled_and_a_stopped_one_is() {
            let dir = tempfile::tempdir().expect("tempdir");
            let ledger = dir.path().join("run.json");
            write_acts(&ledger, 0);
            let allowance = Duration::from_secs(75 * 60);
            let ceiling = Duration::from_secs(24 * 3600);
            // The attempt: one act every 30 minutes (slower than the old 8 min/act
            // allowance) for six acts, then it completes.
            let l = ledger.clone();
            let attempt = async move {
                for n in 1..=6u64 {
                    tokio::time::sleep(Duration::from_secs(30 * 60)).await;
                    write_acts(&l, n);
                }
                "settled"
            };
            let out = attempt_with_advancement_watchdog(Some(&ledger), allowance, ceiling, || false, attempt).await;
            assert_eq!(out, Ok("settled"), "advancing at 30 min/act is not a stall");

            // The wedge: acts stop at 3 and nothing moves again.
            write_acts(&ledger, 3);
            let l = ledger.clone();
            let wedged = async move {
                tokio::time::sleep(Duration::from_secs(10 * 60)).await;
                write_acts(&l, 3);
                std::future::pending::<&str>().await
            };
            let out = attempt_with_advancement_watchdog(Some(&ledger), allowance, ceiling, || false, wedged).await;
            assert_eq!(out, Err(AttemptHalt::Stalled { acts: 3, allowance }));
        }

        #[tokio::test(start_paused = true)]
        async fn the_ceiling_names_itself_and_an_attached_call_has_only_the_ceiling() {
            let dir = dir_with(0);
            let ledger = dir.path().join("run.json");
            let allowance = Duration::from_secs(75 * 60);
            let ceiling = Duration::from_secs(3 * 3600);
            // Advances every 20 min forever: never stalled, but the ceiling fires.
            let l = ledger.clone();
            let forever = async move {
                let mut n = 0u64;
                loop {
                    tokio::time::sleep(Duration::from_secs(20 * 60)).await;
                    n += 1;
                    write_acts(&l, n);
                }
                #[allow(unreachable_code)]
                ()
            };
            let out = attempt_with_advancement_watchdog(Some(&ledger), allowance, ceiling, || false, forever).await;
            assert!(matches!(out, Err(AttemptHalt::Ceiling { acts, .. }) if acts >= 8), "{out:?}");
            // No ledger (an attached call): stall cannot be judged, the ceiling still can.
            let out = attempt_with_advancement_watchdog(None, allowance, ceiling, || false, std::future::pending::<()>()).await;
            assert!(matches!(out, Err(AttemptHalt::Ceiling { acts: 0, .. })), "{out:?}");
        }

        // what this catches (card 4558eef9): a stop must reach the ACT LOOP — an
        // advancing solve, never stalled and far under its ceiling, ends at the next poll
        // once the stop flag is raised, and the halt names itself cancelled with the acts
        // it had reached.
        #[tokio::test(start_paused = true)]
        async fn a_round_stop_ends_an_advancing_attempt_at_the_next_poll() {
            let dir = dir_with(0);
            let ledger = dir.path().join("run.json");
            let allowance = Duration::from_secs(75 * 60);
            let ceiling = Duration::from_secs(24 * 3600);
            let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
            let l = ledger.clone();
            let s = stop.clone();
            let advancing = async move {
                for n in 1..=4u64 {
                    tokio::time::sleep(Duration::from_secs(5 * 60)).await;
                    write_acts(&l, n);
                }
                // The operator stops the round at act 4; the run would otherwise go on.
                s.store(true, std::sync::atomic::Ordering::SeqCst);
                std::future::pending::<&str>().await
            };
            let asked = { let s = stop.clone(); move || s.load(std::sync::atomic::Ordering::SeqCst) };
            let out = attempt_with_advancement_watchdog(Some(&ledger), allowance, ceiling, asked, advancing).await;
            assert_eq!(out, Err(AttemptHalt::Cancelled { acts: 4 }), "the stop reached the loop within one poll");
        }

        fn dir_with(acts: u64) -> tempfile::TempDir {
            let dir = tempfile::tempdir().expect("tempdir");
            write_acts(&dir.path().join("run.json"), acts);
            dir
        }
    }

    mod grade_close_classification {
        use super::super::{classify_grade_for_close, CardCloseDecision};
        use serde_json::json;

        // what this catches: the env-absence catch-all swallowing the two
        // "the env was fine, this grader just measured nothing" cases. Measured
        // 2026-08-27 on django-11734 (#2531): the solve's grade failed to
        // re-point the CACHED env at her dirty workspace and this classifier
        // called the environment absent — while benchmark_grade, re-pointing the
        // same cached env at a pristine clone, ran 40/40 pass-to-pass seconds
        // later. The consequence was a lying log line and the per-boot parking
        // brake armed against a demonstrably healthy instance. Order is the
        // invariant: the specific "measured nothing" reasons must be decided
        // BEFORE the generic any-error rule, or they get swallowed again.
        #[test]
        fn measured_nothing_is_never_an_environment_absence() {
            let repoint = json!({
                "error": format!(
                    "{}: could not re-point django__django-11734's cached env at /tmp/ws: \
                     The build backend returned an error",
                    crate::cognition::swe_bench::ENV_CACHE_REPOINT_FAILED
                ),
                "gate_ok": true,
            });
            assert_eq!(
                classify_grade_for_close(Some(&repoint)),
                CardCloseDecision::CacheRepointFailed,
                "a stale cache must never read as an absent environment"
            );

            let no_patch = json!({"error": "no candidate patch was produced"});
            assert_eq!(
                classify_grade_for_close(Some(&no_patch)),
                CardCloseDecision::NoPatch
            );

            // The genuine env-class cases still classify as absent.
            let broken_control = json!({"gate_ok": false, "error": null});
            assert_eq!(
                classify_grade_for_close(Some(&broken_control)),
                CardCloseDecision::EnvAbsent,
                "a broken control (f2p already passing pristine) stays env-class"
            );
            let unbuildable = json!({"error": "could not create a Python 3.8 venv", "gate_ok": true});
            assert_eq!(
                classify_grade_for_close(Some(&unbuildable)),
                CardCloseDecision::EnvAbsent
            );

            // A real verdict closes the card; no grade on disk closes nothing.
            let real = json!({"error": null, "gate_ok": true, "resolved": false});
            assert_eq!(
                classify_grade_for_close(Some(&real)),
                CardCloseDecision::Close
            );
            assert_eq!(classify_grade_for_close(None), CardCloseDecision::NoVerdict);
        }
    }

    mod room_mint_or_rejoin {
        use crate::identity::ActivityRoom;
        use uuid::Uuid;

        // what this catches: the #425 regression door. A solve with no room used to run
        // INVISIBLY under Uuid::nil() (13,209 turns, 8.7% of all turns, before it was
        // measured). The law (Joel 2026-08-26) is mint-or-rejoin: a dispatched run REJOINS
        // the activity it was given; a bare run MINTS its own. Nil can never re-enter as
        // a room because ActivityRoom refuses it at construction.
        #[test]
        fn a_room_param_is_rejoined_and_nil_is_unrepresentable() {
            let real = Uuid::from_u128(7);
            let rejoined = ActivityRoom::from_uuid(real).expect("real room rejoins");
            assert_eq!(rejoined.as_uuid(), real, "the room must survive unchanged");
            assert!(ActivityRoom::from_uuid(Uuid::nil()).is_err(), "nil is refused");
            let minted = ActivityRoom::mint();
            assert!(!minted.as_uuid().is_nil(), "a bare solve mints a REAL activity");
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
