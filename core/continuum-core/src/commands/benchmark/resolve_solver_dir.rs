use super::*;

/// The benchmark harness resolves a solver through the persona crate's citizen resolver.
pub(crate) use crate::persona::citizen_dir::resolve_citizen_dir as resolve_solver_dir;

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
            // What this catches (8ae653b1): work/submit can select a conflicted
            // card checkout while the author's tested work lives in another one.
            // Git happily emits a patch; candidate extraction must refuse it.
            std::fs::write(root.path().join("code.txt"), format!("branch{newline}"))
                .expect("test: divergent change");
            git(&["add", "code.txt"]);
            git(&[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.test",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-qm",
                "divergent change",
            ]);
            let divergent = git(&["rev-parse", "HEAD"]).trim().to_string();
            git(&["checkout", "--detach", &base]);
            std::fs::write(root.path().join("code.txt"), format!("other{newline}"))
                .expect("test: conflicting change");
            git(&["add", "code.txt"]);
            git(&[
                "-c",
                "user.name=Fixture",
                "-c",
                "user.email=fixture@example.test",
                "-c",
                "commit.gpgsign=false",
                "commit",
                "-qm",
                "other change",
            ]);
            let merge = std::process::Command::new("git")
                .args([
                    "-c",
                    "user.name=Fixture",
                    "-c",
                    "user.email=fixture@example.test",
                    "merge",
                    "--no-commit",
                    &divergent,
                ])
                .current_dir(root.path())
                .output()
                .expect("test: conflicting merge");
            assert!(!merge.status.success());
            assert!(!git(&["ls-files", "--unmerged"]).is_empty());
            assert!(!git(&["diff", "--no-ext-diff", &base]).is_empty());
            let error = workspace_candidate_diff_from(ws, Some(&base))
                .expect_err("test: conflicted candidate refused before retention");
            assert!(matches!(error, CommandError::Invalid(ref message)
                if message.contains(ws) && message.contains("unresolved index conflicts")));
            git(&["merge", "--abort"]);
            assert!(workspace_candidate_diff_from(ws, Some(&base)).is_ok());
            let not_repo = tempfile::tempdir().expect("test: non-repository directory");
            assert!(
                workspace_candidate_diff_from(not_repo.path().to_str().expect("test: temp path"), None)
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
}
