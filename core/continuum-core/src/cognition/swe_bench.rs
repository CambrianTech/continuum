//! SWE-bench: the repo-test grader, in Rust.
//!
//! A SWE-bench instance is a real GitHub issue plus the repo at the commit before its fix. A
//! solution is a patch. The protocol is fixed and small:
//!
//! ```text
//! clone @ base_commit -> apply model_patch -> apply test_patch -> run tests
//! RESOLVED  iff  every FAIL_TO_PASS passes AND every PASS_TO_PASS passes
//! ```
//!
//! **Why this is Rust and not a script.** Benchmarks are the curriculum spine — they feed
//! training signal, eval integrity, and the paper claim — so every benchmark operation is a
//! command with a handle and events, exactly like a model download
//! ([[benchmark-infra-is-substrate-commands-handles-events-never-bash]]). The Python harness
//! this replaces cost, in one morning: a zsh word-splitting bug that silently ran the whole
//! instance list as a single bogus id, a 60-minute blind poll loop whose output was buffered
//! so a dead dispatch looked identical to a working one, and no liveness signal at all. None
//! of those are Python's fault in principle and all of them are what hand-rolled harnesses
//! actually do in practice.
//!
//! **The one thing that stays Python is the SUBJECT.** flask's and sympy's test suites are
//! pytest; running them is what the benchmark IS, no more a dependency of ours than `rustc`
//! is for `humaneval-rs`. We never write Python — we invoke the artifact's own tests and read
//! the verdicts. `uv` (itself a Rust binary) builds the per-instance environment.
//!
//! Three hazards are load-bearing here, each learned the expensive way today:
//!
//! 1. **The gate.** FAIL_TO_PASS must FAIL on the pristine tree. Eight runs were scored
//!    against a clone left at HEAD where the fix already existed, so 0 and 1 meant the same
//!    thing ([[the-swe-workspace-was-never-at-the-base-commit-the-bug-was-not-there]]).
//! 2. **Id shape.** sympy's FAIL_TO_PASS entries are bare function names, not pytest node
//!    ids. Feeding those to pytest as paths yields "file or directory not found", which scores
//!    as a failure and mis-scored GOLD on three instances whose environments were fine.
//! 3. **Era.** Both the dependency graph AND the interpreter have one. 2014 code does
//!    `from collections import Mapping`, deleted in 3.10; no pin rescues that.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// One instance — the fields the protocol actually needs.
///
/// **Deliberately provenance-agnostic.** These fields describe a repo, a commit, a fix, and the
/// tests that define the bug — none of that is specific to a HuggingFace row. A teacher
/// synthesizing work (revert a known-good commit in a real repo, keep its tests as the
/// FAIL_TO_PASS set — that is `gym/mine`, #133) produces the SAME type, and `grade()` scores it
/// with zero new code.
///
/// That matters because benchmarks are becoming curriculum, not just measurement: the corpus
/// compounds as instances accumulate, and a teacher extends it via simulated work. The
/// expensive mistake would be a second, divergent "generated instance" struct — so `load_dataset`
/// is the only HF-shaped thing here, and everything downstream takes `&SweInstance`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SweInstance {
    pub instance_id: String,
    pub repo: String,
    pub base_commit: String,
    /// The gold patch. The spine check grades THIS; it must resolve or the environment is wrong.
    pub patch: String,
    /// The tests that define the bug. Applied to the tree after the candidate patch.
    pub test_patch: String,
    pub problem_statement: String,
    #[serde(default)]
    pub created_at: String,
    #[serde(rename = "FAIL_TO_PASS", default)]
    pub fail_to_pass: String,
    #[serde(rename = "PASS_TO_PASS", default)]
    pub pass_to_pass: String,
}

impl SweInstance {
    /// Test-id lists arrive JSON-encoded inside a string field.
    pub fn f2p(&self) -> Vec<String> {
        serde_json::from_str(&self.fail_to_pass).unwrap_or_default()
    }
    pub fn p2p(&self) -> Vec<String> {
        serde_json::from_str(&self.pass_to_pass).unwrap_or_default()
    }
    /// The era to resolve dependencies and the interpreter against.
    pub fn year(&self) -> u32 {
        self.created_at
            .get(..4)
            .and_then(|y| y.parse().ok())
            .unwrap_or(2023)
    }
}

/// What a graded run concluded, with enough detail that a zero is never ambiguous.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SweVerdict {
    pub instance_id: String,
    pub resolved: bool,
    pub f2p_passed: usize,
    pub f2p_total: usize,
    pub p2p_passed: usize,
    pub p2p_total: usize,
    /// False when FAIL_TO_PASS already passed on the pristine tree — the task carries no bug
    /// here, so no score from it distinguishes a fix from a no-op.
    pub gate_ok: bool,
    /// Set when the run could not produce a verdict at all (clone, patch, or env failure).
    /// A verdict with `error` set is NOT a zero — it is an absence, and must never be
    /// tallied as a failed attempt.
    pub error: Option<String>,
    /// The NAMES of the tests that failed, sorted, so a verdict can teach.
    /// "PASS_TO_PASS 6/11" is a count with nothing to act on; "your patch broke
    /// test_arguments and test_unit" is what a human reviewer would say (Joel,
    /// 2026-08-08: "you or any human could tell him what's wrong — or the grader").
    /// This is what the experience stream and the room verdict carry forward.
    #[serde(default)]
    pub failed_tests: Vec<String>,
    /// The failing FAIL_TO_PASS run's OUTPUT tail (capped) — the assertion diff a
    /// human reviewer would paste. Glass-boxed on atlas-sympy-24066-n4 (2026-08-08):
    /// she synthesized ~90% of the gold patch and missed on one predicate
    /// (`== Dimension(1)` vs `is_dimensionless`); the retry verdict named the
    /// failing TEST but not what it PRINTED — the leftover
    /// `Dimension(impedance*capacitance/time)` in the assertion output is the fact
    /// that teaches equality isn't enough. Format-agnostic (a report TAIL, not a
    /// parsed section) so sympy's own runner and pytest both carry it.
    #[serde(default)]
    pub failure_excerpt: Option<String>,
}

/// Where a detached benchmark run journals its state. One file per run, rewritten in place:
/// `state: "running"` at dispatch, then the verdict — or a killed-by-reboot marker.
pub fn solve_ledger_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".continuum").join("progress")
}

/// Runs this ledger dir believes are IN FLIGHT — written at dispatch, not yet resolved.
///
/// Two callers, one truth: the reboot guard asks before killing the core, and the boot reaper
/// asks after. Returning `(run_id, instance)` rather than a bool is what lets the guard NAME
/// what it would destroy, which is the difference between a policy and a nag.
pub fn in_flight_solve_runs_in(dir: &Path) -> Vec<(String, String)> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut live = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        if !name.starts_with("swe-solve-") || !name.ends_with(".json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else { continue };
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else { continue };
        if v.get("state").and_then(|s| s.as_str()) != Some("running") {
            continue;
        }
        let run_id = name
            .trim_start_matches("swe-solve-")
            .trim_end_matches(".json")
            .to_string();
        let instance = v
            .get("instance")
            .and_then(|i| i.as_str())
            .unwrap_or("unknown")
            .to_string();
        live.push((run_id, instance));
    }
    live.sort();
    live
}

/// Convenience over the real ledger dir.
pub fn in_flight_solve_runs() -> Vec<(String, String)> {
    in_flight_solve_runs_in(&solve_ledger_dir())
}

/// At boot, any run still marked `running` was owned by a core that no longer exists — a
/// reboot, a crash, a SIGKILL. Rewrite it as a FAILED run naming the cause.
///
/// Glass-boxed the day `swe-solve` shipped: two reboots silently killed a detached run, and
/// its ledger simply never appeared. A poller cannot distinguish "still working" from "died an
/// hour ago" when the evidence for both is an absent file — the same shape as #137's 41 train
/// jobs submitted with zero outcomes recorded. The marker at dispatch is what makes the death
/// observable; this reap is what makes it honest.
pub fn reap_orphaned_solve_runs_in(dir: &Path) -> Vec<String> {
    let mut reaped = Vec::new();
    for (run_id, instance) in in_flight_solve_runs_in(dir) {
        let path = dir.join(format!("swe-solve-{run_id}.json"));
        let marker = serde_json::json!({
            "failed": true,
            "runId": run_id,
            "instance": instance,
            "error": "killed by a core restart — the run was in flight when the core that owned \
                      it went away. Nothing was scored; re-dispatch to measure this instance.",
        });
        if std::fs::write(&path, marker.to_string()).is_ok() {
            reaped.push(run_id);
        }
    }
    reaped
}

/// Convenience over the real ledger dir.
pub fn reap_orphaned_solve_runs() -> Vec<String> {
    reap_orphaned_solve_runs_in(&solve_ledger_dir())
}

/// Where cached datasets and per-instance environments live. A governed cache class, not a
/// scratch dir — see the disk-eviction contract.
pub fn swe_cache_dir() -> PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".into());
    PathBuf::from(home).join(".continuum").join("benchmarks").join("swe")
}

/// Fetch a dataset split, cached on first use. On-demand, never a gated install step.
pub async fn load_dataset(dataset: &str) -> Result<Vec<SweInstance>, String> {
    let cache = swe_cache_dir().join(format!("{}.json", dataset.replace('/', "__")));
    if let Ok(bytes) = std::fs::read(&cache) {
        if let Ok(rows) = serde_json::from_slice::<Vec<SweInstance>>(&bytes) {
            if !rows.is_empty() {
                return Ok(rows);
            }
        }
    }
    let mut rows: Vec<SweInstance> = Vec::new();
    // The datasets-server caps a page at 100; SWE-bench Lite is 300.
    for offset in (0..2000).step_by(100) {
        let url = format!(
            "https://datasets-server.huggingface.co/rows?dataset={}&config=default&split=test&offset={}&length=100",
            urlencoding_encode(dataset),
            offset
        );
        let resp = reqwest::get(&url)
            .await
            .map_err(|e| format!("dataset fetch failed at offset {offset}: {e}"))?;
        let body: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| format!("dataset decode failed at offset {offset}: {e}"))?;
        let page = body.get("rows").and_then(|r| r.as_array()).cloned().unwrap_or_default();
        if page.is_empty() {
            break;
        }
        for entry in page {
            if let Some(row) = entry.get("row") {
                if let Ok(inst) = serde_json::from_value::<SweInstance>(row.clone()) {
                    rows.push(inst);
                }
            }
        }
    }
    if rows.is_empty() {
        return Err(format!("{dataset} returned no usable rows"));
    }
    let _ = std::fs::create_dir_all(swe_cache_dir());
    let _ = std::fs::write(&cache, serde_json::to_vec(&rows).unwrap_or_default());
    Ok(rows)
}

/// Minimal percent-encoding for the one query param we send (the dataset name's `/`).
fn urlencoding_encode(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'A'..='Z' | 'a'..='z' | '0'..='9' | '-' | '_' | '.' | '~' => c.to_string(),
            other => format!("%{:02X}", other as u32),
        })
        .collect()
}

/// Ceiling on any single grader subprocess. Nothing legitimate here — a git
/// mirror fetch, a uv venv build, one instance's full pytest files — takes
/// this long; the class this kills is INFINITE (task #381: sympy
/// symbolic-computation hangs orphaned to launchd at 100% CPU for ELEVEN
/// HOURS, detected by the operator's cooling fan, not by any instrument).
const SUBPROCESS_CEILING: std::time::Duration = std::time::Duration::from_secs(15 * 60);

pub(crate) async fn run(program: &str, args: &[&str], cwd: Option<&Path>) -> Result<std::process::Output, String> {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    // #381: the child leads its own PROCESS GROUP so a kill reaches the whole
    // tree — pytest spawns grandchildren (sympy's own runner, plugins) that
    // survive a parent-only kill and orphan to launchd. kill_on_drop covers
    // the drop path (a core reboot cancels this future mid-await); the
    // explicit killpg below covers the grandchildren the drop-kill misses.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(cmd.as_std_mut(), 0);
    cmd.kill_on_drop(true);
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());
    let child = cmd
        .spawn()
        .map_err(|e| format!("could not run `{program}`: {e}"))?;
    let child_pid = child.id();
    match tokio::time::timeout(SUBPROCESS_CEILING, child.wait_with_output()).await {
        Ok(out) => out.map_err(|e| format!("could not run `{program}`: {e}")),
        Err(_elapsed) => {
            // The timed-out future was just dropped, killing the DIRECT child;
            // now kill its group (pgid == child pid, it was made group leader
            // at spawn) so grandchildren die with it. Windows has no process
            // groups in this shape — kill_on_drop alone is the story there.
            #[cfg(unix)]
            if let Some(pid) = child_pid {
                unsafe {
                    libc::killpg(pid as i32, libc::SIGKILL);
                }
            }
            crate::probe!(
                class = "benchmark.subprocess.ceiling",
                program = %program,
                ceiling_s = SUBPROCESS_CEILING.as_secs(),
                "grader subprocess exceeded the ceiling — whole process group killed \
                 (a hung test run is an environment fault, never a verdict)"
            );
            Err(format!(
                "`{program}` exceeded the {}s subprocess ceiling and was killed \
                 (whole process group) — a hung run is an environment fault, \
                 never a verdict (task #381)",
                SUBPROCESS_CEILING.as_secs()
            ))
        }
    }
}

/// Clone the repo at `base_commit`. The commit is the whole point — a clone left at HEAD is
/// how eight runs got scored against a tree with the fix already in it.
pub async fn clone_at(instance: &SweInstance, repo_dir: &Path) -> Result<(), String> {
    if repo_dir.exists() {
        let _ = std::fs::remove_dir_all(repo_dir);
    }
    // The PARENT must exist first. `git clone` creates its target directory but not the chain
    // above it, and the failure surfaces late and cryptically — as a mid-fetch "unable to write
    // .git/objects/pack/*.pack: No such file or directory", which reads like a disk or network
    // fault rather than a missing mkdir.
    if let Some(parent) = repo_dir.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("could not create {}: {e}", parent.display()))?;
    }
    // ONE network fetch per REPO, not per clone.
    //
    // The protocol needs two trees per instance — the one she works in and a pristine one to
    // score in — and a sweep re-grades the same repo across many instances. Cloning from
    // GitHub each time was ~240 MB of network AND disk per tree: a 300-instance Lite sweep
    // would have been ~140 GB and hours of fetch. A local mirror plus `--shared` clones makes
    // every tree after the first nearly free (objects are borrowed, not copied) and costs one
    // fetch per repo for the whole sweep.
    //
    // `--shared` is safe precisely because these trees are DISPOSABLE: nothing here is ever
    // pushed, and the mirror is only ever fast-forwarded. The usual caveat — pruning the parent
    // can corrupt a borrower — does not apply to a cache we recreate from scratch.
    let mirror = swe_cache_dir()
        .join("mirrors")
        .join(instance.repo.replace('/', "__"));
    let url = format!("https://github.com/{}.git", instance.repo);
    if !mirror.exists() {
        if let Some(parent) = mirror.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let out = run(
            "git",
            &["clone", "--quiet", "--bare", &url, &mirror.to_string_lossy()],
            None,
        )
        .await?;
        if !out.status.success() {
            return Err(format!(
                "mirror clone of {} failed: {}",
                instance.repo,
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
    }

    let out = run(
        "git",
        &[
            "clone",
            "--quiet",
            "--shared",
            &mirror.to_string_lossy(),
            &repo_dir.to_string_lossy(),
        ],
        None,
    )
    .await?;
    if !out.status.success() {
        return Err(format!(
            "clone of {} from its local mirror failed: {}",
            instance.repo,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    let out = run("git", &["checkout", "--quiet", &instance.base_commit], Some(repo_dir)).await?;
    if !out.status.success() {
        // A mirror created earlier can predate this instance's base_commit. Refresh it once and
        // retry rather than failing — the alternative is a cache that silently rots into
        // "instance not gradeable" as the dataset grows.
        let _ = run("git", &["fetch", "--quiet", "--all"], Some(&mirror)).await;
        let _ = run("git", &["fetch", "--quiet", "origin"], Some(repo_dir)).await;
        let retry =
            run("git", &["checkout", "--quiet", &instance.base_commit], Some(repo_dir)).await?;
        if !retry.status.success() {
            return Err(format!(
                "checkout {} failed even after refreshing the local mirror: {}",
                &instance.base_commit[..12.min(instance.base_commit.len())],
                String::from_utf8_lossy(&retry.stderr).trim()
            ));
        }
    }
    Ok(())
}

/// Apply a patch, tolerating the whitespace drift that trips a strict apply.
pub async fn apply_patch(repo_dir: &Path, text: &str, what: &str) -> Result<(), String> {
    if text.trim().is_empty() {
        return Ok(());
    }
    let path = repo_dir.join(format!(".{what}.patch"));
    std::fs::write(&path, text).map_err(|e| format!("could not stage {what} patch: {e}"))?;
    let p = path.to_string_lossy().to_string();
    for extra in [vec![], vec!["--ignore-whitespace"], vec!["--ignore-whitespace", "-C1"]] {
        let mut args = vec!["apply"];
        args.extend(extra);
        args.push(&p);
        if let Ok(out) = run("git", &args, Some(repo_dir)).await {
            if out.status.success() {
                let _ = std::fs::remove_file(&path);
                return Ok(());
            }
        }
    }
    Err(format!("could not apply {what} patch — the tree is not what the patch expects"))
}

/// The interpreter an instance's own code could actually have run on.
///
/// 3.9 is the last release carrying the `collections.Mapping` aliases pre-2020 code reaches
/// for; uv's own floor is 3.8, so there is no lower rung to offer. Coarse on purpose — the
/// gold gate is the arbiter, and a wrong guess fails loudly rather than producing a
/// plausible number.
///
/// The 2020..=2022 rung is 3.10, learned live (pylint-5859, 2026-08-11): Python 3.11
/// removed `inspect.formatargspec`, which era sdist BUILDS still import (wrapt 1.13.3,
/// pulled by 2022 astroid) — every 2022-era env with such a dep died at build on 3.11.
/// 3.11 released 2022-10; a March-2022 dependency graph never targeted it. The same
/// mismatch is the leading suspect for the p2p-0/N broken baselines (flask-5063,
/// pytest-5103) whose envs were built on 3.11 before this rung existed.
pub fn interpreter_for_year(year: u32) -> &'static str {
    if year < 2020 {
        "3.9"
    } else if year <= 2022 {
        "3.10"
    } else {
        "3.11"
    }
}

/// Build (or reuse) the per-instance environment. Per-instance rather than per-repo because
/// instances span years of a repo's history and their dependency graphs genuinely differ.
/// The repo's declared build-time dependencies, from `[build-system].requires` in its
/// `pyproject.toml`. Empty when there is no pyproject, no `[build-system]` table, or no
/// `requires` array. These must be pre-installed into the venv when building with
/// `--no-build-isolation` (see the call site) — pip won't fetch them for us in that mode.
fn build_requires(repo_dir: &Path) -> Vec<String> {
    let text = match std::fs::read_to_string(repo_dir.join("pyproject.toml")) {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };
    let parsed: toml::Value = match toml::from_str(&text) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    parsed
        .get("build-system")
        .and_then(|bs| bs.get("requires"))
        .and_then(|r| r.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str())
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default()
}

pub async fn ensure_env(instance: &SweInstance, repo_dir: &Path) -> Result<PathBuf, String> {
    let env_dir = swe_cache_dir().join("envs").join(&instance.instance_id);
    let py = env_dir.join("bin").join("python");
    if py.exists() {
        return Ok(py);
    }
    let _ = std::fs::create_dir_all(env_dir.parent().unwrap_or(&env_dir));
    let uv = which("uv").ok_or_else(|| {
        "uv is not installed — it builds the per-instance environment (a Rust binary, \
         install from https://astral.sh/uv)"
            .to_string()
    })?;
    let interpreter = interpreter_for_year(instance.year());
    let env_s = env_dir.to_string_lossy().to_string();
    let py_s = py.to_string_lossy().to_string();

    let out = run(&uv, &["venv", "--python", interpreter, "-q", &env_s], None).await?;
    if !out.status.success() {
        return Err(format!(
            "could not create a Python {interpreter} venv: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }

    // THE SPLIT: SUBJECT vs HARNESS.
    //
    // Subject — the repo's own runtime dependency graph — is pinned to the instance's date,
    // because those versions define the behavior under test. A plain install resolves 2026
    // packages against 2021 code: flask 2.0 asks for `Werkzeug>=2.0`, gets 3.x, which deleted
    // `url_quote`, and the repo cannot even import.
    //
    // Harness — pytest, setuptools, wheel — is deliberately MODERN, because it has to run on
    // this machine's interpreter. Date-pinning it breaks in ways unrelated to the instance:
    // 2021 `py` raises `AttributeError: __spec__` under 3.11's import machinery, and 2021
    // setuptools predates PEP 660 so `build_meta:__legacy__` has no `build_editable`.
    //
    // `--no-build-isolation` is what lets the two coexist: the build runs against the modern
    // setuptools already in the venv instead of pip fetching a date-pinned one.
    //
    // setuptools is pinned `<70`, NOT bare-latest. Two era-2020..2022 build requirements fight:
    // an editable install (`-e .`) needs PEP 660 `build_editable`, which landed in setuptools 64;
    // and those repos' C-extension setup code imports legacy APIs like `setuptools.dep_util`
    // (astropy's `wcs/setup_package.py`), which setuptools REMOVED in 70.0. setuptools 69.x is the
    // only version that has BOTH — bare-latest (>=70) builds the pure-Python repos but dies on
    // every C-extension instance with `ModuleNotFoundError: setuptools.dep_util`. This is the whole
    // era class, not one repo (#380 "pin era deps"). pytest/wheel stay latest.
    let _ = run(
        &uv,
        &["pip", "install", "-q", "--python", &py_s, "pytest", "setuptools<70", "wheel"],
        None,
    )
    .await?;

    // BUILD REQUIRES: `--no-build-isolation` means pip will NOT fetch the repo's declared
    // build-time dependencies — it builds against whatever is already in the venv. A repo whose
    // setup.py imports a build helper (astropy: `import extension_helpers`; any C-extension repo:
    // cython, numpy) therefore fails with `ModuleNotFoundError` at the metadata step unless we
    // pre-install what its own `[build-system].requires` declares. We honor that declaration
    // rather than hardcoding per-repo build deps — the pyproject IS the source of truth. Installed
    // MODERN (no date-pin) like pytest/setuptools/wheel above: these are build harness, run on
    // THIS interpreter, and carry their own version pins where they matter (e.g. cython==0.29.22).
    let build_reqs = build_requires(repo_dir);
    if !build_reqs.is_empty() {
        let mut breq_args = vec!["pip", "install", "-q", "--python", &py_s];
        breq_args.extend(build_reqs.iter().map(String::as_str));
        let out = run(&uv, &breq_args, None).await?;
        if !out.status.success() {
            // Non-fatal: some declared build deps (e.g. `oldest-supported-numpy` on a fresh
            // interpreter) may not resolve, yet the build can still succeed against the modern
            // setuptools already present. Let the `-e .` step below be the real gate; surface this
            // only as a breadcrumb if that step then fails.
            tracing::warn!(
                instance = %instance.instance_id,
                requires = ?build_reqs,
                stderr = %String::from_utf8_lossy(&out.stderr).trim(),
                "build-system.requires install had non-zero exit — proceeding to -e . anyway"
            );
        }
    }

    let as_of = if instance.created_at.is_empty() {
        None
    } else {
        Some(instance.created_at.clone())
    };
    let repo_s = repo_dir.to_string_lossy().to_string();

    // DELETED-HISTORY packages: `--exclude-newer <date>` assumes PyPI still carries the
    // uploads that existed at that date. It does not always — `atomicwrites` had its whole
    // pre-2022 history deleted by its author, so for any pre-2022 instance the date pin
    // leaves ZERO candidates and resolution fails outright (pytest-dev__pytest-5103, live
    // 2026-08-11). uv's error names both the package and the earliest surviving upload, and
    // its own suggested remedy is a per-package `--exclude-newer-package` cutoff. We heal
    // exactly as instructed: parse the hint, override ONLY that package to just past its
    // earliest surviving upload, and retry. The subject stays date-pinned; the override is
    // the minimum needed for a resolvable graph, discovered from the resolver's own evidence
    // rather than a hand-maintained package list. Bounded: each round must surface a NEW
    // package or we stop, and history-holes per graph are few.
    let mut overrides: Vec<String> = Vec::new();
    let out = loop {
        let mut args = vec!["pip", "install", "-q", "--python", &py_s];
        if let Some(ref date) = as_of {
            args.push("--exclude-newer");
            args.push(date);
        }
        for pin in &overrides {
            args.push("--exclude-newer-package");
            args.push(pin);
        }
        args.extend(["--no-build-isolation", "-e", "."]);
        let out = run(&uv, &args, Some(Path::new(&repo_s))).await?;
        if out.status.success() || as_of.is_none() {
            break out;
        }
        let stderr = String::from_utf8_lossy(&out.stderr).to_string();
        // Two heal arms, same bounded loop: (1) deleted-history — the date pin leaves zero
        // candidates, uv's hint names the earliest surviving upload; (2) metadata-mismatch —
        // an era sdist with no wheel for this platform builds as version 0.0.0
        // (setuptools_scm without git metadata; live 2026-08-11: lazy-object-proxy 1.7.1 has
        // no arm64 wheel, pulled by 2022 pylint→astroid), so the ONE unbuildable package's
        // cutoff is lifted entirely — a modern wheel-shipping release of a shim library, in
        // an otherwise era-pure graph, disclosed on the probe. Both parse uv's OWN evidence;
        // no hand-maintained package list.
        match deleted_history_override(&stderr).or_else(|| metadata_mismatch_override(&stderr)) {
            Some(pin) if !overrides.contains(&pin) && overrides.len() < 8 => {
                tracing::warn!(
                    instance = %instance.instance_id,
                    r#override = %pin,
                    "date-pinned resolution hit an unresolvable era package — retrying with \
                     a per-package cutoff derived from uv's own error"
                );
                overrides.push(pin);
            }
            _ => break out,
        }
    };
    if !out.status.success() {
        // DELETE the half-built env rather than cache it. Keying on "the directory exists"
        // made a failed install sticky: every later run reused a venv with no repo in it and
        // reported a gold failure whose real cause was three steps upstream.
        let _ = std::fs::remove_dir_all(&env_dir);
        return Err(format!(
            "could not install {}'s repo into a venv — a cached broken env would poison every \
             later run, so it was removed: {}",
            instance.instance_id,
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(py)
}

/// Parse uv's deleted-history hint into an `--exclude-newer-package` value (`pkg=cutoff`).
///
/// The hint shape (uv 0.11):
/// ```text
/// hint: `atomicwrites` was filtered by `exclude-newer` to only include packages uploaded
/// before 2019-04-13T16:17:45Z. The latest version satisfying the requirement is v1.4.1,
/// published at 2022-07-08T18:31:40.459Z. Consider using `exclude-newer-package` to
/// override the cutoff for this package.
/// ```
/// The cutoff is that surviving upload's timestamp plus one second — `--exclude-newer-package`
/// is exclusive ("prior to"), so the publish instant itself must land inside the window, and
/// one second past it admits nothing else.
fn deleted_history_override(stderr: &str) -> Option<String> {
    let hint_at = stderr.find("Consider using `exclude-newer-package`")?;
    let region = &stderr[..hint_at];
    let pkg = {
        let rest = &region[region.rfind("hint: `")? + "hint: `".len()..];
        rest.split('`').next()?
    };
    let published = {
        let rest = &region[region.rfind("published at ")? + "published at ".len()..];
        &rest[..=rest.find('Z')?]
    };
    let ts = chrono::DateTime::parse_from_rfc3339(published).ok()?;
    let cutoff =
        (ts + chrono::Duration::seconds(1)).to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    if pkg.is_empty() || pkg.contains(char::is_whitespace) {
        return None;
    }
    Some(format!("{pkg}={cutoff}"))
}

/// Parse uv's metadata-version-mismatch failure into an `--exclude-newer-package` value
/// that LIFTS the era cutoff for the one unbuildable package.
///
/// The failure shape (uv 0.11, live pylint-5859 2026-08-11):
/// ```text
/// ╰─▶ Package metadata version `0.0.0` does not match given version `1.7.1`
/// hint: `lazy-object-proxy` (v1.7.1) was included because `pylint` (v2.13.0.dev0) ...
/// ```
/// The `0.0.0` is an sdist built without git metadata (setuptools_scm fallback) — it happens
/// exactly when the era release ships no wheel for this platform, so no date-pinned retry can
/// ever succeed. Lifting that ONE package's cutoff (far-future bound) lets uv take a modern
/// wheel-shipping release; the rest of the graph stays era-pinned. Verified live: pylint
/// 2.13.0-dev0 + astroid 2.9.3 (era-correct) with only the shim floated.
fn metadata_mismatch_override(stderr: &str) -> Option<String> {
    if !stderr.contains("Package metadata version `0.0.0` does not match given version") {
        return None;
    }
    let rest = &stderr[stderr.find("hint: `")? + "hint: `".len()..];
    let pkg = rest.split('`').next()?;
    if pkg.is_empty() || pkg.contains(char::is_whitespace) {
        return None;
    }
    Some(format!("{pkg}=9999-01-01T00:00:00Z"))
}

fn which(bin: &str) -> Option<String> {
    let path = std::env::var("PATH").ok()?;
    for dir in path.split(':') {
        let candidate = Path::new(dir).join(bin);
        if candidate.is_file() {
            return Some(candidate.to_string_lossy().to_string());
        }
    }
    None
}

/// The test files an instance's own `test_patch` touches — the scope to run.
pub fn patched_test_files(test_patch: &str) -> Vec<String> {
    let mut files: Vec<String> = test_patch
        .lines()
        .filter_map(|l| l.strip_prefix("+++ b/"))
        .map(|p| p.split_whitespace().next().unwrap_or(p).to_string())
        .collect();
    files.sort();
    files.dedup();
    files
}

/// Resolve pytest's `-v` report into a verdict per node id AND per bare function name.
///
/// The dataset does not use one id shape. pytest and flask instances give node ids
/// (`tests/test_x.py::test_y`); sympy gives BARE function names because sympy ships its own
/// runner. Looking up both is what makes one grader serve every repo.
pub fn parse_pytest_report(report: &str) -> (HashMap<String, bool>, HashMap<String, bool>) {
    let mut by_node = HashMap::new();
    let mut by_func: HashMap<String, bool> = HashMap::new();
    for line in report.lines() {
        let line = line.trim();
        let Some((node, rest)) = line.split_once(char::is_whitespace) else {
            continue;
        };
        if !node.contains("::") {
            continue;
        }
        let verdict = rest.trim_start();
        let ok = if verdict.starts_with("PASSED")
            || verdict.starts_with("XFAIL")
            || verdict.starts_with("SKIPPED")
        {
            true
        } else if verdict.starts_with("FAILED")
            || verdict.starts_with("ERROR")
            || verdict.starts_with("XPASS")
        {
            false
        } else {
            continue;
        };
        by_node.insert(node.to_string(), ok);
        let func = node.rsplit("::").next().unwrap_or(node).split('[').next().unwrap_or(node);
        // Same bare name in two files: passing only if every one passed.
        let entry = by_func.entry(func.to_string()).or_insert(true);
        *entry = *entry && ok;
    }
    (by_node, by_func)
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
    let key = id.rsplit("::").next().unwrap_or(id);
    let key = key.rsplit('.').next().unwrap_or(key);
    let key = key.split('[').next().unwrap_or(key);
    by_func.get(key).copied()
}

/// Run the instance's test files once and resolve every required id against that report.
/// A test absent from the report counts as failed — but the absence is knowable, not silent.
pub async fn run_tests(
    repo_dir: &Path,
    venv_py: &Path,
    ids: &[String],
    test_files: &[String],
) -> (HashMap<String, bool>, String) {
    if test_files.is_empty() || ids.is_empty() {
        return (ids.iter().map(|i| (i.clone(), false)).collect(), String::new());
    }
    let mut args: Vec<&str> = vec!["-m", "pytest"];
    for f in test_files {
        args.push(f);
    }
    args.extend(["-v", "--no-header", "-rN", "-p", "no:cacheprovider"]);
    let Ok(out) = run(&venv_py.to_string_lossy(), &args, Some(repo_dir)).await else {
        return (ids.iter().map(|i| (i.clone(), false)).collect(), String::new());
    };
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let (by_node, by_func) = parse_pytest_report(&report);
    let verdicts = ids
        .iter()
        .map(|id| (id.clone(), verdict_for(id, &by_node, &by_func).unwrap_or(false)))
        .collect();
    // The report rides back so the grader can excerpt the FAILURE OUTPUT into the
    // verdict — the assertion diff is the teaching half a bare test name lacks.
    (verdicts, report)
}

/// Cap on the failure-output excerpt a verdict carries. Failures and the short
/// summary sit at the END of a test run's output (pytest and sympy's own runner
/// alike), so the TAIL is the format-agnostic excerpt. Bounded so a pathological
/// run (a runaway traceback, a print loop) can't flood the retry prompt.
const FAILURE_EXCERPT_MAX: usize = 2000;

/// Tail of a test report, char-capped on a char boundary.
fn report_tail(report: &str) -> String {
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
fn compose_failure_excerpt(
    p2p_broken: &[String],
    p2p_report: &str,
    f2p_still_failing: bool,
    f2p_report: &str,
) -> Option<String> {
    let mut sections: Vec<String> = Vec::new();
    if !p2p_broken.is_empty() {
        const NAME_CAP: usize = 10;
        let shown: Vec<&str> = p2p_broken.iter().take(NAME_CAP).map(|s| s.as_str()).collect();
        let more = p2p_broken.len().saturating_sub(NAME_CAP);
        let more_note = if more > 0 { format!(" (+{more} more)") } else { String::new() };
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
        sections.push(report_tail(f2p_report));
    }
    if sections.is_empty() {
        None
    } else {
        Some(sections.join("\n\n"))
    }
}

pub async fn grade(
    instance: &SweInstance,
    repo_dir: &Path,
    model_patch: Option<&str>,
) -> SweVerdict {
    let mut verdict = SweVerdict {
        instance_id: instance.instance_id.clone(),
        ..Default::default()
    };
    let f2p = instance.f2p();
    let p2p: Vec<String> = instance.p2p().into_iter().take(P2P_SAMPLE).collect();
    verdict.f2p_total = f2p.len();
    verdict.p2p_total = p2p.len();
    let test_files = patched_test_files(&instance.test_patch);

    let venv_py = match ensure_env(instance, repo_dir).await {
        Ok(p) => p,
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
    let (pre, _) = run_tests(repo_dir, &venv_py, &f2p, &test_files).await;
    let already: Vec<&String> = pre.iter().filter(|(_, ok)| **ok).map(|(id, _)| id).collect();
    verdict.gate_ok = already.is_empty();
    if !verdict.gate_ok {
        verdict.error = Some(format!(
            "UNGRADEABLE — FAIL_TO_PASS already passes on the pristine tree ({already:?}). The \
             bug is not in this checkout; every score from this tree is void."
        ));
        return verdict;
    }

    // Reset, then run the real protocol: model patch first, tests second.
    let _ = run("git", &["checkout", "--quiet", "."], Some(repo_dir)).await;
    if let Some(patch) = model_patch {
        if let Err(e) = apply_patch(repo_dir, patch, "model").await {
            verdict.error = Some(format!("candidate patch did not apply: {e}"));
            return verdict;
        }
    }
    if let Err(e) = apply_patch(repo_dir, &instance.test_patch, "test").await {
        verdict.error = Some(e);
        return verdict;
    }

    let (f2p_res, f2p_report) = run_tests(repo_dir, &venv_py, &f2p, &test_files).await;
    let (p2p_res, p2p_report) = run_tests(repo_dir, &venv_py, &p2p, &test_files).await;
    verdict.f2p_passed = f2p_res.values().filter(|ok| **ok).count();
    verdict.p2p_passed = p2p_res.values().filter(|ok| **ok).count();
    verdict.failed_tests = f2p_res
        .iter()
        .chain(p2p_res.iter())
        .filter(|(_, ok)| !**ok)
        .map(|(id, _)| id.clone())
        .collect();
    verdict.failed_tests.sort();
    let mut p2p_broken: Vec<String> = p2p_res
        .iter()
        .filter(|(_, ok)| !**ok)
        .map(|(id, _)| id.clone())
        .collect();
    p2p_broken.sort();
    verdict.failure_excerpt = compose_failure_excerpt(
        &p2p_broken,
        &p2p_report,
        verdict.f2p_passed < verdict.f2p_total,
        &f2p_report,
    );
    verdict.resolved = verdict.f2p_passed == verdict.f2p_total
        && verdict.p2p_passed == verdict.p2p_total
        && verdict.f2p_total > 0;
    verdict
}

#[cfg(test)]
mod tests {
    use super::*;

    // what this catches: the deleted-history env-build failure (pytest-dev__pytest-5103,
    // live 2026-08-11) — `atomicwrites`' pre-2022 uploads were deleted from PyPI, so the
    // date-pinned resolve has ZERO candidates and the env build dies. The heal parses uv's
    // own hint into an `--exclude-newer-package` value cut one second past the earliest
    // surviving upload; unrelated errors must parse to None (no retry storm).
    #[test]
    fn deleted_history_hint_parses_into_a_per_package_cutoff() {
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

    // what this catches: the wheel-less era sdist (pylint-5859, live 2026-08-11) —
    // lazy-object-proxy 1.7.1 has no arm64 wheel and its sdist stamps 0.0.0, so the
    // date-pinned install can NEVER succeed; the heal must lift exactly that package's
    // cutoff. Unrelated errors and mismatches without a named package parse to None.
    #[test]
    fn metadata_mismatch_parses_into_a_lifted_cutoff() {
        let stderr = "\u{2570}\u{2500}\u{25b6} Package metadata version `0.0.0` does not \
            match given version `1.7.1`\n\nhint: `lazy-object-proxy` (v1.7.1) was included \
            because `pylint` (v2.13.0.dev0) depends on `astroid` (v2.9.3)";
        assert_eq!(
            metadata_mismatch_override(stderr).as_deref(),
            Some("lazy-object-proxy=9999-01-01T00:00:00Z"),
        );
        assert_eq!(metadata_mismatch_override("error: some other failure"), None);
        assert_eq!(
            metadata_mismatch_override(
                "Package metadata version `0.0.0` does not match given version `1.0` (no hint)"
            ),
            None,
            "mismatch without a named package must not synthesize an override"
        );
    }

    // what this catches: the hidden-collateral verdict (atlas-24066-n7) — a patch that
    // broke 30 pass-to-pass tests produced retry feedback built from the f2p report
    // alone, so she resubmitted the identical broken patch twice. The REGRESSION
    // section must lead the excerpt whenever p2p broke, name the broken tests, and
    // still carry the f2p tail after it; no-breakage keeps the old f2p-only shape.
    #[test]
    fn regression_breakage_leads_the_failure_excerpt() {
        let broken: Vec<String> = (0..12).map(|i| format!("test_p2p_{i}")).collect();
        let both = compose_failure_excerpt(&broken, "E ImportError: cannot import name 'Exp'", true, "E AssertionError: target still fails")
            .expect("both sections");
        assert!(both.starts_with("REGRESSION"), "breakage must LEAD: {both}");
        assert!(both.contains("BROKE 12 test(s)"));
        assert!(both.contains("test_p2p_0") && both.contains("(+2 more)"), "names capped at 10: {both}");
        assert!(both.contains("ImportError") && both.contains("AssertionError"), "both report tails present");
        let regression_at = both.find("REGRESSION").unwrap();
        let f2p_at = both.find("AssertionError").unwrap();
        assert!(regression_at < f2p_at, "regression before target-test tail");

        let clean = compose_failure_excerpt(&[], "", true, "E AssertionError: target still fails")
            .expect("f2p-only");
        assert!(!clean.contains("REGRESSION"), "no fabricated regression on a clean tree");

        assert!(compose_failure_excerpt(&[], "", false, "noise").is_none(), "nothing failing → no excerpt");
    }

    // what this catches: the id-shape assumption that mis-scored GOLD as a real failure.
    // sympy's FAIL_TO_PASS entries are bare function names because sympy ships its own runner;
    // handing those to pytest as paths produced "file or directory not found", scored as a
    // failing test, and returned RESOLVED=0 on three instances whose environments were fine.
    #[test]
    fn a_required_id_resolves_by_node_id_or_by_bare_function_name() {
        let report = "\
tests/test_polysys.py::test_solve_poly_system PASSED [ 50%]
tests/test_polysys.py::test_solve_biquadratic FAILED [100%]
tests/test_x.py::TestC::test_param[3-4] PASSED";
        let (by_node, by_func) = parse_pytest_report(report);

        // flask/pytest shape — full node id.
        assert_eq!(
            verdict_for("tests/test_polysys.py::test_solve_poly_system", &by_node, &by_func),
            Some(true)
        );
        // sympy shape — BARE function name, the case that was broken.
        assert_eq!(verdict_for("test_solve_poly_system", &by_node, &by_func), Some(true));
        assert_eq!(verdict_for("test_solve_biquadratic", &by_node, &by_func), Some(false));
        // parametrised tests resolve by their base name.
        assert_eq!(verdict_for("test_param", &by_node, &by_func), Some(true));
        // an id nothing in the report matches is UNKNOWN, never a silent pass.
        assert_eq!(verdict_for("test_never_ran", &by_node, &by_func), None);
    }

    // what this catches: a bare name appearing in two files where one fails — counting it as
    // passing would let real breakage through.
    #[test]
    fn a_duplicated_bare_name_passes_only_if_every_occurrence_passed() {
        let report = "\
tests/a.py::test_shared PASSED
tests/b.py::test_shared FAILED";
        let (by_node, by_func) = parse_pytest_report(report);
        assert_eq!(verdict_for("test_shared", &by_node, &by_func), Some(false));
        assert_eq!(verdict_for("tests/a.py::test_shared", &by_node, &by_func), Some(true));
    }

    // what this catches: the scope of the test run. Running the whole suite is slow and
    // running the wrong file scores nothing; the instance's own test_patch names the files.
    #[test]
    fn the_test_scope_comes_from_the_instances_own_test_patch() {
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
    fn the_interpreter_is_chosen_by_the_instances_era() {
        assert_eq!(interpreter_for_year(2014), "3.9");
        assert_eq!(interpreter_for_year(2019), "3.9");
        assert_eq!(interpreter_for_year(2020), "3.10");
        assert_eq!(interpreter_for_year(2021), "3.10");
        assert_eq!(interpreter_for_year(2022), "3.10");
        assert_eq!(interpreter_for_year(2023), "3.11");
    }

    // what this catches: the silent-death hole. A detached run wrote NOTHING until it
    // finished, so "still working" and "died an hour ago" were the same observation — an
    // absent file — and two core reboots killed a run with no trace. A `running` marker that
    // outlives its core must be journaled as failed, naming the cause.
    #[test]
    fn a_run_still_marked_running_at_boot_is_journaled_as_killed() {
        let dir = tempfile::tempdir().expect("tmp");
        let p = dir.path();
        std::fs::write(
            p.join("swe-solve-alive.json"),
            r#"{"state":"running","runId":"alive","instance":"sympy__sympy-22005"}"#,
        )
        .unwrap();
        // A FINISHED run must survive the reap untouched — reaping a real verdict would
        // destroy the only record of a measurement that actually happened.
        std::fs::write(
            p.join("swe-solve-done.json"),
            r#"{"instance":"sympy__sympy-21379","acts":7,"detached":false}"#,
        )
        .unwrap();
        // An unrelated ledger from another subsystem is not ours to touch.
        std::fs::write(p.join("agent-solve-other.json"), r#"{"state":"running"}"#).unwrap();

        assert_eq!(
            in_flight_solve_runs_in(p),
            vec![("alive".to_string(), "sympy__sympy-22005".to_string())],
            "only OUR unfinished runs count as in flight"
        );

        let reaped = reap_orphaned_solve_runs_in(p);
        assert_eq!(reaped, vec!["alive".to_string()]);

        let after = std::fs::read_to_string(p.join("swe-solve-alive.json")).unwrap();
        assert!(after.contains("\"failed\":true"), "the orphan is now a FAILED run: {after}");
        assert!(
            after.contains("killed by a core restart"),
            "and it names the cause rather than leaving a bare zero: {after}"
        );
        let done = std::fs::read_to_string(p.join("swe-solve-done.json")).unwrap();
        assert!(done.contains("\"acts\":7"), "a finished verdict is never rewritten");
        let other = std::fs::read_to_string(p.join("agent-solve-other.json")).unwrap();
        assert!(!other.contains("failed"), "another subsystem's ledger is untouched");

        assert!(
            in_flight_solve_runs_in(p).is_empty(),
            "after the reap nothing is in flight — a second boot must not re-reap"
        );
    }

    // what this catches: test-id lists arrive JSON-encoded inside a string field, and a
    // missing/blank one must yield an empty list rather than panicking a whole run.
    #[test]
    fn test_id_lists_decode_from_their_string_field() {
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
        };
        assert_eq!(inst.f2p(), vec!["test_a".to_string(), "test_b".to_string()]);
        assert!(inst.p2p().is_empty(), "a blank list must not panic the run");
        assert_eq!(inst.year(), 2021);
    }
}
