use super::*;

pub async fn grade(instance: &SweInstance, model_patch: Option<&str>) -> SweVerdict {
    let _hold = GradeCheckoutHold::take(&instance.instance_id);
    let mut verdict = SweVerdict {
        instance_id: instance.instance_id.clone(),
        ..Default::default()
    };
    // The grade's OWN path — see `ensure_grade_checkout`. Shadowed as `repo_dir` so every use
    // below reads the grade's tree and there is no way to reach a caller's tree from here.
    let repo_dir = &match ensure_grade_checkout(instance).await {
        Ok(d) => d,
        Err(e) => {
            verdict.error = Some(format!("could not stage the grade checkout: {e}"));
            return verdict;
        }
    };
    let f2p = instance.f2p();
    let p2p: Vec<String> = instance.p2p().into_iter().take(P2P_SAMPLE).collect();
    verdict.f2p_total = f2p.len();
    verdict.p2p_total = p2p.len();
    let test_files = patched_test_files(&instance.test_patch);
    let runner = runner_for_repo(&instance.repo);

    let venv_py = match ensure_env(instance, repo_dir).await {
        Ok(p) => p,
        Err(e) => {
            verdict.error = Some(e);
            return verdict;
        }
    };
    let harness = match Harness::for_instance(instance, venv_py, runner) {
        Ok(h) => h,
        Err(e) => {
            verdict.error = Some(e);
            return verdict;
        }
    };

    // THE GATE, on a pristine tree: FAIL_TO_PASS must FAIL. That failure IS the bug. If it
    // passes here the checkout does not contain the bug, and nothing measured against it can
    // distinguish a fix from a no-op.
    if let Err(e) = apply_patch(repo_dir, &instance.test_patch, "gate-test").await {
        verdict.error = Some(e);
        return verdict;
    }
    let (pre, pre_report) = run_harness(repo_dir, &harness, &f2p, &test_files).await;
    // A SKIPPED FAIL_TO_PASS test is an ENV fault, decided before the gate can read the skip
    // as green (2026-09-12, scikit-learn-26323 on a pandas-less env).
    let skipped_pre = skipped_of(&f2p, &skipped_in_report(&pre_report));
    if !skipped_pre.is_empty() {
        verdict.gate_ok = false;
        verdict.error = Some(format!(
            "UNGRADEABLE — FAIL_TO_PASS test(s) SKIPPED on the pristine tree ({skipped_pre:?}): \
             the environment cannot run them (an import the suite skips on, a missing fixture), \
             so no score from this tree measures the fix. Fix the environment spec for this \
             instance's version, then re-grade."
        ));
        return verdict;
    }
    // Sorted so the receipt names tests in one order run after run.
    let pre_sorted: Vec<(String, bool)> = {
        let mut v: Vec<(String, bool)> = pre.iter().map(|(id, ok)| (id.clone(), *ok)).collect();
        v.sort();
        v
    };
    match pristine_gate(&pre_sorted) {
        PristineGate::Void { already } => {
            verdict.gate_ok = false;
            verdict.error = Some(format!(
                "UNGRADEABLE — every FAIL_TO_PASS test already passes on the pristine tree \
                 ({already:?}). The bug is not in this checkout; every score from this tree \
                 is void."
            ));
            return verdict;
        }
        PristineGate::Holds { pristine_green } => {
            verdict.gate_ok = true;
            if !pristine_green.is_empty() {
                crate::probe!(
                    class = "benchmark.grade.pristine_green",
                    instance = instance.instance_id.as_str(),
                    already = %format!("{pristine_green:?}"),
                    f2p_total = f2p.len(),
                    "some FAIL_TO_PASS tests are already green on the pristine tree — the gate \
                     holds on the rest; a backend/env parity note, not a void"
                );
            }
            verdict.pristine_green = pristine_green;
        }
    }

    // Reset, then run the real protocol: model patch first, tests second.
    reset_worktree(repo_dir).await;
    if let Some(patch) = model_patch {
        if let Err(e) = apply_patch(repo_dir, patch, "model").await {
            verdict.error = Some(format!("candidate patch did not apply: {e}"));
            return verdict;
        }
    }
    // THE TESTS ARE NOT THE SOLVER'S TO TOUCH. Restore every path the test patch targets to
    // its pristine base state before applying it. Two things this buys, and the second is the
    // one that matters more than the ungradeable it fixes:
    //
    // 1. The test patch always applies. Measured 2026-08-20 on a live Lite round:
    //    `pallets__flask-4992` produced a real 1,192-byte patch, and grading died with
    //    "could not apply test patch — the tree is not what the patch expects", scoring
    //    UNGRADEABLE. The four-arm ladder in `apply_patch` (exact → ignore-whitespace →
    //    -C1 → --3way) is a fuzz ladder: it can only rescue an overlap it can still merge,
    //    and `--3way` additionally needs the patch's pre-image blobs in the object database.
    //    Fuzzing harder was never the fix; not fighting the solver's edit is.
    //
    // 2. A solver CANNOT influence its own grade. Before this, a candidate patch that edited
    //    a test file either broke the apply (visible, an ungradeable) or — where the edits
    //    merged — silently graded the solver's OWN tests. That second case is the dangerous
    //    one precisely because it looks like a clean pass.
    //
    // Restoring only the test patch's own paths is the minimum that achieves both: every
    // source edit the solver made is preserved and is exactly what gets scored.
    if let Err(e) = restore_paths_to_base(repo_dir, &instance.test_patch).await {
        verdict.error = Some(format!("could not restore test files to pristine: {e}"));
        return verdict;
    }
    if let Err(e) = apply_patch(repo_dir, &instance.test_patch, "test").await {
        verdict.error = Some(e);
        return verdict;
    }

    let (f2p_res, f2p_report) = run_harness(repo_dir, &harness, &f2p, &test_files).await;
    let skipped_after = skipped_of(&f2p, &skipped_in_report(&f2p_report));
    if !skipped_after.is_empty() {
        verdict.error = Some(format!(
            "UNGRADEABLE — FAIL_TO_PASS test(s) SKIPPED after the patch ({skipped_after:?}): the \
             environment cannot run them; this is an env fault, not a failed fix."
        ));
        return verdict;
    }
    let (p2p_res, p2p_report) = run_harness(repo_dir, &harness, &p2p, &test_files).await;
    verdict.f2p_passed = f2p_res.values().filter(|ok| **ok).count();
    verdict.p2p_passed = p2p_res.values().filter(|ok| **ok).count();

    // THE GATE'S OTHER HALF (#383 family, live 2026-08-11): PASS_TO_PASS is defined as
    // "passes before AND after the fix" — so a tree where p2p passes ZERO of N is not a
    // graded failure, it is a suite that does not run in this environment at all
    // (pytest-dev__pytest-5103 graded p2p 0/40 with an EMPTY patch: the era env cannot run
    // pytest's own suite, and that env fault was recorded as a capability verdict). The
    // f2p half of the gate cannot catch this: f2p "fails on pristine" is exactly what a
    // broken suite also produces. Distinguish the two the only honest way — re-run p2p on
    // the PRISTINE tree, paid only when the suspicious all-fail shape appears: pristine
    // ALSO passes zero → the env is broken, void the tree; pristine passes any → the
    // candidate patch genuinely broke the suite and the graded numbers stand.
    //
    // THE SAME QUESTION, PER TEST (Kimi, django-11749, 2026-10-06): one PASS_TO_PASS test
    // failed after her patch, and she showed it fails on the unpatched base under this env's
    // interpreter too. A test that fails BEFORE any fix is by definition not "pass to pass"
    // here; charging it to the candidate made her verdict wait on someone else's ruling.
    // So the pristine run is paid whenever ANY p2p test fails, and each one that fails on
    // the pristine tree as well is voided BY NAME: excluded from the score and carried in
    // `env_void_p2p`, so the verdict decides itself and still says what it set aside.
    if verdict.p2p_total > 0 && verdict.p2p_passed < verdict.p2p_total {
        reset_worktree(repo_dir).await;
        if let Err(e) = apply_patch(repo_dir, &instance.test_patch, "p2p-gate").await {
            verdict.error = Some(e);
            return verdict;
        }
        let (pristine_p2p, pristine_report) =
            run_harness(repo_dir, &harness, &p2p, &test_files).await;
        verdict.env_void_p2p = env_void_p2p(&p2p_res, &pristine_p2p);
        if pristine_p2p.values().filter(|ok| **ok).count() == 0 {
            verdict.gate_ok = false;
            // CARRY THE REPORT. This verdict is the ONLY artifact of the pristine run, and
            // without the run's own output "the suite does not run in this environment" is
            // a conclusion with its evidence deleted — the reader is left to reproduce it
            // by hand and guess at the difference.
            //
            // Measured 2026-08-17, and it cost hours: astropy-14365 graded 0-of-8 here
            // while the SAME invocation (`-m pytest <file> -v -p no:cacheprovider`, same
            // venv, same tree) run by hand gave 8 passed + the instance's own bug failing —
            // exactly the gate condition. Three hypotheses got proposed and none could be
            // settled, because the one thing that would have named the divergence was
            // discarded into `_` on this line. An instrument that knows the answer and
            // drops it is worse than one that never looked.
            let tail = report_tail(&pristine_report);
            verdict.error = Some(format!(
                "UNGRADEABLE — PASS_TO_PASS passes 0 of {} on the PRISTINE tree: the \
                 suite does not run in this environment, so every score from this tree \
                 is an env fault, never a capability verdict.{}",
                verdict.p2p_total,
                if tail.is_empty() {
                    " The pristine run produced NO output at all — the harness never \
                     executed (a missing interpreter, a refused invocation, a run that \
                     died before writing a byte), which is a different fault from a suite \
                     that ran and failed.".to_string()
                } else {
                    format!("\n\nPRISTINE RUN OUTPUT (tail):\n{tail}")
                }
            ));
            return verdict;
        }
    }

    verdict.failed_tests = f2p_res
        .iter()
        .chain(p2p_res.iter())
        .filter(|(_, ok)| !**ok)
        .map(|(id, _)| id.clone())
        .collect();
    verdict.failed_tests.sort();
    let mut p2p_broken: Vec<String> = p2p_res
        .iter()
        .filter(|(id, ok)| !**ok && !verdict.env_void_p2p.contains(*id))
        .map(|(id, _)| id.clone())
        .collect();
    p2p_broken.sort();
    // A TREE THAT DOES NOT COMPILE IS TOLD SO. Every test "fails" when the test binary never
    // built, and the old composition read that as "REGRESSION — you broke N tests that passed
    // before" with the one actionable compiler line buried three thousand characters into a
    // tail of unrelated warnings, twice (2026-09-12, astral-sh__ruff-15309: `.args` for
    // `.arguments.args`; the holder re-read her diff for a regression that did not exist).
    // The compiler's own first error leads; the regression story is not told about a build.
    verdict.failure_excerpt = match build_failure_excerpt(&p2p_report)
        .or_else(|| build_failure_excerpt(&f2p_report))
    {
        Some(compile) => Some(format!(
            "DOES NOT COMPILE — the test build failed, so no test ran; nothing here is a \
             regression or a wrong answer yet. Fix the build first. Compiler:\n{compile}"
        )),
        None => compose_failure_excerpt(
            &p2p_broken,
            &p2p_report,
            verdict.f2p_passed < verdict.f2p_total,
            &f2p_report,
        ),
    };
    verdict.resolved = verdict.f2p_passed == verdict.f2p_total
        && p2p_broken.is_empty()
        && verdict.f2p_total > 0;
    verdict
}

/// PASS_TO_PASS tests that failed with the candidate AND on the pristine tree in the same
/// environment: env faults, not regressions. A test the pristine run did not report at all
/// is NOT voided — absence of evidence keeps it charged, so a harness that skips the
/// pristine run can never launder a real break into a void.
pub(super) fn env_void_p2p(
    candidate: &std::collections::HashMap<String, bool>,
    pristine: &std::collections::HashMap<String, bool>,
) -> Vec<String> {
    let mut void: Vec<String> = candidate
        .iter()
        .filter(|(id, ok)| !**ok && pristine.get(*id) == Some(&false))
        .map(|(id, _)| id.clone())
        .collect();
    void.sort();
    void
}
