use super::*;

/// Inputs to `benchmark/swe-grade`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/SweGradeParams.ts"
)]
pub struct SweGradeParams {
    /// The instance to grade, e.g. `sympy__sympy-22005`.
    pub instance: String,
    /// Dataset to resolve the instance from. Defaults to SWE-bench Lite.
    #[serde(default)]
    #[ts(optional)]
    pub dataset: Option<String>,
    /// THE SPINE CHECK: grade the dataset's own gold patch. It MUST resolve — if it does not,
    /// the environment is wrong and no other number from it means anything. Run this before
    /// trusting any solver's score on an instance you have not graded before.
    #[serde(default)]
    #[ts(optional)]
    pub gold: Option<bool>,
    /// A candidate patch to grade (unified diff). Ignored when `gold` is set.
    #[serde(default)]
    #[ts(optional)]
    pub patch: Option<String>,
    /// A working tree a solver has already edited — its `git diff` becomes the candidate
    /// patch. Grading still happens in a FRESH clone at `base_commit`, so a solver that dirtied
    /// its workspace cannot launder that into a passing score.
    #[serde(default)]
    #[ts(optional)]
    pub workspace: Option<String>,
}

/// Result of `benchmark/swe-grade` — the full verdict, never a bare boolean.
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/SweGradeResult.ts"
)]
pub struct SweGradeResult {
    pub instance: String,
    /// True only when every FAIL_TO_PASS and every sampled PASS_TO_PASS passed.
    pub resolved: bool,
    pub fail_to_pass_passed: u32,
    pub fail_to_pass_total: u32,
    pub pass_to_pass_passed: u32,
    pub pass_to_pass_total: u32,
    /// False when FAIL_TO_PASS already passed on the pristine tree — the checkout carries no
    /// bug, so `resolved: false` from such a run means NOTHING about the solver.
    pub gate_ok: bool,
    /// Set when no verdict could be produced (clone/patch/environment failure). A result with
    /// `error` is an ABSENCE, not a zero, and must never be tallied as a failed attempt.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub error: Option<String>,
    /// How many bytes of candidate patch were graded — 0 means the solver changed nothing,
    /// which is a harness signal, not a model score.
    pub patch_bytes: u32,
    /// The NAMES of the failing tests — the actionable half of the verdict. A count
    /// teaches nothing; a named test is what a human reviewer (or the citizen herself,
    /// next attempt) can actually chase.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub failed_tests: Vec<String>,
    /// The failing FAIL_TO_PASS run's output tail (capped) — the assertion diff.
    /// Names say WHICH test failed; this says WHAT it printed, which is the half
    /// a next attempt (or a human reviewer) actually reasons from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub failure_excerpt: Option<String>,
}

impl From<(SweVerdict, usize)> for SweGradeResult {
    fn from((v, patch_bytes): (SweVerdict, usize)) -> Self {
        SweGradeResult {
            instance: v.instance_id,
            resolved: v.resolved,
            fail_to_pass_passed: v.f2p_passed as u32,
            fail_to_pass_total: v.f2p_total as u32,
            pass_to_pass_passed: v.p2p_passed as u32,
            pass_to_pass_total: v.p2p_total as u32,
            gate_ok: v.gate_ok,
            error: v.error,
            patch_bytes: patch_bytes as u32,
            failed_tests: v.failed_tests,
            failure_excerpt: v.failure_excerpt,
        }
    }
}

/// Grade one SWE-bench instance by the official protocol.
#[derive(Default)]
pub struct BenchmarkSweGrade;

// Self-registered at its own declaration site — the registry has no central list.
crate::register_stateless_command!(BenchmarkSweGrade);

#[async_trait]
impl ActionCommand for BenchmarkSweGrade {
    const NAME: &'static str = "benchmark/swe-grade";
    const ACCESS: AccessLevel = AccessLevel::AiSafe;
    const DESCRIPTION: &'static str =
        "Grade one SWE-bench instance by the official protocol: clone at base_commit, apply the \
         candidate patch, apply the instance's test_patch, run its tests. RESOLVED only when every \
         FAIL_TO_PASS and every sampled PASS_TO_PASS passes. Refuses to report on a tree it cannot \
         vouch for — FAIL_TO_PASS must FAIL on the pristine checkout (that failure IS the bug), and \
         `gateOk: false` marks a run whose score is void. Pass `gold: true` first on any instance \
         not graded here before: the dataset's own patch must resolve, or the environment is wrong \
         and no other number from it means anything.";
    type Params = SweGradeParams;
    type Output = SweGradeResult;

    async fn run(&self, _ctx: &Ctx, p: SweGradeParams) -> Result<SweGradeResult, CommandError> {
        grade_swe(p).await
    }
}

/// Paths that are NEVER part of a solution — the ONE list, shared by every reading of
/// "her work" ([`workspace_candidate_diff_from`] and every staged-workspace reader).
///
/// Two kinds, and the second is a SECURITY boundary, not tidiness:
///
/// 1. Build/cache byproducts. A `python3 -c ...` verify step left
///    `__pycache__/calc.cpython-314.pyc` in a graded patch; real SWE-bench/aider patches are
///    SOURCE-only. Anything a task might legitimately produce (`build`/`dist`/`target`) is kept.
///
/// 2. **Agent-scope state the SUBSTRATE writes into her tree.** airc creates its scope at the
///    enclosing git root, so a citizen working inside a cloned bench repo gets `.airc/` —
///    `events.sqlite`, a work-board cache, and **`identity.key`, a private keypair** — created
///    under the repo she is being graded on. This has bitten twice: card b34f7eb5, where Atlas's
///    first grade carried 91KB of staged `.airc` blobs and the fresh clone refused the WHOLE
///    candidate (a real fix voided by files no solver wrote); and 2026-08-18, where
///    sympy-22714's tree still held `.airc/identity.key` with git status `A` — already
///    intent-added, because `workspace_patch` ran `git add -A -N` with an exclude list that
///    lacked `.airc`. The grader was safe (it excluded `.airc` inline) but `workspace_patch`
///    was not, and IT is the reading that feeds `files_changed` → `format_solve_lesson` →
///    the curriculum. A credential could have been written into training data as
///    "I changed: .airc/identity.key".
///
/// A second inline `git diff` drifts on the exclude rules, so the rule lives in ONE place
/// and every reading consumes it ([[the-compression-principle]]).
pub(crate) const SOLUTION_PATH_EXCLUDES: &[&str] = &[
    // Agent/substrate scope — never authored by the solver, and credential-bearing.
    ":(exclude,glob)**/.airc/**",
    ":(exclude,glob)**/.continuum/**",
    // Build + cache byproducts.
    ":(exclude,glob)**/__pycache__/**",
    ":(exclude,glob)**/*.pyc",
    ":(exclude,glob)**/*.pyo",
    ":(exclude,glob)**/.pytest_cache/**",
    ":(exclude,glob)**/.mypy_cache/**",
    ":(exclude,glob)**/.ruff_cache/**",
    ":(exclude,glob)**/node_modules/**",
    ":(exclude,glob)**/.DS_Store",
];

/// Her work is everything since the instance's BASE COMMIT — committed,
/// staged, and unstaged alike. The old `diff HEAD` collector read only the
/// dirty tree, so a citizen who COMMITTED her fix (sympy-12481, 2026-08-27:
/// "Fix Permutation constructor to compose non-disjoint cycles left-to-right",
/// the exact task, sitting in a commit) graded as "no candidate patch" — the
/// harness punishing her best engineering habit. With `base` given, diff from
/// there. An unreadable base is an error: falling back to HEAD would silently
/// substitute a different candidate, especially when shipping work to another node.
pub(crate) fn workspace_candidate_diff_from(
    ws: &str,
    base: Option<&str>,
) -> Result<String, CommandError> {
    // git diff BASE succeeds even with unmerged index entries, publishing conflict
    // markers and unrelated reverse deltas as a candidate (card 8ae653b1).
    // Same conservative local metadata read bound as work/checkout_change; this
    // is a safety deadline, not a prediction of git latency. Truncation refuses.
    let unmerged = crate::system_resources::bounded_command::probe(
        "git",
        &["-C", ws, "ls-files", "--unmerged", "-z"],
        std::time::Duration::from_secs(10),
    );
    let Some(entries) = unmerged.stdout_if_ok() else {
        return Err(CommandError::Internal(format!(
            "could not inspect {ws}'s candidate index: {}",
            unmerged.outcome()
        )));
    };
    if !entries.is_empty() {
        return Err(CommandError::Invalid(format!(
            "candidate source {ws} has unresolved index conflicts; resolve or abort the merge/rebase in this card's checkout before submitting; work in another checkout is not this candidate"
        )));
    }
    let base = base.unwrap_or("HEAD"); // unwrap_or: the no-base caller explicitly requests working changes against HEAD
    if base.is_empty() || base.starts_with('-') {
        return Err(CommandError::Invalid(
            "candidate base must name a single git commit".into(),
        ));
    }
    // Resolve exactly one commit before diffing: revision ranges and tree/blob
    // objects are not an instance base, even though `git diff` accepts them.
    let revision = format!("{base}^{{commit}}");
    let resolved = std::process::Command::new("git")
        .args(["rev-parse", "--verify", "--end-of-options", &revision])
        .current_dir(ws)
        .output()
        .map_err(|e| CommandError::Internal(format!("could not resolve candidate base: {e}")))?;
    if !resolved.status.success() {
        return Err(CommandError::Invalid(format!(
            "candidate base {base} must name a single git commit: {}",
            String::from_utf8_lossy(&resolved.stderr).trim()
        )));
    }
    let commit = String::from_utf8(resolved.stdout)
        .map_err(|e| CommandError::Internal(format!("invalid git commit id: {e}")))?;
    // A transferable patch must contain binary changes and must not depend on
    // this machine's external diff/textconv programs or display preferences.
    let mut args: Vec<&str> = vec![
        "diff",
        "--binary",
        "--full-index",
        "--no-ext-diff",
        "--no-textconv",
        "--no-color",
        "--src-prefix=a/",
        "--dst-prefix=b/",
        commit.trim(),
        "--",
        ".",
    ];
    args.extend_from_slice(SOLUTION_PATH_EXCLUDES);
    let out = std::process::Command::new("git")
        .args(&args)
        .current_dir(ws)
        .output()
        .map_err(|e| CommandError::Internal(format!("could not read {ws}'s diff: {e}")))?;
    if !out.status.success() {
        return Err(CommandError::Internal(format!(
            "could not read {ws}'s candidate against {base} ({}): {}",
            out.status,
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    String::from_utf8(out.stdout).map_err(|e| {
        CommandError::Internal(format!(
            "candidate diff is not UTF-8; refusing to change patch bytes: {e}"
        ))
    })
}

/// The `benchmark/swe-grade` body, callable without a command context — the
/// hands-free autograde on `agent/solve` completion invokes the SAME grader
/// The base commit an instance's checkout stands on, found the way the grader finds
/// it: every known SWE dataset searched for the row. `None` = no dataset names it —
/// an absence, never a guessed HEAD. Read by `work/submit` to shape a citizen's
/// submission from her checkout (2026-09-17).
pub(crate) async fn swe_base_commit_for(instance: &str) -> Option<String> {
    let datasets: Vec<String> = known_benchmarks()
        .iter()
        .filter_map(|b| b.swe_dataset())
        .map(|d| d.to_string())
        .collect();
    for dataset in &datasets {
        let rows = swe_bench::load_dataset(dataset).await.ok()?;
        if let Some(r) = rows.into_iter().find(|r| r.instance_id == instance) {
            return Some(r.base_commit);
        }
    }
    None
}

/// (fresh clone at base_commit, held-out tests, experience-stream write) as
/// the operator verb. One grader, never two.
pub(crate) async fn grade_swe(p: SweGradeParams) -> Result<SweGradeResult, CommandError> {
    // Dataset resolution: an explicit dataset wins; otherwise SEARCH every known
    // SWE dataset for the instance. The old default hardcoded Lite, so any
    // Verified-only instance was ungradeable — glass-boxed live 2026-08-26:
    // astropy-13236 (a swe-bench-verified dispatch) auto-graded as
    // "[not_found] … not found in princeton-nlp/SWE-bench_Lite" and the whole
    // attempt's verdict vanished. The instance names its dataset; the grader's
    // job is to find it, not to guess one.
    let candidate_datasets: Vec<String> = match p.dataset.clone() {
        Some(d) => vec![d],
        None => known_benchmarks()
            .iter()
            .filter_map(|b| b.swe_dataset())
            .map(|d| d.to_string())
            .collect(),
    };
    let mut instance_row = None;
    let mut searched = Vec::new();
    for dataset in &candidate_datasets {
        let rows = swe_bench::load_dataset(dataset)
            .await
            .map_err(CommandError::Internal)?;
        if let Some(r) = rows.into_iter().find(|r| r.instance_id == p.instance) {
            instance_row = Some((r, dataset.clone()));
            break;
        }
        searched.push(dataset.clone());
    }
    let Some((instance, dataset)) = instance_row else {
        return Err(CommandError::NotFound(format!(
            "{} not found in any known SWE dataset (searched: {})",
            p.instance,
            searched.join(", ")
        )));
    };
    let _ = &dataset; // named for the receipt below; instance carries everything else

    // Resolve the candidate patch. A workspace's diff is READ here but graded in a fresh
    // clone below — where the solver worked is never where the score is taken.
    // RESOLVE WHICH COPY, rather than trusting the caller to have picked right.
    //
    // The same instance is legitimately staged into MULTIPLE citizens' workspaces —
    // dispatch round-robins over the roster. On 2026-08-18 `astropy__astropy-14995` sat in
    // Atlas's tree (dirty: a real fix) AND Asha's (clean: staged, never worked), and an
    // operator grade pointed at the clean one returned a confident `resolved: false` for
    // work that existed ten directories away. Deletion was never the risk; AMBIGUITY was.
    //
    // So an omitted `workspace` no longer means "no candidate" — it means ASK. The answer
    // comes from `staged_workspace`, the module that already owns "which checkout is this
    // instance", and it refuses on ambiguity for the same reason its sibling does: grading
    // either of two worked copies scores one citizen's diff against another's card.
    let resolved_workspace: Option<String> = match p.workspace.clone() {
        Some(ws) => Some(ws),
        None if p.gold.unwrap_or(false) || p.patch.is_some() => None,
        None => {
            use crate::persona::staged_workspace::{grade_target, owners_of, GradeTarget};
            let copies = owners_of(&instance.instance_id);
            match grade_target(&copies) {
                GradeTarget::One(path) => {
                    continuum_base::probe!(
                        class = "benchmark.grade.workspace_resolved",
                        instance = %instance.instance_id,
                        staged_copies = copies.len(),
                        path = %path.display(),
                        "resolved the one WORKED staged copy — never guessed between citizens"
                    );
                    Some(path.to_string_lossy().to_string())
                }
                // No worked copy: fall through with no candidate. The empty-candidate guard
                // below turns that into an ABSENCE, which is the honest verdict.
                GradeTarget::NoWork => None,
                GradeTarget::Ambiguous(paths) => {
                    return Err(CommandError::Invalid(format!(
                        "{} is staged with real work in {} citizens' workspaces — refusing to \
                         guess which one this grade is about, because grading either scores one \
                         citizen's diff against the other's card. Pass workspace=<path> \
                         explicitly. Candidates: {}",
                        instance.instance_id,
                        paths.len(),
                        paths
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )));
                }
            }
        }
    };

    let candidate: Option<String> = if p.gold.unwrap_or(false) {
        Some(instance.patch.clone())
    } else if let Some(ws) = resolved_workspace.as_ref() {
        Some(workspace_candidate_diff_from(ws, Some(&instance.base_commit))?)
    } else {
        p.patch.clone()
    };
    let patch_bytes = candidate.as_ref().map(|c| c.len()).unwrap_or(0);

    // AN EMPTY CANDIDATE IS AN ABSENCE, NOT A ZERO — refuse before spending a clone on it.
    //
    // Grading a pristine tree at `base_commit` ALWAYS yields `resolved: false` with
    // `gate_ok: true`: the FAIL_TO_PASS test correctly fails because the bug is still there.
    // That is byte-identical downstream to a citizen who tried and missed, so recording it
    // manufactures a capability zero out of nothing — the #384/#386 class, and the exact
    // failure this arm's own `gold_gate` comment exists to prevent one screen above.
    //
    // Caught by the positive control on 2026-08-18, minutes after verdict persistence landed:
    // re-grading astropy-14995 (a REAL pass, watched at F2P 1/1 / P2P 40/40 that afternoon)
    // returned `patchBytes: 0, resolved: false` and PERSISTED it. Her tree had been re-cloned
    // — `git reflog` showed exactly two entries, `clone` then `checkout`, no work — so the
    // artifact was gone and the harness scored its own absence as her failure. One run of the
    // control turned a silent laundering bug into a named one.
    //
    // Also stops the experience stream teaching "you failed" from a tree that was wiped: the
    // append below gates on `error.is_none()`, so an absence correctly teaches nothing.
    if !p.gold.unwrap_or(false) && patch_bytes == 0 {
        return Ok(SweGradeResult::from((
            SweVerdict {
                instance_id: instance.instance_id.clone(),
                error: Some(format!(
                    "no candidate patch to grade for {} — the workspace holds no diff (a fresh \
                     or reset checkout), so there is nothing to score. This is an ABSENCE, not \
                     a failed attempt: grading a pristine tree would report resolved=false for \
                     a citizen who never got the chance.",
                    instance.instance_id
                )),
                ..Default::default()
            },
            patch_bytes,
        )));
    }

    let work = swe_bench::swe_cache_dir()
        .join("work")
        .join(&instance.instance_id);
    let repo = work.join("repo");
    let _ = std::fs::create_dir_all(&work);
    if let Err(e) = swe_bench::clone_at(&instance, &repo).await {
        return Ok(SweGradeResult::from((
            SweVerdict {
                instance_id: instance.instance_id,
                error: Some(e),
                ..Default::default()
            },
            patch_bytes,
        )));
    }
    // THE SPINE CHECK IS NOW ENFORCED, not just run. `gold` graded through the plain
    // `grade` path returned a bare `resolved: false`, which is byte-identical downstream to
    // a citizen's capability zero — so the `gold` doc's own demand ("if it does not, the
    // environment is wrong and no other number from it means anything") was a sentence
    // addressed to a human and enforced by nobody. `gold_gate` stamps the verdict's `error`
    // with WHY, and an `error` is contractually an ABSENCE, never a tallied failure
    // (see `SweVerdict::error`). One path, so every caller inherits the labelling.
    let verdict = if p.gold.unwrap_or(false) {
        swe_bench::gold_gate(&instance).await
    } else {
        swe_bench::grade(&instance, candidate.as_deref()).await
    };
    // STAMP the adapter and the citizen at the ONE place this arm produces a verdict. The
    // verdict store is SHARED across adapters (benchmarks are adapters, not runners), so a
    // reader must be able to ask which benchmark scored a row and whose hands it scored —
    // as FIELDS. Recovering either by parsing the shape of `instance_id` or by guessing at
    // a workspace path is the string-shape inference this repo forbids, and it is why a
    // resolved row could never say who resolved it.
    let verdict = {
        let mut verdict = verdict;
        verdict.benchmark = p.dataset.clone().unwrap_or_default();
        // The staged checkout's own path names its owner; `citizen_peer_dir_of` is the
        // inverse of the layout `resolve_solver_dir` builds, so writer and reader cannot
        // drift. An operator grade of a bare patch has no citizen dir and stays empty —
        // honestly absent, never a guessed name.
        verdict.solver = p
            .workspace
            .as_deref()
            .map(std::path::Path::new)
            .and_then(citizen_peer_dir_of)
            .and_then(|d| d.file_name().map(|n| n.to_string_lossy().into_owned()))
            .unwrap_or_default();
        verdict
    };

    // PERSIST THE VERDICT before anything else consumes it. Until 2026-08-18 this arm
    // computed a score, taught from it, and returned it — writing nothing durable. Two real
    // Lite resolutions were watched passing that afternoon and left no trace; `benchmark/runs`
    // went on rendering both artifacts as `ungraded`, and the day's honest rate could not be
    // stated from anything the system held. A measurement the system cannot remember is not a
    // measurement. `record_verdict` itself refuses gold and errored verdicts, so the board can
    // never be laundered by a positive control or an env fault (see its doc).
    let recorded = swe_bench::record_verdict(&verdict, p.gold.unwrap_or(false));
    let verdict_written = matches!(recorded, Ok(Some(_)));
    // A close that does NOT happen must be a row, never an absence (IntelMac,
    // batch8 read): a fix that makes the system say less ships the probe for its
    // new quiet state, or the next reader mistakes "skipped" for "nothing to do".
    let board_close_skipped: Option<&str> = match &recorded {
        Ok(Some(_)) => None,
        Ok(None) => Some("no_verdict_written"),
        Err(_) => Some("verdict_not_persisted"),
    };
    match recorded {
        Ok(Some(path)) => continuum_base::probe!(
            class = "benchmark.verdict.recorded",
            instance = %verdict.instance_id,
            resolved = verdict.resolved,
            path = %path.display(),
            "verdict persisted — the board and every later reader now see this grade"
        ),
        Ok(None) => {}
        // Fail LOUD in the log but never fail the grade: the verdict in hand is still true,
        // and refusing to return it would lose the measurement twice over.
        Err(e) => tracing::warn!(
            instance = %verdict.instance_id,
            error = %e,
            "VERDICT NOT PERSISTED — this grade is real but the system will forget it"
        ),
    }
    // THE BOARD FOLLOWS THE VERDICT, both directions — one place for every grading path
    // (`modules::verdict_board`): resolved closes the card and the room hears it; failed
    // returns the card to its holder with the failing tests named. Only a RECORDED verdict
    // moves anything: an ungradeable result (record_verdict → Ok(None)) leaves the card
    // where it was — on 7d22a26e6 the first sweep closed 13346/26323/14983's cards on
    // refusals and a citizen's real patch vanished from the deck without a grade.
    if verdict_written && !p.gold.unwrap_or(false) {
        crate::modules::verdict_board::follow(&verdict).await;
    }
    if let Some(reason) = board_close_skipped {
        if !p.gold.unwrap_or(false) {
            continuum_base::probe!(
                class = "benchmark.verdict.board_close_skipped",
                instance = verdict.instance_id.as_str(),
                reason,
                "no verdict was written, so the board card stays where it was — the instance is not settled"
            );
        }
    }

    // #319: a WORKSPACE grade is a citizen's lived, objectively judged work —
    // append it to her experience stream. Only her: the gold/raw-patch arms are
    // harness plumbing, not experience. And only a REAL verdict: an errored run
    // is an ABSENCE (harness fault), and teaching from a harness failure would
    // corrupt the reward signal (`an_errored_verdict_is_an_absence_not_a_zero`).
    if verdict.error.is_none() {
        // The RESOLVED workspace, not the caller's parameter: a grade that resolved the
        // owner must teach THAT citizen, or the lesson lands on nobody (and, before
        // resolution existed, could have landed on whoever the operator happened to name).
        if let Some(peer_dir) = resolved_workspace
            .as_ref()
            .and_then(|ws| citizen_peer_dir_of(std::path::Path::new(ws)))
        {
            let task = crate::cognition::eval::EvalTask {
                id: instance.instance_id.clone(),
                prompt: instance.problem_statement.clone(),
                ..Default::default()
            };
            // Name the failures — a count is a score, a name is a lesson (Joel,
            // 2026-08-08). "PASS_TO_PASS 6/11" told Atlas nothing; "your change
            // broke test_arguments" is what a human reviewer would have said.
            let broke = if verdict.failed_tests.is_empty() {
                String::new()
            } else {
                format!(" — failing: {}", verdict.failed_tests.join(", "))
            };
            let detail = format!(
                "swe-bench {}: resolved={} FAIL_TO_PASS {}/{} PASS_TO_PASS {}/{}{}",
                instance.instance_id,
                verdict.resolved,
                verdict.f2p_passed,
                verdict.f2p_total,
                verdict.p2p_passed,
                verdict.p2p_total,
                broke
            );
            let episode = crate::cognition::experience::ExperienceRecord::from_kanban_grade(
                &task,
                candidate.as_deref().unwrap_or(""),
                verdict.resolved,
                &detail,
            );
            if let Err(e) = crate::cognition::experience::append_experience(&peer_dir, &episode) {
                tracing::warn!(
                    workspace = ?p.workspace,
                    error = %e,
                    "swe-grade outcome could not be appended to the experience \
                     stream — the verdict stands, but this lesson was LOST"
                );
            }
        }
    }

    Ok(SweGradeResult::from((verdict, patch_bytes)))
}

/// The citizen peer dir owning a workspace path: the `<...>/citizens/peers/<uuid>`
/// prefix of `path`, or `None` when the path is not inside a citizen's home (an
/// operator scratch tree, the gold arm's cache clone). Path shape is the SAME one
/// `resolve_solver_dir` resolves into — this is its inverse, not a second layout.
pub(super) fn citizen_peer_dir_of(path: &std::path::Path) -> Option<std::path::PathBuf> {
    let comps: Vec<&std::ffi::OsStr> = path.iter().collect();
    let peers_at = comps
        .windows(2)
        .position(|w| w[0] == "citizens" && w[1] == "peers")?;
    // citizens/peers/<uuid> — need the uuid component after the pair.
    let uuid_at = peers_at + 2;
    if uuid_at >= comps.len() {
        return None;
    }
    Some(comps[..=uuid_at].iter().collect())
}

#[cfg(test)]
pub(super) mod swe_grade_tests {
    use super::super::*;

    // what this catches: name/access wiring — grading is a read on the AiSafe surface, so a
    // persona can score her own work without an operator in the loop.
    #[test]
    fn swe_grade_name_and_access_wired() {
        assert_eq!(BenchmarkSweGrade::NAME, "benchmark/swe-grade");
        assert!(matches!(BenchmarkSweGrade::ACCESS, AccessLevel::AiSafe));
    }

    // what this catches: a verdict that could not run must NOT read as a scored zero. Tallying
    // an environment failure as "the model failed" is how a broken harness becomes a number.
    #[test]
    fn an_errored_verdict_is_an_absence_not_a_zero() {
        let v = SweVerdict {
            instance_id: "x__y-1".into(),
            error: Some("could not install repo".into()),
            ..Default::default()
        };
        let r = SweGradeResult::from((v, 0));
        assert!(!r.resolved);
        assert!(r.error.is_some(), "the reason must survive to the caller");
        assert_eq!(
            r.fail_to_pass_total, 0,
            "no tests ran, so nothing was attempted"
        );
    }

    // what this catches: the experience-stream producer must attribute a graded
    // workspace to the RIGHT citizen and stay silent for non-citizen paths — a
    // wrong peer dir would file her lesson into someone else's mind (#319).
    #[test]
    fn citizen_peer_dir_resolves_from_workspace_path_or_not_at_all() {
        let p = std::path::Path::new(
            "/Users/x/.continuum/citizens/peers/fe4dac17-aaaa-4bbb-8ccc-000000000001/workspace/swe/pallets__flask-4992",
        );
        let d = citizen_peer_dir_of(p).expect("workspace path resolves");
        assert!(d.ends_with("citizens/peers/fe4dac17-aaaa-4bbb-8ccc-000000000001"));
        assert!(
            citizen_peer_dir_of(std::path::Path::new("/tmp/swe-work/repo")).is_none(),
            "an operator scratch tree is nobody's experience"
        );
        assert!(
            citizen_peer_dir_of(std::path::Path::new("/x/citizens/peers")).is_none(),
            "the pair with no uuid after it must not resolve"
        );
    }

    // what this catches: the gate is reported, not just enforced. A caller tallying results
    // must be able to EXCLUDE ungradeable instances rather than counting them as failures.
    #[test]
    fn the_gate_verdict_reaches_the_caller() {
        let v = SweVerdict {
            instance_id: "x__y-1".into(),
            gate_ok: false,
            error: Some("UNGRADEABLE — FAIL_TO_PASS already passes".into()),
            ..Default::default()
        };
        let r = SweGradeResult::from((v, 512));
        assert!(!r.gate_ok);
        assert_eq!(
            r.patch_bytes, 512,
            "patch size is reported even when the tree is void"
        );
    }

}
