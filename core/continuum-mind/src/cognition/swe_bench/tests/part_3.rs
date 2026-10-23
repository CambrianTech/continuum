#![cfg(test)]
use super::*;

    // what this catches: django report ids never resolving. The dataset's FAIL_TO_PASS
    // shape is `test_name (module.Class)`; django ≤4.0 prints exactly that, ≥4.1 appends
    // the method to the class path — both must hit the same dataset id, and the bare-name
    // fallback must extract the LEADING token (the rsplit('.') chain used for pytest ids
    // would extract `Class)` and miss forever).
    #[test]
    pub(super) fn a_django_report_resolves_dataset_shaped_ids_across_django_versions() {
        let report = "\
Testing against Django installed in '/x/django'
test_combine (expressions.tests.CombinedExprTests) ... ok
test_broken (expressions.tests.CombinedExprTests) ... FAIL
test_new_form (migrations.test_operations.OperationTests.test_new_form) ... ok
test_errored (migrations.test_operations.OperationTests.test_errored) ... ERROR
test_skipped (expressions.tests.SkipTests) ... skipped 'no oracle'
System check identified no issues (0 silenced).
FAIL: test_broken (expressions.tests.CombinedExprTests)";
        let (by_node, by_func) = parse_django_report(report);

        assert_eq!(
            verdict_for(
                "test_combine (expressions.tests.CombinedExprTests)",
                &by_node,
                &by_func
            ),
            Some(true)
        );
        assert_eq!(
            verdict_for(
                "test_broken (expressions.tests.CombinedExprTests)",
                &by_node,
                &by_func
            ),
            Some(false),
            "the `FAIL:` detail header must not overwrite the per-test verdict"
        );
        // django ≥4.1 line shape normalizes back to the dataset id.
        assert_eq!(
            verdict_for(
                "test_new_form (migrations.test_operations.OperationTests)",
                &by_node,
                &by_func
            ),
            Some(true)
        );
        assert_eq!(
            verdict_for(
                "test_errored (migrations.test_operations.OperationTests)",
                &by_node,
                &by_func
            ),
            Some(false),
            "ERROR is a failure, exactly as in the pytest parser"
        );
        assert_eq!(
            verdict_for(
                "test_skipped (expressions.tests.SkipTests)",
                &by_node,
                &by_func
            ),
            Some(true),
            "skipped counts as not-failed, matching the pytest parser's SKIPPED rule"
        );
        // Bare-name fallback goes through the leading token, not rsplit('.').
        assert_eq!(
            verdict_for("test_combine (some.other.Class)", &by_node, &by_func),
            Some(true)
        );
        assert_eq!(
            verdict_for("test_never_ran (expressions.tests.X)", &by_node, &by_func),
            None,
            "an id nothing in the report matches is UNKNOWN, never a silent pass"
        );
    }

    // Per-env-class POSITIVE CONTROLS (#380's gold gate): build the REAL env for one
    // instance of a class that previously could not be graded and prove its own harness
    // executes on the pristine tree. Network- and minutes-heavy, so compile-time gated
    // like every other load test: `cargo test --features stress-tests`.
    #[cfg(feature = "stress-tests")]
    pub(super) mod stress {
        use super::super::*;

        async fn lite_instance(id: &str) -> SweInstance {
            load_dataset("princeton-nlp/SWE-bench_Lite")
                .await
                .expect("dataset")
                .into_iter()
                .find(|i| i.instance_id == id)
                .unwrap_or_else(|| panic!("{id} not in Lite"))
        }

        // what this catches: the astropy env class (#383 cause 2) — pyerfa's sdist build
        // imports jinja2, which `--no-build-isolation` never installs, so every astropy
        // instance died at env build (`ModuleNotFoundError: No module named 'jinja2'`,
        // live 2026-08-16). Building the real env end-to-end is the only honest proof.
        #[tokio::test(flavor = "multi_thread")]
        async fn positive_control_the_astropy_env_class_builds() {
            let inst = lite_instance("astropy__astropy-12907").await;
            let work = swe_cache_dir().join("work").join(&inst.instance_id);
            clone_at(&inst, &work).await.expect("clone at base_commit");
            let py = ensure_env(&inst, &work).await.expect("env build");
            assert!(py.exists(), "venv python must exist: {}", py.display());
        }

        // what this catches: the django env+runner class (#383 cause 1) — on the pristine
        // tree with the test_patch applied, django's OWN runner must produce the known
        // profile: every FAIL_TO_PASS id resolves to a definite FAIL (the bug is present
        // and the report parses). An id resolving to None means the runner/report seam is
        // broken; resolving to true means the tree isn't at base.
        #[tokio::test(flavor = "multi_thread")]
        async fn positive_control_a_django_instance_shows_the_known_fail_profile() {
            let inst = lite_instance("django__django-16139").await;
            assert_eq!(runner_for_repo(&inst.repo), TestRunner::DjangoRuntests);
            let work = swe_cache_dir().join("work").join(&inst.instance_id);
            clone_at(&inst, &work).await.expect("clone at base_commit");
            let py = ensure_env(&inst, &work).await.expect("env build");
            apply_patch(&work, &inst.test_patch, "gate-test")
                .await
                .expect("test patch applies");
            let files = patched_test_files(&inst.test_patch);
            let (_, report) = run_tests(
                &work,
                &py,
                &inst.f2p(),
                &files,
                TestRunner::DjangoRuntests,
            )
            .await;
            assert!(!report.is_empty(), "runtests.py produced no output at all");
            // `run_tests` collapses UNKNOWN to false, which would let a broken seam
            // masquerade as the known-fail profile — so require a DEFINITE fail: the
            // report line was found, parsed, and says the bug is present.
            let (by_node, by_func) = parse_django_report(&report);
            for id in inst.f2p() {
                assert_eq!(
                    verdict_for(&id, &by_node, &by_func),
                    Some(false),
                    "{id} must resolve to a DEFINITE fail on the pristine tree; None \
                     means the runner/report seam never saw it, true means the tree \
                     is not at base. Report tail:\n{}",
                    report_tail(&report)
                );
            }
            reset_worktree(&work).await;
        }
    }

    // what this catches: the pristine gate voiding an instance that still has a failing
    // FAIL_TO_PASS test (django-13346 on 2026-09-07: one f2p green on this backend, the real
    // bug's test red → ten instances refused, gold patch included).
    #[test]
    pub(super) fn the_pristine_gate_voids_only_when_every_f2p_test_is_already_green() {
        let t = |id: &str, ok: bool| (id.to_string(), ok);
        assert_eq!(
            pristine_gate(&[t("test_key_iregex", true), t("test_key_in", false)]),
            PristineGate::Holds { pristine_green: vec!["test_key_iregex".to_string()] }
        );
        assert_eq!(
            pristine_gate(&[t("a", true), t("b", true)]),
            PristineGate::Void { already: vec!["a".to_string(), "b".to_string()] }
        );
        assert_eq!(
            pristine_gate(&[t("a", false)]),
            PristineGate::Holds { pristine_green: vec![] }
        );
        // No f2p tests at all is not "everything already passes": nothing to void on.
        assert_eq!(pristine_gate(&[]), PristineGate::Holds { pristine_green: vec![] });
    }

    // what this catches: the harness erasing a resolve the team earned (sympy-22456,
    // 2026-09-07: resolved 14:13Z by one citizen, overwritten 16:15Z by another citizen's
    // failed regrade of the same instance, and the reported score lost a point that a real
    // patch had won).
    #[test]
    pub(super) fn a_resolve_is_never_replaced_by_a_later_failure() {
        let resolved = SweVerdict { instance_id: "sympy__sympy-22456".into(), resolved: true, f2p_passed: 1, f2p_total: 1, ..Default::default() };
        let failed = SweVerdict { instance_id: "sympy__sympy-22456".into(), resolved: false, f2p_passed: 0, f2p_total: 1, ..Default::default() };
        assert_eq!(verdict_write_for(Some(&resolved), &failed), VerdictWrite::KeepTheResolve);
        // Everything else records: nothing on disk, a fresh resolve, or failure over failure.
        assert_eq!(verdict_write_for(None, &failed), VerdictWrite::Record);
        assert_eq!(verdict_write_for(Some(&failed), &resolved), VerdictWrite::Record, "a resolve is always an improvement");
        assert_eq!(verdict_write_for(Some(&failed), &failed), VerdictWrite::Record, "the freshest failure detail is what she acts on");
        assert_eq!(verdict_write_for(Some(&resolved), &resolved), VerdictWrite::Record);
    }
