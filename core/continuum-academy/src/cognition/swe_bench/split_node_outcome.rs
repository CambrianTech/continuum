use super::*;

/// Split a `-v` report line into `(node_id, outcome…)`.
///
/// A node id is NOT whitespace-free: parameterized ids embed their params
/// verbatim, and a `pytest.raises(match=…)` glob puts real spaces inside the
/// brackets — pytest-7432's `test_xfail_raises[(AttributeError, IndexError)-…-*1
/// TypeError*]`. The old first-whitespace split truncated every such line into a
/// node fragment plus a rest that starts mid-param, so the whole family was
/// invisible to the grader and each required id fell through to "failed" — three
/// phantom p2p regressions on an f2p-clean solve (measured live 2026-09-01).
///
/// So: find the outcome WORD (whitespace-delimited) and split there.
pub(super) fn split_node_outcome(line: &str) -> Option<(&str, &str)> {
    // Short-summary rows lead WITH the outcome (`FAILED x.py::t - message`) and
    // their free-text tail may contain outcome-shaped words — they are not the
    // verbose shape this parser reads, so refuse them before scanning.
    if PYTEST_OUTCOMES.iter().any(|o| {
        line.strip_prefix(o)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with(char::is_whitespace))
    }) {
        return None;
    }
    let mut split_at: Option<usize> = None;
    for outcome in PYTEST_OUTCOMES {
        let mut from = 0;
        while let Some(found) = line[from..].find(outcome) {
            let at = from + found;
            let ends_clean = line[at + outcome.len()..]
                .chars()
                .next()
                .is_none_or(char::is_whitespace);
            if at > 0 && line[..at].ends_with(char::is_whitespace) && ends_clean {
                // Earliest valid outcome token wins — a param glob later in the
                // line can never pull the split point rightward past it.
                split_at = Some(split_at.map_or(at, |best| best.min(at)));
                break;
            }
            from = at + outcome.len();
        }
    }
    split_at.map(|at| (line[..at].trim_end(), &line[at..]))
}

/// Look one required id up in a parsed report, by node id or by bare function name.
pub fn verdict_for(
    id: &str,
    by_node: &HashMap<String, bool>,
    by_func: &HashMap<String, bool>,
) -> Option<bool> {
    if let Some(v) = by_node.get(id) {
        return Some(*v);
    }
    // Upstream truncation: SWE-bench's own log parser split parameterized node
    // ids on whitespace when it BUILT the required lists, so the dataset can
    // require `test_xfail_raises[(AttributeError,` — a fragment no report ever
    // contains verbatim (pytest-7432, three phantom p2p regressions on an
    // f2p-clean solve, 2026-09-01). An id with an unclosed `[` is such a
    // fragment, and it is a strict PREFIX of exactly the parameterized cases it
    // meant — score it as the AND of every reported node it prefixes, which
    // keeps those tests in the denominator instead of dropping them.
    if id.contains('[') && !id.ends_with(']') {
        let mut all_ok: Option<bool> = None;
        for (node, ok) in by_node {
            if node.starts_with(id) {
                all_ok = Some(all_ok.unwrap_or(true) && *ok);
            }
        }
        if all_ok.is_some() {
            return all_ok;
        }
    }
    // django ids are `test_name (module.Class)` — the bare name is the LEADING token, and
    // the rsplit('.') chain below would extract `Class)` instead. Must come first.
    let key = if let Some((func, _)) = id.split_once(" (") {
        func
    } else {
        let key = id.rsplit("::").next().unwrap_or(id);
        let key = key.rsplit('.').next().unwrap_or(key);
        key.split('[').next().unwrap_or(key)
    };
    by_func.get(key).copied()
}

/// Run the instance's test files once and resolve every required id against that report.
/// A test absent from the report counts as failed — but the absence is knowable, not silent.
/// Restore a grade worktree to its checked-out state: revert tracked edits AND remove
/// files a candidate patch CREATED (checkout alone leaves those untracked in place, and a
/// leftover broken module poisons later runs — live: pytest-11143 attempt 3 misread a real
/// capability regression as an env void, 2026-08-12). No `-x`: gitignored editable-install
/// artifacts (*.egg-info) must survive.
pub async fn reset_worktree(repo_dir: &Path) {
    let _ = run("git", &["checkout", "--quiet", "."], Some(repo_dir)).await;
    let _ = run("git", &["clean", "-fdq"], Some(repo_dir)).await;
}

pub async fn run_tests(
    repo_dir: &Path,
    venv_py: &Path,
    ids: &[String],
    test_files: &[String],
    runner: TestRunner,
) -> (HashMap<String, bool>, String) {
    if test_files.is_empty() || ids.is_empty() {
        return (
            ids.iter().map(|i| (i.clone(), false)).collect(),
            String::new(),
        );
    }
    // Pytest flags must be era-portable: the interpreter under `-m pytest` can be the repo's
    // OWN pytest (pytest-dev instances: the editable install IS the subject) or an era-pinned
    // one. `--no-header` (6.1+) and `-rN` (5.1+) made pytest 4.4 exit 4 with "unrecognized
    // arguments" before running a single test — every id read as failed, and the whole
    // 2019-pytest class graded p2p 0/N (live: pytest-5221 retry, 2026-08-12). Cosmetic
    // flags are not worth a version gate; `-v` and no:cacheprovider go back to 2.x.
    // django is graded from CANONICAL ids, not from prose. `install_django_json_runner`
    // drops a DiscoverRunner subclass into the clone that emits one machine-readable row
    // per test keyed by `test.id()`; `test_invocation` then asks runtests.py to use it.
    // If the drop fails we fall back to the verbose-report parser — reported, never silent,
    // because a fallback that scores is indistinguishable from a fallback that lies.
    let django_json = match runner {
        TestRunner::DjangoRuntests => install_django_json_runner(repo_dir).await,
        TestRunner::Pytest => false,
    };
    let owned_args = test_invocation_with(runner, test_files, django_json);
    let args: Vec<&str> = owned_args.iter().map(String::as_str).collect();
    let Ok(out) = run(&venv_py.to_string_lossy(), &args, Some(repo_dir)).await else {
        return (
            ids.iter().map(|i| (i.clone(), false)).collect(),
            String::new(),
        );
    };
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let (by_node, by_func) = match runner {
        TestRunner::Pytest => parse_pytest_report(&report),
        // Machine-readable rows when the JSON runner is installed. If it emitted NOTHING
        // (an import error in the runner or settings shim, a runtests.py that rejected the
        // invocation, a crash before any test ran) fall back to the verbose report rather
        // than scoring every id as failed — and say so, because a silent fallback here
        // reads as a citizen's zero. This fallback is what kept django-10914's broken
        // `--testrunner` invocation from grading as 40 capability failures.
        TestRunner::DjangoRuntests if django_json => {
            let (by_node, by_func) = parse_django_json(&report);
            if by_node.is_empty() {
                tracing::warn!(
                    "the django JSON runner emitted no rows — falling back to the verbose \
                     report. Grades from this run are less trustworthy (#383)."
                );
                parse_django_report(&report)
            } else {
                (by_node, by_func)
            }
        }
        TestRunner::DjangoRuntests => parse_django_report(&report),
    };
    let verdicts = ids
        .iter()
        .map(|id| {
            (
                id.clone(),
                verdict_for(id, &by_node, &by_func).unwrap_or(false),
            )
        })
        .collect();
    // The report rides back so the grader can excerpt the FAILURE OUTPUT into the
    // verdict — the assertion diff is the teaching half a bare test name lacks.
    (verdicts, report)
}

/// Cap on the failure-output excerpt a verdict carries. Failures and the short
/// summary sit at the END of a test run's output (pytest and sympy's own runner
/// alike), so the TAIL is the format-agnostic excerpt. Bounded so a pathological
/// run (a runaway traceback, a print loop) can't flood the retry prompt.
pub(super) const FAILURE_EXCERPT_MAX: usize = 2000;

/// Tail of a test report, char-capped on a char boundary.
pub(super) fn report_tail(report: &str) -> String {
    let trimmed = report.trim_end();
    if trimmed.len() <= FAILURE_EXCERPT_MAX {
        return trimmed.to_string();
    }
    let mut start = trimmed.len() - FAILURE_EXCERPT_MAX;
    while !trimmed.is_char_boundary(start) {
        start += 1;
    }
    format!("…{}", &trimmed[start..])
}

/// How many PASS_TO_PASS tests to sample. The full set runs to hundreds on some instances and
/// its job is regression detection; a broad sample catches breakage without paying for the
/// tail. Named rather than inlined so a number in a report is traceable to a decision.
pub const P2P_SAMPLE: usize = 40;

/// Grade one candidate patch against one instance, running the full protocol.
///
/// `model_patch` empty means "grade the tree as the solver left it" — the caller has already
/// edited `repo_dir` in place and the diff is implicit.
/// The retry loop's feedback text, composed from BOTH grading reports. The
/// REGRESSION section leads and is unmissable — glass-boxed 2026-08-08
/// (atlas-sympy-24066-n7): her attempt-1 edit broke all 30 pass-to-pass tests,
/// but the old excerpt was built from the fail-to-pass report alone, so the
/// retries never showed her the breakage — she resubmitted a byte-identical
/// broken patch twice. A verdict that hides the collateral damage teaches
/// "my patch just doesn't fix it yet" when the truth is "my patch destroyed
/// the tree". Pure so the composition is unit-testable without a venv.
pub(super) fn compose_failure_excerpt(
    p2p_broken: &[String],
    p2p_report: &str,
    f2p_still_failing: bool,
    f2p_report: &str,
) -> Option<String> {
    let mut sections: Vec<String> = Vec::new();
    if !p2p_broken.is_empty() {
        const NAME_CAP: usize = 10;
        let shown: Vec<&str> = p2p_broken
            .iter()
            .take(NAME_CAP)
            .map(|s| s.as_str())
            .collect();
        let more = p2p_broken.len().saturating_sub(NAME_CAP);
        let more_note = if more > 0 {
            format!(" (+{more} more)")
        } else {
            String::new()
        };
        let tail = if p2p_report.trim().is_empty() {
            String::new()
        } else {
            format!(" Broken-test output tail:\n{}", report_tail(p2p_report))
        };
        sections.push(format!(
            "REGRESSION — your changes BROKE {} test(s) that PASSED before you touched the \
             tree: {}{}. Fix or revert that breakage FIRST: a patch that destroys working \
             behavior grades as a failure no matter what else it does. A wrong symbol name or \
             a deleted line in code you didn't mean to change is the usual cause — re-read \
             your own diff.{}",
            p2p_broken.len(),
            shown.join(", "),
            more_note,
            tail
        ));
    }
    // The target-test tail, where every runner puts its failures + summary.
    // This is what turns "test_issue_24062 failed" into
    // "AssertionError: Dimension(impedance*capacitance/time) != 1".
    if f2p_still_failing && !f2p_report.trim().is_empty() {
        // THE FAILING TEST'S OWN OUTPUT, not the report's tail. 2026-09-12: a 1/2 verdict's
        // excerpt was a wall of `CONTINUUM_TEST {"ok": true}` rows — the one assertion that
        // mattered never reached the holder. The tail stays as the fallback when no section
        // for a named test can be found.
        sections.push(failing_test_excerpt(f2p_report).unwrap_or_else(|| report_tail(f2p_report))); // unwrap_or_else: no per-test section in this report shape = the tail, as before
    }
    if sections.is_empty() {
        None
    } else {
        Some(sections.join("\n\n"))
    }
}

/// The failing tests' OWN output from a harness report: pytest's `_____ test_name _____`
/// sections and its `FAILED node - reason` summary lines; unittest/Django's `FAIL:` / `ERROR:`
/// blocks up to their closing rule. Capped per test and in count so the excerpt is read. `None`
/// when the report carries no such section (a shape the tail must cover).
pub(super) fn failing_test_excerpt(report: &str) -> Option<String> {
    const MAX_TESTS: usize = 3;
    const MAX_LINES: usize = 40;
    let clean = strip_ansi(report);
    let lines: Vec<&str> = clean.lines().collect();
    let mut sections: Vec<String> = Vec::new();
    let is_pytest_head = |l: &str| l.starts_with("____") && l.ends_with("____") && l.trim_matches('_').trim().len() > 0;
    let is_unittest_head = |l: &str| l.starts_with("FAIL: ") || l.starts_with("ERROR: ");
    let mut i = 0;
    while i < lines.len() && sections.len() < MAX_TESTS {
        let l = lines[i].trim_end();
        if is_pytest_head(l) || is_unittest_head(l) {
            let mut block: Vec<&str> = vec![l];
            let mut j = i + 1;
            while j < lines.len() && block.len() < MAX_LINES {
                let m = lines[j].trim_end();
                if is_pytest_head(m) || is_unittest_head(m) || m.starts_with("=====") || m.starts_with("- generated") {
                    break;
                }
                block.push(m);
                j += 1;
            }
            if block.len() >= MAX_LINES {
                block.push("…");
            }
            sections.push(block.join("\n"));
            i = j;
            continue;
        }
        i += 1;
    }
    if sections.is_empty() {
        // pytest's short summary is the last resort: one line per failed test with its reason.
        let summary: Vec<&str> = lines
            .iter()
            .map(|l| l.trim())
            .filter(|l| l.starts_with("FAILED ") || l.starts_with("ERROR "))
            .take(MAX_TESTS)
            .collect();
        if summary.is_empty() {
            return None;
        }
        return Some(summary.join("\n"));
    }
    Some(sections.join("\n\n"))
}

/// The compiler's account of a build that failed, from a harness report — or `None` when
/// the report is a test run. Cargo: from the first `error[E…]` / `error: could not compile`
/// line, the block up to its next blank line (the diagnostic with its `-->` location and
/// `help:`), capped. Go: the `# package` header and the `undefined:` / `cannot use` lines
/// that follow it. Pure so the shape is pinned by a test against the real rustc text.
pub(super) fn build_failure_excerpt(report: &str) -> Option<String> {
    const MAX_LINES: usize = 14;
    let lines: Vec<&str> = report.lines().collect();
    let go_build_failed = report.contains("undefined:") || report.contains("cannot use");
    let start = lines.iter().position(|l| {
        let l = l.trim_start();
        l.starts_with("error[E")
            || l.starts_with("error: could not compile")
            || (go_build_failed && l.starts_with("# "))
    })?;
    let mut block: Vec<&str> = Vec::new();
    for l in &lines[start..] {
        if block.len() >= MAX_LINES {
            block.push("…");
            break;
        }
        if l.trim().is_empty() && !block.is_empty() {
            break;
        }
        block.push(l);
    }
    Some(block.join("\n"))
}

/// THE GOLD GATE: grade the instance's OWN gold patch and require it to resolve.
///
/// This is the spine check [`SweInstance::patch`]'s own doc has promised since that field
/// was written — "the spine check grades THIS; it must resolve or the environment is
/// wrong" — and which did not exist. `grade(.., None)` means "grade the tree as the solver
/// left it", not "grade gold", so nothing in the tree ever validated an env against a
/// known-correct patch.
///
/// WHY THIS IS THE KEYSTONE FOR EVERY NUMBER WE REPORT. Without it a `resolved: false` has
/// two indistinguishable causes: the citizen's patch was wrong, or the environment cannot
/// score a correct patch at all. Measured 2026-08-17 on this box, the second is live and
/// unquantified: a 2019-era django env carries pytest 8.4.2, and the module's own notes
/// record era suites importing pytest internals that modern pytest deleted (flask 2.2's
/// `from _pytest.monkeypatch import notset`). So today an unknown fraction of our zeros are
/// harness artifacts being tallied as capability. That is not a measurement — it is noise
/// with a number attached, and it is why 114/300 (#383) and #380 cannot be told apart from
/// model failure by looking at scores.
///
/// The gate makes the distinction mechanical: gold resolves → the env can score, so a
/// citizen's zero is HERS. Gold fails → the env is disqualified and no result from it may
/// be reported as capability ([[an-absence-is-an-unfinished-measurement]]).
///
/// Deliberately a thin caller over [`grade`], not a parallel scorer: it must exercise the
/// EXACT clone → apply → test path a real attempt takes, or it proves nothing about that
/// path. A second implementation that agreed with itself would be the classic dead
/// instrument.
///
/// `gate_ok == false` in the returned verdict is a DIFFERENT fact and is not a gate
/// failure: it means FAIL_TO_PASS already passed on the pristine tree, so the instance
/// carries no bug here. Callers must not conflate "this task cannot distinguish a fix"
/// with "this environment is broken".
pub async fn gold_gate(instance: &SweInstance) -> SweVerdict {
    let mut verdict = grade(instance, Some(&instance.patch)).await;
    // An env that cannot score its own gold patch is disqualified, and the reason has to
    // survive into the receipt — a bare `resolved: false` here would read downstream as a
    // capability zero, which is the exact confusion this gate exists to end.
    if verdict.error.is_none() && !verdict.resolved {
        verdict.error = Some(format!(
            "GOLD GATE FAILED for {}: the instance's own gold patch did not resolve \
             (FAIL_TO_PASS {}/{}, PASS_TO_PASS {}/{}). The environment cannot score a \
             known-correct patch, so NO result from it is a capability measurement — \
             not a zero, an absence. Era deps are the leading suspect (#380): check the \
             interpreter rung against `interpreter_for_year` and the harness pytest \
             version against what this era's suite can import.",
            instance.instance_id,
            verdict.f2p_passed,
            verdict.f2p_total,
            verdict.p2p_passed,
            verdict.p2p_total,
        ));
    }
    verdict
}

/// The GRADE's own checkout — one stable path per instance, owned by the grading phase and
/// never shared with a solver.
///
/// # A phase owns its path for the phase's whole life (Joel, 2026-08-20)
///
/// Grading used to be handed the SOLVER's working tree and mutate it in place:
/// `reset_worktree` → apply model patch → apply test patch. That makes one path mean different
/// things at different instants — the citizen's workspace is reset out from under her, carries
/// a patch she didn't write, and ends in a state neither phase asked for. Every downstream
/// confusion in this file's history has that shape: `-e .` pinned to whichever tree built the
/// venv first, pristine `p2p` reading 0/N because the grade imported a dirty workspace, the
/// "already passes on the pristine tree" gate firing against a tree that was not pristine.
///
/// So the grade gets its own directory and builds its own state in it, from the base commit,
/// every time. The solver's path stays the solver's for the whole recipe. `clone_at` already
/// clears and re-stages, so the checkout is pristine by construction rather than by a reset we
/// have to remember to call.
pub async fn ensure_grade_checkout(instance: &SweInstance) -> Result<PathBuf, String> {
    let root = swe_cache_dir().join("grades");
    let dir = root.join(&instance.instance_id);
    // Debris from a grade whose process died is swept here, off the async thread: a
    // Django checkout is ~100k files and removing it is seconds of blocking I/O.
    let live = live_grade_ids();
    let _ = tokio::task::spawn_blocking(move || sweep_orphan_grade_checkouts(&root, &live)).await;
    clone_at(instance, &dir).await?;
    Ok(dir)
}

/// Grades in flight in THIS process, counted per instance (two grades of one instance
/// share one path — the tree goes only when the last of them finishes).
pub(super) static LIVE_GRADES: std::sync::LazyLock<std::sync::Mutex<std::collections::HashMap<String, usize>>> =
    std::sync::LazyLock::new(Default::default);

pub(super) fn live_grade_ids() -> std::collections::HashSet<String> {
    LIVE_GRADES.lock().expect("live-grade registry poisoned").keys().cloned().collect() // a poisoned registry means a grade panicked holding it; the count is no longer trustworthy
}

/// A grade's hold on its checkout. The checkout exists for the grade and for nothing
/// after it: `clone_at` re-stages it from the base commit on every grade, so a tree
/// kept past its grade is never read again — it only sits on disk.
///
/// Measured 2026-09-25 (M5): 914,676 retained files under `benchmarks/swe/grades`,
/// every one indexed by Spotlight, and fseventsd grown to 14 GB with grading re-writing
/// thousands of those files per minute. Dropping the hold removes the tree (RAII covers
/// every return path of `grade`, including cancellation); a process that dies mid-grade
/// leaves its tree for [`sweep_orphan_grade_checkouts`] at the next grade.
pub(super) struct GradeCheckoutHold {
    pub(super) instance_id: String,
    pub(super) dir: PathBuf,
}

impl GradeCheckoutHold {
    pub(super) fn take(instance_id: &str) -> Self {
        *LIVE_GRADES
            .lock()
            .expect("live-grade registry poisoned") // a poisoned registry means a grade panicked holding it; the count is no longer trustworthy
            .entry(instance_id.to_string())
            .or_default() += 1;
        Self {
            instance_id: instance_id.to_string(),
            dir: swe_cache_dir().join("grades").join(instance_id),
        }
    }
}

impl Drop for GradeCheckoutHold {
    fn drop(&mut self) {
        let last = {
            let mut live = LIVE_GRADES.lock().expect("live-grade registry poisoned"); // a poisoned registry means a grade panicked holding it; the count is no longer trustworthy
            let n = live.get_mut(&self.instance_id).map(|n| {
                *n -= 1;
                *n
            });
            if n == Some(0) {
                live.remove(&self.instance_id);
            }
            n == Some(0)
        };
        if !last {
            return;
        }
        let dir = self.dir.clone();
        let remove = move || {
            if let Err(e) = std::fs::remove_dir_all(&dir) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    continuum_base::probe!(class = "swe.grade_checkout_release_failed", dir = %dir.display(), error = %e);
                }
            }
        };
        match tokio::runtime::Handle::try_current() {
            Ok(h) => drop(h.spawn_blocking(remove)),
            Err(_) => remove(),
        }
    }
}

/// Remove every grade checkout (and dead `.cloning-*` staging tree) under `root` whose
/// instance is not in `live`. Any tree here that no live grade holds is debris: the next
/// grade of that instance re-clones from the base commit regardless.
pub(super) fn sweep_orphan_grade_checkouts(root: &Path, live: &std::collections::HashSet<String>) -> usize {
    let Ok(entries) = std::fs::read_dir(root) else { return 0 };
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let id = name.split(".cloning-").next().unwrap_or(&name); // split always yields a first piece; the fallback is unreachable
        if live.contains(id) {
            continue;
        }
        let path = entry.path();
        let gone = if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
        if gone.is_ok() {
            removed += 1;
        }
    }
    if removed > 0 {
        continuum_base::probe!(class = "swe.grade_checkouts_swept", root = %root.display(), removed = removed);
    }
    removed
}

/// The gate's decision, as a pure function of the pristine FAIL_TO_PASS run.
#[derive(Debug, PartialEq, Eq)]
pub enum PristineGate {
    /// Every FAIL_TO_PASS test already passes: nothing is left to fix, no score from this
    /// tree can distinguish a fix from a no-op.
    Void { already: Vec<String> },
    /// At least one FAIL_TO_PASS test still fails: the bug is here and the grade stands on
    /// it. `pristine_green` names the ones that were green before any patch.
    Holds { pristine_green: Vec<String> },
}

/// The tests a harness report says were SKIPPED — pytest's `SKIPPED` verdict lines and the
/// Django runner's `"skipped": true` rows. A skipped FAIL_TO_PASS test is neither a pass nor a
/// fail: the environment could not run it (`pytest.importorskip("pandas")` on a box without
/// pandas), so any score from that run is an ENV fault. 2026-09-12: scikit-learn-26323's only
/// f2p test skipped on the pristine tree, the parser counted the skip as green, the gate said
/// "already passes", and a correct fix was voided. Pure; pinned by a test on both shapes.
pub fn skipped_in_report(report: &str) -> Vec<String> {
    let clean = strip_ansi(report);
    let mut out = Vec::new();
    for line in clean.lines() {
        let line = line.trim();
        if let Some(payload) = line.strip_prefix("CONTINUUM_TEST ") {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(payload) {
                if v.get("skipped").and_then(|x| x.as_bool()) == Some(true) {
                    if let Some(id) = v.get("id").and_then(|x| x.as_str()) {
                        out.push(id.to_string());
                    }
                }
            }
            continue;
        }
        if let Some((node, verdict)) = split_node_outcome(line) {
            if node.contains("::") && verdict.starts_with("SKIPPED") {
                out.push(node.to_string());
            }
        }
    }
    out
}

/// Which of `wanted` (FAIL_TO_PASS ids, in any id shape the parsers accept) were skipped —
/// the ids a verdict names in its env fault.
pub fn skipped_of<'a>(wanted: &'a [String], skipped: &[String]) -> Vec<&'a String> {
    wanted
        .iter()
        .filter(|w| {
            skipped.iter().any(|s| {
                s == *w
                    || s.ends_with(w.as_str())
                    || w.ends_with(s.as_str())
                    || s.rsplit("::").next() == w.rsplit("::").next().filter(|f| !f.is_empty())
            })
        })
        .collect()
}

/// THE GATE, decided. Voids only when NOTHING is left to fix.
///
/// Before 2026-09-07 one pristine-green FAIL_TO_PASS test voided the whole instance: ten of
/// the fifteen refusals on the M5 that day were this shape (django-13346: `test_key_iregex`
/// green on this box's sqlite while `test_key_in` — the actual bug — still failed), and the
/// gold gate refused its own gold patch on them. The official harness runs no pristine
/// check at all; ours exists to stop a no-op from scoring, and a tree with a still-failing
/// FAIL_TO_PASS test cannot score a no-op — the grade below still demands EVERY f2p test
/// pass after the patch. So the void is reserved for the case it was written for.
pub fn pristine_gate(pre: &[(String, bool)]) -> PristineGate {
    let green: Vec<String> = pre.iter().filter(|(_, ok)| *ok).map(|(id, _)| id.clone()).collect();
    let any_red = pre.iter().any(|(_, ok)| !*ok);
    if !pre.is_empty() && !any_red {
        PristineGate::Void { already: green }
    } else {
        PristineGate::Holds { pristine_green: green }
    }
}
