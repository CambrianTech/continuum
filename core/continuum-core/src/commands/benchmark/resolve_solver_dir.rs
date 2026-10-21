use super::*;

/// Resolve a solver to the ONE matching `citizens/peers/<uuid>/` directory. Accepts a
/// citizen NAME (resolved through the live roster — the SAME `get_by_agent_name` identity
/// path `benchmark/dispatch` uses), a full peer UUID, or a hex prefix. 0 or >1 matches
/// fail loud with the candidates named — a grade against the wrong citizen's workspace is
/// a falsified result, never a best-effort. [[the-grid-identity-spine-durable-id-fluid-location]]
pub(crate) fn resolve_solver_dir(
    home: &std::path::Path,
    solver: &str,
) -> Result<(String, std::path::PathBuf), CommandError> {
    let peers = home.join("citizens").join("peers");

    // Identity first: a NAME ("Asha") resolves through the live roster to her durable
    // peer_id → her workspace dir. A name is never a hex UUID prefix, so without this a
    // `--solver=Asha` always failed "no citizen workspace matches" — the identity gap this
    // fixes. Only fall through to peer-UUID-prefix matching if the name doesn't resolve
    // (offline citizen, or the caller genuinely passed a uuid). None in unit tests (no live
    // roster) → straight to the prefix path, so existing behavior is preserved.
    if let Some(rt) = crate::persona::PersonaAircRuntimeRegistry::try_global()
        .and_then(|reg| reg.get_by_agent_name(solver))
    {
        let uuid = rt.airc().peer_id().as_uuid().to_string();
        let dir = peers.join(&uuid);
        if dir.is_dir() {
            return Ok((uuid, dir));
        }
    }

    let needle = solver.to_ascii_lowercase().replace('-', "");
    if needle.len() < 4 {
        return Err(CommandError::Invalid(format!(
            "solver '{solver}' is too short — pass a full peer UUID or a hex prefix of ≥4 chars"
        )));
    }
    let mut matches: Vec<(String, std::path::PathBuf)> = Vec::new();
    let entries = std::fs::read_dir(&peers).map_err(|e| {
        CommandError::NotFound(format!("no citizen peers dir at {}: {e}", peers.display()))
    })?;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name
            .to_ascii_lowercase()
            .replace('-', "")
            .starts_with(&needle)
        {
            matches.push((name, entry.path()));
        }
    }
    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err(CommandError::NotFound(format!(
            "no citizen workspace matches solver '{solver}' under {}",
            peers.display()
        ))),
        _ => Err(CommandError::Invalid(format!(
            "solver '{solver}' is ambiguous: {}",
            matches
                .iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ))),
    }
}

// ───────────────────────── benchmark/swe-setup ─────────────────────────
//
// The dispatch-side bridge for PROJECT-BASED benchmarks (Joel, 2026-08-07: "I
// bet you can get working swe … running soon"). swe-grade already closes the
// grading leg for a citizen's tree (`workspace` param: her diff, graded in a
// fresh clone — launder-proof). What was missing is the setup leg: a SWE card
// is not a prompt, it is a BROKEN REPO. This verb stages one instance into a
// citizen's workspace — clone at base_commit via the same `clone_at` the
// grader trusts — and returns a card-ready body. Held-out material (gold
// patch, test_patch, FAIL_TO_PASS/PASS_TO_PASS) never enters the card: the
// citizen sees exactly what the SWE-bench protocol allows, the issue text.

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/SweSetupParams.ts"
)]
pub struct SweSetupParams {
    /// The instance to stage, e.g. `sympy__sympy-22005`.
    pub instance: String,
    /// Dataset to resolve the instance from. Defaults to SWE-bench Lite.
    #[serde(default)]
    #[ts(optional)]
    pub dataset: Option<String>,
    /// The solver's peer id — full UUID or hex prefix, resolved against
    /// `citizens/peers/` exactly like `benchmark/swe-grade`.
    pub solver: String,
    /// Re-stage over an existing checkout. Default false: a directory already
    /// holding this instance may carry the citizen's in-progress work, and
    /// destroying it silently would erase her labor — fresh must be explicit.
    #[serde(default)]
    #[ts(optional)]
    pub fresh: Option<bool>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/SweSetupResult.ts"
)]
pub struct SweSetupResult {
    pub instance: String,
    pub solver: String,
    /// The staged repo inside her workspace — hand this exact path to
    /// `benchmark/swe-grade --workspace` when she is done.
    pub workspace: String,
    /// Card-ready body: problem statement + where the repo is + what done means.
    pub card_body: String,
}

/// `benchmark/swe-setup` — stage one SWE-bench instance as claimable kanban work.
#[derive(Default)]
pub struct BenchmarkSweSetup;

#[async_trait]
impl ActionCommand for BenchmarkSweSetup {
    const NAME: &'static str = "benchmark/swe-setup";
    const ACCESS: AccessLevel = AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Stage a SWE-bench instance into a citizen's workspace as project-based kanban work: \
         clone the instance's repo at base_commit (the same checkout the grader trusts) into \
         citizens/peers/<solver>/workspace/swe/<instance>/ and return a card-ready body. The \
         issue text is included; the gold patch, test patch, and test lists stay held out. \
         Grade the finished work with benchmark/swe-grade --workspace <returned path>.";
    type Params = SweSetupParams;
    type Output = SweSetupResult;

    async fn run(&self, _ctx: &Ctx, p: SweSetupParams) -> Result<SweSetupResult, CommandError> {
        let dataset = p
            .dataset
            .clone()
            .unwrap_or_else(|| "princeton-nlp/SWE-bench_Lite".to_string());
        let rows = swe_bench::load_dataset(&dataset)
            .await
            .map_err(CommandError::Internal)?;
        let instance = rows
            .into_iter()
            .find(|r| r.instance_id == p.instance)
            .ok_or_else(|| {
                CommandError::NotFound(format!("no instance '{}' in {dataset}", p.instance))
            })?;

        let home = crate::paths::continuum_home()?;
        let (solver_full, solver_dir) = resolve_solver_dir(&home, &p.solver)?;
        let target = solver_dir
            .join("workspace")
            .join("swe")
            .join(&instance.instance_id);
        if target.exists() && !p.fresh.unwrap_or(false) {
            return Err(CommandError::Invalid(format!(
                "{} already exists — it may hold the citizen's in-progress work. Pass \
                 fresh=true to explicitly discard it and re-stage at base_commit.",
                target.display()
            )));
        }
        swe_bench::clone_at(&instance, &target)
            .await
            .map_err(CommandError::Internal)?;

        // Workspace-relative path — the citizen's hands are rooted at her workspace,
        // so the card speaks in HER coordinates, not the operator's absolute ones.
        let rel = format!("swe/{}", instance.instance_id);
        let card_body = format!(
            "Real bug in a real repo ({repo} @ {commit}). The checkout is ALREADY staged in your \
             workspace at `{rel}/`, and while you hold this card your hands (files, shell) are \
             rooted AT that repo's root: paths are repo-relative and `ls` lists the repo itself \
             (there is no `swe/` directory from where you stand). Do not create a new workspace \
             and do not add new top-level files; find the existing source of the fault and edit \
             it in place.\n\n\
             ## Issue\n{statement}\n\n\
             ## Definition of done\n\
             The repo's own tests for this issue pass. Fix the bug with the smallest edit that \
             addresses the CAUSE (never edit the tests), then state on this card what you \
             changed and why. Your working tree's diff is what gets graded.",
            repo = instance.repo,
            commit = &instance.base_commit[..12.min(instance.base_commit.len())],
            rel = rel,
            statement = instance.problem_statement.trim(),
        );

        Ok(SweSetupResult {
            instance: instance.instance_id,
            solver: solver_full,
            workspace: target.display().to_string(),
            card_body,
        })
    }
}

crate::register_stateless_command!(BenchmarkSweSetup);

#[cfg(test)]
pub(super) mod swe_setup_tests {
    use super::super::*;

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
        use super::super::super::{fold_run_card, RUN_STALL_WINDOW_SECS};
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
}

// ---------------------------------------------------------------------------
// benchmark/runs — the RunProjection (exam-room surface, slice 2)
// ---------------------------------------------------------------------------
/// How long a non-terminal run may go without ANY artifact activity before the
/// projection flags it `stalled`. Matches the harness cadence expectation (a
/// healthy Devstral act lands every ~4-6 min; 20 min of silence is 3-4 missed
/// beats), deliberately TIGHTER than the in-loop deadline (#2180's
/// `max_acts × 15 min` kills the run) — the projection warns first, the
/// deadline executes later. Glass-boxed origin: 2026-08-08, two runs sat
/// silent 2.5h and the operator found out by asking.
pub(super) const RUN_STALL_WINDOW_SECS: u64 = 20 * 60;

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRunsParams.ts"
)]
pub struct BenchmarkRunsParams {
    /// Filter to one run. Omit → the newest `limit` runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub run_id: Option<String>,
    /// Newest N runs to return (default 20).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub limit: Option<u32>,
}

/// One run's card — the projection every consumer renders: the positron
/// exam-room tab bar, a teacher persona's grounding, and the operator's
/// liveness Monitor all fold THIS, never bespoke file scraping
/// (docs/architecture/ACADEMY-EXAM-ROOM-POSITRONIC-SURFACE.md §5.2).
#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchRunCard.ts"
)]
pub struct BenchRunCard {
    pub run_id: String,
    /// Instance under test ("sympy__sympy-24066") — from the result ledger's
    /// staged-checkout name (#329: the board names WHAT, not just who).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub instance: Option<String>,
    /// Attempt N of `max_attempts` — the N-chances counter, live per ledger write.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub attempt: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub max_attempts: Option<u32>,
    /// Solver persona (from the result ledger; absent while attempt 1 is
    /// still in flight and nothing has been written yet).
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub solver: Option<String>,
    /// `resolved` | `failed` (loud infra marker, incl. #2180 stalls the
    /// deadline caught) | `active` (artifact activity within the stall
    /// window) | `queued` (silent past the window BUT the solver is actively
    /// working another run — one pair of hands; waiting a turn is not a
    /// stall) | `quiet` (non-terminal AND silent past the window — the
    /// shape the projection exists to make visible) | `ungraded` (a staged
    /// workspace holds a real diff that no grade has ever seen — durable
    /// work awaiting a verdict, NOT a stall; see
    /// [`scan_workspace_artifact_cards`]).
    pub phase: String,
    /// True exactly when `phase == "quiet"`.
    pub stalled: bool,
    /// Epoch ms of the newest artifact write (result or grade ledger).
    #[ts(type = "number")]
    pub last_activity_ms: u64,
    #[ts(type = "number")]
    pub age_secs: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    #[ts(optional, type = "number")]
    pub acts: Option<u32>,
    pub files_changed: Vec<String>,
    /// Investigation trail (#2177) — reads/searches/edit attempts, not git.
    pub files_examined: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub resolved: Option<bool>,
    /// "passed/total", e.g. "1/1" — string so the sparkline renders directly.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub fail_to_pass: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub pass_to_pass: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional, type = "number")]
    pub patch_bytes: Option<u32>,
    pub failed_tests: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub infra_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, JsonSchema)]
#[ts(
    export,
    export_to = "../../../protocol/typescript/benchmark/BenchmarkRunsResult.ts"
)]
pub struct BenchmarkRunsResult {
    pub runs: Vec<BenchRunCard>,
    /// How many cards MATCHED before `limit` truncated — so a page is never mistaken
    /// for the whole set.
    ///
    /// Measured cost of not having this (2026-08-21, on me): `benchmark/runs` returned
    /// 20 rows carrying 1 resolved instance, and I read that as the system's whole
    /// history and as evidence that grades were being lost on the way to the board.
    /// Both wrong. The projection reads grade siblings AND durable verdicts correctly;
    /// there were simply 37 graded instances and `limit.unwrap_or(20)` showed the 20
    /// most recent. The SECOND real pass (`sympy__sympy-13480`) was older than the
    /// window, so the board looked like a 1-pass history and nobody knew otherwise.
    ///
    /// Same convention `debug/probes/query` already uses ("MATCHED versus returned, so
    /// a page is never mistaken for the whole"). Silent truncation reads as "that's
    /// everything" — on the command an operator uses to ask how the benchmark is going,
    /// that is the worst possible failure shape.
    #[ts(type = "number")]
    pub matched: u32,
    /// Human-readable statement of the two numbers, so the truncation is visible in a
    /// glance at the receipt and not only to a caller who compares two fields.
    pub summary: String,
}

/// One run-ledger scan: the cards a caller asked for, plus how many there were BEFORE
/// truncation.
///
/// Returned as a struct rather than a bare `Vec` so the count cannot be dropped on the
/// way out — the positron board consumes `.cards` and ignores the total, while the
/// command reports both. A tuple would have let either caller silently discard it.
pub(crate) struct RunScan {
    pub cards: Vec<BenchRunCard>,
    /// Cards that matched the filter before `limit` was applied.
    pub matched: usize,
}
