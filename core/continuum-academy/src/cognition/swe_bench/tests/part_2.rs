#![cfg(test)]
use super::*;

    // what this catches: the wheel-less era sdist (pylint-5859, live 2026-08-11) —
    // lazy-object-proxy 1.7.1 has no arm64 wheel and its sdist stamps 0.0.0, so the
    // date-pinned install can NEVER succeed; the heal must lift exactly that package's
    // cutoff. Unrelated errors and mismatches without a named package parse to None.
    #[test]
    pub(super) fn metadata_mismatch_parses_into_a_lifted_cutoff() {
        let stderr = "\u{2570}\u{2500}\u{25b6} Package metadata version `0.0.0` does not \
            match given version `1.7.1`\n\nhint: `lazy-object-proxy` (v1.7.1) was included \
            because `pylint` (v2.13.0.dev0) depends on `astroid` (v2.9.3)";
        assert_eq!(
            metadata_mismatch_override(stderr).as_deref(),
            Some("lazy-object-proxy=9999-01-01T00:00:00Z"),
        );
        assert_eq!(
            metadata_mismatch_override("error: some other failure"),
            None
        );
        assert_eq!(
            metadata_mismatch_override(
                "Package metadata version `0.0.0` does not match given version `1.0` (no hint)"
            ),
            None,
            "mismatch without a named package must not synthesize an override"
        );
    }

    // what this catches: the setuptools/importlib-metadata build clash (pytest-5413 fresh
    // env, live 2026-08-12) — a 2019 graph pulls importlib-metadata 0.x, which crashes the
    // modern setuptools that --no-build-isolation builds with; the heal lifts exactly that
    // package's cutoff. Unrelated errors parse to None.
    #[test]
    pub(super) fn setuptools_importlib_clash_lifts_that_packages_cutoff() {
        let stderr = "SetuptoolsWarning: Incompatibility problem.\n\
            `importlib-metadata` version is incompatible with `setuptools`.\n\
            This problem is likely to be solved by installing an updated version of \
            `importlib-metadata`.";
        assert_eq!(
            setuptools_importlib_clash_override(stderr).as_deref(),
            Some("importlib-metadata=9999-01-01T00:00:00Z"),
        );
        assert_eq!(
            setuptools_importlib_clash_override("error: unrelated"),
            None
        );
    }

    // what this catches: the hidden-collateral verdict (atlas-24066-n7) — a patch that
    // broke 30 pass-to-pass tests produced retry feedback built from the f2p report
    // alone, so she resubmitted the identical broken patch twice. The REGRESSION
    // section must lead the excerpt whenever p2p broke, name the broken tests, and
    // still carry the f2p tail after it; no-breakage keeps the old f2p-only shape.
    // what this catches (2026-09-12, ruff-15309): a test build that never ran reported as
    // "REGRESSION — you broke 4 tests", the compiler's line buried in a warning tail.
    // what this catches (2026-09-12, sklearn-26323): a skipped FAIL_TO_PASS test read as green
    // (pytest `SKIPPED`) or as a pass (Django's row), voiding or failing a correct fix.
    #[test]
    pub(super) fn a_skipped_test_is_named_in_both_report_shapes() {
        let pytest = "sklearn/compose/tests/test_column_transformer.py::test_remainder_set_output SKIPPED (could not import 'pandas')\nsklearn/compose/tests/test_a.py::test_ok PASSED\n";
        assert_eq!(skipped_in_report(pytest), vec!["sklearn/compose/tests/test_column_transformer.py::test_remainder_set_output".to_string()]);
        let django = "CONTINUUM_TEST {\"id\": \"a.B.test_x\", \"ok\": true, \"skipped\": true}\nCONTINUUM_TEST {\"id\": \"a.B.test_y\", \"ok\": true}\n";
        assert_eq!(skipped_in_report(django), vec!["a.B.test_x".to_string()]);
        let wanted = vec!["sklearn/compose/tests/test_column_transformer.py::test_remainder_set_output".to_string(), "a.B.test_y".to_string()];
        assert_eq!(skipped_of(&wanted, &skipped_in_report(pytest)).len(), 1);
    }

    // what this catches (2026-09-12, django-16667): a near-miss verdict's excerpt showing the
    // report's tail of `ok` rows instead of the failing test's assertion.
    #[test]
    pub(super) fn a_near_miss_excerpt_quotes_the_failing_tests_own_section() {
        let pytest = "tests/a.py::test_ok PASSED\n=================================== FAILURES ===================================\n_____________________ test_remainder_set_output _____________________\n\n    def test_remainder_set_output():\n>       assert isinstance(out, pd.DataFrame)\nE       AssertionError: assert False\n\ntests/a.py:12: AssertionError\n=========================== short test summary info ============================\nFAILED tests/a.py::test_remainder_set_output - AssertionError: assert False\n";
        let e = failing_test_excerpt(pytest).expect("a section exists");
        assert!(e.starts_with("_____"), "{e}");
        assert!(e.contains("AssertionError: assert False"));
        assert!(!e.contains("test_ok PASSED"), "passing rows are not excerpt material");
        let unittest = "CONTINUUM_TEST {\"id\": \"a.B.test_y\", \"ok\": true}\nFAIL: test_x (a.B)\n----------------------------------------------------------------------\nTraceback (most recent call last):\n  File \"a.py\", line 3, in test_x\nAssertionError: 1 != 2\n\n----------------------------------------------------------------------\nRan 2 tests\n";
        let e = failing_test_excerpt(unittest).expect("unittest block");
        assert!(e.starts_with("FAIL: test_x"));
        assert!(e.contains("AssertionError: 1 != 2"));
        assert!(!e.contains("\"ok\": true"));
        assert_eq!(failing_test_excerpt("nothing here\n"), None);
    }

    #[test]
    pub(super) fn a_tree_that_does_not_compile_is_told_so_with_the_compilers_first_error_leading() {
        let report = "warning: hiding a lifetime that's elided elsewhere is confusing\n  --> crates/x.rs:60:23\n\n   Compiling ruff_linter v0.8.6\nerror[E0609]: no field `args` on type `&ruff_python_ast::ExprCall`\n  --> crates/ruff_linter/src/rules/pyflakes/fixes.rs:99:10\n   |\n99 |         .args\n   |          ^^^^ unknown field\nhelp: one of the expressions' fields has a field of the same name\n\nerror: could not compile `ruff_linter` (lib test) due to 2 previous errors\n";
        let excerpt = build_failure_excerpt(report).expect("a build failure is named");
        assert!(excerpt.starts_with("error[E0609]: no field `args`"), "{excerpt}");
        assert!(excerpt.contains("fixes.rs:99:10"), "the location rides along: {excerpt}");
        assert!(!excerpt.contains("hiding a lifetime"), "warnings before the error are not the story");
        assert_eq!(build_failure_excerpt("test a::b ... ok\ntest a::c ... FAILED\n"), None, "a test run is not a build failure");
    }

    #[test]
    pub(super) fn regression_breakage_leads_the_failure_excerpt() {
        let broken: Vec<String> = (0..12).map(|i| format!("test_p2p_{i}")).collect();
        let both = compose_failure_excerpt(
            &broken,
            "E ImportError: cannot import name 'Exp'",
            true,
            "E AssertionError: target still fails",
        )
        .expect("both sections");
        assert!(both.starts_with("REGRESSION"), "breakage must LEAD: {both}");
        assert!(both.contains("BROKE 12 test(s)"));
        assert!(
            both.contains("test_p2p_0") && both.contains("(+2 more)"),
            "names capped at 10: {both}"
        );
        assert!(
            both.contains("ImportError") && both.contains("AssertionError"),
            "both report tails present"
        );
        let regression_at = both.find("REGRESSION").unwrap();
        let f2p_at = both.find("AssertionError").unwrap();
        assert!(regression_at < f2p_at, "regression before target-test tail");

        let clean = compose_failure_excerpt(&[], "", true, "E AssertionError: target still fails")
            .expect("f2p-only");
        assert!(
            !clean.contains("REGRESSION"),
            "no fabricated regression on a clean tree"
        );

        assert!(
            compose_failure_excerpt(&[], "", false, "noise").is_none(),
            "nothing failing → no excerpt"
        );
    }

    // what this catches: the id-shape assumption that mis-scored GOLD as a real failure.
    // sympy's FAIL_TO_PASS entries are bare function names because sympy ships its own runner;
    // handing those to pytest as paths produced "file or directory not found", scored as a
    // failing test, and returned RESOLVED=0 on three instances whose environments were fine.
    #[test]
    pub(super) fn a_required_id_resolves_by_node_id_or_by_bare_function_name() {
        let report = "\
tests/test_polysys.py::test_solve_poly_system PASSED [ 50%]
tests/test_polysys.py::test_solve_biquadratic FAILED [100%]
tests/test_x.py::TestC::test_param[3-4] PASSED";
        let (by_node, by_func) = parse_pytest_report(report);

        // flask/pytest shape — full node id.
        assert_eq!(
            verdict_for(
                "tests/test_polysys.py::test_solve_poly_system",
                &by_node,
                &by_func
            ),
            Some(true)
        );
        // sympy shape — BARE function name, the case that was broken.
        assert_eq!(
            verdict_for("test_solve_poly_system", &by_node, &by_func),
            Some(true)
        );
        assert_eq!(
            verdict_for("test_solve_biquadratic", &by_node, &by_func),
            Some(false)
        );
        // parametrised tests resolve by their base name.
        assert_eq!(verdict_for("test_param", &by_node, &by_func), Some(true));
        // an id nothing in the report matches is UNKNOWN, never a silent pass.
        assert_eq!(verdict_for("test_never_ran", &by_node, &by_func), None);
    }

    // what this catches: a COLORIZED report scoring 0 of N and being blamed on the environment.
    // Live incident 2026-08-18 (astropy-14995): astropy's own setup.cfg carries
    // `addopts = --color=yes`, so pytest emitted color onto a pipe. The escape lands INSIDE the
    // node id and before the verdict, `starts_with("PASSED")` never matched, and the grader
    // reported "UNGRADEABLE — PASS_TO_PASS passes 0 of 40 on the PRISTINE tree: the suite does
    // not run in this environment" — in a run whose own tail read `1 failed, 179 passed`. The
    // env was fine; the grader could not read it. Bytes below are the real shape pytest emits.
    #[test]
    pub(super) fn a_colorized_report_parses_exactly_like_a_plain_one() {
        let plain = "\
astropy/nddata/mixins/tests/test_ndarithmetic.py::test_nddata_bitmask_arithmetic FAILED
astropy/nddata/mixins/tests/test_ndarithmetic.py::test_arithmetics_data PASSED";
        let colored = "\
astropy/nddata/mixins/tests/test_ndarithmetic.py::\u{1b}[1mtest_nddata_bitmask_arithmetic\u{1b}[0m \u{1b}[31mFAILED\u{1b}[0m
astropy/nddata/mixins/tests/test_ndarithmetic.py::\u{1b}[1mtest_arithmetics_data\u{1b}[0m \u{1b}[32mPASSED\u{1b}[0m";

        let (plain_node, plain_func) = parse_pytest_report(plain);
        let (color_node, color_func) = parse_pytest_report(colored);
        assert_eq!(
            color_node, plain_node,
            "color must not change which node ids were seen"
        );
        assert_eq!(color_func, plain_func, "nor the bare-name verdicts");

        // The specific thing that was returning "0 of N": a PASS must read as a pass.
        assert_eq!(
            verdict_for("test_arithmetics_data", &color_node, &color_func),
            Some(true)
        );
        assert_eq!(
            verdict_for("test_nddata_bitmask_arithmetic", &color_node, &color_func),
            Some(false)
        );
        assert!(
            !color_node.is_empty(),
            "an empty map is the failure mode that got misreported as a broken environment"
        );
    }

    // what this catches: strip_ansi corrupting ordinary text, or allocating when it need not.
    #[test]
    pub(super) fn strip_ansi_is_borrow_only_when_clean_and_lossless_when_not() {
        assert!(matches!(
            strip_ansi("plain text ::  PASSED"),
            std::borrow::Cow::Borrowed(_)
        ));
        assert_eq!(strip_ansi("\u{1b}[32mPASSED\u{1b}[0m"), "PASSED");
        // OSC (title-set) sequences terminate on BEL or ST, and swallow neither more nor less.
        assert_eq!(strip_ansi("a\u{1b}]0;title\u{7}b"), "ab");
        assert_eq!(strip_ansi("a\u{1b}]0;title\u{1b}\\b"), "ab");
        // A truncated escape at end-of-input must not panic or emit garbage.
        assert_eq!(strip_ansi("tail\u{1b}"), "tail");
        assert_eq!(strip_ansi("tail\u{1b}["), "tail");
    }

    // what this catches: a bare name appearing in two files where one fails — counting it as
    // passing would let real breakage through.
    #[test]
    pub(super) fn a_duplicated_bare_name_passes_only_if_every_occurrence_passed() {
        let report = "\
tests/a.py::test_shared PASSED
tests/b.py::test_shared FAILED";
        let (by_node, by_func) = parse_pytest_report(report);
        assert_eq!(verdict_for("test_shared", &by_node, &by_func), Some(false));
        assert_eq!(
            verdict_for("tests/a.py::test_shared", &by_node, &by_func),
            Some(true)
        );
    }

    // what this catches: the scope of the test run. Running the whole suite is slow and
    // running the wrong file scores nothing; the instance's own test_patch names the files.
    #[test]
    pub(super) fn the_test_scope_comes_from_the_instances_own_test_patch() {
        let patch = "\
diff --git a/tests/test_polysys.py b/tests/test_polysys.py
--- a/tests/test_polysys.py
+++ b/tests/test_polysys.py
@@ -1 +1,2 @@
 x
+y
diff --git a/sympy/solvers/tests/test_other.py b/sympy/solvers/tests/test_other.py
--- a/sympy/solvers/tests/test_other.py
+++ b/sympy/solvers/tests/test_other.py
@@ -1 +1,2 @@
 a
+b";
        assert_eq!(
            patched_test_files(patch),
            vec![
                "sympy/solvers/tests/test_other.py".to_string(),
                "tests/test_polysys.py".to_string()
            ]
        );
        assert!(patched_test_files("").is_empty());
    }

    // what this catches: the INTERPRETER has an era too, not just the dependency graph. A 2014
    // requests vendors a urllib3 doing `from collections import Mapping`, deleted in 3.10 — no
    // dependency pin can rescue that, the language moved. The 3.10 rung for 2020..=2022 is the
    // same lesson from the other side (pylint-5859, live 2026-08-11): 3.11 removed
    // `inspect.formatargspec`, which era sdist BUILDS (wrapt 1.13.3) still import — a
    // March-2022 graph never targeted an interpreter released 2022-10.
    #[test]
    pub(super) fn the_interpreter_is_chosen_by_the_instances_era() {
        assert_eq!(interpreter_for_year(2014), "3.9");
        assert_eq!(interpreter_for_year(2019), "3.9");
        // 2020/2021 pinned by the 2026-08-27 coverage map: era pytest 5.4/6.x
        // predates py3.10's AST ("lineno missing" in the assertion rewriter).
        assert_eq!(interpreter_for_year(2020), "3.8");
        assert_eq!(interpreter_for_year(2021), "3.9");
        assert_eq!(interpreter_for_year(2022), "3.10");
        assert_eq!(interpreter_for_year(2023), "3.11");
    }

    // what this catches: a REAL verdict must survive the process that produced it, and the
    // two verdicts that must NEVER be recorded must not be. Before 2026-08-18 nothing was
    // recorded at all: two genuine SWE-bench Lite resolutions were watched passing and the
    // system retained nothing, so the board kept calling their artifacts `ungraded`.
    //
    // The fixture goes through `record_verdict` — the PRODUCTION writer — and is read back
    // through `read_verdict`/`recorded_verdicts`, the readers the board actually uses. A
    // hand-authored file here would test my belief about the writer instead of the writer,
    // which is exactly how the boot reaper stayed green while pointed at a filename nothing
    // emits ([[run-ledgers-are-typed-artifacts]] L4).
    #[test]
    pub(super) fn a_real_verdict_persists_and_gold_or_errored_ones_never_do() {
        let home = tempfile::tempdir().expect("tmp");
        // `verdict_dir` derives from CONTINUUM_HOME via swe_cache_dir; isolate this test's
        // writes rather than touching the operator's real benchmarks root — under the one
        // crate-wide home lock (#4082), restored by the guard's drop.
        let _home = crate::paths::test_home::HomeGuard::set_blocking(home.path());
        std::env::set_var("CONTINUUM_HOME", home.path());

        let real = SweVerdict {
            instance_id: "astropy__astropy-14995".into(),
            resolved: true,
            f2p_passed: 1,
            f2p_total: 1,
            p2p_passed: 40,
            p2p_total: 40,
            gate_ok: true,
            ..Default::default()
        };
        assert!(record_verdict(&real, false).unwrap().is_some());

        let back = read_verdict("astropy__astropy-14995").expect("a recorded verdict reads back");
        assert!(back.resolved, "resolution survives the round trip");
        assert_eq!((back.f2p_passed, back.p2p_total), (1, 40), "counts survive too");
        assert_eq!(
            recorded_verdicts().len(),
            1,
            "and the board's enumerator sees exactly it"
        );

        // A gold pass proves the ENV, not the citizen. Recording it would render a positive
        // control as our result.
        let gold = SweVerdict {
            instance_id: "sympy__sympy-24152".into(),
            resolved: true,
            ..Default::default()
        };
        assert!(record_verdict(&gold, true).unwrap().is_none(), "gold never records");

        // An errored verdict is an ABSENCE, never a scored zero (#384). Two ways in: an env
        // fault, and — found by the live positive control the day this landed — an EMPTY
        // candidate. A pristine tree grades `resolved: false, gate_ok: true` forever, which
        // is indistinguishable from a citizen who tried and missed, so `swe-grade` now stamps
        // an empty candidate as an error rather than letting the board score her absence.
        let errored = SweVerdict {
            instance_id: "django__django-11049".into(),
            error: Some("no candidate patch to grade — the workspace holds no diff".into()),
            ..Default::default()
        };
        assert!(
            record_verdict(&errored, false).unwrap().is_none(),
            "an errored run is an absence, not a tallied failure"
        );

        assert_eq!(
            recorded_verdicts().len(),
            1,
            "neither the control nor the fault reached the durable record"
        );

        // THE STORE IS SHARED ACROSS ADAPTERS. A gym grade banks here through the same
        // writer (`modules::benchmark_grade::gym_verdict` -> `record_verdict`), and the
        // two identity fields must survive the trip: without them a reader has to infer
        // which benchmark scored a row from the SHAPE of `instance_id`, and can never
        // learn whose hands it scored at all. Regression for the gym rounds that resolved
        // on this box while every projection over this store reported zero.
        let gym = SweVerdict {
            instance_id: "conway_step".into(),
            resolved: true,
            f2p_passed: 1,
            f2p_total: 1,
            gate_ok: true,
            benchmark: "games-rs".into(),
            solver: "7711fe60-0000-0000-0000-00000000abcd".into(),
            ..Default::default()
        };
        assert!(record_verdict(&gym, false).unwrap().is_some(), "a gym pass is durable too");
        let back = read_verdict("conway_step").expect("the gym verdict reads back");
        assert_eq!(back.benchmark, "games-rs", "which ADAPTER scored it survives");
        assert_eq!(
            back.solver, "7711fe60-0000-0000-0000-00000000abcd",
            "whose hands were scored survives"
        );
        assert_eq!(
            recorded_verdicts().len(),
            2,
            "the board's enumerator sees SWE and gym rows in one store — one projection,              not a parallel gym board"
        );

    }

    // what this catches: test-id lists arrive JSON-encoded inside a string field, and a
    // missing/blank one must yield an empty list rather than panicking a whole run.
    #[test]
    pub(super) fn test_id_lists_decode_from_their_string_field() {
        let inst = SweInstance {
            instance_id: "x__y-1".into(),
            repo: "x/y".into(),
            base_commit: "abc".into(),
            patch: String::new(),
            test_patch: String::new(),
            problem_statement: String::new(),
            created_at: "2021-05-01T00:00:00Z".into(),
            fail_to_pass: "[\"test_a\", \"test_b\"]".into(),
            pass_to_pass: String::new(),
            eval_script: None,
            log_parser: None,
        };
        assert_eq!(inst.f2p(), vec!["test_a".to_string(), "test_b".to_string()]);
        assert!(inst.p2p().is_empty(), "a blank list must not panic the run");
        assert_eq!(inst.year(), 2021);
    }

    // what this catches: #383's headline — the grader ran pytest for EVERYTHING, so all
    // 114 django instances of Lite-300 (38%) were structurally ungradeable. The runner is
    // selected from the instance's repo field; django resolves to its own runtests.py with
    // the official harness's directive transform (tests/x/y.py → x.y), every pytest repo
    // keeps the exact pre-seam argv, and an UNKNOWN repo defaults to pytest so a new class
    // fails loudly through the pristine gates instead of inventing a third path.
    #[test]
    pub(super) fn the_runner_is_selected_by_repo_and_django_grades_through_runtests() {
        assert_eq!(runner_for_repo("django/django"), TestRunner::DjangoRuntests);
        for repo in ["sympy/sympy", "pytest-dev/pytest", "astropy/astropy", "never/heard-of-it"] {
            assert_eq!(runner_for_repo(repo), TestRunner::Pytest, "{repo}");
        }

        let files = vec![
            "tests/migrations/test_operations.py".to_string(),
            "tests/expressions/tests.py".to_string(),
        ];
        assert_eq!(
            test_invocation(TestRunner::DjangoRuntests, &files),
            vec![
                "tests/runtests.py",
                "--verbosity",
                "2",
                "--settings=test_sqlite",
                "--parallel",
                "1",
                "migrations.test_operations",
                "expressions.tests",
            ],
            "django's command shape mirrors the official per-repo spec"
        );
        assert_eq!(
            test_invocation(TestRunner::Pytest, &files[..1].to_vec()),
            vec![
                "-m",
                "pytest",
                "tests/migrations/test_operations.py",
                "-v",
                "-p",
                "no:cacheprovider",
                "--color=no",
            ],
            "the pytest argv pins the era-portable flag set, color explicitly OFF"
        );
    }

    // what this catches: selecting the JSON runner through a flag runtests.py does not have.
    // Shipped once as `--testrunner=continuum_json_runner.JsonRunner` — argparse rejected the
    // WHOLE invocation ("unrecognized arguments"), so django-10914's own GOLD patch graded
    // 0/40 in 4 seconds. runtests.py has no such flag in ANY era; it reads settings.TEST_RUNNER
    // and defaults it only when unset (verified in-tree at 1.11/2.2/3.2/4.2/5.2/main). So the
    // runner is selected by a settings MODULE, and the invocation must differ from the plain
    // one in exactly one argument — the settings value — and in nothing else.
    #[test]
    pub(super) fn the_json_runner_is_selected_through_settings_never_a_flag() {
        let files = vec!["tests/expressions/tests.py".to_string()];
        let plain = test_invocation_with(TestRunner::DjangoRuntests, &files, false);
        let json = test_invocation_with(TestRunner::DjangoRuntests, &files, true);

        assert_eq!(plain.len(), json.len(), "the JSON path adds no argument");
        let differences: Vec<_> = plain.iter().zip(&json).filter(|(a, b)| a != b).collect();
        assert_eq!(
            differences,
            vec![(
                &"--settings=test_sqlite".to_string(),
                &format!("--settings={DJANGO_JSON_SETTINGS_MODULE}")
            )],
            "settings is the ONLY difference: {plain:?} vs {json:?}"
        );
        assert!(
            !json.iter().any(|a| a.contains("--testrunner")),
            "runtests.py has no --testrunner flag; passing one aborts the run before any test"
        );
        // The shim must inherit django's own suite settings rather than restate them, or it
        // drifts from whatever the era's test_sqlite happens to configure.
        assert!(DJANGO_JSON_SETTINGS_SRC.contains("from test_sqlite import *"));
        assert!(DJANGO_JSON_SETTINGS_SRC
            .contains(&format!("TEST_RUNNER = \"{DJANGO_JSON_RUNNER_MODULE}.JsonRunner\"")));
    }

    // what this catches: a canonical id alone cannot grade django. SWE-bench harvested its
    // django ids from unittest's verbose log, and unittest prints a test's DOCSTRING instead
    // of its id when it has one — so for those tests the dataset's PASS_TO_PASS entry is
    // docstring prose with no id in it at all. Measured on django-10914's gold gate: with
    // ids + renderings only, p2p capped at 38/40 and the two misses were exactly its two
    // docstring-spelled ids. All three spellings must resolve to the same outcome, and a
    // docstring shared by two tests must fold conservatively rather than let a pass mask
    // a fail.
    #[test]
    pub(super) fn a_docstringed_django_test_resolves_by_its_docstring_too() {
        let report = "\
CONTINUUM_TEST {\"id\": \"test_utils.tests.AssertRaisesMsgTest.test_special_re_chars\", \"ok\": true, \"desc\": \"assertRaisesMessage shouldn't interpret RE special chars.\"}
CONTINUUM_TEST {\"id\": \"test_utils.tests.Plain.test_plain\", \"ok\": true}
CONTINUUM_TEST {\"id\": \"a.B.test_shared_one\", \"ok\": true, \"desc\": \"A shared docstring.\"}
CONTINUUM_TEST {\"id\": \"a.B.test_shared_two\", \"ok\": false, \"desc\": \"A shared docstring.\"}
noise that is not a row
CONTINUUM_TEST not json at all";
        let (by_node, by_func) = parse_django_json(report);

        // The dataset's spelling for a docstringed test IS the docstring.
        assert_eq!(
            verdict_for(
                "assertRaisesMessage shouldn't interpret RE special chars.",
                &by_node,
                &by_func
            ),
            Some(true),
            "a docstring-spelled dataset id must resolve"
        );
        // …and the canonical + rendered spellings of that same test still resolve.
        for spelling in [
            "test_utils.tests.AssertRaisesMsgTest.test_special_re_chars",
            "test_special_re_chars (test_utils.tests.AssertRaisesMsgTest)",
        ] {
            assert_eq!(verdict_for(spelling, &by_node, &by_func), Some(true), "{spelling}");
        }
        // A docstring on two tests, one failing, must NOT read as a pass.
        assert_eq!(
            verdict_for("A shared docstring.", &by_node, &by_func),
            Some(false),
            "a shared docstring folds conservatively"
        );
        // A test with no docstring is unaffected, and unparseable lines are skipped.
        assert_eq!(
            verdict_for("test_utils.tests.Plain.test_plain", &by_node, &by_func),
            Some(true)
        );
    }
