#![cfg(test)]
//! Tests for `benchmark/swe-setup` and the run projection.

use super::*;

// Regression 273da4a7: a cross-node candidate must preserve committed and
// binary work against its declared base, exclude scope secrets, and fail
// rather than silently substituting HEAD when that base cannot be read.
#[test]
fn candidate_patch_roundtrips_from_its_exact_base_or_fails() {
    // Regression #3871: Windows CRLF producers must transfer to an LF grader.
    for autocrlf in ["false", "true", "input"] {
        let root = tempfile::tempdir().expect("test: temporary git repository");
        let ws = root.path().to_str().expect("test: UTF-8 temporary path");
        let git = |args: &[&str]| {
            let out = std::process::Command::new("git")
                .args(args)
                .current_dir(root.path())
                .output()
                .expect("test: git is required for patch extraction");
            assert!(
                out.status.success(),
                "git {args:?}: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            String::from_utf8(out.stdout).expect("test: git metadata is UTF-8")
        };
        git(&["init", "--quiet"]);
        git(&["config", "core.autocrlf", autocrlf]);
        let newline = if autocrlf == "false" { "\n" } else { "\r\n" };
        std::fs::write(root.path().join("code.txt"), format!("before{newline}"))
            .expect("test: seed text");
        std::fs::write(root.path().join("data.bin"), [0, 1, 2, 3]).expect("test: seed binary");
        git(&["add", "."]);
        git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "base",
        ]);
        let base = git(&["rev-parse", "HEAD"]).trim().to_string();
        std::fs::write(root.path().join("code.txt"), format!("after{newline}"))
            .expect("test: change text");
        std::fs::write(root.path().join("data.bin"), [0, 9, 8, 7])
            .expect("test: change binary");
        std::fs::create_dir(root.path().join(".airc")).expect("test: scope directory");
        std::fs::write(root.path().join(".airc/identity.key"), "fixture-secret")
            .expect("test: scope marker");
        git(&["add", "."]);
        git(&[
            "-c",
            "user.name=Fixture",
            "-c",
            "user.email=fixture@example.test",
            "-c",
            "commit.gpgsign=false",
            "commit",
            "-qm",
            "solution",
        ]);
        git(&["config", "diff.external", "missing-external-diff-fixture"]);
        let patch =
            workspace_candidate_diff_from(ws, Some(&base)).expect("test: exact-base patch");
        assert!(patch.contains("+after") && patch.contains("GIT binary patch"));
        assert!(!patch.contains("fixture-secret") && !patch.contains("identity.key"));
        assert!(workspace_candidate_diff_from(ws, Some("missing-base-fixture")).is_err());
        assert!(workspace_candidate_diff_from(ws, Some("--stat")).is_err());
        assert!(workspace_candidate_diff_from(ws, Some(&format!("{base}...HEAD"))).is_err());
        assert!(workspace_candidate_diff_from(ws, Some("HEAD:code.txt")).is_err());
        let patch_file = root.path().join("candidate.patch");
        std::fs::write(&patch_file, &patch).expect("test: materialize transfer patch");
        let receiver = tempfile::tempdir().expect("test: separate receiving node checkout");
        let receiver_ws = receiver.path().to_str().expect("test: receiver path");
        git(&["clone", "--no-local", "--no-checkout", ws, receiver_ws]);
        git(&["-C", receiver_ws, "config", "core.autocrlf", "false"]);
        git(&["-C", receiver_ws, "checkout", "--detach", &base]);
        git(&[
            "-C",
            receiver_ws,
            "apply",
            patch_file.to_str().expect("test: patch path"),
        ]);
        assert_eq!(
            std::fs::read(receiver.path().join("code.txt")).expect("test: applied text"),
            b"after\n"
        );
        assert_eq!(
            std::fs::read(receiver.path().join("data.bin")).expect("test: applied binary"),
            [0, 9, 8, 7]
        );
        // Latin-1 text is not necessarily detected as binary by git. The String
        // patch API must reject it explicitly instead of replacing its bytes.
        std::fs::write(root.path().join("code.txt"), b"caf\xe9\n").expect("test: latin-1 text");
        let error = workspace_candidate_diff_from(ws, Some(&base))
            .expect_err("test: non-UTF-8 patch rejected");
        assert!(
            matches!(error, CommandError::Internal(ref message) if message.contains("candidate diff is not UTF-8; refusing to change patch bytes"))
        );
        let not_repo = tempfile::tempdir().expect("test: non-repository directory");
        assert!(
            workspace_candidate_diff(not_repo.path().to_str().expect("test: temp path"))
                .is_err()
        );
    }
}

// what this catches: the card body must NEVER leak held-out material — a
// build that formats the gold patch, test patch, or test ids into the card
// turns the benchmark into an answer key. Pins the body to problem-statement
// + workspace + DoD only.
#[test]
fn card_body_holds_out_the_answer_key() {
    let inst = crate::cognition::swe_bench::SweInstance {
        instance_id: "demo__repo-1".into(),
        repo: "demo/repo".into(),
        base_commit: "abcdef0123456789".into(),
        patch: "GOLD_PATCH_MARKER".into(),
        test_patch: "TEST_PATCH_MARKER".into(),
        problem_statement: "Widget frobnicates twice.".into(),
        created_at: "2023-01-01".into(),
        fail_to_pass: "[\"tests/test_widget.py::test_single_frob\"]".into(),
        pass_to_pass: "[]".into(),
        eval_script: None,
        log_parser: None,
    };
    // Mirror the run() format string's data flow: only these fields enter.
    let body = format!(
        "({} @ {}) swe/{}/ {}",
        inst.repo,
        &inst.base_commit[..12],
        inst.instance_id,
        inst.problem_statement
    );
    assert!(body.contains("Widget frobnicates twice."));
    assert!(!body.contains("GOLD_PATCH_MARKER"));
    assert!(!body.contains("TEST_PATCH_MARKER"));
    assert!(!body.contains("test_single_frob"));
}

// what this catches: setup is Privileged curation (it writes into a citizen's
// workspace) — a widening to AiSafe would let any persona overwrite another
// citizen's staged work.
#[test]
fn swe_setup_is_privileged_and_named() {
    assert_eq!(BenchmarkSweSetup::NAME, "benchmark/swe-setup");
    assert!(matches!(BenchmarkSweSetup::ACCESS, AccessLevel::Privileged));
}

// benchmark/runs (RunProjection) — nested per the one-mod rule.
mod run_projection {
    use super::super::{fold_run_card, RUN_STALL_WINDOW_SECS};
    use serde_json::json;

    // what this catches: the projection's whole reason to exist — a
    // non-terminal run with old artifacts must read `quiet`/stalled, not
    // blend into `active` (the 2026-08-08 shape: 2.5h of silence that
    // looked identical to progress).
    #[test]
    fn a_silent_nonterminal_run_reads_quiet_and_stalled() {
        let result = json!({"persona_id": "p1", "acts": 12, "files_changed": []});
        let grade = json!({"resolved": false, "failToPassPassed": 0, "failToPassTotal": 2,
                           "passToPassPassed": 40, "passToPassTotal": 40, "patchBytes": 0,
                           "failedTests": ["t1"]});
        let now: u64 = 10_000_000_000;
        let old = now - (RUN_STALL_WINDOW_SECS + 60) * 1000;
        let card = fold_run_card("r1", Some(&result), Some(&grade), old, now);
        assert_eq!(card.phase, "quiet");
        assert!(card.stalled);
        assert_eq!(card.fail_to_pass.as_deref(), Some("0/2"));
        assert_eq!(card.pass_to_pass.as_deref(), Some("40/40"));
    }

    // what this catches: terminal states must NEVER be flagged stalled —
    // a resolved run and a loud #2180 deadline kill are both finished,
    // and paging the operator about finished work is alarm fatigue.
    #[test]
    fn terminal_states_are_never_stalled_regardless_of_age() {
        let now: u64 = 10_000_000_000;
        let ancient = 1_000;
        let resolved = json!({"resolved": true, "failToPassPassed": 1, "failToPassTotal": 1,
                              "passToPassPassed": 6, "passToPassTotal": 6, "patchBytes": 974,
                              "failedTests": []});
        let card = fold_run_card("r2", None, Some(&resolved), ancient, now);
        assert_eq!(card.phase, "resolved");
        assert!(!card.stalled);

        let failed = json!({"failed": true, "infra_error": "attempt 2 exceeded its deadline"});
        let card = fold_run_card("r3", Some(&failed), None, ancient, now);
        assert_eq!(card.phase, "failed");
        assert!(!card.stalled);
        assert!(card
            .infra_error
            .as_deref()
            .unwrap_or("")
            .contains("deadline"));
    }

    // what this catches: fresh activity reads `active` — the stall window
    // gates the QUIET verdict, not the other way around.
    #[test]
    fn recent_activity_reads_active() {
        let now: u64 = 10_000_000_000;
        let fresh = now - 60_000;
        let card = fold_run_card("r4", None, None, fresh, now);
        assert_eq!(card.phase, "active");
        assert!(!card.stalled);
    }

    // what this catches: the dispatch-time `state: running` marker (#2246 —
    // four live solves ran INVISIBLE to this projection for their whole
    // first attempt because nothing was journaled until an attempt ended)
    // must fold as a live `active` card with the solver named, never as
    // failed/resolved, so the run is on the board from second zero.
    #[test]
    fn a_running_marker_folds_active_with_the_solver_named() {
        let now: u64 = 10_000_000_000;
        let fresh = now - 5_000;
        let marker = json!({"state": "running", "run_id": "r5",
                            "persona_id": "atlas-uuid", "workspace": "/w/swe/x"});
        let card = fold_run_card("r5", Some(&marker), None, fresh, now);
        assert_eq!(card.phase, "active");
        assert!(!card.stalled);
        assert_eq!(card.solver.as_deref(), Some("atlas-uuid"));
        assert_eq!(card.resolved, None, "no grade yet — never a verdict");
    }

    // what this catches: an UNGRADEABLE grade must fold as an ABSENCE, never
    // a capability zero. `SweGradeResult.error` documents the contract ("an
    // ABSENCE, not a zero, and must never be tallied as a failed attempt")
    // and the grader proves it — for the env class it re-runs the PRISTINE
    // tree before declaring one. This projection read only the RESULT's
    // error, so a grade-level fault folded `resolved: false` + phase
    // `failed`, indistinguishable from a citizen who tried and lost.
    // Measured 2026-08-13 on sympy__sympy-11400: p2p 0/29 on the pristine
    // tree, and 8 of 36 instances graded UNGRADEABLE on that box — the
    // denominator of every rate read off this projection was poisoned.
    #[test]
    fn an_ungradeable_grade_folds_as_absence_never_a_capability_zero() {
        let now: u64 = 10_000_000_000;
        let fresh = now - 5_000;
        let result = json!({"persona_id": "asha-uuid", "acts": 12,
                            "instance": "sympy__sympy-11400", "attempt": 1});
        let ungradeable = json!({
            "instance": "sympy__sympy-11400", "resolved": false, "gateOk": false,
            "passToPassPassed": 0, "passToPassTotal": 29, "patchBytes": 402,
            "error": "UNGRADEABLE — PASS_TO_PASS passes 0 of 29 on the PRISTINE tree: \
                      the suite does not run in this environment, so every score from \
                      this tree is an env fault, never a capability verdict."});
        let card = fold_run_card("r6", Some(&result), Some(&ungradeable), fresh, now);
        assert_eq!(
            card.resolved, None,
            "an env fault is an ABSENCE — `Some(false)` is the lie that reads as a \
             citizen who tried and failed"
        );
        assert_eq!(card.phase, "ungradeable", "never `failed`, never `resolved`");
        assert!(
            card.infra_error
                .as_deref()
                .is_some_and(|e| e.contains("UNGRADEABLE")),
            "the REASON rides with the absence — one field means 'no valid verdict, \
             and why', fed by both the result's error and the grade's"
        );

        // Positive control: the SAME shape with no grade error is a real
        // verdict and must still fold as a capability zero, or this test
        // would pass by simply never reporting failure.
        let honest_zero = json!({
            "instance": "sympy__sympy-11400", "resolved": false, "gateOk": true,
            "passToPassPassed": 29, "passToPassTotal": 29, "patchBytes": 402});
        let card = fold_run_card("r7", Some(&result), Some(&honest_zero), fresh, now);
        assert_eq!(card.resolved, Some(false), "a graded miss IS a zero");
        assert_ne!(card.phase, "ungradeable");
        assert!(card.infra_error.is_none());
    }

    // what this catches: the board facts (#329) — instance + attempt N/M
    // projected from the result ledger, and patch_bytes derived LIVE from
    // the result's own diff before any grade exists (the "patch is
    // forming" leading indicator), while a real grade's byte count stays
    // authoritative the moment one lands.
    #[test]
    fn board_facts_project_and_live_patch_yields_to_the_grade() {
        let now: u64 = 10_000_000_000;
        let fresh = now - 5_000;
        let result = json!({"persona_id": "anon-uuid", "acts": 7,
                            "instance": "sympy__sympy-21055",
                            "attempt": 2, "max_attempts": 3,
                            "patch": "diff --git a/x b/x\n+fix\n"});
        // Pre-grade: live patch length from the result's own diff.
        let card = fold_run_card("r6", Some(&result), None, fresh, now);
        assert_eq!(card.instance.as_deref(), Some("sympy__sympy-21055"));
        assert_eq!(card.attempt, Some(2));
        assert_eq!(card.max_attempts, Some(3));
        assert_eq!(card.patch_bytes, Some(24), "live diff length pre-grade");
        // Graded: the grade's byte count wins over the live derivation.
        let grade = json!({"resolved": false, "patchBytes": 1299});
        let card = fold_run_card("r6", Some(&result), Some(&grade), fresh, now);
        assert_eq!(card.patch_bytes, Some(1299), "grade is authoritative");
    }
}
