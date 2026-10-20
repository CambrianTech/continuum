#![cfg(test)]

    use super::*;

    // what this catches (Kimi, django-11749, 2026-10-06): a PASS_TO_PASS test that fails on
    // the unpatched tree in this env was charged to her patch, and her verdict waited on a
    // ruling. Voided: failed both with the candidate and on pristine. Charged: failed only
    // with the candidate, OR the pristine run did not report it (no evidence, no void).
    #[test]
    pub(super) fn a_p2p_test_failing_before_the_patch_is_voided_and_only_that() {
        use std::collections::HashMap;
        let candidate: HashMap<String, bool> = [("env", false), ("broke", false), ("unseen", false), ("ok", true)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        let pristine: HashMap<String, bool> = [("env", false), ("broke", true), ("ok", true)]
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        assert_eq!(super::env_void_p2p(&candidate, &pristine), vec!["env".to_string()]);
    }

    // what this catches (2026-09-28, Sahar's scikit-learn-25747): an in-flight staging
    // tree read as a staged checkout because it already carries .git. The predicate must
    // recognize exactly the name staging_path_for mints, and never a real instance name.
    #[test]
    pub(super) fn the_in_flight_predicate_knows_the_name_the_mint_makes() {
        let minted = super::staging_path_for(std::path::Path::new("/w/swe/scikit-learn__scikit-learn-25747"));
        let name = minted.file_name().and_then(|n| n.to_str()).expect("utf-8 name");
        assert!(super::is_in_flight_staging_name(name), "{name}");
        assert!(!super::is_in_flight_staging_name("scikit-learn__scikit-learn-25747"));
        assert!(!super::is_in_flight_staging_name("sympy__sympy-18057"));
    }

    // what this catches: grade checkouts retained forever (914k files on the M5,
    // 2026-09-25, fseventsd at 14 GB) — and the opposite failure, a sweep that deletes
    // the tree or staging clone of a grade still in flight.
    #[test]
    pub(super) fn the_orphan_sweep_removes_dead_grade_trees_and_spares_live_ones() {
        let root = tempfile::tempdir().expect("tempdir");
        for d in ["live-1", "live-1.cloning-7-0", "dead-2", "dead-2.cloning-7-1"] {
            std::fs::create_dir_all(root.path().join(d).join("src")).unwrap();
        }
        let live: std::collections::HashSet<String> = ["live-1".to_string()].into();
        assert_eq!(super::sweep_orphan_grade_checkouts(root.path(), &live), 2);
        assert!(root.path().join("live-1").exists());
        assert!(root.path().join("live-1.cloning-7-0").exists());
        assert!(!root.path().join("dead-2").exists());
        assert!(!root.path().join("dead-2.cloning-7-1").exists());
    }

    // what this catches: card cffc9c5e — uv's transport failure read as the instance's
    // env verdict. The signature must fire on the exact stderr the outage produced and
    // stay silent on a resolution / build failure, which IS an env fact.
    #[test]
    pub(super) fn a_network_failure_is_a_could_not_look_never_an_env_fact() {
        use super::{network_failure_signature, COULD_NOT_LOOK};
        let outage = "error: Failed to fetch: `https://pypi.org/simple/cython/`\n  Caused by: Request failed after 3 retries\n  Caused by: error sending request for url (https://pypi.org/simple/cython/)\n  Caused by: client error (Connect)\n  Caused by: dns error: failed to lookup address information: nodename nor servname provided, or not known";
        assert!(network_failure_signature(outage));
        assert!(network_failure_signature("error: Failed to download `numpy==1.21.6`\n  Caused by: Connection reset by peer (os error 54)"));
        assert!(network_failure_signature("  Caused by: Operation timed out (os error 60)"));
        // A resolution hole or a build failure is a fact about the instance.
        assert!(!network_failure_signature("error: Because only atomicwrites<=1.4.1 is available and you require atomicwrites>=1.4.2, we can conclude that the requirements are unsatisfiable."));
        assert!(!network_failure_signature("error: Failed to build `numpy==1.21.6`\n  Caused by: The build backend returned an error\n  Caused by: Call to `setuptools.build_meta.build_wheel` failed (exit status: 1)"));
        assert!(!network_failure_signature(""));
        // The head every reader keys on is one substrate sentence, never uv's wording.
        assert!(COULD_NOT_LOOK.starts_with("COULD NOT LOOK"));
    }

    // what this catches (card 1aac0c72): `uv` missing from the HOST was recorded as the
    // instance's standing refusal; `refusal_stands_at` lifts a marker only when the
    // citizen's work changes, so the grade was never re-tried after uv appeared on the
    // box (sphinx-7889, 5090, 2026-09-21). The reason must carry the COULD_NOT_LOOK head
    // the sweep defers on, and must never classify as an env fault.
    #[test]
    pub(super) fn a_missing_host_prerequisite_is_a_could_not_look_never_a_standing_refusal() {
        use super::{host_prereq_is_could_not_look, uv_missing_is_could_not_look, COULD_NOT_LOOK};
        let uv = uv_missing_is_could_not_look();
        let freetype = host_prereq_is_could_not_look(
            "the HOST freetype that matplotlib needs",
            "install it with `brew install freetype pkg-config`",
        );
        for reason in [&uv, &freetype] {
            assert!(reason.starts_with(COULD_NOT_LOOK), "{reason}");
            assert!(!crate::cognition::swe_verdict_sweep::refusal_is_env_fault(reason), "{reason}");
        }
        assert!(uv.contains("astral.sh/uv"), "still says how to fix it: {uv}");
        assert!(freetype.contains("brew install freetype"), "still says how to fix it: {freetype}");
    }

    // what this catches: the Multilingual harness misread — the marked command not
    // lifted (wrapper kept, the marker comment taken as the command), a cargo/go log
    // read into the wrong verdict, or a Python-family instance routed through a
    // script it does not have. Fixtures are the real ruff-15309 / tokio-4384 tails.
    #[test]
    pub(super) fn a_multilingual_instance_runs_its_own_harness_and_the_python_family_stays_native() {
        use super::{language_of_parser, parse_log_cargo, parse_log_gotest, test_command_from_eval_script, Harness, LogParser, TestRunner};
        let ruff = "git apply -v - <<'EOF_1'\n+x\nEOF_1\n: '>>>>> Start Test Output'\n(cargo test --package ruff_linter 'f52') | cat\n: '>>>>> End Test Output'\ngit checkout 75a2 a.py\n";
        assert_eq!(
            test_command_from_eval_script(ruff).as_deref(),
            Some("cargo test --package ruff_linter 'f52'")
        );
        let tokio = ": '>>>>> Start Test Output'\n(RUSTFLAGS=-Awarnings cargo test --package tokio --test net_types_unwind --features full --no-fail-fast) | cat\n: '>>>>> End Test Output'\n";
        assert_eq!(
            test_command_from_eval_script(tokio).as_deref(),
            Some("RUSTFLAGS=-Awarnings cargo test --package tokio --test net_types_unwind --features full --no-fail-fast")
        );
        assert_eq!(test_command_from_eval_script("no markers here"), None);

        let cargo_log = "running 3 tests\ntest rules::pyflakes::tests::rule_f523 ... ok\ntest io::write_all_buf_vectored ... FAILED\ntest slow::one - should panic ... ok\ntest skipped::x ... ignored\n\nfailures:\n";
        let seen = parse_log_cargo(cargo_log);
        assert_eq!(seen.get("rules::pyflakes::tests::rule_f523"), Some(&true));
        assert_eq!(seen.get("io::write_all_buf_vectored"), Some(&false));
        assert_eq!(seen.get("slow::one"), Some(&true), "the display suffix is not the id");
        assert_eq!(seen.get("skipped::x"), None, "ignored is neither verdict");

        let go_log = "=== RUN   TestServer\n--- PASS: TestServer (0.01s)\n=== RUN   TestRoute/sub\n    --- FAIL: TestRoute/sub (0.00s)\nFAIL\n";
        let seen = parse_log_gotest(go_log);
        assert_eq!(seen.get("TestServer"), Some(&true));
        assert_eq!(seen.get("TestRoute/sub"), Some(&false));

        let mut inst: super::SweInstance = serde_json::from_value(serde_json::json!({
            "instance_id": "tokio-rs__tokio-4384", "repo": "tokio-rs/tokio", "base_commit": "abc",
            "patch": "", "test_patch": "", "problem_statement": "",
            "FAIL_TO_PASS": "[\"net_types_are_unwind_safe\"]", "PASS_TO_PASS": "[]",
            "eval_script": tokio, "log_parser": "parse_log_cargo"
        }))
        .expect("a multilingual row decodes");
        assert_eq!(inst.language(), "rust");
        match Harness::for_instance(&inst, "unused".into(), TestRunner::Pytest) {
            Ok(Harness::Script { parser: LogParser::Cargo, command }) => {
                assert!(command.starts_with("RUSTFLAGS="), "{command}")
            }
            other => panic!("expected the script harness, got {other:?}"),
        }
        inst.log_parser = Some("parse_log_phpunit".into());
        let err = Harness::for_instance(&inst, "unused".into(), TestRunner::Pytest).unwrap_err();
        assert!(err.contains("UNSUPPORTED HARNESS") && err.contains("php"), "{err}");
        assert_eq!(language_of_parser("parse_log_rspec_transformed_json"), "ruby");

        inst.eval_script = None;
        inst.log_parser = None;
        assert_eq!(inst.language(), "python");
        assert_eq!(
            Harness::for_instance(&inst, "venv/bin/python".into(), TestRunner::Pytest),
            Ok(Harness::Native { venv_py: "venv/bin/python".into(), runner: TestRunner::Pytest })
        );
    }

    // what this catches: a verdict that cannot say which harness scored it.
    // Measured 2026-08-28 — 19 of 32 verdicts on this box had been written
    // across ten days by three different harness builds, and regrading ONE from
    // its identical banked patch moved pass-to-pass from 0/40 to 40/40: the old
    // verdict accused her of destroying 40 working tests she never touched.
    // Staleness was only ever visible as an mtime an operator happened to
    // notice. Stamping happens at record_verdict — the ONE durable seam — so no
    // caller can forget it, and old files stay loadable as "<unstamped>".
    #[test]
    pub(super) fn every_durable_verdict_names_the_harness_that_scored_it() {
        let dir = std::env::temp_dir().join(format!("verdict-prov-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let v = SweVerdict {
            instance_id: "astropy__astropy-14182".into(),
            resolved: true,
            f2p_passed: 1,
            f2p_total: 1,
            p2p_passed: 9,
            p2p_total: 9,
            gate_ok: true,
            ..Default::default()
        };
        assert!(
            v.harness_build.is_empty(),
            "a verdict is unstamped until the durable seam stamps it"
        );
        let stamped = serde_json::to_string(&SweVerdict {
            harness_build: env!("CONTINUUM_BUILD_GIT_SHA").to_string(),
            graded_at_ms: 0,
            ..v.clone()
        })
        .unwrap();
        let back: SweVerdict = serde_json::from_str(&stamped).unwrap();
        assert_eq!(
            back.harness_build,
            env!("CONTINUUM_BUILD_GIT_SHA"),
            "the build that scored it must survive the round trip"
        );

        // Verdicts written BEFORE provenance existed must still load — they are
        // the ones whose staleness this field exists to expose, so refusing them
        // would delete the evidence.
        let legacy = r#"{"instance_id":"x","resolved":false,"f2p_passed":0,"f2p_total":1,
            "p2p_passed":0,"p2p_total":0,"gate_ok":true,"failed_tests":[]}"#;
        let old: SweVerdict = serde_json::from_str(legacy).expect("legacy verdicts still load");
        assert!(old.harness_build.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: an upstream required-test list containing tokens that
    // cannot name a test, counted as citizen failures. Measured 2026-08-27 —
    // SWE-bench Lite's pytest-dev__pytest-5227 requires "[", "[100%]" and
    // "[100%]---…" (its own log parser leaked pytest's progress bar), and
    // Verified carries six more ("#15789", a bare quote). She passed 3/3
    // fail-to-pass and all 31 REAL pass-to-pass tests on 5227 and was scored a
    // MISS on those three fragments. An id no solver can satisfy — not even the
    // gold patch — measures nothing, so it must never reach the denominator.
    // The predicate must stay conservative: real ids always carry letters.
    #[test]
    pub(super) fn unsatisfiable_upstream_test_ids_never_reach_the_denominator() {
        use super::{is_unsatisfiable_test_id, SweInstance};
        for junk in ["[", "[100%]", "[100%]------------------------------", "#15789", "\"", "  ", "42", "-"] {
            assert!(
                is_unsatisfiable_test_id(junk),
                "{junk:?} cannot name a test and must be dropped"
            );
        }
        for real in [
            "tests/test_util_rst.py::test_escape",
            "test_adjoint",
            "test_ticket_18785 (queries.tests.Ticket18785Tests)",
            "lib/matplotlib/tests/test_legend.py::test_alpha_rgba[png]",
            "_private_helper",
        ] {
            assert!(
                !is_unsatisfiable_test_id(real),
                "{real:?} is a real test id and must survive"
            );
        }

        // End to end through the accessor the gate/grade/scoreboard all read.
        let inst = SweInstance {
            instance_id: "pytest-dev__pytest-5227".into(),
            repo: "pytest-dev/pytest".into(),
            base_commit: "deadbeef".into(),
            patch: String::new(),
            test_patch: String::new(),
            problem_statement: String::new(),
            created_at: "2019-05-06T00:00:00Z".into(),
            pass_to_pass: serde_json::json!(["[", "[100%]", "testing/test_x.py::test_real"])
                .to_string(),
            fail_to_pass: serde_json::json!(["testing/test_x.py::test_target"]).to_string(),
            eval_script: None,
            log_parser: None,
        };
        assert_eq!(inst.p2p(), vec!["testing/test_x.py::test_real".to_string()]);
        assert_eq!(inst.f2p(), vec!["testing/test_x.py::test_target".to_string()]);
    }

    // what this catches (2026-09-16, xarray-4356 / pandas 1.1.0 under its own -Werror on
    // clang 21): every era C flag is a `-Wno-<diag>` (disabling the diagnostic — the only
    // form a later `-Werror` cannot re-arm; `-Wno-error=` is exactly the form that
    // failed live on 2026-08-27) or a `-D` shim; and the two rows that measured death
    // tonight are present. A row added in the wrong form fails here, not on the next
    // gold gate.
    #[test]
    pub(super) fn every_era_c_flag_disables_a_diagnostic_or_defines_a_shim() {
        for flag in ERA_CFLAGS.split_whitespace() {
            assert!(
                (flag.starts_with("-Wno-") && !flag.starts_with("-Wno-error=")) || flag.starts_with("-D"),
                "era flag in a form -Werror can override: {flag}"
            );
        }
        for row in ["-Wno-unreachable-code-fallthrough", "-Wno-unused-but-set-variable"] {
            assert!(ERA_CFLAGS.split_whitespace().any(|f| f == row), "missing measured row {row}");
        }
    }

    // what this catches: node ids with SPACES inside their params, and the
    // truncated fragments upstream requires for them (pytest-7432, 2026-09-01:
    // the dataset's P2P list carries `test_xfail_raises[(AttributeError,` —
    // its own log parser split the real id on whitespace — while our report
    // parser ALSO split at the first whitespace, so the whole family was
    // invisible and graded as three phantom regressions on an f2p-clean solve).
    // The parser must record the full space-bearing node id; the lookup must
    // score an unclosed-`[` fragment as the AND of the nodes it prefixes; and
    // outcome-leading short-summary rows must never parse as verbose rows.
    // what this catches: card 27a5d94d — an XPASS graded as a miss. The test ran and
    // passed; only its xfail marker (keyed on a package version the era env lacks) says
    // it was expected not to. A strict xfail that passes arrives as FAILED and stays one.
    #[test]
    pub(super) fn an_xpass_is_a_test_that_passed_and_a_strict_one_arrives_as_failed() {
        let report = "\
xarray/tests/test_duck_array_ops.py::test_datetime_mean[False] XPASS [ 50%]
xarray/tests/test_duck_array_ops.py::test_datetime_mean[True] XPASS [ 75%]
xarray/tests/test_duck_array_ops.py::test_strict_marker FAILED [100%]
";
        let (by_node, by_func) = parse_pytest_report(report);
        assert_eq!(by_node.get("xarray/tests/test_duck_array_ops.py::test_datetime_mean[False]"), Some(&true));
        assert_eq!(by_node.get("xarray/tests/test_duck_array_ops.py::test_datetime_mean[True]"), Some(&true));
        assert_eq!(by_func.get("test_datetime_mean"), Some(&true));
        assert_eq!(by_node.get("xarray/tests/test_duck_array_ops.py::test_strict_marker"), Some(&false));
    }

    #[test]
    pub(super) fn space_bearing_param_ids_parse_and_truncated_fragments_score_by_prefix() {
        let report = "\
testing/test_skipping.py::TestXFail::test_xfail_raises[(AttributeError, IndexError)-TypeError-*1 TypeError*] PASSED [ 10%]
testing/test_skipping.py::TestXFail::test_xfail_raises[TypeError-IndexError-*1 IndexError*] FAILED [ 20%]
testing/test_skipping.py::test_plain PASSED [ 30%]
FAILED testing/test_skipping.py::TestXFail::test_xfail_raises[TypeError-IndexError-*1 IndexError*] - assert PASSED
";
        let (by_node, by_func) = parse_pytest_report(report);
        assert_eq!(
            by_node.get("testing/test_skipping.py::TestXFail::test_xfail_raises[(AttributeError, IndexError)-TypeError-*1 TypeError*]"),
            Some(&true),
            "a space-bearing node id must be recorded whole"
        );
        // The short-summary row must not have minted a bogus PASSED node.
        assert_eq!(by_node.len(), 3, "exactly the three verbose rows: {by_node:?}");
        // Upstream's truncated fragment prefixes exactly one real node each.
        assert_eq!(
            verdict_for(
                "testing/test_skipping.py::TestXFail::test_xfail_raises[(AttributeError,",
                &by_node,
                &by_func
            ),
            Some(true),
            "an unclosed-[ fragment scores as the node it prefixes"
        );
        assert_eq!(
            verdict_for(
                "testing/test_skipping.py::TestXFail::test_xfail_raises[TypeError-IndexError-*1",
                &by_node,
                &by_func
            ),
            Some(false),
            "prefix scoring must still see a REAL failure"
        );
    }

    // what this catches: the three wire dialects of one instance family
    // (2026-08-23 import): SWE-bench string-encoded F2P, SWE-rebench REAL-LIST
    // F2P, SWE-bench Pro lowercase names — all must decode into ONE SweInstance
    // whose f2p()/p2p() yield the same ids, or a schema-kin suite silently
    // "fetches 21k rows but NONE decoded" (load_dataset's own failure text).
    #[test]
    pub(super) fn one_swe_instance_decodes_all_three_wire_dialects() {
        let base = serde_json::json!({
            "instance_id": "x__x-1", "repo": "x/x", "base_commit": "abc",
            "patch": "p", "test_patch": "t", "problem_statement": "s",
        });
        let mut swebench = base.clone();
        swebench["FAIL_TO_PASS"] = serde_json::json!("[\"tests/a.py::t1\"]");
        swebench["PASS_TO_PASS"] = serde_json::json!("[\"tests/a.py::t2\"]");
        let mut rebench = base.clone();
        rebench["FAIL_TO_PASS"] = serde_json::json!(["tests/a.py::t1"]);
        rebench["PASS_TO_PASS"] = serde_json::json!(["tests/a.py::t2"]);
        rebench["created_at"] = serde_json::json!("2026-05-12 13:21:50");
        let mut pro = base.clone();
        pro["fail_to_pass"] = serde_json::json!("[\"tests/a.py::t1\"]");
        pro["pass_to_pass"] = serde_json::json!("[\"tests/a.py::t2\"]");
        for (dialect, v) in [("swe-bench", swebench), ("swe-rebench", rebench), ("pro", pro)] {
            let inst: SweInstance = serde_json::from_value(v)
                .unwrap_or_else(|e| panic!("{dialect} dialect failed to decode: {e}"));
            assert_eq!(inst.f2p(), vec!["tests/a.py::t1"], "{dialect} f2p");
            assert_eq!(inst.p2p(), vec!["tests/a.py::t2"], "{dialect} p2p");
        }
        // rebench's space-separated created_at still yields the era year.
        let inst: SweInstance = serde_json::from_value(serde_json::json!({
            "instance_id": "y", "repo": "y/y", "base_commit": "c", "patch": "p",
            "test_patch": "t", "problem_statement": "s",
            "created_at": "2026-05-12 13:21:50",
            "FAIL_TO_PASS": [], "PASS_TO_PASS": []
        }))
        .expect("decodes");
        assert_eq!(inst.year(), 2026);
    }

    // what this catches: the exclude shield's MERGE contract. It runs on every
    // dispatch over already-staged checkouts (self-heal for pre-shield trees), so
    // an overwrite would clobber an operator's own exclude entries on the second
    // pass, and a non-idempotent append would grow the file every dispatch.
    // regression for the 2026-08-22 .airc-in-graded-diff incident + its 76-tree
    // hand backfill.
    #[test]
    pub(super) fn the_exclude_shield_merges_idempotently_and_preserves_operator_entries() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let repo = tmp.path();
        std::fs::create_dir_all(repo.join(".git/info")).unwrap(); // test setup on a fresh tempdir cannot collide
        std::fs::write(repo.join(".git/info/exclude"), "my-scratch/\n").unwrap(); // test setup

        shield_workspace_excludes(repo);
        let after = std::fs::read_to_string(repo.join(".git/info/exclude")).unwrap(); // written above
        assert!(after.contains("my-scratch/"), "operator entry survives: {after}");
        assert!(after.contains(".airc/") && after.contains(".DS_Store"), "{after}");

        shield_workspace_excludes(repo);
        let twice = std::fs::read_to_string(repo.join(".git/info/exclude")).unwrap(); // written above
        assert_eq!(after, twice, "second pass must change nothing");

        // A tree with no exclude file at all gets one (the fresh-clone path).
        let bare = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(bare.path().join(".git")).unwrap(); // test setup
        shield_workspace_excludes(bare.path());
        let made = std::fs::read_to_string(bare.path().join(".git/info/exclude")).unwrap(); // shield created it
        assert!(made.contains(".airc/"), "{made}");
    }

    // what this catches: the staging-collision half of the 2026-08-18 grade loss. Two
    // concurrent stages of the same instance shared `cloning-{pid}` (one pid in the
    // headless core), so the stale-staging sweep deleted a LIVE clone mid-fetch and
    // sympy-18057's finished artifact died at scoring. Staging names must be unique
    // per invocation while still carrying the sweepable `<name>.cloning-` prefix the
    // dead-tree sweep keys on — silently changing that prefix would turn every crashed
    // run's staging tree into permanent disk litter.
    #[test]
    pub(super) fn staging_paths_never_collide_and_stay_sweepable() {
        let repo = Path::new("/tmp/swe/work/sympy__sympy-18057/repo");
        let a = staging_path_for(repo);
        let b = staging_path_for(repo);
        assert_ne!(a, b, "two invocations for the SAME destination must never share a tree");
        for p in [&a, &b] {
            let name = p.file_name().unwrap().to_string_lossy();
            assert!(
                name.starts_with("repo.cloning-"),
                "staging name {name} lost the sweepable prefix — dead trees would accumulate"
            );
            assert_eq!(p.parent(), repo.parent(), "staging must be a sibling of its destination");
        }
        // A crashed clone with .git may contain real citizen edits. Its nonce
        // does not authorize deletion, even if another final checkout exists.
        let root = tempfile::tempdir().unwrap();
        let destination = root.path().join("repo");
        let interrupted = staging_path_for(&destination);
        std::fs::create_dir_all(interrupted.join(".git")).unwrap();
        std::fs::write(interrupted.join("work.txt"), "citizen work").unwrap();
        std::fs::create_dir(&destination).unwrap();
        std::fs::write(destination.join("keep.txt"), "final work").unwrap();
        assert!(refuse_unrecovered_checkouts(&destination).is_err());
        assert_eq!(std::fs::read_to_string(interrupted.join("work.txt")).unwrap(), "citizen work");
        assert!(destination.join("keep.txt").exists());
        assert!(refuse_unrecovered_checkouts(&root.path().join("other")).is_ok());
    }

    // what this catches: the serialization half of the same incident. clone_at both
    // removes its destination at entry and razes it at the rename commit-point, so two
    // concurrent callers for ONE destination delete each other's trees mid-use. The
    // lock registry must hand the SAME lock to same-destination callers and different
    // locks to different destinations (or a 300-instance sweep would serialize globally).
    #[test]
    pub(super) fn clone_locks_are_per_destination() {
        let a1 = clone_lock_for(Path::new("/tmp/swe/work/i-1/repo"));
        let a2 = clone_lock_for(Path::new("/tmp/swe/work/i-1/repo"));
        let b = clone_lock_for(Path::new("/tmp/swe/work/i-2/repo"));
        assert!(
            std::sync::Arc::ptr_eq(&a1, &a2),
            "same destination must serialize on the same lock"
        );
        assert!(
            !std::sync::Arc::ptr_eq(&a1, &b),
            "different destinations must not contend"
        );
    }

    // what this catches: the path parse behind "the tests are not the solver's to touch".
    // If this under-reads, a test file the solver edited is NOT restored, and the grade either
    // dies ungradeable (visible) or silently scores the solver's own tests (not visible) —
    // the second being why this is parsed strictly rather than best-effort. Measured cause:
    // pallets__flask-4992 graded UNGRADEABLE on a live round, 2026-08-20.
    #[test]
    pub(super) fn diff_target_paths_reads_every_post_image_path() {
        let diff = "\
diff --git a/tests/test_cli.py b/tests/test_cli.py
--- a/tests/test_cli.py
+++ b/tests/test_cli.py
@@ -1 +1 @@
-x
+y
diff --git a/src/flask/app.py b/src/flask/app.py
--- a/src/flask/app.py
+++ b/src/flask/app.py\t2020-01-01
@@ -1 +1 @@
-a
+b
diff --git a/gone.py b/gone.py
--- a/gone.py
+++ /dev/null
diff --git a/odd name.py b/odd name.py
--- \"a/odd name.py\"
+++ \"b/odd name.py\"
";
        let got = diff_target_paths(diff);
        assert_eq!(
            got,
            vec![
                "tests/test_cli.py".to_string(),
                "src/flask/app.py".to_string(), // trailing \t timestamp stripped
                "odd name.py".to_string(),      // quotes stripped
            ],
            "a deletion has no post-image path and must not be restored; \
             a quoted or timestamped header must still yield its path"
        );
    }

    // what this catches: an empty/whitespace patch must be a no-op, not a git invocation on
    // an empty path — the guard that keeps the restore harmless for instances whose test
    // patch is absent.
    #[test]
    pub(super) fn diff_target_paths_is_empty_for_a_patchless_instance() {
        assert!(diff_target_paths("").is_empty());
        assert!(diff_target_paths("\n  \n").is_empty());
    }

    // what this catches: the env installs `-e .` — RUNTIME deps only — and astropy's
    // conftest.py opens with `import hypothesis`, a TEST dep. pytest then dies loading
    // conftest, PASS_TO_PASS reads 0/9 on the PRISTINE tree, the gold gate refuses to score,
    // and every astropy instance is ungradeable no matter what a citizen writes (measured
    // 2026-08-17). This picks the group whose contents the suite needs. The preference ORDER
    // is the load-bearing part: a repo declaring both `dev` and `test` must get `test`, or we
    // install a kitchen-sink group and inherit its resolution failures.
    #[test]
    pub(super) fn the_test_extra_is_the_narrowest_group_the_repo_declares() {
        let dir = std::env::temp_dir().join(format!("swe-extra-{}", std::process::id()));
        let write = |body: &str| {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("pyproject.toml"), body).unwrap();
        };

        write("[project.optional-dependencies]\ndev = []\ntest = []\n");
        assert_eq!(test_extra_name(&dir).as_deref(), Some("test"), "test beats dev");

        write("[project.optional-dependencies]\ndocs = []\ntesting = []\n");
        assert_eq!(
            test_extra_name(&dir).as_deref(),
            Some("testing"),
            "a repo that spells it `testing` still gets its suite deps"
        );

        // No test-shaped group, and no pyproject at all, both mean "install as before" —
        // NOT "install something plausible". A wrong extra is a resolve failure on a repo
        // that grades fine today.
        write("[project.optional-dependencies]\ndocs = []\n");
        assert_eq!(test_extra_name(&dir), None, "docs-only declares no suite deps");

        // THE CASE THAT SHIPPED BROKEN (2026-08-17): astropy 5.3 has no
        // [project.optional-dependencies] at all — its suite deps are setuptools
        // declarative config. A pyproject-only parser returns None for the exact repo this
        // fix exists for, which is what the live gold gate caught after I had already
        // claimed the fix worked.
        std::fs::remove_file(dir.join("pyproject.toml")).unwrap();
        std::fs::write(
            dir.join("setup.cfg"),
            "[options]\npackages = find:\n\n[options.extras_require]\ntest =  # Required to run the suite.\n    pytest>=7.0\n    pytest-astropy>=0.10\ntest_all =\n    objgraph\n[options.package_data]\n* = data/*\n",
        )
        .unwrap();
        assert_eq!(
            test_extra_name(&dir).as_deref(),
            Some("test"),
            "setup.cfg extras are declarations too — and `test_all`, which sorts first \
             alphabetically and is a superset, must NOT win over `test`"
        );
        let _ = std::fs::remove_file(dir.join("setup.cfg"));
        write("[project]\nname = \"x\"\n");
        assert_eq!(test_extra_name(&dir), None, "no optional-dependencies table");
        std::fs::remove_file(dir.join("pyproject.toml")).unwrap();
        assert_eq!(test_extra_name(&dir), None, "no pyproject at all");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // what this catches: a SECOND swe env root. On 2026-08-17 two roots existed —
    // `swe_cache_dir()/envs` (live, 46 envs / 8 repos) and `~/.continuum/cache/swe-envs`
    // (retired, 14 envs / 3 repos, the legacy python default). Nothing failed: each root was
    // internally consistent, so whichever you `ls`ed answered confidently. Reading the retired
    // one produced "77% of staged instances have no environment", which was reported as the
    // benchmark's root cause and had a design approved on it. Truth was 95% coverage.
    //
    // A cache with two roots cannot report its own coverage. So: the env root is derived from
    // swe_cache_dir() and appears as a path literal NOWHERE else in the crate. If you need the
    // envs dir, call the function. See swe_cache_dir's doc for the full incident.
    #[test]
    pub(super) fn the_swe_env_root_has_exactly_one_spelling() {
        let files = crate::test_env::source_tree::crate_sources();

        // The retired root, and any hand-spelled sibling of the live one. ASSEMBLED at
        // runtime, never written whole: a literal needle would match its own declaration and
        // the guard would fail on itself (it did, first run). Comments are stripped too, so
        // this test's prose and swe_cache_dir's doc can name the paths freely.
        let retired = format!("cache/{}-envs", "swe");
        let live_spelled_out = format!("benchmarks/{}/envs", "swe");
        let banned = [retired.as_str(), live_spelled_out.as_str()];
        let mut hits = Vec::new();
        for file in &files {
            for (n, raw) in file.text.lines().enumerate() {
                let code = crate::test_env::source_tree::code_only(raw);
                for b in banned {
                    if code.contains(b) {
                        hits.push(format!("{}:{} → {}", file.rel, n + 1, code.trim()));
                    }
                }
            }
        }
        assert!(
            hits.is_empty(),
            "a SECOND spelling of the swe env root appeared — this is exactly how the \
             2026-08-17 coverage misdiagnosis happened (two roots, both real, neither naming \
             the other, opposite answers to the same question). Derive it from \
             `swe_cache_dir()` instead of writing the path:\n  {}",
            hits.join("\n  ")
        );
    }

    // what this catches: the false-env-void misgrade (pytest-11143 attempt 3, live
    // 2026-08-12) — a candidate patch that CREATED a file survived `git checkout .`, the
    // leftover module broke the "pristine" p2p re-run, and a REAL capability regression was
    // voided as an env fault. reset_worktree must revert tracked edits AND remove untracked
    // files, while leaving gitignored artifacts (editable-install *.egg-info) untouched.
    #[tokio::test]
    pub(super) async fn reset_worktree_removes_created_files_but_keeps_ignored_artifacts() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path();
        for args in [
            vec!["init", "-q"],
            vec!["config", "user.email", "t@t"],
            vec!["config", "user.name", "t"],
        ] {
            assert!(run("git", &args, Some(repo))
                .await
                .unwrap()
                .status
                .success());
        }
        std::fs::write(repo.join("tracked.py"), "original").unwrap();
        std::fs::write(repo.join(".gitignore"), "*.egg-info\n").unwrap();
        run("git", &["add", "."], Some(repo)).await.unwrap();
        run("git", &["commit", "-qm", "base"], Some(repo))
            .await
            .unwrap();

        // The three states a candidate patch leaves behind:
        std::fs::write(repo.join("tracked.py"), "edited").unwrap(); // tracked edit
        std::fs::write(repo.join("conftest.py"), "boom").unwrap(); // CREATED file
        std::fs::write(repo.join("pkg.egg-info"), "install artifact").unwrap(); // ignored

        reset_worktree(repo).await;

        assert_eq!(
            std::fs::read_to_string(repo.join("tracked.py")).unwrap(),
            "original"
        );
        assert!(
            !repo.join("conftest.py").exists(),
            "created file must not survive the reset"
        );
        assert!(
            repo.join("pkg.egg-info").exists(),
            "ignored install artifacts must survive"
        );
    }

    // what this catches: the deleted-history env-build failure (pytest-dev__pytest-5103,
    // live 2026-08-11) — `atomicwrites`' pre-2022 uploads were deleted from PyPI, so the
    // date-pinned resolve has ZERO candidates and the env build dies. The heal parses uv's
    // own hint into an `--exclude-newer-package` value cut one second past the earliest
    // surviving upload; unrelated errors must parse to None (no retry storm).
    #[test]
    pub(super) fn deleted_history_hint_parses_into_a_per_package_cutoff() {
        let stderr = "\u{d7} No solution found when resolving dependencies:\n\
            \u{2570}\u{2500}\u{25b6} Because there are no versions of atomicwrites ...\n\
            hint: `atomicwrites` was filtered by `exclude-newer` to only include packages \
            uploaded before 2019-04-13T16:17:45Z. The latest version satisfying the requirement \
            is v1.4.1, published at 2022-07-08T18:31:40.459Z. Consider using \
            `exclude-newer-package` to override the cutoff for this package.";
        assert_eq!(
            deleted_history_override(stderr).as_deref(),
            Some("atomicwrites=2022-07-08T18:31:41Z"),
            "one second past the surviving upload, exclusive-bound safe"
        );

        assert_eq!(
            deleted_history_override("error: could not compile `foo` due to previous errors"),
            None,
            "a non-resolver failure must not synthesize an override"
        );
        assert_eq!(
            deleted_history_override(
                "hint: `pkg` was filtered ... Consider using `exclude-newer-package` ..."
            ),
            None,
            "a hint with no parsable publish timestamp must not synthesize an override"
        );
    }

mod part_2;
pub use part_2::*;
mod part_3;
pub use part_3::*;
