//! `gym/mine` — real-repo task mining: bugfix-revert on well-tested crates.
//!
//! The SWE-bench construction, done locally on Rust (#133,
//! docs/planning/SWE-PROJECT-SOURCING.md): walk a crate's git history for
//! commits that FIX code and ADD/TOUCH tests in the same commit; re-create the
//! broken state (source reverted to the parent, tests kept at the fix); verify
//! the task is REAL both ways (broken → `cargo test` fails; fixed → passes);
//! emit an [`EvalTask`](crate::cognition::eval::EvalTask) with a `dod_shell`
//! grade and a `setup_shell` that re-breaks the checkout for repeatable runs.
//!
//! This is the "problems our small models shouldn't be capable of" proof
//! machine (Joel 2026-07-11): a one-shot model fails project-level repair; the
//! team/loop with hands, recovery, and the repo's own tests as the
//! specification is what wins — and every graded attempt feeds the trigger
//! buffers as curriculum. One library, two projections.
//!
//! Anti-cheat by construction: the `dod_shell` restores the CANONICAL test
//! files from git before running `cargo test`, so editing the tests can never
//! game the grade. Task checkouts are `git worktree`s of the mined repo —
//! shared object store, the same disk economy as citizen layers.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use ts_rs::TS;

use crate::sdk_codegen::{ActionCommand, CommandError, Ctx};

/// Max total changed lines for a candidate commit — bigger diffs are refactors,
/// not localized bugfixes; the task difficulty should come from DIAGNOSIS, not
/// from re-typing a rewrite. (Tunable per the junction doctrine when evidence
/// arrives; anchored small for outlier A.)
const MAX_DIFF_LINES: usize = 150;

/// How a commit's changed files split for candidacy.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct CommitShape {
    /// Non-test `.rs` source files changed.
    pub source_files: Vec<String>,
    /// Test-bearing files changed (under `tests/`, or named `*_test(s).rs`).
    pub test_files: Vec<String>,
    /// Any non-Rust or config files changed (present → skip; keeps tasks pure).
    pub other_files: Vec<String>,
}

/// Classify a commit's changed file list. Pure — unit-tested without git.
pub(crate) fn classify_files(files: &[&str]) -> CommitShape {
    let mut shape = CommitShape::default();
    for f in files {
        let is_rs = f.ends_with(".rs");
        let is_testy = f.contains("tests/")
            || f.ends_with("_test.rs")
            || f.ends_with("_tests.rs");
        if is_rs && is_testy {
            shape.test_files.push(f.to_string());
        } else if is_rs {
            shape.source_files.push(f.to_string());
        } else {
            shape.other_files.push(f.to_string());
        }
    }
    shape
}

/// Is this commit shape a bugfix-revert candidate? Exactly ONE source file
/// (localized fix) and the same commit touches tests (the fix came with its
/// specification) — or the single source file's diff itself adds `#[test]`
/// (inline test module, the common small-crate style).
pub(crate) fn is_candidate(shape: &CommitShape, diff_adds_inline_test: bool) -> bool {
    shape.other_files.is_empty()
        && shape.source_files.len() == 1
        && (!shape.test_files.is_empty() || diff_adds_inline_test)
}

/// Run a git command in `repo`, capturing stdout. Fail loud with the stderr.
fn git(repo: &Path, args: &[&str]) -> Result<String, CommandError> {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()
        .map_err(|e| CommandError::Internal(format!("git spawn failed: {e}")))?;
    if !out.status.success() {
        return Err(CommandError::Internal(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).into_owned())
}

/// Run `cargo test --quiet` in `dir`; Ok(true) = suite green. Output tail kept
/// for the miner's verification record.
fn cargo_test(dir: &Path) -> (bool, String) {
    match std::process::Command::new("cargo")
        .arg("test")
        .arg("--quiet")
        .current_dir(dir)
        .env("CARGO_TARGET_DIR", shared_target_dir())
        .output()
    {
        Ok(o) => {
            let mut s = String::from_utf8_lossy(&o.stdout).into_owned();
            s.push_str(&String::from_utf8_lossy(&o.stderr));
            let tail: String = {
                let n = s.chars().count();
                if n > 1200 {
                    format!("…{}", s.chars().skip(n - 1200).collect::<String>())
                } else {
                    s
                }
            };
            (o.status.success(), tail)
        }
        Err(e) => (false, format!("cargo spawn failed: {e}")),
    }
}

/// The machine's ONE cargo cache — mined-task builds must never grow a
/// per-checkout target/ (the citizen-layer economy applies to gym tasks too).
fn shared_target_dir() -> String {
    std::env::var("CARGO_TARGET_DIR").unwrap_or_else(|_| {
        dirs::home_dir()
            .map(|h| h.join(".continuum/cache/cargo-target").display().to_string())
            .unwrap_or_else(|| "target".to_string())
    })
}

/// One mined, DOUBLY-VERIFIED task, as emitted (a superset of the EvalTask
/// wire fields plus mining provenance for the board/replication doc).
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/gym/MinedTask.ts")]
pub struct MinedTask {
    pub id: String,
    pub prompt: String,
    pub dod_shell: String,
    pub setup_shell: String,
    /// The checkout THIS task lives in — the directory the persona's hands must be rooted at.
    /// Every mined task has its OWN worktree, so the root is per-TASK, never per-run: emitting
    /// it as a FIELD (not just prose inside `prompt`) is what lets the evaluator re-root her
    /// file engine before the task instead of leaving her sandboxed somewhere else, reading a
    /// path she cannot reach. A miner that only narrates the path produces tasks nothing can run.
    pub workspace_root: String,
    /// Provenance: the fixing commit this task was mined from.
    pub commit: String,
    /// The source file the persona must repair.
    pub source_file: String,
    /// Tail of the failing `cargo test` output in the broken state — proof the
    /// task is real, and the examiner's honest hint surface.
    pub failing_output: String,
}

// ─────────────────────────── gym/mine ───────────────────────────

/// Params for `gym/mine`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/gym/GymMineParams.ts")]
pub struct GymMineParams {
    /// Local path to the crate's git clone (the operator/persona clones; the
    /// miner stays network-free and testable).
    pub repo_path: String,
    /// Max verified tasks to emit.
    #[ts(optional, type = "number")]
    #[serde(default)]
    pub limit: Option<u32>,
    /// Where the task JSONL lands. Default: `<repo>-gym/tasks.jsonl`.
    #[ts(optional)]
    #[serde(default)]
    pub out_path: Option<String>,
}

/// Result of `gym/mine`.
#[derive(Debug, Clone, Serialize, Deserialize, TS, schemars::JsonSchema)]
#[serde(rename_all = "camelCase")]
#[ts(export, export_to = "../../../protocol/typescript/gym/GymMineResult.ts")]
pub struct GymMineResult {
    /// Verified tasks emitted.
    #[ts(type = "number")]
    pub mined: u32,
    /// Commits examined.
    #[ts(type = "number")]
    pub commits_scanned: u32,
    /// Candidates that failed verification (broken-didn't-fail or
    /// fixed-didn't-pass) — honest yield accounting, never silently dropped.
    #[ts(type = "number")]
    pub rejected: u32,
    /// The emitted JSONL path.
    pub out_path: String,
    /// Task checkout worktrees root.
    pub tasks_dir: String,
}

/// `gym/mine` — mine bugfix-revert tasks from a crate's git history.
#[derive(Default)]
pub struct GymMine;

#[async_trait::async_trait]
impl ActionCommand for GymMine {
    const NAME: &'static str = "gym/mine";
    const ACCESS: crate::sdk_codegen::AccessLevel = crate::sdk_codegen::AccessLevel::Privileged;
    const DESCRIPTION: &'static str =
        "Mine real-repo bugfix tasks from a crate's git history (bugfix-revert): find \
         commits that fix code and touch tests together, re-create the broken state in a \
         git worktree, verify it fails then passes, and emit EvalTask JSONL with dod_shell \
         (anti-cheat: canonical tests restored before grading) and setup_shell (re-break \
         for repeatable runs). Long-running; run against a LOCAL clone.";
    type Params = GymMineParams;
    type Output = GymMineResult;

    async fn run(&self, _ctx: &Ctx, p: GymMineParams) -> Result<GymMineResult, CommandError> {
        let repo = PathBuf::from(&p.repo_path);
        if !repo.join(".git").exists() {
            return Err(CommandError::Invalid(format!(
                "gym/mine: {} is not a git clone (no .git). Clone the crate locally first.",
                repo.display()
            )));
        }
        let limit = p.limit.unwrap_or(10) as usize;
        let tasks_dir = repo
            .parent()
            .unwrap_or(Path::new("."))
            .join(format!(
                "{}-gym",
                repo.file_name().map(|s| s.to_string_lossy()).unwrap_or_default()
            ));
        std::fs::create_dir_all(&tasks_dir)
            .map_err(|e| CommandError::Internal(format!("tasks dir: {e}")))?;
        let out_path = p
            .out_path
            .map(PathBuf::from)
            .unwrap_or_else(|| tasks_dir.join("tasks.jsonl"));

        // The mining walk is blocking (git + cargo shell-outs, potentially
        // minutes) — off the async worker, one spawn_blocking for the batch.
        let mined = tokio::task::spawn_blocking(move || {
            mine(&repo, &tasks_dir, limit)
        })
        .await
        .map_err(|e| CommandError::Internal(format!("mining task panicked: {e}")))??;

        let mut lines = String::new();
        for t in &mined.tasks {
            lines.push_str(&serde_json::to_string(t).map_err(|e| {
                CommandError::Internal(format!("task serialize: {e}"))
            })?);
            lines.push('\n');
        }
        std::fs::write(&out_path, lines)
            .map_err(|e| CommandError::Internal(format!("write {}: {e}", out_path.display())))?;

        crate::probe!(
            class = "gym.mine",
            repo = %p.repo_path,
            mined = mined.tasks.len(),
            scanned = mined.scanned,
            rejected = mined.rejected,
            out = %out_path.display(),
            "bugfix-revert mining complete"
        );
        Ok(GymMineResult {
            mined: mined.tasks.len() as u32,
            commits_scanned: mined.scanned as u32,
            rejected: mined.rejected as u32,
            out_path: out_path.display().to_string(),
            tasks_dir: mined.tasks_dir.display().to_string(),
        })
    }
}

crate::register_stateless_command!(GymMine);

struct MineOutcome {
    tasks: Vec<MinedTask>,
    scanned: usize,
    rejected: usize,
    tasks_dir: PathBuf,
}

/// The blocking mining walk. Newest-first over non-merge commits touching `.rs`.
fn mine(repo: &Path, tasks_dir: &Path, limit: usize) -> Result<MineOutcome, CommandError> {
    let log = git(repo, &["log", "--no-merges", "--pretty=%H", "--", "*.rs"])?;
    let mut tasks = Vec::new();
    let mut scanned = 0usize;
    let mut rejected = 0usize;

    for commit in log.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if tasks.len() >= limit {
            break;
        }
        scanned += 1;

        // Shape gate: file list + diff stats, cheap before any checkout.
        let files_raw = git(repo, &["show", "--name-only", "--pretty=format:", commit])?;
        let files: Vec<&str> = files_raw.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
        let shape = classify_files(&files);
        let diff = git(repo, &["show", "--pretty=format:", commit])?;
        let added_lines = diff.lines().filter(|l| l.starts_with('+')).count();
        let removed_lines = diff.lines().filter(|l| l.starts_with('-')).count();
        let adds_inline_test = diff
            .lines()
            .any(|l| l.starts_with('+') && l.contains("#[test]"));
        if !is_candidate(&shape, adds_inline_test)
            || added_lines + removed_lines > MAX_DIFF_LINES
        {
            continue;
        }
        // Root-commit guard: a first commit has no parent to revert to.
        if git(repo, &["rev-parse", &format!("{commit}^")]).is_err() {
            continue;
        }
        let source_file = shape.source_files[0].clone();
        let short = &commit[..10.min(commit.len())];
        let task_dir = tasks_dir.join(format!("task_{short}"));

        // Materialize: worktree at the FIX commit (tests present), then revert
        // the source file to the parent (broken). Worktrees share the object
        // store — N tasks ≈ working files only.
        let _ = git(repo, &["worktree", "remove", "--force", &task_dir.display().to_string()]);
        if git(repo, &["worktree", "add", "--detach", &task_dir.display().to_string(), commit]).is_err() {
            rejected += 1;
            continue;
        }
        let break_cmd = format!(
            "git -C {d} checkout {c}^ -- {f}",
            d = task_dir.display(),
            c = commit,
            f = source_file
        );
        let fix_cmd = format!(
            "git -C {d} checkout {c} -- {f}",
            d = task_dir.display(),
            c = commit,
            f = source_file
        );
        let run_sh = |cmd: &str| {
            std::process::Command::new("bash")
                .arg("-lc")
                .arg(cmd)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false)
        };

        // DOUBLE verification: broken must FAIL, fixed must PASS — a task that
        // can't prove both is rejected and counted, never emitted.
        if !run_sh(&break_cmd) {
            rejected += 1;
            continue;
        }
        let (broken_green, failing_output) = cargo_test(&task_dir);
        if broken_green {
            // Reverting the "fix" didn't break the suite → not a real bugfix
            // (or tests don't cover it). Not a task.
            rejected += 1;
            let _ = git(repo, &["worktree", "remove", "--force", &task_dir.display().to_string()]);
            continue;
        }
        if !run_sh(&fix_cmd) {
            rejected += 1;
            continue;
        }
        let (fixed_green, _) = cargo_test(&task_dir);
        if !fixed_green {
            rejected += 1;
            let _ = git(repo, &["worktree", "remove", "--force", &task_dir.display().to_string()]);
            continue;
        }
        // Leave the checkout BROKEN — that's the exam's starting state.
        let _ = run_sh(&break_cmd);

        // Anti-cheat DoD: restore canonical tests from the fix commit before
        // grading, so editing tests can never game the pass.
        let restore_tests = if shape.test_files.is_empty() {
            String::new()
        } else {
            format!(
                "git -C {d} checkout {c} -- {tests} && ",
                d = task_dir.display(),
                c = commit,
                tests = shape.test_files.join(" ")
            )
        };
        tasks.push(MinedTask {
            id: format!("mine_{short}"),
            workspace_root: task_dir.display().to_string(),
            // Addressed from HER frame: the checkout IS her workspace root, so the file is at a
            // relative path her tools accept. The old wording named an absolute path outside the
            // sandbox — correct as narration, unusable as an instruction.
            prompt: format!(
                "Real bug, real repo: YOUR WORKSPACE IS this crate's checkout, and `cargo test` \
                 in it currently FAILS. Diagnose and fix the bug — `{f}` (relative to your \
                 workspace root) contains it, and the test suite is the specification. Read the \
                 failing output, inspect the code, edit the file, and re-run the tests until they \
                 pass. Do not modify the tests.",
                f = source_file
            ),
            dod_shell: format!(
                "{restore_tests}cd {d} && CARGO_TARGET_DIR={t} cargo test --quiet",
                d = task_dir.display(),
                t = shared_target_dir()
            ),
            setup_shell: break_cmd,
            commit: commit.to_string(),
            source_file,
            failing_output,
        });
    }
    Ok(MineOutcome {
        tasks,
        scanned,
        rejected,
        tasks_dir: tasks_dir.to_path_buf(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the candidacy geometry — exactly one source file plus
    // test evidence (separate test files OR inline #[test] additions) qualifies;
    // refactors (many sources), config-touching commits, and testless fixes are
    // excluded. Pure logic, no git.
    #[test]
    fn commit_candidacy_is_one_source_plus_test_evidence() {
        let c = classify_files(&["src/lib.rs", "tests/basic.rs"]);
        assert!(is_candidate(&c, false), "1 source + test file: candidate");

        let c = classify_files(&["src/lib.rs"]);
        assert!(!is_candidate(&c, false), "no test evidence: not a candidate");
        assert!(is_candidate(&c, true), "inline #[test] added: candidate");

        let c = classify_files(&["src/lib.rs", "src/parser.rs", "tests/basic.rs"]);
        assert!(!is_candidate(&c, false), "two sources = refactor, not a localized fix");

        let c = classify_files(&["src/lib.rs", "Cargo.toml", "tests/basic.rs"]);
        assert!(!is_candidate(&c, false), "config churn excluded — tasks stay pure");

        let c = classify_files(&["src/util_tests.rs", "src/lib.rs"]);
        assert!(is_candidate(&c, false), "_tests.rs classifies as test evidence");
    }

    // what this catches: the "superset of the EvalTask wire fields" claim on the
    // ACTUAL wire — MinedTask serializes camelCase (`dodShell`) while EvalTask's
    // fields are snake_case with every field defaulted, so without serde aliases
    // the mined JSONL silently degrades to an empty substring-graded task (grade
    // rot, not an error). Regression for the tasks.jsonl → cognition/eval seam.
    #[test]
    fn mined_task_jsonl_deserializes_as_eval_task() {
        let mined = MinedTask {
            workspace_root: "/tmp/gym/task_abc".into(),
            id: "mine_abc".into(),
            prompt: "fix the bug".into(),
            dod_shell: "cargo test".into(),
            setup_shell: "git checkout HEAD^ -- src/lib.rs".into(),
            commit: "abc".into(),
            source_file: "src/lib.rs".into(),
            failing_output: "1 test failed".into(),
        };
        let wire = serde_json::to_string(&mined).unwrap();
        assert!(wire.contains("dodShell"), "MinedTask wire stays camelCase");
        let task: crate::cognition::eval::EvalTask = serde_json::from_str(&wire).unwrap();
        assert_eq!(task.dod_shell.as_deref(), Some("cargo test"));
        assert_eq!(
            task.setup_shell.as_deref(),
            Some("git checkout HEAD^ -- src/lib.rs")
        );
        assert_eq!(task.id, "mine_abc");
    }

    // what this catches: the END-TO-END mining contract against a real (synthetic)
    // git repo — bug commit, then fix+test commit; the miner must emit exactly one
    // doubly-verified task whose setup re-breaks and whose DoD restores canonical
    // tests. Gated stress-tests: it shells to git+cargo and compiles a crate
    // (seconds), which default `cargo test` must not pay.
    #[cfg(feature = "stress-tests")]
    #[test]
    fn mines_a_doubly_verified_task_from_a_synthetic_repo() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path().join("mini");
        std::fs::create_dir_all(repo.join("src")).unwrap();
        std::fs::create_dir_all(repo.join("tests")).unwrap();
        let sh = |cmd: &str| {
            assert!(
                std::process::Command::new("bash")
                    .arg("-lc")
                    .arg(cmd)
                    .current_dir(&repo)
                    .status()
                    .unwrap()
                    .success(),
                "shell step failed: {cmd}"
            );
        };
        std::fs::write(
            repo.join("Cargo.toml"),
            "[package]\nname = \"mini\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .unwrap();
        // Commit 1: the bug (add returns a-b).
        std::fs::write(repo.join("src/lib.rs"), "pub fn add(a: i32, b: i32) -> i32 { a - b }\n").unwrap();
        sh("git init -q && git add -A && git -c user.email=t@t -c user.name=t commit -qm bug");
        // Commit 2: the fix + the test that specifies it.
        std::fs::write(repo.join("src/lib.rs"), "pub fn add(a: i32, b: i32) -> i32 { a + b }\n").unwrap();
        std::fs::write(
            repo.join("tests/add.rs"),
            "#[test]\nfn adds() { assert_eq!(mini::add(2, 3), 5); }\n",
        )
        .unwrap();
        sh("git add -A && git -c user.email=t@t -c user.name=t commit -qm 'fix add + test'");

        let tasks_dir = dir.path().join("mini-gym");
        let out = mine(&repo, &tasks_dir, 5).expect("mine");
        assert_eq!(out.tasks.len(), 1, "exactly the fix commit qualifies");
        let t = &out.tasks[0];
        assert_eq!(t.source_file, "src/lib.rs");
        assert!(t.dod_shell.contains("cargo test"), "graded by the suite");
        assert!(
            t.dod_shell.contains("checkout") && t.dod_shell.contains("tests/add.rs"),
            "anti-cheat: canonical tests restored before grading: {}",
            t.dod_shell
        );
        assert!(t.setup_shell.contains("^ --"), "setup re-breaks the source");
        assert!(
            t.failing_output.contains("adds") || !t.failing_output.is_empty(),
            "the broken state's failing output is recorded as proof"
        );
        // The checkout is left BROKEN (the exam's starting state).
        let src = std::fs::read_to_string(out.tasks_dir.join(format!("task_{}", &t.commit[..10])).join("src/lib.rs")).unwrap();
        assert!(src.contains("a - b"), "task dir starts broken");
    }
}
