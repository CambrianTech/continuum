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

async fn run(program: &str, args: &[&str], cwd: Option<&Path>) -> Result<std::process::Output, String> {
    let mut cmd = tokio::process::Command::new(program);
    cmd.args(args);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    cmd.output()
        .await
        .map_err(|e| format!("could not run `{program}`: {e}"))
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
pub fn interpreter_for_year(year: u32) -> &'static str {
    if year < 2020 {
        "3.9"
    } else {
        "3.11"
    }
}

/// Build (or reuse) the per-instance environment. Per-instance rather than per-repo because
/// instances span years of a repo's history and their dependency graphs genuinely differ.
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
    let _ = run(
        &uv,
        &["pip", "install", "-q", "--python", &py_s, "pytest", "setuptools", "wheel"],
        None,
    )
    .await?;

    let as_of = if instance.created_at.is_empty() {
        None
    } else {
        Some(instance.created_at.clone())
    };
    let repo_s = repo_dir.to_string_lossy().to_string();
    let mut args = vec!["pip", "install", "-q", "--python", &py_s];
    if let Some(ref date) = as_of {
        args.push("--exclude-newer");
        args.push(date);
    }
    args.extend(["--no-build-isolation", "-e", "."]);
    let out = run(&uv, &args, Some(Path::new(&repo_s))).await?;
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
) -> HashMap<String, bool> {
    if test_files.is_empty() || ids.is_empty() {
        return ids.iter().map(|i| (i.clone(), false)).collect();
    }
    let mut args: Vec<&str> = vec!["-m", "pytest"];
    for f in test_files {
        args.push(f);
    }
    args.extend(["-v", "--no-header", "-rN", "-p", "no:cacheprovider"]);
    let Ok(out) = run(&venv_py.to_string_lossy(), &args, Some(repo_dir)).await else {
        return ids.iter().map(|i| (i.clone(), false)).collect();
    };
    let report = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let (by_node, by_func) = parse_pytest_report(&report);
    ids.iter()
        .map(|id| (id.clone(), verdict_for(id, &by_node, &by_func).unwrap_or(false)))
        .collect()
}

/// How many PASS_TO_PASS tests to sample. The full set runs to hundreds on some instances and
/// its job is regression detection; a broad sample catches breakage without paying for the
/// tail. Named rather than inlined so a number in a report is traceable to a decision.
pub const P2P_SAMPLE: usize = 40;

/// Grade one candidate patch against one instance, running the full protocol.
///
/// `model_patch` empty means "grade the tree as the solver left it" — the caller has already
/// edited `repo_dir` in place and the diff is implicit.
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
    let pre = run_tests(repo_dir, &venv_py, &f2p, &test_files).await;
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

    let f2p_res = run_tests(repo_dir, &venv_py, &f2p, &test_files).await;
    let p2p_res = run_tests(repo_dir, &venv_py, &p2p, &test_files).await;
    verdict.f2p_passed = f2p_res.values().filter(|ok| **ok).count();
    verdict.p2p_passed = p2p_res.values().filter(|ok| **ok).count();
    verdict.resolved = verdict.f2p_passed == verdict.f2p_total
        && verdict.p2p_passed == verdict.p2p_total
        && verdict.f2p_total > 0;
    verdict
}

#[cfg(test)]
mod tests {
    use super::*;

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
    // dependency pin can rescue that, the language moved.
    #[test]
    fn the_interpreter_is_chosen_by_the_instances_era() {
        assert_eq!(interpreter_for_year(2014), "3.9");
        assert_eq!(interpreter_for_year(2019), "3.9");
        assert_eq!(interpreter_for_year(2021), "3.11");
        assert_eq!(interpreter_for_year(2023), "3.11");
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
